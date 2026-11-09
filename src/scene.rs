//! Timed scene presence and conversation audibility share one source contract.
//! Participant bounds are inclusive. Scene links do not imply conversation membership.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn mapping<'a>(value: &'a Yaml, label: &str) -> Result<&'a Mapping> {
    match value {
        Yaml::Mapping(value) => Ok(value),
        _ => Err(invalid(format!("{label} must be an untagged mapping"))),
    }
}
fn required<'a>(value: &'a Yaml, key: &str) -> Result<&'a Yaml> {
    mapping(value, "record member")?
        .get(key)
        .ok_or_else(|| invalid(format!("{key} is required")))
}
fn text<'a>(value: &'a Yaml, label: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!(
            "{label} must be a nonempty untagged string"
        ))),
    }
}
fn prose(value: Option<&Yaml>, label: &str) -> Result<()> {
    match value {
        None | Some(Yaml::Null | Yaml::String(_)) => Ok(()),
        _ => Err(invalid(format!("{label} must be an untagged string"))),
    }
}
fn closed<'a>(
    value: &'a Yaml,
    required: &[&str],
    optional: &[&str],
    label: &str,
) -> Result<&'a Mapping> {
    let map = mapping(value, label)?;
    for key in map.keys() {
        let key = text(key, label)?;
        if !required.contains(&key) && !optional.contains(&key) {
            return Err(invalid(format!("{label} contains unknown field {key:?}")));
        }
    }
    for key in required {
        if !map.contains_key(*key) {
            return Err(invalid(format!("{label}.{key} is required")));
        }
    }
    Ok(map)
}
fn list<'a>(value: &'a Yaml, key: &str) -> Result<&'a [Yaml]> {
    match mapping(value, "record member")?.get(key) {
        None | Some(Yaml::Null) => Ok(&[]),
        Some(Yaml::Sequence(value)) => Ok(value),
        _ => Err(invalid(format!("{key} must be an untagged array"))),
    }
}
fn point(world: &World, value: &Yaml) -> Result<StoryTime> {
    closed(value, &["tick"], &["timeline", "order"], "story time")?;
    if !matches!(required(value, "tick")?, Yaml::Number(number) if number.as_i64().is_some()) {
        return Err(invalid(
            "tick must be an untagged exact signed 64-bit integer",
        ));
    }
    if let Some(value) = value.get("order") {
        if !matches!(value, Yaml::Number(number) if number.as_i64().and_then(|value| i32::try_from(value).ok()).is_some())
        {
            return Err(invalid(
                "order must be an untagged exact signed 32-bit integer",
            ));
        }
    }
    if let Some(value) = value.get("timeline") {
        text(value, "timeline")?;
    }
    let at = StoryTime::from_yaml(value, &world.default_timeline())?;
    world.story_time(at.tick, Some(&at.timeline), at.order)
}
fn optional_point(world: &World, value: Option<&Yaml>) -> Result<Option<StoryTime>> {
    value
        .filter(|v| !matches!(v, Yaml::Null))
        .map(|v| point(world, v))
        .transpose()
}
fn reference<'a>(world: &'a World, value: &Yaml, kind: &str) -> Result<&'a Record> {
    let id = text(value, "reference")?;
    if !valid_id_for_kind(id, kind) {
        return Err(invalid(format!("{id:?} is not a valid {kind} ID")));
    }
    let record = world
        .records
        .get(id)
        .ok_or_else(|| invalid(format!("unknown {kind} reference {id}")))?;
    if record.kind != kind {
        return Err(invalid(format!("{id} must refer to a {kind}")));
    }
    Ok(record)
}
fn finite(value: &Yaml, label: &str) -> Result<f64> {
    match value {
        Yaml::Number(number) => number
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| invalid(format!("{label} must be a finite number"))),
        _ => Err(invalid(format!(
            "{label} must be an untagged finite number"
        ))),
    }
}
fn live(record: &Record) -> bool {
    matches!(record.status(), "active" | "closed")
}

struct Interval {
    start: StoryTime,
    end: Option<StoryTime>,
}
impl Interval {
    fn contains(&self, at: &StoryTime) -> bool {
        self.start.not_after(at) && self.end.as_ref().is_none_or(|end| at.not_after(end))
    }
}

