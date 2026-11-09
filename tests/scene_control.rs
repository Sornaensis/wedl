//! Coordinated intent is checked as one candidate, with exact authored evidence.
#[path = "support/scene_control.rs"]
mod support;

use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeSet;
use support::*;
use wedl::{Error, consequences, model::World, runtime, scene, scene_control, source, validation};

const NEW_SCENE: &str = "scene_00000000000000000000000003";
const EVENT: &str = "event_00000000000000000000000001";
const PRIOR_EVENT: &str = "event_00000000000000000000000002";
const THIRD_ACTOR: &str = "char_00000000000000000000000003";

fn input(world: &World, order: i32, action: Json) -> scene_control::Request {
    scene_control::decode(
        &json!({"expectedFingerprint":world.fingerprint,
        "at":at(order),"action":action})
        .to_string(),
    )
    .unwrap()
}

fn event() -> Json {
    json!({"id":EVENT,"title":"An explicit arrival 🌲"})
}

fn create(world: &World, order: i32) -> scene_control::Request {
    input(
        world,
        order,
        json!({"type":"scene.create","sceneId":NEW_SCENE,
        "title":"A new scene","locationId":DESTINATION,"characterIds":[ACTOR],"event":event()}),
    )
}

fn move_to_scene(world: &World, order: i32) -> scene_control::Request {
    input(
        world,
        order,
        json!({"type":"character.move","characterIds":[ACTOR],
        "locationId":DESTINATION,"sceneId":OTHER_SCENE,"event":event()}),
    )
}

fn advance(world: &World, order: i32) -> scene_control::Request {
    input(
        world,
        order,
        json!({"type":"scene.advance","sceneId":SCENE}),
    )
}

fn preview(world: &World, request: &scene_control::Request) -> Json {
    scene_control::preview(world, request, scene_control::MAX_CHARACTERS).unwrap()
}

fn added(world: &World, kind: &str, id: &str, fields: Json) -> World {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,
        "title":format!("Explicit {kind}"),"status":"canonical"});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nUnrelated authored prose.\n",
        serde_yaml::to_string(&data).unwrap()
    );
    let record = source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap();
    let mut records = world.records.clone();
    assert!(records.insert(id.into(), record).is_none());
    let result =
        source::world_from_records(world.root.clone(), records.into_values().collect(), None)
            .unwrap();
    let validation = validation::validate_world(&result);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    result
}

fn prior_event(
    world: &World,
    order: i32,
    location: Option<&str>,
    participants: &[&str],
    effects: Json,
) -> World {
    let mut fields = json!({"time":point(TICK,order),
        "participants":participants.iter().map(|id| json!({"character":id})).collect::<Vec<_>>(),
        "effects":effects});
    if let Some(location) = location {
        fields["location"] = location.into();
    }
    added(world, "event", PRIOR_EVENT, fields)
}

fn assert_preserved(before: &World, after: &World, ids: &[&str]) {
    for id in ids {
        assert_eq!(
            before.records[*id].raw_bytes, after.records[*id].raw_bytes,
            "{id}"
        );
    }
}

