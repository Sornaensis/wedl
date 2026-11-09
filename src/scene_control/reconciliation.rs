//! Additional continuity checks for complete scene-control candidates only.
//! Ordinary admission runs first. Placement requires an unbound entity state
//! value; a literal or a profile field named `entity` supplies no such proof.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    runtime, scene,
    state_schema::{self, StateType},
};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

enum Reference {
    Absent,
    Entity(String),
    Bound,
}

impl Reference {
    fn entity(&self) -> Option<&str> {
        match self {
            Self::Entity(id) => Some(id),
            _ => None,
        }
    }
}

fn state_reference(
    world: &World,
    owner: &Record,
    state: &BTreeMap<String, Yaml>,
    key: &str,
) -> Result<Reference> {
    let Some(value) = state.get(key) else {
        return Ok(Reference::Absent);
    };
    let definition = state_schema::field_definition(world, &owner.kind, key)?;
    if definition.profile.is_some() {
        return Ok(Reference::Bound);
    }
    if definition.value_type != StateType::Entity {
        return Ok(Reference::Absent);
    }
    Ok(value
        .get("entity")
        .and_then(Yaml::as_str)
        .map(|id| Reference::Entity(id.to_owned()))
        .unwrap_or(Reference::Absent))
}

fn unsupported(owner: &str, key: &str) -> Error {
    Error::Validation(format!(
        "scene-control cannot establish placement from bound state {owner}.{key}; an operative profile reference is not supported"
    ))
}

struct Placement {
    holder: Reference,
    location: Reference,
    container: Reference,
}

/// Follow ordinary container references without inventing object state. A
/// cycle supplies no proof, and an unrelated bound field cannot override an
/// ordinary path that establishes accessibility.
fn accessible(
    world: &World,
    object_id: &str,
    location: &str,
    present: &BTreeSet<&str>,
    at: &StoryTime,
    placements: &mut BTreeMap<String, Placement>,
) -> Result<bool> {
    let mut visiting = BTreeSet::new();
    let mut cursor = object_id.to_owned();
    let mut blocked = None;
    while visiting.insert(cursor.clone()) {
        if !placements.contains_key(&cursor) {
            let owner = world.records.get(&cursor).ok_or_else(|| {
                Error::Validation(format!("unknown scene object or container {cursor}"))
            })?;
            let state = runtime::resolved_state(world, &cursor, at)?;
            placements.insert(
                cursor.clone(),
                Placement {
                    holder: state_reference(world, owner, &state, "holder")?,
                    location: state_reference(world, owner, &state, "location")?,
                    container: state_reference(world, owner, &state, "container")?,
                },
            );
        }
        let placement = &placements[&cursor];
        if placement
            .holder
            .entity()
            .is_some_and(|holder| present.contains(holder))
            || placement.location.entity() == Some(location)
        {
            return Ok(true);
        }
        for (key, value) in [
            ("holder", &placement.holder),
            ("location", &placement.location),
            ("container", &placement.container),
        ] {
            if matches!(value, Reference::Bound) && blocked.is_none() {
                blocked = Some((cursor.clone(), key));
            }
        }
        let Some(container) = placement.container.entity() else {
            break;
        };
        cursor = container.to_owned();
    }
    if let Some((owner, key)) = blocked {
        return Err(unsupported(&owner, key));
    }
    Ok(false)
}

fn event_places(world: &World) -> Result<()> {
    let mut places: BTreeMap<(StoryTime, &str), (&Record, &str)> = BTreeMap::new();
    for event in world
        .records
        .values()
        .filter(|record| record.kind == "event" && record.status() == "canonical")
    {
        let Some(location) = event.string("location") else {
            continue;
        };
        let at = StoryTime::from_yaml(
            event
                .get("time")
                .ok_or_else(|| Error::Validation("event requires time".into()))?,
            &world.default_timeline(),
        )?;
        for participant in event.list("participants").into_iter().flatten() {
            let Some(character) = participant.get("character").and_then(Yaml::as_str) else {
                continue;
            };
            let (previous, previous_location) = places
                .entry((at.clone(), character))
                .or_insert((event, location));
            if *previous_location != location {
                return Err(Error::Validation(format!(
                    "WDL-EVENT-006: {character} participates in canonical events {} at {previous_location} and {} at {location} at {} {}:{}; use a later order for a handoff",
                    previous.id, event.id, at.timeline, at.tick, at.order
                )));
            }
        }
    }
    Ok(())
}

