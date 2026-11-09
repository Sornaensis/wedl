//! Literal current facts admitted by one selected character scene. References
//! never grant knowledge, and operative placement never exposes bound siblings.
use crate::{
    Error, Result,
    lore_types::TypeSpec,
    model::{Record, StoryTime, World},
    narrative::{self, CharacterScope},
    runtime, scene, service,
    state_schema::{self, StateDefinition, StateType},
};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

const MAX_ROWS: usize = 1000;
const MAX_DETAILS: usize = 10_000;
const MAX_PLACEMENT_NODES: usize = 10_000;
const MAX_BYTES: usize = 4 * 1024 * 1024;

fn limit() -> Error {
    Error::InvalidInput(
        "complete writing present context exceeds a bound (1000 rows, 10000 details, 10000 placement nodes, or 4 MiB)"
            .into(),
    )
}
fn malformed() -> Error {
    Error::Validation("writing present context requires literal typed facts".into())
}

#[derive(Default)]
struct Budget {
    bytes: usize,
    details: usize,
    rows: usize,
}
impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(bytes.len());
        if self.bytes > MAX_BYTES {
            return Err(std::io::Error::other("writing present byte bound"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Budget {
    fn row(&mut self) -> Result<()> {
        self.rows += 1;
        if self.rows > MAX_ROWS {
            return Err(limit());
        }
        Ok(())
    }
    fn measure(&mut self, value: &impl Serialize) -> Result<()> {
        serde_json::to_writer(self, value).map_err(|_| limit())
    }
    fn nodes(&mut self, value: &Yaml, depth: usize) -> Result<()> {
        self.details += 1;
        if self.details > MAX_DETAILS || depth > 8 {
            return Err(limit());
        }
        match value {
            Yaml::Mapping(values) => {
                for (key, value) in values {
                    self.nodes(key, depth + 1)?;
                    self.nodes(value, depth + 1)?;
                }
            }
            Yaml::Sequence(values) => {
                for value in values {
                    self.nodes(value, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn copy(&mut self, value: &Yaml) -> Result<Json> {
        self.nodes(value, 0)?;
        self.measure(value)?;
        service::yaml_json(value)
    }
}

fn definition(world: &World, key: &str) -> Result<Option<StateDefinition>> {
    world
        .world_record()?
        .get("state_keys")
        .and_then(|v| v.get("object"))
        .and_then(|v| v.get(key))
        .map(|_| state_schema::field_definition(world, "object", key))
        .transpose()
}
fn state_type(value: StateType) -> &'static str {
    match value {
        StateType::String => "string",
        StateType::Boolean => "boolean",
        StateType::Integer => "integer",
        StateType::Number => "number",
        StateType::Entity => "entity",
        StateType::StringSet => "string-set",
        StateType::EntitySet => "entity-set",
    }
}
fn scalar_type(value: &Yaml) -> Result<&'static str> {
    match value {
        Yaml::Null => Ok("null"),
        Yaml::Bool(_) => Ok("boolean"),
        Yaml::String(_) => Ok("string"),
        Yaml::Number(number) if number.is_i64() || number.is_u64() => Ok("integer"),
        Yaml::Number(number) if number.as_f64().is_some_and(f64::is_finite) => Ok("number"),
        _ => Err(malformed()),
    }
}

/// The validated native scalar/profile placement contract admits only set and
/// clear. Borrow the last value in native event/effect order; no opaque profile
/// value, other state field, or causal citation needs to be cloned.
fn scalar<'a>(
    record: &'a Record,
    key: &str,
    events: &[(&'a Record, StoryTime)],
) -> Option<&'a Yaml> {
    let mut value = record.get("initial_state").and_then(|v| v.get(key));
    for (event, _) in events {
        for effect in event.list("effects").into_iter().flatten() {
            if effect.get("target").and_then(Yaml::as_str) != Some(record.id.as_str())
                || effect.get("key").and_then(Yaml::as_str) != Some(key)
            {
                continue;
            }
            match effect.get("operation").and_then(Yaml::as_str) {
                Some("set") => value = effect.get("value"),
                Some("clear") => value = None,
                _ => {}
            }
        }
    }
    value
}

fn placement<'a>(
    world: &'a World,
    value: Option<&'a Yaml>,
    definition: Option<&StateDefinition>,
    expected: &str,
    allowed: &BTreeSet<String>,
) -> Option<&'a str> {
    let definition = definition?;
    let value = value?;
    let id = if let Some(profile) = &definition.profile {
        let TypeSpec::Object { required, optional } = &profile.fields else {
            return None;
        };
        if !matches!(required.get("entity").or_else(|| optional.get("entity")),
            Some(TypeSpec::Entity { kinds }) if kinds.iter().any(|kind| kind == expected))
        {
            return None;
        }
        let reference = value.get("entity")?.as_mapping()?;
        if reference.len() != 2 || reference.get("kind").and_then(Yaml::as_str) != Some("record") {
            return None;
        }
        reference.get("id")?.as_str()?
    } else {
        if definition.value_type != StateType::Entity {
            return None;
        }
        value.get("entity")?.as_str()?
    };
    world
        .records
        .get(id)
        .filter(|record| record.kind == expected && allowed.contains(id))
        .map(|record| record.id.as_str())
}

#[derive(Clone, Copy)]
struct Placement<'a> {
    holder: Option<&'a str>,
    location: Option<&'a str>,
    container: Option<&'a str>,
}
struct Objects<'a> {
    world: &'a World,
    events: Vec<(&'a Record, StoryTime)>,
    allowed: BTreeSet<String>,
    definitions: [Option<StateDefinition>; 4],
    participants: BTreeSet<String>,
    location: Option<&'a str>,
    placements: BTreeMap<&'a str, Placement<'a>>,
    presence: BTreeMap<&'a str, bool>,
}
impl<'a> Objects<'a> {
    fn placement(&mut self, id: &'a str) -> Result<Placement<'a>> {
        if let Some(value) = self.placements.get(id) {
            return Ok(*value);
        }
        if self.placements.len() == MAX_PLACEMENT_NODES {
            return Err(limit());
        }
        let record = &self.world.records[id];
        let value = Placement {
            holder: placement(
                self.world,
                scalar(record, "holder", &self.events),
                self.definitions[0].as_ref(),
                "character",
                &self.allowed,
            ),
            location: placement(
                self.world,
                scalar(record, "location", &self.events),
                self.definitions[1].as_ref(),
                "location",
                &self.allowed,
            ),
            container: placement(
                self.world,
                scalar(record, "container", &self.events),
                self.definitions[2].as_ref(),
                "object",
                &self.allowed,
            ),
        };
        self.placements.insert(id, value);
        Ok(value)
    }
    fn present(&mut self, id: &'a str) -> Result<bool> {
        let mut path = BTreeSet::new();
        let mut current = id;
        let present = loop {
            if let Some(present) = self.presence.get(current) {
                break *present;
            }
            if !path.insert(current) {
                break false;
            }
            let value = self.placement(current)?;
            if value
                .holder
                .is_some_and(|id| self.participants.contains(id))
                || value.location.is_some_and(|id| Some(id) == self.location)
            {
                break true;
            }
            let Some(container) = value.container else {
                break false;
            };
            current = container;
        };
        for id in path {
            self.presence.insert(id, present);
        }
        Ok(present)
    }
}

fn entity_admitted(value: &Yaml, allowed: &BTreeSet<String>) -> bool {
    value
        .get("entity")
        .and_then(Yaml::as_str)
        .is_some_and(|id| allowed.contains(id))
}

fn set_token(value: &Yaml, entity: bool) -> Result<&str> {
    if entity {
        value.get("entity").and_then(Yaml::as_str)
    } else {
        value.as_str()
    }
    .ok_or_else(malformed)
}

#[derive(Clone, Copy)]
struct SetSummary {
    removed: bool,
    added: Option<usize>,
    in_base: bool,
}
struct SetReplay<'a> {
    owner: &'a str,
    base: &'a [Yaml],
    events: &'a [(&'a Record, StoryTime)],
    after: Option<usize>,
    entity: bool,
    cache: BTreeMap<&'a str, SetSummary>,
}
impl<'a> SetReplay<'a> {
    fn effects(&self) -> impl Iterator<Item = (usize, &'a Yaml)> + '_ {
        self.events
            .iter()
            .flat_map(|(event, _)| event.list("effects").into_iter().flatten())
            .filter(|effect| {
                effect.get("target").and_then(Yaml::as_str) == Some(self.owner)
                    && effect.get("key").and_then(Yaml::as_str) == Some("condition")
            })
            .enumerate()
            .filter(|(index, _)| self.after.is_none_or(|after| *index > after))
    }
    fn summary(&mut self, value: &'a Yaml) -> Result<SetSummary> {
        let token = set_token(value, self.entity)?;
        if let Some(summary) = self.cache.get(token) {
            return Ok(*summary);
        }
        let mut summary = SetSummary {
            removed: false,
            added: None,
            in_base: self
                .base
                .iter()
                .any(|item| state_schema::values_equal(item, value)),
        };
        for (index, effect) in self.effects() {
            let item = effect.get("value").ok_or_else(malformed)?;
            if !state_schema::values_equal(item, value) {
                continue;
            }
            match effect.get("operation").and_then(Yaml::as_str) {
                Some("remove-from-set") => {
                    summary.removed = true;
                    summary.added = None;
                }
                Some("add-to-set") if summary.added.is_none() => summary.added = Some(index),
                Some("add-to-set") => {}
                _ => return Err(malformed()),
            }
        }
        // This is a bounded optimization, never a history-dependent refusal.
        // Once full, other tokens are resolved by the same borrowed scan.
        if self.cache.len() < MAX_DETAILS {
            self.cache.insert(token, summary);
        }
        Ok(summary)
    }
}

/// Native set/clear and set-member equality/order, without cloning opaque
/// state or allocating a historical tail. Complete current membership is
/// admitted before result limits, including when hidden members occur last.
fn condition(
    record: &Record,
    definition: &StateDefinition,
    events: &[(&Record, StoryTime)],
    allowed: &BTreeSet<String>,
    budget: &mut Budget,
) -> Result<Option<Json>> {
    if definition.profile.is_some() {
        return Ok(None);
    }
    let effects = || {
        events
            .iter()
            .flat_map(|(event, _)| event.list("effects").into_iter().flatten())
            .filter(|effect| {
                effect.get("target").and_then(Yaml::as_str) == Some(record.id.as_str())
                    && effect.get("key").and_then(Yaml::as_str) == Some("condition")
            })
    };
    let mut base = record.get("initial_state").and_then(|v| v.get("condition"));
    let mut after = None;
    let mut set_operation = false;
    for (index, effect) in effects().enumerate() {
        match effect.get("operation").and_then(Yaml::as_str) {
            Some("set") | Some("clear") => {
                base = effect.get("value");
                after = Some(index);
                set_operation = false;
            }
            Some("add-to-set") | Some("remove-from-set") => set_operation = true,
            _ => return Err(malformed()),
        }
    }
    if !definition.value_type.is_set() {
        let Some(value) = base else {
            return Ok(None);
        };
        if definition.value_type == StateType::Entity && !entity_admitted(value, allowed) {
            return Ok(None);
        }
        return budget.copy(value).map(Some);
    }
    if base.is_none() && !set_operation {
        return Ok(None);
    }
    let initial = base
        .map(|v| v.as_sequence().ok_or_else(malformed))
        .transpose()?
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut replay = SetReplay {
        owner: &record.id,
        base: initial,
        events,
        after,
        entity: definition.value_type == StateType::EntitySet,
        cache: BTreeMap::new(),
    };
    let mut values = Vec::new();
    let mut overflow = false;
    let mut retain = |value| {
        if values.len() < MAX_DETAILS {
            values.push(value);
        } else {
            overflow = true;
        }
    };
    for value in initial {
        if !replay.summary(value)?.removed {
            if replay.entity && !entity_admitted(value, allowed) {
                return Ok(None);
            }
            retain(value);
        }
    }
    for (index, effect) in effects()
        .enumerate()
        .filter(|(index, _)| after.is_none_or(|after| *index > after))
    {
        if effect.get("operation").and_then(Yaml::as_str) != Some("add-to-set") {
            continue;
        }
        let value = effect.get("value").ok_or_else(malformed)?;
        let summary = replay.summary(value)?;
        if summary.added == Some(index) && (summary.removed || !summary.in_base) {
            if replay.entity && !entity_admitted(value, allowed) {
                return Ok(None);
            }
            retain(value);
        }
    }
    if overflow {
        return Err(limit());
    }
    // Count the final sequence itself on every path, including empty sets and
    // no-op histories, before materializing its admitted members.
    budget.details += 1;
    if budget.details > MAX_DETAILS {
        return Err(limit());
    }
    budget.measure(&values)?;
    let values = values
        .into_iter()
        .map(|value| {
            budget.nodes(value, 1)?;
            service::yaml_json(value)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(Json::Array(values)))
}

fn scalar_map(value: &Yaml, budget: &mut Budget) -> Result<(Json, Json)> {
    let fields = value.as_mapping().ok_or_else(malformed)?;
    // Check the whole literal shape before copying any member.
    for (key, value) in fields {
        key.as_str().ok_or_else(malformed)?;
        scalar_type(value)?;
    }
    budget.nodes(value, 0)?;
    budget.measure(value)?;
    let mut types = Map::new();
    for (key, value) in fields {
        types.insert(key.as_str().unwrap().into(), json!(scalar_type(value)?));
    }
    Ok((service::yaml_json(value)?, Json::Object(types)))
}

pub(super) fn context(
    world: &World,
    selected: &Record,
    viewer: &str,
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Json> {
    let allowed = scope.allowed_ids()?;
    let location = selected
        .string("location")
        .filter(|id| allowed.contains(*id));
    let mut participants = BTreeSet::new();
    for item in selected.list("participants").into_iter().flatten() {
        let id = item
            .get("character")
            .and_then(Yaml::as_str)
            .ok_or_else(malformed)?;
        if allowed.contains(id) && scene::present(world, selected, id, at)? {
            participants.insert(id.to_owned());
        }
    }
    let mut state = Objects {
        world,
        events: runtime::canonical_events(world)?
            .into_iter()
            .filter(|(_, point)| point.not_after(at))
            .collect(),
        allowed,
        definitions: [
            definition(world, "holder")?,
            definition(world, "location")?,
            definition(world, "container")?,
            definition(world, "condition")?,
        ],
        participants,
        location,
        placements: BTreeMap::new(),
        presence: BTreeMap::new(),
    };
    let mut budget = Budget::default();
    let mut objects = Vec::new();
    for value in selected.list("objects").into_iter().flatten() {
        let id = value.as_str().ok_or_else(malformed)?;
        if !state.allowed.contains(id) || !state.present(id)? {
            continue;
        }
        budget.row()?;
        let placement = state.placement(id)?;
        let mut values = Map::new();
        let mut types = Map::new();
        for (key, endpoint) in [
            ("holder", placement.holder),
            ("location", placement.location),
            ("container", placement.container),
        ] {
            if let Some(endpoint) = endpoint {
                values.insert(key.into(), json!({"entity":endpoint}));
                types.insert(key.into(), json!("entity"));
            }
        }
        if let Some(definition) = &state.definitions[3]
            && let Some(value) = condition(
                &world.records[id],
                definition,
                &state.events,
                &state.allowed,
                &mut budget,
            )?
        {
            values.insert("condition".into(), value);
            types.insert("condition".into(), json!(state_type(definition.value_type)));
        }
        objects.push(json!({"objectId":id,"state":values,"valueTypes":types}));
    }
    let mut environments = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "environment" && record.status() == "canonical")
    {
        if !narrative::labels_match(record, viewer)? {
            continue;
        }
        let time = record.get("time").ok_or_else(malformed)?;
        let start = StoryTime::from_yaml(
            time.get("start").ok_or_else(malformed)?,
            &world.default_timeline(),
        )?;
        let end = time
            .get("end")
            .filter(|v| !v.is_null())
            .map(|value| StoryTime::from_yaml(value, &world.default_timeline()))
            .transpose()?;
        if !start.not_after(at) || end.as_ref().is_some_and(|end| !at.not_after(end)) {
            continue;
        }
        let targets = record
            .list("targets")
            .into_iter()
            .flatten()
            .filter_map(Yaml::as_str)
            .filter(|id| *id == selected.id || Some(*id) == location)
            .collect::<Vec<_>>();
        if targets.is_empty() {
            continue;
        }
        budget.row()?;
        budget.measure(&start)?;
        budget.measure(&end)?;
        let mut row = json!({"environmentId":record.id,"targets":targets,"time":{"start":start}});
        if time.get("end").is_some() {
            row["time"]["end"] = json!(end);
        }
        if let Some(value) = record.get("conditions") {
            let (values, types) = scalar_map(value, &mut budget)?;
            row["conditions"] = values;
            row["conditionTypes"] = types;
        }
        if let Some(value) = record.get("sensory") {
            row["sensory"] = budget.copy(value)?;
        }
        environments.push(row);
    }
    let mut relationships = Vec::new();
    for record in world.records.values().filter(|record| {
        record.kind == "relationship"
            && record.status() == "canonical"
            && record.string("from") == Some(viewer)
    }) {
        if !narrative::labels_match(record, viewer)? {
            continue;
        }
        let to = record.string("to").ok_or_else(malformed)?;
        if !state.allowed.contains(to) {
            continue;
        }
        let Some((_, current, time)) = runtime::latest_transition(world, record, at)? else {
            continue;
        };
        budget.row()?;
        budget.measure(&time)?;
        let transition = current.get("id").ok_or_else(malformed)?;
        let mut row = json!({"relationshipId":record.id,"toId":to,"time":time,"transitionId":budget.copy(transition)?});
        for (key, output, source) in [
            ("relationship_kind", "relationshipKind", &record.frontmatter),
            ("relationship_status", "status", current),
        ] {
            if let Some(value) = source.get(key) {
                if !matches!(value, Yaml::Null | Yaml::String(_)) {
                    return Err(malformed());
                }
                row[output] = budget.copy(value)?;
            }
        }
        if let Some(value) = current.get("metrics") {
            if value.is_null() {
                row["metrics"] = Json::Null;
            } else {
                let (values, types) = scalar_map(value, &mut budget)?;
                row["metrics"] = values;
                row["metricTypes"] = types;
            }
        }
        if let Some(value) = current.get("facets") {
            if !value.is_null()
                && !value
                    .as_sequence()
                    .is_some_and(|values| values.iter().all(|v| matches!(v, Yaml::String(_))))
            {
                return Err(malformed());
            }
            row["facets"] = budget.copy(value)?;
        }
        relationships.push(row);
    }
    let result =
        json!({"objects":objects,"environments":environments,"relationships":relationships});
    Budget::default().measure(&result)?;
    Ok(result)
}
