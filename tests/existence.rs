//! Authored existence preserves independent statements and never supplies vital or learned truth.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, character, existence, generational,
    model::{Record, StoryTime, World},
    source, validation,
};
const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
fn id(n: usize) -> String {
    format!("existence_{n:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn yaml(s: &str) -> Yaml {
    serde_yaml::from_str(s).unwrap()
}
fn interval(start: Option<(i64, i32)>, end: Option<(i64, i32)>, timeline: &str) -> Json {
    let mut value = json!({"kind":"interval","timeline":timeline});
    for (key, point) in [("start", start), ("end", end)] {
        if let Some((tick, order)) = point {
            value[key] = json!({"timeline":timeline,"tick":tick,"order":order});
        }
    }
    value
}
fn historical() -> Json {
    json!([{"revision":"a".repeat(40),"path":"former/story/removed.md","blob_oid":"b".repeat(64),
        "target":{"kind":"transition","owner_id":"char_00000000000000000000000999","id":"lost-member"}}])
}
fn record(kind: &str, identifier: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":identifier,"title":identifier});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\n# Opaque untimed body\r\nLiteral char_missing and chronology text.  \rTail\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{identifier}.md")).unwrap()
}
fn episode(n: usize, kind: &str, valid: Json) -> Record {
    let mut fields = json!({"domain":"test.existence","episode_kind":kind,"character_id":ANA,
        "valid":valid,"start_evidence":historical(),"audience":["author"],"perspectives":["author"]});
    if fields["valid"].get("end").is_some() {
        fields["end_evidence"] = historical();
    }
    if kind == "unknown" {
        fields["start_evidence"] = json!([]);
    }
    record("existence-episode", &id(n), fields)
}
fn make_world(extra: Vec<Record>) -> World {
    let mut records = vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":["generational-core-v1","existence-v1"],
        "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
        ),
        record(
            "character",
            ANA,
            json!({"initial_state":{},"aliases":["A visible alias"]}),
        ),
        record("character", OTHER, json!({"initial_state":{}})),
        record("location", PLACE, json!({})),
    ];
    records.extend(extra);
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn base() -> World {
    make_world(vec![episode(1, "embodied", interval(None, None, "main"))])
}
fn change(world: &World, identifier: &str, apply: impl FnOnce(&mut Yaml)) -> World {
    let original = &world.records[identifier];
    let mut data = original.frontmatter.clone();
    apply(&mut data);
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
                if r.id == identifier {
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
fn edit(world: &World, identifier: &str, field: &str, value: Json) -> World {
    change(world, identifier, |data| {
        data[field] = serde_yaml::to_value(value).unwrap()
    })
}
fn read(world: &World, time: Option<&StoryTime>) -> Json {
    existence::list(world, ANA, time, "author", "author", 1000).unwrap()
}
fn ids(value: &Json) -> Vec<String> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["episodeId"].as_str().unwrap().to_owned())
        .collect()
}
fn valid(world: &World) {
    assert!(
        existence::validate(world).is_empty(),
        "{:?}",
        existence::validate(world)
    );
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn invalid(world: &World) {
    assert!(!existence::validate(world).is_empty());
    assert!(existence::list(world, ANA, None, "author", "author", 1000).is_err());
}
fn without_fingerprint(value: &mut Json) {
    match value {
        Json::Object(map) => {
            map.remove("fingerprint");
            for child in map.values_mut() {
                without_fingerprint(child);
            }
        }
        Json::Array(values) => {
            for child in values {
                without_fingerprint(child);
            }
        }
        _ => {}
    }
}

#[test]
fn capability_is_independent_of_lore_but_requires_generational_core() {
    let world = base();
    valid(&world);
    assert!(world.world_record().unwrap().get("profiles").is_none());
    assert_eq!(read(&world, None)["count"], 1);
    for caps in [
        json!([]),
        json!(["generational-core-v1"]),
        json!(["existence-v1"]),
    ] {
        invalid(&edit(&world, WORLD, "capabilities", caps));
    }
    let empty = make_world(vec![]);
    assert_eq!(read(&empty, None)["count"], 0);
    assert!(
        existence::list(
            &edit(&empty, WORLD, "capabilities", json!([])),
            ANA,
            None,
            "author",
            "author",
            1
        )
        .is_err()
    );
}

#[test]
fn all_five_kinds_and_overlapping_same_kind_statements_stay_separate() {
    let world = make_world(vec![
        episode(6, "embodied", interval(None, None, "main")),
        episode(1, "embodied", interval(None, None, "main")),
        episode(2, "disembodied", interval(None, None, "main")),
        episode(3, "constructed", interval(None, None, "main")),
        episode(4, "other", interval(None, None, "main")),
        episode(5, "unknown", interval(None, None, "main")),
    ]);
    valid(&world);
    let value = read(&world, Some(&at(0, 0)));
    assert_eq!(ids(&value), (1..=6).map(id).collect::<Vec<_>>());
    assert_eq!(value["items"][4]["episodeKind"], "unknown");
    assert_eq!(value["items"][4]["startEvidence"], json!([]));
    assert_eq!(value["count"], 6);
    assert!(
        value.get("winner").is_none() && value.get("age").is_none() && value.get("alive").is_none()
    );
}

#[test]
fn exact_inclusive_intervals_orders_timelines_and_all_time_history_are_preserved() {
    let world = make_world(vec![
        episode(
            1,
            "embodied",
            interval(
                Some((i64::MIN, i32::MIN)),
                Some((i64::MAX, i32::MAX)),
                "main",
            ),
        ),
        episode(
            2,
            "constructed",
            interval(Some((5, 1)), Some((5, 2)), "main"),
        ),
        episode(
            3,
            "disembodied",
            interval(Some((9007199254740993, 0)), None, "other"),
        ),
        episode(4, "unknown", interval(None, None, "main")),
    ]);
    valid(&world);
    assert_eq!(ids(&read(&world, Some(&at(5, 0)))), [id(1), id(4)]);
    assert_eq!(ids(&read(&world, Some(&at(5, 1)))), [id(1), id(2), id(4)]);
    assert_eq!(ids(&read(&world, Some(&at(5, 2)))), [id(1), id(2), id(4)]);
    assert_eq!(ids(&read(&world, Some(&at(5, 3)))), [id(1), id(4)]);
    let other = StoryTime::new("other", 9007199254740993, 0).unwrap();
    assert_eq!(ids(&read(&world, Some(&other))), [id(3)]);
    let history = read(&world, None);
    assert_eq!(history["count"], 4);
    assert_eq!(history["at"], Json::Null);
    assert_eq!(
        history["items"][0]["valid"]["start"]["tick"],
        i64::MIN.to_string()
    );
    assert_eq!(
        history["items"][0]["valid"]["end"]["order"],
        i32::MAX.to_string()
    );
    assert_eq!(
        history["items"][2]["valid"]["start"]["tick"],
        "9007199254740993"
    );
}

#[test]
fn unknown_requires_unbounded_validity_empty_start_and_absent_end_evidence() {
    let world = make_world(vec![episode(1, "unknown", interval(None, None, "main"))]);
    valid(&world);
    for (field, value) in [
        ("valid", interval(Some((0, 0)), None, "main")),
        ("valid", interval(None, Some((0, 0)), "main")),
        ("valid", json!({"kind":"timeless"})),
        ("start_evidence", historical()),
        ("end_evidence", json!([])),
        ("end_evidence", historical()),
        ("end_evidence", Json::Null),
    ] {
        invalid(&edit(&world, &id(1), field, value));
    }
}

#[test]
fn affirmative_starts_need_evidence_even_without_a_start_bound() {
    for kind in ["embodied", "disembodied", "constructed", "other"] {
        for start in [None, Some((0, 0))] {
            let world = make_world(vec![episode(1, kind, interval(start, None, "main"))]);
            valid(&world);
            invalid(&edit(&world, &id(1), "start_evidence", json!([])));
            invalid(&change(&world, &id(1), |v| {
                v.as_mapping_mut().unwrap().remove("start_evidence");
            }));
        }
    }
}

#[test]
fn bounded_ends_need_evidence_and_unbounded_ends_forbid_even_empty_evidence() {
    let bounded = make_world(vec![episode(
        1,
        "embodied",
        interval(None, Some((0, 0)), "main"),
    )]);
    valid(&bounded);
    for value in [json!([]), Json::Null, json!({})] {
        invalid(&edit(&bounded, &id(1), "end_evidence", value));
    }
    invalid(&change(&bounded, &id(1), |v| {
        v.as_mapping_mut().unwrap().remove("end_evidence");
    }));
    let open = base();
    for value in [historical(), json!([]), Json::Null] {
        invalid(&edit(&open, &id(1), "end_evidence", value));
    }
    let opened = change(&bounded, &id(1), |v| {
        v["valid"].as_mapping_mut().unwrap().remove("end");
        v.as_mapping_mut().unwrap().remove("end_evidence");
    });
    valid(&opened);
    assert!(read(&opened, None)["items"][0].get("endEvidence").is_none());
}

#[test]
fn historical_pointers_are_unverified_payload_and_never_current_endpoint_references() {
    let world = make_world(vec![episode(
        1,
        "constructed",
        interval(Some((0, 0)), Some((9, 0)), "main"),
    )]);
    valid(&world);
    let before = world.records[&id(1)].raw_bytes.clone();
    let output = read(&world, None);
    let item = &output["items"][0];
    assert_eq!(item["startEvidence"], historical());
    assert_eq!(item["endEvidence"], historical());
    assert_eq!(item["evidenceStatus"], "unverified-historical");
    assert_eq!(item["citation"]["entityId"], id(1));
    assert_eq!(
        item["citation"]["sourcePath"],
        world.records[&id(1)].source_path
    );
    assert_eq!(
        item["citation"]["blobOid"],
        json!(world.records[&id(1)].blob_oid)
    );
    assert_eq!(item["citation"]["section"], "record");
    assert!(
        !world
            .records
            .contains_key("char_00000000000000000000000999")
    );
    assert_eq!(world.records[&id(1)].raw_bytes, before);
    assert!(item.get("body").is_none());
    assert!(!item.to_string().contains("Opaque untimed"));
}

#[test]
fn evidence_syntax_is_closed_safe_unique_and_reused_for_both_endpoints() {
    let world = make_world(vec![episode(
        1,
        "embodied",
        interval(Some((0, 0)), Some((1, 0)), "main"),
    )]);
    let original = historical()[0].clone();
    let mut variants = vec![
        Json::Null,
        json!({}),
        json!([original.clone(), original.clone()]),
    ];
    for (field, value) in [
        ("revision", json!("A".repeat(40))),
        ("revision", json!("ab")),
        ("blob_oid", json!(42)),
        ("path", json!("../outside.md")),
        ("path", json!("/private.md")),
        ("path", json!("dir\\file.md")),
        ("path", json!("dir//file.md")),
        ("path", json!("file:stream")),
        ("target", json!({"kind":"record","id":"x","extra":true})),
        ("target", json!({"kind":"section","id":"x"})),
        ("target", json!({"kind":"record","id":""})),
    ] {
        let mut value_item = original.clone();
        value_item[field] = value;
        variants.push(json!([value_item]));
    }
    for field in ["start_evidence", "end_evidence"] {
        for value in &variants {
            invalid(&edit(&world, &id(1), field, value.clone()));
        }
    }
    for target in [
        json!({"kind":"record","id":"literal-removed"}),
        json!({"kind":"section","owner_id":"old-owner","id":"old-section"}),
    ] {
        let mut item = original.clone();
        item["target"] = target;
        valid(&edit(&world, &id(1), "start_evidence", json!([item])));
    }
}

#[test]
fn validity_rejects_timeless_null_bounds_bad_timelines_and_inexact_coordinates() {
    let world = base();
    let mut cases = vec![
        json!({"kind":"timeless"}),
        interval(None, None, "absent"),
        interval(Some((2, 0)), Some((1, 0)), "main"),
    ];
    let source = interval(Some((0, 0)), None, "main");
    for (field, value) in [
        ("tick", json!("0")),
        ("tick", json!(true)),
        ("tick", json!(0.0)),
        ("tick", json!(u64::MAX)),
        ("order", json!("0")),
        ("order", json!(2147483648i64)),
        ("order", Json::Null),
        ("timeline", json!("other")),
        ("extra", json!(1)),
    ] {
        let mut changed = source.clone();
        changed["start"][field] = value;
        cases.push(changed);
    }
    let mut null = source;
    null["end"] = Json::Null;
    cases.push(null);
    for value in cases {
        invalid(&edit(&world, &id(1), "valid", value));
    }
}

#[test]
fn required_fields_kinds_titles_domains_labels_and_endpoints_validate_all_source() {
    let world = base();
    for field in [
        "character_id",
        "episode_kind",
        "domain",
        "valid",
        "start_evidence",
        "audience",
        "perspectives",
    ] {
        invalid(&change(&world, &id(1), |v| {
            v.as_mapping_mut().unwrap().remove(field);
        }));
    }
    for (field, value) in [
        ("episode_kind", json!("resurrection")),
        ("episode_kind", Json::Null),
        ("episode_kind", json!([])),
        ("character_id", json!(PLACE)),
        ("character_id", json!("char_00000000000000000000000999")),
        ("character_id", json!({"kind":"record","id":ANA})),
        ("title", json!("x".repeat(257))),
        ("domain", json!("standalone")),
        ("domain", json!("space in.domain")),
        ("audience", json!([])),
        ("audience", json!(["z", "a"])),
        ("audience", json!(["a", "a"])),
        ("audience", json!(["line\nbreak"])),
        ("perspectives", Json::Null),
        ("provenance", json!(false)),
    ] {
        invalid(&edit(&world, &id(1), field, value));
    }
    let mut blank = world.records[&id(1)].frontmatter.clone();
    blank["title"] = yaml("' '");
    let raw = format!("---\n{}---\n", serde_yaml::to_string(&blank).unwrap());
    assert!(source::parse_record(raw.as_bytes(), "story/blank.md").is_err());
    valid(&edit(&world, &id(1), "provenance", json!("")));
}

#[test]
fn only_canonical_episodes_project_but_draft_and_retired_source_still_validate() {
    let world = make_world(vec![
        episode(1, "embodied", interval(None, None, "main")),
        episode(2, "other", interval(None, None, "main")),
        episode(3, "constructed", interval(None, None, "main")),
    ]);
    let world = edit(
        &edit(&world, &id(2), "status", json!("draft")),
        &id(3),
        "status",
        json!("retired"),
    );
    valid(&world);
    assert_eq!(ids(&read(&world, None)), [id(1)]);
    for id in [id(2), id(3)] {
        invalid(&edit(&world, &id, "start_evidence", json!([])));
    }
    for value in [json!("live"), json!(true), Json::Null] {
        invalid(&edit(&world, &id(1), "status", value));
    }
}

#[test]
fn character_scope_is_independent_and_retains_ordinary_status_availability() {
    let world = base();
    for field in ["audience", "perspectives"] {
        assert_eq!(read(&edit(&world, ANA, field, json!([])), None)["count"], 0);
        assert_eq!(
            read(&edit(&world, ANA, field, json!(["private"])), None)["count"],
            0
        );
        assert_eq!(
            read(
                &edit(&world, ANA, field, json!(["private", "author", "author"])),
                None
            )["count"],
            1
        );
        assert!(
            existence::list(
                &edit(&world, ANA, field, json!("author")),
                ANA,
                None,
                "author",
                "author",
                10
            )
            .is_err()
        );
    }
    assert_eq!(
        read(&edit(&world, ANA, "status", json!("retired")), None)["count"],
        1
    );
    let other = edit(&world, &id(1), "character_id", json!(OTHER));
    valid(&other);
    assert_eq!(read(&other, None)["count"], 0);
    assert_eq!(
        existence::list(&other, OTHER, None, "author", "author", 1).unwrap()["count"],
        1
    );
}

#[test]
fn episode_scope_and_time_filter_before_complete_limits() {
    let mut world = make_world(
        (1..=6)
            .map(|n| episode(n, "embodied", interval(None, None, "main")))
            .collect(),
    );
    world = edit(&world, &id(2), "audience", json!(["private"]));
    world = edit(&world, &id(3), "perspectives", json!(["private"]));
    world = edit(&world, &id(4), "status", json!("draft"));
    world = edit(
        &world,
        &id(5),
        "valid",
        interval(Some((100, 0)), None, "main"),
    );
    world = edit(&world, &id(6), "character_id", json!(OTHER));
    valid(&world);
    let value = existence::list(&world, ANA, Some(&at(5, 0)), "author", "author", 1).unwrap();
    assert_eq!(ids(&value), [id(1)]);
    assert!(matches!(
        existence::list(&world, ANA, None, "author", "author", 1),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        existence::list(&world, ANA, None, "private", "author", 1).unwrap()["count"],
        1
    );
    assert_eq!(
        existence::list(&world, ANA, None, "author", "private", 1).unwrap()["count"],
        1
    );
}

#[test]
fn unavailable_exact_targets_are_uniform_empty_and_never_resolve_titles_or_aliases() {
    let world = edit(&base(), ANA, "audience", json!(["private"]));
    let mut packets = Vec::new();
    for target in [
        ANA,
        OTHER,
        PLACE,
        "char_00000000000000000000000999",
        "A visible alias",
        "unknown",
        "",
    ] {
        let mut result = existence::list(&world, target, None, "author", "author", 10).unwrap();
        assert_eq!(result["count"], 0);
        assert!(result.get("title").is_none());
        assert_eq!(result["characterId"], target);
        result.as_object_mut().unwrap().remove("characterId");
        packets.push(result);
    }
    assert!(packets.iter().all(|value| value == &packets[0]));
}

#[test]
fn limit_arguments_and_scope_time_arguments_reject_invalid_requests_without_partial_items() {
    let world = base();
    for limit in [0, 1001, usize::MAX] {
        assert!(matches!(
            existence::list(&world, ANA, None, "author", "author", limit),
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(
        existence::list(&world, ANA, None, "author", "author", 1000).unwrap()["count"],
        1
    );
    for label in ["", " ", "a\nb", "a\rb"] {
        assert!(existence::list(&world, ANA, None, label, "author", 1).is_err());
        assert!(existence::list(&world, ANA, None, "author", label, 1).is_err());
    }
    assert!(existence::list(&world, ANA, None, &"a".repeat(257), "author", 1).is_err());
    assert!(
        existence::list(
            &world,
            ANA,
            Some(&StoryTime::new("absent", 0, 0).unwrap()),
            "author",
            "author",
            1
        )
        .is_err()
    );
}

#[test]
fn body_utf8_budget_is_exact_and_opaque_metadata_does_not_become_episode_semantics() {
    let world = base();
    let original = &world.records[&id(1)];
    let with_body = |body: String| {
        let raw = format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&original.frontmatter).unwrap(),
            body
        );
        let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
        source::world_from_records(
            PathBuf::new(),
            world
                .records
                .values()
                .map(|r| {
                    if r.id == original.id {
                        replacement.clone()
                    } else {
                        r.clone()
                    }
                })
                .collect(),
            None,
        )
        .unwrap()
    };
    valid(&with_body("é".repeat(512 * 1024)));
    invalid(&with_body(format!("{}a", "é".repeat(512 * 1024))));
    let extra = edit(
        &world,
        &id(1),
        "author_metadata",
        json!({"chronology":"literal","replaces":"existence_missing","character_id":"char_missing"}),
    );
    valid(&extra);
    assert_eq!(ids(&read(&extra, None)), [id(1)]);
    assert!(!read(&extra, None).to_string().contains("char_missing"));
}

#[test]
fn episodes_preserve_character_learning_budgets_and_legacy_vital_answers() {
    let transition = |n: u32, kind: &str, tick: i64| {
        json!({"transition_id":format!("transition_{n:026}"),"transition_kind":kind,
        "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":tick,"order":0}},"payload":{}})
    };
    let knowledge = record(
        "knowledge",
        "know_00000000000000000000000001",
        json!({"knower":ANA,
        "claim":{"key":"returned","statement":"I was told they returned","object":{"entity":id(1)}},
        "transitions":[{"id":"heard-return","time":{"timeline":"main","tick":2,"order":0},"state":"accepted","confidence":0.7,"acquisition":"heard"}]}),
    );
    let vital = record(
        "vital-history",
        "vital_00000000000000000000000001",
        json!({"character_id":ANA,"disclosure":"known",
        "initialization":transition(1,"vital-initialize",-10),"transitions":[transition(2,"vital-birth",0),transition(3,"vital-death",4)]}),
    );
    let world = make_world(vec![
        knowledge,
        vital,
        episode(1, "embodied", interval(None, None, "main")),
    ]);
    valid(&world);
    let learned = character::context(&world, ANA, &at(5, 0), 100_000).unwrap();
    let search = character::search(&world, ANA, "returned", &at(5, 0), 100).unwrap();
    let budget = character::context(&world, ANA, &at(5, 0), 1).map_err(|e| e.to_string());
    let mut vital_before = generational::vital(&world, ANA, &at(5, 0), 100).unwrap();
    without_fingerprint(&mut vital_before);
    assert_eq!(vital_before["vital"], "dead");
    let changed = change(&world, &id(1), |v| {
        v["title"] = yaml("Later canonical account");
        v["episode_kind"] = yaml("constructed");
        v["valid"] = serde_yaml::to_value(interval(Some((100, 0)), None, "main")).unwrap();
    });
    valid(&changed);
    assert_eq!(
        character::context(&changed, ANA, &at(5, 0), 100_000).unwrap(),
        learned
    );
    assert_eq!(
        character::search(&changed, ANA, "returned", &at(5, 0), 100).unwrap(),
        search
    );
    assert_eq!(
        character::context(&changed, ANA, &at(5, 0), 1).map_err(|e| e.to_string()),
        budget
    );
    let mut vital_after = generational::vital(&changed, ANA, &at(5, 0), 100).unwrap();
    without_fingerprint(&mut vital_after);
    assert_eq!(vital_before, vital_after);
    assert_eq!(read(&changed, Some(&at(5, 0)))["count"], 0);
    assert_eq!(changed.records[ANA].raw_bytes, world.records[ANA].raw_bytes);
}

#[test]
fn reads_preserve_all_source_bytes_and_current_time() {
    let world = make_world(vec![
        episode(1, "embodied", interval(None, None, "main")),
        episode(2, "unknown", interval(None, None, "other")),
    ]);
    valid(&world);
    let before: BTreeMap<_, _> = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect();
    let time = world.current_time().unwrap();
    for horizon in [None, Some(at(0, 0)), Some(at(i64::MAX, i32::MAX))] {
        read(&world, horizon.as_ref());
    }
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<BTreeMap<_, _>>(),
        before
    );
    assert_eq!(world.current_time().unwrap(), time);
}

