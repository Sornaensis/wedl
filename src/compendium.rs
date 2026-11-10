//! Closed presentation reads for the loopback Compendium. Native v1 readers
//! and private writers do not use these context or selector handles.
pub(crate) mod bookmarks;
mod choices;
mod directory;
mod graph;
mod maps;

use crate::{
    model::StoryTime,
    narrative::CharacterScope,
    service::{CompendiumPin, CompendiumSource, CompendiumWitness, Service},
};
use bookmarks::{Authority, Issuer, Keyring, Target};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};

pub(crate) const MAX_QUERY_BYTES: usize = 4096;
const MAX_VIEWS: usize = 128;
const MAX_RESPONSE: usize = 192 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    Invalid,
    Unavailable,
    Changed,
    Source,
}

impl Failure {
    pub(crate) fn presentation(self) -> (u16, &'static str, &'static str) {
        match self {
            Self::Invalid => (400, "invalid_request", "This request cannot be completed."),
            Self::Unavailable => (404, "unavailable", "This selection is unavailable."),
            Self::Changed => (
                409,
                "context_changed",
                "The source changed. Reload this view.",
            ),
            Self::Source => (
                503,
                "source_unavailable",
                "The source is unavailable. Try again.",
            ),
        }
    }
}

type Result<T> = std::result::Result<T, Failure>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    All,
    AsOf,
    Character,
}

impl Mode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "all" => Ok(Self::All),
            "as-of" => Ok(Self::AsOf),
            "character" => Ok(Self::Character),
            _ => Err(Failure::Invalid),
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::AsOf => "as-of",
            Self::Character => "character",
        }
    }
}

#[derive(Clone)]
struct Moment {
    owner: String,
    member: &'static str,
    at: StoryTime,
}

#[derive(Clone)]
enum Lens {
    All,
    AsOf(Moment),
    Character {
        moment: Moment,
        viewer: String,
        scene: Option<String>,
    },
}

impl Lens {
    fn mode(&self) -> Mode {
        match self {
            Self::All => Mode::All,
            Self::AsOf(_) => Mode::AsOf,
            Self::Character { .. } => Mode::Character,
        }
    }

    fn moment(&self) -> Option<&Moment> {
        match self {
            Self::All => None,
            Self::AsOf(moment) | Self::Character { moment, .. } => Some(moment),
        }
    }

    fn scope<'a>(&'a self, pin: &'a CompendiumPin) -> Result<Option<CharacterScope<'a>>> {
        match self {
            Self::Character {
                moment,
                viewer,
                scene,
            } => {
                let scope =
                    CharacterScope::from_compendium_pin(pin, viewer, &moment.at, scene.as_deref())
                        .map_err(|_| Failure::Unavailable)?;
                if !pin
                    .world()
                    .records
                    .get(&moment.owner)
                    .map(|owner| scope.visible(owner))
                    .transpose()
                    .map_err(|_| Failure::Source)?
                    .unwrap_or(false)
                {
                    return Err(Failure::Unavailable);
                }
                Ok(Some(scope))
            }
            _ => Ok(None),
        }
    }
}

pub(crate) struct Request {
    action: Action,
    query: BTreeMap<String, String>,
}

enum Action {
    Context,
    Bookmark,
    Choices,
    Page,
    Record,
    Graph,
    Map,
    Locations,
}

