//! Original immutable Git captures and authored historical citation syntax.
//!
//! A citation is retained author data, not authenticated evidence or an access
//! grant. The internal readers support private history and revision comparison;
//! immutable readers never consult live source, branches, caches or recovery.
//! Explicit comparison helpers separately capture one live HEAD and bounded
//! publication-fenced worktree; these are never aliases for historical reads.
use crate::{Error, Result, model::World, source, validation};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

const MAX_TREE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TREE_OBJECTS: usize = 8192;
const MAX_TREE_PATH_BYTES: usize = 16 * 1024 * 1024;
const MAX_SOURCE_RECORDS: usize = 4096;
const MAX_SOURCE_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SOURCE_NODES: usize = 1_000_000;
const MAX_COMMIT_BYTES: usize = 262_144;
const MAX_ANCESTRY_COMMITS: usize = 8192;
const MAX_ANCESTRY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Target {
    Record { id: String },
    Section { owner_id: String, id: String },
    Transition { owner_id: String, id: String },
}

/// The same closed syntax as canonical assertion evidence. Old targets are not
/// resolved against current source, and claimed object IDs are not proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub revision: String,
    pub path: String,
    pub blob_oid: String,
    pub target: Target,
}

pub(crate) fn validate_citations(citations: &[Citation]) -> Result<()> {
    crate::assertion::validate_evidence_request(&serde_yaml::to_value(citations)?)
}

#[derive(Clone, Debug)]
pub(crate) struct Blob {
    pub revision: String,
    pub path: String,
    pub oid: String,
    pub bytes: Vec<u8>,
}

struct Entry {
    mode: String,
    kind: String,
    oid: String,
    path: String,
}

struct Repository {
    root: PathBuf,
    revision: String,
    root_tree: String,
}

#[derive(Default)]
struct TreeBudget {
    bytes: usize,
    objects: usize,
    paths: usize,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn unavailable() -> Error {
    Error::NotFound("original historical Git source is unavailable".into())
}

pub(crate) fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', '\0'])
        || path.starts_with('/')
        || path.split('/').count() > 64
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(invalid(
            "historical source requires a bounded safe relative path",
        ));
    }
    Ok(())
}

/// Bound stdout while it is read; error text is not part of historical data.
/// All Git inputs are literal identities/paths and no command writes objects.
fn run(
    root: &Path,
    arguments: &[&str],
    maximum: usize,
    input: Option<&[u8]>,
) -> Result<(ExitStatus, Vec<u8>)> {
    let mut command = Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // A caller's selected repository, index, replacement refs, trace output or
    // object directory must not redirect a supposedly original read.
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    let mut child = command.spawn()?;
    if let Some(input) = input {
        let result = child.stdin.take().unwrap().write_all(input);
        if let Err(error) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
    }
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .unwrap()
        .take(maximum.saturating_add(1) as u64)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > maximum {
        let _ = child.kill();
        let _ = child.wait();
        read?;
        return Err(invalid(
            "complete historical Git output exceeds its byte bound",
        ));
    }
    Ok((child.wait()?, bytes))
}

fn checked(root: &Path, arguments: &[&str], maximum: usize) -> Result<Vec<u8>> {
    let (status, bytes) = run(root, arguments, maximum, None)?;
    if !status.success() {
        return Err(unavailable());
    }
    Ok(bytes)
}

impl Repository {
    fn open(root: &Path, revision: &str) -> Result<Self> {
        if !valid_oid(revision) {
            return Err(invalid(
                "historical reads require a literal full lowercase commit OID",
            ));
        }
        let metadata = fs::symlink_metadata(root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(invalid(
                "historical repository root must be a real directory",
            ));
        }
        let root = root.canonicalize()?;
        let actual = checked(&root, &["rev-parse", "--show-toplevel"], 4096)?;
        let actual = std::str::from_utf8(&actual)
            .map_err(|_| invalid("historical repository root must be UTF-8"))?;
        if Path::new(actual.trim()).canonicalize()? != root {
            return Err(invalid(
                "historical reads require the exact Git repository root",
            ));
        }
        let mut result = Self {
            root,
            revision: revision.into(),
            root_tree: String::new(),
        };
        let commit = result.commit(revision)?;
        let tree = commit
            .split(|byte| *byte == b'\n')
            .next()
            .and_then(|line| line.strip_prefix(b"tree "))
            .and_then(|value| std::str::from_utf8(value).ok())
            .filter(|value| valid_oid(value) && value.len() == revision.len())
            .ok_or_else(|| invalid("original commit must name one exact root tree"))?;
        result.root_tree = tree.into();
        Ok(result)
    }

    fn object_size(&self, oid: &str) -> Result<usize> {
        let size = checked(&self.root, &["cat-file", "-s", oid], 32)?;
        std::str::from_utf8(&size)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .ok_or_else(|| invalid("historical object size is invalid"))
    }

