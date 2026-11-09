//! Outcome approval, exact publication receipts and safe replay.
#[path = "support/actions.rs"]
mod support;
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, fs};
use support::*;
use wedl::{
    action_drafts::{self, Draft, ReviewConfirmation, ReviewRequest as ContextReview},
    action_outcomes::{self, ApplyRequest, ReviewRequest, VerifyRequest},
    action_store, consequences,
    model::World,
    source,
    writing_frames::FamilySelection,
};

const PROPOSAL: &str = "proposal_00000000000000000000000001";
const SECOND: &str = "proposal_00000000000000000000000002";
const NEW_OBJECT: &str = "obj_00000000000000000000000077";

fn save(world: &World, draft: Draft, hash: Option<&str>, reviewed: bool) -> Json {
    let mut request = request(world, draft, hash);
    if reviewed {
        let context = ContextReview {
            draft: request.draft.clone(),
            frame: frame(world, "author"),
            briefs: vec![],
            families: FamilySelection::default(),
            max_characters: 100_000,
        };
        let result = action_drafts::review(world, &context).unwrap();
        request.review = Some(ReviewConfirmation {
            frame: context.frame,
            briefs: context.briefs,
            families: context.families,
            max_characters: context.max_characters,
            review_sha256: result["reviewSha256"].as_str().unwrap().into(),
            sufficient: true,
        });
    }
    action_store::save(world, &request).unwrap()
}

fn body_plan(body: &str) -> Json {
    json!([{"type":"record.update","entityId":ACTOR,
        "patch":serde_yaml::to_string(&json!({"kind":"character","body":body})).unwrap()}])
}

fn multi_plan() -> Json {
    let mut operations = body_plan("Published CRLF 🌲\r\nexact trailing spaces  ");
    operations.as_array_mut().unwrap().push(json!({"type":"record.create",
        "draft":format!("kind: object\nid: {NEW_OBJECT}\ntitle: Reviewed object\nbody: Explicit independent object.\n")}));
    operations
}

fn installed(world: &World, operations: Json) -> (Draft, String) {
    let mut draft = draft(world, &id(1));
    let created = save(world, draft.clone(), None, false);
    draft.questions[0].answer = Some(String::new());
    draft.phase = "resolution-needed".into();
    let ready = save(world, draft.clone(), created["sourceSha256"].as_str(), true);
    let answers: Vec<_> = draft
        .questions
        .iter()
        .filter_map(|q| {
            q.answer
                .as_ref()
                .map(|answer| json!({"questionId":q.id,"answer":answer}))
        })
        .collect();
    let mut value = serde_json::to_value(&draft).unwrap();
    value["phase"] = json!("proposed");
    value["proposals"] = json!([{"id":PROPOSAL,"summary":"Human outcome","evidence":[],"resolver":"human",
        "contextFingerprint":world.fingerprint,"questionAnswers":answers,
        "consequences":{"expectedFingerprint":world.fingerprint,"operations":operations}}]);
    draft = action_drafts::decode(&value.to_string()).unwrap();
    let proposed = save(world, draft.clone(), ready["sourceSha256"].as_str(), true);
    (draft, proposed["sourceSha256"].as_str().unwrap().into())
}

fn review_request(world: &World, hash: &str) -> ReviewRequest {
    ReviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: id(1),
        expected_sha256: hash.into(),
        proposal_id: PROPOSAL.into(),
        max_characters: 100_000,
    }
}

fn apply_request(world: &World, hash: &str) -> ApplyRequest {
    let request = review_request(world, hash);
    let report = action_outcomes::review(world, &request).unwrap();
    let mut value = serde_json::to_value(request).unwrap();
    value["reviewSha256"] = report["reviewSha256"].clone();
    value["approved"] = json!(true);
    action_outcomes::decode_apply(&value.to_string()).unwrap()
}

fn verify_request(world: &World, hash: &str) -> VerifyRequest {
    VerifyRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: id(1),
        expected_sha256: hash.into(),
        max_characters: 100_000,
    }
}

