//! Explicit author containment, directed routes and portals. Geometry and
//! story history do not supply edges or inferred travel metrics.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
};

#[path = "spatial_decimal.rs"]
mod decimal;
use decimal::Decimal;
#[path = "spatial_extra.rs"]
mod extra;
pub(crate) use extra::overlay_matches;
pub use extra::{overlays, reachable};

const MAX_EXPANSIONS: usize = 10_000;
const SCOPE: &str = "author-all-time-spatial-source";

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn text<'a>(value: &'a Yaml, field: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!(
            "{field} must be a nonblank untagged string"
        ))),
    }
}
fn mapping<'a>(value: &'a Yaml, field: &str) -> Result<&'a Mapping> {
    match value {
        Yaml::Mapping(value) => Ok(value),
        _ => Err(invalid(format!("{field} must be an untagged mapping"))),
    }
}
fn required<'a>(record: &'a Record, field: &str) -> Result<&'a Yaml> {
    mapping(&record.frontmatter, "frontmatter")?
        .get(field)
        .ok_or_else(|| invalid(format!("{field} is required")))
}
fn status(record: &Record) -> Result<&str> {
    let value = match record.get("status") {
        None => "canonical",
        Some(value) => text(value, "status")?,
    };
    if !["canonical", "retired"].contains(&value) {
        return Err(invalid(
            "spatial record status must be canonical or retired",
        ));
    }
    Ok(value)
}
fn location_ref<'a>(world: &'a World, value: &Yaml, field: &str) -> Result<&'a Record> {
    let id = text(value, field)?;
    world
        .records
        .get(id)
        .filter(|record| record.kind == "location" && valid_id_for_kind(id, "location"))
        .ok_or_else(|| {
            invalid(format!(
                "{field} must reference an existing exact location ID"
            ))
        })
}
fn parent<'a>(world: &'a World, record: &'a Record) -> Result<Option<&'a str>> {
    mapping(&record.frontmatter, "frontmatter")?;
    let canonical = status(record)? == "canonical";
    if record.get("parent").is_some() {
        return Err(invalid("parent is unsupported; use parent_id"));
    }
    let Some(value) = record.get("parent_id") else {
        return Ok(None);
    };
    if matches!(value, Yaml::Null) {
        return Ok(None);
    }
    let target = location_ref(world, value, "parent_id")?;
    if target.id == record.id {
        return Err(invalid("a location cannot contain itself"));
    }
    if canonical && status(target)? != "canonical" {
        return Err(invalid("a canonical location requires a canonical parent"));
    }
    Ok(Some(&target.id))
}
pub(crate) fn metric(value: &Yaml, field: &str) -> Result<()> {
    let fields = mapping(value, field)?;
    if fields.len() != 2 || !fields.contains_key("value") || !fields.contains_key("unit") {
        return Err(invalid(format!("{field} requires exactly value and unit")));
    }
    text(&fields["unit"], &format!("{field}.unit"))?;
    if !matches!(&fields["value"], Yaml::Number(value) if value.as_f64().is_some_and(|v| v.is_finite() && v >= 0.0))
    {
        return Err(invalid(format!(
            "{field}.value must be a finite nonnegative number"
        )));
    }
    Ok(())
}

