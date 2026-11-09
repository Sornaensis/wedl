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
pub fn id(kind: &str, n: usize) -> String {
    format!("{kind}_{n:026}")
}
pub fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
pub fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
pub fn rr(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
pub fn interval(start: i64, end: i64) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start,0),"end":point(end,0)})
}
pub fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut data =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("AUTHOR_TITLE_{id}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let bytes = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        body
    );
    source::parse_record(bytes.as_bytes(), &format!("story/{id}.md")).unwrap()
}
pub fn from(records: Vec<Record>) -> World {
    source::world_from_records(
        PathBuf::new(),
        records,
        Some("family-character-fixture".into()),
    )
    .unwrap()
}
pub fn add(world: &World, record: Record) -> World {
    let mut records = world
        .records
        .values()
        .filter(|r| r.id != record.id)
        .cloned()
        .collect::<Vec<_>>();
    records.push(record);
    from(records)
}
pub fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut data = old.frontmatter.clone();
    change(&mut data);
    let bytes = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        old.body
    );
    add(
        world,
        source::parse_record(bytes.as_bytes(), &old.source_path).unwrap(),
    )
}
pub fn set(world: &World, id: &str, field: &str, value: Json) -> World {
    edit(world, id, |data| {
        data[field] = serde_yaml::to_value(value).unwrap()
    })
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
pub fn section(n: usize, text: &str) -> Json {
    json!({"id":id("description",n),"heading":format!("Dated account {n}"),"markdown":text,
        "valid":interval(0,20),"audience":["author","public"],"perspectives":["author","ordinary"]})
}
pub fn transition(tick: i64, state: &str) -> Json {
    json!({"id":format!("t{tick}"),"time":point(tick,0),"state":state,"confidence":1,
        "acquisition":"heard","note":"PRIVATE_ACQUISITION_NOTE"})
}
pub fn acquisition(n: usize, target: &str) -> Record {
    record(
        "knowledge",
        &id("know", n),
        json!({"knower":VIEWER,
        "claim":{"key":format!("known-{n}"),"statement":"Acquired record identity","object":{"entity":target}},
        "transitions":[transition(5,"accepted")]}),
        "PRIVATE_KNOWLEDGE_BODY",
    )
}
pub fn learned(n: usize, assertion: &str, statement: &str) -> Record {
    record(
        "knowledge",
        &id("know", n),
        json!({"knower":VIEWER,
        "claim":{"key":format!("learned-{n}"),"statement":statement,"assertion_id":assertion},
        "transitions":[transition(5,"accepted"),transition(6,"rejected"),transition(8,"forgotten"),transition(10,"remembered")]}),
        "PRIVATE_LINKED_BODY",
    )
}
pub fn concept(n: usize, profile: &str, values: Json) -> Record {
    record(
        "lore-concept",
        &id("concept", n),
        json!({"profile":profile,"domain":"authored.family",
        "valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"],
        "values":values,"descriptions":[section(1,&format!("Explicit section {n}\r\n"))],
        "provenance":"PRIVATE_CANONICAL_PROVENANCE"}),
        "PRIVATE_CANONICAL_BODY\r\n",
    )
}
pub fn assertion(n: usize, subject: &str, value: &str) -> Record {
    record(
        "assertion",
        &id("assertion", n),
        json!({"domain":"authored.family","subject":rr(subject),
        "predicate":"has-account","value":value,"classification":"author-fact","valid":{"kind":"timeless"},
        "audience":["author"],"perspectives":["author"],"evidence":[]}),
        "PRIVATE_ASSERTION_BODY",
    )
}
pub fn base() -> World {
    let entity = json!({"type":"entity","kinds":["lore-concept"]});
    let fields = json!({"type":"object","required":{},"optional":{
        "traits":{"type":"string"},"common":entity,"parents":{"type":"array","items":entity,"max_items":1024},
        "links":{"type":"array","items":entity,"max_items":1024},"restrictions":{"type":"string"},
        "exceptions":{"type":"array","items":{"type":"string"},"max_items":8},
        "number":{"type":"number"},"choice":{"type":"enum","values":[1,"1"]},"nothing":{"type":"null"},
        "literal":{"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}}
    }});
    let profiles = [
        "common-description",
        "individual-variation",
        "other-profile",
        "magic-practice",
        "magic-tradition",
    ]
    .iter()
    .map(|id| json!({"id":id,"label":id,"fields":fields}))
    .collect::<Vec<_>>();
    from(vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "state_keys":{},"capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],"profiles":profiles,
            "predicates":[{"id":"has-account","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}]}),
            "WORLD_BODY",
        ),
        record("character", VIEWER, json!({}), "PRIVATE_VIEWER_BODY"),
        record("character", OTHER, json!({}), "PRIVATE_OTHER_BODY"),
        concept(
            1,
            "common-description",
            json!({"traits":"Common words","parents":[rr(&id("concept",3)),rr(&id("concept",4))],"number":9007199254740993u64,"choice":1}),
        ),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Individual words","common":rr(&id("concept",1)),"parents":[rr(&id("concept",3)),rr(&id("concept",4))],"choice":"1","nothing":null}),
        ),
        concept(
            3,
            "common-description",
            json!({"traits":"HIDDEN_WORDS","parents":[rr(&id("concept",2))]}),
        ),
        concept(
            4,
            "common-description",
            json!({"traits":"Second common words","parents":[]}),
        ),
        concept(
            10,
            "magic-practice",
            json!({"restrictions":"Current canonical practice","links":[rr(&id("concept",11))],"exceptions":[]}),
        ),
        concept(
            11,
            "magic-tradition",
            json!({"restrictions":"Current canonical tradition","links":[],"exceptions":[]}),
        ),
        acquisition(1, &id("concept", 1)),
        acquisition(2, &id("concept", 2)),
        acquisition(4, &id("concept", 4)),
        acquisition(10, &id("concept", 10)),
        assertion(1, &id("concept", 1), "CANONICAL_TRUE"),
        assertion(2, &id("concept", 10), "CANONICAL_MAGIC"),
        learned(100, &id("assertion", 1), "A retained mistaken explanation"),
        learned(101, &id("assertion", 2), "The learned magic explanation"),
    ])
}
pub fn species_selection() -> wedl::species::Selection {
    wedl::species::Selection {
        common_profile: "common-description".into(),
        individual_profile: "individual-variation".into(),
        individual_link: "common".into(),
        common_link: Some("parents".into()),
        common_fields: BTreeMap::from([
            ("traits".into(), "traits".into()),
            ("life_stage".into(), "number".into()),
            ("language".into(), "choice".into()),
        ]),
        individual_fields: BTreeMap::from([
            ("traits".into(), "traits".into()),
            ("habitat".into(), "parents".into()),
            ("language".into(), "choice".into()),
            ("culture".into(), "nothing".into()),
        ]),
    }
}
pub fn magic_selection() -> wedl::magic::Selection {
    wedl::magic::Selection {
        profiles: BTreeMap::from([
            ("practice".into(), "magic-practice".into()),
            ("tradition".into(), "magic-tradition".into()),
        ]),
        links: BTreeMap::from([
            ("practice".into(), vec!["links".into()]),
            ("tradition".into(), vec!["links".into()]),
        ]),
        restrictions: BTreeMap::from([
            ("practice".into(), "restrictions".into()),
            ("tradition".into(), "restrictions".into()),
        ]),
        exceptions: BTreeMap::from([
            ("practice".into(), "exceptions".into()),
            ("tradition".into(), "exceptions".into()),
        ]),
    }
}
pub fn no_author_material(value: &Json) {
    match value {
        Json::Object(values) => {
            for (key, child) in values {
                assert!(
                    ![
                        "title",
                        "body",
                        "bodyKind",
                        "provenance",
                        "citation",
                        "sourceField",
                        "sourcePath",
                        "blobOid",
                        "fingerprint",
                        "revision",
                        "acquisition",
                        "note"
                    ]
                    .contains(&key.as_str()),
                    "forbidden key {key}"
                );
                no_author_material(child);
            }
        }
        Json::Array(values) => {
            for child in values {
                no_author_material(child);
            }
        }
        Json::String(value) => {
            for secret in [
                "AUTHOR_TITLE_",
                "PRIVATE_",
                "HIDDEN_WORDS",
                "CANONICAL_TRUE",
                "CANONICAL_MAGIC",
            ] {
                assert!(!value.contains(secret), "forbidden prose {value}");
            }
        }
        _ => {}
    }
}