fn tree(world: &World) -> BTreeMap<String, Vec<u8>> {
    walkdir::WalkDir::new(&world.root)
        .into_iter()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(&world.root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn complete_review_binds_exact_source_and_private_transition_without_writes() {
    let (_temp, world) = fixture();
    let (draft, hash) = installed(&world, multi_plan());
    let before = tree(&world);
    let request = review_request(&world, &hash);
    let result = action_outcomes::review(&world, &request).unwrap();
    assert_eq!(tree(&world), before);
    assert_eq!(result["draft"], serde_json::to_value(&draft).unwrap());
    assert_eq!(result["sourceSha256"], hash);
    assert_eq!(result["approvalRequired"], true);
    assert_eq!(result["canApply"], true);
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["privateTransition"]["resultingPhase"], "applied");
    assert_eq!(result["privateTransition"]["preservesHistory"], true);
    assert_eq!(result["privateTransition"]["beforeSha256"], hash);
    assert_eq!(
        result["consequences"],
        consequences::preview(
            &world,
            &draft.proposals[0].consequences,
            request.max_characters
        )
        .unwrap()
    );
    let mut material = result.clone();
    let digest = material
        .as_object_mut()
        .unwrap()
        .remove("reviewSha256")
        .unwrap();
    let binding = material
        .as_object_mut()
        .unwrap()
        .remove("reviewBinding")
        .unwrap();
    let report_hash = material
        .as_object_mut()
        .unwrap()
        .remove("reportSha256")
        .unwrap();
    let mut bytes = b"wedl-action-outcome-report\0".to_vec();
    bytes.extend(serde_json::to_vec(&material).unwrap());
    assert_eq!(report_hash, action_drafts::sha256(&bytes));
    assert_eq!(binding["reportSha256"], report_hash);
    let mut bytes = b"wedl-action-outcome-review\0".to_vec();
    bytes.extend(serde_json::to_vec(&binding).unwrap());
    assert_eq!(digest, action_drafts::sha256(&bytes));
    assert_ne!(digest, result["consequences"]["previewSha256"]);
}

#[test]
fn multi_record_application_matches_review_and_retains_immutable_history() {
    let (_temp, world) = fixture();
    let (before, hash) = installed(&world, multi_plan());
    let request = apply_request(&world, &hash);
    let review = action_outcomes::review(&world, &request.review_request()).unwrap();
    let result = action_outcomes::apply(&world, &request).unwrap();
    assert_eq!(result["writesPerformed"], true);
    assert_eq!(result["alreadyApplied"], false);
    assert_eq!(result["changedFiles"], 2);
    assert_eq!(result["canonicalNoOp"], false);
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(current.fingerprint, result["fingerprint"]);
    for file in review["consequences"]["files"].as_array().unwrap() {
        let bytes = fs::read(world.root.join(file["sourcePath"].as_str().unwrap())).unwrap();
        assert_eq!(bytes, file["afterSource"].as_str().unwrap().as_bytes());
        assert_eq!(action_drafts::sha256(&bytes), file["afterSha256"]);
    }
    let detail = action_store::detail(&current, &id(1), result["sourceSha256"].as_str()).unwrap();
    let mut applied = action_drafts::decode(&detail["source"].as_str().unwrap()).unwrap();
    assert_eq!(applied.phase, "applied");
    let receipt = applied.applied_receipt.take().unwrap();
    assert_eq!(receipt.before_action_sha256, hash);
    assert_eq!(receipt.base_fingerprint, world.fingerprint);
    assert_eq!(receipt.candidate_fingerprint, current.fingerprint);
    assert_eq!(receipt.review_sha256, request.review_sha256);
    applied.phase = "proposed".into();
    assert_eq!(applied, before);
    let verified = action_outcomes::verify(
        &current,
        &verify_request(&current, result["sourceSha256"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(verified["verified"], true);
    assert_eq!(verified["filesMatch"], true);
    assert_eq!(verified["sourceMatches"], true);
    assert_eq!(verified["writesPerformed"], false);
    assert_eq!(verified["verificationScope"], "exact-source-publication");
    assert!(!world.root.join(".wedl-rust/pending-outcome.json").exists());
}

#[test]
fn exact_retry_is_read_only_and_rejects_changed_request_bindings() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let request = apply_request(&world, &hash);
    action_outcomes::apply(&world, &request).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let before = tree(&current);
    let result = action_outcomes::apply(&current, &request).unwrap();
    assert_eq!(result["alreadyApplied"], true);
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(tree(&current), before);
    for key in ["expectedFingerprint", "expectedSha256", "reviewSha256"] {
        let mut changed = serde_json::to_value(&request).unwrap();
        changed[key] = json!("a".repeat(64));
        let changed = action_outcomes::decode_apply(&changed.to_string()).unwrap();
        assert!(action_outcomes::apply(&current, &changed).is_err(), "{key}");
    }
    let mut changed = request.clone();
    changed.max_characters += 1;
    assert!(action_outcomes::apply(&current, &changed).is_err());
    changed = request.clone();
    changed.proposal_id = SECOND.into();
    assert!(action_outcomes::apply(&current, &changed).is_err());
    changed = request.clone();
    changed.approved = false;
    assert!(action_outcomes::apply(&current, &changed).is_err());
    assert_eq!(tree(&current), before);
}

#[test]
fn canonical_noop_still_publishes_one_terminal_receipt_and_replays_without_writes() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(
        &world,
        json!([{"type":"record.update","entityId":ACTOR,"patch":"kind: character\n"}]),
    );
    let before = source_image(&world);
    let request = apply_request(&world, &hash);
    let result = action_outcomes::apply(&world, &request).unwrap();
    assert_eq!(result["canonicalNoOp"], true);
    assert_eq!(result["changedFiles"], 0);
    assert_eq!(result["writesPerformed"], true);
    assert_ne!(result["sourceSha256"], hash);
    assert_eq!(result["fingerprint"], world.fingerprint);
    assert_eq!(source_image(&world), before);
    let frozen = tree(&world);
    assert_eq!(
        action_outcomes::apply(&world, &request).unwrap()["writesPerformed"],
        false
    );
    assert_eq!(tree(&world), frozen);
    assert_eq!(
        action_store::list(&world).unwrap()["items"][0]["phase"],
        "applied"
    );
}

#[test]
fn generic_saves_cannot_create_or_change_terminal_receipts() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let apply = apply_request(&world, &hash);
    let done = action_outcomes::apply(&world, &apply).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let detail = action_store::detail(&current, &id(1), None).unwrap();
    let applied = action_drafts::decode(detail["source"].as_str().unwrap()).unwrap();
    let before = tree(&current);
    let attempt = request(&current, applied.clone(), done["sourceSha256"].as_str());
    assert!(action_store::save(&current, &attempt).is_err());
    let mut value = serde_json::to_value(applied).unwrap();
    value["phase"] = json!("proposed");
    assert!(action_drafts::decode(&value.to_string()).is_err());
    value["phase"] = json!("applied");
    value["title"] = json!("Mutated terminal title");
    assert!(action_drafts::decode(&value.to_string()).is_err());
    value["appliedReceipt"] = Json::Null;
    assert!(action_drafts::decode(&value.to_string()).is_err());
    value.as_object_mut().unwrap().remove("appliedReceipt");
    assert!(action_drafts::decode(&value.to_string()).is_err());
    assert_eq!(tree(&current), before);
}

#[test]
fn review_hash_cannot_be_replaced_by_proposal_preview_or_readiness_hash() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, body_plan("New body"));
    let mut apply = apply_request(&world, &hash);
    let before = tree(&world);
    let preview = action_store::proposal_preview(&world, &apply.review_request()).unwrap();
    apply.review_sha256 = preview["previewSha256"].as_str().unwrap().into();
    assert!(action_outcomes::apply(&world, &apply).is_err());
    apply.review_sha256 = preview["consequences"]["previewSha256"]
        .as_str()
        .unwrap()
        .into();
    assert!(action_outcomes::apply(&world, &apply).is_err());
    apply.approved = false;
    assert!(action_outcomes::apply(&world, &apply).is_err());
    assert_eq!(tree(&world), before);
}