    fn object(&self, kind: &str, oid: &str, maximum: usize) -> Result<Vec<u8>> {
        let size = self.object_size(oid)?;
        if size > maximum {
            return Err(invalid("historical object exceeds its complete byte bound"));
        }
        let bytes = checked(&self.root, &["cat-file", kind, oid], size)?;
        if bytes.len() != size {
            return Err(invalid("historical object size changed during capture"));
        }
        let (status, computed) = run(
            &self.root,
            &["hash-object", "-t", kind, "--stdin"],
            128,
            Some(&bytes),
        )?;
        if !status.success() || std::str::from_utf8(&computed).ok().map(str::trim) != Some(oid) {
            return Err(invalid(
                "historical object bytes differ from their original identity",
            ));
        }
        Ok(bytes)
    }

    fn commit(&self, oid: &str) -> Result<Vec<u8>> {
        if !valid_oid(oid) || oid.len() != self.revision.len() {
            return Err(invalid(
                "historical ancestry requires exact commit identities",
            ));
        }
        if checked(&self.root, &["cat-file", "-t", oid], 16)? != b"commit\n" {
            return Err(invalid(
                "historical revision must identify an actual commit",
            ));
        }
        self.object("commit", oid, MAX_COMMIT_BYTES)
    }

    /// Decode verified binary tree bytes. Tree membership comes from the
    /// commit's original links, never from an unchecked ls-tree association.
    fn tree_entries(
        &self,
        oid: &str,
        prefix: &str,
        budget: &mut TreeBudget,
        selected: Option<&str>,
    ) -> Result<Vec<Entry>> {
        budget.objects += 1;
        if budget.objects > MAX_TREE_OBJECTS {
            return Err(invalid(
                "historical tree traversal exceeds 8192 tree objects",
            ));
        }
        let raw = self.object("tree", oid, MAX_TREE_BYTES - budget.bytes)?;
        budget.bytes += raw.len();
        let hash_bytes = self.revision.len() / 2;
        let mut rows = BTreeMap::new();
        let mut offset = 0;
        while offset < raw.len() {
            let end = raw[offset..]
                .iter()
                .position(|byte| *byte == 0)
                .map(|end| offset + end)
                .ok_or_else(|| invalid("original Git tree entry lacks its separator"))?;
            let header = &raw[offset..end];
            let split = header
                .iter()
                .position(|byte| *byte == b' ')
                .ok_or_else(|| invalid("malformed original Git tree mode/name"))?;
            let mode = std::str::from_utf8(&header[..split])
                .map_err(|_| invalid("original Git tree mode must be ASCII"))?;
            let name = &header[split + 1..];
            if name.is_empty() || name.contains(&b'/') || end + 1 + hash_bytes > raw.len() {
                return Err(invalid(
                    "malformed original Git tree path or object identity",
                ));
            }
            let oid = hex::encode(&raw[end + 1..end + 1 + hash_bytes]);
            offset = end + 1 + hash_bytes;
            // An unrelated repository sibling is not part of the selected
            // canonical/private namespace. Do not reinterpret its filename.
            if selected.is_some_and(|selected| selected.as_bytes() != name) {
                continue;
            }
            let name = std::str::from_utf8(name)
                .map_err(|_| invalid("historical source paths must be UTF-8"))?;
            let (mode, kind) = match mode {
                "40000" => ("040000", "tree"),
                "100644" => ("100644", "blob"),
                "100755" => ("100755", "blob"),
                "120000" => ("120000", "blob"),
                "160000" => ("160000", "commit"),
                _ => return Err(invalid("unsupported original Git tree mode")),
            };
            let path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            safe_path(&path)?;
            budget.paths = budget.paths.saturating_add(path.len());
            if budget.paths > MAX_TREE_PATH_BYTES {
                return Err(invalid("historical tree paths exceed 16 MiB"));
            }
            if rows
                .insert(
                    path.clone(),
                    Entry {
                        mode: mode.into(),
                        kind: kind.into(),
                        oid,
                        path,
                    },
                )
                .is_some()
            {
                return Err(invalid("duplicate exact path in an original Git tree"));
            }
        }
        Ok(rows.into_values().collect())
    }

    fn entry(&self, path: &str, budget: &mut TreeBudget) -> Result<Option<Entry>> {
        safe_path(path)?;
        let parts: Vec<_> = path.split('/').collect();
        let mut tree = self.root_tree.clone();
        let mut prefix = String::new();
        for (index, part) in parts.iter().enumerate() {
            let wanted = if prefix.is_empty() {
                (*part).to_owned()
            } else {
                format!("{prefix}/{part}")
            };
            let Some(row) = self
                .tree_entries(&tree, &prefix, budget, Some(part))?
                .into_iter()
                .find(|row| row.path == wanted)
            else {
                return Ok(None);
            };
            if index + 1 == parts.len() {
                return Ok(Some(row));
            }
            if row.mode != "040000" || row.kind != "tree" {
                return Err(invalid(
                    "historical source parents must be original Git trees",
                ));
            }
            tree = row.oid;
            prefix = row.path;
        }
        unreachable!("safe paths contain at least one component")
    }

