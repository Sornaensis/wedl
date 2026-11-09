//! Reviewed application and exact source verification of one saved proposal.
//!
//! Review and apply are distinct from proposal readiness. Source hashes bind
//! exact local content; they do not authenticate a human identity or establish
//! that someone read a report. No semantic effects or character learning are
//! inferred. The publication layer owns interrupted-write recovery.
use crate::{
    Error, Result, action_drafts, action_store, authoring, consequences,
    model::World,
    publication::{self, FileChange},
    validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

pub type ReviewRequest = action_store::ProposalPreviewRequest;
const MAX_REQUEST_BYTES: usize = 16_384;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRequest {
    pub expected_fingerprint: String,
    pub action_id: String,
    pub expected_sha256: String,
    pub proposal_id: String,
    pub max_characters: usize,
    pub review_sha256: String,
    pub approved: bool,
}

impl ApplyRequest {
    pub fn review_request(&self) -> ReviewRequest {
        ReviewRequest {
            expected_fingerprint: self.expected_fingerprint.clone(),
            action_id: self.action_id.clone(),
            expected_sha256: self.expected_sha256.clone(),
            proposal_id: self.proposal_id.clone(),
            max_characters: self.max_characters,
        }
    }

    fn validate(&self) -> Result<()> {
        self.review_request().validate()?;
        if !action_drafts::valid_hash(&self.review_sha256) {
            return Err(invalid(
                "outcome application requires its exact review hash",
            ));
        }
        if !self.approved {
            return Err(invalid(
                "outcome application requires explicit approved:true",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyRequest {
    pub expected_fingerprint: String,
    pub action_id: String,
    pub expected_sha256: String,
    pub max_characters: usize,
}

impl VerifyRequest {
    fn validate(&self) -> Result<()> {
        if !action_drafts::valid_action_id(&self.action_id)
            || !action_drafts::valid_hash(&self.expected_fingerprint)
            || !action_drafts::valid_hash(&self.expected_sha256)
            || !(256..=1_000_000).contains(&self.max_characters)
        {
            return Err(invalid(
                "outcome verification requires exact source/private bindings and maxCharacters 256..1000000",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedFile {
    pub entity_id: String,
    pub source_path: String,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedReceipt {
    pub proposal_id: String,
    pub proposal_sha256: String,
    pub evidence_sha256: String,
    pub plan_sha256: String,
    pub review_sha256: String,
    pub report_sha256: String,
    pub request_sha256: String,
    pub base_fingerprint: String,
    pub candidate_fingerprint: String,
    pub before_action_sha256: String,
    pub before_draft_sha256: String,
    pub max_characters: usize,
    pub files: Vec<AppliedFile>,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let mut bytes = domain.to_vec();
    bytes.extend(serde_json::to_vec(value)?);
    Ok(action_drafts::sha256(&bytes))
}

fn bounded_report(value: &Json, maximum: usize) -> Result<()> {
    let encoded = serde_json::to_string(value)?;
    if encoded.len() > consequences::MAX_PREVIEW_BYTES || encoded.chars().count() > maximum {
        return Err(invalid(
            "complete outcome report exceeds its character or byte budget",
        ));
    }
    Ok(())
}

fn decode<T: for<'de> Deserialize<'de>>(payload: &str) -> Result<T> {
    if payload.len() > MAX_REQUEST_BYTES {
        return Err(invalid("outcome request exceeds 16 KiB"));
    }
    // All request members are closed scalars; a typed serde decoder rejects
    // duplicate keys without first erasing them in an untyped JSON object.
    serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, duplicate or unknown outcome request fields"))
}

pub fn decode_review(payload: &str) -> Result<ReviewRequest> {
    action_store::decode_proposal_preview(payload)
}

pub fn decode_apply(payload: &str) -> Result<ApplyRequest> {
    let request: ApplyRequest = decode(payload)?;
    request.validate()?;
    Ok(request)
}

pub fn decode_verify(payload: &str) -> Result<VerifyRequest> {
    let request: VerifyRequest = decode(payload)?;
    request.validate()?;
    Ok(request)
}

fn source_path(path: &str) -> bool {
    path.len() <= 4096
        && path.starts_with("story/")
        && !path.contains('\\')
        && Path::new(path).extension().and_then(|e| e.to_str()) == Some("md")
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The review digest binds the complete report digest together with every
/// receipt claim needed for replay. Retaining this small manifest avoids storing
/// all source/diff bytes again while making altered receipt fields invalidate
/// the original approved request, even if the edited file is reserialized.
fn receipt_review_binding(draft: &action_drafts::Draft, receipt: &AppliedReceipt) -> Json {
    json!({
        "actionId":draft.id,"originalBaseFingerprint":draft.base_fingerprint,
        "proposalId":receipt.proposal_id,"proposalSha256":receipt.proposal_sha256,
        "evidenceSha256":receipt.evidence_sha256,
        "planSha256":receipt.plan_sha256,"baseFingerprint":receipt.base_fingerprint,
        "candidateFingerprint":receipt.candidate_fingerprint,
        "beforeActionSha256":receipt.before_action_sha256,
        "beforeDraftSha256":receipt.before_draft_sha256,
        "maxCharacters":receipt.max_characters,"files":receipt.files,
        "reportSha256":receipt.report_sha256,
    })
}

/// Structural source checks deliberately do not resolve a historical proposal
/// against current canon. They bind its immutable content and receipt together.
pub(crate) fn validate_receipt(
    draft: &action_drafts::Draft,
    receipt: &AppliedReceipt,
) -> Result<()> {
    if draft.phase != "applied"
        || !action_drafts::valid_proposal_id(&receipt.proposal_id)
        || !(256..=1_000_000).contains(&receipt.max_characters)
        || [
            &receipt.proposal_sha256,
            &receipt.evidence_sha256,
            &receipt.plan_sha256,
            &receipt.review_sha256,
            &receipt.report_sha256,
            &receipt.request_sha256,
            &receipt.base_fingerprint,
            &receipt.candidate_fingerprint,
            &receipt.before_action_sha256,
            &receipt.before_draft_sha256,
        ]
        .iter()
        .any(|hash| !action_drafts::valid_hash(hash))
    {
        return Err(invalid("invalid applied outcome receipt bindings"));
    }
    let proposal = draft
        .proposals
        .iter()
        .find(|proposal| proposal.id == receipt.proposal_id)
        .ok_or_else(|| invalid("applied receipt requires its retained proposal"))?;
    if !action_drafts::active_proposal_ids(draft).contains(&proposal.id)
        || receipt.base_fingerprint != proposal.context_fingerprint
        || receipt.proposal_sha256 != action_drafts::sha256(&serde_json::to_vec(proposal)?)
        || receipt.plan_sha256 != digest(b"wedl-consequence-plan\0", &proposal.consequences)?
        || receipt.files.len() != proposal.consequences.operations.len()
    {
        return Err(invalid(
            "applied receipt differs from its retained proposal",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for (file, operation) in receipt.files.iter().zip(&proposal.consequences.operations) {
        if !validation::valid_record_id(&file.entity_id)
            || !source_path(&file.source_path)
            || !action_drafts::valid_hash(&file.after_sha256)
            || file
                .before_sha256
                .as_deref()
                .is_some_and(|hash| !action_drafts::valid_hash(hash))
            || !ids.insert(&file.entity_id)
            || !paths.insert(file.source_path.to_lowercase())
        {
            return Err(invalid(
                "applied receipt has invalid or duplicate source files",
            ));
        }
        match operation {
            consequences::Operation::Create { .. } if file.before_sha256.is_none() => {}
            consequences::Operation::Update { entity_id, .. }
                if *entity_id == file.entity_id && file.before_sha256.is_some() => {}
            _ => {
                return Err(invalid(
                    "applied receipt files do not match the ordered plan",
                ));
            }
        }
    }
    if receipt.review_sha256
        != digest(
            b"wedl-action-outcome-review\0",
            &receipt_review_binding(draft, receipt),
        )?
    {
        return Err(invalid(
            "applied receipt fields differ from the bound outcome review",
        ));
    }
    let mut before = draft.clone();
    before.phase = "proposed".into();
    before.applied_receipt = None;
    if receipt.before_draft_sha256 != action_drafts::sha256(&action_drafts::serialize(&before)?) {
        return Err(invalid(
            "applied action content differs from its retained review",
        ));
    }
    let request = ApplyRequest {
        expected_fingerprint: receipt.base_fingerprint.clone(),
        action_id: draft.id.clone(),
        expected_sha256: receipt.before_action_sha256.clone(),
        proposal_id: receipt.proposal_id.clone(),
        max_characters: receipt.max_characters,
        review_sha256: receipt.review_sha256.clone(),
        approved: true,
    };
    if receipt.request_sha256 != digest(b"wedl-action-outcome-apply\0", &request)? {
        return Err(invalid(
            "applied receipt differs from its exact apply request",
        ));
    }
    Ok(())
}

fn ready_answers(draft: &action_drafts::Draft, proposal: &action_drafts::Proposal) -> Result<()> {
    let answers: Vec<_> = draft
        .questions
        .iter()
        .filter_map(|question| {
            question
                .answer
                .as_ref()
                .map(|answer| action_drafts::QuestionAnswer {
                    question_id: question.id.clone(),
                    answer: answer.clone(),
                })
        })
        .collect();
    if draft
        .questions
        .iter()
        .any(|q| q.required && q.answer.is_none())
        || answers != proposal.question_answers
    {
        return Err(Error::Conflict(
            "selected proposal answers differ from the current complete action answers".into(),
        ));
    }
    Ok(())
}

fn reviewed(world: &World, request: &ReviewRequest) -> Result<(Json, action_store::Stored)> {
    request.validate()?;
    let document =
        action_store::load_pinned(world, &request.action_id, Some(&request.expected_sha256))?;
    action_drafts::validate(world, &document.draft)?;
    let preview = action_store::proposal_preview(world, request)?;
    let proposal = document
        .draft
        .proposals
        .iter()
        .find(|p| p.id == request.proposal_id)
        .ok_or_else(|| Error::NotFound("saved proposal is unavailable".into()))?;
    ready_answers(&document.draft, proposal)?;
    let citations = document
        .draft
        .evidence
        .iter()
        .chain(&proposal.evidence)
        .collect::<Vec<_>>();
    let evidence = crate::action_evidence::resolve(&world.root, &citations)?;
    let mut result = preview;
    result["operation"] = json!("action-outcome-review");
    // The preview retains its own nested consequence hash. The saved wrapper's
    // preview digest is not an outcome review and is deliberately not reused.
    result.as_object_mut().unwrap().remove("previewSha256");
    result["draft"] = serde_json::to_value(&document.draft)?;
    result["evidenceSha256"] = json!(digest(b"wedl-action-outcome-evidence\0", &evidence)?);
    result["evidence"] = evidence;
    result["requiredAnswered"] = json!(true);
    result["approvalRequired"] = json!(true);
    result["privateTransition"] = json!({
        "sourcePath":format!("author/actions/{}.json", request.action_id),
        "beforeSha256":document.hash,"resultingPhase":"applied","preservesHistory":true,
    });
    let checks_allow = result["consequences"]
        .get("semanticAnalysis")
        .is_none_or(|analysis| analysis["applyAllowed"] == true);
    result["canApply"] =
        json!(result["consequences"]["validation"]["valid"] == true && checks_allow);
    result["verificationScope"] = json!("exact-source-publication");
    result["notice"] = json!(
        "Review the complete source candidate, resolved original evidence and any explicit semantic analysis, then approve this exact report. Required expectations must pass; advisory results do not supply approval. No consequences are inferred."
    );
    result["reportSha256"] = json!(digest(b"wedl-action-outcome-report\0", &result)?);
    // The complete prospective terminal document must fit before any approval
    // or write can be acted on. This also checks the containing private quota.
    let apply = ApplyRequest {
        expected_fingerprint: request.expected_fingerprint.clone(),
        action_id: request.action_id.clone(),
        expected_sha256: request.expected_sha256.clone(),
        proposal_id: request.proposal_id.clone(),
        max_characters: request.max_characters,
        review_sha256: "0".repeat(64),
        approved: true,
    };
    let provisional = receipt_from_report(&document.draft, &apply, &result)?;
    result["reviewBinding"] = receipt_review_binding(&document.draft, &provisional);
    result["reviewSha256"] = json!(digest(
        b"wedl-action-outcome-review\0",
        &result["reviewBinding"]
    )?);
    bounded_report(&result, request.max_characters)?;
    let apply = ApplyRequest {
        review_sha256: result["reviewSha256"].as_str().unwrap().into(),
        ..apply
    };
    let (_, bytes) = applied_document(&document.draft, &apply, &result)?;
    action_store::check_outcome_replacement(world, &request.action_id, &document.bytes, &bytes)?;
    Ok((result, document))
}

pub fn review(world: &World, request: &ReviewRequest) -> Result<Json> {
    reviewed(world, request).map(|(result, _)| result)
}

fn receipt_from_report(
    draft: &action_drafts::Draft,
    request: &ApplyRequest,
    report: &Json,
) -> Result<AppliedReceipt> {
    let mut files = Vec::new();
    for file in report["consequences"]["files"]
        .as_array()
        .expect("native complete file report")
    {
        files.push(AppliedFile {
            entity_id: file["entityId"].as_str().unwrap().into(),
            source_path: file["sourcePath"].as_str().unwrap().into(),
            before_sha256: file["beforeSha256"].as_str().map(str::to_owned),
            after_sha256: file["afterSha256"].as_str().unwrap().into(),
        });
    }
    let receipt = AppliedReceipt {
        proposal_id: request.proposal_id.clone(),
        proposal_sha256: report["proposalSha256"].as_str().unwrap().into(),
        evidence_sha256: report["evidenceSha256"].as_str().unwrap().into(),
        plan_sha256: report["consequences"]["planSha256"]
            .as_str()
            .unwrap()
            .into(),
        review_sha256: request.review_sha256.clone(),
        report_sha256: report["reportSha256"].as_str().unwrap().into(),
        request_sha256: digest(b"wedl-action-outcome-apply\0", request)?,
        base_fingerprint: request.expected_fingerprint.clone(),
        candidate_fingerprint: report["consequences"]["candidateFingerprint"]
            .as_str()
            .unwrap()
            .into(),
        before_action_sha256: request.expected_sha256.clone(),
        before_draft_sha256: action_drafts::sha256(&action_drafts::serialize(draft)?),
        max_characters: request.max_characters,
        files,
    };
    Ok(receipt)
}

fn applied_document(
    draft: &action_drafts::Draft,
    request: &ApplyRequest,
    report: &Json,
) -> Result<(AppliedReceipt, Vec<u8>)> {
    let receipt = receipt_from_report(draft, request, report)?;
    let mut applied = draft.clone();
    applied.phase = "applied".into();
    applied.applied_receipt = Some(receipt.clone());
    Ok((receipt, action_drafts::serialize(&applied)?))
}

fn receipt_files(world: &World, receipt: &AppliedReceipt) -> (Vec<Json>, bool) {
    let mut all_match = true;
    let files = receipt
        .files
        .iter()
        .map(|expected| {
            let current = world.records.get(&expected.entity_id);
            let current_hash = current.map(|record| action_drafts::sha256(&record.raw_bytes));
            let status = match current {
                None => "missing",
                Some(record) if record.source_path != expected.source_path => "moved",
                Some(_) if current_hash.as_deref() != Some(expected.after_sha256.as_str()) => {
                    "changed"
                }
                Some(_) => "current",
            };
            all_match &= status == "current";
            json!({"entityId":expected.entity_id,"sourcePath":expected.source_path,
            "expectedSha256":expected.after_sha256,"currentSha256":current_hash,
            "currentSourcePath":current.map(|r| &r.source_path),"status":status})
        })
        .collect();
    (files, all_match)
}

fn apply_report(world: &World, receipt: &AppliedReceipt, private_hash: &str, replay: bool) -> Json {
    let changed = receipt
        .files
        .iter()
        .filter(|file| file.before_sha256.as_deref() != Some(file.after_sha256.as_str()))
        .count();
    json!({"protocol":crate::CONTRACT,"operation":"action-outcome-apply",
        "scope":"private-action-drafts","actionId":Json::Null,"proposalId":receipt.proposal_id,
        "baseFingerprint":receipt.base_fingerprint,"fingerprint":receipt.candidate_fingerprint,
        "beforeSha256":receipt.before_action_sha256,"sourceSha256":private_hash,
        "reviewSha256":receipt.review_sha256,"changedFiles":changed,"canonicalNoOp":changed==0,
        "writesPerformed":!replay,"alreadyApplied":replay,"receipt":receipt,
        "cache":"rebuild-on-next-search","gitCommit":Json::Null,
        "verificationScope":"exact-source-publication","sourceMatches":world.fingerprint==receipt.candidate_fingerprint})
}

fn replay(world: &World, request: &ApplyRequest, current: &action_store::Stored) -> Result<Json> {
    let receipt = current
        .draft
        .applied_receipt
        .as_ref()
        .expect("structural receipt");
    if receipt.request_sha256 != digest(b"wedl-action-outcome-apply\0", request)?
        || current.bytes != action_drafts::serialize(&current.draft)?
        || world.fingerprint != receipt.candidate_fingerprint
        || !receipt_files(world, receipt).1
    {
        return Err(Error::Conflict(
            "outcome retry differs from the recorded application or current applied source".into(),
        ));
    }
    let mut result = apply_report(world, receipt, &current.hash, true);
    result["actionId"] = json!(request.action_id);
    bounded_report(&result, request.max_characters)?;
    Ok(result)
}

/// Apply only the exact saved proposal bound by this complete reviewed packet.
/// Native approval is an explicit local caller declaration, not authentication.
pub fn apply(world: &World, request: &ApplyRequest) -> Result<Json> {
    request.validate()?;
    // Refuse invalid first applications before creating writer state, then
    // repeat admission under the lock. An altered request never gains
    // permission from terminal state.
    let initial = action_store::load_pinned(world, &request.action_id, None)?;
    if initial.draft.phase == "applied" {
        // A fully verified retry is a pure generation-fenced capture. It does
        // not recreate a deleted writer/cache directory or lock file.
        return replay(world, request, &initial);
    }
    {
        let (report, _) = reviewed(world, &request.review_request())?;
        if report["reviewSha256"].as_str() != Some(request.review_sha256.as_str()) {
            return Err(Error::Conflict(
                "outcome review changed; review this exact saved proposal again".into(),
            ));
        }
        if report["canApply"] != true {
            return Err(Error::Validation(
                "outcome candidate validation or required semantic checks failed; source was not changed".into(),
            ));
        }
    }
    let _lock = authoring::writer_lock(&world.root)?;
    let current = action_store::load_pinned(world, &request.action_id, None)?;
    if current.draft.phase == "applied" {
        return replay(world, request, &current);
    }
    // Recompute all source/private bindings, candidate validation, answer
    // snapshots and report bytes while the publication writer lock is held.
    let (report, document) = reviewed(world, &request.review_request())?;
    if report["reviewSha256"].as_str() != Some(request.review_sha256.as_str()) {
        return Err(Error::Conflict(
            "outcome review changed; review this exact saved proposal again".into(),
        ));
    }
    if report["canApply"] != true {
        return Err(Error::Validation(
            "outcome candidate validation or required semantic checks failed; source was not changed".into(),
        ));
    }
    let (receipt, applied_bytes) = applied_document(&document.draft, request, &report)?;
    let applied_hash = action_drafts::sha256(&applied_bytes);
    let mut changes = Vec::new();
    for file in report["consequences"]["files"].as_array().unwrap() {
        changes.push(FileChange {
            path: file["sourcePath"].as_str().unwrap().into(),
            before: file["beforeSource"]
                .as_str()
                .map(|source| source.as_bytes().to_vec()),
            after: file["afterSource"].as_str().unwrap().as_bytes().to_vec(),
        });
    }
    changes.push(FileChange {
        path: format!("author/actions/{}.json", request.action_id),
        before: Some(document.bytes),
        after: applied_bytes,
    });
    let mut result = apply_report(world, &receipt, &applied_hash, false);
    result["actionId"] = json!(request.action_id);
    result["sourceMatches"] = json!(true);
    // Output bounds are checked before any publication; a successful save must
    // never turn into a reporting failure due to the chosen complete budget.
    bounded_report(&result, request.max_characters)?;
    publication::publish_locked(world, &receipt.candidate_fingerprint, &changes)?;
    Ok(result)
}

pub fn verify(world: &World, request: &VerifyRequest) -> Result<Json> {
    request.validate()?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "verification source differs from the requested snapshot".into(),
        ));
    }
    let document =
        action_store::load_pinned(world, &request.action_id, Some(&request.expected_sha256))?;
    let receipt = document
        .draft
        .applied_receipt
        .as_ref()
        .ok_or_else(|| invalid("outcome verification requires an applied action receipt"))?;
    let (files, files_match) = receipt_files(world, receipt);
    let source_matches = world.fingerprint == receipt.candidate_fingerprint;
    let mut result = json!({"protocol":crate::CONTRACT,"operation":"action-outcome-verify",
        "scope":"private-action-drafts","actionId":request.action_id,
        "sourceSha256":document.hash,"fingerprint":world.fingerprint,
        "candidateFingerprint":receipt.candidate_fingerprint,"receipt":receipt,
        "sourceMatches":source_matches,"filesMatch":files_match,
        "verified":source_matches && files_match,"files":files,
        "maxCharacters":request.max_characters,"writesPerformed":false,
        "verificationScope":"exact-source-publication",
        "notice":"Verification compares current exact source with the receipt. Historical evidence was resolved at approval; this operation retains that digest without reopening historical Git. When retained analysis exists, semanticVerification separately reevaluates its explicit expectations at their saved horizon. Neither check infers effects or authenticates the receipt."});
    let proposal = document
        .draft
        .proposals
        .iter()
        .find(|proposal| proposal.id == receipt.proposal_id)
        .expect("validated receipt retains its proposal");
    if let Some(analysis) = &proposal.consequences.analysis {
        result["semanticVerification"] = match crate::consequence_semantics::check(world, analysis)
        {
            Ok(report) => report,
            Err(
                Error::InvalidInput(message)
                | Error::Validation(message)
                | Error::Unsupported(message)
                | Error::NotFound(message),
            ) => {
                json!({"outcome":"unavailable","reason":message,"at":analysis.at,
                    "applyAllowed":false,"writesPerformed":false})
            }
            Err(error) => return Err(error),
        };
    }
    bounded_report(&result, request.max_characters)?;
    Ok(result)
}
