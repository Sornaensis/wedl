//! Typed private archive views over the native manuscript and review contracts.
use super::{Handles, choice, closed, field, invalid, unavailable};
use crate::{
    Error, Result,
    consequence_semantics::Time,
    historical, manuscript,
    model::{StoryTime, World},
    reader, retcon_reviews, revision_review,
    service::Service,
    validation, writing_frames,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MANUSCRIPT_SAVE: &str = "/api/manuscripts/save";
const RETCON_SAVE: &str = "/api/retcon-reviews/save";
const SOURCE: &str = "archive-source";
const DOCUMENT: &str = "archive-document";
const POSITION: &str = "archive-position";
const REVIEW: &str = "archive-review";
const DRAFT: &str = "archive-review-draft";
const MOMENT: &str = "archive-moment";
const MANUSCRIPT_EDIT: &str = "archive-manuscript-edit";
const MANUSCRIPT_PROOF: &str = "archive-manuscript-proof";

type Query = BTreeMap<String, String>;

fn bind<T: Serialize>(handles: &mut Handles, kind: &'static str, value: &T) -> Result<String> {
    handles.issue(kind, serde_json::to_string(value)?)
}

fn resolve<T: DeserializeOwned>(handles: &Handles, kind: &'static str, token: &str) -> Result<T> {
    serde_json::from_str(&handles.resolve(kind, token)?).map_err(|_| unavailable())
}

fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name].as_str().ok_or_else(unavailable)
}

fn rows<'a>(value: &'a Value, name: &str) -> Result<&'a [Value]> {
    value[name]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(unavailable)
}

fn machine_word(word: &str) -> bool {
    let private_id = |value: &str| {
        value.rsplit_once('_').is_some_and(|(prefix, suffix)| {
            matches!(
                prefix,
                "work" | "segment" | "reveal" | "review" | "chapter" | "proposal" | "draft"
            ) && suffix.len() == 26
                && suffix
                    .bytes()
                    .all(|ch| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&ch))
        })
    };
    if word.split([':', '/', '.', '\\']).any(|part| {
        (matches!(part.len(), 40 | 64) && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
            || (part.contains('_') && validation::valid_record_id(part))
            || private_id(part)
    }) {
        return true;
    }
    if word.contains('/')
        && (word.contains("story/")
            || word.contains("author/")
            || word.ends_with(".md")
            || word.ends_with(".json"))
        || word.contains('\\')
    {
        return true;
    }
    word.match_indices(':').any(|(offset, _)| {
        let from = word[..offset]
            .rfind([':', '/', '.', '\\'])
            .map_or(0, |index| index + 1);
        matches!(
            &word[from..offset],
            "location" | "map" | "anchor" | "portal" | "route" | "overlay"
        ) && word[offset + 1..]
            .split([':', '.', '\\'])
            .next()
            .is_some_and(|rest| validation::valid_record_id(&word[from..offset + 1 + rest.len()]))
    })
}

fn display(value: &str, limit: usize) -> String {
    let mut output = String::new();
    let mut start = 0;
    let mut remaining = limit;
    for (end, ch) in value
        .char_indices()
        .chain(std::iter::once((value.len(), ' ')))
    {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '/' | '.' | '\\') {
            continue;
        }
        let word = &value[start..end];
        let core = word.trim_end_matches('.');
        let parts = if machine_word(core) {
            ["Reference", &word[core.len()..]]
        } else {
            [word, ""]
        };
        for ch in parts.into_iter().flat_map(str::chars).take(remaining) {
            output.push(ch);
            remaining -= 1;
        }
        if remaining > 0 && end < value.len() && (!ch.is_control() || matches!(ch, '\n' | '\t')) {
            output.push(ch);
            remaining -= 1;
        }
        start = end + ch.len_utf8();
        if remaining == 0 {
            break;
        }
    }
    output
}

fn label(value: &str) -> String {
    let value = display(value, 180);
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn source_name(revision: Option<&str>) -> &'static str {
    if revision.is_some() {
        "Original snapshot"
    } else {
        "Working files"
    }
}

fn sources(service: &Service, handles: &mut Handles) -> Result<Value> {
    let world = service.world();
    let history = world
        .revision
        .clone()
        .map_or_else(|| historical::head(&world.root), Ok)
        .and_then(|head| historical::recent_named_revisions(&world.root, &head));
    source_choices(world.revision.is_none(), history, handles)
}

fn source_choices(
    working_files: bool,
    history: Result<Vec<(String, String)>>,
    handles: &mut Handles,
) -> Result<Value> {
    let mut items = Vec::new();
    if working_files {
        items.push(choice(
            handles,
            SOURCE,
            "Working files",
            serde_json::to_string(&None::<String>)?,
        )?);
    }
    let originals_available = history.as_ref().is_ok_and(|entries| !entries.is_empty());
    if let Ok(history) = history {
        for (index, (revision, subject)) in history.into_iter().enumerate() {
            let subject = label(&subject);
            let name = if subject.is_empty() {
                format!("Original snapshot {}", index + 1)
            } else {
                format!("{subject} · snapshot {}", index + 1)
            };
            items.push(choice(
                handles,
                SOURCE,
                &name,
                serde_json::to_string(&Some(revision))?,
            )?);
        }
    }
    Ok(json!({"items":items,"workingFiles":working_files,"originalsAvailable":originals_available}))
}

fn document_kind(kind: &str) -> Result<&'static str> {
    match kind {
        "work" => Ok("Private work"),
        "manuscript-segment" => Ok("Manuscript segment"),
        "reveal-plan" => Ok("Reveal plan"),
        _ => Err(unavailable()),
    }
}

#[derive(Serialize, Deserialize)]
struct DocumentPin {
    request: manuscript::DetailRequest,
    kind: String,
    author_fingerprint: String,
}

#[derive(Serialize, Deserialize)]
struct PositionPin {
    document: DocumentPin,
    work: String,
    ordinal: usize,
    simulation_at: Option<Time>,
}