#[test]
fn create_detaches_prospective_presence_and_transfers_only_listed_carried_inventory() {
    let (_directory, world) = fixture();
    let image = source_image(&world);
    // The movement is beyond the old cursor: native read presence at its
    // predecessor is false, but the planner must extend authored attendance.
    assert!(!scene::present(&world, &world.records[SCENE], ACTOR, &at(ORDER + 3)).unwrap());
    let request = create(&world, ORDER + 4);
    let report = preview(&world, &request);
    let proposed = candidate(&world, &report);
    assert_eq!(proposed.current_time().unwrap(), request.at);
    for id in [SCENE, OTHER_SCENE, NEW_SCENE] {
        assert_eq!(
            proposed.records[id].frontmatter["time"]["current"],
            serde_yaml::to_value(point(TICK, ORDER + 4)).unwrap()
        );
    }
    let old = &proposed.records[SCENE];
    assert_eq!(
        old.frontmatter["participants"][0]["until"],
        serde_yaml::to_value(point(TICK, ORDER + 3)).unwrap()
    );
    assert_eq!(old.frontmatter["participants"][0]["role"], "witness");
    assert_eq!(old.frontmatter["participants"][0]["point_of_view"], true);
    assert_eq!(
        old.frontmatter["objects"],
        serde_yaml::to_value(json!([PROP])).unwrap()
    );
    assert_eq!(
        proposed.records[NEW_SCENE].frontmatter["objects"],
        serde_yaml::to_value(json!([BAG, CONTAINED])).unwrap()
    );
    assert!(scene::present(&proposed, old, ACTOR, &at(ORDER + 3)).unwrap());
    assert!(!scene::present(&proposed, old, ACTOR, &request.at).unwrap());
    assert!(scene::present(&proposed, &proposed.records[NEW_SCENE], ACTOR, &request.at).unwrap());
    let effects = proposed.records[EVENT].list("effects").unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0]["target"], ACTOR);
    assert_eq!(effects[0]["key"], "location");
    assert_eq!(effects[0]["value"]["entity"], DESTINATION);
    assert_eq!(
        runtime::resolved_state(&proposed, ACTOR, &request.at).unwrap()["location"]["entity"],
        DESTINATION
    );
    assert_eq!(proposed.records.len(), world.records.len() + 2);
    for id in [SCENE, OTHER_SCENE] {
        assert_eq!(proposed.records[id].body, world.records[id].body);
        assert_eq!(
            proposed.records[id].get("x-author"),
            world.records[id].get("x-author")
        );
        assert_eq!(
            proposed.records[id].get("outcome_events"),
            world.records[id].get("outcome_events")
        );
    }
    assert_preserved(
        &world,
        &proposed,
        &[
            ACTOR,
            OTHER_ACTOR,
            BAG,
            CONTAINED,
            PROP,
            ORIGIN,
            DESTINATION,
        ],
    );
    let plan = consequences::decode(&report["plan"].to_string()).unwrap();
    assert_eq!(scene_control::plan(&world, &request).unwrap(), plan);
    assert_eq!(
        consequences::preview(&world, &plan, scene_control::MAX_CHARACTERS).unwrap(),
        report["preview"]
    );
    let writes: Vec<_> = report["preview"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["entityId"].as_str().unwrap())
        .collect();
    assert_eq!(
        writes.len(),
        writes.iter().copied().collect::<BTreeSet<_>>().len()
    );
    assert_eq!(source_image(&world), image);
}

#[test]
fn moving_into_a_scene_appends_once_and_keeps_unlisted_inventory_untouched() {
    let (_directory, world) = fixture();
    let world = changed(&world, SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([BAG, PROP])).unwrap()
    });
    let report = preview(&world, &move_to_scene(&world, ORDER + 1));
    let proposed = candidate(&world, &report);
    assert_eq!(
        proposed.records[OTHER_SCENE].frontmatter["participants"][0],
        world.records[OTHER_SCENE].frontmatter["participants"][0]
    );
    assert_eq!(
        proposed.records[OTHER_SCENE].frontmatter["participants"][1]["character"],
        ACTOR
    );
    assert_eq!(
        proposed.records[OTHER_SCENE].frontmatter["objects"],
        serde_yaml::to_value(json!([BAG])).unwrap()
    );
    assert_preserved(
        &world,
        &proposed,
        &[BAG, CONTAINED, PROP, ACTOR, OTHER_ACTOR],
    );
    assert_eq!(proposed.records.len(), world.records.len() + 1);
}

#[test]
fn movement_classifies_an_intervening_drop_before_the_new_beat() {
    let (_directory, world) = fixture();
    let world = prior_event(
        &world,
        ORDER + 2,
        None,
        &[],
        json!([
            {"id":"effect_00000000000000000000000001","target":BAG,"key":"holder","operation":"clear"},
            {"id":"effect_00000000000000000000000002","target":BAG,"key":"location","operation":"set","value":{"entity":ORIGIN}}
        ]),
    );
    let proposed = candidate(&world, &preview(&world, &create(&world, ORDER + 4)));
    assert!(
        proposed.records[NEW_SCENE]
            .list("objects")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        proposed.records[SCENE].get("objects"),
        world.records[SCENE].get("objects")
    );
    assert_preserved(&world, &proposed, &[BAG, CONTAINED, PROP, PRIOR_EVENT]);
}

