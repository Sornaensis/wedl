//! Explicit, dated character evidence. This is an author-selected projection,
//! not authentication, and does not admit untimed prose or canonical state.
//! Noninterference applies to valid snapshots: hidden source metadata never
//! enters the output, search corpus, ranking, counts, or packet budget.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    scene, validation,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use serde_yaml::Value as Yaml;

const UNAVAILABLE: &str = "character view is unavailable";

fn malformed() -> Error {
    Error::Validation(
        "character evidence is malformed; use author validation for diagnostics".into(),
    )
}

fn text(value: Option<&Yaml>) -> Result<&str> {
    match value {
        Some(Yaml::String(value)) => Ok(value),
        _ => Err(malformed()),
    }
}

fn list(value: Option<&Yaml>) -> Result<&[Yaml]> {
    match value {
        None | Some(Yaml::Null) => Ok(&[]),
        Some(Yaml::Sequence(value)) => Ok(value),
        _ => Err(malformed()),
    }
}

fn point(world: &World, value: Option<&Yaml>) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(value.ok_or_else(malformed)?, &world.default_timeline())
        .map_err(|_| malformed())?;
    world
        .story_time(point.tick, Some(&point.timeline), point.order)
        .map_err(|_| malformed())
}

fn wire_time(value: &Value) -> Result<StoryTime> {
    StoryTime::new(
        value["timeline"].as_str().ok_or_else(malformed)?,
        value["tick"]
            .as_str()
            .ok_or_else(malformed)?
            .parse()
            .map_err(|_| malformed())?,
        value["order"]
            .as_str()
            .ok_or_else(malformed)?
            .parse()
            .map_err(|_| malformed())?,
    )
    .map_err(|_| malformed())
}

fn selected<'a>(world: &'a World, character_id: &str, at: &StoryTime) -> Result<&'a Record> {
    let character = world.records.get(character_id).filter(|record| {
        record.kind == "character" && validation::valid_id_for_kind(character_id, "character")
    });
    let character = character.ok_or_else(|| Error::NotFound(UNAVAILABLE.into()))?;
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    Ok(character)
}

fn envelope(character_id: &str, at: &StoryTime) -> Value {
    json!({"protocol":crate::CONTRACT,"perspective":"character",
        "scope":"explicit-authored-evidence","characterId":character_id,"effectiveTime":at})
}

#[derive(Clone)]
struct Item {
    kind: &'static str,
    record_id: String,
    member_id: String,
    // Preserve the original source ordinal before admission removes siblings.
    source_field: Option<String>,
    at: StoryTime,
    value: Value,
    text: String,
}

fn sort(items: &mut [Item]) {
    items.sort_by(|a, b| {
        (a.at.tick, a.at.order, a.kind, &a.record_id, &a.member_id).cmp(&(
            b.at.tick,
            b.at.order,
            b.kind,
            &b.record_id,
            &b.member_id,
        ))
    });
}

