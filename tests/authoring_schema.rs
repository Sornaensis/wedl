//! Static guidance is checked against actual request construction/decoding.
//! Full source-domain validation remains a separate preview responsibility.
use serde_json::{Value as Json, json};
use std::{collections::BTreeSet, path::PathBuf};
use wedl::{
    authoring_schema, consequences,
    model::{Record, World},
    record_edit, source,
};

const ACTOR: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";

fn id(kind: &str) -> String {
    let prefix = match kind {
        "world" => "world",
        "character" => "char",
        "location" => "loc",
        "object" => "obj",
        "event" => "event",
        "knowledge" => "know",
        "environment" => "env",
        "relationship" => "rel",
        "story-point" => "sp",
        "hypothesis" => "hyp",
        "scene" => "scene",
        "conversation" => "conv",
        "route" => "route",
        "map" => "map",
        "anchor" => "anchor",
        "portal" => "portal",
        "overlay" => "overlay",
        "lore-concept" => "concept",
        "assertion" => "assertion",
        "existence-episode" => "existence",
        "context-brief" => "brief",
        "parentage" => "kinship",
        "union" => "union",
        "organization" => "organization",
        "affiliation" => "affiliation",
        "legacy" => "legacy",
        "tenure" => "tenure",
        "claim" => "claim",
        "vital-history" => "vital",
        other => panic!("missing independent record fixture for {other}"),
    };
    format!("{prefix}_00000000000000000000000009")
}

fn record(kind: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id(kind),"title":"Retained source"});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!(
            "---\r\n{}---\r\nExact body 🌲\r\n",
            serde_yaml::to_string(&data).unwrap().replace('\n', "\r\n")
        )
        .as_bytes(),
        &format!("story/{}.md", id(kind)),
    )
    .unwrap()
}

fn world(extra: Option<Record>) -> World {
    let owner = record(
        "world",
        json!({"capabilities":[],"timelines":[{"id":"main","label":"Main"}],"default_timeline":"main","state_keys":{}}),
    );
    let mut records = vec![owner];
    if let Some(record) = extra {
        records.push(record);
    }
    source::world_from_records(
        PathBuf::from("/static-guidance-does-not-read-source"),
        records,
        None,
    )
    .unwrap()
}

fn fields(value: &Json) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

fn plan() -> Json {
    json!({"expectedFingerprint":"0".repeat(64),"operations":[{"type":"record.create","draft":format!("kind: character\nid: {ACTOR}\ntitle: Proposed actor\n")} ]})
}

fn analysis(predicate: Json) -> Json {
    json!({"at":{"timeline":"main","tick":"9007199254740993","order":"0"},"policy":"advisory","expectations":[{"id":"check","predicate":predicate}]})
}

fn decode(value: &Json) -> bool {
    consequences::decode(&value.to_string()).is_ok()
}

