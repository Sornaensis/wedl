//! Private JSON CAS writes reuse the native writer lock. No receipts, canonical
//! publication, automatic commits or review-driven application are introduced.
use super::*;
use crate::authoring;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_DOCUMENTS: usize = 1024;
const MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 2048;
struct Stored {
    document: Document,
    bytes: Vec<u8>,
    hash: String,
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
fn conflict() -> Error {
    Error::Conflict("retcon review changed; reload its exact private source before saving".into())
}
fn bounds() -> Error {
    invalid("retcon review store exceeds its complete document, entry or byte bound")
}
fn hash(bytes: &[u8]) -> String {
    action_drafts::sha256(bytes)
}
fn directory(root: &Path, create: bool) -> Result<Option<PathBuf>> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("retcon review root must be a real directory"));
    }
    let mut path = root.to_owned();
    for part in ["author", "retcon-reviews"] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid("retcon review parents must be real directories"));
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
fn document(id: &str, bytes: Vec<u8>, blob_oid: Option<String>) -> Result<Stored> {
    let raw =
        std::str::from_utf8(&bytes).map_err(|_| invalid("retcon review source must be UTF-8"))?;
    let document = decode_document(raw)?;
    if document.id != id {
        return Err(invalid(
            "retcon review identity differs from its exact private filename",
        ));
    }
    Ok(Stored {
        document,
        hash: hash(&bytes),
        bytes,
        blob_oid,
    })
}
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
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("retcon review source must be a regular file"));
    }
    Ok(Stamp {
        bytes: metadata.len(),
        modified: metadata.modified()?,
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}
fn read_bytes(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let before = stamp(&fs::symlink_metadata(path)?)?;
    if before.bytes > maximum as u64 {
        return Err(bounds());
    }
    let file = fs::File::open(path)?;
    if stamp(&file.metadata()?)? != before {
        return Err(conflict());
    }
    let mut bytes = Vec::with_capacity(before.bytes as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(bounds());
    }
    if bytes.len() as u64 != before.bytes || stamp(&fs::symlink_metadata(path)?)? != before {
        return Err(conflict());
    }
    Ok(bytes)
}
fn inventory(directory: &Path) -> Result<(BTreeMap<String, Stamp>, usize)> {
    let mut result = BTreeMap::new();
    let mut total = 0usize;
    let mut count = 0usize;
    for (index, entry) in fs::read_dir(directory)?.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(bounds());
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("retcon review filenames must be UTF-8"))?;
        let metadata = stamp(&fs::symlink_metadata(entry.path())?)?;
        if metadata.bytes > MAX_DOCUMENT_BYTES as u64 {
            return Err(bounds());
        }
        total = total.saturating_add(metadata.bytes as usize);
        if total > MAX_STORE_BYTES {
            return Err(bounds());
        }
        if name.starts_with(".wedl-retcon-") {
            result.insert(name, metadata);
            continue;
        }
        if name
            .strip_suffix(".json")
            .is_none_or(|id| !valid_review_id(id))
        {
            return Err(invalid("unexpected file in the retcon review store"));
        }
        count += 1;
        if count > MAX_DOCUMENTS {
            return Err(bounds());
        }
        result.insert(name, metadata);
    }
    Ok((result, total))
}
fn snapshot(root: &Path, revision: Option<&str>) -> Result<Snapshot> {
    let mut result = Snapshot {
        documents: BTreeMap::new(),
        bytes: 0,
    };
    if let Some(revision) = revision {
        for blob in historical::read_tree(
            root,
            revision,
            "author/retcon-reviews",
            MAX_DOCUMENTS,
            MAX_DOCUMENT_BYTES,
            MAX_STORE_BYTES,
        )? {
            let id = blob
                .path
                .strip_prefix("author/retcon-reviews/")
                .and_then(|s| s.strip_suffix(".json"))
                .filter(|id| valid_review_id(id))
                .ok_or_else(|| {
                    invalid("historical retcon review path must be its exact private ID")
                })?;
            result.bytes += blob.bytes.len();
            let stored = document(id, blob.bytes, Some(blob.oid))?;
            if result.documents.insert(id.to_owned(), stored).is_some() {
                return Err(invalid("duplicate historical retcon review identity"));
            }
        }
        return Ok(result);
    }
    let Some(directory) = directory(root, false)? else {
        return Ok(result);
    };
    let (selected, total) = inventory(&directory)?;
    result.bytes = total;
    for name in selected
        .keys()
        .filter(|name| !name.starts_with(".wedl-retcon-"))
    {
        let id = name.strip_suffix(".json").unwrap();
        let bytes = read_bytes(&directory.join(name), MAX_DOCUMENT_BYTES)?;
        result
            .documents
            .insert(id.to_owned(), document(id, bytes, None)?);
    }
    if inventory(&directory)?.0 != selected {
        return Err(conflict());
    }
    for (id, stored) in &result.documents {
        if read_bytes(&directory.join(format!("{id}.json")), MAX_DOCUMENT_BYTES)? != stored.bytes {
            return Err(conflict());
        }
    }
    if inventory(&directory)?.0 != selected {
        return Err(conflict());
    }
    self::directory(root, false)?;
    Ok(result)
}
fn load(root: &Path, request: &DetailRequest) -> Result<Stored> {
    validate_detail(request)?;
    let stored = if let Some(revision) = &request.revision {
        let blob = historical::read_blob(root, revision, &path(&request.id), MAX_DOCUMENT_BYTES)?;
        document(&request.id, blob.bytes, Some(blob.oid))?
    } else {
        let directory = directory(root, false)?
            .ok_or_else(|| Error::NotFound("retcon review is unavailable".into()))?;
        let bytes = read_bytes(
            &directory.join(format!("{}.json", request.id)),
            MAX_DOCUMENT_BYTES,
        )
        .map_err(|error| {
            if matches!(&error, Error::Io(error) if error.kind() == std::io::ErrorKind::NotFound) {
                Error::NotFound("retcon review is unavailable".into())
            } else {
                error
            }
        })?;
        self::directory(root, false)?;
        document(&request.id, bytes, None)?
    };
    if request
        .expected_sha256
        .as_deref()
        .is_some_and(|expected| expected != stored.hash)
    {
        return Err(conflict());
    }
    Ok(stored)
}
fn metadata(root: &Path, stored: &Stored, compared: &Comparison) -> Result<Json> {
    Ok(
        json!({"id":stored.document.id,"title":stored.document.title,"path":path(&stored.document.id),"sourceSha256":stored.hash,"blobOid":stored.blob_oid,
        "pendingCount":stored.document.analysis.item_digests.len()-stored.document.decisions.len(),
        "currentness":currentness(root,&stored.document.analysis,compared)?}),
    )
}
pub fn queue(root: &Path, request: &QueueRequest) -> Result<Json> {
    validate_queue(request)?;
    let compared = Comparison::capture(root);
    let stored = snapshot(root, request.revision.as_deref())?;
    if stored.documents.len() > request.max_items {
        return Err(invalid(
            "complete retcon review queue exceeds maxItems; partial queues are not returned",
        ));
    }
    let mut result = envelope(
        "retcon-reviews-queue",
        request.revision.as_deref(),
        &compared,
    );
    let mut items = Vec::with_capacity(stored.documents.len());
    for value in stored.documents.values() {
        items.push(metadata(root, value, &compared)?);
    }
    result["items"] = items.into();
    codec::bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}
