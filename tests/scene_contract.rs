use serde_json::json;
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    model::{Record, StoryTime, World},
    scene, source,
};

fn id(kind: &str, number: u8) -> String {
    let prefix = match kind {
        "character" => "char",
        "location" => "loc",
        "conversation" => "conv",
        _ => kind,
    };
    format!("{prefix}_{number:026}")
}
fn record(kind: &str, number: u8, extra: &str) -> Record {
    source::parse_record(format!("---\nschema: {}\nkind: {kind}\nid: {}\ntitle: {kind} {number}\n{extra}---\nUNTIMED_SECRET\n", source::SOURCE_SCHEMA, id(kind,number)).as_bytes(), &format!("story/{kind}-{number}.md")).unwrap()
}
fn world(extra: Vec<Record>) -> World {
    let mut records = vec![
        record(
            "world",
            1,
            "capabilities: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: alternate, label: Alternate}]\ncurrent_time: {timeline: main, tick: 20, order: 0}\n",
        ),
        record("location", 1, ""),
        record("location", 2, ""),
    ];
    records.extend((1..=3).map(|n| record("character", n, "initial_state: {}\n")));
    records.extend(extra);
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn scene_record(number: u8) -> Record {
    record(
        "scene",
        number,
        &format!(
            "status: closed\nlocation: {}\ntime: {{start: {{tick: 0}}, current: {{tick: 20}}, end: {{tick: 20}}}}\nparticipants:\n- {{character: {}, from: {{tick: 0}}, until: {{tick: 10}}}}\n- {{character: {}, from: {{tick: 5}}, until: {{tick: 20}}}}\nobservations: []\n",
            id("location", 1),
            id("character", 1),
            id("character", 2)
        ),
    )
}
fn conversation_record() -> Record {
    record(
        "conversation",
        1,
        &format!(
            "status: closed\nlocation: {}\ntime: {{start: {{tick: 0}}, end: {{tick: 20}}}}\nparticipants:\n- {{character: {}, from: {{tick: 0}}, until: {{tick: 10}}}}\n- {{character: {}, from: {{tick: 5}}, until: {{tick: 20}}}}\nturns: []\nrecollections: []\n",
            id("location", 1),
            id("character", 1),
            id("character", 2)
        ),
    )
}
fn edited(base: &World, target: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let original = &base.records[target];
    let mut value = original.frontmatter.clone();
    edit(&mut value);
    let bytes = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&value).unwrap(),
        original.body
    );
    let replacement = source::parse_record(bytes.as_bytes(), &original.source_path).unwrap();
    let records = base
        .records
        .values()
        .map(|record| {
            if record.id == target {
                replacement.clone()
            } else {
                record.clone()
            }
        })
        .collect();
    source::world_from_records(base.root.clone(), records, None).unwrap()
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn valid(world: &World) {
    assert!(
        scene::validate(world).is_empty(),
        "{:#?}",
        scene::validate(world)
    );
}
fn invalid(world: &World) {
    assert!(!scene::validate(world).is_empty());
}
fn full_valid(world: &World) {
    let report = wedl::validation::validate_world(world);
    assert!(report.valid, "{:#?}", report.diagnostics);
}
fn tag(value: Yaml) -> Yaml {
    Yaml::Tagged(Box::new(serde_yaml::value::TaggedValue {
        tag: serde_yaml::value::Tag::new("custom"),
        value,
    }))
}
fn tag_rejected(world: &World) {
    invalid(world);
    assert!(!wedl::validation::validate_world(world).valid);
}
fn beat(speaker: u8, tick: i64, audience: &str) -> Yaml {
    yaml(&format!(
        "{{id: beat-one, at: {{tick: {tick}}}, speaker: {}, text: Hello, audience: {audience}}}",
        id("character", speaker)
    ))
}

