//! General editable starters use exact captured source and the native Plan path.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error,
    authoring_tools::{self, Family, ScaffoldRequest},
    consequences,
    model::{Record, World},
    record_edit, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ACTOR: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const ASSERTION: &str = "assertion_00000000000000000000000001";
const BRIEF: &str = "brief_00000000000000000000000001";
const EPISODE: &str = "existence_00000000000000000000000001";
const HYPOTHESIS: &str = "hyp_00000000000000000000000001";
const ORGANIZATION: &str = "organization_00000000000000000000000009";
const BODY: &str = "\r\n# Retained 🌲\r\nNo final newline  \t";
const BIG: i64 = 9_007_199_254_740_993;

fn from_fields(data: &Yaml, path: &str, body: &str) -> Record {
    let raw = format!(
        "---\r\n# Preserve this source comment and CRLF header.\r\n{}---\r\n{body}",
        serde_yaml::to_string(data).unwrap().replace('\n', "\r\n"),
    );
    source::parse_record(raw.as_bytes(), path).unwrap()
}

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Title {id}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    from_fields(
        &serde_yaml::to_value(data).unwrap(),
        &format!("story/{id}.md"),
        BODY,
    )
}

fn world(extended: bool) -> World {
    let mut fields = json!({
        "capabilities":[],"default_timeline":"main",
        "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
        "current_time":{"timeline":"other","tick":BIG,"order":i32::MIN},
        "state_keys":{"character":{"counter":{"type":"integer"}}},
    });
    if extended {
        fields["capabilities"] = json!(validation::CAPABILITIES);
        fields["profiles"] = json!([]);
        fields["predicates"] = json!([{"id":"has-account","subject_kinds":["character"],"value_type":{"type":"string"}}]);
    }
    let mut records = vec![
        record("world", WORLD, fields),
        record(
            "character",
            ACTOR,
            json!({"aliases":["Captain"],"initial_state":{"counter":BIG},
                "body":"A direct-source frontmatter field, not the Markdown body.",
                "x-opaque":{"values":[1,1.0,"1",null],"exact":BIG}}),
        ),
        record("location", PLACE, json!({})),
    ];
    if extended {
        records.extend([
            record("knowledge", KNOWLEDGE, json!({"knower":ACTOR,"claim":{"key":"note","statement":"Exact claimed words"},"transitions":[]})),
            record("assertion", ASSERTION, json!({"domain":"test.account","subject":{"kind":"record","id":ACTOR},
                "predicate":"has-account","value":"Authored value","classification":"author-fact",
                "valid":{"kind":"timeless"},"evidence":[],"audience":["author"],"perspectives":["author"]})),
            record("context-brief", BRIEF, json!({"domain":"test.brief","scope":"world","valid":{"kind":"timeless"},
                "required_refs":[],"audience":["author"],"perspectives":["author"]})),
            record("existence-episode", EPISODE, json!({"domain":"test.existence","character_id":ACTOR,"episode_kind":"unknown",
                "valid":{"kind":"interval","timeline":"main"},"start_evidence":[],"audience":["author"],"perspectives":["author"]})),
            record("hypothesis", HYPOTHESIS, json!({"status":"open","statement":"An authored possibility","subjects":[ACTOR],
                "alternatives":["One explicit alternative"],"context":"Author-local reasoning","placement":{"timeline":"main"}})),
        ]);
    }
    let world = source::world_from_records(
        PathBuf::from("/authoring-scaffold-never-opens-this-root"),
        records,
        Some("a".repeat(40)),
    )
    .unwrap();
    valid(&world);
    world
}

fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(
        report.valid,
        "{:?}",
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == "error")
            .collect::<Vec<_>>()
    );
}

fn request(world: &World, family: Family, entity: Option<&str>) -> ScaffoldRequest {
    ScaffoldRequest {
        expected_fingerprint: world.fingerprint.clone(),
        family,
        entity: entity.map(str::to_owned),
        id: None,
        title: None,
    }
}