#[test]
fn stale_canonical_and_raw_private_changes_invalidate_apply_without_publication() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, body_plan("New body"));
    let request = apply_request(&world, &hash);
    let mut bytes = fs::read(path(&world, &id(1))).unwrap();
    bytes.push(b'\n');
    fs::write(path(&world, &id(1)), &bytes).unwrap();
    let before = tree(&world);
    assert!(action_outcomes::apply(&world, &request).is_err());
    assert_eq!(tree(&world), before);
    bytes.pop();
    fs::write(path(&world, &id(1)), bytes).unwrap();
    let current = change_source(&world);
    let before = tree(&current);
    assert!(action_outcomes::apply(&world, &request).is_err());
    assert!(action_outcomes::apply(&current, &request).is_err());
    assert_eq!(tree(&current), before);
}

#[test]
fn invalid_final_candidate_is_reviewable_but_never_applies() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(
        &world,
        json!([{"type":"record.update","entityId":ACTOR,
        "patch":"kind: character\ninitial_state: {undeclared: true}\n"}]),
    );
    let review = action_outcomes::review(&world, &review_request(&world, &hash)).unwrap();
    assert_eq!(review["canApply"], false);
    assert_eq!(review["consequences"]["validation"]["valid"], false);
    let request = apply_request(&world, &hash);
    let before = tree(&world);
    assert!(action_outcomes::apply(&world, &request).is_err());
    assert_eq!(tree(&world), before);
}

