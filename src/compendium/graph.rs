//! Presentation links consume only independently admitted directory rows.
//! A typed relationship owner must also prove the edge in the active scope.
use super::bookmarks::{Issuer, Target};
use super::{
    Browser, Failure, Lens, Request, Result, View, choices, directory, display, fold, page_start,
};
use crate::{
    model::{Record, StoryTime, World},
    narrative::CharacterScope,
    service::CompendiumPin,
};
use serde::Serialize;
use serde_json::{Value, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Relation {
    EventParticipant,
    EventLocation,
    Cause,
    SceneParticipant,
    SceneLocation,
    SceneObject,
    StoryOutcome,
    Lore,
    Route,
    RouteOrigin,
    RouteDestination,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Direction {
    Outgoing,
    Incoming,
    TwoWay,
}

#[derive(Serialize)]
struct Choice<'a> {
    handle: Option<String>,
    href: Option<String>,
    label: &'a str,
    kind: &'a str,
    descriptor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    relation: Option<Relation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<Direction>,
}

struct Item<'a> {
    row: &'a directory::Row,
    record: &'a Record,
    descriptor: String,
    member: &'a str,
    relation: Option<Relation>,
    direction: Option<Direction>,
}

impl<'a> Item<'a> {
    fn visible_key(&self) -> (String, &'a str, String, Option<Relation>, Option<Direction>) {
        (
            fold(&self.row.title),
            &self.record.kind,
            fold(&self.descriptor),
            self.relation,
            self.direction,
        )
    }
}

fn choice_page(
    browser: &Browser,
    view: &View,
    request: &Request,
    parent: &[&str],
    mut items: Vec<Item<'_>>,
    named_order: bool,
    routes: &Issuer<'_>,
) -> Result<(Vec<Value>, Option<String>)> {
    let query = fold(request.get("q").unwrap_or("").trim());
    let limit = request.limit()?;
    let limit_key = limit.to_string();
    let mut targets = BTreeMap::new();
    for item in &items {
        targets
            .entry(item.visible_key())
            .or_insert_with(BTreeSet::new)
            .insert(item.row.id.as_str());
    }
    if !named_order {
        items.sort_by_cached_key(|item| (item.visible_key(), item.row.id.as_str(), item.member));
    }
    let mut seen = BTreeSet::new();
    items.retain(|item| {
        let key = item.visible_key();
        (key.0.contains(&query) || key.2.contains(&query)) && seen.insert(key)
    });
    let cursor = |item: &Item<'_>| {
        let relation = item
            .relation
            .map(|relation| format!("{relation:?}"))
            .unwrap_or_default();
        let direction = item
            .direction
            .map(|direction| format!("{direction:?}"))
            .unwrap_or_default();
        let fields = parent
            .iter()
            .copied()
            .chain([
                query.as_str(),
                &limit_key,
                item.row.id.as_str(),
                item.member,
                &item.descriptor,
                &relation,
                &direction,
            ])
            .collect::<Vec<_>>();
        browser.handle(view, "graph-cursor", &fields)
    };
    let start = page_start(&items, request.get("cursor"), cursor)?;
    let end = (start + limit).min(items.len());
    let mut output = Vec::new();
    for item in &items[start..end] {
        let ambiguous = targets[&item.visible_key()].len() > 1;
        let handle = (!ambiguous)
            .then(|| directory::record_handle(browser, view, &item.row.id))
            .transpose()?;
        let href = (!ambiguous)
            .then(|| routes.path(Target::Record(item.record)))
            .transpose()?;
        let descriptor = if ambiguous {
            format!("{} · Ambiguous choice", item.descriptor)
        } else {
            item.descriptor.clone()
        };
        output.push(json!(Choice {
            handle,
            href,
            label: &item.row.title,
            kind: &item.record.kind,
            descriptor,
            relation: item.relation,
            direction: item.direction,
        }));
    }
    let next = (end < items.len())
        .then(|| cursor(&items[end - 1]))
        .transpose()?;
    Ok((output, next))
}

