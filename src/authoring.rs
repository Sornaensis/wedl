//! Pure candidates and one-file worktree saves. Every edit uses the same
//! validation, fingerprint/hash checks, OS writer lock and atomic publication.
use crate::{
    Error, Result,
    model::{Record, World},
    record_edit, source, validation,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

struct PreparedEdit {
    before: Option<Record>,
    after: Record,
    fingerprint: String,
    validation: validation::ValidationReport,
}

impl PreparedEdit {
    fn changed(&self) -> bool {
        self.before
            .as_ref()
            .is_none_or(|before| before.raw_bytes != self.after.raw_bytes)
    }

    fn after_hash(&self) -> String {
        hex::encode(Sha256::digest(&self.after.raw_bytes))
    }

    fn before_hash(&self) -> Option<String> {
        self.before
            .as_ref()
            .map(|before| hex::encode(Sha256::digest(&before.raw_bytes)))
    }

    fn preview(&self, world: &World, operation: &str) -> Result<Value> {
        let source_text = |record: &Record| {
            String::from_utf8(record.raw_bytes.clone())
                .map_err(|_| Error::Data("record source is not UTF-8".into()))
        };
        Ok(json!({
            "protocol":crate::CONTRACT,"operation":format!("{operation}-preview"),
            "entityId":self.after.id,"sourcePath":self.after.source_path,
            "baseFingerprint":world.fingerprint,"baseRevision":world.revision,
            "candidateFingerprint":self.fingerprint,
            "beforeSha256":self.before_hash(),"afterSha256":self.after_hash(),
            "changed":self.changed(),
            "beforeBody":self.before.as_ref().map(|record| &record.body),"afterBody":self.after.body,
            "beforeSource":self.before.as_ref().map(source_text).transpose()?,
            "afterSource":source_text(&self.after)?,
            "validation":self.validation,"writesPerformed":false,
            "canApply":self.validation.valid && world.revision.is_none(),
            "notice":"Preview only. Apply the same request with baseFingerprint and afterSha256; one worktree file will change.",
        }))
    }
}

fn check_base(world: &World, expected_fingerprint: &str) -> Result<()> {
    if expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the expected snapshot".into(),
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "base snapshot failed validation; run validate for diagnostics".into(),
        ));
    }
    Ok(())
}

fn prepare(world: &World, after: Record, create: bool) -> Result<PreparedEdit> {
    let before = world.records.get(&after.id).cloned();
    if create && before.is_some() {
        return Err(Error::Conflict(format!(
            "record {} already exists",
            after.id
        )));
    }
    if !create && before.is_none() {
        return Err(Error::NotFound(format!(
            "record {} does not exist",
            after.id
        )));
    }
    if before
        .as_ref()
        .is_some_and(|record| record.kind != after.kind || record.source_path != after.source_path)
    {
        return Err(Error::InvalidInput(
            "updates cannot change record identity, kind or source path".into(),
        ));
    }
    let mut records: Vec<_> = world
        .records
        .values()
        .filter(|record| record.id != after.id)
        .cloned()
        .collect();
    records.push(after.clone());
    let candidate = source::world_from_records(world.root.clone(), records, None)?;
    Ok(PreparedEdit {
        before,
        after,
        fingerprint: candidate.fingerprint.clone(),
        validation: validation::validate_world(&candidate),
    })
}

fn body_candidate(world: &World, entity: &str, body: &str) -> Result<PreparedEdit> {
    if body.len() > 1024 * 1024 {
        return Err(Error::InvalidInput("replacement body exceeds 1 MiB".into()));
    }
    let record = world.resolve(entity)?;
    let header_end = record
        .raw_bytes
        .len()
        .checked_sub(record.body.len())
        .ok_or_else(|| Error::Data("invalid raw source/body boundary".into()))?;
    let mut bytes = record.raw_bytes[..header_end].to_vec();
    bytes.extend_from_slice(body.as_bytes());
    prepare(
        world,
        source::parse_record(&bytes, &record.source_path)?,
        false,
    )
}

pub fn preview_body_edit(
    world: &World,
    entity: &str,
    new_body: &str,
    expected_fingerprint: &str,
) -> Result<Value> {
    check_base(world, expected_fingerprint)?;
    body_candidate(world, entity, new_body)?.preview(world, "body-edit")
}

pub fn apply_body_edit(
    world: &World,
    entity: &str,
    new_body: &str,
    expected_fingerprint: &str,
    expected_after_sha256: &str,
) -> Result<Value> {
    check_base(world, expected_fingerprint)?;
    apply(
        world,
        body_candidate(world, entity, new_body)?,
        expected_after_sha256,
        "body-edit",
    )
}

pub fn preview_record_create(
    world: &World,
    draft: &str,
    expected_fingerprint: &str,
) -> Result<Value> {
    check_base(world, expected_fingerprint)?;
    prepare(world, record_edit::create(world, draft)?, true)?.preview(world, "record-create")
}

