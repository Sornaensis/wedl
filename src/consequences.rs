//! Pure, bounded previews of explicitly authored record consequences.
//!
//! Plans reuse the existing record-create and record-update YAML grammars. Every
//! operation is constructed against the same captured base; the complete final
//! candidate is then validated once. Ordinary forward and mutual references are
//! supported. Construction that needs a profile binding or target kind newly
//! introduced by another operation is refused by the existing codec. No source
//! placeholders, inferred effects, filesystem access or publication are involved.
//!
//! The line diff removes equal prefixes and suffixes and reports one complete
//! replacement hunk. It is deterministic, not a minimal edit script. Exact source
//! strings are authoritative, including line endings and a missing final newline.
//! Diff header paths with control characters, quotes or backslashes use JSON
//! string quoting so an authored filename cannot introduce an extra header line.
use crate::{
    Error, Result,
    model::{Record, World},
    record_edit, source, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Write};

pub const MAX_OPERATIONS: usize = 64;
pub const MAX_PLAN_BYTES: usize = 262_144;
pub const MIN_CHARACTERS: usize = 256;
pub const MAX_CHARACTERS: usize = 1_000_000;
/// Each of the combined source-plus-diff bytes and the complete compact JSON
/// report must fit this bound. The Unicode-scalar request budget also applies.
pub const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORD_INPUT: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub expected_fingerprint: String,
    pub operations: Vec<Operation>,
    /// Explicit author-local checks and a same-horizon semantic comparison.
    /// Absence requests only the existing exact source preview.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "analysis_field"
    )]
    pub analysis: Option<crate::consequence_semantics::Analysis>,
}

