//! Author-only browser projections and private exact-file exchange. All source
//! changes still go through the existing explicit native writer routes.
use crate::{Error, Result, service::Service};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};

mod archive;
mod candidates;
mod frame;

pub(crate) const MAX_QUERY_BYTES: usize = 32 * 1024;
pub(crate) const RECEIPT_MEDIA: &str = "application/vnd.wedl.workbench+json";
const MAX_HANDLES: usize = 16_384;
const MAX_HANDLE_BYTES: usize = 48 * 1024 * 1024;
const WRITE_PATHS: [&str; 6] = [
    "/api/action-drafts/save",
    "/api/action-drafts/apply-outcome",
    "/api/edit-drafts/save",
    "/api/edit-drafts/apply",
    "/api/manuscripts/save",
    "/api/retcon-reviews/save",
];

pub(super) fn invalid() -> Error {
    Error::InvalidInput("Invalid Workbench selection".into())
}
pub(super) fn unavailable() -> Error {
    Error::NotFound("Workbench selection is unavailable".into())
}

struct Entry {
    kind: &'static str,
    value: String,
}

#[derive(Default)]
pub(super) struct Handles {
    entries: BTreeMap<String, Entry>,
    age: VecDeque<String>,
    bytes: usize,
}
impl Handles {
    pub(super) fn issue(&mut self, kind: &'static str, value: String) -> Result<String> {
        if value.len() > 8 * 1024 * 1024 {
            return Err(invalid());
        }
        let mut bytes = [0; 24];
        getrandom::fill(&mut bytes)
            .map_err(|_| Error::Data("Workbench selector unavailable".into()))?;
        let token = hex::encode(bytes);
        if self.entries.contains_key(&token) {
            return Err(Error::Data("Workbench selector unavailable".into()));
        }
        while self.entries.len() >= MAX_HANDLES || self.bytes + value.len() > MAX_HANDLE_BYTES {
            let expired = self.age.pop_front().ok_or_else(unavailable)?;
            self.bytes -= self
                .entries
                .remove(&expired)
                .ok_or_else(unavailable)?
                .value
                .len();
        }
        self.bytes += value.len();
        self.age.push_back(token.clone());
        self.entries.insert(token.clone(), Entry { kind, value });
        Ok(token)
    }
    pub(super) fn resolve(&self, kind: &'static str, token: &str) -> Result<String> {
        self.entries
            .get(token)
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.value.clone())
            .ok_or_else(unavailable)
    }
    pub(super) fn request(&mut self, route: &'static str, raw: String) -> Result<String> {
        if !WRITE_PATHS.contains(&route) {
            return Err(invalid());
        }
        self.issue(route, raw)
    }
}

#[derive(Default)]
pub(crate) struct Browser(Mutex<Handles>);

pub(crate) fn needs_current(endpoint: &str, query: &BTreeMap<String, String>) -> bool {
    if endpoint.starts_with("archive-") {
        archive::needs_current(endpoint, query)
    } else {
        !matches!(endpoint, "native-request" | "file")
    }
}

impl Browser {
    pub(crate) fn execute(
        &self,
        service: &Service,
        endpoint: &str,
        query: &BTreeMap<String, String>,
        live: bool,
    ) -> Result<Value> {
        let mut handles = self.0.lock().map_err(|_| unavailable())?;
        match endpoint {
            "source" => {
                closed(query, &[])?;
                let world = service.world();
                Ok(
                    json!({"label":display_text(world,&world.world_record()?.title),"current":world.revision.is_none(),"writes":live}),
                )
            }
            "native-request" => {
                closed(query, &["route", "token"])?;
                if !live {
                    return Err(unavailable());
                }
                let route = field(query, "route")?;
                let route = WRITE_PATHS
                    .iter()
                    .copied()
                    .find(|path| *path == route)
                    .ok_or_else(invalid)?;
                Ok(json!({"bytes":handles.resolve(route, field(query, "token")?)?}))
            }
            "file" => {
                closed(query, &["token"])?;
                Ok(json!({"bytes":handles.resolve("native-file", field(query, "token")?)?}))
            }
            "inspect"
                if query.get("route").is_some_and(|route| {
                    matches!(
                        route.as_str(),
                        "/api/manuscripts/save" | "/api/retcon-reviews/save"
                    )
                }) =>
            {
                closed(query, &["route", "token"])?;
                archive::inspect(
                    service,
                    field(query, "route")?,
                    field(query, "token")?,
                    &handles,
                )
            }
            "writing-choices" | "writing-frame" | "writing-import" | "writing-plan"
            | "writing-exchange" => frame::read(service, endpoint, query, &mut handles),
            "action-list"
            | "action-open"
            | "action-review"
            | "action-save"
            | "action-expectation"
            | "action-prepare-context"
            | "action-confirm-context"
            | "edit-list"
            | "edit-open"
            | "edit-review"
            | "edit-save"
            | "approve"
            | "inspect" => candidates::read(service, endpoint, query, &mut handles, live),
            _ if endpoint.starts_with("archive-") => {
                archive::read(service, endpoint, query, &mut handles)
            }
            _ => Err(unavailable()),
        }
    }
    pub(crate) fn prepare_simulation(
        &self,
        service: &Service,
        query: &BTreeMap<String, String>,
    ) -> Result<crate::reader::Prepared> {
        let mut handles = self.0.lock().map_err(|_| unavailable())?;
        archive::prepare_simulation(service, query, &mut handles)
    }
}