fn beliefs(world: &World, character_id: &str, at: &StoryTime) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    for record in world.records.values().filter(|record| {
        record.kind == "knowledge"
            && record.status() == "canonical"
            && record.string("knower") == Some(character_id)
    }) {
        if !crate::narrative::labels_match(record, character_id).map_err(|_| malformed())? {
            continue;
        }
        // Typed generational and assertion-linked claims have separate learning
        // rules. Only their dedicated safe projections may enter this packet.
        if record
            .get("claim")
            .and_then(|claim| claim.get("genealogy").or_else(|| claim.get("assertion_id")))
            .is_some()
        {
            continue;
        }
        let mut latest: Option<(&Yaml, StoryTime)> = None;
        for transition in list(record.get("transitions"))? {
            let time = point(world, transition.get("time"))?;
            if time.not_after(at)
                && latest.as_ref().is_none_or(|(_, previous)| {
                    (time.tick, time.order) > (previous.tick, previous.order)
                })
            {
                latest = Some((transition, time));
            }
        }
        let Some((transition, time)) = latest else {
            continue;
        };
        let state = text(transition.get("state"))?;
        if state == "forgotten" {
            continue;
        }
        if ![
            "accepted",
            "suspected",
            "rejected",
            "uncertain",
            "remembered",
        ]
        .contains(&state)
        {
            return Err(malformed());
        }
        let statement = text(record.get("claim").and_then(|claim| claim.get("statement")))?;
        let member = text(transition.get("id"))?;
        let confidence = transition
            .get("confidence")
            .and_then(Yaml::as_f64)
            .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
            .ok_or_else(malformed)?;
        let value = json!({"knowledgeId":record.id,"transitionId":member,
            "statement":statement,"state":state,"confidence":confidence,
            "acquisition":text(transition.get("acquisition"))?,"time":time});
        items.push(Item {
            kind: "belief",
            record_id: record.id.clone(),
            member_id: member.into(),
            source_field: None,
            at: time,
            value,
            text: statement.into(),
        });
    }
    for value in crate::generational::knowledge::admitted(world, character_id, at, false)? {
        let record_id = value["knowledgeId"]
            .as_str()
            .ok_or_else(malformed)?
            .to_owned();
        let member_id = value["transitionId"]
            .as_str()
            .ok_or_else(malformed)?
            .to_owned();
        let time = wire_time(&value["time"])?;
        let mut searchable = vec![value["kind"].as_str().ok_or_else(malformed)?.to_owned()];
        if let Some(labels) = value["labels"].as_object() {
            for label in labels.values() {
                searchable.push(label.as_str().ok_or_else(malformed)?.to_owned());
            }
        }
        items.push(Item {
            kind: "generational-belief",
            record_id,
            member_id,
            source_field: None,
            at: time,
            value,
            text: searchable.join("\n"),
        });
    }
    for value in crate::learned_assertions::admitted(world, character_id, at)? {
        items.push(Item {
            kind: "assertion-belief",
            record_id: value["knowledgeId"]
                .as_str()
                .ok_or_else(malformed)?
                .to_owned(),
            member_id: value["transitionId"]
                .as_str()
                .ok_or_else(malformed)?
                .to_owned(),
            source_field: None,
            at: wire_time(&value["time"])?,
            text: value["statement"]
                .as_str()
                .ok_or_else(malformed)?
                .to_owned(),
            value,
        });
    }
    sort(&mut items);
    Ok(items)
}

fn beats(world: &World, record: &Record, character_id: &str, at: &StoryTime) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut speech_ids = std::collections::BTreeSet::new();
    let mut interruptions = std::collections::BTreeMap::new();
    for (index, turn) in list(record.get("turns"))?.iter().enumerate() {
        let time = scene::turn_time(world, record, turn).map_err(|_| malformed())?;
        if !time.not_after(at)
            || !scene::audible(world, record, turn, character_id).map_err(|_| malformed())?
        {
            continue;
        }
        let kind = turn
            .get("kind")
            .map(|value| text(Some(value)))
            .transpose()?
            .unwrap_or("speech");
        let member = text(turn.get("id"))?;
        let prose = text(turn.get("text"))?;
        let mut value = json!({"turnId":member,"kind":kind,"at":time,"text":prose});
        match kind {
            "speech" => {
                let speaker = text(turn.get("speaker"))?;
                if !scene::present(world, record, speaker, &time).map_err(|_| malformed())? {
                    return Err(malformed());
                }
                value["speakerId"] = json!(speaker);
                speech_ids.insert(member);
                if let Some(target) = turn.get("interrupts") {
                    interruptions.insert(member, text(Some(target))?);
                }
            }
            "action" => {
                let mut actors = Vec::new();
                for actor in list(turn.get("actors"))? {
                    let actor = text(Some(actor))?;
                    if !scene::present(world, record, actor, &time).map_err(|_| malformed())? {
                        return Err(malformed());
                    }
                    actors.push(actor);
                }
                actors.sort_unstable();
                value["actorIds"] = json!(actors);
            }
            _ => return Err(malformed()),
        }
        items.push(Item {
            kind: if kind == "speech" { "speech" } else { "action" },
            record_id: record.id.clone(),
            member_id: member.into(),
            source_field: Some(format!("turns[{index}]")),
            at: time,
            value,
            text: prose.into(),
        });
    }
    // The reference is not additional evidence. Emit it only if the exact
    // target speech independently passed this same horizon/audibility gate.
    // Hidden target IDs are borrowed above, never copied into output or text.
    for item in &mut items {
        if let Some(&target) = interruptions.get(item.member_id.as_str()) {
            if speech_ids.contains(target) {
                item.value["interrupts"] = json!(target);
            }
        }
    }
    sort(&mut items);
    Ok(items)
}

