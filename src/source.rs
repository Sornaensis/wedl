//! Read canonical UTF-8 Markdown, either from the worktree or literal Git objects.
use crate::{
    error::{Error, Result},
    model::{Record, World},
};
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const SOURCE_SCHEMA: &str = "wedl/v1";

/// Create a minimal current-schema world in a new directory. Existing paths
/// are never reused or overwritten. Git remains an explicit user operation.
pub fn init_world(path: &Path, title: &str) -> Result<World> {
    if title.trim().is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
        return Err(Error::InvalidInput(
            "title must be a nonempty single line of at most 256 UTF-8 bytes".into(),
        ));
    }
    if path.as_os_str().is_empty() {
        return Err(Error::InvalidInput(
            "a new world directory is required".into(),
        ));
    }
    // The atomic directory create also refuses files, symlinks and an existing
    // empty directory. Failures after this point leave only the new scaffold.
    fs::create_dir(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Conflict("world path already exists; choose a new directory".into())
        } else {
            Error::Io(error)
        }
    })?;
    let root = path.canonicalize()?;
    let mut hash = Sha256::new();
    hash.update(root.as_os_str().as_encoded_bytes());
    hash.update(std::process::id().to_be_bytes());
    hash.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| Error::Data(error.to_string()))?
            .as_nanos()
            .to_be_bytes(),
    );
    let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let suffix: String = hash
        .finalize()
        .iter()
        .take(26)
        .map(|value| alphabet[(value & 31) as usize] as char)
        .collect();
    let header = serde_json::json!({
        "schema":SOURCE_SCHEMA,"kind":"world","id":format!("world_{suffix}"),
        "title":title,"domain":"world","status":"canonical","capabilities":[],
        "tags":[],"aliases":[],"default_timeline":"main",
        "timelines":[{"id":"main","label":"Main"}],"threads":[],
        "current_time":{"timeline":"main","tick":0,"order":0},
        "state_keys":{},"relationship_metrics":{},"section_audiences":{"public":["public"]},
    });
    let source = format!("---\n{}---\n\n# {title}\n", serde_yaml::to_string(&header)?);
    fs::create_dir(root.join("story"))?;
    fs::write(root.join("story/world.md"), source)?;
    fs::write(root.join(".gitignore"), ".wedl-rust/\n")?;
    load_world(root, None)
}

pub fn parse_record(bytes: &[u8], path: &str) -> Result<Record> {
    let fail = |message: String| Error::Parse {
        path: path.to_owned(),
        message,
    };
    let text = std::str::from_utf8(bytes).map_err(|_| fail("source must be UTF-8".into()))?;
    let newline = if text.starts_with("---\r\n") {
        "\r\n"
    } else if text.starts_with("---\n") {
        "\n"
    } else {
        return Err(fail("missing YAML frontmatter opener".into()));
    };
    let opener_len = 3 + newline.len();
    let terminator = format!("{newline}---{newline}");
    let end = text[opener_len..]
        .find(&terminator)
        .map(|pos| opener_len + pos)
        .ok_or_else(|| fail("missing YAML frontmatter terminator".into()))?;
    let frontmatter: Value = serde_yaml::from_str(&text[opener_len..end])
        .map_err(|error| fail(format!("invalid YAML: {error}")))?;
    if !frontmatter.is_mapping() {
        return Err(fail("frontmatter must be a mapping".into()));
    }
    if frontmatter.get("schema").and_then(Value::as_str) != Some(SOURCE_SCHEMA) {
        return Err(fail(format!(
            "unsupported source schema; expected {SOURCE_SCHEMA}"
        )));
    }
    let required = |name: &str| -> Result<String> {
        frontmatter
            .get(name)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| fail(format!("{name} must be a nonempty string")))
    };
    let id = required("id")?;
    let kind = required("kind")?;
    let title = required("title")?;
    Ok(Record {
        id,
        kind,
        title,
        source_path: path.to_owned(),
        frontmatter,
        body: text[end + terminator.len()..].to_owned(),
        raw_bytes: bytes.to_vec(),
        blob_oid: None,
    })
}

pub fn world_from_records(
    root: PathBuf,
    records: Vec<Record>,
    revision: Option<String>,
) -> Result<World> {
    let mut by_id = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for record in records {
        if !paths.insert(record.source_path.to_lowercase()) {
            return Err(Error::Validation(format!(
                "case-insensitive source path collision: {}",
                record.source_path
            )));
        }
        if by_id.contains_key(&record.id) {
            return Err(Error::Validation(format!(
                "duplicate record ID {}",
                record.id
            )));
        }
        by_id.insert(record.id.clone(), record);
    }
    let fingerprint = fingerprint_for_records(&by_id);
    let world = World {
        root,
        records: by_id,
        fingerprint,
        revision,
    };
    world.world_record()?;
    Ok(world)
}