    fn tree(&self, prefix: &str) -> Result<Vec<Entry>> {
        let mut budget = TreeBudget::default();
        let Some(selected) = self.entry(prefix, &mut budget)? else {
            return Ok(Vec::new());
        };
        if selected.mode != "040000" || selected.kind != "tree" {
            return Err(invalid(
                "historical source prefix must be an original Git tree",
            ));
        }
        let mut pending = vec![(selected.oid, selected.path)];
        let mut result = BTreeMap::new();
        let mut folded = BTreeSet::new();
        while let Some((oid, path)) = pending.pop() {
            for row in self.tree_entries(&oid, &path, &mut budget, None)? {
                if row.mode == "040000" && row.kind == "tree" {
                    pending.push((row.oid, row.path));
                } else if matches!(row.mode.as_str(), "100644" | "100755") && row.kind == "blob" {
                    if !folded.insert(row.path.to_lowercase()) {
                        return Err(invalid("case-insensitive historical source path collision"));
                    }
                    result.insert(row.path.clone(), row);
                } else {
                    return Err(invalid(
                        "historical source entries must be regular files in the selected tree",
                    ));
                }
            }
        }
        Ok(result.into_values().collect())
    }

    fn blob(&self, entry: Entry, maximum: usize) -> Result<Blob> {
        if !matches!(entry.mode.as_str(), "100644" | "100755") || entry.kind != "blob" {
            return Err(invalid(
                "historical source must be an original regular Git blob",
            ));
        }
        let bytes = self.object("blob", &entry.oid, maximum)?;
        Ok(Blob {
            revision: self.revision.clone(),
            path: entry.path,
            oid: entry.oid,
            bytes,
        })
    }
}

pub(crate) fn read_blob(root: &Path, revision: &str, path: &str, max_bytes: usize) -> Result<Blob> {
    safe_path(path)?;
    if max_bytes > MAX_SOURCE_BYTES {
        return Err(invalid(
            "historical blob bound exceeds the supported source ceiling",
        ));
    }
    let repository = Repository::open(root, revision)?;
    let entry = repository
        .entry(path, &mut TreeBudget::default())?
        .ok_or_else(unavailable)?;
    repository.blob(entry, max_bytes)
}

/// Read the complete selected tree. A genuinely missing prefix is empty; a
/// non-tree parent, unsupported mode, collision or bound refuses the whole read.
pub(crate) fn read_tree(
    root: &Path,
    revision: &str,
    prefix: &str,
    max_files: usize,
    max_file_bytes: usize,
    max_total_bytes: usize,
) -> Result<Vec<Blob>> {
    if max_files > MAX_SOURCE_RECORDS
        || max_file_bytes > MAX_SOURCE_BYTES
        || max_total_bytes > MAX_SOURCE_BYTES
    {
        return Err(invalid(
            "historical tree bounds exceed supported source ceilings",
        ));
    }
    let repository = Repository::open(root, revision)?;
    let entries = repository.tree(prefix)?;
    if entries.len() > max_files {
        return Err(invalid(
            "historical tree exceeds its complete file-count bound",
        ));
    }
    let mut total = 0usize;
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let blob = repository.blob(entry, max_file_bytes.min(max_total_bytes - total))?;
        total += blob.bytes.len();
        result.push(blob);
    }
    Ok(result)
}

#[derive(Default)]
struct SourceBudget {
    nodes: usize,
    scalar_bytes: usize,
}

/// One complete original-source allowance can cover several distinct revisions.
/// Each captured record version is charged before expanded source allocation.
#[derive(Default)]
pub(crate) struct WorldBudget {
    records: usize,
    raw_bytes: usize,
    source: SourceBudget,
}