#[test]
fn inclusive_presence_uses_exact_order_and_optional_bound_inheritance() {
    let base = world(vec![scene_record(1)]);
    valid(&base);
    let scene = &base.records[&id("scene", 1)];
    let alice = id("character", 1);
    assert!(scene::present(&base, scene, &alice, &at(0, 0)).unwrap());
    assert!(scene::present(&base, scene, &alice, &at(10, 0)).unwrap());
    assert!(!scene::present(&base, scene, &alice, &at(10, 1)).unwrap());
    assert!(
        !scene::present(
            &base,
            scene,
            &alice,
            &StoryTime::new("alternate", 5, 0).unwrap()
        )
        .unwrap()
    );
    let inherited = edited(&base, &scene.id, |v| {
        v["participants"] = yaml(&format!(
            "[{{character: {}, from: null, until: null}}]",
            alice
        ));
    });
    valid(&inherited);
    assert!(
        scene::present(
            &inherited,
            &inherited.records[&scene.id],
            &alice,
            &at(20, 0)
        )
        .unwrap()
    );
    assert!(
        !scene::present(
            &inherited,
            &inherited.records[&scene.id],
            &alice,
            &at(20, 1)
        )
        .unwrap()
    );
}

#[test]
fn open_active_scene_presence_stops_at_cursor_while_open_conversation_stays_open() {
    let base = world(vec![scene_record(1), conversation_record()]);
    let scene_id = id("scene", 1);
    let conv_id = id("conversation", 1);
    let alice = id("character", 1);
    let open = edited(&base, &scene_id, |v| {
        v["status"] = yaml("active");
        v["time"] = yaml("{start: {tick: 0}, current: {tick: 10}, end: null}");
        v["participants"] = yaml(&format!("[{{character: {alice}}}]"));
    });
    let open = edited(&open, &conv_id, |v| {
        v["status"] = yaml("active");
        v["time"]["end"] = Yaml::Null;
        v["participants"] = yaml(&format!("[{{character: {alice}}}]"));
    });
    valid(&open);
    assert!(scene::present(&open, &open.records[&scene_id], &alice, &at(10, 0)).unwrap());
    assert!(!scene::present(&open, &open.records[&scene_id], &alice, &at(10, 1)).unwrap());
    assert!(scene::present(&open, &open.records[&conv_id], &alice, &at(100, 0)).unwrap());
    let explicit = edited(&open, &scene_id, |v| {
        v["time"]["end"] = yaml("{tick: 15}");
    });
    valid(&explicit);
    assert!(scene::present(&explicit, &explicit.records[&scene_id], &alice, &at(15, 0)).unwrap());
}

#[test]
fn malformed_intervals_old_bound_name_and_duplicate_participants_are_rejected() {
    let base = world(vec![scene_record(1)]);
    let sid = id("scene", 1);
    for interval in [
        "{start: {tick: 0}, current: {tick: 21}, end: {tick: 20}}",
        "{start: {tick: 0}, current: {tick: 20}}",
        "{start: {tick: 0}, current: {tick: 10}, end: {timeline: alternate, tick: 20}}",
    ] {
        invalid(&edited(&base, &sid, |v| v["time"] = yaml(interval)));
    }
    for member in [
        format!("{{character: {}, to: {{tick: 10}}}}", id("character", 1)),
        format!(
            "{{character: {}, from: {{tick: 11}}, until: {{tick: 10}}}}",
            id("character", 1)
        ),
        format!("{{character: {}, from: {{tick: -1}}}}", id("character", 1)),
    ] {
        invalid(&edited(&base, &sid, |v| {
            v["participants"] = yaml(&format!("[{member}]"))
        }));
    }
    invalid(&edited(&base, &sid, |v| {
        let member = v["participants"][0].clone();
        v["participants"].as_sequence_mut().unwrap().push(member);
    }));
}

#[test]
fn scene_overlap_is_inclusive_and_ignores_planned_records() {
    let base = world(vec![scene_record(1), scene_record(2)]);
    assert!(
        scene::validate(&base)
            .iter()
            .any(|d| d.code == "WDL-SCENE-002")
    );
    let first = edited(&base, &id("scene", 1), |v| {
        v["time"] = yaml("{start: {tick: 0}, current: {tick: 10}, end: {tick: 10}}");
        v["participants"] = yaml(&format!("[{{character: {}}}]", id("character", 1)));
    });
    let touching = edited(&first, &id("scene", 2), |v| {
        v["time"] = yaml("{start: {tick: 10}, current: {tick: 20}, end: {tick: 20}}");
        v["participants"] = yaml(&format!("[{{character: {}}}]", id("character", 1)));
    });
    invalid(&touching);
    let separate = edited(&touching, &id("scene", 2), |v| {
        v["time"]["start"] = yaml("{tick: 10, order: 1}")
    });
    valid(&separate);
    let planned = edited(&touching, &id("scene", 2), |v| {
        v["status"] = yaml("planned")
    });
    valid(&planned);
    assert!(
        !scene::present(
            &planned,
            &planned.records[&id("scene", 2)],
            &id("character", 1),
            &at(15, 0)
        )
        .unwrap()
    );
}