fn recollection(
    world: &World,
    record: &Record,
    character_id: &str,
    at: &StoryTime,
) -> Result<Option<Item>> {
    let mut latest: Option<(&Yaml, StoryTime, usize)> = None;
    for (index, memory) in list(record.get("recollections"))?.iter().enumerate() {
        if memory.get("character").and_then(Yaml::as_str) != Some(character_id) {
            continue;
        }
        let time = point(world, memory.get("at"))?;
        if time.not_after(at)
            && latest.as_ref().is_none_or(|(old, previous, _)| {
                (
                    time.tick,
                    time.order,
                    memory.get("id").and_then(Yaml::as_str),
                ) > (
                    previous.tick,
                    previous.order,
                    old.get("id").and_then(Yaml::as_str),
                )
            })
        {
            latest = Some((memory, time, index));
        }
    }
    let Some((memory, time, index)) = latest else {
        return Ok(None);
    };
    let state = text(memory.get("state"))?;
    if state == "forgotten" {
        return Ok(None);
    }
    if state != "remembered" {
        return Err(malformed());
    }
    let member = text(memory.get("id"))?;
    let summary = text(memory.get("summary"))?;
    let confidence = memory
        .get("confidence")
        .and_then(Yaml::as_f64)
        .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
        .ok_or_else(malformed)?;
    // Only admitted speech IDs may leave the projection. An exact memory
    // reference never causes us to fetch an otherwise hidden transcript.
    let heard = beats(world, record, character_id, &time)?;
    let heard_speech = |id: &str| {
        heard
            .iter()
            .find(|item| item.kind == "speech" && item.member_id == id)
    };
    let mut exact = Vec::new();
    for id in list(memory.get("exact_turns"))? {
        let id = text(Some(id))?;
        if heard_speech(id).is_some() {
            exact.push(id);
        }
    }
    exact.sort_unstable();
    exact.dedup();
    let mut quotes = Vec::new();
    let mut searchable = vec![summary.to_owned()];
    for quote in list(memory.get("remembered_quotes"))? {
        let prose = text(quote.get("text"))?;
        let fidelity = text(quote.get("fidelity"))?;
        let mut value = json!({"text":prose,"fidelity":fidelity});
        if let Some(id) = quote.get("source_turn").and_then(Yaml::as_str) {
            if let Some(heard) = heard_speech(id) {
                value["sourceTurnId"] = json!(id);
                // Preserve the remembered attribution only when that identity
                // is already visible as the referenced speech's speaker.
                if quote.get("speaker").and_then(Yaml::as_str) == heard.value["speakerId"].as_str()
                {
                    value["speakerId"] = heard.value["speakerId"].clone();
                }
            }
        }
        searchable.push(prose.into());
        quotes.push(value);
    }
    let mut value = json!({"recollectionId":member,"at":time,"state":state,"summary":summary,
        "confidence":confidence,"exactTurns":exact,"rememberedQuotes":quotes});
    for (source, output) in [
        ("interpretation", "interpretation"),
        ("emotional_impression", "emotionalImpression"),
    ] {
        if let Some(prose) = memory.get(source).filter(|value| !value.is_null()) {
            let prose = text(Some(prose))?;
            value[output] = json!(prose);
            searchable.push(prose.into());
        }
    }
    Ok(Some(Item {
        kind: "recollection",
        record_id: record.id.clone(),
        member_id: member.into(),
        source_field: Some(format!("recollections[{index}]")),
        at: time,
        value,
        text: searchable.join("\n"),
    }))
}

