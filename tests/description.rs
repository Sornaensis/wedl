//! Dated authored prose is explicit, scoped, cited and independent of character learning.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, character, description,
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const OWNER: &str = "loc_00000000000000000000000001";
const PERSON: &str = "char_00000000000000000000000001";
const CONCEPT: &str = "concept_00000000000000000000000001";
fn id(n: usize) -> String {
    format!("description_{n:026}")
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn member(n: usize) -> Json {
    json!({"id":id(n),"heading":format!("Account {n}"),"markdown":format!("Exact account {n}\r\nSecond line  \rTail\n"),
        "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})
}
fn interval(start: Option<(i64, i32)>, end: Option<(i64, i32)>, timeline: &str) -> Json {
    let mut result = json!({"kind":"interval","timeline":timeline});
    if let Some((tick, order)) = start {
        result["start"] = json!({"timeline":timeline,"tick":tick,"order":order});
    }
    if let Some((tick, order)) = end {
        result["end"] = json!({"timeline":timeline,"tick":tick,"order":order});
    }
    result
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\n\r\n# Opaque body 2030\r\nLiteral tmp:unknown  \r\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn make_world(mut records: Vec<Record>) -> World {
    records.insert(0,record("world",WORLD,json!({
        "capabilities":["descriptions-v1"],"default_timeline":"main",
        "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}
    })));
    if !records.iter().any(|r| r.id == PERSON) {
        records.push(record("character", PERSON, json!({"initial_state":{}})));
    }
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn base(members: Vec<Json>) -> World {
    make_world(vec![record(
        "location",
        OWNER,
        json!({"descriptions":members}),
    )])
}
fn replace(world: &World, id: &str, field: &str, value: Yaml) -> World {
    let mut records = world.records.clone();
    let owner = &records[id];
    let mut data = owner.frontmatter.clone();
    data[field] = value;
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        owner.body
    );
    records.insert(
        id.into(),
        source::parse_record(raw.as_bytes(), &owner.source_path).unwrap(),
    );
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn patch(world: &World, id: &str, field: &str, value: Json) -> World {
    replace(world, id, field, serde_yaml::to_value(value).unwrap())
}
fn read(world: &World, time: Option<&StoryTime>) -> Json {
    description::list(world, OWNER, time, "author", "author", 1000).unwrap()
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["sectionId"].as_str().unwrap())
        .collect()
}

#[test]
fn descriptions_install_independently_preserve_opaque_body_and_literal_strings() {
    let mut literal = member(1);
    literal["heading"] = json!("char_00000000000000000000000999");
    literal["markdown"] =
        json!("entity: char_00000000000000000000000999\nchronology: tmp:unresolved\n");
    let world = base(vec![literal.clone()]);
    let before: BTreeMap<_, _> = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    assert!(
        !world
            .world_record()
            .unwrap()
            .list("capabilities")
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("lore-core-v1"))
    );
    let result = read(&world, None);
    assert_eq!(result["count"], 1);
    assert_eq!(result["items"][0]["markdown"], literal["markdown"]);
    assert!(!result.to_string().contains("Opaque body"));
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<BTreeMap<_, _>>(),
        before
    );
}

#[test]
fn omission_and_explicit_empty_are_valid_but_null_requires_a_real_array() {
    let omitted = make_world(vec![record("location", OWNER, json!({}))]);
    let empty = patch(&omitted, OWNER, "descriptions", json!([]));
    assert!(description::validate(&omitted).is_empty());
    assert!(description::validate(&empty).is_empty());
    assert_eq!(read(&omitted, None)["items"], json!([]));
    assert_eq!(read(&empty, None)["items"], json!([]));
    assert!(omitted.records[OWNER].get("descriptions").is_none());
    assert_eq!(empty.records[OWNER].get("descriptions"), Some(&yaml("[]")));
    assert!(description::validate_request(&Yaml::Null).is_err());
    assert!(!description::validate(&patch(&empty, OWNER, "descriptions", Json::Null)).is_empty());
    let no_cap = patch(&empty, WORLD, "capabilities", json!([]));
    assert!(!description::validate(&no_cap).is_empty());
    assert!(description::list(&no_cap, OWNER, None, "author", "author", 10).is_err());
}

#[test]
fn closed_member_grammar_rejects_bad_ids_fields_values_and_scope_shapes() {
    for (field, value) in [
        ("id", json!("description_bad")),
        ("id", Json::Null),
        ("heading", json!("  ")),
        ("heading", json!("x".repeat(257))),
        ("markdown", Json::Null),
        ("audience", json!([])),
        ("audience", json!(["z", "a"])),
        ("audience", json!(["author", "author"])),
        ("audience", json!([{"id":"author"}])),
        ("perspectives", json!(["a\nb"])),
        ("extra", json!(true)),
        ("replaces", Json::Null),
    ] {
        let mut m = member(1);
        m[field] = value;
        assert!(
            description::validate_request(&serde_yaml::to_value(vec![m]).unwrap()).is_err(),
            "{field}"
        );
    }
    let mut missing = member(1);
    missing.as_object_mut().unwrap().remove("valid");
    for value in [
        json!([missing]),
        json!([null]),
        json!({}),
        json!([member(1), member(1)]),
    ] {
        assert!(description::validate_request(&serde_yaml::to_value(value).unwrap()).is_err());
    }
    let tagged = yaml(
        "- id: description_00000000000000000000000001\n  heading: !label Account\n  markdown: text\n  valid: {kind: timeless}\n  audience: [author]\n  perspectives: [author]\n",
    );
    assert!(description::validate_request(&tagged).is_err());
}

#[test]
fn finite_text_and_aggregate_source_bounds_are_enforced() {
    let mut m = member(1);
    m["heading"] = json!("é".repeat(256));
    m["markdown"] = json!("x".repeat(1024 * 1024));
    assert!(description::validate_request(&serde_yaml::to_value(vec![m.clone()]).unwrap()).is_ok());
    m["markdown"] = json!("é".repeat(1024 * 1024 / 2 + 1));
    assert!(description::validate_request(&serde_yaml::to_value(vec![m]).unwrap()).is_err());
    assert!(
        description::validate_request(
            &serde_yaml::to_value((0..963).map(member).collect::<Vec<_>>()).unwrap()
        )
        .is_ok()
    );
    assert!(
        description::validate_request(
            &serde_yaml::to_value((0..964).map(member).collect::<Vec<_>>()).unwrap()
        )
        .is_err()
    );
    assert!(
        description::validate_request(
            &serde_yaml::to_value((0..1025).map(member).collect::<Vec<_>>()).unwrap()
        )
        .is_err()
    );
}

#[test]
fn validity_requires_exact_closed_times_and_declared_timelines() {
    for valid in [
        json!({"kind":"legacy-unqualified"}),
        json!({"kind":"timeless","start":{}}),
        interval(Some((2, 0)), Some((1, 0)), "main"),
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":true,"order":0}}),
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":1,"order":"0"}}),
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":1.0,"order":0}}),
        json!({"kind":"interval","timeline":"main","start":{"timeline":"other","tick":1,"order":0}}),
        json!({"kind":"interval","timeline":"main","end":null}),
    ] {
        let mut m = member(1);
        m["valid"] = valid;
        assert!(!description::validate(&base(vec![m])).is_empty());
    }
    let mut m = member(1);
    m["valid"] = interval(None, None, "missing");
    assert!(description::validate_request(&serde_yaml::to_value(vec![m.clone()]).unwrap()).is_ok());
    assert!(!description::validate(&base(vec![m])).is_empty());
}

#[test]
fn three_intervals_overlap_timeless_and_open_ends_keep_exact_inclusive_orders() {
    let mut old = member(1);
    old["valid"] = interval(None, Some((0, -1)), "main");
    let mut current = member(2);
    current["valid"] = interval(Some((0, 0)), Some((9, 0)), "main");
    let mut late = member(3);
    late["valid"] = interval(Some((10, 0)), None, "main");
    let mut overlap = member(5);
    overlap["valid"] = interval(Some((2, 0)), Some((5, 0)), "main");
    let world = base(vec![old, current, late, member(4), overlap]);
    for (tick, order, expected) in [
        (0, -1, vec![1, 4]),
        (0, 0, vec![2, 4]),
        (5, 0, vec![2, 4, 5]),
        (5, 1, vec![2, 4]),
        (9, 0, vec![2, 4]),
        (9, 1, vec![4]),
        (10, 0, vec![3, 4]),
    ] {
        let result = read(&world, Some(&at(tick, order)));
        assert_eq!(
            ids(&result),
            expected.iter().map(|n| id(*n)).collect::<Vec<_>>()
        );
    }
    let result = read(&world, None);
    assert_eq!(ids(&result), (1..=5).map(id).collect::<Vec<_>>());
}

#[test]
fn exact_extreme_coordinates_and_other_timelines_are_preserved() {
    let mut m = member(1);
    m["valid"] = interval(
        Some((i64::MAX, i32::MIN)),
        Some((i64::MAX, i32::MAX)),
        "main",
    );
    let mut other = member(2);
    other["valid"] = interval(None, None, "other");
    let world = base(vec![m, other]);
    let result = read(&world, Some(&at(i64::MAX, i32::MIN)));
    assert_eq!(ids(&result), vec![id(1)]);
    assert_eq!(result["at"]["tick"], i64::MAX.to_string());
    assert_eq!(
        result["items"][0]["valid"]["start"]["order"],
        i32::MIN.to_string()
    );
    let result = read(&world, Some(&StoryTime::new("other", 0, 0).unwrap()));
    assert_eq!(ids(&result), vec![id(2)]);
    assert_eq!(read(&world, None)["count"], 2);
    assert!(
        description::list(
            &world,
            OWNER,
            Some(&StoryTime::new("missing", 0, 0).unwrap()),
            "author",
            "author",
            10
        )
        .is_err()
    );
}

#[test]
fn corrections_are_earlier_local_exact_and_preserve_forks_and_alternatives() {
    let first = member(1);
    let mut correction = member(2);
    correction["replaces"] = json!(id(1));
    let mut fork = member(3);
    fork["replaces"] = json!(id(1));
    let mut last = member(5);
    last["replaces"] = json!(id(2));
    let world = base(vec![
        first.clone(),
        correction.clone(),
        fork,
        member(4),
        last,
    ]);
    assert!(description::validate(&world).is_empty());
    assert_eq!(
        ids(&read(&world, Some(&at(0, 0)))),
        vec![id(3), id(4), id(5)]
    );
    assert_eq!(
        ids(&read(&world, None)),
        (1..=5).map(id).collect::<Vec<_>>()
    );
    for bad in [
        vec![correction.clone(), first.clone()],
        vec![correction.clone()],
        vec![member(1), member(1)],
    ] {
        assert!(!description::validate(&base(bad)).is_empty());
    }
    for (field, value) in [
        ("valid", interval(None, None, "main")),
        ("audience", json!(["private"])),
        ("perspectives", json!(["ordinary"])),
    ] {
        let mut bad = correction.clone();
        bad[field] = value;
        assert!(
            !description::validate(&base(vec![first.clone(), bad])).is_empty(),
            "{field}"
        );
    }
    let second = record(
        "object",
        "obj_00000000000000000000000001",
        json!({"descriptions":[correction]}),
    );
    assert!(
        !description::validate(&make_world(vec![
            record("location", OWNER, json!({"descriptions":[first]})),
            second
        ]))
        .is_empty()
    );
}

#[test]
fn local_ids_can_repeat_on_different_owners_without_shared_history() {
    let world = make_world(vec![
        record("location", OWNER, json!({"descriptions":[member(1)]})),
        record(
            "object",
            "obj_00000000000000000000000001",
            json!({"descriptions":[member(1)]}),
        ),
    ]);
    assert!(description::validate(&world).is_empty());
    let a = description::section(&world, OWNER, &id(1), None, "author", "author").unwrap();
    let b = description::section(
        &world,
        "obj_00000000000000000000000001",
        &id(1),
        None,
        "author",
        "author",
    )
    .unwrap();
    assert_ne!(a["target"]["ownerId"], b["target"]["ownerId"]);
    assert_eq!(a["sectionId"], b["sectionId"]);
}

#[test]
fn filtering_precedes_corrections_counts_limits_and_original_source_indexes() {
    let mut hidden = member(1);
    hidden["audience"] = json!(["private"]);
    let mut correction = member(2);
    correction["audience"] = json!(["private"]);
    correction["replaces"] = json!(id(1));
    let world = base(vec![hidden, correction, member(3)]);
    let value = description::list(&world, OWNER, Some(&at(0, 0)), "author", "author", 1).unwrap();
    assert_eq!(ids(&value), vec![id(3)]);
    assert_eq!(value["items"][0]["citation"]["section"], "descriptions[2]");
    assert!(!value.to_string().contains(&id(1)) && !value.to_string().contains(&id(2)));
    let private =
        description::list(&world, OWNER, Some(&at(0, 0)), "private", "author", 1).unwrap();
    assert_eq!(ids(&private), vec![id(2)]);
    assert!(description::list(&world, OWNER, None, "private", "author", 1).is_err());
    let mut hidden_change = member(1);
    hidden_change["audience"] = json!(["private"]);
    hidden_change["markdown"] = json!("A completely different secret.");
    let changed = patch(
        &world,
        OWNER,
        "descriptions",
        json!([
            hidden_change,
            world.records[OWNER].get("descriptions").unwrap()[1],
            member(3)
        ]),
    );
    let a = read(&world, Some(&at(0, 0)));
    let b = read(&changed, Some(&at(0, 0)));
    assert_eq!(a["count"], b["count"]);
    assert_eq!(a["items"][0]["markdown"], b["items"][0]["markdown"]);
    assert_eq!(a["items"][0]["sectionId"], b["items"][0]["sectionId"]);
}

#[test]
fn ordinary_owner_scope_is_independent_and_empty_scope_hides_the_owner() {
    let world = base(vec![member(1)]);
    for (field, scope) in [
        ("audience", json!(["private"])),
        ("perspectives", json!(["ordinary"])),
        ("audience", json!([])),
        ("perspectives", json!([])),
    ] {
        let hidden = patch(&world, OWNER, field, scope);
        assert!(matches!(
            description::list(&hidden, OWNER, None, "author", "author", 1),
            Err(Error::NotFound(_))
        ));
    }
    for bad in [json!("author"), Json::Null, json!([12])] {
        assert!(
            description::list(
                &patch(&world, OWNER, "audience", bad),
                OWNER,
                None,
                "author",
                "author",
                1
            )
            .is_err()
        );
    }
    let ordinary = patch(
        &patch(&world, OWNER, "audience", json!(["z", "author", "author"])),
        OWNER,
        "status",
        json!("draft"),
    );
    assert_eq!(read(&ordinary, Some(&at(i64::MIN, i32::MIN)))["count"], 1);
}

#[test]
fn explicit_section_validity_does_not_inherit_event_or_body_timing() {
    let event = "event_00000000000000000000000001";
    let world = make_world(vec![record(
        "event",
        event,
        json!({"status":"draft","time":{"timeline":"main","tick":100,"order":0},"effects":[],"descriptions":[member(1)]}),
    )]);
    assert_eq!(
        description::list(&world, event, Some(&at(0, 0)), "author", "author", 1).unwrap()["count"],
        1
    );
    assert!(
        !description::list(&world, event, None, "author", "author", 1)
            .unwrap()
            .to_string()
            .contains("Opaque body")
    );
}

#[test]
fn lore_owners_keep_canonical_status_own_validity_and_scope_before_sections() {
    let mut world = make_world(vec![record(
        "lore-concept",
        CONCEPT,
        json!({"profile":"description-owner","domain":"authored.owner","values":{},"valid":interval(Some((10,0)),Some((20,0)),"main"),"audience":["author"],"perspectives":["author"],"descriptions":[member(1)]}),
    )]);
    world = patch(
        &world,
        WORLD,
        "capabilities",
        json!(["lore-core-v1", "descriptions-v1"]),
    );
    world = patch(
        &world,
        WORLD,
        "profiles",
        json!([{"id":"description-owner","label":"Description owner","fields":{"type":"object","required":{},"optional":{}}}]),
    );
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    assert_eq!(
        description::list(&world, CONCEPT, Some(&at(20, 0)), "author", "author", 1).unwrap()["count"],
        1
    );
    for time in [at(9, 0), at(20, 1), StoryTime::new("other", 15, 0).unwrap()] {
        assert!(matches!(
            description::list(&world, CONCEPT, Some(&time), "author", "author", 1),
            Err(Error::NotFound(_))
        ));
    }
    for status in ["draft", "retired"] {
        assert!(matches!(
            description::list(
                &patch(&world, CONCEPT, "status", json!(status)),
                CONCEPT,
                None,
                "author",
                "author",
                1
            ),
            Err(Error::NotFound(_))
        ));
    }
}

#[test]
fn exact_section_reads_use_same_scope_and_history_and_cite_stable_owner_members() {
    let mut correction = member(2);
    correction["replaces"] = json!(id(1));
    let mut world = base(vec![member(1), correction]);
    world.revision = Some("f".repeat(40));
    world.records.get_mut(OWNER).unwrap().blob_oid = Some("a".repeat(40));
    let history = description::section(&world, OWNER, &id(1), None, "author", "author").unwrap();
    assert_eq!(
        history["citation"]["sourcePath"],
        world.records[OWNER].source_path
    );
    assert_eq!(history["citation"]["blobOid"], "a".repeat(40));
    assert_eq!(history["citation"]["revision"], "f".repeat(40));
    assert_eq!(history["citation"]["fingerprint"], world.fingerprint);
    assert_eq!(history["citation"]["section"], "descriptions[0]");
    assert_eq!(
        history["target"],
        json!({"kind":"section","ownerId":OWNER,"sectionId":id(1)})
    );
    assert_eq!(history["markdown"], member(1)["markdown"]);
    assert!(matches!(
        description::section(&world, OWNER, &id(1), Some(&at(0, 0)), "author", "author"),
        Err(Error::NotFound(_))
    ));
    let current =
        description::section(&world, OWNER, &id(2), Some(&at(0, 0)), "author", "author").unwrap();
    assert_eq!(current["replaces"], id(1));
}

#[test]
fn missing_hidden_hypothesis_and_nonexact_targets_have_uniform_unavailable_errors() {
    let world = base(vec![member(1)]);
    let absent = description::section(&world, OWNER, &id(99), None, "author", "author")
        .unwrap_err()
        .to_string();
    for owner in ["missing", "Account 1", "loc_00000000000000000000000999"] {
        assert_eq!(
            description::list(&world, owner, None, "author", "author", 10)
                .unwrap_err()
                .to_string(),
            absent
        );
    }
    assert_eq!(
        description::section(&world, OWNER, "Account 1", None, "author", "author")
            .unwrap_err()
            .to_string(),
        absent
    );
    let hidden = patch(&world, OWNER, "audience", json!([]));
    assert_eq!(
        description::list(&hidden, OWNER, None, "author", "author", 10)
            .unwrap_err()
            .to_string(),
        absent
    );
    let hypothesis = "hyp_00000000000000000000000001";
    let hypotheses = make_world(vec![record("hypothesis", hypothesis, json!({}))]);
    assert_eq!(
        description::list(&hypotheses, hypothesis, None, "author", "author", 10)
            .unwrap_err()
            .to_string(),
        absent
    );
    assert!(
        !description::validate(&patch(&hypotheses, hypothesis, "descriptions", json!([])))
            .is_empty()
    );
}

#[test]
fn complete_limits_are_after_scope_and_replacement_and_query_labels_are_closed() {
    let mut correction = member(2);
    correction["replaces"] = json!(id(1));
    let world = base(vec![member(1), correction]);
    assert_eq!(
        description::list(&world, OWNER, Some(&at(0, 0)), "author", "author", 1).unwrap()["count"],
        1
    );
    assert!(description::list(&world, OWNER, None, "author", "author", 1).is_err());
    for limit in [0, 1001] {
        assert!(description::list(&world, OWNER, None, "author", "author", limit).is_err());
    }
    for label in ["", " ", "bad\nlabel"] {
        assert!(description::list(&world, OWNER, None, label, "author", 10).is_err());
        assert!(description::list(&world, OWNER, None, "author", label, 10).is_err());
    }
}

#[test]
fn invalid_sections_validate_even_when_owner_status_or_scope_is_invisible() {
    let mut invalid = member(1);
    invalid["valid"] = interval(Some((2, 0)), Some((1, 0)), "main");
    let mut world = base(vec![invalid]);
    world = patch(&world, OWNER, "audience", json!(["private"]));
    world = patch(&world, OWNER, "status", json!("draft"));
    assert!(!description::validate(&world).is_empty());
    assert!(!validation::validate_world(&world).valid);
}

#[test]
fn authored_sections_and_revisions_do_not_enter_generic_character_responses() {
    let base = make_world(vec![record("location", OWNER, json!({}))]);
    let context = character::context(&base, PERSON, &at(5, 0), 100_000).unwrap();
    let search = character::search(&base, PERSON, "needle", &at(5, 0), 100).unwrap();
    let budget = character::context(&base, PERSON, &at(5, 0), 1).map_err(|e| e.to_string());
    let mut m = member(1);
    m["markdown"] =
        json!("Needle is a magical hidden power, according to this authored description.");
    let world = patch(
        &patch(&base, OWNER, "descriptions", json!([m.clone()])),
        PERSON,
        "descriptions",
        json!([m.clone()]),
    );
    let mut correction = member(2);
    correction["replaces"] = json!(id(1));
    correction["markdown"] = json!("A changed canonical needle account.");
    let changed = patch(&world, OWNER, "descriptions", json!([m, correction]));
    for candidate in [&world, &changed] {
        assert_eq!(
            character::context(candidate, PERSON, &at(5, 0), 100_000).unwrap(),
            context
        );
        assert_eq!(
            character::search(candidate, PERSON, "needle", &at(5, 0), 100).unwrap(),
            search
        );
        assert_eq!(
            character::context(candidate, PERSON, &at(5, 0), 1).map_err(|e| e.to_string()),
            budget
        );
    }
}
