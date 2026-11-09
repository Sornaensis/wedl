//! Explicit, read-only magic supplements and literal requirement comparisons.
//! Every request reconstructs its admitted capture. Imported result JSON never
//! supplies authority, and comparisons do not approve actions or imply effects.
use crate::{
    Error, Result, magic,
    model::World,
    narrative::CharacterScope,
    validation,
    writing_frames::{self, FamilySelection, Frame, Lens},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_CAPTURE_BYTES: usize = 262_144;
const MAX_OPERAND_BYTES: usize = 16_384;
const MAX_REQUIREMENT_BYTES: usize = 131_072;
const MAX_DEPTH: usize = 32;
const MAX_NUMERIC_BYTES: usize = 256;

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Selector {
    Field {
        field: String,
    },
    Assertion {
        #[serde(rename = "assertionId")]
        assertion_id: String,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub target_id: String,
    pub selector: Selector,
    pub expected_value_raw: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn unsupported_operand() -> Error {
    invalid("requirement operand is invalid or exceeds comparison bounds")
}

fn closed(value: &Json, fields: &[&str]) -> Result<()> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid("requirement fields must be an object"))?;
    if map.len() != fields.len() || fields.iter().any(|field| !map.contains_key(*field)) {
        return Err(invalid("requirement has missing or unknown fields"));
    }
    Ok(())
}

fn field_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

impl Requirement {
    fn validate(&self) -> Result<Literal> {
        if !validation::valid_id_for_kind(&self.target_id, "lore-concept") {
            return Err(invalid("requirement targetId must be an exact concept ID"));
        }
        match &self.selector {
            Selector::Field { field } if field_name(field) => {}
            Selector::Assertion { assertion_id }
                if validation::valid_id_for_kind(assertion_id, "assertion") => {}
            _ => {
                return Err(invalid(
                    "requirement needs one exact root field or assertion ID",
                ));
            }
        }
        literal(&self.expected_value_raw)
    }
}

/// Closed request parsing rejects duplicate keys both in the request and inside
/// expectedValueRaw. Expected numbers are parsed as decimal tokens, never f64.
pub fn decode_requirement(payload: &str) -> Result<Requirement> {
    if payload.len() > MAX_REQUIREMENT_BYTES {
        return Err(invalid("requirement request exceeds 128 KiB"));
    }
    let mut decoder = serde_json::Deserializer::from_str(payload);
    let _: serde_yaml::Value = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate requirement JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid requirement JSON"))?;
    let value: Json =
        serde_json::from_str(payload).map_err(|_| invalid("invalid requirement JSON"))?;
    closed(&value, &["targetId", "selector", "expectedValueRaw"])?;
    let selector = &value["selector"];
    let selector = if selector.get("field").is_some() {
        closed(selector, &["field"])?;
        Selector::Field {
            field: selector["field"]
                .as_str()
                .ok_or_else(|| invalid("requirement field must be a string"))?
                .into(),
        }
    } else {
        closed(selector, &["assertionId"])?;
        Selector::Assertion {
            assertion_id: selector["assertionId"]
                .as_str()
                .ok_or_else(|| invalid("requirement assertionId must be a string"))?
                .into(),
        }
    };
    let requirement = Requirement {
        target_id: value["targetId"]
            .as_str()
            .ok_or_else(|| invalid("requirement targetId must be a string"))?
            .into(),
        selector,
        expected_value_raw: value["expectedValueRaw"]
            .as_str()
            .ok_or_else(|| invalid("expectedValueRaw must be JSON text"))?
            .into(),
    };
    requirement.validate()?;
    Ok(requirement)
}

fn capture_bounds(value: &Json, max_characters: usize) -> Result<()> {
    let text = serde_json::to_string(value)?;
    if text
        .len()
        .checked_add(1)
        .is_none_or(|bytes| bytes > MAX_CAPTURE_BYTES)
        || text.chars().count() > max_characters
    {
        return Err(invalid(
            "action requirement capture exceeds its complete result bounds",
        ));
    }
    Ok(())
}

fn stream_count(value: &Json, name: &str) -> Result<usize> {
    let streams = value[name]
        .as_array()
        .ok_or_else(|| Error::Data("incomplete native magic context".into()))?;
    streams.iter().try_fold(0usize, |count, stream| {
        let items = stream["result"]["items"]
            .as_array()
            .ok_or_else(|| Error::Data("incomplete native magic evidence".into()))?;
        count
            .checked_add(items.len())
            .ok_or_else(|| invalid("action requirement capture exceeds evidence bounds"))
    })
}

/// Capture only explicitly selected magic context at an exact source/frame.
/// Dramatic supplements use the character reading without an author lookup;
/// ordinary dramatic writing context remains a separate, unchanged operation.
pub fn capture(
    world: &World,
    frame: &Frame,
    families: &FamilySelection,
    max_characters: usize,
) -> Result<Json> {
    writing_frames::with_author_snapshot(world, frame, false, |authors| {
        capture_with_authors(world, authors, frame, families, max_characters)
    })
}

fn capture_with_authors(
    world: &World,
    authors: Option<&crate::manuscript::Snapshot>,
    frame: &Frame,
    families: &FamilySelection,
    max_characters: usize,
) -> Result<Json> {
    if !(1..=1_000_000).contains(&max_characters) {
        return Err(invalid(
            "action capture maxCharacters must be from 1 to 1000000",
        ));
    }
    if families.species.is_some() || families.social.is_some() || families.environment.is_some() {
        return Err(invalid(
            "action requirement capture accepts only the magic family",
        ));
    }
    let validated = writing_frames::decode_families(&serde_json::to_string(families)?)?;
    let selection = validated
        .magic
        .as_ref()
        .ok_or_else(|| invalid("action requirement capture needs an explicit magic selection"))?;
    writing_frames::resolve(world, authors, frame)?;
    let author = frame.lens == Lens::Author;
    let context = if author {
        let request = selection
            .author
            .as_ref()
            .ok_or_else(|| invalid("author capture needs an explicit magic author selection"))?;
        let context = magic::compose(
            world,
            &request.root_id,
            &request.selection,
            Some(&frame.at),
            "author",
            "author",
        )?;
        if context["linkageComplete"] != true {
            return Err(Error::NotFound(
                "selected magic linkage is incomplete; capture is unavailable".into(),
            ));
        }
        let records = context["records"]
            .as_array()
            .ok_or_else(|| Error::Data("incomplete native magic records".into()))?;
        if records.len() > 64
            || stream_count(&context, "canon")? > 256
            || stream_count(&context, "sections")? > 256
        {
            return Err(invalid(
                "action requirement capture exceeds evidence bounds",
            ));
        }
        context
    } else {
        let viewer = frame
            .pov_id
            .as_deref()
            .ok_or_else(|| invalid("character capture needs an explicit POV"))?;
        let scope =
            CharacterScope::with_scene(world, viewer, &frame.at, frame.scene_id.as_deref())?;
        let context =
            magic::character_in_scope(world, viewer, &selection.belief_targets, &frame.at, &scope)?;
        if context["complete"] != true || stream_count(&context, "beliefs")? > 256 {
            return Err(invalid(
                "action requirement capture exceeds evidence bounds",
            ));
        }
        context
    };
    let mut result = json!({
        "protocol":crate::CONTRACT, "scope":"action-requirement-capture",
        "frame":frame, "families":families,
        "effectivePerspective":if author {"author"} else {"character"},
        "complete":true, "context":context, "maxCharacters":max_characters,
    });
    capture_bounds(&result, max_characters)?;
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(&result)?));
    result["captureSha256"] = json!(digest);
    capture_bounds(&result, max_characters)?;
    Ok(result)
}

