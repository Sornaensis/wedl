use super::{
    Browser, Failure, Lens, Mode, Moment, Request, Result, View, display, fold, page_start,
};
use crate::{
    model::{Record, StoryTime, World},
    narrative::CharacterScope,
};
use serde::Serialize;
use serde_json::{Value, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Candidate {
    value: String,
    member: &'static str,
    label: String,
    descriptor: String,
    moment: Option<Moment>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Timeline,
    Moment,
    Viewpoint,
    Scene,
    Calendar,
}

impl Menu {
    fn parse(name: &str) -> Result<Self> {
        match name {
            "timeline" => Ok(Self::Timeline),
            "moment" => Ok(Self::Moment),
            "viewpoint" => Ok(Self::Viewpoint),
            "scene" => Ok(Self::Scene),
            "calendar" => Ok(Self::Calendar),
            _ => Err(Failure::Invalid),
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::Moment => "moment",
            Self::Viewpoint => "viewpoint",
            Self::Scene => "scene",
            Self::Calendar => "calendar",
        }
    }
}

#[derive(Serialize)]
struct Choice {
    handle: Option<String>,
    label: String,
    kind: &'static str,
    descriptor: String,
}

pub(super) fn timelines(world: &World) -> Result<Vec<(&str, &str)>> {
    let root = world.world_record().map_err(|_| Failure::Source)?;
    root.list("timelines")
        .ok_or(Failure::Source)?
        .iter()
        .map(|entry| {
            Ok((
                entry
                    .get("id")
                    .and_then(Yaml::as_str)
                    .ok_or(Failure::Source)?,
                entry
                    .get("label")
                    .and_then(Yaml::as_str)
                    .ok_or(Failure::Source)?,
            ))
        })
        .collect()
}

pub(super) fn timeline_handle(browser: &Browser, view: &View, id: &str) -> Result<String> {
    browser.handle(view, "timeline", &[id])
}

fn named(record: &Record, descriptor: impl Into<String>) -> Candidate {
    Candidate {
        value: record.id.clone(),
        member: "",
        label: display(&record.title, 160),
        descriptor: descriptor.into(),
        moment: None,
    }
}

fn allowed(
    world: &World,
    record: &Record,
    scope: Option<&CharacterScope<'_>>,
    at: Option<&StoryTime>,
) -> Result<bool> {
    match scope {
        Some(scope) => scope.visible(record),
        None => crate::description::full_author_record_visible(world, record, at),
    }
    .map_err(|_| Failure::Source)
}

fn moment_descriptor(
    world: &World,
    owner: &Record,
    member: &str,
    scope: Option<&CharacterScope<'_>>,
    at: &StoryTime,
) -> Result<String> {
    let kind = match member {
        "start" => "Scene opening",
        "current" => "Scene position",
        "end" => "Scene closing",
        _ => "Event",
    };
    if let Some(location) = owner
        .string("location")
        .and_then(|id| world.records.get(id))
        && allowed(world, location, scope, Some(at))?
    {
        return Ok(format!("{kind} · {}", display(&location.title, 120)));
    }
    Ok(kind.into())
}

fn moments(
    world: &World,
    lens: &Lens,
    scope: Option<&CharacterScope<'_>>,
    timeline: &str,
) -> Result<Vec<Candidate>> {
    let mut output = Vec::new();
    for owner in world.records.values() {
        if !matches!(owner.kind.as_str(), "event" | "scene") || !allowed(world, owner, scope, None)?
        {
            continue;
        }
        let time = owner.get("time").ok_or(Failure::Source)?;
        let points: &[(&str, Option<&Yaml>)] = if owner.kind == "event" {
            &[("event", Some(time))]
        } else {
            &[
                ("start", time.get("start")),
                ("current", time.get("current")),
                ("end", time.get("end")),
            ]
        };
        for &(member, value) in points {
            let Some(value) = value.filter(|value| !value.is_null()) else {
                continue;
            };
            let at = StoryTime::from_yaml(value, &world.default_timeline())
                .map_err(|_| Failure::Source)?;
            if at.timeline != timeline
                || matches!(lens, Lens::Character { moment, .. } if !at.not_after(&moment.at))
            {
                continue;
            }
            let mut candidate = named(owner, moment_descriptor(world, owner, member, scope, &at)?);
            candidate.member = member;
            candidate.moment = Some(Moment {
                owner: owner.id.clone(),
                member,
                at,
            });
            output.push(candidate);
        }
    }
    Ok(output)
}

fn viewpoints(
    world: &World,
    scope: Option<&CharacterScope<'_>>,
    at: &StoryTime,
) -> Result<Vec<Candidate>> {
    let mut output = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "character")
    {
        if allowed(world, record, scope, Some(at))? {
            output.push(named(record, "Character"));
        }
    }
    Ok(output)
}

