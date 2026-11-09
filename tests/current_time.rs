//! Explicit shared cursor plans retain every unrelated authored value and use
//! the same complete candidate and publication rules as other consequences.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, path::PathBuf};
use wedl::{
    Error, authoring, consequences, current_time,
    model::{Record, StoryTime, World},
    record_edit, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PLACE: &str = "loc_00000000000000000000000001";
const ACTOR: &str = "char_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const CLOSED: &str = "scene_00000000000000000000000901";
const PLANNED: &str = "scene_00000000000000000000000902";
const ABANDONED: &str = "scene_00000000000000000000000903";
const CONVERSATION: &str = "conv_00000000000000000000000001";

fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}

fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,
        "title":format!("Exact {kind} 🌲"),"status":"canonical",
        "x-author":{"literal":null,"numbers":[1,1.0,"1"]}});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let header = format!(
        "---\n# Retained comment when unchanged.\n{}---\n",
        serde_yaml::to_string(&value).unwrap()
    )
    .replace('\n', "\r\n");
    source::parse_record(
        format!("{header}{body}").as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}

fn fixture(active_count: usize) -> World {
    let mut records = vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":[],"threads":[],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "state_keys":{},"current_time":point(10,0)}),
            "Exact world prose  \r\n",
        ),
        record("location", PLACE, json!({}), "Exact location prose.\r\n"),
        record(
            "character",
            ACTOR,
            json!({"initial_state":{}}),
            "Exact character prose.\r\n",
        ),
        record(
            "object",
            OBJECT,
            json!({"initial_state":{},"object_affordances":["can-be-opened","can-be-opened"]}),
            "Literal object prose.",
        ),
        record(
            "event",
            EVENT,
            json!({"time":point(7,0),"effects":[]}),
            "Existing event prose.",
        ),
    ];
    for index in 0..active_count {
        records.push(record("scene",&format!("scene_{:026}",index+1),
            json!({"status":"active","location":PLACE,"time":{"start":point(0,0),"current":point(10,0),"end":point(20,0)},
                "participants":[],"objects":[OBJECT],"environments":[],"observations":[]}),
            "Exact active scene prose.\r\nNo final newline  "));
    }
    for (id, status, start, current, end) in [
        (CLOSED, "closed", 0, 5, 6),
        (PLANNED, "planned", 15, 15, 30),
        (ABANDONED, "abandoned", 0, 4, 8),
    ] {
        records.push(record("scene",id,json!({"status":status,"location":PLACE,
            "time":{"start":point(start,0),"current":point(current,0),"end":point(end,0)},"participants":[]}),"Unchanged non-active prose.\r\n"));
    }
    records.push(record(
        "conversation",
        CONVERSATION,
        json!({"status":"active","location":PLACE,
        "time":{"start":point(2,0),"end":point(30,0)},"participants":[],"turns":[]}),
        "Unchanged conversation.\r\n",
    ));
    let world = source::world_from_records(
        PathBuf::from("/current-time-does-not-access-this-directory"),
        records,
        None,
    )
    .unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn request(world: &World, tick: i64, order: i32) -> current_time::Request {
    current_time::Request {
        expected_fingerprint: world.fingerprint.clone(),
        at: StoryTime::new("main", tick, order).unwrap(),
    }
}
fn preview(world: &World, input: &current_time::Request) -> Json {
    current_time::preview(world, input, current_time::MAX_CHARACTERS).unwrap()
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
fn changed(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let mut original = world.records[id].clone();
    edit(&mut original.frontmatter);
    let source = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&original.frontmatter).unwrap(),
        original.body
    );
    let replacement = source::parse_record(source.as_bytes(), &original.source_path).unwrap();
    let mut records = world.records.clone();
    records.insert(id.into(), replacement);
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn disk(world: World) -> (tempfile::TempDir, World) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("world");
    for record in world.records.values() {
        let path = root.join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &record.raw_bytes).unwrap();
    }
    let world = source::load_world(&root, None).unwrap();
    (directory, world)
}
fn source_image(world: &World) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    world
        .records
        .values()
        .map(|record| {
            let path = world.root.join(&record.source_path);
            (
                record.source_path.clone(),
                (
                    fs::read(&path).unwrap(),
                    fs::metadata(path).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

#[test]
fn closed_request_rejects_duplicates_unknown_fields_missing_values_and_coercions() {
    let world = fixture(0);
    let value = serde_json::to_value(request(&world, 10, 0)).unwrap();
    assert!(current_time::decode(&value.to_string()).is_ok());
    for key in ["expectedFingerprint", "at"] {
        let mut bad = value.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(current_time::decode(&bad.to_string()).is_err());
    }
    for (field, patch) in [
        ("expectedFingerprint", json!([world.fingerprint])),
        ("expectedFingerprint", json!("A".repeat(64))),
        ("extra", json!(true)),
        ("at", json!(null)),
    ] {
        let mut bad = value.clone();
        bad[field] = patch;
        assert!(current_time::decode(&bad.to_string()).is_err());
    }
    for patch in [
        json!({"tick":10}),
        json!({"order":0}),
        json!({"tick":true}),
        json!({"timeline":null}),
        json!({"timeline":" "}),
        json!({"extra":"ignored"}),
    ] {
        let mut bad = value.clone();
        bad["at"]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(current_time::decode(&bad.to_string()).is_err(), "{bad}");
    }
    for key in ["timeline", "tick", "order"] {
        let mut bad = value.clone();
        bad["at"].as_object_mut().unwrap().remove(key);
        assert!(current_time::decode(&bad.to_string()).is_err());
    }
    let duplicated = value
        .to_string()
        .replacen("\"at\":", "\"at\":{},\"at\":", 1);
    assert!(current_time::decode(&duplicated).is_err());
    let escaped = value
        .to_string()
        .replacen("\"tick\":", "\"t\\u0069ck\":\"7\",\"tick\":", 1);
    assert!(current_time::decode(&escaped).is_err());
    assert!(current_time::decode(&" ".repeat(current_time::MAX_REQUEST_BYTES + 1)).is_err());
    assert!(current_time::decode(&(value.to_string() + " true")).is_err());
}

#[test]
fn coordinates_are_exact_signed_decimals_and_native_request_checks_match_wire_rules() {
    let world = fixture(0);
    let mut value = serde_json::to_value(request(&world, i64::MIN, i32::MAX)).unwrap();
    assert_eq!(
        current_time::decode(&value.to_string()).unwrap().at.tick,
        i64::MIN
    );
    assert_eq!(
        current_time::decode(&value.to_string()).unwrap().at.order,
        i32::MAX
    );
    for (field, spelling) in [
        ("tick", "+1"),
        ("tick", "01"),
        ("tick", "-0"),
        ("tick", "1.0"),
        ("tick", " 1"),
        ("tick", "9223372036854775808"),
        ("order", "2147483648"),
        ("order", "-2147483649"),
    ] {
        let mut bad = value.clone();
        bad["at"][field] = spelling.into();
        assert!(
            current_time::decode(&bad.to_string()).is_err(),
            "{spelling}"
        );
    }
    value["at"]["timeline"] = "🌲".repeat(33).into();
    assert!(current_time::decode(&value.to_string()).is_err());
    let mut bad = request(&world, 0, 0);
    bad.at.timeline.clear();
    assert!(current_time::plan(&world, &bad).is_err());
    bad = request(&world, 0, 0);
    bad.expected_fingerprint = "f".repeat(63);
    assert!(current_time::plan(&world, &bad).is_err());
}

#[test]
fn exact_large_clock_plan_uses_numeric_source_and_string_json_without_accessing_a_repository() {
    let world = fixture(0);
    let input = request(&world, 9_007_199_254_740_993, i32::MIN);
    let result = preview(&world, &input);
    assert_eq!(result["request"]["at"]["tick"], "9007199254740993");
    assert_eq!(result["request"]["at"]["order"], i32::MIN.to_string());
    let proposed = candidate(&world, &result);
    assert_eq!(proposed.current_time().unwrap(), input.at);
    assert_eq!(
        proposed.records[WORLD].frontmatter["current_time"]["tick"].as_i64(),
        Some(9_007_199_254_740_993)
    );
    assert_eq!(
        world.current_time().unwrap(),
        StoryTime::new("main", 10, 0).unwrap()
    );
    assert_eq!(result["activeSceneIds"], json!([]));
    assert!(result["preview"]["validation"]["valid"].as_bool().unwrap());
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["canApply"], false);
    assert_eq!(result["validationComplete"], false);
}

#[test]
fn every_active_scene_changes_together_while_all_other_source_and_scene_content_is_retained() {
    let (_directory, world) = disk(fixture(2));
    let before = source_image(&world);
    let input = request(&world, 12, 3);
    let result = preview(&world, &input);
    let ids: Vec<_> = (1..=2).map(|i| format!("scene_{i:026}")).collect();
    assert_eq!(result["activeSceneIds"], json!(ids));
    let plan: consequences::Plan = serde_json::from_value(result["plan"].clone()).unwrap();
    assert_eq!(plan.operations.len(), 3);
    assert_eq!(
        result["preview"],
        consequences::preview(&world, &plan, current_time::MAX_CHARACTERS).unwrap()
    );
    assert_eq!(result["preview"]["changedFiles"], 3);
    assert_eq!(result["preview"]["validation"]["valid"], true);
    let proposed = candidate(&world, &result);
    assert_eq!(proposed.records.len(), world.records.len());
    for (id, old) in &world.records {
        let new = &proposed.records[id];
        assert_eq!(old.body, new.body);
        if id == WORLD {
            let mut fields = new.frontmatter.clone();
            fields["current_time"] = old.frontmatter["current_time"].clone();
            assert_eq!(fields, old.frontmatter);
        } else if ids.contains(id) {
            assert_eq!(
                StoryTime::from_yaml(&new.frontmatter["time"]["current"], "main").unwrap(),
                input.at
            );
            let mut fields = new.frontmatter.clone();
            fields["time"]["current"] = old.frontmatter["time"]["current"].clone();
            assert_eq!(fields, old.frontmatter);
        } else {
            assert_eq!(new.raw_bytes, old.raw_bytes, "unexpected mutation of {id}");
        }
    }
    assert_eq!(source_image(&world), before);
}

#[test]
fn explicit_backward_and_cross_timeline_sets_depend_on_complete_candidate_invariants() {
    let world = fixture(2);
    let backwards = preview(&world, &request(&world, 5, 0));
    assert_eq!(backwards["preview"]["validation"]["valid"], true);
    for (tick, order) in [(-1, 0), (20, 1), (21, 0)] {
        let result = preview(&world, &request(&world, tick, order));
        assert_eq!(result["preview"]["validation"]["valid"], false, "{result}");
        assert_eq!(result["canApply"], false);
        assert_eq!(result["preview"]["files"].as_array().unwrap().len(), 3);
    }
    let mut crossing = request(&world, 10, 0);
    crossing.at.timeline = "other".into();
    assert_eq!(
        preview(&world, &crossing)["preview"]["validation"]["valid"],
        false
    );
    let world = fixture(0);
    crossing.expected_fingerprint = world.fingerprint.clone();
    assert_eq!(
        preview(&world, &crossing)["preview"]["validation"]["valid"],
        true
    );
    assert_eq!(
        preview(&world, &request(&world, i64::MIN, i32::MIN))["preview"]["validation"]["valid"],
        true
    );
}

#[test]
fn same_effective_point_preserves_original_bytes_including_null_end_and_short_source_coordinates() {
    let scene = "scene_00000000000000000000000001";
    let world = fixture(1);
    let world = changed(&world, WORLD, |fields| {
        fields["current_time"] = serde_yaml::from_str("{tick: 10}").unwrap()
    });
    let world = changed(&world, scene, |fields| {
        fields["time"] =
            serde_yaml::from_str("{start: {tick: 0}, current: {tick: 10}, end: null}").unwrap()
    });
    assert!(validation::validate_world(&world).valid);
    let result = preview(&world, &request(&world, 10, 0));
    assert_eq!(result["preview"]["canonicalNoOp"], true);
    assert_eq!(result["preview"]["changedFiles"], 0);
    for file in result["preview"]["files"].as_array().unwrap() {
        assert_eq!(file["beforeSource"], file["afterSource"]);
    }
    let proposed = candidate(&world, &result);
    assert_eq!(proposed.fingerprint, world.fingerprint);
    assert!(proposed.records[scene].frontmatter["time"]["end"].is_null());
}

#[test]
fn changing_an_open_scene_spells_absent_end_using_the_existing_typed_grammar() {
    let scene = "scene_00000000000000000000000001";
    let world = fixture(1);
    let world = changed(&world, scene, |fields| fields["time"]["end"] = Yaml::Null);
    let result = preview(&world, &request(&world, 30, 0));
    assert_eq!(result["preview"]["validation"]["valid"], true);
    let proposed = candidate(&world, &result);
    assert!(
        proposed.records[scene].frontmatter["time"]
            .get("end")
            .is_none()
    );
    assert_eq!(
        proposed.records[scene].frontmatter["time"]["start"],
        world.records[scene].frontmatter["time"]["start"]
    );
    assert_eq!(proposed.records[scene].body, world.records[scene].body);
}

#[test]
fn missing_explicit_world_cursor_is_created_without_changing_its_single_active_scene() {
    let world = fixture(1);
    let world = changed(&world, WORLD, |fields| {
        fields.as_mapping_mut().unwrap().remove("current_time");
    });
    assert!(validation::validate_world(&world).valid);
    let result = preview(&world, &request(&world, 10, 0));
    assert_eq!(result["preview"]["changedFiles"], 1);
    let proposed = candidate(&world, &result);
    assert_eq!(
        proposed.current_time().unwrap(),
        world.current_time().unwrap()
    );
    assert!(proposed.records[WORLD].get("current_time").is_some());
    assert_eq!(
        proposed.records["scene_00000000000000000000000001"].raw_bytes,
        world.records["scene_00000000000000000000000001"].raw_bytes
    );
}

#[test]
fn stale_forged_invalid_or_unknown_timeline_inputs_do_not_produce_a_plan() {
    let world = fixture(1);
    let mut input = request(&world, 11, 0);
    input.expected_fingerprint = "f".repeat(64);
    assert!(matches!(
        current_time::plan(&world, &input),
        Err(Error::Conflict(_))
    ));
    let mut forged = world.clone();
    forged.records.get_mut(WORLD).unwrap().frontmatter["title"] = "Forged".into();
    assert!(matches!(
        current_time::plan(&forged, &request(&world, 11, 0)),
        Err(Error::Validation(_))
    ));
    let invalid = changed(&world, WORLD, |fields| {
        fields["current_time"] = serde_yaml::to_value(point(9, 0)).unwrap()
    });
    assert!(matches!(
        current_time::plan(&invalid, &request(&invalid, 11, 0)),
        Err(Error::Validation(_))
    ));
    let mut unknown = request(&world, 11, 0);
    unknown.at.timeline = "missing".into();
    assert!(matches!(
        current_time::plan(&world, &unknown),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn complete_operation_limit_refuses_instead_of_omitting_active_scenes() {
    let world = fixture(63);
    let plan = current_time::plan(&world, &request(&world, 11, 0)).unwrap();
    assert_eq!(plan.operations.len(), 64);
    let world = fixture(64);
    assert!(current_time::plan(&world, &request(&world, 11, 0)).is_err());
    assert!(
        current_time::preview(
            &world,
            &request(&world, 11, 0),
            current_time::MAX_CHARACTERS
        )
        .is_err()
    );
}

#[test]
fn complete_output_budget_includes_exported_plan_metadata_and_unicode_scalars() {
    let world = fixture(1);
    let input = request(&world, 12, 0);
    let broad = preview(&world, &input);
    let generous = serde_json::to_string(&broad).unwrap().chars().count();
    let bounded = current_time::preview(&world, &input, generous).unwrap();
    let exact = serde_json::to_string(&bounded).unwrap().chars().count();
    let answer = current_time::preview(&world, &input, exact).unwrap();
    assert_eq!(
        serde_json::to_string(&answer).unwrap().chars().count(),
        exact
    );
    assert!(current_time::preview(&world, &input, exact - 1).is_err());
    let bytes = serde_json::to_vec(&answer).unwrap();
    assert!(bytes.len() > exact);
    for invalid in [0, 255, 1_000_001] {
        assert!(current_time::preview(&world, &input, invalid).is_err());
    }
}

#[test]
fn original_historical_snapshot_can_be_previewed_without_substituting_live_source() {
    let mut world = fixture(1);
    world.revision = Some("a".repeat(40));
    let result = preview(&world, &request(&world, 12, 0));
    assert_eq!(result["preview"]["baseRevision"], "a".repeat(40));
    assert_eq!(result["preview"]["baseFingerprint"], world.fingerprint);
    assert_eq!(result["canApply"], false);
}

#[test]
fn typed_world_current_time_requires_complete_numeric_source_clock_and_uses_existing_writer() {
    let directory = tempfile::tempdir().unwrap();
    let world = source::init_world(&directory.path().join("world"), "Clock writer").unwrap();
    let owner = world.world_record().unwrap();
    for bad in [
        "null",
        "{tick: 12}",
        "{timeline: main, tick: 12}",
        "{timeline: main, tick: '12', order: 0}",
        "{timeline: main, tick: true, order: 0}",
        "{timeline: main, tick: 12, order: 0, extra: true}",
        "{timeline: main, tick: 12, order: 2147483648}",
    ] {
        assert!(
            record_edit::update(
                &world,
                &owner.id,
                &format!("kind: world\ncurrent_time: {bad}\n")
            )
            .is_err(),
            "{bad}"
        );
    }
    let patch = "kind: world\ncurrent_time: {timeline: main, tick: 9007199254740993, order: -2}\n";
    let result =
        authoring::preview_record_update(&world, &owner.id, patch, &world.fingerprint).unwrap();
    assert_eq!(result["canApply"], true);
    authoring::apply_record_update(
        &world,
        &owner.id,
        patch,
        &world.fingerprint,
        result["afterSha256"].as_str().unwrap(),
    )
    .unwrap();
    let changed = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        changed.current_time().unwrap(),
        StoryTime::new("main", 9_007_199_254_740_993, -2).unwrap()
    );
    let before = &owner.frontmatter;
    let mut after = changed.world_record().unwrap().frontmatter.clone();
    after["current_time"] = before["current_time"].clone();
    assert_eq!(&after, before);
    assert_eq!(changed.world_record().unwrap().body, owner.body);
}

#[test]
fn uncoordinated_world_only_update_cannot_publish_a_mismatched_active_scene_cursor() {
    let (_directory, world) = disk(fixture(2));
    let before = source_image(&world);
    let patch = "kind: world\ncurrent_time: {timeline: main, tick: 12, order: 0}\n";
    let result =
        authoring::preview_record_update(&world, WORLD, patch, &world.fingerprint).unwrap();
    assert_eq!(result["canApply"], false);
    assert!(matches!(
        authoring::apply_record_update(
            &world,
            WORLD,
            patch,
            &world.fingerprint,
            result["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Validation(_))
    ));
    assert_eq!(source_image(&world), before);
    let coordinated = preview(&world, &request(&world, 12, 0));
    assert_eq!(coordinated["preview"]["validation"]["valid"], true);
}
