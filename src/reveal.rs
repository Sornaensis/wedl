//! Pure reader disclosure at an explicitly selected work occurrence.
//!
//! Source admissions intersect a trusted ordinary scope. These types have no
//! request deserializer; private author access never constructs a reader grant.
//! This module performs no filesystem, Git, cache or learning writes.
use crate::{
    Error, Result, description, historical, lore, manuscript,
    manuscript::{Admission, RevealPlan, Snapshot},
    model::{Record, StoryTime, World},
    narrative::CharacterScope,
    validation,
};
use serde::Serialize;
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

const MAX_SCOPE_RECORDS: usize = 4096;
const MAX_SCOPE_PARTS: usize = 4096;
const MAX_LABELS: usize = 16;
const ORDINARY_BODY_KINDS: &[&str] = &[
    "character",
    "location",
    "object",
    "event",
    "scene",
    "environment",
    "conversation",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn unavailable() -> Error {
    Error::NotFound("reader presentation authority or selected occurrence is unavailable".into())
}
fn source_error() -> Error {
    Error::Validation("invalid reader projection source".into())
}
fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= maximum
}
fn reveal_id(value: &str) -> bool {
    value.strip_prefix("reveal_").is_some_and(|tail| {
        tail.len() == 26
            && tail
                .bytes()
                .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
    })
}
fn permitted(parts: &BTreeSet<String>, mut field: &str) -> bool {
    loop {
        if parts.contains(field) {
            return true;
        }
        let Some(parent) = field.rfind('.') else {
            return false;
        };
        field = &field[..parent];
    }
}

