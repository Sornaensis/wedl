//! Isolated reader presentation over one explicitly installed committed edition.
//!
//! Requests select within trusted policy; they never carry grants or secrets.
//! No policy means uniform refusal before any repository/source access. Public
//! and author-simulation installations are separate and revocable in flight.
use crate::{
    Error, Result, historical, manuscript,
    model::StoryTime,
    reveal::{self, CanonicalScope, Lens, ReaderGrant, ReaderItem},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

mod browser;
pub(crate) mod casefold;
mod transport;
pub use browser::PublicLabels;
pub use transport::{router, serve};

pub const PROTOCOL: &str = "wedl-reader-presentation/v1";
pub const MAX_REQUEST_BYTES: usize = 16_384;
pub const MAX_OCCURRENCES: usize = 1024;
pub const MAX_ITEMS: usize = 1024;
pub const MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SCOPE_BYTES: usize = 1024 * 1024;

/// Captured by a trusted operator from literal Git storage, never wire-decoded.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct RepositoryIdentity([String; 3]);
pub fn repository_identity(root: &Path) -> Result<RepositoryIdentity> {
    historical::repository_identity(root).map(RepositoryIdentity)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextPolicy {
    None,
    AllAtPinnedPlan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Page,
    Search,
}

/// Trusted installation data. Deliberately has no Deserialize or Debug: neither
/// source/request JSON nor accidentally printed diagnostics disclose its secret.
#[derive(Clone)]
pub struct PublicPolicy {
    pub repository: RepositoryIdentity,
    pub revision: String,
    pub work_id: String,
    pub plan_id: String,
    pub canonical_scope: CanonicalScope,
    pub generation: String,
    pub max_occurrence: usize,
    pub cursor_secret: [u8; 32],
    pub publication_id: String,
    pub at: StoryTime,
    pub text_policy: TextPolicy,
}
#[derive(Clone)]
pub struct SimulationPolicy {
    pub repository: RepositoryIdentity,
    pub revision: String,
    pub work_id: String,
    pub plan_id: String,
    pub canonical_scope: CanonicalScope,
    pub generation: String,
    pub max_occurrence: usize,
    pub cursor_secret: [u8; 32],
    pub principal_id: String,
    pub author_permissions: CanonicalScope,
}

#[derive(Default)]
struct Installed {
    public: Option<Arc<PublicPolicy>>,
    public_labels: Arc<PublicLabels>,
    simulation: Option<Arc<SimulationPolicy>>,
}
struct Inner {
    root: Option<PathBuf>,
    installed: RwLock<Installed>,
}
/// Clones share one installed/revocable context; construction does no I/O.
#[derive(Clone)]
pub struct PresentationService(Arc<Inner>);
impl Default for PresentationService {
    fn default() -> Self {
        Self::new(None)
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Reader,
    Preview,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::Preview => "author-preview",
        }
    }
}
#[derive(Clone, Copy)]
enum Failure {
    Unavailable,
    Invalid,
    Limit,
}
fn failure(mode: Mode, failure: Failure) -> Json {
    let (outcome, code, message) = match failure {
        Failure::Unavailable => (
            "unavailable",
            "READER-PRESENTATION-UNAVAILABLE",
            "Reader presentation unavailable.",
        ),
        Failure::Invalid => (
            "invalid",
            "READER-PRESENTATION-SOURCE",
            "Invalid reader presentation source.",
        ),
        Failure::Limit => (
            "limit",
            "READER-PRESENTATION-LIMIT",
            "Reader presentation limit exceeded.",
        ),
    };
    json!({"protocol":PROTOCOL,"mode":mode.label(),"outcome":outcome,"code":code,"message":message})
}
fn invalid_request() -> Error {
    Error::InvalidInput("Invalid reader presentation request.".into())
}
fn invalid_policy() -> Error {
    Error::InvalidInput("Invalid trusted reader presentation policy.".into())
}
fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= maximum
}
fn identifier(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|tail| tail.strip_prefix('_'))
        .is_some_and(|tail| {
            tail.len() == 26
                && tail
                    .bytes()
                    .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
        })
}
fn handle(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn valid_time(at: &StoryTime) -> bool {
    text(&at.timeline, 256)
}

fn required_nullable<'de, D, T>(decoder: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(decoder)
}
fn present_string<'de, D>(decoder: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(decoder).map(Some)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PublicRequest {
    protocol: String,
    publication_id: String,
    revision: String,
    occurrence: usize,
    limit: usize,
    #[serde(deserialize_with = "required_nullable")]
    cursor: Option<String>,
    // Missing is distinct from null: Page forbids the field; Search requires it.
    #[serde(default, deserialize_with = "present_string")]
    query: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTime {
    timeline: String,
    tick: String,
    order: String,
}
impl WireTime {
    fn resolve(self) -> Result<StoryTime> {
        let tick = self.tick.parse::<i64>().map_err(|_| invalid_request())?;
        let order = self.order.parse::<i32>().map_err(|_| invalid_request())?;
        if tick.to_string() != self.tick
            || order.to_string() != self.order
            || !text(&self.timeline, 256)
        {
            return Err(invalid_request());
        }
        Ok(StoryTime {
            timeline: self.timeline,
            tick,
            order,
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreviewRequest {
    protocol: String,
    revision: String,
    work_id: String,
    plan_id: String,
    occurrence: usize,
    operation: Operation,
    at: WireTime,
    text_policy: TextPolicy,
    limit: usize,
    #[serde(deserialize_with = "required_nullable")]
    cursor: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    query: Option<String>,
}
struct Cursor {
    offset: usize,
    mac: [u8; 32],
}
struct Selection {
    revision: String,
    publication_id: Option<String>,
    work_id: Option<String>,
    plan_id: Option<String>,
    occurrence: usize,
    operation: Operation,
    at: Option<StoryTime>,
    text_policy: Option<TextPolicy>,
    limit: usize,
    cursor: Option<Cursor>,
    query: Option<String>,
}
fn decode_cursor(value: Option<String>) -> Result<Option<Cursor>> {
    value
        .map(|value| {
            if value.len() != 72
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid_request());
            }
            let bytes = hex::decode(value).map_err(|_| invalid_request())?;
            let offset =
                u32::from_be_bytes(bytes[..4].try_into().map_err(|_| invalid_request())?) as usize;
            if offset > MAX_ITEMS {
                return Err(invalid_request());
            }
            Ok(Cursor {
                offset,
                mac: bytes[4..].try_into().map_err(|_| invalid_request())?,
            })
        })
        .transpose()
}
fn decode(raw: &[u8], operation: Operation, mode: Mode) -> Result<Selection> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid_request());
    }
    let selection = match mode {
        Mode::Reader => {
            let input: PublicRequest =
                serde_json::from_slice(raw).map_err(|_| invalid_request())?;
            if input.protocol != PROTOCOL || !handle(&input.publication_id) {
                return Err(invalid_request());
            }
            Selection {
                revision: input.revision,
                publication_id: Some(input.publication_id),
                work_id: None,
                plan_id: None,
                occurrence: input.occurrence,
                operation,
                at: None,
                text_policy: None,
                limit: input.limit,
                cursor: decode_cursor(input.cursor)?,
                query: input.query,
            }
        }
        Mode::Preview => {
            let input: PreviewRequest =
                serde_json::from_slice(raw).map_err(|_| invalid_request())?;
            if input.protocol != PROTOCOL
                || !identifier(&input.work_id, "work")
                || !identifier(&input.plan_id, "reveal")
            {
                return Err(invalid_request());
            }
            Selection {
                revision: input.revision,
                publication_id: None,
                work_id: Some(input.work_id),
                plan_id: Some(input.plan_id),
                occurrence: input.occurrence,
                operation: input.operation,
                at: Some(input.at.resolve()?),
                text_policy: Some(input.text_policy),
                limit: input.limit,
                cursor: decode_cursor(input.cursor)?,
                query: input.query,
            }
        }
    };
    if !historical::valid_oid(&selection.revision)
        || selection.occurrence >= MAX_OCCURRENCES
        || !(1..=100).contains(&selection.limit)
        || (selection.operation == Operation::Search) != selection.query.is_some()
        || selection
            .query
            .as_ref()
            .is_some_and(|query| !text(query, 256) || query.len() > 1024)
    {
        return Err(invalid_request());
    }
    Ok(selection)
}

