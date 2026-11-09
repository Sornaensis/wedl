//! Explicit author spatial history, exact applicability and retained corrections.
//! Effective values borrow the actual source record; they never rewrite a World.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet};

const CAPABILITY: &str = "spatial-history-v1";
const MAX_MEMBERS: usize = 1024;
const MAX_LIMIT: usize = 1000;
type Coordinate = (i64, i32);
const FIRST: Coordinate = (i64::MIN, i32::MIN);
const LAST: Coordinate = (i64::MAX, i32::MAX);

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn mapping<'a>(value: &'a Yaml, field: &str) -> Result<&'a Mapping> {
    match value {
        Yaml::Mapping(value) => Ok(value),
        _ => Err(invalid(format!("{field} must be an untagged mapping"))),
    }
}
fn closed<'a>(
    value: &'a Yaml,
    required: &[&str],
    optional: &[&str],
    field: &str,
) -> Result<&'a Mapping> {
    let fields = mapping(value, field)?;
    if required.iter().any(|key| !fields.contains_key(*key))
        || fields.keys().any(|key| {
            !matches!(key,Yaml::String(key) if required.contains(&key.as_str()) || optional.contains(&key.as_str()))
        })
    {
        return Err(invalid(format!("{field} has a closed field contract")));
    }
    Ok(fields)
}
fn text<'a>(value: &'a Yaml, field: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!(
            "{field} must be a nonblank untagged string"
        ))),
    }
}
fn local_id(value: &str) -> bool {
    value.strip_prefix("spatialhistory_").is_some_and(|tail| {
        tail.len() == 26
            && tail
                .bytes()
                .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
    })
}

#[derive(Clone, Copy)]
enum Valid<'a> {
    Timeless,
    Interval {
        timeline: &'a str,
        start: Option<Coordinate>,
        end: Option<Coordinate>,
    },
}
impl Valid<'_> {
    fn bounds(&self) -> (Option<&str>, Coordinate, Coordinate) {
        match self {
            Self::Timeless => (None, FIRST, LAST),
            Self::Interval {
                timeline,
                start,
                end,
            } => (Some(timeline), start.unwrap_or(FIRST), end.unwrap_or(LAST)),
        }
    }
    fn applies(&self, at: &StoryTime) -> bool {
        let (timeline, start, end) = self.bounds();
        timeline.is_none_or(|timeline| timeline == at.timeline)
            && start <= (at.tick, at.order)
            && (at.tick, at.order) <= end
    }
    fn overlaps(&self, other: &Self) -> bool {
        let (left, start, end) = self.bounds();
        let (right, other_start, other_end) = other.bounds();
        (left.is_none() || right.is_none() || left == right)
            && start.max(other_start) <= end.min(other_end)
    }
}
fn point(value: &Yaml, timeline: &str, field: &str) -> Result<Coordinate> {
    let fields = closed(value, &["timeline", "tick", "order"], &[], field)?;
    if text(&fields["timeline"], field)? != timeline {
        return Err(invalid(format!("{field} must use the interval timeline")));
    }
    let tick = match &fields["tick"] {
        Yaml::Number(value) => value.as_i64(),
        _ => None,
    }
    .ok_or_else(|| invalid(format!("{field}.tick must be an exact signed i64")))?;
    let order = match &fields["order"] {
        Yaml::Number(value) => value.as_i64().and_then(|value| i32::try_from(value).ok()),
        _ => None,
    }
    .ok_or_else(|| invalid(format!("{field}.order must be an exact signed i32")))?;
    Ok((tick, order))
}
fn validity<'a>(value: &'a Yaml, field: &str) -> Result<Valid<'a>> {
    let fields = mapping(value, field)?;
    let kind = fields
        .get("kind")
        .ok_or_else(|| invalid(format!("{field}.kind is required")))?;
    match text(kind, field)? {
        "timeless" => {
            closed(value, &["kind"], &[], field)?;
            Ok(Valid::Timeless)
        }
        "interval" => {
            let fields = closed(value, &["kind", "timeline"], &["start", "end"], field)?;
            let timeline = text(&fields["timeline"], field)?;
            let start = fields
                .get("start")
                .map(|value| point(value, timeline, &format!("{field}.start")))
                .transpose()?;
            let end = fields
                .get("end")
                .map(|value| point(value, timeline, &format!("{field}.end")))
                .transpose()?;
            if start.unwrap_or(FIRST) > end.unwrap_or(LAST) {
                return Err(invalid(format!("{field} start must not follow end")));
            }
            Ok(Valid::Interval {
                timeline,
                start,
                end,
            })
        }
        _ => Err(invalid(format!(
            "{field}.kind must be timeless or interval"
        ))),
    }
}

