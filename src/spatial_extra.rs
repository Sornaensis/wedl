//! Explicit directed portals and scoped overlay membership. These presentation
//! filters never grant a character knowledge of an authored spatial fact.
use super::*;
use crate::model::StoryTime;

fn closed<'a>(value: &'a Yaml, keys: &[&str], label: &str) -> Result<&'a Mapping> {
    let fields = mapping(value, label)?;
    if fields.len() != keys.len()
        || fields
            .keys()
            .any(|key| !matches!(key, Yaml::String(key) if keys.contains(&key.as_str())))
    {
        return Err(invalid(format!(
            "{label} requires exactly {}",
            keys.join(", ")
        )));
    }
    Ok(fields)
}

fn ordered_strings<'a>(value: &'a Yaml, label: &str, nonempty: bool) -> Result<Vec<&'a str>> {
    let Yaml::Sequence(values) = value else {
        return Err(invalid(format!("{label} must be a sorted unique array")));
    };
    if nonempty && values.is_empty() {
        return Err(invalid(format!("{label} must not be empty")));
    }
    let mut result = Vec::new();
    for value in values {
        let value = text(value, label)?;
        if result.last().is_some_and(|old| *old >= value) {
            return Err(invalid(format!("{label} must be sorted and unique")));
        }
        result.push(value);
    }
    Ok(result)
}

fn enabled(world: &World, capability: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|record| record.list("capabilities"))
        .is_some_and(|values| {
            ["spatial-core-v1", capability].iter().all(|capability| {
                values
                    .iter()
                    .any(|value| matches!(value,Yaml::String(value) if value==capability))
            })
        })
}

#[derive(Clone)]
pub(super) enum Target<'a> {
    Location(&'a str),
    Position(&'a Yaml),
}

#[derive(Clone)]
pub(super) struct Portal<'a> {
    pub record: &'a Record,
    pub from: &'a str,
    pub target: Target<'a>,
    pub modes: Vec<&'a str>,
}

pub(super) fn portal<'a>(world: &'a World, record: &'a Record) -> Result<Portal<'a>> {
    let canonical = status(record)? == "canonical";
    if !valid_id_for_kind(&record.id, "portal") {
        return Err(invalid("portal ID is invalid"));
    }
    let from = location_ref(
        world,
        required(record, "from_location_id")?,
        "from_location_id",
    )?;
    if canonical && status(from)? != "canonical" {
        return Err(invalid(
            "a canonical portal requires a canonical source location",
        ));
    }
    let target = required(record, "to")?;
    let target = if matches!(target, Yaml::String(_)) {
        let to = location_ref(world, target, "to")?;
        if to.id == from.id {
            return Err(invalid("portal location endpoints must be distinct"));
        }
        if canonical && status(to)? != "canonical" {
            return Err(invalid(
                "a canonical portal requires a canonical target location",
            ));
        }
        Target::Location(to.id.as_str())
    } else {
        crate::geometry::validate_position(world, target, canonical)?;
        Target::Position(target)
    };
    let modes = ordered_strings(required(record, "modes")?, "modes", true)?;
    for field in [
        "direction",
        "to_location_id",
        "availability",
        "uncertainty",
        "route_distance",
        "travel_cost",
        "duration",
    ] {
        if record.get(field).is_some() {
            return Err(invalid(format!("portal does not declare {field}")));
        }
    }
    Ok(Portal {
        record,
        from: from.id.as_str(),
        target,
        modes,
    })
}

pub(super) fn portal_value(world: &World, portal: &Portal<'_>) -> Result<Json> {
    let mut value = json!({"portalId":portal.record.id,"fromLocationId":portal.from,
        "modes":portal.modes,"citation":citation(world,portal.record,"portal")});
    match portal.target {
        Target::Location(id) => {
            value["targetKind"] = json!("location");
            value["toLocationId"] = json!(id);
        }
        Target::Position(position) => {
            value["targetKind"] = json!("position");
            value["position"] = crate::service::yaml_json(position)?;
        }
    }
    Ok(value)
}