#[derive(Clone)]
enum CapturedPolicy {
    Public(Arc<PublicPolicy>),
    Simulation(Arc<SimulationPolicy>),
}
struct Binding<'a> {
    repository: &'a RepositoryIdentity,
    revision: &'a str,
    work_id: &'a str,
    plan_id: &'a str,
    scope: &'a CanonicalScope,
    generation: &'a str,
    ceiling: usize,
    secret: &'a [u8; 32],
}
impl CapturedPolicy {
    fn binding(&self) -> Binding<'_> {
        match self {
            Self::Public(p) => Binding {
                repository: &p.repository,
                revision: &p.revision,
                work_id: &p.work_id,
                plan_id: &p.plan_id,
                scope: &p.canonical_scope,
                generation: &p.generation,
                ceiling: p.max_occurrence,
                secret: &p.cursor_secret,
            },
            Self::Simulation(p) => Binding {
                repository: &p.repository,
                revision: &p.revision,
                work_id: &p.work_id,
                plan_id: &p.plan_id,
                scope: &p.canonical_scope,
                generation: &p.generation,
                ceiling: p.max_occurrence,
                secret: &p.cursor_secret,
            },
        }
    }
    fn admits(&self, request: &Selection) -> bool {
        let b = self.binding();
        request.revision == b.revision
            && request.occurrence <= b.ceiling
            && match self {
                Self::Public(p) => {
                    request.publication_id.as_deref() == Some(p.publication_id.as_str())
                }
                Self::Simulation(_) => {
                    request.work_id.as_deref() == Some(b.work_id)
                        && request.plan_id.as_deref() == Some(b.plan_id)
                }
            }
    }
}
pub(crate) struct Prepared {
    mode: Mode,
    policy: Option<CapturedPolicy>,
    value: Json,
}

