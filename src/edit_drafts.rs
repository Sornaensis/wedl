//! Retained private edits of one existing canonical record.
//!
//! Saving an unfinished buffer does not parse its YAML patch or publish source.
//! A separate complete review and explicit approval publish the target and its
//! terminal private receipt through the shared recoverable publisher.
use crate::{Error, Result, action_drafts, consequences, validation};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{
    io::Write,
    path::{Component, Path},
};

mod outcomes;
mod store;

pub(crate) use outcomes::validate_publication;
pub use outcomes::{apply, review};
pub use store::{detail, list, preview, save};

pub const MAX_DOCUMENT: usize = 262_144;
const MAX_REQUEST: usize = 524_288;
const MAX_TEXT: usize = 131_072;
const MAX_REPORT: usize = 8 * 1024 * 1024;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

pub fn valid_edit_id(value: &str) -> bool {
    value.strip_prefix("edit_").is_some_and(|suffix| {
        suffix.len() == 26
            && suffix
                .bytes()
                .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
    })
}

fn hash(bytes: &[u8]) -> String {
    action_drafts::sha256(bytes)
}
fn valid_hash(value: &str) -> bool {
    action_drafts::valid_hash(value)
}
fn path(id: &str) -> String {
    format!("author/edits/{id}.json")
}

