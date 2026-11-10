//! A complete saved-edit review, one paired publication, and exact replay.
//! Hashes are local integrity bindings, not authenticated human approval.
use super::*;
use crate::{
    authoring,
    model::World,
    publication::{self, FileChange},
    record_edit,
};

fn review_binding(draft: &Draft, receipt: &AppliedReceipt) -> Json {
    json!({"editId":draft.id,"worldId":draft.world_id,"targetId":draft.target_id,
        "baseSourcePath":draft.base_source_path,"baseSourceSha256":draft.base_source_sha256,
        "baseFingerprint":receipt.base_fingerprint,"candidateFingerprint":receipt.candidate_fingerprint,
        "beforeEditSha256":receipt.before_edit_sha256,"beforeDraftSha256":receipt.before_draft_sha256,
        "planSha256":receipt.plan_sha256,"reportSha256":receipt.report_sha256,
        "maxCharacters":receipt.max_characters,"file":receipt.file})
}

pub(super) fn validate_receipt(draft: &Draft, receipt: &AppliedReceipt) -> Result<()> {
    if draft.phase != "applied"
        || !(256..=1_000_000).contains(&receipt.max_characters)
        || [
            &receipt.plan_sha256,
            &receipt.review_sha256,
            &receipt.report_sha256,
            &receipt.request_sha256,
            &receipt.base_fingerprint,
            &receipt.candidate_fingerprint,
            &receipt.before_edit_sha256,
            &receipt.before_draft_sha256,
            &receipt.file.before_sha256,
            &receipt.file.after_sha256,
        ]
        .iter()
        .any(|value| !valid_hash(value))
        || receipt.base_fingerprint != draft.base_fingerprint
        || receipt.file.entity_id != draft.target_id
        || receipt.file.source_path != draft.base_source_path
        || receipt.file.before_sha256 != draft.base_source_sha256
    {
        return Err(invalid(
            "applied edit receipt differs from its retained identity and source pins",
        ));
    }
    let mut before = draft.clone();
    before.phase = "draft".into();
    before.applied_receipt = None;
    if receipt.before_draft_sha256 != hash(&serialize(&before)?)
        || receipt.plan_sha256 != digest(b"wedl-consequence-plan\0", &plan(&before)?)?
        || receipt.review_sha256
            != digest(
                b"wedl-edit-outcome-review\0",
                &review_binding(draft, receipt),
            )?
    {
        return Err(invalid(
            "applied edit content differs from its reviewed buffer, plan or report",
        ));
    }
    let request = ApplyRequest {
        expected_fingerprint: receipt.base_fingerprint.clone(),
        edit_id: draft.id.clone(),
        expected_sha256: receipt.before_edit_sha256.clone(),
        max_characters: receipt.max_characters,
        review_sha256: receipt.review_sha256.clone(),
        approved: true,
    };
    if receipt.request_sha256 != digest(b"wedl-edit-outcome-apply\0", &request)? {
        return Err(invalid(
            "applied edit receipt differs from its exact application request",
        ));
    }
    Ok(())
}

fn apply_request(request: &ReviewRequest, review_sha256: String) -> ApplyRequest {
    ApplyRequest {
        expected_fingerprint: request.expected_fingerprint.clone(),
        edit_id: request.edit_id.clone(),
        expected_sha256: request.expected_sha256.clone(),
        max_characters: request.max_characters,
        review_sha256,
        approved: true,
    }
}