#[test]
fn audibility_requires_origin_membership_and_never_defaults_empty_audience_to_public() {
    let base = world(vec![conversation_record()]);
    let conv = &base.records[&id("conversation", 1)];
    let alice = id("character", 1);
    let bob = id("character", 2);
    let outsider = id("character", 3);
    for audience in ["[public]", "[participants]"] {
        let early = beat(1, 3, audience);
        assert!(scene::audible(&base, conv, &early, &alice).unwrap());
        assert!(!scene::audible(&base, conv, &early, &bob).unwrap());
        assert!(!scene::audible(&base, conv, &early, &outsider).unwrap());
        assert!(scene::audible(&base, conv, &beat(1, 6, audience), &bob).unwrap());
    }
    for audience in ["[]", "[author]", "null"] {
        let private = beat(1, 6, audience);
        assert!(scene::audible(&base, conv, &private, &alice).unwrap());
        assert!(!scene::audible(&base, conv, &private, &bob).unwrap());
    }
    let addressed = beat(1, 6, &format!("[{bob}]"));
    assert!(scene::audible(&base, conv, &addressed, &alice).unwrap());
    assert!(scene::audible(&base, conv, &addressed, &bob).unwrap());
    let action = yaml(&format!(
        "{{id: action, at: {{tick: 6}}, kind: action, actors: [{bob}], text: Waves}}"
    ));
    assert!(scene::audible(&base, conv, &action, &bob).unwrap());
    assert!(!scene::audible(&base, conv, &action, &alice).unwrap());
}

#[test]
fn malformed_beats_absent_recipients_and_unknown_audience_tokens_fail_validation() {
    let base = world(vec![conversation_record()]);
    let cid = id("conversation", 1);
    let good = beat(1, 6, "[participants]");
    valid(&edited(&base, &cid, |v| {
        v["turns"] = Yaml::Sequence(vec![good.clone()])
    }));
    let mut variants = Vec::new();
    let mut missing = good.clone();
    missing.as_mapping_mut().unwrap().remove("at");
    variants.push(missing);
    let mut absent = good.clone();
    absent["speaker"] = yaml(&id("character", 3));
    variants.push(absent);
    let mut end = good.clone();
    end["at"] = yaml("{tick: 21}");
    variants.push(end);
    let mut old = good.clone();
    old["audience"] = yaml(&format!("[char:{}]", id("character", 2)));
    variants.push(old);
    let mut absent = good.clone();
    absent["audience"] = yaml(&format!("[{}]", id("character", 3)));
    variants.push(absent);
    let mut absent = good.clone();
    absent["addressee"] = yaml(&id("character", 3));
    variants.push(absent);
    let mut unknown = good.clone();
    unknown["kind"] = yaml("thought");
    variants.push(unknown);
    let mut duplicate = good.clone();
    duplicate["audience"] = yaml("[participants, participants]");
    variants.push(duplicate);
    for turn in variants {
        invalid(&edited(&base, &cid, |v| {
            v["turns"] = Yaml::Sequence(vec![turn])
        }));
    }
}

#[test]
fn conversation_membership_does_not_inherit_scene_members_or_location() {
    let base = world(vec![scene_record(1), conversation_record()]);
    let sid = id("scene", 1);
    let cid = id("conversation", 1);
    let bob = id("character", 2);
    let isolated = edited(&base, &sid, |v| {
        v["participants"] = yaml(&format!("[{{character: {}}}]", id("character", 1)))
    });
    let isolated = edited(&isolated, &cid, |v| {
        v["scene"] = yaml(&sid);
        v["location"] = yaml(&id("location", 2));
        v["turns"] = Yaml::Sequence(vec![beat(2, 6, "[participants]")]);
    });
    valid(&isolated);
    assert!(!scene::present(&isolated, &isolated.records[&sid], &bob, &at(6, 0)).unwrap());
    assert!(
        scene::audible(
            &isolated,
            &isolated.records[&cid],
            &isolated.records[&cid].get("turns").unwrap()[0],
            &bob
        )
        .unwrap()
    );
}

