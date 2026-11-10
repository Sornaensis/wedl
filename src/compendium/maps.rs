//! Bounded presentation of admitted spatial data from one Compendium pin.
use super::bookmarks::{Issuer, Target};
use super::directory::{self, Row};
use super::{
    Action, Browser, Failure, Lens, MAX_RESPONSE, Mode, Request, Result, View, display, page_start,
};
use crate::{
    geometry,
    model::{Record, StoryTime, World},
    service::CompendiumPin,
    spatial::{self, RouteDetails},
    spatial_history,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const WIDTH: f64 = 760.0;
const HEIGHT: f64 = 400.0;
const MAX_ZOOM: u8 = 4;
const MAX_MAP_CHOICES: usize = 24;
const MAX_LIST: usize = 24;
const MAX_FEATURES: usize = 48;
const MAX_VERTICES: usize = 1024;
type Point = [f64; 2];
type Segment = [Point; 2];

pub(super) fn is_map(record: &Record) -> bool {
    record.kind == "map" && record.status() == "canonical"
}

pub(super) fn map_handle(browser: &Browser, view: &View, id: &str) -> Result<String> {
    browser.handle(view, "map", &[id])
}

#[derive(Serialize)]
struct Viewport {
    zoom: u8,
    x: u8,
    y: u8,
    maximum: u8,
}

impl Viewport {
    fn decode(request: &Request) -> Result<Self> {
        let value = |name, default, maximum| {
            request.get(name).map_or(Ok(default), |value| {
                value
                    .parse::<u8>()
                    .ok()
                    .filter(|value| *value <= maximum)
                    .ok_or(Failure::Invalid)
            })
        };
        let zoom = value("zoom", 0, MAX_ZOOM)?;
        let center = (1 << zoom) - 1;
        let maximum = 2 * center;
        Ok(Self {
            zoom,
            x: value("x", center, maximum)?,
            y: value("y", center, maximum)?,
            maximum,
        })
    }
}

struct Projection {
    min: Point,
    max: Point,
    scale: f64,
    padding: Point,
}

impl Projection {
    fn new(map: &geometry::Map<'_>, viewport: &Viewport) -> Option<Self> {
        let denominator = 2_u8 << viewport.zoom;
        let bounds = |axis: usize, offset| {
            let minimum = map.minimum[axis];
            let maximum = map.maximum[axis];
            let boundary = |part| match part {
                0 => minimum,
                part if part == denominator => maximum,
                part => minimum + (maximum - minimum) * (f64::from(part) / f64::from(denominator)),
            };
            [boundary(offset), boundary(offset + 2)]
        };
        let [left, right] = bounds(0, viewport.x);
        let [bottom, top] = bounds(1, viewport.y);
        let (width, height) = (right - left, top - bottom);
        let scale = (WIDTH / width).min(HEIGHT / height);
        if width <= 0.0
            || height <= 0.0
            || !scale.is_finite()
            || width * scale < 0.01
            || height * scale < 0.01
        {
            return None;
        }
        Some(Self {
            min: [left, bottom],
            max: [right, top],
            scale,
            padding: [
                (WIDTH - width * scale) / 2.0,
                (HEIGHT - height * scale) / 2.0,
            ],
        })
    }

    fn contains(&self, point: Point) -> bool {
        (0..2).all(|axis| (self.min[axis]..=self.max[axis]).contains(&point[axis]))
    }

    fn plot(&self) -> [f64; 4] {
        let [left, top] = self.pixel([self.min[0], self.max[1]]);
        let [right, bottom] = self.pixel([self.max[0], self.min[1]]);
        [left, top, right - left, bottom - top]
    }

    fn pixel(&self, point: Point) -> Point {
        let x = point[0].clamp(self.min[0], self.max[0]);
        let y = point[1].clamp(self.min[1], self.max[1]);
        let rounded = |value: f64, extent| (100.0 * value.clamp(0.0, extent)).round() / 100.0;
        [
            rounded(self.padding[0] + (x - self.min[0]) * self.scale, WIDTH),
            rounded(self.padding[1] + (self.max[1] - y) * self.scale, HEIGHT),
        ]
    }

    fn segment(&self, [start, end]: Segment) -> Option<Segment> {
        let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
        for axis in 0..2 {
            let delta = end[axis] - start[axis];
            if delta == 0.0 {
                if !(self.min[axis]..=self.max[axis]).contains(&start[axis]) {
                    return None;
                }
                continue;
            }
            let low = (self.min[axis] - start[axis]) / delta;
            let high = (self.max[axis] - start[axis]) / delta;
            enter = enter.max(low.min(high));
            leave = leave.min(low.max(high));
            if enter > leave {
                return None;
            }
        }
        let point = |at| {
            self.pixel(std::array::from_fn(|axis| {
                start[axis] + at * (end[axis] - start[axis])
            }))
        };
        Some([point(enter), point(leave)])
    }

    fn ring(&self, mut points: Vec<Point>) -> Vec<Point> {
        points.pop(); // Native rings repeat the first vertex.
        for axis in 0..2 {
            for (bound, lower) in [(self.min[axis], true), (self.max[axis], false)] {
                let inside = |point: Point| {
                    if lower {
                        point[axis] >= bound
                    } else {
                        point[axis] <= bound
                    }
                };
                let Some(mut prior) = points.last().copied() else {
                    return points;
                };
                let mut clipped = Vec::new();
                for point in points {
                    let (prior_inside, current_inside) = (inside(prior), inside(point));
                    if prior_inside != current_inside {
                        let other = 1 - axis;
                        let ratio = (bound - prior[axis]) / (point[axis] - prior[axis]);
                        let mut hit = prior;
                        hit[axis] = bound;
                        hit[other] += ratio * (point[other] - prior[other]);
                        clipped.push(hit);
                    }
                    if current_inside {
                        clipped.push(point);
                    }
                    prior = point;
                }
                points = clipped;
            }
        }
        let mut pixels = points
            .into_iter()
            .map(|point| self.pixel(point))
            .collect::<Vec<_>>();
        pixels.dedup();
        pixels
    }

    fn shape(&self, shape: &geometry::Shape<'_>) -> Option<Drawing<'static>> {
        let points = shape.points.iter().map(|vertex| xy(vertex));
        match shape.kind {
            "point" => {
                let point = xy(&shape.points[0]);
                self.contains(point).then(|| Drawing::Point {
                    point: self.pixel(point),
                })
            }
            "line" => {
                let parts = shape
                    .points
                    .windows(2)
                    .filter_map(|pair| self.segment([xy(&pair[0]), xy(&pair[1])]))
                    .take(MAX_VERTICES / 2 + 1)
                    .collect::<Vec<_>>();
                let first = parts.first()?[0];
                if parts.iter().all(|part| *part == [first, first]) {
                    Some(Drawing::Point { point: first })
                } else {
                    Some(Drawing::Line { parts })
                }
            }
            "polygon" => {
                let ring = self.ring(points.collect());
                let [first, rest @ ..] = ring.as_slice() else {
                    return None;
                };
                let second = rest.iter().find(|point| *point != first)?;
                let area = |point: &Point| {
                    (second[0] - first[0]) * (point[1] - first[1])
                        - (second[1] - first[1]) * (point[0] - first[0])
                };
                if !rest.iter().any(|point| area(point) != 0.0) {
                    return None;
                }
                // Even-odd fill permits return paths along the clip border.
                // Stroke only actual authored edges to avoid false connectors.
                let edges = shape
                    .points
                    .windows(2)
                    .filter_map(|pair| self.segment([xy(&pair[0]), xy(&pair[1])]))
                    .filter(|segment| segment[0] != segment[1])
                    .take(MAX_VERTICES / 2 + 1)
                    .collect();
                Some(Drawing::Polygon { ring, edges })
            }
            _ => None,
        }
    }
}

fn xy(vertex: &[f64]) -> Point {
    [vertex[0], vertex[1]]
}

#[derive(Serialize)]
#[serde(tag = "shape", rename_all = "kebab-case")]
enum Drawing<'a> {
    Point {
        point: Point,
    },
    Line {
        parts: Vec<Segment>,
    },
    Polygon {
        ring: Vec<Point>,
        edges: Vec<Segment>,
    },
    Route {
        segment: Segment,
        direction: &'a str,
        availability: &'a str,
    },
}

impl Drawing<'_> {
    fn vertices(&self) -> usize {
        match self {
            Self::Point { .. } => 1,
            Self::Line { parts } => 2 * parts.len(),
            Self::Polygon { ring, edges } => ring.len() + 2 * edges.len(),
            Self::Route { .. } => 2,
        }
    }
}