fn interval(world: &World, record: &Record) -> Result<Interval> {
    let status = text(required(&record.frontmatter, "status")?, "status")?;
    let value = required(&record.frontmatter, "time")?;
    let (required_fields, optional_fields): (&[&str], &[&str]) = match record.kind.as_str() {
        "scene" => (&["start", "current"], &["end"]),
        "conversation" => (&["start"], &["end"]),
        _ => return Err(invalid("presence requires a scene or conversation")),
    };
    closed(value, required_fields, optional_fields, "time")?;
    let start = point(world, required(value, "start")?)?;
    let end = optional_point(world, value.get("end"))?;
    if status == "closed" && end.is_none() {
        return Err(invalid("a closed scene or conversation requires time.end"));
    }
    if end.as_ref().is_some_and(|end| !start.not_after(end)) {
        return Err(invalid(
            "time.end must be on the start timeline and not precede start",
        ));
    }
    let current = if record.kind == "scene" {
        let current = point(world, required(value, "current")?)?;
        if !start.not_after(&current) || end.as_ref().is_some_and(|end| !current.not_after(end)) {
            return Err(invalid(
                "scene time must satisfy start <= current <= end on one timeline",
            ));
        }
        Some(current)
    } else {
        None
    };
    // An open active scene has authored presence only through its cursor. An
    // open conversation has no cursor: each beat supplies its own exact time.
    let end = end.or_else(|| {
        (record.kind == "scene" && status == "active")
            .then_some(current)
            .flatten()
    });
    Ok(Interval { start, end })
}

fn participant_window(world: &World, participant: &Yaml, parent: &Interval) -> Result<Interval> {
    closed(
        participant,
        &["character"],
        &["from", "until", "role", "point_of_view"],
        "participant",
    )?;
    let start =
        optional_point(world, participant.get("from"))?.unwrap_or_else(|| parent.start.clone());
    let end = optional_point(world, participant.get("until"))?.or_else(|| parent.end.clone());
    if !parent.contains(&start)
        || end
            .as_ref()
            .is_some_and(|end| !parent.contains(end) || !start.not_after(end))
    {
        return Err(invalid(
            "participant from/until must form an inclusive interval inside its parent",
        ));
    }
    Ok(Interval { start, end })
}

/// Inclusive presence in this record, independent of linked scene membership.
/// Planned/abandoned scenes and draft/retired conversations confer no presence.
pub fn present(world: &World, record: &Record, character_id: &str, at: &StoryTime) -> Result<bool> {
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let parent = interval(world, record)?;
    if !live(record) || !parent.contains(at) {
        return Ok(false);
    }
    for participant in list(&record.frontmatter, "participants")? {
        let window = participant_window(world, participant, &parent)?;
        if text(required(participant, "character")?, "participant.character")? == character_id
            && window.contains(at)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every beat has an explicit exact time; an omitted kind still means speech.
pub fn turn_time(world: &World, conversation: &Record, turn: &Yaml) -> Result<StoryTime> {
    if conversation.kind != "conversation" {
        return Err(invalid("turn_time requires a conversation"));
    }
    point(world, required(turn, "at")?)
}

fn audience<'a>(item: &'a Yaml) -> Result<Vec<&'a str>> {
    let mut result = Vec::new();
    for value in list(item, "audience")? {
        let value = text(value, "audience selector")?;
        if !["public", "participants", "author"].contains(&value)
            && !valid_id_for_kind(value, "character")
        {
            return Err(invalid(format!("unknown audience selector {value:?}")));
        }
        if result.contains(&value) {
            return Err(invalid(format!("duplicate audience selector {value:?}")));
        }
        result.push(value);
    }
    Ok(result)
}
fn audience_allows(
    world: &World,
    record: &Record,
    item: &Yaml,
    character_id: &str,
    at: &StoryTime,
) -> Result<bool> {
    let audience = audience(item)?;
    Ok(present(world, record, character_id, at)?
        && audience.iter().any(|selector| {
            ["public", "participants"].contains(selector) || *selector == character_id
        }))
}

