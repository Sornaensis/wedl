//! Explicit author horizons and post-ranked thread grouping. These are source
//! reads; neither control grants character knowledge or expands reader scope.
use crate::{
    Error, Result, description,
    model::{Record, StoryTime, World},
    retrieval::{self, Document, Mode},
};
use serde_json::{Value, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeSet;

/// An omitted horizon retains native all-time source semantics. Supplied thread
/// IDs must be sorted, unique, declared IDs; membership is an ANY grouping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorSearchSelection {
    pub at: Option<StoryTime>,
    pub thread_ids: Option<Vec<String>>,
}

/// Explicit noncanonical discovery leaves canonical ranking, training and
/// result bytes intact. Scores from the two lanes are not comparable: native
/// lexical BM25 and hypothesis occurrence counts have different meanings.
/// `limit` applies independently to each lane, for at most `2 * limit` rows.
pub fn search_with_hypotheses(
    world: &World,
    query: &str,
    limit: usize,
    kind: Option<&str>,
    mode: Mode,
    selection: &AuthorSearchSelection,
) -> Result<Value> {
    let matches = search_with_selection(world, query, limit, kind, mode, selection)?;
    let suppressed_reason = if selection.thread_ids.is_some() {
        Some("thread-selection")
    } else if kind.is_some_and(|kind| kind != "hypothesis") {
        Some("kind-filter")
    } else {
        None
    };
    let items = if suppressed_reason.is_none() {
        crate::hypotheses::source_search(world, query, limit)?
    } else {
        Vec::new()
    };
    Ok(json!({
        "scope":matches["scope"],
        "matches":matches,
        "hypothesisMatches":{
            "scope":"author-all-time", "nonCanonical":true,
            "query":query, "limit":limit, "items":items,
            "included":suppressed_reason.is_none(),
            "suppressedReason":suppressed_reason,
            "notice":"Separate lexical possibility matches across all story time. Limit applies per lane; these scores do not rank canonical results or grant character knowledge."
        }
    }))
}

fn validate_threads(world: &World, selected: Option<&[String]>) -> Result<()> {
    let Some(selected) = selected else {
        return Ok(());
    };
    if selected.is_empty()
        || selected.len() > 32
        || selected.windows(2).any(|pair| pair[0] >= pair[1])
        || selected.iter().any(|id| {
            !id.strip_prefix("thread_").is_some_and(|tail| {
                tail.len() == 26
                    && tail
                        .bytes()
                        .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
            })
        })
    {
        return Err(Error::InvalidInput(
            "threads require 1 to 32 sorted unique exact thread IDs".into(),
        ));
    }
    let declarations = world.world_record()?.list("threads");
    if selected.iter().any(|id| {
        !declarations.is_some_and(|values| {
            values
                .iter()
                .any(|value| value.get("id").and_then(Yaml::as_str) == Some(id.as_str()))
        })
    }) {
        return Err(Error::InvalidInput(
            "threads must name declared exact thread IDs".into(),
        ));
    }
    Ok(())
}

fn matches_threads(record: &Record, selected: Option<&[String]>) -> bool {
    selected.is_none_or(|selected| {
        record.list("threads").is_some_and(|memberships| {
            memberships.iter().filter_map(Yaml::as_str).any(|id| {
                selected
                    .binary_search_by(|selected| selected.as_str().cmp(id))
                    .is_ok()
            })
        })
    })
}

/// Typed histories have their own native discovery endpoints. Their payloads
/// and raw aggregates are not reinterpreted as safely dated prose here.
fn ordinary_source(record: &Record) -> bool {
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

struct Candidate<'a> {
    record: &'a Record,
    key: String,
    body: &'a str,
    section: Option<description::AuthorSection<'a>>,
    structured: Option<super::structured::Structured<'a>>,
}

