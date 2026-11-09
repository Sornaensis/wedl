//! Private action reasoning, explicit readiness and exact captured review.
//! These documents are never canonical records or character knowledge.
use crate::{
    Error, Result, briefs, consequences,
    model::{StoryTime, World},
    validation,
    writing_frames::{self, FamilySelection, Frame},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_DOCUMENT: usize = 262_144;
const MAX_REQUEST: usize = 524_288;
const PHASES: &[&str] = &[
    "draft",
    "context-needed",
    "resolution-needed",
    "proposed",
    "stale",
    "cancelled",
    "applied",
];

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn prefixed_id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|s| s.strip_prefix('_'))
        .is_some_and(|s| {
            s.len() == 26
                && s.bytes()
                    .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
        })
}
pub fn valid_action_id(value: &str) -> bool {
    prefixed_id(value, "action")
}
pub fn valid_proposal_id(value: &str) -> bool {
    prefixed_id(value, "proposal")
}
pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn text(value: &str, maximum: usize, empty: bool) -> Result<()> {
    if value.len() > maximum || (!empty && value.trim().is_empty()) {
        return Err(invalid("action text is empty or exceeds its byte bound"));
    }
    Ok(())
}
fn closed(value: &Json, required: &[&str], optional: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("action fields must be objects"))?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err(invalid("action data has missing or unknown fields"));
    }
    Ok(())
}
fn bounded(value: &Json, depth: usize, work: &mut usize) -> Result<()> {
    *work += 1;
    if depth > 32 || *work > 16_384 {
        return Err(invalid("action JSON exceeds complete structural bounds"));
    }
    match value {
        Json::Array(values) => {
            for item in values {
                bounded(item, depth + 1, work)?;
            }
        }
        Json::Object(values) => {
            for item in values.values() {
                bounded(item, depth + 1, work)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn parse(payload: &str, limit: usize) -> Result<Json> {
    if payload.len() > limit {
        return Err(invalid("action JSON exceeds its byte bound"));
    }
    let mut decoder = serde_json::Deserializer::from_str(payload);
    let _: serde_yaml::Value = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate action JSON"))?;
    decoder.end().map_err(|_| invalid("invalid action JSON"))?;
    let value: Json = serde_json::from_str(payload).map_err(|_| invalid("invalid action JSON"))?;
    bounded(&value, 0, &mut 0)?;
    Ok(value)
}
fn point(value: Json) -> Result<StoryTime> {
    // Reuse the frame's exact decimal-string and closed-time grammar.
    let frame = json!({"fingerprint":"0".repeat(64),"work":null,"sceneId":null,"povId":null,"at":value,"lens":"author"});
    Ok(writing_frames::decode(&serde_json::to_string(&frame)?)?.at)
}
fn deserialize_time<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> std::result::Result<StoryTime, D::Error> {
    point(Json::deserialize(decoder)?).map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Reference {
    Record {
        id: String,
    },
    Section {
        #[serde(rename = "ownerId")]
        owner_id: String,
        id: String,
    },
}
impl Reference {
    fn source(&self) -> Result<serde_yaml::Value> {
        Ok(serde_yaml::to_value(match self {
            Self::Record { id } => json!({"kind":"record","id":id}),
            Self::Section { owner_id, id } => json!({"kind":"section","owner_id":owner_id,"id":id}),
        })?)
    }
    fn structurally_valid(&self) -> bool {
        match self {
            Self::Record { id } => validation::valid_record_id(id),
            Self::Section { owner_id, id } => {
                validation::valid_record_id(owner_id) && !id.trim().is_empty() && id.len() <= 128
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub id: String,
    pub text: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionAnswer {
    pub question_id: String,
    pub answer: String,
}

/// An inert authored alternative. Saving its plan never approves or applies it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proposal {
    pub id: String,
    pub summary: String,
    pub context_fingerprint: String,
    pub evidence: Vec<crate::historical::Citation>,
    pub question_answers: Vec<QuestionAnswer>,
    pub resolver: String,
    pub consequences: consequences::Plan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub id: String,
    pub title: String,
    pub world_id: String,
    pub base_fingerprint: String,
    #[serde(deserialize_with = "deserialize_time")]
    pub horizon: StoryTime,
    pub intent: String,
    pub actor_refs: Vec<Reference>,
    pub target_refs: Vec<Reference>,
    /// Private links to possibilities, never admitted context or character grants.
    pub hypothesis_refs: Vec<Reference>,
    pub evidence: Vec<crate::historical::Citation>,
    pub questions: Vec<Question>,
    pub proposals: Vec<Proposal>,
    pub phase: String,
    pub body_markdown: String,
    /// Present only after reviewed outcome publication, never an ordinary save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_receipt: Option<crate::action_outcomes::AppliedReceipt>,
}

fn structural(draft: &Draft) -> Result<()> {
    if !valid_action_id(&draft.id)
        || !validation::valid_id_for_kind(&draft.world_id, "world")
        || !valid_hash(&draft.base_fingerprint)
        || !PHASES.contains(&draft.phase.as_str())
    {
        return Err(invalid("invalid action identity, provenance or phase"));
    }
    text(&draft.title, 1024, false)?;
    text(&draft.intent, 16_384, false)?;
    text(&draft.body_markdown, 131_072, true)?;
    text(&draft.horizon.timeline, 128, false)?;
    crate::historical::validate_citations(&draft.evidence)?;
    if draft.actor_refs.is_empty()
        || draft.actor_refs.len() > 1024
        || draft.target_refs.len() > 1024
        || draft.hypothesis_refs.len() > 1024
    {
        return Err(invalid(
            "action references require bounded ordered arrays and at least one actor",
        ));
    }
    for refs in [
        &draft.actor_refs,
        &draft.target_refs,
        &draft.hypothesis_refs,
    ] {
        let mut seen = BTreeSet::new();
        for reference in refs {
            if !reference.structurally_valid() || !seen.insert(serde_json::to_string(reference)?) {
                return Err(invalid(
                    "action references must be unique exact owner/member pointers",
                ));
            }
        }
    }
    if draft
        .actor_refs
        .iter()
        .any(|reference| !matches!(reference, Reference::Record { .. }))
    {
        return Err(invalid("action actors must be record references"));
    }
    if draft
        .hypothesis_refs
        .iter()
        .any(|reference| !matches!(reference, Reference::Record { .. }))
    {
        return Err(invalid("action hypotheses must be record references"));
    }
    if draft.questions.len() > 128 {
        return Err(invalid("action questions exceed 128 members"));
    }
    let mut questions = BTreeSet::new();
    for question in &draft.questions {
        if !prefixed_id(&question.id, "question") || !questions.insert(&question.id) {
            return Err(invalid("invalid or duplicate action question ID"));
        }
        text(&question.text, 4096, false)?;
        if let Some(answer) = &question.answer {
            text(answer, 16_384, true)?;
        }
    }
    if draft.proposals.len() > 64 {
        return Err(invalid("action proposals exceed 64 members"));
    }
    let mut proposals = BTreeSet::new();
    for proposal in &draft.proposals {
        if !valid_proposal_id(&proposal.id)
            || proposals.contains(&proposal.id)
            || proposal
                .supersedes
                .as_ref()
                .is_some_and(|id| !proposals.contains(id))
        {
            return Err(invalid(
                "proposals require unique IDs and supersede only earlier history",
            ));
        }
        text(&proposal.summary, 16_384, false)?;
        crate::historical::validate_citations(&proposal.evidence)?;
        if !valid_hash(&proposal.context_fingerprint) || proposal.resolver != "human" {
            return Err(invalid(
                "proposal requires an exact context fingerprint and human resolver",
            ));
        }
        consequences::validate_plan(&proposal.consequences)?;
        if proposal.consequences.expected_fingerprint != proposal.context_fingerprint {
            return Err(invalid(
                "proposal plan must retain its original context fingerprint",
            ));
        }
        if proposal.question_answers.len() > 128 {
            return Err(invalid("proposal question snapshots exceed 128 members"));
        }
        let mut snapshots = BTreeSet::new();
        for answer in &proposal.question_answers {
            if !questions.contains(&answer.question_id) || !snapshots.insert(&answer.question_id) {
                return Err(invalid(
                    "proposal snapshots require unique existing question IDs",
                ));
            }
            text(&answer.answer, 16_384, true)?;
        }
        if !proposal
            .question_answers
            .iter()
            .map(|answer| &answer.question_id)
            .eq(draft
                .questions
                .iter()
                .filter(|question| snapshots.contains(&question.id))
                .map(|question| &question.id))
        {
            return Err(invalid(
                "proposal question snapshots must retain question order",
            ));
        }
        proposals.insert(&proposal.id);
    }
    if matches!(draft.phase.as_str(), "proposed" | "applied") && draft.proposals.is_empty() {
        return Err(invalid("a proposed action requires proposal history"));
    }
    match (&draft.applied_receipt, draft.phase.as_str()) {
        (Some(receipt), "applied") => crate::action_outcomes::validate_receipt(draft, receipt)?,
        (None, "applied") | (Some(_), _) => {
            return Err(invalid(
                "applied actions require exactly one applied receipt",
            ));
        }
        (None, _) => {}
    }
    if serde_json::to_vec(draft)?.len() + 1 > MAX_DOCUMENT {
        return Err(invalid("action document exceeds 256 KiB"));
    }
    Ok(())
}
fn draft_value(value: Json) -> Result<Draft> {
    if value.get("appliedReceipt").is_some_and(Json::is_null) {
        return Err(invalid(
            "appliedReceipt must be omitted or a closed receipt",
        ));
    }
    if let Some(receipt) = value.get("appliedReceipt") {
        // Optional before hashes distinguish creation (explicit null) from updates;
        // missing fields must not silently take serde's Option default.
        if receipt
            .get("files")
            .and_then(Json::as_array)
            .is_some_and(|files| files.iter().any(|file| file.get("beforeSha256").is_none()))
        {
            return Err(invalid(
                "receipt file entries require explicit beforeSha256",
            ));
        }
    }
    if let Some(questions) = value.get("questions").and_then(Json::as_array)
        && questions
            .iter()
            .any(|q| q.get("answer").is_some_and(|answer| !answer.is_string()))
    {
        return Err(invalid(
            "action answers must be omitted or explicit strings",
        ));
    }
    if value
        .get("proposals")
        .and_then(Json::as_array)
        .is_some_and(|proposals| {
            proposals
                .iter()
                .any(|proposal| proposal.get("supersedes").is_some_and(|id| !id.is_string()))
        })
    {
        return Err(invalid(
            "proposal supersedes must be omitted or an exact earlier ID",
        ));
    }
    let draft: Draft =
        serde_json::from_value(value).map_err(|_| invalid("invalid closed action document"))?;
    structural(&draft)?;
    Ok(draft)
}
pub fn decode(payload: &str) -> Result<Draft> {
    draft_value(parse(payload, MAX_DOCUMENT)?)
}
/// Deterministic native source; Markdown strings preserve their exact content.
pub fn serialize(draft: &Draft) -> Result<Vec<u8>> {
    structural(draft)?;
    let mut source = serde_json::to_vec(draft)?;
    source.push(b'\n');
    Ok(source)
}
pub fn validate(world: &World, draft: &Draft) -> Result<()> {
    structural(draft)?;
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "action source is invalid; use author validation for diagnostics".into(),
        ));
    }
    if draft.world_id != world.world_record()?.id {
        return Err(invalid("action belongs to a different world"));
    }
    world.story_time(
        draft.horizon.tick,
        Some(&draft.horizon.timeline),
        draft.horizon.order,
    )?;
    for (actors, references) in [(true, &draft.actor_refs), (false, &draft.target_refs)] {
        for reference in references {
            let record = briefs::resolve_reference_request(world, &reference.source()?)
                .map_err(|_| invalid("action reference is unavailable"))?;
            if actors && !["character", "organization"].contains(&record.kind.as_str()) {
                return Err(invalid(
                    "action actors require character or organization records",
                ));
            }
        }
    }
    // Hypotheses deliberately bypass the canonical brief resolver. They must
    // exist, but linking a possibility does not admit its subjects or prose.
    if draft
        .hypothesis_refs
        .iter()
        .any(|reference| match reference {
            Reference::Record { id } => world
                .records
                .get(id)
                .is_none_or(|record| record.kind != "hypothesis"),
            Reference::Section { .. } => true,
        })
    {
        return Err(invalid("action hypothesis reference is unavailable"));
    }
    Ok(())
}

/// Supersession retires only explicitly named earlier alternatives.
pub fn active_proposal_ids(draft: &Draft) -> Vec<String> {
    let superseded: BTreeSet<_> = draft
        .proposals
        .iter()
        .filter_map(|proposal| proposal.supersedes.as_deref())
        .collect();
    draft
        .proposals
        .iter()
        .filter(|proposal| !superseded.contains(proposal.id.as_str()))
        .map(|proposal| proposal.id.clone())
        .collect()
}

/// Derived read metadata never rewrites authored phase or original provenance.
pub fn effective_phase<'a>(draft: &'a Draft, current_fingerprint: &str) -> &'a str {
    if draft.phase == "proposed" {
        let superseded: BTreeSet<_> = draft
            .proposals
            .iter()
            .filter_map(|proposal| proposal.supersedes.as_deref())
            .collect();
        if draft.proposals.iter().any(|proposal| {
            !superseded.contains(proposal.id.as_str())
                && proposal.context_fingerprint != current_fingerprint
        }) {
            return "stale";
        }
    }
    &draft.phase
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequest {
    pub draft: Draft,
    pub frame: Frame,
    pub briefs: Vec<String>,
    pub families: FamilySelection,
    pub max_characters: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewConfirmation {
    pub frame: Frame,
    pub briefs: Vec<String>,
    pub families: FamilySelection,
    pub max_characters: usize,
    pub review_sha256: String,
    pub sufficient: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequest {
    pub expected_fingerprint: String,
    pub expected_sha256: Option<String>,
    pub draft: Draft,
    pub review: Option<ReviewConfirmation>,
}
fn selection(value: &Json) -> Result<(Frame, Vec<String>, FamilySelection, usize)> {
    let frame = writing_frames::decode(&serde_json::to_string(&value["frame"])?)?;
    let briefs: Vec<String> = serde_json::from_value(value["briefs"].clone())
        .map_err(|_| invalid("action briefs must be an explicit array of IDs"))?;
    if briefs.len() > 1024 {
        return Err(invalid("action briefs exceed 1024 members"));
    }
    let families = writing_frames::decode_families(&serde_json::to_string(&value["families"])?)?;
    let limit = value["maxCharacters"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| (256..=100_000).contains(n))
        .ok_or_else(|| invalid("action review maxCharacters must be from 256 to 100000"))?;
    Ok((frame, briefs, families, limit))
}
pub fn decode_review(payload: &str) -> Result<ReviewRequest> {
    let value = parse(payload, MAX_REQUEST)?;
    closed(
        &value,
        &["draft", "frame", "briefs", "families", "maxCharacters"],
        &[],
    )?;
    let (frame, briefs, families, max_characters) = selection(&value)?;
    Ok(ReviewRequest {
        draft: draft_value(value["draft"].clone())?,
        frame,
        briefs,
        families,
        max_characters,
    })
}
pub fn decode_save(payload: &str) -> Result<SaveRequest> {
    let value = parse(payload, MAX_REQUEST)?;
    closed(
        &value,
        &["expectedFingerprint", "expectedSha256", "draft", "review"],
        &[],
    )?;
    let expected_fingerprint = value["expectedFingerprint"]
        .as_str()
        .filter(|s| valid_hash(s))
        .ok_or_else(|| invalid("action save requires an exact current source fingerprint"))?
        .to_owned();
    let expected_sha256 = if value["expectedSha256"].is_null() {
        None
    } else {
        Some(
            value["expectedSha256"]
                .as_str()
                .filter(|s| valid_hash(s))
                .ok_or_else(|| {
                    invalid("action save hash must be null or an exact lowercase SHA-256")
                })?
                .to_owned(),
        )
    };
    let review = if value["review"].is_null() {
        None
    } else {
        let confirmation = &value["review"];
        closed(
            confirmation,
            &[
                "frame",
                "briefs",
                "families",
                "maxCharacters",
                "reviewSha256",
                "sufficient",
            ],
            &[],
        )?;
        let (frame, briefs, families, max_characters) = selection(confirmation)?;
        let review_sha256 = confirmation["reviewSha256"]
            .as_str()
            .filter(|s| valid_hash(s))
            .ok_or_else(|| invalid("action review requires its exact review hash"))?
            .to_owned();
        let sufficient = confirmation["sufficient"]
            .as_bool()
            .ok_or_else(|| invalid("action sufficiency must be an explicit boolean"))?;
        Some(ReviewConfirmation {
            frame,
            briefs,
            families,
            max_characters,
            review_sha256,
            sufficient,
        })
    };
    Ok(SaveRequest {
        expected_fingerprint,
        expected_sha256,
        draft: draft_value(value["draft"].clone())?,
        review,
    })
}
fn current_only(world: &World) -> Result<()> {
    if world.revision.is_some() {
        return Err(Error::Unsupported(
            "private action workflows require the current worktree".into(),
        ));
    }
    Ok(())
}
/// Readiness context never contains private intent or answers as admitted facts.
/// The explicitly selected frame supplies perspective; the draft supplies time.
pub fn review(world: &World, request: &ReviewRequest) -> Result<Json> {
    current_only(world)?;
    validate(world, &request.draft)?;
    if !(256..=100_000).contains(&request.max_characters) {
        return Err(invalid(
            "action review maxCharacters must be from 256 to 100000",
        ));
    }
    // Validate even native callers' complete frame before replacing its clock.
    let mut frame = writing_frames::decode(&serde_json::to_string(&request.frame)?)?;
    frame.at = request.draft.horizon.clone();
    writing_frames::with_author_snapshot(world, &frame, false, |authors| {
        let context = writing_frames::context_with_families(
            world,
            authors,
            &frame,
            &request.briefs,
            &request.families,
            1_000_000,
        )?;
        let unanswered: Vec<_> = request
            .draft
            .questions
            .iter()
            .filter(|q| q.required && q.answer.is_none())
            .map(|q| q.id.clone())
            .collect();
        let mut result = json!({"protocol":crate::CONTRACT,"frame":frame,
        "draftSha256":sha256(&serialize(&request.draft)?),
        "requiredAnswered":unanswered.is_empty(),"unansweredRequired":unanswered,
        "context":context,"maxCharacters":request.max_characters});
        // Bind the exact supplied selection as well as the complete rendered review.
        // This digest is an integrity check, never an outcome approval capability.
        let material = json!({"review":result,"briefs":request.briefs,"families":request.families});
        let mut bytes = b"wedl-action-review\0".to_vec();
        bytes.extend(serde_json::to_vec(&material)?);
        result["reviewSha256"] = json!(sha256(&bytes));
        let needed = serde_json::to_string(&result)?.chars().count();
        if needed > request.max_characters {
            return Err(invalid(&format!(
                "complete action review needs {needed} characters; maxCharacters is {}",
                request.max_characters
            )));
        }
        Ok(result)
    })
}
pub fn validate_save(world: &World, request: &SaveRequest, previous: Option<&Draft>) -> Result<()> {
    current_only(world)?;
    if request.draft.phase == "applied" || request.draft.applied_receipt.is_some() {
        return Err(invalid(
            "applied receipts can only be created by reviewed outcome application",
        ));
    }
    validate(world, &request.draft)?;
    if !valid_hash(&request.expected_fingerprint)
        || request
            .expected_sha256
            .as_ref()
            .is_some_and(|s| !valid_hash(s))
    {
        return Err(invalid(
            "action save requires exact source and prior-byte bindings",
        ));
    }
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "action source changed; explicitly capture and review again".into(),
        ));
    }
    let draft = &request.draft;
    match previous {
        None => {
            if draft.phase != "draft"
                || !draft.proposals.is_empty()
                || draft.base_fingerprint != world.fingerprint
            {
                return Err(invalid(
                    "a new action must start as draft without proposals at the current original source fingerprint",
                ));
            }
        }
        Some(previous) => {
            structural(previous)?;
            if draft.id != previous.id
                || draft.world_id != previous.world_id
                || draft.base_fingerprint != previous.base_fingerprint
            {
                return Err(invalid(
                    "action identity and original provenance are immutable",
                ));
            }
            let allowed = match previous.phase.as_str() {
                "draft" => &[
                    "draft",
                    "context-needed",
                    "resolution-needed",
                    "stale",
                    "cancelled",
                ][..],
                "context-needed" => {
                    &["context-needed", "resolution-needed", "stale", "cancelled"][..]
                }
                "resolution-needed" => &["resolution-needed", "proposed", "stale", "cancelled"][..],
                "proposed" => &["proposed", "resolution-needed", "stale", "cancelled"][..],
                "stale" => &["stale", "context-needed", "proposed", "cancelled"][..],
                _ => &[][..],
            };
            if !allowed.contains(&draft.phase.as_str()) {
                return Err(invalid("forbidden or terminal action lifecycle transition"));
            }
            if previous.phase == "proposed" && draft.phase != "stale" {
                let mut content = draft.clone();
                content.phase = previous.phase.clone();
                if content != *previous {
                    return Err(invalid(
                        "editing proposed action content requires the stale phase",
                    ));
                }
            }
            if !draft.proposals.starts_with(&previous.proposals) {
                return Err(invalid("prior proposals are immutable append-only history"));
            }
            let added = &draft.proposals[previous.proposals.len()..];
            if !added.is_empty()
                && (!matches!(previous.phase.as_str(), "resolution-needed" | "stale")
                    || draft.phase != "proposed")
            {
                return Err(invalid(
                    "appending proposals requires an explicit resolution transition",
                ));
            }
            if draft.phase == "proposed" && previous.phase != "proposed" && added.is_empty() {
                return Err(invalid(
                    "proposing requires newly captured proposal provenance",
                ));
            }
        }
    }
    let added = &draft.proposals[previous.map_or(0, |previous| previous.proposals.len())..];
    if matches!(draft.phase.as_str(), "resolution-needed" | "proposed")
        && draft
            .questions
            .iter()
            .any(|q| q.required && q.answer.is_none())
    {
        return Err(invalid("required action questions are unanswered"));
    }
    if !added.is_empty() {
        let answers: Vec<_> = draft
            .questions
            .iter()
            .filter_map(|question| {
                question.answer.as_ref().map(|answer| QuestionAnswer {
                    question_id: question.id.clone(),
                    answer: answer.clone(),
                })
            })
            .collect();
        for proposal in added {
            if proposal.context_fingerprint != world.fingerprint
                || proposal.question_answers != answers
            {
                return Err(invalid(
                    "new proposals must capture the current exact context and ordered answers",
                ));
            }
            // The complete candidate must be constructible. Its diagnostic
            // report may remain invalid: a private proposal is not approval.
            consequences::preview(world, &proposal.consequences, 1_000_000)?;
        }
    }
    if draft.phase == "resolution-needed" || !added.is_empty() {
        let confirmation = request
            .review
            .as_ref()
            .filter(|r| r.sufficient)
            .ok_or_else(|| {
                invalid(
                    "resolution or proposal needs a current review and explicit human sufficiency confirmation",
                )
            })?;
        let reviewed = review(
            world,
            &ReviewRequest {
                draft: draft.clone(),
                frame: confirmation.frame.clone(),
                briefs: confirmation.briefs.clone(),
                families: confirmation.families.clone(),
                max_characters: confirmation.max_characters,
            },
        )?;
        if reviewed["reviewSha256"].as_str() != Some(confirmation.review_sha256.as_str()) {
            return Err(Error::Conflict(
                "action review changed; review the exact draft and context again".into(),
            ));
        }
    } else if request.review.is_some() {
        return Err(invalid(
            "human review confirmation belongs only to resolution-needed or newly appended proposal saves",
        ));
    }
    Ok(())
}
