//! Episode admission is shared by every narrative use of its current endpoint.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, assertion, description, existence, governance, lore,
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
const EPISODE_PERSON: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const EPISODE: &str = "existence_00000000000000000000000001";
const POLICY: &str = "concept_00000000000000000000000001";
const SECTION: &str = "description_00000000000000000000000001";
const ROOT_ASSERTION: &str = "assertion_00000000000000000000000001";
const SUBJECT_ASSERTION: &str = "assertion_00000000000000000000000002";
const NESTED_ASSERTION: &str = "assertion_00000000000000000000000003";
const TICK: i64 = 9_007_199_254_740_993;

fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn at(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nActual source bytes.\r\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn assertion(id: &str, subject: &str, predicate: &str, value: Json) -> Record {
    record(
        "assertion",
        id,
        json!({"domain":"authored.statement","subject":reference(subject),
        "predicate":predicate,"value":value,"classification":"author-fact","evidence":[],
        "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
    )
}
fn world() -> World {
    let endpoint = json!({"type":"entity","kinds":["existence-episode"]});
    let nested = json!({"type":"object","required":{"episodes":{"type":"array","items":endpoint,"max_items":8}},"optional":{}});
    let evidence = json!([{"revision":"a".repeat(40),"path":"old/removed.md","blob_oid":"b".repeat(40),
        "target":{"kind":"transition","owner_id":"char_00000000000000000000000999","id":"historical"}}]);
    let records = vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":["generational-core-v1","lore-core-v1","descriptions-v1","assertions-v1","existence-v1","collective-parties-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{},
            "profiles":[{"id":"episode-policy","label":"Explicit policy","fields":{"type":"object","required":{
                "area":{"type":"entity","kinds":["location"]},"issuer":{"type":"entity","kinds":["character"]},
                "basis":{"type":"enum","values":["legal"]},"duties":{"type":"object","required":{"episode":endpoint},"optional":{}}},
                "optional":{"ordinary":{"type":"entity","kinds":["character"]}}}}],
            "predicates":[{"id":"episode-root","subject_kinds":["lore-concept"],"value_type":endpoint},
                {"id":"episode-subject","subject_kinds":["existence-episode"],"value_type":{"type":"string"}},
                {"id":"episode-nested","subject_kinds":["lore-concept"],"value_type":nested}]}),
        ),
        record("character", PERSON, json!({})),
        record("character", EPISODE_PERSON, json!({})),
        record("location", PLACE, json!({})),
        record(
            "existence-episode",
            EPISODE,
            json!({"domain":"authored.existence","character_id":EPISODE_PERSON,"episode_kind":"embodied",
            "valid":{"kind":"interval","timeline":"main","start":{"timeline":"main","tick":TICK,"order":-1},"end":{"timeline":"main","tick":TICK,"order":1}},
            "start_evidence":evidence,"end_evidence":evidence,"audience":["author"],"perspectives":["author"],
            "descriptions":[{"id":SECTION,"heading":"Scoped episode text","markdown":"PROTECTED_EPISODE_DESCRIPTION",
                "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}],
            "concept_links":[{"relation":"account","concept_id":POLICY,"valid":{"kind":"timeless"}}]}),
        ),
        record(
            "lore-concept",
            POLICY,
            json!({"domain":"authored.governance","profile":"episode-policy",
            "values":{"area":reference(PLACE),"issuer":reference(PERSON),"basis":"legal","duties":{"episode":reference(EPISODE)},"ordinary":reference(EPISODE_PERSON)},
            "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
        ),
        assertion(ROOT_ASSERTION, POLICY, "episode-root", reference(EPISODE)),
        assertion(
            SUBJECT_ASSERTION,
            EPISODE,
            "episode-subject",
            json!("A scoped episode statement"),
        ),
        assertion(
            NESTED_ASSERTION,
            POLICY,
            "episode-nested",
            json!({"episodes":[reference(EPISODE)]}),
        ),
    ];
    let world = source::world_from_records(
        PathBuf::new(),
        records,
        Some("facts-admission-fixture".into()),
    )
    .unwrap();
    valid(&world);
    world
}
fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let original = &world.records[id];
    let mut data = original.frontmatter.clone();
    change(&mut data);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
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
        world.revision.clone(),
    )
    .unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn binding() -> governance::Binding {
    governance::Binding {
        profile: "episode-policy".into(),
        roles: [
            ("territory", Some("area")),
            ("party", Some("issuer")),
            ("basis", Some("basis")),
            ("subjects", None),
            ("obligations", Some("duties")),
            ("supersedes", None),
        ]
        .into_iter()
        .map(|(role, field)| (role.into(), field.map(str::to_owned)))
        .collect(),
    }
}
fn has_reference(value: &Json, endpoint: &str) -> bool {
    value["references"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["entityId"] == endpoint)
}
fn admitted(world: &World, at: Option<&StoryTime>) {
    assert_eq!(
        existence::list(world, EPISODE_PERSON, at, "author", "author", 100).unwrap()["count"],
        1
    );
    let descriptions = description::list(world, EPISODE, at, "author", "author", 100).unwrap();
    assert_eq!(descriptions["count"], 1);
    assert_eq!(
        descriptions["items"][0]["markdown"],
        "PROTECTED_EPISODE_DESCRIPTION"
    );
    let concept = lore::concept(world, POLICY, at, "author", "author").unwrap();
    assert_eq!(concept["values"]["duties"]["episode"], reference(EPISODE));
    assert!(has_reference(&concept, EPISODE));
    assert_eq!(
        lore::links(world, EPISODE, at, "author", "author", 100).unwrap()["count"],
        1
    );
    let statements = assertion::as_of(world, POLICY, at, "author", "author", 100).unwrap();
    assert_eq!(statements["count"], 2);
    assert_eq!(statements["items"][0]["assertionId"], ROOT_ASSERTION);
    assert!(has_reference(&statements["items"][0], EPISODE));
    assert_eq!(
        assertion::as_of(world, EPISODE, at, "author", "author", 100).unwrap()["count"],
        1
    );
    let result = governance::query(world, PLACE, &binding(), at, "author", "author", 100).unwrap();
    assert_eq!(result["count"], 1);
    assert_eq!(
        result["items"][0]["roles"]["obligations"]["episode"],
        reference(EPISODE)
    );
    assert_eq!(result["items"][0]["statements"], statements["items"]);
}
fn withheld(world: &World, at: Option<&StoryTime>) {
    valid(world);
    assert_eq!(
        existence::list(world, EPISODE_PERSON, at, "author", "author", 100).unwrap()["count"],
        0
    );
    assert!(matches!(
        description::list(world, EPISODE, at, "author", "author", 100),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        description::section(world, EPISODE, SECTION, at, "author", "author"),
        Err(Error::NotFound(_))
    ));
    let concept = lore::concept(world, POLICY, at, "author", "author").unwrap();
    assert_eq!(concept["values"]["duties"], json!({}));
    assert!(!has_reference(&concept, EPISODE));
    assert!(matches!(
        lore::links(world, EPISODE, at, "author", "author", 100),
        Err(Error::NotFound(_))
    ));
    let statements = assertion::as_of(world, POLICY, at, "author", "author", 100).unwrap();
    assert_eq!(statements["count"], 1);
    assert_eq!(statements["items"][0]["assertionId"], NESTED_ASSERTION);
    assert_eq!(statements["items"][0]["value"]["episodes"], json!([]));
    assert!(!has_reference(&statements["items"][0], EPISODE));
    assert_eq!(
        assertion::as_of(world, EPISODE, at, "author", "author", 100).unwrap()["count"],
        0
    );
    assert_eq!(
        governance::query(world, PLACE, &binding(), at, "author", "author", 100).unwrap()["count"],
        0
    );
    let mut unselected = binding();
    unselected.roles.insert("obligations".into(), None);
    let policy = governance::query(world, PLACE, &unselected, at, "author", "author", 100).unwrap();
    assert_eq!(policy["count"], 1);
    assert_eq!(policy["items"][0]["statements"], statements["items"]);
    assert!(!policy.to_string().contains(EPISODE));
}

#[test]
fn admitted_episode_endpoints_share_exact_inclusive_time_and_all_time_history() {
    let world = world();
    for point in [at(-1), at(0), at(1)] {
        admitted(&world, Some(&point));
    }
    admitted(&world, None);
    for point in [at(-2), at(2), StoryTime::new("other", TICK, 0).unwrap()] {
        withheld(&world, Some(&point));
    }
}

#[test]
fn draft_retired_and_episode_scope_exclusions_apply_to_every_narrative_consumer() {
    let world = world();
    for (field, value) in [
        ("status", json!("draft")),
        ("status", json!("retired")),
        ("audience", json!(["private"])),
        ("perspectives", json!(["private"])),
    ] {
        let changed = edit(&world, EPISODE, |data| {
            data[field] = serde_yaml::to_value(value).unwrap()
        });
        withheld(&changed, Some(&at(0)));
        withheld(&changed, None);
    }
}

#[test]
fn hidden_character_blocks_episode_endpoints_without_widening_ordinary_lore_rules() {
    let world = world();
    for (field, value) in [
        ("audience", json!(["private"])),
        ("perspectives", json!(["private"])),
        ("audience", json!([])),
    ] {
        let changed = edit(&world, EPISODE_PERSON, |data| {
            data[field] = serde_yaml::to_value(value).unwrap()
        });
        withheld(&changed, Some(&at(0)));
        withheld(&changed, None);
        // Ordinary lore endpoints keep their pre-existing author availability.
        let concept = lore::concept(&changed, POLICY, Some(&at(0)), "author", "author").unwrap();
        assert_eq!(concept["values"]["ordinary"], reference(EPISODE_PERSON));
        assert!(has_reference(&concept, EPISODE_PERSON));
    }
    let retired = edit(&world, EPISODE_PERSON, |data| {
        data["status"] = Yaml::String("retired".into())
    });
    admitted(&retired, Some(&at(0)));
}

#[test]
fn indirect_episode_reads_fail_closed_when_the_optional_capability_is_missing() {
    let changed = edit(&world(), WORLD, |data| {
        data["capabilities"]
            .as_sequence_mut()
            .unwrap()
            .retain(|cap| cap.as_str() != Some("existence-v1"))
    });
    assert!(description::list(&changed, EPISODE, None, "author", "author", 100).is_err());
    assert!(lore::concept(&changed, POLICY, None, "author", "author").is_err());
    assert!(assertion::as_of(&changed, POLICY, None, "author", "author", 100).is_err());
    assert!(governance::query(&changed, PLACE, &binding(), None, "author", "author", 100).is_err());
}
