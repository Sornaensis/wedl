//! Process-private SQLite source generations for Compendium reads.
use crate::{Error, Result, historical, model::World, source, validation::ValidationReport};
use rusqlite::{Connection, MAIN_DB, params, serialize::Data, types::ValueRef};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

const COMPATIBILITY: &str = "wedl-rust-compendium-generation/v1";
const PAGE_BYTES: u64 = 4096;
const MAX_PAGES: u64 = 32_768;
const MANIFEST_SCHEMA: &str = "CREATE TABLE manifest (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    body BLOB NOT NULL
) STRICT";
const RECORDS_SCHEMA: &str = "CREATE TABLE records (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    raw_source BLOB NOT NULL,
    blob_oid TEXT
) STRICT, WITHOUT ROWID";

#[derive(PartialEq, Eq, Serialize)]
struct Original {
    repository: [String; 3],
    revision: String,
    object_format: &'static str,
}

#[derive(PartialEq, Eq)]
struct Key {
    root: PathBuf,
    fingerprint: String,
    original: Option<Original>,
}

impl Key {
    fn capture(world: &World) -> Result<Self> {
        let original = match &world.revision {
            Some(revision) => {
                if !historical::valid_oid(revision) {
                    return Err(unavailable());
                }
                Some(Original {
                    repository: historical::repository_identity(&world.root)?,
                    revision: revision.clone(),
                    object_format: if revision.len() == 40 {
                        "sha1"
                    } else {
                        "sha256"
                    },
                })
            }
            None => None,
        };
        Ok(Self {
            root: world.root.clone(),
            fingerprint: world.fingerprint.clone(),
            original,
        })
    }

    fn manifest(&self, world: &World, validation: &ValidationReport) -> Result<Vec<u8>> {
        let scope = hex::encode(Sha256::digest(self.root.as_os_str().as_encoded_bytes()));
        Ok(serde_json::to_vec(&serde_json::json!({
            "compatibility": COMPATIBILITY,
            "source_schema": source::SOURCE_SCHEMA,
            "scope": scope,
            "fingerprint": self.fingerprint,
            "original": self.original,
            "record_count": world.records.len(),
            "validation": {
                "valid": validation.valid,
                "complete": validation.complete,
                "diagnostics_count": validation.diagnostics.len(),
            },
        }))?)
    }
}

pub(crate) struct Generation {
    key: Key,
    world: World,
    // Own the verified image for as long as any request pins its projection.
    _image: Mutex<Connection>,
}

impl Generation {
    pub(crate) fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Default)]
struct State {
    ready: Option<Arc<Generation>>,
    retired: Weak<Generation>,
    worktree: Weak<Generation>,
}

#[derive(Default)]
pub(crate) struct Generations {
    state: Mutex<State>,
}

impl Generations {
    /// A bounded prior image for native byte comparison, never authority to
    /// answer a current request without a successful source fence.
    pub(crate) fn prior_worktree(&self) -> Result<Option<Arc<Generation>>> {
        let state = self.state.lock().map_err(|_| unavailable())?;
        Ok(state.worktree.upgrade())
    }

    /// Keep at most two images, including a staging image or a retired pin.
    /// A same-source caller shares the already verified projection. Failed
    /// candidates leave the prior generation intact but never answer from it.
    pub(crate) fn pin(
        &self,
        world: &World,
        validation: &ValidationReport,
        live: bool,
    ) -> Result<Arc<Generation>> {
        if !validation.valid {
            return Err(Error::Validation(
                "Compendium source validation failed".into(),
            ));
        }
        let key = Key::capture(world)?;
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        let available = state
            .ready
            .as_ref()
            .filter(|ready| ready.key == key)
            .cloned()
            .or_else(|| state.retired.upgrade());
        let generation = if let Some(pinned) = available {
            // The byte fingerprint covers paths and raw content, but not
            // caller-supplied Git provenance. Verify it on reuse as well.
            if pinned.key != key
                || pinned.world.records.len() != world.records.len()
                || pinned.world.records.iter().any(|(id, record)| {
                    world
                        .records
                        .get(id)
                        .is_none_or(|source| source.blob_oid != record.blob_oid)
                })
            {
                return Err(unavailable());
            }
            pinned
        } else {
            let candidate = Arc::new(build(key, world, validation, live)?);
            if let Some(previous) = state.ready.replace(candidate.clone()) {
                state.retired = Arc::downgrade(&previous);
            }
            candidate
        };
        if live {
            state.worktree = Arc::downgrade(&generation);
        }
        Ok(generation)
    }
}

fn unavailable() -> Error {
    Error::Data("Compendium SQLite generation is unavailable".into())
}

