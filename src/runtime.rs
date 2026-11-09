//! Deterministic, author-local queries over one immutable source snapshot.
//!
//! These functions do not implement character-perspective admission. In
//! particular, a packet *about* a character is still an author's packet.

use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    state_schema,
};
use serde_json::{Map, Value, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeMap;

fn field<'a>(value: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    value.as_mapping()?.get(Yaml::String(key.into()))
}

fn object<'a>(value: &'a Yaml, label: &str) -> Result<&'a serde_yaml::Mapping> {
    value
        .as_mapping()
        .ok_or_else(|| Error::Validation(format!("{label} must be a mapping")))
}

fn string<'a>(value: &'a Yaml, key: &str, label: &str) -> Result<&'a str> {
    field(value, key)
        .and_then(Yaml::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Validation(format!("{label}.{key} must be a nonempty string")))
}

fn optional_string<'a>(value: &'a Yaml, key: &str, label: &str) -> Result<Option<&'a str>> {
    match field(value, key) {
        None | Some(Yaml::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| Error::Validation(format!("{label}.{key} must be a string"))),
    }
}

fn list<'a>(value: &'a Yaml, key: &str, label: &str) -> Result<&'a [Yaml]> {
    match field(value, key) {
        None | Some(Yaml::Null) => Ok(&[]),
        Some(value) => value
            .as_sequence()
            .map(Vec::as_slice)
            .ok_or_else(|| Error::Validation(format!("{label}.{key} must be a sequence"))),
    }
}

fn to_json(value: &Yaml) -> Result<Value> {
    Ok(serde_json::to_value(value)?)
}

fn json_field(value: &Yaml, key: &str) -> Result<Value> {
    field(value, key)
        .map(to_json)
        .transpose()
        .map(|v| v.unwrap_or(Value::Null))
}

fn mapping_field(value: &Yaml, key: &str, label: &str) -> Result<Map<String, Value>> {
    match field(value, key) {
        None | Some(Yaml::Null) => Ok(Map::new()),
        Some(value) => {
            object(value, &format!("{label}.{key}"))?;
            to_json(value)?
                .as_object()
                .cloned()
                .ok_or_else(|| Error::Validation(format!("{label}.{key} needs string keys")))
        }
    }
}

fn ensure_timeline(world: &World, timeline: &str) -> Result<()> {
    if timeline.is_empty() {
        return Err(Error::InvalidInput("timeline must be nonempty".into()));
    }
    let config = &world.world_record()?.frontmatter;
    for declaration in list(config, "timelines", "world")? {
        object(declaration, "timeline declaration")?;
        if string(declaration, "id", "timeline declaration")? == timeline {
            return Ok(());
        }
    }
    Err(Error::InvalidInput(format!(
        "unknown timeline {timeline:?}"
    )))
}

fn time(world: &World, value: &Yaml) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(value, &world.default_timeline())?;
    ensure_timeline(world, &point.timeline)?;
    Ok(point)
}

fn required_time(world: &World, value: &Yaml, key: &str, label: &str) -> Result<StoryTime> {
    time(
        world,
        field(value, key).ok_or_else(|| Error::Validation(format!("{label}.{key} is required")))?,
    )
}

fn citation(world: &World, record: &Record, section: &str) -> Value {
    json!({"entityId":record.id,"sourcePath":record.source_path,
        "blobOid":record.blob_oid,"revision":world.revision,
        "fingerprint":world.fingerprint,"section":section})
}

fn envelope(world: &World, at: &StoryTime) -> Value {
    json!({"protocol":crate::CONTRACT,"perspective":"author",
        "revision":world.revision,"fingerprint":world.fingerprint,
        "effectiveTime":at,"timeScope":"as-of"})
}

fn resolve_kind<'a>(world: &'a World, reference: &str, kind: &str) -> Result<&'a Record> {
    let record = world.resolve(reference)?;
    if record.kind != kind {
        return Err(Error::InvalidInput(format!(
            "{} is {}, expected {kind}",
            record.id, record.kind
        )));
    }
    Ok(record)
}

