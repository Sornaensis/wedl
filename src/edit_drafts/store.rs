//! Bounded retained edit storage. Canonical source and private bytes have
//! independent pins; unfinished patch text is never interpreted by this layer.
use super::*;
use crate::{authoring, historical, model::World, publication, source};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_DOCUMENTS: usize = 1024;
const MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 2048;

pub(super) struct Stored {
    pub draft: Draft,
    pub bytes: Vec<u8>,
    pub hash: String,
    pub blob_oid: Option<String>,
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

fn conflict() -> Error {
    Error::Conflict("retained edit changed; reload its exact private source before saving".into())
}
fn bounds() -> Error {
    invalid("retained edit store exceeds its complete document or byte limit")
}

pub(super) fn current(world: &World) -> Result<()> {
    if world.revision.is_some() {
        return Err(invalid(
            "retained edit writes and application require the current worktree",
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "retained edit source is invalid; run author validation for diagnostics".into(),
        ));
    }
    let fresh = source::load_world(&world.root, None)?;
    if fresh.fingerprint != world.fingerprint
        || fresh.records.len() != world.records.len()
        || world.records.iter().any(|(id, record)| {
            fresh.records.get(id).is_none_or(|original| {
                record.raw_bytes != original.raw_bytes
                    || record.source_path != original.source_path
                    || record.id != original.id
                    || record.kind != original.kind
                    || record.title != original.title
                    || record.body != original.body
                    || record.frontmatter != original.frontmatter
                    || record.blob_oid != original.blob_oid
            })
        })
    {
        return Err(Error::Conflict("source changed or the retained capture differs from its exact source; reload the current snapshot".into()));
    }
    Ok(())
}

fn capture<T>(world: &World, read: impl FnOnce() -> Result<T>) -> Result<T> {
    if world.revision.is_some() {
        historical::verify_world(world)?;
        return read();
    }
    let before = publication::read_stamp(&world.root)?;
    let result = (|| {
        current(world)?;
        read()
    })();
    if publication::read_stamp(&world.root)? != before {
        return Err(Error::Conflict(
            "publication changed during the retained edit read; reload".into(),
        ));
    }
    result
}

fn directory(root: &Path, create: bool) -> Result<Option<PathBuf>> {
    let mut path = root.to_owned();
    for component in ["author", "edits"] {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid("retained edit parents must be real directories"));
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

fn document(world: &World, id: &str, bytes: Vec<u8>, blob_oid: Option<String>) -> Result<Stored> {
    if bytes.len() > MAX_DOCUMENT {
        return Err(bounds());
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| invalid("retained edit source must be UTF-8"))?;
    let draft = decode(text)?;
    if draft.id != id || draft.world_id != world.world_record()?.id {
        return Err(invalid(
            "retained edit identity differs from its selected repository or path",
        ));
    }
    let hash = hash(&bytes);
    Ok(Stored {
        draft,
        bytes,
        hash,
        blob_oid,
    })
}

fn read_document(world: &World, file_path: &Path, id: &str, remaining: usize) -> Result<Stored> {
    let metadata = fs::symlink_metadata(file_path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("retained edit sources must be regular files"));
    }
    let maximum = MAX_DOCUMENT.min(remaining);
    if metadata.len() > maximum as u64 {
        return Err(bounds());
    }
    let file = fs::File::open(file_path)?;
    let after_open = fs::symlink_metadata(file_path)?;
    if after_open.file_type().is_symlink() || !after_open.is_file() || !file.metadata()?.is_file() {
        return Err(invalid("retained edit sources must be regular files"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(bounds());
    }
    document(world, id, bytes, None)
}

fn snapshot(world: &World) -> Result<Snapshot> {
    let mut result = Snapshot {
        documents: BTreeMap::new(),
        bytes: 0,
    };
    if let Some(revision) = &world.revision {
        for blob in historical::read_tree(
            &world.root,
            revision,
            "author/edits",
            MAX_DOCUMENTS,
            MAX_DOCUMENT,
            MAX_STORE_BYTES,
        )? {
            let id = blob
                .path
                .strip_prefix("author/edits/")
                .and_then(|name| name.strip_suffix(".json"))
                .filter(|id| valid_edit_id(id))
                .ok_or_else(|| {
                    invalid("historical retained edit path must be its exact private ID")
                })?;
            result.bytes += blob.bytes.len();
            let value = document(world, id, blob.bytes, Some(blob.oid))?;
            if result.documents.insert(id.into(), value).is_some() {
                return Err(invalid("duplicate historical edit identity"));
            }
        }
        return Ok(result);
    }
    let Some(directory) = directory(&world.root, false)? else {
        return Ok(result);
    };
    let mut paths = BTreeMap::new();
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(bounds());
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("retained edit filenames must be exact UTF-8 IDs"))?;
        if name.starts_with(".wedl-edit-") {
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(invalid(
                    "retained edit temporary sources must be regular files",
                ));
            }
            if metadata.len() > MAX_DOCUMENT as u64 {
                return Err(bounds());
            }
            result.bytes = result.bytes.saturating_add(metadata.len() as usize);
            if result.bytes > MAX_STORE_BYTES {
                return Err(bounds());
            }
            continue;
        }
        if paths.len() >= MAX_DOCUMENTS {
            return Err(bounds());
        }
        let id = name
            .strip_suffix(".json")
            .filter(|id| valid_edit_id(id))
            .ok_or_else(|| invalid("unexpected file in the retained edit store"))?;
        paths.insert(id.to_owned(), entry.path());
    }
    for (id, path) in paths {
        let value = read_document(world, &path, &id, MAX_STORE_BYTES - result.bytes)?;
        result.bytes += value.bytes.len();
        result.documents.insert(id, value);
    }
    Ok(result)
}