/// Traverse expanded YAML without constructing Values. Repeated aliases count
/// every expanded scalar/node before the ordinary native parser can copy them.
fn preflight_source(bytes: &[u8], budget: &mut SourceBudget) -> Result<()> {
    use serde::de::{DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
    struct Scan<'a> {
        budget: &'a mut SourceBudget,
        depth: usize,
    }
    impl<'de> DeserializeSeed<'de> for Scan<'_> {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            decoder: D,
        ) -> std::result::Result<(), D::Error> {
            self.budget.nodes += 1;
            if self.depth > 64 || self.budget.nodes > MAX_SOURCE_NODES {
                return Err(serde::de::Error::custom(
                    "expanded historical YAML work bound exceeded",
                ));
            }
            decoder.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Scan<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("bounded original historical YAML")
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i128<E: serde::de::Error>(self, _: i128) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u128<E: serde::de::Error>(self, _: u128) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<(), E> {
            self.budget.scalar_bytes = self.budget.scalar_bytes.saturating_add(value.len());
            if self.budget.scalar_bytes > MAX_SOURCE_BYTES {
                return Err(serde::de::Error::custom(
                    "expanded historical YAML scalar bytes exceed 32 MiB",
                ));
            }
            Ok(())
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<(), A::Error> {
            while values
                .next_element_seed(Scan {
                    budget: &mut *self.budget,
                    depth: self.depth + 1,
                })?
                .is_some()
            {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> std::result::Result<(), A::Error> {
            while values
                .next_key_seed(Scan {
                    budget: &mut *self.budget,
                    depth: self.depth + 1,
                })?
                .is_some()
            {
                values.next_value_seed(Scan {
                    budget: &mut *self.budget,
                    depth: self.depth + 1,
                })?;
            }
            Ok(())
        }
        fn visit_enum<A: EnumAccess<'de>>(self, value: A) -> std::result::Result<(), A::Error> {
            let (_, contents) = value.variant_seed(Scan {
                budget: &mut *self.budget,
                depth: self.depth + 1,
            })?;
            contents.newtype_variant_seed(Scan {
                budget: self.budget,
                depth: self.depth + 1,
            })
        }
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid("historical source must be UTF-8"))?;
    let newline = if text.starts_with("---\r\n") {
        "\r\n"
    } else if text.starts_with("---\n") {
        "\n"
    } else {
        return Err(invalid(
            "historical source is missing its frontmatter opener",
        ));
    };
    let start = 3 + newline.len();
    let end = text[start..]
        .find(&format!("{newline}---{newline}"))
        .map(|end| start + end)
        .ok_or_else(|| invalid("historical source is missing its frontmatter terminator"))?;
    Scan { budget, depth: 0 }
        .deserialize(serde_yaml::Deserializer::from_str(&text[start..end]))
        .map_err(|_| {
            invalid("historical source is invalid or exceeds complete expanded YAML bounds")
        })
}

/// Bind an installed policy to exact literal worktree/Git/common storage paths.
/// This performs only bounded sanitized Git reads and never creates a repository.
pub(crate) fn repository_identity(root: &Path) -> Result<[String; 3]> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("reader repository root must be a real directory"));
    }
    let root = root.canonicalize()?;
    let raw = checked(
        &root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ],
        16_384,
    )?;
    let text = std::str::from_utf8(&raw)
        .map_err(|_| invalid("reader repository identity must be UTF-8"))?;
    let paths = text.lines().collect::<Vec<_>>();
    if paths.len() != 3
        || paths
            .iter()
            .any(|path| path.is_empty() || path.len() > 4096)
    {
        return Err(invalid("reader repository identity is invalid"));
    }
    let mut result = Vec::new();
    for path in paths {
        let path = Path::new(path).canonicalize()?;
        if !path.is_dir() {
            return Err(invalid("reader repository identity requires directories"));
        }
        result.push(
            path.to_str()
                .ok_or_else(|| invalid("reader repository identity must be UTF-8"))?
                .to_owned(),
        );
    }
    if Path::new(&result[0]) != root {
        return Err(invalid(
            "reader policy requires the exact Git repository root",
        ));
    }
    Ok(result.try_into().expect("three verified repository paths"))
}

/// New reader preflight distinguishes known count/byte exhaustion without
/// classifying existing parser errors by their message text.
#[derive(Debug)]
pub(crate) enum ReaderSourceError {
    Limit,
    Source(Error),
}
impl From<Error> for ReaderSourceError {
    fn from(error: Error) -> Self {
        Self::Source(error)
    }
}

/// Inspect authenticated original trees and bounded object-size metadata before
/// reading any full canonical or manuscript source blob. Other private stores
/// are independent of reader presentation in the native model.
pub(crate) fn preflight_reader_sources(
    root: &Path,
    revision: &str,
) -> std::result::Result<(), ReaderSourceError> {
    let repository = Repository::open(root, revision)?;
    let canonical = repository
        .tree("story")?
        .into_iter()
        .filter(|entry| entry.path.ends_with(".md"))
        .collect::<Vec<_>>();
    if canonical.len() > MAX_SOURCE_RECORDS {
        return Err(ReaderSourceError::Limit);
    }
    let mut canonical_bytes = 0usize;
    for entry in canonical {
        let bytes = repository.object_size(&entry.oid)?;
        canonical_bytes = canonical_bytes
            .checked_add(bytes)
            .ok_or(ReaderSourceError::Limit)?;
        if bytes > 2 * 1024 * 1024 || canonical_bytes > MAX_SOURCE_BYTES {
            return Err(ReaderSourceError::Limit);
        }
    }
    let mut private_count = 0usize;
    let mut private_bytes = 0usize;
    for prefix in ["author/segments", "author/works", "author/reveals"] {
        let entries = repository.tree(prefix)?;
        private_count = private_count
            .checked_add(entries.len())
            .ok_or(ReaderSourceError::Limit)?;
        if private_count > 1024 {
            return Err(ReaderSourceError::Limit);
        }
        for entry in entries {
            let bytes = repository.object_size(&entry.oid)?;
            private_bytes = private_bytes
                .checked_add(bytes)
                .ok_or(ReaderSourceError::Limit)?;
            if bytes > 2 * 1024 * 1024 || private_bytes > 16 * 1024 * 1024 {
                return Err(ReaderSourceError::Limit);
            }
        }
    }
    Ok(())
}

/// A bounded original capture. Semantic validation is deliberately left to the
/// caller so revision diagnostics can distinguish parsing and domain validity.
pub(crate) fn load_world(root: &Path, revision: &str) -> Result<World> {
    load_world_with_file_limit(root, revision, MAX_SOURCE_FILE_BYTES)
}

