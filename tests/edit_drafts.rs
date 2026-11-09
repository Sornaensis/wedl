#[path = "support/actions.rs"]
mod fixture;

use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use wedl::{action_drafts, edit_drafts as edit, model::World, source, validation};

const EDIT: &str = "edit_00000000000000000000000001";
const BODY: &str = "\r\n# Retained 🌲\r\n\nPrivate body with trailing space  ";

fn draft(world: &World, patch: &str) -> edit::Draft {
    let target = &world.records[fixture::ACTOR];
    edit::decode(&json!({"id":EDIT,"title":"Unfinished private edit","worldId":world.world_record().unwrap().id,
        "targetId":target.id,"baseFingerprint":world.fingerprint,"baseSourceSha256":action_drafts::sha256(&target.raw_bytes),
        "baseSourcePath":target.source_path,"patch":patch,"bodyMode":"preserve","bodyMarkdown":BODY,"phase":"draft"}).to_string()).unwrap()
}
fn request(world: &World, draft: edit::Draft, prior: Option<&str>) -> edit::SaveRequest {
    edit::decode_save(
        &json!({"expectedFingerprint":world.fingerprint,"expectedSha256":prior,"draft":draft})
            .to_string(),
    )
    .unwrap()
}
fn files(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    fs::metadata(entry.path()).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn unfinished_patch_and_exact_private_prose_survive_save_restart_and_export_without_source_writes()
{
    let (_directory, world) = fixture::fixture();
    let raw = "kind: character\ninitial_state: [ unfinished\nbody: not authoritative\n";
    let retained = draft(&world, raw);
    let input = request(&world, retained.clone(), None);
    let baseline = files(&world.root);
    let preview = edit::preview(&world, &input).unwrap();
    assert_eq!(preview["canSave"], true);
    assert_eq!(preview["writesPerformed"], false);
    assert_eq!(files(&world.root), baseline);
    let saved = edit::save(&world, &input).unwrap();
    assert_eq!(saved["writesPerformed"], true);
    let restarted = source::load_world(&world.root, None).unwrap();
    assert_eq!(restarted.fingerprint, world.fingerprint);
    assert_eq!(
        fixture::source_image(&world),
        fixture::source_image(&restarted)
    );
    let after = files(&world.root);
    let detail = edit::detail(&restarted, EDIT, saved["sourceSha256"].as_str()).unwrap();
    assert_eq!(detail["draft"], json!(retained));
    assert_eq!(detail["draft"]["patch"], raw);
    assert_eq!(detail["draft"]["bodyMarkdown"], BODY);
    assert_eq!(
        edit::decode(detail["source"].as_str().unwrap()).unwrap(),
        retained
    );
    let listed = edit::list(&restarted).unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        listed["items"][0]["currentness"],
        json!({"state":"current","reason":"unchanged"})
    );
    assert_eq!(files(&world.root), after);
    assert!(!restarted.records.contains_key(EDIT));
}

#[test]
fn exact_large_numeric_tokens_and_unknown_fields_remain_opaque_until_review() {
    let (_directory, world) = fixture::fixture();
    let raw =
        "kind: character\nfuture_field: {integer: 9007199254740993, floating: 1.0, text: '1.0'}\n";
    let saved = edit::save(&world, &request(&world, draft(&world, raw), None)).unwrap();
    let detail = edit::detail(&world, EDIT, None).unwrap();
    assert_eq!(detail["draft"]["patch"], raw);
    let review = edit::decode_review(
        &json!({"expectedFingerprint":world.fingerprint,"editId":EDIT,
        "expectedSha256":saved["sourceSha256"],"maxCharacters":100000})
        .to_string(),
    )
    .unwrap();
    let before = files(&world.root);
    let result = edit::review(&world, &review).unwrap();
    assert_eq!(result["canApply"], false);
    assert_eq!(result["validation"]["valid"], false);
    assert_eq!(result["draft"]["patch"], raw);
    assert_eq!(files(&world.root), before);
}