impl Candidate<'_> {
    fn text_bytes(&self) -> Result<usize> {
        if let Some(structured) = &self.structured {
            Ok(structured.text.len())
        } else if let Some(section) = &self.section {
            section
                .heading
                .len()
                .checked_add(1)
                .and_then(|value| value.checked_add(section.markdown.len()))
                .ok_or_else(retrieval::bounds_error)
        } else {
            Ok(self.body.len())
        }
    }
    fn text(&self) -> String {
        if let Some(structured) = &self.structured {
            structured.text.clone()
        } else if let Some(section) = &self.section {
            format!("{}\n{}", section.heading, section.markdown)
        } else {
            self.body.to_owned()
        }
    }
}

fn charge(bytes: &mut usize, length: usize) -> Result<()> {
    *bytes = bytes
        .checked_add(length)
        .ok_or_else(retrieval::bounds_error)?;
    if *bytes > retrieval::MAX_BYTES {
        return Err(retrieval::bounds_error());
    }
    Ok(())
}

fn admit<'a>(
    candidates: &mut Vec<Candidate<'a>>,
    bytes: &mut usize,
    candidate: Candidate<'a>,
) -> Result<()> {
    if candidates.len() == retrieval::MAX_INPUTS {
        return Err(retrieval::bounds_error());
    }
    for size in [
        candidate.key.len(),
        candidate.record.title.len(),
        candidate.text_bytes()?,
        candidate
            .structured
            .as_ref()
            .and_then(|value| value.member_id)
            .map_or(0, str::len),
        1,
    ] {
        charge(bytes, size)?;
    }
    candidates.push(candidate);
    Ok(())
}

