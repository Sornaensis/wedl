//! Bounded, source-pinned catalogue reads. A lens selects an admitted native
//! projection; it supplies no scope grants, compiled state or character facts.
use crate::{
    Error, Result,
    model::{StoryTime, World},
    retrieval::Mode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{cmp::Ordering, collections::BTreeMap, io::Write};
use unicode_normalization::UnicodeNormalization;

mod projection;
mod search;

pub const PROTOCOL: &str = "wedl-core-catalogue/v1";
pub const MAX_REQUEST_BYTES: usize = 16_384;
pub const MAX_SELECTION: usize = 4096;
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_CURSOR_BYTES: usize = 4096;
const COLLATION: &str = "nfc-casefold-scalar-v1";

fn invalid(code: &str) -> Error {
    Error::InvalidInput(format!("{code}: catalogue request cannot be completed"))
}
fn source_error() -> Error {
    Error::Validation(
        "CAT_SOURCE: catalogue source is unavailable; use author validation for diagnostics".into(),
    )
}
fn unavailable() -> Error {
    Error::NotFound("CAT_SCOPE: catalogue scope is unavailable".into())
}

#[derive(Clone, Debug)]
enum Lens {
    AuthorAllTime(String),
    AuthorAsOf,
    Character { character: String, scene: String },
}

#[derive(Clone, Debug)]
struct Filters {
    kinds: Vec<String>,
    profiles: Vec<String>,
    eras: Vec<String>,
    name: String,
}

#[derive(Clone, Debug)]
struct Page {
    filters: Filters,
    sort: String,
    query: Option<String>,
    mode: Option<Mode>,
    threads: Vec<String>,
    limit: usize,
    cursor: Option<Cursor>,
}

#[derive(Clone, Debug)]
enum Operation {
    Page(Page),
    Labels(Vec<String>),
}

/// Constructed only by the bounded closed decoder. Its admission and cursor
/// bindings cannot be modified through caller-created trusted fields.
#[derive(Clone, Debug)]
pub struct Request {
    raw: Json,
    fingerprint: String,
    at: Option<StoryTime>,
    lens: Lens,
    operation: Operation,
}

impl Request {
    pub fn operation(&self) -> &'static str {
        match self.operation {
            Operation::Page(_) => "page",
            Operation::Labels(_) => "labels",
        }
    }
    fn timeline(&self) -> &str {
        match &self.lens {
            Lens::AuthorAllTime(timeline) => timeline,
            _ => {
                &self
                    .at
                    .as_ref()
                    .expect("dated lens validated by decoder")
                    .timeline
            }
        }
    }
}

fn closed(value: &Json, fields: &[&str], code: &str) -> Result<()> {
    let object = value.as_object().ok_or_else(|| invalid(code))?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(invalid(code));
    }
    Ok(())
}
fn text(value: &Json, empty: bool, code: &str) -> Result<String> {
    value
        .as_str()
        .filter(|value| {
            (empty || !value.trim().is_empty())
                && value.len() <= 1024
                && value.chars().count() <= 256
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid(code))
}
fn array(value: &Json, max: usize, code: &str) -> Result<Vec<String>> {
    let values = value
        .as_array()
        .filter(|values| values.len() <= max)
        .ok_or_else(|| invalid(code))?;
    let result = values
        .iter()
        .map(|value| text(value, false, code))
        .collect::<Result<Vec<_>>>()?;
    let unique = result.iter().collect::<std::collections::BTreeSet<_>>();
    if unique.len() != result.len() {
        return Err(invalid(code));
    }
    Ok(result)
}
fn hash_text(value: &Json, code: &str) -> Result<String> {
    value
        .as_str()
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid(code))
}
fn time(value: &Json) -> Result<StoryTime> {
    closed(value, &["timeline", "tick", "order"], "CAT_TIME")?;
    let timeline = text(&value["timeline"], false, "CAT_TIME")?;
    let decimal = |field: &str| -> Result<i64> {
        let input = value[field].as_str().ok_or_else(|| invalid("CAT_TIME"))?;
        let parsed = input.parse::<i64>().map_err(|_| invalid("CAT_TIME"))?;
        if parsed.to_string() != input {
            return Err(invalid("CAT_TIME"));
        }
        Ok(parsed)
    };
    let tick = decimal("tick")?;
    let order = i32::try_from(decimal("order")?).map_err(|_| invalid("CAT_TIME"))?;
    StoryTime::new(timeline, tick, order).map_err(|_| invalid("CAT_TIME"))
}
fn json_decode(raw: &str, code: &str) -> Result<Json> {
    // The YAML value visitor rejects duplicate JSON object members before an
    // untyped JSON value can silently retain only the last occurrence.
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: serde_yaml::Value = Deserialize::deserialize(&mut decoder).map_err(|_| invalid(code))?;
    decoder.end().map_err(|_| invalid(code))?;
    serde_json::from_str(raw).map_err(|_| invalid(code))
}