#[derive(Debug)]
enum ProjectionError {
    Unavailable,
    Invalid,
    Limit,
}
impl From<Error> for ProjectionError {
    fn from(_: Error) -> Self {
        Self::Invalid
    }
}
impl From<historical::ReaderSourceError> for ProjectionError {
    fn from(error: historical::ReaderSourceError) -> Self {
        match error {
            historical::ReaderSourceError::Limit => Self::Limit,
            historical::ReaderSourceError::Source(_error) => Self::Invalid,
        }
    }
}

impl PresentationService {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self(Arc::new(Inner {
            root,
            installed: RwLock::new(Installed::default()),
        }))
    }
    fn validate_binding(&self, binding: &Binding<'_>) -> Result<()> {
        if !historical::valid_oid(binding.revision)
            || !identifier(binding.work_id, "work")
            || !identifier(binding.plan_id, "reveal")
            || !text(binding.generation, 128)
            || binding.ceiling >= MAX_OCCURRENCES
            || binding.scope.validate().is_err()
            || bounded_bytes(binding.scope, MAX_SCOPE_BYTES).is_err()
        {
            return Err(invalid_policy());
        }
        let root = self.0.root.as_deref().ok_or_else(invalid_policy)?;
        if repository_identity(root)? != *binding.repository {
            return Err(invalid_policy());
        }
        Ok(())
    }
    pub fn install_public(&self, policy: Option<PublicPolicy>) -> Result<()> {
        self.install_public_with_labels(policy, PublicLabels::default())
    }
    /// Install public presentation labels with the policy whose immutable work
    /// and reveal plan they describe. Labels never grant access on their own.
    pub fn install_public_with_labels(
        &self,
        policy: Option<PublicPolicy>,
        labels: PublicLabels,
    ) -> Result<()> {
        labels.validate()?;
        let policy = policy.map(Arc::new);
        if let Some(policy) = &policy {
            if !handle(&policy.publication_id) || !valid_time(&policy.at) {
                return Err(invalid_policy());
            }
            self.validate_binding(&CapturedPolicy::Public(policy.clone()).binding())?;
        }
        let mut installed = self.0.installed.write().map_err(|_| invalid_policy())?;
        installed.public_labels = if policy.is_some() {
            Arc::new(labels)
        } else {
            Arc::default()
        };
        installed.public = policy;
        Ok(())
    }
    pub fn install_simulation(&self, policy: Option<SimulationPolicy>) -> Result<()> {
        let policy = policy.map(Arc::new);
        if let Some(policy) = &policy {
            if !text(&policy.principal_id, 256)
                || policy.author_permissions.validate().is_err()
                || policy.author_permissions.world_id != policy.canonical_scope.world_id
                || bounded_bytes(&policy.author_permissions, MAX_SCOPE_BYTES).is_err()
            {
                return Err(invalid_policy());
            }
            self.validate_binding(&CapturedPolicy::Simulation(policy.clone()).binding())?;
        }
        self.0
            .installed
            .write()
            .map_err(|_| invalid_policy())?
            .simulation = policy;
        Ok(())
    }
    /// Every read checks complete wire syntax before choosing the installed
    /// issuer, and verifies that installation again immediately before returning.
    pub fn execute(&self, raw: &[u8], operation: Operation) -> Result<Json> {
        self.prepare(raw, operation, Mode::Reader)
            .map(|prepared| self.emit(prepared))
    }
    pub fn preview(&self, raw: &[u8]) -> Result<Json> {
        self.prepare_preview(raw)
            .map(|prepared| self.emit(prepared))
    }
    pub(crate) fn prepare_preview(&self, raw: &[u8]) -> Result<Prepared> {
        self.prepare(raw, Operation::Page, Mode::Preview)
    }
    fn prepare(&self, raw: &[u8], operation: Operation, mode: Mode) -> Result<Prepared> {
        self.prepare_using(raw, operation, mode, |request, policy| {
            self.project(request, policy, mode)
        })
    }
    fn prepare_using(
        &self,
        raw: &[u8],
        operation: Operation,
        mode: Mode,
        project: impl FnOnce(&Selection, &CapturedPolicy) -> std::result::Result<Json, ProjectionError>,
    ) -> Result<Prepared> {
        let request = decode(raw, operation, mode)?;
        let policy = self
            .0
            .installed
            .read()
            .ok()
            .and_then(|installed| match mode {
                Mode::Reader => installed.public.clone().map(CapturedPolicy::Public),
                Mode::Preview => installed.simulation.clone().map(CapturedPolicy::Simulation),
            });
        let Some(policy) = policy.filter(|policy| policy.admits(&request)) else {
            return Ok(Prepared {
                mode,
                policy: None,
                value: failure(mode, Failure::Unavailable),
            });
        };
        let value = project(&request, &policy).unwrap_or_else(|error| {
            failure(
                mode,
                match error {
                    ProjectionError::Limit => Failure::Limit,
                    ProjectionError::Unavailable => Failure::Unavailable,
                    ProjectionError::Invalid => match mode {
                        Mode::Reader => Failure::Unavailable,
                        Mode::Preview => Failure::Invalid,
                    },
                },
            )
        });
        Ok(Prepared {
            mode,
            policy: Some(policy),
            value,
        })
    }
    pub(crate) fn emit(&self, prepared: Prepared) -> Json {
        let Some(policy) = &prepared.policy else {
            return prepared.value;
        };
        let current = self.0.installed.read().is_ok_and(|installed| match policy {
            CapturedPolicy::Public(old) => installed
                .public
                .as_ref()
                .is_some_and(|new| Arc::ptr_eq(old, new)),
            CapturedPolicy::Simulation(old) => installed
                .simulation
                .as_ref()
                .is_some_and(|new| Arc::ptr_eq(old, new)),
        });
        if current {
            prepared.value
        } else {
            failure(prepared.mode, Failure::Unavailable)
        }
    }
    fn project(
        &self,
        request: &Selection,
        policy: &CapturedPolicy,
        mode: Mode,
    ) -> std::result::Result<Json, ProjectionError> {
        let b = policy.binding();
        let root = self.0.root.as_deref().ok_or(ProjectionError::Unavailable)?;
        if repository_identity(root).ok().as_ref() != Some(b.repository) {
            return Err(ProjectionError::Unavailable);
        }
        let result = self.project_source(root, request, policy, mode);
        // Routing/storage changes take precedence even over authorized source
        // diagnostics. Policy replacement/revocation is checked separately at emit.
        if repository_identity(root).ok().as_ref() != Some(b.repository) {
            return Err(ProjectionError::Unavailable);
        }
        result
    }
    fn project_source(
        &self,
        root: &Path,
        request: &Selection,
        policy: &CapturedPolicy,
        mode: Mode,
    ) -> std::result::Result<Json, ProjectionError> {
        let source = ReaderSource::load(root, &policy.binding(), Some(request.occurrence))?;
        Self::project_loaded(&source, request, policy, mode, None)
    }
    fn project_loaded(
        source: &ReaderSource,
        request: &Selection,
        policy: &CapturedPolicy,
        mode: Mode,
        browser_binding: Option<&[u8; 32]>,
    ) -> std::result::Result<Json, ProjectionError> {
        let b = policy.binding();
        let ReaderSource {
            world,
            plan,
            occurrence_count,
            work_blob,
            plan_blob,
        } = source;
        if request.occurrence >= *occurrence_count {
            return Err(ProjectionError::Unavailable);
        }
        let (scope, at, text_policy, principal) = match policy {
            CapturedPolicy::Public(p) => (
                p.canonical_scope.clone(),
                p.at.clone(),
                p.text_policy,
                "anonymous-public",
            ),
            CapturedPolicy::Simulation(p) => (
                p.canonical_scope.intersect(&p.author_permissions)?,
                request.at.clone().ok_or(ProjectionError::Unavailable)?,
                request.text_policy.ok_or(ProjectionError::Unavailable)?,
                p.principal_id.as_str(),
            ),
        };
        check_horizon(world, &at)?;
        let grant = ReaderGrant {
            reader_id: principal.into(),
            plan_id: Some(b.plan_id.into()),
            revision: b.revision.into(),
            canonical_scope: scope.clone(),
            at: at.clone(),
            lens: Lens::AuthorAsOf,
            audiences: ["public".into()].into(),
            perspectives: ["ordinary".into()].into(),
        };
        let items = reveal::project_resolved(world, plan, request.occurrence, principal, &grant)?
            .into_iter()
            .filter(|item| item.kind != "text" || text_policy == TextPolicy::AllAtPinnedPlan)
            .collect::<Vec<_>>();
        let mut context = json!({"protocol":PROTOCOL,"mode":mode.label(),"operation":request.operation,
            "repository":b.repository,"revision":b.revision,"work":b.work_id,"plan":b.plan_id,
            "workBlob":work_blob,"planBlob":plan_blob,"principal":principal,
            "scope":scope,"policyScope":b.scope,"at":at,"generation":b.generation,"textPolicy":text_policy,
            "ceiling":b.ceiling,"occurrence":request.occurrence,"limit":request.limit});
        match policy {
            CapturedPolicy::Public(p) => context["publication"] = p.publication_id.clone().into(),
            CapturedPolicy::Simulation(p) => {
                context["permissions"] = serde_json::to_value(&p.author_permissions)
                    .map_err(|_| ProjectionError::Invalid)?
            }
        }
        if let Some(binding) = browser_binding {
            context["browser"] = hex::encode(binding).into();
        }
        paginated(
            items,
            *occurrence_count,
            b.ceiling,
            &at,
            b.secret,
            context,
            request,
            mode,
        )
    }
}