#[derive(Serialize, Deserialize)]
struct ManuscriptEdit {
    id: String,
    fingerprint: String,
}

#[derive(Serialize, Deserialize)]
struct ManuscriptProof {
    edit: ManuscriptEdit,
    after_sha256: String,
}

fn document_pin(
    revision: Option<String>,
    author_fingerprint: &str,
    row: &Value,
) -> Result<DocumentPin> {
    Ok(DocumentPin {
        request: manuscript::DetailRequest {
            revision,
            id: string(row, "id")?.into(),
            expected_sha256: Some(string(row, "sourceSha256")?.into()),
        },
        kind: string(row, "kind")?.into(),
        author_fingerprint: author_fingerprint.into(),
    })
}

fn documents(service: &Service, revision: Option<String>, handles: &mut Handles) -> Result<Value> {
    let value = manuscript::list(
        &service.world().root,
        &manuscript::ListRequest {
            revision: revision.clone(),
            kind: None,
            max_items: manuscript::MAX_DOCUMENTS,
        },
    )?;
    let fingerprint = string(&value, "authorFingerprint")?;
    let mut items = Vec::new();
    for row in rows(&value, "items")? {
        let pin = document_pin(revision.clone(), fingerprint, row)?;
        items.push(json!({
            "label":label(string(row, "title")?),
            "kind":document_kind(&pin.kind)?,
            "work":pin.kind == "work",
            "plan":pin.kind == "reveal-plan",
            "token":bind(handles, DOCUMENT, &pin)?,
        }));
    }
    items.sort_by(|left, right| left["label"].as_str().cmp(&right["label"].as_str()));
    Ok(
        json!({"items":items,"source":source_name(revision.as_deref()),"editable":revision.is_none()}),
    )
}

fn exact_document(service: &Service, pin: &DocumentPin) -> Result<Value> {
    let detail = manuscript::detail(&service.world().root, &pin.request)?;
    if detail["authorFingerprint"].as_str() != Some(&pin.author_fingerprint)
        || detail["document"]["kind"].as_str() != Some(&pin.kind)
    {
        return Err(Error::Conflict("Private manuscript source changed".into()));
    }
    Ok(detail)
}

fn document_view(service: &Service, pin: &DocumentPin, handles: &mut Handles) -> Result<Value> {
    let detail = exact_document(service, pin)?;
    let document = &detail["document"];
    Ok(json!({
        "label":label(string(document, "title")?),
        "kind":document_kind(&pin.kind)?,
        "prose":display(string(document, "bodyMarkdown")?, manuscript::MAX_BODY_BYTES),
        "editable":pin.request.revision.is_none(),
        "source":source_name(pin.request.revision.as_deref()),
        "file":handles.issue("native-file", string(document, "rawSource")?.into())?,
    }))
}

fn manuscript_edit(service: &Service, pin: &DocumentPin, handles: &mut Handles) -> Result<Value> {
    let world = service.world();
    if world.revision.is_some() || pin.request.revision.is_some() {
        return Err(unavailable());
    }
    let detail = exact_document(service, pin)?;
    let document = &detail["document"];
    let request = manuscript::SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_author_fingerprint: pin.author_fingerprint.clone(),
        expected_sha256: pin.request.expected_sha256.clone(),
        source: string(document, "rawSource")?.into(),
    };
    let edit = ManuscriptEdit {
        id: pin.request.id.clone(),
        fingerprint: world.fingerprint.clone(),
    };
    Ok(
        json!({"template":handles.request(MANUSCRIPT_SAVE, serde_json::to_string(&request)?)?,
        "edit":bind(handles, MANUSCRIPT_EDIT, &edit)?,"bodyBytes":string(document, "bodyMarkdown")?.len()}),
    )
}

fn manuscript_proof(
    edit: ManuscriptEdit,
    after_sha256: &str,
    handles: &mut Handles,
) -> Result<Value> {
    if !crate::action_drafts::valid_hash(after_sha256) {
        return Err(invalid());
    }
    let proof = ManuscriptProof {
        edit,
        after_sha256: after_sha256.into(),
    };
    Ok(json!({"token":bind(handles, MANUSCRIPT_PROOF, &proof)?}))
}

/// A lost acknowledgement is confirmed only by the independently reloaded
/// exact target bytes and matching canonical source, never by an HTTP retry.
pub(super) fn inspect(
    service: &Service,
    route: &str,
    token: &str,
    handles: &Handles,
) -> Result<Value> {
    let world = service.world();
    if world.revision.is_some() {
        return Err(unavailable());
    }
    let verified = match route {
        MANUSCRIPT_SAVE => {
            let proof: ManuscriptProof = resolve(handles, MANUSCRIPT_PROOF, token)?;
            proof.edit.fingerprint == world.fingerprint
                && manuscript::detail(
                    &world.root,
                    &manuscript::DetailRequest {
                        revision: None,
                        id: proof.edit.id,
                        expected_sha256: Some(proof.after_sha256),
                    },
                )
                .is_ok()
        }
        RETCON_SAVE => {
            let request = retcon_reviews::decode_save(&handles.resolve(RETCON_SAVE, token)?)?;
            let after_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(&request.document)?));
            request.expected_fingerprint == world.fingerprint
                && retcon_reviews::detail(
                    &world.root,
                    &retcon_reviews::DetailRequest {
                        revision: None,
                        id: request.document.id,
                        expected_sha256: Some(after_sha256),
                    },
                )
                .is_ok()
        }
        _ => return Err(invalid()),
    };
    Ok(json!({"verified":verified}))
}