/// Reader capture can impose a stricter blob ceiling without weakening the
/// original loader's complete raw/expanded source bounds or authentication.
pub(crate) fn load_world_with_file_limit(
    root: &Path,
    revision: &str,
    max_file_bytes: usize,
) -> Result<World> {
    load_world_bounded(root, revision, max_file_bytes, &mut WorldBudget::default())
}

/// Original evidence can share the existing complete source/YAML allowance
/// across revisions rather than receiving a fresh allowance for every pointer.
pub(crate) fn load_world_with_budget(
    root: &Path,
    revision: &str,
    budget: &mut WorldBudget,
) -> Result<World> {
    load_world_bounded(root, revision, MAX_SOURCE_FILE_BYTES, budget)
}

fn load_world_bounded(
    root: &Path,
    revision: &str,
    max_file_bytes: usize,
    budget: &mut WorldBudget,
) -> Result<World> {
    if max_file_bytes == 0 || max_file_bytes > MAX_SOURCE_FILE_BYTES {
        return Err(invalid("historical file bound exceeds the source ceiling"));
    }
    let repository = Repository::open(root, revision)?;
    let entries: Vec<_> = repository
        .tree("story")?
        .into_iter()
        .filter(|entry| entry.path.ends_with(".md"))
        .collect();
    budget.records = budget
        .records
        .checked_add(entries.len())
        .filter(|count| *count <= MAX_SOURCE_RECORDS)
        .ok_or_else(|| invalid("historical canonical source exceeds 4096 records"))?;
    let mut records = Vec::with_capacity(entries.len());
    for entry in entries {
        let blob = repository.blob(
            entry,
            max_file_bytes.min(MAX_SOURCE_BYTES - budget.raw_bytes),
        )?;
        budget.raw_bytes += blob.bytes.len();
        preflight_source(&blob.bytes, &mut budget.source)?;
        let mut record = source::parse_record(&blob.bytes, &blob.path)?;
        record.blob_oid = Some(blob.oid);
        records.push(record);
    }
    source::world_from_records(repository.root, records, Some(repository.revision))
}

/// Capture a current working world with the same source/YAML allocation bounds
/// as original Git reads. Native publication and concurrent file-image changes
/// refuse the whole capture; this reader never creates cache or lock files.
pub fn load_current_world(root: &Path) -> Result<World> {
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Stamp {
        bytes: u64,
        modified: std::time::SystemTime,
        #[cfg(unix)]
        device: u64,
        #[cfg(unix)]
        inode: u64,
    }
    fn stamp(metadata: &fs::Metadata) -> Result<Stamp> {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Stamp {
            bytes: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }
    fn drift() -> Error {
        Error::Conflict("working source changed during bounded capture; reload".into())
    }
    fn paths(root: &Path) -> Result<BTreeMap<String, Stamp>> {
        let directory = root.join("story");
        let metadata = fs::symlink_metadata(&directory)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(invalid("working story source must be a real directory"));
        }
        let mut entries = 0usize;
        let mut total = 0u64;
        let mut result = BTreeMap::new();
        for entry in walkdir::WalkDir::new(&directory)
            .follow_links(false)
            .sort_by_file_name()
        {
            let entry = entry.map_err(|_| invalid("working source walk failed"))?;
            entries += 1;
            if entries > MAX_TREE_OBJECTS {
                return Err(invalid("working source exceeds its complete entry bound"));
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("working source may not contain symlinks"));
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .ok()
                .and_then(|path| path.to_str())
                .ok_or_else(|| invalid("working source path must be relative UTF-8"))?;
            safe_path(relative)?;
            if !metadata.is_file()
                || entry.path().extension().and_then(|s| s.to_str()) != Some("md")
            {
                continue;
            }
            total = total.saturating_add(metadata.len());
            if metadata.len() > MAX_SOURCE_FILE_BYTES as u64
                || total > MAX_SOURCE_BYTES as u64
                || result.len() >= MAX_SOURCE_RECORDS
            {
                return Err(invalid("working source exceeds its complete source bounds"));
            }
            result.insert(relative.to_owned(), stamp(&metadata)?);
        }
        Ok(result)
    }
    fn read(root: &Path, path: &str, expected: &Stamp) -> Result<Vec<u8>> {
        let path = root.join(path);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || stamp(&metadata)? != *expected
        {
            return Err(drift());
        }
        let file = fs::File::open(&path)?;
        let opened = file.metadata()?;
        if !opened.is_file() || stamp(&opened)? != *expected {
            return Err(drift());
        }
        let mut bytes = Vec::with_capacity(expected.bytes as usize);
        file.take(expected.bytes + 1).read_to_end(&mut bytes)?;
        let final_metadata = fs::symlink_metadata(&path)?;
        if bytes.len() as u64 != expected.bytes
            || final_metadata.file_type().is_symlink()
            || !final_metadata.is_file()
            || stamp(&final_metadata)? != *expected
        {
            return Err(drift());
        }
        Ok(bytes)
    }
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("working repository root must be a real directory"));
    }
    let root = root.canonicalize()?;
    let publication = crate::publication::read_stamp(&root)?;
    let selected = paths(&root)?;
    let mut records = Vec::with_capacity(selected.len());
    let mut budget = SourceBudget::default();
    for (path, metadata) in &selected {
        let bytes = read(&root, path, metadata)?;
        preflight_source(&bytes, &mut budget)?;
        records.push(source::parse_record(&bytes, path)?);
    }
    if paths(&root)? != selected {
        return Err(drift());
    }
    for record in &records {
        if read(&root, &record.source_path, &selected[&record.source_path])? != record.raw_bytes {
            return Err(drift());
        }
    }
    if paths(&root)? != selected || crate::publication::read_stamp(&root)? != publication {
        return Err(drift());
    }
    source::world_from_records(root, records, None)
}