#[test]
fn apply_requires_exact_present_answer_snapshot_not_just_active_proposal() {
    let (_temp, world) = fixture();
    let (mut draft, hash) = installed(&world, body_plan("New body"));
    draft.questions[0].answer = Some("Different current answer".into());
    let bytes = action_drafts::serialize(&draft).unwrap();
    fs::write(path(&world, &id(1)), &bytes).unwrap();
    assert_ne!(action_drafts::sha256(&bytes), hash);
    let request = review_request(&world, &action_drafts::sha256(&bytes));
    let before = tree(&world);
    assert!(action_outcomes::review(&world, &request).is_err());
    assert_eq!(tree(&world), before);
}

#[test]
fn outcome_verification_distinguishes_whole_source_and_selected_file_drift() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let done = action_outcomes::apply(&world, &apply_request(&world, &hash)).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let hash = done["sourceSha256"].as_str().unwrap();
    let drifted = change_source(&current);
    let report = action_outcomes::verify(&drifted, &verify_request(&drifted, hash)).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["sourceMatches"], false);
    assert_eq!(report["filesMatch"], true);
    let target = &drifted.records[ACTOR];
    let mut bytes = target.raw_bytes.clone();
    bytes.extend(b"\nExternal correction.\n");
    fs::write(drifted.root.join(&target.source_path), bytes).unwrap();
    let changed = source::load_world(&world.root, None).unwrap();
    let report = action_outcomes::verify(&changed, &verify_request(&changed, hash)).unwrap();
    assert_eq!(report["filesMatch"], false);
    assert_eq!(report["files"][0]["status"], "changed");
}

#[test]
fn verification_reports_missing_and_moved_record_without_filling_in_source() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let done = action_outcomes::apply(&world, &apply_request(&world, &hash)).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let object = &current.records[NEW_OBJECT];
    let old = current.root.join(&object.source_path);
    let new = current.root.join("story/moved-object.md");
    fs::rename(&old, &new).unwrap();
    let moved = source::load_world(&world.root, None).unwrap();
    let request = verify_request(&moved, done["sourceSha256"].as_str().unwrap());
    assert_eq!(
        action_outcomes::verify(&moved, &request).unwrap()["files"][1]["status"],
        "moved"
    );
    fs::remove_file(new).unwrap();
    let missing = source::load_world(&world.root, None).unwrap();
    let request = verify_request(&missing, done["sourceSha256"].as_str().unwrap());
    assert_eq!(
        action_outcomes::verify(&missing, &request).unwrap()["files"][1]["status"],
        "missing"
    );
    assert!(!old.exists());
}

#[test]
fn retry_refuses_canonical_or_exact_private_drift() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let request = apply_request(&world, &hash);
    action_outcomes::apply(&world, &request).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let private = fs::read(path(&world, &id(1))).unwrap();
    let mut changed = private.clone();
    changed.push(b'\n');
    fs::write(path(&world, &id(1)), changed).unwrap();
    let before = tree(&current);
    assert!(action_outcomes::apply(&current, &request).is_err());
    assert_eq!(tree(&current), before);
    fs::write(path(&world, &id(1)), private).unwrap();
    let changed = change_source(&current);
    let before = tree(&changed);
    assert!(action_outcomes::apply(&changed, &request).is_err());
    assert_eq!(tree(&changed), before);
}