#[test]
fn native_reads_reject_fields_that_disagree_with_cited_source_bytes_or_fingerprint() {
    let world = base();
    let raw = world.records[&id(1)].raw_bytes.clone();
    let fingerprint = world.fingerprint.clone();
    let mut forged = world.clone();
    forged.records.get_mut(&id(1)).unwrap().frontmatter["episode_kind"] = yaml("constructed");
    // Both kinds satisfy episode grammar; only the composed source check rejects the forgery.
    assert!(existence::validate(&forged).is_empty());
    assert_eq!(forged.records[&id(1)].raw_bytes, raw);
    assert_eq!(forged.fingerprint, fingerprint);
    assert!(matches!(
        existence::list(&forged, ANA, None, "author", "author", 100),
        Err(Error::Validation(_))
    ));
    let mut changed_bytes = world.clone();
    changed_bytes
        .records
        .get_mut(&id(1))
        .unwrap()
        .raw_bytes
        .extend_from_slice(b"Added outside parsed source");
    assert!(matches!(
        existence::list(&changed_bytes, ANA, None, "author", "author", 100),
        Err(Error::Validation(_))
    ));
    let mut changed_fingerprint = world.clone();
    changed_fingerprint.fingerprint = "f".repeat(64);
    assert!(matches!(
        existence::list(&changed_fingerprint, ANA, None, "author", "author", 100),
        Err(Error::Validation(_))
    ));
    assert_eq!(world.records[&id(1)].raw_bytes, raw);
    assert_eq!(world.fingerprint, fingerprint);
    assert_eq!(read(&world, None)["items"][0]["episodeKind"], "embodied");
}