/// Validate every member before source access, including exact decimal time,
/// duplicate fields/targets, paired search inputs and cursor structure.
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("CAT_REQUEST"));
    }
    let value = json_decode(raw, "CAT_REQUEST")?;
    if value["protocol"] != PROTOCOL {
        return Err(invalid("CAT_REQUEST"));
    }
    let mut fields = vec!["protocol", "operation", "fingerprint", "at", "lens"];
    match value["operation"].as_str() {
        Some("page") => fields.extend([
            "filters",
            "sort",
            "q",
            "searchMode",
            "threadIds",
            "limit",
            "cursor",
        ]),
        Some("labels") => fields.push("targets"),
        _ => return Err(invalid("CAT_REQUEST")),
    }
    closed(&value, &fields, "CAT_REQUEST")?;
    let fingerprint = hash_text(&value["fingerprint"], "CAT_REVISION")?;
    let input_lens = &value["lens"];
    let (lens, at) = match input_lens["kind"].as_str() {
        Some("author-all-time") => {
            closed(input_lens, &["kind", "timeline"], "CAT_SCOPE")?;
            if !value["at"].is_null() {
                return Err(invalid("CAT_TIME"));
            }
            (
                Lens::AuthorAllTime(text(&input_lens["timeline"], false, "CAT_TIME")?),
                None,
            )
        }
        Some("author-as-of") => {
            closed(input_lens, &["kind"], "CAT_SCOPE")?;
            (Lens::AuthorAsOf, Some(time(&value["at"])?))
        }
        Some("character") => {
            closed(input_lens, &["kind", "characterId", "sceneId"], "CAT_SCOPE")?;
            (
                Lens::Character {
                    character: text(&input_lens["characterId"], false, "CAT_SCOPE")?,
                    scene: text(&input_lens["sceneId"], false, "CAT_SCOPE")?,
                },
                Some(time(&value["at"])?),
            )
        }
        _ => return Err(invalid("CAT_SCOPE")),
    };
    let operation = if value["operation"] == "labels" {
        let targets = value["targets"]
            .as_array()
            .filter(|targets| targets.len() <= 64)
            .ok_or_else(|| invalid("CAT_REQUEST"))?;
        let mut result = Vec::new();
        for target in targets {
            closed(target, &["kind", "id"], "CAT_REQUEST")?;
            if target["kind"] != "record" {
                return Err(invalid("CAT_REQUEST"));
            }
            let id = text(&target["id"], false, "CAT_REQUEST")?;
            if result.contains(&id) {
                return Err(invalid("CAT_REQUEST"));
            }
            result.push(id);
        }
        Operation::Labels(result)
    } else {
        let input = &value["filters"];
        closed(input, &["kinds", "profiles", "eras", "name"], "CAT_REQUEST")?;
        let filters = Filters {
            kinds: array(&input["kinds"], 8, "CAT_REQUEST")?,
            profiles: array(&input["profiles"], 8, "CAT_REQUEST")?,
            eras: array(&input["eras"], 8, "CAT_REQUEST")?,
            name: text(&input["name"], true, "CAT_REQUEST")?,
        };
        let sort = text(&value["sort"], false, "CAT_SORT")?;
        if ![
            "kind-name",
            "name-asc",
            "name-desc",
            "prominence-high",
            "prominence-low",
            "relevance",
        ]
        .contains(&sort.as_str())
            || matches!(lens, Lens::Character { .. }) && sort.starts_with("prominence-")
        {
            return Err(invalid("CAT_SORT"));
        }
        let (query, mode) = if value["q"].is_null() && value["searchMode"].is_null() {
            (None, None)
        } else {
            let query = text(&value["q"], false, "CAT_REQUEST")?;
            if query.split_whitespace().count() > 64 {
                return Err(invalid("CAT_REQUEST"));
            }
            let mode = value["searchMode"]
                .as_str()
                .ok_or_else(|| invalid("CAT_REQUEST"))?
                .parse::<Mode>()
                .map_err(|_| invalid("CAT_REQUEST"))?;
            (Some(query), Some(mode))
        };
        if sort == "relevance" && query.is_none() {
            return Err(invalid("CAT_SORT"));
        }
        let threads = array(&value["threadIds"], 32, "CAT_REQUEST")?;
        if threads.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(invalid("CAT_REQUEST"));
        }
        let limit = value["limit"]
            .as_u64()
            .filter(|value| (1..=100).contains(value))
            .ok_or_else(|| invalid("CAT_REQUEST"))? as usize;
        let cursor = if value["cursor"].is_null() {
            None
        } else {
            Some(Cursor::decode(
                value["cursor"]
                    .as_str()
                    .ok_or_else(|| invalid("CAT_CURSOR"))?,
                &sort,
            )?)
        };
        Operation::Page(Page {
            filters,
            sort,
            query,
            mode,
            threads,
            limit,
            cursor,
        })
    };
    Ok(Request {
        raw: value,
        fingerprint,
        at,
        lens,
        operation,
    })
}