fn build(
    key: Key,
    source: &World,
    validation: &ValidationReport,
    live: bool,
) -> Result<Generation> {
    if key.original.is_some() {
        historical::verify_world(source)?;
    }
    let manifest = key.manifest(source, validation)?;
    let mut image = reopen_read_only(stage(source, &manifest)?)?;
    let world = materialize(&mut image, source, &manifest)?;

    if live && historical::capture_current_world(&source.root, Some(source))?.is_some() {
        return Err(Error::Conflict(
            "working source changed while compiling its Compendium; reload".into(),
        ));
    }
    if let Some(original) = &key.original
        && historical::repository_identity(&source.root)? != original.repository
    {
        return Err(unavailable());
    }
    Ok(Generation {
        key,
        world,
        _image: Mutex::new(image),
    })
}

fn stage(world: &World, manifest: &[u8]) -> Result<Connection> {
    let mut connection = Connection::open_in_memory()?;
    connection.execute_batch(&format!(
        "PRAGMA page_size={PAGE_BYTES};
         PRAGMA max_page_count={MAX_PAGES};
         PRAGMA temp_store=MEMORY;"
    ))?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(&format!("{MANIFEST_SCHEMA};{RECORDS_SCHEMA};"))?;
    transaction.execute("INSERT INTO manifest VALUES (1, ?1)", [manifest])?;
    let mut budget = historical::WorldBudget::default();
    {
        let mut insert = transaction.prepare("INSERT INTO records VALUES (?1,?2,?3,?4,?5,?6)")?;
        for (id, record) in &world.records {
            if id != &record.id || world.revision.is_some() != record.blob_oid.is_some() {
                return Err(unavailable());
            }
            budget.admit_record(&record.source_path, &record.raw_bytes)?;
            insert.execute(params![
                record.id,
                record.source_path,
                record.kind,
                record.title,
                record.raw_bytes,
                record.blob_oid,
            ])?;
        }
    }
    transaction.commit()?;
    Ok(connection)
}

fn reopen_read_only(staging: Connection) -> Result<Connection> {
    let page_size: u64 = staging.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let pages: u64 = staging.pragma_query_value(None, "page_count", |row| row.get(0))?;
    if page_size != PAGE_BYTES || !(1..=MAX_PAGES).contains(&pages) {
        return Err(unavailable());
    }
    let data = staging.serialize(MAIN_DB)?;
    if data.len() as u64 != page_size * pages {
        return Err(unavailable());
    }
    let Data::Owned(bytes) = data else {
        return Err(unavailable());
    };
    drop(staging);

    let mut connection = Connection::open_in_memory()?;
    connection.deserialize(MAIN_DB, bytes, true)?;
    connection.execute_batch(
        "PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY;",
    )?;
    Ok(connection)
}

