//! Durable private author buffers, separate from canonical story source.
//! Every write binds the current source snapshot and exact prior draft bytes.
use crate::{
    Error, Result,
    action_drafts::{self, Draft, SaveRequest},
    authoring, consequences,
    model::World,
    source, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_DOCUMENTS: usize = 1024;
const MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 2048;

pub(crate) struct Stored {
    pub(crate) draft: Draft,
    pub(crate) bytes: Vec<u8>,
    pub(crate) hash: String,
    blob_oid: Option<String>,
}

struct Snapshot {
    documents: BTreeMap<String, Stored>,
    bytes: usize,
}

struct Prepared {
    before: Option<Stored>,
    bytes: Vec<u8>,
    hash: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn conflict() -> Error {
    Error::Conflict("private action draft changed; reload it before saving".into())
}

fn bounds() -> Error {
    invalid("private action store exceeds its complete document or byte limit")
}

fn exact_id(id: &str) -> Result<()> {
    if !action_drafts::valid_action_id(id) {
        return Err(invalid("an exact private action ID is required"));
    }
    Ok(())
}

fn exact_hash(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "an exact lowercase private source SHA-256 is required",
        ));
    }
    Ok(())
}

fn relative_path(id: &str) -> String {
    format!("author/actions/{id}.json")
}

fn current(world: &World) -> Result<()> {
    if world.revision.is_some() {
        return Err(invalid(
            "private action storage requires the current worktree",
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "private action source is invalid; run author validation for diagnostics".into(),
        ));
    }
    let fresh = source::load_world(&world.root, None)?;
    if fresh.fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source changed since capture; load the current snapshot again".into(),
        ));
    }
    Ok(())
}

/// Bind canonical currentness and the entire private capture to one publication
/// generation. The source fingerprint alone cannot detect a canonical no-op
/// outcome that changes only its private terminal receipt.
fn capture<T>(world: &World, read: impl FnOnce() -> Result<T>) -> Result<T> {
    if world.revision.is_some() {
        current(world)?;
    }
    let before = crate::publication::read_stamp(&world.root)?;
    let result = (|| {
        current(world)?;
        read()
    })();
    if crate::publication::read_stamp(&world.root)? != before {
        return Err(Error::Conflict(
            "outcome publication changed during the private action read; load again".into(),
        ));
    }
    result
}

/// Inspect each component without following symlinks. Read-only calls never
/// create a directory or initialize the writer lock.
fn directory(root: &Path, create: bool) -> Result<Option<PathBuf>> {
    let mut path = root.to_owned();
    for component in ["author", "actions"] {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid("private action parents must be real directories"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !create {
                    return Ok(None);
                }
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(&path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(Some(path))
}

fn read_document(world: &World, path: &Path, id: &str, remaining: usize) -> Result<Stored> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("private action sources must be regular files"));
    }
    let maximum = action_drafts::MAX_DOCUMENT.min(remaining);
    if metadata.len() > maximum as u64 {
        return Err(bounds());
    }
    let file = fs::File::open(path)?;
    let after_open = fs::symlink_metadata(path)?;
    if after_open.file_type().is_symlink() || !after_open.is_file() || !file.metadata()?.is_file() {
        return Err(invalid("private action sources must be regular files"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(bounds());
    }
    decode_document(world, id, bytes)
}

fn decode_document(world: &World, id: &str, bytes: Vec<u8>) -> Result<Stored> {
    let text =
        std::str::from_utf8(&bytes).map_err(|_| invalid("private action source must be UTF-8"))?;
    // A stale target can still be inspected. Current-reference and lifecycle
    // checks belong to the save candidate, not private discovery.
    let draft = action_drafts::decode(text)?;
    if draft.id != id || draft.world_id != world.world_record()?.id {
        return Err(invalid(
            "private action identity does not match its repository or path",
        ));
    }
    let hash = action_drafts::sha256(&bytes);
    Ok(Stored {
        draft,
        bytes,
        hash,
        blob_oid: None,
    })
}

fn snapshot(world: &World) -> Result<Snapshot> {
    let mut result = Snapshot {
        documents: BTreeMap::new(),
        bytes: 0,
    };
    let Some(directory) = directory(&world.root, false)? else {
        return Ok(result);
    };
    // Bound directory entries before collecting paths or decoding documents.
    let mut paths = BTreeMap::new();
    let mut temporary_bytes = 0u64;
    for (index, entry) in fs::read_dir(&directory)?.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(bounds());
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("private action filenames must be exact UTF-8 IDs"))?;
        if name.starts_with(".wedl-action-") {
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(invalid(
                    "private action temporary sources must be regular files",
                ));
            }
            if metadata.len() > action_drafts::MAX_DOCUMENT as u64 {
                return Err(bounds());
            }
            temporary_bytes += metadata.len();
            if temporary_bytes > MAX_STORE_BYTES as u64 {
                return Err(bounds());
            }
            continue;
        }
        if paths.len() >= MAX_DOCUMENTS {
            return Err(bounds());
        }
        let id = name
            .strip_suffix(".json")
            .ok_or_else(|| invalid("unexpected file in the private action store"))?;
        exact_id(id)?;
        paths.insert(id.to_owned(), entry.path());
    }
    for (id, path) in paths {
        let document = read_document(world, &path, &id, MAX_STORE_BYTES - result.bytes)?;
        result.bytes += document.bytes.len();
        result.documents.insert(id, document);
    }
    Ok(result)
}

