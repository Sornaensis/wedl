//! Private manuscript sources and literal work order. These records never join
//! the canonical World; static links confer no learning or reader authority.
use crate::{
    Error, Result, historical,
    model::{Record, StoryTime, World},
    validation,
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod codec;
mod reveal;
mod store;
pub use reveal::{Admission, Milestone, Occurrence, RevealEntry, RevealPlan, resolve_reveal_plan};
pub(crate) use store::snapshot;
pub use store::{
    DetailRequest, ListRequest, ResolveRequest, RevealResolveRequest, SaveRequest, decode_detail,
    decode_list, decode_resolve, decode_reveal_resolve, decode_save, detail, list, resolve,
    resolve_reveal, save, save_preview,
};

/// Shared bounded, duplicate-safe input decoding for the separate reader
/// adapter. Authority is supplied by that adapter, never by this codec.
pub(crate) fn decode_closed_json<T: serde::de::DeserializeOwned>(
    raw: &str,
    maximum: usize,
) -> Result<T> {
    codec::decode_json(raw, maximum)
}

pub const SOURCE_SCHEMA: &str = "wedl-author/v1";
pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
pub const MAX_DOCUMENTS: usize = 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EXPANDED_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SNAPSHOT_NODES: usize = 1_000_000;
pub const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub revision: Option<String>,
    pub fingerprint: String,
    pub documents: BTreeMap<String, Record>,
}