#[derive(Clone)]
struct Route<'a> {
    record: &'a Record,
    from: &'a str,
    to: &'a str,
    direction: &'a str,
    modes: Vec<&'a str>,
    availability: &'a str,
    uncertainty: &'a str,
}
fn route<'a>(world: &'a World, record: &'a Record) -> Result<Route<'a>> {
    let canonical = status(record)? == "canonical";
    let from = location_ref(
        world,
        required(record, "from_location_id")?,
        "from_location_id",
    )?;
    let to = location_ref(world, required(record, "to_location_id")?, "to_location_id")?;
    if from.id == to.id {
        return Err(invalid("route endpoints must be distinct"));
    }
    if canonical && (status(from)? != "canonical" || status(to)? != "canonical") {
        return Err(invalid("canonical routes require canonical endpoints"));
    }
    let direction = text(required(record, "direction")?, "direction")?;
    if !["one-way", "two-way"].contains(&direction) {
        return Err(invalid("direction must be one-way or two-way"));
    }
    let Yaml::Sequence(values) = required(record, "modes")? else {
        return Err(invalid("modes must be a nonempty untagged array"));
    };
    let mut modes = BTreeSet::new();
    for value in values {
        if !modes.insert(text(value, "mode")?) {
            return Err(invalid("modes must be unique"));
        }
    }
    if modes.is_empty() {
        return Err(invalid("modes must not be empty"));
    }
    let availability = record
        .get("availability")
        .map(|v| text(v, "availability"))
        .transpose()?
        .unwrap_or("open");
    if !["open", "closed", "restricted", "unknown"].contains(&availability) {
        return Err(invalid(
            "availability must be open, closed, restricted or unknown",
        ));
    }
    let uncertainty = record
        .get("uncertainty")
        .map(|v| text(v, "uncertainty"))
        .transpose()?
        .unwrap_or("exact");
    if !["exact", "estimated", "unknown"].contains(&uncertainty) {
        return Err(invalid("uncertainty must be exact, estimated or unknown"));
    }
    for field in ["route_distance", "travel_cost", "duration"] {
        if let Some(value) = record.get(field) {
            metric(value, field)?;
        }
    }
    Ok(Route {
        record,
        from: &from.id,
        to: &to.id,
        direction,
        modes: modes.into_iter().collect(),
        availability,
        uncertainty,
    })
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

/// Validate declared parents/routes/portals/overlays for every source status.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut parents = BTreeMap::new();
    for record in world.records.values() {
        let result = match record.kind.as_str() {
            "location" => parent(world, record).map(|value| {
                if let Some(value) = value {
                    parents.insert(record.id.as_str(), value);
                }
            }),
            "route" => route(world, record).map(|_| ()),
            _ => continue,
        };
        if let Err(error) = result {
            diagnostics.push(diagnostic(record, "WDL-SPATIAL-001", error.to_string()));
        }
    }
    let mut finished = BTreeSet::new();
    for start in parents.keys() {
        let mut current = *start;
        let mut chain = BTreeSet::new();
        while !finished.contains(current) {
            if !chain.insert(current) {
                diagnostics.push(diagnostic(
                    &world.records[*start],
                    "WDL-SPATIAL-002",
                    "location parent_id graph must be acyclic".into(),
                ));
                break;
            }
            match parents.get(current) {
                Some(parent) => current = parent,
                None => break,
            }
        }
        finished.extend(chain);
    }
    diagnostics.extend(extra::validate(world));
    diagnostics
}