impl Request {
    pub(crate) fn decode(endpoint: &str, query: BTreeMap<String, String>) -> Result<Self> {
        let action = match endpoint {
            "context" => Action::Context,
            "bookmark" => Action::Bookmark,
            "choices" => Action::Choices,
            "page" => Action::Page,
            "record" => Action::Record,
            "graph" => Action::Graph,
            "map" => Action::Map,
            "locations" => Action::Locations,
            _ => return Err(Failure::Unavailable),
        };
        let allowed: &[&str] = match action {
            Action::Context => &[
                "view",
                "mode",
                "timeline",
                "moment",
                "viewpoint",
                "scene",
                "calendar",
                "date",
                "record",
                "section",
            ],
            Action::Bookmark => &["address"],
            Action::Choices => &[
                "view",
                "menu",
                "mode",
                "timeline",
                "moment",
                "viewpoint",
                "q",
                "limit",
                "cursor",
                "calendar",
                "date",
                "kind",
            ],
            Action::Page => &["view", "kind", "q", "timeline", "limit", "cursor"],
            Action::Record => &["view", "record"],
            Action::Graph => &["view", "root", "timeline", "q", "limit", "cursor"],
            Action::Map => &[
                "view",
                "map",
                "zoom",
                "x",
                "y",
                "limit",
                "cursor",
                "mapCursor",
            ],
            Action::Locations => &["view", "limit", "cursor"],
        };
        if query.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(match action {
                Action::Bookmark => Failure::Unavailable,
                _ => Failure::Invalid,
            });
        }
        let request = Self { action, query };
        if request.get("section").is_some_and(|section| {
            section != "map" || request.get("record").is_some() || request.get("view").is_none()
        }) {
            return Err(Failure::Invalid);
        }
        if matches!(request.action, Action::Bookmark)
            && request.get("address").is_none_or(|value| {
                value.len() != bookmarks::ADDRESS_LENGTH
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(Failure::Unavailable);
        }
        for name in [
            "view",
            "timeline",
            "moment",
            "viewpoint",
            "scene",
            "cursor",
            "record",
            "root",
            "calendar",
            "map",
            "mapCursor",
        ] {
            if request.get(name).is_some_and(|value| {
                value.len() != 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }) {
                return Err(Failure::Unavailable);
            }
        }
        if request
            .get("q")
            .is_some_and(|q| q.len() > 256 || q.chars().any(char::is_control))
        {
            return Err(Failure::Invalid);
        }
        Ok(request)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(String::as_str)
    }

    fn required(&self, key: &str) -> Result<&str> {
        self.get(key).ok_or(Failure::Invalid)
    }

    fn limit(&self) -> Result<usize> {
        match self.get("limit") {
            None => Ok(24),
            Some(limit) => limit
                .parse::<usize>()
                .ok()
                .filter(|limit| (1..=48).contains(limit))
                .ok_or(Failure::Invalid),
        }
    }
}

#[derive(Clone)]
struct View {
    token: String,
    source: CompendiumWitness,
    lens: Lens,
}

/// A router owns a context registry and keyed selectors. Saved view handles
/// retain only weak source identity, so old browser tabs cannot pin a database.
pub(crate) struct Browser {
    secret: Option<[u8; 32]>,
    views: Mutex<VecDeque<View>>,
    authority: Authority,
    transient: Option<Keyring>,
}

impl Browser {
    pub(crate) fn new(authority: Authority) -> Self {
        let mut secret = [0; 32];
        Self {
            secret: getrandom::fill(&mut secret).ok().map(|_| secret),
            views: Mutex::new(VecDeque::new()),
            authority,
            transient: Keyring::transient().ok(),
        }
    }

    fn lookup(&self, token: &str, pin: &CompendiumPin) -> Result<View> {
        let mut views = self.views.lock().map_err(|_| Failure::Source)?;
        let index = views
            .iter()
            .position(|view| view.token == token)
            .ok_or(Failure::Unavailable)?;
        if !views[index].source.matches(pin) {
            return Err(Failure::Changed);
        }
        let view = views.remove(index).expect("view index exists");
        views.push_back(view.clone());
        Ok(view)
    }