/// Original committed private bytes share the selected canonical snapshot's
/// identity. Neither a missing document nor a failed capture falls back to live
/// files. Pending current publications are irrelevant to immutable history.
fn historical_snapshot(world: &World) -> Result<Snapshot> {
    crate::historical::verify_world(world)?;
    let revision = world
        .revision
        .as_deref()
        .expect("verified historical world");
    let blobs = crate::historical::read_tree(
        &world.root,
        revision,
        "author/actions",
        MAX_DOCUMENTS,
        action_drafts::MAX_DOCUMENT,
        MAX_STORE_BYTES,
    )?;
    let mut result = Snapshot {
        documents: BTreeMap::new(),
        bytes: 0,
    };
    for blob in blobs {
        if blob.revision != revision {
            return Err(invalid(
                "historical private action revision differs from its capture",
            ));
        }
        let id = blob
            .path
            .strip_prefix("author/actions/")
            .and_then(|name| name.strip_suffix(".json"))
            .ok_or_else(|| invalid("unexpected file in historical private action store"))?;
        exact_id(id)?;
        let mut document = decode_document(world, id, blob.bytes)?;
        document.blob_oid = Some(blob.oid);
        result.bytes += document.bytes.len();
        result.documents.insert(id.into(), document);
    }
    Ok(result)
}

fn metadata(world: &World, document: &Stored) -> Json {
    let mut result = json!({
        "id":document.draft.id,"title":document.draft.title,"phase":document.draft.phase,
        "effectivePhase":action_drafts::effective_phase(&document.draft,&world.fingerprint),
        "activeProposalIds":action_drafts::active_proposal_ids(&document.draft),
        "path":relative_path(&document.draft.id),"sourceSha256":document.hash,
        "baseFingerprint":document.draft.base_fingerprint,
        "sourceChanged":document.draft.base_fingerprint != world.fingerprint,
    });
    if let Some(oid) = &document.blob_oid {
        result["blobOid"] = json!(oid);
        result["revision"] = json!(world.revision);
    }
    result
}

fn envelope(world: &World, operation: &str) -> Json {
    let mut result = json!({"protocol":crate::CONTRACT,"operation":operation,
        "scope":"private-action-drafts","fingerprint":world.fingerprint});
    if let Some(revision) = &world.revision {
        result["revision"] = json!(revision);
        result["historical"] = json!(true);
        result["writesPerformed"] = json!(false);
    }
    result
}

