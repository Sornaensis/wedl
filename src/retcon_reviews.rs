//! Saved private retcon reviews bind one original committed source and one exact
//! consequence plan. Dispositions never approve or publish canonical source.
//!
//! Currentness compares admitted inputs in one bounded current worktree capture
//! and the complete original first-parent chain. A committed change then revert
//! stays stale. Uncommitted changes have no retained history. Original analysis
//! and historical private bytes are never refreshed by queue/detail reads.
use crate::{
    Error, Result, action_drafts, consequence_semantics::Policy, consequences, historical,
    manuscript, model::World, retcon_impact, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

mod codec;
mod store;
pub use store::{detail, queue, save, save_preview};

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SAVE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_TITLE_BYTES: usize = 1024;
const MAX_BODY_BYTES: usize = 128 * 1024;
const MAX_NOTE_BYTES: usize = 16 * 1024;
const MAX_HISTORY: usize = 256;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn binding() -> Error {
    invalid("REVIEW-BINDING-001: review analysis differs from its exact original capture")
}
fn nullable_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::deserialize(d)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    pub base_revision: String,
    pub plan: consequences::Plan,
    pub impact_request: retcon_impact::Request,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Analysis {
    pub world_id: String,
    pub base_revision: String,
    pub plan: consequences::Plan,
    pub impact_request: retcon_impact::Request,
    pub plan_sha256: String,
    pub report: Json,
    pub report_sha256: String,
    pub item_digests: Vec<String>,
    pub witness: Json,
    pub witness_sha256: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Disposition {
    Reviewed,
    NeedsRevision,
    IntentionallyRetained,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub item_digest: String,
    pub disposition: Disposition,
    pub note: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub analysis: Analysis,
    pub decisions: Vec<Decision>,
    pub body_markdown: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRequest {
    pub expected_fingerprint: String,
    #[serde(deserialize_with = "nullable_string")]
    pub expected_sha256: Option<String>,
    pub document: Document,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueueRequest {
    #[serde(deserialize_with = "nullable_string")]
    pub revision: Option<String>,
    pub max_items: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetailRequest {
    #[serde(deserialize_with = "nullable_string")]
    pub revision: Option<String>,
    pub id: String,
    #[serde(deserialize_with = "nullable_string")]
    pub expected_sha256: Option<String>,
}

pub fn valid_review_id(id: &str) -> bool {
    id.strip_prefix("review_").is_some_and(|suffix| {
        suffix.len() == 26
            && suffix
                .bytes()
                .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
    })
}
fn path(id: &str) -> String {
    format!("author/retcon-reviews/{id}.json")
}
fn check_hash(value: &str) -> Result<()> {
    if !action_drafts::valid_hash(value) {
        return Err(invalid("retcon review pins require lowercase SHA-256"));
    }
    Ok(())
}
fn check_revision(revision: &str) -> Result<()> {
    if !historical::valid_oid(revision) {
        return Err(invalid(
            "retcon review revisions require literal full lowercase commit OIDs",
        ));
    }
    Ok(())
}
fn validate_preview(request: &PreviewRequest) -> Result<()> {
    check_revision(&request.base_revision)?;
    consequences::validate_plan(&request.plan)?;
    retcon_impact::validate_request(&request.impact_request)?;
    codec::bytes(request, 524_288)?;
    Ok(())
}
fn validate_save(request: &SaveRequest) -> Result<()> {
    check_hash(&request.expected_fingerprint)?;
    if let Some(hash) = &request.expected_sha256 {
        check_hash(hash)?;
    }
    validate_document(&request.document)?;
    codec::bytes(request, MAX_SAVE_BYTES)?;
    Ok(())
}
fn validate_queue(request: &QueueRequest) -> Result<()> {
    if let Some(revision) = &request.revision {
        check_revision(revision)?;
    }
    if !(1..=1000).contains(&request.max_items) {
        return Err(invalid("retcon review queue maxItems must be 1..1000"));
    }
    Ok(())
}
fn validate_detail(request: &DetailRequest) -> Result<()> {
    if !valid_review_id(&request.id) {
        return Err(invalid("retcon review detail requires an exact review ID"));
    }
    if let Some(revision) = &request.revision {
        check_revision(revision)?;
    }
    if let Some(hash) = &request.expected_sha256 {
        check_hash(hash)?;
    }
    Ok(())
}
pub fn decode_preview(raw: &str) -> Result<PreviewRequest> {
    let request = codec::decode(raw, 524_288)?;
    validate_preview(&request)?;
    Ok(request)
}
pub fn decode_save(raw: &str) -> Result<SaveRequest> {
    let request = codec::decode(raw, MAX_SAVE_BYTES)?;
    validate_save(&request)?;
    Ok(request)
}
pub fn decode_queue(raw: &str) -> Result<QueueRequest> {
    let request = codec::decode(raw, 16_384)?;
    validate_queue(&request)?;
    Ok(request)
}
pub fn decode_detail(raw: &str) -> Result<DetailRequest> {
    let request = codec::decode(raw, 16_384)?;
    validate_detail(&request)?;
    Ok(request)
}
fn digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let bytes = codec::bytes(&serde_json::to_value(value)?, MAX_DOCUMENT_BYTES)?;
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(bytes);
    Ok(hex::encode(hash.finalize()))
}
fn item_digests(report: &Json) -> Result<Vec<String>> {
    report
        .get("items")
        .and_then(Json::as_array)
        .ok_or_else(binding)?
        .iter()
        .map(|item| digest(b"wedl-retcon-review-item\0", item))
        .collect()
}
fn validate_document(document: &Document) -> Result<()> {
    if !valid_review_id(&document.id)
        || document.title.trim().is_empty()
        || document.title.len() > MAX_TITLE_BYTES
        || document.body_markdown.len() > MAX_BODY_BYTES
    {
        return Err(invalid(
            "retcon review ID, title or body exceeds its closed bounds",
        ));
    }
    let analysis = &document.analysis;
    validate_preview(&PreviewRequest {
        base_revision: analysis.base_revision.clone(),
        plan: analysis.plan.clone(),
        impact_request: analysis.impact_request.clone(),
    })?;
    if analysis.world_id.len() > 256 || analysis.world_id.trim().is_empty() {
        return Err(binding());
    }
    for hash in [
        &analysis.plan_sha256,
        &analysis.report_sha256,
        &analysis.witness_sha256,
    ] {
        check_hash(hash)?;
    }
    codec::report(&analysis.report, analysis)?;
    codec::witness(&analysis.witness, analysis)?;
    if digest(b"wedl-retcon-review-report\0", &analysis.report)? != analysis.report_sha256
        || digest(b"wedl-retcon-review-witness\0", &analysis.witness)? != analysis.witness_sha256
        || item_digests(&analysis.report)? != analysis.item_digests
        || analysis.item_digests.len() > 1000
    {
        return Err(binding());
    }
    let unique: BTreeSet<_> = analysis.item_digests.iter().collect();
    if unique.len() != analysis.item_digests.len() {
        return Err(binding());
    }
    let mut offset = 0;
    for decision in &document.decisions {
        if decision.note.len() > MAX_NOTE_BYTES {
            return Err(invalid("retcon review note exceeds 16 KiB"));
        }
        let Some(index) = analysis.item_digests[offset..]
            .iter()
            .position(|item| item == &decision.item_digest)
        else {
            return Err(binding());
        };
        offset += index + 1;
    }
    codec::bytes(document, MAX_DOCUMENT_BYTES)?;
    Ok(())
}
fn decode_document(raw: &str) -> Result<Document> {
    let document = codec::decode(raw, MAX_DOCUMENT_BYTES)?;
    validate_document(&document)?;
    Ok(document)
}

struct Original {
    analysis: Analysis,
    seeds: Vec<retcon_impact::Reference>,
    hints: std::collections::BTreeMap<retcon_impact::Reference, String>,
    authors: manuscript::Snapshot,
}
fn original(root: &Path, request: &PreviewRequest) -> Result<Original> {
    validate_preview(request)?;
    let base = historical::load_world(root, &request.base_revision)?;
    let authors = manuscript::snapshot(root, Some(&request.base_revision))?;
    original_from_base(&base, authors, request)
}
fn original_from_base(
    base: &World,
    authors: manuscript::Snapshot,
    request: &PreviewRequest,
) -> Result<Original> {
    retcon_impact::preflight(base)?;
    let prepared = consequences::prepare(base, &request.plan, consequences::MAX_CHARACTERS)?;
    if prepared.report["validation"]["valid"] != true {
        return Err(Error::Validation(
            "retcon review requires a valid complete candidate".into(),
        ));
    }
    if request
        .plan
        .analysis
        .as_ref()
        .is_some_and(|analysis| analysis.policy == Policy::Required)
        && prepared.report["semanticAnalysis"]["applyAllowed"] != true
    {
        return Err(Error::Conflict(
            "required consequence checks do not permit a saved retcon review".into(),
        ));
    }
    let plan_sha256 = prepared.report["planSha256"]
        .as_str()
        .ok_or_else(binding)?
        .to_owned();
    let touched: Vec<_> = prepared.report["operations"]
        .as_array()
        .ok_or_else(binding)?
        .iter()
        .map(|item| {
            item["entityId"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(binding)
        })
        .collect::<Result<_>>()?;
    let captured = retcon_impact::capture_with_authors(
        base,
        &prepared.candidate,
        &authors,
        &request.impact_request,
        &touched,
        &plan_sha256,
    )?;
    let analysis = Analysis {
        world_id: base.world_record()?.id.clone(),
        base_revision: request.base_revision.clone(),
        plan: request.plan.clone(),
        impact_request: request.impact_request.clone(),
        plan_sha256,
        report_sha256: digest(b"wedl-retcon-review-report\0", &captured.report)?,
        item_digests: item_digests(&captured.report)?,
        report: captured.report,
        witness_sha256: digest(b"wedl-retcon-review-witness\0", &captured.witness)?,
        witness: captured.witness,
    };
    let unique: BTreeSet<_> = analysis.item_digests.iter().collect();
    if unique.len() != analysis.item_digests.len() {
        return Err(binding());
    }
    codec::bytes(&analysis, MAX_DOCUMENT_BYTES)?;
    Ok(Original {
        analysis,
        seeds: captured.seeds,
        hints: captured.path_hints,
        authors,
    })
}
fn verify_original(
    root: &Path,
    analysis: &Analysis,
    allow_unknown_witness: bool,
) -> Result<Original> {
    let base = historical::load_world(root, &analysis.base_revision)?;
    retcon_impact::preflight(&base)?;
    if !validation::validate_world(&base).valid {
        return Err(Error::Validation(
            "original review source is invalid or unavailable".into(),
        ));
    }
    if base.fingerprint != analysis.plan.expected_fingerprint
        || base.world_record()?.id != analysis.world_id
    {
        return Err(binding());
    }
    let authors = manuscript::snapshot(root, Some(&analysis.base_revision))?;
    // Private original corruption is unavailable evidence, not a forged
    // retained plan. Validate it before classifying reconstruction failures.
    manuscript::validate_manuscript_snapshot(&base, &authors)?;
    let captured = original_from_base(
        &base,
        authors,
        &PreviewRequest {
            base_revision: analysis.base_revision.clone(),
            plan: analysis.plan.clone(),
            impact_request: analysis.impact_request.clone(),
        },
    )
    .map_err(|error| {
        // With the exact valid original source available, an impossible retained
        // plan/target is a forged binding. Resource or unsupported-input failures
        // still prevent a comparison and remain unknown for read operations.
        match &error {
            Error::InvalidInput(message)
                if message.contains("exceeds") || message.contains("bound") =>
            {
                error
            }
            Error::Unsupported(_) => error,
            _ => binding(),
        }
    })?;
    let unsupported = analysis.witness["version"] != retcon_impact::WITNESS_VERSION;
    let mut retained = serde_json::to_value(analysis)?;
    let mut expected = serde_json::to_value(&captured.analysis)?;
    if unsupported && allow_unknown_witness {
        for value in [&mut retained, &mut expected] {
            value.as_object_mut().unwrap().remove("witness");
            value.as_object_mut().unwrap().remove("witnessSha256");
        }
    }
    if codec::bytes(&retained, MAX_DOCUMENT_BYTES)? != codec::bytes(&expected, MAX_DOCUMENT_BYTES)?
    {
        return Err(binding());
    }
    Ok(captured)
}

struct Comparison {
    revision: Option<String>,
    world: Option<World>,
    authors: Option<manuscript::Snapshot>,
}
impl Comparison {
    fn capture(root: &Path) -> Self {
        let revision = historical::head(root).ok();
        let stamp = crate::publication::read_stamp(root).ok();
        let mut world = historical::load_current_world(root).ok().filter(|world| {
            retcon_impact::preflight(world).is_ok() && validation::validate_world(world).valid
        });
        let mut authors = manuscript::snapshot(root, None).ok();
        // Keep one comparison image for the complete queue. Re-reads only
        // fence the capture; none of their values enter its graph or witness.
        let stable = stamp.is_some()
            && world.as_ref().is_some_and(|world| {
                historical::load_current_world(root)
                    .is_ok_and(|fresh| fresh.fingerprint == world.fingerprint)
            })
            && authors.as_ref().is_some_and(|authors| {
                manuscript::snapshot(root, None)
                    .is_ok_and(|fresh| fresh.fingerprint == authors.fingerprint)
            })
            && crate::publication::read_stamp(root).ok() == stamp;
        if !stable {
            world = None;
            authors = None;
        }
        Self {
            revision,
            world,
            authors,
        }
    }
    fn fingerprint(&self) -> Option<&str> {
        self.world.as_ref().map(|world| world.fingerprint.as_str())
    }
    fn currentness(&self, status: &str) -> Json {
        let code = match status {
            "stale" => Some("REVIEW-STALE-001"),
            "unknown" => Some("REVIEW-UNKNOWN-001"),
            _ => None,
        };
        json!({"status":status,"code":code,"comparedRevision":self.revision,"comparedFingerprint":self.fingerprint()})
    }
}

fn comparison_authors(
    world: &World,
    authors: &manuscript::Snapshot,
    original: &manuscript::Snapshot,
) -> Result<()> {
    manuscript::verify_snapshot(authors)?;
    if world.revision != authors.revision {
        return Err(invalid(
            "retcon comparison manuscript revision differs from canonical source",
        ));
    }
    for document in authors.documents.values() {
        if original
            .documents
            .get(&document.id)
            .is_some_and(|previous| {
                previous.source_path == document.source_path
                    && previous.raw_bytes == document.raw_bytes
            })
        {
            // Exact source accepted by the original capture stays evidence if
            // a canonical retcon later removes one of its former link targets.
            continue;
        }
        if document.kind == "manuscript-segment" {
            manuscript::validate_segment_references(world, document)?;
        } else if document.kind == "work" {
            manuscript::validate_work(authors, document)?;
        }
    }
    Ok(())
}

fn currentness(root: &Path, analysis: &Analysis, compared: &Comparison) -> Result<Json> {
    // Reconstruct before using any client-provided hash or witness. A readable
    // but forged binding is an error; missing original inputs remain unknown.
    let captured = match verify_original(root, analysis, true) {
        Ok(captured) => captured,
        Err(Error::InvalidInput(message)) if message.starts_with("REVIEW-BINDING-001") => {
            return Err(binding());
        }
        Err(_) => return Ok(compared.currentness("unknown")),
    };
    if analysis.witness["version"] != retcon_impact::WITNESS_VERSION {
        return Ok(compared.currentness("unknown"));
    }
    let (Some(revision), Some(world), Some(authors)) =
        (&compared.revision, &compared.world, &compared.authors)
    else {
        return Ok(compared.currentness("unknown"));
    };
    let revisions = match historical::first_parent_history(
        root,
        &analysis.base_revision,
        revision,
        MAX_HISTORY,
    ) {
        Ok(revisions) => revisions,
        Err(_) => return Ok(compared.currentness("unknown")),
    };
    let changed = |world: &World, authors: &manuscript::Snapshot| -> Result<bool> {
        if world.world_record()?.id != analysis.world_id {
            return Err(invalid("comparison world identity changed"));
        }
        retcon_impact::preflight(world)?;
        if !validation::validate_world(world).valid {
            return Err(Error::Validation(
                "retcon comparison source is invalid".into(),
            ));
        }
        comparison_authors(world, authors, &captured.authors)?;
        let witness = retcon_impact::witness_with_authors(
            world,
            authors,
            &analysis.impact_request,
            &captured.seeds,
            &captured.hints,
        )?;
        Ok(codec::bytes(&witness, retcon_impact::MAX_WITNESS_BYTES)?
            != codec::bytes(&analysis.witness, retcon_impact::MAX_WITNESS_BYTES)?)
    };
    let mut stale = match changed(world, authors) {
        Ok(changed) => changed,
        Err(_) => return Ok(compared.currentness("unknown")),
    };
    for revision in revisions.iter().rev() {
        let outcome = historical::load_world(root, revision).and_then(|world| {
            let authors = manuscript::snapshot(root, Some(revision))?;
            changed(&world, &authors)
        });
        match outcome {
            Ok(changed) => stale |= changed,
            Err(_) => return Ok(compared.currentness("unknown")),
        }
    }
    Ok(compared.currentness(if stale { "stale" } else { "current" }))
}
fn envelope(operation: &str, revision: Option<&str>, compared: &Comparison) -> Json {
    json!({"protocol":crate::CONTRACT,"operation":operation,"scope":"private-retcon-reviews","revision":revision,
        "comparedRevision":compared.revision,"comparedFingerprint":compared.fingerprint(),"writesPerformed":false,"canApply":false})
}

/// Capture immutable analysis only from the literal current committed source.
/// No IDs are allocated and no source, review, Git object or cache is written.
pub fn preview(root: &Path, request: &PreviewRequest) -> Result<Json> {
    validate_preview(request)?;
    let compared = Comparison::capture(root);
    if compared.revision.as_deref() != Some(&request.base_revision) {
        return Err(Error::Conflict(
            "retcon review preview requires its exact current committed HEAD".into(),
        ));
    }
    let world = compared.world.as_ref().ok_or_else(|| {
        Error::Validation(
            "retcon review preview requires available valid bounded current source".into(),
        )
    })?;
    if world.fingerprint != request.plan.expected_fingerprint {
        return Err(Error::Conflict("retcon review preview requires the original committed source fingerprint to match current source".into()));
    }
    let captured = original(root, request)?;
    if compared
        .authors
        .as_ref()
        .map(|authors| &authors.fingerprint)
        != Some(&captured.authors.fingerprint)
    {
        return Err(Error::Conflict(
            "retcon review preview requires committed manuscript source to match current source"
                .into(),
        ));
    }
    let mut result = envelope(
        "retcon-reviews-preview",
        Some(&request.base_revision),
        &compared,
    );
    result["analysis"] = serde_json::to_value(captured.analysis)?;
    codec::bytes(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}
