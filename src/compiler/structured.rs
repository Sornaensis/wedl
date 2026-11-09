//! Independently dated author search documents from supported core fields.
//! The caller admits the owner first; member times precede text allocation.
use crate::{
    Error, Result,
    lore_state::Bindings,
    model::{Record, StoryTime, World},
    retrieval,
};
use serde_yaml::Value as Yaml;

pub(crate) struct Structured<'a> {
    pub kind: &'static str,
    pub path: String,
    pub member_id: Option<&'a str>,
    pub at: Option<StoryTime>,
    pub until: Option<StoryTime>,
    pub text: String,
}

/// Do not serialize an arbitrary frontmatter object. Only explicitly selected
/// fields reach this builder, and every borrowed string is charged before copy.
#[derive(Default)]
struct Text(String);

impl Text {
    fn push(&mut self, value: &str) -> Result<()> {
        if value.is_empty() {
            return Ok(());
        }
        let separator = usize::from(!self.0.is_empty());
        let size = self
            .0
            .len()
            .checked_add(separator)
            .and_then(|size| size.checked_add(value.len()))
            .ok_or_else(retrieval::bounds_error)?;
        if size > retrieval::MAX_BYTES {
            return Err(retrieval::bounds_error());
        }
        if separator != 0 {
            self.0.push('\n');
        }
        self.0.push_str(value);
        Ok(())
    }

    fn value(&mut self, value: &Yaml) -> Result<()> {
        match value {
            Yaml::Null => self.push("null"),
            Yaml::Bool(value) => self.push(if *value { "true" } else { "false" }),
            Yaml::Number(value) => self.push(&value.to_string()),
            Yaml::String(value) => self.push(value),
            Yaml::Sequence(values) => {
                for value in values {
                    self.value(value)?;
                }
                Ok(())
            }
            Yaml::Mapping(values) => {
                // Typed conditions/entity values have string keys. Existing
                // raw relationship metrics can retain other literal YAML keys;
                // searching them must not tighten the source validator.
                for (key, value) in values {
                    self.value(key)?;
                    self.value(value)?;
                }
                Ok(())
            }
            Yaml::Tagged(value) => {
                self.push(&value.tag.to_string())?;
                self.value(&value.value)
            }
        }
    }

    fn fields(&mut self, value: &Yaml, fields: &[&str]) -> Result<()> {
        for field in fields {
            // Null optional prose/status/containers means absent. Null inside
            // a selected condition or metric value remains literal data.
            if let Some(value) = value.get(*field).filter(|value| !value.is_null()) {
                self.value(value)?;
            }
        }
        Ok(())
    }
}

fn malformed() -> Error {
    Error::Validation("invalid structured author search source".into())
}

fn point(world: &World, value: Option<&Yaml>) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(value.ok_or_else(malformed)?, &world.default_timeline())?;
    world.story_time(point.tick, Some(&point.timeline), point.order)
}

fn until(world: &World, value: Option<&Yaml>) -> Result<Option<StoryTime>> {
    value
        .filter(|value| !value.is_null())
        .map(|value| point(world, Some(value)))
        .transpose()
}

fn list(value: Option<&Yaml>) -> &[Yaml] {
    value
        .and_then(Yaml::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn member<'a>(
    kind: &'static str,
    path: String,
    value: &'a Yaml,
    at: StoryTime,
    until: Option<StoryTime>,
    text: Text,
) -> Structured<'a> {
    Structured {
        kind,
        path,
        member_id: value.get("id").and_then(Yaml::as_str),
        at: Some(at),
        until,
        text: text.0,
    }
}

/// Only ordinary core kinds call this function. Typed lore/assertions/history
/// and linked assertion knowledge retain their separate domain projections.
pub(crate) fn collect<'a>(
    world: &World,
    record: &'a Record,
    at: &StoryTime,
    bindings: &Bindings,
    emit: impl FnMut(Structured<'a>) -> Result<()>,
) -> Result<()> {
    collect_scoped(
        world,
        record,
        Some(at),
        &at.timeline,
        bindings,
        |_| Ok(true),
        emit,
    )
}