#[test]
fn list_composes_shared_annotation_and_description_validation_before_projection() {
    let world = base();
    let mut with_capabilities = change(&world, WORLD, |v| {
        v["capabilities"] = yaml("[generational-core-v1, descriptions-v1, existence-v1]");
    });
    for (field, value) in [
        (
            "chronology",
            json!([{"id":"chronology_00000000000000000000000001","value":{"civil":{"calendar_id":"calendar_00000000000000000099","year":0,"month":1,"day":1}}}]),
        ),
        (
            "descriptions",
            json!([{"id":"description_00000000000000000000000001","heading":"Explicit account","markdown":"Invalid independent applicability","valid":{"kind":"interval","timeline":"undeclared"},"audience":["author"],"perspectives":["author"]}]),
        ),
    ] {
        let malformed = edit(&with_capabilities, &id(1), field, value);
        assert!(existence::validate(&malformed).is_empty());
        assert!(!validation::validate_world(&malformed).valid);
        assert!(matches!(
            existence::list(&malformed, ANA, None, "author", "author", 100),
            Err(Error::Validation(_))
        ));
        assert!(matches!(
            existence::list(&malformed, "missing", None, "author", "author", 100),
            Err(Error::Validation(_))
        ));
    }
    with_capabilities = edit(&with_capabilities, &id(1), "descriptions", json!([]));
    valid(&with_capabilities);
    assert_eq!(read(&with_capabilities, None)["count"], 1);
}