#[derive(Clone)]
enum EdgeSource<'a> {
    Route(Route<'a>),
    Portal(extra::Portal<'a>),
}
#[derive(Clone)]
struct Edge<'a> {
    source: EdgeSource<'a>,
    from: &'a str,
    to: &'a str,
}
impl<'a> Edge<'a> {
    fn record(&self) -> &'a Record {
        match &self.source {
            EdgeSource::Route(route) => route.record,
            EdgeSource::Portal(portal) => portal.record,
        }
    }
    fn modes(&self) -> &[&'a str] {
        match &self.source {
            EdgeSource::Route(route) => &route.modes,
            EdgeSource::Portal(portal) => &portal.modes,
        }
    }
}
struct Graph<'a> {
    parents: BTreeMap<&'a str, &'a str>,
    outgoing: BTreeMap<&'a str, Vec<Edge<'a>>>,
    portals: BTreeMap<&'a str, Vec<extra::Portal<'a>>>,
}
impl<'a> Graph<'a> {
    fn new(world: &'a World) -> Result<Self> {
        if let Some(error) = validate(world).first() {
            return Err(invalid(&error.message));
        }
        let mut graph = Self {
            parents: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            portals: BTreeMap::new(),
        };
        for record in world
            .records
            .values()
            .filter(|record| record.status() == "canonical")
        {
            if record.kind == "location" {
                if let Some(parent) = parent(world, record)? {
                    graph.parents.insert(&record.id, parent);
                }
            } else if record.kind == "route" {
                let route = route(world, record)?;
                if route.availability != "open" {
                    continue;
                }
                let from = route.from;
                let to = route.to;
                graph.outgoing.entry(from).or_default().push(Edge {
                    source: EdgeSource::Route(route.clone()),
                    from,
                    to,
                });
                if route.direction == "two-way" {
                    graph.outgoing.entry(to).or_default().push(Edge {
                        source: EdgeSource::Route(route),
                        from: to,
                        to: from,
                    });
                }
            } else if record.kind == "portal" {
                let portal = extra::portal(world, record)?;
                if let extra::Target::Location(to) = portal.target {
                    graph.outgoing.entry(portal.from).or_default().push(Edge {
                        source: EdgeSource::Portal(portal.clone()),
                        from: portal.from,
                        to,
                    });
                }
                graph.portals.entry(portal.from).or_default().push(portal);
            }
        }
        for edges in graph.outgoing.values_mut() {
            edges.sort_by_key(|edge| (&edge.record().id, edge.to));
        }
        Ok(graph)
    }
    fn edges<'b>(&'b self, id: &str, mode: Option<&'b str>) -> impl Iterator<Item = &'b Edge<'a>> {
        self.outgoing
            .get(id)
            .into_iter()
            .flatten()
            .filter(move |edge| mode.is_none_or(|mode| edge.modes().contains(&mode)))
    }
}
fn arguments(mode: Option<&str>, limit: usize) -> Result<()> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::InvalidInput(
            "limit must be between 1 and 1000".into(),
        ));
    }
    if mode.is_some_and(|mode| mode.trim().is_empty()) {
        return Err(Error::InvalidInput(
            "mode must be a nonblank authored mode".into(),
        ));
    }
    Ok(())
}
fn bounded(count: usize, limit: usize) -> Result<()> {
    if count > limit {
        return Err(Error::InvalidInput(format!(
            "complete spatial result exceeds limit {limit}"
        )));
    }
    Ok(())
}
fn selected<'a>(world: &'a World, reference: &str) -> Result<&'a Record> {
    let record = world.resolve(reference)?;
    if record.kind != "location" || status(record)? != "canonical" {
        return Err(Error::NotFound("canonical location is unavailable".into()));
    }
    Ok(record)
}
fn citation(world: &World, record: &Record, field: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":field})
}
fn location_value(world: &World, id: &str) -> Json {
    let record = &world.records[id];
    json!({"locationId":id,"title":record.title,"citation":citation(world,record,"parent_id")})
}
fn edge_value(world: &World, edge: &Edge<'_>) -> Result<Json> {
    let route = match &edge.source {
        EdgeSource::Portal(portal) => return extra::portal_value(world, portal),
        EdgeSource::Route(route) => route,
    };
    let mut value = json!({"routeId":route.record.id,"fromLocationId":edge.from,"toLocationId":edge.to,
        "direction":route.direction,"modes":route.modes,"availability":route.availability,
        "uncertainty":route.uncertainty,"citation":citation(world,route.record,"route")});
    for (source, output) in [
        ("route_distance", "routeDistance"),
        ("travel_cost", "travelCost"),
        ("duration", "duration"),
    ] {
        if let Some(metric) = route.record.get(source) {
            value[output] = crate::service::yaml_json(metric)?;
        }
    }
    Ok(value)
}

pub(crate) struct RouteDetails<'a> {
    pub(crate) from: &'a str,
    pub(crate) to: &'a str,
    pub(crate) direction: &'a str,
    pub(crate) availability: &'a str,
    pub(crate) distance: Option<&'a Yaml>,
    pub(crate) cost: Option<&'a Yaml>,
    pub(crate) duration: Option<&'a Yaml>,
    modes: Vec<&'a str>,
    uncertainty: &'a str,
}