    fn remember(&self, lens: Lens, pin: &CompendiumPin) -> Result<View> {
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|_| Failure::Source)?;
        let token = hex::encode(random);
        let mut views = self.views.lock().map_err(|_| Failure::Source)?;
        if views.len() == MAX_VIEWS {
            views.pop_front();
        }
        let view = View {
            token,
            source: pin.witness(),
            lens,
        };
        views.push_back(view.clone());
        Ok(view)
    }

    /// HMAC-SHA-256 over length-framed purpose and operands. Only the digest
    /// crosses the wire, so selectors cannot be decoded into source identities.
    fn handle(&self, view: &View, domain: &str, fields: &[&str]) -> Result<String> {
        let secret = self.secret.as_ref().ok_or(Failure::Source)?;
        let mut inner = [0x36; 64];
        let mut outer = [0x5c; 64];
        for (index, byte) in secret.iter().enumerate() {
            inner[index] ^= byte;
            outer[index] ^= byte;
        }
        let mut digest = Sha256::new();
        digest.update(inner);
        for field in [view.token.as_str(), domain]
            .into_iter()
            .chain(fields.iter().copied())
        {
            digest.update((field.len() as u64).to_be_bytes());
            digest.update(field.as_bytes());
        }
        Ok(hex::encode(
            Sha256::new()
                .chain_update(outer)
                .chain_update(digest.finalize())
                .finalize(),
        ))
    }

    pub(crate) fn execute(
        &self,
        service: &Service,
        source: CompendiumSource,
        request: Request,
    ) -> Result<Value> {
        if self.secret.is_none() {
            return Err(Failure::Source);
        }
        let authority = self.authority;
        let keyring = service
            .compendium_keyring()
            .or(self.transient.as_ref())
            .ok_or(Failure::Source)?;
        let address = request.get("address");
        let saved = address
            .map(|address| keyring.open(service.world(), authority, address))
            .transpose()?;
        let pin = service
            .pin_compendium_source(source)
            .map_err(|error| match error {
                _ if saved.is_some() => Failure::Unavailable,
                crate::Error::Conflict(_) if request.get("view").is_some() => Failure::Changed,
                _ => Failure::Source,
            })?;
        let world = pin.world();
        let result = match request.action {
            Action::Context => {
                let (lens, target) = if request.query.is_empty() {
                    (Lens::All, Target::Directory)
                } else {
                    let view = self.lookup(request.required("view")?, &pin)?;
                    let lens = choices::resolve(self, &pin, &view, &request)?;
                    let target = if request.get("section").is_some() {
                        Target::Map(None)
                    } else {
                        request
                            .get("record")
                            .map(|handle| directory::find_record(self, &pin, &view, handle))
                            .transpose()?
                            .flatten()
                            .map_or(Target::Directory, Target::Record)
                    };
                    (lens, target)
                };
                self.present_context(keyring, authority, &pin, lens, target, None)?
            }
            Action::Bookmark => {
                let (lens, target) = saved.ok_or(Failure::Unavailable)?.resolve(world)?;
                self.present_context(keyring, authority, &pin, lens, target, address)
                    .map_err(|_| Failure::Unavailable)?
            }
            Action::Choices => {
                let view = self.lookup(request.required("view")?, &pin)?;
                let routes = Issuer::new(keyring, world, authority, &view.lens)?;
                choices::read(self, &pin, &view, &request, &routes)?
            }
            Action::Page => {
                let view = self.lookup(request.required("view")?, &pin)?;
                let routes = Issuer::new(keyring, world, authority, &view.lens)?;
                directory::page(self, &pin, &view, &request, &routes)?
            }
            Action::Record => {
                let view = self.lookup(request.required("view")?, &pin)?;
                let routes = Issuer::new(keyring, world, authority, &view.lens)?;
                directory::record(self, &pin, &view, &request, &routes)?
            }
            Action::Graph => {
                let view = self.lookup(request.required("view")?, &pin)?;
                let routes = Issuer::new(keyring, world, authority, &view.lens)?;
                graph::read(self, &pin, &view, &request, &routes)?
            }
            Action::Map | Action::Locations => {
                let view = self.lookup(request.required("view")?, &pin)?;
                let routes = Issuer::new(keyring, world, authority, &view.lens)?;
                maps::read(self, &pin, &view, &request, &routes)?
            }
        };
        if serde_json::to_vec(&result)
            .map_err(|_| Failure::Source)?
            .len()
            > MAX_RESPONSE
        {
            return Err(Failure::Source);
        }
        Ok(result)
    }

    fn present_context(
        &self,
        keyring: &Keyring,
        authority: Authority,
        pin: &CompendiumPin,
        lens: Lens,
        mut target: Target<'_>,
        address: Option<&str>,
    ) -> Result<Value> {
        let world = pin.world();
        let scope = lens.scope(pin)?;
        if let Target::Map(Some(map)) = target
            && (scope.is_some()
                || !maps::is_map(map)
                || !directory::rows(world, &lens, None)?
                    .iter()
                    .any(|row| row.id == map.id))
        {
            return Err(Failure::Unavailable);
        }
        if let Target::Record(record) = target
            && !directory::rows(world, &lens, scope.as_ref())?
                .iter()
                .any(|row| row.id == record.id)
        {
            if address.is_some() {
                return Err(Failure::Unavailable);
            }
            target = Target::Directory;
        }
        // Restore does not emit any labels until the complete lens and its
        // destination have both been re-admitted on this one generation.
        let labels = choices::context_labels(world, &lens, scope.as_ref())?;
        let capabilities = Capabilities {
            workbench: lens.mode() != Mode::Character,
            maps: lens.mode() != Mode::Character,
        };
        let routes = Issuer::new(keyring, world, authority, &lens)?;
        let directory = if matches!(lens, Lens::All) {
            "/compendium".to_owned()
        } else {
            routes.path(Target::Directory)?
        };
        let graph = routes.path(Target::Graph)?;
        let map = routes.path(Target::Map(None))?;
        let href = match address {
            Some(address) => format!("{}{address}", bookmarks::PREFIX),
            None => match target {
                Target::Directory => directory.clone(),
                Target::Graph => graph.clone(),
                Target::Map(None) => map.clone(),
                target => routes.path(target)?,
            },
        };
        let view = self.remember(lens, pin)?;
        let destination = match target {
            Target::Directory => json!({"kind":"directory"}),
            Target::Graph => json!({"kind":"graph"}),
            Target::Map(selected) => json!({"kind":"map","map":selected
                .map(|record| maps::map_handle(self, &view, &record.id))
                .transpose()?}),
            Target::Record(record) => {
                json!({"kind":"record","handle":directory::record_handle(self, &view, &record.id)?})
            }
        };
        Ok(json!({
            "viewToken":view.token, "labels":labels, "capabilities":capabilities,
            "href":href, "destination":destination,
            "routes":{"directory":directory,"graph":graph,"map":map,"durable":keyring.durable()},
        }))
    }
}

