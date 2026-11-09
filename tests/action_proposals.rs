//! Proposal history preserves private alternatives without applying their plans.
#[path = "support/actions.rs"]
mod support;
use serde_json::{Value as Json, json};
use support::*;
use wedl::{
    action_drafts::{
        self, Draft, Proposal, Question, QuestionAnswer, ReviewConfirmation, ReviewRequest,
        SaveRequest,
    },
    consequences,
    model::World,
    writing_frames::FamilySelection,
};

fn proposal_id(n: usize) -> String {
    format!("proposal_{n:026}")
}
fn question_id(n: usize) -> String {
    format!("question_{n:026}")
}

fn ready(world: &World) -> Draft {
    let mut draft = draft(world, &id(1));
    draft.phase = "resolution-needed".into();
    draft.questions[0].answer = Some(String::new());
    draft
}

fn proposal(world: &World, draft: &Draft, n: usize) -> Proposal {
    Proposal {
        id: proposal_id(n),
        summary: format!("Private human alternative {n}"),
        context_fingerprint: world.fingerprint.clone(),
        evidence: vec![],
        question_answers: draft
            .questions
            .iter()
            .filter_map(|question| {
                question.answer.as_ref().map(|answer| QuestionAnswer {
                    question_id: question.id.clone(),
                    answer: answer.clone(),
                })
            })
            .collect(),
        resolver: "human".into(),
        consequences: consequences::decode(
            &json!({"expectedFingerprint":world.fingerprint,
            "operations":[{"type":"record.update","entityId":ACTOR,
                "patch":"kind: character\nbody: Proposed canonical prose.\n"}]})
            .to_string(),
        )
        .unwrap(),
        supersedes: None,
    }
}

fn proposed(world: &World, previous: &Draft, n: usize) -> Draft {
    let mut next = previous.clone();
    next.phase = "proposed".into();
    next.proposals.push(proposal(world, &next, n));
    next
}

fn confirmation(world: &World, draft: &Draft) -> ReviewConfirmation {
    let request = ReviewRequest {
        draft: draft.clone(),
        frame: frame(world, "author"),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    };
    let reviewed = action_drafts::review(world, &request).unwrap();
    ReviewConfirmation {
        frame: request.frame,
        briefs: request.briefs,
        families: request.families,
        max_characters: request.max_characters,
        review_sha256: reviewed["reviewSha256"].as_str().unwrap().into(),
        sufficient: true,
    }
}

fn save(world: &World, draft: &Draft, review: bool) -> SaveRequest {
    SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256: Some("a".repeat(64)),
        draft: draft.clone(),
        review: review.then(|| confirmation(world, draft)),
    }
}

fn decode(value: &Json) -> Result<Draft, wedl::Error> {
    action_drafts::decode(&value.to_string())
}

#[test]
fn current_grammar_requires_history_and_closed_human_proposals() {
    let (_directory, world) = fixture();
    let initial = draft(&world, &id(1));
    let mut missing = json!(initial);
    missing.as_object_mut().unwrap().remove("proposals");
    assert!(decode(&missing).is_err());
    assert_eq!(json!(initial)["proposals"], json!([]));
    let next = proposed(&world, &ready(&world), 1);
    assert_eq!(decode(&json!(next)).unwrap(), next);
    for key in [
        "id",
        "summary",
        "contextFingerprint",
        "questionAnswers",
        "evidence",
        "resolver",
        "consequences",
    ] {
        let mut bad = json!(next);
        bad["proposals"][0].as_object_mut().unwrap().remove(key);
        assert!(decode(&bad).is_err(), "missing {key}");
    }
    for (key, value) in [
        ("evidence", Json::Null),
        ("frame", json!({})),
        ("reviewSha256", json!("a".repeat(64))),
        ("resolver", json!("adapter")),
        ("resolver", json!({"kind":"human"})),
        ("summary", json!("  ")),
        ("contextFingerprint", json!("A".repeat(64))),
        ("supersedes", Json::Null),
        ("supersedes", json!(proposal_id(1))),
        ("supersedes", json!(proposal_id(2))),
    ] {
        let mut bad = json!(next);
        bad["proposals"][0][key] = value;
        assert!(decode(&bad).is_err(), "{key}");
    }
    let raw = serde_json::to_string(&next).unwrap();
    for (from, to) in [
        (
            "\"resolver\":\"human\"",
            "\"resolver\":\"human\",\"resolv\\u0065r\":\"human\"",
        ),
        ("\"answer\":\"\"", "\"answer\":\"\",\"answer\":\"\""),
    ] {
        assert!(action_drafts::decode(&raw.replace(from, to)).is_err());
    }
    assert!(action_drafts::valid_proposal_id(&proposal_id(1)));
    assert!(!action_drafts::valid_proposal_id(&id(1)));
}