/// Audibility is evaluated at a beat's own time. Missing/empty audiences confer
/// no recipient access; a present speaker or actor always perceives their beat.
pub fn audible(
    world: &World,
    conversation: &Record,
    turn: &Yaml,
    character_id: &str,
) -> Result<bool> {
    let at = turn_time(world, conversation, turn)?;
    let allowed = audience_allows(world, conversation, turn, character_id, &at)?;
    if !present(world, conversation, character_id, &at)? {
        return Ok(false);
    }
    match turn
        .get("kind")
        .map(|value| text(value, "beat kind"))
        .transpose()?
        .unwrap_or("speech")
    {
        "speech" => Ok(allowed || text(required(turn, "speaker")?, "speaker")? == character_id),
        "action" => {
            let actors = list(turn, "actors")?
                .iter()
                .map(|actor| text(actor, "actor"))
                .collect::<Result<Vec<_>>>()?;
            Ok(allowed || actors.contains(&character_id))
        }
        kind => Err(invalid(format!("unsupported beat kind {kind:?}"))),
    }
}

/// Recorded observations remain dated records, not inferred beliefs. Admission
/// uses origin-time membership; an authored until limits query-time visibility.
pub fn observation_visible(
    world: &World,
    scene: &Record,
    observation: &Yaml,
    character_id: &str,
    at: &StoryTime,
) -> Result<bool> {
    if scene.kind != "scene" {
        return Err(invalid("observation_visible requires a scene"));
    }
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let origin = point(world, required(observation, "at")?)?;
    let until = optional_point(world, observation.get("until"))?;
    if !origin.not_after(at) || until.as_ref().is_some_and(|until| !at.not_after(until)) {
        return Ok(false);
    }
    audience_allows(world, scene, observation, character_id, &origin)
}