#[test]
fn stale_private_saves_retain_their_base_until_explicit_current_reconciliation() {
    let (_directory, world) = fixture::fixture();
    let mut retained = draft(&world, "kind: character\ntitle: Unfinished\n");
    let saved = edit::save(&world, &request(&world, retained.clone(), None)).unwrap();
    let changed = fixture::change_source(&world);
    assert_eq!(
        edit::detail(&changed, EDIT, None).unwrap()["currentness"]["reason"],
        "canonical-changed"
    );
    retained
        .patch
        .push_str("# Keep original context until compared\n");
    let kept = edit::save(
        &changed,
        &request(&changed, retained.clone(), saved["sourceSha256"].as_str()),
    )
    .unwrap();
    assert_eq!(
        edit::detail(&changed, EDIT, None).unwrap()["draft"]["baseFingerprint"],
        world.fingerprint
    );
    let actor = &changed.records[fixture::ACTOR];
    fs::write(
        changed.root.join(&actor.source_path),
        [
            actor.raw_bytes.clone(),
            b"Later explicit actor body.\n".to_vec(),
        ]
        .concat(),
    )
    .unwrap();
    let latest = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        edit::detail(&latest, EDIT, None).unwrap()["currentness"]["reason"],
        "target-changed"
    );
    let unchanged = files(&world.root);
    let mut partial = retained.clone();
    partial.base_fingerprint = latest.fingerprint.clone();
    assert!(
        edit::save(
            &latest,
            &request(&latest, partial, kept["sourceSha256"].as_str())
        )
        .is_err()
    );
    assert_eq!(files(&world.root), unchanged);
    retained.base_fingerprint = latest.fingerprint.clone();
    retained.base_source_sha256 = action_drafts::sha256(&latest.records[fixture::ACTOR].raw_bytes);
    retained.base_source_path = latest.records[fixture::ACTOR].source_path.clone();
    let original_patch = retained.patch.clone();
    edit::save(
        &latest,
        &request(&latest, retained, kept["sourceSha256"].as_str()),
    )
    .unwrap();
    let detail = edit::detail(&latest, EDIT, None).unwrap();
    assert_eq!(detail["currentness"]["state"], "current");
    assert_eq!(detail["draft"]["patch"], original_patch);
    assert_eq!(detail["draft"]["bodyMarkdown"], BODY);
}

#[test]
fn identities_exact_prior_bytes_and_current_source_pins_guard_private_saves() {
    let (_directory, world) = fixture::fixture();
    let retained = draft(&world, "");
    let saved = edit::save(&world, &request(&world, retained.clone(), None)).unwrap();
    let before = files(&world.root);
    for field in ["targetId", "worldId", "baseSourceSha256", "baseSourcePath"] {
        let mut changed = json!(retained);
        changed[field] = match field {
            "targetId" => json!(fixture::PLACE),
            "worldId" => json!("world_00000000000000000000000002"),
            "baseSourceSha256" => json!("0".repeat(64)),
            _ => json!("story/moved.md"),
        };
        let input=edit::decode_save(&json!({"expectedFingerprint":world.fingerprint,"expectedSha256":saved["sourceSha256"],"draft":changed}).to_string()).unwrap();
        assert!(edit::save(&world, &input).is_err(), "{field}");
        assert_eq!(files(&world.root), before);
    }
    assert!(edit::save(&world, &request(&world, retained.clone(), None)).is_err());
    let mut stale = request(&world, retained.clone(), saved["sourceSha256"].as_str());
    stale.expected_fingerprint = "0".repeat(64);
    assert!(edit::save(&world, &stale).is_err());
    assert!(edit::detail(&world, EDIT, Some(&"f".repeat(64))).is_err());
    assert_eq!(files(&world.root), before);
    let path = world.root.join(format!("author/edits/{EDIT}.json"));
    fs::write(&path, serde_json::to_string_pretty(&retained).unwrap()).unwrap();
    let external = files(&world.root);
    assert!(
        edit::save(
            &world,
            &request(&world, retained, saved["sourceSha256"].as_str())
        )
        .is_err()
    );
    assert_eq!(files(&world.root), external);
}

