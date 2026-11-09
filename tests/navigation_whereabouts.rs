use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, fs, path::PathBuf};
use wedl::{
    model::{Record, StoryTime, World},
    navigation::whereabouts::whereabouts,
    source,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const UNKNOWN: &str = "char_00000000000000000000000003";
const HIDDEN: &str = "char_00000000000000000000000004";
const A: &str = "loc_00000000000000000000000001";
const B: &str = "loc_00000000000000000000000002";
const SECRET: &str = "loc_00000000000000000000000003";

fn record(kind: &str, id: &str, title: &str, fields: Json) -> Record {
    let mut value =
        json!({"schema":"wedl/v1","id":id,"kind":kind,"title":title,"status":"canonical"});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\n\nDo not infer travel from this prose.\n",
        serde_yaml::to_string(&value).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}

fn time(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}

fn fixture() -> World {
    let records = vec![
        record(
            "world",
            WORLD,
            "Test world",
            json!({
                "default_timeline":"main","timelines":[{"id":"main"},{"id":"alternate"}],
                "current_time":time(10,0),
                "state_keys":{"character":{
                    "location":{"type":"entity","kind":"location"},
                    "score":{"type":"integer"},
                    "memory":{"type":"entity","kind":"event"}
                }}
            }),
        ),
        record("location", A, "Atrium", json!({})),
        record("location", B, "Bridge", json!({})),
        record(
            "location",
            SECRET,
            "SECRET LOCATION",
            json!({"audience":["withheld"]}),
        ),
        record(
            "character",
            HERO,
            "Ada",
            json!({"role":"  Protagonist  ","initial_state":{"location":{"entity":A}}}),
        ),
        record(
            "character",
            OTHER,
            "Bryn",
            json!({"status":"retired","initial_state":{"location":{"entity":B}}}),
        ),
        record("character", UNKNOWN, "Cora", json!({})),
        record(
            "character",
            HIDDEN,
            "SECRET CHARACTER",
            json!({"audience":["withheld"],"initial_state":{"location":{"entity":SECRET}}}),
        ),
    ];
    World {
        root: PathBuf::new(),
        records: records
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect(),
        fingerprint: "whereabouts-fixture".into(),
        revision: None,
    }
}

fn add(world: &mut World, record: Record) {
    world.records.insert(record.id.clone(), record);
}

fn event(id: &str, when: Json, effects: Json, participants: Json) -> Record {
    record(
        "event",
        id,
        id,
        json!({"time":when,"effects":effects,"participants":participants}),
    )
}

fn movement(id: &str, when: Json, destination: Option<&str>) -> Record {
    let effect = match destination {
        Some(place) => {
            json!({"target":HERO,"key":"location","operation":"set","value":{"entity":place}})
        }
        None => json!({"target":HERO,"key":"location","operation":"clear"}),
    };
    event(id, when, json!([effect]), json!([]))
}

fn scene(
    id: &str,
    status: &str,
    start: Json,
    current: Json,
    end: Option<Json>,
    participants: Json,
) -> Record {
    let mut bounds = json!({"start":start,"current":current});
    if let Some(end) = end {
        bounds["end"] = end;
    }
    record(
        "scene",
        id,
        id,
        json!({"status":status,"time":bounds,"location":A,"participants":participants}),
    )
}

fn relationship(id: &str, from: &str, to: &str, times: Vec<Json>) -> Record {
    let transitions: Vec<_> = times
        .into_iter()
        .enumerate()
        .map(|(index, time)| json!({"id":format!("step-{index}"),"time":time}))
        .collect();
    record(
        "relationship",
        id,
        id,
        json!({"from":from,"to":to,"transitions":transitions}),
    )
}

fn selected(id: &str) -> Vec<String> {
    vec![id.to_owned()]
}

fn entry<'a>(result: &'a Json, id: &str) -> &'a Json {
    result["characters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["character"]["id"] == id)
        .unwrap()
}

fn replace(world: &mut World, id: &str, field: &str, value: Json) {
    world.records.get_mut(id).unwrap().frontmatter[field] = serde_yaml::to_value(value).unwrap();
}

