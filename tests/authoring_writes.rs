use std::{fs, process::Command};
use wedl::{authoring, source, validation};

fn world() -> (tempfile::TempDir, wedl::model::World) {
    let directory = tempfile::tempdir().unwrap();
    let world = source::init_world(&directory.path().join("world"), "New World").unwrap();
    (directory, world)
}

fn preview(world: &wedl::model::World, body: &str) -> serde_json::Value {
    authoring::preview_body_edit(world, "New World", body, &world.fingerprint).unwrap()
}

fn apply(
    world: &wedl::model::World,
    body: &str,
    preview: &serde_json::Value,
) -> wedl::Result<serde_json::Value> {
    authoring::apply_body_edit(
        world,
        "New World",
        body,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    )
}

#[test]
fn init_is_minimal_current_schema_and_refuses_existing_paths() {
    let (_directory, world) = world();
    assert_eq!(world.records.len(), 1);
    assert!(validation::validate_world(&world).valid);
    assert_eq!(
        world.world_record().unwrap().string("schema"),
        Some("wedl/v1")
    );
    assert!(!world.root.join(".git").exists());
    assert!(!world.root.join(".wedl-rust").exists());
    let before = fs::read(world.root.join("story/world.md")).unwrap();
    assert!(source::init_world(&world.root, "Replacement").is_err());
    assert_eq!(fs::read(world.root.join("story/world.md")).unwrap(), before);
    let old = String::from_utf8(before)
        .unwrap()
        .replace("wedl/v1", "wedl/unsupported");
    assert!(source::parse_record(old.as_bytes(), "story/world.md").is_err());
    let invalid = world.root.join("bad-title");
    assert!(source::init_world(&invalid, "\n").is_err());
    assert!(!invalid.exists());
}

#[test]
fn apply_preserves_envelope_git_index_and_head() {
    let (_directory, world) = world();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&world.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    git(&["init", "-q"]);
    git(&["add", "story/world.md"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "initial",
    ]);
    let index = fs::read(world.root.join(".git/index")).unwrap();
    let head = git(&["rev-parse", "HEAD"]);
    let before = world.world_record().unwrap();
    let envelope = &before.raw_bytes[..before.raw_bytes.len() - before.body.len()];
    let body = "\r\n# A changed world 🌲\r\n\nTrailing space  ";
    let result = apply(&world, body, &preview(&world, body)).unwrap();
    let after = source::load_world(&world.root, None).unwrap();
    assert_eq!(after.world_record().unwrap().body, body);
    assert!(
        after
            .world_record()
            .unwrap()
            .raw_bytes
            .starts_with(envelope)
    );
    assert_eq!(result["fingerprint"], after.fingerprint);
    assert_eq!(result["changed"], true);
    assert_eq!(fs::read(world.root.join(".git/index")).unwrap(), index);
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert!(!world.root.join(".wedl-rust/index.sqlite3").exists());
}

#[test]
fn stale_changed_and_invalid_requests_leave_source_untouched() {
    let (_directory, world) = world();
    let base = fs::read(world.root.join("story/world.md")).unwrap();
    let request = preview(&world, "new body");
    assert!(apply(&world, "different body", &request).is_err());
    let invalid_body = "[missing](story:char_00000000000000000000000001)";
    let invalid = preview(&world, invalid_body);
    assert_eq!(invalid["canApply"], false);
    assert!(apply(&world, invalid_body, &invalid).is_err());
    assert_eq!(fs::read(world.root.join("story/world.md")).unwrap(), base);
    assert!(!world.root.join(".wedl-rust").exists());
    let mut external = base.clone();
    external.extend_from_slice(b"\nExternal change\n");
    fs::write(world.root.join("story/world.md"), &external).unwrap();
    assert!(matches!(
        apply(&world, "new body", &request),
        Err(wedl::Error::Conflict(_))
    ));
    assert_eq!(
        fs::read(world.root.join("story/world.md")).unwrap(),
        external
    );
}

#[test]
fn no_op_needs_no_lock_or_cache_and_history_is_read_only() {
    let (_directory, world) = world();
    let body = &world.world_record().unwrap().body;
    let result = apply(&world, body, &preview(&world, body)).unwrap();
    assert_eq!(result["writesPerformed"], false);
    assert!(!world.root.join(".wedl-rust").exists());
    let mut historical = world.clone();
    historical.revision = Some("immutable-commit".into());
    let draft = preview(&historical, "cannot save history");
    assert_eq!(draft["canApply"], false);
    assert!(apply(&historical, "cannot save history", &draft).is_err());
}

#[test]
fn competing_writer_is_refused_and_releasing_lock_allows_apply() {
    let (_directory, world) = world();
    let draft = preview(&world, "one writer");
    fs::create_dir(world.root.join(".wedl-rust")).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(world.root.join(".wedl-rust/write.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(matches!(
        apply(&world, "one writer", &draft),
        Err(wedl::Error::Conflict(_))
    ));
    assert_ne!(
        source::load_world(&world.root, None)
            .unwrap()
            .world_record()
            .unwrap()
            .body,
        "one writer"
    );
    drop(lock);
    assert!(apply(&world, "one writer", &draft).is_ok());
    assert!(matches!(
        apply(&world, "one writer", &draft),
        Err(wedl::Error::Conflict(_))
    ));
}

#[cfg(unix)]
#[test]
fn writer_never_follows_a_cache_symlink() {
    let (_directory, world) = world();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), world.root.join(".wedl-rust")).unwrap();
    assert!(apply(&world, "replacement", &preview(&world, "replacement")).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