#[test]
fn cancellation_is_terminal_and_ordinary_save_never_creates_an_application() {
    let (_directory, world) = fixture::fixture();
    let mut retained = draft(&world, "broken unfinished [");
    let saved = edit::save(&world, &request(&world, retained.clone(), None)).unwrap();
    retained.phase = "cancelled".into();
    let cancelled = edit::save(
        &world,
        &request(&world, retained.clone(), saved["sourceSha256"].as_str()),
    )
    .unwrap();
    let before = files(&world.root);
    assert!(
        edit::save(
            &world,
            &request(&world, retained.clone(), cancelled["sourceSha256"].as_str())
        )
        .is_err()
    );
    retained.phase = "draft".into();
    assert!(
        edit::save(
            &world,
            &request(&world, retained.clone(), cancelled["sourceSha256"].as_str())
        )
        .is_err()
    );
    retained.phase = "applied".into();
    assert!(edit::decode(&json!(retained).to_string()).is_err());
    assert_eq!(files(&world.root), before);
    let mut new = draft(&world, "");
    new.id = "edit_00000000000000000000000002".into();
    new.phase = "cancelled".into();
    assert!(edit::save(&world, &request(&world, new, None)).is_ok());
}

#[test]
fn closed_codecs_and_complete_byte_bounds_refuse_without_private_creation() {
    let (_directory, world) = fixture::fixture();
    let retained = json!(draft(&world, "kind: character\n"));
    assert!(
        edit::decode_save(
            &json!({
                "expectedFingerprint": world.fingerprint,
                "draft": retained,
            })
            .to_string()
        )
        .is_err()
    );
    assert!(
        edit::decode_save(
            &json!({
                "expectedFingerprint": world.fingerprint,
                "expectedSha256": null,
                "draft": retained,
            })
            .to_string()
        )
        .is_ok()
    );
    for value in [
        json!({"extra":true}),
        json!({"appliedReceipt":null}),
        json!({"bodyMode":"merge"}),
        json!({"phase":"approved"}),
        json!({"title":" "}),
        json!({"baseSourcePath":"story/../secret.md"}),
    ] {
        let mut changed = retained.clone();
        changed
            .as_object_mut()
            .unwrap()
            .extend(value.as_object().unwrap().clone());
        assert!(edit::decode(&changed.to_string()).is_err());
    }
    let encoded = retained.to_string();
    assert!(
        edit::decode(&encoded.replacen("{", "{\"id\":\"edit_00000000000000000000000001\",", 1))
            .is_err()
    );
    let mut huge = draft(&world, "");
    huge.patch = "é".repeat(65_537);
    assert!(edit::serialize(&huge).is_err());
    huge.patch = "a".repeat(131_072);
    huge.body_markdown = "b".repeat(131_072);
    assert!(edit::serialize(&huge).is_err());
    let baseline = files(&world.root);
    assert!(
        edit::preview(
            &world,
            &edit::SaveRequest {
                expected_fingerprint: world.fingerprint.clone(),
                expected_sha256: None,
                draft: huge
            }
        )
        .is_err()
    );
    assert_eq!(files(&world.root), baseline);
    assert!(!world.root.join("author").exists());
}

#[test]
fn complete_store_bounds_reject_excess_documents_and_oversized_temporaries() {
    let (_directory, world) = fixture::fixture();
    let directory = world.root.join("author/edits");
    fs::create_dir_all(&directory).unwrap();
    let mut value = draft(&world, "");
    for number in 0..1025 {
        value.id = format!("edit_{number:026}");
        fs::write(
            directory.join(format!("{}.json", value.id)),
            edit::serialize(&value).unwrap(),
        )
        .unwrap();
    }
    assert!(edit::list(&world).is_err());
    for entry in fs::read_dir(&directory).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::write(
        directory.join(".wedl-edit-leftover"),
        vec![b'x'; edit::MAX_DOCUMENT + 1],
    )
    .unwrap();
    assert!(edit::list(&world).is_err());
    assert!(edit::preview(&world, &request(&world, draft(&world, ""), None)).is_err());
}