pub(crate) fn present_simulation(value: Value) -> Result<Value> {
    archive::present_simulation(value)
}

pub(super) fn closed(query: &BTreeMap<String, String>, fields: &[&str]) -> Result<()> {
    if query.keys().any(|field| !fields.contains(&field.as_str())) {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn field<'a>(query: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    query
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)
}
pub(super) fn at(value: &Value) -> Result<crate::model::StoryTime> {
    let timeline = value["timeline"].as_str().ok_or_else(invalid)?;
    let tick = value["tick"]
        .as_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(invalid)?;
    let order = value["order"]
        .as_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(invalid)?;
    crate::model::StoryTime::new(timeline, tick, order)
}
pub(super) fn exact<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(unavailable)
}

pub(super) fn display_text(world: &crate::model::World, raw: &str) -> String {
    fn add(world: &crate::model::World, text: &mut String, token: &str) {
        if token.is_empty() {
            return;
        }
        if let Some(record) = world.records.get(token) {
            text.push_str(&record.title);
        } else if crate::validation::valid_record_id(token)
            || crate::action_drafts::valid_action_id(token)
            || crate::edit_drafts::valid_edit_id(token)
            || crate::action_drafts::valid_proposal_id(token)
            || ((token.len() == 40 || token.len() == 64)
                && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
            || ["story/", "story\\", "author/", "author\\"]
                .iter()
                .any(|prefix| token.starts_with(prefix))
        {
            text.push_str("[private reference]");
        } else {
            text.push_str(token);
        }
    }
    let mut result = String::with_capacity(raw.len());
    let mut start = 0;
    for (index, ch) in raw.char_indices() {
        if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/' | '\\') {
            continue;
        }
        add(world, &mut result, &raw[start..index]);
        result.push(ch);
        start = index + ch.len_utf8();
    }
    add(world, &mut result, &raw[start..]);
    result
}

#[derive(Clone, Serialize)]
pub(super) struct Choice {
    pub(super) label: String,
    pub(super) token: String,
}
pub(super) fn choice(
    handles: &mut Handles,
    kind: &'static str,
    label: &str,
    value: String,
) -> Result<Choice> {
    Ok(Choice {
        label: label.to_owned(),
        token: handles.issue(kind, value)?,
    })
}

#[derive(Serialize, Deserialize)]
pub(super) struct Pinned {
    pub(super) fingerprint: String,
    pub(super) value: String,
}
impl Pinned {
    pub(super) fn bind(world: &crate::model::World, value: String) -> Result<String> {
        Ok(serde_json::to_string(&Self {
            fingerprint: world.fingerprint.clone(),
            value,
        })?)
    }
    pub(super) fn read(world: &crate::model::World, raw: &str) -> Result<String> {
        let value: Self = serde_json::from_str(raw).map_err(|_| unavailable())?;
        if value.fingerprint != world.fingerprint {
            return Err(Error::Conflict("Workbench source changed".into()));
        }
        Ok(value.value)
    }
}

/// A successful writer acknowledgment is not conditional on a later source
/// refresh. Only explicit recognized facts from the native receipt are sent.
pub(crate) fn receipt(envelope: &Value) -> Value {
    let result = &envelope["result"];
    let evidence = result["evidenceVerified"].as_bool().or_else(|| {
        (result["operation"] == "action-outcome-apply")
            .then(|| {
                result["receipt"]["evidenceSha256"]
                    .as_str()
                    .map(crate::action_drafts::valid_hash)
            })
            .flatten()
    });
    json!({
        "outcome":"confirmed",
        "confirmed":true,
        "sourceMatches":result["sourceMatches"].as_bool(),
        "evidenceVerified":evidence,
        "canonicalNoOp":result["canonicalNoOp"].as_bool(),
        "alreadyApplied":result["alreadyApplied"].as_bool(),
        "writesPerformed":result["writesPerformed"].as_bool(),
    })
}
