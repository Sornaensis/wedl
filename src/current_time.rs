//! Explicit shared-current-time plans. The author supplies a complete exact
//! point; one native consequence plan updates the world and every active scene.
//! This module neither advances scenes implicitly nor publishes source.
use crate::{
    Error, Result, consequences,
    model::{StoryTime, World},
    validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::io::Write;

pub const MAX_REQUEST_BYTES: usize = 16_384;
pub const MIN_CHARACTERS: usize = consequences::MIN_CHARACTERS;
pub const MAX_CHARACTERS: usize = consequences::MAX_CHARACTERS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub expected_fingerprint: String,
    pub at: StoryTime,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRequest {
    expected_fingerprint: String,
    at: WireTime,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTime {
    timeline: String,
    tick: String,
    order: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn validate_request(request: &Request) -> Result<()> {
    if request.expected_fingerprint.len() != 64
        || !request
            .expected_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "current-time request requires an exact lowercase source fingerprint",
        ));
    }
    if request.at.timeline.trim().is_empty() || request.at.timeline.len() > 128 {
        return Err(invalid(
            "current-time timeline must be a nonempty string within 128 UTF-8 bytes",
        ));
    }
    Ok(())
}

/// Strict input has no default time, default timeline or request-owned grant.
/// Duplicate keys are refused before typed JSON decoding can collapse them.
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("current-time request exceeds 16 KiB"));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate current-time JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid current-time JSON"))?;
    let wire: WireRequest =
        serde_json::from_str(raw).map_err(|_| invalid("invalid current-time request fields"))?;
    let tick =
        wire.at.tick.parse::<i64>().map_err(|_| {
            invalid("current-time tick must be a canonical decimal string within i64")
        })?;
    let order =
        wire.at.order.parse::<i32>().map_err(|_| {
            invalid("current-time order must be a canonical decimal string within i32")
        })?;
    if tick.to_string() != wire.at.tick || order.to_string() != wire.at.order {
        return Err(invalid(
            "current-time coordinates must be canonical signed decimal strings",
        ));
    }
    let request = Request {
        expected_fingerprint: wire.expected_fingerprint,
        at: StoryTime::new(wire.at.timeline, tick, order)?,
    };
    validate_request(&request)?;
    Ok(request)
}

// Source YAML keeps numeric clock scalars; StoryTime's public serializer uses
// decimal strings to preserve precision on the JSON/browser boundary.
fn source_point(at: &StoryTime) -> Yaml {
    Yaml::Mapping(Mapping::from_iter([
        ("timeline".into(), at.timeline.clone().into()),
        ("tick".into(), at.tick.into()),
        ("order".into(), i64::from(at.order).into()),
    ]))
}

fn same_point(world: &World, value: Option<&Yaml>, at: &StoryTime) -> bool {
    value
        .and_then(|value| StoryTime::from_yaml(value, &world.default_timeline()).ok())
        .as_ref()
        == Some(at)
}

/// Produce a complete explicit Plan against one valid captured source. Every
/// active scene is included, ordered by exact ID after the world operation.
/// Backward/cross-timeline requests are not rewritten: candidate validation
/// decides whether the authored intervals admit that explicit change.
pub fn plan(world: &World, request: &Request) -> Result<consequences::Plan> {
    validate_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the current-time request snapshot".into(),
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "base snapshot failed validation; run validate for diagnostics".into(),
        ));
    }
    world.story_time(
        request.at.tick,
        Some(&request.at.timeline),
        request.at.order,
    )?;
    let active: Vec<_> = world
        .records
        .values()
        .filter(|record| record.kind == "scene" && record.status() == "active")
        .collect();
    if active.len() >= consequences::MAX_OPERATIONS {
        return Err(invalid(
            "complete current-time plan exceeds 63 active scenes plus the world",
        ));
    }

    let point = source_point(&request.at);
    let owner = world.world_record()?;
    let mut fields = Mapping::from_iter([("kind".into(), "world".into())]);
    if !same_point(world, owner.get("current_time"), &request.at) {
        fields.insert("current_time".into(), point.clone());
    }
    let mut operations = vec![consequences::Operation::Update {
        entity_id: owner.id.clone(),
        patch: serde_yaml::to_string(&fields)?,
    }];
    for scene in active {
        let time = scene
            .get("time")
            .and_then(Yaml::as_mapping)
            .ok_or_else(|| Error::Validation("active scene time is unavailable".into()))?;
        let mut fields = Mapping::from_iter([("kind".into(), "scene".into())]);
        if !same_point(world, time.get("current"), &request.at) {
            let mut time = time.clone();
            time.insert("current".into(), point.clone());
            // The native typed scene grammar spells absent end by omission.
            // An unchanged point uses a kind-only operation and retains exact
            // original bytes, including a source's explicit null endpoint.
            if time.get("end").is_some_and(Yaml::is_null) {
                time.remove("end");
            }
            fields.insert("time".into(), time.into());
        }
        operations.push(consequences::Operation::Update {
            entity_id: scene.id.clone(),
            patch: serde_yaml::to_string(&fields)?,
        });
    }
    let plan = consequences::Plan {
        expected_fingerprint: request.expected_fingerprint.clone(),
        operations,
        analysis: None,
    };
    consequences::validate_plan(&plan)?;
    Ok(plan)
}

fn bounded_result(result: &Json, max_characters: usize) -> Result<()> {
    struct Bounded {
        bytes: Vec<u8>,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > consequences::MAX_PREVIEW_BYTES.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("complete current-time JSON bound"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded { bytes: Vec::new() };
    serde_json::to_writer(&mut writer, result)
        .map_err(|_| invalid("current-time preview exceeds 8 MiB of complete JSON"))?;
    let characters = std::str::from_utf8(&writer.bytes)
        .expect("JSON serialization is UTF-8")
        .chars()
        .count();
    if characters > max_characters {
        return Err(Error::InvalidInput(format!(
            "complete current-time preview requires {characters} characters; maxCharacters is {max_characters}"
        )));
    }
    Ok(())
}

/// Return the exact exportable Plan and unchanged complete consequence preview.
/// This report never grants application authority, and counts both the plan
/// and all preview metadata against its complete output budget.
pub fn preview(world: &World, request: &Request, max_characters: usize) -> Result<Json> {
    if !(MIN_CHARACTERS..=MAX_CHARACTERS).contains(&max_characters) {
        return Err(invalid(
            "current-time preview maxCharacters must be 256..1000000",
        ));
    }
    let plan = plan(world, request)?;
    let preview = consequences::preview(world, &plan, max_characters)?;
    let active_scene_ids: Vec<_> = plan
        .operations
        .iter()
        .skip(1)
        .map(|operation| match operation {
            consequences::Operation::Update { entity_id, .. } => entity_id,
            consequences::Operation::Create { .. } => {
                unreachable!("current-time plans only update")
            }
        })
        .collect();
    let result = json!({
        "protocol":crate::CONTRACT,"operation":"current-time-preview",
        "scope":"local-author-current-time","request":request,
        "previousTime":world.current_time()?,"activeSceneIds":active_scene_ids,
        "plan":plan,"preview":preview,"maxCharacters":max_characters,
        "complete":true,"validationComplete":false,"writesPerformed":false,"canApply":false,
    });
    bounded_result(&result, max_characters)?;
    Ok(result)
}
