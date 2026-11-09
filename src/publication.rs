//! Recoverable publication of one reviewed outcome under the shared writer lock.
//!
//! Filesystems do not provide a multi-file rename transaction. A durable journal
//! fences native readers until every exact file has been published and checked.
//! Interrupted publications remain fenced until explicit guarded rollback. The
//! generation fence also detects a transaction completed entirely during a read.
//! External editors must coordinate with the advisory writer lock.
use crate::{
    Error, Result, action_drafts, authoring, edit_drafts, model::World, source, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const JOURNAL: &str = "pending-outcome.json";
const GENERATION: &str = "publication-generation";
const MAX_CHANGES: usize = 65;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct FileChange {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JournalChange {
    path: String,
    before_source: Option<String>,
    after_source: String,
    before_sha256: Option<String>,
    after_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    protocol: String,
    base_fingerprint: String,
    candidate_fingerprint: String,
    changes: Vec<JournalChange>,
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn conflict(message: impl Into<String>) -> Error {
    Error::Conflict(message.into())
}

fn pending() -> Error {
    conflict(
        "an outcome publication is pending; inspect publication status and explicitly recover before reading or writing current source",
    )
}

fn root_path(root: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("publication root must be a real directory"));
    }
    Ok(root.canonicalize()?)
}

fn state_directory(root: &Path) -> Result<Option<PathBuf>> {
    let path = root.join(".wedl-rust");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(invalid("publication state must be a real directory"))
        }
        Ok(_) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn read_file(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid(
            "publication files must be regular files, not links or special files",
        ));
    }
    if metadata.len() > maximum as u64 {
        return Err(invalid("publication file exceeds its complete byte bound"));
    }
    let file = fs::File::open(path)?;
    let after_open = fs::symlink_metadata(path)?;
    if after_open.file_type().is_symlink() || !after_open.is_file() || !file.metadata()?.is_file() {
        return Err(invalid("publication files must be regular files"));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(invalid("publication file exceeds its complete byte bound"));
    }
    Ok(Some(bytes))
}

fn generation(directory: &Path) -> Result<Option<String>> {
    read_file(&directory.join(GENERATION), 65)?
        .map(|bytes| {
            let value = std::str::from_utf8(&bytes)
                .map_err(|_| invalid("publication generation must be UTF-8"))?;
            let token = value.strip_suffix('\n').unwrap_or(value);
            if !valid_hash(token) {
                return Err(invalid("invalid publication generation"));
            }
            Ok(token.to_owned())
        })
        .transpose()
}

/// This read-only fence creates nothing. Compare one stamp before walking source
/// with another after constructing the World, including error paths.
pub(crate) fn read_stamp(root: &Path) -> Result<Option<String>> {
    let Some(directory) = state_directory(root)? else {
        return Ok(None);
    };
    let stamp = generation(&directory)?;
    match fs::symlink_metadata(directory.join(JOURNAL)) {
        Ok(_) => Err(pending()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(stamp),
        Err(error) => Err(error.into()),
    }
}

fn safe_relative(path: &str) -> Result<bool> {
    let relative = Path::new(path);
    if path.is_empty()
        || path.len() > 4096
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("publication requires exact relative source paths"));
    }
    if path.starts_with("story/")
        && relative.extension().and_then(|value| value.to_str()) == Some("md")
    {
        return Ok(true);
    }
    if let Some(id) = path
        .strip_prefix("author/actions/")
        .and_then(|name| name.strip_suffix(".json"))
        && action_drafts::valid_action_id(id)
    {
        return Ok(false);
    }
    if let Some(id) = path
        .strip_prefix("author/edits/")
        .and_then(|name| name.strip_suffix(".json"))
        && edit_drafts::valid_edit_id(id)
    {
        return Ok(false);
    }
    Err(invalid(
        "publication may change only canonical Markdown and one private action or edit document",
    ))
}