#[test]
fn initial_locations_and_retired_cohort_are_explicit_without_prose_inference() {
    let world = fixture();
    let result = whereabouts(&world, &at(10, 0), None, 100).unwrap();
    assert_eq!(result["protocol"], "wedl-rust/v1");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["characters"].as_array().unwrap().len(), 3);
    assert_eq!(entry(&result, HERO)["role"], "Protagonist");
    assert_eq!(entry(&result, OTHER)["recordStatus"], "retired");
    assert_eq!(entry(&result, HERO)["presence"], "offstage");
    assert_eq!(entry(&result, HERO)["location"]["id"], A);
    assert_eq!(entry(&result, HERO)["journey"][0]["kind"], "initial");
    assert!(entry(&result, HERO)["journey"][0]["at"].is_null());
    assert!(entry(&result, HERO)["lastKnownLocation"].is_null());
    assert_eq!(entry(&result, UNKNOWN)["presence"], "unlocated");
    assert_eq!(entry(&result, UNKNOWN)["journey"], json!([]));
    assert!(entry(&result, UNKNOWN)["location"].is_null());
    assert_eq!(entry(&result, HERO)["importance"]["score"], 0.0);
    assert_eq!(result["locations"].as_array().unwrap().len(), 2);
    assert_eq!(result["offstageCharacters"].as_array().unwrap().len(), 2);
    assert_eq!(result["unlocatedCharacters"][0]["id"], UNKNOWN);
    assert_eq!(result["activeScenes"], json!([]));
    assert!(!result.to_string().contains("SECRET"));
    assert!(!result.to_string().contains("infer travel from"));
}

#[test]
fn journeys_retain_set_reaffirmation_clear_and_exact_source_ordinal() {
    let mut world = fixture();
    add(
        &mut world,
        event(
            "event-a",
            time(5, 0),
            json!([
                {"target":HERO,"key":"score","operation":"set","value":1},
                {"target":HERO,"key":"location","operation":"set","value":{"entity":B}}
            ]),
            json!([{"character":HERO}]),
        ),
    );
    add(&mut world, movement("event-b", time(5, 1), Some(B)));
    add(&mut world, movement("event-c", time(5, 2), None));
    add(&mut world, movement("event-future", time(5, 3), Some(A)));
    add(
        &mut world,
        movement(
            "event-alternate",
            json!({"timeline":"alternate","tick":-100}),
            Some(A),
        ),
    );
    let mut draft = movement("event-draft", time(0, 0), Some(B));
    draft.frontmatter["status"] = serde_yaml::Value::String("draft".into());
    add(&mut world, draft);
    let before: BTreeMap<_, _> = world
        .records
        .iter()
        .map(|(id, record)| {
            (
                id.clone(),
                (record.frontmatter.clone(), record.raw_bytes.clone()),
            )
        })
        .collect();
    let result = whereabouts(&world, &at(5, 2), Some(&selected(HERO)), 10).unwrap();
    let hero = entry(&result, HERO);
    let kinds: Vec<_> = hero["journey"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["initial", "move", "reaffirmation", "clear"]);
    assert_eq!(hero["journey"][1]["from"]["id"], A);
    assert_eq!(hero["journey"][1]["to"]["id"], B);
    assert_eq!(hero["journey"][1]["citation"]["section"], "effects[1]");
    assert_eq!(
        hero["journey"][3]["at"],
        json!({"timeline":"main","tick":"5","order":"2"})
    );
    assert_eq!(hero["lastKnownLocation"]["id"], B);
    assert!(hero["location"].is_null());
    assert_eq!(hero["presence"], "unlocated");
    assert_eq!(hero["importance"]["raw"]["events"], 3);
    let before_clear = whereabouts(&world, &at(5, 1), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&before_clear, HERO)["location"]["id"], B);
    assert!(entry(&before_clear, HERO)["lastKnownLocation"].is_null());
    for (id, (yaml, bytes)) in before {
        assert_eq!(world.records[&id].frontmatter, yaml);
        assert_eq!(world.records[&id].raw_bytes, bytes);
    }
}