/// Trusted issuer permissions. An absent section map grants each selected
/// record's fields. A present map is a whitelist: a missing owner grants none.
/// Parent paths permit descendants, e.g. `descriptions` or `bodyMarkdown`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalScope {
    pub world_id: String,
    pub records: BTreeSet<String>,
    pub sections: Option<BTreeMap<String, BTreeSet<String>>>,
}
impl CanonicalScope {
    /// Validate trusted metadata without resolving or inspecting source.
    pub fn validate(&self) -> Result<()> {
        if !text(&self.world_id, 256)
            || self.records.len() > MAX_SCOPE_RECORDS
            || self.records.iter().any(|id| !text(id, 256))
            || self.sections.as_ref().is_some_and(|sections| {
                sections.len() > MAX_SCOPE_RECORDS
                    || sections.iter().any(|(id, parts)| {
                        !self.records.contains(id)
                            || parts.len() > MAX_SCOPE_PARTS
                            || parts.iter().any(|part| !text(part, 512))
                    })
            })
        {
            return Err(invalid("invalid trusted canonical reader scope"));
        }
        Ok(())
    }
    pub fn permits(&self, id: &str, field: &str) -> bool {
        self.records.contains(id)
            && self.sections.as_ref().is_none_or(|sections| {
                sections
                    .get(id)
                    .is_some_and(|parts| permitted(parts, field))
            })
    }
    /// Intersection retains the more specific permitted path. Neither an
    /// absent record nor an explicit empty whitelist can be widened by union.
    pub fn intersect(&self, other: &Self) -> Result<Self> {
        self.validate()?;
        other.validate()?;
        if self.world_id != other.world_id {
            return Err(invalid("reader scopes select different worlds"));
        }
        let records = self.records.intersection(&other.records).cloned().collect();
        let sections = match (&self.sections, &other.sections) {
            (None, None) => None,
            _ => {
                let mut result = BTreeMap::new();
                for id in self.records.intersection(&other.records) {
                    let left = self.sections.as_ref().and_then(|sections| sections.get(id));
                    let right = other
                        .sections
                        .as_ref()
                        .and_then(|sections| sections.get(id));
                    let parts = match (&self.sections, &other.sections) {
                        (None, _) => right.cloned().unwrap_or_default(),
                        (_, None) => left.cloned().unwrap_or_default(),
                        _ => match (left, right) {
                            (Some(left), Some(right)) => left
                                .iter()
                                .filter(|field| permitted(right, field))
                                .chain(right.iter().filter(|field| permitted(left, field)))
                                .cloned()
                                .collect(),
                            _ => BTreeSet::new(),
                        },
                    };
                    result.insert(id.clone(), parts);
                }
                Some(result)
            }
        };
        Ok(Self {
            world_id: self.world_id.clone(),
            records,
            sections,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lens {
    AuthorAsOf,
    Character { character_id: String },
}

/// Authority supplied by a trusted internal issuer, never by reader JSON.
/// A local author session or access to a reveal-plan file is not this grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderGrant {
    pub reader_id: String,
    pub plan_id: Option<String>,
    pub revision: String,
    pub canonical_scope: CanonicalScope,
    pub at: StoryTime,
    pub lens: Lens,
    pub audiences: BTreeSet<String>,
    pub perspectives: BTreeSet<String>,
}

/// The complete unit allowed to cross the reader boundary. Section titles are
/// their own headings. Owner titles, private labels and source metadata are not
/// attached to section-only or text admissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderItem {
    pub kind: String,
    pub id: String,
    pub owner_id: Option<String>,
    pub title: Option<String>,
    pub markdown: String,
}

fn authority_header(reader_id: &str, plan_id: Option<&str>, grant: &ReaderGrant) -> Result<()> {
    if !text(reader_id, 256)
        || grant.reader_id != reader_id
        || grant.plan_id.as_deref() != plan_id
        || plan_id.is_some_and(|id| !reveal_id(id))
        || !historical::valid_oid(&grant.revision)
        || !text(&grant.at.timeline, 256)
        || [&grant.audiences, &grant.perspectives]
            .into_iter()
            .any(|labels| {
                labels.is_empty()
                    || labels.len() > MAX_LABELS
                    || labels.iter().any(|label| !text(label, 256))
            })
        || matches!(&grant.lens, Lens::Character { character_id } if !validation::valid_id_for_kind(character_id, "character"))
    {
        return Err(unavailable());
    }
    grant.canonical_scope.validate().map_err(|_| unavailable())
}
fn authority_world(world: &World, grant: &ReaderGrant) -> Result<()> {
    if world.revision.as_deref() != Some(&grant.revision)
        || world.world_record()?.id != grant.canonical_scope.world_id
        || world
            .story_time(grant.at.tick, Some(&grant.at.timeline), grant.at.order)
            .is_err()
        || matches!(&grant.lens, Lens::Character { character_id } if world.records.get(character_id).is_none_or(|record| record.kind != "character"))
    {
        return Err(unavailable());
    }
    Ok(())
}

/// Revalidate caller-supplied source projections and return only cumulative
/// admitted units. Missing plans grant nothing; current worktree revisions do
/// not form a public reader context. Pure source resolution supports up to
/// 4,096 occurrences; a server may impose its separate lower presentation cap.
pub fn present(
    world: &World,
    authors: &Snapshot,
    plan_id: Option<&str>,
    occurrence: Option<usize>,
    reader_id: &str,
    grant: &ReaderGrant,
) -> Result<Vec<ReaderItem>> {
    authority_header(reader_id, plan_id, grant)?;
    authority_world(world, grant)?;
    if authors.revision.as_deref() != Some(&grant.revision) {
        return Err(unavailable());
    }
    let Some(plan_id) = plan_id else {
        if !validation::validate_world(world).valid {
            return Err(source_error());
        }
        return Ok(Vec::new());
    };
    let plan = manuscript::resolve_reveal_plan(world, authors, plan_id, 4096)?;
    project_resolved(
        world,
        &plan,
        occurrence.ok_or_else(unavailable)?,
        reader_id,
        grant,
    )
}

fn owner_visible(
    world: &World,
    owner: &Record,
    grant: &ReaderGrant,
    character: Option<&CharacterScope<'_>>,
) -> Result<bool> {
    if !grant
        .canonical_scope
        .permits(&owner.id, "frontmatter.title")
    {
        return Ok(false);
    }
    if let Some(character) = character {
        if !character.visible(owner)? {
            return Ok(false);
        }
    }
    // Existence's ordinary boundary includes its independently authorized
    // character endpoint; the episode does not grant that endpoint implicitly.
    if owner.kind == "existence-episode"
        && owner
            .string("character_id")
            .is_none_or(|id| !grant.canonical_scope.permits(id, "frontmatter.title"))
    {
        return Ok(false);
    }
    for audience in &grant.audiences {
        for perspective in &grant.perspectives {
            if description::record_visible(world, owner, Some(&grant.at), audience, perspective)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn point(world: &World, value: Option<&Yaml>) -> Result<Option<StoryTime>> {
    value
        .filter(|value| !value.is_null())
        .map(|value| {
            let at = StoryTime::from_yaml(value, &world.default_timeline())?;
            world.story_time(at.tick, Some(&at.timeline), at.order)
        })
        .transpose()
}
/// Body timing is separate from explicit description validity. Scene and
/// conversation prose summarizes a completed run; environment prose retains
/// its authored closed interval; event prose starts at that event's clock.
fn body_eligible(world: &World, owner: &Record, at: &StoryTime) -> Result<bool> {
    let (start, end) = match owner.kind.as_str() {
        "event" => (point(world, owner.get("time"))?, None),
        "scene" | "conversation" => {
            if owner.status() != "closed" {
                return Ok(false);
            }
            let end = point(world, owner.get("time").and_then(|time| time.get("end")))?;
            if end.is_none() {
                return Ok(false);
            }
            (end, None)
        }
        "environment" => (
            point(world, owner.get("time").and_then(|time| time.get("start")))?,
            point(world, owner.get("time").and_then(|time| time.get("end")))?,
        ),
        _ => (None, None),
    };
    Ok(start.as_ref().is_none_or(|start| start.not_after(at))
        && end.as_ref().is_none_or(|end| at.not_after(end)))
}

struct BodySection<'a> {
    heading: Option<&'a str>,
    ancestors: Vec<&'a str>,
    markdown: &'a str,
}
fn whitespace(character: char) -> bool {
    // Python's pinned \s / str.strip set includes these four information
    // separators in addition to Unicode White_Space.
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}
fn line_boundary(character: char) -> bool {
    matches!(
        character,
        '\n' | '\u{b}'
            | '\u{c}'
            | '\u{1c}'
            | '\u{1d}'
            | '\u{1e}'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
    )
}
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) || !line[level..].chars().next().is_some_and(whitespace) {
        return None;
    }
    let title = line[level..].trim_matches(whitespace);
    (!title.is_empty()).then_some((level, title))
}
fn body_sections(body: &str) -> Vec<BodySection<'_>> {
    let mut sections = Vec::new();
    let mut stack = Vec::<(usize, &str)>::new();
    let mut start = 0;
    let mut offset = 0;
    let mut current_heading = None;
    let mut ancestors = Vec::new();
    for line in body.split_inclusive(line_boundary) {
        if let Some((level, title)) = heading(line) {
            let markdown = body[start..offset].trim_matches(whitespace);
            if !markdown.is_empty() {
                sections.push(BodySection {
                    heading: current_heading,
                    ancestors: ancestors.clone(),
                    markdown,
                });
            }
            while stack.last().is_some_and(|(parent, _)| *parent >= level) {
                stack.pop();
            }
            ancestors = stack.iter().map(|(_, title)| *title).collect();
            stack.push((level, title));
            current_heading = Some(title);
            start = offset;
        }
        offset += line.len();
    }
    let markdown = body[start..].trim_matches(whitespace);
    if !markdown.is_empty() {
        sections.push(BodySection {
            heading: current_heading,
            ancestors,
            markdown,
        });
    }
    sections
}
fn heading_key(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in crate::reader::casefold::fold(value).chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    result
}
fn private_heading(value: &str) -> bool {
    // The pinned private-heading regex uses re.I over ASCII pattern letters.
    // These four Unicode scalars also compare equal to ASCII letters there;
    // full multi-character folding would change the regex's semantics.
    let value = value
        .chars()
        .map(|character| match character {
            '\u{130}' | '\u{131}' => 'i',
            '\u{17f}' => 's',
            '\u{212a}' => 'k',
            character => character.to_ascii_lowercase(),
        })
        .collect::<String>();
    let words = value
        .split(whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match words.as_slice() {
        ["hidden" | "spoiler" | "spoilers"] => true,
        ["hidden", "truth" | "note" | "notes" | "secret" | "secrets"]
        | ["gm" | "internal", "note" | "notes"] => true,
        [
            owner,
            "note" | "notes" | "constraint" | "constraints" | "secret" | "secrets" | "truth",
        ] if matches!(
            *owner,
            "author" | "author's" | "author-only" | "author's-only"
        ) =>
        {
            true
        }
        [
            "author" | "author's",
            "only",
            "note" | "notes" | "constraint" | "constraints" | "secret" | "secrets" | "truth",
        ] => true,
        _ => false,
    }
}
fn explicit_audience<'a>(owner: &'a Record, title: &str) -> Result<Option<Vec<&'a str>>> {
    let Some(value) = owner.get("section_audiences") else {
        return Ok(None);
    };
    let values = value.as_mapping().ok_or_else(source_error)?;
    let key = heading_key(title);
    let Some(value) = values.get(title).or_else(|| values.get(key)) else {
        return Ok(None);
    };
    let values = value.as_sequence().ok_or_else(source_error)?;
    Ok(Some(
        values
            .iter()
            .map(|value| value.as_str().ok_or_else(source_error))
            .collect::<Result<_>>()?,
    ))
}
fn body_audience<'a>(owner: &'a Record, section: &BodySection<'_>) -> Result<Vec<&'a str>> {
    // An explicit mapping or recognized private heading remains effective
    // through its subtree. An explicit nearer mapping can override it. Ordinary
    // child headings alone never make a private ancestor's prose public.
    for candidate in section
        .heading
        .into_iter()
        .chain(section.ancestors.iter().rev().copied())
    {
        if let Some(audience) = explicit_audience(owner, candidate)? {
            return Ok(audience);
        }
        if private_heading(candidate) {
            return Ok(vec!["author"]);
        }
    }
    Ok(match owner.kind.as_str() {
        "character" => {
            if matches!(
                heading_key(section.heading.unwrap_or_default()).as_str(),
                "summary" | "appearance" | "voice" | "manner" | "mannerisms" | "public-dossier"
            ) {
                vec!["public", "self", "author"]
            } else {
                vec!["self", "author"]
            }
        }
        "object" | "location" | "environment" => vec!["public", "author"],
        _ => vec!["author"],
    })
}
fn ordinary_body(owner: &Record, grant: &ReaderGrant) -> Result<String> {
    let normalized = owner.body.replace("\r\n", "\n").replace('\r', "\n");
    let mut pieces = Vec::new();
    for section in body_sections(&normalized) {
        let path = format!("bodyMarkdown.{}", section.heading.unwrap_or("source"));
        if grant.canonical_scope.permits(&owner.id, &path)
            && body_audience(owner, &section)?
                .iter()
                .any(|label| grant.audiences.contains(*label))
        {
            pieces.push(section.markdown);
        }
    }
    Ok(pieces.join("\n\n"))
}