pub fn list(world: &World) -> Result<Json> {
    let stored = if world.revision.is_some() {
        historical_snapshot(world)?
    } else {
        capture(world, || snapshot(world))?
    };
    let mut result = envelope(world, "action-drafts-list");
    result["items"] = Json::Array(
        stored
            .documents
            .values()
            .map(|document| metadata(world, document))
            .collect(),
    );
    Ok(result)
}

pub(crate) fn load_pinned(
    world: &World,
    id: &str,
    expected_sha256: Option<&str>,
) -> Result<Stored> {
    // Saved proposal/outcome consumers deliberately retain current-only reads.
    if world.revision.is_some() {
        return Err(invalid(
            "private action workflow requires the current worktree",
        ));
    }
    exact_id(id)?;
    if let Some(hash) = expected_sha256 {
        exact_hash(hash)?;
    }
    capture(world, || {
        let directory = directory(&world.root, false)?
            .ok_or_else(|| Error::NotFound("private action draft is unavailable".into()))?;
        let path = directory.join(format!("{id}.json"));
        let document =
        read_document(world, &path, id, action_drafts::MAX_DOCUMENT).map_err(|error| {
            if matches!(&error, Error::Io(error) if error.kind() == std::io::ErrorKind::NotFound) {
                Error::NotFound("private action draft is unavailable".into())
            } else {
                error
            }
        })?;
        if expected_sha256.is_some_and(|hash| hash != document.hash) {
            return Err(conflict());
        }
        Ok(document)
    })
}

pub fn detail(world: &World, id: &str, expected_sha256: Option<&str>) -> Result<Json> {
    let document = if world.revision.is_some() {
        exact_id(id)?;
        if let Some(hash) = expected_sha256 {
            exact_hash(hash)?;
        }
        let document = historical_snapshot(world)?
            .documents
            .remove(id)
            .ok_or_else(|| {
                Error::NotFound("historical private action draft is unavailable".into())
            })?;
        if expected_sha256.is_some_and(|hash| hash != document.hash) {
            return Err(conflict());
        }
        document
    } else {
        load_pinned(world, id, expected_sha256)?
    };
    let mut result = envelope(world, "action-drafts-detail");
    result
        .as_object_mut()
        .unwrap()
        .extend(metadata(world, &document).as_object().unwrap().clone());
    result["draft"] = serde_json::to_value(&document.draft)?;
    result["source"] = Json::String(
        String::from_utf8(document.bytes)
            .map_err(|_| invalid("private action source must be UTF-8"))?,
    );
    Ok(result)
}

/// A preview selects only a saved, active human proposal by exact private bytes.
/// The request carries no replacement plan and grants no publication capability.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalPreviewRequest {
    pub expected_fingerprint: String,
    pub action_id: String,
    pub expected_sha256: String,
    pub proposal_id: String,
    pub max_characters: usize,
}

impl ProposalPreviewRequest {
    pub(crate) fn validate(&self) -> Result<()> {
        exact_id(&self.action_id)?;
        exact_hash(&self.expected_fingerprint)?;
        exact_hash(&self.expected_sha256)?;
        if !action_drafts::valid_proposal_id(&self.proposal_id) {
            return Err(invalid("an exact human proposal ID is required"));
        }
        if !(256..=1_000_000).contains(&self.max_characters) {
            return Err(invalid(
                "proposal preview maxCharacters must be from 256 to 1000000",
            ));
        }
        Ok(())
    }
}

pub fn decode_proposal_preview(payload: &str) -> Result<ProposalPreviewRequest> {
    if payload.len() > 16_384 {
        return Err(invalid("proposal preview request exceeds 16 KiB"));
    }
    // A concrete struct rejects missing, duplicate and unknown fields; all
    // values are scalars, so no untyped nested maps can erase duplicate keys.
    let request: ProposalPreviewRequest = serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, duplicate or unknown proposal preview fields"))?;
    request.validate()?;
    Ok(request)
}