fn receipt_from_report(
    draft: &Draft,
    request: &ApplyRequest,
    report: &Json,
) -> Result<AppliedReceipt> {
    let file = &report["consequences"]["files"][0];
    Ok(AppliedReceipt {
        plan_sha256: report["consequences"]["planSha256"]
            .as_str()
            .ok_or_else(|| invalid("edit candidate has no plan binding"))?
            .into(),
        review_sha256: request.review_sha256.clone(),
        report_sha256: report["reportSha256"]
            .as_str()
            .ok_or_else(|| invalid("edit review has no complete report binding"))?
            .into(),
        request_sha256: digest(b"wedl-edit-outcome-apply\0", request)?,
        base_fingerprint: request.expected_fingerprint.clone(),
        candidate_fingerprint: report["consequences"]["candidateFingerprint"]
            .as_str()
            .ok_or_else(|| invalid("edit candidate has no source binding"))?
            .into(),
        before_edit_sha256: request.expected_sha256.clone(),
        before_draft_sha256: hash(&serialize(draft)?),
        max_characters: request.max_characters,
        file: AppliedFile {
            entity_id: file["entityId"]
                .as_str()
                .ok_or_else(|| invalid("edit candidate has no target"))?
                .into(),
            source_path: file["sourcePath"]
                .as_str()
                .ok_or_else(|| invalid("edit candidate has no source path"))?
                .into(),
            before_sha256: file["beforeSha256"]
                .as_str()
                .ok_or_else(|| invalid("edit target must already exist"))?
                .into(),
            after_sha256: file["afterSha256"]
                .as_str()
                .ok_or_else(|| invalid("edit candidate has no exact after source"))?
                .into(),
        },
    })
}

fn applied_document(
    draft: &Draft,
    request: &ApplyRequest,
    report: &Json,
) -> Result<(AppliedReceipt, Vec<u8>)> {
    let receipt = receipt_from_report(draft, request, report)?;
    let mut applied = draft.clone();
    applied.phase = "applied".into();
    applied.applied_receipt = Some(receipt.clone());
    Ok((receipt, serialize(&applied)?))
}

/// Construct the entire report from supplied captured bytes only. Recovery uses
/// this same function while the pending journal intentionally fences live reads.
fn build_review(world: &World, request: &ReviewRequest, document: &store::Stored) -> Result<Json> {
    request.validate()?;
    validate(&document.draft)?;
    let draft = &document.draft;
    if world.revision.is_some() {
        return Err(invalid(
            "saved edit application requires the current worktree",
        ));
    }
    if request.expected_fingerprint != world.fingerprint
        || request.edit_id != draft.id
        || request.expected_sha256 != document.hash
    {
        return Err(Error::Conflict(
            "saved edit review source or private pins changed".into(),
        ));
    }
    if draft.phase != "draft" {
        return Err(invalid(
            "only a saved nonterminal edit may be reviewed for application",
        ));
    }
    if currentness(draft, Some(world))["state"] != "current" {
        return Err(Error::Conflict(
            "saved edit requires explicit same-target reconciliation and resave".into(),
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "edit base source failed validation".into(),
        ));
    }
    let mut result = json!({"protocol":crate::CONTRACT,"operation":"edit-drafts-review","scope":"private-edit-drafts",
        "fingerprint":world.fingerprint,"editId":draft.id,"targetId":draft.target_id,
        "sourceSha256":document.hash,"baseFingerprint":draft.base_fingerprint,
        "draft":draft,"maxCharacters":request.max_characters,"canApply":false,
        "approvalRequired":true,"writesPerformed":false,
        "privateTransition":{"sourcePath":path(&draft.id),"beforeSha256":document.hash,"resultingPhase":"applied","preservesBuffer":true},
        "verificationScope":"exact-source-publication",
        "notice":"Review the exact target changes and retained private transition, then approve this complete report. Saving unfinished editing text supplies no canonical authority."});
    let prepared = (|| {
        let selected = plan(draft)?;
        let consequences::Operation::Update { entity_id, patch } = &selected.operations[0] else {
            unreachable!("one update")
        };
        // Construction failures are reviewable diagnostics; the later complete
        // preview owns whole-result limits and candidate validation.
        record_edit::update(world, entity_id, patch)?;
        Ok::<_, Error>(selected)
    })();
    match prepared {
        Ok(plan) => {
            let candidate = consequences::preview(world, &plan, request.max_characters)?;
            result["canApply"] = json!(candidate["validation"]["valid"] == true);
            result["validation"] = candidate["validation"].clone();
            result["consequences"] = candidate;
        }
        Err(error) => {
            result["consequences"] = Json::Null;
            result["validation"] = json!({"valid":false,"diagnostics":[{"code":"edit.patch.invalid","severity":"error",
                "path":path(&draft.id),"field":"patch","message":error.to_string()}]});
        }
    }
    result["reportSha256"] = json!(digest(b"wedl-edit-outcome-report\0", &result)?);
    if result["consequences"].is_null() {
        result["reviewBinding"] = json!({"editId":draft.id,"sourceSha256":document.hash,
            "fingerprint":world.fingerprint,"maxCharacters":request.max_characters,"reportSha256":result["reportSha256"]});
    } else {
        let provisional =
            receipt_from_report(draft, &apply_request(request, "0".repeat(64)), &result)?;
        result["reviewBinding"] = review_binding(draft, &provisional);
    }
    result["reviewSha256"] = json!(digest(
        b"wedl-edit-outcome-review\0",
        &result["reviewBinding"]
    )?);
    bounded_report(&result, Some(request.max_characters))?;
    Ok(result)
}