#[derive(Serialize)]
struct Capabilities {
    workbench: bool,
    maps: bool,
}

fn fold(value: &str) -> String {
    crate::reader::casefold::fold(value)
}

/// Render authored text as text, removing machine references by syntax. This
/// check cannot depend on whether a withheld record exists in this generation.
fn machine_reference(token: &str) -> bool {
    let separators = [':', '/', '.', '\\', '-'];
    if token.split(separators).any(|word| {
        let digest = (word.len() == 40 || word.len() == 64)
            && word.bytes().all(|byte| byte.is_ascii_hexdigit());
        digest
            || (word.len() <= 64 && word.contains('_') && crate::validation::valid_record_id(word))
    }) {
        return true;
    }
    if token
        .split(':')
        .any(|part| part.starts_with("story/") || part.starts_with("story\\"))
        || token.contains("/story/")
        || token.contains("\\story\\")
    {
        return true;
    }
    // Spatial IDs retain their own colon and slash separators. Test the typed
    // kind and its path together, even when the reference follows a prefix.
    token.match_indices(':').any(|(colon, _)| {
        let start = token[..colon]
            .rfind(separators)
            .map_or(0, |index| index + 1);
        if !matches!(
            &token[start..colon],
            "location" | "map" | "anchor" | "portal" | "route" | "overlay"
        ) {
            return false;
        }
        let end = token[colon + 1..]
            .find([':', '.', '\\'])
            .map_or(token.len(), |index| colon + 1 + index);
        crate::validation::valid_record_id(&token[start..end])
    })
}

fn display(value: &str, limit: usize) -> String {
    let mut text = String::new();
    let mut start = 0;
    let mut remaining = limit;
    for (at, ch) in value
        .char_indices()
        .chain(std::iter::once((value.len(), ' ')))
    {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '/' | '.' | '\\') {
            continue;
        }
        let token = &value[start..at];
        let core = token.trim_end_matches('.');
        let parts = if machine_reference(core) {
            ["Reference", &token[core.len()..]]
        } else {
            [token, ""]
        };
        for character in parts.into_iter().flat_map(str::chars).take(remaining) {
            text.push(character);
            remaining -= 1;
        }
        if remaining > 0 && at != value.len() && (!ch.is_control() || ch == '\n' || ch == '\t') {
            text.push(ch);
            remaining -= 1;
        }
        start = at + ch.len_utf8();
        if remaining == 0 {
            break;
        }
    }
    text
}

fn page_start<T>(
    items: &[T],
    cursor: Option<&str>,
    handle: impl Fn(&T) -> Result<String>,
) -> Result<usize> {
    let Some(cursor) = cursor else { return Ok(0) };
    for (index, item) in items.iter().enumerate() {
        if handle(item)? == cursor {
            return Ok(index + 1);
        }
    }
    Err(Failure::Unavailable)
}
