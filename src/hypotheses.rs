//! Explicit author possibilities, outside canonical time, state and retrieval.
//!
//! Reads and plans consume one captured source snapshot. Adoption links only
//! pre-existing settled records; it never creates evidence, effects or learning.
//! The ordinary consequence/authoring pipeline remains the only writing path.
use crate::{
    Error, Result, consequences,
    model::{Record, World},
    reader::casefold::fold,
    validation::{self, Diagnostic},
};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::{collections::BTreeSet, io::Write};

mod word;
use word::word;

pub const MAX_REQUEST_BYTES: usize = consequences::MAX_PLAN_BYTES;
pub const MAX_ITEMS: usize = 1000;
pub const MAX_VALUES: usize = 1000;
pub const MAX_TEXT_BYTES: usize = 65_536;
pub const MAX_TITLE_BYTES: usize = 1024;
pub const MAX_QUERY_BYTES: usize = 4096;
pub const MIN_CHARACTERS: usize = consequences::MIN_CHARACTERS;
pub const MAX_CHARACTERS: usize = consequences::MAX_CHARACTERS;
pub const MAX_SEARCH_RECORDS: usize = 1024;
pub const MAX_SEARCH_BYTES: usize = 524_288;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub expected_fingerprint: String,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    Create {
        id: String,
        title: String,
        statement: String,
        subjects: Vec<String>,
        alternatives: Vec<String>,
        context: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        placement: Option<Placement>,
    },
    Adopt {
        hypothesis_id: String,
        canonical_records: Vec<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present"
        )]
        note: Option<String>,
    },
    Reject {
        hypothesis_id: String,
        note: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Placement {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub timeline: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub scene_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub event_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub location_id: Option<String>,
}

fn present<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn nonblank(value: &Yaml) -> bool {
    value.as_str().is_some_and(|value| !value.trim().is_empty())
}

fn exact_ids(value: &Yaml, field: &str) -> Result<()> {
    let values = value
        .as_sequence()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| {
            invalid(format!(
                "hypothesis {field} requires a nonempty array of exact record IDs"
            ))
        })?;
    let mut seen = BTreeSet::new();
    for value in values {
        let id = value
            .as_str()
            .filter(|id| validation::valid_record_id(id))
            .ok_or_else(|| invalid(format!("hypothesis {field} requires exact record IDs")))?;
        if !seen.insert(id) {
            return Err(invalid(format!(
                "hypothesis {field} must not contain duplicates"
            )));
        }
    }
    Ok(())
}

/// Syntactic field checks used by the existing typed record codec. References
/// are not resolved here: ordinary complete-candidate forward references work.
/// Raw-source null placement links and resolution:null retain their meaning.
pub fn validate_request_field(field: &str, value: &Yaml) -> Result<()> {
    match field {
        "status" => {
            if !matches!(value.as_str(), Some("open" | "adopted" | "rejected")) {
                return Err(invalid(
                    "hypothesis status must be open, adopted, or rejected",
                ));
            }
        }
        "statement" | "context" => {
            if !nonblank(value) {
                return Err(invalid(format!("hypothesis {field} must be nonempty text")));
            }
        }
        "subjects" => exact_ids(value, field)?,
        "alternatives" => {
            if value
                .as_sequence()
                .is_none_or(|values| values.is_empty() || !values.iter().all(nonblank))
            {
                return Err(invalid(
                    "hypothesis alternatives require a nonempty array of nonempty strings",
                ));
            }
        }
        "placement" => {
            let values = value
                .as_mapping()
                .filter(|values| !values.is_empty())
                .ok_or_else(|| invalid("hypothesis placement requires a nonempty mapping"))?;
            for (key, value) in values {
                match key.as_str() {
                    Some("context" | "timeline") if nonblank(value) => (),
                    Some("scene" | "event" | "location") => {
                        let kind = key.as_str().expect("matched key");
                        if !value.is_null()
                            && value
                                .as_str()
                                .is_none_or(|id| !validation::valid_id_for_kind(id, kind))
                        {
                            return Err(invalid(format!(
                                "hypothesis placement.{kind} requires an exact {kind} ID or null"
                            )));
                        }
                    }
                    Some("context" | "timeline") => {
                        return Err(invalid(
                            "hypothesis placement context and timeline must be nonempty text",
                        ));
                    }
                    _ => {
                        return Err(invalid(
                            "hypothesis placement permits only timeline, scene, event, location, and context",
                        ));
                    }
                }
            }
        }
        "resolution" => {
            if value.is_null() {
                return Ok(());
            }
            let values = value
                .as_mapping()
                .ok_or_else(|| invalid("hypothesis resolution requires a mapping or null"))?;
            for (key, value) in values {
                match key.as_str() {
                    Some("canonical_entities") => {
                        exact_ids(value, "resolution.canonical_entities")?
                    }
                    Some("note") if nonblank(value) => (),
                    Some("note") => {
                        return Err(invalid("hypothesis resolution.note must be nonempty text"));
                    }
                    _ => {
                        return Err(invalid(
                            "hypothesis resolution permits only canonical_entities and note",
                        ));
                    }
                }
            }
        }
        _ => return Err(invalid("unsupported hypothesis field")),
    }
    Ok(())
}