fn calendars(world: &World) -> Result<Vec<Candidate>> {
    let result = crate::chronology::calendars(world, 1000).map_err(|_| Failure::Source)?;
    result["items"]
        .as_array()
        .ok_or(Failure::Source)?
        .iter()
        .map(|calendar| {
            Ok(Candidate {
                value: calendar["calendarId"]
                    .as_str()
                    .ok_or(Failure::Source)?
                    .into(),
                member: "",
                label: display(calendar["label"].as_str().ok_or(Failure::Source)?, 160),
                descriptor: "Authored calendar".into(),
                moment: None,
            })
        })
        .collect()
}

struct Selection<'a> {
    browser: &'a Browser,
    world: &'a World,
    view: &'a View,
    mode: Mode,
    scope: Option<CharacterScope<'a>>,
}

impl<'a> Selection<'a> {
    fn new(browser: &'a Browser, world: &'a World, view: &'a View, mode: Mode) -> Result<Self> {
        if view.lens.mode() == Mode::Character && mode != Mode::Character {
            return Err(Failure::Unavailable);
        }
        Ok(Self {
            browser,
            world,
            view,
            mode,
            scope: view.lens.scope(world)?,
        })
    }

    fn timeline_options(&self) -> Result<Vec<Candidate>> {
        Ok(timelines(self.world)?.into_iter().filter(|(id, _)| {
            !matches!(&self.view.lens, Lens::Character { moment, .. } if *id != moment.at.timeline)
        }).map(|(value, label)| Candidate {
            value: value.into(),
            member: "",
            label: display(label, 160),
            descriptor: "Declared timeline".into(),
            moment: None,
        }).collect())
    }

    fn token(&self, menu: Menu, parents: &[&str], candidate: &Candidate) -> Result<String> {
        if menu == Menu::Timeline {
            return timeline_handle(self.browser, self.view, &candidate.value);
        }
        let fields = [self.mode.wire()]
            .into_iter()
            .chain(parents.iter().copied())
            .chain([candidate.value.as_str(), candidate.member])
            .collect::<Vec<_>>();
        self.browser.handle(self.view, menu.wire(), &fields)
    }

    fn resolve(
        &self,
        menu: Menu,
        parents: &[&str],
        items: Vec<Candidate>,
        token: &str,
    ) -> Result<Candidate> {
        for candidate in &items {
            if self.token(menu, parents, candidate)? == token {
                if items
                    .iter()
                    .filter(|other| same_label(other, candidate))
                    .count()
                    != 1
                {
                    return Err(Failure::Unavailable);
                }
                return Ok(candidate.clone());
            }
        }
        Err(Failure::Unavailable)
    }

    fn timeline(&self, request: &Request) -> Result<Candidate> {
        self.resolve(
            Menu::Timeline,
            &[],
            self.timeline_options()?,
            request.required("timeline")?,
        )
    }

    fn moment_options(&self, request: &Request, timeline: &str) -> Result<Vec<Candidate>> {
        let mut options = moments(self.world, &self.view.lens, self.scope.as_ref(), timeline)?;
        if let Some(dates) = matching_dates(self, request, timeline)? {
            options.retain(|item| {
                item.moment
                    .as_ref()
                    .is_some_and(|moment| dates.contains(&(moment.at.tick, moment.at.order)))
            });
        }
        Ok(options)
    }

    fn moment(&self, request: &Request, timeline: &Candidate) -> Result<Candidate> {
        self.resolve(
            Menu::Moment,
            &[&timeline.value],
            self.moment_options(request, &timeline.value)?,
            request.required("moment")?,
        )
    }

    fn viewpoint(&self, request: &Request, moment: &Moment) -> Result<Candidate> {
        self.resolve(
            Menu::Viewpoint,
            &[&moment.owner, moment.member],
            viewpoints(self.world, self.scope.as_ref(), &moment.at)?,
            request.required("viewpoint")?,
        )
    }

