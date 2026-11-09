//! Exact operational frames stay independent of canonical cursors and author truth.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    character,
    model::{Record, StoryTime, World},
    narrative, source, validation,
    writing_frames::{self, Frame, Lens},
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const PEER: &str = "char_00000000000000000000000002";
const LEFT: &str = "char_00000000000000000000000003";
const SECRET: &str = "char_00000000000000000000000004";
const RELATED: &str = "char_00000000000000000000000005";
const SCENE: &str = "scene_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const SUBJECT: &str = "loc_00000000000000000000000002";
const OBJECT: &str = "obj_00000000000000000000000001";
const ENVIRONMENT: &str = "env_00000000000000000000000001";
const BRIEF: &str = "brief_00000000000000000000000001";
const RELATIONSHIP: &str = "rel_00000000000000000000000001";

fn id(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn interval(start: i64, end: i64) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start,0),"end":point(end,0)})
}
fn section(number: usize, text: &str) -> Json {
    json!({"id":id("description",number),"heading":format!("Dated heading {number}"),"markdown":text,
        "valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"]})
}
fn record(kind: &str, identifier: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":identifier,"title":format!("SOURCE_TITLE_{identifier}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nUNTIMED_BODY_{identifier}\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{identifier}.md")).unwrap()
}
fn from_records(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, Some("a".repeat(40))).unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(
        report.valid,
        "{:?}",
        report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .collect::<Vec<_>>()
    );
}
fn edit(world: &World, identifier: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let owner = &world.records[identifier];
    let mut fields = owner.frontmatter.clone();
    change(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        owner.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &owner.source_path).unwrap();
    from_records(
        world
            .records
            .values()
            .map(|record| {
                if record.id == identifier {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
    )
}
fn set(world: &World, identifier: &str, field: &str, value: Json) -> World {
    edit(world, identifier, |fields| {
        fields[field] = serde_yaml::to_value(value).unwrap()
    })
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    from_records(records)
}
fn knowledge(number: usize, claim: Json, tick: i64) -> Record {
    record(
        "knowledge",
        &id("know", number),
        json!({"knower":VIEWER,"claim":claim,
        "transitions":[{"id":"acquired","time":point(tick,0),"state":"accepted","acquisition":"heard","note":"AUTHOR_NOTE","confidence":0.75}]}),
    )
}
fn world() -> World {
    let mut brief = record(
        "context-brief",
        BRIEF,
        json!({"domain":"writing.test","scope":"world",
        "valid":interval(0,10),"audience":["author","public"],"perspectives":["author","ordinary"],
        "required_refs":[{"kind":"section","owner_id":PLACE,"id":id("description",1)}]}),
    );
    brief.body = "Required writing note: å, ♥ and an exact observation.\n".into();
    brief.raw_bytes = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&brief.frontmatter).unwrap(),
        brief.body
    )
    .into_bytes();
    let world = from_records(vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "capabilities":["descriptions-v1","briefs-v1"],"state_keys":{},"relationship_metrics":{},"current_time":point(5,0)}),
        ),
        record(
            "character",
            VIEWER,
            json!({"title":"Shared name","descriptions":[section(10,"Self dated evidence")]}),
        ),
        record(
            "character",
            PEER,
            json!({"title":"Shared name","descriptions":[section(11,"Present peer evidence")]}),
        ),
        record(
            "character",
            LEFT,
            json!({"descriptions":[section(12,"Former participant evidence")]}),
        ),
        record(
            "character",
            SECRET,
            json!({"audience":["author"],"perspectives":["author"],"descriptions":[section(13,"PRIVATE_PERSON_NEEDLE")]}),
        ),
        record(
            "character",
            RELATED,
            json!({"descriptions":[section(14,"Related person evidence")]}),
        ),
        record(
            "location",
            PLACE,
            json!({"descriptions":[section(1,"A dated harbor observation")] }),
        ),
        record(
            "location",
            SUBJECT,
            json!({"descriptions":[section(2,"Explicit claim subject evidence")] }),
        ),
        record(
            "object",
            OBJECT,
            json!({"descriptions":[section(3,"Explicit scene object evidence")] }),
        ),
        record(
            "environment",
            ENVIRONMENT,
            json!({"targets":[PLACE],"time":{"start":point(0,0),"end":point(10,0)},"descriptions":[section(4,"Explicit scene environment evidence")] }),
        ),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","location":PLACE,"time":{"start":point(0,0),"current":point(5,0),"end":point(10,0)},
            "participants":[{"character":VIEWER},{"character":PEER,"from":point(5,0)},{"character":LEFT,"until":point(4,0)},{"character":SECRET}],
            "objects":[OBJECT],"environments":[ENVIRONMENT],"descriptions":[section(5,"Selected scene dated evidence")]}),
        ),
        record(
            "relationship",
            RELATIONSHIP,
            json!({"from":VIEWER,"to":RELATED,"relationship_kind":"friendship",
            "transitions":[{"id":"met","time":point(3,0),"relationship_status":"active","metrics":{},"facets":[],"note":"PRIVATE_RELATIONSHIP_NOTE"}]}),
        ),
        knowledge(
            1,
            json!({"key":"subject","statement":"A held ordinary statement","subject":SUBJECT}),
            5,
        ),
        knowledge(
            2,
            json!({"key":"brief","statement":"I learned the writing brief","object":{"entity":BRIEF}}),
            5,
        ),
        brief,
    ]);
    valid(&world);
    world
}
fn frame(world: &World, lens: Lens, tick: i64) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: Some(SCENE.into()),
        pov_id: Some(VIEWER.into()),
        at: at(tick, 0),
        lens,
    }
}
fn normalize(mut value: Json) -> Json {
    value["frame"]
        .as_object_mut()
        .unwrap()
        .remove("fingerprint");
    value
}
fn owners(value: &Json) -> Vec<&str> {
    value["descriptions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|section| section["ownerId"].as_str().unwrap())
        .collect()
}

#[test]
fn frame_decode_is_closed_and_requires_exact_canonical_coordinate_strings() {
    let world = world();
    let selected = frame(&world, Lens::Character, 5);
    let wire = json!(&selected);
    assert_eq!(writing_frames::decode(&wire.to_string()).unwrap(), selected);
    let mut cases = Vec::new();
    for tick in [
        json!(5),
        json!("05"),
        json!("+5"),
        json!("-0"),
        json!("9223372036854775808"),
        Json::Null,
    ] {
        let mut bad = wire.clone();
        bad["at"]["tick"] = tick;
        cases.push(bad);
    }
    for order in [
        json!(0),
        json!("2147483648"),
        json!("-2147483649"),
        json!(" 0"),
    ] {
        let mut bad = wire.clone();
        bad["at"]["order"] = order;
        cases.push(bad);
    }
    for (key, value) in [
        ("workId", Json::Null),
        ("extra", json!(true)),
        ("lens", json!("reader")),
        ("povId", Json::Null),
        ("fingerprint", json!("HEAD")),
        ("sceneId", json!([])),
    ] {
        let mut bad = wire.clone();
        bad[key] = value;
        cases.push(bad);
    }
    let mut absent = wire.clone();
    absent.as_object_mut().unwrap().remove("sceneId");
    cases.push(absent);
    let mut missing_time = wire.clone();
    missing_time["at"].as_object_mut().unwrap().remove("order");
    cases.push(missing_time);
    for bad in cases {
        assert!(writing_frames::decode(&bad.to_string()).is_err(), "{bad}");
    }
    let duplicate = wire
        .to_string()
        .replacen("\"tick\":\"5\"", "\"tick\":\"5\",\"tick\":\"6\"", 1);
    assert!(
        writing_frames::decode(&duplicate)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    assert!(writing_frames::decode(&format!("{}{}", wire, " ".repeat(16_384))).is_err());
}

#[test]
fn independent_frames_preserve_exact_time_without_cursor_or_source_mutation() {
    let world = world();
    let before = world
        .records
        .values()
        .map(|r| (&r.id, r.raw_bytes.clone()))
        .collect::<Vec<_>>();
    let first = frame(&world, Lens::Character, 5);
    let mut second = frame(&world, Lens::Author, 9_007_199_254_740_993);
    second.at.order = -7;
    for selected in [&first, &second, &first] {
        assert_eq!(
            writing_frames::resolve(&world, None, selected).unwrap(),
            json!(selected)
        );
    }
    for (tick, order) in [(i64::MIN, i32::MIN), (i64::MAX, i32::MAX)] {
        let selected = Frame {
            at: StoryTime::new("other", tick, order).unwrap(),
            ..second.clone()
        };
        assert_eq!(
            writing_frames::decode(&json!(&selected).to_string()).unwrap(),
            selected
        );
        assert_eq!(
            writing_frames::resolve(&world, None, &selected).unwrap()["at"],
            json!(selected.at)
        );
    }
    assert_eq!(
        world.world_record().unwrap().get("current_time").unwrap()["tick"].as_i64(),
        Some(5)
    );
    assert_eq!(
        before,
        world
            .records
            .values()
            .map(|r| (&r.id, r.raw_bytes.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn stale_missing_wrong_kind_and_withheld_frame_selections_use_generic_errors() {
    let world = world();
    let selected = frame(&world, Lens::Character, 5);
    let expected = writing_frames::resolve(
        &world,
        None,
        &Frame {
            scene_id: Some(id("scene", 99)),
            ..selected.clone()
        },
    )
    .unwrap_err()
    .to_string();
    for bad in [
        Frame {
            scene_id: Some(VIEWER.into()),
            ..selected.clone()
        },
        Frame {
            pov_id: Some(PLACE.into()),
            ..selected.clone()
        },
        Frame {
            fingerprint: "b".repeat(64),
            ..selected.clone()
        },
        Frame {
            at: StoryTime::new("missing", 5, 0).unwrap(),
            ..selected.clone()
        },
        Frame {
            at: at(11, 0),
            ..selected.clone()
        },
    ] {
        assert_eq!(
            writing_frames::resolve(&world, None, &bad)
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            writing_frames::choices(&world, None, &bad, 100)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
    for field in ["audience", "perspectives"] {
        let private = set(&world, SCENE, field, json!(["author"]));
        valid(&private);
        assert_eq!(
            writing_frames::resolve(&private, None, &frame(&private, Lens::Character, 5))
                .unwrap_err()
                .to_string(),
            expected
        );
    }
}

#[test]
fn actual_scene_presence_gates_roots_at_exact_inclusive_boundaries() {
    let world = world();
    for tick in [0, 10] {
        writing_frames::resolve(&world, None, &frame(&world, Lens::Character, tick)).unwrap();
    }
    for selected in [
        Frame {
            at: at(10, 1),
            ..frame(&world, Lens::Character, 10)
        },
        frame(&world, Lens::Character, -1),
    ] {
        assert!(writing_frames::resolve(&world, None, &selected).is_err());
    }
    let four = writing_frames::context(
        &world,
        None,
        &frame(&world, Lens::Character, 4),
        &[],
        100_000,
    )
    .unwrap();
    let five = writing_frames::context(
        &world,
        None,
        &frame(&world, Lens::Character, 5),
        &[],
        100_000,
    )
    .unwrap();
    assert!(owners(&four["characterPrompt"]).contains(&LEFT));
    assert!(!owners(&four["characterPrompt"]).contains(&PEER));
    assert!(!owners(&five["characterPrompt"]).contains(&LEFT));
    assert!(owners(&five["characterPrompt"]).contains(&PEER));
    assert!(!five.to_string().contains(SECRET));
    assert_eq!(five["characterPrompt"]["scene"]["locationId"], PLACE);
}

#[test]
fn scene_scope_adds_only_explicit_roots_and_preserves_old_context_defaults() {
    let world = world();
    let plain = character::context(&world, VIEWER, &at(5, 0), 100_000).unwrap();
    assert_eq!(owners(&plain), vec![VIEWER]);
    let packet = writing_frames::context(
        &world,
        None,
        &frame(&world, Lens::Character, 5),
        &[],
        100_000,
    )
    .unwrap();
    let admitted = owners(&packet["characterPrompt"]);
    for owner in [
        VIEWER,
        PEER,
        PLACE,
        SUBJECT,
        OBJECT,
        ENVIRONMENT,
        SCENE,
        RELATED,
    ] {
        assert!(admitted.contains(&owner), "{owner}: {admitted:?}");
    }
    assert!(
        !packet["characterPrompt"]
            .to_string()
            .contains("PRIVATE_RELATIONSHIP_NOTE")
    );
    assert!(
        !packet["characterPrompt"]
            .to_string()
            .contains("UNTIMED_BODY")
    );
    assert!(
        !packet["characterPrompt"]
            .to_string()
            .contains("SOURCE_TITLE")
    );
    assert!(narrative::descriptions(&world, VIEWER, PLACE, &at(5, 0), 100).is_err());
    assert_eq!(
        character::context(&world, VIEWER, &at(5, 0), 100_000).unwrap(),
        plain
    );
}

#[test]
fn scoped_relationships_and_subject_learning_do_not_bypass_labels_or_time() {
    let world = world();
    let private = set(&world, RELATIONSHIP, "audience", json!(["author"]));
    let private = set(&private, &id("know", 1), "perspectives", json!(["author"]));
    valid(&private);
    let packet = writing_frames::context(
        &private,
        None,
        &frame(&private, Lens::Character, 5),
        &[],
        100_000,
    )
    .unwrap();
    assert!(!owners(&packet["characterPrompt"]).contains(&RELATED));
    assert!(!owners(&packet["characterPrompt"]).contains(&SUBJECT));
    let early = writing_frames::context(
        &world,
        None,
        &frame(&world, Lens::Character, 2),
        &[],
        100_000,
    )
    .unwrap();
    assert!(!owners(&early["characterPrompt"]).contains(&RELATED));
    assert!(!owners(&early["characterPrompt"]).contains(&SUBJECT));
}

#[test]
fn choices_preserve_duplicate_names_and_exact_admitted_story_moments() {
    let world = world();
    let moment = id("event", 1);
    let world = add(
        &world,
        record(
            "event",
            &moment,
            json!({"title":"Named exact moment","time":point(9_007_199_254_740_993,-7),"effects":[]}),
        ),
    );
    valid(&world);
    let author = Frame {
        at: at(9_007_199_254_740_993, -7),
        ..frame(&world, Lens::Author, 5)
    };
    let choices = writing_frames::choices(&world, None, &author, 100).unwrap();
    assert_eq!(
        choices["povs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["label"] == "Shared name")
            .count(),
        2
    );
    assert_eq!(
        choices["moments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == format!("{moment}:time"))
            .unwrap()["at"],
        json!(author.at)
    );
    let character =
        writing_frames::choices(&world, None, &frame(&world, Lens::Character, 5), 100).unwrap();
    assert!(!character.to_string().contains("Named exact moment"));
    assert!(!character.to_string().contains(SECRET));
    assert!(!character.to_string().contains(LEFT));
    assert_eq!(character["works"], json!([]));
    assert!(character["authorFingerprint"].is_null());
    assert_eq!(character["moments"].as_array().unwrap().len(), 2);
}

#[test]
fn private_and_future_source_changes_leave_reselected_character_packets_and_choices_unchanged() {
    let world = world();
    let selected = frame(&world, Lens::Character, 5);
    let packet = writing_frames::context(&world, None, &selected, &[], 100_000).unwrap();
    let choices = writing_frames::choices(&world, None, &selected, 100).unwrap();
    let changed = set(&world, SECRET, "title", json!("DIFFERENT_PRIVATE_TITLE"));
    let changed = set(
        &changed,
        SECRET,
        "descriptions",
        json!([section(13, "DIFFERENT_PRIVATE_WORDS")]),
    );
    let changed = add(
        &changed,
        record(
            "event",
            &id("event", 2),
            json!({"title":"FUTURE_EVENT_NEEDLE","time":point(20,0),"effects":[]}),
        ),
    );
    valid(&changed);
    let new = frame(&changed, Lens::Character, 5);
    assert!(writing_frames::resolve(&changed, None, &selected).is_err());
    assert_eq!(
        normalize(packet),
        normalize(writing_frames::context(&changed, None, &new, &[], 100_000).unwrap())
    );
    assert_eq!(
        normalize(choices),
        normalize(writing_frames::choices(&changed, None, &new, 100).unwrap())
    );
    assert_eq!(
        writing_frames::context(&world, None, &selected, &[], 256)
            .unwrap_err()
            .to_string(),
        writing_frames::context(&changed, None, &new, &[], 256)
            .unwrap_err()
            .to_string()
    );
}

#[test]
fn selected_scene_metadata_is_filtered_before_character_projection() {
    let world = world();
    let hidden = set(&world, PLACE, "audience", json!(["author"]));
    let hidden = set(&hidden, OBJECT, "audience", json!(["author"]));
    let hidden = set(&hidden, ENVIRONMENT, "perspectives", json!(["author"]));
    valid(&hidden);
    let packet = writing_frames::context(
        &hidden,
        None,
        &frame(&hidden, Lens::Character, 5),
        &[],
        100_000,
    )
    .unwrap();
    assert!(packet["characterPrompt"]["scene"]["locationId"].is_null());
    assert_eq!(packet["characterPrompt"]["scene"]["objects"], json!([]));
    assert_eq!(
        packet["characterPrompt"]["scene"]["environments"],
        json!([])
    );
    for owner in [PLACE, OBJECT, ENVIRONMENT] {
        assert!(!owners(&packet["characterPrompt"]).contains(&owner));
    }
}

#[test]
fn dramatic_irony_keeps_the_complete_character_component_separate_and_identical() {
    let world = world();
    let selected = frame(&world, Lens::Character, 5);
    let character =
        writing_frames::context(&world, None, &selected, &[BRIEF.into()], 100_000).unwrap();
    let author = writing_frames::context(
        &world,
        None,
        &Frame {
            lens: Lens::Author,
            ..selected.clone()
        },
        &[BRIEF.into()],
        100_000,
    )
    .unwrap();
    let dramatic = writing_frames::context(
        &world,
        None,
        &Frame {
            lens: Lens::DramaticIrony,
            ..selected
        },
        &[BRIEF.into()],
        100_000,
    )
    .unwrap();
    assert_eq!(dramatic["characterPrompt"], character["characterPrompt"]);
    assert_eq!(dramatic["authorMargin"], author["authorMargin"]);
    assert!(character.get("authorMargin").is_none());
    assert!(author.get("characterPrompt").is_none());
    let safe = dramatic["characterPrompt"].to_string();
    for forbidden in [
        "fingerprint",
        "revision",
        "sourcePath",
        "blobOid",
        "PRIVATE_RELATIONSHIP_NOTE",
    ] {
        assert!(!safe.contains(forbidden), "{forbidden}");
    }
    assert!(
        dramatic["authorMargin"]
            .to_string()
            .contains("PRIVATE_RELATIONSHIP_NOTE")
    );
}

#[test]
fn required_briefs_use_the_scene_scope_and_duplicate_selection_is_stably_deduplicated() {
    let world = world();
    let selected = frame(&world, Lens::Character, 5);
    let packet =
        writing_frames::context(&world, None, &selected, &[BRIEF.into()], 100_000).unwrap();
    assert_eq!(
        packet["characterPrompt"]["briefs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        packet["characterPrompt"]
            .to_string()
            .contains("Required writing note")
    );
    assert_eq!(
        packet,
        writing_frames::context(
            &world,
            None,
            &selected,
            &[BRIEF.into(), BRIEF.into()],
            100_000
        )
        .unwrap()
    );
    assert!(
        writing_frames::context(
            &world,
            None,
            &frame(&world, Lens::Character, 4),
            &[BRIEF.into()],
            100_000
        )
        .is_err()
    );
    assert!(
        writing_frames::context(&world, None, &selected, &vec![BRIEF.into(); 101], 100_000)
            .is_err()
    );
}

#[test]
fn a_late_unavailable_pin_refuses_everything_before_packet_budget_errors() {
    let world = world();
    let mut future = section(9, "FUTURE_REQUIRED_WORDS");
    future["valid"] = interval(20, 30);
    let world = set(
        &world,
        PLACE,
        "descriptions",
        json!([section(1, "A dated harbor observation"), future]),
    );
    let world = set(
        &world,
        BRIEF,
        "required_refs",
        json!([
        {"kind":"section","owner_id":PLACE,"id":id("description",1)},
        {"kind":"section","owner_id":PLACE,"id":id("description",9)}]),
    );
    valid(&world);
    for lens in [Lens::Author, Lens::Character, Lens::DramaticIrony] {
        let selected = frame(&world, lens, 5);
        let small = writing_frames::context(&world, None, &selected, &[BRIEF.into()], 1)
            .unwrap_err()
            .to_string();
        let large = writing_frames::context(&world, None, &selected, &[BRIEF.into()], 100_000)
            .unwrap_err()
            .to_string();
        assert_eq!(small, large);
        assert!(small.contains("writing frame is unavailable"));
        assert!(!small.contains("FUTURE_REQUIRED_WORDS"));
    }
}

#[test]
fn brief_owner_status_and_validity_constrain_generic_description_admission() {
    let world = world();
    let world = set(
        &world,
        BRIEF,
        "descriptions",
        json!([section(20, "BRIEF_OWNER_SECTION")]),
    );
    assert!(
        character::context(&world, VIEWER, &at(5, 0), 100_000)
            .unwrap()
            .to_string()
            .contains("BRIEF_OWNER_SECTION")
    );
    for changed in [
        set(&world, BRIEF, "status", json!("draft")),
        set(&world, BRIEF, "status", json!("retired")),
        set(&world, BRIEF, "valid", interval(20, 30)),
    ] {
        valid(&changed);
        assert!(
            !character::context(&changed, VIEWER, &at(5, 0), 100_000)
                .unwrap()
                .to_string()
                .contains("BRIEF_OWNER_SECTION")
        );
        assert!(
            !writing_frames::context(
                &changed,
                None,
                &frame(&changed, Lens::Character, 5),
                &[],
                100_000
            )
            .unwrap()["characterPrompt"]
                .to_string()
                .contains("BRIEF_OWNER_SECTION")
        );
    }
}

#[test]
fn writing_context_enforces_one_complete_unicode_scalar_budget_including_frame_and_pins() {
    let world = world();
    let selected = frame(&world, Lens::DramaticIrony, 5);
    let mut budget = 100_000;
    for _ in 0..5 {
        let packet =
            writing_frames::context(&world, None, &selected, &[BRIEF.into()], budget).unwrap();
        let needed = packet.to_string().chars().count();
        if needed == budget {
            break;
        }
        budget = needed;
    }
    let packet = writing_frames::context(&world, None, &selected, &[BRIEF.into()], budget).unwrap();
    assert_eq!(packet.to_string().chars().count(), budget);
    assert!(packet.to_string().len() > budget);
    assert!(
        writing_frames::context(&world, None, &selected, &[BRIEF.into()], budget - 1)
            .unwrap_err()
            .to_string()
            .contains("complete writing context needs")
    );
    for limit in [0, 1001] {
        assert!(writing_frames::choices(&world, None, &selected, limit).is_err());
    }
    assert!(
        writing_frames::choices(&world, None, &selected, 1)
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
}

#[test]
fn missing_operational_selections_are_not_inferred_and_empty_briefs_need_no_capability() {
    let world = world();
    for selected in [
        Frame {
            scene_id: None,
            ..frame(&world, Lens::Character, 5)
        },
        Frame {
            scene_id: None,
            pov_id: None,
            ..frame(&world, Lens::Author, 5)
        },
    ] {
        writing_frames::resolve(&world, None, &selected).unwrap();
        assert!(
            writing_frames::context(&world, None, &selected, &[], 100_000)
                .unwrap_err()
                .to_string()
                .contains("explicit scene and POV")
        );
    }
    let stripped = from_records(
        world
            .records
            .values()
            .filter(|r| r.id != BRIEF && r.id != id("know", 2))
            .cloned()
            .collect(),
    );
    let stripped = set(&stripped, WORLD, "capabilities", json!(["descriptions-v1"]));
    valid(&stripped);
    writing_frames::context(
        &stripped,
        None,
        &frame(&stripped, Lens::Character, 5),
        &[],
        100_000,
    )
    .unwrap();
}

#[test]
fn invalid_captured_source_is_generic_and_old_immutable_worlds_remain_readable() {
    let world = world();
    let selected = frame(&world, Lens::Character, 5);
    let original = writing_frames::context(&world, None, &selected, &[], 100_000).unwrap();
    let newer = from_records(
        world
            .records
            .values()
            .filter(|r| r.id != SCENE)
            .cloned()
            .collect(),
    );
    valid(&newer);
    assert!(writing_frames::resolve(&newer, None, &selected).is_err());
    assert_eq!(
        writing_frames::context(&world, None, &selected, &[], 100_000).unwrap(),
        original
    );
    for mode in 0..3 {
        let mut corrupt = world.clone();
        match mode {
            0 => corrupt
                .records
                .get_mut(SECRET)
                .unwrap()
                .body
                .push_str("PRIVATE_CORRUPTION"),
            1 => {
                corrupt.records.get_mut(SECRET).unwrap().frontmatter["title"] =
                    Yaml::String("PRIVATE_CORRUPTION".into())
            }
            _ => corrupt.fingerprint = "f".repeat(64),
        }
        for error in [
            writing_frames::resolve(&corrupt, None, &selected).unwrap_err(),
            writing_frames::choices(&corrupt, None, &selected, 100).unwrap_err(),
            writing_frames::context(&corrupt, None, &selected, &[], 100_000).unwrap_err(),
        ] {
            assert!(
                error
                    .to_string()
                    .contains("writing frame source is invalid")
            );
            assert!(!error.to_string().contains("PRIVATE_CORRUPTION"));
        }
    }
}

#[test]
fn full_author_writing_briefs_join_public_and_author_required_material() {
    let world = world();
    let world = set(&world, BRIEF, "audience", json!(["public"]));
    let world = set(&world, BRIEF, "perspectives", json!(["ordinary"]));
    let world = set(&world, PLACE, "audience", json!(["author"]));
    valid(&world);
    assert_eq!(
        wedl::briefs::compose(&world, BRIEF, Some(&at(5, 0)), "author", "author", 10).unwrap()["outcome"],
        "unknown"
    );
    let packet = writing_frames::context(
        &world,
        None,
        &frame(&world, Lens::Author, 5),
        &[BRIEF.into()],
        100_000,
    )
    .unwrap();
    let expected = wedl::briefs::compose_full_author(&world, BRIEF, Some(&at(5, 0)), 1000).unwrap();
    assert_eq!(packet["authorMargin"]["briefs"], expected["items"]);
    assert!(
        writing_frames::context(
            &world,
            None,
            &frame(&world, Lens::Character, 5),
            &[BRIEF.into()],
            100_000
        )
        .is_err()
    );
}
