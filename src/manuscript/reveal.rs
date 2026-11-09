//! Literal occurrence-bound reveal sources. Compilation validates private
//! structure and references; it does not create reader authority or disclose
//! any manuscript source. The separate reader projection owns that boundary.
use super::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Milestone {
    pub chapter_id: String,
    pub segment_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub chapter_id: String,
    pub segment_index: usize,
    pub ordinal: usize,
    pub segment_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admission {
    Record { id: String },
    Section { owner_id: String, id: String },
    Text { id: String, markdown: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevealEntry {
    pub milestone: Milestone,
    pub ordinal: usize,
    pub admissions: Vec<Admission>,
}

/// Validated private input, not a reader result or a reader authorization.
/// Public callers can mutate this value; only the resolving caller may treat
/// its fresh result as admitted input without verifying source again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevealPlan {
    pub plan_id: String,
    pub work_id: String,
    pub occurrences: Vec<Occurrence>,
    pub entries: Vec<RevealEntry>,
}

enum AdmissionRef<'a> {
    Record(&'a str),
    Section(&'a str, &'a str),
    Text(&'a str, &'a str),
}
impl<'a> AdmissionRef<'a> {
    fn key(&self) -> (&'static str, Option<&'a str>, &'a str) {
        match self {
            Self::Record(id) => ("record", None, id),
            Self::Section(owner_id, id) => ("section", Some(owner_id), id),
            Self::Text(id, _) => ("text", None, id),
        }
    }
    fn into_owned(self) -> Admission {
        match self {
            Self::Record(id) => Admission::Record { id: id.into() },
            Self::Section(owner_id, id) => Admission::Section {
                owner_id: owner_id.into(),
                id: id.into(),
            },
            Self::Text(id, markdown) => Admission::Text {
                id: id.into(),
                markdown: markdown.into(),
            },
        }
    }
}

fn milestone(value: &Yaml) -> Result<Milestone> {
    let fields = closed(value, &["chapter_id", "segment_index"], &[])?;
    let chapter_id = text(&fields["chapter_id"])?;
    if !valid_id(chapter_id, "chapter") {
        return Err(invalid("reveal milestone requires an exact chapter ID"));
    }
    let segment_index = match &fields["segment_index"] {
        Yaml::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        _ => None,
    }
    .filter(|value| *value < 1024)
    .ok_or_else(|| invalid("reveal milestone requires an integer segment index in 0..1023"))?;
    Ok(Milestone {
        chapter_id: chapter_id.into(),
        segment_index,
    })
}

fn admission(value: &Yaml) -> Result<AdmissionRef<'_>> {
    let kind = value
        .get("kind")
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid("reveal admission requires an exact kind"))?;
    match kind {
        "record" => {
            let fields = closed(value, &["kind", "target"], &[])?;
            let target = closed(&fields["target"], &["kind", "id"], &[])?;
            if target["kind"].as_str() != Some("record") {
                return Err(invalid(
                    "reveal record admission requires an exact record target",
                ));
            }
            Ok(AdmissionRef::Record(text(&target["id"])?))
        }
        "section" => {
            let fields = closed(value, &["kind", "target"], &[])?;
            let target = closed(&fields["target"], &["kind", "owner_id", "id"], &[])?;
            if target["kind"].as_str() != Some("section") {
                return Err(invalid(
                    "reveal section admission requires an exact section target",
                ));
            }
            Ok(AdmissionRef::Section(
                text(&target["owner_id"])?,
                text(&target["id"])?,
            ))
        }
        "text" => {
            let fields = closed(value, &["kind", "id", "markdown"], &[])?;
            let id = text(&fields["id"])?;
            if !valid_id(id, "revealtext") {
                return Err(invalid("reveal text requires an exact local revealtext ID"));
            }
            let Yaml::String(markdown) = &fields["markdown"] else {
                return Err(invalid("reveal text Markdown must be a string"));
            };
            Ok(AdmissionRef::Text(id, markdown))
        }
        _ => Err(invalid(
            "reveal admission supports only record, section and text",
        )),
    }
}

