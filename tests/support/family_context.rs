#![allow(dead_code)]
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    model::{Record, StoryTime, World},
    source, validation,
};

pub const WORLD: &str = "world_00000000000000000000000000";
pub const VIEWER: &str = "char_00000000000000000000000001";
pub const OTHER: &str = "char_00000000000000000000000002";
pub const SCENE: &str = "scene_00000000000000000000000001";
pub const PLACE: &str = "loc_00000000000000000000000001";

pub fn id(kind: &str, n: usize) -> String {
    format!("{kind}_{n:026}")
}
pub fn concept_id(n: usize) -> String {
    id("concept", n)
}
pub fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
pub fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
pub fn reference(n: usize) -> Json {
    json!({"kind":"record","id":concept_id(n)})
}
pub fn interval(start: i64, end: i64) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start,0),"end":point(end,0)})
}
pub fn section(n: usize, text: &str) -> Json {
    json!({"id":id("description",n),"heading":format!("Dated section {n}"),"markdown":text,
        "valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"]})
}
pub fn record(kind: &str, identifier: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":identifier,"title":format!("AUTHOR_TITLE_{identifier}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nAUTHOR_BODY_{identifier}\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{identifier}.md")).unwrap()
}
pub fn concept(n: usize, profile: &str, values: Json) -> Record {
    record(
        "lore-concept",
        &concept_id(n),
        json!({"domain":"family.example","profile":profile,
        "values":values,"valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"],
        "descriptions":[section(n,&format!("Dated family account {n}"))],"provenance":"AUTHOR_PROVENANCE"}),
    )
}
pub fn knowledge(n: usize, claim: Json, tick: i64) -> Record {
    record(
        "knowledge",
        &id("know", n),
        json!({"knower":VIEWER,"claim":claim,
        "transitions":[{"id":"learned","time":point(tick,0),"state":"accepted","confidence":0.75,"acquisition":"heard","note":"AUTHOR_ACQUISITION"}]}),
    )
}
pub fn acquire(n: usize, target: &str) -> Record {
    knowledge(
        n,
        json!({"key":format!("known-{n}"),"statement":format!("I recognize item {n}"),"object":{"entity":target}}),
        2,
    )
}
pub fn from_records(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, Some("a".repeat(40))).unwrap()
}
pub fn valid(world: &World) {
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
pub fn edit(world: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[target];
    let mut fields = old.frontmatter.clone();
    change(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        old.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &old.source_path).unwrap();
    from_records(
        world
            .records
            .values()
            .map(|r| {
                if r.id == target {
                    replacement.clone()
                } else {
                    r.clone()
                }
            })
            .collect(),
    )
}
pub fn set(world: &World, target: &str, field: &str, value: Json) -> World {
    edit(world, target, |data| {
        data[field] = serde_yaml::to_value(value).unwrap()
    })
}
pub fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    from_records(records)
}
pub fn remove(world: &World, target: &str) -> World {
    from_records(
        world
            .records
            .values()
            .filter(|r| r.id != target)
            .cloned()
            .collect(),
    )
}
pub fn social_selection() -> wedl::social::Selection {
    wedl::social::Selection {
        language: concept_id(1),
        sense: Some(concept_id(2)),
        form: None,
        contexts: vec![concept_id(6)],
    }
}
pub fn species_selection() -> wedl::species::Selection {
    wedl::species::Selection {
        common_profile: "creature-common".into(),
        individual_profile: "creature-individual".into(),
        individual_link: "common".into(),
        common_link: Some("common".into()),
        common_fields: BTreeMap::from([("traits".into(), "traits".into())]),
        individual_fields: BTreeMap::from([("traits".into(), "traits".into())]),
    }
}
pub fn magic_selection() -> wedl::magic::Selection {
    wedl::magic::Selection {
        profiles: BTreeMap::from([("tradition".into(), "spell-tradition".into())]),
        links: BTreeMap::new(),
        restrictions: BTreeMap::from([("tradition".into(), "restrictions".into())]),
        exceptions: BTreeMap::from([("tradition".into(), "exceptions".into())]),
    }
}
pub fn families() -> Json {
    json!({"species":{"individualId":concept_id(20),"selection":species_selection(),"beliefTargets":[id("assertion",90)]},
        "magic":{"author":{"rootId":concept_id(30),"selection":magic_selection()},"beliefTargets":[id("assertion",90)]},
        "social":{"language":concept_id(1),"sense":concept_id(2),"form":null,"contexts":[concept_id(6)],"beliefTargets":[id("know",90)]},
        "environment":{"contexts":[concept_id(10)],"beliefTargets":[id("assertion",90)]}})
}
pub fn world() -> World {
    let entity = json!({"type":"entity","kinds":["lore-concept"]});
    let entities = json!({"type":"array","max_items":1024,"items":entity});
    let literal = json!({"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}});
    let profile = |name: &str, required: Json, optional: Json| json!({"id":name,"label":"AUTHOR_PROFILE_LABEL","fields":{"type":"object","required":required,"optional":optional}});
    let environment_fields = json!({"resources":entities,"dependencies":entities,"note":{"type":"string"},"literal":literal});
    let profiles = vec![
        profile(
            "social-language",
            json!({}),
            json!({"scripts":entities,"note":{"type":"string"}}),
        ),
        profile(
            "social-sense",
            json!({"language":entity,"gloss":{"type":"string"}}),
            json!({}),
        ),
        profile(
            "social-era-spelling",
            json!({"language":entity,"sense":entity,"form":{"type":"string"}}),
            json!({"scripts":entities,"register":entity}),
        ),
        profile(
            "social-script",
            json!({}),
            json!({"note":{"type":"string"}}),
        ),
        profile(
            "social-register",
            json!({"language":entity,"label":{"type":"string"}}),
            json!({}),
        ),
        profile(
            "social-culture",
            json!({}),
            json!({"languages":entities,"note":{"type":"string"}}),
        ),
        profile("context-habitat", json!({}), environment_fields.clone()),
        profile("context-resource", json!({}), environment_fields.clone()),
        profile("context-technology", json!({}), environment_fields.clone()),
        profile("context-era", json!({}), environment_fields),
        profile(
            "creature-individual",
            json!({"common":entity,"traits":{"type":"string"}}),
            json!({}),
        ),
        profile(
            "creature-common",
            json!({"traits":{"type":"string"}}),
            json!({"common":entities}),
        ),
        profile(
            "spell-tradition",
            json!({"restrictions":{"type":"string"},"exceptions":{"type":"string"}}),
            json!({}),
        ),
    ];
    let mut records = vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":["lore-core-v1","descriptions-v1","assertions-v1","briefs-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"branch","label":"Branch"}],"state_keys":{},"relationship_metrics":{},
            "profiles":profiles,"predicates":[{"id":"context-availability","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
                {"id":"context-dependency-evidence","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}]}),
        ),
        record("character", VIEWER, json!({"initial_state":{}})),
        record("character", OTHER, json!({"initial_state":{}})),
        record("location", PLACE, json!({})),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","location":PLACE,"time":{"start":point(0,0),"current":point(5,0),"end":point(20,0)},"participants":[{"character":VIEWER}]}),
        ),
        concept(
            1,
            "social-language",
            json!({"scripts":[reference(4)],"note":"River language"}),
        ),
        concept(
            2,
            "social-sense",
            json!({"language":reference(1),"gloss":"The river bank"}),
        ),
        concept(
            3,
            "social-era-spelling",
            json!({"language":reference(1),"sense":reference(2),"form":"bank","scripts":[reference(4)],"register":reference(5)}),
        ),
        concept(4, "social-script", json!({"note":"Curved lettering"})),
        concept(
            5,
            "social-register",
            json!({"language":reference(1),"label":"Ceremonial"}),
        ),
        concept(
            6,
            "social-culture",
            json!({"languages":[reference(1)],"note":"Shared cultural account"}),
        ),
        concept(
            10,
            "context-habitat",
            json!({"resources":[reference(12),reference(11)],"note":"Flood plain","literal":{"kind":"record","id":concept_id(12)}}),
        ),
        concept(
            11,
            "context-resource",
            json!({"dependencies":[reference(14)],"note":"River clay"}),
        ),
        concept(12, "context-resource", json!({"note":"PRIVATE_RESOURCE"})),
        concept(13, "context-technology", json!({"note":"Raised kiln"})),
        concept(14, "context-era", json!({"note":"A separate authored era"})),
        concept(
            18,
            "context-habitat",
            json!({"note":"Selected-scene-only account"}),
        ),
        concept(
            20,
            "creature-individual",
            json!({"common":reference(21),"traits":"Individual shell mark"}),
        ),
        concept(
            21,
            "creature-common",
            json!({"common":[reference(22)],"traits":"Common shell account"}),
        ),
        concept(
            22,
            "creature-common",
            json!({"common":[],"traits":"PRIVATE_COMMON_TRAIT"}),
        ),
        concept(
            30,
            "spell-tradition",
            json!({"restrictions":"AUTHOR_MAGIC_RESTRICTION","exceptions":"AUTHOR_MAGIC_EXCEPTION"}),
        ),
        record(
            "assertion",
            &id("assertion", 90),
            json!({"domain":"family.claim","subject":reference(10),"predicate":"context-availability",
            "value":"AUTHOR_CANON_VALUE","classification":"author-fact","valid":{"kind":"timeless"},"evidence":[],"audience":["author"],"perspectives":["author"]}),
        ),
        knowledge(
            90,
            json!({"key":"learned-claim","statement":"I was told the kiln keeps its heat.","assertion_id":id("assertion",90)}),
            3,
        ),
        knowledge(
            91,
            json!({"key":"scene-subject","statement":"The scene identifies a habitat.","subject":concept_id(18)}),
            3,
        ),
        record(
            "context-brief",
            &id("brief", 80),
            json!({"domain":"family.brief","scope":"world","valid":{"kind":"timeless"},
            "audience":["author","public"],"perspectives":["author","ordinary"],
            "required_refs":[{"kind":"section","owner_id":concept_id(1),"id":id("description",1)}]}),
        ),
    ];
    for n in [1, 2, 3, 4, 5, 6, 10, 11, 12, 13, 14, 20, 21] {
        records.push(acquire(n, &concept_id(n)));
    }
    records.push(acquire(80, &id("brief", 80)));
    let world = from_records(records);
    let world = set(
        &world,
        &concept_id(10),
        "concept_links",
        json!([{"relation":"uses","concept_id":concept_id(13),"valid":{"kind":"timeless"}}]),
    );
    let world = set(&world, &concept_id(12), "audience", json!(["author"]));
    let world = set(&world, &concept_id(22), "audience", json!(["author"]));
    let world = set(&world, &concept_id(30), "audience", json!(["author"]));
    valid(&world);
    world
}