/// Exact source identity independent of mutable parsed projections.
pub(crate) fn fingerprint_for_records(records: &BTreeMap<String, Record>) -> String {
    let mut hash = Sha256::new();
    hash.update(b"wedl-rs/source-v1\0");
    // Length framing prevents ambiguity between paths and source bytes.
    let mut ordered: Vec<_> = records.values().collect();
    ordered.sort_by_key(|record| &record.source_path);
    for record in ordered {
        hash.update((record.source_path.len() as u64).to_be_bytes());
        hash.update(record.source_path.as_bytes());
        hash.update((record.raw_bytes.len() as u64).to_be_bytes());
        hash.update(&record.raw_bytes);
    }
    hex::encode(hash.finalize())
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(root)
        .args(args);
    // Match the original-object reader: inherited Git settings must not
    // redirect source selection or turn a read into a trace-file write.
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !output.status.success() {
        return Err(Error::InvalidInput(format!(
            "Git read failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

pub fn load_world(root: impl AsRef<Path>, revision: Option<&str>) -> Result<World> {
    let requested = root.as_ref();
    if fs::symlink_metadata(requested)?.file_type().is_symlink() {
        return Err(Error::InvalidInput(
            "repository root must not be a symlink".into(),
        ));
    }
    let root = requested.canonicalize()?;
    if let Some(revision) = revision {
        load_git_world(root, revision)
    } else {
        let stamp = crate::publication::read_stamp(&root)?;
        let result = load_worktree_unfenced(root.clone());
        if crate::publication::read_stamp(&root)? != stamp {
            return Err(Error::Conflict(
                "outcome publication changed during source capture; reload the current snapshot"
                    .into(),
            ));
        }
        result
    }
}

/// Only the journal publisher/recovery may bypass the publication read fence.
/// Path, duplicate-ID and source grammar checks remain unchanged.
pub(crate) fn load_worktree_unfenced(root: PathBuf) -> Result<World> {
    let source = root.join("story");
    if fs::symlink_metadata(&source)?.file_type().is_symlink() {
        return Err(Error::InvalidInput(
            "story directory must not be a symlink".into(),
        ));
    }
    if !source.is_dir() {
        return Err(Error::InvalidInput(
            "repository requires a story directory".into(),
        ));
    }
    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(&source)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| Error::Data(format!("source walk failed: {error}")))?;
        if entry.file_type().is_symlink() {
            return Err(Error::InvalidInput(format!(
                "symlink in source tree: {}",
                entry.path().display()
            )));
        }
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|v| v.to_str()) != Some("md")
        {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&root)
            .map_err(|error| Error::Data(error.to_string()))?;
        let path = relative
            .to_str()
            .ok_or_else(|| Error::InvalidInput("source path must be UTF-8".into()))?
            .replace('\\', "/");
        records.push(parse_record(&fs::read(entry.path())?, &path)?);
    }
    world_from_records(root, records, None)
}

fn load_git_world(root: PathBuf, revision: &str) -> Result<World> {
    if revision.trim().is_empty() {
        return Err(Error::InvalidInput("revision must not be empty".into()));
    }
    let git_root = git(&root, &["rev-parse", "--show-toplevel"])?;
    let actual = PathBuf::from(
        String::from_utf8(git_root)
            .map_err(|_| Error::Data("Git root is not UTF-8".into()))?
            .trim(),
    );
    if actual.canonicalize()? != root {
        return Err(Error::InvalidInput(
            "selected world must be the Git repository root for revision reads".into(),
        ));
    }
    let target = format!("{revision}^{{commit}}");
    let commit = String::from_utf8(git(
        &root,
        &["rev-parse", "--verify", "--end-of-options", &target],
    )?)
    .map_err(|_| Error::Data("invalid commit identity".into()))?
    .trim()
    .to_owned();
    if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Data("invalid commit identity".into()));
    }
    let tree = git(&root, &["ls-tree", "-r", "-z", &commit, "--", "story"])?;
    let mut records = Vec::new();
    for entry in tree.split(|b| *b == 0).filter(|entry| !entry.is_empty()) {
        let line = std::str::from_utf8(entry)
            .map_err(|_| Error::InvalidInput("source path must be UTF-8".into()))?;
        let (metadata, path) = line
            .split_once('\t')
            .ok_or_else(|| Error::Data("malformed Git tree response".into()))?;
        let parts: Vec<_> = metadata.split_whitespace().collect();
        if parts.len() != 3 {
            return Err(Error::Data("malformed Git tree entry".into()));
        }
        if !matches!(parts[0], "100644" | "100755") || parts[1] != "blob" {
            return Err(Error::InvalidInput(format!(
                "nonregular entry in source tree: {path}"
            )));
        }
        if !path.starts_with("story/") || !path.ends_with(".md") {
            continue;
        }
        let mut record = parse_record(&git(&root, &["cat-file", "blob", parts[2]])?, path)?;
        record.blob_oid = Some(parts[2].to_owned());
        records.push(record);
    }
    world_from_records(root, records, Some(commit))
}