#[test]
fn observations_use_origin_presence_and_inclusive_expiry_without_assuming_memory() {
    let base = world(vec![scene_record(1)]);
    let scene = &base.records[&id("scene", 1)];
    let alice = id("character", 1);
    let bob = id("character", 2);
    let early = yaml("{id: early, at: {tick: 3}, text: A flash, audience: [public]}");
    assert!(!scene::observation_visible(&base, scene, &early, &bob, &at(6, 0)).unwrap());
    assert!(scene::observation_visible(&base, scene, &early, &alice, &at(30, 0)).unwrap());
    let expires = yaml(
        "{id: expires, at: {tick: 6}, until: {tick: 8}, text: A flame, audience: [participants]}",
    );
    assert!(scene::observation_visible(&base, scene, &expires, &bob, &at(8, 0)).unwrap());
    assert!(!scene::observation_visible(&base, scene, &expires, &bob, &at(8, 1)).unwrap());
    assert!(!scene::observation_visible(&base, scene, &expires, &bob, &at(5, 0)).unwrap());
    let no_audience = yaml("{id: private, at: {tick: 6}, text: Author annotation}");
    assert!(!scene::observation_visible(&base, scene, &no_audience, &alice, &at(8, 0)).unwrap());
}

#[test]
fn author_scene_query_sorts_time_and_bounds_observations_outcomes_and_conversations() {
    let event = record("event", 1, "time: {tick: 18}\nstatus: canonical\n");
    let base = world(vec![scene_record(1), conversation_record(), event]);
    let sid = id("scene", 1);
    let cid = id("conversation", 1);
    let base = edited(&base, &cid, |v| {
        v["time"]["start"] = yaml("{tick: 12}");
        v["participants"] = yaml(&format!("[{{character: {}}}]", id("character", 2)));
    });
    let base = edited(&base, &sid, |v| {
        v["conversations"] = yaml(&format!("[{cid}]"));
        v["outcome_events"] = yaml(&format!("[{}]", id("event", 1)));
        v["observations"] = yaml(
            "[{id: late, at: {tick: 10}, text: Later, audience: [author]}, {id: early, at: {tick: 3}, text: Earlier, audience: [author]}, {id: future, at: {tick: 18}, text: FUTURE_SECRET, audience: [author]}]",
        );
    });
    valid(&base);
    let result = scene::query(&base, &sid, &at(10, 0)).unwrap();
    assert_eq!(result["observations"][0]["id"], "early");
    assert_eq!(result["observations"][1]["id"], "late");
    assert_eq!(result["presentParticipants"].as_array().unwrap().len(), 2);
    assert_eq!(result["outcomeEvents"], json!([]));
    assert_eq!(result["conversations"], json!([]));
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(!encoded.contains("UNTIMED_SECRET"));
    assert!(!encoded.contains("FUTURE_SECRET"));
    let later = scene::query(&base, &sid, &at(18, 0)).unwrap();
    assert_eq!(later["outcomeEvents"].as_array().unwrap().len(), 1);
    assert_eq!(later["conversations"].as_array().unwrap().len(), 1);
}

#[test]
fn recollections_may_follow_end_but_references_cannot_be_missing_or_future() {
    let base = world(vec![conversation_record()]);
    let cid = id("conversation", 1);
    let bob = id("character", 2);
    let memory = yaml(&format!(
        "{{id: memory, at: {{tick: 30}}, character: {bob}, state: remembered, confidence: 0.9, summary: Authored memory, exact_turns: [beat-one], remembered_quotes: [{{source_turn: beat-one, speaker: {bob}, text: Misattributed words, fidelity: approximate}}]}}"
    ));
    let base = edited(&base, &cid, |v| {
        v["turns"] = Yaml::Sequence(vec![beat(1, 3, "[author]")]);
        v["recollections"] = Yaml::Sequence(vec![memory.clone()]);
    });
    valid(&base);
    // Inaccessible canonical refs are valid author data; the character view must
    // filter them rather than expand their private source text.
    assert!(
        !scene::audible(
            &base,
            &base.records[&cid],
            &base.records[&cid].get("turns").unwrap()[0],
            &bob
        )
        .unwrap()
    );
    for field_value in [
        ("at", yaml("{tick: 2}")),
        ("confidence", yaml("1.1")),
        ("confidence", yaml(".nan")),
        ("character", yaml(&id("character", 3))),
        ("exact_turns", yaml("[missing]")),
    ] {
        let bad = edited(&base, &cid, |v| {
            v["recollections"][0][field_value.0] = field_value.1
        });
        invalid(&bad);
    }
    let duplicate = edited(&base, &cid, |v| {
        let mut second = memory.clone();
        second["id"] = yaml("memory-two");
        v["recollections"].as_sequence_mut().unwrap().push(second);
    });
    invalid(&duplicate);
}

