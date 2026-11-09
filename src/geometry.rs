//! Author queries over explicit 2D/3D maps and static or dated placements.
//! Geometry and literal map anchors never supply travel or inferred conversion.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::BTreeMap;

const SCOPE: &str = "author-all-time-spatial-source";
const MAX_COORDINATE: f64 = 9_007_199_254_740_991.0;
const MAX_VERTICES: usize = 10_000;

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
fn required<'a>(record: &'a Record, field: &str) -> Result<&'a Yaml> {
    record
        .get(field)
        .ok_or_else(|| invalid(format!("{field} is required")))
}
fn closed<'a>(value: &'a Yaml, fields: &[&str], label: &str) -> Result<&'a Mapping> {
    match value {
        Yaml::Mapping(value)
            if value.len() == fields.len()
                && value.keys().all(
                    |key| matches!(key, Yaml::String(key) if fields.contains(&key.as_str())),
                ) =>
        {
            Ok(value)
        }
        _ => Err(invalid(format!(
            "{label} requires exactly {}",
            fields.join(", ")
        ))),
    }
}
fn canonical(record: &Record) -> Result<bool> {
    match record.get("status") {
        None => Ok(true),
        Some(Yaml::String(value)) if value == "canonical" => Ok(true),
        Some(Yaml::String(value)) if value == "retired" => Ok(false),
        _ => Err(invalid(
            "map/location/anchor status must be canonical or retired",
        )),
    }
}
fn valid_coordinate(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_COORDINATE
}
/// Shared shape check; map-specific dimensionality and bounds remain candidate checks.
pub(crate) fn coordinates(value: &Yaml, field: &str) -> Result<Vec<f64>> {
    let fail = || {
        invalid(format!(
            "{field} requires two or three finite numeric coordinates with absolute values at most 9007199254740991"
        ))
    };
    let Yaml::Sequence(values) = value else {
        return Err(fail());
    };
    if ![2, 3].contains(&values.len()) {
        return Err(fail());
    }
    values
        .iter()
        .map(|value| match value {
            Yaml::Number(value) => value
                .as_f64()
                .filter(|value| valid_coordinate(*value))
                .ok_or_else(fail),
            _ => Err(fail()),
        })
        .collect()
}
fn metric(value: &Yaml, label: &str) -> Result<()> {
    let fields = closed(value, &["value", "unit"], label)?;
    text(&fields["unit"], &format!("{label}.unit"))?;
    if !matches!(&fields["value"],Yaml::Number(value) if value.as_f64().is_some_and(|v|v.is_finite() && v>=0.0))
    {
        return Err(invalid(format!(
            "{label}.value must be a finite nonnegative number"
        )));
    }
    Ok(())
}
struct Map<'a> {
    record: &'a Record,
    canonical: bool,
    crs: &'a str,
    axes: [&'a str; 2],
    unit: &'a str,
    policy: &'a str,
    minimum: Vec<f64>,
    maximum: Vec<f64>,
}
impl<'a> Map<'a> {
    fn parse(record: &'a Record) -> Result<Self> {
        let canonical = canonical(record)?;
        if !valid_id_for_kind(&record.id, "map") {
            return Err(invalid("map ID is invalid"));
        }
        let crs = text(required(record, "crs")?, "crs")?;
        let expected = if crs
            .strip_prefix("local-planar:")
            .is_some_and(|name| !name.trim().is_empty())
        {
            ["east", "north"]
        } else if crs == "EPSG:4326" {
            ["longitude", "latitude"]
        } else {
            return Err(invalid(
                "crs must be local-planar: followed by a nonblank name, or EPSG:4326",
            ));
        };
        let axes = required(record, "axis_order")?;
        if !matches!(axes,Yaml::Sequence(values) if values.len()==2 && values.iter().zip(expected).all(|(value,expected)|matches!(value,Yaml::String(value) if value==expected)))
        {
            return Err(invalid(format!(
                "axis_order must be [{}, {}] for this CRS",
                expected[0], expected[1]
            )));
        }
        let unit = text(required(record, "unit")?, "unit")?;
        if crs == "EPSG:4326" && unit != "degree" {
            return Err(invalid("EPSG:4326 requires unit: degree"));
        }
        let policy = record
            .get("z_policy")
            .map(|v| text(v, "z_policy"))
            .transpose()?
            .unwrap_or("forbidden");
        if !["forbidden", "optional-level", "required"].contains(&policy) {
            return Err(invalid(
                "z_policy must be forbidden, optional-level or required",
            ));
        }
        let bounds = closed(required(record, "bounds")?, &["min", "max"], "bounds")?;
        let minimum = coordinates(&bounds["min"], "bounds.min")?;
        let maximum = coordinates(&bounds["max"], "bounds.max")?;
        if minimum.len() != maximum.len()
            || (policy == "forbidden" && minimum.len() != 2)
            || (policy == "required" && minimum.len() != 3)
        {
            return Err(invalid("map bounds dimensionality must match its z_policy"));
        }
        if minimum.iter().zip(&maximum).any(|(low, high)| low >= high) {
            return Err(invalid("map bounds require min < max on every axis"));
        }
        if crs == "EPSG:4326"
            && (minimum[0] < -180.0
                || maximum[0] > 180.0
                || minimum[1] < -90.0
                || maximum[1] > 90.0)
        {
            return Err(invalid(
                "EPSG:4326 bounds require longitude in [-180,180] and latitude in [-90,90]",
            ));
        }
        if let Some(value) = record.get("origin") {
            let origin = closed(value, &["label", "coordinates"], "origin")?;
            text(&origin["label"], "origin.label")?;
            if coordinates(&origin["coordinates"], "origin.coordinates")?.len() != minimum.len() {
                return Err(invalid("origin dimensionality must match map bounds"));
            }
        }
        if let Some(value) = record.get("scale") {
            metric(value, "scale")?
        }
        Ok(Self {
            record,
            canonical,
            crs,
            axes: expected,
            unit,
            policy,
            minimum,
            maximum,
        })
    }
    fn accepts(&self, point: &[f64], label: &str) -> Result<()> {
        if point.len() != self.minimum.len() {
            return Err(invalid(format!(
                "{label} dimensionality must match map bounds"
            )));
        }
        if point
            .iter()
            .zip(self.minimum.iter().zip(&self.maximum))
            .any(|(value, (minimum, maximum))| value < minimum || value > maximum)
        {
            return Err(invalid(format!(
                "{label} must be within its map's inclusive bounds"
            )));
        }
        Ok(())
    }
}
struct Shape<'a> {
    kind: &'a str,
    authored: &'a Yaml,
    points: Vec<Vec<f64>>,
}
fn shape<'a>(value: &'a Yaml) -> Result<Shape<'a>> {
    let fields = closed(value, &["kind", "coordinates"], "spatial.geometry")?;
    let kind = text(&fields["kind"], "spatial.geometry.kind")?;
    let authored = &fields["coordinates"];
    let points = match kind {
        "point" => vec![coordinates(authored, "spatial.geometry.coordinates")?],
        "line" | "polygon" => {
            let Yaml::Sequence(values) = authored else {
                return Err(invalid(
                    "line/polygon coordinates must be an array of vertices",
                ));
            };
            let minimum = if kind == "line" { 2 } else { 4 };
            if !(minimum..=MAX_VERTICES).contains(&values.len()) {
                return Err(invalid(format!(
                    "{kind} requires {minimum}..10000 vertices"
                )));
            }
            let points = values
                .iter()
                .map(|value| coordinates(value, "spatial.geometry.coordinates"))
                .collect::<Result<Vec<_>>>()?;
            if points.iter().any(|point| point.len() != points[0].len()) {
                return Err(invalid("geometry vertices must have one dimensionality"));
            }
            if kind == "polygon" && points.first() != points.last() {
                return Err(invalid(
                    "polygon must repeat its first vertex as the last vertex",
                ));
            }
            points
        }
        _ => return Err(invalid("geometry kind must be point, line or polygon")),
    };
    Ok(Shape {
        kind,
        authored,
        points,
    })
}
/// Closed literal shape grammar shared by the writer; it does not resolve maps.
pub(crate) fn validate_shape(value: &Yaml) -> Result<()> {
    shape(value).map(|_| ())
}
struct Placement<'a> {
    record: &'a Record,
    canonical: bool,
    map_id: &'a str,
    shape: Shape<'a>,
    minimum: Vec<f64>,
    maximum: Vec<f64>,
}
impl<'a> Placement<'a> {
    fn parse(record: &'a Record, maps: &BTreeMap<&str, Map<'_>>) -> Result<Option<Self>> {
        Self::parse_value(record, record.get("spatial"), maps)
    }
    fn parse_value(
        record: &'a Record,
        value: Option<&'a Yaml>,
        maps: &BTreeMap<&str, Map<'_>>,
    ) -> Result<Option<Self>> {
        let canonical = canonical(record)?;
        let Some(value) = value else {
            return Ok(None);
        };
        if matches!(value, Yaml::Null) {
            return Ok(None);
        }
        let fields = closed(value, &["map_id", "geometry"], "spatial")?;
        let map_id = text(&fields["map_id"], "spatial.map_id")?;
        let map = maps.get(map_id).ok_or_else(|| {
            invalid("spatial.map_id must reference an existing valid exact map ID")
        })?;
        if canonical && !map.canonical {
            return Err(invalid("a canonical placement requires a canonical map"));
        }
        let shape = shape(&fields["geometry"])?;
        for point in &shape.points {
            map.accepts(point, "spatial.geometry.coordinates")?
        }
        let mut minimum = shape.points[0].clone();
        let mut maximum = minimum.clone();
        for point in &shape.points {
            for (axis, value) in point.iter().enumerate() {
                minimum[axis] = minimum[axis].min(*value);
                maximum[axis] = maximum[axis].max(*value)
            }
        }
        Ok(Some(Self {
            record,
            canonical,
            map_id,
            shape,
            minimum,
            maximum,
        }))
    }
}
/// A complete historical placement uses the same map and shape codecs as source.
/// This helper does not recurse into history or whole-world validation.
pub(crate) fn validate_placement(
    world: &World,
    value: &Yaml,
    require_canonical: bool,
) -> Result<()> {
    let fields = closed(value, &["map_id", "geometry"], "spatial")?;
    let id = text(&fields["map_id"], "spatial.map_id")?;
    let record = world
        .records
        .get(id)
        .filter(|r| r.kind == "map" && valid_id_for_kind(id, "map"))
        .ok_or_else(|| invalid("spatial.map_id must reference an existing exact map ID"))?;
    let map = Map::parse(record)?;
    if require_canonical && !map.canonical {
        return Err(invalid("a canonical placement requires a canonical map"));
    }
    for point in &shape(&fields["geometry"])?.points {
        map.accepts(point, "spatial.geometry.coordinates")?;
    }
    Ok(())
}
/// Exact map/coordinate endpoint validation, without graph/domain recursion.
pub(crate) fn validate_position(
    world: &World,
    value: &Yaml,
    require_canonical: bool,
) -> Result<()> {
    let fields = closed(value, &["map_id", "coordinates"], "map position")?;
    let id = text(&fields["map_id"], "map position.map_id")?;
    let record = world
        .records
        .get(id)
        .filter(|r| r.kind == "map" && valid_id_for_kind(id, "map"))
        .ok_or_else(|| invalid("map position requires an existing exact map ID"))?;
    let map = Map::parse(record)?;
    if require_canonical && !map.canonical {
        return Err(invalid("a canonical map position requires a canonical map"));
    }
    map.accepts(
        &coordinates(&fields["coordinates"], "map position.coordinates")?,
        "map position.coordinates",
    )
}
struct Anchor<'a> {
    record: &'a Record,
    canonical: bool,
    from: &'a Yaml,
    to: &'a Yaml,
}
impl<'a> Anchor<'a> {
    fn parse(world: &World, record: &'a Record) -> Result<Self> {
        let canonical = canonical(record)?;
        if !valid_id_for_kind(&record.id, "anchor") {
            return Err(invalid("anchor ID is invalid"));
        }
        let from = required(record, "from")?;
        let to = required(record, "to")?;
        validate_position(world, from, canonical)?;
        validate_position(world, to, canonical)?;
        if from.get("map_id") == to.get("map_id") {
            return Err(invalid("an anchor must connect different maps"));
        }
        if record
            .get("conversion")
            .is_some_and(|v| !matches!(v, Yaml::String(_)))
        {
            return Err(invalid("anchor conversion must be literal untagged prose"));
        }
        Ok(Self {
            record,
            canonical,
            from,
            to,
        })
    }
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
struct Geometry<'a> {
    maps: BTreeMap<&'a str, Map<'a>>,
    placements: Vec<Placement<'a>>,
    anchors: Vec<Anchor<'a>>,
}
fn inspect(world: &World) -> (Geometry<'_>, Vec<Diagnostic>) {
    let mut geometry = Geometry {
        maps: BTreeMap::new(),
        placements: Vec::new(),
        anchors: Vec::new(),
    };
    let mut diagnostics = Vec::new();
    let enabled = world
        .world_record()
        .ok()
        .and_then(|r| r.list("capabilities"))
        .is_some_and(|values| {
            ["spatial-core-v1", "geometry-v1"].iter().all(|required| {
                values
                    .iter()
                    .any(|value| matches!(value,Yaml::String(value) if value==required))
            })
        });
    for record in world.records.values().filter(|r| r.kind == "map") {
        if !enabled {
            diagnostics.push(diagnostic(
                record,
                "NAR-CAPABILITY-001",
                "map geometry requires declared spatial-core-v1 and geometry-v1 capabilities"
                    .into(),
            ))
        }
        match Map::parse(record) {
            Ok(map) => {
                geometry.maps.insert(&record.id, map);
            }
            Err(error) => {
                diagnostics.push(diagnostic(record, "WDL-GEOMETRY-001", error.to_string()))
            }
        }
    }
    for record in world.records.values().filter(|r| r.kind == "location") {
        if !enabled
            && record
                .get("spatial")
                .is_some_and(|v| !matches!(v, Yaml::Null))
        {
            diagnostics.push(diagnostic(
                record,
                "NAR-CAPABILITY-001",
                "location geometry requires declared spatial-core-v1 and geometry-v1 capabilities"
                    .into(),
            ))
        }
        match Placement::parse(record, &geometry.maps) {
            Ok(Some(placement)) => geometry.placements.push(placement),
            Ok(None) => (),
            Err(error) => {
                diagnostics.push(diagnostic(record, "WDL-GEOMETRY-002", error.to_string()))
            }
        }
    }
    for record in world.records.values().filter(|r| r.kind == "anchor") {
        if !enabled {
            diagnostics.push(diagnostic(
                record,
                "NAR-CAPABILITY-001",
                "anchors require declared spatial-core-v1 and geometry-v1 capabilities".into(),
            ))
        }
        match Anchor::parse(world, record) {
            Ok(anchor) => geometry.anchors.push(anchor),
            Err(error) => {
                diagnostics.push(diagnostic(record, "WDL-GEOMETRY-003", error.to_string()))
            }
        }
    }
    (geometry, diagnostics)
}
/// Validate maps, literal shapes and map anchors in every source status.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    inspect(world).1
}
fn read(world: &World) -> Result<Geometry<'_>> {
    let (geometry, diagnostics) = inspect(world);
    if let Some(error) = diagnostics.first() {
        return Err(invalid(&error.message));
    }
    Ok(geometry)
}
fn read_at<'a>(world: &'a World, at: &StoryTime) -> Result<Geometry<'a>> {
    crate::spatial_history::check_query(world, at)?;
    let mut geometry = read(world)?;
    geometry.placements.clear();
    // Select the effective placement for every location before map membership,
    // geometry bounds or result limits. A moved-in placement must be discoverable.
    for record in world.records.values().filter(|r| r.kind == "location") {
        let value = crate::spatial_history::effective_value(world, record, "spatial", at)?;
        if let Some(placement) = Placement::parse_value(record, value, &geometry.maps)? {
            geometry.placements.push(placement);
        }
    }
    Ok(geometry)
}
fn limit_argument(limit: usize) -> Result<()> {
    if !(1..=1000).contains(&limit) {
        Err(Error::InvalidInput(
            "limit must be between 1 and 1000".into(),
        ))
    } else {
        Ok(())
    }
}
fn bounded(count: usize, limit: usize) -> Result<()> {
    if count > limit {
        Err(Error::InvalidInput(format!(
            "complete geometry result exceeds limit {limit}"
        )))
    } else {
        Ok(())
    }
}
fn query_point(value: &[f64]) -> Result<()> {
    if ![2, 3].contains(&value.len()) || !value.iter().copied().all(valid_coordinate) {
        Err(Error::InvalidInput("coordinates require two or three finite values with absolute values at most 9007199254740991".into()))
    } else {
        Ok(())
    }
}
fn dimension(point: &[f64], map: &Map<'_>) -> Result<()> {
    if point.len() != map.minimum.len() {
        Err(Error::InvalidInput(
            "query coordinate dimensionality must match the selected map".into(),
        ))
    } else {
        Ok(())
    }
}
fn selected_map<'a, 'b>(
    world: &'a World,
    geometry: &'b Geometry<'a>,
    reference: &str,
) -> Result<&'b Map<'a>> {
    let record = world.resolve(reference)?;
    geometry
        .maps
        .get(record.id.as_str())
        .filter(|map| map.canonical)
        .ok_or_else(|| Error::NotFound("canonical map is unavailable".into()))
}
fn citation(world: &World, record: &Record, field: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,"revision":world.revision,"fingerprint":world.fingerprint,"section":field})
}
fn map_value(world: &World, map: &Map<'_>) -> Result<Json> {
    let mut result = json!({"mapId":map.record.id,"title":map.record.title,"crs":map.crs,"axisOrder":map.axes,"unit":map.unit,"zPolicy":map.policy,"dimensions":map.minimum.len(),"bounds":crate::service::yaml_json(required(map.record,"bounds")?)?,"citation":citation(world,map.record,"map")});
    for field in ["origin", "scale"] {
        if let Some(value) = map.record.get(field) {
            result[field] = crate::service::yaml_json(value)?
        }
    }
    Ok(result)
}
fn placement_value(world: &World, placement: &Placement<'_>) -> Result<Json> {
    Ok(
        json!({"locationId":placement.record.id,"title":placement.record.title,"geometryKind":placement.shape.kind,"coordinates":crate::service::yaml_json(placement.shape.authored)?,"citation":citation(world,placement.record,"spatial")}),
    )
}
fn placement_value_at(
    world: &World,
    placement: &Placement<'_>,
    at: Option<&StoryTime>,
) -> Result<Json> {
    let mut value = placement_value(world, placement)?;
    if let Some(at) = at {
        let citations =
            crate::spatial_history::field_citations(world, placement.record, "spatial", at)?;
        if let Some(selected) = citations.last() {
            value["citation"] = selected.clone();
        }
        value["citations"] = json!(citations);
    }
    Ok(value)
}
fn historical_query(value: &mut Json, at: Option<&StoryTime>) {
    if let Some(at) = at {
        value["scope"] = json!("author-as-of-spatial-history");
        value["basis"] = json!("same-map-effective-geometry");
        value["at"] = json!(at);
    }
}
fn query_value(world: &World, map: &Map<'_>, limit: usize, items: Vec<Json>) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,"basis":"same-map-authored-geometry","mapId":map.record.id,"crs":map.crs,"axisOrder":map.axes,"unit":map.unit,"dimensions":map.minimum.len(),"items":items,"limit":limit,"citation":citation(world,map.record,"map")})
}
/// Canonical authored map declarations in stable ID order.
pub fn maps(world: &World, limit: usize) -> Result<Json> {
    limit_argument(limit)?;
    let geometry = read(world)?;
    let mut items = Vec::new();
    for map in geometry.maps.values().filter(|map| map.canonical) {
        items.push(map_value(world, map)?);
        bounded(items.len(), limit)?
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,"basis":"authored-maps","items":items,"limit":limit}),
    )
}
/// Complete authored geometry-bound intersections in the selected map.
pub fn bbox(
    world: &World,
    reference: &str,
    minimum: impl AsRef<[f64]>,
    maximum: impl AsRef<[f64]>,
    limit: usize,
) -> Result<Json> {
    bbox_relation(
        world,
        reference,
        minimum.as_ref(),
        maximum.as_ref(),
        "intersects",
        limit,
    )
}
/// Bounds compare geometry envelopes, not an exact line/polygon-box intersection.
pub fn bbox_relation(
    world: &World,
    reference: &str,
    minimum: &[f64],
    maximum: &[f64],
    relation: &str,
    limit: usize,
) -> Result<Json> {
    bbox_selected(world, reference, minimum, maximum, relation, None, limit)
}
/// Complete effective geometry at an exact authored horizon, with static fallback.
pub fn bbox_at(
    world: &World,
    reference: &str,
    minimum: &[f64],
    maximum: &[f64],
    relation: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    bbox_selected(
        world,
        reference,
        minimum,
        maximum,
        relation,
        Some(at),
        limit,
    )
}
fn bbox_selected(
    world: &World,
    reference: &str,
    minimum: &[f64],
    maximum: &[f64],
    relation: &str,
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    query_point(minimum)?;
    query_point(maximum)?;
    if minimum.len() != maximum.len() || minimum.iter().zip(maximum).any(|(min, max)| min > max) {
        return Err(Error::InvalidInput(
            "query bounds require equal dimensions and min <= max on every axis".into(),
        ));
    }
    if !["intersects", "within"].contains(&relation) {
        return Err(Error::InvalidInput(
            "bbox relation must be intersects or within".into(),
        ));
    }
    let geometry = match at {
        Some(at) => read_at(world, at)?,
        None => read(world)?,
    };
    let map = selected_map(world, &geometry, reference)?;
    dimension(minimum, map)?;
    let mut items = Vec::new();
    for placement in geometry
        .placements
        .iter()
        .filter(|p| p.canonical && p.map_id == map.record.id)
    {
        let matched = (0..minimum.len()).all(|axis| {
            if relation == "intersects" {
                placement.minimum[axis] <= maximum[axis] && placement.maximum[axis] >= minimum[axis]
            } else {
                placement.minimum[axis] >= minimum[axis] && placement.maximum[axis] <= maximum[axis]
            }
        });
        if matched {
            items.push(placement_value_at(world, placement, at)?);
            bounded(items.len(), limit)?
        }
    }
    let mut result = query_value(world, map, limit, items);
    result["bounds"] = json!({"min":minimum,"max":maximum});
    result["relation"] = json!(relation);
    historical_query(&mut result, at);
    Ok(result)
}
fn norm(vector: &[f64]) -> f64 {
    vector
        .iter()
        .fold(0.0, |length, value| length.hypot(*value))
}
fn distance(point: &[f64], other: &[f64]) -> f64 {
    point
        .iter()
        .zip(other)
        .fold(0.0, |length, (a, b)| length.hypot(a - b))
}
fn segment_distance(point: &[f64], start: &[f64], end: &[f64]) -> f64 {
    let direction: Vec<_> = end.iter().zip(start).map(|(a, b)| a - b).collect();
    let scale = direction.iter().map(|v| v.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return distance(point, start);
    }
    let unit: Vec<_> = direction.iter().map(|v| v / scale).collect();
    let denominator: f64 = unit.iter().map(|v| v * v).sum();
    let progress = (point
        .iter()
        .zip(start)
        .zip(&unit)
        .map(|((p, s), u)| (p - s) * u)
        .sum::<f64>()
        / scale
        / denominator)
        .clamp(0.0, 1.0);
    let closest: Vec<_> = start
        .iter()
        .zip(&direction)
        .map(|(s, d)| s + progress * d)
        .collect();
    distance(point, &closest)
}
fn inside_polygon(point: &[f64], vertices: &[Vec<f64>]) -> bool {
    let mut inside = false;
    for pair in vertices.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if (a[1] > point[1]) != (b[1] > point[1]) {
            let crossing = (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) + a[0];
            if point[0] < crossing {
                inside = !inside
            }
        }
    }
    inside
}
fn scale_tiny_direction(vector: &mut [f64]) {
    let mut largest = vector.iter().map(|value| value.abs()).fold(0.0, f64::max);
    // Power-of-two multiplication retains the authored binary proportions.
    // Do not normalize ordinary vectors before the cross product: division by
    // independently rounded norms can make collinear vertices appear planar.
    while largest > 0.0 && largest < 2.0_f64.powi(-500) {
        for value in vector.iter_mut() {
            *value *= 2.0_f64.powi(512);
        }
        largest *= 2.0_f64.powi(512);
    }
}
fn polygon_distance(point: &[f64], vertices: &[Vec<f64>]) -> f64 {
    let boundary = vertices
        .windows(2)
        .map(|pair| segment_distance(point, &pair[0], &pair[1]))
        .fold(f64::INFINITY, f64::min);
    if point.len() == 2 {
        return if inside_polygon(point, vertices) {
            0.0
        } else {
            boundary
        };
    }
    let origin = &vertices[0];
    // The first nonzero vector and first non-collinear partner define the
    // authored polygon's projection plane, matching the pinned query kernel.
    // Safe source coordinates bound cross products well below f64 overflow.
    // Exact binary scaling handles tiny vectors; the scan remains linear.
    let first = vertices
        .iter()
        .skip(1)
        .map(|v| v.iter().zip(origin).map(|(v, o)| v - o).collect::<Vec<_>>())
        .find(|v| norm(v) > 0.0);
    let Some(mut first) = first else {
        return boundary;
    };
    scale_tiny_direction(&mut first);
    let mut normal = None;
    for vertex in vertices.iter().skip(2) {
        let mut other = vertex
            .iter()
            .zip(origin)
            .map(|(v, o)| v - o)
            .collect::<Vec<_>>();
        if norm(&other) == 0.0 {
            continue;
        }
        scale_tiny_direction(&mut other);
        let mut candidate = vec![
            first[1] * other[2] - first[2] * other[1],
            first[2] * other[0] - first[0] * other[2],
            first[0] * other[1] - first[1] * other[0],
        ];
        let length = norm(&candidate);
        if length != 0.0 {
            for v in &mut candidate {
                *v /= length
            }
            normal = Some(candidate);
            break;
        }
    }
    let Some(normal) = normal else {
        return boundary;
    };
    let offset = point
        .iter()
        .zip(origin)
        .zip(&normal)
        .map(|((p, o), n)| (p - o) * n)
        .sum::<f64>();
    let projected = point
        .iter()
        .zip(&normal)
        .map(|(p, n)| p - offset * n)
        .collect::<Vec<_>>();
    let mut dropped = 0;
    for axis in 1..3 {
        if normal[axis].abs() > normal[dropped].abs() {
            dropped = axis
        }
    }
    let project = |value: &[f64]| {
        value
            .iter()
            .enumerate()
            .filter_map(|(axis, v)| (axis != dropped).then_some(*v))
            .collect::<Vec<_>>()
    };
    if inside_polygon(
        &project(&projected),
        &vertices.iter().map(|v| project(v)).collect::<Vec<_>>(),
    ) {
        offset.abs()
    } else {
        boundary
    }
}
fn geometry_distance(point: &[f64], shape: &Shape<'_>) -> f64 {
    match shape.kind {
        "point" => distance(point, &shape.points[0]),
        "line" => shape
            .points
            .windows(2)
            .map(|pair| segment_distance(point, &pair[0], &pair[1]))
            .fold(f64::INFINITY, f64::min),
        _ => polygon_distance(point, &shape.points),
    }
}
/// Complete same-map Euclidean coordinate-space proximity in authored units.
/// EPSG:4326 uses degree coordinates, without a geodesic/unit conversion.
pub fn nearby(
    world: &World,
    reference: &str,
    point: impl AsRef<[f64]>,
    radius: f64,
    limit: usize,
) -> Result<Json> {
    nearby_selected(world, reference, point.as_ref(), radius, None, limit)
}
/// Same-map distance to the effective authored placement at an exact horizon.
pub fn nearby_at(
    world: &World,
    reference: &str,
    point: &[f64],
    radius: f64,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    nearby_selected(world, reference, point, radius, Some(at), limit)
}
fn nearby_selected(
    world: &World,
    reference: &str,
    point: &[f64],
    radius: f64,
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    query_point(point)?;
    if !radius.is_finite() || radius < 0.0 {
        return Err(Error::InvalidInput(
            "radius must be a finite nonnegative number".into(),
        ));
    }
    let geometry = match at {
        Some(at) => read_at(world, at)?,
        None => read(world)?,
    };
    let map = selected_map(world, &geometry, reference)?;
    dimension(point, map)?;
    let mut items = Vec::new();
    for placement in geometry
        .placements
        .iter()
        .filter(|p| p.canonical && p.map_id == map.record.id)
    {
        let distance = geometry_distance(point, &placement.shape);
        if distance <= radius {
            let mut item = placement_value_at(world, placement, at)?;
            item["distance"] = json!(distance);
            items.push(item);
            bounded(items.len(), limit)?
        }
    }
    let mut result = query_value(world, map, limit, items);
    result["point"] = json!(point);
    result["radius"] = json!(radius);
    historical_query(&mut result, at);
    Ok(result)
}
/// Literal authored point correspondences. These are not executable transforms
/// and never authorize interpolation, inverse conversion or cross-map travel.
pub fn anchors(
    world: &World,
    from_map: Option<&str>,
    to_map: Option<&str>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    let geometry = read(world)?;
    let from_map = from_map
        .map(|id| selected_map(world, &geometry, id).map(|m| m.record.id.as_str()))
        .transpose()?;
    let to_map = to_map
        .map(|id| selected_map(world, &geometry, id).map(|m| m.record.id.as_str()))
        .transpose()?;
    let mut items = Vec::new();
    for anchor in geometry.anchors.iter().filter(|a| {
        a.canonical
            && from_map.is_none_or(|id| a.from.get("map_id").and_then(Yaml::as_str) == Some(id))
            && to_map.is_none_or(|id| a.to.get("map_id").and_then(Yaml::as_str) == Some(id))
    }) {
        let mut item = json!({"anchorId":anchor.record.id,"title":anchor.record.title,"from":crate::service::yaml_json(anchor.from)?,"to":crate::service::yaml_json(anchor.to)?,"citation":citation(world,anchor.record,"anchor")});
        if let Some(value) = anchor.record.get("conversion") {
            item["conversion"] = crate::service::yaml_json(value)?
        }
        items.push(item);
        bounded(items.len(), limit)?;
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,"basis":"explicit-authored-map-anchors","fromMapId":from_map,"toMapId":to_map,"items":items,"limit":limit}),
    )
}