pub fn proposal_preview(world: &World, request: &ProposalPreviewRequest) -> Result<Json> {
    request.validate()?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "proposal source changed; load and review the current source explicitly".into(),
        ));
    }
    let document = load_pinned(world, &request.action_id, Some(&request.expected_sha256))?;
    let draft = &document.draft;
    if draft.phase != "proposed"
        || action_drafts::effective_phase(draft, &world.fingerprint) != "proposed"
        || !action_drafts::active_proposal_ids(draft).contains(&request.proposal_id)
    {
        return Err(Error::Conflict(
            "selected saved proposal is not active at the current source".into(),
        ));
    }
    let proposal = draft
        .proposals
        .iter()
        .find(|item| item.id == request.proposal_id)
        .ok_or_else(|| Error::NotFound("saved proposal is unavailable".into()))?;
    if proposal.context_fingerprint != world.fingerprint
        || proposal.consequences.expected_fingerprint != world.fingerprint
    {
        return Err(Error::Conflict(
            "selected proposal context is stale; capture a new proposal explicitly".into(),
        ));
    }
    let candidate = consequences::preview(world, &proposal.consequences, request.max_characters)?;
    let mut result = json!({
        "protocol":crate::CONTRACT,"operation":"action-proposal-preview",
        "scope":"private-action-drafts","fingerprint":world.fingerprint,
        "actionId":draft.id,"sourceSha256":document.hash,
        "baseFingerprint":draft.base_fingerprint,"contextFingerprint":proposal.context_fingerprint,
        "proposalId":proposal.id,"proposalSha256":action_drafts::sha256(&serde_json::to_vec(proposal)?),
        "phase":draft.phase,"effectivePhase":"proposed","consequences":candidate,
        "maxCharacters":request.max_characters,"writesPerformed":false,"canApply":false,
    });
    let mut bytes = b"wedl-action-proposal-preview\0".to_vec();
    bytes.extend(serde_json::to_vec(&result)?);
    result["previewSha256"] = json!(action_drafts::sha256(&bytes));
    let needed = serde_json::to_string(&result)?.chars().count();
    if needed > request.max_characters {
        return Err(invalid(&format!(
            "complete saved proposal preview needs {needed} characters; maxCharacters is {}",
            request.max_characters
        )));
    }
    Ok(result)
}

fn prepare(world: &World, request: &SaveRequest) -> Result<Prepared> {
    capture(world, || prepare_unfenced(world, request))
}

fn prepare_unfenced(world: &World, request: &SaveRequest) -> Result<Prepared> {
    exact_id(&request.draft.id)?;
    exact_hash(&request.expected_fingerprint)?;
    if let Some(hash) = &request.expected_sha256 {
        exact_hash(hash)?;
    }
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the expected snapshot".into(),
        ));
    }
    current(world)?;
    let mut stored = snapshot(world)?;
    let before = stored.documents.remove(&request.draft.id);
    if before.as_ref().map(|document| document.hash.as_str()) != request.expected_sha256.as_deref()
    {
        return Err(conflict());
    }
    action_drafts::validate_save(
        world,
        request,
        before.as_ref().map(|document| &document.draft),
    )?;
    let bytes = action_drafts::serialize(&request.draft)?;
    if stored.documents.len() >= MAX_DOCUMENTS
        || stored.bytes - before.as_ref().map_or(0, |document| document.bytes.len()) + bytes.len()
            > MAX_STORE_BYTES
    {
        return Err(bounds());
    }
    let hash = action_drafts::sha256(&bytes);
    Ok(Prepared {
        before,
        bytes,
        hash,
    })
}

fn changed(prepared: &Prepared) -> bool {
    prepared
        .before
        .as_ref()
        .is_none_or(|before| before.bytes != prepared.bytes)
}

