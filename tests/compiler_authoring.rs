use std::{
    fs,
    path::{Path, PathBuf},
};
use wedl::{authoring, compiler, source};

fn copy_tree(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.unwrap();
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn example() -> (tempfile::TempDir, wedl::model::World) {
    let temporary = tempfile::tempdir().unwrap();
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass/story"),
        &temporary.path().join("story"),
    );
    let world = source::load_world(temporary.path(), None).unwrap();
    (temporary, world)
}

#[test]
fn status_is_pure_and_compile_builds_a_real_searchable_index() {
    let (directory, world) = example();
    assert_eq!(compiler::status(&world).unwrap()["ready"], false);
    assert!(!directory.path().join(".wedl-rust").exists());
    let report = compiler::compile(&world).unwrap();
    assert_eq!(report.record_count, 64);
    assert!(!report.validation_complete);
    assert_eq!(compiler::status(&world).unwrap()["ready"], true);
    let cache = rusqlite::Connection::open(&report.path).unwrap();
    let schema: String = cache
        .query_row(
            "SELECT value FROM metadata WHERE key='source_schema'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(schema, source::SOURCE_SCHEMA);
    cache
        .execute(
            "UPDATE metadata SET value='obsolete' WHERE key='source_schema'",
            [],
        )
        .unwrap();
    drop(cache);
    assert_eq!(compiler::status(&world).unwrap()["ready"], false);
    let results = compiler::search(&world, "ferry", 100, None).unwrap();
    assert_eq!(compiler::status(&world).unwrap()["ready"], true);
    assert!(!results["items"].as_array().unwrap().is_empty());
    assert!(
        results["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["kind"] != "hypothesis")
    );
    assert_eq!(results["scope"], "author-all-time");
    let narrow = compiler::search(&world, "ferry", 100, Some("knowledge")).unwrap();
    assert!(
        narrow["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["kind"] == "knowledge")
    );
}

#[test]
fn source_fingerprint_invalidates_cache_and_rebuilds_with_new_text() {
    let (_directory, world) = example();
    compiler::compile(&world).unwrap();
    let record = world.resolve("Mina Vale").unwrap();
    let path = world.root.join(&record.source_path);
    let mut bytes = record.raw_bytes.clone();
    bytes.extend_from_slice(b"\nRustCacheSentinel\n");
    fs::write(&path, bytes).unwrap();
    let updated = source::load_world(&world.root, None).unwrap();
    assert_ne!(world.fingerprint, updated.fingerprint);
    assert_eq!(compiler::status(&updated).unwrap()["ready"], false);
    let results = compiler::search(&updated, "RustCacheSentinel", 10, None).unwrap();
    assert_eq!(results["items"].as_array().unwrap().len(), 1);
    assert_eq!(results["items"][0]["id"], record.id);
    assert_eq!(compiler::status(&updated).unwrap()["ready"], true);
    // Old snapshots are not served another snapshot's newly published index.
    assert_eq!(compiler::status(&world).unwrap()["ready"], false);
}

#[test]
fn invalid_source_cannot_replace_a_previous_index() {
    let (_directory, world) = example();
    let report = compiler::compile(&world).unwrap();
    let before = fs::read(&report.path).unwrap();
    let mut invalid = world.clone();
    invalid.records.retain(|_, record| record.kind != "world");
    assert!(compiler::compile(&invalid).is_err());
    assert_eq!(fs::read(&report.path).unwrap(), before);
    let temporary_files = fs::read_dir(report.path.parent().unwrap()).unwrap().count();
    assert_eq!(temporary_files, 1);
}

#[test]
fn corrupt_projection_is_rebuilt_and_bad_queries_do_not_create_files() {
    let (directory, world) = example();
    for (query, limit) in [("", 10), ("ferry", 0), ("ferry", 101)] {
        assert!(compiler::search(&world, query, limit, None).is_err());
    }
    assert!(!directory.path().join(".wedl-rust").exists());
    let report = compiler::compile(&world).unwrap();
    fs::write(&report.path, b"this is not sqlite").unwrap();
    assert_eq!(compiler::status(&world).unwrap()["ready"], false);
    assert!(
        !compiler::search(&world, "ferry", 10, None).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Literal FTS punctuation is data, not a way to select every row.
    let injection = compiler::search(&world, "ferry OR nonexistenttoken", 100, None).unwrap();
    assert!(injection["items"].as_array().unwrap().is_empty());
}

#[test]
fn body_preview_is_snapshot_bound_pure_and_preserves_empty_replacement() {
    let (directory, world) = example();
    let record = world.resolve("Mina Vale").unwrap();
    let before = fs::read(world.root.join(&record.source_path)).unwrap();
    let preview =
        authoring::preview_body_edit(&world, "Mina Vale", "", &world.fingerprint).unwrap();
    assert_eq!(preview["afterBody"], "");
    assert_eq!(preview["writesPerformed"], false);
    assert_eq!(preview["canApply"], true);
    assert_eq!(preview["validation"]["complete"], false);
    use sha2::{Digest, Sha256};
    let unchanged_envelope = &before[..before.len() - record.body.len()];
    assert_eq!(
        preview["afterSha256"],
        hex::encode(Sha256::digest(unchanged_envelope))
    );
    assert_eq!(
        fs::read(world.root.join(&record.source_path)).unwrap(),
        before
    );
    assert!(!directory.path().join(".wedl-rust").exists());
    assert!(authoring::preview_body_edit(&world, "Mina Vale", "changed", "stale").is_err());
    let unchanged =
        authoring::preview_body_edit(&world, "Mina Vale", &record.body, &world.fingerprint)
            .unwrap();
    assert_eq!(unchanged["changed"], false);
    assert_eq!(unchanged["beforeSha256"], unchanged["afterSha256"]);
}

#[cfg(unix)]
#[test]
fn cache_symlink_is_refused() {
    let (directory, world) = example();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join(".wedl-rust")).unwrap();
    assert!(compiler::compile(&world).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
