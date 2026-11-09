//! Closed authored profile types. Values describe lore; they run no defaults,
//! expressions, coercion, inference or reference rewriting.
use crate::{
    Error, Result,
    model::World,
    validation::{is_record_kind, valid_id_for_kind},
};
use serde_yaml::{Mapping, Number, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DEPTH: usize = 16;
pub const MAX_NODES: usize = 16_384;
pub const MAX_KEYS: usize = 256;
pub const MAX_ARRAY: usize = 1024;

fn invalid(label: &str, message: &str) -> Error {
    Error::Validation(format!("{label}: {message}"))
}
fn mapping<'a>(value: &'a Yaml, label: &str) -> Result<&'a Mapping> {
    match value {
        Yaml::Mapping(value) => Ok(value),
        _ => Err(invalid(label, "must be an untagged mapping")),
    }
}
fn closed<'a>(value: &'a Yaml, keys: &[&str], label: &str) -> Result<&'a Mapping> {
    let fields = mapping(value, label)?;
    if fields.len() != keys.len()
        || fields
            .keys()
            .any(|key| !matches!(key,Yaml::String(key) if keys.contains(&key.as_str())))
    {
        return Err(invalid(label, "has missing or undeclared fields"));
    }
    Ok(fields)
}
fn text<'a>(value: &'a Yaml, label: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) => Ok(value),
        _ => Err(invalid(label, "must be an untagged string")),
    }
}
/// Profile identifiers and relation tokens have a closed ASCII spelling.
pub fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
fn field_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Bound every occurrence, including keys, before recursive typed processing.
/// serde_yaml values own their children, so aliases are already expanded trees.
pub fn bounded(value: &Yaml, label: &str) -> Result<()> {
    let mut pending = vec![(value, 1usize)];
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > MAX_NODES || depth > MAX_DEPTH {
            return Err(invalid(label, "exceeds the 16384-node or 16-level budget"));
        }
        match value {
            Yaml::Mapping(values) => {
                if values.len() > MAX_KEYS {
                    return Err(invalid(label, "an object exceeds 256 keys"));
                }
                for (key, value) in values {
                    if !matches!(key, Yaml::String(_)) {
                        return Err(invalid(label, "object keys must be strings"));
                    }
                    pending.push((key, depth + 1));
                    pending.push((value, depth + 1));
                }
            }
            Yaml::Sequence(values) => {
                if values.len() > MAX_ARRAY {
                    return Err(invalid(label, "an array exceeds 1024 items"));
                }
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Yaml::Number(value) => {
                if !value.as_f64().is_some_and(f64::is_finite) {
                    return Err(invalid(label, "numbers must be finite"));
                }
            }
            Yaml::Tagged(_) => return Err(invalid(label, "tagged values are not profile data")),
            Yaml::Null | Yaml::Bool(_) | Yaml::String(_) => {}
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub enum TypeSpec {
    String,
    Boolean,
    Number,
    Null,
    Entity {
        kinds: Vec<String>,
    },
    Enum {
        values: Vec<Yaml>,
    },
    Array {
        items: Box<TypeSpec>,
        max_items: usize,
    },
    Object {
        required: BTreeMap<String, TypeSpec>,
        optional: BTreeMap<String, TypeSpec>,
    },
}

// Comparing an integer through f64 would merge distinct source integers above
// 2^53. i128 represents every integer supported by the YAML number codec, so an
// integral float can be compared exactly without rounding the integer operand.
fn integer(value: &Number) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}
fn number_equal(left: &Number, right: &Number) -> bool {
    match (integer(left), integer(right)) {
        (Some(left), Some(right)) => left == right,
        (Some(integer), None) => right
            .as_f64()
            .is_some_and(|number| number.fract() == 0.0 && number as i128 == integer),
        (None, Some(integer)) => left
            .as_f64()
            .is_some_and(|number| number.fract() == 0.0 && number as i128 == integer),
        (None, None) => left.as_f64() == right.as_f64(),
    }
}
fn scalar_equal(left: &Yaml, right: &Yaml) -> bool {
    match (left, right) {
        (Yaml::Null, Yaml::Null) => true,
        (Yaml::Bool(left), Yaml::Bool(right)) => left == right,
        (Yaml::Number(left), Yaml::Number(right)) => number_equal(left, right),
        (Yaml::String(left), Yaml::String(right)) => left == right,
        _ => false,
    }
}
fn build_type(value: &Yaml, label: &str) -> Result<TypeSpec> {
    let fields = mapping(value, label)?;
    let name = fields
        .get("type")
        .ok_or_else(|| invalid(label, "requires type"))?;
    match text(name, label)? {
        "string" | "boolean" | "number" | "null" => {
            closed(value, &["type"], label)?;
            Ok(match name.as_str().unwrap() {
                "string" => TypeSpec::String,
                "boolean" => TypeSpec::Boolean,
                "number" => TypeSpec::Number,
                _ => TypeSpec::Null,
            })
        }
        "entity" => {
            let fields = closed(value, &["type", "kinds"], label)?;
            let Yaml::Sequence(values) = &fields["kinds"] else {
                return Err(invalid(
                    label,
                    "entity kinds must be a nonempty unique array",
                ));
            };
            if values.is_empty() {
                return Err(invalid(label, "entity kinds must be nonempty"));
            }
            let mut seen = BTreeSet::new();
            let mut kinds = Vec::new();
            for value in values {
                let kind = text(value, label)?;
                if kind == "hypothesis" || !is_record_kind(kind) || !seen.insert(kind) {
                    return Err(invalid(
                        label,
                        "entity kinds must be unique known canonical kinds",
                    ));
                }
                kinds.push(kind.to_owned());
            }
            Ok(TypeSpec::Entity { kinds })
        }
        "enum" => {
            let fields = closed(value, &["type", "values"], label)?;
            let Yaml::Sequence(values) = &fields["values"] else {
                return Err(invalid(
                    label,
                    "enum values must be a nonempty scalar array",
                ));
            };
            if values.is_empty() {
                return Err(invalid(label, "enum values must be nonempty"));
            }
            for (index, value) in values.iter().enumerate() {
                if !matches!(
                    value,
                    Yaml::Null | Yaml::Bool(_) | Yaml::Number(_) | Yaml::String(_)
                ) {
                    return Err(invalid(label, "enum alternatives must be scalar data"));
                }
                if values[..index]
                    .iter()
                    .any(|other| scalar_equal(value, other))
                {
                    return Err(invalid(
                        label,
                        "enum alternatives must be unique by type and value",
                    ));
                }
            }
            Ok(TypeSpec::Enum {
                values: values.clone(),
            })
        }
        "array" => {
            let fields = closed(value, &["type", "items", "max_items"], label)?;
            let max_items = match &fields["max_items"] {
                Yaml::Number(value) => value.as_u64(),
                _ => None,
            }
            .filter(|value| *value <= MAX_ARRAY as u64)
            .ok_or_else(|| invalid(label, "max_items must be an integer from 0 to 1024"))?;
            Ok(TypeSpec::Array {
                items: Box::new(build_type(&fields["items"], &format!("{label}.items"))?),
                max_items: max_items as usize,
            })
        }
        "object" => {
            let fields = closed(value, &["type", "required", "optional"], label)?;
            let required = mapping(&fields["required"], &format!("{label}.required"))?;
            let optional = mapping(&fields["optional"], &format!("{label}.optional"))?;
            if required.keys().any(|key| optional.contains_key(key)) {
                return Err(invalid(
                    label,
                    "required and optional fields must be disjoint",
                ));
            }
            let build_fields =
                |values: &Mapping, group: &str| -> Result<BTreeMap<String, TypeSpec>> {
                    let mut result = BTreeMap::new();
                    for (key, value) in values {
                        let key = text(key, label)?;
                        if !field_token(key) {
                            return Err(invalid(
                                label,
                                "object field names must be lowercase field tokens",
                            ));
                        }
                        result.insert(
                            key.to_owned(),
                            build_type(value, &format!("{label}.{group}.{key}"))?,
                        );
                    }
                    Ok(result)
                };
            Ok(TypeSpec::Object {
                required: build_fields(required, "required")?,
                optional: build_fields(optional, "optional")?,
            })
        }
        _ => Err(invalid(label, "unsupported profile type")),
    }
}
/// Validate a bounded, closed type declaration without executing it.
pub fn validate_type(value: &Yaml) -> Result<TypeSpec> {
    bounded(value, "type")?;
    build_type(value, "type")
}

impl TypeSpec {
    /// Parse a closed finite type independently of an object-root profile.
    pub(crate) fn parse(value: &Yaml, label: &str) -> Result<Self> {
        bounded(value, label)?;
        build_type(value, label)
    }

    fn walk(
        &self,
        value: &Yaml,
        world: &World,
        label: &str,
        references: &mut Vec<(String, String)>,
    ) -> Result<()> {
        match self {
            Self::String if matches!(value, Yaml::String(_)) => Ok(()),
            Self::Boolean if matches!(value, Yaml::Bool(_)) => Ok(()),
            Self::Number if matches!(value, Yaml::Number(_)) => Ok(()),
            Self::Null if matches!(value, Yaml::Null) => Ok(()),
            Self::Entity { kinds } => {
                let fields = closed(value, &["kind", "id"], label)?;
                if text(&fields["kind"], label)? != "record" {
                    return Err(invalid(label, "entity value must be a record reference"));
                }
                let id = text(&fields["id"], label)?;
                let record = world
                    .records
                    .get(id)
                    .ok_or_else(|| invalid(label, "entity reference is absent from the source"))?;
                if !kinds.contains(&record.kind) || !valid_id_for_kind(id, &record.kind) {
                    return Err(invalid(
                        label,
                        "entity reference must resolve to an exact declared kind",
                    ));
                }
                references.push((format!("{label}.id"), id.to_owned()));
                Ok(())
            }
            Self::Enum { values } => {
                if values
                    .iter()
                    .any(|candidate| scalar_equal(value, candidate))
                {
                    Ok(())
                } else {
                    Err(invalid(label, "value is not a declared enum alternative"))
                }
            }
            Self::Array { items, max_items } => {
                let Yaml::Sequence(values) = value else {
                    return Err(invalid(label, "value must be an array"));
                };
                if values.len() > *max_items {
                    return Err(invalid(label, "array exceeds its declared max_items"));
                }
                for (index, value) in values.iter().enumerate() {
                    items.walk(value, world, &format!("{label}[{index}]"), references)?;
                }
                Ok(())
            }
            Self::Object { required, optional } => {
                let fields = mapping(value, label)?;
                if required.keys().any(|key| !fields.contains_key(key)) {
                    return Err(invalid(label, "required object field is missing"));
                }
                for (key, value) in fields {
                    let key = text(key, label)?;
                    let spec = required
                        .get(key)
                        .or_else(|| optional.get(key))
                        .ok_or_else(|| invalid(label, "undeclared object field"))?;
                    spec.walk(value, world, &format!("{label}.{key}"), references)?;
                }
                Ok(())
            }
            _ => Err(invalid(label, "value does not match its declared type")),
        }
    }
    pub fn validate_value(&self, value: &Yaml, world: &World) -> Result<()> {
        self.references(value, world).map(|_| ())
    }
    pub fn references(&self, value: &Yaml, world: &World) -> Result<Vec<(String, String)>> {
        bounded(value, "values")?;
        let mut references = Vec::new();
        self.walk(value, world, "values", &mut references)?;
        Ok(references)
    }
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: String,
    pub label: String,
    pub fields: TypeSpec,
    /// The authored declaration retained for the explicit author catalogue.
    pub declaration: Yaml,
}
impl Profile {
    pub fn validate_value(&self, value: &Yaml, world: &World) -> Result<()> {
        self.fields.validate_value(value, world)
    }
    pub fn references(&self, value: &Yaml, world: &World) -> Result<Vec<(String, String)>> {
        self.fields.references(value, world)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub profiles: BTreeMap<String, Profile>,
}
impl Registry {
    pub fn parse(value: &Yaml) -> Result<Self> {
        bounded(value, "profiles")?;
        let Yaml::Sequence(values) = value else {
            return Err(invalid("profiles", "must be an array"));
        };
        let mut profiles = BTreeMap::new();
        for (index, value) in values.iter().enumerate() {
            let label = format!("profiles[{index}]");
            let fields = closed(value, &["id", "label", "fields"], &label)?;
            let id = text(&fields["id"], &label)?;
            if !token(id) {
                return Err(invalid(&label, "id must be a lowercase profile token"));
            }
            let name = text(&fields["label"], &label)?;
            if name.trim().is_empty() || name.chars().count() > 256 {
                return Err(invalid(
                    &label,
                    "label must be nonblank and at most 256 characters",
                ));
            }
            if profiles.contains_key(id) {
                return Err(invalid(&label, "profile id must be unique"));
            }
            let spec = build_type(&fields["fields"], &format!("{label}.fields"))?;
            if !matches!(spec, TypeSpec::Object { .. }) {
                return Err(invalid(&label, "profile fields must have an object root"));
            }
            profiles.insert(
                id.to_owned(),
                Profile {
                    id: id.to_owned(),
                    label: name.to_owned(),
                    fields: spec,
                    declaration: value.clone(),
                },
            );
        }
        Ok(Self { profiles })
    }
    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.get(id)
    }
}