#[test]
fn scene_relocation_blocks_stationary_props_even_when_location_is_unchanged() {
    let (_directory, world) = fixture();
    for destination in [ORIGIN, DESTINATION] {
        let request = input(
            &world,
            ORDER + 2,
            json!({"type":"scene.advance","sceneId":SCENE,
            "locationId":destination,"event":event()}),
        );
        let error = scene_control::plan(&world, &request)
            .unwrap_err()
            .to_string();
        assert!(error.contains("stationary"), "{error}");
        assert!(error.contains(PROP), "{error}");
    }
    let world = changed(&world, SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([BAG, CONTAINED])).unwrap()
    });
    let request = input(
        &world,
        ORDER + 2,
        json!({"type":"scene.advance","sceneId":SCENE,
        "locationId":DESTINATION,"event":event()}),
    );
    let proposed = candidate(&world, &preview(&world, &request));
    assert_eq!(
        proposed.records[SCENE].string("location"),
        Some(DESTINATION)
    );
    assert_eq!(
        proposed.records[SCENE].get("objects"),
        world.records[SCENE].get("objects")
    );
    assert_eq!(
        proposed.records[SCENE].get("participants"),
        world.records[SCENE].get("participants")
    );
    assert_preserved(&world, &proposed, &[ACTOR, BAG, CONTAINED, PROP]);
}

#[test]
fn relocation_uses_requested_inclusive_attendance_instead_of_old_cursor_cast() {
    let (_directory, world) = fixture();
    let world = added(
        &world,
        "character",
        THIRD_ACTOR,
        json!({"initial_state":{"location":{"entity":ORIGIN}}}),
    );
    let world = changed(&world, SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([])).unwrap();
        fields["time"]["end"] = serde_yaml::to_value(point(TICK, ORDER + 10)).unwrap();
        fields["participants"].as_sequence_mut().unwrap().push(
            serde_yaml::to_value(json!({
            "character":THIRD_ACTOR,"until":point(TICK, ORDER + 2)}))
            .unwrap(),
        );
    });
    assert!(validation::validate_world(&world).valid);
    for (order, count) in [(ORDER + 2, 2), (ORDER + 3, 1)] {
        let request = input(
            &world,
            order,
            json!({"type":"scene.advance","sceneId":SCENE,
            "locationId":DESTINATION,"event":event()}),
        );
        let proposed = candidate(&world, &preview(&world, &request));
        assert_eq!(
            proposed.records[EVENT].list("effects").unwrap().len(),
            count
        );
        assert_eq!(
            proposed.records[SCENE].get("participants"),
            world.records[SCENE].get("participants")
        );
    }
}

#[test]
fn empty_scene_relocation_needs_no_event_but_cannot_discard_a_supplied_event() {
    let (_directory, world) = fixture();
    let world = changed(&world, SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([])).unwrap();
        fields["participants"][0]["until"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap();
    });
    let mut action = json!({"type":"scene.advance","sceneId":SCENE,"locationId":DESTINATION});
    let proposed = candidate(
        &world,
        &preview(&world, &input(&world, ORDER + 1, action.clone())),
    );
    assert_eq!(proposed.records.len(), world.records.len());
    assert_eq!(
        proposed.records[SCENE].string("location"),
        Some(DESTINATION)
    );
    action["event"] = event();
    assert!(scene_control::plan(&world, &input(&world, ORDER + 1, action)).is_err());
}