#[test]
fn exported_request_schema_resolves_every_reference_without_its_guidance_wrapper() {
    let result = authoring_schema::schema();
    let exported: Json =
        serde_json::from_slice(&serde_json::to_vec(&result["requestSchema"]).unwrap()).unwrap();
    fn walk(root: &Json, value: &Json) {
        match value {
            Json::Object(fields) => {
                if let Some(reference) = fields.get("$ref") {
                    let fragment = reference
                        .as_str()
                        .unwrap()
                        .strip_prefix('#')
                        .expect("schema has no external reference dependency");
                    assert!(root.pointer(fragment).is_some(), "unresolved {fragment}");
                }
                for value in fields.values() {
                    walk(root, value);
                }
            }
            Json::Array(values) => {
                for value in values {
                    walk(root, value);
                }
            }
            _ => {}
        }
    }
    walk(&exported, &exported);
    assert_eq!(
        exported["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(exported["$ref"], "#/$defs/ConsequencePlan");
    assert_eq!(result["sourceIndependent"], true);
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["validationComplete"], false);
    assert!(result.get("fingerprint").is_none() && result.get("revision").is_none());
}

#[test]
fn every_advertised_creation_requirement_is_enforced_by_the_native_record_codec() {
    let result = authoring_schema::schema();
    let kinds = result["recordKinds"].as_array().unwrap();
    let names = kinds
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(names.len(), 29);
    assert_eq!(
        kinds
            .iter()
            .filter(|entry| entry["createSupported"] == true)
            .count(),
        28
    );
    let world = world(None);
    for entry in kinds {
        let kind = entry["kind"].as_str().unwrap();
        let required = fields(&entry["createRequired"]);
        let optional = fields(&entry["createOptional"]);
        assert!(required.iter().all(|key| !optional.contains(key)), "{kind}");
        let mut input = json!({"kind":kind,"id":id(kind),"title":"New source"});
        if kind == "world" {
            assert!(required.is_empty() && optional.is_empty());
            assert_eq!(entry["createDefaults"], Json::Null);
            assert!(
                record_edit::create(&world, &input.to_string())
                    .unwrap_err()
                    .to_string()
                    .contains("update-only")
            );
            continue;
        }
        for key in &required {
            if input.get(*key).is_none() {
                input[*key] = Json::Null;
            }
        }
        // Placeholder values deliberately stop at typed field validation. This
        // checks complete top-level requiredness without claiming a valid world.
        if let Err(error) = record_edit::create(&world, &input.to_string()) {
            assert!(
                !error.to_string().ends_with(" is required"),
                "{kind}: an unadvertised requirement: {error}"
            );
        }
        for key in required {
            let mut missing = input.clone();
            missing.as_object_mut().unwrap().remove(key);
            let error = record_edit::create(&world, &missing.to_string()).unwrap_err();
            assert!(
                error.to_string().contains(&format!(".{key} is required")),
                "{kind}.{key}: {error}"
            );
        }
    }
}

#[test]
fn update_guidance_keeps_identity_closed_and_statuses_match_actual_edits() {
    let result = authoring_schema::schema();
    for entry in result["recordKinds"].as_array().unwrap() {
        let kind = entry["kind"].as_str().unwrap();
        // These are source-consistent structural edit fixtures. The native
        // candidate validator separately owns complete per-domain requirements.
        let world = if kind == "world" {
            world(None)
        } else {
            world(Some(record(kind, json!({}))))
        };
        let target = id(kind);
        let before = world.records[&target].raw_bytes.clone();
        assert_eq!(entry["updateRequired"], json!(["kind"]));
        let optional = fields(&entry["updateOptional"]);
        for reserved in ["kind", "id", "schema", "sourcePath"] {
            assert!(!optional.contains(&reserved));
        }
        assert_eq!(
            record_edit::update(&world, &target, &json!({"kind":kind}).to_string())
                .unwrap()
                .raw_bytes,
            before
        );
        for status in fields(&entry["statuses"]) {
            let updated = record_edit::update(
                &world,
                &target,
                &json!({"kind":kind,"status":status}).to_string(),
            )
            .unwrap();
            assert_eq!(updated.string("status"), Some(status));
        }
        for extra in [
            json!({"kind":kind,"id":target}),
            json!({"kind":kind,"schema":"wedl/v1"}),
            json!({"kind":kind,"status":"unsupported"}),
        ] {
            assert!(
                record_edit::update(&world, &target, &extra.to_string()).is_err(),
                "{kind}: {extra}"
            );
        }
        if kind == "hypothesis" {
            for key in [
                "chronology",
                "concept_links",
                "descriptions",
                "threads",
                "audience",
                "perspectives",
            ] {
                assert!(!optional.contains(&key));
                assert!(
                    record_edit::update(&world, &target, &json!({"kind":kind,key:[]}).to_string())
                        .is_err()
                );
            }
        }
        assert_eq!(world.records[&target].raw_bytes, before);
    }
}

#[test]
fn literal_creation_defaults_describe_real_constructed_records() {
    let result = authoring_schema::schema();
    let world = world(None);
    for (kind, extra) in [
        ("character", json!({})),
        ("location", json!({})),
        ("object", json!({})),
        ("event", json!({"time":{"tick":0}})),
        (
            "knowledge",
            json!({"knower":ACTOR,"claim":{"key":"claim","statement":"Literal claim"}}),
        ),
        (
            "relationship",
            json!({"from":ACTOR,"to":"char_00000000000000000000000002"}),
        ),
        (
            "scene",
            json!({"location":PLACE,"time":{"start":{"tick":0},"current":{"tick":0}}}),
        ),
        (
            "conversation",
            json!({"location":PLACE,"time":{"start":{"tick":0}}}),
        ),
        (
            "route",
            json!({"from_location_id":PLACE,"to_location_id":"loc_00000000000000000000000002","direction":"one-way","modes":["walk"]}),
        ),
        (
            "hypothesis",
            json!({"statement":"Maybe","subjects":[ACTOR],"alternatives":["Possible"],"context":"Question","placement":{"context":"Question"}}),
        ),
        (
            "organization",
            json!({"organization_kind":"institution","initialization":{"transition_id":"transition_00000000000000000000000001","transition_kind":"organization-initialize","applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":0,"order":0}},"payload":{"title":"New organization","aliases":[]}}}),
        ),
    ] {
        let mut request = json!({"kind":kind,"id":id(kind),"title":"New source"});
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let record = record_edit::create(&world, &request.to_string()).unwrap();
        let entry = result["recordKinds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == kind)
            .unwrap();
        for (key, value) in entry["createDefaults"].as_object().unwrap() {
            if key == "body" {
                assert_eq!(record.body, value.as_str().unwrap());
            } else {
                assert_eq!(
                    serde_json::to_value(record.get(key).unwrap()).unwrap(),
                    *value,
                    "{kind}.{key}"
                );
            }
        }
    }
}