struct Section<'a> {
    id: &'a str,
    heading: &'a str,
    markdown: &'a str,
    replaces: Option<&'a str>,
}
fn labels(value: Option<&Yaml>, allowed: &BTreeSet<String>) -> Result<bool> {
    let values = value.and_then(Yaml::as_sequence).ok_or_else(source_error)?;
    let mut matched = false;
    for value in values {
        matched |= allowed.contains(value.as_str().ok_or_else(source_error)?);
    }
    Ok(matched)
}
fn selected_sections<'a>(
    world: &World,
    owner: &'a Record,
    grant: &ReaderGrant,
) -> Result<Vec<Section<'a>>> {
    let mut eligible = Vec::new();
    for member in owner.list("descriptions").into_iter().flatten() {
        let id = member
            .get("id")
            .and_then(Yaml::as_str)
            .ok_or_else(source_error)?;
        if !grant
            .canonical_scope
            .permits(&owner.id, &format!("descriptions.{id}"))
            || !labels(member.get("audience"), &grant.audiences)?
            || !labels(member.get("perspectives"), &grant.perspectives)?
            || !lore::validity(
                member.get("valid").ok_or_else(source_error)?,
                Some(world),
                "reader description validity",
            )?
            .applies(Some(&grant.at))
        {
            continue;
        }
        if let Lens::Character { character_id } = &grant.lens {
            // The native character boundary is an additional intersection;
            // issuer labels do not add privately authored character evidence.
            if !labels(
                member.get("audience"),
                &BTreeSet::from(["public".into(), character_id.clone()]),
            )? || !labels(
                member.get("perspectives"),
                &BTreeSet::from(["ordinary".into(), character_id.clone()]),
            )? {
                continue;
            }
        }
        eligible.push(Section {
            id,
            heading: member
                .get("heading")
                .and_then(Yaml::as_str)
                .ok_or_else(source_error)?,
            markdown: member
                .get("markdown")
                .and_then(Yaml::as_str)
                .ok_or_else(source_error)?,
            replaces: member
                .get("replaces")
                .map(|value| value.as_str().ok_or_else(source_error))
                .transpose()?,
        });
    }
    // Ordinary permits, labels and time filter before correction suppression.
    // Exact reveal-admission IDs are applied by the caller after this step.
    let replaced = eligible
        .iter()
        .filter_map(|section| section.replaces)
        .collect::<BTreeSet<_>>();
    Ok(eligible
        .into_iter()
        .filter(|section| !replaced.contains(section.id))
        .collect())
}