#[test]
fn tied_events_follow_record_id_and_effect_order_without_clock_arithmetic() {
    let mut world = fixture();
    add(
        &mut world,
        movement("event-z", time(i64::MAX, i32::MAX), None),
    );
    add(
        &mut world,
        movement("event-a", time(i64::MAX, i32::MAX), Some(B)),
    );
    let result = whereabouts(&world, &at(i64::MAX, i32::MAX), Some(&selected(HERO)), 10).unwrap();
    let hero = entry(&result, HERO);
    assert_eq!(hero["journey"][1]["event"]["id"], "event-a");
    assert_eq!(hero["journey"][2]["event"]["id"], "event-z");
    assert_eq!(hero["journey"][1]["at"]["tick"], i64::MAX.to_string());
    assert_eq!(hero["journey"][1]["at"]["order"], i32::MAX.to_string());
    assert_eq!(hero["lastKnownLocation"]["id"], B);
    let before = whereabouts(
        &world,
        &at(i64::MAX, i32::MAX - 1),
        Some(&selected(HERO)),
        10,
    )
    .unwrap();
    assert_eq!(entry(&before, HERO)["journey"].as_array().unwrap().len(), 1);
    let earliest = whereabouts(&world, &at(i64::MIN, i32::MIN), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(earliest["effectiveTime"]["tick"], i64::MIN.to_string());
    assert_eq!(entry(&earliest, HERO)["location"]["id"], A);
}

#[test]
fn historical_closed_scenes_and_inclusive_attendance_never_create_location() {
    let mut world = fixture();
    add(
        &mut world,
        scene(
            "scene-front",
            "closed",
            time(5, 0),
            time(12, 0),
            Some(time(12, 0)),
            json!([{"character":HERO,"from":time(5,0),"until":time(10,0)}, {"character":UNKNOWN}]),
        ),
    );
    add(
        &mut world,
        scene(
            "scene-future",
            "active",
            time(11, 0),
            time(20, 0),
            None,
            json!([{"character":HERO}]),
        ),
    );
    for status in ["planned", "abandoned"] {
        add(
            &mut world,
            scene(
                &format!("scene-{status}"),
                status,
                time(0, 0),
                time(20, 0),
                None,
                json!([{"character":OTHER}]),
            ),
        );
    }
    let result = whereabouts(&world, &at(10, 0), None, 100).unwrap();
    assert_eq!(entry(&result, HERO)["presence"], "active-scene");
    assert_eq!(entry(&result, HERO)["activeScene"]["id"], "scene-front");
    assert_eq!(entry(&result, UNKNOWN)["presence"], "unlocated");
    assert_eq!(entry(&result, UNKNOWN)["activeScene"]["id"], "scene-front");
    assert_eq!(entry(&result, UNKNOWN)["journey"], json!([]));
    assert_eq!(entry(&result, OTHER)["presence"], "offstage");
    assert_eq!(result["activeScenes"].as_array().unwrap().len(), 1);
    let after_exit = whereabouts(&world, &at(10, 1), None, 100).unwrap();
    assert_eq!(entry(&after_exit, HERO)["presence"], "offstage");
    assert_eq!(entry(&after_exit, HERO)["importance"]["raw"]["scenes"], 1);
    assert_eq!(
        after_exit["activeScenes"][0]["characters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let after_end = whereabouts(&world, &at(13, 0), Some(&selected(UNKNOWN)), 100).unwrap();
    assert_eq!(after_end["activeScenes"], json!([]));
    assert_eq!(entry(&after_end, UNKNOWN)["importance"]["raw"]["scenes"], 1);
}

#[test]
fn active_scene_presence_stops_at_cursor_but_keeps_prior_appearance_evidence() {
    let mut world = fixture();
    add(
        &mut world,
        scene(
            "scene-open",
            "active",
            time(0, 0),
            time(9, 0),
            None,
            json!([{"character":HERO,"point_of_view":true}]),
        ),
    );
    let during = whereabouts(&world, &at(9, 0), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&during, HERO)["presence"], "active-scene");
    let after = whereabouts(&world, &at(9, 1), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&after, HERO)["presence"], "offstage");
    assert_eq!(entry(&after, HERO)["importance"]["raw"]["scenes"], 1);
    assert_eq!(
        entry(&after, HERO)["importance"]["raw"]["pointOfViewScenes"],
        1
    );
}

#[test]
fn prominence_normalizes_against_the_complete_admitted_cohort_before_filtering() {
    let mut world = fixture();
    add(
        &mut world,
        scene(
            "scene-one",
            "closed",
            time(0, 0),
            time(5, 0),
            Some(time(5, 0)),
            json!([{"character":HERO,"point_of_view":true},{"character":OTHER}]),
        ),
    );
    for name in ["scene-two", "scene-three"] {
        add(
            &mut world,
            scene(
                name,
                "closed",
                time(0, 0),
                time(5, 0),
                Some(time(5, 0)),
                json!([{"character":OTHER,"point_of_view":true}]),
            ),
        );
    }
    add(
        &mut world,
        event(
            "event-one",
            time(1, 0),
            json!([
                {"target":HERO,"key":"score","operation":"set","value":1},
                {"target":HERO,"key":"score","operation":"set","value":2}
            ]),
            json!([{"character":HERO},{"character":OTHER}]),
        ),
    );
    add(
        &mut world,
        event(
            "event-two",
            time(2, 0),
            json!([
                {"target":OTHER,"key":"score","operation":"set","value":3}
            ]),
            json!([{"character":OTHER}]),
        ),
    );
    add(
        &mut world,
        relationship("rel-forward", HERO, OTHER, vec![time(1, 0)]),
    );
    add(
        &mut world,
        relationship("rel-reverse", OTHER, HERO, vec![time(2, 0)]),
    );
    add(
        &mut world,
        relationship("rel-future", HERO, UNKNOWN, vec![time(99, 0)]),
    );
    add(
        &mut world,
        relationship("rel-hidden", HERO, HIDDEN, vec![time(0, 0)]),
    );
    let all = whereabouts(&world, &at(5, 0), None, 100).unwrap();
    let one = whereabouts(&world, &at(5, 0), Some(&selected(HERO)), 100).unwrap();
    let hero = entry(&all, HERO);
    assert_eq!(hero["importance"], entry(&one, HERO)["importance"]);
    assert_eq!(
        hero["importance"]["raw"],
        json!({"scenes":1,"pointOfViewScenes":1,"events":1,"relationshipNeighbors":1})
    );
    assert_eq!(
        hero["importance"]["contributions"],
        json!({"scenes":20.0,"pointOfViewScenes":15.77,"events":12.62,"relationshipNeighbors":15.0})
    );
    assert_eq!(hero["importance"]["score"], 63.39);
    assert_eq!(entry(&all, OTHER)["importance"]["score"], 100.0);
    assert_eq!(one["activeScenes"].as_array().unwrap().len(), 1);
    assert_eq!(
        one["activeScenes"][0]["characters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(one["activeScenes"][0]["characters"][0]["id"], HERO);
    assert!(!one.to_string().contains(OTHER));
    assert!(!one.to_string().contains(HIDDEN));
}

#[test]
fn timeline_scopes_do_not_share_event_scene_or_neighbor_evidence() {
    let mut world = fixture();
    let other_time = json!({"timeline":"alternate","tick":0,"order":0});
    add(
        &mut world,
        relationship(
            "rel-alternate",
            HERO,
            OTHER,
            vec![other_time.clone(), time(20, 0)],
        ),
    );
    add(
        &mut world,
        event(
            "event-alternate",
            other_time,
            json!([]),
            json!([{"character":HERO}]),
        ),
    );
    let main = whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&main, HERO)["importance"]["raw"]["events"], 0);
    assert_eq!(
        entry(&main, HERO)["importance"]["raw"]["relationshipNeighbors"],
        0
    );
    let alternate = whereabouts(
        &world,
        &StoryTime {
            timeline: "alternate".into(),
            tick: 0,
            order: 0,
        },
        Some(&selected(HERO)),
        10,
    )
    .unwrap();
    assert_eq!(entry(&alternate, HERO)["importance"]["raw"]["events"], 1);
    assert_eq!(
        entry(&alternate, HERO)["importance"]["raw"]["relationshipNeighbors"],
        1
    );
}

#[test]
fn hidden_and_future_evidence_cannot_change_an_admitted_projection() {
    let mut world = fixture();
    add(
        &mut world,
        movement("event-withheld-place", time(1, 0), Some(SECRET)),
    );
    let mut hidden_event = movement("event-hidden", time(1, 0), Some(B));
    hidden_event.frontmatter["audience"] = serde_yaml::to_value(json!(["withheld"])).unwrap();
    add(&mut world, hidden_event);
    add(&mut world, movement("event-future", time(11, 0), Some(B)));
    let mut hidden_scene = scene(
        "scene-hidden",
        "active",
        time(0, 0),
        time(20, 0),
        None,
        json!([{"character":HERO,"point_of_view":true}]),
    );
    hidden_scene.frontmatter["audience"] = serde_yaml::to_value(json!(["withheld"])).unwrap();
    add(&mut world, hidden_scene);
    let mut front = scene(
        "scene-front",
        "active",
        time(0, 0),
        time(20, 0),
        None,
        json!([{"character":HERO}]),
    );
    front.frontmatter["location"] = serde_yaml::Value::String(SECRET.into());
    add(&mut world, front);
    add(
        &mut world,
        scene(
            "scene-future-entrance",
            "active",
            time(0, 0),
            time(20, 0),
            None,
            json!([{"character":HERO,"from":time(15,0),"point_of_view":true}]),
        ),
    );
    let before = whereabouts(&world, &at(10, 0), None, 100).unwrap();
    assert_eq!(entry(&before, HERO)["location"]["id"], A);
    assert_eq!(entry(&before, HERO)["importance"]["raw"]["events"], 0);
    assert!(before["activeScenes"][0]["location"].is_null());
    let serialized = before.to_string();
    for hidden in [
        SECRET,
        HIDDEN,
        "event-hidden",
        "event-future",
        "scene-hidden",
        "scene-future-entrance",
        "SECRET",
    ] {
        assert!(!serialized.contains(hidden), "leaked {hidden}");
    }
    for id in [
        SECRET,
        HIDDEN,
        "event-hidden",
        "event-future",
        "scene-hidden",
        "scene-future-entrance",
    ] {
        world.records.get_mut(id).unwrap().title = format!("CHANGED SECRET {id}");
        world.records.get_mut(id).unwrap().body = "CHANGED SECRET PROSE".repeat(200);
    }
    replace(
        &mut world,
        "scene-future-entrance",
        "participants",
        json!([{"character":HERO,"from":time(18,0),"point_of_view":false}]),
    );
    assert_eq!(whereabouts(&world, &at(10, 0), None, 100).unwrap(), before);
}

#[test]
fn future_scalar_reference_effect_is_withheld_before_event_prominence() {
    let mut world = fixture();
    add(
        &mut world,
        event("event-not-yet", time(20, 0), json!([]), json!([])),
    );
    add(
        &mut world,
        event(
            "event-remember",
            time(1, 0),
            json!([
                {"target":HERO,"key":"memory","operation":"set","value":{"entity":"event-not-yet"}}
            ]),
            json!([]),
        ),
    );
    let before = whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&before, HERO)["importance"]["raw"]["events"], 0);
    let after = whereabouts(&world, &at(20, 0), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&after, HERO)["importance"]["raw"]["events"], 1);
}

