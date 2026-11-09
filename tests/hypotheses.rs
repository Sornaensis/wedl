//! Author possibilities stay all-time and noncanonical across validation,
//! discovery and reviewed record plans.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, path::PathBuf};
use wedl::{
    Error, authoring, consequences, hypotheses,
    model::{Record, StoryTime, World},
    record_edit, runtime, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PLACE: &str = "loc_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const RETIRED: &str = "obj_00000000000000000000000002";
const ACTOR: &str = "char_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const DRAFT: &str = "event_00000000000000000000000002";
const ACTIVE: &str = "scene_00000000000000000000000001";
const PLANNED: &str = "scene_00000000000000000000000002";
const CONVERSATION: &str = "conv_00000000000000000000000001";
const HYPOTHESIS: &str = "hyp_00000000000000000000000001";
const CREATED: &str = "hyp_00000000000000000000000002";
const MISSING: &str = "obj_00000000000000000000000999";

fn point(tick: i64) -> Json {
    json!({"timeline":"main","tick":tick,"order":0})
}

fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Exact {kind}"),"status":"canonical"});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n# Exact source comment.\n{}---\n{body}",
        serde_yaml::to_string(&value).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}

fn fixture() -> World {
    let records = vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":[],"threads":[],"state_keys":{},
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "current_time":point(10)}),
            "World prose.\n",
        ),
        record("location", PLACE, json!({}), "Location prose.\n"),
        record(
            "object",
            OBJECT,
            json!({"initial_state":{},"object_affordances":["can-open"]}),
            "Object prose.\n",
        ),
        record(
            "object",
            RETIRED,
            json!({"status":"retired"}),
            "Retired prose.\n",
        ),
        record(
            "character",
            ACTOR,
            json!({"initial_state":{}}),
            "Character prose.\n",
        ),
        record(
            "event",
            EVENT,
            json!({"time":point(50),"effects":[]}),
            "Canonical future event.\n",
        ),
        record(
            "event",
            DRAFT,
            json!({"status":"draft","time":point(50),"effects":[]}),
            "Draft event.\n",
        ),
        record(
            "scene",
            ACTIVE,
            json!({"status":"active","location":PLACE,"participants":[],"objects":[OBJECT],
            "time":{"start":point(0),"current":point(10),"end":point(20)}}),
            "Active scene.\n",
        ),
        record(
            "scene",
            PLANNED,
            json!({"status":"planned","location":PLACE,"participants":[],
            "time":{"start":point(20),"current":point(20),"end":point(30)}}),
            "Planned scene.\n",
        ),
        record(
            "conversation",
            CONVERSATION,
            json!({"status":"active","location":PLACE,"participants":[],
            "time":{"start":point(2),"end":point(30)},"turns":[]}),
            "Conversation.\n",
        ),
        record(
            "hypothesis",
            HYPOTHESIS,
            json!({"title":"A possibility 🌲","domain":"author.experimental","status":"open",
            "aliases":["Alias only"],"tags":["literal-tag"],"statement":"  Straße may be copied.  ","subjects":[OBJECT],
            "alternatives":["  Alternativeonly A  ","B","B"],"context":"  Contextonly author note  ",
            "placement":{"timeline":"other","scene":ACTIVE,"event":EVENT,"location":PLACE,"context":"  Placement context  "}}),
            "# Exact body\r\n\r\nBodyonly possibility.\r\nNo final newline  ",
        ),
    ];
    let world = source::world_from_records(
        PathBuf::from("/hypothesis-preview-never-reads-this-root"),
        records,
        None,
    )
    .unwrap();
    assert_valid(&world);
    world
}

fn assert_valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}