pub(crate) fn canonical_events(world: &World) -> Result<Vec<(&Record, StoryTime)>> {
    let mut events = Vec::new();
    for record in world.records.values().filter(|r| r.kind == "event") {
        let status =
            optional_string(&record.frontmatter, "status", &record.id)?.unwrap_or("canonical");
        if status == "canonical" {
            events.push((
                record,
                required_time(world, &record.frontmatter, "time", &record.id)?,
            ));
        }
    }
    events.sort_by(|(a, ta), (b, tb)| {
        (&ta.timeline, ta.tick, ta.order, &a.id).cmp(&(&tb.timeline, tb.tick, tb.order, &b.id))
    });
    Ok(events)
}

pub(crate) fn replay_state(
    world: &World,
    reference: &str,
    at: &StoryTime,
) -> Result<(BTreeMap<String, Yaml>, Map<String, Value>)> {
    replay_state_with_effects(world, reference, at, |_| Ok(true))
}

/// A read-only semantic consumer can withhold complete effect operands before
/// replay without changing the validated world or renumbering source members.
pub(crate) fn replay_state_with_effects(
    world: &World,
    reference: &str,
    at: &StoryTime,
    mut admits: impl FnMut(&Yaml) -> Result<bool>,
) -> Result<(BTreeMap<String, Yaml>, Map<String, Value>)> {
    replay_state_with_event_effects(world, reference, at, |_, effect| admits(effect))
}

/// Admitted author readers can also withhold the owning event before replay.
/// The established canonical and semantic callers keep their existing policy.
pub(crate) fn replay_state_with_event_effects(
    world: &World,
    reference: &str,
    at: &StoryTime,
    mut admits: impl FnMut(&Record, &Yaml) -> Result<bool>,
) -> Result<(BTreeMap<String, Yaml>, Map<String, Value>)> {
    ensure_timeline(world, &at.timeline)?;
    let record = world.resolve(reference)?;
    let mut values = state_schema::initial_state(world, record)?;
    let mut citations = Map::new();
    for key in values.keys() {
        citations.insert(key.clone(), citation(world, record, "initial_state"));
    }
    for (event, point) in canonical_events(world)? {
        if !point.not_after(at) {
            continue;
        }
        for (ordinal, effect) in list(&event.frontmatter, "effects", &event.id)?
            .iter()
            .enumerate()
        {
            object(effect, "effect")?;
            if string(effect, "target", &event.id)? != record.id {
                continue;
            }
            if !admits(event, effect)? {
                continue;
            }
            let key = string(effect, "key", &event.id)?;
            state_schema::apply_effect(world, record, &mut values, effect)?;
            let effect_id = optional_string(effect, "id", &event.id)?
                .map(str::to_owned)
                .unwrap_or_else(|| format!("effect:{ordinal}"));
            let mut source = citation(world, event, &format!("effects[{ordinal}]"));
            source["effectId"] = json!(effect_id);
            source["time"] = json!(point);
            citations.insert(key.into(), source);
        }
    }
    Ok((values, citations))
}

/// Typed raw values for internal predicates. No JSON string/number conversion.
pub fn resolved_state(
    world: &World,
    reference: &str,
    at: &StoryTime,
) -> Result<BTreeMap<String, Yaml>> {
    replay_state(world, reference, at).map(|(values, _)| values)
}

