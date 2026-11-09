//! Pure full-author discovery and editable Plans from one validated source.
//! Transport adapters capture the source; these helpers never read or write it.
use crate::{Error, Result, model::World};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::io::Write;

mod context;
mod scaffold;

pub const MAX_ITEMS: usize = 4096;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    Changeset,
    Generational,
    Lore,
    Briefs,
    Assertions,
    Descriptions,
    SpatialHistory,
    Existence,
    Governance,
}

impl Family {
    pub const ALL: [Self; 9] = [
        Self::Changeset,
        Self::Generational,
        Self::Lore,
        Self::Briefs,
        Self::Assertions,
        Self::Descriptions,
        Self::SpatialHistory,
        Self::Existence,
        Self::Governance,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Changeset => "changeset",
            Self::Generational => "generational",
            Self::Lore => "lore",
            Self::Briefs => "briefs",
            Self::Assertions => "assertions",
            Self::Descriptions => "descriptions",
            Self::SpatialHistory => "spatial-history",
            Self::Existence => "existence",
            Self::Governance => "governance",
        }
    }

    pub fn supports_kind(self, kind: &str) -> bool {
        if !crate::validation::is_record_kind(kind) {
            return false;
        }
        match self {
            Self::Changeset | Self::Lore => true,
            Self::Generational => false,
            Self::Briefs => kind == "context-brief",
            Self::Assertions => matches!(kind, "assertion" | "knowledge" | "world"),
            Self::Descriptions => kind != "hypothesis",
            Self::SpatialHistory => matches!(kind, "location" | "route"),
            Self::Existence => kind == "existence-episode",
            Self::Governance => matches!(kind, "world" | "lore-concept" | "assertion"),
        }
    }

    pub fn required_capabilities(self) -> &'static [&'static str] {
        match self {
            Self::Changeset | Self::Lore => &[],
            Self::Generational => &["generational-core-v1"],
            Self::Briefs => &["briefs-v1"],
            Self::Assertions => &["lore-core-v1", "assertions-v1"],
            Self::Descriptions => &["descriptions-v1"],
            Self::SpatialHistory => &["spatial-core-v1", "spatial-history-v1"],
            Self::Existence => &["generational-core-v1", "existence-v1"],
            Self::Governance => &[
                "generational-core-v1",
                "lore-core-v1",
                "collective-parties-v1",
            ],
        }
    }

    pub fn available(self, world: &World) -> bool {
        let capabilities = world
            .world_record()
            .ok()
            .and_then(|owner| owner.list("capabilities"));
        self.required_capabilities().iter().all(|required| {
            capabilities
                .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(*required)))
        })
    }
}

fn optional_string<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(decoder).map(Some)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScaffoldRequest {
    pub expected_fingerprint: String,
    pub family: Family,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_string"
    )]
    pub entity: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_string"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional_string"
    )]
    pub title: Option<String>,
}

pub(super) fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn validate_request(request: &ScaffoldRequest) -> Result<()> {
    if request.expected_fingerprint.len() != 64
        || !request
            .expected_fingerprint
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid(
            "authoring scaffold requires an exact source fingerprint",
        ));
    }
    for (field, value, limit) in [
        ("entity", &request.entity, 1024),
        ("id", &request.id, 1024),
        ("title", &request.title, 256),
    ] {
        if value.as_ref().is_some_and(|text| {
            text.trim().is_empty() || text.len() > limit || text.chars().any(char::is_control)
        }) {
            return Err(invalid(format!(
                "authoring scaffold {field} must be a nonempty single line within {limit} UTF-8 bytes"
            )));
        }
    }
    Ok(())
}

/// A small closed request. Serde rejects repeated fields and explicit nulls.
pub fn decode_scaffold(payload: &str) -> Result<ScaffoldRequest> {
    if payload.len() > MAX_REQUEST_BYTES {
        return Err(invalid("authoring scaffold request exceeds 16384 bytes"));
    }
    let request: ScaffoldRequest = serde_json::from_str(payload)
        .map_err(|error| invalid(format!("invalid authoring scaffold request: {error}")))?;
    validate_request(&request)?;
    Ok(request)
}

fn admit(world: &World) -> Result<()> {
    if !crate::validation::validate_world(world).valid {
        return Err(Error::Validation(
            "authoring helpers require valid captured source; run validate for diagnostics".into(),
        ));
    }
    Ok(())
}

struct ResponseSize {
    bytes: usize,
    exceeded: bool,
}
impl Write for ResponseSize {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_RESPONSE_BYTES.saturating_sub(self.bytes) {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "complete authoring response exceeds 1 MiB",
            ));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn finish(world: &World, operation: &str, mut result: Json) -> Result<Json> {
    let object = result
        .as_object_mut()
        .ok_or_else(|| Error::Data("invalid internal authoring helper result".into()))?;
    for (key, value) in [
        ("protocol", json!(crate::CONTRACT)),
        ("operation", json!(operation)),
        ("sourceSchema", json!(crate::source::SOURCE_SCHEMA)),
        ("fingerprint", json!(world.fingerprint)),
        ("revision", json!(world.revision)),
        ("sourceIndependent", json!(false)),
        ("writesPerformed", json!(false)),
        ("validationComplete", json!(false)),
    ] {
        object.insert(key.into(), value);
    }
    let mut size = ResponseSize {
        bytes: 0,
        exceeded: false,
    };
    let encoded = serde_json::to_writer(&mut size, &result);
    if size.exceeded {
        return Err(invalid("complete authoring response exceeds 1 MiB"));
    }
    encoded?;
    Ok(result)
}

/// Complete current-source author choices. Overflow refuses the whole response.
pub fn context(world: &World, max_items: usize) -> Result<Json> {
    if !(1..=MAX_ITEMS).contains(&max_items) {
        return Err(invalid("authoring context maxItems must be 1..4096"));
    }
    admit(world)?;
    finish(
        world,
        "authoring.context",
        context::build(world, max_items)?,
    )
}

/// Return an editable Plan proven against the selected immutable capture.
pub fn scaffold(world: &World, request: &ScaffoldRequest) -> Result<Json> {
    validate_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "authoring scaffold source fingerprint changed".into(),
        ));
    }
    admit(world)?;
    if !request.family.available(world) {
        return Err(invalid(format!(
            "{} scaffolds require declared capabilities: {}",
            request.family.name(),
            request.family.required_capabilities().join(", ")
        )));
    }
    finish(
        world,
        "authoring.scaffold",
        scaffold::build(world, request)?,
    )
}