pub(super) fn record_choices(
    browser: &Browser,
    pin: &CompendiumPin,
    view: &View,
    request: &Request,
    routes: &Issuer<'_>,
) -> Result<Value> {
    if [
        "mode",
        "timeline",
        "moment",
        "viewpoint",
        "calendar",
        "date",
    ]
    .iter()
    .any(|key| request.get(key).is_some())
    {
        return Err(Failure::Invalid);
    }
    let filter = request.get("kind");
    if filter.is_some_and(|kind| !crate::validation::is_record_kind(kind)) {
        return Err(Failure::Invalid);
    }
    let world = pin.world();
    let scope = view.lens.scope(pin)?;
    let rows = directory::rows(world, &view.lens, scope.as_ref())?;
    let items = rows
        .iter()
        .filter_map(|row| {
            let kind = world.records[&row.id].kind.as_str();
            filter
                .is_none_or(|selected| selected == kind)
                .then(|| Item {
                    row,
                    record: &world.records[&row.id],
                    descriptor: directory::kind_label(kind),
                    member: "",
                    relation: None,
                    direction: None,
                })
        })
        .collect();
    let (items, next) = choice_page(
        browser,
        view,
        request,
        &["directory-records", filter.unwrap_or("")],
        items,
        false,
        routes,
    )?;
    Ok(json!({"viewToken":view.token, "items":items, "nextCursor":next}))
}

struct Projection<'a> {
    world: &'a World,
    lens: &'a Lens,
    rows: BTreeMap<&'a str, &'a directory::Row>,
    at: Option<&'a StoryTime>,
    focus: &'a str,
    output: Vec<Item<'a>>,
}

impl<'a> Projection<'a> {
    fn time(&self, value: Option<&Yaml>) -> Result<StoryTime> {
        StoryTime::from_yaml(
            value.ok_or(Failure::Source)?,
            &self.world.default_timeline(),
        )
        .map_err(|_| Failure::Source)
    }

    fn edge(
        &mut self,
        source: &str,
        target: &str,
        relation: Relation,
        label: String,
        timeline: Option<&str>,
        two_way: bool,
    ) {
        let (target_id, direction) = if source == self.focus {
            (target, Direction::Outgoing)
        } else if target == self.focus {
            (source, Direction::Incoming)
        } else {
            return;
        };
        let (Some(source), Some(target)) = (self.rows.get(source), self.rows.get(target)) else {
            return;
        };
        if !source.timelines.iter().any(|id| {
            timeline.is_none_or(|timeline| id == timeline) && target.timelines.contains(id)
        }) {
            return;
        }
        let row = self.rows[target_id];
        self.output.push(Item {
            row,
            record: &self.world.records[&row.id],
            descriptor: label,
            member: "",
            relation: Some(relation),
            direction: Some(if two_way {
                Direction::TwoWay
            } else {
                direction
            }),
        });
    }