/// The pinned settled-record predicate, including scene/conversation statuses.
pub fn is_adoptable_canonical_record(record: &Record) -> bool {
    match record.kind.as_str() {
        "hypothesis" => false,
        "scene" | "conversation" => matches!(record.status(), "active" | "closed" | "retired"),
        _ => matches!(record.status(), "canonical" | "retired"),
    }
}

fn declared_timeline(world: &World, timeline: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|owner| owner.list("timelines"))
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value.get("id").and_then(Yaml::as_str) == Some(timeline))
        })
}

fn diagnostic(record: &Record, code: &str, field: &str, message: &str) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message.into(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some(field.into()),
    }
}

/// Validate nonoperative source grammar without invoking clocks, state or
/// causality. Native canonical concept links remain unavailable to hypotheses.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    const ALLOWED: &[&str] = &[
        "schema",
        "kind",
        "id",
        "title",
        "domain",
        "status",
        "tags",
        "aliases",
        "section_audiences",
        "provenance",
        "statement",
        "subjects",
        "alternatives",
        "context",
        "placement",
        "resolution",
    ];
    let mut out = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "hypothesis")
    {
        let mut emit = |code: &str, field: &str, message: &str| {
            out.push(diagnostic(record, code, field, message))
        };
        if let Some(fields) = record.frontmatter.as_mapping() {
            let keys: BTreeSet<_> = fields.keys().filter_map(Yaml::as_str).collect();
            for key in keys {
                if !ALLOWED.contains(&key) {
                    emit(
                        "WDL-HYP-010",
                        key,
                        "hypotheses permit only their nonoperative frontmatter fields",
                    );
                }
            }
        }
        // The shared native validator historically checked only supplied
        // statuses; hypotheses require their explicit noncanonical status.
        if record.get("status").is_none() {
            emit(
                "WDL-STATUS-001",
                "status",
                "hypothesis requires open, adopted, or rejected status",
            );
        }
        for field in ["statement", "context"] {
            if record.get(field).is_none_or(|value| !nonblank(value)) {
                emit(
                    "WDL-HYP-001",
                    field,
                    "hypothesis requires nonempty statement and context text",
                );
            }
        }
        match record.list("subjects") {
            Some(values)
                if !values.is_empty()
                    && values.iter().all(|value| {
                        value
                            .as_str()
                            .is_some_and(|id| world.records.contains_key(id))
                    }) =>
            {
                let unique: BTreeSet<_> = values.iter().filter_map(Yaml::as_str).collect();
                if unique.len() != values.len() {
                    emit(
                        "WDL-HYP-003",
                        "subjects",
                        "hypothesis subjects must not contain duplicates",
                    );
                }
            }
            _ => emit(
                "WDL-HYP-002",
                "subjects",
                "hypothesis subjects require a nonempty array of existing record references",
            ),
        }
        if record
            .get("alternatives")
            .is_none_or(|value| validate_request_field("alternatives", value).is_err())
        {
            emit(
                "WDL-HYP-004",
                "alternatives",
                "hypothesis alternatives require a nonempty array of nonempty strings",
            );
        }
        match record.get("placement").and_then(Yaml::as_mapping) {
            Some(values) if !values.is_empty() => {
                if !values.keys().any(|key| {
                    matches!(
                        key.as_str(),
                        Some("timeline" | "scene" | "event" | "location" | "context")
                    )
                }) {
                    emit(
                        "WDL-HYP-005",
                        "placement",
                        "hypothesis requires placement context",
                    );
                }
                for (key, value) in values {
                    match key.as_str() {
                        Some("context") => {
                            if !nonblank(value) {
                                emit(
                                    "WDL-HYP-009",
                                    "placement.context",
                                    "hypothesis placement context must be nonempty text",
                                );
                            }
                        }
                        Some("timeline") => {
                            if value.as_str().is_none_or(|timeline| {
                                timeline.trim().is_empty() || !declared_timeline(world, timeline)
                            }) {
                                emit(
                                    "WDL-HYP-007",
                                    "placement.timeline",
                                    "hypothesis placement timeline must be a declared timeline name",
                                );
                            }
                        }
                        Some(kind @ ("scene" | "event" | "location")) => {
                            if !value.is_null()
                                && value
                                    .as_str()
                                    .and_then(|id| world.records.get(id))
                                    .is_none_or(|target| target.kind != kind)
                            {
                                emit(
                                    "WDL-HYP-008",
                                    &format!("placement.{kind}"),
                                    "hypothesis placement link must reference its declared record kind",
                                );
                            }
                        }
                        _ => emit(
                            "WDL-HYP-006",
                            &format!("placement.{}", key.as_str().unwrap_or("<nonstring>")),
                            "hypothesis placement permits only timeline, scene, event, location, and context",
                        ),
                    }
                }
            }
            _ => emit(
                "WDL-HYP-005",
                "placement",
                "hypothesis requires placement context",
            ),
        }
        let resolution = record.get("resolution");
        if record.status() == "open" {
            if resolution.is_some_and(|value| !value.is_null()) {
                emit(
                    "WDL-HYP-011",
                    "resolution",
                    "an open hypothesis must not have a resolution",
                );
            }
            continue;
        }
        let Some(resolution) = resolution.and_then(Yaml::as_mapping) else {
            emit(
                "WDL-HYP-011",
                "resolution",
                "a resolved hypothesis requires a resolution mapping",
            );
            continue;
        };
        if !matches!(record.status(), "adopted" | "rejected") {
            continue;
        }
        let adopted = record.status() == "adopted";
        for key in resolution.keys() {
            if key.as_str() != Some("note")
                && !(adopted && key.as_str() == Some("canonical_entities"))
            {
                emit(
                    "WDL-HYP-014",
                    &format!("resolution.{}", key.as_str().unwrap_or("<nonstring>")),
                    "resolution contains a field unavailable for this hypothesis status",
                );
            }
        }
        let note = resolution.get("note");
        if (!adopted && note.is_none()) || note.is_some_and(|value| !nonblank(value)) {
            emit(
                "WDL-HYP-013",
                "resolution.note",
                "a rejection requires a nonempty note; an adoption note must be nonempty when supplied",
            );
        }
        if adopted {
            match resolution
                .get("canonical_entities")
                .and_then(Yaml::as_sequence)
            {
                Some(values) if !values.is_empty() => {
                    for (index, value) in values.iter().enumerate() {
                        if value
                            .as_str()
                            .and_then(|id| world.records.get(id))
                            .is_none_or(|target| !is_adoptable_canonical_record(target))
                        {
                            emit(
                                "WDL-HYP-012",
                                &format!("resolution.canonical_entities[{index}]"),
                                "adoption references must be existing settled canonical records",
                            );
                        }
                    }
                    let unique: BTreeSet<_> = values.iter().filter_map(Yaml::as_str).collect();
                    if unique.len() != values.len() {
                        emit(
                            "WDL-HYP-012",
                            "resolution.canonical_entities",
                            "adoption references must not contain duplicates",
                        );
                    }
                }
                _ => emit(
                    "WDL-HYP-011",
                    "resolution.canonical_entities",
                    "an adopted hypothesis requires one or more canonical entities",
                ),
            }
        }
    }
    out
}