/// Capture a live comparison once. Historical private content remains readable
/// when current source is invalid, pending publication, unavailable or changed.
fn comparison(world: &World) -> Option<World> {
    if world.revision.is_none() {
        return Some(world.clone());
    }
    let live = source::load_world(&world.root, None).ok()?;
    validation::validate_world(&live).valid.then_some(live)
}

fn metadata(value: &Stored, compared: Option<&World>) -> Json {
    json!({"id":value.draft.id,"title":value.draft.title,"phase":value.draft.phase,
        "path":path(&value.draft.id),"sourceSha256":value.hash,"blobOid":value.blob_oid,
        "targetId":value.draft.target_id,"baseFingerprint":value.draft.base_fingerprint,
        "baseSourceSha256":value.draft.base_source_sha256,"baseSourcePath":value.draft.base_source_path,
        "currentness":currentness(&value.draft,compared)})
}

fn envelope(world: &World, operation: &str, compared: Option<&World>) -> Json {
    json!({"protocol":crate::CONTRACT,"operation":operation,"scope":"private-edit-drafts",
        "fingerprint":world.fingerprint,"revision":world.revision,
        "comparedFingerprint":compared.map(|world| &world.fingerprint),"writesPerformed":false})
}

pub fn list(world: &World) -> Result<Json> {
    let stored = capture(world, || snapshot(world))?;
    let compared = comparison(world);
    let mut result = envelope(world, "edit-drafts-list", compared.as_ref());
    result["items"] = Json::Array(
        stored
            .documents
            .values()
            .map(|value| metadata(value, compared.as_ref()))
            .collect(),
    );
    bounded_report(&result, None)?;
    Ok(result)
}

pub(super) fn load_pinned(
    world: &World,
    id: &str,
    expected_sha256: Option<&str>,
) -> Result<Stored> {
    if !valid_edit_id(id) || expected_sha256.is_some_and(|hash| !valid_hash(hash)) {
        return Err(invalid(
            "retained edit detail requires an exact ID and optional SHA-256",
        ));
    }
    let value = capture(world, || {
        if let Some(revision) = &world.revision {
            let blob = historical::read_blob(&world.root, revision, &path(id), MAX_DOCUMENT)?;
            document(world, id, blob.bytes, Some(blob.oid))
        } else {
            let directory = directory(&world.root, false)?
                .ok_or_else(|| Error::NotFound("retained edit is unavailable".into()))?;
            read_document(
                world,
                &directory.join(format!("{id}.json")),
                id,
                MAX_DOCUMENT,
            )
            .map_err(|error| {
                if matches!(&error,Error::Io(error) if error.kind()==std::io::ErrorKind::NotFound) {
                    Error::NotFound("retained edit is unavailable".into())
                } else {
                    error
                }
            })
        }
    })?;
    if expected_sha256.is_some_and(|expected| expected != value.hash) {
        return Err(conflict());
    }
    Ok(value)
}

pub fn detail(world: &World, id: &str, expected_sha256: Option<&str>) -> Result<Json> {
    let value = load_pinned(world, id, expected_sha256)?;
    let compared = comparison(world);
    let mut result = envelope(world, "edit-drafts-detail", compared.as_ref());
    result.as_object_mut().unwrap().extend(
        metadata(&value, compared.as_ref())
            .as_object()
            .unwrap()
            .clone(),
    );
    result["draft"] = serde_json::to_value(&value.draft)?;
    result["source"] = String::from_utf8(value.bytes)
        .map_err(|_| invalid("retained edit source must be UTF-8"))?
        .into();
    bounded_report(&result, None)?;
    Ok(result)
}

fn admit_save(world: &World, request: &SaveRequest, previous: Option<&Draft>) -> Result<()> {
    validate(&request.draft)?;
    let draft = &request.draft;
    if !valid_hash(&request.expected_fingerprint)
        || request
            .expected_sha256
            .as_deref()
            .is_some_and(|hash| !valid_hash(hash))
    {
        return Err(invalid(
            "retained edit save requires exact source/private pins",
        ));
    }
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the edit save snapshot".into(),
        ));
    }
    if draft.world_id != world.world_record()?.id {
        return Err(invalid("retained edit belongs to another world"));
    }
    if draft.phase == "applied" || draft.applied_receipt.is_some() {
        return Err(invalid("ordinary edit save cannot mint applied history"));
    }
    let base_changed = if let Some(old) = previous {
        if old.phase != "draft" {
            return Err(invalid("cancelled and applied edits are terminal"));
        }
        if old.id != draft.id || old.world_id != draft.world_id || old.target_id != draft.target_id
        {
            return Err(invalid(
                "retained edit identity, world and target cannot change in place",
            ));
        }
        old.base_fingerprint != draft.base_fingerprint
            || old.base_source_sha256 != draft.base_source_sha256
            || old.base_source_path != draft.base_source_path
    } else {
        true
    };
    if base_changed {
        if currentness(draft, Some(world))["state"] != "current" {
            return Err(Error::Conflict("new or explicitly reconciled edits require the exact current target and complete source pins".into()));
        }
    }
    Ok(())
}