#[test]
fn history_reference_effects_obey_exact_first_transition_and_hidden_mutation_invariance() {
    let mut world = fixture();
    replace(
        &mut world,
        WORLD,
        "state_keys",
        json!({"character":{
            "location":{"type":"entity","kind":"location"},
            "memory":{"type":"entity"}
        }}),
    );
    add(
        &mut world,
        relationship("relationship-not-yet", HERO, OTHER, vec![time(10, 2)]),
    );
    add(
        &mut world,
        record(
            "knowledge",
            "knowledge-not-yet",
            "Future knowledge",
            json!({"knower":HERO,"claim":{"key":"gate.open","statement":"The gate is open"},
                "transitions":[{"id":"learned","time":time(10,1),"state":"accepted",
                    "confidence":1.0,"acquisition":"observed"}]}),
        ),
    );
    for id in ["relationship-not-yet", "knowledge-not-yet"] {
        let mut hidden = world.records[id].clone();
        hidden.id = format!("hidden-{id}");
        hidden.frontmatter["id"] = serde_yaml::Value::String(hidden.id.clone());
        hidden.frontmatter["audience"] = serde_yaml::to_value(json!(["withheld"])).unwrap();
        hidden.frontmatter["transitions"][0]["time"] = serde_yaml::to_value(time(1, 0)).unwrap();
        add(&mut world, hidden);
        for target in [id.to_owned(), format!("hidden-{id}")] {
            add(
                &mut world,
                event(
                    &format!("event-remember-{target}"),
                    time(1, 0),
                    json!([{"target":HERO,"key":"memory","operation":"set","value":{"entity":target}}]),
                    json!([]),
                ),
            );
        }
    }
    let before = whereabouts(&world, &at(10, 0), None, 100).unwrap();
    assert_eq!(entry(&before, HERO)["importance"]["raw"]["events"], 0);
    for id in [
        "relationship-not-yet",
        "knowledge-not-yet",
        "hidden-relationship-not-yet",
        "hidden-knowledge-not-yet",
    ] {
        let record = world.records.get_mut(id).unwrap();
        record.title = format!("CHANGED SECRET {id}");
        record.body = "CHANGED SECRET PROSE".repeat(100);
        let transitions = record.frontmatter["transitions"].as_sequence_mut().unwrap();
        let mut later = transitions[0].clone();
        later["id"] = serde_yaml::Value::String("changed-later".into());
        later["time"] = serde_yaml::to_value(time(11, 0)).unwrap();
        transitions.push(later);
        if id.starts_with("hidden-") {
            transitions[0]["time"] = serde_yaml::to_value(time(0, 0)).unwrap();
        }
    }
    assert_eq!(whereabouts(&world, &at(10, 0), None, 100).unwrap(), before);
    let knowledge_only = whereabouts(&world, &at(10, 1), None, 100).unwrap();
    assert_eq!(
        entry(&knowledge_only, HERO)["importance"]["raw"]["events"],
        1
    );
    let both = whereabouts(&world, &at(10, 2), None, 100).unwrap();
    assert_eq!(entry(&both, HERO)["importance"]["raw"]["events"], 2);
}