/// Shared exact fields. Compendium callers provide already-pinned validated
/// source, an admitted route, and a checked horizon for effective history.
pub(crate) fn route_details<'a>(
    world: &'a World,
    record: &'a Record,
    at: Option<&StoryTime>,
) -> Result<RouteDetails<'a>> {
    if record.kind != "route" || status(record)? != "canonical" {
        return Err(Error::NotFound("canonical route is unavailable".into()));
    }
    let route = route(world, record)?;
    let effective = |field: &str| match at {
        Some(at) => crate::spatial_history::effective_value(world, record, field, at),
        None => Ok(record.get(field)),
    };
    let availability = effective("availability")?
        .map(|value| text(value, "availability"))
        .transpose()?
        .unwrap_or("open");
    Ok(RouteDetails {
        from: route.from,
        to: route.to,
        direction: route.direction,
        availability,
        distance: effective("route_distance")?,
        cost: effective("travel_cost")?,
        duration: effective("duration")?,
        modes: route.modes,
        uncertainty: route.uncertainty,
    })
}

/// Inspect one canonical route, including closed routes. An explicit horizon
/// selects authored historical availability/metrics without changing path reads.
pub fn route_info(world: &World, reference: &str, at: Option<&StoryTime>) -> Result<Json> {
    if let Some(at) = at {
        crate::spatial_history::check_query(world, at)?;
    }
    if let Some(error) = validate(world).first() {
        return Err(invalid(&error.message));
    }
    let record = world.resolve(reference)?;
    let route = route_details(world, record, at)?;
    let mut value = json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,
        "basis":"authored-route","routeId":record.id,"title":record.title,
        "fromLocationId":route.from,"toLocationId":route.to,"direction":route.direction,
        "modes":route.modes,"availability":route.availability,"uncertainty":route.uncertainty,
        "citation":citation(world,record,"route")});
    for (metric, output) in [
        (route.distance, "routeDistance"),
        (route.cost, "travelCost"),
        (route.duration, "duration"),
    ] {
        value[output] = metric
            .map(crate::service::yaml_json)
            .transpose()?
            .unwrap_or(Json::Null);
    }
    if let Some(at) = at {
        value["scope"] = json!("author-as-of-spatial-history");
        value["basis"] = json!("effective-route");
        value["at"] = json!(at);
        let mut fields = serde_json::Map::new();
        for (field, output) in [
            ("availability", "availability"),
            ("route_distance", "routeDistance"),
            ("travel_cost", "travelCost"),
            ("duration", "duration"),
        ] {
            fields.insert(
                output.into(),
                json!(crate::spatial_history::field_citations(
                    world, record, field, at
                )?),
            );
        }
        value["fieldCitations"] = Json::Object(fields);
    }
    Ok(value)
}

/// Canonical location, nearest parent first, direct children, and usable outbound
/// edges. Each complete collection is bounded independently; nothing is truncated.
pub fn location(world: &World, reference: &str, mode: Option<&str>, limit: usize) -> Result<Json> {
    arguments(mode, limit)?;
    let record = selected(world, reference)?;
    let graph = Graph::new(world)?;
    let mut ancestors = Vec::new();
    let mut current = record.id.as_str();
    while let Some(parent) = graph.parents.get(current) {
        ancestors.push(location_value(world, parent));
        bounded(ancestors.len(), limit)?;
        current = parent;
    }
    let mut children = Vec::new();
    for (child, parent) in &graph.parents {
        if *parent == record.id {
            children.push(location_value(world, child));
            bounded(children.len(), limit)?;
        }
    }
    let mut outgoing = Vec::new();
    for edge in graph.edges(&record.id, mode) {
        if !matches!(edge.source, EdgeSource::Route(_)) {
            continue;
        }
        outgoing.push(edge_value(world, edge)?);
        bounded(outgoing.len(), limit)?;
    }
    let mut portals = Vec::new();
    for portal in graph
        .portals
        .get(record.id.as_str())
        .into_iter()
        .flatten()
        .filter(|portal| mode.is_none_or(|mode| portal.modes.contains(&mode)))
    {
        portals.push(extra::portal_value(world, portal)?);
        bounded(portals.len(), limit)?;
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,
        "basis":"explicit-parent-id-and-authored-directed-routes-and-portals","locationId":record.id,"title":record.title,
        "parentId":graph.parents.get(record.id.as_str()),"ancestors":ancestors,"children":children,"outgoingRoutes":outgoing,"outgoingPortals":portals,
        "mode":mode,"limit":limit,"citation":citation(world,record,"parent_id")}),
    )
}