fn text_bound(value: &str, maximum: usize, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(invalid(format!(
            "hypothesis {field} must be nonempty text within {maximum} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn id_bound(id: &str, kind: Option<&str>, field: &str) -> Result<()> {
    let valid = kind.map_or_else(
        || validation::valid_record_id(id),
        |kind| validation::valid_id_for_kind(id, kind),
    );
    if !valid {
        return Err(invalid(format!(
            "hypothesis {field} requires an exact record ID"
        )));
    }
    Ok(())
}

fn ids_bound(values: &[String], field: &str) -> Result<()> {
    if values.is_empty() || values.len() > MAX_VALUES {
        return Err(invalid(format!(
            "hypothesis {field} requires 1..1000 exact record IDs"
        )));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        id_bound(value, None, field)?;
        if !seen.insert(value) {
            return Err(invalid(format!(
                "hypothesis {field} must not contain duplicates"
            )));
        }
    }
    Ok(())
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    max_characters: Option<usize>,
    characters: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "complete hypothesis JSON bound exceeded",
            ));
        }
        // Every JSON UTF-8 scalar has exactly one non-continuation byte. This
        // remains exact even if a writer call splits a multibyte scalar.
        let characters = bytes.iter().filter(|byte| **byte & 0xc0 != 0x80).count();
        if self
            .max_characters
            .is_some_and(|maximum| characters > maximum.saturating_sub(self.characters))
        {
            return Err(std::io::Error::other(
                "complete hypothesis JSON character bound exceeded",
            ));
        }
        self.characters += characters;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encode<T: Serialize>(value: &T, maximum: usize) -> Result<Vec<u8>> {
    encode_bounded(value, maximum, None)
}

fn encode_bounded<T: Serialize>(
    value: &T,
    maximum: usize,
    max_characters: Option<usize>,
) -> Result<Vec<u8>> {
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum,
        max_characters,
        characters: 0,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        invalid(match max_characters {
            Some(characters) => format!("complete hypothesis JSON exceeds {maximum} encoded bytes or {characters} characters"),
            None => format!("complete hypothesis JSON exceeds {maximum} encoded bytes"),
        })
    })?;
    Ok(writer.bytes)
}