/// Charge admitted source incrementally before retaining another document.
/// Raw source bounds alone do not bound a collection of finite YAML aliases.
#[derive(Default)]
pub(crate) struct SourceBudget {
    nodes: usize,
    scalar_bytes: usize,
}
impl SourceBudget {
    pub(crate) fn charge(&mut self, record: &Record) -> Result<()> {
        self.scalar_bytes = self.scalar_bytes.saturating_add(record.body.len());
        let mut pending = vec![&record.frontmatter];
        while let Some(value) = pending.pop() {
            self.nodes = self.nodes.saturating_add(1);
            match value {
                Yaml::String(value) => {
                    self.scalar_bytes = self.scalar_bytes.saturating_add(value.len());
                }
                Yaml::Mapping(values) => {
                    for (key, value) in values {
                        pending.push(key);
                        pending.push(value);
                    }
                }
                Yaml::Sequence(values) => pending.extend(values),
                Yaml::Tagged(_) => return Err(invalid("manuscript source tags are not admitted")),
                _ => {}
            }
            if self.nodes > MAX_SNAPSHOT_NODES || self.scalar_bytes > MAX_EXPANDED_SNAPSHOT_BYTES {
                return Err(invalid(
                    "manuscript snapshot exceeds 16 MiB complete expanded text or 1000000 expanded nodes",
                ));
            }
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn valid_id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_prefix('_'))
        .is_some_and(|value| {
            value.len() == 26
                && value
                    .bytes()
                    .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
        })
}
pub fn document_path(kind: &str, id: &str) -> Result<String> {
    let (prefix, directory) = match kind {
        "manuscript-segment" => ("segment", "segments"),
        "work" => ("work", "works"),
        "reveal-plan" => ("reveal", "reveals"),
        _ => return Err(invalid("unsupported manuscript source kind")),
    };
    if !valid_id(id, prefix) {
        return Err(invalid("manuscript source requires an exact immutable ID"));
    }
    Ok(format!("author/{directory}/{id}.md"))
}
fn closed<'a>(value: &'a Yaml, required: &[&str], optional: &[&str]) -> Result<&'a Mapping> {
    let Yaml::Mapping(fields) = value else {
        return Err(invalid("manuscript fields require an untagged mapping"));
    };
    if required.iter().any(|field| !fields.contains_key(*field))
        || fields.keys().any(|key| !matches!(key, Yaml::String(key) if required.contains(&key.as_str()) || optional.contains(&key.as_str()))) {
        return Err(invalid("manuscript source has missing or unknown fields"));
    }
    Ok(fields)
}
fn text(value: &Yaml) -> Result<&str> {
    match value {
        Yaml::String(value) if !value.is_empty() => Ok(value),
        _ => Err(invalid(
            "manuscript reference or label must be a nonempty string",
        )),
    }
}
fn title(value: &Yaml) -> Result<&str> {
    let value = text(value)?;
    if value.trim().is_empty() || value.chars().count() > 256 {
        return Err(invalid(
            "manuscript titles require 1..256 nonblank characters",
        ));
    }
    Ok(value)
}
fn array(value: &Yaml) -> Result<&[Yaml]> {
    match value {
        Yaml::Sequence(value) if value.len() <= 1024 => Ok(value),
        _ => Err(invalid(
            "manuscript source requires an array of at most 1024 entries",
        )),
    }
}
fn reference(value: &Yaml) -> Result<historical::Target> {
    let kind = value
        .get("kind")
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid("invalid manuscript source reference"))?;
    let keys = if kind == "record" {
        vec!["kind", "id"]
    } else {
        vec!["kind", "owner_id", "id"]
    };
    let fields = closed(value, &keys, &[])?;
    let id = text(&fields["id"])?;
    Ok(match kind {
        "record" => historical::Target::Record { id: id.into() },
        "section" => historical::Target::Section {
            owner_id: text(&fields["owner_id"])?.into(),
            id: id.into(),
        },
        "transition" => historical::Target::Transition {
            owner_id: text(&fields["owner_id"])?.into(),
            id: id.into(),
        },
        _ => {
            return Err(invalid(
                "manuscript source references support only record, section and transition",
            ));
        }
    })
}
fn placement(value: &Yaml) -> Result<StoryTime> {
    let fields = closed(value, &["timeline", "tick", "order"], &[])?;
    let timeline = text(&fields["timeline"])?;
    let tick = match &fields["tick"] {
        Yaml::Number(value) => value.as_i64(),
        _ => None,
    }
    .ok_or_else(|| invalid("manuscript placement tick requires an exact signed 64-bit integer"))?;
    let order = match &fields["order"] {
        Yaml::Number(value) => value.as_i64().and_then(|value| i32::try_from(value).ok()),
        _ => None,
    }
    .ok_or_else(|| invalid("manuscript placement order requires an exact signed 32-bit integer"))?;
    StoryTime::new(timeline, tick, order)
}
fn validate_fields(value: &Yaml) -> Result<(&str, &str, &str)> {
    let kind = value
        .get("kind")
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid("manuscript source requires an exact kind"))?;
    let (required, optional): (&[&str], &[&str]) = match kind {
        "manuscript-segment" => (
            &["schema", "kind", "id", "title", "source_links"],
            &["placement", "pov_character_id"],
        ),
        "work" => (
            &[
                "schema",
                "kind",
                "id",
                "title",
                "chapters",
                "duplicate_policy",
            ],
            &[],
        ),
        "reveal-plan" => (&["schema", "kind", "id", "title", "work", "entries"], &[]),
        _ => return Err(invalid("unsupported manuscript source kind")),
    };
    let fields = closed(value, required, optional)?;
    if fields["schema"].as_str() != Some(SOURCE_SCHEMA) {
        return Err(invalid("manuscript source schema must be wedl-author/v1"));
    }
    let id = text(&fields["id"])?;
    document_path(kind, id)?;
    let title = title(&fields["title"])?;
    if kind == "manuscript-segment" {
        for value in array(&fields["source_links"])? {
            reference(value)?;
        }
        if let Some(value) = fields.get("placement") {
            placement(value)?;
        }
        if let Some(value) = fields.get("pov_character_id") {
            text(value)?;
        }
    } else if kind == "work" {
        let policy = text(&fields["duplicate_policy"])?;
        if !matches!(policy, "reject" | "allow-explicit-reuse") {
            return Err(invalid("invalid manuscript work duplicate policy"));
        }
        let mut chapter_ids = BTreeSet::new();
        let mut segment_ids = BTreeSet::new();
        for chapter in array(&fields["chapters"])? {
            let chapter = closed(chapter, &["id", "title", "segments"], &[])?;
            let id = text(&chapter["id"])?;
            if !valid_id(id, "chapter") || !chapter_ids.insert(id) {
                return Err(invalid("work chapter IDs must be unique local chapter IDs"));
            }
            self::title(&chapter["title"])?;
            let segments = array(&chapter["segments"])?;
            if segments.is_empty() {
                return Err(invalid("work chapters require nonempty segment arrays"));
            }
            for segment in segments {
                let segment = closed(segment, &["kind", "id"], &[])?;
                let id = text(&segment["id"])?;
                if segment["kind"].as_str() != Some("author-record") || !valid_id(id, "segment") {
                    return Err(invalid(
                        "work requires exact manuscript-segment author references",
                    ));
                }
                if !segment_ids.insert(id) && policy == "reject" {
                    return Err(invalid(
                        "work duplicate segment requires explicit reuse policy",
                    ));
                }
            }
        }
    } else {
        reveal::validate_fields(value)?;
    }
    Ok((kind, id, title))
}

