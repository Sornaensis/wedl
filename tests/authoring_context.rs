//! Context is complete full-author guidance, pinned to the captured native bytes.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use wedl::{
    Error,
    authoring_tools::{self, MAX_ITEMS, MAX_RESPONSE_BYTES},
    model::{Record, World},
    record_edit, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ACTOR: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const HYPOTHESIS: &str = "hyp_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const RELATIONSHIP: &str = "rel_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const ORGANIZATION: &str = "organization_00000000000000000000000001";
const SECTION: &str = "description_00000000000000000000000001";
const INITIAL: &str = "transition_00000000000000000000000001";
const LATER: &str = "transition_00000000000000000000000002";
const BIG: i64 = 9_007_199_254_740_993;

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut value =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Authored {kind}")});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\r\n# Retained source comment\r\n{}---\r\nPRIVATE_BODY 🌲\r\nNo final newline  ",
        serde_yaml::to_string(&value).unwrap().replace('\n', "\r\n")
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn assemble(records: Vec<Record>) -> World {
    source::world_from_records(
        PathBuf::from("/authoring-context-does-not-read-disk"),
        records,
        Some("a".repeat(40)),
    )
    .unwrap()
}
fn base() -> World {
    assemble(vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":[],"timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "default_timeline":"main","current_time":{"timeline":"other","tick":BIG,"order":i32::MIN},"state_keys":{}}),
        ),
        record("character", ACTOR, json!({})),
        record("location", PLACE, json!({"status":"retired"})),
        record(
            "event",
            EVENT,
            json!({"status":"draft","time":{"timeline":"main","tick":BIG,"order":1}}),
        ),
        record(
            "hypothesis",
            HYPOTHESIS,
            json!({"status":"open","statement":"A possible explanation","subjects":[ACTOR],
            "alternatives":["One"],"context":"Private author choice","placement":{"context":"An idea"}}),
        ),
    ])
}
fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut fields = old.frontmatter.clone();
    change(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        old.body
    );
    let mut records = world.records.clone();
    records.insert(
        id.into(),
        source::parse_record(raw.as_bytes(), &old.source_path).unwrap(),
    );
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn read(world: &World) -> Json {
    valid(world);
    authoring_tools::context(world, MAX_ITEMS).unwrap()
}
fn image(world: &World) -> BTreeMap<String, Vec<u8>> {
    world
        .records
        .iter()
        .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
        .collect()
}
fn field<'a>(rows: &'a Json, key: &str, expected: &str) -> &'a Json {
    rows.as_array()
        .unwrap()
        .iter()
        .find(|row| row[key] == expected)
        .unwrap()
}
fn decoded(value: &Json) -> Yaml {
    serde_yaml::from_str(value.as_str().unwrap()).unwrap()
}