pub fn detail(root: &Path, request: &DetailRequest) -> Result<Json> {
    validate_detail(request)?;
    let compared = Comparison::capture(root);
    let stored = load(root, request)?;
    let mut result = envelope(
        "retcon-reviews-detail",
        request.revision.as_deref(),
        &compared,
    );
    result.as_object_mut().unwrap().extend(
        metadata(root, &stored, &compared)?
            .as_object()
            .unwrap()
            .clone(),
    );
    result["document"] = serde_json::to_value(&stored.document)?;
    result["source"] = String::from_utf8(stored.bytes)
        .expect("validated UTF-8")
        .into();
    codec::bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}
fn current(world: &World) -> Result<()> {
    if world.revision.is_some() {
        return Err(invalid("retcon review saves require the current worktree"));
    }
    let fresh = historical::load_current_world(&world.root)?;
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
        return Err(Error::Conflict("current source changed or differs from its exact retained capture; reload before saving".into()));
    }
    retcon_impact::preflight(&fresh)?;
    if !validation::validate_world(&fresh).valid {
        return Err(Error::Validation(
            "retcon review save requires valid current source".into(),
        ));
    }
    Ok(())
}
fn prepare(world: &World, request: &SaveRequest) -> Result<Prepared> {
    validate_save(request)?;
    current(world)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "current save fingerprint differs from the current source snapshot".into(),
        ));
    }
    if request.document.analysis.world_id != world.world_record()?.id {
        return Err(invalid("retcon review belongs to another world"));
    }
    let mut stored = snapshot(&world.root, None)?;
    let before = stored.documents.remove(&request.document.id);
    if before.as_ref().map(|before| before.hash.as_str()) != request.expected_sha256.as_deref() {
        return Err(conflict());
    }
    if let Some(before) = &before {
        if codec::bytes(&before.document.analysis, MAX_DOCUMENT_BYTES)?
            != codec::bytes(&request.document.analysis, MAX_DOCUMENT_BYTES)?
        {
            return Err(invalid(
                "retcon review original analysis is immutable; capture a new review ID",
            ));
        }
    }
    verify_original(&world.root, &request.document.analysis, false)?;
    let bytes = codec::bytes(&request.document, MAX_DOCUMENT_BYTES)?;
    if stored.documents.len() >= MAX_DOCUMENTS
        || stored.bytes - before.as_ref().map_or(0, |before| before.bytes.len()) + bytes.len()
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
    prepared: &Prepared,
    preview: bool,
) -> Result<Json> {
    let result = json!({"protocol":crate::CONTRACT,"operation":if preview {"retcon-reviews-save-preview"}else{"retcon-reviews-save"},"scope":"private-retcon-reviews",
        "id":request.document.id,"path":path(&request.document.id),"fingerprint":world.fingerprint,"revision":null,
        "beforeSha256":prepared.before.as_ref().map(|before|&before.hash),"afterSha256":prepared.hash,"sourceSha256":prepared.hash,
        "changed":changed(prepared),"writesPerformed":!preview&&changed(prepared),"canApply":false,"gitCommit":null});
    codec::bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}