struct Conversation {
    id: String,
    beats: Vec<Item>,
    recollection: Option<Item>,
}

impl Conversation {
    fn value(&self) -> Value {
        json!({"conversationId":self.id,"beats":self.beats.iter().map(|item|&item.value).collect::<Vec<_>>(),
            "recollection":self.recollection.as_ref().map(|item|&item.value)})
    }
}

fn admitted_conversation(
    world: &World,
    record: &Record,
    character_id: &str,
    at: &StoryTime,
) -> Result<Option<Conversation>> {
    if !["active", "closed"].contains(&record.status()) {
        return Ok(None);
    }
    let beats = beats(world, record, character_id, at)?;
    let recollection = recollection(world, record, character_id, at)?;
    if beats.is_empty() && recollection.is_none() {
        return Ok(None);
    }
    Ok(Some(Conversation {
        id: record.id.clone(),
        beats,
        recollection,
    }))
}

struct Evidence {
    beliefs: Vec<Item>,
    descriptions: Vec<Item>,
    observations: Vec<Item>,
    conversations: Vec<Conversation>,
}

impl Evidence {
    fn documents(&self) -> Vec<&Item> {
        let mut items: Vec<_> = self
            .beliefs
            .iter()
            .chain(&self.descriptions)
            .chain(&self.observations)
            .chain(self.conversations.iter().flat_map(|conversation| {
                conversation
                    .beats
                    .iter()
                    .chain(conversation.recollection.iter())
            }))
            .collect();
        items.sort_by_key(|item| (item.kind, &item.record_id, &item.member_id));
        items
    }
}