#[test]
fn full_author_context_is_deterministic_source_pinned_and_never_reads_or_mutates_disk() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = base();
    world.root = directory.path().join("absent");
    let before = image(&world);
    let context = read(&world);
    assert_eq!(context, read(&world));
    assert_eq!(context["protocol"], "wedl-rust/v1");
    assert_eq!(context["operation"], "authoring.context");
    assert_eq!(context["sourceSchema"], "wedl/v1");
    assert_eq!(context["fingerprint"], world.fingerprint);
    assert_eq!(
        context["revision"],
        world.revision.as_ref().unwrap().as_str()
    );
    assert_eq!(context["sourceIndependent"], false);
    assert_eq!(context["writesPerformed"], false);
    assert_eq!(context["validationComplete"], false);
    assert_eq!(context["perspective"], "author");
    assert_eq!(context["worldId"], WORLD);
    assert_eq!(context["defaultTimeline"], "main");
    assert_eq!(
        context["currentTime"],
        json!({"timeline":"other","tick":BIG.to_string(),"order":i32::MIN.to_string()})
    );
    let choices = context["records"].as_array().unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|row| row["target"]["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        world.records.keys().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(
        field(&context["records"], "kind", "event")["status"],
        "draft"
    );
    assert_eq!(
        field(&context["records"], "kind", "location")["status"],
        "retired"
    );
    assert_eq!(
        field(&context["records"], "kind", "hypothesis")["status"],
        "open"
    );
    assert!(!context.to_string().contains("PRIVATE_BODY"));
    assert_eq!(image(&world), before);
    assert!(!world.root.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn absent_and_empty_declarations_remain_distinct_without_creating_defaults() {
    let omitted = edit(&base(), WORLD, |data| {
        data.as_mapping_mut()
            .unwrap()
            .remove(Yaml::String("state_keys".into()));
        data["capabilities"] =
            serde_yaml::to_value(json!(["lore-core-v1", "assertions-v1"])).unwrap();
    });
    let empty = edit(&omitted, WORLD, |data| {
        for key in ["profiles", "predicates"] {
            data[key] = Yaml::Sequence(vec![]);
        }
        for key in [
            "state_profile_bindings",
            "state_keys",
            "relationship_metrics",
        ] {
            data[key] = Yaml::Mapping(Default::default());
        }
    });
    let before = read(&omitted);
    let after = read(&empty);
    for key in [
        "profiles",
        "predicates",
        "state_profile_bindings",
        "state_keys",
        "relationship_metrics",
    ] {
        assert_eq!(
            before["declarations"][key],
            json!({"present":false,"yaml":null}),
            "{key}"
        );
        assert_eq!(after["declarations"][key]["present"], true, "{key}");
        assert_eq!(
            decoded(&after["declarations"][key]["yaml"]),
            empty.records[WORLD].frontmatter[key]
        );
    }
    for key in [
        "profiles",
        "predicates",
        "stateProfileBindings",
        "stateKeys",
        "relationshipMetrics",
    ] {
        assert_eq!(before[key], json!([]));
        assert_eq!(after[key], json!([]));
    }
    assert_ne!(before["fingerprint"], after["fingerprint"]);
}

#[test]
fn typed_declarations_keep_exact_numbers_scalar_types_bindings_and_constraints() {
    let world = edit(&base(), WORLD, |data| {
        data["capabilities"] =
            serde_yaml::to_value(json!(["lore-core-v1", "assertions-v1"])).unwrap();
        data["timelines"][0]["origin"] =
            serde_yaml::to_value(json!({"tick":i64::MIN,"label":"Exact beginning"})).unwrap();
        data["profiles"] = serde_yaml::from_str("- id: notes\n  label: Typed notes\n  fields:\n    type: object\n    required: {}\n    optional:\n      choice: {type: enum, values: [1, '1', true, null, 9007199254740993]}\n      count: {type: number}\n").unwrap();
        data["predicates"] = serde_yaml::from_str("- id: remembers\n  subject_kinds: [character]\n  value_type: {type: enum, values: [17, '17', 18446744073709551615]}\n").unwrap();
        data["state_keys"] = serde_yaml::from_str("character:\n  notes: {type: string}\n  position: {type: entity, kind: location, exclusive_group: where}\n  score: {type: integer}\n").unwrap();
        data["state_profile_bindings"] =
            serde_yaml::from_str("character: {notes: notes}\n").unwrap();
        data["relationship_metrics"] = serde_yaml::from_str("trust: {min: -1.0, max: 1.0, default: 0}\ncount: {min: 9007199254740993, max: 9007199254740994}\n").unwrap();
    });
    let context = read(&world);
    assert_eq!(
        decoded(&context["timelines"][0]["definitionYaml"]),
        world.records[WORLD].frontmatter["timelines"][0]
    );
    assert_eq!(context["timelines"][0]["sourceField"], "timelines[0]");
    for key in [
        "profiles",
        "predicates",
        "state_profile_bindings",
        "state_keys",
        "relationship_metrics",
    ] {
        assert_eq!(
            decoded(&context["declarations"][key]["yaml"]),
            world.records[WORLD].frontmatter[key],
            "{key}"
        );
    }
    let profile = field(&context["profiles"], "id", "notes");
    assert_eq!(profile["sourceField"], "profiles[0]");
    assert_eq!(profile["label"], "Typed notes");
    let definition = decoded(&profile["definitionYaml"]);
    let alternatives = &definition["fields"]["optional"]["choice"]["values"];
    assert_eq!(alternatives[0].as_i64(), Some(1));
    assert_eq!(alternatives[1].as_str(), Some("1"));
    assert_eq!(alternatives[4].as_i64(), Some(BIG));
    let predicate = decoded(&field(&context["predicates"], "id", "remembers")["definitionYaml"]);
    assert_eq!(
        predicate["value_type"]["values"][2].as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        context["stateProfileBindings"],
        json!([{"entityKind":"character","key":"notes","profileId":"notes"}])
    );
    let position = field(&context["stateKeys"], "key", "position");
    assert_eq!(position["valueType"], "entity");
    assert_eq!(
        decoded(&position["definitionYaml"]),
        world.records[WORLD].frontmatter["state_keys"]["character"]["position"]
    );
    let metric =
        decoded(&field(&context["relationshipMetrics"], "name", "count")["definitionYaml"]);
    assert_eq!(metric["min"].as_i64(), Some(BIG));
    assert_eq!(metric["max"].as_i64(), Some(BIG + 1));
}

fn section() -> Json {
    json!({"id":SECTION,"heading":"Exact heading","markdown":"Authored words","valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})
}
fn members_world() -> World {
    let mut world = edit(&base(), WORLD, |data| {
        data["capabilities"] = serde_yaml::to_value(json!([
            "generational-core-v1",
            "descriptions-v1",
            "briefs-v1"
        ]))
        .unwrap();
        let chronology: Yaml = serde_yaml::from_str(include_str!(
            "../examples/authoring/19-world-chronology.yaml"
        ))
        .unwrap();
        data["chronology"] = chronology["chronology"].clone();
    });
    world = edit(&world, PLACE, |data| {
        data["descriptions"] = serde_yaml::to_value(vec![section()]).unwrap();
    });
    world = add(&world, record("character", OTHER, json!({})));
    world = add(
        &world,
        record(
            "knowledge",
            KNOWLEDGE,
            json!({"knower":ACTOR,"claim":{"key":"known","statement":"Reported"},
        "descriptions":[section()], "transitions":[{"id":SECTION,"time":{"tick":1},"state":"accepted","confidence":0.8,"acquisition":"heard"}]}),
        ),
    );
    world = add(
        &world,
        record(
            "relationship",
            RELATIONSHIP,
            json!({"from":ACTOR,"to":OTHER,"transitions":[{"id":"met","time":{"tick":1},"facets":["friend"]}]}),
        ),
    );
    world = add(
        &world,
        record(
            "story-point",
            POINT,
            json!({"activation_policy":"manual","lifecycle":{"initial_state":"dormant",
        "transitions":[{"id":"chosen","time":{"tick":1},"state":"active"}]}}),
        ),
    );
    world = add(
        &world,
        record(
            "organization",
            ORGANIZATION,
            json!({"organization_kind":"institution",
        "initialization":{"transition_id":INITIAL,"transition_kind":"organization-initialize","applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":0,"order":0}},"payload":{"title":"First title","aliases":[]}},
        "transitions":[{"transition_id":LATER,"transition_kind":"organization-rename","applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":1,"order":0}},"payload":{"title":"Later title","aliases":[]}}]}),
        ),
    );
    world
}