    fn event(&mut self, owner: &'a Record) -> Result<()> {
        if owner.status() != "canonical" {
            return Ok(());
        }
        let at = self.time(owner.get("time"))?;
        if self.at.is_some_and(|through| !at.not_after(through)) {
            return Ok(());
        }
        let participants = owner.list("participants").into_iter().flatten();
        if let Lens::Character { viewer, .. } = self.lens
            && !participants
                .clone()
                .any(|member| member.get("character").and_then(Yaml::as_str) == Some(viewer))
        {
            // Knowing the name of an event and its endpoints does not
            // disclose its participant roster. Authored participation is the
            // available event evidence for this character.
            return Ok(());
        }
        let character = matches!(self.lens, Lens::Character { .. });
        if let Some(target) = owner.string("location") {
            self.edge(
                &owner.id,
                target,
                Relation::EventLocation,
                "Event location".into(),
                Some(&at.timeline),
                false,
            );
        }
        for member in participants {
            let target = member
                .get("character")
                .and_then(Yaml::as_str)
                .ok_or(Failure::Source)?;
            let mut label = "Event participant".to_owned();
            if !character && let Some(role) = member.get("role").and_then(Yaml::as_str) {
                label.push_str(" · ");
                label.push_str(&display(role, 120));
            }
            self.edge(
                &owner.id,
                target,
                Relation::EventParticipant,
                label,
                Some(&at.timeline),
                false,
            );
        }
        if character {
            return Ok(());
        }
        for member in owner.list("causes").into_iter().flatten() {
            let id = member
                .as_str()
                .or_else(|| member.get("event").and_then(Yaml::as_str))
                .ok_or(Failure::Source)?;
            if owner.id != self.focus && id != self.focus {
                continue;
            }
            let Some(cause) = self.world.records.get(id).filter(|record| {
                self.rows.contains_key(id)
                    && record.kind == "event"
                    && record.status() == "canonical"
            }) else {
                continue;
            };
            let cause_at = self.time(cause.get("time"))?;
            if cause_at != at && cause_at.not_after(&at) {
                self.edge(
                    id,
                    &owner.id,
                    Relation::Cause,
                    "Authored cause and effect".into(),
                    Some(&at.timeline),
                    false,
                );
            }
        }
        Ok(())
    }

    fn scene(&mut self, owner: &'a Record) -> Result<()> {
        if !matches!(owner.status(), "active" | "closed") {
            return Ok(());
        }
        if let Lens::Character { viewer, scene, .. } = self.lens
            && (scene.as_deref() != Some(&owner.id)
                || !crate::scene::present(
                    self.world,
                    owner,
                    viewer,
                    self.at.ok_or(Failure::Source)?,
                )
                .map_err(|_| Failure::Source)?)
        {
            return Ok(());
        }
        let start = self.time(owner.get("time").and_then(|time| time.get("start")))?;
        if self.at.is_some_and(|at| !start.not_after(at)) {
            return Ok(());
        }
        if let Some(location) = owner.string("location") {
            self.edge(
                &owner.id,
                location,
                Relation::SceneLocation,
                "Scene location".into(),
                Some(&start.timeline),
                false,
            );
        }
        for object in owner.list("objects").into_iter().flatten() {
            let target = object.as_str().ok_or(Failure::Source)?;
            self.edge(
                &owner.id,
                target,
                Relation::SceneObject,
                "Listed in scene".into(),
                Some(&start.timeline),
                false,
            );
        }
        for member in owner.list("participants").into_iter().flatten() {
            let target = member
                .get("character")
                .and_then(Yaml::as_str)
                .ok_or(Failure::Source)?;
            if !self.rows.contains_key(target) {
                continue;
            }
            if let Some(at) = self.at
                && !crate::scene::present(self.world, owner, target, at)
                    .map_err(|_| Failure::Source)?
            {
                continue;
            }
            let mut label = "Scene participant".to_owned();
            if !matches!(self.lens, Lens::Character { .. })
                && let Some(role) = member.get("role").and_then(Yaml::as_str)
            {
                label.push_str(" · ");
                label.push_str(&display(role, 120));
            }
            self.edge(
                &owner.id,
                target,
                Relation::SceneParticipant,
                label,
                Some(&start.timeline),
                false,
            );
        }
        Ok(())
    }

    fn outcome(&mut self, owner: &'a Record) -> Result<()> {
        if matches!(self.lens, Lens::Character { .. })
            || !crate::navigation::relations::settled_visible(self.world, owner, self.at)
                .map_err(|_| Failure::Source)?
        {
            return Ok(());
        }
        for outcome in owner.list("outcome_events").into_iter().flatten() {
            let id = outcome.as_str().ok_or(Failure::Source)?;
            let Some(event) = self.world.records.get(id).filter(|record| {
                self.rows.contains_key(id)
                    && record.kind == "event"
                    && record.status() == "canonical"
            }) else {
                continue;
            };
            let at = self.time(event.get("time"))?;
            if crate::navigation::relations::settled_visible(self.world, event, self.at)
                .map_err(|_| Failure::Source)?
            {
                self.edge(
                    &owner.id,
                    id,
                    Relation::StoryOutcome,
                    "Authored story outcome".into(),
                    Some(&at.timeline),
                    false,
                );
            }
        }
        Ok(())
    }