fn evidence(
    world: &World,
    character_id: &str,
    at: &StoryTime,
    scope: Option<&crate::narrative::CharacterScope<'_>>,
) -> Result<Evidence> {
    selected(world, character_id, at)?;
    let descriptions = match scope {
        Some(scope) => {
            crate::narrative::admitted_descriptions_in_scope(world, character_id, at, scope)
        }
        None => crate::narrative::admitted_descriptions(world, character_id, at),
    }?
    .into_iter()
    .map(|value| {
        Ok(Item {
            kind: "description",
            record_id: value["ownerId"].as_str().ok_or_else(malformed)?.to_owned(),
            member_id: value["sectionId"]
                .as_str()
                .ok_or_else(malformed)?
                .to_owned(),
            source_field: None,
            at: at.clone(),
            text: format!(
                "{}\n{}",
                value["heading"].as_str().ok_or_else(malformed)?,
                value["markdown"].as_str().ok_or_else(malformed)?
            ),
            value,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let mut observations = Vec::new();
    let mut conversations = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| ["active", "closed"].contains(&record.status()))
    {
        if record.kind == "scene" {
            for (index, observation) in list(record.get("observations"))?.iter().enumerate() {
                if !scene::observation_visible(world, record, observation, character_id, at)
                    .map_err(|_| malformed())?
                {
                    continue;
                }
                let time = point(world, observation.get("at"))?;
                let member = text(observation.get("id"))?;
                let prose = text(observation.get("text"))?;
                observations.push(Item {kind:"observation",record_id:record.id.clone(),member_id:member.into(),source_field:Some(format!("observations[{index}]")),at:time.clone(),
                    value:json!({"sceneId":record.id,"observationId":member,"at":time,"text":prose}),text:prose.into()});
            }
        } else if record.kind == "conversation" {
            if let Some(value) = admitted_conversation(world, record, character_id, at)? {
                conversations.push(value);
            }
        }
    }
    sort(&mut observations);
    Ok(Evidence {
        beliefs: beliefs(world, character_id, at)?,
        descriptions,
        observations,
        conversations,
    })
}

/// Complete character packet. Budget checking happens only after admission.
pub fn context(
    world: &World,
    character_id: &str,
    at: &StoryTime,
    max_chars: usize,
) -> Result<Value> {
    context_inner(world, character_id, at, None, max_chars)
}

pub(crate) fn context_in_scope(
    world: &World,
    character_id: &str,
    at: &StoryTime,
    scope: &crate::narrative::CharacterScope<'_>,
    max_chars: usize,
) -> Result<Value> {
    context_inner(world, character_id, at, Some(scope), max_chars)
}

fn context_inner(
    world: &World,
    character_id: &str,
    at: &StoryTime,
    scope: Option<&crate::narrative::CharacterScope<'_>>,
    max_chars: usize,
) -> Result<Value> {
    if !(1..=1_000_000).contains(&max_chars) {
        return Err(Error::InvalidInput(
            "max_chars must be between 1 and 1000000".into(),
        ));
    }
    let evidence = evidence(world, character_id, at, scope)?;
    let mut result = envelope(character_id, at);
    result["beliefs"] = json!(
        evidence
            .beliefs
            .iter()
            .map(|item| &item.value)
            .collect::<Vec<_>>()
    );
    result["observations"] = json!(
        evidence
            .observations
            .iter()
            .map(|item| &item.value)
            .collect::<Vec<_>>()
    );
    if !evidence.descriptions.is_empty() {
        result["descriptions"] = json!(
            evidence
                .descriptions
                .iter()
                .map(|item| &item.value)
                .collect::<Vec<_>>()
        );
    }
    result["conversations"] = json!(
        evidence
            .conversations
            .iter()
            .map(Conversation::value)
            .collect::<Vec<_>>()
    );
    result["maxCharacters"] = json!(max_chars);
    let needed = serde_json::to_string(&result)?.chars().count();
    if needed > max_chars {
        return Err(Error::InvalidInput(format!(
            "complete character context needs {needed} characters; max_chars is {max_chars}"
        )));
    }
    Ok(result)
}

/// Dated conversation evidence, with indistinguishable missing/inaccessible IDs.
pub fn conversation(
    world: &World,
    character_id: &str,
    conversation_id: &str,
    at: &StoryTime,
) -> Result<Value> {
    selected(world, character_id, at)?;
    let record = world
        .records
        .get(conversation_id)
        .filter(|record| {
            record.kind == "conversation"
                && validation::valid_id_for_kind(conversation_id, "conversation")
        })
        .ok_or_else(|| Error::NotFound(UNAVAILABLE.into()))?;
    let conversation = admitted_conversation(world, record, character_id, at)?
        .ok_or_else(|| Error::NotFound(UNAVAILABLE.into()))?;
    let mut result = envelope(character_id, at);
    let value = conversation.value();
    for key in ["conversationId", "beats", "recollection"] {
        result[key] = value[key].clone();
    }
    Ok(result)
}

/// Literal-word AND search in an ephemeral admitted-only FTS corpus.
/// No global author index, on-disk cache, hidden count, or global score is used.
pub fn search(
    world: &World,
    character_id: &str,
    query: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Value> {
    if query.trim().is_empty() || query.len() > 4096 {
        return Err(Error::InvalidInput(
            "query must contain 1–4096 UTF-8 bytes".into(),
        ));
    }
    if !(1..=100).contains(&limit) {
        return Err(Error::InvalidInput(
            "limit must be between 1 and 100".into(),
        ));
    }
    let words: Vec<_> = query.split_whitespace().collect();
    if words.len() > 64 {
        return Err(Error::InvalidInput(
            "query must contain at most 64 words".into(),
        ));
    }
    let expression = words
        .iter()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    let evidence = evidence(world, character_id, at, None)?;
    let documents = evidence.documents();
    let mut connection = Connection::open_in_memory()?;
    connection.execute_batch("CREATE VIRTUAL TABLE admitted_fts USING fts5(text, kind UNINDEXED, record_id UNINDEXED, member_id UNINDEXED, tokenize='unicode61');")?;
    let transaction = connection.transaction()?;
    for (index, item) in documents.iter().enumerate() {
        transaction.execute(
            "INSERT INTO admitted_fts(rowid,text,kind,record_id,member_id) VALUES(?1,?2,?3,?4,?5)",
            params![
                index as i64 + 1,
                item.text,
                item.kind,
                item.record_id,
                item.member_id
            ],
        )?;
    }
    transaction.commit()?;
    let total: i64 = connection.query_row(
        "SELECT count(*) FROM admitted_fts WHERE admitted_fts MATCH ?1",
        [&expression],
        |row| row.get(0),
    )?;
    let mut statement=connection.prepare("SELECT rowid,snippet(admitted_fts,0,'','',' … ',24),bm25(admitted_fts) FROM admitted_fts WHERE admitted_fts MATCH ?1 ORDER BY bm25(admitted_fts),kind,record_id,member_id LIMIT ?2")?;
    let rows = statement.query_map(params![expression, limit as i64], |row| {
        let index: usize = row.get::<_, i64>(0)? as usize - 1;
        let item = documents[index];
        Ok(
            json!({"kind":item.kind,"recordId":item.record_id,"memberId":item.member_id,
            "snippet":row.get::<_,String>(1)?,"score":row.get::<_,f64>(2)?,"value":item.value}),
        )
    })?;
    let items = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let mut result = envelope(character_id, at);
    result["query"] = json!(query);
    result["limit"] = json!(limit);
    result["total"] = json!(total);
    result["items"] = json!(items);
    Ok(result)
}

/// Charge the shared complete input bounds before copying owned search input.
fn charge_retrieval_input(count: &mut usize, bytes: &mut usize, lengths: &[usize]) -> Result<()> {
    *count = count
        .checked_add(1)
        .ok_or_else(crate::retrieval::bounds_error)?;
    for length in lengths {
        *bytes = bytes
            .checked_add(*length)
            .ok_or_else(crate::retrieval::bounds_error)?;
    }
    if *count > crate::retrieval::MAX_INPUTS || *bytes > crate::retrieval::MAX_BYTES {
        return Err(crate::retrieval::bounds_error());
    }
    Ok(())
}

/// The semantic prior is ordinary, explicit public source, never character
/// evidence. Only timeless descriptions on unqualified world/location/object
/// owners train it; returned candidates still come exclusively from evidence().
fn public_timeless_prior(
    world: &World,
    count: &mut usize,
    bytes: &mut usize,
) -> Result<Vec<String>> {
    let owner_allows = |record: &Record, field: &str, label: &str| {
        record.get(field).is_none()
            || record
                .list(field)
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(label)))
    };
    let mut training = Vec::new();
    for owner in world.records.values().filter(|record| {
        matches!(record.kind.as_str(), "world" | "location" | "object")
            && matches!(record.status(), "canonical" | "active" | "closed")
            && record.get("valid").is_none()
            && record.get("time").is_none()
            && owner_allows(record, "audience", "public")
            && owner_allows(record, "perspectives", "ordinary")
    }) {
        let eligible = list(owner.get("descriptions"))?
            .iter()
            .filter(|section| {
                section
                    .get("valid")
                    .and_then(|valid| valid.get("kind"))
                    .and_then(Yaml::as_str)
                    == Some("timeless")
                    && section
                        .get("audience")
                        .and_then(Yaml::as_sequence)
                        .is_some_and(|values| {
                            values.iter().any(|value| value.as_str() == Some("public"))
                        })
                    && section
                        .get("perspectives")
                        .and_then(Yaml::as_sequence)
                        .is_some_and(|values| {
                            values
                                .iter()
                                .any(|value| value.as_str() == Some("ordinary"))
                        })
            })
            .collect::<Vec<_>>();
        let replaced = eligible
            .iter()
            .filter_map(|section| section.get("replaces").and_then(Yaml::as_str))
            .collect::<std::collections::BTreeSet<_>>();
        for section in eligible {
            if replaced.contains(text(section.get("id"))?) {
                continue;
            }
            let heading = text(section.get("heading"))?;
            let markdown = text(section.get("markdown"))?;
            charge_retrieval_input(count, bytes, &[heading.len(), 1, markdown.len()])?;
            training.push(format!("{heading}\n{markdown}"));
        }
    }
    Ok(training)
}

/// Explicit local semantic/hybrid retrieval over the same admitted corpus.
/// Lexical mode is the original function and retains its exact output contract.
pub fn search_with_mode(
    world: &World,
    character_id: &str,
    query: &str,
    at: &StoryTime,
    limit: usize,
    mode: crate::retrieval::Mode,
) -> Result<Value> {
    if mode == crate::retrieval::Mode::Lexical {
        return search(world, character_id, query, at, limit);
    }
    crate::retrieval::validate_request(query, limit)?;
    if !validation::validate_world(world).valid {
        return Err(malformed());
    }
    let evidence = evidence(world, character_id, at, None)?;
    let admitted = evidence.documents();
    let mut count = 0usize;
    let mut bytes = query.len();
    for item in &admitted {
        charge_retrieval_input(
            &mut count,
            &mut bytes,
            &[
                item.kind.len(),
                1,
                item.record_id.len(),
                1,
                item.member_id.len(),
                item.text.len(),
                1,
            ],
        )?;
    }
    let training = public_timeless_prior(world, &mut count, &mut bytes)?;
    if training.is_empty() {
        return Err(Error::InvalidInput(
            "semantic retrieval requires an eligible public timeless description prior".into(),
        ));
    }
    let documents = admitted
        .iter()
        .map(|item| crate::retrieval::Document {
            key: format!("{}\0{}\0{}", item.kind, item.record_id, item.member_id),
            title: String::new(),
            text: item.text.clone(),
        })
        .collect::<Vec<_>>();
    let ranked = crate::retrieval::rank(query, &documents, &training, mode, limit, false)?;
    let mut items = Vec::new();
    for hit in ranked.hits {
        let item = admitted[hit.index];
        let mut value = json!({"kind":item.kind,"recordId":item.record_id,
            "memberId":item.member_id,"value":item.value});
        hit.write_metadata(&mut value);
        items.push(value);
    }
    let mut result = envelope(character_id, at);
    result["query"] = json!(query);
    result["limit"] = json!(limit);
    result["total"] = json!(ranked.total);
    result["items"] = json!(items);
    result["mode"] = json!(mode.as_str());
    result["retrieval"] = crate::retrieval::model_metadata(ranked.dimensions);
    Ok(result)
}

/// Request-local catalogue reuse of the same admitted evidence. The caller's
/// already admitted record selection only narrows this result; it grants no
/// owner, prose, scene, observation or learned target.
pub(crate) struct CatalogueDocument {
    pub kind: &'static str,
    pub record_id: String,
    pub member_id: String,
    /// Observations, beats and recollections are fields, not description sections.
    pub source_field: Option<String>,
    pub text: String,
    pub value: Value,
}

pub(crate) fn catalogue_documents(
    world: &World,
    viewer: &str,
    at: &StoryTime,
    scope: &crate::narrative::CharacterScope<'_>,
    eligible: &std::collections::BTreeSet<String>,
) -> Result<Vec<CatalogueDocument>> {
    if !scope.matches(world, viewer, at) {
        return Err(malformed());
    }
    let evidence = evidence(world, viewer, at, Some(scope))?;
    Ok(evidence
        .beliefs
        .into_iter()
        .chain(evidence.descriptions)
        .chain(evidence.observations)
        .chain(evidence.conversations.into_iter().flat_map(|conversation| {
            conversation
                .beats
                .into_iter()
                .chain(conversation.recollection)
        }))
        .filter(|item| eligible.contains(&item.record_id))
        .map(|item| CatalogueDocument {
            kind: item.kind,
            record_id: item.record_id,
            member_id: item.member_id,
            source_field: item.source_field,
            text: item.text,
            value: item.value,
        })
        .collect())
}

/// The catalogue shares the existing fixed public description prior. Candidate
/// selection never teaches a character or silently supplies a fitted fallback.
pub(crate) fn catalogue_prior(
    world: &World,
    count: &mut usize,
    bytes: &mut usize,
) -> Result<Vec<String>> {
    public_timeless_prior(world, count, bytes)
}
