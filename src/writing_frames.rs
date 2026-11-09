//! Stateless writing selections pinned to one exact captured source snapshot.
//! Frames never change source, shared cursors, authoring times or another tab.
use crate::{
    Error, Result, briefs, character, environment, family_context, historical, magic, manuscript,
    model::{Record, StoryTime, World},
    narrative::{self, CharacterScope},
    publication, runtime, scene, social, species, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;

mod present;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lens {
    Author,
    Character,
    DramaticIrony,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub fingerprint: String,
    pub work: Option<WorkSelection>,
    pub scene_id: Option<String>,
    pub pov_id: Option<String>,
    pub at: StoryTime,
    pub lens: Lens,
}

/// A private work is independently pinned; it never supplies scene, POV, time,
/// character knowledge, manuscript prose, or reader disclosure to the frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSelection {
    pub work_id: String,
    pub author_fingerprint: String,
}

/// Optional context selections are a separate request, never additional frame
/// fields or knowledge grants. Empty selections preserve the original packet.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilySelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub species: Option<SpeciesFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magic: Option<MagicFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social: Option<SocialFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentFamily>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeciesFamily {
    pub individual_id: String,
    pub selection: species::Selection,
    #[serde(default)]
    pub belief_targets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MagicAuthor {
    pub root_id: String,
    pub selection: magic::Selection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MagicFamily {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<MagicAuthor>,
    #[serde(default)]
    pub belief_targets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialFamily {
    pub language: String,
    pub sense: Option<String>,
    pub form: Option<String>,
    pub contexts: Vec<String>,
    #[serde(default)]
    pub belief_targets: Vec<String>,
}
impl SocialFamily {
    fn selection(&self) -> social::Selection {
        social::Selection {
            language: self.language.clone(),
            sense: self.sense.clone(),
            form: self.form.clone(),
            contexts: self.contexts.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentFamily {
    pub contexts: Vec<String>,
    #[serde(default)]
    pub belief_targets: Vec<String>,
}

impl FamilySelection {
    fn validate(&self) -> Result<()> {
        if serde_json::to_vec(self)?.len() > 16_384 {
            return Err(invalid("writing family selections exceed 16 KiB"));
        }
        let exact_concept = |id: &str| {
            if validation::valid_id_for_kind(id, "lore-concept") {
                Ok(())
            } else {
                Err(invalid("writing family roots must be exact concept IDs"))
            }
        };
        if let Some(request) = &self.species {
            exact_concept(&request.individual_id)?;
            request.selection.validate()?;
            family_context::validate_belief_targets(&request.belief_targets)?;
        }
        if let Some(request) = &self.magic {
            if let Some(author) = &request.author {
                exact_concept(&author.root_id)?;
                author.selection.validate()?;
            }
            family_context::validate_belief_targets(&request.belief_targets)?;
        }
        if let Some(request) = &self.social {
            request.selection().validate()?;
            family_context::validate_belief_targets(&request.belief_targets)?;
        }
        if let Some(request) = &self.environment {
            environment::validate_roots(&request.contexts)?;
            family_context::validate_belief_targets(&request.belief_targets)?;
        }
        Ok(())
    }
}

/// Closed family JSON rejects unknown, duplicate and null family members before
/// any source read. Canonical selectors for magic remain author-only inputs.
pub fn decode_families(payload: &str) -> Result<FamilySelection> {
    if payload.len() > 16_384 {
        return Err(invalid("writing family selections exceed 16 KiB"));
    }
    let mut decoder = serde_json::Deserializer::from_str(payload);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate writing family JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid writing family JSON"))?;
    let value: Json =
        serde_json::from_str(payload).map_err(|_| invalid("invalid writing family JSON"))?;
    let fields = value
        .as_object()
        .ok_or_else(|| invalid("writing families must be an object"))?;
    for (family, request) in fields {
        let required: &[&str] = match family.as_str() {
            "species" => &["individualId", "selection"],
            "social" => &["language", "sense", "form", "contexts"],
            "environment" => &["contexts"],
            "magic" => &[],
            _ => return Err(invalid("invalid writing family fields")),
        };
        let object = request
            .as_object()
            .ok_or_else(|| invalid("writing family members must be objects"))?;
        if required.iter().any(|key| !object.contains_key(*key))
            || (family == "magic" && object.get("author").is_some_and(Json::is_null))
        {
            return Err(invalid("invalid writing family fields"));
        }
    }
    let selection: FamilySelection =
        serde_json::from_value(value).map_err(|_| invalid("invalid writing family selections"))?;
    selection.validate()?;
    Ok(selection)
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn unavailable() -> Error {
    Error::NotFound("writing frame is unavailable; select it again".into())
}

fn source_invalid() -> Error {
    Error::Validation(
        "writing frame source is invalid; use author validation for diagnostics".into(),
    )
}

fn closed(value: &Json, keys: &[&str]) -> Result<()> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid("writing frame fields must be an object"))?;
    if map.len() != keys.len() || keys.iter().any(|key| !map.contains_key(*key)) {
        return Err(invalid("invalid writing frame fields"));
    }
    Ok(())
}

fn identifier(value: &Json) -> Result<Option<String>> {
    match value {
        Json::Null => Ok(None),
        Json::String(value) if !value.is_empty() && value.len() <= 128 => Ok(Some(value.clone())),
        _ => Err(invalid("writing frame selectors must be exact IDs or null")),
    }
}

fn decimal(value: &Json, bits: u8) -> Result<i64> {
    let text = value
        .as_str()
        .ok_or_else(|| invalid("writing frame time requires canonical decimal strings"))?;
    let number = text
        .parse::<i64>()
        .map_err(|_| invalid("writing frame time requires bounded canonical decimal strings"))?;
    if number.to_string() != text || (bits == 32 && i32::try_from(number).is_err()) {
        return Err(invalid(
            "writing frame time requires bounded canonical decimal strings",
        ));
    }
    Ok(number)
}

fn fingerprint(value: &Json) -> Result<String> {
    value
        .as_str()
        .filter(|text| {
            text.len() == 64
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid("writing frame fingerprints require exact lowercase source hashes"))
}

/// Closed JSON parsing rejects duplicate fields at every level before resolution.
pub fn decode(payload: &str) -> Result<Frame> {
    if payload.len() > 16_384 {
        return Err(invalid("writing frame request exceeds 16 KiB"));
    }
    let mut decoder = serde_json::Deserializer::from_str(payload);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate writing frame JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid writing frame JSON"))?;
    let value: Json =
        serde_json::from_str(payload).map_err(|_| invalid("invalid writing frame JSON"))?;
    closed(
        &value,
        &["fingerprint", "work", "sceneId", "povId", "at", "lens"],
    )?;
    let canonical_fingerprint = fingerprint(&value["fingerprint"])?;
    let work = if value["work"].is_null() {
        None
    } else {
        let selection = &value["work"];
        closed(selection, &["workId", "authorFingerprint"])?;
        let work_id = selection["workId"]
            .as_str()
            .filter(|id| manuscript::document_path("work", id).is_ok())
            .ok_or_else(|| invalid("writing frame work requires an exact work ID"))?;
        Some(WorkSelection {
            work_id: work_id.into(),
            author_fingerprint: fingerprint(&selection["authorFingerprint"])?,
        })
    };
    let scene_id = identifier(&value["sceneId"])?;
    let pov_id = identifier(&value["povId"])?;
    let lens = match value["lens"].as_str() {
        Some("author") => Lens::Author,
        Some("character") => Lens::Character,
        Some("dramatic-irony") => Lens::DramaticIrony,
        _ => return Err(invalid("invalid writing frame lens")),
    };
    if lens != Lens::Author && pov_id.is_none() {
        return Err(invalid(
            "this writing lens requires an explicit POV character",
        ));
    }
    let time = &value["at"];
    closed(time, &["timeline", "tick", "order"])?;
    let timeline = time["timeline"]
        .as_str()
        .filter(|value| !value.trim().is_empty() && value.len() <= 128)
        .ok_or_else(|| invalid("writing frame requires an explicit timeline"))?;
    let at = StoryTime::new(
        timeline,
        decimal(&time["tick"], 64)?,
        decimal(&time["order"], 32)? as i32,
    )?;
    Ok(Frame {
        fingerprint: canonical_fingerprint,
        work,
        scene_id,
        pov_id,
        at,
        lens,
    })
}

fn checked_canonical<'a>(world: &'a World, frame: &'a Frame) -> Result<Option<CharacterScope<'a>>> {
    // Native callers can construct Frame directly; retain the same input bounds.
    decode(&serde_json::to_string(frame)?)?;
    if !validation::validate_world(world).valid {
        return Err(source_invalid());
    }
    if frame.fingerprint != world.fingerprint
        || world
            .story_time(frame.at.tick, Some(&frame.at.timeline), frame.at.order)
            .is_err()
    {
        return Err(unavailable());
    }
    for (id, kind) in [
        (frame.scene_id.as_deref(), "scene"),
        (frame.pov_id.as_deref(), "character"),
    ] {
        if id.is_some_and(|id| {
            world
                .records
                .get(id)
                .is_none_or(|record| record.kind != kind)
        }) {
            return Err(unavailable());
        }
    }
    if frame.lens == Lens::Author {
        Ok(None)
    } else {
        CharacterScope::with_scene(
            world,
            frame.pov_id.as_deref().ok_or_else(unavailable)?,
            &frame.at,
            frame.scene_id.as_deref(),
        )
        .map(Some)
        .map_err(|_| unavailable())
    }
}

fn checked_authors(world: &World, authors: &manuscript::Snapshot) -> Result<()> {
    if authors.revision != world.revision {
        return Err(unavailable());
    }
    manuscript::verify_snapshot(authors).map_err(|_| unavailable())
}

fn checked<'a>(
    world: &'a World,
    authors: Option<&manuscript::Snapshot>,
    frame: &'a Frame,
) -> Result<Option<CharacterScope<'a>>> {
    let scope = checked_canonical(world, frame)?;
    if let Some(selection) = &frame.work {
        let authors = authors.ok_or_else(unavailable)?;
        if selection.author_fingerprint != authors.fingerprint {
            return Err(unavailable());
        }
        checked_authors(world, authors)?;
        let work = authors
            .documents
            .get(&selection.work_id)
            .ok_or_else(unavailable)?;
        manuscript::validate_work_references(world, authors, work).map_err(|_| unavailable())?;
    }
    Ok(scope)
}

fn current_matches(world: &World) -> Result<()> {
    let fresh = historical::load_current_world(&world.root).map_err(|_| unavailable())?;
    if fresh.fingerprint != world.fingerprint {
        return Err(unavailable());
    }
    Ok(())
}

/// Filesystem adapter for private work binding or explicit author work discovery.
/// Unbound operations without author discovery remain filesystem-independent.
/// Current reads fence the complete canonical/private pair before returning;
/// historical reads use the same verified original commit without live fallback.
pub fn with_author_snapshot(
    world: &World,
    frame: &Frame,
    discover_works: bool,
    read: impl FnOnce(Option<&manuscript::Snapshot>) -> Result<Json>,
) -> Result<Json> {
    if frame.work.is_none() && !(discover_works && frame.lens == Lens::Author) {
        return read(None);
    }
    checked_canonical(world, frame)?;
    let publication = if let Some(revision) = world.revision.as_deref() {
        let original = historical::load_world(&world.root, revision).map_err(|_| unavailable())?;
        if original.fingerprint != world.fingerprint {
            return Err(unavailable());
        }
        None
    } else {
        let stamp = publication::read_stamp(&world.root).map_err(|_| unavailable())?;
        current_matches(world)?;
        stamp
    };
    let authors =
        manuscript::snapshot(&world.root, world.revision.as_deref()).map_err(|_| unavailable())?;
    let result = read(Some(&authors));
    if world.revision.is_none() {
        current_matches(world)?;
        let fresh = manuscript::snapshot(&world.root, None).map_err(|_| unavailable())?;
        if fresh.fingerprint != authors.fingerprint
            || publication::read_stamp(&world.root).map_err(|_| unavailable())? != publication
        {
            return Err(unavailable());
        }
    }
    result
}

/// Echo a valid captured selection without storing it or supplying missing fields.
pub fn resolve(
    world: &World,
    authors: Option<&manuscript::Snapshot>,
    frame: &Frame,
) -> Result<Json> {
    checked(world, authors, frame)?;
    Ok(json!(frame))
}

fn point(world: &World, value: Option<&Yaml>) -> Result<StoryTime> {
    StoryTime::from_yaml(value.ok_or_else(source_invalid)?, &world.default_timeline())
        .map_err(|_| source_invalid())
}

fn name(record: &Record) -> Json {
    json!({"id":record.id,"label":record.title})
}

fn sort_names(items: &mut [Json]) {
    items.sort_by_key(|item| {
        (
            crate::reader::casefold::fold(item["label"].as_str().unwrap()),
            item["id"].as_str().unwrap().to_owned(),
        )
    });
}

/// Admit choices before exposing labels, ordering or complete-list bounds.
pub fn choices(
    world: &World,
    authors: Option<&manuscript::Snapshot>,
    frame: &Frame,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid("writing frame limit must be from 1 to 1000"));
    }
    let scope = checked(world, authors, frame)?;
    let mut works = Vec::new();
    let mut author_fingerprint = None;
    if frame.lens == Lens::Author {
        if let Some(authors) = authors {
            checked_authors(world, authors)?;
            for work in authors
                .documents
                .values()
                .filter(|record| record.kind == "work")
            {
                manuscript::validate_work_references(world, authors, work)
                    .map_err(|_| unavailable())?;
                works.push(name(work));
            }
            if !works.is_empty() {
                author_fingerprint = Some(authors.fingerprint.as_str());
            }
        }
    }
    let mut scenes = Vec::new();
    let mut povs = Vec::new();
    let mut moments = Vec::new();
    for record in world.records.values() {
        match record.kind.as_str() {
            "scene" => {
                if let Some(viewer) = frame.pov_id.as_deref().filter(|_| scope.is_some())
                    && (!narrative::labels_match(record, viewer)?
                        || !scene::present(world, record, viewer, &frame.at)?)
                {
                    continue;
                }
                let time = record.get("time").ok_or_else(source_invalid)?;
                if !point(world, time.get("start"))?.not_after(&frame.at) {
                    continue;
                }
                scenes.push(name(record));
                for key in ["start", "current", "end"] {
                    if let Some(value) = time.get(key).filter(|value| !value.is_null()) {
                        let at = point(world, Some(value))?;
                        if at.not_after(&frame.at) {
                            moments.push((at.clone(), format!("{}:{key}", record.id),
                                json!({"id":format!("{}:{key}",record.id),"label":format!("{} — {key}",record.title),"at":at})));
                        }
                    }
                }
            }
            "character" => {
                if scope
                    .as_ref()
                    .map(|scope| scope.visible(record))
                    .transpose()?
                    .unwrap_or(true)
                {
                    povs.push(name(record));
                }
            }
            "event" => {
                if !scope
                    .as_ref()
                    .map(|scope| scope.visible(record))
                    .transpose()?
                    .unwrap_or(true)
                {
                    continue;
                }
                let at = point(world, record.get("time"))?;
                if at.not_after(&frame.at) {
                    moments.push((
                        at.clone(),
                        format!("{}:time", record.id),
                        json!({"id":format!("{}:time",record.id),"label":record.title,"at":at}),
                    ));
                }
            }
            _ => {}
        }
    }
    if [works.len(), scenes.len(), povs.len(), moments.len()]
        .into_iter()
        .any(|count| count > limit)
    {
        return Err(invalid(
            "writing frame choices exceed the limit; refusing an incomplete result",
        ));
    }
    sort_names(&mut works);
    sort_names(&mut scenes);
    sort_names(&mut povs);
    moments.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    Ok(
        json!({"protocol":crate::CONTRACT,"frame":frame,"works":works,"authorFingerprint":author_fingerprint,"scenes":scenes,"povs":povs,
        "moments":moments.into_iter().map(|(_,_,value)|value).collect::<Vec<_>>(),"limit":limit}),
    )
}

fn selected_scene(
    world: &World,
    record: &Record,
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Json> {
    let allowed = scope.allowed_ids()?;
    let location = record.string("location").filter(|id| allowed.contains(*id));
    let mut participants = Vec::new();
    for member in record.list("participants").into_iter().flatten() {
        let id = member
            .get("character")
            .and_then(Yaml::as_str)
            .ok_or_else(source_invalid)?;
        if allowed.contains(id) && scene::present(world, record, id, at)? {
            participants.push(id);
        }
    }
    participants.sort_unstable();
    let roots = |key| {
        record
            .list(key)
            .into_iter()
            .flatten()
            .filter_map(Yaml::as_str)
            .filter(|id| allowed.contains(*id))
            .collect::<Vec<_>>()
    };
    Ok(
        json!({"sceneId":record.id,"effectiveTime":at,"locationId":location,
        "presentParticipants":participants,"objects":roots("objects"),"environments":roots("environments")}),
    )
}

fn brief_selection(ids: &[String]) -> Result<()> {
    if ids.len() > 100
        || ids
            .iter()
            .any(|id| !validation::valid_id_for_kind(id, "context-brief"))
    {
        return Err(invalid(
            "writing context requires at most 100 exact brief IDs",
        ));
    }
    Ok(())
}

/// Complete native evidence at one captured frame. Required pins precede budgets.
pub fn context(
    world: &World,
    authors: Option<&manuscript::Snapshot>,
    frame: &Frame,
    brief_ids: &[String],
    max_characters: usize,
) -> Result<Json> {
    context_with_families(
        world,
        authors,
        frame,
        brief_ids,
        &FamilySelection::default(),
        max_characters,
    )
}

/// Optional specialized family reads share the exact captured frame horizon and
/// independently admitted lens. They never alter the captured frame itself.
pub fn context_with_families(
    world: &World,
    authors: Option<&manuscript::Snapshot>,
    frame: &Frame,
    brief_ids: &[String],
    families: &FamilySelection,
    max_characters: usize,
) -> Result<Json> {
    if !(1..=1_000_000).contains(&max_characters) {
        return Err(invalid(
            "writing context maxCharacters must be from 1 to 1000000",
        ));
    }
    brief_selection(brief_ids)?;
    families.validate()?;
    if let Some(magic) = &families.magic {
        if frame.lens != Lens::Author && magic.belief_targets.is_empty() {
            return Err(invalid(
                "character magic context requires one to eight explicit learned targets",
            ));
        }
        if frame.lens != Lens::Character && magic.author.is_none() {
            return Err(invalid(
                "author magic context requires an explicit author selection",
            ));
        }
    }
    let scope = checked(world, authors, frame)?;
    let viewer = frame
        .pov_id
        .as_deref()
        .ok_or_else(|| invalid("writing context requires an explicit scene and POV"))?;
    let scene_id = frame
        .scene_id
        .as_deref()
        .ok_or_else(|| invalid("writing context requires an explicit scene and POV"))?;
    let scene_record = world.records.get(scene_id).ok_or_else(unavailable)?;
    // Collect both required sets before building or measuring either component.
    let character_pins = if frame.lens != Lens::Author && !brief_ids.is_empty() {
        Some(briefs::collect_character(
            world,
            brief_ids,
            viewer,
            &frame.at,
            scope.as_ref().ok_or_else(unavailable)?,
            1000,
        ))
    } else {
        None
    };
    let author_pins = if frame.lens != Lens::Character && !brief_ids.is_empty() {
        Some(briefs::collect_full_author(
            world,
            brief_ids,
            Some(&frame.at),
            1000,
        ))
    } else {
        None
    };
    if matches!(character_pins, Some(Ok(None))) || matches!(author_pins, Some(Ok(None))) {
        return Err(unavailable());
    }
    let family_result = family_components(world, frame, families, scope.as_ref());
    if matches!(&family_result, Err(Error::NotFound(_))) {
        return Err(unavailable());
    }
    let character_pins = character_pins.transpose()?.flatten();
    let author_pins = author_pins.transpose()?.flatten();
    let (character_families, author_families) = family_result?;
    let mut result = json!({"protocol":crate::CONTRACT,"perspective":frame.lens,
        "scope":"captured-writing-frame","frame":frame,"maxCharacters":max_characters});
    if frame.lens != Lens::Author {
        let scope = scope.as_ref().ok_or_else(unavailable)?;
        let mut packet = character::context_in_scope(world, viewer, &frame.at, scope, 1_000_000)?;
        packet.as_object_mut().unwrap().remove("maxCharacters");
        packet["scene"] = selected_scene(world, scene_record, &frame.at, scope)?;
        packet["present"] = present::context(world, scene_record, viewer, &frame.at, scope)?;
        if let Some(pins) = character_pins {
            packet["briefs"] = json!(pins);
        }
        if let Some(families) = character_families {
            packet["familyContexts"] = families;
        }
        result["characterPrompt"] = packet;
    }
    if frame.lens != Lens::Character {
        let mut packet = runtime::context(world, viewer, &frame.at, 1_000_000)?;
        packet.as_object_mut().unwrap().remove("maxCharacters");
        packet["scene"] = scene::query(world, scene_id, &frame.at)?;
        if let Some(pins) = author_pins {
            packet["briefs"] = json!(pins);
        }
        if let Some(families) = author_families {
            packet["familyContexts"] = families;
        }
        result["authorMargin"] = packet;
    }
    let needed = serde_json::to_string(&result)?.chars().count();
    if needed > max_characters {
        return Err(invalid(&format!(
            "complete writing context needs {needed} characters; maxCharacters is {max_characters}"
        )));
    }
    Ok(result)
}

fn family_components(
    world: &World,
    frame: &Frame,
    families: &FamilySelection,
    scope: Option<&CharacterScope<'_>>,
) -> Result<(Option<Json>, Option<Json>)> {
    let viewer = frame.pov_id.as_deref().unwrap();
    let mut reads = Vec::new();
    if frame.lens != Lens::Author {
        let scope = scope.ok_or_else(unavailable)?;
        if let Some(request) = &families.species {
            reads.push((
                true,
                "species",
                species::character_compose_in_scope(
                    world,
                    viewer,
                    &request.individual_id,
                    &request.selection,
                    &request.belief_targets,
                    &frame.at,
                    scope,
                ),
            ));
        }
        if let Some(request) = &families.magic {
            reads.push((
                true,
                "magic",
                magic::character_in_scope(world, viewer, &request.belief_targets, &frame.at, scope),
            ));
        }
        if let Some(request) = &families.social {
            reads.push((
                true,
                "social",
                social::character_context_in_scope(
                    world,
                    viewer,
                    &request.selection(),
                    &request.belief_targets,
                    &frame.at,
                    scope,
                ),
            ));
        }
        if let Some(request) = &families.environment {
            reads.push((
                true,
                "environment",
                environment::character_context_in_scope(
                    world,
                    viewer,
                    &request.contexts,
                    &request.belief_targets,
                    &frame.at,
                    scope,
                ),
            ));
        }
    }
    if frame.lens != Lens::Character {
        if let Some(request) = &families.species {
            reads.push((
                false,
                "species",
                species::compose(
                    world,
                    &request.individual_id,
                    &request.selection,
                    Some(&frame.at),
                    "author",
                    "author",
                ),
            ));
        }
        if let Some(request) = &families.magic {
            let author = request.author.as_ref().ok_or_else(|| {
                invalid("author magic context requires an explicit author selection")
            })?;
            reads.push((
                false,
                "magic",
                magic::compose(
                    world,
                    &author.root_id,
                    &author.selection,
                    Some(&frame.at),
                    "author",
                    "author",
                ),
            ));
        }
        if let Some(request) = &families.social {
            reads.push((
                false,
                "social",
                social::context(world, &request.selection(), &frame.at, "author", "author"),
            ));
        }
        if let Some(request) = &families.environment {
            reads.push((
                false,
                "environment",
                environment::context(world, &request.contexts, &frame.at, "author", "author"),
            ));
        }
    }
    // Resolve all selected families before returning a limit failure. An
    // inaccessible required family never returns a partial neighboring packet.
    if reads
        .iter()
        .any(|(_, _, result)| matches!(result, Err(Error::NotFound(_))))
    {
        return Err(unavailable());
    }
    let mut character = serde_json::Map::new();
    let mut author = serde_json::Map::new();
    for (is_character, family, result) in reads {
        let output = if is_character {
            &mut character
        } else {
            &mut author
        };
        output.insert(family.into(), result?);
    }
    Ok((
        (!character.is_empty()).then_some(Json::Object(character)),
        (!author.is_empty()).then_some(Json::Object(author)),
    ))
}