fn result(world: &World, family: Family, entity: &str) -> Json {
    authoring_tools::scaffold(world, &request(world, family, Some(entity))).unwrap()
}

fn plan(value: &Json) -> consequences::Plan {
    consequences::decode(&value["plan"].to_string()).unwrap()
}

fn patch(value: &Json) -> Yaml {
    serde_yaml::from_str(value["plan"]["operations"][0]["patch"].as_str().unwrap()).unwrap()
}

fn source_image(world: &World) -> BTreeMap<String, Vec<u8>> {
    world
        .records
        .iter()
        .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
        .collect()
}

fn edit(world: &World, id: &str, modify: impl FnOnce(&mut Yaml)) -> World {
    let mut fields = world.records[id].frontmatter.clone();
    modify(&mut fields);
    let old = &world.records[id];
    let replacement = from_fields(&fields, &old.source_path, &old.body);
    let mut records = world.records.clone();
    records.insert(id.into(), replacement);
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}

#[test]
fn full_editable_patch_is_an_exact_no_op_without_synthesized_defaults() {
    let world = world(false);
    let before = source_image(&world);
    let value = result(&world, Family::Lore, ACTOR);
    let fields = patch(&value);
    assert_eq!(value["operation"], "authoring.scaffold");
    assert_eq!(value["fingerprint"], world.fingerprint);
    assert_eq!(value["revision"], world.revision.as_ref().unwrap().as_str());
    assert_eq!(value["sourceIndependent"], false);
    assert_eq!(value["writesPerformed"], false);
    assert_eq!(value["validationComplete"], false);
    assert_eq!(value["canonicalNoOp"], true);
    assert_eq!(value["previewValid"], true);
    assert_eq!(fields["body"].as_str(), Some(BODY));
    assert_eq!(fields["initial_state"]["counter"].as_i64(), Some(BIG));
    assert!(fields.get("tags").is_none() && fields.get("status").is_none());
    for field in ["id", "schema", "x-opaque"] {
        assert!(fields.get(field).is_none(), "{field}");
    }
    assert_eq!(
        value["retainedFields"],
        json!(["body", "id", "schema", "x-opaque"])
    );
    let original = &world.records[ACTOR];
    assert_eq!(
        value["retainedSource"].as_str().unwrap().as_bytes(),
        original.raw_bytes
    );
    let preview =
        consequences::preview(&world, &plan(&value), consequences::MAX_CHARACTERS).unwrap();
    assert_eq!(preview["canonicalNoOp"], true);
    assert_eq!(
        preview["files"][0]["afterSource"]
            .as_str()
            .unwrap()
            .as_bytes(),
        original.raw_bytes
    );
    assert_eq!(source_image(&world), before);
    assert_eq!(result(&world, Family::Lore, ACTOR), value);
}

#[test]
fn returned_plan_can_be_edited_through_existing_codecs_without_losing_retained_source() {
    let world = world(false);
    let before = &world.records[ACTOR];
    let value = result(&world, Family::Changeset, ACTOR);
    let mut fields = patch(&value);
    let body = "\r\nUpdated body 🌲\n  Exact trailing bytes\r";
    fields["body"] = Yaml::String(body.into());
    let mut edited = plan(&value);
    if let consequences::Operation::Update { patch, .. } = &mut edited.operations[0] {
        *patch = serde_yaml::to_string(&fields).unwrap();
    } else {
        panic!("expected update");
    }
    let preview = consequences::preview(&world, &edited, consequences::MAX_CHARACTERS).unwrap();
    assert_eq!(preview["validation"]["valid"], true);
    assert_eq!(preview["changedFiles"], 1);
    let after_source = preview["files"][0]["afterSource"].as_str().unwrap();
    let after = source::parse_record(after_source.as_bytes(), &before.source_path).unwrap();
    let header_bytes = before.raw_bytes.len() - before.body.len();
    assert_eq!(
        &after.raw_bytes[..header_bytes],
        &before.raw_bytes[..header_bytes]
    );
    assert_eq!(after.body, body);
    assert_eq!(after.frontmatter, before.frontmatter);

    fields["title"] = Yaml::String("Edited authored title".into());
    let after =
        record_edit::update(&world, ACTOR, &serde_yaml::to_string(&fields).unwrap()).unwrap();
    assert_eq!(after.title, "Edited authored title");
    assert_eq!(after.get("x-opaque"), before.get("x-opaque"));
    assert_eq!(after.get("body"), before.get("body"));
    assert!(after.get("status").is_none() && after.get("tags").is_none());
    assert_eq!(after.body, body);
}