/// Select the live HEAD exactly once, then prove that the selected literal OID
/// is an original commit in this exact repository. This is comparison metadata,
/// never an alias accepted by the immutable-source reader.
pub(crate) fn head(root: &Path) -> Result<String> {
    let bytes = checked(root, &["rev-parse", "--verify", "HEAD"], 128)?;
    let revision = std::str::from_utf8(&bytes)
        .map(str::trim)
        .ok()
        .filter(|value| valid_oid(value))
        .ok_or_else(|| invalid("live HEAD must be one full commit OID"))?;
    Repository::open(root, revision)?;
    Ok(revision.into())
}

/// Original first-parent commits after `base`, in chronological order. A full
/// base sentinel is required within the requested 1..256 commit bound; merging
/// an unrelated base through a second parent never establishes this history.
pub(crate) fn first_parent_history(
    root: &Path,
    base: &str,
    descendant: &str,
    maximum: usize,
) -> Result<Vec<String>> {
    if !(1..=256).contains(&maximum) {
        return Err(invalid("retcon history requires a 1..256 commit bound"));
    }
    let repository = Repository::open(root, descendant)?;
    repository.commit(base)?;
    let mut current = descendant.to_owned();
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    while current != base {
        if result.len() >= maximum || !seen.insert(current.clone()) {
            return Err(invalid(
                "complete original first-parent history exceeds its bound",
            ));
        }
        let bytes = repository.commit(&current)?;
        total = total.saturating_add(bytes.len());
        if total > MAX_ANCESTRY_BYTES {
            return Err(invalid("original first-parent commit bytes exceed 16 MiB"));
        }
        let mut first_parent = None;
        let mut count = 0usize;
        for line in bytes
            .split(|byte| *byte == b'\n')
            .take_while(|line| !line.is_empty())
        {
            let Some(parent) = line.strip_prefix(b"parent ") else {
                continue;
            };
            let parent = std::str::from_utf8(parent)
                .ok()
                .filter(|parent| valid_oid(parent) && parent.len() == base.len())
                .ok_or_else(|| invalid("original first parent must be an exact OID"))?;
            count += 1;
            if count > 64 {
                return Err(invalid("original commit exceeds 64 parents"));
            }
            if first_parent.is_none() {
                first_parent = Some(parent.to_owned());
            }
        }
        result.push(current);
        current = first_parent.ok_or_else(|| {
            Error::NotFound("original base is outside the complete first-parent history".into())
        })?;
    }
    result.reverse();
    Ok(result)
}

/// Protect private historical identity against a caller-supplied World whose
/// parsed values, paths or raw bytes do not belong to its claimed commit.
pub(crate) fn verify_world(world: &World) -> Result<()> {
    let revision = world
        .revision
        .as_deref()
        .ok_or_else(|| invalid("a historical capture requires its original commit"))?;
    let captured = load_world(&world.root, revision)?;
    if captured.fingerprint != world.fingerprint
        || captured.records.len() != world.records.len()
        || captured.records.iter().any(|(id, expected)| {
            world.records.get(id).is_none_or(|actual| {
                actual.id != expected.id
                    || actual.kind != expected.kind
                    || actual.title != expected.title
                    || actual.source_path != expected.source_path
                    || actual.frontmatter != expected.frontmatter
                    || actual.body != expected.body
                    || actual.raw_bytes != expected.raw_bytes
                    || actual.blob_oid != expected.blob_oid
            })
        })
    {
        return Err(Error::Conflict(
            "historical capture differs from its original Git source".into(),
        ));
    }
    if !validation::validate_world(&captured).valid {
        return Err(Error::Validation(
            "historical canonical source is invalid".into(),
        ));
    }
    Ok(())
}

