#[path = "support/actions.rs"]
mod actions;

use actions::*;
use serde_json::{Value as Json, json};
use std::{fs, process::Command};
use wedl::{Error, action_drafts, action_store, character, compiler, source};

fn stored_hash(world: &wedl::model::World, id: &str) -> String {
    action_store::detail(world, id, None).unwrap()["sourceSha256"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn save_draft(world: &wedl::model::World, n: usize) -> Json {
    action_store::save(world, &request(world, draft(world, &id(n)), None)).unwrap()
}

#[test]
fn discovery_and_preview_are_pure_and_preview_names_exact_candidate_bytes() {
    let (_directory, world) = fixture();
    let before = source_image(&world);
    assert_eq!(action_store::list(&world).unwrap()["items"], json!([]));
    assert!(matches!(
        action_store::detail(&world, &id(1), None),
        Err(Error::NotFound(_))
    ));
    let draft = draft(&world, &id(1));
    let preview = action_store::preview(&world, &request(&world, draft.clone(), None)).unwrap();
    assert_eq!(preview["writesPerformed"], false);
    assert_eq!(preview["beforeSha256"], Json::Null);
    assert_eq!(preview["beforeSource"], Json::Null);
    assert_eq!(preview["changed"], true);
    let bytes = action_drafts::serialize(&draft).unwrap();
    assert_eq!(preview["afterSource"].as_str().unwrap().as_bytes(), bytes);
    assert_eq!(preview["afterSha256"], action_drafts::sha256(&bytes));
    assert!(
        preview["afterSource"]
            .as_str()
            .unwrap()
            .contains("9007199254740993")
    );
    assert_eq!(source_image(&world), before);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn retained_source_roundtrips_restart_and_private_edits_do_not_enter_canonical_reads() {
    let (_directory, world) = fixture();
    let before = source_image(&world);
    let author = compiler::search(&world, "Private needle", 100, None).unwrap();
    let character = character::search(&world, ACTOR, "Private needle", &horizon(), 100).unwrap();
    let index = fs::read(world.root.join(".wedl-rust/index.sqlite3")).unwrap();
    let request = request(&world, draft(&world, &id(1)), None);
    let preview = action_store::preview(&world, &request).unwrap();
    let saved = action_store::save(&world, &request).unwrap();
    assert_eq!(saved["afterSha256"], preview["afterSha256"]);
    assert_eq!(saved["writesPerformed"], true);
    let raw = fs::read(path(&world, &id(1))).unwrap();
    assert_eq!(raw, preview["afterSource"].as_str().unwrap().as_bytes());
    let restarted = source::load_world(&world.root, None).unwrap();
    let detail = action_store::detail(
        &restarted,
        &id(1),
        Some(saved["afterSha256"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(detail["source"].as_str().unwrap().as_bytes(), raw);
    assert_eq!(
        detail["draft"],
        serde_json::to_value(&request.draft).unwrap()
    );
    assert_eq!(detail["draft"]["horizon"]["tick"], "9007199254740993");
    assert_eq!(
        detail["draft"]["bodyMarkdown"],
        "\r\nPrivate needle 🌲\r\n\nTrailing space  "
    );
    assert!(
        !detail["draft"]["questions"][0]
            .as_object()
            .unwrap()
            .contains_key("answer")
    );
    assert_eq!(restarted.fingerprint, world.fingerprint);
    assert_eq!(restarted.records.len(), world.records.len());
    assert_eq!(source_image(&world), before);
    assert_eq!(
        compiler::search(&restarted, "Private needle", 100, None).unwrap(),
        author
    );
    assert_eq!(
        character::search(&restarted, ACTOR, "Private needle", &horizon(), 100).unwrap(),
        character
    );
    assert_eq!(
        fs::read(world.root.join(".wedl-rust/index.sqlite3")).unwrap(),
        index
    );
}

#[test]
fn separate_drafts_are_stably_listed_and_only_the_selected_exact_bytes_change() {
    let (_directory, world) = fixture();
    save_draft(&world, 2);
    save_draft(&world, 1);
    let other = fs::read(path(&world, &id(2))).unwrap();
    let hash = stored_hash(&world, &id(1));
    let mut edited = draft(&world, &id(1));
    edited.questions[0].answer = Some(String::new());
    edited.body_markdown = "Explicitly retained new text.\r\n".into();
    let saved = action_store::save(&world, &request(&world, edited.clone(), Some(&hash))).unwrap();
    assert_ne!(saved["afterSha256"], hash);
    assert_eq!(saved["beforeSha256"], hash);
    assert_eq!(fs::read(path(&world, &id(2))).unwrap(), other);
    let items = action_store::list(&world).unwrap()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        items
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![id(1), id(2)]
    );
    let detail = action_store::detail(&world, &id(1), None).unwrap();
    assert_eq!(detail["draft"]["questions"][0]["answer"], "");
    assert_eq!(detail["draft"], serde_json::to_value(edited).unwrap());
}

#[test]
fn create_update_and_pinned_detail_refuse_stale_or_missing_hashes_without_changes() {
    let (_directory, world) = fixture();
    let saved = save_draft(&world, 1);
    let raw = fs::read(path(&world, &id(1))).unwrap();
    let hash = saved["afterSha256"].as_str().unwrap();
    for request in [
        request(&world, draft(&world, &id(1)), None),
        request(&world, draft(&world, &id(1)), Some(&"0".repeat(64))),
        request(&world, draft(&world, &id(2)), Some(hash)),
    ] {
        assert!(matches!(
            action_store::preview(&world, &request),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            action_store::save(&world, &request),
            Err(Error::Conflict(_))
        ));
    }
    assert!(matches!(
        action_store::detail(&world, &id(1), Some(&"0".repeat(64))),
        Err(Error::Conflict(_))
    ));
    assert!(action_store::detail(&world, &id(1), Some("0")).is_err());
    assert_eq!(fs::read(path(&world, &id(1))).unwrap(), raw);
    assert!(!path(&world, &id(2)).exists());
    let unchanged =
        action_store::save(&world, &request(&world, draft(&world, &id(1)), Some(hash))).unwrap();
    assert_eq!(unchanged["writesPerformed"], false);
    assert_eq!(unchanged["changed"], false);
}

#[test]
fn stale_story_snapshots_refuse_but_current_reads_retain_original_provenance() {
    let (_directory, world) = fixture();
    save_draft(&world, 1);
    let raw = fs::read(path(&world, &id(1))).unwrap();
    let hash = stored_hash(&world, &id(1));
    let fresh = change_source(&world);
    let mut edited = draft(&world, &id(1));
    edited.body_markdown = "Retained unfinished work despite source drift.".into();
    let stale = request(&world, edited.clone(), Some(&hash));
    assert!(matches!(
        action_store::list(&world),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        action_store::detail(&world, &id(1), None),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        action_store::preview(&world, &stale),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        action_store::save(&world, &stale),
        Err(Error::Conflict(_))
    ));
    assert_eq!(fs::read(path(&world, &id(1))).unwrap(), raw);
    let detail = action_store::detail(&fresh, &id(1), Some(&hash)).unwrap();
    assert_eq!(detail["sourceChanged"], true);
    assert_eq!(detail["baseFingerprint"], world.fingerprint);
    let saved = action_store::save(&fresh, &request(&fresh, edited, Some(&hash))).unwrap();
    assert_eq!(saved["sourceChanged"], true);
    assert_eq!(saved["baseFingerprint"], world.fingerprint);
    assert_eq!(
        source::load_world(&fresh.root, None).unwrap().fingerprint,
        fresh.fingerprint
    );
}

#[test]
fn dangling_stored_pointers_remain_inspectable_and_save_revalidates_current_references() {
    let (_directory, world) = fixture();
    save_draft(&world, 1);
    fs::remove_file(world.root.join(&world.records[PLACE].source_path)).unwrap();
    // Remove the scene too so the new canonical source stays valid.
    fs::remove_file(world.root.join(&world.records[SCENE].source_path)).unwrap();
    let fresh = source::load_world(&world.root, None).unwrap();
    assert!(wedl::validation::validate_world(&fresh).valid);
    let detail = action_store::detail(&fresh, &id(1), None).unwrap();
    assert_eq!(
        action_store::list(&fresh).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let retained = action_drafts::decode(detail["source"].as_str().unwrap()).unwrap();
    let hash = detail["sourceSha256"].as_str().unwrap();
    assert!(action_store::save(&fresh, &request(&fresh, retained, Some(hash))).is_err());
    assert_eq!(
        action_store::detail(&fresh, &id(1), Some(hash)).unwrap(),
        detail
    );
}

#[test]
fn cancelled_drafts_are_retained_and_terminal_including_identical_saves() {
    let (_directory, world) = fixture();
    save_draft(&world, 1);
    let hash = stored_hash(&world, &id(1));
    let mut cancelled = draft(&world, &id(1));
    cancelled.phase = "cancelled".into();
    action_store::save(&world, &request(&world, cancelled.clone(), Some(&hash))).unwrap();
    let detail = action_store::detail(&world, &id(1), None).unwrap();
    assert_eq!(detail["phase"], "cancelled");
    let hash = detail["sourceSha256"].as_str().unwrap();
    assert!(action_store::save(&world, &request(&world, cancelled, Some(hash))).is_err());
    assert!(
        action_store::save(&world, &request(&world, draft(&world, &id(1)), Some(hash))).is_err()
    );
    assert_eq!(
        action_store::detail(&world, &id(1), Some(hash)).unwrap(),
        detail
    );
}

#[test]
fn historical_worlds_and_forged_captured_models_never_create_private_state() {
    let (_directory, world) = fixture();
    let mut historical = world.clone();
    historical.revision = Some("a".repeat(40));
    let request = request(&world, draft(&world, &id(1)), None);
    for result in [
        action_store::list(&historical),
        action_store::detail(&historical, &id(1), None),
        action_store::preview(&historical, &request),
        action_store::save(&historical, &request),
    ] {
        assert!(result.is_err());
    }
    let mut forged = world.clone();
    forged
        .records
        .get_mut(ACTOR)
        .unwrap()
        .body
        .push_str("Forged projection");
    assert!(matches!(
        action_store::save(&forged, &request),
        Err(Error::Validation(_))
    ));
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn action_writes_share_the_source_writer_lock_and_do_not_advance_git() {
    let (_directory, world) = fixture();
    let git = |arguments: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&world.root)
            .args(arguments)
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
    git(&["add", "story"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "source",
    ]);
    let head = git(&["rev-parse", "HEAD"]);
    let index = fs::read(world.root.join(".git/index")).unwrap();
    fs::create_dir(world.root.join(".wedl-rust")).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(world.root.join(".wedl-rust/write.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let request = request(&world, draft(&world, &id(1)), None);
    assert!(matches!(
        action_store::save(&world, &request),
        Err(Error::Conflict(_))
    ));
    assert!(!world.root.join("author").exists());
    drop(lock);
    action_store::save(&world, &request).unwrap();
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(world.root.join(".git/index")).unwrap(), index);
    assert!(!world.root.join(".wedl-rust/index.sqlite3").exists());
}

#[test]
fn imported_invalid_document_or_path_identity_cannot_be_repaired_by_ordinary_save() {
    let (_directory, world) = fixture();
    save_draft(&world, 1);
    let valid = serde_json::to_value(draft(&world, &id(1))).unwrap();
    for (field, value) in [
        ("phase", json!("approved")),
        ("baseFingerprint", json!("not-a-hash")),
        ("id", json!(id(2))),
        ("worldId", json!("world_00000000000000000000000000")),
    ] {
        let mut imported = valid.clone();
        imported[field] = value;
        let raw = serde_json::to_vec(&imported).unwrap();
        fs::write(path(&world, &id(1)), &raw).unwrap();
        assert!(action_store::list(&world).is_err());
        assert!(action_store::detail(&world, &id(1), None).is_err());
        let hash = action_drafts::sha256(&raw);
        assert!(
            action_store::save(&world, &request(&world, draft(&world, &id(1)), Some(&hash)))
                .is_err()
        );
        assert_eq!(fs::read(path(&world, &id(1))).unwrap(), raw);
    }
}

#[test]
fn store_counts_are_complete_and_never_truncated() {
    let (_directory, world) = fixture();
    fs::create_dir_all(world.root.join("author/actions")).unwrap();
    for n in 0..1024 {
        fs::write(
            path(&world, &id(n)),
            action_drafts::serialize(&draft(&world, &id(n))).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        action_store::list(&world).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1024
    );
    assert!(action_store::save(&world, &request(&world, draft(&world, &id(1024)), None)).is_err());
    assert!(!path(&world, &id(1024)).exists());
    fs::write(path(&world, &id(1024)), b"{}").unwrap();
    assert!(action_store::list(&world).is_err());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn complete_stored_bytes_are_bounded_and_replacement_releases_the_old_charge() {
    let (_directory, world) = fixture();
    fs::create_dir_all(world.root.join("author/actions")).unwrap();
    // Outer JSON whitespace remains original retained source, so it is charged
    // even though the decoded private envelope is small.
    for n in 0..64 {
        let mut bytes = action_drafts::serialize(&draft(&world, &id(n))).unwrap();
        bytes.resize(action_drafts::MAX_DOCUMENT, b' ');
        fs::write(path(&world, &id(n)), bytes).unwrap();
    }
    assert_eq!(
        action_store::list(&world).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        64
    );
    assert!(action_store::save(&world, &request(&world, draft(&world, &id(64)), None)).is_err());
    assert!(!path(&world, &id(64)).exists());
    let hash = stored_hash(&world, &id(0));
    let result =
        action_store::save(&world, &request(&world, draft(&world, &id(0)), Some(&hash))).unwrap();
    assert_eq!(result["changed"], true);
    assert!(fs::metadata(path(&world, &id(0))).unwrap().len() < 4096);
    save_draft(&world, 64);
    let oversized = fs::OpenOptions::new()
        .write(true)
        .open(path(&world, &id(64)))
        .unwrap();
    oversized
        .set_len(action_drafts::MAX_DOCUMENT as u64 + 1)
        .unwrap();
    assert!(action_store::list(&world).is_err());
    assert!(action_store::detail(&world, &id(64), None).is_err());
}

#[test]
fn only_bounded_regular_publication_temporary_files_are_ignored_without_cleanup() {
    let (_directory, world) = fixture();
    save_draft(&world, 1);
    let directory = world.root.join("author/actions");
    let temporary = directory.join(".wedl-action-interrupted");
    fs::write(&temporary, b"Unfinished private bytes").unwrap();
    assert_eq!(
        action_store::list(&world).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(fs::read(&temporary).unwrap(), b"Unfinished private bytes");
    let file = fs::OpenOptions::new().write(true).open(&temporary).unwrap();
    file.set_len(action_drafts::MAX_DOCUMENT as u64 + 1)
        .unwrap();
    assert!(action_store::list(&world).is_err());
    fs::remove_file(&temporary).unwrap();
    fs::write(directory.join("unexpected.json"), b"{}").unwrap();
    assert!(action_store::list(&world).is_err());
}

#[cfg(unix)]
#[test]
fn parents_targets_locks_and_temporary_files_never_follow_symlinks() {
    use std::os::unix::fs::symlink;
    for location in [
        "author",
        "author/actions",
        "target",
        "temporary",
        "cache",
        "lock",
    ] {
        let (_directory, world) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, b"unchanged").unwrap();
        let link = match location {
            "author" => world.root.join("author"),
            "author/actions" => {
                fs::create_dir(world.root.join("author")).unwrap();
                world.root.join("author/actions")
            }
            "target" | "temporary" => {
                fs::create_dir_all(world.root.join("author/actions")).unwrap();
                if location == "target" {
                    path(&world, &id(1))
                } else {
                    world.root.join("author/actions/.wedl-action-link")
                }
            }
            "cache" => world.root.join(".wedl-rust"),
            _ => {
                fs::create_dir(world.root.join(".wedl-rust")).unwrap();
                world.root.join(".wedl-rust/write.lock")
            }
        };
        symlink(
            if ["target", "temporary", "lock"].contains(&location) {
                &sentinel
            } else {
                outside.path()
            },
            link,
        )
        .unwrap();
        assert!(
            action_store::save(&world, &request(&world, draft(&world, &id(1)), None)).is_err(),
            "{location}"
        );
        assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    }
}

#[cfg(unix)]
#[test]
fn private_files_start_restricted_and_updates_preserve_existing_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, world) = fixture();
    save_draft(&world, 1);
    let path = path(&world, &id(1));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let hash = stored_hash(&world, &id(1));
    let mut changed = draft(&world, &id(1));
    changed.title = "Changed private title".into();
    action_store::save(&world, &request(&world, changed, Some(&hash))).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o440)).unwrap();
    let hash = stored_hash(&world, &id(1));
    assert!(
        action_store::save(&world, &request(&world, draft(&world, &id(1)), Some(&hash))).is_err()
    );
    assert_eq!(stored_hash(&world, &id(1)), hash);
}
