use super::{Browser, Failure, Lens, Request, Result, View, choices, display, fold, page_start};
use crate::{
    model::{Record, StoryTime, World},
    narrative::CharacterScope,
    service::CompendiumPin,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Row {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) timelines: BTreeSet<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Card<'a> {
    handle: String,
    title: &'a str,
    kind: &'a str,
    kind_label: String,
}

#[derive(Serialize)]
struct Section {
    heading: String,
    text: String,
}

#[derive(Serialize)]
struct Facet {
    handle: String,
    label: String,
    count: usize,
}

pub(super) fn kind_label(kind: &str) -> String {
    kind.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn rows(
    world: &World,
    lens: &Lens,
    scope: Option<&CharacterScope<'_>>,
) -> Result<Vec<Row>> {
    let at = lens.moment().map(|moment| &moment.at);
    let timelines = match at {
        Some(at) => vec![at.timeline.as_str()],
        None => choices::timelines(world)?
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
    };
    let character = match (lens, scope) {
        (Lens::Character { viewer, scene, .. }, Some(scope)) => {
            Some((viewer.as_str(), scene.as_deref(), scope))
        }
        _ => None,
    };
    let mut result = BTreeMap::new();
    for timeline in timelines {
        for (id, title, learned) in crate::catalogue::browser_rows(world, timeline, at, character)
            .map_err(|_| Failure::Source)?
        {
            let record = &world.records[&id];
            let title = if scope.is_some() && record.kind == "knowledge" {
                let statement = record
                    .get("claim")
                    .and_then(|claim| claim.get("statement"))
                    .and_then(serde_yaml::Value::as_str);
                display(
                    learned
                        .as_deref()
                        .or(statement)
                        .unwrap_or("Learned evidence"),
                    160,
                )
            } else {
                display(&title, 160)
            };
            result
                .entry(id.clone())
                .or_insert_with(|| Row {
                    id,
                    title,
                    timelines: BTreeSet::new(),
                })
                .timelines
                .insert(timeline.to_owned());
        }
    }
    let mut result = result.into_values().collect::<Vec<_>>();
    result.sort_by_cached_key(|row| (fold(&row.title), row.id.clone()));
    Ok(result)
}

pub(super) fn record_handle(browser: &Browser, view: &View, id: &str) -> Result<String> {
    browser.handle(view, "record", &[id])
}

fn card<'a>(browser: &Browser, view: &View, world: &'a World, row: &'a Row) -> Result<Card<'a>> {
    let record = &world.records[&row.id];
    Ok(Card {
        handle: record_handle(browser, view, &row.id)?,
        title: &row.title,
        kind: &record.kind,
        kind_label: kind_label(&record.kind),
    })
}

fn admitted_sections(values: Vec<Value>) -> Result<Vec<Section>> {
    values
        .into_iter()
        .map(|value| {
            Ok(Section {
                heading: display(value["heading"].as_str().ok_or(Failure::Source)?, 160),
                text: display(value["markdown"].as_str().ok_or(Failure::Source)?, 4096),
            })
        })
        .collect()
}

fn author_prose_allowed(world: &World, record: &Record, at: Option<&StoryTime>) -> Result<bool> {
    if matches!(record.kind.as_str(), "scene" | "conversation")
        && record
            .get("time")
            .and_then(|time| time.get("end"))
            .is_none_or(|end| end.is_null())
    {
        return Ok(false);
    }
    at.map(|at| crate::narrative::author_body_visible(world, record, at))
        .transpose()
        .map(|visible| visible.unwrap_or(true))
        .map_err(|_| Failure::Source)
}

fn author_sections(world: &World, record: &Record, at: Option<&StoryTime>) -> Result<Vec<Section>> {
    let values =
        crate::description::full_author_values(world, record, at).map_err(|_| Failure::Source)?;
    let mut sections = admitted_sections(values)?;
    if !record.body.trim().is_empty() && author_prose_allowed(world, record, at)? {
        sections.insert(
            0,
            Section {
                heading: "Overview".into(),
                text: display(record.body.trim(), 4096),
            },
        );
    }
    Ok(sections)
}

fn character_evidence(
    world: &World,
    lens: &Lens,
    scope: &CharacterScope<'_>,
    eligible: BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<Section>>> {
    let Lens::Character { viewer, moment, .. } = lens else {
        return Err(Failure::Source);
    };
    let mut sections = BTreeMap::<String, Vec<Section>>::new();
    for document in
        crate::character::catalogue_documents(world, viewer, &moment.at, scope, &eligible)
            .map_err(|_| Failure::Source)?
    {
        let kind = match document.kind {
            "belief" | "assertion-belief" | "generational-belief" => "Learned evidence",
            "description" => "Description",
            "observation" => "Observation",
            _ => "Remembered evidence",
        };
        sections
            .entry(document.record_id)
            .or_default()
            .push(Section {
                heading: display(document.value["heading"].as_str().unwrap_or(kind), 160),
                text: display(document.text.trim(), 4096),
            });
    }
    Ok(sections)
}

pub(super) fn page(
    browser: &Browser,
    pin: &CompendiumPin,
    view: &View,
    request: &Request,
) -> Result<Value> {
    let world = pin.world();
    let scope = view.lens.scope(pin)?;
    let mut rows = rows(world, &view.lens, scope.as_ref())?;
    let query = fold(request.get("q").unwrap_or("").trim());
    let kind = request.get("kind").unwrap_or("");
    if !kind.is_empty() && !crate::validation::is_record_kind(kind) {
        return Err(Failure::Invalid);
    }
    let at = view.lens.moment().map(|moment| &moment.at);
    let active_kind =
        !kind.is_empty() && rows.iter().any(|row| world.records[&row.id].kind == kind);
    let admitted_timelines = request
        .get("timeline")
        .map(|_| {
            rows.iter()
                .flat_map(|row| row.timelines.iter().cloned())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    if !query.is_empty() {
        let evidence = scope
            .as_ref()
            .map(|scope| {
                character_evidence(
                    world,
                    &view.lens,
                    scope,
                    rows.iter().map(|row| row.id.clone()).collect(),
                )
            })
            .transpose()?;
        let terms = query.split_whitespace().collect::<Vec<_>>();
        let mut matches = Vec::new();
        for row in rows {
            let record = &world.records[&row.id];
            let mut texts = vec![fold(&row.title)];
            if let Some(evidence) = &evidence {
                for section in evidence.get(&row.id).into_iter().flatten() {
                    texts.extend([fold(&section.heading), fold(&section.text)]);
                }
            } else {
                texts.extend(
                    record
                        .list("aliases")
                        .into_iter()
                        .flatten()
                        .filter_map(serde_yaml::Value::as_str)
                        .map(|alias| fold(&display(alias, 160))),
                );
                for section in author_sections(world, record, at)? {
                    texts.extend([fold(&section.heading), fold(&section.text)]);
                }
            }
            if terms
                .iter()
                .all(|term| texts.iter().any(|text| text.contains(term)))
            {
                matches.push(row);
            }
        }
        rows = matches;
    }

    let mut kinds = BTreeMap::new();
    let mut timelines = BTreeMap::new();
    if active_kind {
        kinds.insert(kind, 0);
    }
    for row in &rows {
        *kinds
            .entry(world.records[&row.id].kind.as_str())
            .or_insert(0) += 1;
        for id in &row.timelines {
            *timelines.entry(id.as_str()).or_insert(0) += 1;
        }
    }
    let kind_facets = kinds
        .into_iter()
        .map(|(kind, count)| Facet {
            handle: kind.into(),
            label: kind_label(kind),
            count,
        })
        .collect::<Vec<_>>();
    let mut timeline_facets = Vec::new();
    let mut timeline_filter = None;
    for (id, label) in choices::timelines(world)? {
        if at.is_some_and(|at| at.timeline != id) {
            continue;
        }
        let handle = choices::timeline_handle(browser, view, id)?;
        let selected = request.get("timeline") == Some(handle.as_str());
        if selected {
            timeline_filter = Some(id);
        }
        let count = timelines
            .get(id)
            .copied()
            .or_else(|| (selected && admitted_timelines.contains(id)).then_some(0));
        if let Some(count) = count {
            timeline_facets.push(Facet {
                handle,
                label: display(label, 160),
                count,
            });
        }
    }
    if request.get("timeline").is_some() && timeline_filter.is_none() {
        return Err(Failure::Unavailable);
    }
    rows.retain(|row| {
        (kind.is_empty() || world.records[&row.id].kind == kind)
            && timeline_filter.is_none_or(|timeline| row.timelines.contains(timeline))
    });
    let limit = request.limit()?;
    let limit_key = limit.to_string();
    let cursor = |row: &Row| {
        browser.handle(
            view,
            "directory-cursor",
            &[
                &query,
                kind,
                timeline_filter.unwrap_or(""),
                &limit_key,
                &row.id,
            ],
        )
    };
    let start = page_start(&rows, request.get("cursor"), cursor)?;
    let end = (start + limit).min(rows.len());
    let items = rows[start..end]
        .iter()
        .map(|row| card(browser, view, world, row))
        .collect::<Result<Vec<_>>>()?;
    let next_cursor = (end < rows.len())
        .then(|| cursor(&rows[end - 1]))
        .transpose()?;
    Ok(json!({
        "viewToken":view.token,
        "items":items,
        "facets":{"kinds":kind_facets,"timelines":timeline_facets},
        "total":rows.len(),
        "nextCursor":next_cursor,
    }))
}

pub(super) fn record(
    browser: &Browser,
    pin: &CompendiumPin,
    view: &View,
    request: &Request,
) -> Result<Value> {
    let target = request.required("record")?;
    let world = pin.world();
    let scope = view.lens.scope(pin)?;
    for row in rows(world, &view.lens, scope.as_ref())? {
        if record_handle(browser, view, &row.id)? != target {
            continue;
        }
        let record = &world.records[&row.id];
        let sections = if let Some(scope) = &scope {
            character_evidence(world, &view.lens, scope, BTreeSet::from([row.id.clone()]))?
                .remove(&row.id)
                .unwrap_or_default()
        } else {
            author_sections(world, record, view.lens.moment().map(|moment| &moment.at))?
        };
        return Ok(
            json!({"viewToken":view.token, "record":card(browser, view, world, &row)?, "sections":sections}),
        );
    }
    Err(Failure::Unavailable)
}
