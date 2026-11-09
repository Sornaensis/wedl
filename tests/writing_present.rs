//! Current physical facts belong to an explicitly selected Writing scene.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    character,
    model::{Record, StoryTime, World},
    runtime, source, validation,
    writing_frames::{self, Frame, Lens},
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const PEER: &str = "char_00000000000000000000000002";
const HIDDEN: &str = "char_00000000000000000000000003";
const RELATED: &str = "char_00000000000000000000000004";
const SCENE: &str = "scene_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const OTHER: &str = "loc_00000000000000000000000002";
const OBJECT: &str = "obj_00000000000000000000000001";
const BOX: &str = "obj_00000000000000000000000002";
const ENV: &str = "env_00000000000000000000000001";
const REL: &str = "rel_00000000000000000000000001";

fn id(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn record(kind: &str, id: &str, mut fields: Json) -> Record {
    if kind == "event" {
        let owner = id.rsplit('_').next().unwrap().parse::<usize>().unwrap();
        if let Some(effects) = fields.get_mut("effects").and_then(Json::as_array_mut) {
            for (index, effect) in effects.iter_mut().enumerate() {
                effect["id"] = json!(self::id("effect", owner * 100_000 + index));
            }
        }
    }
    let mut value =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("PRIVATE_TITLE_{id}")});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!(
            "---\n{}---\nPRIVATE_BODY_{id}\n",
            serde_yaml::to_string(&value).unwrap()
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn world(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, Some("a".repeat(40))).unwrap()
}
fn valid(world: &World) {
    let result = validation::validate_world(world);
    assert!(
        result.valid,
        "{:?}",
        result
            .diagnostics
            .iter()
            .filter(|v| v.severity == "error")
            .collect::<Vec<_>>()
    );
}
fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let record = &world.records[id];
    let mut data = record.frontmatter.clone();
    change(&mut data);
    let replacement = source::parse_record(
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&data).unwrap(),
            record.body
        )
        .as_bytes(),
        &record.source_path,
    )
    .unwrap();
    self::world(
        world
            .records
            .values()
            .map(|r| {
                if r.id == id {
                    replacement.clone()
                } else {
                    r.clone()
                }
            })
            .collect(),
    )
}
fn set(world: &World, id: &str, field: &str, value: Json) -> World {
    edit(world, id, |data| {
        data[field] = serde_yaml::to_value(value).unwrap()
    })
}
fn add(world: &World, values: Vec<Record>) -> World {
    self::world(world.records.values().cloned().chain(values).collect())
}
fn fixture() -> World {
    world(vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "current_time":point(5,0),"capabilities":[],"state_keys":{"object":{"holder":{"type":"entity","kind":"character"},"location":{"type":"entity","kind":"location"},"container":{"type":"entity","kind":"object"},"condition":{"type":"integer"}}},"relationship_metrics":{"trust":{},"fear":{}}}),
        ),
        record("character", VIEWER, json!({})),
        record("character", PEER, json!({})),
        record(
            "character",
            HIDDEN,
            json!({"audience":["author"],"perspectives":["author"]}),
        ),
        record("character", RELATED, json!({})),
        record("location", PLACE, json!({})),
        record("location", OTHER, json!({})),
        record(
            "object",
            OBJECT,
            json!({"initial_state":{"holder":{"entity":VIEWER},"condition":0}}),
        ),
        record(
            "object",
            BOX,
            json!({"initial_state":{"location":{"entity":PLACE}}}),
        ),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","time":{"start":point(i64::MIN,i32::MIN),"current":point(5,0),"end":point(i64::MAX,i32::MAX)},
            "location":PLACE,"participants":[{"character":VIEWER},{"character":PEER,"from":point(5,0)},{"character":HIDDEN}],"objects":[OBJECT]}),
        ),
        record(
            "environment",
            ENV,
            json!({"time":{"start":point(0,0),"end":point(5,0)},"targets":[SCENE,PLACE,OTHER],
            "conditions":{"temperature":9007199254740993_i64,"signal":"9007199254740993","quiet":false,"empty":null,"ratio":1.0},"sensory":["Rain on the roof. 🌧"]}),
        ),
        record(
            "relationship",
            REL,
            json!({"from":VIEWER,"to":RELATED,"relationship_kind":"friendship","transitions":[
            {"id":"old","time":point(0,0),"relationship_status":"active","metrics":{"trust":1},"facets":["familiar"]},
            {"id":"now","time":point(5,0),"relationship_status":"ended","metrics":{"trust":9007199254740993_i64,"fear":0.5},"facets":["wary"],"note":"PRIVATE_RELATIONSHIP_NOTE"},
            {"id":"future","time":point(5,1),"metrics":{"trust":2},"facets":["PRIVATE_FUTURE_FACET"]}]}),
        ),
    ])
}
fn frame(world: &World, time: StoryTime, lens: Lens) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: Some(SCENE.into()),
        pov_id: Some(VIEWER.into()),
        at: time,
        lens,
    }
}
fn read(world: &World, time: StoryTime, lens: Lens, budget: usize) -> Json {
    valid(world);
    writing_frames::context(world, None, &frame(world, time, lens), &[], budget).unwrap()
}
fn prompt(world: &World) -> Json {
    read(world, at(5, 0), Lens::Character, 1_000_000)["characterPrompt"].clone()
}
fn event(number: usize, time: Json, effects: Json) -> Record {
    record(
        "event",
        &id("event", number),
        json!({"time":time,"effects":effects,"audience":["author"],"perspectives":["author"]}),
    )
}
fn effect(key: &str, operation: &str, value: Option<Json>) -> Json {
    let mut result = json!({"target":OBJECT,"key":key,"operation":operation});
    if let Some(value) = value {
        result["value"] = value;
    }
    result
}
fn condition_type(world: &World, declaration: Json, value: Json) -> World {
    let world = edit(world, WORLD, |data| {
        data["state_keys"]["object"]["condition"] = serde_yaml::to_value(declaration).unwrap()
    });
    set(
        &world,
        OBJECT,
        "initial_state",
        json!({"holder":{"entity":VIEWER},"condition":value}),
    )
}