struct Member<'a> {
    id: &'a str,
    aspect: &'a str,
    field: &'a str,
    valid: Valid<'a>,
    valid_source: &'a Yaml,
    value: &'a Yaml,
    replaces: Option<&'a str>,
}
impl<'a> Member<'a> {
    fn field_value(&self) -> &'a Yaml {
        if self.field == "spatial" {
            self.value
        } else {
            &self.value[self.field]
        }
    }
}
fn members(value: &Yaml, check_conflicts: bool) -> Result<Vec<Member<'_>>> {
    let Yaml::Sequence(values) = value else {
        return Err(invalid("spatial_history must be an ordered untagged array"));
    };
    if values.len() > MAX_MEMBERS {
        return Err(invalid("spatial_history exceeds 1024 members"));
    }
    let mut result: Vec<Member<'_>> = Vec::with_capacity(values.len());
    let mut earlier: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        let label = format!("spatial_history[{index}]");
        let fields = closed(
            value,
            &["id", "aspect", "valid", "value"],
            &["replaces"],
            &label,
        )?;
        let id = text(&fields["id"], &format!("{label}.id"))?;
        if !local_id(id) || earlier.contains_key(id) {
            return Err(invalid(format!(
                "{label}.id must be a locally unique spatialhistory_ ID"
            )));
        }
        let aspect = text(&fields["aspect"], &format!("{label}.aspect"))?;
        let valid_source = &fields["valid"];
        let valid = validity(valid_source, &format!("{label}.valid"))?;
        let data = &fields["value"];
        let field = match aspect {
            "placement" | "geometry" => {
                let position = closed(
                    data,
                    &["map_id", "geometry"],
                    &[],
                    &format!("{label}.value"),
                )?;
                let map_id = text(&position["map_id"], &format!("{label}.value.map_id"))?;
                if !valid_id_for_kind(map_id, "map") {
                    return Err(invalid(format!(
                        "{label}.value.map_id must be an exact map ID"
                    )));
                }
                crate::geometry::validate_shape(&position["geometry"])?;
                "spatial"
            }
            "parentage" => {
                let fields = closed(data, &["parent_id"], &[], &format!("{label}.value"))?;
                if !matches!(&fields["parent_id"], Yaml::Null) {
                    let parent = text(&fields["parent_id"], &format!("{label}.value.parent_id"))?;
                    if !valid_id_for_kind(parent, "location") {
                        return Err(invalid(format!(
                            "{label}.value.parent_id must be a location ID or null"
                        )));
                    }
                }
                "parent_id"
            }
            "route-availability" => {
                let fields = closed(data, &["availability"], &[], &format!("{label}.value"))?;
                let value = text(
                    &fields["availability"],
                    &format!("{label}.value.availability"),
                )?;
                if !["open", "closed", "restricted", "unknown"].contains(&value) {
                    return Err(invalid(format!("{label}.value.availability is invalid")));
                }
                "availability"
            }
            "route-metric" => {
                let fields = mapping(data, &format!("{label}.value"))?;
                if fields.len() != 1 {
                    return Err(invalid(format!(
                        "{label}.value must name exactly one route metric"
                    )));
                }
                let (key, value) = fields.iter().next().unwrap();
                let metric = text(key, &format!("{label}.value"))?;
                if !["route_distance", "travel_cost", "duration"].contains(&metric) {
                    return Err(invalid(format!(
                        "{label}.value names an unsupported route metric"
                    )));
                }
                crate::spatial::metric(value, &format!("{label}.value.{metric}"))?;
                metric
            }
            _ => return Err(invalid(format!("{label}.aspect is unsupported"))),
        };
        let replaces = fields
            .get("replaces")
            .map(|v| text(v, &format!("{label}.replaces")))
            .transpose()?;
        if check_conflicts {
            let mut ancestors = BTreeSet::new();
            if let Some(replaced) = replaces {
                let previous: &Member<'_> = earlier
                    .get(replaced)
                    .map(|index| &result[*index])
                    .ok_or_else(|| {
                        invalid(format!(
                            "{label}.replaces must name an earlier member of this owner"
                        ))
                    })?;
                if aspect != previous.aspect
                    || valid_source != previous.valid_source
                    || field != previous.field
                {
                    return Err(invalid(format!(
                        "{label}.replaces must retain the exact aspect, validity and metric target"
                    )));
                }
                let mut current = Some(replaced);
                while let Some(id) = current {
                    ancestors.insert(id);
                    current = result[earlier[id]].replaces;
                }
            }
            for previous in &result {
                if previous.field == field
                    && valid.overlaps(&previous.valid)
                    && !ancestors.contains(previous.id)
                {
                    return Err(invalid(format!(
                        "{label} overlaps the same effective field without an exact correction chain"
                    )));
                }
            }
        }
        earlier.insert(id, result.len());
        result.push(Member {
            id,
            aspect,
            field,
            valid,
            valid_source,
            value: data,
            replaces,
        });
    }
    Ok(result)
}