fn target(root: &Path, relative: &str, create_parents: bool) -> Result<PathBuf> {
    safe_relative(relative)?;
    let parent = Path::new(relative)
        .parent()
        .ok_or_else(|| invalid("source has no parent"))?;
    let mut path = root.to_owned();
    for component in parent.components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid(
                    "publication source parents must be real directories",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create_parents {
                    let mut builder = fs::DirBuilder::new();
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::DirBuilderExt;
                        if relative.starts_with("author/") {
                            builder.mode(0o700);
                        }
                    }
                    builder.create(&path)?;
                    sync_directory(path.parent().unwrap())?;
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(relative))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    // Portable std cannot open a Windows directory for FlushFileBuffers. File
    // contents are synced; crash durability on that platform remains unverified.
    Ok(())
}

fn atomic_file(path: &Path, bytes: &[u8], before: Option<&[u8]>) -> Result<()> {
    let current = read_file(path, MAX_JOURNAL_BYTES)?;
    if current.as_deref() != before {
        return Err(conflict(
            "publication target changed; existing bytes were preserved",
        ));
    }
    let permissions = if current.is_some() {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.permissions().readonly() {
            return Err(invalid("publication target is read-only"));
        }
        Some(metadata.permissions())
    } else {
        None
    };
    let parent = path
        .parent()
        .ok_or_else(|| invalid("publication target has no parent"))?;
    let prefix = match (
        parent
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str()),
        parent.file_name().and_then(|name| name.to_str()),
    ) {
        (Some("author"), Some("actions")) => ".wedl-action-",
        (Some("author"), Some("edits")) => ".wedl-edit-",
        _ => ".wedl-publish-",
    };
    let mut temporary = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    // Cooperative writers hold the same lock. This recheck cannot provide an
    // OS compare-and-swap against an uncooperative external editor.
    if read_file(path, MAX_JOURNAL_BYTES)?.as_deref() != before {
        return Err(conflict(
            "publication target changed; existing bytes were preserved",
        ));
    }
    if before.is_none() {
        temporary.persist_noclobber(path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                conflict("publication target appeared; existing bytes were preserved")
            } else {
                Error::Io(error.error)
            }
        })?;
    } else {
        temporary
            .persist(path)
            .map_err(|error| Error::Io(error.error))?;
    }
    sync_directory(parent)
}

fn validate_journal(journal: &Journal) -> Result<()> {
    if journal.protocol != crate::CONTRACT
        || !valid_hash(&journal.base_fingerprint)
        || !valid_hash(&journal.candidate_fingerprint)
        || journal.changes.is_empty()
        || journal.changes.len() > MAX_CHANGES
    {
        return Err(invalid("invalid or over-bound publication journal"));
    }
    let mut paths = BTreeSet::new();
    let mut private_count = 0;
    let mut byte_count = 0usize;
    for change in &journal.changes {
        if !paths.insert(change.path.to_lowercase()) {
            return Err(invalid("duplicate publication source path"));
        }
        let canonical = safe_relative(&change.path)?;
        if !canonical {
            private_count += 1;
            if change.before_source.is_none() {
                return Err(invalid(
                    "outcome publication requires an existing private author document",
                ));
            }
            for text in change
                .before_source
                .iter()
                .chain(std::iter::once(&change.after_source))
            {
                let path = if change.path.starts_with("author/edits/") {
                    format!("author/edits/{}.json", edit_drafts::decode(text)?.id)
                } else {
                    format!("author/actions/{}.json", action_drafts::decode(text)?.id)
                };
                if change.path != path {
                    return Err(invalid(
                        "private author journal identity differs from its path",
                    ));
                }
            }
        } else {
            for text in change
                .before_source
                .iter()
                .chain(std::iter::once(&change.after_source))
            {
                source::parse_record(text.as_bytes(), &change.path)?;
            }
        }
        byte_count = byte_count
            .saturating_add(change.after_source.len())
            .saturating_add(change.before_source.as_ref().map_or(0, String::len));
        if byte_count > MAX_SOURCE_BYTES
            || change
                .before_source
                .as_ref()
                .map(|text| hash(text.as_bytes()))
                != change.before_sha256
            || hash(change.after_source.as_bytes()) != change.after_sha256
        {
            return Err(invalid(
                "publication journal source bounds or hashes do not match",
            ));
        }
    }
    if private_count != 1 {
        return Err(invalid(
            "outcome publication requires exactly one private author document",
        ));
    }
    Ok(())
}

