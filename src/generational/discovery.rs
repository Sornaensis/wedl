//! Horizon-bound names, literal search, and complete cited context packets.
//! Learned views consume admitted assertions only. In particular, their names
//! never fall back to an entity's canonical title, alias, or source metadata.
use super::*;

const NAME_KINDS: &[&str] = &["character", "organization", "legacy", "event"];

fn name_kind(id: &str) -> Option<&'static str> {
    NAME_KINDS
        .iter()
        .copied()
        .find(|kind| valid_id_for_kind(id, kind))
}

fn ids_valid(ids: &[String]) -> Result<()> {
    if ids.is_empty()
        || ids.len() > 1000
        || ids.iter().any(|id| name_kind(id).is_none())
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err(Error::InvalidInput(
            "labels require 1..1000 unique character, organization, legacy, or event IDs".into(),
        ));
    }
    Ok(())
}

fn prefix_valid(kind: &str, prefix: &str) -> Result<String> {
    if !NAME_KINDS.contains(&kind) || prefix.chars().count() > 64 {
        return Err(Error::InvalidInput(
            "discovery requires a supported name kind and a prefix of at most 64 characters".into(),
        ));
    }
    Ok(prefix.trim().to_lowercase())
}

fn words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn query_words(value: &str) -> Result<Vec<String>> {
    let words = words(value);
    if words.is_empty()
        || words.len() > 8
        || words.iter().any(|word| word.chars().count() > 32)
        || value.chars().count() > 512
    {
        return Err(Error::InvalidInput(
            "generational search requires 1..8 word prefixes of at most 32 characters".into(),
        ));
    }
    Ok(words)
}

fn matches_words(text: &str, query: &[String]) -> bool {
    let words = words(text);
    query
        .iter()
        .all(|prefix| words.iter().any(|word| word.starts_with(prefix)))
}

fn learned_envelope(viewer: &str, at: &StoryTime, limit: usize) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"character",
        "scope":"explicit-authored-evidence","characterId":viewer,"effectiveTime":at,
        "complete":true,"limit":limit})
}

fn finish(mut result: Json, field: &str, rows: Vec<Json>, limit: usize) -> Result<Json> {
    bounded(rows.len(), limit)?;
    result["available"] = json!(!rows.is_empty());
    result["count"] = json!(rows.len());
    result[field] = json!(rows);
    Ok(result)
}

/// Returns one row per explicit assertion label, preserving disagreements and
/// provenance. Holding an expired assertion retains its labels; rejecting or
/// forgetting it removes them through the common admission gate.
fn learned_names(rows: &[Json]) -> Vec<Json> {
    let mut labels = Vec::new();
    for row in rows {
        for (id, label) in row["labels"].as_object().into_iter().flatten() {
            let Some(kind) = name_kind(id) else {
                continue;
            };
            labels.push(
                json!({"id":id,"kind":kind,"title":label,"matchedName":label,
                "knowledgeId":row["knowledgeId"],"learnedAt":row["learnedAt"],
                "beliefState":row["beliefState"],"confidence":row["confidence"],
                "applicable":row["applicable"],"uncertain":row["uncertain"],
                "citations":row["citations"],"evidence":row["evidence"]}),
            );
        }
    }
    labels.sort_by_key(|row| {
        (
            row["title"].as_str().unwrap_or_default().to_lowercase(),
            row["id"].as_str().unwrap_or_default().to_owned(),
            row["knowledgeId"].as_str().unwrap_or_default().to_owned(),
        )
    });
    labels
}

/// Resolves only an exact typed ID or an unambiguous held, explicit learned
/// label. Exact IDs do not probe canonical existence or resolve canonical names.
pub fn resolve_learned(
    world: &World,
    viewer: &str,
    selector: &str,
    at: &StoryTime,
    kind: Option<&str>,
) -> Result<String> {
    let rows = super::knowledge::admitted(world, viewer, at, true)?;
    if kind.is_some_and(|kind| !NAME_KINDS.contains(&kind)) {
        return Err(Error::InvalidInput("unsupported learned name kind".into()));
    }
    if name_kind(selector).is_some_and(|actual| kind.is_none_or(|kind| kind == actual)) {
        return Ok(selector.into());
    }
    let key = selector.trim().to_lowercase();
    let matches: BTreeSet<_> = learned_names(&rows)
        .into_iter()
        .filter(|row| {
            kind.is_none_or(|kind| row["kind"] == kind)
                && row["title"]
                    .as_str()
                    .is_some_and(|label| label.trim().to_lowercase() == key)
        })
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap()),
        0 => Err(Error::NotFound("learned name is unavailable".into())),
        _ => Err(Error::InvalidInput(
            "learned name is ambiguous; use an exact ID".into(),
        )),
    }
}