struct Overlay<'a> {
    record: &'a Record,
    lifecycle: &'a str,
    locations: Vec<&'a str>,
    audience: Vec<&'a str>,
    perspectives: Vec<&'a str>,
    valid: Option<(StoryTime, StoryTime)>,
}

fn point(world: &World, value: &Yaml, label: &str) -> Result<StoryTime> {
    let fields = closed(value, &["timeline", "tick", "order"], label)?;
    text(&fields["timeline"], label)?;
    if !matches!(&fields["tick"],Yaml::Number(value) if value.as_i64().is_some())
        || !matches!(&fields["order"],Yaml::Number(value) if value.as_i64().and_then(|n|i32::try_from(n).ok()).is_some())
    {
        return Err(invalid(format!(
            "{label} requires exact signed i64 tick and i32 order"
        )));
    }
    let at = StoryTime::from_yaml(value, &world.default_timeline())?;
    world.story_time(at.tick, Some(&at.timeline), at.order)
}

fn overlay<'a>(world: &'a World, record: &'a Record) -> Result<Overlay<'a>> {
    let canonical = status(record)? == "canonical";
    if !valid_id_for_kind(&record.id, "overlay") {
        return Err(invalid("overlay ID is invalid"));
    }
    let lifecycle = text(required(record, "lifecycle")?, "lifecycle")?;
    let membership = closed(
        required(record, "membership")?,
        &["location_ids"],
        "membership",
    )?;
    let locations = ordered_strings(
        &membership["location_ids"],
        "membership.location_ids",
        false,
    )?;
    for id in &locations {
        let target = world
            .records
            .get(*id)
            .filter(|target| target.kind == "location" && valid_id_for_kind(id, "location"))
            .ok_or_else(|| invalid("overlay membership requires exact existing location IDs"))?;
        if canonical && status(target)? != "canonical" {
            return Err(invalid(
                "a canonical overlay requires canonical member locations",
            ));
        }
    }
    let audience = ordered_strings(required(record, "audience")?, "audience", false)?;
    let perspectives = ordered_strings(required(record, "perspectives")?, "perspectives", false)?;
    for field in ["faction", "institution", "calendar"] {
        if record.get(field).is_some() {
            return Err(invalid(format!(
                "overlay {field} is not an authored membership field"
            )));
        }
    }
    let valid = match lifecycle {
        "static" if record.get("valid").is_none() => None,
        "static" => return Err(invalid("static overlay must omit valid")),
        "time-bounded" => {
            let fields = closed(required(record, "valid")?, &["start", "end"], "valid")?;
            let start = point(world, &fields["start"], "valid.start")?;
            let end = point(world, &fields["end"], "valid.end")?;
            if !start.not_after(&end) {
                return Err(invalid("overlay validity must be ordered on one timeline"));
            }
            Some((start, end))
        }
        _ => return Err(invalid("overlay lifecycle must be static or time-bounded")),
    };
    Ok(Overlay {
        record,
        lifecycle,
        locations,
        audience,
        perspectives,
        valid,
    })
}

/// Literal territory membership for author governance. History does not invent
/// a membership horizon for time-bounded overlays.
pub(crate) fn overlay_matches(
    world: &World,
    record: &Record,
    target_id: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<bool> {
    if record.kind != "overlay" || record.status() != "canonical" {
        return Ok(false);
    }
    let value = overlay(world, record)?;
    if !value.audience.contains(&audience) || !value.perspectives.contains(&view) {
        return Ok(false);
    }
    if record.id != target_id && !value.locations.contains(&target_id) {
        return Ok(false);
    }
    Ok(match value.valid {
        None => true,
        Some((start, end)) => at.is_some_and(|at| start.not_after(at) && at.not_after(&end)),
    })
}

pub(super) fn validate(world: &World) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| ["portal", "overlay"].contains(&record.kind.as_str()))
    {
        let capability = if record.kind == "portal" {
            "route-v1"
        } else {
            "overlay-v1"
        };
        if !enabled(world, capability) {
            diagnostics.push(diagnostic(
                record,
                "NAR-CAPABILITY-001",
                format!(
                    "{} requires declared spatial-core-v1 and {capability}",
                    record.kind
                ),
            ));
        }
        let result = if record.kind == "portal" {
            portal(world, record).map(|_| ())
        } else {
            overlay(world, record).map(|_| ())
        };
        if let Err(error) = result {
            diagnostics.push(diagnostic(
                record,
                if record.kind == "portal" {
                    "WDL-PORTAL-001"
                } else {
                    "WDL-OVERLAY-001"
                },
                error.to_string(),
            ));
        }
    }
    diagnostics
}