fn read_journal(root: &Path) -> Result<Option<(Journal, Vec<u8>)>> {
    let Some(directory) = state_directory(root)? else {
        return Ok(None);
    };
    let Some(bytes) = read_file(&directory.join(JOURNAL), MAX_JOURNAL_BYTES)? else {
        return Ok(None);
    };
    let journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("invalid, duplicate or unknown publication journal fields"))?;
    validate_journal(&journal)?;
    Ok(Some((journal, bytes)))
}

fn inspect_targets(root: &Path, journal: &Journal, exact_before: bool) -> Result<()> {
    for change in &journal.changes {
        let path = target(root, &change.path, false)?;
        let current = read_file(&path, MAX_SOURCE_BYTES)?;
        let before = change.before_source.as_deref().map(str::as_bytes);
        if current.as_deref() != before
            && (exact_before || current.as_deref() != Some(change.after_source.as_bytes()))
        {
            return Err(conflict(format!(
                "publication target {:?} has unrecognized bytes; no recovery changes were made",
                change.path
            )));
        }
        if current.is_some() && fs::symlink_metadata(&path)?.permissions().readonly() {
            return Err(invalid("publication target is read-only"));
        }
    }
    Ok(())
}

/// Reconstruct both complete snapshots using exact retained before/after files.
/// The unchecked loader is confined to this module and still rejects unsafe
/// source. It is required while the pending journal intentionally fences reads.
fn reconstructed(root: &Path, journal: &Journal) -> Result<(World, World)> {
    let current = source::load_worktree_unfenced(root.to_owned())?;
    let mut base: BTreeMap<_, _> = current
        .records
        .values()
        .map(|record| (record.source_path.clone(), record.clone()))
        .collect();
    for change in &journal.changes {
        if safe_relative(&change.path)? {
            base.remove(&change.path);
            if let Some(text) = &change.before_source {
                base.insert(
                    change.path.clone(),
                    source::parse_record(text.as_bytes(), &change.path)?,
                );
            }
        }
    }
    let base = source::world_from_records(root.to_owned(), base.into_values().collect(), None)?;
    let mut candidate: BTreeMap<_, _> = base
        .records
        .values()
        .map(|record| (record.source_path.clone(), record.clone()))
        .collect();
    for change in &journal.changes {
        if safe_relative(&change.path)? {
            candidate.insert(
                change.path.clone(),
                source::parse_record(change.after_source.as_bytes(), &change.path)?,
            );
        }
    }
    let candidate =
        source::world_from_records(root.to_owned(), candidate.into_values().collect(), None)?;
    if base.fingerprint != journal.base_fingerprint
        || candidate.fingerprint != journal.candidate_fingerprint
    {
        return Err(conflict(
            "publication snapshots no longer match the journal; unrelated canonical edits must be reconciled before recovery",
        ));
    }
    if !validation::validate_world(&base).valid || !validation::validate_world(&candidate).valid {
        return Err(Error::Validation(
            "publication journal contains an invalid base or candidate".into(),
        ));
    }
    let world_id = base.world_record()?.id.as_str();
    if candidate.world_record()?.id != world_id {
        return Err(invalid("publication cannot change world identity"));
    }
    for change in &journal.changes {
        if !safe_relative(&change.path)? {
            let before_source = change
                .before_source
                .as_deref()
                .expect("validated private before source");
            if change.path.starts_with("author/edits/") {
                let canonical: Vec<_> = journal
                    .changes
                    .iter()
                    .filter(|item| item.path.starts_with("story/"))
                    .map(|item| FileChange {
                        path: item.path.clone(),
                        before: item
                            .before_source
                            .as_ref()
                            .map(|text| text.as_bytes().to_vec()),
                        after: item.after_source.as_bytes().to_vec(),
                    })
                    .collect();
                edit_drafts::validate_publication(
                    &base,
                    &candidate,
                    before_source,
                    &change.after_source,
                    &canonical,
                )?;
                continue;
            }
            let before = action_drafts::decode(before_source)?;
            let after = action_drafts::decode(&change.after_source)?;
            if before.world_id != world_id || after.world_id != world_id {
                return Err(invalid("publication action belongs to another world"));
            }
            let receipt = after.applied_receipt.as_ref().ok_or_else(|| {
                invalid("publication journal requires an applied outcome receipt")
            })?;
            let mut original = after.clone();
            original.phase = "proposed".into();
            original.applied_receipt = None;
            if before != original
                || before.phase != "proposed"
                || action_drafts::effective_phase(&before, &base.fingerprint) != "proposed"
                || receipt.before_action_sha256 != hash(before_source.as_bytes())
                || receipt.base_fingerprint != base.fingerprint
                || receipt.candidate_fingerprint != candidate.fingerprint
            {
                return Err(invalid(
                    "publication journal private history differs from its reviewed outcome",
                ));
            }
            action_drafts::validate(&base, &before)?;
            let proposal = before
                .proposals
                .iter()
                .find(|proposal| proposal.id == receipt.proposal_id)
                .ok_or_else(|| invalid("publication proposal is unavailable"))?;
            let preview = crate::consequences::preview(
                &base,
                &proposal.consequences,
                receipt.max_characters,
            )?;
            if preview["candidateFingerprint"].as_str() != Some(candidate.fingerprint.as_str()) {
                return Err(invalid(
                    "publication journal candidate differs from its retained proposal",
                ));
            }
            let canonical: Vec<_> = journal
                .changes
                .iter()
                .filter(|change| change.path.starts_with("story/"))
                .collect();
            let files = preview["files"]
                .as_array()
                .expect("native preview file array");
            if canonical.len() != files.len() || files.len() != receipt.files.len() {
                return Err(invalid(
                    "publication journal does not contain the complete ordered proposal",
                ));
            }
            for ((change, file), saved) in canonical.iter().zip(files).zip(&receipt.files) {
                if file["sourcePath"].as_str() != Some(change.path.as_str())
                    || file["beforeSource"].as_str() != change.before_source.as_deref()
                    || file["afterSource"].as_str() != Some(change.after_source.as_str())
                    || file["entityId"].as_str() != Some(saved.entity_id.as_str())
                    || saved.source_path != change.path
                    || saved.before_sha256 != change.before_sha256
                    || saved.after_sha256 != change.after_sha256
                {
                    return Err(invalid(
                        "publication journal files differ from the exact retained proposal or receipt",
                    ));
                }
            }
        }
    }
    Ok((base, candidate))
}