#[test]
fn literal_and_untyped_profile_values_named_location_are_not_placement() {
    let mut literal = fixture();
    replace(
        &mut literal,
        WORLD,
        "state_keys",
        json!({"character":{"location":{"type":"string"}}}),
    );
    replace(&mut literal, HERO, "initial_state", json!({"location":A}));
    replace(&mut literal, OTHER, "initial_state", json!({"location":B}));
    let read = whereabouts(&literal, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    assert!(entry(&read, HERO)["location"].is_null());
    assert_eq!(entry(&read, HERO)["journey"], json!([]));

    let mut bound = fixture();
    replace(&mut bound, WORLD, "capabilities", json!(["lore-core-v1"]));
    replace(
        &mut bound,
        WORLD,
        "profiles",
        json!([{"id":"literal-placement","label":"Literal placement",
        "fields":{"type":"object","required":{"entity":{"type":"string"}},"optional":{}}}]),
    );
    replace(
        &mut bound,
        WORLD,
        "state_profile_bindings",
        json!({"character":{"location":"literal-placement"}}),
    );
    add(&mut bound, movement("event-profile", time(1, 0), Some(B)));
    let read = whereabouts(&bound, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    assert!(entry(&read, HERO)["location"].is_null());
    assert_eq!(entry(&read, HERO)["journey"], json!([]));
    assert_eq!(entry(&read, HERO)["importance"]["raw"]["events"], 1);
}

fn bind_location(world: &mut World, fields: Json) {
    replace(world, WORLD, "capabilities", json!(["lore-core-v1"]));
    replace(
        world,
        WORLD,
        "profiles",
        json!([{"id":"placement","label":"Placement","fields":fields}]),
    );
    replace(
        world,
        WORLD,
        "state_profile_bindings",
        json!({"character":{"location":"placement"}}),
    );
    for (character, place) in [(HERO, A), (OTHER, B), (HIDDEN, SECRET)] {
        replace(
            world,
            character,
            "initial_state",
            json!({"location":{"entity":{"kind":"record","id":place}}}),
        );
    }
}

fn typed_placement(optional: bool) -> Json {
    let entity = json!({"type":"entity","kinds":["location","character"]});
    if optional {
        json!({"type":"object","required":{},"optional":{"entity":entity,"destination":entity}})
    } else {
        json!({"type":"object","required":{"entity":entity},"optional":{}})
    }
}

fn bound_movement(id: &str, when: Json, value: Json) -> Record {
    event(
        id,
        when,
        json!([{"target":HERO,"key":"location","operation":"set","value":value}]),
        json!([]),
    )
}

#[test]
fn typed_profile_location_replays_set_reaffirmation_and_clear_without_generic_state_exposure() {
    let mut world = fixture();
    bind_location(&mut world, typed_placement(false));
    let tick = 9_007_199_254_740_993;
    for (id, order) in [("event-bound-move", 1), ("event-bound-reaffirm", 2)] {
        add(
            &mut world,
            bound_movement(
                id,
                time(tick, order),
                json!({"entity":{"kind":"record","id":B}}),
            ),
        );
    }
    add(
        &mut world,
        movement("event-bound-clear", time(tick, 3), None),
    );
    let result = whereabouts(&world, &at(tick, 3), Some(&selected(HERO)), 10).unwrap();
    let hero = entry(&result, HERO);
    let kinds: Vec<_> = hero["journey"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["initial", "move", "reaffirmation", "clear"]);
    assert_eq!(hero["journey"][0]["to"]["id"], A);
    assert_eq!(hero["journey"][1]["to"]["id"], B);
    assert_eq!(hero["journey"][2]["at"]["tick"], tick.to_string());
    assert!(hero["location"].is_null());
    assert_eq!(hero["lastKnownLocation"]["id"], B);
    let during = whereabouts(&world, &at(tick, 2), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&during, HERO)["location"]["id"], B);
    let generic = wedl::runtime::state(&world, HERO, &at(tick, 2)).unwrap();
    assert!(generic["state"].get("location").is_none());
    assert!(generic["citations"].get("location").is_none());
}

#[test]
fn profile_placement_never_guesses_literal_sibling_nested_or_wrong_kind_leaves() {
    let typed = json!({"type":"entity","kinds":["location"]});
    let reference = json!({"kind":"record","id":A});
    let cases = vec![
        (
            json!({"type":"object","required":{"entity":{"type":"string"}},"optional":{}}),
            json!({"entity":A}),
        ),
        (
            json!({"type":"object","required":{"entity":{"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}}},"optional":{}}),
            json!({"entity":reference}),
        ),
        (
            json!({"type":"object","required":{"destination":typed},"optional":{}}),
            json!({"destination":reference}),
        ),
        (
            json!({"type":"object","required":{"placement":{"type":"object","required":{"entity":typed},"optional":{}}},"optional":{}}),
            json!({"placement":{"entity":reference}}),
        ),
        (
            json!({"type":"object","required":{"entity":{"type":"entity","kinds":["character"]}},"optional":{}}),
            json!({"entity":{"kind":"record","id":HERO}}),
        ),
    ];
    for (fields, value) in cases {
        let mut world = fixture();
        bind_location(&mut world, fields);
        for id in [HERO, OTHER, HIDDEN] {
            replace(&mut world, id, "initial_state", json!({"location":value}));
        }
        let read = whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
        assert!(
            entry(&read, HERO)["location"].is_null(),
            "interpreted {value}"
        );
        assert_eq!(entry(&read, HERO)["journey"], json!([]));
    }
}

#[test]
fn optional_profile_leaf_absence_removes_operative_location_without_guessing_a_sibling() {
    let mut world = fixture();
    bind_location(&mut world, typed_placement(true));
    add(
        &mut world,
        bound_movement(
            "event-no-leaf",
            time(1, 0),
            json!({"destination":{"kind":"record","id":B}}),
        ),
    );
    let result = whereabouts(&world, &at(1, 0), Some(&selected(HERO)), 10).unwrap();
    let hero = entry(&result, HERO);
    assert!(hero["location"].is_null());
    assert_eq!(hero["lastKnownLocation"]["id"], A);
    assert_eq!(hero["journey"][1]["kind"], "move");
    assert!(hero["journey"][1]["to"].is_null());
    assert!(!result.to_string().contains(B));
    add(
        &mut world,
        bound_movement(
            "event-place-again",
            time(2, 0),
            json!({"entity":{"kind":"record","id":B}}),
        ),
    );
    let later = whereabouts(&world, &at(2, 0), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&later, HERO)["location"]["id"], B);
    assert_eq!(entry(&later, HERO)["journey"][2]["kind"], "move");
    assert!(entry(&later, HERO)["journey"][2]["from"].is_null());
    replace(&mut world, HERO, "initial_state", json!({"location":{}}));
    let before = whereabouts(&world, &at(0, 0), Some(&selected(HERO)), 10).unwrap();
    assert_eq!(entry(&before, HERO)["journey"], json!([]));
}

#[test]
fn typed_profile_requires_an_actual_location_and_refuses_malformed_or_missing_references() {
    let mut world = fixture();
    bind_location(&mut world, typed_placement(false));
    add(
        &mut world,
        bound_movement(
            "event-character-leaf",
            time(1, 0),
            json!({"entity":{"kind":"record","id":OTHER}}),
        ),
    );
    let read = whereabouts(&world, &at(1, 0), Some(&selected(HERO)), 10).unwrap();
    assert!(entry(&read, HERO)["location"].is_null());
    assert_eq!(entry(&read, HERO)["lastKnownLocation"]["id"], A);
    for value in [
        json!({"entity":A}),
        json!({"entity":{"kind":"record","id":A,"extra":true}}),
        json!({"entity":{"kind":"unknown","id":A}}),
        json!({"entity":{"kind":"record","id":"loc_00000000000000000000000999"}}),
        json!({}),
    ] {
        let mut malformed = fixture();
        bind_location(&mut malformed, typed_placement(false));
        replace(
            &mut malformed,
            HERO,
            "initial_state",
            json!({"location":value}),
        );
        assert!(
            whereabouts(&malformed, &at(0, 0), Some(&selected(HERO)), 10).is_err(),
            "accepted {value}"
        );
    }
}

#[test]
fn hidden_profile_location_is_withheld_before_journey_and_prominence_projection() {
    let mut world = fixture();
    bind_location(&mut world, typed_placement(false));
    let before = whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    add(
        &mut world,
        bound_movement(
            "event-bound-hidden",
            time(1, 0),
            json!({"entity":{"kind":"record","id":SECRET}}),
        ),
    );
    assert_eq!(
        whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap(),
        before
    );
    world.records.get_mut(SECRET).unwrap().title = "CHANGED HIDDEN LOCATION".into();
    assert_eq!(
        whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap(),
        before
    );
    replace(
        &mut world,
        HERO,
        "initial_state",
        json!({"location":{"entity":{"kind":"record","id":SECRET}}}),
    );
    let hidden_initial = whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    assert!(entry(&hidden_initial, HERO)["location"].is_null());
    assert!(entry(&hidden_initial, HERO)["lastKnownLocation"].is_null());
    assert_eq!(entry(&hidden_initial, HERO)["journey"], json!([]));
    assert!(!hidden_initial.to_string().contains(SECRET));
}

#[test]
fn selection_is_exact_unique_available_and_bounded() {
    let world = fixture();
    for ids in [
        vec![],
        vec![HERO.to_owned(), HERO.to_owned()],
        vec!["Ada".into()],
        vec![HIDDEN.into()],
        vec![A.into()],
        vec!["missing".into()],
        vec![" ".into()],
        vec![HERO.to_owned(); 33],
    ] {
        assert!(
            whereabouts(&world, &at(10, 0), Some(&ids), 100).is_err(),
            "accepted {ids:?}"
        );
    }
    for limit in [0, 1001, usize::MAX] {
        assert!(whereabouts(&world, &at(10, 0), None, limit).is_err());
    }
    assert!(
        whereabouts(
            &world,
            &StoryTime {
                timeline: "undeclared".into(),
                tick: 0,
                order: 0
            },
            None,
            100
        )
        .is_err()
    );
    let ids = vec![OTHER.to_owned(), HERO.to_owned()];
    let result = whereabouts(&world, &at(10, 0), Some(&ids), 10).unwrap();
    assert_eq!(result["characters"][0]["character"]["id"], HERO);
    assert_eq!(result["characterFilter"][1]["id"], OTHER);
}

#[test]
fn incomplete_results_are_refused_instead_of_silently_truncated() {
    let mut world = fixture();
    assert!(
        whereabouts(&world, &at(10, 0), None, 1)
            .unwrap_err()
            .to_string()
            .contains("characters")
    );
    assert!(whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 1).is_ok());
    add(&mut world, movement("event-one", time(1, 0), Some(B)));
    assert!(
        whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 1)
            .unwrap_err()
            .to_string()
            .contains("journey entries")
    );
    for index in 0..1000 {
        add(
            &mut world,
            record(
                "character",
                &format!("cohort-{index}"),
                "Extra admitted character",
                json!({}),
            ),
        );
    }
    assert!(
        whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10)
            .unwrap_err()
            .to_string()
            .contains("cohort")
    );
    for index in 0..1000 {
        replace(
            &mut world,
            &format!("cohort-{index}"),
            "audience",
            json!(["withheld"]),
        );
    }
    assert!(whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10).is_ok());
}