/// Closed request grammar. Source references, capabilities and cross-record
/// consistency remain candidate-world validation checks.
pub fn validate_request(value: &Yaml) -> Result<()> {
    members(value, false)
        .map(|_| ())
        .map_err(|error| Error::InvalidInput(error.to_string()))
}
fn capability(world: &World, name: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|record| record.list("capabilities"))
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| matches!(value,Yaml::String(value) if value==name))
        })
}
fn require_capabilities(world: &World) -> Result<()> {
    if capability(world, "spatial-core-v1") && capability(world, CAPABILITY) {
        Ok(())
    } else {
        Err(invalid(
            "spatial history requires declared spatial-core-v1 and spatial-history-v1 capabilities",
        ))
    }
}
fn actual(world: &World, record: &Record) -> Result<()> {
    if world
        .records
        .get(&record.id)
        .is_some_and(|owner| std::ptr::eq(owner, record))
    {
        Ok(())
    } else {
        Err(invalid("spatial history requires the actual source owner"))
    }
}
fn record_members<'a>(world: &World, record: &'a Record) -> Result<Vec<Member<'a>>> {
    actual(world, record)?;
    let Some(value) = record.get("spatial_history") else {
        return Ok(Vec::new());
    };
    require_capabilities(world)?;
    if !["location", "route"].contains(&record.kind.as_str()) {
        return Err(invalid("only locations and routes may own spatial_history"));
    }
    let canonical = match record.get("status") {
        None => true,
        Some(Yaml::String(value)) if value == "canonical" => true,
        Some(Yaml::String(value)) if value == "retired" => false,
        _ => {
            return Err(invalid(
                "spatial history owner status must be canonical or retired",
            ));
        }
    };
    let values = members(value, true)?;
    for member in &values {
        if let Valid::Interval { timeline, .. } = member.valid {
            world
                .story_time(0, Some(timeline), 0)
                .map_err(|e| invalid(e.to_string()))?;
        }
        match (record.kind.as_str(), member.aspect) {
            ("location", "placement" | "geometry") => {
                if !capability(world, "geometry-v1") {
                    return Err(invalid(
                        "spatial placement/geometry history requires geometry-v1",
                    ));
                }
                crate::geometry::validate_placement(world, member.value, canonical)?;
            }
            ("location", "parentage") => {
                if let Yaml::String(id) = member.field_value() {
                    let target = world
                        .records
                        .get(id)
                        .filter(|r| r.kind == "location")
                        .ok_or_else(|| {
                            invalid("historical parent must be an existing exact location ID")
                        })?;
                    if target.id == record.id {
                        return Err(invalid("historical parent cannot be the owner location"));
                    }
                    if canonical && target.status() != "canonical" {
                        return Err(invalid(
                            "canonical historical parentage requires a canonical parent",
                        ));
                    }
                }
            }
            ("route", "route-availability" | "route-metric") => {
                if !capability(world, "route-v1") {
                    return Err(invalid("route history requires route-v1"));
                }
            }
            _ => {
                return Err(invalid(
                    "spatial history aspect does not belong to its owner kind",
                ));
            }
        }
    }
    Ok(values)
}
fn selected<'a, 'b>(members: &'b [Member<'a>], at: &StoryTime) -> Vec<&'b Member<'a>> {
    let eligible: Vec<_> = members
        .iter()
        .filter(|member| member.valid.applies(at))
        .collect();
    let replaced: BTreeSet<_> = eligible
        .iter()
        .filter_map(|member| member.replaces)
        .collect();
    eligible
        .into_iter()
        .filter(|member| !replaced.contains(member.id))
        .collect()
}
fn successor((tick, order): Coordinate) -> Option<Coordinate> {
    if order < i32::MAX {
        Some((tick, order + 1))
    } else if tick < i64::MAX {
        Some((tick + 1, i32::MIN))
    } else {
        None
    }
}
fn diagnostic(record: &Record, error: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "WDL-SPATIAL-HISTORY-001".into(),
        message: error.into(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some("spatial_history".into()),
    }
}
/// Validate every source history, including retired owners and containment at
/// the exact boundaries where an effective parent can change.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut histories = BTreeMap::new();
    for record in world
        .records
        .values()
        .filter(|record| record.get("spatial_history").is_some())
    {
        match record_members(world, record) {
            Ok(values) => {
                histories.insert(record.id.as_str(), values);
            }
            Err(error) => diagnostics.push(diagnostic(record, error.to_string())),
        }
    }
    if !diagnostics.is_empty() || histories.is_empty() {
        return diagnostics;
    }
    let locations: Vec<_> = world
        .records
        .values()
        .filter(|r| r.kind == "location")
        .collect();
    let timelines: BTreeSet<_> = world
        .world_record()
        .ok()
        .and_then(|r| r.list("timelines"))
        .into_iter()
        .flatten()
        .filter_map(|value| value.get("id").and_then(Yaml::as_str))
        .collect();
    for timeline in timelines {
        let mut boundaries = BTreeSet::from([FIRST]);
        for values in histories.values() {
            for member in values.iter().filter(|member| member.field == "parent_id") {
                let (own, start, end) = member.valid.bounds();
                if own.is_none_or(|own| own == timeline) {
                    boundaries.insert(start);
                    boundaries.insert(end);
                    if let Some(next) = successor(end) {
                        boundaries.insert(next);
                    }
                }
            }
        }
        for (tick, order) in boundaries {
            let at = StoryTime {
                timeline: timeline.into(),
                tick,
                order,
            };
            let mut parents = BTreeMap::new();
            for record in &locations {
                let active = histories
                    .get(record.id.as_str())
                    .map(|members| selected(members, &at))
                    .unwrap_or_default();
                let value = active
                    .iter()
                    .find(|member| member.field == "parent_id")
                    .map(|member| member.field_value())
                    .or_else(|| record.get("parent_id"));
                let parent = match value {
                    None | Some(Yaml::Null) => None,
                    Some(Yaml::String(id)) => Some(id.as_str()),
                    _ => {
                        diagnostics.push(diagnostic(
                            record,
                            "effective parentage requires exact location IDs or null",
                        ));
                        return diagnostics;
                    }
                };
                parents.insert(record.id.as_str(), parent);
            }
            let mut finished = BTreeSet::new();
            for record in &locations {
                let mut current = Some(record.id.as_str());
                let mut chain = BTreeSet::new();
                while let Some(id) = current {
                    if finished.contains(id) {
                        break;
                    }
                    if !chain.insert(id) {
                        diagnostics.push(diagnostic(record,format!("effective location parentage contains a cycle at {timeline}:{tick}/{order}")));
                        return diagnostics;
                    }
                    current = parents.get(id).copied().flatten();
                }
                finished.extend(chain);
            }
        }
    }
    diagnostics
}
fn check_source(world: &World) -> Result<()> {
    require_capabilities(world)?;
    if let Some(problem) = validate(world).first() {
        return Err(invalid(problem.message.clone()));
    }
    Ok(())
}
/// Shared entry gate for dated geometry and route adapters.
pub(crate) fn check_query(world: &World, at: &StoryTime) -> Result<()> {
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    check_source(world)
}
fn known_field(field: &str) -> Result<()> {
    if [
        "spatial",
        "parent_id",
        "availability",
        "route_distance",
        "travel_cost",
        "duration",
    ]
    .contains(&field)
    {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            "unsupported effective spatial field".into(),
        ))
    }
}
/// Borrow the winning authored value, or the exact original static fallback.
pub(crate) fn effective_value<'a>(
    world: &World,
    record: &'a Record,
    field: &str,
    at: &StoryTime,
) -> Result<Option<&'a Yaml>> {
    actual(world, record)?;
    known_field(field)?;
    let members = record_members(world, record)?;
    Ok(selected(&members, at)
        .into_iter()
        .find(|member| member.field == field)
        .map(|member| member.field_value())
        .or_else(|| record.get(field)))
}
fn citation(world: &World, record: &Record, field: &str, member_id: Option<&str>) -> Json {
    let mut value = json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":field});
    if let Some(member_id) = member_id {
        value["memberId"] = json!(member_id);
    }
    value
}
fn chain_citations(
    world: &World,
    record: &Record,
    members: &[Member<'_>],
    member: &Member<'_>,
) -> Vec<Json> {
    let mut chain = vec![member.id];
    let mut previous = member.replaces;
    while let Some(id) = previous {
        chain.push(id);
        previous = members
            .iter()
            .find(|member| member.id == id)
            .and_then(|member| member.replaces);
    }
    chain.reverse();
    chain
        .into_iter()
        .map(|id| citation(world, record, &format!("spatial_history.{id}"), Some(id)))
        .collect()
}
/// Exact field evidence. An absent static field has no authored citation.
pub(crate) fn field_citations(
    world: &World,
    record: &Record,
    field: &str,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    actual(world, record)?;
    known_field(field)?;
    let members = record_members(world, record)?;
    if let Some(member) = selected(&members, at)
        .into_iter()
        .find(|member| member.field == field)
    {
        Ok(chain_citations(world, record, &members, member))
    } else if record.get(field).is_some() {
        Ok(vec![citation(world, record, field, None)])
    } else {
        Ok(Vec::new())
    }
}
/// Complete authored member history, or applicable unsuperseded members at an
/// exact author horizon. All-time reads retain every correction source member.
pub fn history(
    world: &World,
    reference: &str,
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Json> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Error::InvalidInput(
            "spatial history limit must be from 1 to 1000".into(),
        ));
    }
    if let Some(at) = at {
        check_query(world, at)?;
    } else {
        check_source(world)?;
    }
    let record = world.resolve(reference)?;
    if !["location", "route"].contains(&record.kind.as_str()) || record.status() != "canonical" {
        return Err(Error::NotFound(
            "canonical spatial history owner is unavailable".into(),
        ));
    }
    let members = record_members(world, record)?;
    let chosen = if let Some(at) = at {
        selected(&members, at)
    } else {
        members.iter().collect()
    };
    if chosen.len() > limit {
        return Err(Error::InvalidInput(format!(
            "spatial history has more than {limit} members; refusing an incomplete result"
        )));
    }
    let mut items = Vec::with_capacity(chosen.len());
    for member in chosen {
        let mut item = json!({"memberId":member.id,"aspect":member.aspect,
            "valid":crate::service::yaml_json(member.valid_source)?,"value":crate::service::yaml_json(member.value)?,
            "citations":chain_citations(world,record,&members,member)});
        if let Some(replaces) = member.replaces {
            item["replaces"] = json!(replaces);
        }
        items.push(item);
    }
    Ok(json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some() {"author-as-of-spatial-history"} else {"author-all-time-spatial-history"},
        "basis":"authored-spatial-history","ownerId":record.id,"ownerKind":record.kind,"title":record.title,
        "at":at,"count":items.len(),"items":items,"limit":limit}))
}