#[test]
fn declared_analysis_variants_follow_actual_closed_native_decoding() {
    let result = authoring_schema::schema();
    let definitions = &result["requestSchema"]["$defs"];
    let variants = definitions["ExpectationPredicate"]["oneOf"]
        .as_array()
        .unwrap();
    let predicates = [
        json!({"kind":"state.equals","targetId":ACTOR,"key":"count","expectedValueRaw":"9007199254740993"}),
        json!({"kind":"state.absent","targetId":"location:town/north-quay","key":"owner"}),
        json!({"kind":"knowledge.state","knowledgeId":ACTOR,"state":"remembered"}),
        json!({"kind":"relationship.matches","relationshipId":ACTOR,"values":{"metrics":{},"facets":["",""]}}),
        json!({"kind":"story-point.state","storyPointId":ACTOR,"state":"resolved"}),
        json!({"kind":"outcome.linked","eventId":ACTOR,"targetId":PLACE}),
        json!({"kind":"transition.caused","recordId":ACTOR,"transitionId":"authored transition","eventId":ACTOR}),
    ];
    assert_eq!(variants.len(), predicates.len());
    for predicate in predicates {
        let definition = variants
            .iter()
            .map(|variant| {
                result["requestSchema"]
                    .pointer(variant["$ref"].as_str().unwrap().strip_prefix('#').unwrap())
                    .unwrap()
            })
            .find(|definition| definition["properties"]["kind"]["const"] == predicate["kind"])
            .unwrap();
        assert_eq!(definition["additionalProperties"], false);
        assert_eq!(
            fields(&definition["required"])
                .into_iter()
                .collect::<BTreeSet<_>>(),
            predicate
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect()
        );
        let mut input = plan();
        input["analysis"] = analysis(predicate.clone());
        assert!(decode(&input), "{input}");
        for key in fields(&definition["required"]) {
            let mut missing = input.clone();
            missing["analysis"]["expectations"][0]["predicate"]
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(!decode(&missing), "missing {key}: {missing}");
        }
        input["analysis"]["expectations"][0]["predicate"]["unexpected"] = json!(true);
        assert!(!decode(&input));
    }
    assert!(decode(&plan()));
    let mut null = plan();
    null["analysis"] = Json::Null;
    assert!(!decode(&null));
    assert_eq!(definitions["Analysis"]["type"], "object");
    assert!(!fields(&definitions["ConsequencePlan"]["required"]).contains(&"analysis"));
}

#[test]
fn published_limits_and_omission_rules_match_native_boundaries() {
    let result = authoring_schema::schema();
    let definitions = &result["requestSchema"]["$defs"];
    for values in [
        json!({"metrics":{}}),
        json!({"facets":[]}),
        json!({"facets":["",""]}),
        json!({"status":"active"}),
    ] {
        let mut input = plan();
        input["analysis"] =
            analysis(json!({"kind":"relationship.matches","relationshipId":ACTOR,"values":values}));
        assert!(decode(&input));
    }
    for values in [
        json!({}),
        json!({"metrics":null}),
        json!({"facets":null}),
        json!({"status":null}),
    ] {
        let mut input = plan();
        input["analysis"] =
            analysis(json!({"kind":"relationship.matches","relationshipId":ACTOR,"values":values}));
        assert!(!decode(&input));
    }
    assert_eq!(definitions["RelationshipValues"]["minProperties"], 1);
    assert_eq!(definitions["RelationshipValues"]["required"], json!([]));
    let limit = result["bounds"]["maxPlanOperations"].as_u64().unwrap() as usize;
    assert_eq!(
        definitions["ConsequencePlan"]["properties"]["operations"]["maxItems"],
        limit
    );
    for (count, accepted) in [(0, false), (1, true), (limit, true), (limit + 1, false)] {
        let mut input = plan();
        input["operations"] = json!(vec![input["operations"][0].clone(); count]);
        assert_eq!(decode(&input), accepted, "operation count {count}");
    }
    let base = plan().to_string();
    let bytes = result["bounds"]["maxPlanBytes"].as_u64().unwrap() as usize;
    let exact = format!("{base}{}", " ".repeat(bytes - base.len()));
    assert!(consequences::decode(&exact).is_ok());
    assert!(consequences::decode(&(exact + " ")).is_err());
    let maximum = result["bounds"]["maxExpectations"].as_u64().unwrap() as usize;
    for (count, accepted) in [(0, true), (maximum, true), (maximum + 1, false)] {
        let mut input = plan();
        input["analysis"] = analysis(json!({"kind":"state.absent","targetId":ACTOR,"key":"state"}));
        input["analysis"]["expectations"]=json!((0..count).map(|i|json!({"id":format!("check-{i}"),"predicate":{"kind":"state.absent","targetId":ACTOR,"key":"state"}})).collect::<Vec<_>>());
        assert_eq!(decode(&input), accepted, "expectation count {count}");
    }
}