pub fn apply_record_create(
    world: &World,
    draft: &str,
    expected_fingerprint: &str,
    expected_after_sha256: &str,
) -> Result<Value> {
    check_base(world, expected_fingerprint)?;
    apply(
        world,
        prepare(world, record_edit::create(world, draft)?, true)?,
        expected_after_sha256,
        "record-create",
    )
}

pub fn preview_record_update(
    world: &World,
    entity: &str,
    patch: &str,
    expected_fingerprint: &str,
) -> Result<Value> {
    check_base(world, expected_fingerprint)?;
    prepare(world, record_edit::update(world, entity, patch)?, false)?
        .preview(world, "record-update")
}

pub fn apply_record_update(
    world: &World,
    entity: &str,
    patch: &str,
    expected_fingerprint: &str,
    expected_after_sha256: &str,
) -> Result<Value> {
    check_base(world, expected_fingerprint)?;
    apply(
        world,
        prepare(world, record_edit::update(world, entity, patch)?, false)?,
        expected_after_sha256,
        "record-update",
    )
}

fn fresh_snapshot(world: &World) -> Result<World> {
    let fresh = source::load_world(&world.root, None)?;
    if fresh.fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source changed since preview; preview again".into(),
        ));
    }
    Ok(fresh)
}

pub(crate) fn writer_lock(root: &Path) -> Result<fs::File> {
    let directory = root.join(".wedl-rust");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(Error::InvalidInput(
                "writer directory must be a real directory".into(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::create_dir_all(&directory)?;
    let path = directory.join("write.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(Error::InvalidInput(
                "writer lock must be a regular file".into(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    lock.try_lock()
        .map_err(|error| Error::Conflict(format!("writer lock unavailable: {error}")))?;
    Ok(lock)
}

fn source_target(root: &Path, relative: &str, create: bool) -> Result<PathBuf> {
    let path = Path::new(relative);
    if !relative.starts_with("story/")
        || relative.contains('\\')
        || path.extension().and_then(|value| value.to_str()) != Some("md")
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(Error::InvalidInput(
            "record path must be a Markdown file under story/".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::Data("record has no parent".into()))?;
    let mut directory = root.to_owned();
    for component in parent.components() {
        directory.push(component);
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(Error::InvalidInput(
                    "source parent must be a real directory".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&directory)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(root.join(path))
}

fn apply(world: &World, edit: PreparedEdit, expected_hash: &str, operation: &str) -> Result<Value> {
    if world.revision.is_some() {
        return Err(Error::InvalidInput(
            "historical snapshots are read-only".into(),
        ));
    }
    if edit.after_hash() != expected_hash {
        return Err(Error::Conflict(
            "replacement differs from the previewed source hash".into(),
        ));
    }
    if !edit.validation.valid {
        return Err(Error::Validation(
            "candidate failed validation; source was not changed".into(),
        ));
    }
    let fresh = fresh_snapshot(world)?;
    if edit.changed() {
        let _lock = writer_lock(&fresh.root)?;
        fresh_snapshot(world)?;
        let create = edit.before.is_none();
        let target = source_target(&fresh.root, &edit.after.source_path, create)?;
        let permissions = match fs::symlink_metadata(&target) {
            Ok(_) if create => {
                return Err(Error::Conflict("new record path already exists".into()));
            }
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(Error::InvalidInput(
                    "source target must be a regular file".into(),
                ));
            }
            Ok(metadata) if metadata.permissions().readonly() => {
                return Err(Error::InvalidInput("source file is read-only".into()));
            }
            Ok(metadata) => Some(metadata.permissions()),
            Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut temporary = tempfile::Builder::new().prefix(".wedl-edit-").tempfile_in(
            target
                .parent()
                .ok_or_else(|| Error::Data("record has no parent".into()))?,
        )?;
        temporary.write_all(&edit.after.raw_bytes)?;
        if let Some(permissions) = permissions {
            temporary.as_file().set_permissions(permissions)?;
        }
        temporary.as_file().sync_all()?;
        // Advisory locks serialize our writers. External editors must coordinate;
        // the final snapshot check cannot provide filesystem compare-and-swap.
        fresh_snapshot(world)?;
        if create {
            temporary.persist_noclobber(&target).map_err(|error| {
                if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::Conflict("new record path already exists".into())
                } else {
                    Error::Io(error.error)
                }
            })?;
        } else {
            temporary
                .persist(&target)
                .map_err(|error| Error::Io(error.error))?;
        }
    }
    Ok(json!({
        "protocol":crate::CONTRACT,"operation":operation,"entityId":edit.after.id,
        "sourcePath":edit.after.source_path,"changed":edit.changed(),"writesPerformed":edit.changed(),
        "baseFingerprint":world.fingerprint,"fingerprint":edit.fingerprint,
        "beforeSha256":edit.before_hash(),"afterSha256":expected_hash,
        "cache":"rebuild-on-next-search","gitCommit":null,
    }))
}