fn analysis_field<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<Option<crate::consequence_semantics::Analysis>, D::Error> {
    crate::consequence_semantics::Analysis::deserialize(decoder).map(Some)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Operation {
    #[serde(rename = "record.create")]
    Create { draft: String },
    #[serde(rename = "record.update")]
    Update {
        #[serde(rename = "entityId")]
        entity_id: String,
        patch: String,
    },
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn bounded_json(value: &Json, depth: usize, work: &mut usize) -> Result<()> {
    *work += 1;
    if depth > 32 || *work > 16_384 {
        return Err(invalid(
            "consequence plan exceeds complete structural bounds",
        ));
    }
    match value {
        Json::Array(items) => {
            for item in items {
                bounded_json(item, depth + 1, work)?;
            }
        }
        Json::Object(items) => {
            for item in items.values() {
                bounded_json(item, depth + 1, work)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Decode a closed JSON plan without interpreting its retained YAML strings.
/// Historical plans do not resolve references against a later source snapshot.
pub fn decode(payload: &str) -> Result<Plan> {
    if payload.len() > MAX_PLAN_BYTES {
        return Err(invalid("consequence plan exceeds 262144 encoded bytes"));
    }
    // The YAML value visitor rejects duplicate JSON keys, including escaped
    // spellings, before serde_json can collapse them into a map.
    let mut decoder = serde_json::Deserializer::from_str(payload);
    let _: serde_yaml::Value = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate consequence plan JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid consequence plan JSON"))?;
    let value: Json =
        serde_json::from_str(payload).map_err(|_| invalid("invalid consequence plan JSON"))?;
    bounded_json(&value, 0, &mut 0)?;
    let plan: Plan = serde_json::from_value(value)
        .map_err(|error| invalid(format!("invalid consequence plan shape: {error}")))?;
    validate_plan(&plan)?;
    Ok(plan)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("complete JSON byte bound exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encode<T: Serialize>(value: &T, maximum: usize, message: &str) -> Result<Vec<u8>> {
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        maximum,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut writer, value);
    if writer.exceeded {
        return Err(invalid(message));
    }
    result?;
    Ok(writer.bytes)
}

fn plan_bytes(plan: &Plan) -> Result<Vec<u8>> {
    encode(
        plan,
        MAX_PLAN_BYTES,
        "consequence plan exceeds 262144 encoded bytes",
    )
}

/// Check only the versionless plan envelope, exact identities and complete
/// bounds. Source grammar and reference applicability belong to `preview`.
pub fn validate_plan(plan: &Plan) -> Result<()> {
    if plan.expected_fingerprint.len() != 64
        || !plan
            .expected_fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "consequence plan requires an exact source fingerprint",
        ));
    }
    if plan.operations.is_empty() || plan.operations.len() > MAX_OPERATIONS {
        return Err(invalid("consequence plan requires 1..64 operations"));
    }
    if let Some(analysis) = &plan.analysis {
        crate::consequence_semantics::validate_analysis(analysis)?;
    }
    let mut raw_bytes = 0usize;
    for operation in &plan.operations {
        let input = match operation {
            Operation::Create { draft } => draft,
            Operation::Update { entity_id, patch } => {
                if !validation::valid_record_id(entity_id) {
                    return Err(invalid("consequence updates require an exact record ID"));
                }
                raw_bytes = raw_bytes.saturating_add(entity_id.len());
                patch
            }
        };
        if input.trim().is_empty() || input.len() > MAX_RECORD_INPUT {
            return Err(invalid(
                "consequence record input is empty or exceeds 1 MiB",
            ));
        }
        raw_bytes = raw_bytes.saturating_add(input.len());
        if raw_bytes > MAX_PLAN_BYTES {
            return Err(invalid("consequence plan exceeds 262144 encoded bytes"));
        }
    }
    plan_bytes(plan)?;
    Ok(())
}

fn digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(bytes);
    hex::encode(hash.finalize())
}

fn construction_error(index: usize, error: Error) -> Error {
    let message = format!(
        "consequence operation {index} could not be constructed against the captured base: {error}"
    );
    match error {
        Error::Conflict(_) => Error::Conflict(message),
        Error::NotFound(_) => Error::NotFound(message),
        Error::Unsupported(_) => Error::Unsupported(message),
        _ => invalid(message),
    }
}

struct ByteBudget(usize);
impl ByteBudget {
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.0 = self.0.saturating_add(bytes);
        if self.0 > MAX_PREVIEW_BYTES {
            return Err(invalid(
                "consequence preview exceeds 8 MiB of complete source and diff bytes",
            ));
        }
        Ok(())
    }
}

fn source_text(record: &Record) -> Result<&str> {
    std::str::from_utf8(&record.raw_bytes)
        .map_err(|_| Error::Data("record source is not UTF-8".into()))
}

fn diff_line_bytes(text: &str) -> usize {
    text.len()
        .saturating_add(text.split_inclusive('\n').count())
        .saturating_add(if !text.is_empty() && !text.ends_with('\n') {
            1 + "\\ No newline at end of file\n".len()
        } else {
            0
        })
}

fn diff_lines(output: &mut String, marker: char, text: &str) {
    for line in text.split_inclusive('\n') {
        output.push(marker);
        output.push_str(line);
        if !line.ends_with('\n') {
            output.push('\n');
            output.push_str("\\ No newline at end of file\n");
        }
    }
}

fn diff_path(prefix: &str, path: &str) -> Result<String> {
    if path.len() > MAX_PREVIEW_BYTES.saturating_sub(prefix.len()) {
        return Err(invalid(
            "consequence diff path exceeds the complete byte bound",
        ));
    }
    let path = format!("{prefix}{path}");
    if path
        .chars()
        .any(|character| character.is_control() || matches!(character, '"' | '\\'))
    {
        let bytes = encode(
            &path,
            MAX_PREVIEW_BYTES,
            "consequence diff path exceeds the complete byte bound",
        )?;
        Ok(String::from_utf8(bytes).expect("JSON serializer produces UTF-8"))
    } else {
        Ok(path)
    }
}

fn line_diff(
    before: Option<&str>,
    after: &str,
    path: &str,
    budget: &mut ByteBudget,
) -> Result<String> {
    let old = before.unwrap_or("");
    if old == after {
        return Ok(String::new());
    }
    let mut prefix_bytes = 0usize;
    let mut prefix_lines = 0usize;
    for (left, right) in old.split_inclusive('\n').zip(after.split_inclusive('\n')) {
        if left != right {
            break;
        }
        prefix_bytes += left.len();
        prefix_lines += 1;
    }
    let mut suffix_bytes = 0usize;
    for (left, right) in old[prefix_bytes..]
        .split_inclusive('\n')
        .rev()
        .zip(after[prefix_bytes..].split_inclusive('\n').rev())
    {
        if left != right {
            break;
        }
        suffix_bytes += left.len();
    }
    let old = &old[prefix_bytes..old.len() - suffix_bytes];
    let new = &after[prefix_bytes..after.len() - suffix_bytes];
    let old_count = old.split_inclusive('\n').count();
    let new_count = new.split_inclusive('\n').count();
    let old_start = prefix_lines + usize::from(old_count != 0);
    let new_start = prefix_lines + usize::from(new_count != 0);
    let header = format!(
        "--- {}\n+++ {}\n@@ -{old_start},{old_count} +{new_start},{new_count} @@\n",
        if before.is_some() {
            diff_path("a/", path)?
        } else {
            "/dev/null".into()
        },
        diff_path("b/", path)?,
    );
    let bytes = header
        .len()
        .saturating_add(diff_line_bytes(old))
        .saturating_add(diff_line_bytes(new));
    budget.charge(bytes)?;
    let mut output = String::with_capacity(bytes);
    output.push_str(&header);
    diff_lines(&mut output, '-', old);
    diff_lines(&mut output, '+', new);
    Ok(output)
}

/// Construct and inspect one complete candidate without reading or writing any
/// filesystem state. Invalid final candidates return their full diagnostics.
/// `canApply` is always false: there is no canonical batch apply in this API.
pub fn preview(world: &World, plan: &Plan, max_characters: usize) -> Result<Json> {
    Ok(prepare(world, plan, max_characters)?.report)
}

/// One construction shared by source preview and informational author analyses.
/// The candidate carries no invented revision; all output is bounded before it
/// leaves this operation. Callers must inspect the report's validation result.
pub(crate) struct Prepared {
    pub candidate: World,
    pub report: Json,
}

pub(crate) fn prepare(world: &World, plan: &Plan, max_characters: usize) -> Result<Prepared> {
    validate_plan(plan)?;
    if !(MIN_CHARACTERS..=MAX_CHARACTERS).contains(&max_characters) {
        return Err(invalid(
            "consequence preview maxCharacters must be 256..1000000",
        ));
    }
    if plan.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the consequence plan snapshot".into(),
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "base snapshot failed validation; run validate for diagnostics".into(),
        ));
    }

    // Admit every retained before-source before asking the codec to clone it.
    let mut budget = ByteBudget(0);
    let mut updates = BTreeSet::new();
    for (index, operation) in plan.operations.iter().enumerate() {
        if let Operation::Update { entity_id, .. } = operation {
            if !updates.insert(entity_id) {
                return Err(Error::Conflict(format!(
                    "consequence plan writes record {entity_id:?} more than once"
                )));
            }
            let before = world.records.get(entity_id).ok_or_else(|| {
                Error::NotFound(format!(
                    "consequence operation {index} has no exact base record {entity_id:?}"
                ))
            })?;
            budget.charge(before.raw_bytes.len())?;
        }
    }

    let mut seen = BTreeSet::new();
    let mut after_records = Vec::with_capacity(plan.operations.len());
    for (index, operation) in plan.operations.iter().enumerate() {
        let after = match operation {
            Operation::Create { draft } => record_edit::create(world, draft),
            Operation::Update { entity_id, patch } => record_edit::update(world, entity_id, patch),
        }
        .map_err(|error| construction_error(index, error))?;
        if !seen.insert(after.id.clone()) {
            return Err(Error::Conflict(format!(
                "consequence plan writes record {:?} more than once",
                after.id
            )));
        }
        if let Operation::Update { entity_id, .. } = operation {
            let before = &world.records[entity_id];
            if after.id != *entity_id
                || after.kind != before.kind
                || after.source_path != before.source_path
            {
                return Err(invalid(
                    "consequence updates cannot change identity, kind or path",
                ));
            }
        }
        budget.charge(after.raw_bytes.len())?;
        after_records.push(after);
    }

    let mut records: Vec<_> = world
        .records
        .values()
        .filter(|record| !seen.contains(&record.id))
        .cloned()
        .collect();
    records.extend(after_records.iter().cloned());
    let candidate = source::world_from_records(world.root.clone(), records, None)?;
    let validation = validation::validate_world(&candidate);
    // Refuse the whole result if diagnostics alone exceed the output bound.
    encode(
        &validation,
        MAX_PREVIEW_BYTES,
        "consequence preview exceeds 8 MiB of complete JSON",
    )?;

    let mut operations = Vec::with_capacity(plan.operations.len());
    let mut files = Vec::with_capacity(plan.operations.len());
    let mut changed_files = 0usize;
    for (index, (operation, after)) in plan.operations.iter().zip(&after_records).enumerate() {
        let before = world.records.get(&after.id);
        let before_source = before.map(source_text).transpose()?;
        let after_source = source_text(after)?;
        let changed = before.is_none_or(|record| record.raw_bytes != after.raw_bytes);
        changed_files += usize::from(changed);
        let diff = line_diff(before_source, after_source, &after.source_path, &mut budget)?;
        let kind = match operation {
            Operation::Create { .. } => "record.create",
            Operation::Update { .. } => "record.update",
        };
        operations.push(json!({
            "index":index,"type":kind,"entityId":after.id,
            "sourcePath":after.source_path,"changed":changed,
        }));
        files.push(json!({
            "entityId":after.id,"sourcePath":after.source_path,"changed":changed,
            "beforeSha256":before.map(|record|digest(b"",&record.raw_bytes)),
            "afterSha256":digest(b"",&after.raw_bytes),
            "beforeSource":before_source,"afterSource":after_source,"diff":diff,
        }));
    }
    let mut result = json!({
        "protocol":crate::CONTRACT,
        "baseFingerprint":world.fingerprint,"baseRevision":world.revision,
        "candidateFingerprint":candidate.fingerprint,
        "planSha256":digest(b"wedl-consequence-plan\0",&plan_bytes(plan)?),
        "operations":operations,"files":files,"validation":validation,
        "changedFiles":changed_files,"canonicalNoOp":changed_files==0,
        "maxCharacters":max_characters,"writesPerformed":false,"canApply":false,
        "notice":"Read-only explicit consequence preview. This preview does not approve or apply canonical source.",
    });
    if let Some(analysis) = &plan.analysis {
        result["semanticAnalysis"] = if validation.valid {
            crate::consequence_semantics::analyze(world, &candidate, analysis)?
        } else {
            json!({"outcome":"unavailable","reason":"invalid-candidate",
                "at":analysis.at,"applyAllowed":false,"writesPerformed":false})
        };
    }
    let bytes = encode(
        &result,
        MAX_PREVIEW_BYTES,
        "consequence preview exceeds 8 MiB of complete JSON",
    )?;
    result["previewSha256"] = digest(b"wedl-consequence-preview\0", &bytes).into();
    let bytes = encode(
        &result,
        MAX_PREVIEW_BYTES,
        "consequence preview exceeds 8 MiB of complete JSON",
    )?;
    let characters = std::str::from_utf8(&bytes)
        .expect("JSON serializer produces UTF-8")
        .chars()
        .count();
    if characters > max_characters {
        return Err(invalid(format!(
            "complete consequence preview requires {characters} characters; maxCharacters is {max_characters}"
        )));
    }
    Ok(Prepared {
        candidate,
        report: result,
    })
}