fn changed(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut fields = old.frontmatter.clone();
    edit(&mut fields);
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

fn yaml(value: Json) -> Yaml {
    serde_yaml::to_value(value).unwrap()
}
fn input(world: &World, action: Json) -> Json {
    json!({"expectedFingerprint":world.fingerprint,"action":action})
}
fn create(world: &World) -> Json {
    input(
        world,
        json!({"type":"create","id":CREATED,"title":"  Maybe 🌲  ","statement":"  Literal statement  ",
        "subjects":[OBJECT,ACTOR],"alternatives":["  Keep spaces  ","Second","Second"],"context":"  Context stays  ",
        "placement":{"timeline":"other","sceneId":ACTIVE,"eventId":EVENT,"locationId":PLACE}}),
    )
}
fn adopt(world: &World, targets: &[&str]) -> Json {
    input(
        world,
        json!({"type":"adopt","hypothesisId":HYPOTHESIS,"canonicalRecords":targets,"note":"  Literal adoption note  "}),
    )
}
fn reject(world: &World) -> Json {
    input(
        world,
        json!({"type":"reject","hypothesisId":HYPOTHESIS,"note":"  Literal rejection note  "}),
    )
}
fn preview(world: &World, value: &Json) -> Json {
    let request = hypotheses::decode(&value.to_string()).unwrap();
    hypotheses::preview(world, &request, hypotheses::MAX_CHARACTERS).unwrap()
}
fn candidate(world: &World, result: &Json) -> World {
    let mut records = world.records.clone();
    for file in result["preview"]["files"].as_array().unwrap() {
        let record = source::parse_record(
            file["afterSource"].as_str().unwrap().as_bytes(),
            file["sourcePath"].as_str().unwrap(),
        )
        .unwrap();
        records.insert(record.id.clone(), record);
    }
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}
fn has_code(world: &World, code: &str) -> bool {
    validation::validate_world(world)
        .diagnostics
        .iter()
        .any(|d| d.code == code)
}

#[test]
fn closed_request_rejects_duplicate_unknown_missing_coerced_and_null_fields() {
    let world = fixture();
    for valid in [create(&world), adopt(&world, &[EVENT]), reject(&world)] {
        assert!(hypotheses::decode(&valid.to_string()).is_ok());
    }
    for field in ["expectedFingerprint", "action"] {
        let mut value = create(&world);
        value.as_object_mut().unwrap().remove(field);
        assert!(hypotheses::decode(&value.to_string()).is_err(), "{field}");
    }
    for field in [
        "id",
        "title",
        "statement",
        "subjects",
        "alternatives",
        "context",
    ] {
        let mut value = create(&world);
        value["action"].as_object_mut().unwrap().remove(field);
        assert!(hypotheses::decode(&value.to_string()).is_err(), "{field}");
    }
    for (field, value) in [
        ("title", json!(9)),
        ("subjects", json!(OBJECT)),
        ("statement", Json::Null),
        ("placement", Json::Null),
    ] {
        let mut request = create(&world);
        request["action"][field] = value;
        assert!(hypotheses::decode(&request.to_string()).is_err(), "{field}");
    }
    for field in ["timeline", "sceneId", "eventId", "locationId"] {
        let mut value = create(&world);
        value["action"]["placement"][field] = Json::Null;
        assert!(hypotheses::decode(&value.to_string()).is_err());
    }
    for (path, field) in [
        ("", "perspective"),
        ("", "at"),
        ("action", "effects"),
        ("action", "body"),
        ("placement", "tick"),
        ("placement", "context"),
    ] {
        let mut value = create(&world);
        match path {
            "" => value[field] = json!("author"),
            "action" => value["action"][field] = json!([]),
            _ => value["action"]["placement"][field] = json!(1),
        }
        assert!(
            hypotheses::decode(&value.to_string()).is_err(),
            "{path}.{field}"
        );
    }
    let base = reject(&world).to_string();
    for duplicated in [
        base.replacen("\"note\":", "\"note\":\"old\",\"note\":", 1),
        base.replacen("\"note\":", "\"n\\u006fte\":\"old\",\"note\":", 1),
        base.replacen("\"type\":", "\"type\":\"adopt\",\"type\":", 1),
        format!("{base}{{}}"),
    ] {
        assert!(hypotheses::decode(&duplicated).is_err());
    }
    let mut value = adopt(&world, &[EVENT]);
    value["action"]["note"] = Json::Null;
    assert!(hypotheses::decode(&value.to_string()).is_err());
}

#[test]
fn exact_ids_nonblank_values_collections_and_whole_request_are_bounded() {
    let world = fixture();
    for (field, value) in [
        ("id", json!(OBJECT)),
        ("id", json!("A possibility 🌲")),
        ("title", json!(" \n")),
        ("statement", json!("")),
        ("context", json!("\t")),
        ("subjects", json!([])),
        ("subjects", json!([OBJECT, OBJECT])),
        ("subjects", json!(["Exact object"])),
        ("alternatives", json!([])),
        ("alternatives", json!(["okay", ""])),
    ] {
        let mut request = create(&world);
        request["action"][field] = value;
        assert!(hypotheses::decode(&request.to_string()).is_err(), "{field}");
    }
    for (field, maximum) in [
        ("title", hypotheses::MAX_TITLE_BYTES),
        ("statement", hypotheses::MAX_TEXT_BYTES),
        ("context", hypotheses::MAX_TEXT_BYTES),
    ] {
        let mut value = create(&world);
        value["action"][field] = json!("x".repeat(maximum + 1));
        assert!(hypotheses::decode(&value.to_string()).is_err());
    }
    let mut value = create(&world);
    value["action"]["alternatives"] = json!(vec!["x"; hypotheses::MAX_VALUES + 1]);
    assert!(hypotheses::decode(&value.to_string()).is_err());
    value["action"]["alternatives"] = json!(vec!["x"; hypotheses::MAX_VALUES]);
    assert!(hypotheses::decode(&value.to_string()).is_ok());
    assert!(hypotheses::decode(&" ".repeat(hypotheses::MAX_REQUEST_BYTES + 1)).is_err());
    let mut request = hypotheses::decode(&create(&world).to_string()).unwrap();
    if let hypotheses::Action::Create { alternatives, .. } = &mut request.action {
        *alternatives = vec!["x".into(); hypotheses::MAX_VALUES + 1];
    }
    assert!(hypotheses::plan(&world, &request).is_err());
}

#[test]
fn create_produces_one_reviewable_record_and_preserves_literal_values() {
    let world = fixture();
    let value = create(&world);
    let result = preview(&world, &value);
    assert_eq!(result["operation"], "hypothesis-preview");
    assert_eq!(result["scope"], "local-author-hypotheses");
    for field in ["nonCanonical", "complete"] {
        assert_eq!(result[field], true);
    }
    for field in ["canApply", "writesPerformed", "validationComplete"] {
        assert_eq!(result[field], false);
    }
    assert_eq!(result["request"], value);
    assert_eq!(result["plan"]["operations"].as_array().unwrap().len(), 1);
    assert_eq!(result["plan"]["operations"][0]["type"], "record.create");
    assert_eq!(result["preview"]["validation"]["valid"], true);
    let next = candidate(&world, &result);
    let created = &next.records[CREATED];
    assert_eq!(created.status(), "open");
    assert_eq!(created.string("domain"), Some("author.possibilities"));
    for field in ["title", "statement", "subjects", "alternatives", "context"] {
        assert_eq!(
            created.get(field),
            Some(&yaml(value["action"][field].clone())),
            "{field}"
        );
    }
    assert_eq!(created.body, "#   Maybe 🌲  \n\n  Literal statement  \n");
    assert_eq!(
        created.get("placement").unwrap()["context"],
        yaml(value["action"]["context"].clone())
    );
    assert_eq!(
        created.get("placement").unwrap()["scene"].as_str(),
        Some(ACTIVE)
    );
    assert!(created.get("resolution").is_none());
    for (id, old) in &world.records {
        assert_eq!(next.records[id].raw_bytes, old.raw_bytes, "{id}");
    }
    assert_valid(&next);
    let plan = consequences::decode(&result["plan"].to_string()).unwrap();
    assert_eq!(
        consequences::preview(&world, &plan, hypotheses::MAX_CHARACTERS).unwrap(),
        result["preview"]
    );
}

#[test]
fn create_context_only_placement_has_no_default_timeline_or_cursor() {
    let world = fixture();
    let mut value = create(&world);
    value["action"].as_object_mut().unwrap().remove("placement");
    let next = candidate(&world, &preview(&world, &value));
    assert_eq!(
        next.records[CREATED].get("placement"),
        Some(&yaml(json!({"context":"  Context stays  "})))
    );
    assert_eq!(
        next.records[WORLD].raw_bytes,
        world.records[WORLD].raw_bytes
    );
    assert_eq!(
        next.records[ACTIVE].raw_bytes,
        world.records[ACTIVE].raw_bytes
    );
}

#[test]
fn create_refs_and_timeline_resolve_only_exactly_against_captured_source() {
    let world = fixture();
    for (field, replacement) in [
        ("id", json!(HYPOTHESIS)),
        ("subjects", json!([MISSING])),
        ("subjects", json!([CREATED])),
    ] {
        let mut value = create(&world);
        value["action"][field] = replacement;
        assert!(
            hypotheses::plan(&world, &hypotheses::decode(&value.to_string()).unwrap()).is_err()
        );
    }
    for (field, replacement) in [
        ("timeline", json!("missing")),
        ("sceneId", json!("scene_00000000000000000000000999")),
        ("eventId", json!(OBJECT)),
    ] {
        let mut value = create(&world);
        value["action"]["placement"][field] = replacement;
        assert!(
            hypotheses::decode(&value.to_string())
                .and_then(|request| hypotheses::plan(&world, &request))
                .is_err()
        );
    }
    let mut value = create(&world);
    value["expectedFingerprint"] = json!("0".repeat(64));
    assert!(matches!(
        hypotheses::plan(&world, &hypotheses::decode(&value.to_string()).unwrap()),
        Err(Error::Conflict(_))
    ));
    value["expectedFingerprint"] = json!("A".repeat(64));
    assert!(hypotheses::decode(&value.to_string()).is_err());
}

#[test]
fn adoption_links_preexisting_settled_records_without_rewriting_source_payload() {
    let world = fixture();
    let result = preview(
        &world,
        &adopt(&world, &[WORLD, RETIRED, EVENT, ACTIVE, CONVERSATION]),
    );
    let next = candidate(&world, &result);
    let old = &world.records[HYPOTHESIS];
    let resolved = &next.records[HYPOTHESIS];
    assert_eq!(resolved.status(), "adopted");
    assert_eq!(resolved.body, old.body);
    let mut before = old.frontmatter.clone();
    let mut after = resolved.frontmatter.clone();
    for fields in [&mut before, &mut after] {
        fields.as_mapping_mut().unwrap().remove("status");
        fields.as_mapping_mut().unwrap().remove("resolution");
    }
    assert_eq!(before, after);
    assert_eq!(
        resolved.get("resolution").unwrap()["note"].as_str(),
        Some("  Literal adoption note  ")
    );
    for (id, record) in &world.records {
        if id != HYPOTHESIS {
            assert_eq!(next.records[id].raw_bytes, record.raw_bytes);
        }
    }
    assert_valid(&next);
    // A future canonical record is settled even though it is after the current cursor.
    assert_eq!(world.current_time().unwrap().tick, 10);
    assert_eq!(
        world.records[EVENT].get("time").unwrap()["tick"].as_i64(),
        Some(50)
    );
}

#[test]
fn adoptable_status_predicate_matches_pinned_semantics_without_extra_whitelist() {
    let source = fixture().records[OBJECT].clone();
    for kind in [
        "world",
        "character",
        "event",
        "knowledge",
        "object",
        "location",
        "relationship",
        "story-point",
        "scene",
        "conversation",
        "hypothesis",
    ] {
        for status in [
            "canonical",
            "retired",
            "draft",
            "active",
            "closed",
            "planned",
            "abandoned",
            "open",
            "adopted",
            "rejected",
        ] {
            let mut target = source.clone();
            target.kind = kind.into();
            target.frontmatter["status"] = status.into();
            let expected = match kind {
                "hypothesis" => false,
                "scene" | "conversation" => matches!(status, "active" | "closed" | "retired"),
                _ => matches!(status, "canonical" | "retired"),
            };
            assert_eq!(
                hypotheses::is_adoptable_canonical_record(&target),
                expected,
                "{kind}/{status}"
            );
        }
    }
}

#[test]
fn adoption_rejects_drafts_plans_hypotheses_duplicates_and_missing_targets() {
    let world = fixture();
    for targets in [
        vec![DRAFT],
        vec![PLANNED],
        vec![HYPOTHESIS],
        vec![MISSING],
        vec![EVENT, EVENT],
        vec![],
    ] {
        assert!(
            hypotheses::decode(&adopt(&world, &targets).to_string())
                .and_then(|request| hypotheses::plan(&world, &request))
                .is_err(),
            "{targets:?}"
        );
    }
    for note in [json!(""), json!(" \n"), json!(12), Json::Null] {
        let mut value = adopt(&world, &[EVENT]);
        value["action"]["note"] = note;
        assert!(hypotheses::decode(&value.to_string()).is_err());
    }
    let mut value = adopt(&world, &[EVENT]);
    value["action"].as_object_mut().unwrap().remove("note");
    let next = candidate(&world, &preview(&world, &value));
    assert!(
        next.records[HYPOTHESIS]
            .get("resolution")
            .unwrap()
            .get("note")
            .is_none()
    );
}

#[test]
fn rejection_replaces_resolution_and_explicit_reresolution_is_available() {
    let original = fixture();
    let adopted = candidate(&original, &preview(&original, &adopt(&original, &[EVENT])));
    let rejected = candidate(&adopted, &preview(&adopted, &reject(&adopted)));
    assert_eq!(rejected.records[HYPOTHESIS].status(), "rejected");
    assert_eq!(
        rejected.records[HYPOTHESIS].get("resolution"),
        Some(&yaml(json!({"note":"  Literal rejection note  "})))
    );
    assert_eq!(
        rejected.records[HYPOTHESIS].body,
        original.records[HYPOTHESIS].body
    );
    let readopted = candidate(
        &rejected,
        &preview(&rejected, &adopt(&rejected, &[RETIRED])),
    );
    assert_eq!(readopted.records[HYPOTHESIS].status(), "adopted");
    assert_valid(&readopted);
}

#[test]
fn repeated_resolution_plan_is_a_semantic_noop_and_retains_exact_bytes() {
    let world = fixture();
    let adopted = candidate(&world, &preview(&world, &adopt(&world, &[EVENT])));
    let result = preview(&adopted, &adopt(&adopted, &[EVENT]));
    let again = candidate(&adopted, &result);
    assert_eq!(again.fingerprint, adopted.fingerprint);
    assert_eq!(
        again.records[HYPOTHESIS].raw_bytes,
        adopted.records[HYPOTHESIS].raw_bytes
    );
}

#[test]
fn source_requires_nonoperative_shape_instead_of_accepting_partial_notes() {
    let world = fixture();
    for field in [
        "status",
        "statement",
        "subjects",
        "alternatives",
        "context",
        "placement",
    ] {
        let invalid = changed(&world, HYPOTHESIS, |fields| {
            fields.as_mapping_mut().unwrap().remove(field);
        });
        assert!(!validation::validate_world(&invalid).valid, "{field}");
    }
    for field in [
        "time",
        "chronology",
        "effects",
        "initial_state",
        "descriptions",
        "threads",
        "concept_links",
        "x-extra",
        "current_time",
        "knowledge",
    ] {
        let invalid = changed(&world, HYPOTHESIS, |fields| fields[field] = yaml(json!([])));
        assert!(has_code(&invalid, "WDL-HYP-010"), "{field}");
    }
    for (field, value, code) in [
        ("statement", json!(false), "WDL-HYP-001"),
        ("context", json!(" \n"), "WDL-HYP-001"),
        ("subjects", json!([OBJECT, OBJECT]), "WDL-HYP-003"),
        ("subjects", json!([MISSING]), "WDL-HYP-002"),
        ("alternatives", json!(["", "okay"]), "WDL-HYP-004"),
        ("alternatives", json!([1]), "WDL-HYP-004"),
        ("placement", json!({}), "WDL-HYP-005"),
        ("placement", json!({"tick":10}), "WDL-HYP-006"),
        ("placement", json!({"timeline":"missing"}), "WDL-HYP-007"),
        ("placement", json!({"scene":OBJECT}), "WDL-HYP-008"),
        ("placement", json!({"context":""}), "WDL-HYP-009"),
    ] {
        assert!(
            has_code(
                &changed(&world, HYPOTHESIS, |fields| fields[field] = yaml(value)),
                code
            ),
            "{field}/{code}"
        );
    }
}

#[test]
fn source_resolution_rules_cover_null_unknown_fields_notes_and_target_settlement() {
    let world = fixture();
    assert_valid(&changed(&world, HYPOTHESIS, |fields| {
        fields["resolution"] = Yaml::Null
    }));
    assert_valid(&changed(&world, HYPOTHESIS, |fields| {
        fields["placement"] = yaml(json!({"scene":null}))
    }));
    assert!(has_code(
        &changed(&world, HYPOTHESIS, |fields| fields["resolution"] =
            yaml(json!({}))),
        "WDL-HYP-011"
    ));
    for (status, resolution, code) in [
        ("adopted", json!({}), "WDL-HYP-011"),
        (
            "adopted",
            json!({"canonical_entities":[DRAFT]}),
            "WDL-HYP-012",
        ),
        (
            "adopted",
            json!({"canonical_entities":[HYPOTHESIS]}),
            "WDL-HYP-012",
        ),
        (
            "adopted",
            json!({"canonical_entities":[EVENT,EVENT]}),
            "WDL-HYP-012",
        ),
        (
            "adopted",
            json!({"canonical_entities":[EVENT],"note":""}),
            "WDL-HYP-013",
        ),
        (
            "adopted",
            json!({"canonical_entities":[EVENT],"truth":true}),
            "WDL-HYP-014",
        ),
        ("rejected", json!({}), "WDL-HYP-013"),
        (
            "rejected",
            json!({"note":"why","canonical_entities":[EVENT]}),
            "WDL-HYP-014",
        ),
    ] {
        let invalid = changed(&world, HYPOTHESIS, |fields| {
            fields["status"] = status.into();
            fields["resolution"] = yaml(resolution)
        });
        assert!(has_code(&invalid, code), "{status}/{code}");
    }
}

#[test]
fn typed_record_codec_can_reopen_explicitly_and_support_candidate_forward_references() {
    let world = fixture();
    let adopted = candidate(&world, &preview(&world, &adopt(&world, &[EVENT])));
    let reopened = record_edit::update(
        &adopted,
        HYPOTHESIS,
        "kind: hypothesis\nstatus: open\nresolution: null\n",
    )
    .unwrap();
    assert_eq!(reopened.status(), "open");
    let forward = "obj_00000000000000000000000009";
    let draft = format!(
        "kind: hypothesis\nid: {CREATED}\ntitle: A forward note\nstatement: Maybe\nsubjects: [{forward}]\nalternatives: [One]\ncontext: Author\nplacement: {{context: Author}}\n"
    );
    let plan = consequences::Plan {
        expected_fingerprint: world.fingerprint.clone(),
        analysis: None,
        operations: vec![
            consequences::Operation::Create { draft },
            consequences::Operation::Create {
                draft: format!("kind: object\nid: {forward}\ntitle: Explicit new object\n"),
            },
        ],
    };
    let result = consequences::preview(&world, &plan, hypotheses::MAX_CHARACTERS).unwrap();
    assert_eq!(result["validation"]["valid"], true);
}

#[test]
fn list_and_detail_return_literal_noncanonical_identity_with_snapshot_citations() {
    let mut world = fixture();
    world.revision = Some("a".repeat(40));
    world.records.get_mut(HYPOTHESIS).unwrap().blob_oid = Some("b".repeat(40));
    let list =
        hypotheses::list(&world, Some("open"), None, 100, hypotheses::MAX_CHARACTERS).unwrap();
    assert_eq!(list["operation"], "hypothesis-list");
    assert_eq!(list["scope"], "author-all-time-hypotheses");
    assert_eq!(list["status"], "open");
    assert_eq!(list["text"], Json::Null);
    assert_eq!(list["count"], 1);
    let detail = hypotheses::detail(&world, HYPOTHESIS, hypotheses::MAX_CHARACTERS).unwrap();
    assert_eq!(detail["hypothesis"], list["hypotheses"][0]);
    let value = &detail["hypothesis"];
    assert_eq!(value["id"], HYPOTHESIS);
    assert_eq!(value["nonCanonical"], true);
    assert_eq!(value["statement"], "  Straße may be copied.  ");
    assert_eq!(
        value["alternatives"],
        json!(["  Alternativeonly A  ", "B", "B"])
    );
    assert_eq!(value["subjects"][0]["id"], OBJECT);
    assert_eq!(value["placement"]["scene"]["id"], ACTIVE);
    assert_eq!(value["placement"]["context"], "  Placement context  ");
    assert_eq!(value["citation"]["fingerprint"], world.fingerprint);
    assert_eq!(value["citation"]["revision"], "a".repeat(40));
    assert_eq!(value["citation"]["blobOid"], "b".repeat(40));
    assert_eq!(value["citation"]["entityId"], HYPOTHESIS);
    assert!(value.get("body").is_none());
    assert!(value.get("at").is_none());
    assert!(hypotheses::detail(&world, "A possibility 🌲", hypotheses::MAX_CHARACTERS).is_err());
    assert!(hypotheses::detail(&world, OBJECT, hypotheses::MAX_CHARACTERS).is_err());
}

#[test]
fn list_filter_uses_unicode_casefold_and_only_title_statement_body() {
    let world = fixture();
    for query in ["STRASSE", "bodyONLY", "possibility 🌲", "  Straße may"] {
        assert_eq!(
            hypotheses::list(&world, None, Some(query), 10, hypotheses::MAX_CHARACTERS).unwrap()["count"],
            1,
            "{query}"
        );
    }
    for query in [
        "Alternativeonly",
        "Contextonly",
        "literal-tag",
        "Alias only",
    ] {
        assert_eq!(
            hypotheses::list(&world, None, Some(query), 10, hypotheses::MAX_CHARACTERS).unwrap()["count"],
            0,
            "{query}"
        );
    }
    assert_eq!(
        hypotheses::list(
            &world,
            Some("adopted"),
            None,
            10,
            hypotheses::MAX_CHARACTERS
        )
        .unwrap()["count"],
        0
    );
    assert_eq!(
        hypotheses::list(&world, None, Some(""), 10, hypotheses::MAX_CHARACTERS).unwrap()["count"],
        1
    );
}

#[test]
fn all_time_projection_ignores_cursor_changes_and_resolution_never_changes_canonical_state() {
    let world = fixture();
    let next = candidate(&world, &preview(&world, &adopt(&world, &[EVENT])));
    let at = StoryTime::new("main", 10, 0).unwrap();
    assert_eq!(
        runtime::resolved_state(&world, OBJECT, &at).unwrap(),
        runtime::resolved_state(&next, OBJECT, &at).unwrap()
    );
    let mut before_knowledge = runtime::knowledge(&world, ACTOR, &at, false).unwrap();
    let mut after_knowledge = runtime::knowledge(&next, ACTOR, &at, false).unwrap();
    before_knowledge
        .as_object_mut()
        .unwrap()
        .remove("fingerprint");
    after_knowledge
        .as_object_mut()
        .unwrap()
        .remove("fingerprint");
    assert_eq!(before_knowledge, after_knowledge);
    let shifted = changed(
        &changed(&world, WORLD, |v| v["current_time"] = yaml(point(11))),
        ACTIVE,
        |v| v["time"]["current"] = yaml(point(11)),
    );
    let mut before =
        hypotheses::detail(&world, HYPOTHESIS, hypotheses::MAX_CHARACTERS).unwrap()["hypothesis"]
            .clone();
    let mut after =
        hypotheses::detail(&shifted, HYPOTHESIS, hypotheses::MAX_CHARACTERS).unwrap()["hypothesis"]
            .clone();
    before.as_object_mut().unwrap().remove("citation");
    after.as_object_mut().unwrap().remove("citation");
    assert_eq!(before, after);
}

#[test]
fn invalid_or_stale_source_is_refused_before_projection_or_plan() {
    let world = fixture();
    let invalid = changed(&world, HYPOTHESIS, |v| v["statement"] = false.into());
    assert!(matches!(
        hypotheses::list(&invalid, None, None, 10, hypotheses::MAX_CHARACTERS),
        Err(Error::Validation(_))
    ));
    assert!(hypotheses::detail(&invalid, HYPOTHESIS, hypotheses::MAX_CHARACTERS).is_err());
    let request = hypotheses::decode(&reject(&invalid).to_string()).unwrap();
    assert!(hypotheses::plan(&invalid, &request).is_err());
    let mut stale = world.clone();
    stale.records.get_mut(HYPOTHESIS).unwrap().frontmatter["statement"] =
        "Unparsed mutation".into();
    assert!(hypotheses::list(&stale, None, None, 10, hypotheses::MAX_CHARACTERS).is_err());
}

#[test]
fn complete_output_and_item_budgets_never_return_partial_success() {
    let world = fixture();
    for max in [0, 255, hypotheses::MAX_CHARACTERS + 1] {
        assert!(hypotheses::list(&world, None, None, 10, max).is_err());
        assert!(hypotheses::detail(&world, HYPOTHESIS, max).is_err());
        assert!(
            hypotheses::preview(
                &world,
                &hypotheses::decode(&reject(&world).to_string()).unwrap(),
                max
            )
            .is_err()
        );
    }
    for max in [0, hypotheses::MAX_ITEMS + 1] {
        assert!(hypotheses::list(&world, None, None, max, hypotheses::MAX_CHARACTERS).is_err());
    }
    assert!(hypotheses::detail(&world, HYPOTHESIS, 256).is_err());
    let request = hypotheses::decode(&reject(&world).to_string()).unwrap();
    assert!(hypotheses::preview(&world, &request, 256).is_err());
    let two = candidate(&world, &preview(&world, &create(&world)));
    assert!(hypotheses::list(&two, None, None, 1, hypotheses::MAX_CHARACTERS).is_err());
    assert_eq!(
        hypotheses::list(&two, None, Some("Straße"), 1, hypotheses::MAX_CHARACTERS).unwrap()["count"],
        1
    );
    assert!(hypotheses::list(&world, Some("Open"), None, 10, hypotheses::MAX_CHARACTERS).is_err());
    assert!(
        hypotheses::list(
            &world,
            None,
            Some(&"🌲".repeat(1025)),
            10,
            hypotheses::MAX_CHARACTERS
        )
        .is_err()
    );
    let result = hypotheses::detail(&world, HYPOTHESIS, 10_000).unwrap();
    let size = result.to_string().chars().count();
    assert!(hypotheses::detail(&world, HYPOTHESIS, size + 10).is_ok());
    assert!(hypotheses::detail(&world, HYPOTHESIS, size - 10).is_err());
}

#[test]
fn direct_typed_requests_cannot_bypass_complete_json_or_plan_encoding_bounds() {
    let world = fixture();
    let mut request = hypotheses::decode(&create(&world).to_string()).unwrap();
    if let hypotheses::Action::Create { alternatives, .. } = &mut request.action {
        *alternatives = vec!["x".repeat(60_000); 5];
    }
    assert!(hypotheses::plan(&world, &request).is_err());
    let mut value = create(&world);
    value["action"]["statement"] = json!("x".repeat(hypotheses::MAX_TEXT_BYTES));
    value["action"]["alternatives"] = json!([
        "y".repeat(hypotheses::MAX_TEXT_BYTES),
        "z".repeat(hypotheses::MAX_TEXT_BYTES)
    ]);
    let request = hypotheses::decode(&value.to_string()).unwrap();
    assert!(hypotheses::plan(&world, &request).is_err());
}

#[test]
fn source_lexical_lane_includes_alternatives_and_context_with_pinned_scoring() {
    let world = fixture();
    for query in ["STRASSE", "alternativeonly", "contextonly"] {
        let hits = hypotheses::source_search(&world, query, 10).unwrap();
        assert_eq!(hits.len(), 1, "{query}");
        assert_eq!(hits[0]["id"], HYPOTHESIS);
        assert_eq!(hits[0]["lanes"], json!(["hypothesis-source"]));
        assert_eq!(hits[0]["nonCanonical"], true);
        assert_eq!(hits[0]["metadata"]["hypothesisStatus"], "open");
        assert_eq!(hits[0]["audienceKind"], "author");
    }
    assert_eq!(
        hypotheses::source_search(&world, "\"Straße may\"", 10).unwrap()[0]["score"],
        4.0
    );
    assert_eq!(
        hypotheses::source_search(&world, "STRASSE strasse", 10).unwrap()[0]["score"],
        1.0
    );
    assert!(
        hypotheses::source_search(&world, "stras", 10)
            .unwrap()
            .is_empty()
    );
    assert!(
        hypotheses::source_search(&world, "!?", 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        hypotheses::source_search(&world, "be", 10).unwrap().len(),
        1
    ); // stopword-only fallback
    assert_eq!(hypotheses::source_search(&world, "a", 10).unwrap().len(), 0); // one-character terms excluded
}

#[test]
fn source_lexical_lane_orders_ties_by_exact_id_and_bounds_all_inputs() {
    let world = fixture();
    let mut value = create(&world);
    value["action"]["context"] = json!("Contextonly");
    let two = candidate(&world, &preview(&world, &value));
    let hits = hypotheses::source_search(&two, "contextonly", 1).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], HYPOTHESIS);
    for limit in [0, 101] {
        assert!(hypotheses::source_search(&world, "strasse", limit).is_err());
    }
    for query in ["".into(), " ".into(), "x".repeat(4097), "word ".repeat(65)] {
        assert!(hypotheses::source_search(&world, &query, 10).is_err());
    }
    let large = changed(&world, HYPOTHESIS, |v| {
        v["statement"] = "x".repeat(hypotheses::MAX_SEARCH_BYTES).into()
    });
    assert!(hypotheses::source_search(&large, "strasse", 10).is_err());
}