fn unique_member<'a>(item: &'a Yaml, seen: &mut BTreeSet<String>, label: &str) -> Result<&'a str> {
    let id = text(required(item, "id")?, label)?;
    if !seen.insert(id.to_owned()) {
        return Err(invalid(format!("duplicate {label} {id:?}")));
    }
    Ok(id)
}
fn validate_audience(world: &World, record: &Record, item: &Yaml, at: &StoryTime) -> Result<()> {
    for selector in audience(item)? {
        if ["public", "participants", "author"].contains(&selector) {
            continue;
        }
        reference(world, &Yaml::String(selector.into()), "character")?;
        if !present_for_validation(world, record, selector, at)? {
            return Err(invalid(format!(
                "audience recipient {selector} is absent at the item's time"
            )));
        }
    }
    Ok(())
}
// Shape validation also applies to planned/draft records, whose authored
// membership is meaningful for checking their contents but not live admission.
fn present_for_validation(
    world: &World,
    record: &Record,
    character: &str,
    at: &StoryTime,
) -> Result<bool> {
    let parent = interval(world, record)?;
    if !parent.contains(at) {
        return Ok(false);
    }
    for item in list(&record.frontmatter, "participants")? {
        let window = participant_window(world, item, &parent)?;
        if text(required(item, "character")?, "participant.character")? == character
            && window.contains(at)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn validate_addressee(
    world: &World,
    record: &Record,
    value: Option<&Yaml>,
    at: &StoryTime,
) -> Result<()> {
    let Some(value) = value.filter(|v| !matches!(v, Yaml::Null)) else {
        return Ok(());
    };
    if matches!(value, Yaml::String(value) if value == "participants") {
        return Ok(());
    }
    let values = match value {
        Yaml::Sequence(values) => values.as_slice(),
        _ => std::slice::from_ref(value),
    };
    let mut seen = BTreeSet::new();
    for value in values {
        let character = reference(world, value, "character")?;
        if !seen.insert(&character.id) || !present_for_validation(world, record, &character.id, at)?
        {
            return Err(invalid(
                "addressees must be unique characters present at the beat time",
            ));
        }
    }
    Ok(())
}

fn validate_record(world: &World, record: &Record) -> Result<()> {
    let parent = interval(world, record)?;
    reference(
        world,
        required(&record.frontmatter, "location")?,
        "location",
    )?;
    let allowed = if record.kind == "scene" {
        &["planned", "active", "closed", "abandoned"][..]
    } else {
        &["draft", "active", "closed", "retired"][..]
    };
    if !allowed.contains(&record.status()) {
        return Err(invalid("invalid scene/conversation status"));
    }
    let mut participants = BTreeSet::new();
    for participant in list(&record.frontmatter, "participants")? {
        participant_window(world, participant, &parent)?;
        let character = reference(world, required(participant, "character")?, "character")?;
        if !participants.insert(character.id.as_str()) {
            return Err(invalid(
                "a character may have only one participant row in a record",
            ));
        }
        prose(participant.get("role"), "participant.role")?;
        if participant
            .get("point_of_view")
            .is_some_and(|v| !matches!(v, Yaml::Null | Yaml::Bool(_)))
        {
            return Err(invalid("participant.point_of_view must be boolean"));
        }
    }
    let mut member_ids = BTreeSet::new();
    if record.kind == "scene" {
        for observation in list(&record.frontmatter, "observations")? {
            closed(
                observation,
                &["id", "at", "text"],
                &["until", "audience", "salience"],
                "observation",
            )?;
            unique_member(observation, &mut member_ids, "observation ID")?;
            text(required(observation, "text")?, "observation.text")?;
            let at = point(world, required(observation, "at")?)?;
            if !parent.contains(&at) {
                return Err(invalid("observation.at must lie inside the scene interval"));
            }
            if optional_point(world, observation.get("until"))?
                .as_ref()
                .is_some_and(|end| !at.not_after(end))
            {
                return Err(invalid(
                    "observation.until must not precede its at or change timeline",
                ));
            }
            if let Some(value) = observation
                .get("salience")
                .filter(|v| !matches!(v, Yaml::Null))
            {
                if finite(value, "salience")? < 0.0 {
                    return Err(invalid("salience must be nonnegative"));
                }
            }
            validate_audience(world, record, observation, &at)?;
        }
        for (field, kind) in [
            ("conversations", "conversation"),
            ("outcome_events", "event"),
            ("objects", "object"),
            ("environments", "environment"),
            ("story_points", "story-point"),
        ] {
            let mut seen = BTreeSet::new();
            for value in list(&record.frontmatter, field)? {
                let target = reference(world, value, kind)?;
                if !seen.insert(&target.id) {
                    return Err(invalid(format!("duplicate {field} reference")));
                }
            }
        }
        return Ok(());
    }
    if let Some(scene) = record.get("scene").filter(|v| !matches!(v, Yaml::Null)) {
        reference(world, scene, "scene")?;
    }
    let mut turns: BTreeMap<&str, StoryTime> = BTreeMap::new();
    let mut earlier_speech = BTreeSet::new();
    for turn in list(&record.frontmatter, "turns")? {
        let kind = turn
            .get("kind")
            .map(|v| text(v, "beat kind"))
            .transpose()?
            .unwrap_or("speech");
        match kind {
            "speech" => {
                closed(
                    turn,
                    &["id", "at", "text", "speaker"],
                    &["kind", "audience", "delivery", "addressee", "interrupts"],
                    "speech beat",
                )?;
            }
            "action" => {
                closed(
                    turn,
                    &["id", "at", "text", "actors"],
                    &["kind", "audience"],
                    "action beat",
                )?;
            }
            _ => return Err(invalid(format!("unsupported beat kind {kind:?}"))),
        }
        let id = unique_member(turn, &mut member_ids, "beat ID")?;
        text(required(turn, "text")?, "beat.text")?;
        let at = turn_time(world, record, turn)?;
        if !parent.contains(&at) {
            return Err(invalid("beat.at must lie inside the conversation interval"));
        }
        if kind == "speech" {
            let speaker = reference(world, required(turn, "speaker")?, "character")?;
            if !present_for_validation(world, record, &speaker.id, &at)? {
                return Err(invalid("speaker must be present at the beat time"));
            }
            prose(turn.get("delivery"), "delivery")?;
            validate_addressee(world, record, turn.get("addressee"), &at)?;
            if let Some(target) = turn.get("interrupts") {
                let target = text(target, "interrupts")?;
                let target_time = turns
                    .get(target)
                    .filter(|_| earlier_speech.contains(target))
                    .ok_or_else(|| {
                        invalid("interrupts must target an earlier local speech beat")
                    })?;
                if !target_time.not_after(&at) || target_time == &at {
                    return Err(invalid(
                        "interrupted speech must be strictly earlier on the same timeline",
                    ));
                }
            }
            earlier_speech.insert(id);
        } else {
            let actors = list(turn, "actors")?;
            if actors.is_empty() {
                return Err(invalid("action beat requires at least one actor"));
            }
            let mut seen = BTreeSet::new();
            for actor in actors {
                let actor = reference(world, actor, "character")?;
                if !seen.insert(&actor.id)
                    || !present_for_validation(world, record, &actor.id, &at)?
                {
                    return Err(invalid(
                        "action actors must be unique characters present at the beat time",
                    ));
                }
            }
        }
        validate_audience(world, record, turn, &at)?;
        turns.insert(id, at);
    }
    let mut memory_coordinates = BTreeSet::new();
    for memory in list(&record.frontmatter, "recollections")? {
        closed(
            memory,
            &["id", "at", "character", "state", "confidence", "summary"],
            &[
                "exact_turns",
                "remembered_quotes",
                "interpretation",
                "emotional_impression",
            ],
            "recollection",
        )?;
        unique_member(memory, &mut member_ids, "recollection ID")?;
        let character = reference(world, required(memory, "character")?, "character")?;
        if !participants.contains(character.id.as_str()) {
            return Err(invalid(
                "recollection owner must be a conversation participant",
            ));
        }
        let at = point(world, required(memory, "at")?)?;
        if !parent.start.not_after(&at) {
            return Err(invalid(
                "recollection.at must not precede conversation start or change timeline",
            ));
        }
        if !memory_coordinates.insert((character.id.as_str(), at.clone())) {
            return Err(invalid(
                "a character may have only one recollection at an exact time in a conversation",
            ));
        }
        let confidence = finite(required(memory, "confidence")?, "recollection.confidence")?;
        if !(0.0..=1.0).contains(&confidence) {
            return Err(invalid("recollection.confidence must be between 0 and 1"));
        }
        let state = text(required(memory, "state")?, "recollection.state")?;
        if !["remembered", "forgotten"].contains(&state) {
            return Err(invalid(
                "recollection.state must be remembered or forgotten",
            ));
        }
        text(required(memory, "summary")?, "recollection.summary")?;
        for key in ["interpretation", "emotional_impression"] {
            prose(memory.get(key), key)?;
        }
        let check_source = |value: &Yaml| -> Result<()> {
            let id = text(value, "local turn reference")?;
            let source_time = turns
                .get(id)
                .ok_or_else(|| invalid(format!("unknown local turn {id}")))?;
            if !source_time.not_after(&at) {
                return Err(invalid("recollection cannot refer to a later turn"));
            }
            Ok(())
        };
        let mut exact = BTreeSet::new();
        for value in list(memory, "exact_turns")? {
            check_source(value)?;
            if !exact.insert(text(value, "exact_turns")?) {
                return Err(invalid("duplicate exact_turns reference"));
            }
        }
        for quote in list(memory, "remembered_quotes")? {
            closed(
                quote,
                &["source_turn", "speaker", "text", "fidelity"],
                &[],
                "remembered quote",
            )?;
            check_source(required(quote, "source_turn")?)?;
            reference(world, required(quote, "speaker")?, "character")?;
            text(required(quote, "text")?, "remembered quote text")?;
            if !["exact", "approximate"]
                .contains(&text(required(quote, "fidelity")?, "quote fidelity")?)
            {
                return Err(invalid(
                    "remembered quote fidelity must be exact or approximate",
                ));
            }
        }
    }
    Ok(())
}

fn diagnostic(record: &Record, code: &str, message: String) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message,
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: None,
    }
}