#[test]
fn applied_receipt_rejects_unknown_missing_duplicate_or_rebound_fields() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    action_outcomes::apply(&world, &apply_request(&world, &hash)).unwrap();
    let source = fs::read_to_string(path(&world, &id(1))).unwrap();
    let original: Json = serde_json::from_str(&source).unwrap();
    for field in [
        "candidateFingerprint",
        "requestSha256",
        "beforeDraftSha256",
        "proposalSha256",
        "planSha256",
    ] {
        let mut changed = original.clone();
        changed["appliedReceipt"][field] = json!("a".repeat(64));
        assert!(
            action_drafts::decode(&changed.to_string()).is_err(),
            "{field}"
        );
    }
    for field in ["beforeSha256", "afterSha256"] {
        let mut changed = original.clone();
        changed["appliedReceipt"]["files"][0][field] = json!("a".repeat(64));
        assert!(
            action_drafts::decode(&changed.to_string()).is_err(),
            "{field}"
        );
    }
    let mut changed = original.clone();
    changed["appliedReceipt"]["approve"] = json!(true);
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    changed = original.clone();
    changed["appliedReceipt"]["files"][0]
        .as_object_mut()
        .unwrap()
        .remove("beforeSha256");
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    changed = original.clone();
    changed["appliedReceipt"]["files"][0]["sourcePath"] = json!("../outside.md");
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    let duplicate = source.replacen(
        "\"appliedReceipt\":{",
        "\"appliedReceipt\":{\"proposalId\":\"proposal_00000000000000000000000001\",",
        1,
    );
    assert!(action_drafts::decode(&duplicate).is_err());
}

#[test]
fn closed_outcome_decoders_and_complete_budgets_reject_ambiguous_inputs() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let apply = apply_request(&world, &hash);
    let original = serde_json::to_value(&apply).unwrap();
    for key in original.as_object().unwrap().keys() {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(key);
        assert!(
            action_outcomes::decode_apply(&value.to_string()).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("approved", Json::Null),
        ("approved", json!(false)),
        ("approve", json!(true)),
        ("plan", json!({})),
        ("maxCharacters", json!(100_000.5)),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        assert!(
            action_outcomes::decode_apply(&bad.to_string()).is_err(),
            "{key}"
        );
    }
    let duplicate = serde_json::to_string(&apply)
        .unwrap()
        .replacen('{', "{\"approved\":true,", 1);
    assert!(action_outcomes::decode_apply(&duplicate).is_err());
    assert!(action_outcomes::decode_apply(&" ".repeat(16_385)).is_err());
    let mut tiny = review_request(&world, &hash);
    tiny.max_characters = 256;
    let before = tree(&world);
    assert!(action_outcomes::review(&world, &tiny).is_err());
    assert_eq!(tree(&world), before);
    let mut small = apply.clone();
    small.max_characters = 999;
    assert!(action_outcomes::apply(&world, &small).is_err());
    assert_eq!(tree(&world), before);
}

#[test]
fn inactive_superseded_cancelled_and_historical_actions_cannot_apply() {
    let (_temp, world) = fixture();
    let (mut draft, hash) = installed(&world, body_plan("Candidate"));
    let apply = apply_request(&world, &hash);
    draft.phase = "cancelled".into();
    let cancelled = save(&world, draft.clone(), Some(&hash), false);
    let mut review = review_request(&world, cancelled["sourceSha256"].as_str().unwrap());
    assert!(action_outcomes::review(&world, &review).is_err());
    assert!(action_outcomes::apply(&world, &apply).is_err());
    // Direct source editing does not grant authority; active selection and the
    // exact private hash are always checked against the captured document.
    draft.phase = "proposed".into();
    let mut newer = draft.proposals[0].clone();
    newer.id = SECOND.into();
    newer.supersedes = Some(PROPOSAL.into());
    draft.proposals.push(newer);
    let bytes = action_drafts::serialize(&draft).unwrap();
    fs::write(path(&world, &id(1)), &bytes).unwrap();
    review.expected_sha256 = action_drafts::sha256(&bytes);
    assert!(action_outcomes::review(&world, &review).is_err());
    review.proposal_id = SECOND.into();
    assert!(action_outcomes::review(&world, &review).is_ok());
    let mut historical = world.clone();
    historical.revision = Some("captured-history".into());
    assert!(action_outcomes::review(&historical, &review).is_err());
}

#[test]
fn exact_terminal_retry_does_not_recreate_removed_writer_metadata() {
    let (_temp, world) = fixture();
    let (_, hash) = installed(&world, multi_plan());
    let request = apply_request(&world, &hash);
    action_outcomes::apply(&world, &request).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    fs::remove_dir_all(current.root.join(".wedl-rust")).unwrap();
    let before = tree(&current);
    let result = action_outcomes::apply(&current, &request).unwrap();
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["alreadyApplied"], true);
    assert_eq!(tree(&current), before);
    assert!(!current.root.join(".wedl-rust").exists());
}
