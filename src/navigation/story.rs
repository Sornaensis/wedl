//! Dated authored plot trails alongside the existing lifecycle evaluator.
//! Nearby scenes/events are never inferred to have advanced a story point.
use super::{
    causal::continuity_advisories,
    relations::{
        ByteBudget, bounded, citation, complete, envelope, limit_argument, named, point,
        settled_visible, visible,
    },
};
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    state_schema::{self, StateDefinition, StateType},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

fn malformed() -> Error {
    Error::Validation("invalid story navigation source".into())
}

#[derive(Default)]
struct Scope {
    labels: BTreeSet<String>,
    records: BTreeSet<String>,
    state: BTreeMap<String, BTreeMap<String, Yaml>>,
    unavailable_conditions: BTreeSet<String>,
}

fn value_available(
    world: &World,
    records: &BTreeSet<String>,
    definition: &StateDefinition,
    value: &Yaml,
) -> Result<bool> {
    if let Some(profile) = &definition.profile {
        return Ok(profile
            .references(value, world)?
            .iter()
            .all(|(_, id)| records.contains(id)));
    }
    let entity_available = |value: &Yaml| -> Result<bool> {
        Ok(records.contains(
            value
                .get("entity")
                .and_then(Yaml::as_str)
                .ok_or_else(malformed)?,
        ))
    };
    match definition.value_type {
        StateType::Entity => entity_available(value),
        StateType::EntitySet => {
            // Whole values are arrays; set add/remove operations use one
            // declared entity item. Literal string sets never take this path.
            if let Some(values) = value.as_sequence() {
                for value in values {
                    if !entity_available(value)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            } else {
                entity_available(value)
            }
        }
        _ => Ok(true),
    }
}

fn state_conditions<'a>(
    raw: &'a Yaml,
    depth: usize,
    nodes: &mut usize,
    output: &mut Vec<(&'a str, &'a str, &'a Yaml)>,
) -> Result<()> {
    *nodes += 1;
    if depth > 32 || *nodes > 1000 {
        return Err(Error::Validation(
            "when expression exceeds depth 32 or 1000 nodes".into(),
        ));
    }
    let fields = raw.as_mapping().ok_or_else(malformed)?;
    for (operator, value) in fields {
        match operator.as_str() {
            Some("all" | "any") => {
                for child in value.as_sequence().ok_or_else(malformed)? {
                    state_conditions(child, depth + 1, nodes, output)?;
                }
            }
            Some("not") => state_conditions(value, depth + 1, nodes, output)?,
            Some("state") => output.push((
                value
                    .get("entity")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?,
                value
                    .get("key")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?,
                value.get("equals").ok_or_else(malformed)?,
            )),
            _ => {}
        }
    }
    Ok(())
}

fn scope(world: &World, at: &StoryTime) -> Result<Scope> {
    let mut scope = Scope::default();
    for record in world.records.values() {
        if crate::description::full_author_record_visible(world, record, None)? {
            scope.labels.insert(record.id.clone());
        }
        if visible(world, record, Some(at))? {
            scope.records.insert(record.id.clone());
        }
    }
    let mut predicates = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "story-point" && record.status() == "canonical")
    {
        if !scope.records.contains(&record.id) {
            continue;
        }
        let mut conditions = Vec::new();
        if let Some(raw) = record.get("when") {
            state_conditions(raw, 0, &mut 0, &mut conditions)?;
        }
        bounded(predicates.len() + conditions.len(), 1000)?;
        for (owner_id, key, equals) in conditions {
            let owner = world.records.get(owner_id).ok_or_else(malformed)?;
            let definition = state_schema::field_definition(world, &owner.kind, key)?;
            if !scope.records.contains(owner_id)
                || !value_available(world, &scope.records, &definition, equals)?
            {
                scope.unavailable_conditions.insert(record.id.clone());
            }
            scope.state.entry(owner_id.into()).or_default();
            predicates.push((record.id.as_str(), owner_id, key));
        }
    }
    for (id, state) in &mut scope.state {
        if !scope.records.contains(id) {
            continue;
        }
        let owner = &world.records[id];
        let mut withheld = false;
        let replay =
            crate::runtime::replay_state_with_event_effects(world, id, at, |event, effect| {
                if !scope.records.contains(&event.id) {
                    withheld = true;
                    return Ok(false);
                }
                let Some(value) = effect.get("value") else {
                    return Ok(true);
                };
                let key = effect
                    .get("key")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?;
                let definition = state_schema::field_definition(world, &owner.kind, key)?;
                let available = value_available(world, &scope.records, &definition, value)?;
                withheld |= !available;
                Ok(available)
            });
        *state = match replay {
            Ok((values, _)) => values,
            Err(Error::Validation(_)) if withheld => {
                // A hidden clear/offset can be necessary for a later visible
                // effect to respect exclusivity or numeric bounds. Confirm
                // canonical replay is valid, then mark this incomplete fold
                // unavailable rather than reintroducing its hidden effects.
                crate::runtime::resolved_state(world, id, at)?;
                for (point_id, owner_id, _) in &predicates {
                    if *owner_id == id {
                        scope.unavailable_conditions.insert((*point_id).into());
                    }
                }
                BTreeMap::new()
            }
            Err(error) => return Err(error),
        };
        for (point_id, owner_id, key) in &predicates {
            if *owner_id != id {
                continue;
            }
            if let Some(value) = state.get(*key) {
                let definition = state_schema::field_definition(world, &owner.kind, key)?;
                if !value_available(world, &scope.records, &definition, value)? {
                    scope.unavailable_conditions.insert((*point_id).into());
                }
            }
        }
    }
    Ok(scope)
}

