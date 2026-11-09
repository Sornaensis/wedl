//! Saved proposal previews bind private bytes and current source without writes.
#[path = "support/actions.rs"]
mod support;
use serde_json::{Value as Json, json};
use std::fs;
use support::*;
use wedl::{
    action_drafts::{self, Draft, ReviewConfirmation, ReviewRequest},
    action_store::{self, ProposalPreviewRequest},
    consequences,
    model::World,
    writing_frames::FamilySelection,
};

fn proposal_id(n: usize) -> String {
    format!("proposal_{n:026}")
}

fn save(world: &World, draft: Draft, hash: Option<&str>, reviewed: bool) -> Json {
    let mut request = request(world, draft, hash);
    if reviewed {
        let selection = ReviewRequest {
            draft: request.draft.clone(),
            frame: frame(world, "author"),
            briefs: vec![],
            families: FamilySelection::default(),
            max_characters: 100_000,
        };
        let read = action_drafts::review(world, &selection).unwrap();
        request.review = Some(ReviewConfirmation {
            frame: selection.frame,
            briefs: selection.briefs,
            families: selection.families,
            max_characters: selection.max_characters,
            review_sha256: read["reviewSha256"].as_str().unwrap().into(),
            sufficient: true,
        });
    }
    action_store::save(world, &request).unwrap()
}

fn append(
    world: &World,
    mut draft: Draft,
    number: usize,
    supersedes: Option<String>,
    operations: Json,
) -> Draft {
    let answers: Vec<_> = draft
        .questions
        .iter()
        .filter_map(|q| {
            q.answer
                .as_ref()
                .map(|answer| json!({"questionId":q.id,"answer":answer}))
        })
        .collect();
    let mut proposal = json!({"id":proposal_id(number),"summary":format!("Human alternative {number}"),
        "contextFingerprint":world.fingerprint,"questionAnswers":answers,"evidence":[],"resolver":"human",
        "consequences":{"expectedFingerprint":world.fingerprint,"operations":operations}});
    if let Some(old) = supersedes {
        proposal["supersedes"] = json!(old);
    }
    let mut value = serde_json::to_value(&draft).unwrap();
    value["proposals"].as_array_mut().unwrap().push(proposal);
    value["phase"] = json!("proposed");
    draft = action_drafts::decode(&value.to_string()).unwrap();
    draft
}

fn body_plan(body: &str) -> Json {
    let patch = serde_yaml::to_string(&json!({"kind":"character","body":body})).unwrap();
    json!([{"type":"record.update","entityId":ACTOR,"patch":patch}])
}

fn installed(world: &World, operations: Json) -> (Draft, String) {
    let mut draft = draft(world, &id(1));
    let created = save(world, draft.clone(), None, false);
    draft.phase = "resolution-needed".into();
    draft.questions[0].answer = Some(String::new());
    let ready = save(world, draft.clone(), created["sourceSha256"].as_str(), true);
    let proposed = append(world, draft, 1, None, operations);
    let saved = save(
        world,
        proposed.clone(),
        ready["sourceSha256"].as_str(),
        true,
    );
    (proposed, saved["sourceSha256"].as_str().unwrap().into())
}

fn preview_request(world: &World, hash: &str, number: usize) -> ProposalPreviewRequest {
    action_store::decode_proposal_preview(&json!({"expectedFingerprint":world.fingerprint,
        "actionId":id(1),"expectedSha256":hash,"proposalId":proposal_id(number),"maxCharacters":100_000}).to_string()).unwrap()
}