fn expanded(expansions: &mut usize) -> Result<()> {
    *expansions += 1;
    if *expansions > MAX_EXPANSIONS {
        return Err(Error::InvalidInput(
            "spatial path expansion budget exceeds 10000 locations".into(),
        ));
    }
    Ok(())
}

fn reconstruct<'a>(
    from: &str,
    to: &str,
    previous: &BTreeMap<&str, Edge<'a>>,
) -> Result<Vec<Edge<'a>>> {
    let mut current = to;
    let mut edges = Vec::new();
    while current != from {
        let edge = previous
            .get(current)
            .ok_or_else(|| Error::Data("spatial path predecessor is missing".into()))?;
        edges.push(edge.clone());
        current = edge.from;
    }
    edges.reverse();
    Ok(edges)
}

fn hop_path<'a>(
    graph: &Graph<'a>,
    from: &'a str,
    to: &str,
    mode: Option<&str>,
) -> Result<Option<Vec<Edge<'a>>>> {
    let mut queue = VecDeque::from([from]);
    let mut seen = BTreeSet::from([from]);
    let mut previous = BTreeMap::new();
    let mut expansions = 0;
    while let Some(current) = queue.pop_front() {
        if current == to {
            return Ok(Some(reconstruct(from, to, &previous)?));
        }
        expanded(&mut expansions)?;
        for edge in graph.edges(current, mode) {
            if seen.insert(edge.to) {
                previous.insert(edge.to, edge.clone());
                queue.push_back(edge.to);
            }
        }
    }
    Ok(None)
}

fn weight<'a>(edge: &Edge<'a>, metric: &str) -> Result<Option<(&'a str, Decimal)>> {
    let EdgeSource::Route(route) = &edge.source else {
        return Ok(None);
    };
    let Some(value) = route.record.get(metric) else {
        return Ok(None);
    };
    let fields = mapping(value, metric)?;
    let unit = text(&fields["unit"], "metric.unit")?;
    let Yaml::Number(number) = &fields["value"] else {
        return Err(invalid("metric.value must be a number"));
    };
    Ok(Some((unit, Decimal::number(number)?)))
}

fn route_sequence<'a>(
    from: &str,
    to: &str,
    previous: &BTreeMap<&str, Edge<'a>>,
) -> Result<Vec<&'a str>> {
    let mut current = to;
    let mut ids = Vec::new();
    while current != from {
        let edge = previous
            .get(current)
            .ok_or_else(|| Error::Data("spatial path predecessor is missing".into()))?;
        ids.push(edge.record().id.as_str());
        current = edge.from;
    }
    ids.reverse();
    Ok(ids)
}

fn weighted_path<'a>(
    graph: &Graph<'a>,
    from: &'a str,
    to: &str,
    mode: Option<&str>,
    metric: &str,
    unit: &str,
    expansions: &mut usize,
) -> Result<Option<(Vec<Edge<'a>>, Decimal)>> {
    let mut queue = BinaryHeap::from([Reverse((Decimal::zero(), 0_usize, from))]);
    let mut best = BTreeMap::from([(from, (Decimal::zero(), 0_usize))]);
    let mut previous = BTreeMap::new();
    while let Some(Reverse((total, hops, current))) = queue.pop() {
        if !best
            .get(current)
            .is_some_and(|known| known.0 == total && known.1 == hops)
        {
            continue;
        }
        if current == to {
            return Ok(Some((reconstruct(from, to, &previous)?, total)));
        }
        expanded(expansions)?;
        for edge in graph.edges(current, mode) {
            let Some((edge_unit, edge_weight)) = weight(edge, metric)? else {
                continue;
            };
            if edge_unit != unit {
                continue;
            }
            let candidate = (total.add(&edge_weight)?, hops + 1);
            match best
                .get(edge.to)
                .map_or(Ordering::Less, |known| candidate.cmp(known))
            {
                Ordering::Less => {
                    best.insert(edge.to, candidate.clone());
                    previous.insert(edge.to, edge.clone());
                    queue.push(Reverse((candidate.0, candidate.1, edge.to)));
                }
                Ordering::Equal => {
                    let mut ids = route_sequence(from, current, &previous)?;
                    ids.push(&edge.record().id);
                    if ids < route_sequence(from, edge.to, &previous)? {
                        // With nonnegative cost and increasing hops, every equal
                        // candidate's predecessor settles before its destination.
                        // Its final lexicographic tie is ready before expansion.
                        previous.insert(edge.to, edge.clone());
                    }
                }
                Ordering::Greater => {}
            }
        }
    }
    Ok(None)
}