fn event_link(world: &World, id: &str, through: &StoryTime) -> Result<Option<Json>> {
    let Some(event) = world.records.get(id) else {
        return Ok(None);
    };
    if event.kind != "event"
        || event.status() != "canonical"
        || !settled_visible(world, event, Some(through))?
    {
        return Ok(None);
    }
    let mut value = named(event);
    value["time"] = json!(point(world, event.get("time"))?);
    value["citation"] = citation(world, event, "time");
    Ok(Some(value))
}

fn trail(
    world: &World,
    record: &Record,
    at: &StoryTime,
    limit: usize,
    members: &mut usize,
    bytes: &mut ByteBudget,
) -> Result<Json> {
    let mut transitions = Vec::new();
    for (index, member) in record
        .get("lifecycle")
        .and_then(|value| value.get("transitions"))
        .and_then(Yaml::as_sequence)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let time = point(world, member.get("time"))?;
        if !time.not_after(at) {
            continue;
        }
        *members += 1;
        bounded(*members, limit)?;
        let cause = member
            .get("causing_event")
            .and_then(Yaml::as_str)
            .map(|id| event_link(world, id, &time))
            .transpose()?
            .flatten();
        let value = json!({"transitionId":member.get("id").and_then(Yaml::as_str).ok_or_else(malformed)?,
            "at":time,"state":member.get("state").and_then(Yaml::as_str).ok_or_else(malformed)?,
            "note":member.get("note").and_then(Yaml::as_str),"causingEvent":cause,
            "citation":citation(world,record,&format!("lifecycle.transitions[{index}]"))});
        bytes.charge(&value)?;
        transitions.push((time, index, value));
    }
    transitions.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut outcomes = Vec::new();
    for (index, value) in record
        .list("outcome_events")
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(id) = value.as_str() else {
            return Err(malformed());
        };
        let Some(event) = event_link(world, id, at)? else {
            continue;
        };
        *members += 1;
        bounded(*members, limit)?;
        let source = &world.records[id];
        let time = point(world, source.get("time"))?;
        let value = json!({"event":event,"at":time,
            "citation":citation(world,record,&format!("outcome_events[{index}]"))});
        bytes.charge(&value)?;
        outcomes.push((time, source.title.to_lowercase(), id, value));
    }
    outcomes.sort_by(|a, b| (&a.0, &a.1, a.2).cmp(&(&b.0, &b.1, b.2)));
    Ok(json!({
        "transitions":transitions.into_iter().map(|(_,_,value)|value).collect::<Vec<_>>(),
        "outcomeEvents":outcomes.into_iter().map(|(_,_,_,value)|value).collect::<Vec<_>>()
    }))
}