fn validate_request(request: &Request) -> Result<()> {
    if request.expected_fingerprint.len() != 64
        || !request
            .expected_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "hypothesis request requires an exact lowercase source fingerprint",
        ));
    }
    encode(request, MAX_REQUEST_BYTES)?;
    match &request.action {
        Action::Create {
            id,
            title,
            statement,
            subjects,
            alternatives,
            context,
            placement,
        } => {
            id_bound(id, Some("hypothesis"), "id")?;
            text_bound(title, MAX_TITLE_BYTES, "title")?;
            text_bound(statement, MAX_TEXT_BYTES, "statement")?;
            text_bound(context, MAX_TEXT_BYTES, "context")?;
            ids_bound(subjects, "subjects")?;
            if alternatives.is_empty() || alternatives.len() > MAX_VALUES {
                return Err(invalid("hypothesis alternatives requires 1..1000 strings"));
            }
            for alternative in alternatives {
                text_bound(alternative, MAX_TEXT_BYTES, "alternative")?;
            }
            if let Some(placement) = placement {
                if let Some(timeline) = &placement.timeline {
                    text_bound(timeline, 128, "placement.timeline")?;
                }
                for (id, kind) in [
                    (&placement.scene_id, "scene"),
                    (&placement.event_id, "event"),
                    (&placement.location_id, "location"),
                ] {
                    if let Some(id) = id {
                        id_bound(id, Some(kind), "placement link")?;
                    }
                }
            }
        }
        Action::Adopt {
            hypothesis_id,
            canonical_records,
            note,
        } => {
            id_bound(hypothesis_id, Some("hypothesis"), "hypothesisId")?;
            ids_bound(canonical_records, "canonicalRecords")?;
            if let Some(note) = note {
                text_bound(note, MAX_TEXT_BYTES, "note")?;
            }
        }
        Action::Reject {
            hypothesis_id,
            note,
        } => {
            id_bound(hypothesis_id, Some("hypothesis"), "hypothesisId")?;
            text_bound(note, MAX_TEXT_BYTES, "note")?;
        }
    }
    Ok(())
}

/// Closed, duplicate-safe JSON. Missing optional fields are distinct from null;
/// no request-owned perspective, horizon, grant, effects or candidate is accepted.
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("hypothesis request exceeds 262144 encoded bytes"));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate hypothesis JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid hypothesis JSON"))?;
    let request: Request =
        serde_json::from_str(raw).map_err(|_| invalid("invalid hypothesis request fields"))?;
    validate_request(&request)?;
    Ok(request)
}

fn gate(world: &World) -> Result<()> {
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "base snapshot failed validation; run validate for diagnostics".into(),
        ));
    }
    Ok(())
}

fn existing<'a>(world: &'a World, id: &str, kind: Option<&str>) -> Result<&'a Record> {
    world
        .records
        .get(id)
        .filter(|record| kind.is_none_or(|kind| record.kind == kind))
        .ok_or_else(|| invalid("hypothesis reference is unavailable at the captured source"))
}