pub fn learned_labels(
    world: &World,
    viewer: &str,
    ids: &[String],
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    ids_valid(ids)?;
    let rows = super::knowledge::admitted(world, viewer, at, true)?;
    let labels = learned_names(&rows)
        .into_iter()
        .filter(|row| ids.iter().any(|id| row["id"] == *id))
        .collect();
    finish(learned_envelope(viewer, at, limit), "labels", labels, limit)
}

pub fn learned_discover(
    world: &World,
    viewer: &str,
    kind: &str,
    prefix: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    let prefix = prefix_valid(kind, prefix)?;
    let rows = super::knowledge::admitted(world, viewer, at, true)?;
    let labels = learned_names(&rows)
        .into_iter()
        .filter(|row| {
            row["kind"] == kind
                && row["title"]
                    .as_str()
                    .is_some_and(|label| label.trim().to_lowercase().starts_with(&prefix))
        })
        .collect();
    finish(
        learned_envelope(viewer, at, limit),
        "results",
        labels,
        limit,
    )
}

/// Searches assertion kinds and explicit labels only, with AND across Unicode
/// lowercase word prefixes. Literal narrative prose and canonical names do not
/// enter this corpus. Results include currently held expired assertions.
pub fn learned_search(
    world: &World,
    viewer: &str,
    text: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    let query = query_words(text)?;
    let mut rows = super::knowledge::admitted(world, viewer, at, true)?;
    rows.retain(|row| {
        let mut corpus = row["kind"].as_str().unwrap_or_default().to_owned();
        for label in row["labels"].as_object().into_iter().flatten() {
            corpus.push(' ');
            corpus.push_str(label.1.as_str().unwrap_or_default());
        }
        matches_words(&corpus, &query)
    });
    rows.sort_by(|left, right| {
        left["knowledgeId"]
            .as_str()
            .cmp(&right["knowledgeId"].as_str())
    });
    finish(learned_envelope(viewer, at, limit), "results", rows, limit)
}

// Author names are admitted by a visible canonical generational fact, never by
// scanning an undated entity list. Organization/legacy names use literal folded
// titles and previously visible effective names, not the record header title.
struct Name {
    id: String,
    kind: String,
    title: String,
    names: BTreeSet<String>,
    citations: Vec<Json>,
}

impl Name {
    fn json(&self, matched: Option<&str>) -> Json {
        let mut result = json!({"id":self.id,"kind":self.kind,"title":self.title,
            "citations":self.citations});
        if let Some(matched) = matched {
            result["matchedName"] = json!(matched);
        }
        result
    }
}