/// Authored directed paths. Weighted totals use exact decimal arithmetic within
/// one authored unit; ties prefer fewer hops, then the route-ID sequence.
pub fn path(
    world: &World,
    from: &str,
    to: &str,
    mode: Option<&str>,
    metric: &str,
    unit: Option<&str>,
    limit: usize,
) -> Result<Json> {
    arguments(mode, limit)?;
    if !["hops", "route_distance", "travel_cost", "duration"].contains(&metric) {
        return Err(Error::InvalidInput(
            "metric must be hops, route_distance, travel_cost, or duration".into(),
        ));
    }
    if metric == "hops" && unit.is_some() {
        return Err(Error::InvalidInput(
            "unit is unsupported with metric hops".into(),
        ));
    }
    if unit.is_some_and(|unit| unit.trim().is_empty()) {
        return Err(Error::InvalidInput(
            "unit must be a nonblank authored unit".into(),
        ));
    }
    let from = selected(world, from)?;
    let to = selected(world, to)?;
    let graph = Graph::new(world)?;
    let mut total = Json::Null;
    let traversed = if metric == "hops" {
        hop_path(&graph, &from.id, &to.id, mode)?
    } else if from.id == to.id {
        total = json!({"value":"0","unit":unit});
        Some(Vec::new())
    } else {
        let units = if let Some(unit) = unit {
            BTreeSet::from([unit])
        } else {
            let mut units = BTreeSet::new();
            for edge in graph.edges(&from.id, mode) {
                if let Some((unit, _)) = weight(edge, metric)? {
                    units.insert(unit);
                }
            }
            units
        };
        let mut found = None;
        let mut expansions = 0;
        for unit in units {
            if let Some((edges, value)) = weighted_path(
                &graph,
                &from.id,
                &to.id,
                mode,
                metric,
                unit,
                &mut expansions,
            )? {
                if found.is_some() {
                    return Err(Error::InvalidInput(
                        "multiple authored metric units reach the destination; specify unit".into(),
                    ));
                }
                total = json!({"value":value.to_string(),"unit":unit});
                found = Some(edges);
            }
        }
        found
    };
    let found = traversed.is_some();
    let mut locations = Vec::new();
    let mut edges = Vec::new();
    if let Some(traversed) = traversed {
        bounded(traversed.len() + 1, limit)?;
        locations.push(from.id.as_str());
        for edge in traversed {
            locations.push(edge.to);
            edges.push(edge_value(world, &edge)?);
        }
    }
    let route_ids: Vec<_> = edges
        .iter()
        .filter_map(|edge| edge.get("routeId").cloned())
        .collect();
    let portal_ids: Vec<_> = edges
        .iter()
        .filter_map(|edge| edge.get("portalId").cloned())
        .collect();
    let edge_ids: Vec<_> = edges
        .iter()
        .map(|edge| {
            edge.get("routeId")
                .or_else(|| edge.get("portalId"))
                .unwrap()
                .clone()
        })
        .collect();
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,
        "basis":if metric=="hops" {"authored-directed-route-and-location-portal-edges"}else{"authored-directed-routes"},"metric":metric,"total":total,"fromLocationId":from.id,"toLocationId":to.id,
        "found":found,"hops":found.then_some(edges.len()),"locationIds":locations,"routeIds":route_ids,"portalIds":portal_ids,"edgeIds":edge_ids,"edges":edges,
        "mode":mode,"limit":limit}),
    )
}
