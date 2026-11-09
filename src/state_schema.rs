//! Compact, closed state declarations and the operations that use them.
use crate::{
    Error, Result,
    model::{Record, World},
};
use serde_yaml::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateType {
    String,
    Boolean,
    Integer,
    Number,
    Entity,
    StringSet,
    EntitySet,
}

impl StateType {
    pub fn is_set(self) -> bool {
        matches!(self, Self::StringSet | Self::EntitySet)
    }
}

#[derive(Clone, Debug)]
pub struct StateDefinition {
    pub value_type: StateType,
    pub kind: Option<String>,
    pub exclusive_group: Option<String>,
    /// An explicit binding replaces the ordinary leaf contract, but retains
    /// the declared key and its exclusivity group.
    pub profile: Option<crate::lore_types::Profile>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

fn literal_string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    match value {
        Value::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!("{field} must be a nonempty string"))),
    }
}

/// Declarations have exactly `type`, optional entity `kind`, and optional
/// `exclusive_group`. Legacy names and unknown fields are errors.
pub fn validate_definition(value: &Value) -> Result<StateDefinition> {
    let Value::Mapping(mapping) = value else {
        return Err(invalid("state declaration must be a mapping"));
    };
    for key in mapping.keys() {
        let key = literal_string(key, "state declaration key")?;
        if !["type", "kind", "exclusive_group"].contains(&key) {
            return Err(invalid(format!("unknown state declaration field {key:?}")));
        }
    }
    let type_name = mapping
        .get("type")
        .ok_or_else(|| invalid("state declaration requires type"))?;
    let value_type = match literal_string(type_name, "state type")? {
        "string" => StateType::String,
        "boolean" => StateType::Boolean,
        "integer" => StateType::Integer,
        "number" => StateType::Number,
        "entity" => StateType::Entity,
        "string-set" => StateType::StringSet,
        "entity-set" => StateType::EntitySet,
        name => return Err(invalid(format!("unknown state type {name:?}"))),
    };
    let kind = mapping
        .get("kind")
        .map(|value| literal_string(value, "kind").map(str::to_owned))
        .transpose()?;
    if let Some(kind) = &kind {
        if !matches!(value_type, StateType::Entity | StateType::EntitySet) {
            return Err(invalid("kind is only valid for entity or entity-set state"));
        }
        if !crate::validation::is_record_kind(kind) {
            return Err(invalid(format!("unknown entity kind {kind:?}")));
        }
    }
    let exclusive_group = mapping
        .get("exclusive_group")
        .map(|value| literal_string(value, "exclusive_group").map(str::to_owned))
        .transpose()?;
    Ok(StateDefinition {
        value_type,
        kind,
        exclusive_group,
        profile: None,
    })
}

pub fn field_definition(world: &World, kind: &str, key: &str) -> Result<StateDefinition> {
    let definitions = world
        .world_record()?
        .get("state_keys")
        .ok_or_else(|| invalid(format!("undeclared state key {kind}.{key}")))?;
    let Value::Mapping(definitions) = definitions else {
        return Err(invalid("world.state_keys must be a mapping"));
    };
    let fields = definitions
        .get(kind)
        .ok_or_else(|| invalid(format!("undeclared state key {kind}.{key}")))?;
    let Value::Mapping(fields) = fields else {
        return Err(invalid(format!("state_keys.{kind} must be a mapping")));
    };
    let declaration = fields
        .get(key)
        .ok_or_else(|| invalid(format!("undeclared state key {kind}.{key}")))?;
    let mut definition = validate_definition(declaration)?;
    definition.profile = crate::lore_state::bound_profile(world, kind, key)?;
    Ok(definition)
}

fn validate_entity(world: &World, definition: &StateDefinition, value: &Value) -> Result<()> {
    let Value::Mapping(mapping) = value else {
        return Err(invalid("entity state must be exactly {entity: ID}"));
    };
    if mapping.len() != 1 || !mapping.contains_key("entity") {
        return Err(invalid("entity state must be exactly {entity: ID}"));
    }
    let id = literal_string(mapping.get("entity").expect("checked key"), "entity")?;
    let record = world
        .records
        .get(id)
        .ok_or_else(|| invalid(format!("unknown state entity {id:?}")))?;
    if definition
        .kind
        .as_ref()
        .is_some_and(|kind| *kind != record.kind)
    {
        return Err(invalid(format!(
            "state entity {id:?} is {}, expected {}",
            record.kind,
            definition.kind.as_deref().unwrap()
        )));
    }
    Ok(())
}