/// Complete directed reachability through open routes and location portals.
/// Map positions, containment, links and anchors do not add location edges.
pub fn reachable(world: &World, reference: &str, mode: Option<&str>, limit: usize) -> Result<Json> {
    arguments(mode, limit)?;
    let record = selected(world, reference)?;
    let graph = Graph::new(world)?;
    let mut seen = BTreeSet::from([record.id.as_str()]);
    let mut queue = VecDeque::from([record.id.as_str()]);
    let mut expansions = 0;
    while let Some(current) = queue.pop_front() {
        expanded(&mut expansions)?;
        for edge in graph.edges(current, mode) {
            if seen.insert(edge.to) {
                bounded(seen.len(), limit)?;
                queue.push_back(edge.to);
            }
        }
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,
        "basis":"authored-directed-route-and-location-portal-edges","locationId":record.id,
        "locationIds":seen,"count":seen.len(),"mode":mode,"limit":limit,"complete":true}),
    )
}

/// Exact authored overlay membership filtered by atlas audience/view labels.
/// This is an author presentation query, not character evidence admission.
pub fn overlays(
    world: &World,
    location: &str,
    at: &StoryTime,
    audience: &str,
    view: &str,
    overlay_id: Option<&str>,
    limit: usize,
) -> Result<Json> {
    arguments(None, limit)?;
    if audience.trim().is_empty() || view.trim().is_empty() {
        return Err(Error::InvalidInput(
            "overlay audience and view must be nonblank authored labels".into(),
        ));
    }
    if overlay_id.is_some_and(|id| !valid_id_for_kind(id, "overlay")) {
        return Err(Error::InvalidInput(
            "overlay selector must be an exact overlay ID".into(),
        ));
    }
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let location = selected(world, location)?;
    if let Some(error) = validate(world).first() {
        return Err(invalid(&error.message));
    }
    let mut scoped = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "overlay" && record.status() == "canonical")
    {
        let overlay = overlay(world, record)?;
        if overlay.audience.contains(&audience) && overlay.perspectives.contains(&view) {
            scoped.push(overlay);
        }
    }
    // The same error covers an absent, retired or differently scoped exact ID.
    // Authorization is checked before applicability to preserve that boundary.
    if overlay_id.is_some_and(|id| !scoped.iter().any(|overlay| overlay.record.id == id)) {
        return Err(Error::NotFound(
            "overlay is unavailable in this presentation scope".into(),
        ));
    }
    let mut items = Vec::new();
    for overlay in scoped {
        if overlay_id.is_some_and(|id| overlay.record.id != id)
            || !overlay.locations.contains(&location.id.as_str())
            || overlay
                .valid
                .as_ref()
                .is_some_and(|(start, end)| !start.not_after(at) || !at.not_after(end))
        {
            continue;
        }
        let valid = overlay
            .valid
            .map(|(start, end)| json!({"start":start,"end":end}));
        items.push(json!({"overlayId":overlay.record.id,"title":overlay.record.title,
            "lifecycle":overlay.lifecycle,"valid":valid,"citation":citation(world,overlay.record,"membership")}));
        bounded(items.len(), limit)?;
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":"author-as-of-overlay-membership",
        "effectiveTime":at,"locationId":location.id,"audience":audience,"view":view,"overlayId":overlay_id,
        "count":items.len(),"items":items,"limit":limit,"complete":true}),
    )
}
