//! Bounded independent private-source reads and guarded exact-source saves.
//! Manuscript files never join the canonical World or its fingerprint.
use super::*;
use crate::{authoring, publication};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_READ_REQUEST_BYTES: usize = 16 * 1024;
const MAX_SAVE_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 2048;
const TEMP_PREFIX: &str = ".wedl-manuscript-";

fn required_nullable<'de, D, T>(decoder: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(decoder)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListRequest {
    #[serde(deserialize_with = "required_nullable")]
    pub revision: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub kind: Option<String>,
    pub max_items: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetailRequest {
    #[serde(deserialize_with = "required_nullable")]
    pub revision: Option<String>,
    pub id: String,
    #[serde(deserialize_with = "required_nullable")]
    pub expected_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveRequest {
    #[serde(deserialize_with = "required_nullable")]
    pub revision: Option<String>,
    pub work_id: String,
    #[serde(deserialize_with = "required_nullable")]
    pub expected_author_fingerprint: Option<String>,
    pub max_occurrences: usize,
    pub max_characters: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevealResolveRequest {
    #[serde(deserialize_with = "required_nullable")]
    pub revision: Option<String>,
    pub plan_id: String,
    #[serde(deserialize_with = "required_nullable")]
    pub expected_author_fingerprint: Option<String>,
    pub max_occurrences: usize,
    pub max_characters: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRequest {
    pub expected_fingerprint: String,
    pub expected_author_fingerprint: String,
    #[serde(deserialize_with = "required_nullable")]
    pub expected_sha256: Option<String>,
    pub source: String,
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn validate_revision(revision: Option<&str>) -> Result<()> {
    if revision.is_some_and(|revision| !historical::valid_oid(revision)) {
        return Err(invalid(
            "manuscript revision requires null or a full lowercase commit OID",
        ));
    }
    Ok(())
}
fn validate_hash(value: Option<&str>) -> Result<()> {
    if value.is_some_and(|value| !valid_hash(value)) {
        return Err(invalid("manuscript source pins require lowercase SHA-256"));
    }
    Ok(())
}
fn validate_list(request: &ListRequest) -> Result<()> {
    validate_revision(request.revision.as_deref())?;
    if request
        .kind
        .as_deref()
        .is_some_and(|kind| !matches!(kind, "work" | "manuscript-segment" | "reveal-plan"))
        || !(1..=MAX_DOCUMENTS).contains(&request.max_items)
    {
        return Err(invalid(
            "manuscript list requires a supported kind and maxItems in 1..1024",
        ));
    }
    Ok(())
}
fn validate_detail(request: &DetailRequest) -> Result<()> {
    validate_revision(request.revision.as_deref())?;
    validate_hash(request.expected_sha256.as_deref())?;
    if !valid_id(&request.id, "segment")
        && !valid_id(&request.id, "work")
        && !valid_id(&request.id, "reveal")
    {
        return Err(invalid(
            "manuscript detail requires an exact work, segment or reveal ID",
        ));
    }
    Ok(())
}
fn validate_resolve(request: &ResolveRequest) -> Result<()> {
    validate_revision(request.revision.as_deref())?;
    validate_hash(request.expected_author_fingerprint.as_deref())?;
    if !valid_id(&request.work_id, "work")
        || !(1..=4096).contains(&request.max_occurrences)
        || !(256..=1_000_000).contains(&request.max_characters)
    {
        return Err(invalid(
            "manuscript resolve requires an exact work ID, maxOccurrences in 1..4096 and maxCharacters in 256..1000000",
        ));
    }
    Ok(())
}
fn validate_save(request: &SaveRequest) -> Result<()> {
    validate_hash(Some(&request.expected_fingerprint))?;
    validate_hash(Some(&request.expected_author_fingerprint))?;
    validate_hash(request.expected_sha256.as_deref())?;
    if request.source.len() > MAX_DOCUMENT_BYTES {
        return Err(invalid("manuscript save source exceeds 2 MiB"));
    }
    Ok(())
}

fn validate_reveal_resolve(request: &RevealResolveRequest) -> Result<()> {
    validate_revision(request.revision.as_deref())?;
    validate_hash(request.expected_author_fingerprint.as_deref())?;
    if !valid_id(&request.plan_id, "reveal")
        || !(1..=4096).contains(&request.max_occurrences)
        || !(256..=1_000_000).contains(&request.max_characters)
    {
        return Err(invalid(
            "reveal resolve requires an exact plan ID, maxOccurrences in 1..4096 and maxCharacters in 256..1000000",
        ));
    }
    Ok(())
}

pub fn decode_list(raw: &str) -> Result<ListRequest> {
    let request = decode_closed_json(raw, MAX_READ_REQUEST_BYTES)?;
    validate_list(&request)?;
    Ok(request)
}
pub fn decode_detail(raw: &str) -> Result<DetailRequest> {
    let request = decode_closed_json(raw, MAX_READ_REQUEST_BYTES)?;
    validate_detail(&request)?;
    Ok(request)
}
pub fn decode_resolve(raw: &str) -> Result<ResolveRequest> {
    let request = decode_closed_json(raw, MAX_READ_REQUEST_BYTES)?;
    validate_resolve(&request)?;
    Ok(request)
}
pub fn decode_save(raw: &str) -> Result<SaveRequest> {
    let request = decode_closed_json(raw, MAX_SAVE_REQUEST_BYTES)?;
    validate_save(&request)?;
    Ok(request)
}

pub fn decode_reveal_resolve(raw: &str) -> Result<RevealResolveRequest> {
    let request = decode_closed_json(raw, MAX_READ_REQUEST_BYTES)?;
    validate_reveal_resolve(&request)?;
    Ok(request)
}

fn conflict() -> Error {
    Error::Conflict(
        "manuscript or canonical source changed; reload exact source before continuing".into(),
    )
}
fn bounds() -> Error {
    invalid("manuscript source exceeds its complete file, directory-entry or byte bound")
}
fn source_hash(record: &Record) -> String {
    hex::encode(Sha256::digest(&record.raw_bytes))
}

fn directory(root: &Path, folder: &str, create: bool) -> Result<Option<PathBuf>> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("manuscript root must be a real directory"));
    }
    let mut path = root.to_owned();
    for part in ["author", folder] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(invalid("manuscript parents must be real directories"));
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
        return Err(invalid("manuscript source must be a regular file"));
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
fn read_bytes(path: &Path, expected: &Stamp) -> Result<Vec<u8>> {
    if expected.bytes > MAX_DOCUMENT_BYTES as u64 {
        return Err(bounds());
    }
    if stamp(&fs::symlink_metadata(path)?)? != *expected {
        return Err(conflict());
    }
    let file = fs::File::open(path)?;
    if stamp(&file.metadata()?)? != *expected {
        return Err(conflict());
    }
    let mut bytes = Vec::with_capacity(expected.bytes as usize);
    file.take(expected.bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != expected.bytes || stamp(&fs::symlink_metadata(path)?)? != *expected {
        return Err(conflict());
    }
    Ok(bytes)
}

fn inventory(root: &Path) -> Result<BTreeMap<String, Stamp>> {
    let mut result = BTreeMap::new();
    let mut entries = 0usize;
    let mut documents = 0usize;
    let mut bytes = 0usize;
    for (folder, kind) in [
        ("segments", "manuscript-segment"),
        ("works", "work"),
        ("reveals", "reveal-plan"),
    ] {
        let Some(directory) = directory(root, folder, false)? else {
            continue;
        };
        for entry in fs::read_dir(directory)? {
            entries += 1;
            if entries > MAX_DIRECTORY_ENTRIES {
                return Err(bounds());
            }
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("manuscript filenames must be UTF-8"))?;
            let metadata = stamp(&fs::symlink_metadata(entry.path())?)?;
            if metadata.bytes > MAX_DOCUMENT_BYTES as u64 {
                return Err(bounds());
            }
            let path = format!("author/{folder}/{name}");
            // Atomic-save temporary files participate in the read fence, but
            // are never source documents or manuscript fingerprint inputs.
            if !name.starts_with(TEMP_PREFIX) {
                let id = name
                    .strip_suffix(".md")
                    .ok_or_else(|| invalid("unexpected file in a manuscript source directory"))?;
                if document_path(kind, id)? != path {
                    return Err(invalid(
                        "manuscript source path must be its exact immutable ID",
                    ));
                }
                documents += 1;
                bytes = bytes.saturating_add(metadata.bytes as usize);
                if documents > MAX_DOCUMENTS || bytes > MAX_SNAPSHOT_BYTES {
                    return Err(bounds());
                }
            }
            result.insert(path, metadata);
        }
    }
    Ok(result)
}

/// Exact private capture only: canonical source is deliberately not consulted.
pub(crate) fn snapshot(root: &Path, revision: Option<&str>) -> Result<Snapshot> {
    validate_revision(revision)?;
    let mut records = Vec::new();
    let mut expanded = SourceBudget::default();
    if let Some(revision) = revision {
        let mut total = 0usize;
        for prefix in ["author/segments", "author/works", "author/reveals"] {
            for blob in historical::read_tree(
                root,
                revision,
                prefix,
                MAX_DOCUMENTS - records.len(),
                MAX_DOCUMENT_BYTES,
                MAX_SNAPSHOT_BYTES - total,
            )? {
                total += blob.bytes.len();
                let mut record = parse_document(&blob.bytes, &blob.path)?;
                record.blob_oid = Some(blob.oid);
                expanded.charge(&record)?;
                records.push(record);
            }
        }
        return finish_snapshot(records, Some(revision.to_owned()));
    }
    let selected = inventory(root)?;
    for (path, stamp) in &selected {
        if path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with(TEMP_PREFIX))
        {
            continue;
        }
        let bytes = read_bytes(&root.join(path), stamp)?;
        let record = parse_document(&bytes, path)?;
        expanded.charge(&record)?;
        records.push(record);
    }
    if inventory(root)? != selected {
        return Err(conflict());
    }
    for record in &records {
        if read_bytes(
            &root.join(&record.source_path),
            &selected[&record.source_path],
        )? != record.raw_bytes
        {
            return Err(conflict());
        }
    }
    if inventory(root)? != selected {
        return Err(conflict());
    }
    finish_snapshot(records, None)
}

fn envelope(operation: &str, authors: &Snapshot) -> Json {
    json!({"protocol":crate::CONTRACT,"operation":operation,"scope":"private-manuscripts",
        "revision":authors.revision,"authorFingerprint":authors.fingerprint,
        "complete":true,"readerAuthorized":false,"validationComplete":false,
        "writesPerformed":false,"canApply":false})
}
fn metadata(record: &Record) -> Json {
    json!({"id":record.id,"kind":record.kind,"title":record.title,
        "path":record.source_path,"sourceSha256":source_hash(record),"blobOid":record.blob_oid})
}

pub fn list(root: &Path, request: &ListRequest) -> Result<Json> {
    validate_list(request)?;
    let authors = snapshot(root, request.revision.as_deref())?;
    let selected: Vec<_> = authors
        .documents
        .values()
        .filter(|record| {
            request
                .kind
                .as_deref()
                .is_none_or(|kind| kind == record.kind)
        })
        .collect();
    if selected.len() > request.max_items {
        return Err(invalid(
            "complete manuscript list exceeds maxItems; partial lists are not returned",
        ));
    }
    let mut result = envelope("manuscripts-list", &authors);
    result["items"] = selected
        .into_iter()
        .map(metadata)
        .collect::<Vec<_>>()
        .into();
    codec::json_bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}

pub fn detail(root: &Path, request: &DetailRequest) -> Result<Json> {
    validate_detail(request)?;
    let authors = snapshot(root, request.revision.as_deref())?;
    let record = authors
        .documents
        .get(&request.id)
        .ok_or_else(|| Error::NotFound("manuscript source is unavailable".into()))?;
    if request
        .expected_sha256
        .as_deref()
        .is_some_and(|expected| expected != source_hash(record))
    {
        return Err(conflict());
    }
    let raw = std::str::from_utf8(&record.raw_bytes).expect("validated UTF-8 manuscript");
    let yaml_start = raw.find('\n').expect("validated YAML opener") + 1;
    let prefix = &raw[..raw.len() - record.body.len()];
    let yaml_end = prefix.len() - if prefix.ends_with("---\r\n") { 5 } else { 4 };
    let mut document = metadata(record);
    document["frontmatter"] = serde_json::to_value(&record.frontmatter)?;
    if let Some(point) = record.get("placement") {
        document["frontmatter"]["placement"] = serde_json::to_value(placement(point)?)?;
    }
    document["frontmatterYaml"] = raw[yaml_start..yaml_end].into();
    document["bodyMarkdown"] = record.body.clone().into();
    document["rawSource"] = raw.into();
    let mut result = envelope("manuscripts-detail", &authors);
    result["document"] = document;
    codec::json_bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}

/// Compare the complete retained World against another bounded original read.
/// A caller cannot save using edited projections with a copied fingerprint.
fn current(world: &World) -> Result<()> {
    if world.revision.is_some() {
        return Err(invalid("manuscript saves require the current worktree"));
    }
    let fresh = historical::load_current_world(&world.root)?;
    if fresh.fingerprint != world.fingerprint
        || fresh.records.len() != world.records.len()
        || world.records.iter().any(|(id, record)| {
            fresh.records.get(id).is_none_or(|original| {
                record.id != original.id
                    || record.kind != original.kind
                    || record.title != original.title
                    || record.source_path != original.source_path
                    || record.frontmatter != original.frontmatter
                    || record.body != original.body
                    || record.raw_bytes != original.raw_bytes
                    || record.blob_oid != original.blob_oid
            })
        })
    {
        return Err(conflict());
    }
    Ok(())
}
fn check_private(root: &Path, expected: &str) -> Result<()> {
    if snapshot(root, None)?.fingerprint != expected {
        return Err(conflict());
    }
    Ok(())
}

pub fn resolve(root: &Path, request: &ResolveRequest) -> Result<Json> {
    validate_resolve(request)?;
    let publication = if request.revision.is_none() {
        publication::read_stamp(root)?
    } else {
        None
    };
    let world = match request.revision.as_deref() {
        Some(revision) => historical::load_world(root, revision)?,
        None => historical::load_current_world(root)?,
    };
    let authors = snapshot(root, request.revision.as_deref())?;
    if request
        .expected_author_fingerprint
        .as_deref()
        .is_some_and(|expected| expected != authors.fingerprint)
    {
        return Err(conflict());
    }
    let result = resolve_work(
        &world,
        &authors,
        &request.work_id,
        request.max_occurrences,
        request.max_characters,
    )?;
    if request.revision.is_none() {
        current(&world)?;
        check_private(root, &authors.fingerprint)?;
        if publication::read_stamp(root)? != publication {
            return Err(conflict());
        }
    }
    Ok(result)
}

/// Private author resolution may inspect a coherent current pair or one exact
/// original commit. The separate reader service accepts only committed source.
pub fn resolve_reveal(root: &Path, request: &RevealResolveRequest) -> Result<Json> {
    validate_reveal_resolve(request)?;
    let publication = if request.revision.is_none() {
        publication::read_stamp(root)?
    } else {
        None
    };
    let world = match request.revision.as_deref() {
        Some(revision) => historical::load_world(root, revision)?,
        None => historical::load_current_world(root)?,
    };
    let authors = snapshot(root, request.revision.as_deref())?;
    if request
        .expected_author_fingerprint
        .as_deref()
        .is_some_and(|expected| expected != authors.fingerprint)
    {
        return Err(conflict());
    }
    let result = reveal::report(
        &world,
        &authors,
        &request.plan_id,
        request.max_occurrences,
        request.max_characters,
    )?;
    if request.revision.is_none() {
        current(&world)?;
        check_private(root, &authors.fingerprint)?;
        if publication::read_stamp(root)? != publication {
            return Err(conflict());
        }
    }
    Ok(result)
}

struct Prepared {
    before: Option<Record>,
    after: Record,
    before_fingerprint: String,
    after_fingerprint: String,
}
impl Prepared {
    fn changed(&self) -> bool {
        self.before
            .as_ref()
            .is_none_or(|before| before.raw_bytes != self.after.raw_bytes)
    }
}

fn prepare(world: &World, request: &SaveRequest) -> Result<Prepared> {
    validate_save(request)?;
    let after = parse_source(request.source.as_bytes())?;
    if world.revision.is_some() || request.expected_fingerprint != world.fingerprint {
        return Err(conflict());
    }
    let publication = publication::read_stamp(&world.root)?;
    current(world)?;
    let mut authors = snapshot(&world.root, None)?;
    if authors.fingerprint != request.expected_author_fingerprint {
        return Err(conflict());
    }
    let before = authors.documents.remove(&after.id);
    if before.as_ref().map(source_hash).as_deref() != request.expected_sha256.as_deref() {
        return Err(conflict());
    }
    let before_fingerprint = authors.fingerprint;
    let mut records: Vec<_> = authors.documents.into_values().collect();
    records.push(after.clone());
    let candidate = snapshot_from_records(records, None)?;
    validate_snapshot(world, &candidate)?;
    current(world)?;
    check_private(&world.root, &before_fingerprint)?;
    if publication::read_stamp(&world.root)? != publication {
        return Err(conflict());
    }
    Ok(Prepared {
        before,
        after,
        before_fingerprint,
        after_fingerprint: candidate.fingerprint,
    })
}

fn write_report(world: &World, prepared: &Prepared, preview: bool) -> Result<Json> {
    let mut result = json!({"protocol":crate::CONTRACT,
        "operation":if preview {"manuscripts-preview-save"} else {"manuscripts-save"},
        "scope":"private-manuscripts","id":prepared.after.id,"kind":prepared.after.kind,
        "path":prepared.after.source_path,"fingerprint":world.fingerprint,"revision":null,
        "authorFingerprint":if preview {&prepared.before_fingerprint} else {&prepared.after_fingerprint},
        "beforeAuthorFingerprint":prepared.before_fingerprint,"afterAuthorFingerprint":prepared.after_fingerprint,
        "beforeSha256":prepared.before.as_ref().map(source_hash),"afterSha256":source_hash(&prepared.after),
        "sourceSha256":source_hash(&prepared.after),
        "afterSource":std::str::from_utf8(&prepared.after.raw_bytes).expect("validated UTF-8"),
        "changed":prepared.changed(),"canSave":true,"writesPerformed":!preview && prepared.changed(),
        "canApply":false,"readerAuthorized":false,"validationComplete":false,"gitCommit":null});
    if preview {
        result["beforeSource"] = prepared
            .before
            .as_ref()
            .map(|before| std::str::from_utf8(&before.raw_bytes).expect("validated UTF-8"))
            .into();
    }
    codec::json_bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}

pub fn save_preview(world: &World, request: &SaveRequest) -> Result<Json> {
    write_report(world, &prepare(world, request)?, true)
}

pub fn save(world: &World, request: &SaveRequest) -> Result<Json> {
    let initial = prepare(world, request)?;
    if !initial.changed() {
        return write_report(world, &initial, false);
    }
    let _lock = authoring::writer_lock(&world.root)?;
    let prepared = prepare(world, request)?;
    if prepared.after.raw_bytes != initial.after.raw_bytes
        || prepared.before_fingerprint != initial.before_fingerprint
        || prepared.after_fingerprint != initial.after_fingerprint
    {
        return Err(conflict());
    }
    let report = write_report(world, &prepared, false)?;
    let folder = match prepared.after.kind.as_str() {
        "work" => "works",
        "manuscript-segment" => "segments",
        "reveal-plan" => "reveals",
        _ => return Err(invalid("unsupported private source kind")),
    };
    let directory = directory(&world.root, folder, true)?.expect("created manuscript directory");
    let target = world.root.join(&prepared.after.source_path);
    let permissions = if let Some(before) = &prepared.before {
        let metadata = fs::symlink_metadata(&target)?;
        let expected = stamp(&metadata)?;
        if metadata.permissions().readonly() {
            return Err(invalid("manuscript source is not writable"));
        }
        if read_bytes(&target, &expected)? != before.raw_bytes {
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
        .prefix(TEMP_PREFIX)
        .tempfile_in(&directory)?;
    temporary.write_all(&prepared.after.raw_bytes)?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    current(world)?;
    check_private(&world.root, &prepared.before_fingerprint)?;
    self::directory(&world.root, folder, false)?;
    if let Some(before) = &prepared.before {
        let expected = stamp(&fs::symlink_metadata(&target)?)?;
        if read_bytes(&target, &expected)? != before.raw_bytes {
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