/// Build one ordinary record operation. No terminal transition is invented:
/// adopted/rejected notes may be resolved again by an explicit author request.
pub fn plan(world: &World, request: &Request) -> Result<consequences::Plan> {
    validate_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the hypothesis request snapshot".into(),
        ));
    }
    gate(world)?;
    let operation = match &request.action {
        Action::Create {
            id,
            title,
            statement,
            subjects,
            alternatives,
            context,
            placement,
        } => {
            if world.records.contains_key(id) {
                return Err(Error::Conflict("hypothesis ID already exists".into()));
            }
            for id in subjects {
                existing(world, id, None)?;
            }
            let mut source_placement =
                Mapping::from_iter([("context".into(), context.clone().into())]);
            if let Some(placement) = placement {
                if let Some(timeline) = &placement.timeline {
                    if !declared_timeline(world, timeline) {
                        return Err(invalid("hypothesis placement timeline must be declared"));
                    }
                    source_placement.insert("timeline".into(), timeline.clone().into());
                }
                for (id, kind) in [
                    (&placement.scene_id, "scene"),
                    (&placement.event_id, "event"),
                    (&placement.location_id, "location"),
                ] {
                    if let Some(id) = id {
                        existing(world, id, Some(kind))?;
                        source_placement.insert(kind.into(), id.clone().into());
                    }
                }
            }
            let fields = json!({"kind":"hypothesis","id":id,"title":title,"status":"open","domain":"author.possibilities",
                "statement":statement,"subjects":subjects,"alternatives":alternatives,"context":context,
                "placement":source_placement,"body":format!("# {title}\n\n{statement}\n")});
            consequences::Operation::Create {
                draft: serde_yaml::to_string(&fields)?,
            }
        }
        Action::Adopt {
            hypothesis_id,
            canonical_records,
            note,
        } => {
            existing(world, hypothesis_id, Some("hypothesis"))?;
            for id in canonical_records {
                if !is_adoptable_canonical_record(existing(world, id, None)?) {
                    return Err(invalid(
                        "hypothesis adoption requires already settled canonical records",
                    ));
                }
            }
            let mut resolution = json!({"canonical_entities":canonical_records});
            if let Some(note) = note {
                resolution["note"] = json!(note);
            }
            consequences::Operation::Update {
                entity_id: hypothesis_id.clone(),
                patch: serde_yaml::to_string(
                    &json!({"kind":"hypothesis","status":"adopted","resolution":resolution}),
                )?,
            }
        }
        Action::Reject {
            hypothesis_id,
            note,
        } => {
            existing(world, hypothesis_id, Some("hypothesis"))?;
            consequences::Operation::Update {
                entity_id: hypothesis_id.clone(),
                patch: serde_yaml::to_string(
                    &json!({"kind":"hypothesis","status":"rejected","resolution":{"note":note}}),
                )?,
            }
        }
    };
    let plan = consequences::Plan {
        expected_fingerprint: request.expected_fingerprint.clone(),
        operations: vec![operation],
        analysis: None,
    };
    consequences::validate_plan(&plan)?;
    Ok(plan)
}

fn budget(max_characters: usize) -> Result<()> {
    if !(MIN_CHARACTERS..=MAX_CHARACTERS).contains(&max_characters) {
        return Err(invalid("hypothesis maxCharacters must be 256..1000000"));
    }
    Ok(())
}

fn bounded_result(result: &Json, max_characters: usize) -> Result<()> {
    encode_bounded(
        result,
        consequences::MAX_PREVIEW_BYTES,
        Some(max_characters),
    )?;
    Ok(())
}

fn envelope(world: &World, operation: &str, scope: &str, max_characters: usize) -> Json {
    json!({"protocol":crate::CONTRACT,"operation":operation,"perspective":"author","scope":scope,
        "fingerprint":world.fingerprint,"revision":world.revision,"nonCanonical":true,
        "maxCharacters":max_characters,"complete":true,"validationComplete":false,"writesPerformed":false,"canApply":false})
}

/// Exact Plan and full candidate validation/diff, with no new writer or grant.
pub fn preview(world: &World, request: &Request, max_characters: usize) -> Result<Json> {
    budget(max_characters)?;
    let plan = plan(world, request)?;
    let report = consequences::preview(world, &plan, max_characters)?;
    let mut result = envelope(
        world,
        "hypothesis-preview",
        "local-author-hypotheses",
        max_characters,
    );
    result["request"] = json!(request);
    result["plan"] = json!(plan);
    result["preview"] = report;
    bounded_result(&result, max_characters)?;
    Ok(result)
}