pub fn save_preview(world: &World, request: &SaveRequest) -> Result<Json> {
    let prepared = prepare(world, request)?;
    let mut result = write_report(world, request, &prepared, true)?;
    result["beforeSource"] = prepared
        .before
        .map(|before| String::from_utf8(before.bytes).expect("validated UTF-8"))
        .into();
    result["afterSource"] = String::from_utf8(prepared.bytes)
        .expect("encoded UTF-8")
        .into();
    result["canSave"] = true.into();
    codec::bytes(&result, MAX_REPORT_BYTES)?;
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
        || prepared.before.as_ref().map(|before| &before.bytes)
            != initial.before.as_ref().map(|before| &before.bytes)
    {
        return Err(conflict());
    }
    let report = write_report(world, request, &prepared, false)?;
    let directory = directory(&world.root, true)?.expect("created private review directory");
    let target = directory.join(format!("{}.json", request.document.id));
    let permissions = if let Some(before) = &prepared.before {
        let metadata = fs::symlink_metadata(&target)?;
        stamp(&metadata)?;
        if metadata.permissions().readonly() {
            return Err(invalid("retcon review source is not writable"));
        }
        if read_bytes(&target, MAX_DOCUMENT_BYTES)? != before.bytes {
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
        .prefix(".wedl-retcon-")
        .tempfile_in(&directory)?;
    temporary.write_all(&prepared.bytes)?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    current(world)?;
    self::directory(&world.root, false)?;
    if let Some(before) = &prepared.before {
        if read_bytes(&target, MAX_DOCUMENT_BYTES)? != before.bytes {
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
