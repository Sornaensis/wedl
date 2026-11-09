//! Approval-time original evidence for one saved action and selected proposal.
//! These reads bind source identity; they never import source text into context.
use crate::{
    Error, Result, action_drafts, briefs,
    historical::{self, Citation},
    validation,
};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path};

const MAX_REVISIONS: usize = 64;

pub(crate) fn resolve(root: &Path, citations: &[&Citation]) -> Result<Json> {
    // Both arrays already have their own 1,024-item syntax bound. Preserve their
    // concatenated order and repeated occurrences across the two arrays.
    let mut revisions = BTreeMap::<&str, Vec<usize>>::new();
    for (index, citation) in citations.iter().enumerate() {
        revisions.entry(&citation.revision).or_default().push(index);
        if revisions.len() > MAX_REVISIONS {
            return Err(Error::InvalidInput(
                "complete saved evidence exceeds 64 original revisions".into(),
            ));
        }
    }
    let mut evidence = vec![Json::Null; citations.len()];
    let mut budget = historical::WorldBudget::default();
    for (revision, indices) in revisions {
        let world = historical::load_world_with_budget(root, revision, &mut budget)?;
        if !validation::validate_world(&world).valid {
            return Err(Error::Validation(
                "saved evidence requires a valid original historical world".into(),
            ));
        }
        for index in indices {
            let citation = citations[index];
            let owner =
                briefs::resolve_reference_request(&world, &serde_yaml::to_value(&citation.target)?)
                    .map_err(|_| unavailable())?;
            if owner.source_path != citation.path
                || owner.blob_oid.as_deref() != Some(citation.blob_oid.as_str())
            {
                return Err(unavailable());
            }
            evidence[index] = json!({
                "citation":citation,
                "sourceSha256":action_drafts::sha256(&owner.raw_bytes),
            });
        }
    }
    Ok(Json::Array(evidence))
}

fn unavailable() -> Error {
    Error::InvalidInput(
        "saved evidence does not identify the exact historical owner and member".into(),
    )
}
