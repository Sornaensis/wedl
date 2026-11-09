use std::fs;
use wedl::{Error, authoring, source, validation};

const HERO: &str = "char_00000000000000000000000001";

fn world() -> (tempfile::TempDir, wedl::model::World) {
    let dir = tempfile::tempdir().unwrap();
    let world = source::init_world(&dir.path().join("world"), "Writing").unwrap();
    (dir, world)
}

fn character() -> String {
    format!("kind: character\nid: {HERO}\ntitle: Ana\nbody: |\n  # Ana\n\n  Original prose.\n")
}

fn create(world: &wedl::model::World) -> wedl::model::World {
    let draft = character();
    let preview = authoring::preview_record_create(world, &draft, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    authoring::apply_record_create(
        world,
        &draft,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    )
    .unwrap();
    source::load_world(&world.root, None).unwrap()
}

#[test]
fn create_preview_is_pure_and_apply_publishes_exact_candidate() {
    let (_dir, world) = world();
    let original = fs::read(world.root.join("story/world.md")).unwrap();
    let draft = character();
    let preview = authoring::preview_record_create(&world, &draft, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert!(preview["beforeSha256"].is_null());
    assert!(preview["beforeSource"].is_null());
    assert_eq!(preview["writesPerformed"], false);
    assert!(!world.root.join("story/characters").exists());
    assert!(!world.root.join(".wedl-rust").exists());
    let saved = authoring::apply_record_create(
        &world,
        &draft,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    )
    .unwrap();
    let after = source::load_world(&world.root, None).unwrap();
    assert_eq!(after.records.len(), 2);
    assert!(validation::validate_world(&after).valid);
    assert_eq!(saved["fingerprint"], after.fingerprint);
    assert_eq!(preview["candidateFingerprint"], after.fingerprint);
    assert_eq!(saved["writesPerformed"], true);
    assert_eq!(
        fs::read_to_string(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap()
    );
    assert_eq!(
        fs::read(world.root.join("story/world.md")).unwrap(),
        original
    );
    assert!(!world.root.join(".wedl-rust/index.sqlite3").exists());
    assert!(authoring::preview_record_create(&after, &draft, &after.fingerprint).is_err());
}

#[test]
fn structured_update_preserves_unedited_metadata_prose_and_permissions() {
    let (_dir, world) = world();
    let world = create(&world);
    let path = world.root.join(&world.resolve(HERO).unwrap().source_path);
    let raw = fs::read_to_string(&path).unwrap();
    let raw = raw.replacen("\n---\n", "\nx-author-note: keep me\n---\n", 1);
    fs::write(&path, &raw).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let world = source::load_world(&world.root, None).unwrap();
    let original = world.resolve(HERO).unwrap();
    let patch = "kind: character\ntitle: Ana Reed\naliases: [Ana]\n";
    let preview =
        authoring::preview_record_update(&world, HERO, patch, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    authoring::apply_record_update(
        &world,
        HERO,
        patch,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    )
    .unwrap();
    let after = source::load_world(&world.root, None).unwrap();
    let changed = after.resolve("Ana Reed").unwrap();
    assert_eq!(changed.id, original.id);
    assert_eq!(changed.source_path, original.source_path);
    assert_eq!(changed.body, original.body);
    assert_eq!(changed.string("x-author-note"), Some("keep me"));
    assert_eq!(after.resolve("Ana").unwrap().id, HERO);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}

#[test]
fn invalid_or_changed_candidates_never_write_source() {
    let (_dir, world) = world();
    let invalid =
        format!("kind: character\nid: {HERO}\ntitle: Ana\ninitial_state: {{undeclared: true}}\n");
    let preview = authoring::preview_record_create(&world, &invalid, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false);
    assert!(matches!(
        authoring::apply_record_create(
            &world,
            &invalid,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Validation(_))
    ));
    let draft = character();
    let preview = authoring::preview_record_create(&world, &draft, &world.fingerprint).unwrap();
    assert!(matches!(
        authoring::apply_record_create(
            &world,
            &draft.replace("title: Ana", "title: Changed"),
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Conflict(_))
    ));
    assert!(!world.root.join(".wedl-rust").exists());
    assert!(!world.root.join("story/characters").exists());
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[test]
fn creation_refuses_occupied_paths_without_overwriting_them() {
    let (_dir, world) = world();
    let draft = character();
    let preview = authoring::preview_record_create(&world, &draft, &world.fingerprint).unwrap();
    let target = world.root.join(preview["sourcePath"].as_str().unwrap());
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep.txt"), "occupied").unwrap();
    // An empty source directory is absent from the source fingerprint, but still
    // must block publication of a record at that path.
    assert!(matches!(
        authoring::apply_record_create(
            &world,
            &draft,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "occupied"
    );
    assert_eq!(
        source::load_world(&world.root, None).unwrap().records.len(),
        1
    );
}

#[test]
fn record_writes_share_the_body_writer_lock_and_history_refusal() {
    let (_dir, world) = world();
    let draft = character();
    let preview = authoring::preview_record_create(&world, &draft, &world.fingerprint).unwrap();
    let hash = preview["afterSha256"].as_str().unwrap();
    let mut historical = world.clone();
    historical.revision = Some("historical".into());
    assert_eq!(
        authoring::preview_record_create(&historical, &draft, &historical.fingerprint).unwrap()["canApply"],
        false
    );
    assert!(matches!(
        authoring::apply_record_create(&historical, &draft, &historical.fingerprint, hash),
        Err(Error::InvalidInput(_))
    ));
    assert!(!world.root.join(".wedl-rust").exists());
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
        authoring::apply_record_create(&world, &draft, &world.fingerprint, hash),
        Err(Error::Conflict(_))
    ));
    assert!(!world.root.join("story/characters").exists());
    drop(lock);
    assert!(authoring::apply_record_create(&world, &draft, &world.fingerprint, hash).is_ok());
}

#[test]
fn no_op_keeps_exact_source_and_stale_snapshots_are_refused() {
    let (_dir, world) = world();
    let world = create(&world);
    let record = world.resolve(HERO).unwrap();
    let patch = "kind: character\ntitle: Ana\n";
    let preview =
        authoring::preview_record_update(&world, HERO, patch, &world.fingerprint).unwrap();
    assert_eq!(preview["changed"], false);
    assert_eq!(preview["beforeSha256"], preview["afterSha256"]);
    let hash = preview["afterSha256"].as_str().unwrap();
    let saved =
        authoring::apply_record_update(&world, HERO, patch, &world.fingerprint, hash).unwrap();
    assert_eq!(saved["writesPerformed"], false);
    assert_eq!(
        fs::read(world.root.join(&record.source_path)).unwrap(),
        record.raw_bytes
    );
    let path = world.root.join("story/world.md");
    let mut external = fs::read(&path).unwrap();
    external.extend_from_slice(b"\nExternal edit\n");
    fs::write(&path, &external).unwrap();
    assert!(matches!(
        authoring::apply_record_update(&world, HERO, patch, &world.fingerprint, hash),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), external);
}
