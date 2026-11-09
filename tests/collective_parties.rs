//! Public Party tags are source/display conversions, never wider legacy endpoints.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    generational, governance,
    model::{Record, StoryTime, World},
    source, validation,
};

fn id(kind: &str, number: u32) -> String {
    let prefix = match kind {
        "character" => "char",
        "location" => "loc",
        "lore-concept" => "concept",
        "parentage" => "kinship",
        _ => kind,
    };
    format!("{prefix}_{number:026}")
}
fn reference(kind: &str, number: u32) -> Json {
    json!({"kind":"record","id":id(kind,number)})
}
fn transition(number: u32, kind: &str, payload: Json) -> Json {
    json!({"transition_id":id("transition",number),"transition_kind":kind,
        "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":0,"order":0}},"payload":payload})
}
fn record(kind: &str, number: u32, fields: Json) -> Record {
    let identifier = id(kind, number);
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":identifier,"title":identifier});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nExact collective prose.\r\nNo authority grant.  \n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{identifier}.md")).unwrap()
}
fn world() -> World {
    let mut records = vec![
        record(
            "world",
            1,
            json!({"capabilities":["generational-core-v1","lore-core-v1","collective-parties-v1"],
            "timelines":[{"id":"main","label":"Main"}],"default_timeline":"main","state_keys":{},
            "profiles":[{"id":"an-authored-collective","label":"No built-in group semantics",
                "fields":{"type":"object","required":{"holder":{"type":"entity","kinds":["character","organization","lore-concept"]}},"optional":{}}}]}),
        ),
        record("character", 1, json!({})),
        record("character", 2, json!({})),
        record("location", 1, json!({})),
        record(
            "lore-concept",
            1,
            json!({"domain":"authored.collective","profile":"an-authored-collective",
            "values":{"holder":reference("organization",1)},"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
        ),
        record(
            "legacy",
            1,
            json!({"legacy_kind":"office","organization_id":id("organization",1),
            "initialization":transition(10,"legacy-initialize",json!({"title":"Authored office","aliases":[]})),"transitions":[]}),
        ),
        record(
            "claim",
            1,
            json!({"legacy_id":id("legacy",1),"claimant_id":id("character",1),
            "initialization":transition(20,"claim-initialize",json!({"competes_with":[]})),"transitions":[]}),
        ),
        record(
            "parentage",
            1,
            json!({"child_id":id("character",1),"parent_id":id("character",2),
            "initialization":transition(30,"parentage-initialize",json!({"basis":"biological"})),"transitions":[]}),
        ),
    ];
    for number in 1..=2 {
        records.push(record("organization",number,json!({"organization_kind":"institution",
            "parent_id":if number==2 {Some(id("organization",1))} else {None},
            "initialization":transition(number,"organization-initialize",json!({"title":format!("Organization {number}"),"aliases":[]})),"transitions":[]})));
        records.push(record("tenure",number,json!({"legacy_id":id("legacy",1),
            "initialization":transition(number+10,"tenure-initialize",json!({"holder_id":id("character",number),"basis":if number==1 {"legal"} else {"de-facto"}})),"transitions":[]})));
    }
    source::world_from_records(PathBuf::new(), records, Some("collective-fixture".into())).unwrap()
}
fn edit(world: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let original = &world.records[target];
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
            .map(|record| {
                if record.id == target {
                    replacement.clone()
                } else {
                    record.clone()
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

#[test]
fn exact_party_tags_round_trip_actual_record_refs_without_changing_source() {
    let world = world();
    valid(&world);
    let before = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    for (tag, kind) in [
        ("character", "character"),
        ("organization", "organization"),
        ("concept", "lore-concept"),
    ] {
        let party = json!({"kind":tag,"id":id(kind,1)});
        governance::validate_party(&world, &party).unwrap();
        assert_eq!(
            governance::party_from_reference(&world, &reference(kind, 1)).unwrap(),
            party
        );
        assert_eq!(
            governance::party_reference(&world, &party).unwrap(),
            reference(kind, 1)
        );
    }
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<BTreeMap<_, _>>(),
        before
    );
    let hidden = edit(&world, &id("organization", 1), |data| {
        data["audience"] = serde_yaml::to_value(json!(["private"])).unwrap()
    });
    // Pure conversion itself does not prove endpoint presentation admission.
    assert_eq!(
        governance::party_from_reference(&hidden, &reference("organization", 1)).unwrap(),
        json!({"kind":"organization","id":id("organization",1)})
    );
}

#[test]
fn party_and_ref_shapes_are_closed_and_resolve_actual_endpoint_kinds() {
    let world = world();
    for party in [
        Json::Null,
        json!([]),
        json!({}),
        json!({"kind":[],"id":id("character",1)}),
        json!({"kind":"lore-concept","id":id("lore-concept",1)}),
        json!({"kind":"character","id":id("organization",1)}),
        json!({"kind":"concept","id":id("location",1)}),
        json!({"kind":"organization","id":id("organization",99)}),
        json!({"kind":"character","id":null}),
        json!({"kind":"concept","id":{"id":id("lore-concept",1)}}),
        json!({"kind":"character","id":id("character",1),"grant":true}),
    ] {
        assert!(
            governance::validate_party(&world, &party).is_err(),
            "{party}"
        );
        assert!(
            governance::party_reference(&world, &party).is_err(),
            "{party}"
        );
    }
    for reference in [
        reference("location", 1),
        reference("organization", 99),
        json!({"kind":"organization","id":id("organization",1)}),
        json!({"kind":"record","id":id("organization",1),"audience":"public"}),
        json!({"kind":"record","id":null}),
        json!({"kind":"record"}),
    ] {
        assert!(
            governance::party_from_reference(&world, &reference).is_err(),
            "{reference}"
        );
    }
    let party_in_source = edit(&world, &id("lore-concept", 1), |data| {
        data["values"]["holder"] =
            serde_yaml::to_value(json!({"kind":"organization","id":id("organization",1)})).unwrap();
    });
    assert!(!validation::validate_world(&party_in_source).valid);
}

#[test]
fn converters_check_source_identity_and_capabilities_without_reusable_grants() {
    let world = world();
    let party = json!({"kind":"organization","id":id("organization",1)});
    let mut forged = world.clone();
    forged
        .records
        .get_mut(&id("organization", 1))
        .unwrap()
        .title = "Model-only title".into();
    assert!(governance::party_from_reference(&forged, &reference("character", 1)).is_err());
    assert!(governance::party_reference(&forged, &party).is_err());
    governance::validate_party(&forged, &party).unwrap();
    for omitted in [
        "collective-parties-v1",
        "lore-core-v1",
        "generational-core-v1",
    ] {
        let unavailable = edit(&world, &id("world", 1), |data| {
            data["capabilities"]
                .as_sequence_mut()
                .unwrap()
                .retain(|value| value.as_str() != Some(omitted))
        });
        assert!(governance::validate_party(&unavailable, &party).is_err());
        assert!(
            governance::party_from_reference(&unavailable, &reference("organization", 1)).is_err()
        );
        assert!(governance::party_reference(&unavailable, &party).is_err());
    }
}

#[test]
fn collective_profiles_preserve_legacy_tenure_claim_parentage_and_organization_rules() {
    let world = world();
    valid(&world);
    let at = StoryTime::new("main", 5, 0).unwrap();
    let legacy = generational::legacy(&world, &id("legacy", 1), &at, 100).unwrap();
    assert_eq!(legacy["holderCount"], 2);
    assert_eq!(legacy["holders"][0]["value"]["basis"], "legal");
    assert_eq!(legacy["holders"][1]["value"]["basis"], "de-facto");
    assert_eq!(
        legacy["holders"][0]["value"]["holderId"],
        id("character", 1)
    );
    assert_eq!(
        legacy["holders"][1]["value"]["holderId"],
        id("character", 2)
    );
    let cases = [
        ("tenure", 1, "initialization.payload.holder_id"),
        ("claim", 1, "claimant_id"),
        ("parentage", 1, "parent_id"),
        ("organization", 2, "parent_id"),
    ];
    for (kind, number, field) in cases {
        let bad = edit(&world, &id(kind, number), |data| {
            let replacement = Yaml::String(if kind == "organization" {
                id("lore-concept", 1)
            } else {
                id("organization", 1)
            });
            if kind == "tenure" {
                data["initialization"]["payload"]["holder_id"] = replacement;
            } else {
                data[field] = replacement;
            }
        });
        let report = validation::validate_world(&bad);
        assert!(!report.valid, "{kind} {field}");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error"
                    && diagnostic.entity_id.as_deref() == Some(id(kind, number).as_str())),
            "{:?}",
            report.diagnostics
        );
    }
    governance::party_reference(
        &world,
        &json!({"kind":"organization","id":id("organization",1)}),
    )
    .unwrap();
    assert_eq!(
        generational::legacy(&world, &id("legacy", 1), &at, 100).unwrap(),
        legacy
    );
}