#[test]
fn close_changes_only_the_selected_scene_and_retains_explicit_member_ends() {
    let (_directory, world) = fixture();
    let world = changed(&world, SCENE, |fields| {
        fields["participants"][0]["until"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap()
    });
    let request = input(
        &world,
        ORDER + 5,
        json!({"type":"scene.close","sceneId":SCENE}),
    );
    let report = preview(&world, &request);
    let proposed = candidate(&world, &report);
    assert_eq!(report["preview"]["changedFiles"], 1);
    assert_eq!(proposed.records[SCENE].status(), "closed");
    assert_eq!(
        proposed.records[SCENE].frontmatter["time"]["end"],
        serde_yaml::to_value(point(TICK, ORDER + 5)).unwrap()
    );
    assert_eq!(
        proposed.records[SCENE].get("participants"),
        world.records[SCENE].get("participants")
    );
    assert_eq!(proposed.current_time().unwrap(), horizon());
    for (id, record) in &world.records {
        if id != SCENE {
            assert_eq!(record.raw_bytes, proposed.records[id].raw_bytes);
        }
    }
    assert_eq!(world.records[SCENE].body, proposed.records[SCENE].body);
    assert_eq!(world.records.len(), proposed.records.len());
}

#[test]
fn event_only_movement_never_detaches_or_advances_and_future_conflicts_are_reconciled_later() {
    let (_directory, world) = fixture();
    let action = json!({"type":"character.move","characterIds":[ACTOR],"locationId":DESTINATION,"event":event()});
    let request = input(&world, ORDER + 2, action.clone());
    let proposed = candidate(&world, &preview(&world, &request));
    for (id, record) in &world.records {
        assert_eq!(record.raw_bytes, proposed.records[id].raw_bytes);
    }
    assert_eq!(proposed.current_time().unwrap(), horizon());
    // The generic native validator intentionally retains its existing scope.
    assert!(validation::validate_world(&proposed).valid);
    assert!(matches!(
        scene_control::plan(&proposed, &advance(&proposed, ORDER + 2)),
        Err(Error::Validation(_))
    ));
    assert!(matches!(
        scene_control::plan(&world, &input(&world, ORDER, action)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn coordinated_movement_can_repair_an_existing_explicit_location_change() {
    let (_directory, world) = fixture();
    let world = prior_event(
        &world,
        ORDER,
        Some(DESTINATION),
        &[ACTOR],
        json!([
            {"id":"effect_00000000000000000000000001","target":ACTOR,"key":"location","operation":"set","value":{"entity":DESTINATION}}
        ]),
    );
    assert!(matches!(
        scene_control::plan(&world, &advance(&world, ORDER)),
        Err(Error::Validation(_))
    ));
    let proposed = candidate(&world, &preview(&world, &move_to_scene(&world, ORDER + 1)));
    assert_eq!(
        proposed.records[OTHER_SCENE].frontmatter["participants"][1]["character"],
        ACTOR
    );
    assert_preserved(&world, &proposed, &[PRIOR_EVENT]);
}

#[test]
fn active_scene_cursor_guards_and_real_endpoints_are_never_silently_rewritten() {
    let (_directory, world) = fixture();
    for mut request in [
        create(&world, ORDER - 1),
        move_to_scene(&world, ORDER - 1),
        advance(&world, ORDER - 1),
    ] {
        assert!(scene_control::plan(&world, &request).is_err());
        request.at = wedl::model::StoryTime::new("other", TICK, ORDER + 1).unwrap();
        assert!(scene_control::plan(&world, &request).is_err());
    }
    for status in ["planned", "closed", "abandoned"] {
        let world = changed(&world, SCENE, |fields| {
            fields["status"] = status.into();
            if status == "closed" {
                fields["time"]["end"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap();
            }
        });
        assert!(validation::validate_world(&world).valid);
        assert!(scene_control::plan(&world, &advance(&world, ORDER + 1)).is_err());
    }
    let bounded = changed(&world, SCENE, |fields| {
        fields["time"]["end"] = serde_yaml::to_value(point(TICK, ORDER + 1)).unwrap()
    });
    assert!(matches!(
        scene_control::plan(&bounded, &advance(&bounded, ORDER + 2)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn minimum_order_handoffs_refuse_without_borrowing_a_tick() {
    let (_directory, world) = fixture();
    for mut request in [create(&world, ORDER), move_to_scene(&world, ORDER)] {
        request.at.tick += 1;
        request.at.order = i32::MIN;
        let error = scene_control::plan(&world, &request)
            .unwrap_err()
            .to_string();
        assert!(error.contains("minimum same-tick order"), "{error}");
    }
    let mut request = input(
        &world,
        ORDER,
        json!({"type":"character.move","characterIds":[ACTOR],"locationId":DESTINATION,"event":event()}),
    );
    request.at.tick += 1;
    request.at.order = i32::MIN;
    let proposed = candidate(&world, &preview(&world, &request));
    assert_eq!(proposed.current_time().unwrap(), horizon());
    assert_eq!(
        proposed.records[EVENT].frontmatter["time"]["order"],
        i64::from(i32::MIN)
    );
}

#[test]
fn first_scene_has_no_synthetic_zero_or_default_timeline_cursor() {
    let (_directory, world) = fixture();
    let world = changed(&world, WORLD, |fields| {
        fields.as_mapping_mut().unwrap().remove("current_time");
    });
    let world = source::world_from_records(
        world.root.clone(),
        world
            .records
            .values()
            .filter(|record| record.kind != "scene")
            .cloned()
            .collect(),
        None,
    )
    .unwrap();
    assert!(validation::validate_world(&world).valid);
    for (timeline, tick) in [("main", -10), ("other", 11)] {
        let mut request = create(&world, 0);
        request.at = wedl::model::StoryTime::new(timeline, tick, 0).unwrap();
        let report = preview(&world, &request);
        assert!(report["previousTime"].is_null());
        let proposed = candidate(&world, &report);
        assert_eq!(proposed.current_time().unwrap(), request.at);
        assert_eq!(
            proposed.records[NEW_SCENE].frontmatter["time"]["start"]["tick"],
            tick
        );
        assert_eq!(
            proposed.records[NEW_SCENE].frontmatter["time"]["start"]["timeline"],
            timeline
        );
    }
}

#[test]
fn ended_destination_participants_cannot_be_reentered_or_duplicated() {
    let (_directory, world) = fixture();
    let first = candidate(&world, &preview(&world, &move_to_scene(&world, ORDER + 1)));
    let ended = changed(&first, OTHER_SCENE, |fields| {
        fields["participants"][1]["until"] = serde_yaml::to_value(point(TICK, ORDER + 1)).unwrap()
    });
    let mut request = move_to_scene(&ended, ORDER + 2);
    if let scene_control::Action::CharacterMove { event, .. } = &mut request.action {
        event.id = PRIOR_EVENT.into();
    }
    let error = scene_control::plan(&ended, &request)
        .unwrap_err()
        .to_string();
    assert!(error.contains("re-enter"), "{error}");
    let mut repeated = move_to_scene(&first, ORDER + 2);
    if let scene_control::Action::CharacterMove { event, .. } = &mut repeated.action {
        event.id = PRIOR_EVENT.into();
    }
    let after = candidate(&first, &preview(&first, &repeated));
    assert_eq!(
        after.records[OTHER_SCENE]
            .list("participants")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        after.records[OTHER_SCENE].get("objects"),
        first.records[OTHER_SCENE].get("objects")
    );
}

#[test]
fn optional_source_nulls_keep_noops_exact_and_survive_changed_typed_arrays_as_absence() {
    let (_directory, world) = fixture();
    let world = changed(&world, SCENE, |fields| {
        fields["time"]["end"] = Yaml::Null;
        for key in ["from", "until", "role", "point_of_view"] {
            fields["participants"][0][key] = Yaml::Null;
        }
    });
    let world = changed(&world, OTHER_SCENE, |fields| {
        fields["time"]["end"] = Yaml::Null;
        for key in ["from", "until", "role", "point_of_view"] {
            fields["participants"][0][key] = Yaml::Null;
        }
    });
    assert!(validation::validate_world(&world).valid);
    let report = preview(&world, &advance(&world, ORDER));
    assert_eq!(report["preview"]["canonicalNoOp"], true);
    assert_eq!(report["preview"]["changedFiles"], 0);
    for file in report["preview"]["files"].as_array().unwrap() {
        assert_eq!(file["beforeSource"], file["afterSource"]);
    }
    let proposed = candidate(&world, &preview(&world, &move_to_scene(&world, ORDER + 1)));
    assert!(validation::validate_world(&proposed).valid);
    for id in [SCENE, OTHER_SCENE] {
        assert_eq!(proposed.records[id].body, world.records[id].body);
        assert_eq!(
            proposed.records[id].get("x-author"),
            world.records[id].get("x-author")
        );
        assert!(
            proposed.records[id].frontmatter["time"]
                .get("end")
                .is_none()
        );
        assert!(
            proposed.records[id].frontmatter["participants"][0]
                .get("role")
                .is_none()
        );
    }
}

#[test]
fn state_and_event_reconciliation_refuse_unsupported_placement_without_expanding_global_validation()
{
    let (_directory, world) = fixture();
    let missing = changed(&world, ACTOR, |fields| {
        fields["initial_state"] = serde_yaml::to_value(json!({})).unwrap()
    });
    assert!(validation::validate_world(&missing).valid);
    assert!(matches!(
        scene_control::plan(&missing, &advance(&missing, ORDER)),
        Err(Error::Validation(_))
    ));
    let duplicate = changed(&world, OTHER_SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([BAG])).unwrap()
    });
    assert!(validation::validate_world(&duplicate).valid);
    assert!(matches!(
        scene_control::plan(&duplicate, &advance(&duplicate, ORDER)),
        Err(Error::Validation(_))
    ));
    let inaccessible = changed(&world, BAG, |fields| {
        fields["initial_state"] = serde_yaml::to_value(json!({})).unwrap()
    });
    assert!(validation::validate_world(&inaccessible).valid);
    assert!(matches!(
        scene_control::plan(&inaccessible, &advance(&inaccessible, ORDER)),
        Err(Error::Validation(_))
    ));
    let clash = prior_event(&world, ORDER + 1, Some(ORIGIN), &[ACTOR], json!([]));
    let action = json!({"type":"character.move","characterIds":[ACTOR],"locationId":DESTINATION,"event":event()});
    assert!(matches!(
        scene_control::plan(&clash, &input(&clash, ORDER + 1, action.clone())),
        Err(Error::Validation(_))
    ));
    let same_place = prior_event(&world, ORDER + 1, Some(DESTINATION), &[ACTOR], json!([]));
    assert!(scene_control::plan(&same_place, &input(&same_place, ORDER + 1, action)).is_ok());
}

#[test]
fn container_cycles_fail_closed_and_unlisted_containers_can_prove_carriage() {
    let (_directory, world) = fixture();
    let world = changed(&world, SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([CONTAINED])).unwrap()
    });
    let world = changed(&world, CONTAINED, |fields| {
        fields["initial_state"]["location"] =
            serde_yaml::to_value(json!({"entity":ORIGIN})).unwrap()
    });
    let request = input(
        &world,
        ORDER + 1,
        json!({"type":"scene.advance","sceneId":SCENE,"locationId":DESTINATION,"event":event()}),
    );
    let proposed = candidate(&world, &preview(&world, &request));
    assert_eq!(
        proposed.records[SCENE].string("location"),
        Some(DESTINATION)
    );
    // Remove direct location proof, then make the two containers cyclic.
    let cycle = changed(&world, CONTAINED, |fields| {
        fields["initial_state"]
            .as_mapping_mut()
            .unwrap()
            .remove("location")
            .map(|_| ())
            .unwrap()
    });
    let cycle = changed(&cycle, BAG, |fields| {
        fields["initial_state"] =
            serde_yaml::to_value(json!({"container":{"entity":CONTAINED}})).unwrap()
    });
    assert!(validation::validate_world(&cycle).valid);
    assert!(matches!(
        scene_control::plan(&cycle, &advance(&cycle, ORDER)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn deterministic_independent_effect_ids_do_not_depend_on_character_order() {
    let (_directory, world) = fixture();
    let mut plans = Vec::new();
    for cast in [vec![ACTOR, OTHER_ACTOR], vec![OTHER_ACTOR, ACTOR]] {
        let request = input(
            &world,
            ORDER + 2,
            json!({"type":"character.move","characterIds":cast,
            "locationId":DESTINATION,"event":event()}),
        );
        let proposed = candidate(&world, &preview(&world, &request));
        let effects = proposed.records[EVENT].list("effects").unwrap();
        let values = effects
            .iter()
            .map(|effect| {
                (
                    effect["target"].as_str().unwrap().to_owned(),
                    effect["id"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(values.values().collect::<BTreeSet<_>>().len(), 2);
        for id in values.values() {
            assert_eq!(id.len(), 33);
            assert!(id.starts_with("effect_"));
        }
        plans.push(values);
    }
    assert_eq!(plans[0], plans[1]);
}

#[test]
fn recursive_carriage_obeys_declared_container_kinds_without_inventing_one() {
    let (_directory, world) = fixture();
    let world = changed(&world, WORLD, |fields| {
        fields["state_keys"]["object"]["container"] =
            serde_yaml::to_value(json!({"type":"entity"})).unwrap();
        fields["state_keys"]["character"]["holder"] =
            serde_yaml::to_value(json!({"type":"entity","kind":"character"})).unwrap();
    });
    let world = added(
        &world,
        "character",
        THIRD_ACTOR,
        json!({"initial_state":{
        "location":{"entity":ORIGIN},"holder":{"entity":ACTOR}}}),
    );
    let world = changed(&world, CONTAINED, |fields| {
        fields["initial_state"] = serde_yaml::to_value(json!({
        "location":{"entity":ORIGIN},"container":{"entity":THIRD_ACTOR}}))
        .unwrap()
    });
    let world = changed(&world, SCENE, |fields| {
        fields["objects"] = serde_yaml::to_value(json!([CONTAINED])).unwrap()
    });
    assert!(validation::validate_world(&world).valid);
    let request = input(
        &world,
        ORDER + 1,
        json!({"type":"scene.advance","sceneId":SCENE,"locationId":DESTINATION,"event":event()}),
    );
    let proposed = candidate(&world, &preview(&world, &request));
    assert_eq!(
        proposed.records[SCENE].string("location"),
        Some(DESTINATION)
    );
    assert_preserved(&world, &proposed, &[CONTAINED, THIRD_ACTOR]);
    let constrained = changed(&world, WORLD, |fields| {
        fields["state_keys"]["object"]["container"]["kind"] = "object".into()
    });
    assert!(!validation::validate_world(&constrained).valid);
    assert!(matches!(
        scene_control::plan(&constrained, &advance(&constrained, ORDER + 1)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn closed_wire_rejects_nulls_duplicate_escaped_keys_and_bounded_titles() {
    let (_directory, world) = fixture();
    let valid = serde_json::to_value(advance(&world, ORDER + 1)).unwrap();
    for (field, value) in [
        ("locationId", Json::Null),
        ("event", Json::Null),
        ("sceneId", json!("alias")),
        ("unexpected", json!(true)),
    ] {
        let mut bad = valid.clone();
        bad["action"][field] = value;
        assert!(scene_control::decode(&bad.to_string()).is_err());
    }
    let raw = valid.to_string().replace(
        "\"sceneId\":",
        &format!("\"scene\\u0049d\":\"{SCENE}\",\"sceneId\":"),
    );
    assert!(scene_control::decode(&raw).is_err());
    let mut request = create(&world, ORDER + 1);
    if let scene_control::Action::SceneCreate { title, .. } = &mut request.action {
        *title = "🌲".repeat(257);
    }
    assert!(scene_control::plan(&world, &request).is_err());
    assert!(scene_control::decode(&serde_json::to_string(&request).unwrap()).is_err());
}

#[test]
fn full_plan_and_preview_bounds_refuse_omissions_and_count_wrapper_content() {
    let (_directory, world) = fixture();
    let request = create(&world, ORDER + 1);
    let report = preview(&world, &request);
    let length = serde_json::to_string(&report).unwrap().chars().count();
    assert!(scene_control::preview(&world, &request, length - 100).is_err());
    assert_eq!(
        scene_control::preview(&world, &request, length + 100).unwrap()["plan"],
        report["plan"]
    );
    let mut crowded = world;
    for number in 4..=64 {
        let id = format!("scene_{number:026}");
        // The native ID alphabet accepts these decimal digits as Crockford.
        crowded = added(
            &crowded,
            "scene",
            &id,
            json!({"status":"active","location":ORIGIN,
            "time":{"start":point(TICK-1,0),"current":point(TICK,ORDER)},"participants":[],"objects":[]}),
        );
    }
    assert_eq!(
        crowded
            .records
            .values()
            .filter(|record| record.kind == "scene")
            .count(),
        63
    );
    assert_eq!(
        scene_control::plan(&crowded, &advance(&crowded, ORDER + 1))
            .unwrap()
            .operations
            .len(),
        64
    );
    assert!(scene_control::plan(&crowded, &create(&crowded, ORDER + 1)).is_err());
}