fn reviewed(world: &World, request: &ReviewRequest) -> Result<(Json, store::Stored)> {
    store::current(world)?;
    let document = store::load_pinned(world, &request.edit_id, Some(&request.expected_sha256))?;
    let report = build_review(world, request, &document)?;
    if !report["consequences"].is_null() {
        let request = apply_request(
            request,
            report["reviewSha256"]
                .as_str()
                .expect("review digest")
                .into(),
        );
        let (_, bytes) = applied_document(&document.draft, &request, &report)?;
        store::check_replacement(world, &request.edit_id, &document.bytes, &bytes)?;
    }
    Ok((report, document))
}

pub fn review(world: &World, request: &ReviewRequest) -> Result<Json> {
    reviewed(world, request).map(|(report, _)| report)
}

fn apply_report(
    request: &ApplyRequest,
    receipt: &AppliedReceipt,
    source_sha256: &str,
    replay: bool,
) -> Json {
    let changed = receipt.file.before_sha256 != receipt.file.after_sha256;
    json!({"protocol":crate::CONTRACT,"operation":"edit-drafts-apply","scope":"private-edit-drafts",
        "editId":request.edit_id,"targetId":receipt.file.entity_id,"fingerprint":receipt.candidate_fingerprint,
        "baseFingerprint":receipt.base_fingerprint,"beforeSha256":receipt.before_edit_sha256,
        "sourceSha256":source_sha256,"reviewSha256":receipt.review_sha256,"receipt":receipt,
        "changedFiles":usize::from(changed),"canonicalNoOp":!changed,"writesPerformed":!replay,
        "alreadyApplied":replay,"verificationScope":"exact-source-publication","sourceMatches":true,
        "gitCommit":null,"cache":"rebuild-on-next-search"})
}

fn replay(world: &World, request: &ApplyRequest, document: &store::Stored) -> Result<Json> {
    let receipt = document
        .draft
        .applied_receipt
        .as_ref()
        .expect("structural terminal receipt");
    if receipt.request_sha256 != digest(b"wedl-edit-outcome-apply\0", request)?
        || document.bytes != serialize(&document.draft)?
        || world.fingerprint != receipt.candidate_fingerprint
        || world
            .records
            .get(&receipt.file.entity_id)
            .is_none_or(|record| {
                record.source_path != receipt.file.source_path
                    || hash(&record.raw_bytes) != receipt.file.after_sha256
            })
    {
        return Err(Error::Conflict("edit retry differs from its recorded request, exact terminal bytes or current applied source".into()));
    }
    let result = apply_report(request, receipt, &document.hash, true);
    bounded_report(&result, Some(request.max_characters))?;
    Ok(result)
}

/// Inspect a completed application using the native replay checks without writing.
pub(crate) fn inspect_applied(world: &World, request: &ApplyRequest) -> Result<Json> {
    request.validate()?;
    if world.revision.is_some() {
        return Err(invalid(
            "edit application inspection requires the current worktree",
        ));
    }
    let document = store::load_pinned(world, &request.edit_id, None)?;
    if document.draft.phase != "applied" {
        return Err(invalid(
            "edit application inspection requires an applied edit",
        ));
    }
    replay(world, request, &document)
}

fn approved(report: &Json, request: &ApplyRequest) -> Result<()> {
    if report["reviewSha256"].as_str() != Some(request.review_sha256.as_str()) {
        return Err(Error::Conflict(
            "edit review changed; review the exact saved buffer again".into(),
        ));
    }
    if report["canApply"] != true {
        return Err(Error::Validation(
            "retained editing text or candidate source failed validation; source was not changed"
                .into(),
        ));
    }
    Ok(())
}