fn work_positions(service: &Service, pin: DocumentPin, handles: &mut Handles) -> Result<Value> {
    if pin.kind != "work" {
        return Err(unavailable());
    }
    exact_document(service, &pin)?;
    let value = manuscript::resolve(
        &service.world().root,
        &manuscript::ResolveRequest {
            revision: pin.request.revision.clone(),
            work_id: pin.request.id.clone(),
            expected_author_fingerprint: Some(pin.author_fingerprint.clone()),
            max_occurrences: 4096,
            max_characters: 1_000_000,
        },
    )?;
    let simulation_at = pin
        .request
        .revision
        .as_deref()
        .map(|revision| {
            historical::load_world(&service.world().root, revision)
                .and_then(|world| world.current_time())
                .map(time)
        })
        .transpose()?;
    let segments: BTreeMap<_, _> = rows(&value, "segments")?
        .iter()
        .map(|segment| Ok((string(segment, "id")?, segment)))
        .collect::<Result<_>>()?;
    let mut items = Vec::new();
    for chapter in rows(&value, "chapters")? {
        let chapter_name = label(string(chapter, "title")?);
        for (index, occurrence) in rows(chapter, "occurrences")?.iter().enumerate() {
            let segment = segments
                .get(string(occurrence, "segmentId")?)
                .ok_or_else(unavailable)?;
            let name = label(string(segment, "title")?);
            let pin = PositionPin {
                document: document_pin(
                    pin.request.revision.clone(),
                    &pin.author_fingerprint,
                    segment,
                )?,
                work: pin.request.id.clone(),
                ordinal: occurrence["ordinal"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(unavailable)?,
                simulation_at: simulation_at.clone(),
            };
            items.push(
                json!({"label":format!("{chapter_name} · {name} · Position {}", index + 1),
                "token":bind(handles, POSITION, &pin)?}),
            );
        }
    }
    Ok(json!({"items":items}))
}

pub(super) fn prepare_simulation(
    service: &Service,
    query: &Query,
    handles: &mut Handles,
) -> Result<reader::Prepared> {
    closed(query, &["position", "plan", "text"])?;
    let pin: PositionPin = resolve(handles, POSITION, field(query, "position")?)?;
    let plan: DocumentPin = resolve(handles, DOCUMENT, field(query, "plan")?)?;
    let revision = pin
        .document
        .request
        .revision
        .as_deref()
        .ok_or_else(unavailable)?;
    if plan.kind != "reveal-plan"
        || plan.request.revision.as_deref() != Some(revision)
        || plan.author_fingerprint != pin.document.author_fingerprint
    {
        return Err(unavailable());
    }
    // Both selectors were issued from the same immutable private snapshot.
    // Reader checks simulation authority before doing any source reads and
    // rechecks the same installation before emitting the projection.
    let at = pin.simulation_at.ok_or_else(unavailable)?;
    let text_policy = match query.get("text").map(String::as_str) {
        None | Some("none") => "none",
        Some("admitted") => "all-at-pinned-plan",
        _ => return Err(invalid()),
    };
    let request = json!({"protocol":reader::PROTOCOL,"revision":revision,
        "workId":pin.work,"planId":plan.request.id,"occurrence":pin.ordinal,
        "operation":"page","at":at,"textPolicy":text_policy,"limit":100,"cursor":null});
    service
        .reader()
        .prepare_preview(&serde_json::to_vec(&request)?)
}

pub(super) fn present_simulation(value: Value) -> Result<Value> {
    if value["mode"] != "author-preview" || value["outcome"] != "available" {
        return Err(unavailable());
    }
    let items = rows(&value, "items")?.iter().map(|row| Ok(json!({
        "label":row["title"].as_str().map(label).unwrap_or_else(|| "Admitted passage".into()),
        "prose":display(string(row, "markdown")?, reader::MAX_MARKDOWN_BYTES),
    }))).collect::<Result<Vec<_>>>()?;
    Ok(json!({"items":items,"moreAvailable":value["nextCursor"].is_string()}))
}

fn status(value: &Value) -> &'static str {
    match value["currentness"]["status"].as_str() {
        Some("current") => "Current",
        Some("stale") => "Stale",
        _ => "Unknown",
    }
}

fn review_queue(
    service: &Service,
    revision: Option<String>,
    handles: &mut Handles,
) -> Result<Value> {
    let value = retcon_reviews::queue(
        &service.world().root,
        &retcon_reviews::QueueRequest {
            revision: revision.clone(),
            max_items: 1000,
        },
    )?;
    let mut items = Vec::new();
    for item in rows(&value, "items")? {
        let request = retcon_reviews::DetailRequest {
            revision: revision.clone(),
            id: string(item, "id")?.into(),
            expected_sha256: Some(string(item, "sourceSha256")?.into()),
        };
        items.push(
            json!({"label":label(string(item, "title")?), "state":status(item),
            "pending":item["pendingCount"].as_u64(),"token":bind(handles, REVIEW, &request)?}),
        );
    }
    items.sort_by(|left, right| left["label"].as_str().cmp(&right["label"].as_str()));
    Ok(json!({"items":items,"source":source_name(revision.as_deref())}))
}

#[derive(Serialize, Deserialize)]
enum DraftSource {
    Retained(retcon_reviews::DetailRequest),
    Captured(Box<retcon_reviews::Document>),
}

#[derive(Serialize, Deserialize)]
struct ReviewDraft {
    source: DraftSource,
    fingerprint: String,
}

fn relation(value: Option<&str>) -> &'static str {
    match value {
        Some("cause") => "Causal link",
        Some("acquisition") => "Knowledge acquisition",
        Some("typed-entity") => "Typed record reference",
        Some("effect") => "Effect",
        Some("participant") => "Participation",
        Some("outcome") => "Story outcome",
        Some("location-link" | "spatial-history" | "spatial") => "Spatial dependency",
        Some("assertion" | "predicate") => "Assertion evidence",
        Some("genealogy-reference" | "genealogy-evidence" | "tenure") => "Family and affiliation",
        Some("concept") => "Concept link",
        Some("authored-reference" | "source-member" | "reference") => "Authored reference",
        Some("pin" | "brief-scope" | "correction") => "Source dependency",
        Some("manuscript-source" | "manuscript-pov" | "work-segment") => "Manuscript dependency",
        _ => "Dependency",
    }
}