#[test]
fn source_lexical_queries_handle_quoted_escapes_and_python_word_categories() {
    let world = changed(&fixture(), HYPOTHESIS, |value| {
        value["statement"] =
            "xy\u{05b0}zz ab\u{093e}cd one \"quoted\" phrase ab²cd under_score".into();
    });
    // Combining marks with the Unicode Alphabetic property are still not
    // Python word characters. Number categories, including superscripts, are.
    for term in ["xy", "zz", "ab", "cd"] {
        assert_eq!(
            hypotheses::source_search(&world, term, 10).unwrap().len(),
            1,
            "{term}"
        );
    }
    assert_eq!(
        hypotheses::source_search(&world, "xy\u{05b0}zz", 10).unwrap()[0]["score"],
        2.0
    );
    assert!(
        hypotheses::source_search(&world, "under", 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        hypotheses::source_search(&world, "ab²cd", 10).unwrap()[0]["score"],
        1.0
    );
    assert_eq!(
        hypotheses::source_search(&world, "\"one \\\"quoted\\\" phrase\"", 10).unwrap()[0]["score"],
        4.0
    );
    assert_eq!(
        hypotheses::source_search(&world, "\"quoted\" \"quoted\"", 10).unwrap()[0]["score"],
        4.0
    );
    assert_eq!(
        hypotheses::source_search(&world, "\"unterminated quoted", 10).unwrap()[0]["score"],
        1.0
    );
    assert_eq!(
        hypotheses::source_search(&world, "\"\u{001c}quoted\u{001f}\"", 10).unwrap()[0]["score"],
        4.0
    );
    let punctuation = changed(&world, HYPOTHESIS, |value| {
        value["statement"] = "a--- ----".into()
    });
    assert_eq!(
        hypotheses::source_search(&punctuation, "--", 10).unwrap()[0]["score"],
        3.0
    );
}

#[test]
fn complete_projection_budgets_count_expanded_references_escaping_and_unicode() {
    let world = fixture();
    let two = candidate(&world, &preview(&world, &create(&world)));
    let expanded = changed(&two, OBJECT, |fields| {
        fields["title"] = "🌲\"\\\n".repeat(10_000).into()
    });
    let mut limit = hypotheses::MAX_CHARACTERS;
    for _ in 0..3 {
        let report = hypotheses::list(&expanded, None, None, 2, limit).unwrap();
        let required = report.to_string().chars().count();
        if required == limit {
            break;
        }
        limit = required;
    }
    let exact = hypotheses::list(&expanded, None, None, 2, limit).unwrap();
    assert_eq!(exact.to_string().chars().count(), limit);
    assert!(exact.to_string().len() > limit);
    let actual = exact["hypotheses"][0]["subjects"][0]["title"]
        .as_str()
        .unwrap();
    let expected = expanded.records[OBJECT].title.as_str();
    assert!(
        actual == expected,
        "reference title differed: actual {} bytes, source {} bytes",
        actual.len(),
        expected.len()
    );
    assert!(hypotheses::list(&expanded, None, None, 2, limit - 1).is_err());
    let oversized = changed(&world, OBJECT, |fields| {
        fields["title"] = "x".repeat(hypotheses::MAX_CHARACTERS).into()
    });
    assert!(hypotheses::detail(&oversized, HYPOTHESIS, hypotheses::MAX_CHARACTERS).is_err());
    assert!(hypotheses::list(&oversized, None, None, 1, hypotheses::MAX_CHARACTERS).is_err());
    // A filtered-out owner never expands its subject's title into the result.
    assert_eq!(
        hypotheses::list(&oversized, Some("rejected"), None, 1, 1000).unwrap()["count"],
        0
    );
}

#[test]
fn id_shaped_statements_alternatives_context_and_notes_stay_literal() {
    let world = fixture();
    let mut value = create(&world);
    for field in ["title", "statement", "context"] {
        value["action"][field] = json!(MISSING);
    }
    value["action"]["alternatives"] = json!([MISSING]);
    let next = candidate(&world, &preview(&world, &value));
    assert_valid(&next);
    let mut request = adopt(&next, &[EVENT]);
    request["action"]["hypothesisId"] = json!(CREATED);
    request["action"]["note"] = json!(MISSING);
    let adopted = candidate(&next, &preview(&next, &request));
    assert_valid(&adopted);
    assert_eq!(
        adopted.records[CREATED].get("resolution").unwrap()["note"].as_str(),
        Some(MISSING)
    );
    assert!(!adopted.records.contains_key(MISSING));
}

#[test]
fn reads_previews_and_historical_plans_do_not_touch_files_or_indexes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("world");
    for record in fixture().records.values() {
        let path = root.join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &record.raw_bytes).unwrap();
    }
    let world = source::load_world(&root, None).unwrap();
    let image = |world: &World| -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
        world
            .records
            .values()
            .map(|r| {
                let p = world.root.join(&r.source_path);
                (
                    r.source_path.clone(),
                    (
                        fs::read(&p).unwrap(),
                        fs::metadata(p).unwrap().modified().unwrap(),
                    ),
                )
            })
            .collect()
    };
    let before = image(&world);
    hypotheses::list(&world, None, None, 100, hypotheses::MAX_CHARACTERS).unwrap();
    hypotheses::detail(&world, HYPOTHESIS, hypotheses::MAX_CHARACTERS).unwrap();
    hypotheses::source_search(&world, "strasse", 10).unwrap();
    let result = preview(&world, &adopt(&world, &[EVENT]));
    assert_eq!(image(&world), before);
    assert!(!root.join(".wedl").exists());
    let mut historical = world.clone();
    historical.root = PathBuf::from("/no-live-root-for-this-snapshot");
    historical.revision = Some("a".repeat(40));
    let old = preview(&historical, &reject(&historical));
    assert_eq!(old["revision"], "a".repeat(40));
    assert_eq!(old["canApply"], false);
    // The exported operation is reviewed/applied by the existing writer only.
    let operation = &result["plan"]["operations"][0];
    let generic = authoring::preview_record_update(
        &world,
        HYPOTHESIS,
        operation["patch"].as_str().unwrap(),
        &world.fingerprint,
    )
    .unwrap();
    assert_eq!(generic["validation"]["valid"], true);
    assert_eq!(image(&world), before);
}