#[test]
fn scene_wire_times_preserve_i64_and_i32_extremes() {
    let base = world(vec![scene_record(1)]);
    let sid = id("scene", 1);
    let huge = edited(&base, &sid, |v| {
        v["time"] = yaml(
            "{start: {tick: -9223372036854775808, order: -2147483648}, current: {tick: 9223372036854775807, order: 2147483647}, end: {tick: 9223372036854775807, order: 2147483647}}",
        );
        v["participants"] = yaml(&format!("[{{character: {}}}]", id("character", 1)));
        v["observations"] = yaml(
            "[{id: last, at: {tick: 9223372036854775807, order: 2147483647}, text: Last, audience: [public]}]",
        );
    });
    valid(&huge);
    let result = scene::query(&huge, &sid, &at(i64::MAX, i32::MAX)).unwrap();
    assert_eq!(
        result["observations"][0]["at"]["tick"],
        i64::MAX.to_string()
    );
    assert_eq!(
        result["observations"][0]["at"]["order"],
        i32::MAX.to_string()
    );
}

#[test]
fn all_four_complete_corpora_satisfy_scene_and_conversation_contract() {
    let mut records = 0;
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let world = source::load_world(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(name),
            None,
        )
        .unwrap();
        let diagnostics = scene::validate(&world);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:#?}");
        records += world.records.len();
    }
    assert_eq!(records, 642);
}

#[test]
fn ordinary_null_optional_bounds_and_expiry_pass_full_validation() {
    let base = world(vec![scene_record(1), conversation_record()]);
    let sid = id("scene", 1);
    let cid = id("conversation", 1);
    let with_nulls = edited(&base, &sid, |v| {
        for member in v["participants"].as_sequence_mut().unwrap() {
            member["from"] = Yaml::Null;
            member["until"] = Yaml::Null;
        }
        v["observations"] = yaml(
            "[{id: observation, at: {tick: 6}, until: null, text: Witnessed, audience: [participants]}]",
        );
    });
    let with_nulls = edited(&with_nulls, &cid, |v| {
        for member in v["participants"].as_sequence_mut().unwrap() {
            member["from"] = Yaml::Null;
            member["until"] = Yaml::Null;
        }
    });
    full_valid(&with_nulls);
    assert!(
        scene::present(
            &with_nulls,
            &with_nulls.records[&sid],
            &id("character", 1),
            &at(20, 0)
        )
        .unwrap()
    );
    assert!(
        scene::observation_visible(
            &with_nulls,
            &with_nulls.records[&sid],
            &with_nulls.records[&sid].get("observations").unwrap()[0],
            &id("character", 1),
            &at(30, 0)
        )
        .unwrap()
    );
}

fn content_world() -> World {
    let base = world(vec![scene_record(1), conversation_record()]);
    let base = edited(&base, &id("scene", 1), |v| {
        v["observations"] = yaml(
            "[{id: observation, at: {tick: 6}, text: Witnessed, audience: [participants], salience: 1.5}]",
        );
    });
    let base = edited(&base, &id("conversation", 1), |v| {
        v["turns"] = Yaml::Sequence(vec![beat(1, 6, "[participants]")]);
        v["recollections"] = yaml(&format!(
            "[{{id: memory, at: {{tick: 30}}, character: {}, state: remembered, confidence: 0.9, summary: A memory}}]",
            id("character", 2)
        ));
    });
    full_valid(&base);
    base
}

#[test]
fn tagged_admitted_text_and_optional_prose_are_invalid_source() {
    let base = content_world();
    let sid = id("scene", 1);
    let cid = id("conversation", 1);
    tag_rejected(&edited(&base, &sid, |v| {
        v["observations"][0]["text"] = tag(yaml("Witnessed"))
    }));
    tag_rejected(&edited(&base, &cid, |v| {
        v["turns"][0]["text"] = tag(yaml("Hello"))
    }));
    tag_rejected(&edited(&base, &cid, |v| {
        v["recollections"][0]["summary"] = tag(yaml("A memory"))
    }));
    for value in [tag(yaml("annotation")), tag(Yaml::Null)] {
        tag_rejected(&edited(&base, &sid, |v| {
            v["participants"][0]["role"] = value.clone()
        }));
        tag_rejected(&edited(&base, &cid, |v| {
            v["turns"][0]["delivery"] = value.clone()
        }));
        for field in ["interpretation", "emotional_impression"] {
            tag_rejected(&edited(&base, &cid, |v| {
                v["recollections"][0][field] = value.clone()
            }));
        }
    }
}