#[test]
fn saved_preview_equals_pure_candidate_and_preserves_private_and_canonical_bytes() {
    let (_directory, world) = fixture();
    let before = source_image(&world);
    let (draft, hash) = installed(
        &world,
        body_plan("Explicit candidate prose.\r\nOnly in the preview.\r\n"),
    );
    let private = fs::read(path(&world, &id(1))).unwrap();
    let request = preview_request(&world, &hash, 1);
    let result = action_store::proposal_preview(&world, &request).unwrap();
    let pure = consequences::preview(
        &world,
        &draft.proposals[0].consequences,
        request.max_characters,
    )
    .unwrap();
    assert_eq!(result["consequences"], pure);
    assert_eq!(result["sourceSha256"], hash);
    assert_eq!(result["baseFingerprint"], world.fingerprint);
    assert_eq!(result["contextFingerprint"], world.fingerprint);
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["canApply"], false);
    assert_eq!(source_image(&world), before);
    assert_eq!(fs::read(path(&world, &id(1))).unwrap(), private);
    assert!(!world.root.join(".wedl-rust/index.sqlite").exists());
    let metadata = action_store::list(&world).unwrap();
    assert_eq!(metadata["items"][0]["phase"], "proposed");
    assert_eq!(metadata["items"][0]["effectivePhase"], "proposed");
    assert_eq!(
        metadata["items"][0]["activeProposalIds"],
        json!([proposal_id(1)])
    );
}