fn check_horizon(
    world: &crate::model::World,
    at: &StoryTime,
) -> std::result::Result<(), ProjectionError> {
    world
        .story_time(at.tick, Some(&at.timeline), at.order)
        .map(|_| ())
        .map_err(|_| ProjectionError::Unavailable)
}

struct ReaderSource {
    world: crate::model::World,
    plan: manuscript::RevealPlan,
    occurrence_count: usize,
    work_blob: Option<String>,
    plan_blob: Option<String>,
}
impl ReaderSource {
    fn load(
        root: &Path,
        b: &Binding<'_>,
        occurrence: Option<usize>,
    ) -> std::result::Result<Self, ProjectionError> {
        historical::preflight_reader_sources(root, b.revision)?;
        let world = historical::load_world_with_file_limit(root, b.revision, 2 * 1024 * 1024)?;
        let authors = manuscript::snapshot(root, Some(b.revision))?;
        let plan_record = authors
            .documents
            .get(b.plan_id)
            .ok_or(ProjectionError::Unavailable)?;
        if plan_record.kind != "reveal-plan"
            || plan_record.frontmatter["work"]["id"].as_str() != Some(b.work_id)
            || world.world_record()?.id != b.scope.world_id
        {
            return Err(ProjectionError::Unavailable);
        }
        let work_record = authors
            .documents
            .get(b.work_id)
            .filter(|record| record.kind == "work")
            .ok_or(ProjectionError::Unavailable)?;
        let occurrence_count = work_record
            .list("chapters")
            .ok_or(ProjectionError::Invalid)?
            .iter()
            .map(|chapter| chapter["segments"].as_sequence().map_or(0, Vec::len))
            .sum::<usize>();
        if occurrence_count > MAX_OCCURRENCES {
            return Err(ProjectionError::Limit);
        }
        if occurrence.is_some_and(|occurrence| occurrence >= occurrence_count) {
            return Err(ProjectionError::Unavailable);
        }
        let plan = manuscript::resolve_reveal_plan(&world, &authors, b.plan_id, MAX_OCCURRENCES)?;
        if plan.work_id != b.work_id {
            return Err(ProjectionError::Unavailable);
        }
        Ok(Self {
            world,
            plan,
            occurrence_count,
            work_blob: work_record.blob_oid.clone(),
            plan_blob: plan_record.blob_oid.clone(),
        })
    }
}