#[test]
fn selection_requires_exact_ids_and_family_specific_request_shapes() {
    let world = world(false);
    let default =
        authoring_tools::scaffold(&world, &request(&world, Family::Changeset, None)).unwrap();
    assert_eq!(default["target"]["id"], WORLD);
    for entity in [
        "Captain",
        "unknown",
        "Title char_00000000000000000000000001",
    ] {
        assert!(matches!(
            authoring_tools::scaffold(&world, &request(&world, Family::Lore, Some(entity))),
            Err(Error::NotFound(_))
        ));
    }
    assert!(authoring_tools::scaffold(&world, &request(&world, Family::Lore, None)).is_err());
    for field in ["id", "title"] {
        let mut input = request(&world, Family::Lore, Some(ACTOR));
        if field == "id" {
            input.id = Some(ORGANIZATION.into());
        } else {
            input.title = Some("Unexpected".into());
        }
        assert!(authoring_tools::scaffold(&world, &input).is_err());
    }
}

#[test]
fn specialized_scaffolds_enforce_owner_kinds_without_mutating_any_source() {
    let world = world(true);
    let before = source_image(&world);
    for (family, good, bad) in [
        (Family::Briefs, BRIEF, ACTOR),
        (Family::Assertions, ASSERTION, PLACE),
        (Family::Descriptions, ACTOR, HYPOTHESIS),
        (Family::SpatialHistory, PLACE, ACTOR),
        (Family::Existence, EPISODE, ASSERTION),
        (Family::Governance, WORLD, ACTOR),
    ] {
        assert_eq!(result(&world, family, good)["canonicalNoOp"], true);
        let error =
            authoring_tools::scaffold(&world, &request(&world, family, Some(bad))).unwrap_err();
        assert!(
            error.to_string().contains("cannot edit an owner"),
            "{error}"
        );
    }
    for owner in [KNOWLEDGE, WORLD] {
        assert_eq!(
            result(&world, Family::Assertions, owner)["canonicalNoOp"],
            true
        );
    }
    assert_eq!(
        result(&world, Family::Governance, ASSERTION)["canonicalNoOp"],
        true
    );
    assert_eq!(
        result(&world, Family::Lore, HYPOTHESIS)["canonicalNoOp"],
        true
    );
    assert_eq!(source_image(&world), before);
}

#[test]
fn unavailable_families_do_not_enable_capabilities_or_choose_another_owner() {
    let world = world(false);
    let before = source_image(&world);
    for family in Family::ALL {
        if matches!(family, Family::Changeset | Family::Lore) {
            continue;
        }
        let error =
            authoring_tools::scaffold(&world, &request(&world, family, Some(WORLD))).unwrap_err();
        assert!(
            error.to_string().contains("require declared capabilities"),
            "{family:?}: {error}"
        );
    }
    assert_eq!(source_image(&world), before);
}