fn enrich(
    world: &World,
    record: &Record,
    mut item: Json,
    at: &StoryTime,
    limit: usize,
    members: &mut usize,
    scope: &Scope,
    bytes: &mut ByteBudget,
) -> Result<Json> {
    // The canonical evaluator retains its existing unrestricted author
    // semantics. This admitted navigation view refuses to evaluate a whole
    // condition/dependency set through a withheld endpoint; in particular a
    // hidden false leaf must not become true under `not` or an `any` branch.
    let mut condition_available = !scope.unavailable_conditions.contains(&record.id);
    let mut requirements_available = true;
    let mut redact_temporal_reasons = false;
    for (path, id) in crate::revision_review::references(world, record)? {
        let condition = path == "when" || path.starts_with("when.");
        let requirement = path.starts_with("requires[");
        if !condition && !requirement {
            continue;
        }
        // An event that has not happened and a relationship without a current
        // transition are false predicates, not unavailable identities. Keep
        // their boolean semantics while withholding their future link labels.
        let temporal_predicate =
            condition && (path.ends_with(".event") || path.ends_with(".relationship.id"));
        let mut available = if temporal_predicate {
            scope.labels.contains(&id)
        } else {
            scope.records.contains(&id)
        };
        redact_temporal_reasons |= temporal_predicate && available && !scope.records.contains(&id);
        if condition && path.ends_with(".relationship.id") && available {
            let relationship = &world.records[&id];
            available = ["from", "to"].iter().all(|key| {
                relationship
                    .string(key)
                    .is_some_and(|id| scope.labels.contains(id))
            });
        }
        condition_available &= !condition || available;
        requirements_available &= !requirement || available;
    }
    if !condition_available || !requirements_available || redact_temporal_reasons {
        if !condition_available {
            item["conditionSatisfied"] = json!(false);
        }
        if !requirements_available {
            item["requirementsSatisfied"] = json!(false);
        }
        if !condition_available || !requirements_available {
            item["eligible"] = json!(false);
        }
        let mut reasons = Vec::new();
        if item["storedState"] != "dormant" {
            reasons.push(format!(
                "stored lifecycle is {}; eligibility requires dormant",
                item["storedState"].as_str().ok_or_else(malformed)?
            ));
        }
        if item["requirementsSatisfied"] == false {
            reasons.push(
                if requirements_available {
                    "authored requirements are not satisfied"
                } else {
                    "authored requirements are unavailable at this horizon"
                }
                .to_owned(),
            );
        }
        if item["conditionSatisfied"] == false {
            reasons.push(
                if condition_available {
                    "authored condition is not satisfied"
                } else {
                    "authored condition is unavailable at this horizon"
                }
                .to_owned(),
            );
        }
        item["blockedReasons"] = json!(reasons);
    }
    let trail = trail(world, record, at, limit, members, bytes)?;
    // Preserve the existing convenience IDs, with the same independent
    // admission as the named original-source links beside them.
    item["outcomeEvents"] = json!(
        trail["outcomeEvents"]
            .as_array()
            .ok_or_else(malformed)?
            .iter()
            .map(|value| value["event"]["id"].clone())
            .collect::<Vec<_>>()
    );
    bytes.charge(&item)?;
    item["plotTrail"] = trail;
    Ok(item)
}

/// All admitted canonical candidates keep the existing priority/title order.
/// An explicit scene is evaluator context, never a story-point list filter.
pub fn story_points(
    world: &World,
    at: &StoryTime,
    scene: Option<&str>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let scope = scope(world, at)?;
    let mut evaluated = crate::story::story_points_with_admitted_context(
        world,
        at,
        scene,
        &scope.state,
        &scope.records,
    )?;
    let values = evaluated["items"].take();
    let mut items = Vec::new();
    let mut members = 0;
    let mut bytes = ByteBudget::default();
    for item in values.as_array().ok_or_else(malformed)? {
        let id = item["storyPointId"].as_str().ok_or_else(malformed)?;
        let record = world.records.get(id).ok_or_else(malformed)?;
        if !settled_visible(world, record, Some(at))? {
            continue;
        }
        bounded(items.len() + 1, limit)?;
        items.push(enrich(
            world,
            record,
            item.clone(),
            at,
            limit,
            &mut members,
            &scope,
            &mut bytes,
        )?);
    }
    let mut result = envelope(world, Some(at), limit);
    result["sceneId"] = json!(scene);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["continuityAdvisories"] = json!(continuity_advisories(world, at, limit)?);
    complete(result)
}

/// Detail for one exact admitted canonical story point, evaluated in the same
/// world as its requirements. The original authored transition indexes remain.
pub fn story_point_trail(
    world: &World,
    id: &str,
    at: &StoryTime,
    scene: Option<&str>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    if !crate::validation::valid_id_for_kind(id, "story-point") {
        return Err(Error::InvalidInput(
            "story-point navigation requires an exact story-point ID".into(),
        ));
    }
    let unavailable = || Error::NotFound("story point is unavailable".into());
    let record = world.records.get(id).ok_or_else(unavailable)?;
    if record.kind != "story-point"
        || record.status() != "canonical"
        || !settled_visible(world, record, Some(at))?
    {
        return Err(unavailable());
    }
    let scope = scope(world, at)?;
    let evaluated = crate::story::story_points_with_admitted_context(
        world,
        at,
        scene,
        &scope.state,
        &scope.records,
    )?;
    let item = evaluated["items"]
        .as_array()
        .ok_or_else(malformed)?
        .iter()
        .find(|value| value["storyPointId"] == id)
        .ok_or_else(unavailable)?;
    let mut result = envelope(world, Some(at), limit);
    result["sceneId"] = json!(scene);
    result["storyPointId"] = json!(id);
    result["storyPoint"] = enrich(
        world,
        record,
        item.clone(),
        at,
        limit,
        &mut 0,
        &scope,
        &mut ByteBudget::default(),
    )?;
    result["continuityAdvisories"] = json!(continuity_advisories(world, at, limit)?);
    complete(result)
}