fn work_id(value: &Yaml) -> Result<&str> {
    let fields = closed(value, &["kind", "id"], &[])?;
    let id = text(&fields["id"])?;
    if fields["kind"].as_str() != Some("author-record") || !valid_id(id, "work") {
        return Err(invalid(
            "reveal plan requires an exact work author reference",
        ));
    }
    Ok(id)
}

/// Source-only admission: listing and exact detail do not require a currently
/// resolvable work or canonical target. Cross-reference checks occur in compile
/// and complete candidate save validation.
pub(super) fn validate_fields(value: &Yaml) -> Result<()> {
    work_id(&value["work"])?;
    let mut milestones = BTreeSet::new();
    let mut admitted = BTreeSet::new();
    for entry in array(&value["entries"])? {
        let fields = closed(entry, &["milestone", "admissions"], &[])?;
        if !milestones.insert(milestone(&fields["milestone"])?) {
            return Err(invalid("reveal milestones must be unique"));
        }
        for value in array(&fields["admissions"])? {
            if !admitted.insert(admission(value)?.key()) {
                return Err(invalid(
                    "reveal admissions and text IDs must be globally unique",
                ));
            }
        }
    }
    Ok(())
}

fn reference(world: &World, value: &AdmissionRef<'_>) -> Result<()> {
    let (owner_id, section_id) = match value {
        AdmissionRef::Record(id) => (*id, None),
        AdmissionRef::Section(owner_id, id) => (*owner_id, Some(*id)),
        AdmissionRef::Text(_, _) => return Ok(()),
    };
    let owner = world.records.get(owner_id).ok_or_else(|| {
        invalid("reveal canonical reference does not resolve at the selected revision")
    })?;
    if let Some(section_id) = section_id
        && owner.list("descriptions").is_none_or(|sections| {
            sections
                .iter()
                .filter(|section| section.get("id").and_then(Yaml::as_str) == Some(section_id))
                .count()
                != 1
        })
    {
        return Err(invalid(
            "reveal section does not resolve exactly on its owner",
        ));
    }
    Ok(())
}

/// The caller already checked raw source identity, revision and canonical
/// validity. This pass resolves source references without interpreting prose.
fn compile(
    world: &World,
    authors: &Snapshot,
    plan: &Record,
    max_occurrences: usize,
) -> Result<RevealPlan> {
    if plan.kind != "reveal-plan" {
        return Err(invalid("reveal consumer requires a reveal plan"));
    }
    let work_id = work_id(plan.get("work").expect("admitted reveal work"))?;
    let work = authors
        .documents
        .get(work_id)
        .filter(|record| record.kind == "work")
        .ok_or_else(|| invalid("reveal work does not resolve at the selected revision"))?;
    // Validate every segment identity even when entries admit only text or no
    // entries exist. Work occurrence identity does not come from disclosure.
    work_segments(authors, work)?;
    let mut occurrences = Vec::new();
    let mut positions = BTreeMap::new();
    for chapter in work.list("chapters").expect("admitted work chapters") {
        let chapter_id = chapter["id"].as_str().expect("admitted chapter ID");
        for (segment_index, segment) in chapter["segments"]
            .as_sequence()
            .expect("admitted chapter segments")
            .iter()
            .enumerate()
        {
            let ordinal = occurrences.len();
            if ordinal >= max_occurrences {
                return Err(invalid("complete reveal work exceeds maxOccurrences"));
            }
            positions.insert((chapter_id, segment_index), ordinal);
            occurrences.push(Occurrence {
                chapter_id: chapter_id.into(),
                segment_index,
                ordinal,
                segment_id: segment["id"].as_str().expect("admitted segment ID").into(),
            });
        }
    }
    let mut previous = None;
    let mut entries = Vec::new();
    for value in plan.list("entries").expect("admitted reveal entries") {
        let milestone = milestone(&value["milestone"])?;
        let ordinal = *positions
            .get(&(milestone.chapter_id.as_str(), milestone.segment_index))
            .ok_or_else(|| invalid("reveal milestone does not resolve a work occurrence"))?;
        if previous.is_some_and(|previous| ordinal <= previous) {
            return Err(invalid(
                "reveal milestones must follow authored work occurrence order",
            ));
        }
        previous = Some(ordinal);
        let mut admissions = Vec::new();
        for value in array(&value["admissions"])? {
            let value = admission(value)?;
            reference(world, &value)?;
            admissions.push(value.into_owned());
        }
        entries.push(RevealEntry {
            milestone,
            ordinal,
            admissions,
        });
    }
    Ok(RevealPlan {
        plan_id: plan.id.clone(),
        work_id: work_id.into(),
        occurrences,
        entries,
    })
}