/// Replay canonical event effects at one timeline coordinate. Integer fields
/// use decimal-string JSON values so browsers preserve the complete i64 range.
pub fn state(world: &World, reference: &str, at: &StoryTime) -> Result<Value> {
    let record = world.resolve(reference)?;
    let (values, mut citations) = replay_state(world, reference, at)?;
    let bindings = crate::lore_state::Bindings::parse(world)?;
    citations.retain(|key, _| bindings.get(&record.kind, key).is_none());
    let mut wire_values = Map::new();
    for (key, value) in values {
        if bindings.get(&record.kind, &key).is_some() {
            continue;
        }
        let definition = state_schema::field_definition(world, &record.kind, &key)?;
        wire_values.insert(key, state_schema::value_to_json(&definition, &value)?);
    }
    let mut result = envelope(world, at);
    result["entityId"] = json!(record.id);
    result["title"] = json!(record.title);
    result["state"] = Value::Object(wire_values);
    result["citations"] = Value::Object(citations);
    Ok(result)
}

pub(crate) fn latest_transition<'a>(
    world: &World,
    record: &'a Record,
    at: &StoryTime,
) -> Result<Option<(usize, &'a Yaml, StoryTime)>> {
    let mut selected: Option<(usize, &Yaml, StoryTime)> = None;
    for (ordinal, transition) in list(&record.frontmatter, "transitions", &record.id)?
        .iter()
        .enumerate()
    {
        object(transition, &record.id)?;
        let point = required_time(world, transition, "time", &record.id)?;
        if point.not_after(at)
            && selected.as_ref().is_none_or(|(old_ordinal, _, old)| {
                (point.tick, point.order, ordinal) > (old.tick, old.order, *old_ordinal)
            })
        {
            selected = Some((ordinal, transition, point));
        }
    }
    Ok(selected)
}

/// Return an author's view of a character's authored beliefs at the horizon.
pub fn knowledge(
    world: &World,
    reference: &str,
    at: &StoryTime,
    include_forgotten: bool,
) -> Result<Value> {
    ensure_timeline(world, &at.timeline)?;
    let character = resolve_kind(world, reference, "character")?;
    let mut items = Vec::new();
    for record in world.records.values().filter(|r| r.kind == "knowledge") {
        if optional_string(&record.frontmatter, "status", &record.id)?.unwrap_or("canonical")
            != "canonical"
        {
            continue;
        }
        if string(&record.frontmatter, "knower", &record.id)? != character.id {
            continue;
        }
        let Some((ordinal, active, point)) = latest_transition(world, record, at)? else {
            continue;
        };
        let state = string(active, "state", &record.id)?;
        if ![
            "accepted",
            "suspected",
            "rejected",
            "uncertain",
            "remembered",
            "forgotten",
        ]
        .contains(&state)
        {
            return Err(Error::Unsupported(format!(
                "unsupported knowledge state {state:?} in {}",
                record.id
            )));
        }
        if state == "forgotten" && !include_forgotten {
            continue;
        }
        let claim = field(&record.frontmatter, "claim")
            .ok_or_else(|| Error::Validation(format!("{}: claim is required", record.id)))?;
        object(claim, &record.id)?;
        let mut claim_value = to_json(claim)?;
        if let Some(valid) = field(claim, "genealogy").and_then(|value| field(value, "valid")) {
            for boundary in ["from", "until"] {
                if let Some(point) = field(valid, boundary) {
                    claim_value["genealogy"]["valid"][boundary] = json!(time(world, point)?);
                }
            }
        }
        let mut source = citation(world, record, &format!("transitions[{ordinal}]"));
        source["time"] = json!(point);
        items.push(json!({"knowledgeId":record.id,"claimKey":json_field(claim,"key")?,
            "statement":json_field(claim,"statement")?,"claim":claim_value,"state":state,
            "confidence":json_field(active,"confidence")?,"acquisition":json_field(active,"acquisition")?,
            "sourceEntityId":json_field(active,"source_entity")?,"causingEventId":json_field(active,"causing_event")?,
            "note":json_field(active,"note")?,"time":point,"transitionId":json_field(active,"id")?,"citation":source}));
    }
    items.sort_by(|a, b| {
        (
            a["claimKey"].as_str().unwrap_or(""),
            a["knowledgeId"].as_str(),
        )
            .cmp(&(
                b["claimKey"].as_str().unwrap_or(""),
                b["knowledgeId"].as_str(),
            ))
    });
    let mut result = envelope(world, at);
    result["characterId"] = json!(character.id);
    result["includeForgotten"] = json!(include_forgotten);
    result["items"] = json!(items);
    Ok(result)
}