fn owner_id(owner: &Value) -> Option<&str> {
    ["id", "owner_id", "entity_id"]
        .into_iter()
        .find_map(|key| owner[key].as_str())
}

fn owner_label(original: Option<&World>, owner: &Value) -> String {
    original
        .and_then(|world| world.records.get(owner_id(owner)?))
        .map(|record| label(&record.title))
        .unwrap_or_else(|| "Affected record".into())
}

fn review_view(
    service: &Service,
    document: &retcon_reviews::Document,
    compared: &'static str,
    draft: Option<&ReviewDraft>,
    file: Option<String>,
    handles: &mut Handles,
) -> Result<Value> {
    let report = rows(&document.analysis.report, "items")?;
    if report.len() != document.analysis.item_digests.len() {
        return Err(unavailable());
    }
    let original =
        historical::load_world(&service.world().root, &document.analysis.base_revision).ok();
    let admitted_originals: BTreeSet<_> = report
        .iter()
        .filter(|item| item["snapshot"] == "base")
        .filter_map(|item| owner_id(&item["owner"]))
        .collect();
    let decisions: BTreeMap<_, _> = document
        .decisions
        .iter()
        .map(|row| (&row.item_digest, row))
        .collect();
    let impacts = report
        .iter()
        .zip(&document.analysis.item_digests)
        .map(|(item, digest)| {
            let decision = decisions.get(digest);
            let visible_original = original.as_ref().filter(|_| {
                owner_id(&item["owner"]).is_some_and(|id| admitted_originals.contains(id))
            });
            json!({
                "record":owner_label(visible_original, &item["owner"]),
                "relation":relation(item["relation"].as_str()),
                "source":if item["snapshot"] == "base" { "Original" } else { "Candidate" },
                "disposition":decision.map(|entry| entry.disposition),
                "note":decision.map(|entry| display(&entry.note, 16 * 1024)).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let token = draft.map(|draft| bind(handles, DRAFT, draft)).transpose()?;
    let file = file
        .map(|bytes| handles.issue("native-file", bytes))
        .transpose()?;
    Ok(
        json!({"label":label(&document.title),"original":"Fixed original analysis",
        "comparison":compared,"impacts":impacts,"draft":token,"file":file}),
    )
}

fn review_detail(
    service: &Service,
    request: retcon_reviews::DetailRequest,
    handles: &mut Handles,
    edit: bool,
) -> Result<Value> {
    let value = retcon_reviews::detail(&service.world().root, &request)?;
    let document = serde_json::from_value(value["document"].clone()).map_err(|_| unavailable())?;
    let editable = request.revision.is_none() && service.world().revision.is_none();
    let draft = if edit {
        if !editable {
            return Err(unavailable());
        }
        Some(ReviewDraft {
            source: DraftSource::Retained(request),
            fingerprint: service.world().fingerprint.clone(),
        })
    } else {
        None
    };
    let mut view = review_view(
        service,
        &document,
        status(&value),
        draft.as_ref(),
        Some(string(&value, "source")?.into()),
        handles,
    )?;
    view["editable"] = editable.into();
    Ok(view)
}

fn review_id() -> Result<String> {
    let mut bytes = [0u8; 26];
    getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
    let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let suffix: String = bytes
        .into_iter()
        .map(|byte| char::from(alphabet[usize::from(byte & 31)]))
        .collect();
    Ok(format!("review_{suffix}"))
}

fn capture_review(service: &Service, raw: &str, handles: &mut Handles) -> Result<Value> {
    if service.world().revision.is_some() {
        return Err(unavailable());
    }
    let input = retcon_reviews::decode_preview(raw)?;
    let preview = retcon_reviews::preview(&service.world().root, &input)?;
    if preview["comparedFingerprint"].as_str() != Some(&service.world().fingerprint) {
        return Err(Error::Conflict("Workbench source changed".into()));
    }
    let document = retcon_reviews::Document {
        id: review_id()?,
        title: "Retcon review".into(),
        analysis: serde_json::from_value(preview["analysis"].clone()).map_err(|_| unavailable())?,
        decisions: Vec::new(),
        body_markdown: String::new(),
    };
    let draft = ReviewDraft {
        source: DraftSource::Captured(Box::new(document.clone())),
        fingerprint: service.world().fingerprint.clone(),
    };
    review_view(service, &document, "Current", Some(&draft), None, handles)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewChange {
    index: usize,
    #[serde(deserialize_with = "required_nullable")]
    disposition: Option<retcon_reviews::Disposition>,
    #[serde(deserialize_with = "required_nullable")]
    note: Option<String>,
}

fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::deserialize(decoder)
}

fn changed_note(original: &str, note: Option<String>) -> Result<String> {
    let Some(note) = note else {
        return Ok(original.into());
    };
    let bytes = original.as_bytes();
    let mut crlf = false;
    let mut lf = false;
    let mut cr = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'\r' {
            if bytes.get(index + 1) == Some(&b'\n') {
                crlf = true;
            } else {
                cr = true;
            }
        } else if byte == b'\n' && (index == 0 || bytes[index - 1] != b'\r') {
            lf = true;
        }
    }
    if u8::from(crlf) + u8::from(lf) + u8::from(cr) > 1 {
        return Err(invalid());
    }
    let note = if crlf || cr {
        note.replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', if crlf { "\r\n" } else { "\r" })
    } else {
        note
    };
    if note.len() > 16 * 1024 {
        return Err(invalid());
    }
    Ok(note)
}

fn apply_decisions(
    document: &mut retcon_reviews::Document,
    changes: Vec<ReviewChange>,
) -> Result<()> {
    let mut decisions: BTreeMap<_, _> = std::mem::take(&mut document.decisions)
        .into_iter()
        .map(|row| (row.item_digest.clone(), row))
        .collect();
    let mut seen = BTreeSet::new();
    for change in changes {
        let digest = document
            .analysis
            .item_digests
            .get(change.index)
            .ok_or_else(invalid)?;
        if !seen.insert(change.index)
            || change.disposition.is_none()
                && change.note.as_deref().is_some_and(|note| !note.is_empty())
        {
            return Err(invalid());
        }
        if let Some(disposition) = change.disposition {
            let previous = decisions
                .get(digest)
                .map(|row| row.note.as_str())
                .unwrap_or_default();
            let note = changed_note(previous, change.note)?;
            decisions.insert(
                digest.clone(),
                retcon_reviews::Decision {
                    item_digest: digest.clone(),
                    disposition,
                    note,
                },
            );
        } else {
            decisions.remove(digest);
        }
    }
    document.decisions = document
        .analysis
        .item_digests
        .iter()
        .filter_map(|digest| decisions.remove(digest))
        .collect();
    Ok(())
}

fn prepare_review(
    service: &Service,
    draft: ReviewDraft,
    raw: &str,
    title: Option<&str>,
    handles: &mut Handles,
) -> Result<Value> {
    let world = service.world();
    if world.revision.is_some() || world.fingerprint != draft.fingerprint {
        return Err(Error::Conflict("Workbench source changed".into()));
    }
    let (mut document, expected_sha256) = match draft.source {
        DraftSource::Retained(request) => {
            if request.revision.is_some() {
                return Err(unavailable());
            }
            let value = retcon_reviews::detail(&world.root, &request)?;
            (
                serde_json::from_value(value["document"].clone()).map_err(|_| unavailable())?,
                request.expected_sha256,
            )
        }
        DraftSource::Captured(document) => (*document, None),
    };
    let changes = manuscript::decode_closed_json(raw, 20 * 1024)?;
    apply_decisions(&mut document, changes)?;
    if let Some(title) = title {
        document.title = title.into();
    }
    let request = retcon_reviews::SaveRequest {
        expected_fingerprint: draft.fingerprint,
        expected_sha256,
        document,
    };
    let preview = retcon_reviews::save_preview(world, &request)?;
    if preview["canSave"] != true || preview["writesPerformed"] != false {
        return Err(unavailable());
    }
    let raw = serde_json::to_string(&request)?;
    Ok(json!({"label":"Private review checked",
        "request":handles.request(RETCON_SAVE,raw.clone())?,
        "file":handles.issue("native-file",raw)?}))
}

#[derive(Serialize, Deserialize)]
struct RevisionMoment {
    revision: String,
    at: Time,
}

fn time(at: StoryTime) -> Time {
    Time {
        timeline: at.timeline,
        tick: at.tick.to_string(),
        order: at.order.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SEGMENT: &str = "segment_00000000000000000000000001";
    const WORK: &str = "work_00000000000000000000000001";
    const CHAPTER: &str = "chapter_00000000000000000000000001";

    #[test]
    fn unavailable_original_ancestry_does_not_discard_working_files() {
        let missing_parent = || Error::NotFound("Original first parent is unavailable".into());
        let mut handles = Handles::default();
        let live = source_choices(true, Err(missing_parent()), &mut handles).unwrap();
        assert_eq!(live["originalsAvailable"], false);
        assert_eq!(live["items"][0]["label"], "Working files");
        assert_eq!(live["items"].as_array().unwrap().len(), 1);
        let token = live["items"][0]["token"].as_str().unwrap();
        assert_eq!(
            resolve::<Option<String>>(&handles, SOURCE, token).unwrap(),
            None
        );
        let fixed = source_choices(false, Err(missing_parent()), &mut handles).unwrap();
        assert_eq!(fixed["originalsAvailable"], false);
        assert_eq!(fixed["items"], json!([]));
    }

    fn get(
        service: &Service,
        handles: &mut Handles,
        endpoint: &str,
        fields: &[(&str, &str)],
    ) -> Result<Value> {
        let query = fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        read(service, endpoint, &query, handles)
    }

    #[test]
    fn named_private_selection_pins_exact_source_and_keeps_machine_fields_off_the_wire() {
        let temp = tempfile::tempdir().unwrap();
        let world = crate::source::init_world(&temp.path().join("world"), "Archive test").unwrap();
        let segment_path = world
            .root
            .join(manuscript::document_path("manuscript-segment", SEGMENT).unwrap());
        let work_path = world
            .root
            .join(manuscript::document_path("work", WORK).unwrap());
        for path in [&segment_path, &work_path] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        let source = format!(
            "---\r\nschema: wedl-author/v1\r\nkind: manuscript-segment\r\nid: {SEGMENT}\r\ntitle: Landing at dawn\r\nsource_links: []\r\n---\r\nExact private prose 🌲\r\nTrailing  "
        );
        fs::write(&segment_path, &source).unwrap();
        fs::write(work_path, format!("---\nschema: wedl-author/v1\nkind: work\nid: {WORK}\ntitle: Harbor work\nduplicate_policy: allow-explicit-reuse\nchapters:\n - id: {CHAPTER}\n   title: First chapter\n   segments:\n    - {{kind: author-record, id: {SEGMENT}}}\n    - {{kind: author-record, id: {SEGMENT}}}\n---\nPrivate commentary.\n")).unwrap();
        let fingerprint = world.fingerprint.clone();
        let service = Service::new(world);
        let mut handles = Handles::default();
        let roots = get(&service, &mut handles, "archive-sources", &[]).unwrap();
        let source_choice = roots["items"][0]["token"].as_str().unwrap();
        let list = get(
            &service,
            &mut handles,
            "archive-documents",
            &[("source", source_choice)],
        )
        .unwrap();
        assert!(!list.to_string().contains(SEGMENT));
        assert!(!list.to_string().contains(WORK));
        let items = rows(&list, "items").unwrap();
        let work = items.iter().find(|item| item["work"] == true).unwrap()["token"]
            .as_str()
            .unwrap();
        let positions = get(
            &service,
            &mut handles,
            "archive-work-positions",
            &[("document", work)],
        )
        .unwrap();
        let positions = rows(&positions, "items").unwrap();
        assert_eq!(positions.len(), 2);
        assert_ne!(positions[0]["token"], positions[1]["token"]);
        assert!(
            positions[0]["label"]
                .as_str()
                .unwrap()
                .contains("Landing at dawn")
        );
        let position = positions[1]["token"].as_str().unwrap();
        let detail = get(
            &service,
            &mut handles,
            "archive-position",
            &[("position", position)],
        )
        .unwrap();
        assert_eq!(detail["label"], "Landing at dawn");
        assert!(!detail.to_string().contains(SEGMENT));
        assert_eq!(
            handles
                .resolve("native-file", detail["file"].as_str().unwrap())
                .unwrap(),
            source
        );
        let document = items
            .iter()
            .find(|item| item["kind"] == "Manuscript segment")
            .unwrap()["token"]
            .as_str()
            .unwrap();
        let edit = get(
            &service,
            &mut handles,
            "archive-manuscript-edit",
            &[("document", document)],
        )
        .unwrap();
        assert!(!edit.to_string().contains(&fingerprint));
        let exact = handles
            .resolve(MANUSCRIPT_SAVE, edit["template"].as_str().unwrap())
            .unwrap();
        let exact = manuscript::decode_save(&exact).unwrap();
        assert_eq!(exact.source, source);
        assert_eq!(exact.expected_fingerprint, fingerprint);
        assert_eq!(
            manuscript::save_preview(service.world(), &exact).unwrap()["canSave"],
            true
        );
        let changed = source.replace("Exact", "New");
        let sha = hex::encode(Sha256::digest(changed.as_bytes()));
        let proof = get(
            &service,
            &mut handles,
            MANUSCRIPT_PROOF,
            &[("edit", edit["edit"].as_str().unwrap()), ("sha256", &sha)],
        )
        .unwrap();
        let proof = proof["token"].as_str().unwrap();
        assert_eq!(
            inspect(&service, MANUSCRIPT_SAVE, proof, &handles).unwrap()["verified"],
            false
        );
        let mut exact = exact;
        exact.source = changed.clone();
        assert_eq!(
            manuscript::save(service.world(), &exact).unwrap()["writesPerformed"],
            true
        );
        assert_eq!(fs::read_to_string(segment_path).unwrap(), changed);
        assert_eq!(
            inspect(&service, MANUSCRIPT_SAVE, proof, &handles).unwrap()["verified"],
            true
        );
        assert!(
            get(
                &service,
                &mut handles,
                "archive-position",
                &[("position", position)]
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn archive_routes_serve_the_asset_and_refuse_unbound_simulation() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let temp = tempfile::tempdir().unwrap();
        let world = crate::source::init_world(&temp.path().join("world"), "Archive test").unwrap();
        let app = crate::http::router(Service::new(world));
        let request = |uri| {
            Request::builder()
                .uri(uri)
                .header("host", "127.0.0.1")
                .body(Body::empty())
                .unwrap()
        };
        let asset = app
            .clone()
            .oneshot(request("/workbench-archive.js"))
            .await
            .unwrap();
        assert_eq!(asset.status(), StatusCode::OK);
        let bytes = asset.into_body().collect().await.unwrap().to_bytes();
        let script = std::str::from_utf8(&bytes).unwrap();
        assert!(script.contains("workbench.register('manuscripts'"));
        let response = app
            .clone()
            .oneshot(request("/api/workbench/archive-sources"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["items"][0]["label"], "Working files");
        let response = app
            .oneshot(request(
                "/api/workbench/archive-simulation?position=forged&plan=forged",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!std::str::from_utf8(&bytes).unwrap().contains("forged"));
    }

    #[test]
    fn decisions_preserve_original_analysis_and_its_exact_item_order() {
        let at = Time {
            timeline: "main".into(),
            tick: "9007199254740995".into(),
            order: "-2".into(),
        };
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        let mut document = retcon_reviews::Document {
            id: "review_00000000000000000000000001".into(),
            title: "Retcon".into(),
            analysis: retcon_reviews::Analysis {
                world_id: "world_00000000000000000000000001".into(),
                base_revision: "a".repeat(40),
                plan: crate::consequences::Plan {
                    expected_fingerprint: first.clone(),
                    operations: vec![],
                    analysis: None,
                },
                impact_request: crate::retcon_impact::Request {
                    target: crate::retcon_impact::Reference::Record {
                        id: "location:harbor".into(),
                    },
                    at,
                    depth: 2,
                    max_items: 2,
                },
                plan_sha256: first.clone(),
                report: json!({"items":[{},{}]}),
                report_sha256: first.clone(),
                item_digests: vec![first.clone(), second.clone()],
                witness: json!({}),
                witness_sha256: second.clone(),
            },
            decisions: vec![retcon_reviews::Decision {
                item_digest: second.clone(),
                disposition: retcon_reviews::Disposition::Reviewed,
                note: "Old note\r\nWith original endings.".into(),
            }],
            body_markdown: "Original private note.\r\n".into(),
        };
        let original = document.analysis.clone();
        let updates = serde_json::from_value(json!([
            {"index":1,"disposition":"needs-revision","note":"Exact\n🌲 "},
            {"index":0,"disposition":"reviewed","note":null},
        ]))
        .unwrap();
        apply_decisions(&mut document, updates).unwrap();
        assert_eq!(document.analysis, original);
        assert_eq!(document.decisions[0].item_digest, first);
        assert_eq!(document.decisions[1].item_digest, second);
        assert_eq!(document.decisions[1].note, "Exact\r\n🌲 ");
    }

    #[test]
    fn simulation_and_revision_views_discard_machine_metadata_and_keep_typed_values() {
        let id = "char_00000000000000000000000003";
        let oid = "a".repeat(40);
        let value = present_simulation(json!({"mode":"author-preview","outcome":"available",
            "revision":oid,"occurrence":7,"items":[{"id":id,"title":"Admitted heading",
                "markdown":format!("Inert <tag> ref:{id} story/private.md sha256:{}", "a".repeat(64))}],
            "nextCursor":null})).unwrap();
        assert!(!value.to_string().contains(id));
        assert!(!value.to_string().contains(&oid));
        assert!(!value.to_string().contains("story/private.md"));
        assert!(
            value["items"][0]["prose"]
                .as_str()
                .unwrap()
                .contains("<tag>")
        );
        assert!(
            present_simulation(json!({"mode":"reader","outcome":"available","items":[]})).is_err()
        );
        let names = BTreeMap::from([(id, "Named person".into())]);
        assert_eq!(
            conflict_value(&json!({"present":false}), &names)["kind"],
            "absent"
        );
        assert_eq!(
            conflict_value(
                &json!({"present":true,"encoding":"canonical-json","text":"null"}),
                &names
            )["kind"],
            "empty"
        );
        let value = typed_value(&json!([9007199254740995_u64, id, true]), &names, 0);
        assert_eq!(value["items"][0]["label"], "9007199254740995");
        assert_eq!(value["items"][1]["label"], "Named person");
        assert_eq!(value["items"][2]["kind"], "boolean");
    }

    #[test]
    fn absent_simulation_policy_refuses_bound_positions_without_loading_a_commit() {
        let temp = tempfile::tempdir().unwrap();
        let world = crate::source::init_world(&temp.path().join("world"), "Archive test").unwrap();
        let service = Service::new(world);
        let revision = "a".repeat(40);
        let pin = |kind: &str, id: &str| DocumentPin {
            request: manuscript::DetailRequest {
                revision: Some(revision.clone()),
                id: id.into(),
                expected_sha256: Some("b".repeat(64)),
            },
            kind: kind.into(),
            author_fingerprint: "c".repeat(64),
        };
        let mut handles = Handles::default();
        let position = bind(
            &mut handles,
            POSITION,
            &PositionPin {
                document: pin("manuscript-segment", SEGMENT),
                work: WORK.into(),
                ordinal: 0,
                simulation_at: Some(Time {
                    timeline: "main".into(),
                    tick: "0".into(),
                    order: "0".into(),
                }),
            },
        )
        .unwrap();
        let plan = bind(
            &mut handles,
            DOCUMENT,
            &pin("reveal-plan", "reveal_00000000000000000000000001"),
        )
        .unwrap();
        let prepared = prepare_simulation(
            &service,
            &BTreeMap::from([("position".into(), position), ("plan".into(), plan)]),
            &mut handles,
        )
        .unwrap();
        let value = service.reader().emit(prepared);
        assert_eq!(value["code"], "READER-PRESENTATION-UNAVAILABLE");
        assert!(!value.to_string().contains(&revision));
        assert!(present_simulation(value).is_err());
    }
}

fn revision_moments(service: &Service, revision: String, handles: &mut Handles) -> Result<Value> {
    let world = historical::load_world(&service.world().root, &revision)?;
    let current = world.current_time()?;
    let frame = writing_frames::Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: None,
        pov_id: None,
        at: current.clone(),
        lens: writing_frames::Lens::Author,
    };
    let choices = writing_frames::choices(&world, None, &frame, 1000)?;
    let mut items = Vec::new();
    items.push(json!({"label":"Snapshot’s current story position",
        "token":bind(handles, MOMENT, &RevisionMoment{revision:revision.clone(),at:time(current)})?}));
    for (index, moment) in rows(&choices, "moments")?.iter().enumerate() {
        let at = serde_json::from_value(moment["at"].clone()).map_err(|_| unavailable())?;
        items.push(
            json!({"label":format!("{} · Position {}", label(string(moment, "label")?), index + 1),
            "token":bind(handles, MOMENT, &RevisionMoment{revision:revision.clone(),at})?}),
        );
    }
    Ok(json!({"items":items}))
}

fn version_label(version: &Value) -> Option<String> {
    version["title"].as_str().map(label)
}

fn change_label(value: &Value) -> &'static str {
    match value.as_str() {
        Some("added") => "Added",
        Some("removed" | "deleted") => "Removed",
        Some("modified" | "changed") => "Changed",
        Some("moved") => "Moved",
        Some("none") => "Unchanged",
        _ => "Compared",
    }
}

fn field_name(value: &Value) -> &'static str {
    match value
        .as_str()
        .and_then(|section| section.strip_prefix("frontmatter.").or(Some(section)))
        .and_then(|section| section.split('.').next())
    {
        Some("title") => "Title",
        Some("description" | "descriptions") => "Description",
        Some("body" | "bodyMarkdown" | "markdown") => "Prose",
        Some("status") => "Status",
        Some("timeline" | "time" | "placement") => "Story placement",
        Some("presence" | "participants" | "objects") => "Presence",
        Some("states" | "state" | "initial_state" | "state_transitions") => "State",
        Some("sections") => "Sections",
        Some("aliases") => "Alternate names",
        _ => "Source field",
    }
}

fn typed_value(value: &Value, names: &BTreeMap<&str, String>, depth: usize) -> Value {
    if depth > 4 {
        return json!({"kind":"structured","label":"Nested authored value"});
    }
    match value {
        Value::Null => json!({"kind":"empty","label":"Explicitly empty"}),
        Value::Bool(value) => json!({"kind":"boolean","label":if *value { "Yes" } else { "No" }}),
        Value::Number(value) => json!({"kind":"number","label":value.to_string()}),
        Value::String(value) => json!({"kind":"text","label":names.get(value.as_str()).cloned()
            .unwrap_or_else(|| if value.is_empty() { "Empty text".into() } else { display(value, 4096) })}),
        Value::Array(items) if items.len() <= 64 => json!({"kind":"list",
            "label":if items.is_empty() { "Empty list" } else { "Ordered values" },
            "items":items.iter().map(|item| typed_value(item,names,depth+1)).collect::<Vec<_>>()}),
        Value::Object(fields) if fields.len() == 1 => {
            if let Some(reference) = fields.get("id").or_else(|| fields.get("entity")) {
                typed_value(reference, names, depth + 1)
            } else {
                json!({"kind":"structured","label":"Structured authored value"})
            }
        }
        _ => json!({"kind":"structured","label":"Structured authored value"}),
    }
}

fn conflict_value(value: &Value, names: &BTreeMap<&str, String>) -> Value {
    if value["present"] == false || value["presence"] == "absent" {
        return json!({"kind":"absent","label":"Absent"});
    }
    match (value["encoding"].as_str(), value["text"].as_str()) {
        (Some("markdown"), Some(text)) => typed_value(&Value::String(text.into()), names, 0),
        (Some("canonical-json"), Some(text)) => serde_json::from_str::<Value>(text)
            .map(|value| typed_value(&value, names, 0))
            .unwrap_or_else(|_| json!({"kind":"structured","label":"Structured authored value"})),
        _ if value.get("payload").is_some() => typed_value(&value["payload"], names, 0),
        _ => json!({"kind":"structured","label":"See native comparison file"}),
    }
}

fn compare_revisions(
    service: &Service,
    request: revision_review::Request,
    handles: &mut Handles,
) -> Result<Value> {
    let value = revision_review::review(&service.world().root, &request)?;
    let mut names = BTreeMap::new();
    let mut changes = Vec::new();
    for row in rows(&value, "changes")? {
        let title = ["base", "left", "right"]
            .into_iter()
            .find_map(|key| version_label(&row[key]))
            .unwrap_or_else(|| "Story record".into());
        if let Some(id) = row["id"].as_str() {
            names.insert(id, title.clone());
        }
        let fields = row["positions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|position| field_name(&position["section"]))
            .collect::<Vec<_>>();
        changes.push(
            json!({"label":title, "left":change_label(&row["leftChange"]),
            "right":change_label(&row["rightChange"]), "fields":fields}),
        );
    }
    let conflicts = rows(&value, "conflicts")?
        .iter()
        .map(|row| {
            let title = ["id", "ownerId", "entityId"]
                .into_iter()
                .find_map(|key| row[key].as_str())
                .and_then(|id| names.get(id))
                .map(String::as_str)
                .unwrap_or("Story record");
            json!({"label":title,"field":field_name(&row["position"]["section"]),
            "base":conflict_value(&row["base"],&names),"left":conflict_value(&row["left"],&names),
            "right":conflict_value(&row["right"],&names)})
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"changes":changes,"conflicts":conflicts,"complete":value["complete"] == true,
        "file":handles.issue("native-file",serde_json::to_string(&value)?)?}),
    )
}

pub(super) fn needs_current(endpoint: &str, _query: &Query) -> bool {
    matches!(
        endpoint,
        "archive-manuscript-edit"
            | "archive-review-edit"
            | "archive-review-prepare"
            | "archive-review-capture"
    )
}

pub(super) fn read(
    service: &Service,
    endpoint: &str,
    query: &Query,
    handles: &mut Handles,
) -> Result<Value> {
    match endpoint {
        "archive-sources" => {
            closed(query, &[])?;
            sources(service, handles)
        }
        "archive-documents" | "archive-reviews" | "archive-revision-moments" => {
            closed(query, &["source"])?;
            let source: Option<String> = resolve(handles, SOURCE, field(query, "source")?)?;
            match endpoint {
                "archive-documents" => documents(service, source, handles),
                "archive-reviews" => review_queue(service, source, handles),
                _ => revision_moments(service, source.ok_or_else(unavailable)?, handles),
            }
        }
        "archive-document" | "archive-manuscript-edit" | "archive-work-positions" => {
            closed(query, &["document"])?;
            let pin = resolve(handles, DOCUMENT, field(query, "document")?)?;
            match endpoint {
                "archive-document" => document_view(service, &pin, handles),
                "archive-manuscript-edit" => manuscript_edit(service, &pin, handles),
                _ => work_positions(service, pin, handles),
            }
        }
        "archive-position" => {
            closed(query, &["position"])?;
            let pin: PositionPin = resolve(handles, POSITION, field(query, "position")?)?;
            document_view(service, &pin.document, handles)
        }
        MANUSCRIPT_PROOF => {
            closed(query, &["edit", "sha256"])?;
            let edit = resolve(handles, MANUSCRIPT_EDIT, field(query, "edit")?)?;
            manuscript_proof(edit, field(query, "sha256")?, handles)
        }
        "archive-review" | "archive-review-edit" => {
            closed(query, &["review"])?;
            let request = resolve(handles, REVIEW, field(query, "review")?)?;
            review_detail(service, request, handles, endpoint == "archive-review-edit")
        }
        "archive-review-capture" => {
            closed(query, &["request"])?;
            capture_review(service, field(query, "request")?, handles)
        }
        "archive-review-prepare" => {
            closed(query, &["draft", "changes", "title"])?;
            let draft = resolve(handles, DRAFT, field(query, "draft")?)?;
            prepare_review(
                service,
                draft,
                field(query, "changes")?,
                query.get("title").map(String::as_str),
                handles,
            )
        }
        "archive-revision-compare" => {
            closed(query, &["base", "left", "right", "moment"])?;
            let original = |name| -> Result<String> {
                resolve::<Option<String>>(handles, SOURCE, field(query, name)?)?
                    .ok_or_else(unavailable)
            };
            let moment: RevisionMoment = resolve(handles, MOMENT, field(query, "moment")?)?;
            let base = original("base")?;
            if base != moment.revision {
                return Err(unavailable());
            }
            compare_revisions(
                service,
                revision_review::Request {
                    base_revision: base,
                    left_revision: original("left")?,
                    right_revision: original("right")?,
                    at: moment.at,
                    max_items: 1000,
                    max_characters: 1_000_000,
                },
                handles,
            )
        }
        _ => Err(unavailable()),
    }
}