/// Scalar items accepted by add-to-set/remove-from-set. No coercion or aliases.
pub fn validate_item(world: &World, definition: &StateDefinition, value: &Value) -> Result<()> {
    if definition.profile.is_some() {
        return Err(invalid(
            "add-to-set and remove-from-set cannot target an object-profile-bound key",
        ));
    }
    match definition.value_type {
        StateType::StringSet if matches!(value, Value::String(_)) => Ok(()),
        StateType::StringSet => Err(invalid("string-set item must be a string")),
        StateType::EntitySet => validate_entity(world, definition, value),
        _ => Err(invalid(
            "add-to-set and remove-from-set require a declared set type",
        )),
    }
}

/// Whole values are validated before a state becomes visible or is replayed.
pub fn validate_value(world: &World, definition: &StateDefinition, value: &Value) -> Result<()> {
    if let Some(profile) = &definition.profile {
        return profile.validate_value(value, world);
    }
    match definition.value_type {
        StateType::String if matches!(value, Value::String(_)) => Ok(()),
        StateType::Boolean if matches!(value, Value::Bool(_)) => Ok(()),
        StateType::Integer if matches!(value, Value::Number(number) if number.as_i64().is_some() && !number.is_f64()) => {
            Ok(())
        }
        StateType::Number if matches!(value, Value::Number(number) if number.as_f64().is_some_and(f64::is_finite)) => {
            Ok(())
        }
        StateType::Entity => validate_entity(world, definition, value),
        StateType::StringSet | StateType::EntitySet => {
            let Value::Sequence(values) = value else {
                return Err(invalid("set state must be an array"));
            };
            for (index, value) in values.iter().enumerate() {
                validate_item(world, definition, value)?;
                if values[..index]
                    .iter()
                    .any(|other| values_equal(other, value))
                {
                    return Err(invalid(format!(
                        "set state contains a duplicate item at index {index}"
                    )));
                }
            }
            Ok(())
        }
        StateType::String => Err(invalid("state value must be a string")),
        StateType::Boolean => Err(invalid("state value must be a boolean")),
        StateType::Integer => Err(invalid(
            "state value must be an exact signed 64-bit integer",
        )),
        StateType::Number => Err(invalid("state value must be a finite number")),
    }
}

/// Exact typed structural equality. In particular, true, 1 and 1.0 differ.
pub fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a.is_f64() == b.is_f64() && a == b,
        (Value::Sequence(a), Value::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| values_equal(a, b))
        }
        (Value::Mapping(a), Value::Mapping(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.iter().any(|(other_key, other_value)| {
                        values_equal(key, other_key) && values_equal(value, other_value)
                    })
                })
        }
        _ => false,
    }
}

/// Sets compare by membership; their stored array order is otherwise retained.
pub fn values_equal_for_definition(
    definition: &StateDefinition,
    left: &Value,
    right: &Value,
) -> bool {
    if definition.profile.is_none() && definition.value_type.is_set() {
        match (left, right) {
            (Value::Sequence(a), Value::Sequence(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|item| b.iter().any(|other| values_equal(item, other)))
            }
            _ => false,
        }
    } else {
        values_equal(left, right)
    }
}

pub fn initial_state(world: &World, record: &Record) -> Result<BTreeMap<String, Value>> {
    let mut state = BTreeMap::new();
    if let Some(value) = record.get("initial_state") {
        let Value::Mapping(values) = value else {
            return Err(invalid("initial_state must be a mapping"));
        };
        for (key, value) in values {
            let key = literal_string(key, "initial_state key")?;
            let definition = field_definition(world, &record.kind, key)?;
            validate_value(world, &definition, value)?;
            state.insert(key.into(), value.clone());
        }
    }
    ensure_exclusive(world, record, &state)?;
    Ok(state)
}