/// Explicit dated selection. No selection delegates to the established
/// all-time API without altering its result shape, ranking, or cache behavior.
pub fn search_with_selection(
    world: &World,
    query: &str,
    limit: usize,
    kind: Option<&str>,
    mode: Mode,
    selection: &AuthorSearchSelection,
) -> Result<Value> {
    if selection.at.is_none() && selection.thread_ids.is_none() {
        return super::search_with_mode(world, query, limit, kind, mode);
    }
    retrieval::validate_request(query, limit)?;
    super::validation_gate(world)?;
    if let Some(at) = &selection.at {
        world.story_time(at.tick, Some(&at.timeline), at.order)?;
    }
    validate_threads(world, selection.thread_ids.as_deref())?;

    // Existing lexical kind filtering occurs after global all-time BM25
    // statistics. Thread-only requests preserve those exact established scores.
    let legacy_lexical = selection.at.is_none() && mode == Mode::Lexical;
    let mut candidates = Vec::new();
    let mut bytes = query.len();
    let bindings = selection
        .at
        .as_ref()
        .map(|_| crate::lore_state::Bindings::parse(world))
        .transpose()?;
    for record in world.records.values() {
        if record.kind == "hypothesis"
            || (!legacy_lexical && kind.is_some_and(|kind| record.kind != kind))
        {
            continue;
        }
        if let Some(at) = &selection.at {
            if record.kind == "context-brief"
                || !description::author_record_visible(world, record, at)?
            {
                continue;
            }
            if ordinary_source(record) {
                let body = if crate::narrative::author_body_visible(world, record, at)? {
                    record.body.as_str()
                } else {
                    ""
                };
                admit(
                    &mut candidates,
                    &mut bytes,
                    Candidate {
                        record,
                        key: record.id.clone(),
                        body,
                        section: None,
                        structured: None,
                    },
                )?;
                super::structured::collect(
                    world,
                    record,
                    at,
                    bindings.as_ref().expect("dated request has bindings"),
                    |structured| {
                        let key = format!("{}:{}:{}", record.id, structured.kind, structured.path);
                        admit(
                            &mut candidates,
                            &mut bytes,
                            Candidate {
                                record,
                                key,
                                body: "",
                                section: None,
                                structured: Some(structured),
                            },
                        )
                    },
                )?;
            }
            for section in description::author_sections(world, record, at)? {
                let key = format!("{}:description:{}", record.id, section.id);
                admit(
                    &mut candidates,
                    &mut bytes,
                    Candidate {
                        record,
                        key,
                        body: "",
                        section: Some(section),
                        structured: None,
                    },
                )?;
            }
        } else {
            admit(
                &mut candidates,
                &mut bytes,
                Candidate {
                    record,
                    key: record.id.clone(),
                    body: &record.body,
                    section: None,
                    structured: None,
                },
            )?;
        }
    }

    // Admission above is borrowed and byte-bounded. Every request-local lane
    // gets the same owned projection; only grouping is deferred until ranking.
    let documents: Vec<_> = candidates
        .iter()
        .map(|candidate| Document {
            key: candidate.key.clone(),
            title: candidate.record.title.clone(),
            text: candidate.text(),
        })
        .collect();
    let mut training = BTreeSet::new();
    if !legacy_lexical {
        for document in &documents {
            let text = if document.text.is_empty() {
                document.title.clone()
            } else {
                format!("{}\n{}", document.title, document.text)
            };
            if !training.contains(&text) {
                charge(&mut bytes, text.len())?;
                if documents.len() + training.len() + 1 > retrieval::MAX_INPUTS {
                    return Err(retrieval::bounds_error());
                }
                training.insert(text);
            }
        }
    }
    let ranked = retrieval::rank_complete(
        query,
        &documents,
        &training.into_iter().collect::<Vec<_>>(),
        mode,
        true,
    )?;
    let ranked_total = ranked
        .hits
        .iter()
        .filter(|hit| kind.is_none_or(|kind| candidates[hit.index].record.kind == kind))
        .count();
    let selected: Vec<_> = ranked
        .hits
        .iter()
        .filter(|hit| {
            let record = candidates[hit.index].record;
            kind.is_none_or(|kind| record.kind == kind)
                && matches_threads(record, selection.thread_ids.as_deref())
        })
        .collect();
    let items: Vec<_> = selected
        .iter()
        .take(limit)
        .map(|hit| {
            let candidate = &candidates[hit.index];
            let record = candidate.record;
            let mut item = json!({
                "id":record.id,"kind":record.kind,"title":record.title,
                "sourcePath":record.source_path,"blobOid":record.blob_oid,
            });
            if legacy_lexical {
                item["snippet"] = json!(hit.snippet);
                item["score"] = json!(-hit.score);
            } else {
                hit.write_metadata(&mut item);
            }
            if selection.at.is_some() {
                item["documentKey"] = json!(candidate.key);
                if let Some(structured) = &candidate.structured {
                    item["documentKind"] = json!(structured.kind);
                    item["status"] = json!(record.status());
                    item["at"] = json!(structured.at);
                    item["until"] = json!(structured.until);
                    if let Some(id) = structured.member_id {
                        item["memberId"] = json!(id);
                    }
                    item["citation"] = crate::lore::citation(world, record, &structured.path);
                } else if let Some(section) = &candidate.section {
                    item["documentKind"] = json!("description");
                    item["sectionId"] = json!(section.id);
                    item["heading"] = json!(section.heading);
                    item["citation"] = crate::lore::citation(
                        world,
                        record,
                        &format!("descriptions[{}]", section.index),
                    );
                } else {
                    item["documentKind"] = json!("source-prose");
                    item["bodyIncluded"] = json!(!candidate.body.is_empty());
                }
            }
            item
        })
        .collect();
    let mut result = json!({
        "protocol":crate::CONTRACT,
        "scope":if selection.at.is_some() { "author-as-of" } else { "author-all-time" },
        "at":selection.at,"threadIds":selection.thread_ids,
        "query":query,"fingerprint":world.fingerprint,"revision":world.revision,
        "mode":mode.as_str(),"items":items,"limit":limit,
        "total":selected.len(),"rankedTotal":ranked_total,
        "candidateWindow":"complete-admitted-corpus","hypothesesIncluded":false,
        "notice":"Author source selection; dated prose and structured members retain independent time gates. Thread grouping grants no character knowledge.",
    });
    if !legacy_lexical {
        result["retrieval"] = retrieval::model_metadata(ranked.dimensions);
    }
    if selection.at.is_some() {
        result["projection"] = json!("native-author-dated");
    }
    Ok(result)
}