struct Budget {
    bytes: usize,
    max: usize,
    code: &'static str,
}
impl Budget {
    fn new(max: usize, code: &'static str) -> Self {
        Self {
            bytes: 0,
            max,
            code,
        }
    }
    fn charge(&mut self, value: &impl Serialize) -> Result<()> {
        let code = self.code;
        serde_json::to_writer(self, value).map_err(|_| invalid(code))
    }
}
impl Write for Budget {
    fn write(&mut self, value: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(value.len());
        if self.bytes > self.max {
            return Err(std::io::Error::other(self.code));
        }
        Ok(value.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct HashWriter {
    hash: Sha256,
    budget: Budget,
}
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.budget.write(bytes)?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Cursor {
    binding: String,
    after: Vec<Json>,
}
impl Cursor {
    fn decode(raw: &str, sort: &str) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_CURSOR_BYTES {
            return Err(invalid("CAT_CURSOR"));
        }
        let decoded = unbase64(raw)?;
        let string = std::str::from_utf8(&decoded).map_err(|_| invalid("CAT_CURSOR"))?;
        let value = json_decode(string, "CAT_CURSOR")?;
        closed(&value, &["v", "binding", "after"], "CAT_CURSOR")?;
        if value["v"].as_u64() != Some(1) {
            return Err(invalid("CAT_CURSOR"));
        }
        let binding = hash_text(&value["binding"], "CAT_CURSOR")?;
        let after = value["after"]
            .as_array()
            .ok_or_else(|| invalid("CAT_CURSOR"))?
            .clone();
        let strings = |values: &[Json]| values.iter().all(Json::is_string);
        let finite = |value: &Json| value.as_f64().is_some_and(f64::is_finite);
        let valid = match sort {
            "prominence-high" | "prominence-low" => {
                after.len() == 5
                    && after[0].is_boolean()
                    && finite(&after[1])
                    && strings(&after[2..])
            }
            "relevance" => after.len() == 2 && finite(&after[0]) && after[1].is_string(),
            "kind-name" => after.len() == 4 && strings(&after),
            _ => after.len() == 3 && strings(&after),
        };
        if !valid || base64(&serde_json::to_vec(&value).map_err(|_| invalid("CAT_CURSOR"))?) != raw
        {
            return Err(invalid("CAT_CURSOR"));
        }
        Ok(Self { binding, after })
    }
    fn encode(&self) -> Result<String> {
        let raw = serde_json::to_vec(&json!({"v":1,"binding":self.binding,"after":self.after}))?;
        let encoded = base64(&raw);
        if encoded.len() > MAX_CURSOR_BYTES {
            return Err(invalid("CAT_CURSOR"));
        }
        Ok(encoded)
    }
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().saturating_mul(4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        out.push(TABLE[(chunk[0] >> 2) as usize] as char);
        out.push(
            TABLE[(((chunk[0] & 3) << 4) | chunk.get(1).copied().unwrap_or(0) >> 4) as usize]
                as char,
        );
        if chunk.len() > 1 {
            out.push(
                TABLE[(((chunk[1] & 15) << 2) | chunk.get(2).copied().unwrap_or(0) >> 6) as usize]
                    as char,
            );
        }
        if chunk.len() > 2 {
            out.push(TABLE[(chunk[2] & 63) as usize] as char);
        }
    }
    out
}
fn unbase64(value: &str) -> Result<Vec<u8>> {
    let mut values = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        values.push(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(invalid("CAT_CURSOR")),
        });
    }
    if values.len() % 4 == 1 {
        return Err(invalid("CAT_CURSOR"));
    }
    let mut out = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        out.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk.len() > 2 {
            out.push((chunk[1] << 4) | (chunk[2] >> 2));
        }
        if chunk.len() > 3 {
            out.push((chunk[2] << 6) | chunk[3]);
        }
    }
    if base64(&out) != value {
        return Err(invalid("CAT_CURSOR"));
    }
    Ok(out)
}