/// Structural and temporal checks for the compact scene/conversation contract.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut occupied: BTreeMap<String, Vec<(&Record, Interval)>> = BTreeMap::new();
    for record in world
        .records
        .values()
        .filter(|r| matches!(r.kind.as_str(), "scene" | "conversation"))
    {
        if let Err(error) = validate_record(world, record) {
            diagnostics.push(diagnostic(
                record,
                if record.kind == "scene" {
                    "WDL-SCENE-001"
                } else {
                    "WDL-CONVERSATION-001"
                },
                error.to_string(),
            ));
            continue;
        }
        if record.kind != "scene" || !live(record) {
            continue;
        }
        let parent = interval(world, record).expect("validated interval");
        for participant in
            list(&record.frontmatter, "participants").expect("validated participants")
        {
            let character = participant["character"]
                .as_str()
                .expect("validated character");
            let window = participant_window(world, participant, &parent)
                .expect("validated participant interval");
            let previous = occupied.entry(character.into()).or_default();
            for (other, other_window) in previous.iter() {
                if window.start.timeline == other_window.start.timeline
                    && window
                        .end
                        .as_ref()
                        .is_none_or(|end| other_window.start.not_after(end))
                    && other_window
                        .end
                        .as_ref()
                        .is_none_or(|end| window.start.not_after(end))
                {
                    diagnostics.push(diagnostic(
                        record,
                        "WDL-SCENE-002",
                        format!(
                            "{character} has overlapping inclusive scene membership in {} and {}",
                            other.id, record.id
                        ),
                    ));
                }
            }
            previous.push((record, window));
        }
    }
    diagnostics
}