#[test]
fn tagged_arrays_and_nested_mappings_cannot_pass_scene_validation() {
    let base = content_world();
    let sid = id("scene", 1);
    let cid = id("conversation", 1);
    for (target, fields) in [
        (&sid, &["participants", "observations"][..]),
        (&cid, &["participants", "turns", "recollections"][..]),
    ] {
        for field in fields {
            tag_rejected(&edited(&base, target, |v| {
                v[*field] = tag(v[*field].clone())
            }));
            tag_rejected(&edited(&base, target, |v| {
                v[*field][0] = tag(v[*field][0].clone())
            }));
        }
    }
    for field in ["exact_turns", "remembered_quotes"] {
        tag_rejected(&edited(&base, &cid, |v| {
            v["recollections"][0][field] = tag(yaml("[]"))
        }));
    }
    tag_rejected(&edited(&base, &cid, |v| {
        v["turns"][0]["audience"] = tag(yaml("[participants]"))
    }));
    tag_rejected(&edited(&base, &cid, |v| {
        v["turns"] = yaml(&format!(
            "[{{id: action, kind: action, at: {{tick: 6}}, text: Waves, actors: [{}]}}]",
            id("character", 1)
        ));
        v["turns"][0]["actors"] = tag(v["turns"][0]["actors"].clone());
    }));
    tag_rejected(&edited(&base, &cid, |v| {
        v["turns"][0]["addressee"] = tag(yaml(&format!("[{}]", id("character", 2))))
    }));
    for target in [&sid, &cid] {
        tag_rejected(&edited(&base, target, |v| {
            v["time"] = tag(v["time"].clone())
        }));
        tag_rejected(&edited(&base, target, |v| {
            v["time"]["start"] = tag(v["time"]["start"].clone())
        }));
    }
    let tagged_time = edited(&base, &cid, |v| {
        v["turns"][0]["at"] = tag(yaml("{tick: 6}"))
    });
    tag_rejected(&tagged_time);
    assert!(
        scene::turn_time(
            &tagged_time,
            &tagged_time.records[&cid],
            &tagged_time.records[&cid].get("turns").unwrap()[0]
        )
        .is_err()
    );
}

#[test]
fn tagged_numbers_booleans_and_optional_nulls_are_not_coerced_or_omitted() {
    let base = content_world();
    let sid = id("scene", 1);
    let cid = id("conversation", 1);
    for field in ["from", "until"] {
        tag_rejected(&edited(&base, &sid, |v| {
            v["participants"][0][field] = tag(Yaml::Null)
        }));
    }
    tag_rejected(&edited(&base, &sid, |v| {
        v["participants"][0]["point_of_view"] = tag(yaml("true"))
    }));
    for value in [tag(yaml("1.5")), tag(Yaml::Null)] {
        tag_rejected(&edited(&base, &sid, |v| {
            v["observations"][0]["salience"] = value.clone()
        }));
    }
    tag_rejected(&edited(&base, &sid, |v| {
        v["observations"][0]["until"] = tag(Yaml::Null)
    }));
    tag_rejected(&edited(&base, &cid, |v| v["scene"] = tag(Yaml::Null)));
    tag_rejected(&edited(&base, &cid, |v| {
        v["turns"][0]["addressee"] = tag(Yaml::Null)
    }));
    tag_rejected(&edited(&base, &cid, |v| {
        v["recollections"][0]["confidence"] = tag(yaml("0.9"))
    }));
    for (field, value) in [
        ("tick", yaml("6")),
        ("order", yaml("0")),
        ("timeline", yaml("main")),
    ] {
        tag_rejected(&edited(&base, &cid, |v| {
            v["turns"][0]["at"][field] = tag(value)
        }));
    }
    tag_rejected(&edited(&base, &sid, |v| {
        v["status"] = yaml("active");
        v["time"]["end"] = tag(Yaml::Null);
    }));
}