fn source_path(value: &str) -> bool {
    value.len() <= 4096
        && value.starts_with("story/")
        && !value.contains(['\\', '\0'])
        && Path::new(value)
            .extension()
            .and_then(|value| value.to_str())
            == Some("md")
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && value
            .split('/')
            .all(|part| !matches!(part, "" | "." | ".."))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BodyMode {
    Preserve,
    Replace,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedFile {
    pub entity_id: String,
    pub source_path: String,
    pub before_sha256: String,
    pub after_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedReceipt {
    pub plan_sha256: String,
    pub review_sha256: String,
    pub report_sha256: String,
    pub request_sha256: String,
    pub base_fingerprint: String,
    pub candidate_fingerprint: String,
    pub before_edit_sha256: String,
    pub before_draft_sha256: String,
    pub max_characters: usize,
    pub file: AppliedFile,
}

fn receipt_field<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<AppliedReceipt>, D::Error> {
    AppliedReceipt::deserialize(decoder).map(Some)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub id: String,
    pub title: String,
    pub world_id: String,
    pub target_id: String,
    pub base_fingerprint: String,
    pub base_source_sha256: String,
    pub base_source_path: String,
    /// Opaque until source review. Empty, unfinished and invalid text is retained.
    pub patch: String,
    pub body_mode: BodyMode,
    pub body_markdown: String,
    pub phase: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "receipt_field"
    )]
    pub applied_receipt: Option<AppliedReceipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRequest {
    pub expected_fingerprint: String,
    #[serde(deserialize_with = "prior_hash_field")]
    pub expected_sha256: Option<String>,
    pub draft: Draft,
}

fn prior_hash_field<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(decoder)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRequest {
    pub expected_fingerprint: String,
    pub edit_id: String,
    pub expected_sha256: String,
    pub max_characters: usize,
}

impl ReviewRequest {
    fn validate(&self) -> Result<()> {
        if !valid_edit_id(&self.edit_id)
            || !valid_hash(&self.expected_fingerprint)
            || !valid_hash(&self.expected_sha256)
            || !(256..=1_000_000).contains(&self.max_characters)
        {
            return Err(invalid(
                "edit review requires exact source/private pins and maxCharacters 256..1000000",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRequest {
    pub expected_fingerprint: String,
    pub edit_id: String,
    pub expected_sha256: String,
    pub max_characters: usize,
    pub review_sha256: String,
    pub approved: bool,
}

impl ApplyRequest {
    pub fn review_request(&self) -> ReviewRequest {
        ReviewRequest {
            expected_fingerprint: self.expected_fingerprint.clone(),
            edit_id: self.edit_id.clone(),
            expected_sha256: self.expected_sha256.clone(),
            max_characters: self.max_characters,
        }
    }
    fn validate(&self) -> Result<()> {
        self.review_request().validate()?;
        if !valid_hash(&self.review_sha256) || !self.approved {
            return Err(invalid(
                "edit application requires its exact review hash and explicit approved:true",
            ));
        }
        Ok(())
    }
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "complete edit JSON byte bound exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encoded(value: &impl Serialize, maximum: usize) -> Result<Vec<u8>> {
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| invalid("complete edit JSON exceeds its byte bound"))?;
    Ok(writer.bytes)
}
fn digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let mut bytes = domain.to_vec();
    bytes.extend(encoded(value, MAX_REPORT)?);
    Ok(hash(&bytes))
}
fn bounded_report(value: &Json, max_characters: Option<usize>) -> Result<()> {
    let bytes = encoded(value, MAX_REPORT)?;
    if max_characters.is_some_and(|maximum| {
        std::str::from_utf8(&bytes)
            .expect("JSON UTF-8")
            .chars()
            .count()
            > maximum
    }) {
        return Err(invalid("complete edit report exceeds maxCharacters"));
    }
    Ok(())
}

fn validate(draft: &Draft) -> Result<()> {
    if !valid_edit_id(&draft.id)
        || !validation::valid_id_for_kind(&draft.world_id, "world")
        || !validation::valid_record_id(&draft.target_id)
        || !valid_hash(&draft.base_fingerprint)
        || !valid_hash(&draft.base_source_sha256)
        || !source_path(&draft.base_source_path)
        || !["draft", "cancelled", "applied"].contains(&draft.phase.as_str())
        || draft.title.trim().is_empty()
        || draft.title.len() > 1024
        || draft.patch.len() > MAX_TEXT
        || draft.body_markdown.len() > MAX_TEXT
    {
        return Err(invalid(
            "invalid or over-bound retained edit identity, source pins or text",
        ));
    }
    if (draft.phase == "applied") != draft.applied_receipt.is_some() {
        return Err(invalid(
            "a retained edit receipt exists exactly in applied history",
        ));
    }
    encoded(draft, MAX_DOCUMENT)?;
    if let Some(receipt) = &draft.applied_receipt {
        outcomes::validate_receipt(draft, receipt)?;
    }
    Ok(())
}

pub fn decode(payload: &str) -> Result<Draft> {
    if payload.len() > MAX_DOCUMENT {
        return Err(invalid("retained edit exceeds 256 KiB"));
    }
    let draft: Draft = serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, duplicate or unknown retained edit fields"))?;
    validate(&draft)?;
    Ok(draft)
}
pub fn serialize(draft: &Draft) -> Result<Vec<u8>> {
    validate(draft)?;
    let mut bytes = encoded(draft, MAX_DOCUMENT - 1)?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub fn decode_save(payload: &str) -> Result<SaveRequest> {
    if payload.len() > MAX_REQUEST {
        return Err(invalid("retained edit save exceeds 512 KiB"));
    }
    let request: SaveRequest = serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, duplicate or unknown edit save fields"))?;
    validate(&request.draft)?;
    if !valid_hash(&request.expected_fingerprint)
        || request
            .expected_sha256
            .as_deref()
            .is_some_and(|value| !valid_hash(value))
    {
        return Err(invalid(
            "edit save requires exact source and optional prior private hashes",
        ));
    }
    Ok(request)
}
pub fn decode_review(payload: &str) -> Result<ReviewRequest> {
    if payload.len() > 16_384 {
        return Err(invalid("edit review request exceeds 16 KiB"));
    }
    let request: ReviewRequest = serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, duplicate or unknown edit review fields"))?;
    request.validate()?;
    Ok(request)
}
pub fn decode_apply(payload: &str) -> Result<ApplyRequest> {
    if payload.len() > 16_384 {
        return Err(invalid("edit application request exceeds 16 KiB"));
    }
    let request: ApplyRequest = serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, duplicate or unknown edit application fields"))?;
    request.validate()?;
    Ok(request)
}

/// Interpret only for source review, never for storage. Body replacement has
/// one authority: bodyMode plus the retained body, not a hidden patch member.
fn plan(draft: &Draft) -> Result<consequences::Plan> {
    preflight_patch(&draft.patch)?;
    let value: serde_yaml::Value = serde_yaml::from_str(&draft.patch)
        .map_err(|_| invalid("retained patch is not valid duplicate-free YAML"))?;
    let serde_yaml::Value::Mapping(mut fields) = value else {
        return Err(invalid("retained patch must be a YAML mapping"));
    };
    if fields.contains_key(serde_yaml::Value::String("body".into())) {
        return Err(invalid(
            "retained patch must omit body; bodyMode owns prose replacement",
        ));
    }
    if draft.body_mode == BodyMode::Replace {
        fields.insert(
            serde_yaml::Value::String("body".into()),
            serde_yaml::Value::String(draft.body_markdown.clone()),
        );
    }
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum: consequences::MAX_PLAN_BYTES,
    };
    serde_yaml::to_writer(&mut writer, &fields)
        .map_err(|_| invalid("complete expanded edit patch exceeds its YAML byte bound"))?;
    let plan = consequences::Plan {
        expected_fingerprint: draft.base_fingerprint.clone(),
        operations: vec![consequences::Operation::Update {
            entity_id: draft.target_id.clone(),
            patch: String::from_utf8(writer.bytes).expect("YAML serializer produces UTF-8"),
        }],
        analysis: None,
    };
    consequences::validate_plan(&plan)?;
    Ok(plan)
}

fn preflight_patch(patch: &str) -> Result<()> {
    if patch.len() > MAX_TEXT {
        return Err(invalid("retained edit patch exceeds its raw byte bound"));
    }
    crate::record_edit::preflight_yaml(patch, MAX_TEXT, consequences::MAX_PLAN_BYTES)
        .map_err(|_| invalid("retained patch is invalid or exceeds complete expanded YAML bounds"))
}

fn currentness(draft: &Draft, compared: Option<&crate::model::World>) -> Json {
    let Some(world) = compared else {
        return json!({"state":"unavailable","reason":"unavailable"});
    };
    let Ok(owner) = world.world_record() else {
        return json!({"state":"unavailable","reason":"unavailable"});
    };
    let reason = if owner.id != draft.world_id {
        "repository-changed"
    } else if world.records.get(&draft.target_id).is_none_or(|record| {
        record.source_path != draft.base_source_path
            || hash(&record.raw_bytes) != draft.base_source_sha256
    }) {
        "target-changed"
    } else if world.fingerprint != draft.base_fingerprint {
        "canonical-changed"
    } else {
        "unchanged"
    };
    json!({"state":if reason=="unchanged" { "current" } else { "stale" },"reason":reason})
}