/// Admit one exact private Markdown source. Body bytes, line endings, trailing
/// whitespace and final-newline absence remain unchanged.
pub fn parse_document(bytes: &[u8], path: &str) -> Result<Record> {
    let record = parse_source(bytes)?;
    if record.source_path != path {
        return Err(invalid(
            "manuscript source identity differs from its exact private path",
        ));
    }
    Ok(record)
}
pub(super) fn parse_source(bytes: &[u8]) -> Result<Record> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(invalid("manuscript document exceeds 2 MiB"));
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| invalid("manuscript source must be UTF-8"))?;
    let newline = if raw.starts_with("---\r\n") {
        "\r\n"
    } else if raw.starts_with("---\n") {
        "\n"
    } else {
        return Err(invalid("manuscript source is missing its YAML opener"));
    };
    let opening = 3 + newline.len();
    let delimiter = format!("{newline}---{newline}");
    let closing = raw[opening..]
        .find(&delimiter)
        .map(|at| opening + at)
        .ok_or_else(|| invalid("manuscript source is missing its YAML terminator"))?;
    let body = &raw[closing + delimiter.len()..];
    if body.len() > MAX_BODY_BYTES {
        return Err(invalid("manuscript Markdown body exceeds 1 MiB"));
    }
    let frontmatter = codec::yaml(&raw[opening..closing])?;
    let (kind, id, title) = validate_fields(&frontmatter)?;
    let source_path = document_path(kind, id)?;
    Ok(Record {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        source_path,
        frontmatter,
        body: body.into(),
        raw_bytes: bytes.to_vec(),
        blob_oid: None,
    })
}
fn verify_record(record: &Record, revision: Option<&str>) -> Result<()> {
    let parsed = parse_document(&record.raw_bytes, &record.source_path)?;
    if parsed.id != record.id
        || parsed.kind != record.kind
        || parsed.title != record.title
        || parsed.frontmatter != record.frontmatter
        || parsed.body != record.body
    {
        return Err(invalid(
            "manuscript record projection differs from its exact raw source",
        ));
    }
    verify_blob(record, revision)
}
fn verify_blob(record: &Record, revision: Option<&str>) -> Result<()> {
    match (revision, record.blob_oid.as_deref()) {
        (Some(revision), Some(oid))
            if historical::valid_oid(oid) && oid.len() == revision.len() => {}
        (None, None) => {}
        _ => {
            return Err(invalid(
                "manuscript blob identity differs from its current or historical snapshot",
            ));
        }
    }
    Ok(())
}
fn fingerprint(records: &BTreeMap<String, Record>) -> String {
    let mut hash = Sha256::new();
    hash.update(b"wedl-rs/manuscripts-v1\0");
    let mut records: Vec<_> = records.values().collect();
    records.sort_by_key(|record| &record.source_path);
    for record in records {
        hash.update((record.source_path.len() as u64).to_be_bytes());
        hash.update(record.source_path.as_bytes());
        hash.update((record.raw_bytes.len() as u64).to_be_bytes());
        hash.update(&record.raw_bytes);
    }
    hex::encode(hash.finalize())
}
pub fn snapshot_from_records(records: Vec<Record>, revision: Option<String>) -> Result<Snapshot> {
    build_snapshot(records, revision, true)
}
/// Store-only finalization after each document has just passed the bounded
/// source parser and incremental complete expansion budget. Public mutable
/// record inputs still use `snapshot_from_records` and exact revalidation.
fn finish_snapshot(records: Vec<Record>, revision: Option<String>) -> Result<Snapshot> {
    build_snapshot(records, revision, false)
}
fn build_snapshot(
    records: Vec<Record>,
    revision: Option<String>,
    verify: bool,
) -> Result<Snapshot> {
    if records.len() > MAX_DOCUMENTS {
        return Err(invalid("manuscript snapshot exceeds 1024 documents"));
    }
    if revision
        .as_deref()
        .is_some_and(|revision| !historical::valid_oid(revision))
    {
        return Err(invalid(
            "manuscript history requires a full lowercase commit OID",
        ));
    }
    let mut documents = BTreeMap::new();
    let mut total = 0usize;
    let mut expanded = SourceBudget::default();
    for record in records {
        total = total.saturating_add(record.raw_bytes.len());
        if total > MAX_SNAPSHOT_BYTES {
            return Err(invalid("manuscript snapshot exceeds 16 MiB"));
        }
        if verify {
            verify_record(&record, revision.as_deref())?;
        } else {
            verify_blob(&record, revision.as_deref())?;
        }
        expanded.charge(&record)?;
        if documents.insert(record.id.clone(), record).is_some() {
            return Err(invalid("duplicate manuscript record identity"));
        }
    }
    Ok(Snapshot {
        revision,
        fingerprint: fingerprint(&documents),
        documents,
    })
}
pub(crate) fn verify_snapshot(authors: &Snapshot) -> Result<()> {
    if authors.documents.len() > MAX_DOCUMENTS
        || authors
            .revision
            .as_deref()
            .is_some_and(|revision| !historical::valid_oid(revision))
    {
        return Err(invalid(
            "manuscript snapshot has invalid document bounds or revision",
        ));
    }
    let mut total = 0usize;
    let mut expanded = SourceBudget::default();
    for (id, record) in &authors.documents {
        total = total.saturating_add(record.raw_bytes.len());
        if total > MAX_SNAPSHOT_BYTES {
            return Err(invalid("manuscript snapshot exceeds 16 MiB"));
        }
        if id != &record.id {
            return Err(invalid(
                "manuscript snapshot index differs from record identity",
            ));
        }
        verify_record(record, authors.revision.as_deref())?;
        expanded.charge(record)?;
    }
    if fingerprint(&authors.documents) != authors.fingerprint {
        return Err(invalid(
            "manuscript snapshot fingerprint differs from exact source",
        ));
    }
    Ok(())
}
fn world_valid(world: &World) -> Result<()> {
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "canonical manuscript context failed validation".into(),
        ));
    }
    Ok(())
}
fn same_revision(world: &World, authors: &Snapshot) -> Result<()> {
    if world.revision != authors.revision {
        return Err(invalid(
            "manuscript and canonical sources require the same selected revision",
        ));
    }
    Ok(())
}
/// Internal reference pass for callers that already verified the complete
/// source-bound manuscript snapshot and canonical World once.
pub(crate) fn validate_segment_references(world: &World, record: &Record) -> Result<()> {
    if record.kind != "manuscript-segment" {
        return Err(invalid("manuscript consumer requires a segment"));
    }
    for value in record.list("source_links").expect("admitted source links") {
        let reference = reference(value)?;
        let (owner_id, member) = match &reference {
            historical::Target::Record { id } => (id, None),
            historical::Target::Section { owner_id, id } => (owner_id, Some(("section", id))),
            historical::Target::Transition { owner_id, id } => (owner_id, Some(("transition", id))),
        };
        let owner = world.records.get(owner_id).ok_or_else(|| {
            invalid("manuscript source reference does not resolve at the selected revision")
        })?;
        if let Some((kind, member_id)) = member {
            let (key, values) = if kind == "section" {
                ("id", owner.get("descriptions"))
            } else {
                match owner.kind.as_str() {
                    "knowledge" | "relationship" => ("id", owner.get("transitions")),
                    "story-point" => (
                        "id",
                        owner
                            .get("lifecycle")
                            .and_then(|value| value.get("transitions")),
                    ),
                    "organization" | "parentage" | "union" | "affiliation" | "legacy"
                    | "tenure" | "claim" | "vital-history" => {
                        ("transition_id", owner.get("transitions"))
                    }
                    _ => return Err(invalid("manuscript transition owner is unavailable")),
                }
            };
            if values.and_then(Yaml::as_sequence).is_none_or(|values| {
                values
                    .iter()
                    .filter(|value| value.get(key).and_then(Yaml::as_str) == Some(member_id))
                    .count()
                    != 1
            }) {
                return Err(invalid(
                    "manuscript member reference does not resolve exactly on its owner",
                ));
            }
        }
    }
    if let Some(value) = record.get("placement") {
        let point = placement(value)?;
        world.story_time(point.tick, Some(&point.timeline), point.order)?;
    }
    if let Some(value) = record.get("pov_character_id") {
        let id = text(value)?;
        if world
            .records
            .get(id)
            .is_none_or(|record| record.kind != "character")
        {
            return Err(invalid("manuscript POV must resolve an exact character"));
        }
    }
    Ok(())
}
/// Validate one source-bound segment against canonical source, without inferring
/// any audience, scene, chronology, narrative admission or character knowledge.
pub fn validate_segment(world: &World, record: &Record) -> Result<()> {
    verify_record(record, world.revision.as_deref())?;
    world_valid(world)?;
    validate_segment_references(world, record)
}
fn work_segments<'a>(
    authors: &'a Snapshot,
    work: &'a Record,
) -> Result<BTreeMap<&'a str, &'a Record>> {
    if work.kind != "work" {
        return Err(invalid("work consumer requires a work"));
    }
    let mut result = BTreeMap::new();
    for chapter in work.list("chapters").expect("admitted chapters") {
        for reference in chapter["segments"]
            .as_sequence()
            .expect("admitted segments")
        {
            let id = reference["id"].as_str().expect("admitted segment ID");
            let segment = authors
                .documents
                .get(id)
                .filter(|record| record.kind == "manuscript-segment")
                .ok_or_else(|| invalid("work segment does not resolve at the selected revision"))?;
            result.insert(id, segment);
        }
    }
    Ok(result)
}
/// Internal work-reference pass after `verify_snapshot`; does not revalidate
/// canonical source or reread any private file.
pub(crate) fn validate_work(authors: &Snapshot, record: &Record) -> Result<()> {
    work_segments(authors, record).map(|_| ())
}
/// Validate the complete selected work closure after canonical and private
/// snapshots have been checked. This does not serialize or disclose prose and
/// imposes no reader/report occurrence bound on otherwise valid author source.
pub(crate) fn validate_work_references(
    world: &World,
    authors: &Snapshot,
    record: &Record,
) -> Result<()> {
    for segment in work_segments(authors, record)?.values() {
        validate_segment_references(world, segment)?;
    }
    Ok(())
}
/// Retcon's installed private consumer covers segments and works. Reveal plans
/// share source storage but do not acquire invented retcon dependency edges.
pub(crate) fn validate_manuscript_snapshot(world: &World, authors: &Snapshot) -> Result<()> {
    same_revision(world, authors)?;
    verify_snapshot(authors)?;
    world_valid(world)?;
    for record in authors.documents.values() {
        if record.kind == "manuscript-segment" {
            validate_segment_references(world, record)?;
        } else if record.kind == "work" {
            validate_work(authors, record)?;
        }
    }
    Ok(())
}
pub fn validate_snapshot(world: &World, authors: &Snapshot) -> Result<()> {
    validate_manuscript_snapshot(world, authors)?;
    for record in authors.documents.values() {
        if record.kind == "reveal-plan" {
            reveal::validate_references(world, authors, record)?;
        }
    }
    Ok(())
}
fn source_json(record: &Record) -> Json {
    json!({"id":record.id,"kind":record.kind,"title":record.title,"sourcePath":record.source_path,
        "sourceSha256":hex::encode(Sha256::digest(&record.raw_bytes)),"blobOid":record.blob_oid,"bodyMarkdown":record.body})
}
/// Resolve complete private authored order. Reused prose occurs once in the
/// segment table; each occurrence retains its chapter-local index and ordinal.
/// Limits refuse the complete result and never truncate or authorize a reader.
pub fn resolve_work(
    world: &World,
    authors: &Snapshot,
    id: &str,
    max_occurrences: usize,
    max_characters: usize,
) -> Result<Json> {
    if !valid_id(id, "work")
        || !(1..=4096).contains(&max_occurrences)
        || !(256..=1_000_000).contains(&max_characters)
    {
        return Err(invalid(
            "work resolution requires an exact work ID, maxOccurrences 1..4096 and maxCharacters 256..1000000",
        ));
    }
    same_revision(world, authors)?;
    verify_snapshot(authors)?;
    world_valid(world)?;
    let work = authors
        .documents
        .get(id)
        .filter(|record| record.kind == "work")
        .ok_or_else(|| Error::NotFound("manuscript work is unavailable".into()))?;
    let selected = work_segments(authors, work)?;
    for segment in selected.values() {
        validate_segment_references(world, segment)?;
    }
    let body_characters: BTreeMap<_, _> = selected
        .iter()
        .map(|(id, record)| (*id, record.body.chars().count()))
        .collect();
    if work.body.chars().count() + body_characters.values().sum::<usize>() > max_characters {
        return Err(invalid("complete work report exceeds maxCharacters"));
    }
    let mut ordinal = 0usize;
    let mut prose_characters = 0usize;
    let mut chapters = Vec::new();
    for chapter in work.list("chapters").expect("admitted chapters") {
        let chapter_id = chapter["id"].as_str().expect("admitted chapter ID");
        let mut occurrences = Vec::new();
        for (index, reference) in chapter["segments"]
            .as_sequence()
            .expect("admitted segments")
            .iter()
            .enumerate()
        {
            if ordinal >= max_occurrences {
                return Err(invalid("complete work exceeds maxOccurrences"));
            }
            let segment_id = reference["id"].as_str().expect("admitted segment ID");
            prose_characters += body_characters[segment_id];
            occurrences.push(json!({"chapterId":chapter_id,"segmentIndex":index,"ordinal":ordinal,"segmentId":segment_id}));
            ordinal += 1;
        }
        chapters.push(json!({"id":chapter_id,"title":chapter["title"].as_str().expect("admitted chapter title"),"occurrences":occurrences}));
    }
    let mut segments = Vec::new();
    for segment in selected.values() {
        let mut value = source_json(segment);
        value["sourceLinks"] =
            serde_json::to_value(segment.list("source_links").expect("admitted links"))?;
        if let Some(point) = segment.get("placement") {
            value["placement"] = serde_json::to_value(placement(point)?)?;
        }
        if let Some(id) = segment.string("pov_character_id") {
            value["povCharacterId"] = id.into();
        }
        segments.push(value);
    }
    let mut work_value = source_json(work);
    work_value["duplicatePolicy"] = work
        .string("duplicate_policy")
        .expect("admitted duplicate policy")
        .into();
    let report = json!({"protocol":crate::CONTRACT,"operation":"manuscripts-resolve","scope":"private-manuscripts",
        "fingerprint":world.fingerprint,"revision":authors.revision,"authorFingerprint":authors.fingerprint,
        "work":work_value,"chapters":chapters,"segments":segments,"occurrenceCount":ordinal,"uniqueSegmentCount":selected.len(),"proseCharacters":prose_characters,
        "bounds":{"maxOccurrences":max_occurrences,"maxCharacters":max_characters},"complete":true,"readerAuthorized":false,"validationComplete":false,"writesPerformed":false,"canApply":false});
    let bytes = codec::json_bytes(&report, MAX_REPORT_BYTES)?;
    if std::str::from_utf8(&bytes)
        .expect("serialized JSON UTF-8")
        .chars()
        .count()
        > max_characters
    {
        return Err(invalid("complete work report exceeds maxCharacters"));
    }
    Ok(report)
}
