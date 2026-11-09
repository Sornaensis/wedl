//! One complete, bounded admitted corpus. Search grouping is applied only after
//! native ranking; a page limit never changes statistics or candidate scores.
use super::projection::{Selection, citation, learned_citation};
use super::{Lens, Page, Request, invalid, source_error};
use crate::{
    Result,
    model::{Record, World},
    narrative::CharacterScope,
    retrieval::{self, Document, Mode},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeSet;

struct Candidate {
    record_id: String,
    document: Document,
    kind: String,
    heading: Option<String>,
    citation: Json,
}
struct Corpus {
    candidates: Vec<Candidate>,
    bytes: usize,
}
impl Corpus {
    fn new(query: &str) -> Self {
        Self {
            candidates: Vec::new(),
            bytes: query.len(),
        }
    }
    fn charge(&mut self, size: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(size)
            .ok_or_else(|| invalid("CAT_TEXT_LIMIT"))?;
        if self.bytes > retrieval::MAX_BYTES {
            return Err(invalid("CAT_TEXT_LIMIT"));
        }
        Ok(())
    }
    fn push(
        &mut self,
        record_id: &str,
        key: String,
        kind: &str,
        title: &str,
        text: String,
        heading: Option<String>,
        citation: Json,
    ) -> Result<()> {
        if self.candidates.len() == retrieval::MAX_INPUTS {
            return Err(invalid("CAT_DOCUMENT_LIMIT"));
        }
        for size in [key.len(), title.len(), text.len(), 2] {
            self.charge(size)?;
        }
        self.candidates.push(Candidate {
            record_id: record_id.into(),
            document: Document {
                key,
                title: title.into(),
                text,
            },
            kind: kind.into(),
            heading,
            citation,
        });
        Ok(())
    }
}
fn text(parts: impl IntoIterator<Item = String>) -> Result<String> {
    let mut out = String::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let needed = out
            .len()
            .checked_add(part.len())
            .and_then(|n| n.checked_add(usize::from(!out.is_empty())))
            .ok_or_else(|| invalid("CAT_TEXT_LIMIT"))?;
        if needed > retrieval::MAX_BYTES {
            return Err(invalid("CAT_TEXT_LIMIT"));
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&part);
    }
    Ok(out)
}
fn scalar_text(value: &Json, out: &mut String) -> Result<()> {
    fn push(out: &mut String, value: &str) -> Result<()> {
        if out.len().saturating_add(value.len()).saturating_add(1) > retrieval::MAX_BYTES {
            return Err(invalid("CAT_TEXT_LIMIT"));
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(value);
        Ok(())
    }
    match value {
        Json::String(value) => push(out, value),
        Json::Null => push(out, "null"),
        Json::Bool(value) => push(out, if *value { "true" } else { "false" }),
        Json::Number(value) => push(out, &value.to_string()),
        Json::Array(values) => {
            for value in values {
                scalar_text(value, out)?;
            }
            Ok(())
        }
        Json::Object(values) => {
            for (key, value) in values {
                push(out, key)?;
                scalar_text(value, out)?;
            }
            Ok(())
        }
    }
}
fn ordinary(record: &Record) -> bool {
    !matches!(
        record.kind.as_str(),
        "organization"
            | "parentage"
            | "union"
            | "affiliation"
            | "legacy"
            | "tenure"
            | "claim"
            | "vital-history"
            | "lore-concept"
            | "assertion"
            | "context-brief"
            | "existence-episode"
    ) && !(record.kind == "knowledge"
        && record
            .get("claim")
            .is_some_and(|claim| claim.get("assertion_id").is_some()))
}
fn effect_admitted(world: &World, effect: &Yaml, allowed: &BTreeSet<String>) -> Result<bool> {
    let target = effect
        .get("target")
        .and_then(Yaml::as_str)
        .ok_or_else(source_error)?;
    if !allowed.contains(target) {
        return Ok(false);
    }
    let key = effect
        .get("key")
        .and_then(Yaml::as_str)
        .ok_or_else(source_error)?;
    let definition = crate::state_schema::field_definition(world, &world.records[target].kind, key)
        .map_err(|_| source_error())?;
    let admits = |value: &Yaml| {
        value
            .get("entity")
            .and_then(Yaml::as_str)
            .is_some_and(|id| allowed.contains(id))
    };
    let Some(value) = effect.get("value") else {
        return Ok(true);
    };
    Ok(match definition.value_type {
        crate::state_schema::StateType::Entity { .. } => admits(value),
        crate::state_schema::StateType::EntitySet { .. } => match value.as_sequence() {
            Some(values) => values.iter().all(admits),
            None => admits(value),
        },
        _ => true,
    })
}
fn registry(world: &World) -> Result<crate::lore_types::Registry> {
    match world.world_record()?.get("profiles") {
        Some(value) => crate::lore_types::Registry::parse(value).map_err(|_| source_error()),
        None => Ok(crate::lore_types::Registry::default()),
    }
}

pub(super) fn select(
    world: &World,
    request: &Request,
    page: &Page,
    selected: &Selection,
    scope: Option<&CharacterScope<'_>>,
) -> Result<(Vec<Json>, Json)> {
    let query = page.query.as_deref().expect("search request");
    let mode = page.mode.expect("paired search mode");
    let mut corpus = Corpus::new(query);
    let registry = registry(world)?;
    let bindings = crate::lore_state::Bindings::parse(world).map_err(|_| source_error())?;
    for (id, row) in &selected.rows {
        let record = &world.records[id];
        if row.learned.is_some() {
            continue;
        }
        let title = row.item["title"].as_str().unwrap();
        // Identity words are admitted before this point. Character prose comes
        // exclusively from explicit learned/observed/described evidence below.
        let aggregate_closed = !matches!(record.kind.as_str(), "scene" | "conversation")
            || record
                .get("time")
                .and_then(|time| time.get("end"))
                .is_some_and(|end| !end.is_null());
        let dated_body = request
            .at
            .as_ref()
            .map(|at| crate::narrative::author_body_visible(world, record, at))
            .transpose()
            .map_err(|_| source_error())?
            .unwrap_or(true);
        let body_allowed = scope.is_none() && ordinary(record) && aggregate_closed && dated_body;
        let mut parts = record
            .list("aliases")
            .into_iter()
            .flatten()
            .filter_map(Yaml::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        parts.push(title.to_owned());
        if body_allowed {
            parts.push(record.body.clone());
        }
        let prose = text(parts)?;
        corpus.push(
            id,
            format!("{id}:metadata"),
            "metadata",
            title,
            prose,
            None,
            citation(world, record, None),
        )?;
        if record.kind == "lore-concept" {
            let profile = registry
                .get(record.string("profile").ok_or_else(source_error)?)
                .ok_or_else(source_error)?;
            let (values, _) = crate::lore::project_value(
                profile,
                record.get("values").ok_or_else(source_error)?,
                &selected.allowed,
            )
            .map_err(|_| source_error())?;
            let mut prose = String::new();
            scalar_text(&values, &mut prose)?;
            if let Some(domain) = record.string("domain") {
                prose = text([domain.into(), prose])?;
            }
            if scope.is_none() {
                prose = text([prose, record.body.clone()])?;
            }
            corpus.push(
                id,
                format!("{id}:lore-concept"),
                "lore-concept",
                title,
                prose,
                None,
                citation(world, record, None),
            )?;
        }
        if let Some(assertion) = selected.assertions.get(id) {
            let mut prose = String::new();
            for key in ["predicate", "classification", "value"] {
                scalar_text(&assertion[key], &mut prose)?;
            }
            corpus.push(
                id,
                format!("{id}:assertion"),
                "assertion",
                title,
                prose,
                None,
                citation(world, record, None),
            )?;
        }
        if scope.is_none() {
            for value in crate::description::full_author_values(world, record, request.at.as_ref())
                .map_err(|_| source_error())?
            {
                if value["valid"]["kind"] == "interval"
                    && value["valid"]["timeline"].as_str() != Some(request.timeline())
                {
                    continue;
                }
                let section = value["sectionId"].as_str().ok_or_else(source_error)?;
                let heading = value["heading"].as_str().ok_or_else(source_error)?;
                let prose = text([
                    heading.into(),
                    value["markdown"].as_str().ok_or_else(source_error)?.into(),
                ])?;
                corpus.push(
                    id,
                    format!("{id}:description:{section}"),
                    "description",
                    title,
                    prose,
                    Some(heading.into()),
                    citation(world, record, Some(section)),
                )?;
            }
            if ordinary(record) {
                let emit = |structured: crate::compiler::structured::Structured<'_>| {
                    let mut source = citation(world, record, None);
                    source["field"] = json!(structured.path);
                    corpus.push(
                        id,
                        format!("{id}:{}:{}", structured.kind, structured.path),
                        structured.kind,
                        title,
                        structured.text,
                        None,
                        source,
                    )
                };
                crate::compiler::structured::collect_scoped(
                    world,
                    record,
                    request.at.as_ref(),
                    request.timeline(),
                    &bindings,
                    |effect| effect_admitted(world, effect, &selected.allowed),
                    emit,
                )
                .map_err(|error| match error {
                    crate::Error::InvalidInput(_) => invalid("CAT_TEXT_LIMIT"),
                    _ => source_error(),
                })?;
            }
        }
    }
    if let (Some(scope), Lens::Character { character, .. }) = (scope, &request.lens) {
        let eligible = selected.rows.keys().cloned().collect::<BTreeSet<_>>();
        let documents = crate::character::catalogue_documents(
            world,
            character,
            request.at.as_ref().unwrap(),
            scope,
            &eligible,
        )
        .map_err(|_| source_error())?;
        for document in documents {
            let row = &selected.rows[&document.record_id];
            let record = &world.records[&document.record_id];
            let cite = if let Some(learned) = &row.learned {
                learned_citation(world, learned)
            } else if document.kind == "belief"
                || document.kind == "assertion-belief"
                || document.kind == "generational-belief"
            {
                learned_citation(world, &document.value)
            } else if document.kind == "description" {
                citation(world, record, Some(&document.member_id))
            } else {
                // Other character evidence names an exact field on its owner;
                // it must not fabricate a canonical description-section target.
                let mut source = citation(world, record, None);
                source["field"] = json!(document.source_field.as_deref().ok_or_else(source_error)?);
                source
            };
            let heading = document
                .value
                .get("heading")
                .and_then(Json::as_str)
                .map(str::to_owned);
            corpus.push(
                &document.record_id,
                format!(
                    "{}:{}:{}",
                    document.record_id, document.kind, document.member_id
                ),
                document.kind,
                row.item["title"].as_str().unwrap(),
                document.text,
                heading,
                cite,
            )?;
        }
    }
    let mut training = Vec::new();
    if mode != Mode::Lexical {
        if scope.is_some() {
            let mut count = corpus.candidates.len();
            let mut bytes = corpus.bytes;
            training = crate::character::catalogue_prior(world, &mut count, &mut bytes)
                .map_err(|_| invalid("CAT_TEXT_LIMIT"))?;
            if training.is_empty() {
                return Err(invalid("CAT_PROFILE"));
            }
        } else {
            let mut seen = BTreeSet::new();
            for index in 0..corpus.candidates.len() {
                let document = &corpus.candidates[index].document;
                let prose = text([document.title.clone(), document.text.clone()])?;
                if !seen.contains(&prose) {
                    if corpus.candidates.len() + training.len() == retrieval::MAX_INPUTS {
                        return Err(invalid("CAT_DOCUMENT_LIMIT"));
                    }
                    corpus.charge(prose.len())?;
                    seen.insert(prose.clone());
                    training.push(prose);
                }
            }
        }
    }
    let candidates = corpus.candidates;
    let documents = candidates
        .iter()
        .map(|candidate| Document {
            key: candidate.document.key.clone(),
            title: candidate.document.title.clone(),
            text: candidate.document.text.clone(),
        })
        .collect::<Vec<_>>();
    let ranked = retrieval::rank_complete(query, &documents, &training, mode, scope.is_none())
        .map_err(|_| invalid("CAT_PROFILE"))?;
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for hit in ranked.hits {
        let candidate = &candidates[hit.index];
        if !seen.insert(&candidate.record_id) {
            continue;
        }
        let mut item = selected.rows[&candidate.record_id].item.clone();
        let mut matched = json!({"documentId":candidate.document.key,"documentKind":candidate.kind,"heading":candidate.heading,
            "snippet":hit.snippet,"score":hit.score,"citation":candidate.citation,
            "lanes":match mode{Mode::Lexical=>vec!["lexical"],Mode::Semantic=>vec!["semantic"],Mode::Hybrid=>{let mut lanes=Vec::new();if hit.lexical_rank.is_some(){lanes.push("lexical");}if hit.semantic_rank.is_some(){lanes.push("semantic");}lanes}}});
        hit.write_metadata(&mut matched);
        item["match"] = matched;
        items.push(item);
    }
    let state = json!({"mode":mode.as_str(),"candidateWindow":"complete-admitted-corpus",
        "profile":{"maxInputs":retrieval::MAX_INPUTS,"maxTextBytes":retrieval::MAX_BYTES,"ephemeral":true,"characterSemanticPrior":"public-timeless-descriptions"},
        "fts":{"available":mode!=Mode::Semantic,"provider":if mode!=Mode::Semantic{Some("sqlite-fts5")}else{None}},
        "vector":{"available":mode!=Mode::Lexical,"model":if mode!=Mode::Lexical{retrieval::model_metadata(ranked.dimensions)}else{Json::Null}}});
    Ok((items, state))
}