/// Directed relationships from the selected character, never inferred reciprocals.
pub fn relationships(world: &World, reference: &str, at: &StoryTime) -> Result<Value> {
    ensure_timeline(world, &at.timeline)?;
    let character = resolve_kind(world, reference, "character")?;
    let mut items = Vec::new();
    for record in world.records.values().filter(|r| r.kind == "relationship") {
        if string(&record.frontmatter, "from", &record.id)? != character.id {
            continue;
        }
        let Some((ordinal, active, point)) = latest_transition(world, record, at)? else {
            continue;
        };
        let metrics = mapping_field(active, "metrics", &record.id)?;
        let facets = list(active, "facets", &record.id)?
            .iter()
            .map(to_json)
            .collect::<Result<Vec<_>>>()?;
        items.push(json!({"relationshipId":record.id,
            "from":character.id,"to":string(&record.frontmatter,"to",&record.id)?,
            "relationshipKind":json_field(&record.frontmatter,"relationship_kind")?,
            "status":optional_string(active,"relationship_status",&record.id)?.unwrap_or("active"),
            "metrics":metrics,"facets":facets,"note":json_field(active,"note")?,
            "time":point,"transitionId":json_field(active,"id")?,
            "citation":citation(world,record,&format!("transitions[{ordinal}]"))}));
    }
    let mut result = envelope(world, at);
    result["characterId"] = json!(character.id);
    result["items"] = json!(items);
    Ok(result)
}

/// Canonical event catalogue, with an optional as-of horizon on one timeline.
pub fn timeline(world: &World, timeline_id: &str, at: Option<&StoryTime>) -> Result<Value> {
    ensure_timeline(world, timeline_id)?;
    if at.is_some_and(|point| point.timeline != timeline_id) {
        return Err(Error::InvalidInput(
            "timeline and horizon must agree".into(),
        ));
    }
    let mut events = Vec::new();
    for (event, point) in canonical_events(world)? {
        if point.timeline != timeline_id || at.is_some_and(|at| !point.not_after(at)) {
            continue;
        }
        events.push(json!({"entityId":event.id,"title":event.title,"time":point,
            "locationId":json_field(&event.frontmatter,"location")?,
            "participants":list(&event.frontmatter,"participants",&event.id)?.iter().map(to_json).collect::<Result<Vec<_>>>()?,
            "citation":citation(world,event,"time")}));
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","revision":world.revision,
        "fingerprint":world.fingerprint,"timeline":timeline_id,"effectiveTime":at,
        "timeScope":if at.is_some() {"as-of"} else {"all-time"},"events":events}),
    )
}