// Borrow source strings through serialization. Referenced titles can be much
// larger than their IDs; expanding them into an unbounded Json tree first
// would defeat the complete output budget.
struct Reference<'a>(&'a Record);
impl Serialize for Reference<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("id", &self.0.id)?;
        map.serialize_entry("kind", &self.0.kind)?;
        map.serialize_entry("title", &self.0.title)?;
        map.end()
    }
}
fn reference<'a>(world: &'a World, value: &Yaml) -> Option<Reference<'a>> {
    value
        .as_str()
        .and_then(|id| world.records.get(id))
        .map(Reference)
}
struct References<'a> {
    world: &'a World,
    values: &'a [Yaml],
}
impl Serialize for References<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.values.len()))?;
        for value in self.values {
            let target = reference(self.world, value).ok_or_else(|| {
                <S::Error as serde::ser::Error>::custom("hypothesis reference unavailable")
            })?;
            sequence.serialize_element(&target)?;
        }
        sequence.end()
    }
}
struct PlacementProjection<'a> {
    world: &'a World,
    record: &'a Record,
}
impl Serialize for PlacementProjection<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(fields) = self.record.get("placement").and_then(Yaml::as_mapping) {
            for field in ["timeline", "context"] {
                if let Some(value) = fields.get(field).and_then(Yaml::as_str) {
                    map.serialize_entry(field, value)?;
                }
            }
            for field in ["scene", "event", "location"] {
                if let Some(target) = fields
                    .get(field)
                    .and_then(|value| reference(self.world, value))
                {
                    map.serialize_entry(field, &target)?;
                }
            }
        }
        map.end()
    }
}
struct ResolutionProjection<'a> {
    world: &'a World,
    record: &'a Record,
}
impl Serialize for ResolutionProjection<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let fields = self
            .record
            .get("resolution")
            .and_then(Yaml::as_mapping)
            .expect("validated resolution");
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("note", &fields.get("note").and_then(Yaml::as_str))?;
        if self.record.status() == "adopted" {
            let values = fields
                .get("canonical_entities")
                .and_then(Yaml::as_sequence)
                .expect("validated resolution links");
            map.serialize_entry(
                "canonicalRecords",
                &References {
                    world: self.world,
                    values,
                },
            )?;
        }
        map.end()
    }
}
struct Citation<'a> {
    world: &'a World,
    record: &'a Record,
}
impl Serialize for Citation<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(6))?;
        map.serialize_entry("entityId", &self.record.id)?;
        map.serialize_entry("sourcePath", &self.record.source_path)?;
        map.serialize_entry("blobOid", &self.record.blob_oid)?;
        map.serialize_entry("revision", &self.world.revision)?;
        map.serialize_entry("fingerprint", &self.world.fingerprint)?;
        map.serialize_entry("section", "hypothesis")?;
        map.end()
    }
}
struct Projection<'a> {
    world: &'a World,
    record: &'a Record,
}
impl Serialize for Projection<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let Self { world, record } = self;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &record.id)?;
        map.serialize_entry("kind", "hypothesis")?;
        map.serialize_entry("title", &record.title)?;
        map.serialize_entry("status", record.status())?;
        map.serialize_entry("nonCanonical", &true)?;
        map.serialize_entry("statement", &record.string("statement"))?;
        map.serialize_entry(
            "subjects",
            &References {
                world,
                values: record.list("subjects").expect("validated subjects"),
            },
        )?;
        map.serialize_entry(
            "alternatives",
            &record.list("alternatives").expect("validated alternatives"),
        )?;
        map.serialize_entry("context", &record.string("context"))?;
        map.serialize_entry("placement", &PlacementProjection { world, record })?;
        map.serialize_entry("citation", &Citation { world, record })?;
        if record.get("resolution").is_some_and(Yaml::is_mapping) {
            map.serialize_entry("resolution", &ResolutionProjection { world, record })?;
        }
        map.end()
    }
}
fn projection(
    world: &World,
    record: &Record,
    remaining_bytes: usize,
    remaining_characters: usize,
) -> Result<(Json, usize, usize)> {
    let encoded = encode_bounded(
        &Projection { world, record },
        remaining_bytes,
        Some(remaining_characters),
    )?;
    let characters = encoded.iter().filter(|byte| **byte & 0xc0 != 0x80).count();
    Ok((serde_json::from_slice(&encoded)?, encoded.len(), characters))
}

