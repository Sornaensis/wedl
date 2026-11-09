//! Explicit conversation intents retain exact source, clocks and admission.
#[path = "support/conversation_control.rs"]
mod support;
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use support::*;
use wedl::{
    Error, character, consequences, conversation_control, model::World, runtime, scene, validation,
};

fn request(value: &Json) -> conversation_control::Request {
    conversation_control::decode(&value.to_string()).unwrap()
}

fn preview(world: &World, value: &Json) -> Json {
    conversation_control::preview(world, &request(value), 100_000).unwrap()
}

fn valid(world: &World) {
    let value = validation::validate_world(world);
    assert!(value.valid, "{:?}", value.diagnostics);
}

#[test]
fn create_reconciles_one_scene_link_at_future_exact_time_without_moving_clocks() {
    let (_directory, world) = fixture();
    let before = source_image(&world);
    assert!(!scene::present(&world, &world.records[SCENE], THIRD, &at(ORDER + 1)).unwrap());
    let input = create(&world);
    let report = preview(&world, &input);
    let proposed = candidate(&world, &report);
    valid(&proposed);
    assert_eq!(proposed.records.len(), world.records.len() + 1);
    assert_eq!(
        proposed.current_time().unwrap(),
        world.current_time().unwrap()
    );
    let created = &proposed.records[NEW_CONVERSATION];
    assert_eq!(created.status(), "active");
    assert_eq!(created.frontmatter["scene"].as_str(), Some(SCENE));
    assert_eq!(created.frontmatter["location"].as_str(), Some(ORIGIN));
    assert_eq!(
        created.frontmatter["time"]["start"],
        serde_yaml::to_value(point(TICK, ORDER + 1)).unwrap()
    );
    assert_eq!(
        created.frontmatter["participants"][0]["character"].as_str(),
        Some(THIRD)
    );
    assert_eq!(
        created.frontmatter["participants"][0]["from"],
        created.frontmatter["time"]["start"]
    );
    assert_eq!(created.list("turns").unwrap().len(), 0);
    assert_eq!(created.list("recollections").unwrap().len(), 0);
    assert_eq!(
        created.body,
        format!("# {}\n", input["action"]["title"].as_str().unwrap())
    );
    let selected = &proposed.records[SCENE];
    assert_eq!(
        selected.frontmatter["conversations"],
        serde_yaml::to_value(json!([CONVERSATION, NEW_CONVERSATION])).unwrap()
    );
    assert_eq!(selected.body, world.records[SCENE].body);
    for key in [
        "time",
        "participants",
        "objects",
        "observations",
        "x-author",
    ] {
        assert_eq!(selected.get(key), world.records[SCENE].get(key), "{key}");
    }
    for (id, record) in &world.records {
        if id != SCENE {
            assert_eq!(proposed.records[id].raw_bytes, record.raw_bytes, "{id}");
        }
    }
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(report["canApply"], false);
    assert_eq!(report["preview"]["validation"]["valid"], true);
    assert_eq!(source_image(&world), before);
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn create_admits_inclusive_authored_presence_without_admitting_other_scene_cast() {
    let (_directory, world) = fixture();
    let window = changed(&world, SCENE, |fields| {
        fields["time"]["end"] = serde_yaml::to_value(point(TICK, ORDER + 2)).unwrap();
        fields["participants"][1]["from"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap();
        fields["participants"][1]["until"] = serde_yaml::to_value(point(TICK, ORDER + 2)).unwrap();
    });
    valid(&window);
    for order in [ORDER, ORDER + 2] {
        let mut input = create(&window);
        input["at"] = json!(at(order));
        assert!(conversation_control::plan(&window, &request(&input)).is_ok());
    }
    for (order, cast) in [(ORDER + 3, vec![THIRD]), (ORDER, vec![OTHER_ACTOR])] {
        let mut input = create(&window);
        input["at"] = json!(at(order));
        input["action"]["characterIds"] = json!(cast);
        assert!(conversation_control::plan(&window, &request(&input)).is_err());
    }
}

#[test]
fn create_refuses_stale_existing_inactive_backward_and_cross_timeline_requests() {
    let (_directory, world) = fixture();
    let before = source_image(&world);
    let mut stale = create(&world);
    stale["expectedFingerprint"] = "f".repeat(64).into();
    assert!(matches!(
        conversation_control::plan(&world, &request(&stale)),
        Err(Error::Conflict(_))
    ));
    let mut reused = create(&world);
    reused["action"]["conversationId"] = CONVERSATION.into();
    assert!(matches!(
        conversation_control::plan(&world, &request(&reused)),
        Err(Error::Conflict(_))
    ));
    for mode in ["backward", "other", "unknown-scene", "excluded"] {
        let mut input = create(&world);
        match mode {
            "backward" => input["at"] = json!(at(ORDER - 1)),
            "other" => input["at"]["timeline"] = "other".into(),
            "unknown-scene" => {
                input["action"]["sceneId"] = "scene_00000000000000000000000099".into()
            }
            _ => input["action"]["characterIds"] = json!([OTHER_ACTOR]),
        }
        assert!(
            conversation_control::plan(&world, &request(&input)).is_err(),
            "{mode}"
        );
    }
    for status in ["planned", "closed", "abandoned"] {
        let inactive = changed(&world, SCENE, |fields| {
            fields["status"] = status.into();
            fields["time"]["end"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap();
        });
        valid(&inactive);
        assert!(conversation_control::plan(&inactive, &request(&create(&inactive))).is_err());
    }
    assert_eq!(source_image(&world), before);
}

#[test]
fn speech_and_action_append_preserve_existing_turns_memory_body_and_all_other_source() {
    let (_directory, world) = fixture();
    let image = source_image(&world);
    for input in [speech(&world), action(&world)] {
        let report = preview(&world, &input);
        let proposed = candidate(&world, &report);
        valid(&proposed);
        let old = &world.records[CONVERSATION];
        let new = &proposed.records[CONVERSATION];
        assert_eq!(&new.list("turns").unwrap()[..2], old.list("turns").unwrap());
        assert_eq!(new.body, old.body);
        for key in [
            "time",
            "participants",
            "recollections",
            "scene",
            "location",
            "x-author",
        ] {
            assert_eq!(new.get(key), old.get(key), "{key}");
        }
        let added = &new.list("turns").unwrap()[2];
        assert_eq!(added["text"].as_str(), input["action"]["text"].as_str());
        assert_eq!(
            added["at"],
            serde_yaml::to_value(point(TICK, ORDER + 1)).unwrap()
        );
        assert_eq!(
            added["audience"],
            serde_yaml::to_value(json!(["participants"])).unwrap()
        );
        assert_eq!(proposed.records.len(), world.records.len());
        for (id, record) in &world.records {
            if id != CONVERSATION {
                assert_eq!(record.raw_bytes, proposed.records[id].raw_bytes, "{id}");
            }
        }
        let before_view = runtime::conversation(&proposed, CONVERSATION, &at(ORDER)).unwrap();
        let at_view = runtime::conversation(&proposed, CONVERSATION, &at(ORDER + 1)).unwrap();
        assert_eq!(before_view["beats"].as_array().unwrap().len(), 2);
        assert_eq!(at_view["beats"].as_array().unwrap().len(), 3);
        assert!(
            character::conversation(&proposed, OTHER_ACTOR, CONVERSATION, &at(ORDER + 1)).is_err()
        );
        assert_eq!(
            runtime::knowledge(&world, ACTOR, &at(ORDER + 1), false).unwrap()["items"],
            runtime::knowledge(&proposed, ACTOR, &at(ORDER + 1), false).unwrap()["items"]
        );
        assert_eq!(source_image(&world), image);
    }
}

#[test]
fn interrupt_last_selects_earlier_omitted_kind_speech_and_retains_only_admitted_targets() {
    let (_directory, original) = fixture();
    let world = changed(&original, CONVERSATION, |fields| {
        fields["turns"][0]["audience"] = serde_yaml::to_value(json!(["author"])).unwrap();
    });
    valid(&world);
    let mut input = speech(&world);
    input["action"]["speakerId"] = THIRD.into();
    input["action"]
        .as_object_mut()
        .unwrap()
        .remove("addresseeId");
    input["action"]["interruptLast"] = true.into();
    let proposed = candidate(&world, &preview(&world, &input));
    assert_eq!(
        proposed.records[CONVERSATION].frontmatter["turns"][2]["interrupts"].as_str(),
        Some("original-speech")
    );
    let author = runtime::conversation(&proposed, CONVERSATION, &at(ORDER + 1)).unwrap();
    assert_eq!(author["beats"][2]["interrupts"], "original-speech");
    let listener = character::conversation(&proposed, THIRD, CONVERSATION, &at(ORDER + 1)).unwrap();
    let last = listener["beats"].as_array().unwrap().last().unwrap();
    assert!(last.get("interrupts").is_none());
    assert!(!listener.to_string().contains("original-speech"));
    let speaker = character::conversation(&proposed, ACTOR, CONVERSATION, &at(ORDER + 1)).unwrap();
    assert_eq!(
        speaker["beats"].as_array().unwrap().last().unwrap()["interrupts"],
        "original-speech"
    );
}

#[test]
fn interruption_uses_latest_speech_and_refuses_a_conversation_without_one() {
    let (_directory, world) = fixture();
    let first = candidate(&world, &preview(&world, &speech(&world)));
    let mut beat = action(&first);
    beat["at"] = json!(at(ORDER + 2));
    let second = candidate(&first, &preview(&first, &beat));
    let mut interrupt = speech(&second);
    interrupt["at"] = json!(at(ORDER + 3));
    interrupt["action"]["turnId"] = "latest-interruption".into();
    interrupt["action"]["interruptLast"] = true.into();
    let third = candidate(&second, &preview(&second, &interrupt));
    assert_eq!(
        third.records[CONVERSATION]
            .list("turns")
            .unwrap()
            .last()
            .unwrap()["interrupts"]
            .as_str(),
        Some("new-speech")
    );
    let no_speech = changed(&world, CONVERSATION, |fields| {
        fields["turns"].as_sequence_mut().unwrap().remove(0);
        fields["recollections"] = Yaml::Sequence(vec![]);
    });
    valid(&no_speech);
    let mut input = speech(&no_speech);
    input["action"]["interruptLast"] = true.into();
    assert!(conversation_control::plan(&no_speech, &request(&input)).is_err());
}

#[test]
fn explicit_append_before_a_later_shared_cursor_does_not_rewind_or_advance_it() {
    let (_directory, original) = fixture();
    let mut world = changed(&original, WORLD, |fields| {
        fields["current_time"] = serde_yaml::to_value(point(TICK, ORDER + 5)).unwrap();
    });
    for id in [SCENE, OTHER_SCENE] {
        world = changed(&world, id, |fields| {
            fields["time"]["current"] = serde_yaml::to_value(point(TICK, ORDER + 5)).unwrap();
        });
    }
    valid(&world);
    let proposed = candidate(&world, &preview(&world, &speech(&world)));
    assert_eq!(proposed.current_time().unwrap(), at(ORDER + 5));
    for id in [WORLD, SCENE, OTHER_SCENE] {
        assert_eq!(world.records[id].raw_bytes, proposed.records[id].raw_bytes);
    }
}

#[test]
fn append_presence_is_inclusive_for_speaker_addressee_and_action_actors() {
    let (_directory, original) = fixture();
    let world = changed(&original, CONVERSATION, |fields| {
        fields["participants"][1]["from"] = serde_yaml::to_value(point(TICK, ORDER + 1)).unwrap();
        fields["participants"][1]["until"] = serde_yaml::to_value(point(TICK, ORDER + 2)).unwrap();
    });
    valid(&world);
    for order in [ORDER, ORDER + 1, ORDER + 2, ORDER + 3] {
        for mut input in [speech(&world), action(&world)] {
            input["at"] = json!(at(order));
            assert_eq!(
                conversation_control::plan(&world, &request(&input)).is_ok(),
                [ORDER + 1, ORDER + 2].contains(&order),
                "{input}"
            );
        }
        let mut input = speech(&world);
        input["at"] = json!(at(order));
        input["action"]["speakerId"] = THIRD.into();
        input["action"]
            .as_object_mut()
            .unwrap()
            .remove("addresseeId");
        assert_eq!(
            conversation_control::plan(&world, &request(&input)).is_ok(),
            [ORDER + 1, ORDER + 2].contains(&order)
        );
    }
}

#[test]
fn append_refuses_duplicate_or_reordered_beats_inactive_records_and_nonparticipants() {
    let (_directory, world) = fixture();
    for mode in [
        "duplicate",
        "earlier",
        "equal",
        "other",
        "speaker",
        "addressee",
        "actor",
    ] {
        let mut input = if mode == "actor" {
            action(&world)
        } else {
            speech(&world)
        };
        match mode {
            "duplicate" => input["action"]["turnId"] = "original-speech".into(),
            "earlier" => input["at"] = json!(at(ORDER - 2)),
            "equal" => input["at"] = json!(at(ORDER - 1)),
            "other" => input["at"]["timeline"] = "other".into(),
            "speaker" => input["action"]["speakerId"] = OTHER_ACTOR.into(),
            "addressee" => input["action"]["addresseeId"] = OTHER_ACTOR.into(),
            _ => input["action"]["actorIds"] = json!([OTHER_ACTOR]),
        }
        assert!(
            conversation_control::plan(&world, &request(&input)).is_err(),
            "{mode}"
        );
    }
    for status in ["draft", "closed", "retired"] {
        let inactive = changed(&world, CONVERSATION, |fields| {
            fields["status"] = status.into();
            fields["time"]["end"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap();
        });
        valid(&inactive);
        assert!(conversation_control::plan(&inactive, &request(&speech(&inactive))).is_err());
    }
    let unsorted = changed(&world, CONVERSATION, |fields| {
        fields["turns"].as_sequence_mut().unwrap().swap(0, 1);
    });
    assert!(conversation_control::plan(&unsorted, &request(&speech(&unsorted))).is_err());
}

#[test]
fn complete_preview_replays_exported_plan_and_refuses_small_or_invalid_budgets() {
    let (_directory, world) = fixture();
    let image = source_image(&world);
    for input in [create(&world), speech(&world), action(&world)] {
        let req = request(&input);
        let report = preview(&world, &input);
        let exported = consequences::decode(&report["plan"].to_string()).unwrap();
        assert_eq!(
            serde_json::to_value(conversation_control::plan(&world, &req).unwrap()).unwrap(),
            report["plan"]
        );
        assert_eq!(
            consequences::preview(&world, &exported, 100_000).unwrap(),
            report["preview"]
        );
        for limit in [255, 256, 1_000_001] {
            assert!(conversation_control::preview(&world, &req, limit).is_err());
        }
    }
    assert_eq!(source_image(&world), image);
}