/// Author transcript. Dated recollections remain separate from verbatim speech.
pub fn conversation(world: &World, reference: &str, at: &StoryTime) -> Result<Value> {
    ensure_timeline(world, &at.timeline)?;
    let record = resolve_kind(world, reference, "conversation")?;
    let interval = record
        .get("time")
        .ok_or_else(|| Error::Validation(format!("{}: time required", record.id)))?;
    object(interval, &record.id)?;
    required_time(world, interval, "start", &record.id)?;
    let mut beats = Vec::new();
    for (ordinal, turn) in list(&record.frontmatter, "turns", &record.id)?
        .iter()
        .enumerate()
    {
        object(turn, &record.id)?;
        let point = crate::scene::turn_time(world, record, turn)?;
        if !point.not_after(at) {
            continue;
        }
        let kind = optional_string(turn, "kind", &record.id)?.unwrap_or("speech");
        if kind != "speech" && kind != "action" {
            return Err(Error::Unsupported(format!(
                "{}: unsupported conversation beat {kind:?}",
                record.id
            )));
        }
        let mut value = json!({"id":json_field(turn,"id")?,"kind":kind,"at":point,
            "text":string(turn,"text",&record.id)?,"audience":json_field(turn,"audience")?,
            "citation":citation(world,record,&format!("turns[{ordinal}]"))});
        if kind == "speech" {
            value["speakerId"] = json!(string(turn, "speaker", &record.id)?);
            value["delivery"] = json_field(turn, "delivery")?;
            value["addressee"] = json_field(turn, "addressee")?;
            if turn.get("interrupts").is_some() {
                value["interrupts"] = json_field(turn, "interrupts")?;
            }
        } else {
            value["actors"] = json!(
                list(turn, "actors", &record.id)?
                    .iter()
                    .map(to_json)
                    .collect::<Result<Vec<_>>>()?
            );
        }
        beats.push((point, string(turn, "id", &record.id)?.to_owned(), value));
    }
    beats.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let beats: Vec<Value> = beats.into_iter().map(|(_, _, value)| value).collect();
    let mut recollections = Vec::new();
    for (ordinal, recollection) in list(&record.frontmatter, "recollections", &record.id)?
        .iter()
        .enumerate()
    {
        object(recollection, &record.id)?;
        let point = required_time(world, recollection, "at", &record.id)?;
        if point.not_after(at) {
            let mut value = to_json(recollection)?;
            value["at"] = json!(point);
            value["citation"] = citation(world, record, &format!("recollections[{ordinal}]"));
            recollections.push((
                point,
                string(recollection, "id", &record.id)?.to_owned(),
                value,
            ));
        }
    }
    recollections.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let recollections: Vec<Value> = recollections
        .into_iter()
        .map(|(_, _, value)| value)
        .collect();
    let verbatim: Vec<Value> = beats
        .iter()
        .filter(|v| v["kind"] == "speech")
        .cloned()
        .collect();
    let mut result = envelope(world, at);
    result["conversationId"] = json!(record.id);
    result["title"] = json!(record.title);
    result["verbatimTurns"] = json!(verbatim);
    result["beats"] = json!(beats);
    result["recollections"] = json!(recollections);
    Ok(result)
}

/// Complete, bounded author packet. Oversized packets fail without silent loss.
/// Timeless Markdown bodies are deliberately absent from as-of packets.
pub fn context(world: &World, reference: &str, at: &StoryTime, max_chars: usize) -> Result<Value> {
    ensure_timeline(world, &at.timeline)?;
    if max_chars == 0 || max_chars > 1_000_000 {
        return Err(Error::InvalidInput(
            "max_chars must be between 1 and 1000000".into(),
        ));
    }
    let character = resolve_kind(world, reference, "character")?;
    let mut scenes = Vec::new();
    for scene in world.records.values().filter(|r| r.kind == "scene") {
        if crate::scene::present(world, scene, &character.id, at)? {
            scenes.push(json!({"sceneId":scene.id,"title":scene.title,
                "locationId":json_field(&scene.frontmatter,"location")?,
                "citation":citation(world,scene,"time-and-participants")}));
        }
    }
    let mut packet = envelope(world, at);
    packet["scope"] =
        json!("author-local packet about a character; not a character-perspective view");
    packet["characterId"] = json!(character.id);
    packet["title"] = json!(character.title);
    packet["state"] = state(world, &character.id, at)?;
    packet["knowledge"] = knowledge(world, &character.id, at, false)?;
    packet["relationships"] = relationships(world, &character.id, at)?;
    packet["scenes"] = json!(scenes);
    packet["maxCharacters"] = json!(max_chars);
    packet["omittedSections"] = json!([
        "untimed prose",
        "character audience admission",
        "advanced domain semantics"
    ]);
    let needed = serde_json::to_string(&packet)?.chars().count();
    if needed > max_chars {
        return Err(Error::InvalidInput(format!(
            "complete author context needs {needed} characters; max_chars is {max_chars}"
        )));
    }
    Ok(packet)
}
