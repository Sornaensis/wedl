//! Small core declaration and directed-relationship checks shared by native
//! drafts and complete candidates. None of these checks create reciprocal
//! relationships, fill metric defaults, or infer transitions.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::{self, Diagnostic},
};
use serde_yaml::{Mapping, Number, Value};
use std::collections::BTreeSet;

fn invalid(label: &str, message: impl std::fmt::Display) -> Error {
    Error::Validation(format!("{label}: {message}"))
}
fn text<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    match value {
        Value::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(label, "expected a nonblank string")),
    }
}
fn map<'a>(value: &'a Value, label: &str) -> Result<&'a Mapping> {
    match value {
        Value::Mapping(value) => Ok(value),
        _ => Err(invalid(label, "expected a mapping")),
    }
}
fn list<'a>(value: &'a Value, label: &str) -> Result<&'a [Value]> {
    match value {
        Value::Sequence(value) => Ok(value),
        _ => Err(invalid(label, "expected an array")),
    }
}
fn closed<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
    label: &str,
) -> Result<&'a Mapping> {
    let fields = map(value, label)?;
    for key in fields.keys() {
        let key = text(key, label)?;
        if !required.contains(&key) && !optional.contains(&key) {
            return Err(invalid(label, format!("unknown field {key}")));
        }
    }
    if required.iter().any(|key| !fields.contains_key(*key)) {
        return Err(invalid(label, "missing required field"));
    }
    Ok(fields)
}
fn strings(value: &Value, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in list(value, label)? {
        if !seen.insert(text(value, label)?) {
            return Err(invalid(label, "duplicate string"));
        }
    }
    Ok(())
}
fn literal_strings(value: &Value, label: &str) -> Result<()> {
    for value in list(value, label)? {
        if !matches!(value, Value::String(_)) {
            return Err(invalid(label, "expected literal strings"));
        }
    }
    Ok(())
}
pub(crate) fn finite_number<'a>(value: &'a Value, label: &str) -> Result<&'a Number> {
    match value {
        Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => Ok(number),
        _ => Err(invalid(label, "expected a finite number")),
    }
}
fn metric_definition(value: &Value, label: &str) -> Result<()> {
    let fields = closed(value, &[], &["min", "max", "default"], label)?;
    for key in ["min", "max", "default"] {
        if let Some(value) = fields.get(key) {
            finite_number(value, label)?;
        }
    }
    Ok(())
}
pub(crate) fn validate_metrics_request(value: &Value) -> Result<()> {
    for (name, definition) in map(value, "relationship_metrics")? {
        let name = text(name, "relationship metric name")?;
        metric_definition(definition, &format!("relationship_metrics.{name}"))?;
    }
    Ok(())
}
pub(crate) fn thread_id(value: &str) -> bool {
    value.strip_prefix("thread_").is_some_and(|suffix| {
        suffix.len() == 26
            && suffix
                .bytes()
                .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
    })
}
fn reference(value: &Value, kind: &str, label: &str) -> Result<()> {
    if !validation::valid_id_for_kind(text(value, label)?, kind) {
        return Err(invalid(label, format!("expected an exact {kind} ID")));
    }
    Ok(())
}
fn point(world: &World, value: &Value, label: &str, strict: bool) -> Result<StoryTime> {
    if strict {
        closed(value, &["tick"], &["timeline", "order"], label)?;
    } else {
        map(value, label)?;
    }
    StoryTime::from_yaml(value, &world.default_timeline()).map_err(|error| invalid(label, error))
}
pub(crate) fn validate_relationship_field(world: &World, field: &str, value: &Value) -> Result<()> {
    relationship_field(world, field, value, true)
}
fn relationship_field(world: &World, field: &str, value: &Value, strict: bool) -> Result<()> {
    match field {
        "from" | "to" => reference(value, "character", field)?,
        "inverse" if value.is_null() => {}
        "inverse" => reference(value, "relationship", field)?,
        "relationship_kind" => {
            if strict {
                text(value, field)?;
            }
        }
        "transitions" => {
            let mut ids = BTreeSet::new();
            for (index, value) in list(value, field)?.iter().enumerate() {
                let label = format!("transitions[{index}]");
                let fields = if strict {
                    closed(
                        value,
                        &["id", "time"],
                        &[
                            "relationship_status",
                            "metrics",
                            "facets",
                            "causing_event",
                            "note",
                        ],
                        &label,
                    )?
                } else {
                    // Existing transition extensions remain opaque. The typed
                    // request contract still accepts only its listed fields.
                    let fields = map(value, &label)?;
                    if !fields.contains_key("id") || !fields.contains_key("time") {
                        return Err(invalid(&label, "id and time are required"));
                    }
                    fields
                };
                if !ids.insert(text(&fields["id"], &format!("{label}.id"))?) {
                    return Err(invalid(&label, "duplicate transition ID"));
                }
                point(world, &fields["time"], &format!("{label}.time"), strict)?;
                if let Some(value) = fields.get("relationship_status").filter(|_| strict) {
                    text(value, &format!("{label}.relationship_status"))?;
                }
                if let Some(value) = fields.get("metrics").filter(|_| strict) {
                    for (name, value) in map(value, &format!("{label}.metrics"))? {
                        let name = text(name, "metric name")?;
                        finite_number(value, &format!("{label}.metrics.{name}"))?;
                    }
                }
                if let Some(value) = fields.get("facets").filter(|_| strict) {
                    literal_strings(value, &format!("{label}.facets"))?;
                }
                if let Some(value) = fields.get("causing_event").filter(|v| !v.is_null()) {
                    reference(value, "event", &format!("{label}.causing_event"))?;
                }
                if strict
                    && fields
                        .get("note")
                        .is_some_and(|v| !matches!(v, Value::String(_)))
                {
                    return Err(invalid(&format!("{label}.note"), "expected a string"));
                }
            }
        }
        _ => return Err(invalid(field, "unknown relationship field")),
    }
    Ok(())
}
fn resolved<'a>(world: &'a World, value: &Value, kind: &str, label: &str) -> Result<&'a Record> {
    reference(value, kind, label)?;
    world
        .records
        .get(value.as_str().unwrap())
        .filter(|record| record.kind == kind)
        .ok_or_else(|| invalid(label, format!("expected an existing {kind}")))
}
fn relationship(world: &World, record: &Record) -> Result<()> {
    for field in ["from", "to"] {
        let value = record
            .get(field)
            .ok_or_else(|| invalid(field, "required"))?;
        resolved(world, value, "character", field)?;
    }
    for field in ["relationship_kind", "inverse", "transitions"] {
        if let Some(value) = record.get(field) {
            relationship_field(world, field, value, false)?;
        }
    }
    if let Some(value) = record.get("inverse").filter(|value| !value.is_null()) {
        resolved(world, value, "relationship", "inverse")?;
    }
    let registry = world.world_record()?.get("relationship_metrics");
    for (index, transition) in record.list("transitions").into_iter().flatten().enumerate() {
        let label = format!("transitions[{index}]");
        let at = point(world, &transition["time"], &format!("{label}.time"), false)?;
        if let Some(cause) = transition.get("causing_event").filter(|v| !v.is_null()) {
            let cause = resolved(world, cause, "event", &format!("{label}.causing_event"))?;
            let time = cause
                .get("time")
                .ok_or_else(|| invalid(&label, "cause has no time"))?;
            if cause.status() != "canonical"
                || !point(world, time, "cause.time", false)?.not_after(&at)
            {
                return Err(invalid(
                    &format!("{label}.causing_event"),
                    "expected a canonical event at or before the transition on the same timeline",
                ));
            }
        }
        if let Some(Value::Mapping(values)) = transition.get("metrics") {
            for (name, _) in values {
                let name = text(name, "metric name")?;
                registry
                    .and_then(|registry| registry.get(name))
                    .ok_or_else(|| {
                        invalid(
                            &format!("{label}.metrics.{name}"),
                            "metric is not declared by the world",
                        )
                    })?;
            }
        }
    }
    Ok(())
}
fn common(record: &Record) -> Result<()> {
    for field in ["audience", "perspectives"] {
        if let Some(value) = record.get(field) {
            if record.kind == "knowledge" {
                strings(value, field)?;
            } else {
                literal_strings(value, field)?;
            }
        }
    }
    if record.kind == "world" {
        if let Some(value) = record.get("default_timeline") {
            text(value, "default_timeline")?;
        }
        if let Some(values) = record.list("timelines") {
            for declaration in values {
                if let Some(origin) = declaration.get("origin") {
                    let origin = closed(origin, &["tick", "label"], &[], "timeline origin")?;
                    text(&origin["label"], "timeline origin.label")?;
                    let tick = match &origin["tick"] {
                        Value::Number(number) if !number.is_f64() => number.as_i64(),
                        _ => None,
                    }
                    .ok_or_else(|| {
                        invalid("timeline origin.tick", "expected a signed 64-bit integer")
                    })?;
                    StoryTime::new("main", tick, 0)?;
                }
            }
        }
        if let Some(value) = record.get("threads") {
            for declaration in list(value, "threads")? {
                closed(declaration, &["id", "label"], &[], "thread declaration")?;
            }
        }
        // Existing raw metric policies remain opaque to source validation.
        // Relationship uses below require their declared names, while selected
        // semantic checks retain their established unsupported-policy result.
    }
    Ok(())
}
pub(crate) fn validate(world: &World) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for record in world.records.values() {
        for (code, result) in [
            ("WDL-CORE-FIELDS", common(record)),
            (
                "WDL-RELATIONSHIP-001",
                if record.kind == "relationship" {
                    relationship(world, record)
                } else {
                    Ok(())
                },
            ),
        ] {
            if let Err(error) = result {
                out.push(Diagnostic {
                    code: code.into(),
                    message: error.to_string(),
                    severity: "error".into(),
                    entity_id: Some(record.id.clone()),
                    path: Some(record.source_path.clone()),
                    field: Some(
                        if record.kind == "relationship" {
                            "transitions"
                        } else {
                            "frontmatter"
                        }
                        .into(),
                    ),
                });
            }
        }
    }
    out
}