#[test]
fn member_choices_use_native_owner_namespaces_and_actual_source_fields() {
    let world = members_world();
    let context = read(&world);
    let members = context["members"].as_array().unwrap();
    let targets = members
        .iter()
        .map(|row| row["target"].clone())
        .collect::<Vec<_>>();
    for expected in [
        json!({"kind":"section","owner_id":PLACE,"id":SECTION}),
        json!({"kind":"section","owner_id":KNOWLEDGE,"id":SECTION}),
        json!({"kind":"transition","owner_id":KNOWLEDGE,"id":SECTION}),
        json!({"kind":"transition","owner_id":RELATIONSHIP,"id":"met"}),
        json!({"kind":"transition","owner_id":POINT,"id":"chosen"}),
        json!({"kind":"transition","owner_id":ORGANIZATION,"id":INITIAL}),
        json!({"kind":"transition","owner_id":ORGANIZATION,"id":LATER}),
    ] {
        assert!(targets.contains(&expected), "missing {expected}");
    }
    let fields = members
        .iter()
        .map(|row| row["sourceField"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for expected in [
        "descriptions[0]",
        "chronology.calendars[0]",
        "chronology.calendars[1]",
        "chronology.eras[0]",
        "chronology.anchors[0]",
        "chronology.anchors[1]",
        "lifecycle.transitions[0]",
        "transitions[0]",
        "initialization",
    ] {
        assert!(fields.contains(expected), "missing {expected}");
    }
    // Every advertised pointer is accepted by the existing complete candidate
    // validator, including an equal member ID in distinct native namespaces.
    let draft = json!({"kind":"context-brief","id":"brief_00000000000000000000000001","title":"All advertised members",
        "domain":"story.reference","scope":"world","valid":{"kind":"timeless"},"required_refs":targets,
        "audience":["author"],"perspectives":["author"]});
    let created = record_edit::create(&world, &serde_yaml::to_string(&draft).unwrap()).unwrap();
    valid(&add(&world, created));
}

#[test]
fn family_choices_apply_declared_capabilities_and_exact_owner_eligibility() {
    let minimal = read(&base());
    for family in minimal["families"].as_array().unwrap() {
        assert_eq!(
            family["available"],
            json!(["changeset", "lore"].contains(&family["family"].as_str().unwrap()))
        );
    }
    assert_eq!(
        field(&minimal["families"], "family", "generational")["mode"],
        "create"
    );
    let enabled = edit(&base(), WORLD, |data| {
        data["capabilities"] = serde_yaml::to_value(json!([
            "generational-core-v1",
            "spatial-core-v1",
            "lore-core-v1",
            "descriptions-v1",
            "assertions-v1",
            "existence-v1",
            "collective-parties-v1",
            "spatial-history-v1",
            "briefs-v1"
        ]))
        .unwrap();
    });
    let context = read(&enabled);
    assert!(
        context["families"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["available"] == true)
    );
    assert_eq!(
        field(&context["records"], "kind", "hypothesis")["scaffoldFamilies"],
        json!(["changeset", "lore"])
    );
    assert_eq!(
        field(&context["records"], "kind", "location")["scaffoldFamilies"],
        json!(["changeset", "lore", "descriptions", "spatial-history"])
    );
    assert_eq!(
        field(&context["records"], "kind", "world")["scaffoldFamilies"],
        json!([
            "changeset",
            "lore",
            "assertions",
            "descriptions",
            "governance"
        ])
    );
    assert!(context["records"].as_array().unwrap().iter().all(|row| {
        !row["scaffoldFamilies"]
            .as_array()
            .unwrap()
            .contains(&json!("generational"))
    }));
}

#[test]
fn context_rejects_semantically_invalid_or_divergent_source_images() {
    let world = base();
    let mut changed = world.clone();
    changed.fingerprint = "0".repeat(64);
    assert!(matches!(
        authoring_tools::context(&changed, MAX_ITEMS),
        Err(Error::Validation(_))
    ));
    let mut changed = world.clone();
    changed.records.get_mut(ACTOR).unwrap().title = "Forged parsed title".into();
    assert!(matches!(
        authoring_tools::context(&changed, MAX_ITEMS),
        Err(Error::Validation(_))
    ));
    let mut changed = world.clone();
    changed
        .records
        .get_mut(ACTOR)
        .unwrap()
        .raw_bytes
        .extend_from_slice(b"Changed bytes");
    assert!(matches!(
        authoring_tools::context(&changed, MAX_ITEMS),
        Err(Error::Validation(_))
    ));
    let invalid = edit(&world, WORLD, |data| {
        data["default_timeline"] = Yaml::String("unknown".into());
    });
    assert!(matches!(
        authoring_tools::context(&invalid, MAX_ITEMS),
        Err(Error::Validation(_))
    ));
    assert_eq!(image(&world), image(&base()));
}

#[test]
fn complete_selection_count_refuses_partial_records_members_or_declarations() {
    let world = base();
    // Nine family choices, two timelines and five source records.
    let exact = authoring_tools::context(&world, 16).unwrap();
    assert_eq!(exact["selectionCount"], 16);
    assert_eq!(exact["maxItems"], 16);
    for limit in [0, 1, 15, MAX_ITEMS + 1] {
        assert!(
            matches!(
                authoring_tools::context(&world, limit),
                Err(Error::InvalidInput(_))
            ),
            "{limit}"
        );
    }
    let world = members_world();
    let all = read(&world);
    let count = all["selectionCount"].as_u64().unwrap() as usize;
    assert!(authoring_tools::context(&world, count).is_ok());
    assert!(matches!(
        authoring_tools::context(&world, count - 1),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn complete_byte_bounds_include_utf8_yaml_and_json_escaping() {
    for title in [
        format!("Name {}", "é".repeat(MAX_RESPONSE_BYTES / 2)),
        format!("Name {}", "\0".repeat(MAX_RESPONSE_BYTES / 5)),
    ] {
        let world = edit(&base(), ACTOR, |data| {
            data["title"] = Yaml::String(title);
        });
        valid(&world);
        let error = authoring_tools::context(&world, MAX_ITEMS).unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)), "{error}");
        assert!(error.to_string().contains("1 MiB"), "{error}");
    }
    let huge = edit(&base(), WORLD, |data| {
        data["capabilities"] = serde_yaml::to_value(json!(["lore-core-v1"])).unwrap();
        data["profiles"] = serde_yaml::to_value(json!([{"id":"large","label":"Large declaration","fields":{"type":"object","required":{},"optional":{"choice":{"type":"enum","values":["x".repeat(MAX_RESPONSE_BYTES)]}}}}])).unwrap();
    });
    valid(&huge);
    let error = authoring_tools::context(&huge, MAX_ITEMS).unwrap_err();
    assert!(
        matches!(error, Error::InvalidInput(_)) && error.to_string().contains("1 MiB"),
        "{error}"
    );
    let bounded = edit(&base(), ACTOR, |data| {
        data["title"] = Yaml::String("é".repeat(20_000));
    });
    let result = read(&bounded);
    assert!(serde_json::to_vec(&result).unwrap().len() < MAX_RESPONSE_BYTES);
}