/// A comparison always recaptures current admitted source rather than trusting
/// a supplied capture, hash, context body, or previously calculated outcome.
pub fn compare(
    world: &World,
    frame: &Frame,
    families: &FamilySelection,
    requirement: &Requirement,
    max_characters: usize,
) -> Result<Json> {
    let expected = requirement.validate()?;
    writing_frames::with_author_snapshot(world, frame, false, |authors| {
        let captured = capture_with_authors(world, authors, frame, families, max_characters)?;
        let mut result = json!({
            "protocol":crate::CONTRACT, "scope":"action-requirement-comparison",
            "frame":frame, "captureSha256":captured["captureSha256"],
            "requirement":requirement, "outcome":"unknown", "actualRaw":null, "citations":[],
        });
        if frame.lens == Lens::Author {
            if let Some((value, shape, citation)) = operand(&captured["context"], requirement) {
                // Unsupported actual operands remain unknown. Expected operands are
                // explicit request input and were already validated above.
                if let Ok(actual) = projected_literal(value, shape, 0) {
                    if let Ok(raw) = actual.raw() {
                        result["outcome"] = json!(if actual == expected {
                            "matches"
                        } else {
                            "differs"
                        });
                        result["actualRaw"] = json!(raw);
                        result["citations"] = json!([citation]);
                    }
                }
            }
        }
        capture_bounds(&result, max_characters)?;
        Ok(result)
    })
}