fn write_report(world: &World, request: &SaveRequest, prepared: &Prepared, preview: bool) -> Json {
    let mut result = envelope(
        world,
        if preview {
            "action-drafts-preview"
        } else {
            "action-drafts-save"
        },
    );
    result.as_object_mut().unwrap().extend(
        json!({
            "id":request.draft.id,"path":relative_path(&request.draft.id),
            "baseFingerprint":request.draft.base_fingerprint,
            "sourceChanged":request.draft.base_fingerprint != world.fingerprint,
            "beforeSha256":prepared.before.as_ref().map(|before| &before.hash),
            "afterSha256":prepared.hash,"sourceSha256":prepared.hash,
            "changed":changed(prepared),"writesPerformed":!preview && changed(prepared),
            "gitCommit":null,
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    result
}

pub fn preview(world: &World, request: &SaveRequest) -> Result<Json> {
    let prepared = prepare(world, request)?;
    let mut result = write_report(world, request, &prepared, true);
    result["beforeSource"] = match &prepared.before {
        Some(before) => Json::String(
            String::from_utf8(before.bytes.clone())
                .map_err(|_| invalid("private action source must be UTF-8"))?,
        ),
        None => Json::Null,
    };
    result["afterSource"] = Json::String(
        String::from_utf8(prepared.bytes)
            .map_err(|_| invalid("private action source must be UTF-8"))?,
    );
    result["canSave"] = json!(true);
    Ok(result)
}

pub fn save(world: &World, request: &SaveRequest) -> Result<Json> {
    // Complete admission precedes lock or directory creation. An ordinary
    // identical save has no publication and needs no writer lock.
    let initial = prepare(world, request)?;
    if !changed(&initial) {
        return Ok(write_report(world, request, &initial, false));
    }
    let _lock = authoring::writer_lock(&world.root)?;
    let prepared = prepare(world, request)?;
    if prepared.bytes != initial.bytes
        || prepared.before.as_ref().map(|before| &before.bytes)
            != initial.before.as_ref().map(|before| &before.bytes)
    {
        return Err(conflict());
    }
    let private_directory = directory(&world.root, true)?.expect("created private directory");
    let target = private_directory.join(format!("{}.json", request.draft.id));
    let permissions = if let Some(before) = &prepared.before {
        let metadata = fs::symlink_metadata(&target)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid("private action sources must be regular files"));
        }
        if metadata.permissions().readonly() {
            return Err(invalid("private action source is read-only"));
        }
        if read_document(
            world,
            &target,
            &request.draft.id,
            action_drafts::MAX_DOCUMENT,
        )?
        .hash
            != before.hash
        {
            return Err(conflict());
        }
        Some(metadata.permissions())
    } else {
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(conflict()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        }
    };
    let mut temporary = tempfile::Builder::new()
        .prefix(".wedl-action-")
        .tempfile_in(&private_directory)?;
    temporary.write_all(&prepared.bytes)?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    current(world)?;
    // Advisory locks serialize WEDL writers. External editors must coordinate;
    // this final exact-byte check is not filesystem compare-and-swap.
    directory(&world.root, false)?;
    if let Some(before) = &prepared.before {
        let latest = read_document(
            world,
            &target,
            &request.draft.id,
            action_drafts::MAX_DOCUMENT,
        )?;
        if latest.bytes != before.bytes {
            return Err(conflict());
        }
        temporary
            .persist(&target)
            .map_err(|error| Error::Io(error.error))?;
    } else {
        temporary.persist_noclobber(&target).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                conflict()
            } else {
                Error::Io(error.error)
            }
        })?;
    }
    Ok(write_report(world, request, &prepared, false))
}

/// Outcome application keeps the private-store quota and exact prior-byte checks
/// while bypassing only the ordinary (non-outcome) phase transition API.
pub(crate) fn check_outcome_replacement(
    world: &World,
    id: &str,
    before: &[u8],
    after: &[u8],
) -> Result<()> {
    capture(world, || {
        let mut stored = snapshot(world)?;
        let previous = stored.documents.remove(id).ok_or_else(conflict)?;
        if previous.bytes != before {
            return Err(conflict());
        }
        if after.len() > action_drafts::MAX_DOCUMENT
            || stored.bytes - previous.bytes.len() + after.len() > MAX_STORE_BYTES
        {
            return Err(bounds());
        }
        Ok(())
    })
}