#[test]
fn selected_scene_has_literal_current_facts_with_strict_ordinary_context_and_separate_author_truth()
{
    let world = fixture();
    let before = world
        .records
        .values()
        .map(|r| r.raw_bytes.clone())
        .collect::<Vec<_>>();
    let ordinary = character::context(&world, VIEWER, &at(5, 0), 1_000_000).unwrap();
    assert!(ordinary.get("present").is_none());
    let character = prompt(&world);
    let present = &character["present"];
    assert_eq!(
        present["objects"],
        json!([{"objectId":OBJECT,"state":{"holder":{"entity":VIEWER},"condition":"0"},"valueTypes":{"holder":"entity","condition":"integer"}}])
    );
    assert_eq!(present["environments"][0]["targets"], json!([SCENE, PLACE]));
    assert_eq!(
        present["environments"][0]["conditions"],
        json!({"temperature":"9007199254740993","signal":"9007199254740993","quiet":false,"empty":null,"ratio":1.0})
    );
    assert_eq!(
        present["environments"][0]["conditionTypes"],
        json!({"temperature":"integer","signal":"string","quiet":"boolean","empty":"null","ratio":"number"})
    );
    let relation = &present["relationships"][0];
    assert_eq!(relation["toId"], RELATED);
    assert_eq!(relation["status"], "ended");
    assert_eq!(relation["transitionId"], "now");
    assert_eq!(relation["metrics"]["trust"], "9007199254740993");
    assert_eq!(
        relation["metricTypes"],
        json!({"trust":"integer","fear":"number"})
    );
    for hidden in [
        "PRIVATE_",
        "sourcePath",
        "blobOid",
        "fingerprint",
        "revision",
        "transitions[",
        OTHER,
    ] {
        assert!(
            !character.to_string().contains(hidden),
            "{hidden}: {character}"
        );
    }
    let dramatic = read(&world, at(5, 0), Lens::DramaticIrony, 1_000_000);
    assert_eq!(dramatic["characterPrompt"], character);
    assert!(dramatic["authorMargin"].get("present").is_none());
    assert!(
        read(&world, at(5, 0), Lens::Author, 1_000_000)
            .get("characterPrompt")
            .is_none()
    );
    assert_eq!(
        world
            .records
            .values()
            .map(|r| r.raw_bytes.clone())
            .collect::<Vec<_>>(),
        before
    );
}