pub fn apply(world: &World, request: &ApplyRequest) -> Result<Json> {
    request.validate()?;
    store::current(world)?;
    let initial = store::load_pinned(world, &request.edit_id, None)?;
    if initial.draft.phase == "applied" {
        return replay(world, request, &initial);
    }
    let (report, _) = reviewed(world, &request.review_request())?;
    approved(&report, request)?;
    let _lock = authoring::writer_lock(&world.root)?;
    let current = store::load_pinned(world, &request.edit_id, None)?;
    if current.draft.phase == "applied" {
        return replay(world, request, &current);
    }
    let (report, document) = reviewed(world, &request.review_request())?;
    approved(&report, request)?;
    let (receipt, bytes) = applied_document(&document.draft, request, &report)?;
    let after_hash = hash(&bytes);
    let file = &report["consequences"]["files"][0];
    let changes = [
        FileChange {
            path: receipt.file.source_path.clone(),
            before: Some(
                file["beforeSource"]
                    .as_str()
                    .expect("update before source")
                    .as_bytes()
                    .to_vec(),
            ),
            after: file["afterSource"]
                .as_str()
                .expect("update after source")
                .as_bytes()
                .to_vec(),
        },
        FileChange {
            path: path(&request.edit_id),
            before: Some(document.bytes),
            after: bytes,
        },
    ];
    let result = apply_report(request, &receipt, &after_hash, false);
    bounded_report(&result, Some(request.max_characters))?;
    publication::publish_locked(world, &receipt.candidate_fingerprint, &changes)?;
    Ok(result)
}

/// Validate an edit pair solely from reconstructed immutable before/after source.
/// This remains callable during rollback while live reads are fenced.
pub(crate) fn validate_publication(
    base: &World,
    candidate: &World,
    before_source: &str,
    after_source: &str,
    canonical_changes: &[FileChange],
) -> Result<()> {
    let before = decode(before_source)?;
    let after = decode(after_source)?;
    let receipt = after
        .applied_receipt
        .as_ref()
        .ok_or_else(|| invalid("edit publication requires a terminal receipt"))?;
    let mut original = after.clone();
    original.phase = "draft".into();
    original.applied_receipt = None;
    if before != original
        || before.phase != "draft"
        || receipt.before_edit_sha256 != hash(before_source.as_bytes())
        || receipt.base_fingerprint != base.fingerprint
        || receipt.candidate_fingerprint != candidate.fingerprint
        || before.world_id != base.world_record()?.id
        || candidate.world_record()?.id != before.world_id
        || canonical_changes.len() != 1
    {
        return Err(invalid(
            "edit publication differs from its reviewed private/source pair",
        ));
    }
    let request = ReviewRequest {
        expected_fingerprint: receipt.base_fingerprint.clone(),
        edit_id: before.id.clone(),
        expected_sha256: receipt.before_edit_sha256.clone(),
        max_characters: receipt.max_characters,
    };
    let document = store::Stored {
        draft: before,
        bytes: before_source.as_bytes().to_vec(),
        hash: request.expected_sha256.clone(),
        blob_oid: None,
    };
    let report = build_review(base, &request, &document)?;
    let apply = apply_request(&request, receipt.review_sha256.clone());
    approved(&report, &apply)?;
    let (_, expected_after) = applied_document(&document.draft, &apply, &report)?;
    let file = &report["consequences"]["files"][0];
    let change = &canonical_changes[0];
    if expected_after != after_source.as_bytes()
        || report["consequences"]["candidateFingerprint"].as_str()
            != Some(candidate.fingerprint.as_str())
        || change.path != receipt.file.source_path
        || change.before.as_deref() != file["beforeSource"].as_str().map(str::as_bytes)
        || change.after
            != file["afterSource"]
                .as_str()
                .expect("candidate source")
                .as_bytes()
    {
        return Err(invalid(
            "edit publication target or terminal receipt differs from its exact complete review",
        ));
    }
    Ok(())
}