fn prepare(world: &World, request: &SaveRequest) -> Result<Prepared> {
    current(world)?;
    capture(world, || {
        let mut stored = snapshot(world)?;
        let before = stored.documents.remove(&request.draft.id);
        if before.as_ref().map(|value| value.hash.as_str()) != request.expected_sha256.as_deref() {
            return Err(conflict());
        }
        admit_save(world, request, before.as_ref().map(|value| &value.draft))?;
        let bytes = serialize(&request.draft)?;
        if stored.documents.len() >= MAX_DOCUMENTS
            || stored.bytes - before.as_ref().map_or(0, |value| value.bytes.len()) + bytes.len()
                > MAX_STORE_BYTES
        {
            return Err(bounds());
        }
        let hash = hash(&bytes);
        Ok(Prepared {
            before,
            bytes,
            hash,
        })
    })
}
fn changed(value: &Prepared) -> bool {
    value
        .before
        .as_ref()
        .is_none_or(|before| before.bytes != value.bytes)
}
fn write_report(
    world: &World,
    request: &SaveRequest,
    value: &Prepared,
    preview: bool,
) -> Result<Json> {
    let mut result = envelope(
        world,
        if preview {
            "edit-drafts-preview"
        } else {
            "edit-drafts-save"
        },
        Some(world),
    );
    result.as_object_mut().unwrap().extend(json!({"id":request.draft.id,"path":path(&request.draft.id),
        "targetId":request.draft.target_id,"baseFingerprint":request.draft.base_fingerprint,
        "beforeSha256":value.before.as_ref().map(|before|&before.hash),"afterSha256":value.hash,
        "sourceSha256":value.hash,"changed":changed(value),"writesPerformed":!preview&&changed(value),
        "currentness":currentness(&request.draft,Some(world)),"gitCommit":null}).as_object().unwrap().clone());
    bounded_report(&result, None)?;
    Ok(result)
}
pub fn preview(world: &World, request: &SaveRequest) -> Result<Json> {
    let prepared = prepare(world, request)?;
    let mut result = write_report(world, request, &prepared, true)?;
    result["beforeSource"] = prepared
        .before
        .map(|value| String::from_utf8(value.bytes).expect("decoded UTF-8"))
        .into();
    result["afterSource"] = String::from_utf8(prepared.bytes)
        .expect("encoded UTF-8")
        .into();
    result["canSave"] = true.into();
    bounded_report(&result, None)?;
    Ok(result)
}
pub fn save(world: &World, request: &SaveRequest) -> Result<Json> {
    let initial = prepare(world, request)?;
    if !changed(&initial) {
        return write_report(world, request, &initial, false);
    }
    let _lock = authoring::writer_lock(&world.root)?;
    let prepared = prepare(world, request)?;
    if prepared.bytes != initial.bytes
        || prepared.before.as_ref().map(|value| &value.bytes)
            != initial.before.as_ref().map(|value| &value.bytes)
    {
        return Err(conflict());
    }
    let report = write_report(world, request, &prepared, false)?;
    let directory = directory(&world.root, true)?.expect("created edit directory");
    let target = directory.join(format!("{}.json", request.draft.id));
    let permissions = if let Some(before) = &prepared.before {
        let metadata = fs::symlink_metadata(&target)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.permissions().readonly()
        {
            return Err(invalid(
                "retained edit source is not a writable regular file",
            ));
        }
        if read_document(world, &target, &request.draft.id, MAX_DOCUMENT)?.bytes != before.bytes {
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
        .prefix(".wedl-edit-")
        .tempfile_in(&directory)?;
    temporary.write_all(&prepared.bytes)?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    current(world)?;
    self::directory(&world.root, false)?;
    if let Some(before) = &prepared.before {
        if read_document(world, &target, &request.draft.id, MAX_DOCUMENT)?.bytes != before.bytes {
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
    Ok(report)
}

pub(super) fn check_replacement(
    world: &World,
    id: &str,
    before: &[u8],
    after: &[u8],
) -> Result<()> {
    current(world)?;
    capture(world, || {
        let mut stored = snapshot(world)?;
        let previous = stored.documents.remove(id).ok_or_else(conflict)?;
        if previous.bytes != before {
            return Err(conflict());
        }
        if after.len() > MAX_DOCUMENT
            || stored.bytes - previous.bytes.len() + after.len() > MAX_STORE_BYTES
        {
            return Err(bounds());
        }
        Ok(())
    })
}