#[test]
fn presence_requires_native_placement_and_independently_admitted_recursive_containers() {
    let world = fixture();
    let child = id("obj", 3);
    let absent = id("obj", 4);
    let cycle_a = id("obj", 5);
    let cycle_b = id("obj", 6);
    let world = add(
        &world,
        vec![
            record(
                "object",
                &child,
                json!({"initial_state":{"container":{"entity":BOX}}}),
            ),
            record(
                "object",
                &absent,
                json!({"initial_state":{"holder":{"entity":PEER}}}),
            ),
            record(
                "object",
                &cycle_a,
                json!({"initial_state":{"container":{"entity":cycle_b}}}),
            ),
            record(
                "object",
                &cycle_b,
                json!({"initial_state":{"container":{"entity":cycle_a}}}),
            ),
        ],
    );
    let world = set(
        &world,
        SCENE,
        "objects",
        json!([OBJECT, child, absent, cycle_a, cycle_b]),
    );
    let read_before = read(&world, at(4, 0), Lens::Character, 1_000_000);
    assert_eq!(
        read_before["characterPrompt"]["present"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let known = add(
        &world,
        vec![record(
            "knowledge",
            &id("know", 1),
            json!({"knower":VIEWER,"claim":{"key":"box","statement":"I recognize the box","object":{"entity":BOX}},"transitions":[{"id":"learned","time":point(0,0),"state":"accepted","acquisition":"observed","confidence":1.0}]}),
        )],
    );
    let values = prompt(&known)["present"]["objects"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        values
            .iter()
            .map(|v| v["objectId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![OBJECT, child.as_str(), absent.as_str()]
    );
    assert_eq!(values[1]["state"], json!({"container":{"entity":BOX}}));
    let hidden = set(&known, BOX, "audience", json!(["author"]));
    assert_eq!(
        prompt(&hidden)["present"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let cleared = add(
        &known,
        vec![event(
            1,
            point(5, 0),
            json!([{"target":BOX,"key":"location","operation":"clear"}]),
        )],
    );
    assert_eq!(
        prompt(&cleared)["present"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn state_uses_canonical_native_order_even_when_the_cause_is_private() {
    let world = add(
        &fixture(),
        vec![
            event(
                2,
                point(5, 0),
                json!([effect("condition", "set", Some(json!(i64::MIN)))]),
            ),
            event(
                1,
                point(5, -1),
                json!([effect("condition", "set", Some(json!(i64::MAX)))]),
            ),
            event(
                3,
                point(5, 1),
                json!([effect("condition", "set", Some(json!(7)))]),
            ),
            record(
                "event",
                &id("event", 4),
                json!({"status":"draft","time":point(5,0),"effects":[effect("condition","set",Some(json!(8)))]}),
            ),
        ],
    );
    assert_eq!(
        runtime::resolved_state(&world, OBJECT, &at(5, 0)).unwrap()["condition"].as_i64(),
        Some(i64::MIN)
    );
    assert_eq!(
        prompt(&world)["present"]["objects"][0]["state"]["condition"],
        i64::MIN.to_string()
    );
    let shifted = read(&world, at(5, 1), Lens::Character, 1_000_000);
    assert_eq!(
        shifted["characterPrompt"]["present"]["objects"][0]["state"]["condition"],
        "7"
    );
    assert_eq!(
        shifted["characterPrompt"]["present"]["environments"],
        json!([])
    );
}

#[test]
fn condition_scalars_sets_and_entity_admission_preserve_native_types_and_order() {
    for (declaration, value, expected, kind) in [
        (
            json!({"type":"boolean"}),
            json!(false),
            json!(false),
            "boolean",
        ),
        (json!({"type":"string"}), json!(""), json!(""), "string"),
        (json!({"type":"number"}), json!(1.0), json!(1.0), "number"),
        (
            json!({"type":"number"}),
            json!(u64::MAX),
            json!(u64::MAX.to_string()),
            "number",
        ),
        (
            json!({"type":"integer"}),
            json!(i64::MIN),
            json!(i64::MIN.to_string()),
            "integer",
        ),
        (
            json!({"type":"string"}),
            json!(HIDDEN),
            json!(HIDDEN),
            "string",
        ),
    ] {
        let world = condition_type(&fixture(), declaration, value);
        let row = prompt(&world)["present"]["objects"][0].clone();
        assert_eq!(row["state"]["condition"], expected);
        assert_eq!(row["valueTypes"]["condition"], kind);
    }
    let world = condition_type(
        &fixture(),
        json!({"type":"string-set"}),
        json!(["a", "b", "c"]),
    );
    let world = add(
        &world,
        vec![event(
            1,
            point(5, 0),
            json!([
                effect("condition", "add-to-set", Some(json!("b"))),
                effect("condition", "remove-from-set", Some(json!("a"))),
                effect("condition", "add-to-set", Some(json!("d"))),
                effect("condition", "add-to-set", Some(json!("a"))),
                effect("condition", "remove-from-set", Some(json!("c"))),
                effect("condition", "remove-from-set", Some(json!("missing")))
            ]),
        )],
    );
    let native = runtime::resolved_state(&world, OBJECT, &at(5, 0)).unwrap();
    assert_eq!(
        prompt(&world)["present"]["objects"][0]["state"]["condition"],
        serde_json::to_value(&native["condition"]).unwrap()
    );
    let hidden = condition_type(
        &fixture(),
        json!({"type":"entity-set","kind":"character"}),
        json!([{"entity":PEER},{"entity":HIDDEN}]),
    );
    assert!(
        prompt(&hidden)["present"]["objects"][0]["state"]
            .get("condition")
            .is_none()
    );
    let removed = add(
        &hidden,
        vec![event(
            1,
            point(5, -1),
            json!([effect(
                "condition",
                "remove-from-set",
                Some(json!({"entity":HIDDEN}))
            )]),
        )],
    );
    assert_eq!(
        prompt(&removed)["present"]["objects"][0]["state"]["condition"],
        json!([{"entity":PEER}])
    );
    let cleared = add(
        &removed,
        vec![event(
            2,
            point(5, 0),
            json!([
                effect("condition", "clear", None),
                effect("condition", "remove-from-set", Some(json!({"entity":PEER})))
            ]),
        )],
    );
    assert_eq!(
        prompt(&cleared)["present"]["objects"][0]["state"]["condition"],
        json!([])
    );
}

#[test]
fn repeated_set_history_does_not_charge_withheld_or_small_final_conditions() {
    let base = condition_type(
        &fixture(),
        json!({"type":"entity-set","kind":"character"}),
        json!([]),
    );
    let hidden = add(
        &base,
        vec![event(
            1,
            point(5, -1),
            Json::Array(
                (0..10_001)
                    .map(|_| effect("condition", "add-to-set", Some(json!({"entity":HIDDEN}))))
                    .collect(),
            ),
        )],
    );
    assert!(
        prompt(&hidden)["present"]["objects"][0]["state"]
            .get("condition")
            .is_none()
    );
    let known = condition_type(
        &base,
        json!({"type":"entity-set","kind":"character"}),
        json!([{"entity":PEER}]),
    );
    let repeated = add(
        &known,
        vec![event(
            1,
            point(5, 0),
            Json::Array(
                (0..10_001)
                    .map(|_| effect("condition", "add-to-set", Some(json!({"entity":PEER}))))
                    .collect(),
            ),
        )],
    );
    assert_eq!(prompt(&repeated), prompt(&known));
    let empty = add(
        &hidden,
        vec![event(
            2,
            point(5, 0),
            json!([effect(
                "condition",
                "remove-from-set",
                Some(json!({"entity":HIDDEN}))
            )]),
        )],
    );
    assert_eq!(prompt(&empty), prompt(&base));
}

#[test]
fn bound_placement_only_uses_declared_root_entity_and_withholds_condition_and_siblings() {
    let world = fixture();
    let profile = json!({"id":"placement","label":"Private profile","fields":{"type":"object","required":{"entity":{"type":"entity","kinds":["character","location"]}},"optional":{"secret":{"type":"string"}}}});
    let world = set(&world, WORLD, "capabilities", json!(["lore-core-v1"]));
    let world = set(&world, WORLD, "profiles", json!([profile]));
    let world = set(
        &world,
        WORLD,
        "state_profile_bindings",
        json!({"object":{"holder":"placement","condition":"placement"}}),
    );
    let world = set(
        &world,
        OBJECT,
        "initial_state",
        json!({"holder":{"entity":{"kind":"record","id":VIEWER},"secret":"PRIVATE_BOUND_SIBLING"},"condition":{"entity":{"kind":"record","id":VIEWER},"secret":"PRIVATE_BOUND_CONDITION"}}),
    );
    let expected = prompt(&world);
    assert_eq!(
        expected["present"]["objects"][0]["state"],
        json!({"holder":{"entity":VIEWER}})
    );
    let changed = edit(&world, OBJECT, |data| {
        data["initial_state"]["holder"]["secret"] = Yaml::String("changed".repeat(10000))
    });
    assert_eq!(prompt(&changed), expected);
    let wrong_kind = edit(&world, OBJECT, |data| {
        data["initial_state"]["holder"]["entity"]["id"] = Yaml::String(PLACE.into())
    });
    assert_eq!(prompt(&wrong_kind)["present"]["objects"], json!([]));
    let literal = set(
        &world,
        WORLD,
        "profiles",
        json!([{"id":"placement","label":"Literal","fields":{"type":"object","required":{"entity":{"type":"string"}},"optional":{}}}]),
    );
    let literal = set(
        &literal,
        OBJECT,
        "initial_state",
        json!({"holder":{"entity":VIEWER},"condition":{"entity":VIEWER}}),
    );
    assert_eq!(prompt(&literal)["present"]["objects"], json!([]));
}

#[test]
fn exact_horizon_extrema_and_inclusive_environment_boundaries() {
    for (tick, order) in [(i64::MIN, i32::MIN), (i64::MAX, i32::MAX)] {
        let world = fixture();
        let time = point(tick, order);
        let world = set(&world, ENV, "time", json!({"start":time,"end":time}));
        let world = set(
            &world,
            REL,
            "transitions",
            json!([{"id":"extreme","time":time,"metrics":{"trust":i64::MIN}}]),
        );
        let world = add(
            &world,
            vec![event(
                1,
                time,
                json!([effect("condition", "set", Some(json!(i64::MAX)))]),
            )],
        );
        let result = read(&world, at(tick, order), Lens::Character, 1_000_000);
        let present = &result["characterPrompt"]["present"];
        assert_eq!(
            present["environments"][0]["time"]["start"],
            json!(at(tick, order))
        );
        assert_eq!(present["relationships"][0]["time"], json!(at(tick, order)));
        assert_eq!(
            present["objects"][0]["state"]["condition"],
            i64::MAX.to_string()
        );
    }
    let world = fixture();
    let before = read(&world, at(5, -1), Lens::Character, 1_000_000);
    assert_eq!(
        before["characterPrompt"]["present"]["relationships"][0]["transitionId"],
        "old"
    );
    let other = set(
        &world,
        ENV,
        "time",
        json!({"start":{"timeline":"other","tick":0,"order":0}}),
    );
    assert_eq!(prompt(&other)["present"]["environments"], json!([]));
}

#[test]
fn missing_empty_and_null_relationship_fields_remain_distinct() {
    let world = set(&fixture(), ENV, "conditions", json!({}));
    let world = set(&world, ENV, "sensory", json!([]));
    let world = set(
        &world,
        REL,
        "transitions",
        json!([{"id":"only","time":point(5,0)}]),
    );
    let world = edit(&world, REL, |data| {
        data.as_mapping_mut().unwrap().remove("relationship_kind");
    });
    let result = prompt(&world);
    assert_eq!(
        result["present"]["environments"][0]["conditions"],
        json!({})
    );
    assert_eq!(result["present"]["environments"][0]["sensory"], json!([]));
    let row = &result["present"]["relationships"][0];
    for key in [
        "metrics",
        "metricTypes",
        "facets",
        "status",
        "relationshipKind",
    ] {
        assert!(row.get(key).is_none(), "{key}");
    }
    let missing = edit(&world, ENV, |data| {
        data.as_mapping_mut().unwrap().remove("conditions");
        data.as_mapping_mut().unwrap().remove("sensory");
    });
    assert!(
        prompt(&missing)["present"]["environments"][0]
            .get("conditions")
            .is_none()
    );
    let empty = set(
        &missing,
        REL,
        "transitions",
        json!([{"id":"only","time":point(5,0),"metrics":{},"facets":[]}]),
    );
    assert_eq!(
        prompt(&empty)["present"]["relationships"][0]["metrics"],
        json!({})
    );
    assert_eq!(
        prompt(&empty)["present"]["relationships"][0]["facets"],
        json!([])
    );
    let null = set(
        &missing,
        REL,
        "transitions",
        json!([{"id":"only","time":point(5,0),"metrics":null,"facets":null,"relationship_status":null}]),
    );
    assert_eq!(
        prompt(&null)["present"]["relationships"][0]["metrics"],
        Json::Null
    );
    assert_eq!(
        prompt(&null)["present"]["relationships"][0]["status"],
        Json::Null
    );
}

#[test]
fn hidden_inactive_unrelated_and_incoming_sources_do_not_affect_results_or_limits() {
    let world = fixture();
    let expected = prompt(&world);
    let large = "PRIVATE_UNADMITTED".repeat(260_000);
    let hidden = add(
        &world,
        vec![
            record(
                "environment",
                &id("env", 2),
                json!({"audience":["author"],"targets":[SCENE],"time":{"start":point(0,0)},"sensory":[large]}),
            ),
            record(
                "environment",
                &id("env", 3),
                json!({"targets":[SCENE],"time":{"start":point(5,1)},"conditions":{"private":large}}),
            ),
            record(
                "environment",
                &id("env", 4),
                json!({"targets":[OTHER],"time":{"start":point(0,0)},"conditions":{"private":large}}),
            ),
            record(
                "environment",
                &id("env", 5),
                json!({"status":"retired","targets":[SCENE],"time":{"start":point(0,0)},"sensory":[large]}),
            ),
            record(
                "relationship",
                &id("rel", 2),
                json!({"from":RELATED,"to":VIEWER,"transitions":[{"id":"incoming","time":point(0,0),"facets":[large]}]}),
            ),
            record(
                "relationship",
                &id("rel", 3),
                json!({"from":VIEWER,"to":HIDDEN,"transitions":[{"id":"secret","time":point(0,0),"facets":[large]}]}),
            ),
            record(
                "relationship",
                &id("rel", 4),
                json!({"from":VIEWER,"to":PEER,"audience":["author"],"transitions":[{"id":"secret","time":point(0,0),"facets":[large]}]}),
            ),
        ],
    );
    assert_eq!(prompt(&hidden), expected);
    let changed = edit(&hidden, OBJECT, |data| {
        data["title"] = Yaml::String("different author title".into())
    });
    assert_eq!(prompt(&changed), expected);
}

#[test]
fn complete_result_limits_follow_admission_and_parent_unicode_budget() {
    let world = fixture();
    let full = read(&world, at(5, 0), Lens::Character, 999_999);
    let needed = full.to_string().chars().count();
    let mut exact = needed;
    loop {
        let mut candidate = full.clone();
        candidate["maxCharacters"] = json!(exact);
        let count = candidate.to_string().chars().count();
        if count == exact {
            break;
        }
        exact = count;
    }
    assert!(
        writing_frames::context(
            &world,
            None,
            &frame(&world, at(5, 0), Lens::Character),
            &[],
            exact
        )
        .is_ok()
    );
    assert!(
        writing_frames::context(
            &world,
            None,
            &frame(&world, at(5, 0), Lens::Character),
            &[],
            exact - 1
        )
        .unwrap_err()
        .to_string()
        .contains("complete writing context needs")
    );
    let absent = (10..1011)
        .map(|n| record("object", &id("obj", n), json!({})))
        .collect::<Vec<_>>();
    let list = absent.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    let many = set(&add(&world, absent), SCENE, "objects", json!(list));
    assert_eq!(prompt(&many)["present"]["objects"], json!([]));
    let environments = (10..1010)
        .map(|n| {
            record(
                "environment",
                &id("env", n),
                json!({"targets":[SCENE],"time":{"start":point(0,0)}}),
            )
        })
        .collect::<Vec<_>>();
    let many = add(&world, environments);
    valid(&many);
    assert!(
        writing_frames::context(
            &many,
            None,
            &frame(&many, at(5, 0), Lens::Character),
            &[],
            1_000_000
        )
        .unwrap_err()
        .to_string()
        .contains("complete writing present")
    );
    let too_large = set(&world, ENV, "sensory", json!(["💧".repeat(1_100_000)]));
    valid(&too_large);
    assert!(
        writing_frames::context(
            &too_large,
            None,
            &frame(&too_large, at(5, 0), Lens::Character),
            &[],
            1_000_000
        )
        .unwrap_err()
        .to_string()
        .contains("complete writing present")
    );
    let too_many = add(&world, (10..20).map(|number| record("environment", &id("env", number),
        json!({"targets":[SCENE],"time":{"start":point(0,0)},"sensory":(0..1000).map(|index|format!("detail {index}")).collect::<Vec<_>>()}))).collect());
    valid(&too_many);
    assert!(
        writing_frames::context(
            &too_many,
            None,
            &frame(&too_many, at(5, 0), Lens::Character),
            &[],
            1_000_000
        )
        .unwrap_err()
        .to_string()
        .contains("complete writing present")
    );
}