/// Validate only after the complete native candidate has passed ordinary
/// admission. No base-world check prevents explicit effects from repairing a
/// prior inconsistency, and existing read/edit APIs keep their own contracts.
pub(super) fn validate(world: &World) -> Result<()> {
    event_places(world)?;
    let scenes: Vec<_> = world
        .records
        .values()
        .filter(|record| record.kind == "scene" && record.status() == "active")
        .collect();
    if scenes.is_empty() {
        return Ok(());
    }
    let at = world.current_time()?;
    let mut listed = BTreeMap::new();
    let mut placements = BTreeMap::new();
    for selected in scenes {
        let location = selected.string("location").ok_or_else(|| {
            Error::Validation(format!("active scene {} requires location", selected.id))
        })?;
        let mut present = BTreeSet::new();
        for participant in selected.list("participants").into_iter().flatten() {
            let Some(character) = participant.get("character").and_then(Yaml::as_str) else {
                continue;
            };
            if scene::present(world, selected, character, &at)? {
                present.insert(character);
            }
        }
        for character in &present {
            let owner = &world.records[*character];
            let state = runtime::resolved_state(world, character, &at)?;
            let actual = state_reference(world, owner, &state, "location")?;
            if matches!(actual, Reference::Bound) {
                return Err(unsupported(character, "location"));
            }
            if actual.entity() != Some(location) {
                return Err(Error::Validation(format!(
                    "WDL-SCENE-020: active participant {character} resolves to ordinary entity location {:?}, not location {location} of scene {}",
                    actual.entity(),
                    selected.id
                )));
            }
        }
        for value in selected.list("objects").into_iter().flatten() {
            let object = value.as_str().ok_or_else(|| {
                Error::Validation(format!("scene {} objects must be record IDs", selected.id))
            })?;
            if let Some(previous) = listed.insert(object, selected.id.as_str()) {
                if previous != selected.id {
                    return Err(Error::Validation(format!(
                        "WDL-SCENE-024: active scene object {object} is listed in both {previous} and {}",
                        selected.id
                    )));
                }
            }
            if !accessible(world, object, location, &present, &at, &mut placements)? {
                return Err(Error::Validation(format!(
                    "WDL-SCENE-021: active scene object {object} in {} is not located in the scene, held by a present participant, or contained in an accessible object",
                    selected.id
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{source, validation};
    use serde_json::{Value as Json, json};
    use std::path::PathBuf;

    const WORLD: &str = "world_00000000000000000000000000";
    const PLACE: &str = "loc_00000000000000000000000001";
    const ELSEWHERE: &str = "loc_00000000000000000000000002";
    const ACTOR: &str = "char_00000000000000000000000001";
    const SCENE: &str = "scene_00000000000000000000000001";
    const OTHER_SCENE: &str = "scene_00000000000000000000000002";
    const ITEM: &str = "obj_00000000000000000000000001";
    const BAG: &str = "obj_00000000000000000000000002";
    const EVENT: &str = "event_00000000000000000000000001";
    const OTHER_EVENT: &str = "event_00000000000000000000000002";
    const EFFECT: &str = "effect_00000000000000000000000001";
    const OTHER_EFFECT: &str = "effect_00000000000000000000000002";

    fn point(tick: i64, order: i32) -> Json {
        json!({"timeline":"main","tick":tick,"order":order})
    }

    fn record(kind: &str, id: &str, fields: Json) -> Json {
        let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,
            "title":id,"status":"canonical"});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        value
    }

    fn fixture() -> Vec<Json> {
        vec![
            record(
                "world",
                WORLD,
                json!({
                    "capabilities":[],"threads":[],"default_timeline":"main",
                    "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
                    "current_time":point(10,0),
                    "state_keys":{
                        "character":{"location":{"type":"entity","kind":"location"}},
                        "object":{
                            "location":{"type":"entity","kind":"location"},
                            "holder":{"type":"entity","kind":"character"},
                            "container":{"type":"entity","kind":"object"}
                        }
                    }
                }),
            ),
            record("location", PLACE, json!({})),
            record("location", ELSEWHERE, json!({})),
            record(
                "character",
                ACTOR,
                json!({"initial_state":{"location":{"entity":PLACE}}}),
            ),
            record("object", ITEM, json!({"initial_state":{}})),
            record("object", BAG, json!({"initial_state":{}})),
            record(
                "scene",
                SCENE,
                json!({
                    "status":"active","location":PLACE,
                    "time":{"start":point(0,0),"current":point(10,0)},
                    "participants":[{"character":ACTOR}],"objects":[]
                }),
            ),
        ]
    }

    fn edit<'a>(records: &'a mut [Json], id: &str) -> &'a mut Json {
        records.iter_mut().find(|value| value["id"] == id).unwrap()
    }

    fn world(records: &[Json]) -> World {
        let records = records
            .iter()
            .map(|value| {
                let text = format!("---\n{}---\n", serde_yaml::to_string(value).unwrap());
                source::parse_record(
                    text.as_bytes(),
                    &format!("story/{}.md", value["id"].as_str().unwrap()),
                )
                .unwrap()
            })
            .collect();
        let world = source::world_from_records(
            PathBuf::from("/scene-control-reconciliation-tests"),
            records,
            None,
        )
        .unwrap();
        let report = validation::validate_world(&world);
        assert!(report.valid, "{:?}", report.diagnostics);
        world
    }

    fn event(id: &str, at: Json, location: &str, effects: Json) -> Json {
        record(
            "event",
            id,
            json!({"time":at,"location":location,
            "participants":[{"character":ACTOR}],"effects":effects}),
        )
    }

    fn literal_profile(records: &mut [Json], bindings: Json) {
        let owner = edit(records, WORLD);
        owner["capabilities"] = json!(["lore-core-v1"]);
        owner["state_profile_bindings"] = bindings;
        owner["profiles"] = json!([{"id":"literal-placement","label":"Literal placement",
            "fields":{"type":"object","required":{"entity":{"type":"string"}},"optional":{}}}]);
    }

    #[test]
    fn active_cast_uses_exact_replayed_state_and_inclusive_presence() {
        let mut records = fixture();
        records.push(event(
            EVENT,
            point(10, 1),
            ELSEWHERE,
            json!([{
                "id":EFFECT,"target":ACTOR,"key":"location","operation":"set",
                "value":{"entity":ELSEWHERE}
            }]),
        ));
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, WORLD)["current_time"] = point(10, 1);
        edit(&mut records, SCENE)["time"]["current"] = point(10, 1);
        edit(&mut records, SCENE)["participants"][0]["until"] = point(10, 1);
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-020")
        );
        edit(&mut records, SCENE)["participants"][0]["until"] = point(10, 0);
        assert!(validate(&world(&records)).is_ok());
    }

    #[test]
    fn absent_cleared_literal_and_bound_character_location_are_not_placement() {
        let mut records = fixture();
        edit(&mut records, ACTOR)["initial_state"] = json!({});
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-020")
        );
        edit(&mut records, WORLD)["state_keys"]["character"] = json!({});
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-020")
        );

        let mut records = fixture();
        records.push(event(
            EVENT,
            point(10, 0),
            PLACE,
            json!([{
                "id":EFFECT,"target":ACTOR,"key":"location","operation":"clear"
            }]),
        ));
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-020")
        );

        let mut records = fixture();
        edit(&mut records, WORLD)["state_keys"]["character"]["location"] = json!({"type":"string"});
        edit(&mut records, ACTOR)["initial_state"]["location"] = json!(PLACE);
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-020")
        );

        let mut records = fixture();
        literal_profile(
            &mut records,
            json!({"character":{"location":"literal-placement"}}),
        );
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("bound state")
        );
    }

    #[test]
    fn listed_objects_follow_unlisted_containers_and_explicit_drop_effects() {
        let mut records = fixture();
        edit(&mut records, SCENE)["objects"] = json!([ITEM]);
        edit(&mut records, ITEM)["initial_state"] = json!({"container":{"entity":BAG}});
        edit(&mut records, BAG)["initial_state"] = json!({"holder":{"entity":ACTOR}});
        records.push(event(
            EVENT,
            point(10, 1),
            PLACE,
            json!([
                {"id":EFFECT,"target":BAG,"key":"holder","operation":"clear"},
                {"id":OTHER_EFFECT,"target":BAG,"key":"location","operation":"set",
                    "value":{"entity":ELSEWHERE}}
            ]),
        ));
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, WORLD)["current_time"] = point(10, 1);
        edit(&mut records, SCENE)["time"]["current"] = point(10, 1);
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-021")
        );

        records.retain(|value| value["id"] != EVENT);
        edit(&mut records, BAG)["initial_state"] = json!({"container":{"entity":ITEM}});
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-021")
        );
    }

    #[test]
    fn ordinary_accessibility_proof_does_not_interpret_irrelevant_bound_values() {
        let mut records = fixture();
        edit(&mut records, SCENE)["objects"] = json!([ITEM]);
        edit(&mut records, ITEM)["initial_state"] = json!({
            "holder":{"entity":ACTOR},"location":{"entity":PLACE}
        });
        literal_profile(
            &mut records,
            json!({"object":{"holder":"literal-placement"}}),
        );
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, ITEM)["initial_state"]
            .as_object_mut()
            .unwrap()
            .remove("location");
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("bound state")
        );

        edit(&mut records, ITEM)["initial_state"]["container"] = json!({"entity":BAG});
        edit(&mut records, BAG)["initial_state"] = json!({"location":{"entity":PLACE}});
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, ITEM)["initial_state"]
            .as_object_mut()
            .unwrap()
            .remove("holder");
        literal_profile(
            &mut records,
            json!({"object":{"container":"literal-placement"}}),
        );
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("bound state")
        );

        let mut records = fixture();
        edit(&mut records, SCENE)["objects"] = json!([ITEM]);
        edit(&mut records, WORLD)["state_keys"]["object"]["holder"] = json!({"type":"string"});
        edit(&mut records, ITEM)["initial_state"] = json!({"holder":ACTOR});
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-021")
        );
    }

    #[test]
    fn only_active_scenes_may_not_share_a_listed_object() {
        let mut records = fixture();
        edit(&mut records, SCENE)["objects"] = json!([ITEM]);
        edit(&mut records, ITEM)["initial_state"] = json!({"location":{"entity":PLACE}});
        records.push(record(
            "scene",
            OTHER_SCENE,
            json!({"status":"active","location":PLACE,
            "time":{"start":point(0,0),"current":point(10,0)},"participants":[],"objects":[ITEM]}),
        ));
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-SCENE-024")
        );
        edit(&mut records, OTHER_SCENE)["status"] = json!("closed");
        edit(&mut records, OTHER_SCENE)["time"]["end"] = point(10, 0);
        assert!(validate(&world(&records)).is_ok());
    }

    #[test]
    fn canonical_event_places_are_independent_exact_coordinates() {
        let mut records = fixture();
        let at = point(i64::MAX, i32::MIN);
        records.push(event(EVENT, at.clone(), ELSEWHERE, json!([])));
        // Event participation alone does not rewrite the active cast's state.
        assert!(validate(&world(&records)).is_ok());
        records.push(event(OTHER_EVENT, at.clone(), PLACE, json!([])));
        assert!(
            validate(&world(&records))
                .unwrap_err()
                .to_string()
                .contains("WDL-EVENT-006")
        );
        edit(&mut records, OTHER_EVENT)["location"] = json!(ELSEWHERE);
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, OTHER_EVENT)["location"] = json!(PLACE);
        edit(&mut records, OTHER_EVENT)["time"]["order"] = json!(i32::MIN + 1);
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, OTHER_EVENT)["time"] = at.clone();
        edit(&mut records, OTHER_EVENT)["time"]["timeline"] = json!("other");
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, OTHER_EVENT)["time"] = at;
        edit(&mut records, OTHER_EVENT)["status"] = json!("draft");
        assert!(validate(&world(&records)).is_ok());
        edit(&mut records, OTHER_EVENT)["status"] = json!("canonical");
        edit(&mut records, OTHER_EVENT)
            .as_object_mut()
            .unwrap()
            .remove("location");
        assert!(validate(&world(&records)).is_ok());
    }
}