/// Character history gates every referenced endpoint before correction suppression.
pub(crate) fn character_values(
    world: &World,
    owner: &Record,
    at: &StoryTime,
    allowed: &BTreeSet<String>,
) -> Result<Vec<Json>> {
    let members = record_members(world, owner)?;
    let eligible = members
        .iter()
        .filter(|member| {
            if !member.valid.applies(at) {
                return false;
            }
            let endpoint = match member.aspect {
                "placement" | "geometry" => member.value.get("map_id").and_then(Yaml::as_str),
                "parentage" => member.value.get("parent_id").and_then(Yaml::as_str),
                _ => None,
            };
            endpoint.is_none_or(|id| allowed.contains(id))
        })
        .collect::<Vec<_>>();
    let ids = eligible
        .iter()
        .map(|member| member.id)
        .collect::<BTreeSet<_>>();
    let suppressed = eligible
        .iter()
        .filter_map(|member| member.replaces)
        .collect::<BTreeSet<_>>();
    eligible
        .into_iter()
        .filter(|member| !suppressed.contains(member.id))
        .map(|member| {
            let mut item = json!({"memberId":member.id,"aspect":member.aspect,
            "valid":crate::service::yaml_json(member.valid_source)?,
            "value":crate::service::yaml_json(member.value)?,
            "target":{"kind":"section","ownerId":owner.id,"sectionId":member.id}});
            if let Some(previous) = member.replaces.filter(|id| ids.contains(id)) {
                item["replaces"] = json!(previous);
            }
            Ok(item)
        })
        .collect()
}