/// Project a freshly source-compiled plan. Only trusted crate callers can skip
/// the public mutable-source verification; this helper never resolves files.
pub(crate) fn project_resolved(
    world: &World,
    plan: &RevealPlan,
    occurrence: usize,
    reader_id: &str,
    grant: &ReaderGrant,
) -> Result<Vec<ReaderItem>> {
    authority_header(reader_id, Some(&plan.plan_id), grant)?;
    authority_world(world, grant)?;
    if plan
        .occurrences
        .get(occurrence)
        .is_none_or(|selected| selected.ordinal != occurrence)
    {
        return Err(unavailable());
    }
    let character = match &grant.lens {
        Lens::AuthorAsOf => None,
        Lens::Character { character_id } => {
            Some(CharacterScope::new(world, character_id, &grant.at)?)
        }
    };
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    let mut sections = BTreeMap::new();
    for entry in &plan.entries {
        if entry.ordinal > occurrence {
            break;
        }
        for admission in &entry.admissions {
            if let Admission::Text { id, markdown } = admission {
                if seen.insert(("text", None, id.as_str())) {
                    result.push(ReaderItem {
                        kind: "text".into(),
                        id: id.clone(),
                        owner_id: None,
                        title: None,
                        markdown: markdown.clone(),
                    });
                }
                continue;
            }
            let (id, exact_section) = match admission {
                Admission::Record { id } => (id.as_str(), None),
                Admission::Section { owner_id, id } => (owner_id.as_str(), Some(id.as_str())),
                Admission::Text { .. } => unreachable!(),
            };
            let Some(owner) = world.records.get(id) else {
                return Err(source_error());
            };
            if !owner_visible(world, owner, grant, character.as_ref())? {
                continue;
            }
            if exact_section.is_none()
                && grant.lens == Lens::AuthorAsOf
                && ORDINARY_BODY_KINDS.contains(&owner.kind.as_str())
                && body_eligible(world, owner, &grant.at)?
                && seen.insert(("record", None, owner.id.as_str()))
            {
                result.push(ReaderItem {
                    kind: "record".into(),
                    id: owner.id.clone(),
                    owner_id: None,
                    title: Some(owner.title.clone()),
                    markdown: ordinary_body(owner, grant)?,
                });
            }
            if !sections.contains_key(id) {
                sections.insert(id, selected_sections(world, owner, grant)?);
            }
            for section in &sections[id] {
                if exact_section.is_none_or(|selected| selected == section.id)
                    && seen.insert(("section", Some(owner.id.as_str()), section.id))
                {
                    result.push(ReaderItem {
                        kind: "section".into(),
                        id: section.id.into(),
                        owner_id: Some(owner.id.clone()),
                        title: Some(section.heading.into()),
                        markdown: section.markdown.into(),
                    });
                }
            }
        }
    }
    Ok(result)
}