/// Walk original commit parents with complete work/byte bounds. This avoids
/// mutable grafts, shallow boundaries and replacement refs changing ancestry.
pub(crate) fn is_ancestor(root: &Path, base: &str, descendant: &str) -> Result<bool> {
    let repository = Repository::open(root, base)?;
    repository.commit(descendant)?;
    let mut pending = vec![descendant.to_owned()];
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    while let Some(commit) = pending.pop() {
        if commit == base {
            return Ok(true);
        }
        if !seen.insert(commit.clone()) {
            continue;
        }
        if seen.len() > MAX_ANCESTRY_COMMITS {
            return Err(invalid("historical ancestry exceeds 8192 original commits"));
        }
        let bytes = repository.commit(&commit)?;
        total = total.saturating_add(bytes.len());
        if total > MAX_ANCESTRY_BYTES {
            return Err(invalid(
                "historical ancestry exceeds 16 MiB of original commit bytes",
            ));
        }
        let header = bytes
            .split(|byte| *byte == b'\n')
            .take_while(|line| !line.is_empty());
        let mut parents = 0usize;
        for line in header {
            let Some(parent) = line.strip_prefix(b"parent ") else {
                continue;
            };
            let parent = std::str::from_utf8(parent)
                .map_err(|_| invalid("original commit parent must be an exact OID"))?;
            if !valid_oid(parent) || parent.len() != base.len() {
                return Err(invalid("original commit parent must be an exact OID"));
            }
            parents += 1;
            if parents > 64 {
                return Err(invalid("original commit exceeds 64 parents"));
            }
            pending.push(parent.to_owned());
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(arguments)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_AUTHOR_NAME", "Historical helper test")
            .env("GIT_AUTHOR_EMAIL", "history@example.invalid")
            .env("GIT_COMMITTER_NAME", "Historical helper test")
            .env("GIT_COMMITTER_EMAIL", "history@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    fn fixture(format: &str) -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        git(
            root.path(),
            &["init", "--quiet", &format!("--object-format={format}")],
        );
        fs::create_dir(root.path().join("story")).unwrap();
        fs::write(root.path().join("story/a.md"), b"old exact source\r\n").unwrap();
        fs::write(root.path().join("story/b.md"), b"other exact source\n").unwrap();
        git(root.path(), &["add", "--", "story"]);
        git(root.path(), &["commit", "--quiet", "-m", "original"]);
        let revision = git(root.path(), &["rev-parse", "HEAD"]);
        (root, revision)
    }

    #[test]
    fn original_blobs_preserve_both_git_hash_formats_and_never_accept_ref_aliases() {
        for format in ["sha1", "sha256"] {
            let (directory, revision) = fixture(format);
            let root = directory.path();
            fs::write(root.join("story/a.md"), b"later live content").unwrap();
            let blob = read_blob(root, &revision, "story/a.md", 128).unwrap();
            assert_eq!(blob.bytes, b"old exact source\r\n");
            assert_eq!(blob.revision, revision);
            assert_eq!(
                blob.oid,
                git(root, &["rev-parse", &format!("{revision}:story/a.md")])
            );
            assert_eq!(blob.oid.len(), if format == "sha1" { 40 } else { 64 });
            for alias in [
                "HEAD".to_owned(),
                revision[..12].to_owned(),
                revision.to_uppercase(),
                format!("{revision}^{{commit}}"),
                blob.oid.clone(),
            ] {
                assert!(read_blob(root, &alias, "story/a.md", 128).is_err());
            }
            assert!(read_blob(&root.join("story"), &revision, "a.md", 128).is_err());
        }
    }

    #[test]
    fn complete_tree_and_blob_bounds_refuse_without_falling_back_or_truncating() {
        let (directory, revision) = fixture("sha1");
        let root = directory.path();
        let values = read_tree(root, &revision, "story", 2, 128, 256).unwrap();
        assert_eq!(
            values
                .iter()
                .map(|blob| blob.path.as_str())
                .collect::<Vec<_>>(),
            vec!["story/a.md", "story/b.md"]
        );
        assert!(
            read_tree(root, &revision, "missing", 0, 0, 0)
                .unwrap()
                .is_empty()
        );
        for (count, per_file, total) in [(1, 128, 256), (2, 4, 256), (2, 128, 20)] {
            assert!(read_tree(root, &revision, "story", count, per_file, total).is_err());
        }
        assert!(read_blob(root, &revision, "story/a.md", 2).is_err());
        assert!(read_blob(root, &revision, "story/absent.md", 128).is_err());
        for path in [
            "",
            "/story/a.md",
            "story/../a.md",
            "story/./a.md",
            "story//a.md",
            "story\\a.md",
            "story/a.md/child",
            "story/*",
        ] {
            assert!(read_blob(root, &revision, path, 128).is_err(), "{path}");
        }
        assert!(read_tree(root, &revision, "story/a.md", 2, 128, 256).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            // Unrelated repository siblings do not become canonical path input.
            fs::write(
                root.join(std::ffi::OsString::from_vec(vec![0xff])),
                b"unrelated",
            )
            .unwrap();
            git(root, &["add", "--", "."]);
            git(
                root,
                &["commit", "--quiet", "-m", "unrelated non-UTF8 sibling"],
            );
            let later = git(root, &["rev-parse", "HEAD"]);
            assert_eq!(
                read_tree(root, &later, "story", 2, 128, 256).unwrap().len(),
                2
            );
        }
    }

    #[test]
    fn literal_colon_components_preserve_native_source_paths() {
        let (directory, _) = fixture("sha1");
        let root = directory.path();
        let path = "story/part:one/entry:two.md";
        let original = b"original colon-named source\r\n";
        fs::create_dir(root.join("story/part:one")).unwrap();
        fs::write(root.join(path), original).unwrap();
        git(root, &["add", "--", "story"]);
        git(root, &["commit", "--quiet", "-m", "native colon paths"]);
        let revision = git(root, &["rev-parse", "HEAD"]);
        let oid = git(root, &["hash-object", "--", path]);
        fs::write(root.join(path), b"different live bytes").unwrap();
        let blob = read_blob(root, &revision, path, 128).unwrap();
        assert_eq!(blob.path, path);
        assert_eq!(blob.oid, oid);
        assert_eq!(blob.bytes, original);
        let complete = read_tree(root, &revision, "story", 3, 128, 384).unwrap();
        assert_eq!(complete.len(), 3);
        let selected = read_tree(root, &revision, "story/part:one", 1, 128, 128).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].path, path);
        assert_eq!(selected[0].oid, oid);
        assert_eq!(selected[0].bytes, original);
    }

    #[test]
    fn a_corrupted_loose_blob_cannot_keep_its_claimed_original_identity() {
        let (directory, revision) = fixture("sha1");
        let root = directory.path();
        let a = git(root, &["rev-parse", &format!("{revision}:story/a.md")]);
        let b = git(root, &["rev-parse", &format!("{revision}:story/b.md")]);
        let object_path = |oid: &str| root.join(".git/objects").join(&oid[..2]).join(&oid[2..]);
        fs::remove_file(object_path(&a)).unwrap();
        fs::copy(object_path(&b), object_path(&a)).unwrap();
        assert!(read_blob(root, &revision, "story/a.md", 128).is_err());
    }

    #[test]
    fn original_path_membership_refuses_corrupted_root_and_nested_tree_objects() {
        for nested in [false, true] {
            let (directory, original) = fixture("sha1");
            let root = directory.path();
            let selector = if nested { ":story" } else { "^{tree}" };
            let before = git(root, &["rev-parse", &format!("{original}{selector}")]);
            fs::write(root.join("story/a.md"), b"different later blob").unwrap();
            git(root, &["add", "--", "story"]);
            git(
                root,
                &["commit", "--quiet", "-m", "different original tree"],
            );
            let after = git(root, &["rev-parse", &format!("HEAD{selector}")]);
            let object_path = |oid: &str| root.join(".git/objects").join(&oid[..2]).join(&oid[2..]);
            fs::remove_file(object_path(&before)).unwrap();
            fs::copy(object_path(&after), object_path(&before)).unwrap();
            assert!(read_blob(root, &original, "story/a.md", 128).is_err());
            assert!(read_tree(root, &original, "story", 2, 128, 256).is_err());
        }
    }

    #[test]
    fn historical_yaml_alias_expansion_is_bounded_before_native_value_construction() {
        let directory = tempfile::tempdir().unwrap();
        let world =
            source::init_world(&directory.path().join("world"), "Original alias source").unwrap();
        let root = &world.root;
        git(root, &["init", "--quiet"]);
        let original = String::from_utf8(world.world_record().unwrap().raw_bytes.clone()).unwrap();
        let write = |fields: &str| {
            fs::write(
                root.join(&world.world_record().unwrap().source_path),
                original.replacen("\n---\n", &format!("\n{fields}\n---\n"), 1),
            )
            .unwrap();
            git(root, &["add", "--", "story"]);
            git(
                root,
                &["commit", "--quiet", "-m", "authored source aliases"],
            );
            git(root, &["rev-parse", "HEAD"])
        };
        let small = write("x-anchor: &shared {note: small}\nx-values: [*shared, *shared]");
        let captured = load_world(root, &small).unwrap();
        assert!(validation::validate_world(&captured).valid);
        assert_eq!(
            captured
                .world_record()
                .unwrap()
                .get("x-values")
                .unwrap()
                .as_sequence()
                .unwrap()
                .len(),
            2
        );
        let large = "x".repeat(1024 * 1024);
        let aliases = ["*shared"; 33].join(",");
        let large_revision = write(&format!("x-anchor: &shared {large}\nx-values: [{aliases}]"));
        let error = load_world(root, &large_revision).unwrap_err();
        assert!(error.to_string().contains("expanded YAML"), "{error}");
    }

    #[test]
    fn ancestry_uses_original_parents_even_when_git_grafts_or_shallow_markers_disagree() {
        let (directory, base) = fixture("sha1");
        let root = directory.path();
        fs::write(root.join("story/a.md"), b"descendant bytes").unwrap();
        git(root, &["add", "--", "story"]);
        git(root, &["commit", "--quiet", "-m", "descendant"]);
        let descendant = git(root, &["rev-parse", "HEAD"]);
        let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
        let unrelated = git(
            root,
            &["commit-tree", &tree, "-m", "unrelated original root"],
        );
        fs::create_dir_all(root.join(".git/info")).unwrap();
        fs::write(
            root.join(".git/info/grafts"),
            format!("{descendant} {unrelated}\n"),
        )
        .unwrap();
        fs::write(root.join(".git/shallow"), format!("{descendant}\n")).unwrap();
        assert!(is_ancestor(root, &base, &descendant).unwrap());
        assert!(is_ancestor(root, &base, &base).unwrap());
        assert!(!is_ancestor(root, &unrelated, &descendant).unwrap());
        assert!(!is_ancestor(root, &descendant, &base).unwrap());
        assert!(is_ancestor(root, "HEAD", &descendant).is_err());
    }
}