/// A complete output limit is checked while serializing, before an oversized
/// JSON string is allocated. Sources and projection are separately bounded.
fn bounded_bytes<T: Serialize>(
    value: &T,
    maximum: usize,
) -> std::result::Result<Vec<u8>, ProjectionError> {
    struct Bounded {
        bytes: Vec<u8>,
        maximum: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::FileTooLarge,
                    "reader response bound",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| ProjectionError::Limit)?;
    Ok(writer.bytes)
}
/// Source, item-count and Markdown budgets already bound this input. Hash the
/// complete admitted dataset incrementally so escaped JSON is not retained and
/// a search is judged against its complete selected response, as specified.
fn dataset_digest(items: &[ReaderItem]) -> std::result::Result<[u8; 32], ProjectionError> {
    struct HashWriter(Sha256);
    impl Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, items).map_err(|_| ProjectionError::Invalid)?;
    Ok(writer.0.finalize().into())
}
fn hmac_sha256(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for (i, byte) in block.iter().enumerate() {
        inner[i] ^= byte;
        outer[i] ^= byte;
    }
    let mut hash = Sha256::new();
    hash.update(inner);
    hash.update(value);
    let middle = hash.finalize();
    let mut hash = Sha256::new();
    hash.update(outer);
    hash.update(middle);
    hash.finalize().into()
}
fn equal_mac(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}
fn cursor_mac(secret: &[u8; 32], context: &[u8; 32], offset: usize, mode: Mode) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(128);
    bytes.extend_from_slice(PROTOCOL.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(mode.label().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(context);
    bytes.extend_from_slice(&(offset as u32).to_be_bytes());
    hmac_sha256(secret, &bytes)
}
fn encode_cursor(secret: &[u8; 32], context: &[u8; 32], offset: usize, mode: Mode) -> String {
    let mut bytes = Vec::with_capacity(36);
    bytes.extend_from_slice(&(offset as u32).to_be_bytes());
    bytes.extend_from_slice(&cursor_mac(secret, context, offset, mode));
    hex::encode(bytes)
}
fn paginated(
    mut items: Vec<ReaderItem>,
    occurrence_count: usize,
    ceiling: usize,
    at: &StoryTime,
    secret: &[u8; 32],
    mut context: Json,
    request: &Selection,
    mode: Mode,
) -> std::result::Result<Json, ProjectionError> {
    if items.len() > MAX_ITEMS
        || items
            .iter()
            .try_fold(0usize, |sum, item| sum.checked_add(item.markdown.len()))
            .is_none_or(|bytes| bytes > MAX_MARKDOWN_BYTES)
    {
        return Err(ProjectionError::Limit);
    }
    let dataset = dataset_digest(&items)?;
    context["dataset"] = hex::encode(dataset).into();
    if let Some(query) = &request.query {
        context["query"] = query.clone().into();
        let folded_query = casefold::fold(query);
        items.retain(|item| {
            casefold::fold(item.title.as_deref().unwrap_or("")).contains(&folded_query)
                || casefold::fold(&item.markdown).contains(&folded_query)
        });
    }
    let count = items.len();
    let context: [u8; 32] = Sha256::digest(bounded_bytes(&context, MAX_RESPONSE_BYTES)?).into();
    let offset = request.cursor.as_ref().map_or(0, |cursor| cursor.offset);
    if offset > count
        || request.cursor.as_ref().is_some_and(|cursor| {
            !equal_mac(&cursor.mac, &cursor_mac(secret, &context, offset, mode))
        })
    {
        return Err(ProjectionError::Unavailable);
    }
    let end = (offset + request.limit).min(count);
    let next_cursor = (end < count).then(|| encode_cursor(secret, &context, end, mode));
    let mut response = json!({"protocol":PROTOCOL,"mode":mode.label(),"operation":request.operation,
        "outcome":"available","code":"READER-PRESENTATION-OK","revision":request.revision,
        "occurrence":request.occurrence,"at":at,
        "navigation":{"previousOccurrence":request.occurrence.checked_sub(1),
            "nextOccurrence":(request.occurrence+1 < occurrence_count.min(ceiling+1)).then_some(request.occurrence+1)},
        "items":items,"count":count,"nextCursor":next_cursor});
    // Check all matching admitted units, not only this page; an exact small
    // limit never hides a complete-response overflow.
    bounded_bytes(&response, MAX_RESPONSE_BYTES)?;
    response["items"] = response["items"].as_array().expect("reader units array")[offset..end]
        .to_vec()
        .into();
    Ok(response)
}

#[cfg(test)]
mod tests;
