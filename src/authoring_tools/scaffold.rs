//! Editable, exact-source native Plans. Nothing here reads or writes a file.
//! Updates must prove a byte-identical no-op before their scaffold is returned.
use super::{Family, MAX_RESPONSE_BYTES, ScaffoldRequest};
use crate::{
    Error, Result, consequences,
    model::{Record, World},
    record_edit, validation,
};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn target(record: &Record) -> Json {
    json!({"id":record.id,"kind":record.kind,"sourcePath":record.source_path})
}

fn checked_preview(world: &World, plan: &consequences::Plan) -> Result<Json> {
    // This also applies the Plan's complete encoded-byte/operation bounds.
    let preview = consequences::preview(world, plan, consequences::MAX_CHARACTERS)?;
    if preview["validation"]["valid"] != true {
        return Err(Error::Validation(
            "authoring scaffold candidate failed native preview validation".into(),
        ));
    }
    Ok(preview)
}

fn next_member_id(record: &Record, field: &str, prefix: &str) -> Result<String> {
    let mut used = BTreeSet::new();
    if let Some(value) = record.get(field) {
        let members = value
            .as_sequence()
            .ok_or_else(|| invalid(format!("{field} must be an authored member array")))?;
        for member in members {
            let id = member
                .get("id")
                .and_then(serde_yaml::Value::as_str)
                .ok_or_else(|| invalid(format!("{field} member requires an exact ID")))?;
            used.insert(id);
        }
    }
    // There are at most used.len() occupied IDs, so one of these candidates is
    // necessarily fresh. Decimal digits are a subset of the native Crockford
    // alphabet; no random state, clock or repository mutation is involved.
    for number in 1..=used.len() + 1 {
        let id = format!("{prefix}_{number:026}");
        if !used.contains(id.as_str()) {
            return Ok(id);
        }
    }
    unreachable!("one more candidate than occupied local IDs")
}

fn existing(world: &World, request: &ScaffoldRequest) -> Result<Json> {
    if request.id.is_some() || request.title.is_some() {
        return Err(invalid(
            "existing-record scaffolds do not accept a new ID or title",
        ));
    }
    let record = match request.entity.as_deref() {
        Some(entity) => world.records.get(entity).ok_or_else(|| {
            Error::NotFound(format!(
                "authoring scaffold requires an existing exact record ID {entity:?}"
            ))
        })?,
        None if request.family == Family::Changeset => world.world_record()?,
        None => {
            return Err(invalid(
                "this authoring scaffold requires an exact entity ID",
            ));
        }
    };
    if !request.family.supports_kind(&record.kind) {
        return Err(invalid(format!(
            "{} scaffold cannot edit an owner of kind {:?}",
            request.family.name(),
            record.kind
        )));
    }
    if record.raw_bytes.len() > MAX_RESPONSE_BYTES {
        return Err(invalid(
            "authoring scaffold retained source exceeds the complete response bound",
        ));
    }
    let (patch, retained_fields) = record_edit::scaffold_patch(record)?;
    let plan = consequences::Plan {
        expected_fingerprint: world.fingerprint.clone(),
        operations: vec![consequences::Operation::Update {
            entity_id: record.id.clone(),
            patch,
        }],
        analysis: None,
    };
    let preview = checked_preview(world, &plan)?;
    let raw = std::str::from_utf8(&record.raw_bytes)
        .map_err(|_| invalid("authoring scaffold retained source must be UTF-8"))?;
    if preview["canonicalNoOp"] != true
        || preview["candidateFingerprint"] != world.fingerprint
        || preview["files"][0]["beforeSource"].as_str() != Some(raw)
        || preview["files"][0]["afterSource"].as_str() != Some(raw)
    {
        return Err(invalid(
            "existing source cannot be represented by a byte-preserving native scaffold",
        ));
    }
    let mut result = json!({
        "family":request.family.name(),"target":target(record),"plan":plan,
        "retainedSource":raw,"retainedFields":retained_fields,
        "canonicalNoOp":true,"previewValid":true,
        "notice":"Editable no-op Plan. Omitted fields remain unchanged; supplied arrays and maps replace complete values. Preview the edited Plan before reviewed publication.",
    });
    let member = match request.family {
        Family::Descriptions => Some(("descriptions", "description")),
        Family::SpatialHistory => Some(("spatial_history", "spatialhistory")),
        _ => None,
    };
    if let Some((field, prefix)) = member {
        result["memberField"] = json!(field);
        result["memberPresent"] = json!(record.get(field).is_some());
        if let Some(value) = record.get(field) {
            result["membersYaml"] = json!(serde_yaml::to_string(value)?);
        }
        result["nextMemberId"] = json!(next_member_id(record, field, prefix)?);
    }
    Ok(result)
}

fn organization(world: &World, request: &ScaffoldRequest) -> Result<Json> {
    if request.entity.is_some() {
        return Err(invalid(
            "generational creation does not accept an existing entity",
        ));
    }
    let id = request
        .id
        .as_deref()
        .ok_or_else(|| invalid("generational scaffold requires an explicit new organization ID"))?;
    let title = request
        .title
        .as_deref()
        .ok_or_else(|| invalid("generational scaffold requires an explicit organization title"))?;
    if !validation::valid_id_for_kind(id, "organization") {
        return Err(invalid(
            "generational scaffold requires an exact native organization ID",
        ));
    }
    let at = world.current_time()?;
    let transition_id = format!(
        "transition_{}",
        id.strip_prefix("organization_")
            .expect("validated organization ID")
    );
    let draft = serde_yaml::to_string(&json!({
        "kind":"organization","id":id,"title":title,"organization_kind":"house",
        "initialization":{
            "transition_id":transition_id,"transition_kind":"organization-initialize",
            "applicability":{"applicability_kind":"instant","point":{
                "timeline":at.timeline,"tick":at.tick,"order":at.order,
            }},
            "payload":{"title":title,"aliases":[]},
        },
        "transitions":[],
    }))?;
    let plan = consequences::Plan {
        expected_fingerprint: world.fingerprint.clone(),
        operations: vec![consequences::Operation::Create { draft }],
        analysis: None,
    };
    let preview = checked_preview(world, &plan)?;
    if preview["changedFiles"] != 1 || preview["files"][0]["beforeSource"] != Json::Null {
        return Err(invalid(
            "generational scaffold must create exactly one new organization",
        ));
    }
    Ok(json!({
        "family":request.family.name(),
        "target":{"id":id,"kind":"organization","sourcePath":preview["files"][0]["sourcePath"]},
        "plan":plan,"at":at,"canonicalNoOp":false,"previewValid":true,
        "notice":"Editable organization-create Plan. The starter uses organization_kind: house and the captured native current time; change the authored values as needed and preview before reviewed publication.",
    }))
}

pub(super) fn build(world: &World, request: &ScaffoldRequest) -> Result<Json> {
    if request.family == Family::Generational {
        organization(world, request)
    } else {
        existing(world, request)
    }
}