/// Literal casefolded title/statement/body filter, independent of current time.
/// No successful read silently truncates a projection or matching collection.
pub fn list(
    world: &World,
    status: Option<&str>,
    text: Option<&str>,
    max_items: usize,
    max_characters: usize,
) -> Result<Json> {
    budget(max_characters)?;
    if !(1..=MAX_ITEMS).contains(&max_items) {
        return Err(invalid("hypothesis maxItems must be 1..1000"));
    }
    if status.is_some_and(|status| !matches!(status, "open" | "adopted" | "rejected")) {
        return Err(invalid(
            "hypothesis status must be open, adopted, or rejected",
        ));
    }
    if text.is_some_and(|text| text.len() > MAX_QUERY_BYTES) {
        return Err(invalid("hypothesis text filter exceeds 4096 UTF-8 bytes"));
    }
    gate(world)?;
    let text_folded = text.map(fold).unwrap_or_default();
    let mut records = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "hypothesis")
    {
        if status.is_some_and(|status| record.status() != status) {
            continue;
        }
        if !text_folded.is_empty() {
            let source = format!(
                "{}\n{}\n{}",
                record.title,
                record.string("statement").unwrap_or(""),
                record.body
            );
            if !fold(&source).contains(&text_folded) {
                continue;
            }
        }
        if records.len() == max_items {
            return Err(invalid(format!(
                "hypotheses have more than {max_items} matches; refusing an incomplete result"
            )));
        }
        records.push(record);
    }
    let mut result = envelope(
        world,
        "hypothesis-list",
        "author-all-time-hypotheses",
        max_characters,
    );
    result["maxItems"] = json!(max_items);
    result["status"] = json!(status);
    result["text"] = json!(text);
    result["count"] = json!(records.len());
    result["hypotheses"] = json!([]);
    let base = encode_bounded(
        &result,
        consequences::MAX_PREVIEW_BYTES,
        Some(max_characters),
    )?;
    let mut remaining_bytes = consequences::MAX_PREVIEW_BYTES - base.len();
    let mut remaining_characters =
        max_characters - base.iter().filter(|byte| **byte & 0xc0 != 0x80).count();
    let mut values = Vec::new();
    for record in records {
        if !values.is_empty() {
            remaining_bytes = remaining_bytes
                .checked_sub(1)
                .ok_or_else(|| invalid("complete hypothesis list byte bound exceeded"))?;
            remaining_characters = remaining_characters
                .checked_sub(1)
                .ok_or_else(|| invalid("complete hypothesis list character bound exceeded"))?;
        }
        let (value, bytes, characters) =
            projection(world, record, remaining_bytes, remaining_characters)?;
        remaining_bytes -= bytes;
        remaining_characters -= characters;
        values.push(value);
    }
    result["hypotheses"] = Json::Array(values);
    bounded_result(&result, max_characters)?;
    Ok(result)
}

pub fn detail(world: &World, hypothesis_id: &str, max_characters: usize) -> Result<Json> {
    budget(max_characters)?;
    id_bound(hypothesis_id, Some("hypothesis"), "hypothesisId")?;
    gate(world)?;
    let record = existing(world, hypothesis_id, Some("hypothesis"))?;
    let mut result = envelope(
        world,
        "hypothesis-detail",
        "author-all-time-hypotheses",
        max_characters,
    );
    result["hypothesis"] = Json::Null;
    let base = encode_bounded(
        &result,
        consequences::MAX_PREVIEW_BYTES,
        Some(max_characters),
    )?;
    // The four placeholder characters in null are replaced by the projection.
    let remaining_bytes = consequences::MAX_PREVIEW_BYTES - (base.len() - 4);
    let remaining_characters =
        max_characters - (base.iter().filter(|byte| **byte & 0xc0 != 0x80).count() - 4);
    result["hypothesis"] = projection(world, record, remaining_bytes, remaining_characters)?.0;
    bounded_result(&result, max_characters)?;
    Ok(result)
}

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "hers", "him", "his", "how", "i", "in", "is", "it", "its", "me", "my", "of", "on",
    "or", "our", "she", "that", "the", "their", "them", "they", "this", "to", "was", "we", "what",
    "when", "where", "which", "who", "why", "with", "you", "your",
];

fn term_character(character: char) -> bool {
    word(character) || matches!(character, '\'' | '-')
}

// Native dependency-free transcription of pinned lexical.fts_query_parts.
// Query quotes are literal phrase syntax, never executable SQL/FTS expressions.
fn query_parts(query: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let characters: Vec<_> = query.chars().collect();
    let mut phrases = BTreeSet::new();
    let mut terms = BTreeSet::new();
    let mut cursor = 0;
    while cursor < characters.len() {
        if characters[cursor] == '"' {
            let mut end = cursor + 1;
            let mut found = None;
            while end < characters.len() {
                if characters[end] == '\\' {
                    if end + 1 == characters.len() || characters[end + 1] == '\n' {
                        break;
                    }
                    end += 2;
                } else if characters[end] == '"' {
                    found = Some(end);
                    break;
                } else {
                    end += 1;
                }
            }
            if let Some(end) = found {
                let raw: String = characters[cursor + 1..end].iter().collect();
                let unescaped = raw.replace("\\\"", "\"");
                let phrase = fold(unescaped.trim_matches(|character: char| {
                    character.is_whitespace() || ('\u{001c}'..='\u{001f}').contains(&character)
                }));
                if !phrase.is_empty() {
                    phrases.insert(phrase);
                }
                cursor = end + 1;
                continue;
            }
        }
        if term_character(characters[cursor]) {
            let start = cursor;
            while cursor < characters.len() && term_character(characters[cursor]) {
                cursor += 1;
            }
            let term = fold(&characters[start..cursor].iter().collect::<String>());
            if term.chars().count() > 1 && !STOPWORDS.contains(&term.as_str()) {
                terms.insert(term);
            }
        } else {
            cursor += 1;
        }
    }
    if phrases.is_empty() && terms.is_empty() {
        let all: BTreeSet<_> = query
            .split(|character| !term_character(character))
            .filter(|term| term.chars().count() > 1)
            .map(fold)
            .collect();
        let filtered: BTreeSet<_> = all
            .iter()
            .filter(|term| !STOPWORDS.contains(&term.as_str()))
            .cloned()
            .collect();
        terms = if filtered.is_empty() { all } else { filtered };
    }
    (phrases, terms)
}

