#[path = "support/actions.rs"]
mod fixture;

use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, fs, path::Path};
use wedl::{action_drafts, edit_drafts as edit, model::World, publication, source, validation};

const EDIT: &str = "edit_00000000000000000000000001";
const BODY: &str = "\r\nExact retained 🌲 prose\r\n\nNo final newline  ";

fn draft(world: &World, patch: &str, mode: &str) -> edit::Draft {
    let target = &world.records[fixture::ACTOR];
    edit::decode(&json!({"id":EDIT,"title":"Retained canonical change","worldId":world.world_record().unwrap().id,
        "targetId":target.id,"baseFingerprint":world.fingerprint,"baseSourceSha256":action_drafts::sha256(&target.raw_bytes),
        "baseSourcePath":target.source_path,"patch":patch,"bodyMode":mode,"bodyMarkdown":BODY,"phase":"draft"}).to_string()).unwrap()
}
fn save(world: &World, draft: edit::Draft, prior: Option<&str>) -> Json {
    edit::save(
        world,
        &edit::SaveRequest {
            expected_fingerprint: world.fingerprint.clone(),
            expected_sha256: prior.map(str::to_owned),
            draft,
        },
    )
    .unwrap()
}
fn review(world: &World, saved: &Json) -> edit::ReviewRequest {
    edit::decode_review(
        &json!({"expectedFingerprint":world.fingerprint,"editId":EDIT,
        "expectedSha256":saved["sourceSha256"],"maxCharacters":100000})
        .to_string(),
    )
    .unwrap()
}
fn approve(request: &edit::ReviewRequest, report: &Json) -> edit::ApplyRequest {
    let mut value = json!(request);
    value["reviewSha256"] = report["reviewSha256"].clone();
    value["approved"] = true.into();
    edit::decode_apply(&value.to_string()).unwrap()
}
fn path(world: &World) -> std::path::PathBuf {
    world.root.join(format!("author/edits/{EDIT}.json"))
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

#[test]
fn complete_saved_review_and_paired_application_preserve_exact_retained_work() {
    let (_directory, world) = fixture::fixture();
    let original = draft(
        &world,
        "kind: character\ntitle: Reviewed character\n",
        "replace",
    );
    let saved = save(&world, original.clone(), None);
    let request = review(&world, &saved);
    let before = files(&world.root);
    let report = edit::review(&world, &request).unwrap();
    assert_eq!(report["canApply"], true);
    assert_eq!(report["approvalRequired"], true);
    assert_eq!(report["consequences"]["canApply"], false);
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(files(&world.root), before);
    let input = approve(&request, &report);
    let applied = edit::apply(&world, &input).unwrap();
    assert_eq!(applied["writesPerformed"], true);
    assert_eq!(applied["alreadyApplied"], false);
    assert_eq!(applied["changedFiles"], 1);
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        current.fingerprint,
        report["consequences"]["candidateFingerprint"]
    );
    assert_eq!(current.records[fixture::ACTOR].title, "Reviewed character");
    assert_eq!(current.records[fixture::ACTOR].body, BODY);
    let detail = edit::detail(&current, EDIT, None).unwrap();
    let stored = edit::decode(detail["source"].as_str().unwrap()).unwrap();
    assert_eq!(stored.phase, "applied");
    assert_eq!(
        stored.applied_receipt.as_ref().unwrap().review_sha256,
        report["reviewSha256"]
    );
    let mut retained = stored.clone();
    retained.phase = "draft".into();
    retained.applied_receipt = None;
    assert_eq!(retained, original);
    assert_eq!(detail["sourceSha256"], applied["sourceSha256"]);
    assert_eq!(publication::status(&world.root).unwrap()["pending"], false);
    assert!(validation::validate_world(&current).valid);
    let unchanged = files(&world.root);
    let replay = edit::apply(&current, &input).unwrap();
    assert_eq!(replay["alreadyApplied"], true);
    assert_eq!(replay["writesPerformed"], false);
    assert_eq!(files(&world.root), unchanged);
}