#[derive(Default)]
struct Drawings<'a> {
    items: Vec<(&'a Row, Drawing<'a>)>,
    vertices: usize,
    over_budget: bool,
}

impl<'a> Drawings<'a> {
    fn add(&mut self, row: &'a Row, drawing: Drawing<'a>) {
        self.vertices += drawing.vertices();
        if self.items.len() == MAX_FEATURES || self.vertices > MAX_VERTICES {
            self.items.clear();
            self.over_budget = true;
        } else if !self.over_budget {
            self.items.push((row, drawing));
        }
    }
}

#[derive(Serialize)]
struct Detail {
    label: &'static str,
    value: String,
}

fn metric(label: &'static str, value: &Value) -> Option<Detail> {
    let amount = &value["value"];
    let amount = match amount {
        Value::String(number) => number.clone(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    let unit = display(value["unit"].as_str()?, 48);
    (!unit.is_empty()).then_some(Detail {
        label,
        value: format!("{amount} {unit}"),
    })
}

fn author_details(
    route: &RouteDetails<'_>,
    admitted: &BTreeMap<&str, &Row>,
) -> Result<Vec<Detail>> {
    let mut details = vec![
        Detail {
            label: "From",
            value: admitted[route.from].title.clone(),
        },
        Detail {
            label: "To",
            value: admitted[route.to].title.clone(),
        },
        Detail {
            label: "Direction",
            value: directory::kind_label(route.direction),
        },
        Detail {
            label: "Availability",
            value: directory::kind_label(route.availability),
        },
    ];
    for (label, value) in [
        ("Distance", route.distance),
        ("Travel cost", route.cost),
        ("Duration", route.duration),
    ] {
        if let Some(value) = value {
            let value = crate::service::yaml_json(value).map_err(|_| Failure::Source)?;
            details.extend(metric(label, &value));
        }
    }
    Ok(details)
}

fn route_endpoints_admitted(record: &Record, row: &Row, admitted: &BTreeMap<&str, &Row>) -> bool {
    let from = record
        .string("from_location_id")
        .and_then(|id| admitted.get(id));
    let to = record
        .string("to_location_id")
        .and_then(|id| admitted.get(id));
    match (from, to) {
        (Some(from), Some(to)) => row
            .timelines
            .iter()
            .any(|timeline| from.timelines.contains(timeline) && to.timelines.contains(timeline)),
        _ => false,
    }
}

fn character_details(
    world: &World,
    row: &Row,
    admitted: &BTreeMap<&str, &Row>,
    at: &StoryTime,
    allowed: &BTreeSet<String>,
) -> Result<Vec<Detail>> {
    let record = &world.records[&row.id];
    if record.status() != "canonical"
        || (record.kind == "route" && !route_endpoints_admitted(record, row, admitted))
    {
        return Ok(Vec::new());
    }
    let history = spatial_history::character_values(world, record, at, allowed)
        .map_err(|_| Failure::Source)?;
    let mut details = Vec::new();
    for item in history {
        let value = &item["value"];
        match item["aspect"].as_str() {
            Some("parentage") => {
                if let Some(parent) = value["parent_id"].as_str().and_then(|id| admitted.get(id)) {
                    details.push(Detail {
                        label: "Within",
                        value: parent.title.clone(),
                    });
                }
            }
            Some("route-availability") => {
                if let Some(availability) = value["availability"].as_str() {
                    details.push(Detail {
                        label: "Availability",
                        value: directory::kind_label(availability),
                    });
                }
            }
            Some("route-metric") => {
                for (label, field) in [
                    ("Distance", "route_distance"),
                    ("Travel cost", "travel_cost"),
                    ("Duration", "duration"),
                ] {
                    details.extend(metric(label, &value[field]));
                }
            }
            _ => (), // Even admitted history placements stay outside this presentation.
        }
    }
    Ok(details)
}

fn location_page(
    browser: &Browser,
    view: &View,
    world: &World,
    request: &Request,
    rows: &[&Row],
    routes: &Issuer<'_>,
    details: impl Fn(&Row) -> Result<Vec<Detail>>,
) -> Result<Value> {
    let cursor = |row: &&Row| browser.handle(view, "locations-page", &[&row.id]);
    let start = page_start(rows, request.get("cursor"), cursor)?;
    let end = (start + request.limit()?.min(MAX_LIST)).min(rows.len());
    let mut items = Vec::new();
    for row in &rows[start..end] {
        let record = &world.records[&row.id];
        items.push(json!({
            "title": row.title,
            "kindLabel": directory::kind_label(&record.kind),
            "href": routes.path(Target::Record(record))?,
            "details": details(row)?,
        }));
    }
    let next = (end < rows.len())
        .then(|| cursor(&rows[end - 1]))
        .transpose()?;
    Ok(json!({ "items": items, "nextCursor": next }))
}

fn choices(browser: &Browser, view: &View, rows: &[&Row], cursor: Option<&str>) -> Result<Value> {
    let cursor_for = |row: &&Row| browser.handle(view, "map-choices-page", &[&row.id]);
    let start = page_start(rows, cursor, cursor_for)?;
    let end = (start + MAX_MAP_CHOICES).min(rows.len());
    let items = rows[start..end]
        .iter()
        .map(|row| Ok(json!({ "handle": map_handle(browser, view, &row.id)?, "label": row.title })))
        .collect::<Result<Vec<_>>>()?;
    let next = (end < rows.len())
        .then(|| cursor_for(&rows[end - 1]))
        .transpose()?;
    Ok(json!({ "items": items, "nextCursor": next }))
}

fn author_map(
    browser: &Browser,
    view: &View,
    world: &World,
    request: &Request,
    routes: &Issuer<'_>,
    rows: &[Row],
    spatial_rows: &[&Row],
    admitted: &BTreeMap<&str, &Row>,
) -> Result<Value> {
    let map_rows = rows
        .iter()
        .filter(|row| is_map(&world.records[&row.id]))
        .collect::<Vec<_>>();
    if request.get("mapCursor").is_some() && request.query.len() != 2 {
        return Err(Failure::Invalid);
    }
    let map_choices = choices(browser, view, &map_rows, request.get("mapCursor"))?;
    if request.get("mapCursor").is_some() {
        return Ok(json!({ "viewToken": view.token, "choices": map_choices }));
    }
    let viewport = Viewport::decode(request)?;
    let selected = if let Some(handle) = request.get("map") {
        let index = page_start(&map_rows, Some(handle), |row| {
            map_handle(browser, view, &row.id)
        })?;
        Some(map_rows[index - 1])
    } else {
        map_rows.first().copied()
    };
    let map = selected
        .map(|row| geometry::Map::compendium(&world.records[&row.id]))
        .transpose()
        .map_err(|_| Failure::Source)?;
    let at = view.lens.moment().map(|moment| &moment.at);
    // The pin has validated the source and the named lens supplies a valid
    // horizon. The native dated projection also requires these capabilities.
    let supported = at.is_none()
        || ["spatial-core-v1", "spatial-history-v1"]
            .iter()
            .all(|name| crate::narrative::require_capability(world, name).is_ok());
    let projection = map
        .as_ref()
        .filter(|_| supported)
        .and_then(|map| Projection::new(map, &viewport));
    let mut placements = BTreeMap::new();
    let mut route_info = BTreeMap::new();
    let mut drawings = Drawings::default();
    if supported {
        for row in spatial_rows {
            let record = &world.records[&row.id];
            match record.kind.as_str() {
                "location" => {
                    let Some(map) = &map else { continue };
                    let Some(shape) = map
                        .compendium_placement(world, record, at)
                        .map_err(|_| Failure::Source)?
                    else {
                        continue;
                    };
                    placements.insert(
                        record.id.as_str(),
                        (shape.kind == "point").then(|| shape.points[0].clone()),
                    );
                    if request.get("cursor").is_none()
                        && !drawings.over_budget
                        && let Some(drawing) =
                            projection.as_ref().and_then(|view| view.shape(&shape))
                    {
                        drawings.add(row, drawing);
                    }
                }
                "route"
                    if record.status() == "canonical"
                        && route_endpoints_admitted(record, row, admitted) =>
                {
                    route_info.insert(
                        record.id.as_str(),
                        spatial::route_details(world, record, at).map_err(|_| Failure::Source)?,
                    );
                }
                _ => (),
            }
        }
        if let Some(projection) = &projection
            && request.get("cursor").is_none()
        {
            for row in spatial_rows {
                if drawings.over_budget {
                    break;
                }
                if let Some(route) = route_info.get(row.id.as_str())
                    && let (Some(Some(from)), Some(Some(to))) =
                        (placements.get(route.from), placements.get(route.to))
                    && let Some(segment) = projection.segment([xy(from), xy(to)])
                    && segment[0] != segment[1]
                {
                    drawings.add(
                        row,
                        Drawing::Route {
                            segment,
                            direction: route.direction,
                            availability: route.availability,
                        },
                    );
                }
            }
        }
    }
    let locations = location_page(browser, view, world, request, spatial_rows, routes, |row| {
        if let Some(route) = route_info.get(row.id.as_str()) {
            return author_details(route, admitted);
        }
        let mut details = Vec::new();
        if let Some(map) = &map
            && supported
            && world.records[&row.id].kind == "location"
        {
            let placement = placements.get(row.id.as_str());
            details.push(Detail {
                label: "Placement",
                value: if placement.is_some() {
                    "On this map"
                } else {
                    "Unavailable on this map"
                }
                .into(),
            });
            if let Some(elevation) = placement
                .and_then(Option::as_ref)
                .and_then(|point| point.get(2))
            {
                details.push(Detail {
                    label: "Elevation",
                    value: format!("{elevation} {}", display(map.unit, 48)),
                });
            }
        }
        Ok(details)
    })?;
    if request.get("cursor").is_some() {
        return Ok(json!({ "viewToken": view.token, "locations": locations }));
    }
    let status = match (&map, supported, &projection, drawings.over_budget) {
        (_, false, _, _) => "unsupported",
        (None, _, _, _) => "no-map",
        (_, _, None, _) => "unusable",
        (_, _, _, true) => "over-budget",
        _ => "ready",
    };
    let features = drawings.items.into_iter().map(|(row, drawing)| {
        let record = &world.records[&row.id];
        Ok(json!({ "title": row.title, "href": routes.path(Target::Record(record))?, "drawing": drawing }))
    }).collect::<Result<Vec<_>>>()?;
    let map = selected
        .zip(map.as_ref())
        .map(|(row, map)| {
            let record = &world.records[&row.id];
            Ok(json!({
                "handle": map_handle(browser, view, &row.id)?,
                "label": row.title,
                "href": routes.path(Target::Map(Some(record)))?,
                "crs": if map.crs == "EPSG:4326" { "EPSG:4326" } else { "local-planar" },
                "axes": map.axes,
                "unit": display(map.unit, 48),
                "topView": map.minimum.len() == 3,
            }))
        })
        .transpose()?;
    let mut result = json!({
        "viewToken": view.token, "status": status, "viewport": viewport, "map": map,
        "choices": map_choices, "features": features, "locations": locations,
        "plot": projection.as_ref().map(Projection::plot),
    });
    if serde_json::to_vec(&result)
        .map_err(|_| Failure::Source)?
        .len()
        > MAX_RESPONSE
    {
        result["status"] = json!("over-budget");
        result["features"] = json!([]);
    }
    Ok(result)
}

pub(super) fn read(
    browser: &Browser,
    pin: &CompendiumPin,
    view: &View,
    request: &Request,
    routes: &Issuer<'_>,
) -> Result<Value> {
    let character = view.lens.mode() == Mode::Character;
    if character != matches!(request.action, Action::Locations) {
        return Err(Failure::Unavailable);
    }
    let world = pin.world();
    let scope = view.lens.scope(pin)?;
    let rows = directory::rows(world, &view.lens, scope.as_ref())?;
    let admitted = rows
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let spatial_rows = rows
        .iter()
        .filter(|row| matches!(world.records[&row.id].kind.as_str(), "location" | "route"))
        .collect::<Vec<_>>();
    if let (Lens::Character { moment, .. }, Some(scope)) = (&view.lens, scope) {
        let allowed = crate::narrative::require_capability(world, "spatial-history-v1")
            .is_ok()
            .then(|| scope.allowed_ids())
            .transpose()
            .map_err(|_| Failure::Source)?;
        let locations = location_page(
            browser,
            view,
            world,
            request,
            &spatial_rows,
            routes,
            |row| {
                if let Some(allowed) = &allowed {
                    character_details(world, row, &admitted, &moment.at, allowed)
                } else {
                    Ok(Vec::new())
                }
            },
        )?;
        return Ok(json!({ "viewToken": view.token, "locations": locations }));
    }
    author_map(
        browser,
        view,
        world,
        request,
        routes,
        &rows,
        &spatial_rows,
        &admitted,
    )
}