fn canonical_histories(world: &World, at: &StoryTime) -> Result<Vec<Json>> {
    canonical::require_valid(world)?;
    let mut rows = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| canonical(record) && GENERATIONAL_KINDS.contains(&record.kind.as_str()))
    {
        let mut row = super::history(world, &record.id, at, MAX_HISTORY)?;
        if row["available"] != true {
            continue;
        }
        if record.kind == "vital-history" {
            row["history"]
                .as_array_mut()
                .unwrap()
                .retain(|entry| entry["transitionKind"] != "vital-initialize");
            if row["history"].as_array().unwrap().is_empty() {
                continue;
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

fn add_entity_name(world: &World, names: &mut BTreeMap<String, Name>, id: &str, evidence: &Json) {
    let Some(record) = world.records.get(id).filter(|record| {
        canonical(record) && ["character", "event"].contains(&record.kind.as_str())
    }) else {
        return;
    };
    let name = names.entry(id.into()).or_insert_with(|| {
        let mut aliases = BTreeSet::from([record.title.clone()]);
        aliases.extend(
            record
                .list("aliases")
                .into_iter()
                .flatten()
                .filter_map(Yaml::as_str)
                .map(str::to_owned),
        );
        Name {
            id: record.id.clone(),
            kind: record.kind.clone(),
            title: record.title.clone(),
            names: aliases,
            citations: vec![citation(world, record, "title")],
        }
    });
    if !name.citations.contains(evidence) {
        name.citations.push(evidence.clone());
    }
}

fn author_names(world: &World, at: &StoryTime) -> Result<BTreeMap<String, Name>> {
    let mut names = BTreeMap::new();
    for row in canonical_histories(world, at)? {
        let record = &world.records[row["recordId"].as_str().unwrap()];
        if ["organization", "legacy"].contains(&record.kind.as_str()) {
            let title = row["value"]["title"].as_str().unwrap().to_owned();
            let mut aliases = BTreeSet::from([title.clone()]);
            for entry in row["history"].as_array().unwrap() {
                if entry["superseded"] == true {
                    continue;
                }
                if let Some(title) = entry["payload"]["title"].as_str() {
                    aliases.insert(title.into());
                }
                aliases.extend(
                    entry["payload"]["aliases"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Json::as_str)
                        .map(str::to_owned),
                );
            }
            names.insert(
                record.id.clone(),
                Name {
                    id: record.id.clone(),
                    kind: record.kind.clone(),
                    title,
                    names: aliases,
                    citations: row["citations"].as_array().unwrap().clone(),
                },
            );
        }
        for entry in row["history"].as_array().unwrap() {
            for key in [
                "child_id",
                "parent_id",
                "character_id",
                "claimant_id",
                "holder_id",
            ] {
                if let Some(id) = entry["payload"][key]
                    .as_str()
                    .or_else(|| record.string(key))
                {
                    add_entity_name(world, &mut names, id, &entry["citation"]);
                }
            }
            for id in entry["payload"]["participant_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
            {
                add_entity_name(world, &mut names, id, &entry["citation"]);
            }
            if let Some(id) = entry["causeEventId"].as_str() {
                add_entity_name(world, &mut names, id, &entry["citation"]);
            }
        }
    }
    Ok(names)
}

pub fn labels(world: &World, ids: &[String], at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    ids_valid(ids)?;
    let result = envelope(world, at, limit)?;
    let rows = author_names(world, at)?
        .into_values()
        .filter(|name| ids.contains(&name.id))
        .map(|name| name.json(None))
        .collect();
    finish(result, "labels", rows, limit)
}

pub fn discover(
    world: &World,
    kind: &str,
    prefix: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    let prefix = prefix_valid(kind, prefix)?;
    let result = envelope(world, at, limit)?;
    let mut matches = BTreeMap::new();
    for name in author_names(world, at)?
        .into_values()
        .filter(|name| name.kind == kind)
    {
        for matched in &name.names {
            let key = matched.trim().to_lowercase();
            if key.starts_with(&prefix) {
                matches
                    .entry((key, name.id.clone()))
                    .or_insert_with(|| name.json(Some(matched)));
            }
        }
    }
    finish(result, "results", matches.into_values().collect(), limit)
}

/// One result per matching canonical record. The matching literal transitions
/// and their citations remain visible; future/undisclosed transitions, source
/// prose, and record header titles never enter the structural search corpus.
pub fn search(world: &World, text: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    let query = query_words(text)?;
    let result = envelope(world, at, limit)?;
    let mut rows = Vec::new();
    for row in canonical_histories(world, at)? {
        let mut matches = Vec::new();
        for entry in row["history"].as_array().unwrap() {
            let structure = json!({"kind":row["kind"],"recordId":row["recordId"],
                "transitionKind":entry["transitionKind"],"payload":entry["payload"],
                "causeEventId":entry["causeEventId"],"replacesTransitionId":entry["replacesTransitionId"]});
            if matches_words(&serde_json::to_string(&structure)?, &query) {
                matches.push(entry.clone());
            }
        }
        if !matches.is_empty() {
            bounded(matches.len(), limit)?;
            rows.push(json!({"recordId":row["recordId"],"kind":row["kind"],"matches":matches}));
        }
    }
    finish(result, "results", rows, limit)
}

fn name_ids(value: &Json, ids: &mut BTreeSet<String>) {
    match value {
        Json::Array(values) => values.iter().for_each(|value| name_ids(value, ids)),
        Json::Object(values) => {
            for (key, value) in values {
                if key == "characterId"
                    && values
                        .get("perspective")
                        .is_some_and(|value| value == "character")
                {
                    continue;
                }
                if key.ends_with("Id")
                    || key.ends_with("_id")
                    || ["participantIds", "participant_ids", "from", "to"].contains(&key.as_str())
                {
                    for id in value.as_str().into_iter().chain(
                        value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Json::as_str),
                    ) {
                        if name_kind(id).is_some() {
                            ids.insert(id.into());
                        }
                    }
                }
                name_ids(value, ids);
            }
        }
        _ => {}
    }
}

fn context_budget(max_chars: usize) -> Result<()> {
    if !(80..=1_000_000).contains(&max_chars) {
        return Err(Error::InvalidInput(
            "generational context max_chars must be 80..1000000".into(),
        ));
    }
    Ok(())
}

fn packet(
    mut result: Json,
    subject: &str,
    items: Vec<Json>,
    labels: Vec<Json>,
    max_chars: usize,
) -> Result<Json> {
    result["subjectId"] = json!(subject);
    result["items"] = json!(items);
    result["labels"] = json!(labels);
    result["maxCharacters"] = json!(max_chars);
    result["truncated"] = json!(false);
    if serde_json::to_string(&result)?.chars().count() > max_chars {
        return Err(Error::InvalidInput(
            "generational context exceeds max_chars; increase the budget for a complete packet"
                .into(),
        ));
    }
    Ok(result)
}

fn section(items: &mut Vec<Json>, kind: &str, result: Json) {
    if result["count"].as_u64().unwrap_or_default() > 0 {
        items.push(json!({"kind":kind,"result":result}));
    }
}

/// Composes complete canonical family, ancestry, descendants, and disclosed
/// vital reads. A traversal or final serialized size overflow fails the entire
/// packet; no partial context is silently returned.
pub fn context(
    world: &World,
    subject: &str,
    at: &StoryTime,
    max_depth: usize,
    limit: usize,
    max_chars: usize,
) -> Result<Json> {
    limits(limit, max_depth)?;
    context_budget(max_chars)?;
    let result = envelope(world, at, limit)?;
    let subject = query_character(world, subject)?.id.as_str();
    let mut items = Vec::new();
    section(
        &mut items,
        "family",
        super::family(world, subject, at, limit)?,
    );
    for direction in [LineageDirection::Ancestors, LineageDirection::Descendants] {
        section(
            &mut items,
            if direction == LineageDirection::Ancestors {
                "ancestors"
            } else {
                "descendants"
            },
            super::lineage(world, subject, at, direction, max_depth, limit)?,
        );
    }
    section(
        &mut items,
        "vital",
        super::vital(world, subject, at, limit)?,
    );
    let mut ids = BTreeSet::from([subject.to_owned()]);
    for item in &items {
        name_ids(item, &mut ids);
    }
    let labels = author_names(world, at)?
        .into_values()
        .filter(|name| ids.contains(&name.id))
        .map(|name| name.json(None))
        .collect::<Vec<_>>();
    bounded(labels.len(), limit)?;
    packet(result, subject, items, labels, max_chars)
}

/// The same context shape using admitted character assertions only. Held
/// history is explicit even when no longer applicable, and labels are restricted
/// to the subject and endpoints appearing in the admitted packet.
pub fn learned_context(
    world: &World,
    viewer: &str,
    subject: &str,
    at: &StoryTime,
    max_depth: usize,
    limit: usize,
    max_chars: usize,
) -> Result<Json> {
    limits(limit, max_depth)?;
    context_budget(max_chars)?;
    let subject = resolve_learned(world, viewer, subject, at, Some("character"))?;
    let mut items = Vec::new();
    section(
        &mut items,
        "family",
        super::knowledge::family(world, viewer, &subject, at, limit)?,
    );
    for direction in [LineageDirection::Ancestors, LineageDirection::Descendants] {
        section(
            &mut items,
            if direction == LineageDirection::Ancestors {
                "ancestors"
            } else {
                "descendants"
            },
            super::knowledge::lineage(world, viewer, &subject, at, direction, max_depth, limit)?,
        );
    }
    section(
        &mut items,
        "vital",
        super::knowledge::vital(world, viewer, &subject, at, limit)?,
    );
    section(
        &mut items,
        "learned-history",
        super::knowledge::assertions(world, viewer, at, None, Some(&subject), true, limit)?,
    );
    let mut ids = BTreeSet::from([subject.clone()]);
    for item in &items {
        name_ids(item, &mut ids);
    }
    let rows = super::knowledge::admitted(world, viewer, at, true)?;
    let labels = learned_names(&rows)
        .into_iter()
        .filter(|row| row["id"].as_str().is_some_and(|id| ids.contains(id)))
        .collect::<Vec<_>>();
    bounded(labels.len(), limit)?;
    packet(
        learned_envelope(viewer, at, limit),
        &subject,
        items,
        labels,
        max_chars,
    )
}