    fn lore(&mut self, owner: &'a Record, allowed: &BTreeSet<String>) -> Result<()> {
        if let Some(at) = self
            .at
            .filter(|_| matches!(self.lens, Lens::Character { .. }))
        {
            for link in crate::lore::character_links(self.world, owner, allowed, at)
                .map_err(|_| Failure::Source)?
            {
                let target = link["conceptId"].as_str().ok_or(Failure::Source)?;
                let label = link["relation"].as_str().ok_or(Failure::Source)?;
                self.edge(
                    &owner.id,
                    target,
                    Relation::Lore,
                    lore_label(label),
                    Some(&at.timeline),
                    false,
                );
            }
            return Ok(());
        }
        for member in owner.list("concept_links").into_iter().flatten() {
            let target = member
                .get("concept_id")
                .and_then(Yaml::as_str)
                .ok_or(Failure::Source)?;
            let valid = member.get("valid").ok_or(Failure::Source)?;
            if !crate::lore::validity(valid, Some(self.world), "concept link validity")
                .map_err(|_| Failure::Source)?
                .applies(self.at)
            {
                continue;
            }
            let label = member
                .get("relation")
                .and_then(Yaml::as_str)
                .ok_or(Failure::Source)?;
            self.edge(
                &owner.id,
                target,
                Relation::Lore,
                lore_label(label),
                valid.get("timeline").and_then(Yaml::as_str),
                false,
            );
        }
        Ok(())
    }

    fn route(&mut self, owner: &'a Record) -> Result<()> {
        if owner.status() != "canonical" || matches!(self.lens, Lens::Character { .. }) {
            return Ok(());
        }
        let from = owner.string("from_location_id").ok_or(Failure::Source)?;
        let to = owner.string("to_location_id").ok_or(Failure::Source)?;
        if ![from, to, &owner.id].contains(&self.focus) {
            return Ok(());
        }
        let (Some(origin), Some(destination)) = (self.rows.get(from), self.rows.get(to)) else {
            return Ok(());
        };
        let route = self.rows[owner.id.as_str()];
        if !route.timelines.iter().any(|id| {
            self.at.is_none_or(|at| id == &at.timeline)
                && origin.timelines.contains(id)
                && destination.timelines.contains(id)
        }) {
            return Ok(());
        }
        let direction = owner.string("direction").ok_or(Failure::Source)?;
        let availability = match self.at {
            Some(at) => {
                crate::spatial_history::effective_value(self.world, owner, "availability", at)
                    .map_err(|_| Failure::Source)?
                    .and_then(Yaml::as_str)
                    .unwrap_or("open")
            }
            None => owner.string("availability").unwrap_or("open"),
        };
        let timeline = self.at.map(|at| at.timeline.as_str());
        let label = format!("{} · {}", route.title, directory::kind_label(availability));
        self.edge(
            from,
            to,
            Relation::Route,
            label,
            timeline,
            direction == "two-way",
        );
        self.edge(
            &owner.id,
            from,
            Relation::RouteOrigin,
            "Route origin".into(),
            timeline,
            false,
        );
        self.edge(
            &owner.id,
            to,
            Relation::RouteDestination,
            "Route destination".into(),
            timeline,
            false,
        );
        Ok(())
    }
}

fn lore_label(relation: &str) -> String {
    format!(
        "Lore link · {}",
        directory::kind_label(&display(relation, 120))
    )
}