fn advance_generation(directory: &Path, journal_hash: &str, disposition: &str) -> Result<()> {
    let prior = read_file(&directory.join(GENERATION), 65)?;
    let mut material = b"wedl-publication-generation\0".to_vec();
    material.extend_from_slice(prior.as_deref().unwrap_or_default());
    material.extend_from_slice(journal_hash.as_bytes());
    material.extend_from_slice(disposition.as_bytes());
    let next = format!("{}\n", hash(&material));
    atomic_file(
        &directory.join(GENERATION),
        next.as_bytes(),
        prior.as_deref(),
    )
}

/// Caller holds the shared native writer lock and has freshly recomputed the
/// outcome review and sufficiency checks. All exact source/file pins are checked
/// again here before the durable journal becomes visible.
pub(crate) fn publish_locked(
    world: &World,
    candidate_fingerprint: &str,
    changes: &[FileChange],
) -> Result<()> {
    if world.revision.is_some() {
        return Err(invalid("outcome publication requires the current worktree"));
    }
    let root = root_path(&world.root)?;
    let fresh = source::load_world(&root, None)?;
    if fresh.fingerprint != world.fingerprint {
        return Err(conflict(
            "canonical source changed before outcome publication",
        ));
    }
    let journal = Journal {
        protocol: crate::CONTRACT.into(),
        base_fingerprint: world.fingerprint.clone(),
        candidate_fingerprint: candidate_fingerprint.into(),
        changes: changes
            .iter()
            .map(|change| {
                Ok(JournalChange {
                    path: change.path.clone(),
                    before_source: change
                        .before
                        .as_ref()
                        .map(|bytes| {
                            String::from_utf8(bytes.clone())
                                .map_err(|_| invalid("publication before source must be UTF-8"))
                        })
                        .transpose()?,
                    after_source: String::from_utf8(change.after.clone())
                        .map_err(|_| invalid("publication after source must be UTF-8"))?,
                    before_sha256: change.before.as_ref().map(|bytes| hash(bytes)),
                    after_sha256: hash(&change.after),
                })
            })
            .collect::<Result<_>>()?,
    };
    validate_journal(&journal)?;
    inspect_targets(&root, &journal, true)?;
    reconstructed(&root, &journal)?;
    let bytes = serde_json::to_vec(&journal)?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(invalid(
            "publication journal exceeds its complete 64 MiB bound",
        ));
    }
    let directory = state_directory(&root)?
        .ok_or_else(|| invalid("publication requires the shared writer lock"))?;
    generation(&directory)?;
    // The lock may have just created .wedl-rust. Persist that directory entry
    // before making any source mutation depend on its recovery journal.
    sync_directory(&root)?;
    inspect_targets(&root, &journal, true)?;
    if source::load_world(&root, None)?.fingerprint != world.fingerprint {
        return Err(conflict(
            "canonical source changed before outcome publication",
        ));
    }
    atomic_file(&directory.join(JOURNAL), &bytes, None)?;
    // Once the journal is durable, every error leaves it in place. Native
    // readers stay closed and explicit recovery restores the previous snapshot.
    for change in &journal.changes {
        let before = change.before_source.as_deref().map(str::as_bytes);
        if before == Some(change.after_source.as_bytes()) {
            continue;
        }
        let path = target(&root, &change.path, true)?;
        atomic_file(&path, change.after_source.as_bytes(), before)?;
    }
    for change in &journal.changes {
        if read_file(&target(&root, &change.path, false)?, MAX_SOURCE_BYTES)?.as_deref()
            != Some(change.after_source.as_bytes())
        {
            return Err(conflict(
                "published outcome differs from its exact candidate; recovery is required",
            ));
        }
    }
    if source::load_worktree_unfenced(root.clone())?.fingerprint != candidate_fingerprint {
        return Err(conflict(
            "published canonical source differs from its candidate; recovery is required",
        ));
    }
    advance_generation(&directory, &hash(&bytes), "committed")?;
    if read_file(&directory.join(JOURNAL), MAX_JOURNAL_BYTES)?.as_deref() != Some(bytes.as_slice())
    {
        return Err(conflict(
            "publication journal changed; recovery is required",
        ));
    }
    fs::remove_file(directory.join(JOURNAL))?;
    sync_directory(&directory).map_err(|error| Error::Data(format!(
        "outcome source and private receipt are published, but commit durability is unconfirmed ({error}); inspect the saved action before retrying"
    )))?;
    Ok(())
}