fn citation(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":section})
}

/// Author-only, timed scene projection. Untimed Markdown is deliberately absent.
pub fn query(world: &World, reference_str: &str, at: &StoryTime) -> Result<Json> {
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let record = world.resolve(reference_str)?;
    if record.kind != "scene" {
        return Err(Error::InvalidInput(format!("{} is not a scene", record.id)));
    }
    let parent = interval(world, record)?;
    let mut participants = Vec::new();
    for (index, member) in list(&record.frontmatter, "participants")?
        .iter()
        .enumerate()
    {
        let character = text(required(member, "character")?, "participant.character")?;
        if present(world, record, character, at)? {
            participants.push(json!({"characterId":character,"role":member.get("role"),
                "pointOfView":member.get("point_of_view"),"citation":citation(world,record,&format!("participants[{index}]"))}));
        }
    }
    participants.sort_by(|a, b| a["characterId"].as_str().cmp(&b["characterId"].as_str()));
    let mut observations = Vec::new();
    for (index, item) in list(&record.frontmatter, "observations")?
        .iter()
        .enumerate()
    {
        let origin = point(world, required(item, "at")?)?;
        let until = optional_point(world, item.get("until"))?;
        if live(record)
            && parent.contains(&origin)
            && origin.not_after(at)
            && until.as_ref().is_none_or(|end| at.not_after(end))
        {
            let id = text(required(item, "id")?, "observation ID")?;
            let mut value = serde_json::to_value(item)?;
            value["at"] = json!(origin);
            if item.get("until").is_some() {
                value["until"] = json!(until);
            }
            value["citation"] = citation(world, record, &format!("observations[{index}]"));
            observations.push((origin, id.to_owned(), value));
        }
    }
    observations.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut conversations = Vec::new();
    for item in list(&record.frontmatter, "conversations")? {
        let conversation = reference(world, item, "conversation")?;
        let start = interval(world, conversation)?.start;
        if live(conversation) && start.not_after(at) {
            conversations.push((
                start.clone(),
                conversation.id.clone(),
                json!({"conversationId":conversation.id,"title":conversation.title,
                "start":start,"citation":citation(world,conversation,"time.start")}),
            ));
        }
    }
    conversations.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut outcomes = Vec::new();
    for item in list(&record.frontmatter, "outcome_events")? {
        let event = reference(world, item, "event")?;
        let time = point(world, required(&event.frontmatter, "time")?)?;
        if event.status() == "canonical" && time.not_after(at) {
            outcomes.push((time.clone(), event.id.clone(), json!({"eventId":event.id,"title":event.title,"time":time,"citation":citation(world,event,"time")})));
        }
    }
    outcomes.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","revision":world.revision,"fingerprint":world.fingerprint,
        "effectiveTime":at,"timeScope":"as-of","sceneId":record.id,"title":record.title,"status":record.status(),
        "locationId":record.get("location"),"activeAtTime":live(record)&&parent.contains(at),"presentParticipants":participants,
        "observations":observations.into_iter().map(|(_,_,v)|v).collect::<Vec<_>>(),
        "conversations":conversations.into_iter().map(|(_,_,v)|v).collect::<Vec<_>>(),
        "outcomeEvents":outcomes.into_iter().map(|(_,_,v)|v).collect::<Vec<_>>(),"citation":citation(world,record,"time-and-participants")}),
    )
}