fn neighborhood<'a>(
    world: &'a World,
    lens: &'a Lens,
    rows: &'a [directory::Row],
    focus: &'a str,
) -> Result<Vec<Item<'a>>> {
    let mut graph = Projection {
        world,
        lens,
        rows: rows.iter().map(|row| (row.id.as_str(), row)).collect(),
        at: lens.moment().map(|moment| &moment.at),
        focus,
        output: Vec::new(),
    };
    let allowed = rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    for row in rows {
        let owner = &world.records[&row.id];
        match owner.kind.as_str() {
            "event" => graph.event(owner)?,
            "scene" => graph.scene(owner)?,
            "story-point" => graph.outcome(owner)?,
            "route" => graph.route(owner)?,
            _ => {}
        }
        if owner.get("concept_links").is_some() {
            graph.lore(owner, &allowed)?;
        }
    }
    Ok(graph.output)
}

fn timeline_page(
    browser: &Browser,
    world: &World,
    view: &View,
    request: &Request,
    scope: Option<&CharacterScope<'_>>,
    rows: &[directory::Row],
    routes: &Issuer<'_>,
) -> Result<Value> {
    let declared = choices::timelines(world)?;
    let default = world.default_timeline();
    let active = view.lens.moment().map(|moment| &moment.at);
    let mut selected = None;
    for (id, label) in declared {
        if active.is_some_and(|at| at.timeline != id) {
            continue;
        }
        let handle = choices::timeline_handle(browser, view, id)?;
        if request.get("timeline").map_or_else(
            || active.map_or(id == default, |at| id == at.timeline),
            |choice| choice == handle,
        ) {
            selected = Some((id, display(label, 160), handle));
        }
    }
    let (timeline, label, handle) = selected.ok_or(Failure::Unavailable)?;
    let admitted = rows
        .iter()
        .filter(|row| row.timelines.contains(timeline))
        .map(|row| (row.id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let mut positions = choices::moments(world, &view.lens, scope, timeline)?;
    positions.retain(|candidate| {
        candidate.moment.as_ref().is_some_and(|moment| {
            admitted.contains_key(moment.owner.as_str())
                && active.is_none_or(|at| moment.at.not_after(at))
        })
    });
    positions.sort_by(|a, b| {
        a.moment
            .as_ref()
            .map(|moment| &moment.at)
            .cmp(&b.moment.as_ref().map(|moment| &moment.at))
            .then_with(|| fold(&a.label).cmp(&fold(&b.label)))
            .then_with(|| a.member.cmp(b.member))
            .then_with(|| a.value.cmp(&b.value))
    });
    let items = positions
        .iter()
        .map(|candidate| Item {
            row: admitted[candidate.value.as_str()],
            record: &world.records[&candidate.value],
            descriptor: candidate.descriptor.clone(),
            member: candidate.member,
            relation: None,
            direction: None,
        })
        .collect();
    let (items, next) = choice_page(
        browser,
        view,
        request,
        &["timeline-positions", timeline],
        items,
        true,
        routes,
    )?;
    Ok(json!({
        "viewToken":view.token, "items":items, "nextCursor":next,
        "timeline":{"handle":handle,"label":label}
    }))
}

pub(super) fn read(
    browser: &Browser,
    pin: &CompendiumPin,
    view: &View,
    request: &Request,
    routes: &Issuer<'_>,
) -> Result<Value> {
    let world = pin.world();
    let scope = view.lens.scope(pin)?;
    let rows = directory::rows(world, &view.lens, scope.as_ref())?;
    let Some(root) = request.get("root") else {
        return timeline_page(browser, world, view, request, scope.as_ref(), &rows, routes);
    };
    if request.get("timeline").is_some() {
        return Err(Failure::Invalid);
    }
    let mut focus = None;
    for row in &rows {
        if directory::record_handle(browser, view, &row.id)? == root {
            focus = Some(row);
            break;
        }
    }
    let focus = focus.ok_or(Failure::Unavailable)?;
    let items = neighborhood(world, &view.lens, &rows, &focus.id)?;
    let (items, next) = choice_page(
        browser,
        view,
        request,
        &["neighborhood", &focus.id],
        items,
        false,
        routes,
    )?;
    Ok(json!({
        "viewToken":view.token, "items":items, "nextCursor":next,
        "causalAvailable":scope.is_none()
    }))
}