fn materialize(connection: &mut Connection, source: &World, manifest: &[u8]) -> Result<World> {
    let transaction = connection.transaction()?;
    {
        let mut schema = transaction.prepare(
            "SELECT type, name, sql FROM sqlite_schema
             WHERE name NOT GLOB 'sqlite_*' ORDER BY name",
        )?;
        let mut rows = schema.query([])?;
        for (table, definition) in [("manifest", MANIFEST_SCHEMA), ("records", RECORDS_SCHEMA)] {
            let row = rows.next()?.ok_or_else(unavailable)?;
            if row.get_ref(0)? != ValueRef::Text(b"table")
                || row.get_ref(1)? != ValueRef::Text(table.as_bytes())
                || row.get_ref(2)? != ValueRef::Text(definition.as_bytes())
            {
                return Err(unavailable());
            }
        }
        if rows.next()?.is_some() {
            return Err(unavailable());
        }
    }
    let check: String = transaction.query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))?;
    let manifest_matches =
        transaction.query_row("SELECT body FROM manifest WHERE singleton = 1", [], |row| {
            Ok(row.get_ref(0)? == ValueRef::Blob(manifest))
        })?;
    if check != "ok" || !manifest_matches {
        return Err(unavailable());
    }
    let mut records = Vec::with_capacity(source.records.len());
    {
        let mut select = transaction.prepare(
            "SELECT id, source_path, kind, title, raw_source, blob_oid FROM records ORDER BY id",
        )?;
        let mut rows = select.query([])?;
        for (id, expected) in &source.records {
            let row = rows.next()?.ok_or_else(unavailable)?;
            let blob = expected
                .blob_oid
                .as_ref()
                .map_or(ValueRef::Null, |oid| ValueRef::Text(oid.as_bytes()));
            if row.get_ref(0)? != ValueRef::Text(id.as_bytes())
                || row.get_ref(1)? != ValueRef::Text(expected.source_path.as_bytes())
                || row.get_ref(2)? != ValueRef::Text(expected.kind.as_bytes())
                || row.get_ref(3)? != ValueRef::Text(expected.title.as_bytes())
                || row.get_ref(5)? != blob
            {
                return Err(unavailable());
            }
            let ValueRef::Blob(raw) = row.get_ref(4)? else {
                return Err(unavailable());
            };
            if raw != expected.raw_bytes {
                return Err(unavailable());
            }
            let mut record = source::parse_record(raw, &expected.source_path)?;
            if record.id != expected.id
                || record.kind != expected.kind
                || record.title != expected.title
                || record.frontmatter != expected.frontmatter
                || record.body != expected.body
            {
                return Err(unavailable());
            }
            record.blob_oid = expected.blob_oid.clone();
            records.push(record);
        }
        if rows.next()?.is_some() {
            return Err(unavailable());
        }
    }
    let world = source::world_from_records(source.root.clone(), records, source.revision.clone())?;
    if world.fingerprint != source.fingerprint {
        return Err(unavailable());
    }
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::validate_world;
    use std::{fs, sync::Barrier, thread};

    fn fixture() -> (tempfile::TempDir, World) {
        let directory = tempfile::tempdir().unwrap();
        let world = source::init_world(&directory.path().join("world"), "First view").unwrap();
        (directory, world)
    }

    fn change(world: &World, before: &str, after: &str) -> World {
        let path = world.root.join(&world.world_record().unwrap().source_path);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(before));
        fs::write(path, text.replace(before, after)).unwrap();
        source::load_world(&world.root, None).unwrap()
    }

    #[test]
    fn verifies_schema_manifest_and_exact_source_bytes_on_reopen() {
        let (_directory, original) = fixture();
        let path = original.root.join("story/Main-World.md");
        let previous = original.root.join("story/world.md");
        let crlf = fs::read_to_string(&previous).unwrap().replace('\n', "\r\n");
        fs::write(path, &crlf).unwrap();
        fs::remove_file(previous).unwrap();
        let original = source::load_world(&original.root, None).unwrap();
        let manifest = Key::capture(&original)
            .unwrap()
            .manifest(&original, &validate_world(&original))
            .unwrap();
        let mut read_only = reopen_read_only(stage(&original, &manifest).unwrap()).unwrap();

        let copy = materialize(&mut read_only, &original, &manifest).unwrap();
        let record = copy.world_record().unwrap();
        assert_eq!(record.source_path, "story/Main-World.md");
        assert_eq!(record.raw_bytes, crlf.as_bytes());
        assert_eq!(copy.fingerprint, original.fingerprint);
        assert!(materialize(&mut read_only, &original, b"wrong manifest").is_err());
        read_only.execute_batch("PRAGMA query_only=OFF;").unwrap();
        assert!(matches!(
            read_only.execute("DELETE FROM records", []),
            Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::ReadOnly
        ));

        let mut writer = stage(&original, &manifest).unwrap();
        writer
            .execute("CREATE TABLE unrelated (value INTEGER)", [])
            .unwrap();
        assert!(materialize(&mut writer, &original, &manifest).is_err());
        writer.execute("DROP TABLE unrelated", []).unwrap();
        writer
            .execute(
                "UPDATE records SET raw_source = ?1",
                [b"foreign".as_slice()],
            )
            .unwrap();
        assert!(materialize(&mut writer, &original, &manifest).is_err());
    }

    #[test]
    fn concurrent_pins_share_one_image_and_retirement_is_bounded() {
        let (_directory, original) = fixture();
        let generations = Generations::default();
        let report = validate_world(&original);
        let barrier = Barrier::new(4);
        let pins = thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        generations.pin(&original, &report, false).unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert!(pins.iter().all(|pin| Arc::ptr_eq(pin, &pins[0])));
        let mut invalid = report.clone();
        invalid.valid = false;
        assert!(matches!(
            generations.pin(&original, &invalid, false),
            Err(Error::Validation(_))
        ));
        let mut forged = original.clone();
        forged.records.values_mut().next().unwrap().blob_oid = Some("a".repeat(40));
        assert!(generations.pin(&forged, &report, false).is_err());
        let old = Arc::downgrade(&pins[0]);

        let changed = change(&original, "First view", "Second view");
        let next = generations
            .pin(&changed, &validate_world(&changed), false)
            .unwrap();
        assert_ne!(next.world().fingerprint, pins[0].world().fingerprint);
        assert_eq!(pins[0].world().world_record().unwrap().title, "First view");
        let third = change(&changed, "Second view", "Third view");
        let report = validate_world(&third);
        assert!(generations.pin(&third, &report, false).is_err());
        assert!(old.upgrade().is_some());

        drop(pins);
        assert!(old.upgrade().is_none());
        assert!(generations.pin(&third, &report, false).is_ok());
    }

    #[test]
    fn drift_during_a_new_live_build_refuses_without_replacing_the_ready_image() {
        let (_directory, initial) = fixture();
        let generations = Generations::default();
        let report = validate_world(&initial);
        let ready = generations.pin(&initial, &report, true).unwrap();
        let captured = change(&initial, "First view", "Other view");
        let current = change(&captured, "Other view", "Third view");
        assert!(matches!(
            generations.pin(&captured, &validate_world(&captured), true),
            Err(Error::Conflict(_))
        ));
        assert!(Arc::ptr_eq(
            &ready,
            &generations.pin(&initial, &report, false).unwrap()
        ));
        assert_eq!(
            generations
                .pin(&current, &validate_world(&current), true)
                .unwrap()
                .world()
                .fingerprint,
            current.fingerprint
        );
    }
}