pub fn ensure_exclusive(
    world: &World,
    record: &Record,
    state: &BTreeMap<String, Value>,
) -> Result<()> {
    let mut groups: BTreeMap<String, &str> = BTreeMap::new();
    for key in state.keys() {
        if let Some(group) = field_definition(world, &record.kind, key)?.exclusive_group {
            if let Some(other) = groups.insert(group.clone(), key) {
                return Err(invalid(format!(
                    "exclusive state group {group:?} has both {other:?} and {key:?}"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Operation {
    Set,
    Clear,
    Add,
    Remove,
}

fn checked_effect<'a>(
    world: &World,
    record: &Record,
    effect: &'a Value,
) -> Result<(&'a str, Operation, StateDefinition, Option<&'a Value>)> {
    let Value::Mapping(effect) = effect else {
        return Err(invalid("effect must be a mapping"));
    };
    let target = effect
        .get("target")
        .ok_or_else(|| invalid("effect requires target"))?;
    if literal_string(target, "effect.target")? != record.id {
        return Err(invalid("effect target differs from state owner"));
    }
    let key = literal_string(
        effect
            .get("key")
            .ok_or_else(|| invalid("effect requires key"))?,
        "effect.key",
    )?;
    let operation = literal_string(
        effect
            .get("operation")
            .ok_or_else(|| invalid("effect requires operation"))?,
        "effect.operation",
    )?;
    let definition = field_definition(world, &record.kind, key)?;
    let value = effect.get("value");
    let operation = match operation {
        "set" => {
            validate_value(
                world,
                &definition,
                value.ok_or_else(|| invalid("set requires value"))?,
            )?;
            Operation::Set
        }
        "clear" => {
            if value.is_some() {
                return Err(invalid("clear forbids a value field"));
            }
            Operation::Clear
        }
        "add-to-set" | "remove-from-set" => {
            validate_item(
                world,
                &definition,
                value.ok_or_else(|| invalid("set operation requires value"))?,
            )?;
            if operation == "add-to-set" {
                Operation::Add
            } else {
                Operation::Remove
            }
        }
        other => {
            return Err(Error::Unsupported(format!(
                "unsupported effect operation {other:?}"
            )));
        }
    };
    Ok((key, operation, definition, value))
}

pub fn validate_effect(world: &World, record: &Record, effect: &Value) -> Result<()> {
    checked_effect(world, record, effect).map(|_| ())
}

/// Each effect is validated and applied to a private candidate before replacing
/// the current state. Failure leaves the supplied state unchanged.
pub fn apply_effect(
    world: &World,
    record: &Record,
    state: &mut BTreeMap<String, Value>,
    effect: &Value,
) -> Result<()> {
    let (key, operation, definition, value) = checked_effect(world, record, effect)?;
    if let Some(current) = state.get(key) {
        validate_value(world, &definition, current)?;
    }
    let mut candidate = state.clone();
    match operation {
        Operation::Set => {
            candidate.insert(key.into(), value.expect("validated value").clone());
        }
        Operation::Clear => {
            candidate.remove(key);
        }
        Operation::Add | Operation::Remove => {
            let entry = candidate
                .entry(key.into())
                .or_insert_with(|| Value::Sequence(Vec::new()));
            let values = entry
                .as_sequence_mut()
                .ok_or_else(|| invalid("set state must be an array"))?;
            let value = value.expect("validated item");
            match operation {
                Operation::Add if !values.iter().any(|item| values_equal(item, value)) => {
                    values.push(value.clone())
                }
                Operation::Remove => values.retain(|item| !values_equal(item, value)),
                _ => {}
            }
        }
    }
    ensure_exclusive(world, record, &candidate)?;
    *state = candidate;
    Ok(())
}

/// Integer fields use decimal strings in JSON, preserving all i64 bits in a
/// browser. Other field types retain their ordinary JSON representation.
pub fn value_to_json(definition: &StateDefinition, value: &Value) -> Result<serde_json::Value> {
    if definition.profile.is_some() {
        crate::service::yaml_json(value)
    } else if definition.value_type == StateType::Integer {
        match value {
            Value::Number(number) if !number.is_f64() => number
                .as_i64()
                .map(|value| serde_json::Value::String(value.to_string()))
                .ok_or_else(|| invalid("state value must be an exact signed 64-bit integer")),
            _ => Err(invalid(
                "state value must be an exact signed 64-bit integer",
            )),
        }
    } else {
        Ok(serde_json::to_value(value)?)
    }
}