#[test]
fn preview_requests_reject_unknown_duplicate_unpinned_and_oversized_inputs() {
    let (_directory, world) = fixture();
    let (_, hash) = installed(&world, body_plan("Candidate"));
    let original = serde_json::to_value(preview_request(&world, &hash, 1)).unwrap();
    for key in original.as_object().unwrap().keys() {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(
            action_store::decode_proposal_preview(&missing.to_string()).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("draft", json!({})),
        ("plan", json!({})),
        ("approve", json!(true)),
    ] {
        let mut invalid = original.clone();
        invalid[key] = value;
        assert!(action_store::decode_proposal_preview(&invalid.to_string()).is_err());
    }
    let duplicate = original.to_string().replace(
        "\"maxCharacters\":100000",
        "\"maxCharacters\":100000,\"maxCharacters\":100000",
    );
    assert!(action_store::decode_proposal_preview(&duplicate).is_err());
    for value in [
        json!(255),
        json!(1000001),
        json!(100000.0),
        json!("100000"),
        Json::Null,
    ] {
        let mut invalid = original.clone();
        invalid["maxCharacters"] = value;
        assert!(action_store::decode_proposal_preview(&invalid.to_string()).is_err());
    }
    let mut invalid = original;
    invalid["expectedSha256"] = Json::Null;
    assert!(action_store::decode_proposal_preview(&invalid.to_string()).is_err());
}

#[test]
fn independent_source_private_proposal_and_phase_bindings_refuse_without_leaking_candidate() {
    let (_directory, world) = fixture();
    let (draft, hash) = installed(&world, body_plan("CANDIDATE_SECRET_SENTINEL"));
    let before = source_image(&world);
    let private = fs::read(path(&world, &id(1))).unwrap();
    for mutation in 0..5 {
        let mut request = preview_request(&world, &hash, 1);
        match mutation {
            0 => request.expected_fingerprint = "b".repeat(64),
            1 => request.expected_sha256 = "b".repeat(64),
            2 => request.proposal_id = proposal_id(9),
            3 => request.action_id = id(9),
            _ => request.max_characters = 256,
        }
        let error = action_store::proposal_preview(&world, &request)
            .unwrap_err()
            .json()
            .to_string();
        assert!(!error.contains("CANDIDATE_SECRET_SENTINEL"));
        assert!(!error.contains(&world.fingerprint));
    }
    assert_eq!(source_image(&world), before);
    assert_eq!(fs::read(path(&world, &id(1))).unwrap(), private);
    let mut stale = draft;
    stale.phase = "stale".into();
    let saved = save(&world, stale, Some(&hash), false);
    assert!(
        action_store::proposal_preview(
            &world,
            &preview_request(&world, saved["sourceSha256"].as_str().unwrap(), 1)
        )
        .is_err()
    );
}

#[test]
fn semantically_invalid_private_plan_has_complete_diagnostics_and_never_publishes() {
    let (_directory, world) = fixture();
    let before = source_image(&world);
    let input = serde_yaml::to_string(
        &json!({"kind":"knowledge","id":"know_00000000000000000000000001",
        "title":"Explicit invalid candidate","knower":"char_00000000000000000000000099",
        "claim":{"key":"literal","statement":"Authored candidate words."},"transitions":[]}),
    )
    .unwrap();
    let (_, hash) = installed(&world, json!([{"type":"record.create","draft":input}]));
    let private = fs::read(path(&world, &id(1))).unwrap();
    let preview =
        action_store::proposal_preview(&world, &preview_request(&world, &hash, 1)).unwrap();
    assert_eq!(preview["consequences"]["validation"]["valid"], false);
    assert!(
        !preview["consequences"]["validation"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(preview["canApply"], false);
    assert_eq!(source_image(&world), before);
    assert_eq!(fs::read(path(&world, &id(1))).unwrap(), private);
}

#[test]
fn canonical_drift_is_read_only_and_explicit_supersession_keeps_original_history() {
    let (_directory, world) = fixture();
    let (mut draft, hash) = installed(&world, body_plan("First proposed alternative"));
    let original = draft.proposals[0].clone();
    let before = fs::read(path(&world, &id(1))).unwrap();
    let current = change_source(&world);
    let metadata = action_store::detail(&current, &id(1), Some(&hash)).unwrap();
    assert_eq!(metadata["phase"], "proposed");
    assert_eq!(metadata["effectivePhase"], "stale");
    assert_eq!(metadata["sourceChanged"], true);
    assert_eq!(fs::read(path(&current, &id(1))).unwrap(), before);
    assert!(
        action_store::proposal_preview(&current, &preview_request(&current, &hash, 1)).is_err()
    );
    draft.phase = "stale".into();
    let stale = save(&current, draft.clone(), Some(&hash), false);
    let draft = append(
        &current,
        draft,
        2,
        Some(proposal_id(1)),
        body_plan("Second proposed alternative"),
    );
    let saved = save(
        &current,
        draft.clone(),
        stale["sourceSha256"].as_str(),
        true,
    );
    assert_eq!(draft.proposals[0], original);
    assert_eq!(draft.base_fingerprint, world.fingerprint);
    assert_ne!(
        draft.proposals[1].context_fingerprint,
        draft.base_fingerprint
    );
    let hash = saved["sourceSha256"].as_str().unwrap();
    let metadata = action_store::detail(&current, &id(1), Some(hash)).unwrap();
    assert_eq!(metadata["effectivePhase"], "proposed");
    assert_eq!(metadata["sourceChanged"], true);
    assert_eq!(metadata["activeProposalIds"], json!([proposal_id(2)]));
    assert!(action_store::proposal_preview(&current, &preview_request(&current, hash, 1)).is_err());
    let fresh =
        action_store::proposal_preview(&current, &preview_request(&current, hash, 2)).unwrap();
    assert_eq!(fresh["baseFingerprint"], world.fingerprint);
    assert_eq!(fresh["contextFingerprint"], current.fingerprint);
}

#[test]
fn a_new_parallel_alternative_does_not_hide_an_older_active_stale_context() {
    let (_directory, world) = fixture();
    let (mut draft, hash) = installed(&world, body_plan("First alternative"));
    let current = change_source(&world);
    draft.phase = "stale".into();
    let stale = save(&current, draft.clone(), Some(&hash), false);
    let parallel = append(
        &current,
        draft,
        2,
        None,
        body_plan("Parallel current alternative"),
    );
    let saved = save(&current, parallel, stale["sourceSha256"].as_str(), true);
    let hash = saved["sourceSha256"].as_str().unwrap();
    let detail = action_store::detail(&current, &id(1), Some(hash)).unwrap();
    assert_eq!(detail["phase"], "proposed");
    assert_eq!(detail["effectivePhase"], "stale");
    assert_eq!(
        detail["activeProposalIds"],
        json!([proposal_id(1), proposal_id(2)])
    );
    assert!(action_store::proposal_preview(&current, &preview_request(&current, hash, 2)).is_err());
}
