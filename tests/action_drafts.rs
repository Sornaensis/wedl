//! Private reasoning never becomes admitted evidence or an outcome authority.
#[path = "support/family_context.rs"]
mod support;
use serde_json::{Value as Json, json};
use support::*;
use wedl::{
    action_drafts::{self, Draft, ReviewConfirmation, ReviewRequest, SaveRequest},
    model::World,
    writing_frames::{self, FamilySelection, Frame, Lens},
};

fn current_world() -> World {
    let mut world = world();
    world.revision = None;
    world
}
fn value(world: &World) -> Json {
    json!({"id":id("action",1),"title":"Private intention","worldId":WORLD,
        "baseFingerprint":world.fingerprint,"horizon":{"timeline":"main","tick":"5","order":"0"},
        "intent":"PRIVATE_INTENT_NOT_EVIDENCE","actorRefs":[{"kind":"record","id":VIEWER}],
        "evidence":[],"targetRefs":[{"kind":"section","ownerId":concept_id(1),"id":id("description",1)}],"hypothesisRefs":[],
        "questions":[{"id":id("question",1),"text":"PRIVATE_QUESTION","required":true}],"proposals":[],
        "phase":"draft","bodyMarkdown":"Private reasoning.\r\nSecond line.\r\n"})
}
fn decode(value: &Json) -> Draft {
    action_drafts::decode(&value.to_string()).unwrap()
}
fn frame(world: &World, lens: Lens) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: Some(SCENE.into()),
        pov_id: Some(VIEWER.into()),
        at: at(1, 0),
        lens,
    }
}
fn review_request(world: &World, draft: Draft, lens: Lens) -> ReviewRequest {
    ReviewRequest {
        draft,
        frame: frame(world, lens),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    }
}
fn save_request(world: &World, draft: Draft) -> SaveRequest {
    SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256: Some("a".repeat(64)),
        draft,
        review: None,
    }
}
fn confirmation(world: &World, draft: Draft, lens: Lens) -> ReviewConfirmation {
    let request = review_request(world, draft, lens);
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

#[test]
fn exact_source_roundtrip_preserves_crlf_and_question_answer_presence() {
    let world = current_world();
    let original = decode(&value(&world));
    let bytes = action_drafts::serialize(&original).unwrap();
    let restored = action_drafts::decode(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(original, restored);
    assert!(bytes.ends_with(b"\n"));
    assert_eq!(
        restored.body_markdown,
        "Private reasoning.\r\nSecond line.\r\n"
    );
    assert!(restored.questions[0].answer.is_none());
    assert!(restored.proposals.is_empty());
    let mut explicit = value(&world);
    explicit["questions"][0]["answer"] = json!("");
    assert_eq!(decode(&explicit).questions[0].answer.as_deref(), Some(""));
    for invalid in [Json::Null, json!(false), json!(0), json!([])] {
        explicit["questions"][0]["answer"] = invalid;
        assert!(action_drafts::decode(&explicit.to_string()).is_err());
    }
}

#[test]
fn exact_time_uses_canonical_strings_over_full_signed_ranges() {
    let world = current_world();
    let mut data = value(&world);
    for (tick, order) in [
        (i64::MIN, i32::MIN),
        (9_007_199_254_740_993, 0),
        (i64::MAX, i32::MAX),
    ] {
        data["horizon"]["tick"] = json!(tick.to_string());
        data["horizon"]["order"] = json!(order.to_string());
        let draft = decode(&data);
        assert_eq!((draft.horizon.tick, draft.horizon.order), (tick, order));
        action_drafts::validate(&world, &draft).unwrap();
    }
    for bad in [
        json!(1),
        json!("01"),
        json!("+1"),
        json!("-0"),
        json!("1.0"),
        json!("9223372036854775808"),
    ] {
        data["horizon"]["tick"] = bad;
        assert!(action_drafts::decode(&data.to_string()).is_err());
    }
}

#[test]
fn duplicate_unknown_missing_and_unsupported_fields_are_rejected() {
    let world = current_world();
    let original = value(&world);
    for key in [
        "proposalHistory",
        "requirements",
        "resolverPolicy",
        "hypothesisLinks",
        "application",
    ] {
        let mut data = original.clone();
        data[key] = json!([]);
        assert!(action_drafts::decode(&data.to_string()).is_err(), "{key}");
    }
    for key in original.as_object().unwrap().keys() {
        let mut data = original.clone();
        data.as_object_mut().unwrap().remove(key);
        assert!(
            action_drafts::decode(&data.to_string()).is_err(),
            "missing {key}"
        );
    }
    let raw = original.to_string();
    for (from, to) in [
        ("\"required\":true", "\"required\":true,\"required\":true"),
        (
            "\"phase\":\"draft\"",
            "\"phase\":\"draft\",\"phase\":\"draft\"",
        ),
    ] {
        assert!(action_drafts::decode(&raw.replace(from, to)).is_err());
    }
    for phase in ["proposed", "approved", "applied", "unknown"] {
        let mut data = original.clone();
        data["phase"] = json!(phase);
        assert!(action_drafts::decode(&data.to_string()).is_err());
    }
}

#[test]
fn actor_and_target_pointers_resolve_actual_unique_owners_and_members() {
    let world = current_world();
    action_drafts::validate(&world, &decode(&value(&world))).unwrap();
    let mut data = value(&world);
    data["actorRefs"] = json!([{"kind":"record","id":PLACE}]);
    assert!(action_drafts::validate(&world, &decode(&data)).is_err());
    data = value(&world);
    data["targetRefs"][0]["ownerId"] = json!(PLACE);
    assert!(action_drafts::validate(&world, &decode(&data)).is_err());
    data = value(&world);
    let duplicate = data["targetRefs"][0].clone();
    data["targetRefs"].as_array_mut().unwrap().push(duplicate);
    assert!(action_drafts::decode(&data.to_string()).is_err());
    data = value(&world);
    data["actorRefs"] = json!([]);
    assert!(action_drafts::decode(&data.to_string()).is_err());
    data = value(&world);
    data["actorRefs"] = data["targetRefs"].clone();
    assert!(action_drafts::decode(&data.to_string()).is_err());
    data = value(&world);
    data["targetRefs"][0]["kind"] = json!("transition");
    assert!(action_drafts::decode(&data.to_string()).is_err());
}

#[test]
fn stale_pointer_documents_remain_decodable_but_cannot_be_resaved() {
    let world = current_world();
    let draft = decode(&value(&world));
    let mut changed = remove(&world, &concept_id(1));
    changed.revision = None;
    // An unresolvable pointer cannot become canonical evidence through a draft.
    assert_eq!(
        action_drafts::decode(
            &String::from_utf8(action_drafts::serialize(&draft).unwrap()).unwrap()
        )
        .unwrap(),
        draft
    );
    assert!(action_drafts::validate(&changed, &draft).is_err());
}

#[test]
fn new_drafts_require_current_original_provenance_and_initial_phase() {
    let world = current_world();
    let draft = decode(&value(&world));
    let request = save_request(&world, draft.clone());
    action_drafts::validate_save(&world, &request, None).unwrap();
    for phase in ["context-needed", "resolution-needed", "stale", "cancelled"] {
        let mut request = request.clone();
        request.draft.phase = phase.into();
        assert!(action_drafts::validate_save(&world, &request, None).is_err());
    }
    let mut request = request.clone();
    request.draft.base_fingerprint = "b".repeat(64);
    assert!(action_drafts::validate_save(&world, &request, None).is_err());
    request = save_request(&world, draft);
    request.expected_fingerprint = "b".repeat(64);
    assert!(action_drafts::validate_save(&world, &request, None).is_err());
}

#[test]
fn identity_and_original_source_are_immutable_after_private_save() {
    let world = current_world();
    let draft = decode(&value(&world));
    for (field, replacement) in [
        ("id", id("action", 2)),
        ("worldId", id("world", 2)),
        ("baseFingerprint", "b".repeat(64)),
    ] {
        let mut data = value(&world);
        data[field] = json!(replacement);
        let request = save_request(&world, decode(&data));
        assert!(action_drafts::validate_save(&world, &request, Some(&draft)).is_err());
    }
    let mut old = draft.clone();
    old.base_fingerprint = "b".repeat(64);
    let mut updated = old.clone();
    updated.intent = "Explicitly revised private intention".into();
    action_drafts::validate_save(&world, &save_request(&world, updated), Some(&old)).unwrap();
}

#[test]
fn bounded_phase_transitions_and_terminal_cancellation_are_enforced() {
    let world = current_world();
    let phases = [
        "draft",
        "context-needed",
        "resolution-needed",
        "stale",
        "cancelled",
    ];
    for old in phases {
        for new in phases {
            if new == "resolution-needed" {
                continue;
            }
            let mut previous = decode(&value(&world));
            previous.phase = old.into();
            let mut next = previous.clone();
            next.phase = new.into();
            let expected = match old {
                "draft" => true,
                "context-needed" => new != "draft",
                "resolution-needed" => ["stale", "cancelled"].contains(&new),
                "stale" => ["stale", "context-needed", "cancelled"].contains(&new),
                _ => false,
            };
            assert_eq!(
                action_drafts::validate_save(&world, &save_request(&world, next), Some(&previous))
                    .is_ok(),
                expected,
                "{old} -> {new}"
            );
        }
    }
}

#[test]
fn review_uses_saved_action_horizon_without_changing_selected_frame_or_source() {
    let world = current_world();
    let draft = decode(&value(&world));
    let request = review_request(&world, draft, Lens::Character);
    assert_eq!(request.frame.at.tick, 1);
    let before = world.fingerprint.clone();
    let reviewed = action_drafts::review(&world, &request).unwrap();
    assert_eq!(reviewed["frame"]["at"]["tick"], "5");
    let mut actual_frame = request.frame.clone();
    actual_frame.at = at(5, 0);
    assert_eq!(
        reviewed["context"],
        writing_frames::context(&world, None, &actual_frame, &[], 1_000_000).unwrap()
    );
    assert_eq!(request.frame.at.tick, 1);
    assert_eq!(world.fingerprint, before);
    assert!(!reviewed.to_string().contains("PRIVATE_INTENT_NOT_EVIDENCE"));
    assert!(!reviewed.to_string().contains("PRIVATE_QUESTION"));
    assert!(!reviewed.to_string().contains("Private reasoning"));
}

#[test]
fn saved_horizon_requires_actual_scene_presence_and_current_exact_context() {
    let world = current_world();
    let mut request = review_request(&world, decode(&value(&world)), Lens::Character);
    request.draft.horizon = at(21, 0);
    assert!(action_drafts::review(&world, &request).is_err());
    request.draft.horizon = at(5, 0);
    request.frame.pov_id = Some(OTHER.into());
    assert!(action_drafts::review(&world, &request).is_err());
    request.frame = frame(&world, Lens::Character);
    request.frame.fingerprint = "b".repeat(64);
    assert!(action_drafts::review(&world, &request).is_err());
    request.frame = frame(&world, Lens::Author);
    request.frame.scene_id = None;
    assert!(action_drafts::review(&world, &request).is_err());
}

#[test]
fn dramatic_review_keeps_character_prompt_equal_and_author_margin_separate() {
    let world = current_world();
    let draft = decode(&value(&world));
    let character = action_drafts::review(
        &world,
        &review_request(&world, draft.clone(), Lens::Character),
    )
    .unwrap();
    let dramatic =
        action_drafts::review(&world, &review_request(&world, draft, Lens::DramaticIrony)).unwrap();
    assert_eq!(
        dramatic["context"]["characterPrompt"],
        character["context"]["characterPrompt"]
    );
    assert!(character["context"].get("authorMargin").is_none());
    assert!(dramatic["context"].get("authorMargin").is_some());
}

#[test]
fn required_question_readiness_is_separate_from_explicit_human_sufficiency() {
    let world = current_world();
    let previous = decode(&value(&world));
    let mut next = previous.clone();
    next.phase = "resolution-needed".into();
    let reviewed =
        action_drafts::review(&world, &review_request(&world, next.clone(), Lens::Author)).unwrap();
    assert_eq!(reviewed["requiredAnswered"], false);
    assert_eq!(reviewed["unansweredRequired"], json!([id("question", 1)]));
    let mut request = save_request(&world, next.clone());
    request.review = Some(confirmation(&world, next, Lens::Author));
    assert!(action_drafts::validate_save(&world, &request, Some(&previous)).is_err());
    request.draft.questions[0].answer = Some(String::new());
    request.review = None;
    assert!(action_drafts::validate_save(&world, &request, Some(&previous)).is_err());
    request.review = Some(confirmation(&world, request.draft.clone(), Lens::Author));
    request.review.as_mut().unwrap().sufficient = false;
    assert!(action_drafts::validate_save(&world, &request, Some(&previous)).is_err());
    request.review.as_mut().unwrap().sufficient = true;
    action_drafts::validate_save(&world, &request, Some(&previous)).unwrap();
}

#[test]
fn every_resolution_save_binds_exact_reviewed_draft_and_context() {
    let world = current_world();
    let previous = decode(&value(&world));
    let mut next = previous.clone();
    next.phase = "resolution-needed".into();
    next.questions[0].answer = Some("".into());
    let mut request = save_request(&world, next.clone());
    request.review = Some(confirmation(&world, next.clone(), Lens::Author));
    action_drafts::validate_save(&world, &request, Some(&previous)).unwrap();
    for change in 0..7 {
        let mut changed = request.clone();
        match change {
            0 => changed.draft.intent.push('!'),
            1 => changed.draft.body_markdown.push('!'),
            2 => changed.draft.questions[0].answer = Some("Changed".into()),
            3 => changed.draft.questions[0].text.push('!'),
            4 => changed.review.as_mut().unwrap().frame.lens = Lens::Character,
            5 => changed.review.as_mut().unwrap().max_characters = 99_999,
            _ => changed.draft.horizon = at(6, 0),
        }
        assert!(
            action_drafts::validate_save(&world, &changed, Some(&previous)).is_err(),
            "{change}"
        );
    }
    let mut no_confirmation = save_request(&world, next.clone());
    no_confirmation.review = None;
    assert!(action_drafts::validate_save(&world, &no_confirmation, Some(&next)).is_err());
    let mut wrong_phase = request.clone();
    wrong_phase.draft.phase = "stale".into();
    assert!(action_drafts::validate_save(&world, &wrong_phase, Some(&previous)).is_err());
}

#[test]
fn full_review_limit_is_complete_unicode_scalar_count_and_never_truncates() {
    let world = current_world();
    let mut request = review_request(&world, decode(&value(&world)), Lens::Character);
    request.max_characters = 256;
    assert!(action_drafts::review(&world, &request).is_err());
    request.max_characters = 100_001;
    assert!(action_drafts::review(&world, &request).is_err());
    request.max_characters = 100_000;
    let result = action_drafts::review(&world, &request).unwrap();
    let mut limit = result.to_string().chars().count();
    // The decimal budget field changes the exact packet length itself.
    loop {
        request.max_characters = limit;
        match action_drafts::review(&world, &request) {
            Ok(result) => {
                let needed = result.to_string().chars().count();
                if needed == limit {
                    break;
                }
                limit = needed;
            }
            Err(_) => {
                limit += 1;
            }
        }
    }
    request.max_characters -= 1;
    assert!(action_drafts::review(&world, &request).is_err());
}

#[test]
fn private_review_and_saves_refuse_historical_worlds_and_invalid_source() {
    let mut world = current_world();
    let draft = decode(&value(&world));
    world.revision = Some("a".repeat(40));
    assert!(
        action_drafts::review(&world, &review_request(&world, draft.clone(), Lens::Author))
            .is_err()
    );
    assert!(
        action_drafts::validate_save(&world, &save_request(&world, draft.clone()), None).is_err()
    );
    world.revision = None;
    world.records.remove(WORLD);
    assert!(action_drafts::validate(&world, &draft).is_err());
}

#[test]
fn save_and_review_requests_are_closed_at_every_layer() {
    let world = current_world();
    let draft = decode(&value(&world));
    let request = save_request(&world, draft.clone());
    let original = serde_json::to_value(&request).unwrap();
    action_drafts::decode_save(&original.to_string()).unwrap();
    for key in original.as_object().unwrap().keys() {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(action_drafts::decode_save(&changed.to_string()).is_err());
    }
    let mut changed = original.clone();
    changed["requirements"] = json!([]);
    assert!(action_drafts::decode_save(&changed.to_string()).is_err());
    let request = review_request(&world, draft, Lens::Author);
    let original = serde_json::to_value(&request).unwrap();
    action_drafts::decode_review(&original.to_string()).unwrap();
    for (key, bad) in [
        ("maxCharacters", json!(100000.0)),
        ("briefs", Json::Null),
        ("families", json!({"magic":null})),
        ("frame", json!({})),
    ] {
        let mut changed = original.clone();
        changed[key] = bad;
        assert!(action_drafts::decode_review(&changed.to_string()).is_err());
    }
}

#[test]
fn bounded_documents_refuse_excess_questions_text_references_and_depth() {
    let world = current_world();
    let original = value(&world);
    let mut changed = original.clone();
    changed["bodyMarkdown"] = json!("x".repeat(131_073));
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    changed = original.clone();
    changed["questions"] = json!(
        (0..129)
            .map(|n| json!({"id":id("question",n),"text":"Q","required":false}))
            .collect::<Vec<_>>()
    );
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    changed = original.clone();
    changed["targetRefs"] = json!(
        (0..1025)
            .map(|n| json!({"kind":"record","id":id("char",n)}))
            .collect::<Vec<_>>()
    );
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    changed = original;
    changed["title"] = json!(" ");
    assert!(action_drafts::decode(&changed.to_string()).is_err());
    assert!(action_drafts::decode(&format!("{}0{}", "[".repeat(40), "]".repeat(40))).is_err());
}

#[test]
fn author_review_remains_operational_while_character_lenses_require_actual_presence() {
    let world = current_world();
    for absent_by_time in [true, false] {
        let mut request = review_request(&world, decode(&value(&world)), Lens::Author);
        if absent_by_time {
            request.draft.horizon = at(21, 0);
        } else {
            request.frame.pov_id = Some(OTHER.into());
        }
        let author = action_drafts::review(&world, &request).unwrap();
        assert!(author["context"].get("authorMargin").is_some());
        assert!(author["context"].get("characterPrompt").is_none());
        for lens in [Lens::Character, Lens::DramaticIrony] {
            request.frame.lens = lens;
            assert!(action_drafts::review(&world, &request).is_err());
        }
    }
}

#[test]
fn exact_review_binding_covers_equivalent_rendered_selections_and_refreshed_source() {
    let world = current_world();
    let previous = decode(&value(&world));
    let mut next = previous.clone();
    next.phase = "resolution-needed".into();
    next.questions[0].answer = Some(String::new());
    let mut selected = review_request(&world, next.clone(), Lens::Author);
    selected.briefs = vec![id("brief", 80)];
    selected.families = writing_frames::decode_families(
        &json!({"magic":{"author":{"rootId":concept_id(30),"selection":magic_selection()},"beliefTargets":[]}}).to_string(),
    ).unwrap();
    let baseline = action_drafts::review(&world, &selected).unwrap();
    let confirmation = ReviewConfirmation {
        frame: selected.frame.clone(),
        briefs: selected.briefs.clone(),
        families: selected.families.clone(),
        max_characters: selected.max_characters,
        review_sha256: baseline["reviewSha256"].as_str().unwrap().into(),
        sufficient: true,
    };
    let mut save = save_request(&world, next);
    save.review = Some(confirmation.clone());
    action_drafts::validate_save(&world, &save, Some(&previous)).unwrap();

    for duplicate_brief in [true, false] {
        let mut changed = selected.clone();
        if duplicate_brief {
            changed.briefs.push(id("brief", 80));
        } else {
            // Author context does not read the independently selected learned
            // targets, but the exact request still belongs to this review.
            changed.families.magic.as_mut().unwrap().belief_targets = vec![id("assertion", 999)];
        }
        let reviewed = action_drafts::review(&world, &changed).unwrap();
        assert_eq!(reviewed["context"], baseline["context"]);
        assert_ne!(reviewed["reviewSha256"], baseline["reviewSha256"]);
        let mut changed_save = save.clone();
        let captured = changed_save.review.as_mut().unwrap();
        captured.briefs = changed.briefs;
        captured.families = changed.families;
        assert!(matches!(
            action_drafts::validate_save(&world, &changed_save, Some(&previous)),
            Err(wedl::Error::Conflict(_))
        ));
    }

    let mut fresh = set(
        &world,
        WORLD,
        "title",
        json!("Explicitly changed canonical source"),
    );
    fresh.revision = None;
    save.expected_fingerprint = fresh.fingerprint.clone();
    save.review.as_mut().unwrap().frame.fingerprint = fresh.fingerprint.clone();
    assert!(matches!(
        action_drafts::validate_save(&fresh, &save, Some(&previous)),
        Err(wedl::Error::Conflict(_))
    ));
    // A freshly computed explicit review can retain the immutable original base.
    let confirmation = save.review.as_mut().unwrap();
    let reviewed = action_drafts::review(
        &fresh,
        &ReviewRequest {
            draft: save.draft.clone(),
            frame: confirmation.frame.clone(),
            briefs: confirmation.briefs.clone(),
            families: confirmation.families.clone(),
            max_characters: confirmation.max_characters,
        },
    )
    .unwrap();
    confirmation.review_sha256 = reviewed["reviewSha256"].as_str().unwrap().into();
    action_drafts::validate_save(&fresh, &save, Some(&previous)).unwrap();
    assert_eq!(save.draft.base_fingerprint, world.fingerprint);
}