#[test]
fn historical_edit_bytes_survive_dirty_live_buffers_and_compare_live_context_separately() {
    let (_directory, world) = fixture::fixture();
    edit::save(
        &world,
        &request(&world, draft(&world, "unfinished: ["), None),
    )
    .unwrap();
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["config", "user.name", "Edit test"]);
    git(
        &world.root,
        &["config", "user.email", "edit@example.invalid"],
    );
    git(&world.root, &["add", "--", "story", "author"]);
    git(&world.root, &["commit", "-qm", "Retained edit"]);
    let revision = git(&world.root, &["rev-parse", "HEAD"]);
    let historical = source::load_world(&world.root, Some(&revision)).unwrap();
    let original = edit::detail(&historical, EDIT, None).unwrap();
    assert_eq!(original["revision"], revision);
    assert!(
        original["blobOid"]
            .as_str()
            .is_some_and(|value| value.len() == 40)
    );
    assert_eq!(original["currentness"]["state"], "current");
    fs::write(
        world.root.join(format!("author/edits/{EDIT}.json")),
        "not a private document",
    )
    .unwrap();
    let changed = fixture::change_source(&world);
    let baseline = files(&world.root);
    let detail = edit::detail(&historical, EDIT, None).unwrap();
    assert_eq!(detail["source"], original["source"]);
    assert_eq!(detail["blobOid"], original["blobOid"]);
    assert_eq!(detail["comparedFingerprint"], changed.fingerprint);
    assert_eq!(detail["currentness"]["reason"], "canonical-changed");
    assert_eq!(
        edit::list(&historical).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(files(&world.root), baseline);
    fs::write(world.root.join("story/world.md"), "invalid current source").unwrap();
    let baseline = files(&world.root);
    let unavailable = edit::detail(&historical, EDIT, None).unwrap();
    assert_eq!(unavailable["source"], original["source"]);
    assert_eq!(
        unavailable["currentness"],
        json!({"state":"unavailable","reason":"unavailable"})
    );
    assert!(unavailable["comparedFingerprint"].is_null());
    assert_eq!(files(&world.root), baseline);
}

#[test]
fn historical_edit_writes_refuse_and_current_reads_detect_crafted_source_fields() {
    let (_directory, world) = fixture::fixture();
    let retained = draft(&world, "kind: character\n");
    edit::save(&world, &request(&world, retained.clone(), None)).unwrap();
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["config", "user.name", "Edit test"]);
    git(
        &world.root,
        &["config", "user.email", "edit@example.invalid"],
    );
    git(&world.root, &["add", "--", "story", "author"]);
    git(&world.root, &["commit", "-qm", "Retained edit"]);
    let historical =
        source::load_world(&world.root, Some(&git(&world.root, &["rev-parse", "HEAD"]))).unwrap();
    let detail = edit::detail(&historical, EDIT, None).unwrap();
    let before = files(&world.root);
    assert!(
        edit::save(
            &historical,
            &request(&historical, retained, detail["sourceSha256"].as_str())
        )
        .is_err()
    );
    assert_eq!(files(&world.root), before);
    let mut crafted = world.clone();
    crafted.records.get_mut(fixture::PLACE).unwrap().title = "Fabricated in-memory title".into();
    assert!(edit::list(&crafted).is_err());
    assert_eq!(files(&world.root), before);
    assert!(validation::validate_world(&world).valid);
}

#[cfg(unix)]
#[test]
fn private_symlinks_and_read_only_targets_are_not_overwritten() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let (_directory, world) = fixture::fixture();
    let retained = draft(&world, "");
    let outside = world.root.parent().unwrap().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, world.root.join("author")).unwrap();
    assert!(edit::save(&world, &request(&world, retained.clone(), None)).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::remove_file(world.root.join("author")).unwrap();
    let saved = edit::save(&world, &request(&world, retained.clone(), None)).unwrap();
    let path = world.root.join(format!("author/edits/{EDIT}.json"));
    let original = fs::read(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let mut changed = retained;
    changed.title = "Edited private title".into();
    assert!(
        edit::save(
            &world,
            &request(&world, changed, saved["sourceSha256"].as_str())
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o400
    );
}
