use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use wedl::{action_drafts, action_outcomes, action_store, authoring, publication, source};

#[path = "support/actions.rs"]
mod support;

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn pending(world: &wedl::model::World, after_body: &str) -> (Json, Vec<u8>, Vec<u8>, String) {
    pending_with_create(world, after_body, false)
}

fn pending_with_create(
    world: &wedl::model::World,
    after_body: &str,
    create: bool,
) -> (Json, Vec<u8>, Vec<u8>, String) {
    let draft = support::draft(world, &support::id(1));
    action_store::save(world, &support::request(world, draft.clone(), None)).unwrap();
    let private = format!("author/actions/{}.json", draft.id);
    let mut value = serde_json::to_value(draft).unwrap();
    value["phase"] = "proposed".into();
    value["questions"][0]["answer"] = "Reviewed fixture".into();
    value["proposals"] = json!([{
        "id":"proposal_00000000000000000000000001","summary":"Exact reviewed change",
        "contextFingerprint":world.fingerprint,"evidence":[],"resolver":"human",
        "questionAnswers":[{"questionId":"question_00000000000000000000000001","answer":"Reviewed fixture"}],
        "consequences":{"expectedFingerprint":world.fingerprint,"operations":[{
            "type":"record.update","entityId":support::ACTOR,
            "patch":serde_yaml::to_string(&json!({"kind":"character","body":after_body})).unwrap()
        }]}
    }]);
    if create {
        value["proposals"][0]["consequences"]["operations"].as_array_mut().unwrap().push(json!({
            "type":"record.create","draft":"kind: object\nid: obj_00000000000000000000000051\ntitle: Newly added object\n"
        }));
    }
    let proposed = action_drafts::decode(&value.to_string()).unwrap();
    let private_before = action_drafts::serialize(&proposed).unwrap();
    fs::write(world.root.join(&private), &private_before).unwrap();
    let request = action_outcomes::decode_review(&json!({
        "expectedFingerprint":world.fingerprint,"actionId":proposed.id,
        "expectedSha256":hash(&private_before),"proposalId":"proposal_00000000000000000000000001",
        "maxCharacters":100000
    }).to_string()).unwrap();
    let preview = action_outcomes::review(world, &request).unwrap();
    let apply = action_outcomes::decode_apply(
        &json!({
            "expectedFingerprint":world.fingerprint,"actionId":proposed.id,
            "expectedSha256":hash(&private_before),"proposalId":request.proposal_id,
            "maxCharacters":100000,"reviewSha256":preview["reviewSha256"],"approved":true
        })
        .to_string(),
    )
    .unwrap();
    action_outcomes::apply(world, &apply).unwrap();
    let private_after = fs::read(world.root.join(&private)).unwrap();
    let file = &preview["consequences"]["files"][0];
    let before = file["beforeSource"].as_str().unwrap().as_bytes().to_vec();
    let after = file["afterSource"].as_str().unwrap().as_bytes().to_vec();
    // Simulate a process interruption by reinstating an exact valid journal and
    // original bytes. Individual tests publish any desired subset of its files.
    let mut changes = Vec::new();
    for file in preview["consequences"]["files"].as_array().unwrap() {
        let path = world.root.join(file["sourcePath"].as_str().unwrap());
        if let Some(before) = file["beforeSource"].as_str() {
            fs::write(&path, before).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        changes.push(json!({"path":file["sourcePath"],"beforeSource":file["beforeSource"],
            "afterSource":file["afterSource"],"beforeSha256":file["beforeSha256"],"afterSha256":file["afterSha256"]}));
    }
    fs::write(world.root.join(&private), &private_before).unwrap();
    changes.push(json!({"path":private,"beforeSource":std::str::from_utf8(&private_before).unwrap(),
             "afterSource":std::str::from_utf8(&private_after).unwrap(),"beforeSha256":hash(&private_before),"afterSha256":hash(&private_after)}));
    let journal = json!({"protocol":wedl::CONTRACT,"baseFingerprint":world.fingerprint,
        "candidateFingerprint":preview["consequences"]["candidateFingerprint"],"changes":changes});
    let bytes = serde_json::to_vec(&journal).unwrap();
    fs::write(world.root.join(".wedl-rust/pending-outcome.json"), &bytes).unwrap();
    (journal, before, after, hash(&bytes))
}

fn journal_path(root: &Path) -> std::path::PathBuf {
    root.join(".wedl-rust/pending-outcome.json")
}

#[test]
fn status_with_no_publication_does_not_create_files() {
    let (_temp, world) = support::fixture();
    assert!(!world.root.join(".wedl-rust").exists());
    let report = publication::status(&world.root).unwrap();
    assert_eq!(report["pending"], false);
    assert_eq!(report["writesPerformed"], false);
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn pending_journal_fences_current_source_and_existing_writers() {
    let (_temp, world) = support::fixture();
    let (_journal, _before, _after, journal_hash) = pending(&world, "After 🌲\r\n");
    assert!(
        source::load_world(&world.root, None)
            .unwrap_err()
            .to_string()
            .contains("publication is pending")
    );
    assert!(action_store::list(&world).is_err());
    let preview =
        authoring::preview_body_edit(&world, support::ACTOR, "New", &world.fingerprint).unwrap();
    assert!(
        authoring::apply_body_edit(
            &world,
            support::ACTOR,
            "New",
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        )
        .is_err()
    );
    let report = publication::status(&world.root).unwrap();
    assert_eq!(report["journalSha256"], journal_hash);
    assert_eq!(report["recovery"], "rollback");
}

#[test]
fn interrupted_partial_publication_rolls_back_exact_source_and_private_bytes() {
    let (_temp, world) = support::fixture();
    let (journal, before, after, journal_hash) = pending(&world, "After 🌲\r\nno final newline");
    let path = journal["changes"][0]["path"].as_str().unwrap();
    fs::write(world.root.join(path), &after).unwrap();
    let private = journal["changes"][1]["path"].as_str().unwrap();
    fs::write(
        world.root.join(private),
        journal["changes"][1]["afterSource"].as_str().unwrap(),
    )
    .unwrap();
    let report = publication::recover(&world.root, &journal_hash).unwrap();
    assert_eq!(report["disposition"], "rolled-back");
    assert_eq!(fs::read(world.root.join(path)).unwrap(), before);
    assert_eq!(
        fs::read_to_string(world.root.join(private)).unwrap(),
        journal["changes"][1]["beforeSource"].as_str().unwrap()
    );
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    assert!(!journal_path(&world.root).exists());
    assert_eq!(
        fs::read_to_string(world.root.join(".wedl-rust/publication-generation"))
            .unwrap()
            .len(),
        65
    );
    assert!(publication::recover(&world.root, &journal_hash).is_err());
}

#[test]
fn journal_hash_mismatch_and_unknown_target_bytes_refuse_before_any_recovery_write() {
    let (_temp, world) = support::fixture();
    let (journal, _before, after, journal_hash) = pending(&world, "Changed actor");
    let path = journal["changes"][0]["path"].as_str().unwrap();
    fs::write(world.root.join(path), &after).unwrap();
    assert!(publication::recover(&world.root, &"0".repeat(64)).is_err());
    let private = journal["changes"][1]["path"].as_str().unwrap();
    fs::write(world.root.join(private), "external private change").unwrap();
    assert!(publication::recover(&world.root, &journal_hash).is_err());
    assert_eq!(fs::read(world.root.join(path)).unwrap(), after);
    assert_eq!(
        fs::read_to_string(world.root.join(private)).unwrap(),
        "external private change"
    );
    assert!(journal_path(&world.root).exists());
}

#[test]
fn unrelated_canonical_edit_refuses_recovery_without_overwriting_it() {
    let (_temp, world) = support::fixture();
    let (journal, _before, after, journal_hash) = pending(&world, "Changed actor");
    let path = journal["changes"][0]["path"].as_str().unwrap();
    fs::write(world.root.join(path), &after).unwrap();
    let place = &world.records[support::PLACE];
    let mut place_bytes = place.raw_bytes.clone();
    place_bytes.extend_from_slice(b"External unrelated addition\n");
    fs::write(world.root.join(&place.source_path), &place_bytes).unwrap();
    assert!(publication::recover(&world.root, &journal_hash).is_err());
    assert_eq!(fs::read(world.root.join(path)).unwrap(), after);
    assert_eq!(
        fs::read(world.root.join(&place.source_path)).unwrap(),
        place_bytes
    );
    assert!(journal_path(&world.root).exists());
}

#[test]
fn interrupted_private_temporary_file_does_not_poison_discovery_after_recovery() {
    let (_temp, world) = support::fixture();
    let (_journal, _before, _after, journal_hash) = pending(&world, "After");
    fs::write(
        world.root.join("author/actions/.wedl-action-crash"),
        b"partial source",
    )
    .unwrap();
    publication::recover(&world.root, &journal_hash).unwrap();
    let report = action_store::list(&source::load_world(&world.root, None).unwrap()).unwrap();
    assert_eq!(report["items"].as_array().unwrap().len(), 1);
}

#[test]
fn malformed_journal_path_or_source_hash_never_opens_current_read_fence() {
    let (_temp, world) = support::fixture();
    let (mut journal, _before, _after, _) = pending(&world, "After");
    journal["changes"][0]["path"] = "story/../outside.md".into();
    let bytes = serde_json::to_vec(&journal).unwrap();
    fs::write(journal_path(&world.root), &bytes).unwrap();
    assert!(publication::status(&world.root).is_err());
    assert!(publication::recover(&world.root, &hash(&bytes)).is_err());
    assert!(source::load_world(&world.root, None).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_target_refuses_recovery_and_preserves_outside_file() {
    use std::os::unix::fs::symlink;
    let (temp, world) = support::fixture();
    let (journal, _before, _after, journal_hash) = pending(&world, "After");
    let target = world
        .root
        .join(journal["changes"][0]["path"].as_str().unwrap());
    let outside = temp.path().join("outside.md");
    fs::write(&outside, "outside protected").unwrap();
    fs::remove_file(&target).unwrap();
    symlink(&outside, &target).unwrap();
    assert!(publication::recover(&world.root, &journal_hash).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "outside protected");
    assert!(journal_path(&world.root).exists());
}

#[test]
fn recovery_removes_only_exact_new_source_and_preserves_unrelated_files() {
    let (_temp, world) = support::fixture();
    let (journal, _before, _after, journal_hash) = pending_with_create(&world, "After", true);
    let create = &journal["changes"][1];
    let path = world.root.join(create["path"].as_str().unwrap());
    fs::write(&path, create["afterSource"].as_str().unwrap()).unwrap();
    fs::write(world.root.join("story/note.txt"), "Unrelated text").unwrap();
    publication::recover(&world.root, &journal_hash).unwrap();
    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(world.root.join("story/note.txt")).unwrap(),
        "Unrelated text"
    );
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[test]
fn recovery_respects_the_shared_writer_lock() {
    let (_temp, world) = support::fixture();
    let (journal, before, after, journal_hash) = pending(&world, "After");
    let path = world
        .root
        .join(journal["changes"][0]["path"].as_str().unwrap());
    fs::write(&path, &after).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(world.root.join(".wedl-rust/write.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(publication::recover(&world.root, &journal_hash).is_err());
    assert_eq!(fs::read(&path).unwrap(), after);
    drop(lock);
    publication::recover(&world.root, &journal_hash).unwrap();
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn private_pair_tampering_is_not_a_valid_recovery_instruction() {
    let (_temp, world) = support::fixture();
    let (mut journal, _before, _after, _) = pending(&world, "After");
    let original = journal["changes"][1]["beforeSource"].as_str().unwrap();
    let mut draft = action_drafts::decode(original).unwrap();
    draft.body_markdown.push_str("Unreviewed modification");
    let bytes = action_drafts::serialize(&draft).unwrap();
    journal["changes"][1]["beforeSource"] = std::str::from_utf8(&bytes).unwrap().into();
    journal["changes"][1]["beforeSha256"] = hash(&bytes).into();
    fs::write(
        world
            .root
            .join(journal["changes"][1]["path"].as_str().unwrap()),
        &bytes,
    )
    .unwrap();
    let journal_bytes = serde_json::to_vec(&journal).unwrap();
    fs::write(journal_path(&world.root), &journal_bytes).unwrap();
    assert!(publication::recover(&world.root, &hash(&journal_bytes)).is_err());
    assert_eq!(
        fs::read(
            world
                .root
                .join(journal["changes"][1]["path"].as_str().unwrap())
        )
        .unwrap(),
        bytes
    );
    assert!(journal_path(&world.root).exists());
}

#[cfg(unix)]
#[test]
fn rollback_preserves_permissions_and_refuses_read_only_targets() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, world) = support::fixture();
    let (journal, _before, after, journal_hash) = pending(&world, "After");
    let path = world
        .root
        .join(journal["changes"][0]["path"].as_str().unwrap());
    fs::write(&path, &after).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(publication::recover(&world.root, &journal_hash).is_err());
    assert_eq!(fs::read(&path).unwrap(), after);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    publication::recover(&world.root, &journal_hash).unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