fn operand<'a>(
    context: &'a Json,
    requirement: &Requirement,
) -> Option<(&'a Json, &'a Json, &'a Json)> {
    match &requirement.selector {
        Selector::Field { field } => {
            let records = context["records"].as_array()?;
            let records = records
                .iter()
                .filter(|record| record["conceptId"] == requirement.target_id)
                .collect::<Vec<_>>();
            if records.len() != 1 {
                return None;
            }
            let fields = records[0]["fields"].as_object()?;
            let selected = fields
                .values()
                .filter(|value| value["field"] == *field)
                .collect::<Vec<_>>();
            if selected.len() != 1 || selected[0]["known"] != true {
                return None;
            }
            let item = selected[0];
            Some((
                item.get("value")?,
                item.get("valueType")?,
                item.get("citation")?,
            ))
        }
        Selector::Assertion { assertion_id } => {
            let assertions = context["canon"]
                .as_array()?
                .iter()
                .filter(|stream| stream["ownerId"] == requirement.target_id)
                .flat_map(|stream| stream["result"]["items"].as_array().into_iter().flatten())
                .collect::<Vec<_>>();
            let selected = assertions
                .iter()
                .filter(|item| item["assertionId"] == *assertion_id)
                .copied()
                .collect::<Vec<_>>();
            if selected.len() != 1 {
                return None;
            }
            let selected = selected[0];
            if selected["classification"] != "author-fact"
                || assertions.iter().any(|item| {
                    item["assertionId"] != *assertion_id
                        && item["subject"] == selected["subject"]
                        && item["predicate"] == selected["predicate"]
                })
            {
                return None;
            }
            Some((
                selected.get("value")?,
                selected.get("valueType")?,
                selected.get("citation")?,
            ))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Literal {
    Null,
    Boolean(bool),
    String(String),
    Number {
        negative: bool,
        coefficient: String,
        exponent: i32,
    },
    Array(Vec<Literal>),
    Object(BTreeMap<String, Literal>),
}

impl Literal {
    fn raw(&self) -> Result<String> {
        fn append(out: &mut String, text: &str) -> Result<()> {
            if out
                .len()
                .checked_add(text.len())
                .is_none_or(|size| size > MAX_OPERAND_BYTES)
            {
                return Err(unsupported_operand());
            }
            out.push_str(text);
            Ok(())
        }
        fn write(value: &Literal, out: &mut String) -> Result<()> {
            match value {
                Literal::Null => append(out, "null"),
                Literal::Boolean(value) => append(out, if *value { "true" } else { "false" }),
                Literal::String(value) => append(out, &serde_json::to_string(value)?),
                Literal::Number {
                    negative,
                    coefficient,
                    exponent,
                } => {
                    if *negative {
                        append(out, "-")?;
                    }
                    append(out, coefficient)?;
                    if *exponent != 0 {
                        append(out, &format!("e{exponent}"))?;
                    }
                    Ok(())
                }
                Literal::Array(values) => {
                    append(out, "[")?;
                    for (index, value) in values.iter().enumerate() {
                        if index != 0 {
                            append(out, ",")?;
                        }
                        write(value, out)?;
                    }
                    append(out, "]")
                }
                Literal::Object(values) => {
                    append(out, "{")?;
                    for (index, (key, value)) in values.iter().enumerate() {
                        if index != 0 {
                            append(out, ",")?;
                        }
                        append(out, &serde_json::to_string(key)?)?;
                        append(out, ":")?;
                        write(value, out)?;
                    }
                    append(out, "}")
                }
            }
        }
        let mut raw = String::new();
        write(self, &mut raw)?;
        Ok(raw)
    }
}

fn number(raw: &str) -> Result<Literal> {
    if raw.len() > MAX_NUMERIC_BYTES {
        return Err(unsupported_operand());
    }
    let (negative, raw) = raw
        .strip_prefix('-')
        .map_or((false, raw), |raw| (true, raw));
    let (mantissa, exponent) = raw.split_once(['e', 'E']).map_or((raw, "0"), |parts| parts);
    let explicit_exponent: i32 = exponent.parse().map_err(|_| unsupported_operand())?;
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut exponent = explicit_exponent
        .checked_sub(fraction.len() as i32)
        .ok_or_else(unsupported_operand)?;
    if !(-1024..=1024).contains(&exponent) {
        return Err(unsupported_operand());
    }
    let digits = format!("{integer}{fraction}");
    let mut coefficient = digits.trim_start_matches('0').to_owned();
    if coefficient.is_empty() {
        return Ok(Literal::Number {
            negative: false,
            coefficient: "0".into(),
            exponent: 0,
        });
    }
    while coefficient.ends_with('0') {
        coefficient.pop();
        exponent += 1;
    }
    Ok(Literal::Number {
        negative,
        coefficient,
        exponent,
    })
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }
    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }
    fn take(&mut self, expected: u8) -> Result<()> {
        self.whitespace();
        if self.peek() != Some(expected) {
            return Err(unsupported_operand());
        }
        self.offset += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        self.whitespace();
        let start = self.offset;
        self.take(b'"')?;
        while let Some(byte) = self.peek() {
            self.offset += 1;
            match byte {
                b'"' => {
                    return serde_json::from_str(&self.input[start..self.offset])
                        .map_err(|_| unsupported_operand());
                }
                b'\\' => {
                    if self.peek().is_none() {
                        return Err(unsupported_operand());
                    }
                    self.offset += 1;
                }
                _ => {}
            }
        }
        Err(unsupported_operand())
    }
    fn value(&mut self, depth: usize) -> Result<Literal> {
        if depth > MAX_DEPTH {
            return Err(unsupported_operand());
        }
        self.whitespace();
        match self.peek() {
            Some(b'"') => Ok(Literal::String(self.string()?)),
            Some(b'[') => {
                self.offset += 1;
                self.whitespace();
                let mut values = Vec::new();
                if self.peek() == Some(b']') {
                    self.offset += 1;
                    return Ok(Literal::Array(values));
                }
                loop {
                    values.push(self.value(depth + 1)?);
                    self.whitespace();
                    if self.peek() == Some(b']') {
                        self.offset += 1;
                        break;
                    }
                    self.take(b',')?;
                }
                Ok(Literal::Array(values))
            }
            Some(b'{') => {
                self.offset += 1;
                self.whitespace();
                let mut values = BTreeMap::new();
                if self.peek() == Some(b'}') {
                    self.offset += 1;
                    return Ok(Literal::Object(values));
                }
                loop {
                    let key = self.string()?;
                    self.take(b':')?;
                    if values.insert(key, self.value(depth + 1)?).is_some() {
                        return Err(unsupported_operand());
                    }
                    self.whitespace();
                    if self.peek() == Some(b'}') {
                        self.offset += 1;
                        break;
                    }
                    self.take(b',')?;
                }
                Ok(Literal::Object(values))
            }
            Some(b'n') if self.input[self.offset..].starts_with("null") => {
                self.offset += 4;
                Ok(Literal::Null)
            }
            Some(b't') if self.input[self.offset..].starts_with("true") => {
                self.offset += 4;
                Ok(Literal::Boolean(true))
            }
            Some(b'f') if self.input[self.offset..].starts_with("false") => {
                self.offset += 5;
                Ok(Literal::Boolean(false))
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.offset;
                if self.peek() == Some(b'-') {
                    self.offset += 1;
                }
                match self.peek() {
                    Some(b'0') => self.offset += 1,
                    Some(b'1'..=b'9') => {
                        while matches!(self.peek(), Some(b'0'..=b'9')) {
                            self.offset += 1;
                        }
                    }
                    _ => return Err(unsupported_operand()),
                }
                if self.peek() == Some(b'.') {
                    self.offset += 1;
                    if !matches!(self.peek(), Some(b'0'..=b'9')) {
                        return Err(unsupported_operand());
                    }
                    while matches!(self.peek(), Some(b'0'..=b'9')) {
                        self.offset += 1;
                    }
                }
                if matches!(self.peek(), Some(b'e' | b'E')) {
                    self.offset += 1;
                    if matches!(self.peek(), Some(b'+' | b'-')) {
                        self.offset += 1;
                    }
                    if !matches!(self.peek(), Some(b'0'..=b'9')) {
                        return Err(unsupported_operand());
                    }
                    while matches!(self.peek(), Some(b'0'..=b'9')) {
                        self.offset += 1;
                    }
                }
                number(&self.input[start..self.offset])
            }
            _ => Err(unsupported_operand()),
        }
    }
}

fn literal(raw: &str) -> Result<Literal> {
    if raw.len() > MAX_OPERAND_BYTES {
        return Err(unsupported_operand());
    }
    let mut parser = Parser {
        input: raw,
        offset: 0,
    };
    let result = parser.value(0)?;
    parser.whitespace();
    if parser.offset != raw.len() {
        return Err(unsupported_operand());
    }
    Ok(result)
}

/// Native transport retains integer values as strings. Only their admitted
/// used-type descriptors restore numeric identity for literal comparisons.
fn projected_literal(value: &Json, shape: &Json, depth: usize) -> Result<Literal> {
    if depth > MAX_DEPTH {
        return Err(unsupported_operand());
    }
    let kind = if shape["type"] == "enum" {
        shape["scalarType"].as_str()
    } else {
        shape["type"].as_str()
    };
    match kind {
        Some("null") if value.is_null() => Ok(Literal::Null),
        Some("boolean") => value
            .as_bool()
            .map(Literal::Boolean)
            .ok_or_else(unsupported_operand),
        Some("string") => value
            .as_str()
            .filter(|text| text.len() <= MAX_OPERAND_BYTES)
            .map(|text| Literal::String(text.into()))
            .ok_or_else(unsupported_operand),
        Some("number") => {
            let raw = match value {
                Json::String(text) => text.clone(),
                Json::Number(number) => number.to_string(),
                _ => return Err(unsupported_operand()),
            };
            match literal(&raw)? {
                value @ Literal::Number { .. } => Ok(value),
                _ => Err(unsupported_operand()),
            }
        }
        Some("entity") => {
            let map = value.as_object().ok_or_else(unsupported_operand)?;
            if map.len() != 2 || value["kind"] != "record" || !value["id"].is_string() {
                return Err(unsupported_operand());
            }
            Ok(Literal::Object(BTreeMap::from([
                ("kind".into(), Literal::String("record".into())),
                (
                    "id".into(),
                    Literal::String(value["id"].as_str().unwrap().into()),
                ),
            ])))
        }
        Some("array") => {
            let values = value.as_array().ok_or_else(unsupported_operand)?;
            let shapes = shape["items"].as_array().ok_or_else(unsupported_operand)?;
            if values.len() != shapes.len() {
                return Err(unsupported_operand());
            }
            Ok(Literal::Array(
                values
                    .iter()
                    .zip(shapes)
                    .map(|(value, shape)| projected_literal(value, shape, depth + 1))
                    .collect::<Result<_>>()?,
            ))
        }
        Some("object") => {
            let values = value.as_object().ok_or_else(unsupported_operand)?;
            let shapes = shape["fields"]
                .as_object()
                .ok_or_else(unsupported_operand)?;
            if values.len() != shapes.len() {
                return Err(unsupported_operand());
            }
            Ok(Literal::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        let shape = shapes.get(key).ok_or_else(unsupported_operand)?;
                        Ok((key.clone(), projected_literal(value, shape, depth + 1)?))
                    })
                    .collect::<Result<_>>()?,
            ))
        }
        _ => Err(unsupported_operand()),
    }
}