#[test]
fn exact_history_roundtrip_keeps_raw_plan_numbers_comments_and_crlf() {
    let (_directory, world) = fixture();
    let mut next = proposed(&world, &ready(&world), 1);
    let patch = "kind: character\r\n# Preserve authored tokens.\r\nx-opaque: {large: 9007199254740993, float: 1.0, zero: -0.0}\r\nbody: \"Proposed 🌲\\r\\n\"\r\n";
    next.proposals[0].consequences = consequences::decode(
        &json!({"expectedFingerprint":world.fingerprint,
        "operations":[{"type":"record.update","entityId":ACTOR,"patch":patch}]})
        .to_string(),
    )
    .unwrap();
    let bytes = action_drafts::serialize(&next).unwrap();
    let restored = action_drafts::decode(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(restored, next);
    assert_eq!(
        json!(restored.proposals[0].consequences)["operations"][0]["patch"],
        patch
    );
    assert_eq!(restored.horizon.tick, TICK);
    assert_eq!(restored.proposals[0].question_answers[0].answer, "");
    assert_eq!(restored.body_markdown, next.body_markdown);
}

#[test]
fn every_new_proposal_requires_current_context_exact_answers_and_fresh_sufficiency() {
    let (_directory, world) = fixture();
    let previous = ready(&world);
    let next = proposed(&world, &previous, 1);
    let request = save(&world, &next, true);
    action_drafts::validate_save(&world, &request, Some(&previous)).unwrap();
    for change in 0..6 {
        let mut bad = request.clone();
        match change {
            0 => bad.review = None,
            1 => bad.review.as_mut().unwrap().sufficient = false,
            2 => {
                bad.draft.proposals[0].context_fingerprint = "b".repeat(64);
                bad.draft.proposals[0].consequences.expected_fingerprint = "b".repeat(64);
            }
            3 => bad.draft.proposals[0].question_answers.clear(),
            4 => bad.draft.proposals[0].question_answers[0].answer = "Changed snapshot".into(),
            _ => {
                bad.draft.questions[0].answer = None;
                bad.draft.proposals[0].question_answers.clear();
            }
        }
        assert!(
            action_drafts::validate_save(&world, &bad, Some(&previous)).is_err(),
            "{change}"
        );
    }
    let mut bad = json!(next);
    bad["proposals"][0]["consequences"]["expectedFingerprint"] = json!("b".repeat(64));
    assert!(decode(&bad).is_err());
}

#[test]
fn complete_review_digest_binds_all_new_proposal_content() {
    let (_directory, world) = fixture();
    let previous = ready(&world);
    let next = proposed(&world, &previous, 1);
    let request = save(&world, &next, true);
    for change in 0..5 {
        let mut bad = request.clone();
        match change {
            0 => bad.draft.proposals[0].summary.push('!'),
            1 => bad.draft.proposals[0].id = proposal_id(2),
            2 => bad.draft.body_markdown.push('!'),
            3 => bad.draft.intent.push('!'),
            _ => bad.draft.proposals[0].consequences = consequences::decode(&json!({"expectedFingerprint":world.fingerprint,
                "operations":[{"type":"record.update","entityId":ACTOR,"patch":"kind: character\nbody: Different proposed words.\n"}]}).to_string()).unwrap(),
        }
        assert!(
            matches!(
                action_drafts::validate_save(&world, &bad, Some(&previous)),
                Err(wedl::Error::Conflict(_))
            ),
            "{change}"
        );
        bad.review = Some(confirmation(&world, &bad.draft));
        action_drafts::validate_save(&world, &bad, Some(&previous)).unwrap();
    }
}

#[test]
fn proposal_append_requires_an_explicit_resolution_transition() {
    let (_directory, world) = fixture();
    for old in [
        "draft",
        "context-needed",
        "resolution-needed",
        "proposed",
        "stale",
        "cancelled",
    ] {
        let mut previous = ready(&world);
        previous.phase = old.into();
        if old == "proposed" {
            previous.proposals.push(proposal(&world, &previous, 1));
        }
        for new in [
            "draft",
            "context-needed",
            "resolution-needed",
            "proposed",
            "stale",
            "cancelled",
        ] {
            let mut next = proposed(&world, &previous, 2);
            next.phase = new.into();
            let request = save(&world, &next, true);
            assert_eq!(
                action_drafts::validate_save(&world, &request, Some(&previous)).is_ok(),
                ["resolution-needed", "stale"].contains(&old) && new == "proposed",
                "{old}->{new}"
            );
        }
    }
    let mut next = proposed(&world, &ready(&world), 1);
    next.phase = "draft".into();
    assert!(action_drafts::validate_save(&world, &save(&world, &next, false), None).is_err());
    for old in ["resolution-needed", "stale"] {
        let mut previous = proposed(&world, &ready(&world), 1);
        previous.phase = old.into();
        let mut next = previous.clone();
        next.phase = "proposed".into();
        assert!(
            action_drafts::validate_save(&world, &save(&world, &next, true), Some(&previous))
                .is_err()
        );
    }
}

#[test]
fn prior_proposals_are_an_exact_immutable_prefix_even_when_stale() {
    let (_directory, world) = fixture();
    let mut previous = proposed(&world, &ready(&world), 1);
    previous.proposals.push(proposal(&world, &previous, 2));
    previous.phase = "stale".into();
    for change in 0..6 {
        let mut next = previous.clone();
        match change {
            0 => {
                next.proposals.pop();
            }
            1 => next.proposals.swap(0, 1),
            2 => next.proposals[0].summary.push('!'),
            3 => next.proposals[0].question_answers[0].answer.push('!'),
            4 => next.proposals[1].supersedes = Some(next.proposals[0].id.clone()),
            _ => {
                next.proposals[0].context_fingerprint = "a".repeat(64);
                next.proposals[0].consequences.expected_fingerprint = "a".repeat(64);
            }
        }
        assert!(
            action_drafts::validate_save(&world, &save(&world, &next, false), Some(&previous))
                .is_err(),
            "{change}"
        );
    }
    let mut next = proposed(&world, &previous, 3);
    next.proposals[2].supersedes = Some(proposal_id(1));
    action_drafts::validate_save(&world, &save(&world, &next, true), Some(&previous)).unwrap();
    assert_eq!(next.proposals[..2], previous.proposals);
}

#[test]
fn historical_question_ids_and_order_remain_addressable_after_edits() {
    let (_directory, world) = fixture();
    let mut previous = ready(&world);
    previous.questions.push(Question {
        id: question_id(2),
        text: "Second question".into(),
        required: false,
        answer: Some("Second answer".into()),
    });
    previous.questions.push(Question {
        id: question_id(3),
        text: "Unanswered optional".into(),
        required: false,
        answer: None,
    });
    previous = proposed(&world, &previous, 1);
    let mut next = previous.clone();
    next.phase = "stale".into();
    next.questions[0].text = "Reworded current question".into();
    next.questions[0].answer = Some("New answer".into());
    action_drafts::validate_save(&world, &save(&world, &next, false), Some(&previous)).unwrap();
    assert_eq!(
        next.proposals[0].question_answers,
        previous.proposals[0].question_answers
    );
    for change in 0..4 {
        let mut bad = next.clone();
        match change {
            0 => {
                bad.questions.remove(0);
            }
            1 => bad.questions.swap(0, 1),
            2 => bad.proposals[0].question_answers.swap(0, 1),
            _ => {
                let duplicate = bad.proposals[0].question_answers[0].clone();
                bad.proposals[0].question_answers.push(duplicate);
            }
        }
        assert!(decode(&json!(bad)).is_err(), "{change}");
    }
    next.questions.remove(2); // No historical answer ever named this optional question.
    action_drafts::validate_save(&world, &save(&world, &next, false), Some(&previous)).unwrap();
}

#[test]
fn proposed_content_edits_require_stale_but_phase_only_rejection_and_cancel_preserve_it() {
    let (_directory, world) = fixture();
    let previous = proposed(&world, &ready(&world), 1);
    action_drafts::validate_save(&world, &save(&world, &previous, false), Some(&previous)).unwrap();
    assert!(
        action_drafts::validate_save(&world, &save(&world, &previous, true), Some(&previous))
            .is_err()
    );
    for phase in ["resolution-needed", "cancelled", "stale"] {
        let mut next = previous.clone();
        next.phase = phase.into();
        action_drafts::validate_save(
            &world,
            &save(&world, &next, phase == "resolution-needed"),
            Some(&previous),
        )
        .unwrap();
        for change in 0..4 {
            let mut edited = next.clone();
            match change {
                0 => edited.intent.push('!'),
                1 => edited.body_markdown.push('!'),
                2 => edited.questions[0].answer = Some("New answer".into()),
                _ => edited.title.push('!'),
            }
            assert_eq!(
                action_drafts::validate_save(
                    &world,
                    &save(&world, &edited, phase == "resolution-needed"),
                    Some(&previous)
                )
                .is_ok(),
                phase == "stale"
            );
        }
        if phase == "cancelled" {
            assert!(
                action_drafts::validate_save(&world, &save(&world, &next, false), Some(&next))
                    .is_err()
            );
        }
    }
}

#[test]
fn effective_staleness_checks_every_active_alternative_without_rewriting_history() {
    let (_directory, world) = fixture();
    let mut next = proposed(&world, &ready(&world), 1);
    let original = next.clone();
    assert_eq!(
        action_drafts::effective_phase(&next, &world.fingerprint),
        "proposed"
    );
    assert_eq!(
        action_drafts::effective_phase(&next, &"b".repeat(64)),
        "stale"
    );
    assert_eq!(next, original);
    let mut current = proposal(&world, &next, 2);
    current.context_fingerprint = "b".repeat(64);
    current.consequences.expected_fingerprint = "b".repeat(64);
    next.proposals.push(current);
    assert_eq!(
        action_drafts::active_proposal_ids(&next),
        vec![proposal_id(1), proposal_id(2)]
    );
    assert_eq!(
        action_drafts::effective_phase(&next, &"b".repeat(64)),
        "stale"
    );
    next.proposals[1].supersedes = Some(proposal_id(1));
    assert_eq!(
        action_drafts::active_proposal_ids(&next),
        vec![proposal_id(2)]
    );
    assert_eq!(
        action_drafts::effective_phase(&next, &"b".repeat(64)),
        "proposed"
    );
    assert_ne!(next.base_fingerprint, "b".repeat(64));
    for phase in [
        "draft",
        "context-needed",
        "resolution-needed",
        "stale",
        "cancelled",
    ] {
        next.phase = phase.into();
        assert_eq!(
            action_drafts::effective_phase(&next, &"c".repeat(64)),
            phase
        );
    }
}

#[test]
fn explicit_refresh_preserves_original_base_and_prior_context_and_plan() {
    let (_directory, world) = fixture();
    let original = proposed(&world, &ready(&world), 1);
    let fresh = change_source(&world);
    assert_eq!(
        action_drafts::effective_phase(&original, &fresh.fingerprint),
        "stale"
    );
    let mut previous = original.clone();
    previous.phase = "stale".into();
    let mut next = proposed(&fresh, &previous, 2);
    next.proposals[1].supersedes = Some(proposal_id(1));
    action_drafts::validate_save(&fresh, &save(&fresh, &next, true), Some(&previous)).unwrap();
    assert_eq!(next.base_fingerprint, world.fingerprint);
    assert_eq!(next.proposals[0], original.proposals[0]);
    assert_eq!(next.proposals[1].context_fingerprint, fresh.fingerprint);
    assert_eq!(
        action_drafts::effective_phase(&next, &fresh.fingerprint),
        "proposed"
    );
    let mut rebound = next.clone();
    rebound.base_fingerprint = fresh.fingerprint.clone();
    assert!(
        action_drafts::validate_save(&fresh, &save(&fresh, &rebound, true), Some(&previous))
            .is_err()
    );
}

#[test]
fn historical_plans_are_structural_but_new_append_must_be_constructible() {
    let (_directory, world) = fixture();
    let mut previous = proposed(&world, &ready(&world), 1);
    previous.proposals[0].consequences = consequences::decode(&json!({"expectedFingerprint":world.fingerprint,
        "operations":[{"type":"record.update","entityId":"char_00000000000000000000000009","patch":"kind: character\nbody: Historical target.\n"}]}).to_string()).unwrap();
    previous.phase = "stale".into();
    assert_eq!(decode(&json!(previous)).unwrap(), previous);
    action_drafts::validate_save(&world, &save(&world, &previous, false), Some(&previous)).unwrap();
    let mut next = proposed(&world, &previous, 2);
    next.proposals[1].consequences = previous.proposals[0].consequences.clone();
    assert!(
        action_drafts::validate_save(&world, &save(&world, &next, true), Some(&previous)).is_err()
    );
    next.proposals[1] = proposal(&world, &next, 2);
    action_drafts::validate_save(&world, &save(&world, &next, true), Some(&previous)).unwrap();
}

#[test]
fn diagnostic_invalid_candidate_can_be_saved_as_inert_private_proposal() {
    let (_directory, world) = fixture();
    let previous = ready(&world);
    let mut next = proposed(&world, &previous, 1);
    let draft = format!(
        "kind: event\nid: event_00000000000000000000000005\ntitle: Unresolved proposed event\ntime: {{timeline: main, tick: {TICK}, order: 2}}\ncauses: [event_00000000000000000000000009]\n"
    );
    next.proposals[0].consequences = consequences::decode(
        &json!({"expectedFingerprint":world.fingerprint,
        "operations":[{"type":"record.create","draft":draft}]})
        .to_string(),
    )
    .unwrap();
    let before = source_image(&world);
    let preview =
        consequences::preview(&world, &next.proposals[0].consequences, 1_000_000).unwrap();
    assert_eq!(preview["validation"]["valid"], false);
    action_drafts::validate_save(&world, &save(&world, &next, true), Some(&previous)).unwrap();
    assert_eq!(source_image(&world), before);
    assert!(
        !world
            .records
            .contains_key("event_00000000000000000000000005")
    );
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn proposal_bounds_refuse_whole_history_instead_of_truncating() {
    let (_directory, world) = fixture();
    let mut next = ready(&world);
    next.phase = "proposed".into();
    for n in 0..64 {
        next.proposals.push(proposal(&world, &next, n));
    }
    assert_eq!(decode(&json!(next)).unwrap().proposals.len(), 64);
    let mut excessive = next.clone();
    excessive.proposals.push(proposal(&world, &next, 64));
    assert!(decode(&json!(excessive)).is_err());
    assert!(action_drafts::serialize(&excessive).is_err());
    next.proposals[0].summary = "x".repeat(16_385);
    assert!(decode(&json!(next)).is_err());
    for proposal in &mut next.proposals {
        proposal.summary = "x".repeat(5000);
    }
    assert!(action_drafts::serialize(&next).is_err());
}

#[test]
fn private_proposals_never_enter_admitted_character_review_context() {
    let (_directory, world) = fixture();
    let previous = ready(&world);
    let next = proposed(&world, &previous, 1);
    let request = |draft| ReviewRequest {
        draft,
        frame: frame(&world, "character"),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    };
    let baseline = action_drafts::review(&world, &request(previous)).unwrap();
    let reviewed = action_drafts::review(&world, &request(next)).unwrap();
    assert_eq!(baseline["context"], reviewed["context"]);
    assert_ne!(baseline["draftSha256"], reviewed["draftSha256"]);
    assert_ne!(baseline["reviewSha256"], reviewed["reviewSha256"]);
    let context = reviewed["context"].to_string();
    for private in [
        "Private human alternative",
        "Proposed canonical prose",
        "proposal_",
        "consequences",
    ] {
        assert!(!context.contains(private), "{private}");
    }
}