fn fold(value: &str) -> String {
    crate::reader::casefold::fold(&value.nfc().collect::<String>())
}

/// Closed projection adapter for the Compendium. A caller supplies its native
/// admitted scope and merges timelines before doing any presentation filtering.
pub(crate) fn browser_rows(
    world: &World,
    timeline: &str,
    at: Option<&StoryTime>,
    character: Option<(&str, Option<&str>, &crate::narrative::CharacterScope<'_>)>,
) -> Result<Vec<(String, String, Option<String>)>> {
    let lens = if let Some((viewer, scene, _)) = character {
        Lens::Character {
            character: viewer.into(),
            scene: scene.unwrap_or_default().into(),
        }
    } else if at.is_some() {
        Lens::AuthorAsOf
    } else {
        Lens::AuthorAllTime(timeline.into())
    };
    let request = Request {
        raw: Json::Null,
        fingerprint: world.fingerprint.clone(),
        at: at.cloned(),
        lens,
        operation: Operation::Page(Page {
            filters: Filters {
                kinds: Vec::new(),
                profiles: Vec::new(),
                eras: Vec::new(),
                name: String::new(),
            },
            sort: "name-asc".into(),
            query: None,
            mode: None,
            threads: Vec::new(),
            limit: MAX_SELECTION,
            cursor: None,
        }),
    };
    let selected = projection::select_browser(
        world,
        &request,
        character.map(|(_, _, scope)| scope),
        character.and_then(|(_, scene, _)| scene),
    )?;
    selected
        .rows
        .into_iter()
        .map(|(id, row)| {
            let title = row.item["title"]
                .as_str()
                .ok_or_else(source_error)?
                .to_owned();
            let learned = row
                .learned
                .and_then(|value| value["statement"].as_str().map(str::to_owned));
            Ok((id, title, learned))
        })
        .collect()
}
fn name_key(item: &Json) -> Vec<Json> {
    let title = item["title"]
        .as_str()
        .expect("projected title")
        .nfc()
        .collect::<String>();
    vec![
        json!(crate::reader::casefold::fold(&title)),
        json!(title),
        item["id"].clone(),
    ]
}
fn sort_key(item: &Json, sort: &str, importance: &BTreeMap<String, f64>) -> Vec<Json> {
    let mut name = name_key(item);
    match sort {
        "kind-name" => {
            name.insert(0, item["kind"].clone());
            name
        }
        "prominence-high" | "prominence-low" => {
            let score = importance.get(item["id"].as_str().expect("projected id"));
            let mut key = vec![
                json!(score.is_none()),
                json!(score.map_or(0.0, |score| if sort == "prominence-high" {
                    -*score
                } else {
                    *score
                })),
            ];
            key.extend(name);
            key
        }
        "relevance" => vec![
            json!(
                -item["match"]["score"]
                    .as_f64()
                    .expect("ranked finite score")
            ),
            item["match"]["documentId"].clone(),
        ],
        _ => name,
    }
}
fn compare(left: &[Json], right: &[Json]) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        let order = match (left, right) {
            (Json::String(left), Json::String(right)) => left.cmp(right),
            (Json::Bool(left), Json::Bool(right)) => left.cmp(right),
            (Json::Number(left), Json::Number(right)) => {
                left.as_f64().unwrap().total_cmp(&right.as_f64().unwrap())
            }
            _ => unreachable!("sort keys have a fixed shape"),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

fn facets(world: &World, items: &[Json]) -> Result<Json> {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut profiles: BTreeMap<&str, usize> = BTreeMap::new();
    let mut eras: BTreeMap<&str, usize> = BTreeMap::new();
    for item in items {
        *kinds.entry(item["kind"].as_str().unwrap()).or_default() += 1;
        if let Some(profile) = item["profile"].as_str() {
            *profiles.entry(profile).or_default() += 1;
        }
        for era in item["eraIds"].as_array().unwrap() {
            *eras.entry(era.as_str().unwrap()).or_default() += 1;
        }
    }
    if [kinds.len(), profiles.len(), eras.len()]
        .into_iter()
        .any(|size| size > 64)
    {
        return Err(invalid("CAT_FACET_LIMIT"));
    }
    let labels: BTreeMap<_, _> = world
        .world_record()?
        .get("chronology")
        .and_then(|v| v.get("eras"))
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|era| {
            Some((
                era.get("id")?.as_str()?,
                era.get("label").and_then(serde_yaml::Value::as_str),
            ))
        })
        .collect();
    Ok(json!({
        "kinds":kinds.into_iter().map(|(value,count)| json!({"value":value,"count":count})).collect::<Vec<_>>(),
        "profiles":profiles.into_iter().map(|(id,count)| json!({"id":id,"count":count})).collect::<Vec<_>>(),
        "eras":eras.into_iter().map(|(id,count)| json!({"id":id,"label":labels.get(id).copied().flatten().unwrap_or(id),"count":count})).collect::<Vec<_>>()
    }))
}

/// Execute against one captured source world. Caller adapters own source
/// capture/rechecks; this function checks the exact pin and captured validity.
pub fn read(world: &World, request: &Request) -> Result<Json> {
    if world.fingerprint != request.fingerprint {
        return Err(Error::Conflict(
            "CAT_REVISION: catalogue source fingerprint changed".into(),
        ));
    }
    if !crate::validation::validate_world(world).valid {
        return Err(source_error());
    }
    world
        .story_time(0, Some(request.timeline()), 0)
        .map_err(|_| invalid("CAT_TIME"))?;
    let character_scope = match &request.lens {
        Lens::Character { character, scene } => Some(
            crate::narrative::CharacterScope::with_scene(
                world,
                character,
                request.at.as_ref().unwrap(),
                Some(scene),
            )
            .map_err(|_| unavailable())?,
        ),
        _ => None,
    };
    let selected = projection::select(world, request, character_scope.as_ref())?;
    let mut context = json!({"at":request.raw["at"],"lens":request.raw["lens"]});
    let mut result = json!({"protocol":PROTOCOL,"operation":request.operation(),"fingerprint":world.fingerprint,"revision":world.revision,"context":null,
        "outcome":"available","code":null,"coverage":null,"items":[],"facets":null,"total":null,"nextCursor":null,"searchState":null});
    match &request.operation {
        Operation::Labels(targets) => {
            result["items"] = json!(targets.iter().map(|id| {
                let row = selected.rows.get(id);
                json!({"target":{"kind":"record","id":id},"status":if row.is_some(){"available"}else{"unavailable"},"kind":row.map(|row|&row.item["kind"]),"title":row.map(|row|&row.item["title"])})
            }).collect::<Vec<_>>());
            result["total"] = json!(targets.len());
            result["coverage"] = json!("requested-targets");
        }
        Operation::Page(page) => {
            for key in ["filters", "sort", "q", "searchMode", "threadIds", "limit"] {
                context[key] = request.raw[key].clone();
            }
            context["collation"] = json!(COLLATION);
            context["prominenceAt"] = Json::Null;
            context["prominenceOrigin"] = Json::Null;
            let mut items;
            let search_state;
            if page.query.is_some() {
                let found =
                    search::select(world, request, page, &selected, character_scope.as_ref())?;
                items = found.0;
                search_state = found.1;
            } else {
                items = selected.rows.values().map(|row| row.item.clone()).collect();
                search_state = Json::Null;
            }
            // Grouping cannot alter admitted corpus statistics or ranking.
            if !page.threads.is_empty() {
                items.retain(|item| !item["threadIds"].as_array().unwrap().is_empty());
            }
            let importance = if page.sort.starts_with("prominence-") {
                let at = request
                    .at
                    .clone()
                    .map(Ok)
                    .unwrap_or_else(|| world.current_time())
                    .map_err(|_| invalid("CAT_TIME"))?;
                context["prominenceAt"] = json!(at);
                context["prominenceOrigin"] = json!(if request.at.is_some() {
                    "explicit-horizon"
                } else {
                    "shared-current"
                });
                crate::navigation::whereabouts::prominence(world, &at)
                    .map_err(|_| invalid("CAT_SELECTION_LIMIT"))?
            } else {
                BTreeMap::new()
            };
            items.sort_by(|left, right| {
                let order = compare(
                    &sort_key(left, &page.sort, &importance),
                    &sort_key(right, &page.sort, &importance),
                );
                if page.sort == "name-desc" {
                    order.reverse()
                } else {
                    order
                }
            });
            let facet_value = facets(world, &items)?;
            let mut proof_request = request.raw.clone();
            proof_request.as_object_mut().unwrap().remove("cursor");
            // Native source pin and complete admitted result bind selection,
            // time, request-local model, facets and stable continuation order.
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Proof<'a> {
                request: &'a Json,
                context: &'a Json,
                items: &'a [Json],
                facets: &'a Json,
                search_state: &'a Json,
                known: &'a [String],
            }
            let proof = Proof {
                request: &proof_request,
                context: &context,
                items: &items,
                facets: &facet_value,
                search_state: &search_state,
                known: &selected.known,
            };
            let mut digest = HashWriter {
                hash: Sha256::new(),
                budget: Budget::new(8 * 1024 * 1024, "CAT_TEXT_LIMIT"),
            };
            serde_json::to_writer(&mut digest, &proof).map_err(|_| invalid("CAT_TEXT_LIMIT"))?;
            let binding = hex::encode(digest.hash.finalize());
            let start = if let Some(cursor) = &page.cursor {
                if cursor.binding != binding {
                    return Err(invalid("CAT_CURSOR"));
                }
                let matching: Vec<_> = items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| sort_key(item, &page.sort, &importance) == cursor.after)
                    .collect();
                if matching.len() != 1 {
                    return Err(invalid("CAT_CURSOR"));
                }
                matching[0].0 + 1
            } else {
                0
            };
            let end = start.saturating_add(page.limit).min(items.len());
            let next_cursor = if end < items.len() {
                Some(
                    Cursor {
                        binding,
                        after: sort_key(&items[end - 1], &page.sort, &importance),
                    }
                    .encode()?,
                )
            } else {
                None
            };
            result["total"] = json!(items.len());
            result["items"] = json!(&items[start..end]);
            result["facets"] = facet_value;
            result["nextCursor"] = json!(next_cursor);
            result["searchState"] = search_state;
            result["coverage"] = json!(if page.query.is_some() {
                "complete-admitted-search"
            } else {
                "selection"
            });
        }
    }
    result["context"] = context;
    Budget::new(MAX_RESPONSE_BYTES, "CAT_RESPONSE_LIMIT").charge(&result)?;
    Ok(result)
}