#[test]
fn preserve_body_ignores_retained_private_prose_and_patch_body_cannot_bypass_the_mode() {
    let (_directory, world) = fixture::fixture();
    let body = world.records[fixture::ACTOR].body.clone();
    let saved = save(
        &world,
        draft(
            &world,
            "kind: character\ntitle: Field-only edit\n",
            "preserve",
        ),
        None,
    );
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    edit::apply(&world, &approve(&request, &report)).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(current.records[fixture::ACTOR].body, body);
    assert_eq!(
        edit::detail(&current, EDIT, None).unwrap()["draft"]["bodyMarkdown"],
        BODY
    );
    for mode in ["preserve", "replace"] {
        let (_directory, world) = fixture::fixture();
        let saved = save(
            &world,
            draft(&world, "kind: character\nbody: Hidden patch prose\n", mode),
            None,
        );
        let request = review(&world, &saved);
        let before = files(&world.root);
        let report = edit::review(&world, &request).unwrap();
        assert_eq!(report["canApply"], false);
        assert!(report["consequences"].is_null());
        assert!(edit::apply(&world, &approve(&request, &report)).is_err());
        assert_eq!(files(&world.root), before);
    }
}

#[test]
fn invalid_raw_and_invalid_candidate_are_retained_but_never_publish() {
    for patch in [
        "kind: character\ninitial_state: [unfinished",
        "kind: character\ninitial_state: {undeclared: true}\n",
    ] {
        let (_directory, world) = fixture::fixture();
        let saved = save(&world, draft(&world, patch, "replace"), None);
        let request = review(&world, &saved);
        let before = files(&world.root);
        let report = edit::review(&world, &request).unwrap();
        assert_eq!(report["canApply"], false);
        assert_eq!(report["validation"]["valid"], false);
        assert!(edit::apply(&world, &approve(&request, &report)).is_err());
        assert_eq!(files(&world.root), before);
        assert_eq!(
            edit::detail(&world, EDIT, None).unwrap()["draft"]["patch"],
            patch
        );
    }
}