/// Inspect pending recovery without loading canonical source or creating files.
pub fn status(root: &Path) -> Result<Json> {
    let root = root_path(root)?;
    let Some((journal, bytes)) = read_journal(&root)? else {
        return Ok(
            json!({"protocol":crate::CONTRACT,"operation":"publication-status","pending":false,"writesPerformed":false}),
        );
    };
    Ok(
        json!({"protocol":crate::CONTRACT,"operation":"publication-status","pending":true,
        "journalSha256":hash(&bytes),"baseFingerprint":journal.base_fingerprint,
        "candidateFingerprint":journal.candidate_fingerprint,"files":journal.changes.iter().map(|change| json!({
            "sourcePath":change.path,"beforeSha256":change.before_sha256,"afterSha256":change.after_sha256
        })).collect::<Vec<_>>(),"recovery":"rollback","writesPerformed":false}),
    )
}

/// Explicit rollback of a pending transaction, pinned to its exact journal.
/// All targets and reconstructed snapshots are checked before the first write.
/// Failure retains the journal and does not overwrite unrecognized external edits.
pub fn recover(root: &Path, expected_sha256: &str) -> Result<Json> {
    if !valid_hash(expected_sha256) {
        return Err(invalid("recovery requires an exact journal SHA-256"));
    }
    let root = root_path(root)?;
    let Some((initial, initial_bytes)) = read_journal(&root)? else {
        return Err(conflict("there is no pending publication to recover"));
    };
    if hash(&initial_bytes) != expected_sha256 {
        return Err(conflict("publication journal changed; inspect it again"));
    }
    inspect_targets(&root, &initial, false)?;
    reconstructed(&root, &initial)?;
    let _lock = authoring::writer_lock(&root)?;
    let Some((journal, bytes)) = read_journal(&root)? else {
        return Err(conflict("pending publication disappeared"));
    };
    if bytes != initial_bytes {
        return Err(conflict("publication journal changed; inspect it again"));
    }
    inspect_targets(&root, &journal, false)?;
    reconstructed(&root, &journal)?;
    for change in journal.changes.iter().rev() {
        let path = target(&root, &change.path, false)?;
        let current = read_file(&path, MAX_SOURCE_BYTES)?;
        let before = change.before_source.as_deref().map(str::as_bytes);
        if current.as_deref() == before {
            continue;
        }
        if current.as_deref() != Some(change.after_source.as_bytes()) {
            return Err(conflict(
                "publication target changed during recovery; recovery journal was retained",
            ));
        }
        if let Some(before) = before {
            atomic_file(&path, before, Some(change.after_source.as_bytes()))?;
        } else {
            // Current bytes were checked immediately before unlinking. External
            // editors must honor the same advisory lock.
            fs::remove_file(&path)?;
            sync_directory(path.parent().unwrap())?;
        }
    }
    inspect_targets(&root, &journal, true)?;
    if source::load_worktree_unfenced(root.clone())?.fingerprint != journal.base_fingerprint {
        return Err(conflict(
            "recovered source differs from its original snapshot; journal retained",
        ));
    }
    let directory =
        state_directory(&root)?.ok_or_else(|| invalid("publication state disappeared"))?;
    advance_generation(&directory, expected_sha256, "rolled-back")?;
    if read_file(&directory.join(JOURNAL), MAX_JOURNAL_BYTES)?.as_deref() != Some(bytes.as_slice())
    {
        return Err(conflict(
            "publication journal changed during recovery; journal retained",
        ));
    }
    fs::remove_file(directory.join(JOURNAL))?;
    sync_directory(&directory).map_err(|error| Error::Data(format!(
        "outcome rollback is visible, but recovery durability is unconfirmed ({error}); inspect source and publication status before retrying"
    )))?;
    Ok(
        json!({"protocol":crate::CONTRACT,"operation":"publication-recover","recovered":true,
        "disposition":"rolled-back","journalSha256":expected_sha256,"fingerprint":journal.base_fingerprint,
        "writesPerformed":true,"notice":"The interrupted outcome was rolled back. Review and approve again before applying."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_transaction_changes_reader_stamp_even_when_marker_is_gone() {
        let root = tempfile::tempdir().unwrap();
        let initial = read_stamp(root.path()).unwrap();
        fs::create_dir(root.path().join(".wedl-rust")).unwrap();
        let directory = root.path().join(".wedl-rust");
        fs::write(directory.join(JOURNAL), "pending").unwrap();
        assert!(read_stamp(root.path()).is_err());
        advance_generation(&directory, &"1".repeat(64), "committed").unwrap();
        fs::remove_file(directory.join(JOURNAL)).unwrap();
        let committed = read_stamp(root.path()).unwrap();
        assert_ne!(committed, initial);
        // Reusing identical publication material still advances the fence.
        advance_generation(&directory, &"1".repeat(64), "committed").unwrap();
        assert_ne!(read_stamp(root.path()).unwrap(), committed);
    }
}