#[test]
fn member_guidance_preserves_omission_empty_arrays_and_selects_a_fresh_local_id() {
    let base = world(true);
    for (family, field, prefix) in [
        (Family::Descriptions, "descriptions", "description"),
        (Family::SpatialHistory, "spatial_history", "spatialhistory"),
    ] {
        let absent = result(&base, family, PLACE);
        assert_eq!(absent["memberField"], field);
        assert_eq!(absent["memberPresent"], false);
        assert!(absent.get("membersYaml").is_none());
        assert!(patch(&absent).get(field).is_none());
        assert_eq!(absent["nextMemberId"], format!("{prefix}_{:026}", 1));

        let empty = edit(&base, PLACE, |fields| {
            fields[field] = Yaml::Sequence(vec![]);
        });
        valid(&empty);
        let empty = result(&empty, family, PLACE);
        assert_eq!(empty["memberPresent"], true);
        assert_eq!(
            serde_yaml::from_str::<Yaml>(empty["membersYaml"].as_str().unwrap()).unwrap(),
            Yaml::Sequence(vec![])
        );
        assert_eq!(patch(&empty)[field], Yaml::Sequence(vec![]));

        let members = [1, 2, 4].into_iter().map(|number| {
            if family == Family::Descriptions {
                json!({"id":format!("{prefix}_{number:026}"),"heading":format!("Member {number}"),"markdown":"Exact member 🌲\r\n",
                    "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})
            } else {
                let point = json!({"timeline":"other","tick":BIG + number,"order":i32::MIN});
                json!({"id":format!("{prefix}_{number:026}"),"aspect":"parentage",
                    "valid":{"kind":"interval","timeline":"other","start":point,"end":point},"value":{"parent_id":null}})
            }
        }).collect::<Vec<_>>();
        let populated = edit(&base, PLACE, |fields| {
            fields[field] = serde_yaml::to_value(&members).unwrap();
        });
        valid(&populated);
        let value = result(&populated, family, PLACE);
        assert_eq!(value["nextMemberId"], format!("{prefix}_{:026}", 3));
        assert_eq!(
            serde_yaml::from_str::<Yaml>(value["membersYaml"].as_str().unwrap()).unwrap(),
            populated.records[PLACE].frontmatter[field]
        );
        assert_eq!(value["canonicalNoOp"], true);
        assert_eq!(
            patch(&value)[field],
            populated.records[PLACE].frontmatter[field]
        );
    }
}

#[test]
fn organization_starter_uses_explicit_identity_and_exact_native_current_time() {
    let world = world(true);
    let before = source_image(&world);
    let mut input = request(&world, Family::Generational, None);
    input.id = Some(ORGANIZATION.into());
    input.title = Some("An authored organization 🌲".into());
    let value = authoring_tools::scaffold(&world, &input).unwrap();
    assert_eq!(value["target"]["id"], ORGANIZATION);
    assert_eq!(value["target"]["kind"], "organization");
    assert_eq!(
        value["at"],
        json!({"timeline":"other","tick":BIG.to_string(),"order":i32::MIN.to_string()})
    );
    assert_eq!(value["canonicalNoOp"], false);
    assert!(value.get("retainedSource").is_none());
    let plan = plan(&value);
    let consequences::Operation::Create { draft } = &plan.operations[0] else {
        panic!("expected create");
    };
    let fields: Yaml = serde_yaml::from_str(draft).unwrap();
    assert_eq!(
        fields["initialization"]["applicability"]["point"]["tick"].as_i64(),
        Some(BIG)
    );
    assert_eq!(
        fields["initialization"]["applicability"]["point"]["order"].as_i64(),
        Some(i64::from(i32::MIN))
    );
    let created = record_edit::create(&world, draft).unwrap();
    assert_eq!(created.title, input.title.as_deref().unwrap());
    assert_eq!(created.string("organization_kind"), Some("house"));
    assert_eq!(
        created.get("initialization").unwrap()["payload"]["title"].as_str(),
        Some(created.title.as_str())
    );
    let preview = consequences::preview(&world, &plan, consequences::MAX_CHARACTERS).unwrap();
    assert_eq!(preview["validation"]["valid"], true);
    assert_eq!(preview["changedFiles"], 1);
    assert_eq!(source_image(&world), before);
    assert_eq!(authoring_tools::scaffold(&world, &input).unwrap(), value);

    for omission in ["id", "title"] {
        let mut bad = input.clone();
        if omission == "id" {
            bad.id = None;
        } else {
            bad.title = None;
        }
        assert!(authoring_tools::scaffold(&world, &bad).is_err());
    }
    let mut bad = input.clone();
    bad.entity = Some(ACTOR.into());
    assert!(authoring_tools::scaffold(&world, &bad).is_err());
    bad = input.clone();
    bad.id = Some(ACTOR.into());
    assert!(authoring_tools::scaffold(&world, &bad).is_err());

    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(created);
    let changed =
        source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap();
    input.expected_fingerprint = changed.fingerprint.clone();
    assert!(matches!(
        authoring_tools::scaffold(&changed, &input),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn valid_source_values_outside_the_edit_codec_are_refused_without_silent_omission() {
    let world = world(true);
    let organization = record(
        "organization",
        ORGANIZATION,
        json!({"organization_kind":"house","parent_id":null,
        "initialization":{"transition_id":"transition_00000000000000000000000009","transition_kind":"organization-initialize",
            "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":0,"order":0}},
            "payload":{"title":"Current title","aliases":[]}},"transitions":[]}),
    );
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(organization);
    let world =
        source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap();
    valid(&world);
    // The source accepts explicit null; a supplied typed patch does not. The
    // generic scaffold must reject it rather than quietly omit this field.
    assert!(record_edit::update(&world, ORGANIZATION, "kind: organization\n").is_ok());
    let error =
        authoring_tools::scaffold(&world, &request(&world, Family::Lore, Some(ORGANIZATION)))
            .unwrap_err();
    assert!(error.to_string().contains("constructed"), "{error}");
    assert_eq!(
        world.records[ORGANIZATION].get("parent_id"),
        Some(&Yaml::Null)
    );
}

#[test]
fn stale_pins_and_mutated_projections_are_refused_before_returning_a_plan() {
    let world = world(false);
    let input = request(&world, Family::Lore, Some(ACTOR));
    let changed = edit(&world, PLACE, |fields| {
        fields["title"] = Yaml::String("Changed neighbor".into());
    });
    assert!(matches!(
        authoring_tools::scaffold(&changed, &input),
        Err(Error::Conflict(_))
    ));
    let mut malformed = world.clone();
    malformed
        .records
        .get_mut(ACTOR)
        .unwrap()
        .body
        .push_str("forged");
    assert!(matches!(
        authoring_tools::scaffold(&malformed, &input),
        Err(Error::Validation(_))
    ));
    let mut malformed = world.clone();
    malformed
        .records
        .get_mut(PLACE)
        .unwrap()
        .raw_bytes
        .push(b'x');
    assert!(matches!(
        authoring_tools::scaffold(&malformed, &input),
        Err(Error::Validation(_))
    ));
    let mut malformed = world.clone();
    malformed.fingerprint = "0".repeat(64);
    let mut input = input;
    input.expected_fingerprint = malformed.fingerprint.clone();
    assert!(matches!(
        authoring_tools::scaffold(&malformed, &input),
        Err(Error::Validation(_))
    ));
}

#[test]
fn complete_plan_overflow_is_refused_instead_of_truncating_editable_source() {
    let world = world(false);
    let before = &world.records[ACTOR];
    let replacement = from_fields(
        &before.frontmatter,
        &before.source_path,
        &"x".repeat(consequences::MAX_PLAN_BYTES),
    );
    let mut records = world.records.clone();
    records.insert(ACTOR.into(), replacement);
    let world = source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap();
    valid(&world);
    let before = source_image(&world);
    let error =
        authoring_tools::scaffold(&world, &request(&world, Family::Lore, Some(ACTOR))).unwrap_err();
    assert!(error.to_string().contains("262144"), "{error}");
    assert_eq!(source_image(&world), before);
}