#[test]
fn complete_evidence_and_serialized_byte_budgets_refuse_oversize_reads() {
    let mut world = fixture();
    let mut participants = vec![json!({"character":OTHER}), json!({"character":UNKNOWN})];
    for index in 10..18 {
        let id = format!("char_{index:026}");
        add(
            &mut world,
            record("character", &id, "Additional cohort member", json!({})),
        );
        participants.push(json!({"character":id}));
    }
    let prototype = event(
        "event-prototype",
        time(1, 0),
        json!([]),
        json!(participants),
    );
    for index in 0..1001 {
        let mut item = prototype.clone();
        item.id = format!("event-{index:05}");
        item.frontmatter["id"] = serde_yaml::Value::String(item.id.clone());
        add(&mut world, item);
    }
    assert!(
        whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10)
            .unwrap_err()
            .to_string()
            .contains("evidence")
    );
    let mut world = fixture();
    world.records.get_mut(HERO).unwrap().title = "x".repeat(4 * 1024 * 1024 + 1);
    assert!(
        whereabouts(&world, &at(10, 0), Some(&selected(HERO)), 10)
            .unwrap_err()
            .to_string()
            .contains("4 MiB")
    );
}

#[test]
fn source_reads_preserve_all_bytes_mtimes_and_directory_contents() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("story")).unwrap();
    let fixture = fixture();
    for record in fixture.records.values() {
        let exact = String::from_utf8(record.raw_bytes.clone())
            .unwrap()
            .replace('\n', "\r\n")
            + "  \r\n";
        fs::write(directory.path().join(&record.source_path), exact).unwrap();
    }
    let before: BTreeMap<_, _> = fixture
        .records
        .values()
        .map(|record| {
            let path = directory.path().join(&record.source_path);
            (
                path.clone(),
                (
                    fs::read(&path).unwrap(),
                    fs::metadata(&path).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect();
    let loaded = source::load_world(directory.path(), None).unwrap();
    let before_source: Vec<_> = loaded
        .records
        .values()
        .map(|record| (record.frontmatter.clone(), record.raw_bytes.clone()))
        .collect();
    whereabouts(&loaded, &at(10, 0), None, 100).unwrap();
    whereabouts(&loaded, &at(10, 0), Some(&selected(HERO)), 10).unwrap();
    for (path, (bytes, mtime)) in before {
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    }
    assert_eq!(
        loaded
            .records
            .values()
            .map(|record| (record.frontmatter.clone(), record.raw_bytes.clone()))
            .collect::<Vec<_>>(),
        before_source
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