fn term_count(text: &str, term: &str) -> usize {
    let mut offset = 0;
    let mut count = 0;
    while let Some(relative) = text[offset..].find(term) {
        let start = offset + relative;
        let end = start + term.len();
        if text[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !word(character))
            && text[end..]
                .chars()
                .next()
                .is_none_or(|character| !word(character))
        {
            count += 1;
            offset = end;
        } else {
            // A failed lookaround does not consume a potential overlapping
            // start. Accepted matches, like Python re.finditer, do not overlap.
            offset = start
                + text[start..]
                    .chars()
                    .next()
                    .expect("nonempty term")
                    .len_utf8();
        }
    }
    count
}

/// A separate all-time author lexical lane. It never opens an index, fits a
/// model, consults a horizon, or adds hypotheses to canonical retrieval input.
/// Callers own the explicit opt-in and must omit this lane for thread filtering.
pub fn source_search(world: &World, query: &str, limit: usize) -> Result<Vec<Json>> {
    if query.trim().is_empty()
        || query.len() > MAX_QUERY_BYTES
        || query.split_whitespace().count() > 64
    {
        return Err(invalid(
            "hypothesis search query requires 1..4096 UTF-8 bytes and at most 64 words",
        ));
    }
    if !(1..=100).contains(&limit) {
        return Err(invalid("hypothesis search limit must be 1..100"));
    }
    gate(world)?;
    let (phrases, terms) = query_parts(query);
    if phrases.is_empty() && terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut ranked = Vec::new();
    let mut count = 0;
    let mut bytes: usize = 0;
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "hypothesis")
    {
        count += 1;
        if count > MAX_SEARCH_RECORDS {
            return Err(invalid(
                "hypothesis search exceeds 1024 complete source records",
            ));
        }
        // Count the complete projection before allocating its joined text.
        let alternatives = record.list("alternatives").expect("validated alternatives");
        let fields = [
            record.title.as_str(),
            record.string("statement").unwrap_or(""),
            record.string("context").unwrap_or(""),
            record.body.as_str(),
        ];
        let size = fields
            .iter()
            .map(|value| value.len())
            .chain(
                alternatives
                    .iter()
                    .map(|value| value.as_str().expect("validated alternative").len()),
            )
            .try_fold(
                alternatives.len().saturating_sub(1)
                    + fields.iter().filter(|value| !value.is_empty()).count(),
                usize::checked_add,
            )
            .ok_or_else(|| invalid("hypothesis search source byte count overflow"))?;
        bytes = bytes
            .checked_add(size)
            .ok_or_else(|| invalid("hypothesis search source byte count overflow"))?;
        if bytes > MAX_SEARCH_BYTES {
            return Err(invalid(
                "hypothesis search exceeds 524288 complete source UTF-8 bytes",
            ));
        }
        let mut alternative_text = String::new();
        for (index, value) in alternatives.iter().enumerate() {
            if index != 0 {
                alternative_text.push(' ');
            }
            alternative_text.push_str(value.as_str().expect("validated alternative"));
        }
        let text = [
            fields[0],
            fields[1],
            alternative_text.as_str(),
            fields[2],
            fields[3],
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        let folded = fold(&text);
        let score: usize = phrases
            .iter()
            .map(|phrase| folded.matches(phrase.as_str()).count() * 4)
            .sum::<usize>()
            + terms
                .iter()
                .map(|term| term_count(&folded, term))
                .sum::<usize>();
        if score != 0 {
            ranked.push((score, record, text));
        }
    }
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });
    Ok(ranked.into_iter().take(limit).map(|(score, record, text)| {
        let snippet: String = text.chars().take(300).collect();
        json!({"id":record.id,"kind":"hypothesis","title":record.title,"sourcePath":record.source_path,"blobOid":record.blob_oid,
            "citation":crate::lore::citation(world,record,"hypothesis"),"nonCanonical":true,
            "documentId":format!("hypothesis:{}",record.id),"entityId":record.id,"documentKind":"hypothesis","heading":"possibility",
            "audienceKind":"author","text":text,"metadata":{"title":record.title,"nonCanonical":true,"hypothesisStatus":record.status()},
            "score":score as f64,"lanes":["hypothesis-source"],"snippet":snippet})
    }).collect())
}