/// Internal source selector for admitted catalogue documents. A missing horizon
/// includes every authored position on the one explicit timeline. The caller's
/// effect gate runs before any target/value text is allocated or charged.
pub(crate) fn collect_scoped<'a>(
    world: &World,
    record: &'a Record,
    at: Option<&StoryTime>,
    timeline: &str,
    bindings: &Bindings,
    mut admit_effect: impl FnMut(&Yaml) -> Result<bool>,
    mut emit: impl FnMut(Structured<'a>) -> Result<()>,
) -> Result<()> {
    let started =
        |point: &StoryTime| point.timeline == timeline && at.is_none_or(|at| point.not_after(at));
    let active =
        |end: Option<&StoryTime>| at.is_none_or(|at| end.is_none_or(|end| at.not_after(end)));
    if record.kind == "environment" {
        let interval = record.get("time").ok_or_else(malformed)?;
        let start = point(world, interval.get("start"))?;
        let end = until(world, interval.get("end"))?;
        if started(&start) && active(end.as_ref()) {
            let mut text = Text::default();
            text.fields(&record.frontmatter, &["conditions", "sensory"])?;
            if !text.0.is_empty() {
                emit(Structured {
                    kind: "environment",
                    path: "conditions/sensory".into(),
                    member_id: None,
                    at: Some(start),
                    until: end,
                    text: text.0,
                })?;
            }
        }
    }

    if matches!(
        record.kind.as_str(),
        "knowledge" | "relationship" | "story-point"
    ) {
        let (transitions, path) = if record.kind == "story-point" {
            (
                list(
                    record
                        .get("lifecycle")
                        .and_then(|value| value.get("transitions")),
                ),
                "lifecycle.transitions",
            )
        } else {
            (list(record.get("transitions")), "transitions")
        };
        for (index, transition) in transitions.iter().enumerate() {
            let time = point(world, transition.get("time"))?;
            if !started(&time) {
                continue;
            }
            let mut text = Text::default();
            text.fields(
                transition,
                &["state", "relationship_status", "metrics", "facets", "note"],
            )?;
            if !text.0.is_empty() {
                emit(member(
                    "transition",
                    format!("{path}[{index}]"),
                    transition,
                    time,
                    None,
                    text,
                ))?;
            }
        }
    }

    if record.kind == "event" {
        let time = point(world, record.get("time"))?;
        if started(&time) {
            for (index, effect) in list(record.get("effects")).iter().enumerate() {
                if !admit_effect(effect)? {
                    continue;
                }
                let target_id = effect
                    .get("target")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?;
                let target = world.records.get(target_id).ok_or_else(malformed)?;
                let key = effect
                    .get("key")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?;
                // Ordinary retrieval has no typed-value scope. A bound value
                // must never affect tokens, training, snippets or budgets.
                if bindings.get(&target.kind, key).is_some() {
                    continue;
                }
                let mut text = Text::default();
                text.fields(effect, &["target", "key", "operation", "value"])?;
                emit(member(
                    "event-effect",
                    format!("effects[{index}]"),
                    effect,
                    time.clone(),
                    None,
                    text,
                ))?;
            }
        }
    }

    if record.kind == "knowledge" && record.status() == "canonical" {
        let mut earliest = None;
        for transition in list(record.get("transitions")) {
            let time = point(world, transition.get("time"))?;
            if earliest.as_ref().is_none_or(|previous| time < *previous) {
                earliest = Some(time);
            }
        }
        // As in ordinary source prose, an authored statement with no timed
        // transitions is timeless for authors. This grants no character
        // knowledge. Forgetting later does not erase author history.
        if earliest.as_ref().is_none_or(started) {
            let mut text = Text::default();
            if let Some(statement) = record.get("claim").and_then(|value| value.get("statement")) {
                text.value(statement)?;
            }
            if !text.0.is_empty() {
                emit(Structured {
                    kind: "knowledge-claim",
                    path: "claim.statement".into(),
                    member_id: None,
                    at: earliest,
                    until: None,
                    text: text.0,
                })?;
            }
        }
    }

    if record.kind == "scene" && matches!(record.status(), "active" | "closed") {
        for (index, observation) in list(record.get("observations")).iter().enumerate() {
            let time = point(world, observation.get("at"))?;
            let end = until(world, observation.get("until"))?;
            if !started(&time) || !active(end.as_ref()) {
                continue;
            }
            let mut text = Text::default();
            text.fields(observation, &["text"])?;
            emit(member(
                "scene-observation",
                format!("observations[{index}]"),
                observation,
                time,
                end,
                text,
            ))?;
        }
    }

    if record.kind == "conversation" && matches!(record.status(), "active" | "closed") {
        let turns = list(record.get("turns"));
        for (index, turn) in turns.iter().enumerate() {
            let time = crate::scene::turn_time(world, record, turn)?;
            if !started(&time) {
                continue;
            }
            let kind = if turn.get("kind").and_then(Yaml::as_str) == Some("action") {
                "conversation-action"
            } else {
                "conversation-turn"
            };
            let mut text = Text::default();
            text.fields(turn, &["text"])?;
            emit(member(
                kind,
                format!("turns[{index}]"),
                turn,
                time,
                None,
                text,
            ))?;
        }
        for (index, memory) in list(record.get("recollections")).iter().enumerate() {
            let time = point(world, memory.get("at"))?;
            if !started(&time) || memory.get("state").and_then(Yaml::as_str) == Some("forgotten") {
                continue;
            }
            let mut text = Text::default();
            text.fields(
                memory,
                &["summary", "interpretation", "emotional_impression"],
            )?;
            for exact in list(memory.get("exact_turns")) {
                let id = exact.as_str().ok_or_else(malformed)?;
                let turn = turns
                    .iter()
                    .find(|turn| turn.get("id").and_then(Yaml::as_str) == Some(id))
                    .ok_or_else(malformed)?;
                // Native source permits an action reference in exact_turns;
                // it remains an action, never reconstructed spoken words.
                if turn.get("kind").and_then(Yaml::as_str) != Some("action") {
                    text.fields(turn, &["text"])?;
                }
            }
            for quote in list(memory.get("remembered_quotes")) {
                text.fields(quote, &["text"])?;
            }
            emit(member(
                "conversation-recollection",
                format!("recollections[{index}]"),
                memory,
                time,
                None,
                text,
            ))?;
        }
    }
    Ok(())
}