    fn scenes(&self, moment: &Moment, viewer: &str) -> Result<Vec<Candidate>> {
        // Changing viewpoint does not grant advance discovery of that
        // character's scenes. Complete the admitted viewpoint change first.
        if matches!(&self.view.lens, Lens::Character { viewer: active, .. } if active != viewer) {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        for record in self
            .world
            .records
            .values()
            .filter(|record| record.kind == "scene")
        {
            if crate::narrative::labels_match(record, viewer).map_err(|_| Failure::Source)?
                && crate::scene::present(self.world, record, viewer, &moment.at)
                    .map_err(|_| Failure::Source)?
            {
                output.push(named(record, "Present in this scene"));
            }
        }
        Ok(output)
    }
}

pub(super) fn resolve(
    browser: &Browser,
    world: &World,
    view: &View,
    request: &Request,
) -> Result<Lens> {
    let mode = Mode::parse(request.required("mode")?)?;
    let selection = Selection::new(browser, world, view, mode)?;
    if mode == Mode::All {
        if [
            "timeline",
            "moment",
            "viewpoint",
            "scene",
            "calendar",
            "date",
        ]
        .iter()
        .any(|key| request.get(key).is_some())
        {
            return Err(Failure::Invalid);
        }
        return Ok(Lens::All);
    }
    let timeline = selection.timeline(request)?;
    let chosen = selection.moment(request, &timeline)?;
    let moment = chosen.moment.ok_or(Failure::Source)?;
    if mode == Mode::AsOf {
        if request.get("viewpoint").is_some() || request.get("scene").is_some() {
            return Err(Failure::Invalid);
        }
        return Ok(Lens::AsOf(moment));
    }
    let viewer = selection.viewpoint(request, &moment)?.value;
    let scene = request
        .get("scene")
        .map(|token| {
            selection
                .resolve(
                    Menu::Scene,
                    &[&moment.owner, moment.member, &viewer],
                    selection.scenes(&moment, &viewer)?,
                    token,
                )
                .map(|candidate| candidate.value)
        })
        .transpose()?;
    Ok(Lens::Character {
        moment,
        viewer,
        scene,
    })
}

fn same_label(left: &Candidate, right: &Candidate) -> bool {
    fold(&left.label) == fold(&right.label) && fold(&left.descriptor) == fold(&right.descriptor)
}

fn matching_dates(
    selection: &Selection<'_>,
    request: &Request,
    timeline: &str,
) -> Result<Option<BTreeSet<(i64, i32)>>> {
    let (calendar, date) = match (request.get("calendar"), request.get("date")) {
        (None, None) => return Ok(None),
        (Some(calendar), Some(date)) if selection.mode == Mode::AsOf && date.len() <= 24 => {
            (calendar, date)
        }
        _ => return Err(Failure::Invalid),
    };
    let calendar = selection.resolve(Menu::Calendar, &[], calendars(selection.world)?, calendar)?;
    let mut fields = date.rsplitn(3, '-');
    let day = fields
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or(Failure::Invalid)?;
    let month = fields
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or(Failure::Invalid)?;
    let year = fields
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or(Failure::Invalid)?;
    let date = crate::chronology::DateInput::Civil {
        calendar: calendar.value,
        year,
        month: Some(month),
        day: Some(day),
    };
    let resolved =
        crate::chronology::story_times(selection.world, &date, None, Some(timeline), 1000)
            .map_err(|_| Failure::Unavailable)?;
    let mut points = BTreeSet::new();
    for item in resolved["items"].as_array().ok_or(Failure::Source)? {
        let at = &item["storyTime"];
        let tick = at["tick"]
            .as_str()
            .and_then(|value| value.parse().ok())
            .ok_or(Failure::Source)?;
        let order = at["order"]
            .as_str()
            .and_then(|value| value.parse().ok())
            .ok_or(Failure::Source)?;
        points.insert((tick, order));
    }
    Ok(Some(points))
}

pub(super) fn read(
    browser: &Browser,
    world: &World,
    view: &View,
    request: &Request,
) -> Result<Value> {
    let menu = Menu::parse(request.required("menu")?)?;
    let mode = Mode::parse(request.required("mode")?)?;
    let selection = Selection::new(browser, world, view, mode)?;
    let expected: &[&str] = match menu {
        Menu::Timeline | Menu::Calendar => &[],
        Menu::Moment => &["timeline"],
        Menu::Viewpoint => &["timeline", "moment"],
        Menu::Scene => &["timeline", "moment", "viewpoint"],
    };
    if mode == Mode::All
        || (menu == Menu::Calendar && mode != Mode::AsOf)
        || (matches!(menu, Menu::Viewpoint | Menu::Scene) && mode != Mode::Character)
        || ["timeline", "moment", "viewpoint"]
            .iter()
            .any(|key| request.get(key).is_some() != expected.contains(key))
        || (menu != Menu::Moment
            && (request.get("calendar").is_some() || request.get("date").is_some()))
    {
        return Err(Failure::Invalid);
    }
    let mut parents = Vec::new();
    let mut items = match menu {
        Menu::Timeline => selection.timeline_options()?,
        Menu::Calendar => calendars(world)?,
        _ => {
            let timeline = selection.timeline(request)?;
            if menu == Menu::Moment {
                let candidates = selection.moment_options(request, &timeline.value)?;
                parents.push(timeline.value);
                candidates
            } else {
                let moment = selection
                    .moment(request, &timeline)?
                    .moment
                    .ok_or(Failure::Source)?;
                let candidates = if menu == Menu::Viewpoint {
                    viewpoints(world, selection.scope.as_ref(), &moment.at)?
                } else {
                    let viewer = selection.viewpoint(request, &moment)?.value;
                    let scenes = selection.scenes(&moment, &viewer)?;
                    parents.push(viewer);
                    scenes
                };
                parents.insert(0, moment.member.into());
                parents.insert(0, moment.owner);
                candidates
            }
        }
    };
    let query = fold(request.get("q").unwrap_or("").trim());
    // Count collisions before query or paging so neither can turn an
    // ambiguous displayed choice into a selectable one.
    let key = |item: &Candidate| (fold(&item.label), fold(&item.descriptor));
    let mut multiplicity = BTreeMap::new();
    for item in &items {
        *multiplicity.entry(key(item)).or_insert(0usize) += 1;
    }
    items.retain(|item| {
        fold(&item.label).contains(&query) || fold(&item.descriptor).contains(&query)
    });
    items.sort_by(|a, b| {
        a.moment
            .as_ref()
            .map(|moment| &moment.at)
            .cmp(&b.moment.as_ref().map(|moment| &moment.at))
            .then_with(|| key(a).cmp(&key(b)))
            .then_with(|| a.value.cmp(&b.value))
            .then_with(|| a.member.cmp(b.member))
    });
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(key(item)));
    let parent_fields = parents.iter().map(String::as_str).collect::<Vec<_>>();
    let limit = request.limit()?;
    let limit_key = limit.to_string();
    let cursor = |candidate: &Candidate| {
        let fields = [
            menu.wire(),
            mode.wire(),
            query.as_str(),
            request.get("calendar").unwrap_or(""),
            request.get("date").unwrap_or(""),
            &limit_key,
        ]
        .into_iter()
        .chain(parent_fields.iter().copied())
        .chain([candidate.value.as_str(), candidate.member])
        .collect::<Vec<_>>();
        browser.handle(view, "choice-cursor", &fields)
    };
    let start = page_start(&items, request.get("cursor"), cursor)?;
    let end = (start + limit).min(items.len());
    let choices = items[start..end]
        .iter()
        .map(|candidate| {
            let ambiguous = multiplicity[&key(candidate)] > 1;
            Ok(Choice {
                handle: (!ambiguous)
                    .then(|| selection.token(menu, &parent_fields, candidate))
                    .transpose()?,
                label: candidate.label.clone(),
                kind: menu.wire(),
                descriptor: if ambiguous {
                    format!("{} · Ambiguous choice", candidate.descriptor)
                } else {
                    candidate.descriptor.clone()
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let next_cursor = (end < items.len())
        .then(|| cursor(&items[end - 1]))
        .transpose()?;
    Ok(json!({"viewToken":view.token, "items":choices, "nextCursor":next_cursor}))
}

#[derive(Serialize)]
pub(super) struct ContextLabels {
    title: String,
    mode: &'static str,
    timeline: Option<String>,
    moment: Option<String>,
    descriptor: Option<String>,
    viewpoint: Option<String>,
    scene: Option<String>,
}

pub(super) fn context_labels(
    world: &World,
    lens: &Lens,
    scope: Option<&CharacterScope<'_>>,
) -> Result<ContextLabels> {
    let record = |id: &str| world.records.get(id).ok_or(Failure::Unavailable);
    let root = world.world_record().map_err(|_| Failure::Source)?;
    let title = if scope.is_none() {
        display(&root.title, 160)
    } else {
        "Compendium".into()
    };
    let mut labels = ContextLabels {
        title,
        mode: lens.mode().wire(),
        timeline: None,
        moment: None,
        descriptor: None,
        viewpoint: None,
        scene: None,
    };
    if let Some(moment) = lens.moment() {
        labels.timeline = timelines(world)?
            .into_iter()
            .find(|(id, _)| *id == moment.at.timeline)
            .map(|(_, label)| display(label, 160));
        let owner = record(&moment.owner)?;
        labels.moment = Some(display(&owner.title, 160));
        labels.descriptor = Some(moment_descriptor(
            world,
            owner,
            moment.member,
            scope,
            &moment.at,
        )?);
    }
    if let Lens::Character { viewer, scene, .. } = lens {
        let viewpoint = record(viewer)?;
        if !allowed(world, viewpoint, scope, None)? {
            return Err(Failure::Unavailable);
        }
        labels.viewpoint = Some(display(&viewpoint.title, 160));
        labels.scene = scene
            .as_ref()
            .map(|id| record(id).map(|scene| display(&scene.title, 160)))
            .transpose()?;
    }
    Ok(labels)
}