#[test]
fn yaml_alias_expansion_is_bounded_before_constructing_the_patch() {
    let (_directory, world) = fixture::fixture();
    let patch = format!(
        "kind: character\ntitle: &large {}\naliases: [*large, *large, *large, *large]\n",
        "x".repeat(65_536)
    );
    let saved = save(&world, draft(&world, &patch, "preserve"), None);
    let request = review(&world, &saved);
    let before = files(&world.root);
    let report = edit::review(&world, &request).unwrap();
    assert_eq!(report["canApply"], false);
    assert!(report["consequences"].is_null());
    assert!(
        report["validation"]["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("expanded YAML bounds")
    );
    assert_eq!(report["draft"]["patch"], patch);
    assert_eq!(files(&world.root), before);

    let ordinary = save(
        &world,
        draft(
            &world,
            "kind: character\ntitle: &label Reviewed alias\naliases: [*label]\n",
            "preserve",
        ),
        saved["sourceSha256"].as_str(),
    );
    assert_eq!(
        edit::review(&world, &review(&world, &ordinary)).unwrap()["canApply"],
        true
    );
}

#[test]
fn canonical_noop_still_terminalizes_once_and_retry_creates_no_writer_metadata() {
    let (_directory, world) = fixture::fixture();
    let saved = save(&world, draft(&world, "kind: character\n", "preserve"), None);
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    assert_eq!(report["consequences"]["canonicalNoOp"], true);
    let input = approve(&request, &report);
    let before = fixture::source_image(&world);
    let result = edit::apply(&world, &input).unwrap();
    assert_eq!(result["canonicalNoOp"], true);
    assert_eq!(result["changedFiles"], 0);
    assert_eq!(result["writesPerformed"], true);
    assert_eq!(fixture::source_image(&world), before);
    assert_eq!(result["fingerprint"], world.fingerprint);
    fs::remove_dir_all(world.root.join(".wedl-rust")).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let exact = files(&world.root);
    assert_eq!(
        edit::apply(&current, &input).unwrap()["alreadyApplied"],
        true
    );
    assert_eq!(files(&world.root), exact);
    assert!(!world.root.join(".wedl-rust").exists());
    let stored = edit::decode(
        edit::detail(&current, EDIT, None).unwrap()["source"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(
        edit::save(
            &current,
            &edit::SaveRequest {
                expected_fingerprint: current.fingerprint.clone(),
                expected_sha256: result["sourceSha256"].as_str().map(str::to_owned),
                draft: stored
            }
        )
        .is_err()
    );
}

#[test]
fn explicit_approval_full_report_budget_and_independent_bindings_cannot_be_substituted() {
    let (_directory, world) = fixture::fixture();
    let saved = save(
        &world,
        draft(&world, "kind: character\ntitle: Candidate\n", "replace"),
        None,
    );
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    let input = approve(&request, &report);
    let before = files(&world.root);
    let mut denied = input.clone();
    denied.approved = false;
    assert!(edit::apply(&world, &denied).is_err());
    let mut changed = input.clone();
    changed.max_characters += 1;
    assert!(edit::apply(&world, &changed).is_err());
    let mut changed = input.clone();
    changed.expected_fingerprint = "0".repeat(64);
    assert!(edit::apply(&world, &changed).is_err());
    let mut changed = input.clone();
    changed.expected_sha256 = "0".repeat(64);
    assert!(edit::apply(&world, &changed).is_err());
    let mut changed = input.clone();
    changed.review_sha256 = report["consequences"]["previewSha256"]
        .as_str()
        .unwrap()
        .into();
    assert!(edit::apply(&world, &changed).is_err());
    let mut limited = request.clone();
    limited.max_characters = 256;
    assert!(edit::review(&world, &limited).is_err());
    assert_eq!(files(&world.root), before);
    let mut other = request;
    other.max_characters += 1;
    let new = edit::review(&world, &other).unwrap();
    assert_ne!(new["reportSha256"], report["reportSha256"]);
    assert_ne!(new["reviewSha256"], report["reviewSha256"]);
}

#[test]
fn stale_source_or_exact_private_changes_require_a_fresh_saved_review() {
    let (_directory, world) = fixture::fixture();
    let retained = draft(&world, "kind: character\ntitle: Candidate\n", "replace");
    let saved = save(&world, retained.clone(), None);
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    let input = approve(&request, &report);
    fs::write(
        path(&world),
        serde_json::to_string_pretty(&retained).unwrap(),
    )
    .unwrap();
    let before = files(&world.root);
    assert!(edit::apply(&world, &input).is_err());
    assert_eq!(files(&world.root), before);
    let detail = edit::detail(&world, EDIT, None).unwrap();
    let fresh = edit::ReviewRequest {
        expected_sha256: detail["sourceSha256"].as_str().unwrap().into(),
        ..request
    };
    let fresh_report = edit::review(&world, &fresh).unwrap();
    assert_ne!(fresh_report["reviewSha256"], report["reviewSha256"]);
    let fresh_input = approve(&fresh, &fresh_report);
    let changed = fixture::change_source(&world);
    let before = files(&world.root);
    assert!(edit::apply(&changed, &fresh_input).is_err());
    assert!(edit::review(&changed, &fresh).is_err());
    assert_eq!(files(&world.root), before);
}

#[test]
fn receipt_rejects_rebound_fields_and_retry_refuses_source_or_terminal_byte_drift() {
    let (_directory, world) = fixture::fixture();
    let saved = save(
        &world,
        draft(&world, "kind: character\ntitle: Reviewed\n", "replace"),
        None,
    );
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    let input = approve(&request, &report);
    edit::apply(&world, &input).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let detail = edit::detail(&current, EDIT, None).unwrap();
    let original = detail["draft"].clone();
    for field in [
        "planSha256",
        "reviewSha256",
        "reportSha256",
        "requestSha256",
        "baseFingerprint",
        "candidateFingerprint",
        "beforeEditSha256",
        "beforeDraftSha256",
    ] {
        let mut changed = original.clone();
        changed["appliedReceipt"][field] = json!("f".repeat(64));
        assert!(edit::decode(&changed.to_string()).is_err(), "{field}");
    }
    let mut changed = original.clone();
    changed["bodyMarkdown"] = json!("Different retained prose");
    assert!(edit::decode(&changed.to_string()).is_err());
    let mut changed = original.clone();
    changed["appliedReceipt"]["file"]["afterSha256"] = json!("f".repeat(64));
    assert!(edit::decode(&changed.to_string()).is_err());
    fs::write(
        path(&current),
        serde_json::to_string_pretty(&original).unwrap(),
    )
    .unwrap();
    let before = files(&current.root);
    assert!(edit::apply(&current, &input).is_err());
    assert_eq!(files(&current.root), before);
    fs::write(path(&current), detail["source"].as_str().unwrap()).unwrap();
    let drift = fixture::change_source(&current);
    let before = files(&current.root);
    assert!(edit::apply(&drift, &input).is_err());
    assert_eq!(files(&current.root), before);
}

#[test]
fn raw_private_source_and_normalized_draft_hashes_remain_distinct_through_application() {
    let (_directory, world) = fixture::fixture();
    let retained = draft(&world, "kind: character\ntitle: Reviewed\n", "replace");
    save(&world, retained.clone(), None);
    fs::write(
        path(&world),
        serde_json::to_string_pretty(&retained).unwrap(),
    )
    .unwrap();
    let detail = edit::detail(&world, EDIT, None).unwrap();
    let request = review(&world, &detail);
    let report = edit::review(&world, &request).unwrap();
    let result = edit::apply(&world, &approve(&request, &report)).unwrap();
    assert_eq!(
        result["receipt"]["beforeEditSha256"],
        detail["sourceSha256"]
    );
    assert_eq!(
        result["receipt"]["beforeDraftSha256"],
        action_drafts::sha256(&edit::serialize(&retained).unwrap())
    );
    assert_ne!(
        result["receipt"]["beforeEditSha256"],
        result["receipt"]["beforeDraftSha256"]
    );
}

#[test]
fn interrupted_edit_publication_is_fenced_and_recovers_target_and_private_history_together() {
    let (_directory, world) = fixture::fixture();
    let saved = save(
        &world,
        draft(
            &world,
            "kind: character\ntitle: Recovered target\n",
            "replace",
        ),
        None,
    );
    let private_before = fs::read(path(&world)).unwrap();
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    let input = approve(&request, &report);
    let canonical_path = world.records[fixture::ACTOR].source_path.clone();
    let canonical_before = world.records[fixture::ACTOR].raw_bytes.clone();
    let applied = edit::apply(&world, &input).unwrap();
    let canonical_after = fs::read(world.root.join(&canonical_path)).unwrap();
    let private_after = fs::read(path(&world)).unwrap();
    fs::write(path(&world), &private_before).unwrap();
    let change = |path: &str, before: &[u8], after: &[u8]| json!({"path":path,"beforeSource":std::str::from_utf8(before).unwrap(),"afterSource":std::str::from_utf8(after).unwrap(),"beforeSha256":action_drafts::sha256(before),"afterSha256":action_drafts::sha256(after)});
    let journal = json!({"protocol":wedl::CONTRACT,"baseFingerprint":world.fingerprint,"candidateFingerprint":applied["fingerprint"],"changes":[
        change(&canonical_path,&canonical_before,&canonical_after),change(&format!("author/edits/{EDIT}.json"),&private_before,&private_after)]});
    let bytes = serde_json::to_vec(&journal).unwrap();
    let journal_path = world.root.join(".wedl-rust/pending-outcome.json");
    fs::write(&journal_path, &bytes).unwrap();
    assert!(source::load_world(&world.root, None).is_err());
    assert!(edit::detail(&world, EDIT, None).is_err());
    let inspected = publication::status(&world.root).unwrap();
    assert_eq!(inspected["pending"], true);
    publication::recover(&world.root, &action_drafts::sha256(&bytes)).unwrap();
    assert_eq!(
        fs::read(world.root.join(&canonical_path)).unwrap(),
        canonical_before
    );
    assert_eq!(fs::read(path(&world)).unwrap(), private_before);
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    assert!(!journal_path.exists());
    assert_eq!(
        edit::detail(&world, EDIT, None).unwrap()["draft"]["phase"],
        "draft"
    );
    edit::apply(&world, &input).unwrap();
    assert_eq!(fs::read(path(&world)).unwrap(), private_after);
}

#[test]
fn forged_private_journal_pair_and_unrelated_recovery_edits_refuse_without_changes() {
    let (_directory, world) = fixture::fixture();
    let saved = save(
        &world,
        draft(
            &world,
            "kind: character\ntitle: Reviewed target\n",
            "replace",
        ),
        None,
    );
    let before_private = fs::read(path(&world)).unwrap();
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    let applied = edit::apply(&world, &approve(&request, &report)).unwrap();
    let after_private = fs::read(path(&world)).unwrap();
    let target = &world.records[fixture::ACTOR];
    let after_target = fs::read(world.root.join(&target.source_path)).unwrap();
    let change = |path: &str, before: &[u8], after: &[u8]| json!({"path":path,"beforeSource":std::str::from_utf8(before).unwrap(),"afterSource":std::str::from_utf8(after).unwrap(),"beforeSha256":action_drafts::sha256(before),"afterSha256":action_drafts::sha256(after)});
    let mut journal = json!({"protocol":wedl::CONTRACT,"baseFingerprint":world.fingerprint,"candidateFingerprint":applied["fingerprint"],"changes":[
        change(&target.source_path,&target.raw_bytes,&after_target),change(&format!("author/edits/{EDIT}.json"),&before_private,&after_private)]});
    let mut forged: Json = serde_json::from_slice(&after_private).unwrap();
    forged["title"] = json!("Forged retained history");
    let forged_bytes = serde_json::to_vec(&forged).unwrap();
    journal["changes"][1]["afterSource"] = json!(std::str::from_utf8(&forged_bytes).unwrap());
    journal["changes"][1]["afterSha256"] = json!(action_drafts::sha256(&forged_bytes));
    let journal_path = world.root.join(".wedl-rust/pending-outcome.json");
    let bytes = serde_json::to_vec(&journal).unwrap();
    fs::write(&journal_path, &bytes).unwrap();
    let before = files(&world.root);
    assert!(publication::recover(&world.root, &action_drafts::sha256(&bytes)).is_err());
    assert_eq!(files(&world.root), before);
    journal["changes"][1] = change(
        &format!("author/edits/{EDIT}.json"),
        &before_private,
        &after_private,
    );
    let bytes = serde_json::to_vec(&journal).unwrap();
    fs::write(&journal_path, &bytes).unwrap();
    fs::write(
        world.root.join(&target.source_path),
        [
            after_target,
            b"Unrelated edit after interruption\n".to_vec(),
        ]
        .concat(),
    )
    .unwrap();
    let before = files(&world.root);
    assert!(publication::recover(&world.root, &action_drafts::sha256(&bytes)).is_err());
    assert_eq!(files(&world.root), before);
}

#[test]
fn review_and_apply_decoders_are_closed_and_reject_duplicate_or_missing_authority_fields() {
    let (_directory, world) = fixture::fixture();
    let saved = save(&world, draft(&world, "kind: character\n", "preserve"), None);
    let request = review(&world, &saved);
    let report = edit::review(&world, &request).unwrap();
    let input = json!(approve(&request, &report));
    for key in [
        "approved",
        "reviewSha256",
        "expectedFingerprint",
        "expectedSha256",
        "editId",
        "maxCharacters",
    ] {
        let mut value = input.clone();
        value.as_object_mut().unwrap().remove(key);
        assert!(edit::decode_apply(&value.to_string()).is_err(), "{key}");
    }
    let mut value = input.clone();
    value["operation"] = "record.update".into();
    assert!(edit::decode_apply(&value.to_string()).is_err());
    let duplicate = input.to_string().replacen("{", "{\"approved\":true,", 1);
    assert!(edit::decode_apply(&duplicate).is_err());
    let mut value = json!(request);
    value["maxCharacters"] = 255.into();
    assert!(edit::decode_review(&value.to_string()).is_err());
}