/// Complete candidate saves already checked every segment once. Validate the
/// remaining reveal references without repeating canonical segment validation.
pub(super) fn validate_references(world: &World, authors: &Snapshot, plan: &Record) -> Result<()> {
    compile(world, authors, plan, 4096).map(|_| ())
}

/// Resolve a source-bound private reveal plan and its complete work closure.
/// Matching current snapshots support private author previews/saves. Reader
/// consumers separately require original committed source and trusted authority.
pub fn resolve_reveal_plan(
    world: &World,
    authors: &Snapshot,
    id: &str,
    max_occurrences: usize,
) -> Result<RevealPlan> {
    if !valid_id(id, "reveal") || !(1..=4096).contains(&max_occurrences) {
        return Err(invalid(
            "reveal resolution requires an exact plan ID and maxOccurrences 1..4096",
        ));
    }
    same_revision(world, authors)?;
    verify_snapshot(authors)?;
    world_valid(world)?;
    let plan = authors
        .documents
        .get(id)
        .filter(|record| record.kind == "reveal-plan")
        .ok_or_else(|| Error::NotFound("reveal plan is unavailable".into()))?;
    let compiled = compile(world, authors, plan, max_occurrences)?;
    let work = &authors.documents[&compiled.work_id];
    for segment in work_segments(authors, work)?.values() {
        validate_segment_references(world, segment)?;
    }
    Ok(compiled)
}

fn admission_json(admission: &Admission) -> Json {
    match admission {
        Admission::Record { id } => json!({"kind":"record","target":{"kind":"record","id":id}}),
        Admission::Section { owner_id, id } => {
            json!({"kind":"section","target":{"kind":"section","owner_id":owner_id,"id":id}})
        }
        Admission::Text { id, markdown } => json!({"kind":"text","id":id,"markdown":markdown}),
    }
}

pub(super) fn report(
    world: &World,
    authors: &Snapshot,
    id: &str,
    max_occurrences: usize,
    max_characters: usize,
) -> Result<Json> {
    let compiled = resolve_reveal_plan(world, authors, id, max_occurrences)?;
    let mut report = resolve_work(
        world,
        authors,
        &compiled.work_id,
        max_occurrences,
        max_characters,
    )?;
    report["operation"] = "reveal-plans-resolve".into();
    report["scope"] = "private-reveal-plans".into();
    let mut plan = source_json(&authors.documents[id]);
    plan["workId"] = compiled.work_id.into();
    report["plan"] = plan;
    report["entries"] = compiled
        .entries
        .iter()
        .map(|entry| {
            json!({"milestone":entry.milestone,"ordinal":entry.ordinal,
            "admissions":entry.admissions.iter().map(admission_json).collect::<Vec<_>>()})
        })
        .collect::<Vec<_>>()
        .into();
    let bytes = codec::json_bytes(&report, MAX_REPORT_BYTES)?;
    if std::str::from_utf8(&bytes)
        .expect("serialized JSON UTF-8")
        .chars()
        .count()
        > max_characters
    {
        return Err(invalid("complete reveal report exceeds maxCharacters"));
    }
    Ok(report)
}
