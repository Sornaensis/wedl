//! Coordinated scene lifecycle and independent character movement, expressed
//! as one explicit native consequence Plan. No source is published here.
use crate::{
    Error, Result, consequences,
    model::{Record, StoryTime, World},
    runtime, state_schema, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

mod reconciliation;

pub const MAX_REQUEST_BYTES: usize = 16_384;
pub const MAX_TITLE_BYTES: usize = 1_024;
pub const MIN_CHARACTERS: usize = consequences::MIN_CHARACTERS;
pub const MAX_CHARACTERS: usize = consequences::MAX_CHARACTERS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub expected_fingerprint: String,
    pub at: StoryTime,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementEvent {
    pub id: String,
    pub title: String,
}

// Omission is the only absent optional request field. Explicit null must not
// silently discard an author-supplied destination or event.
fn optional<'de, D, T>(decoder: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(decoder).map(Some)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Action {
    #[serde(rename = "scene.create")]
    SceneCreate {
        scene_id: String,
        title: String,
        location_id: String,
        character_ids: Vec<String>,
        event: MovementEvent,
    },
    #[serde(rename = "scene.advance")]
    SceneAdvance {
        scene_id: String,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        location_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        event: Option<MovementEvent>,
    },
    #[serde(rename = "scene.close")]
    SceneClose { scene_id: String },
    #[serde(rename = "character.move")]
    CharacterMove {
        character_ids: Vec<String>,
        location_id: String,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        scene_id: Option<String>,
        event: MovementEvent,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRequest {
    expected_fingerprint: String,
    at: WireTime,
    action: Action,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTime {
    timeline: String,
    tick: String,
    order: String,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn identifier(value: &str, kind: &str) -> Result<()> {
    if !validation::valid_id_for_kind(value, kind) {
        return Err(invalid(format!(
            "scene-control requires an exact {kind} ID"
        )));
    }
    Ok(())
}

fn title(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > MAX_TITLE_BYTES {
        return Err(invalid(
            "scene-control title must be nonempty and within 1024 UTF-8 bytes",
        ));
    }
    Ok(())
}

fn characters(values: &[String]) -> Result<()> {
    if values.is_empty() || values.len() > MAX_REQUEST_BYTES / 31 {
        return Err(invalid(
            "scene-control requires a bounded nonempty characterIds list",
        ));
    }
    let mut seen = BTreeSet::new();
    for id in values {
        identifier(id, "character")?;
        if !seen.insert(id) {
            return Err(invalid("scene-control characterIds must be distinct"));
        }
    }
    Ok(())
}

fn validate_event(event: &MovementEvent) -> Result<()> {
    identifier(&event.id, "event")?;
    title(&event.title)
}

fn validate_request(request: &Request) -> Result<()> {
    if request.expected_fingerprint.len() != 64
        || !request
            .expected_fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "scene-control requires an exact lowercase source fingerprint",
        ));
    }
    if request.at.timeline.trim().is_empty() || request.at.timeline.len() > 128 {
        return Err(invalid(
            "scene-control timeline must be nonempty and within 128 UTF-8 bytes",
        ));
    }
    match &request.action {
        Action::SceneCreate {
            scene_id,
            title: name,
            location_id,
            character_ids,
            event,
        } => {
            identifier(scene_id, "scene")?;
            title(name)?;
            identifier(location_id, "location")?;
            characters(character_ids)?;
            validate_event(event)?;
        }
        Action::SceneAdvance {
            scene_id,
            location_id,
            event,
        } => {
            identifier(scene_id, "scene")?;
            if let Some(id) = location_id {
                identifier(id, "location")?;
            }
            if let Some(event) = event {
                validate_event(event)?;
                if location_id.is_none() {
                    return Err(invalid(
                        "scene.advance event requires an explicit locationId",
                    ));
                }
            }
        }
        Action::SceneClose { scene_id } => identifier(scene_id, "scene")?,
        Action::CharacterMove {
            character_ids,
            location_id,
            scene_id,
            event,
        } => {
            characters(character_ids)?;
            identifier(location_id, "location")?;
            if let Some(id) = scene_id {
                identifier(id, "scene")?;
            }
            validate_event(event)?;
        }
    }
    if serde_json::to_vec(request)?.len() > MAX_REQUEST_BYTES {
        return Err(invalid("scene-control request exceeds 16 KiB"));
    }
    Ok(())
}

/// Decode a closed request with exact decimal string coordinates. There are
/// no default clocks, aliases, generated record identities or optional nulls.
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("scene-control request exceeds 16 KiB"));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate scene-control JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid scene-control JSON"))?;
    let wire: WireRequest =
        serde_json::from_str(raw).map_err(|_| invalid("invalid scene-control request fields"))?;
    let tick =
        wire.at.tick.parse::<i64>().map_err(|_| {
            invalid("scene-control tick must be a canonical decimal string within i64")
        })?;
    let order = wire.at.order.parse::<i32>().map_err(|_| {
        invalid("scene-control order must be a canonical decimal string within i32")
    })?;
    if tick.to_string() != wire.at.tick || order.to_string() != wire.at.order {
        return Err(invalid(
            "scene-control coordinates must be canonical signed decimal strings",
        ));
    }
    let request = Request {
        expected_fingerprint: wire.expected_fingerprint,
        at: StoryTime::new(wire.at.timeline, tick, order)?,
        action: wire.action,
    };
    validate_request(&request)?;
    Ok(request)
}

fn exact<'a>(world: &'a World, id: &str, kind: &str) -> Result<&'a Record> {
    let record = world
        .records
        .get(id)
        .ok_or_else(|| Error::NotFound(format!("unknown exact {kind} ID {id:?}")))?;
    if record.kind != kind {
        return Err(invalid(format!("{id:?} is not a {kind}")));
    }
    Ok(record)
}

fn source_point(at: &StoryTime) -> Yaml {
    Mapping::from_iter([
        ("timeline".into(), at.timeline.clone().into()),
        ("tick".into(), at.tick.into()),
        ("order".into(), i64::from(at.order).into()),
    ])
    .into()
}

fn point(world: &World, value: &Yaml) -> Result<StoryTime> {
    StoryTime::from_yaml(value, &world.default_timeline())
}

fn optional_point(world: &World, value: Option<&Yaml>) -> Result<Option<StoryTime>> {
    value
        .filter(|value| !value.is_null())
        .map(|value| point(world, value))
        .transpose()
}

fn list<'a>(record: &'a Record, field: &str) -> &'a [Yaml] {
    record.list(field).map(Vec::as_slice).unwrap_or(&[])
}

// A planner is extending the authored interval. Native read presence correctly
// stops at the old current cursor, so it cannot decide prospective membership.
fn prospective_present(world: &World, scene: &Record, id: &str, at: &StoryTime) -> Result<bool> {
    let time = scene.get("time").expect("validated scene time");
    let start = point(world, &time["start"])?;
    let end = optional_point(world, time.get("end"))?;
    if !start.not_after(at) || end.as_ref().is_some_and(|end| !at.not_after(end)) {
        return Ok(false);
    }
    let Some(row) = list(scene, "participants")
        .iter()
        .find(|row| row["character"].as_str() == Some(id))
    else {
        return Ok(false);
    };
    let from = optional_point(world, row.get("from"))?.unwrap_or(start);
    let until = optional_point(world, row.get("until"))?.or(end);
    Ok(from.not_after(at) && until.as_ref().is_none_or(|until| at.not_after(until)))
}

fn present_characters(world: &World, scene: &Record, at: &StoryTime) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for row in list(scene, "participants") {
        let id = row["character"].as_str().expect("validated participant");
        if prospective_present(world, scene, id, at)? {
            result.push(id.into());
        }
    }
    Ok(result)
}

fn before(at: &StoryTime) -> StoryTime {
    StoryTime {
        order: at.order.saturating_sub(1),
        ..at.clone()
    }
}

fn active(world: &World) -> impl Iterator<Item = &Record> {
    world
        .records
        .values()
        .filter(|record| record.kind == "scene" && record.status() == "active")
}

fn not_before_cursor(world: &World, at: &StoryTime) -> Result<()> {
    if !world.current_time()?.not_after(at) {
        return Err(invalid(
            "scene-control cannot move the shared cursor backward or to another timeline; use an explicit current-time plan",
        ));
    }
    Ok(())
}

fn not_before_authored_cursor(world: &World, at: &StoryTime) -> Result<()> {
    if optional_point(world, world.world_record()?.get("current_time"))?
        .as_ref()
        .is_some_and(|cursor| !cursor.not_after(at))
    {
        return Err(invalid(
            "scene.create cannot move the authored world cursor backward or to another timeline; use an explicit current-time plan",
        ));
    }
    Ok(())
}

fn previous_time(world: &World) -> Result<Option<StoryTime>> {
    if let Some(at) = optional_point(world, world.world_record()?.get("current_time"))? {
        return Ok(Some(at));
    }
    active(world)
        .next()
        .map(|scene| point(world, &scene.frontmatter["time"]["current"]))
        .transpose()
}

fn active_scene<'a>(world: &'a World, id: &str, at: &StoryTime) -> Result<&'a Record> {
    let scene = exact(world, id, "scene")?;
    if scene.status() != "active" {
        return Err(invalid("scene-control requires an active scene"));
    }
    not_before_cursor(world, at)?;
    Ok(scene)
}

/// One patch per exact base record. Consequence operations are constructed
/// independently against the base; sequential patches would lose changes.
#[derive(Default)]
struct Changes {
    patches: BTreeMap<String, Mapping>,
    creates: Vec<Mapping>,
    before_bytes: usize,
}

impl Changes {
    fn touch(&mut self, record: &Record) -> Result<&mut Mapping> {
        if !self.patches.contains_key(&record.id) {
            if self.patches.len() + self.creates.len() >= consequences::MAX_OPERATIONS {
                return Err(invalid("complete scene-control plan exceeds 64 operations"));
            }
            self.before_bytes = self.before_bytes.saturating_add(record.raw_bytes.len());
            if self.before_bytes > consequences::MAX_PREVIEW_BYTES {
                return Err(invalid("complete scene-control source exceeds 8 MiB"));
            }
        }
        Ok(self
            .patches
            .entry(record.id.clone())
            .or_insert_with(|| Mapping::from_iter([("kind".into(), record.kind.clone().into())])))
    }

    fn set(&mut self, record: &Record, key: &str, value: Yaml) -> Result<()> {
        let patch = self.touch(record)?;
        if record
            .get(key)
            .is_some_and(|old| state_schema::values_equal(old, &value))
        {
            patch.remove(key);
        } else {
            patch.insert(key.into(), value);
        }
        Ok(())
    }

    fn value<'a>(&'a self, record: &'a Record, key: &str) -> Option<&'a Yaml> {
        self.patches
            .get(&record.id)
            .and_then(|patch| patch.get(key))
            .or_else(|| record.get(key))
    }

    fn clock(&mut self, world: &World, scene: &Record, at: &StoryTime, close: bool) -> Result<()> {
        self.touch(scene)?;
        let time = self.value(scene, "time").expect("validated scene time");
        let same_current = point(world, &time["current"])? == *at;
        let same_end = optional_point(world, time.get("end"))?.as_ref() == Some(at);
        if same_current && (!close || same_end) {
            return Ok(());
        }
        let mut time = time.as_mapping().expect("validated time map").clone();
        if !same_current {
            time.insert("current".into(), source_point(at));
        }
        if close && !same_end {
            time.insert("end".into(), source_point(at));
        }
        if time.get("end").is_some_and(Yaml::is_null) {
            time.remove("end");
        }
        self.set(scene, "time", time.into())
    }

    fn shared_clock(&mut self, world: &World, at: &StoryTime) -> Result<()> {
        let owner = world.world_record()?;
        self.touch(owner)?;
        if optional_point(world, owner.get("current_time"))?.as_ref() != Some(at) {
            self.set(owner, "current_time", source_point(at))?;
        }
        for scene in active(world) {
            self.clock(world, scene, at, false)?;
        }
        Ok(())
    }

    fn create(&mut self, draft: Mapping) -> Result<()> {
        if self.patches.len() + self.creates.len() >= consequences::MAX_OPERATIONS {
            return Err(invalid("complete scene-control plan exceeds 64 operations"));
        }
        self.creates.push(draft);
        Ok(())
    }

    fn finish(self, fingerprint: &str) -> Result<consequences::Plan> {
        let mut operations = Vec::with_capacity(self.patches.len() + self.creates.len());
        for (entity_id, fields) in self.patches {
            operations.push(consequences::Operation::Update {
                entity_id,
                patch: serde_yaml::to_string(&fields)?,
            });
        }
        for fields in self.creates {
            operations.push(consequences::Operation::Create {
                draft: serde_yaml::to_string(&fields)?,
            });
        }
        let plan = consequences::Plan {
            expected_fingerprint: fingerprint.into(),
            operations,
            analysis: None,
        };
        consequences::validate_plan(&plan)?;
        Ok(plan)
    }
}

fn participants_for_patch(mut rows: Vec<Yaml>) -> Yaml {
    // Native source permits these nulls as absence; submitted typed rows use
    // omission. Only a genuinely changed participant array comes through here.
    for row in &mut rows {
        let fields = row.as_mapping_mut().expect("validated participant map");
        for key in ["from", "until", "role", "point_of_view"] {
            if fields.get(key).is_some_and(Yaml::is_null) {
                fields.remove(key);
            }
        }
    }
    rows.into()
}

fn detach(
    world: &World,
    changes: &mut Changes,
    ids: &[String],
    at: &StoryTime,
    exclude: Option<&str>,
) -> Result<()> {
    if at.order == i32::MIN {
        return Err(invalid(
            "cannot reconcile scene presence at the minimum same-tick order; choose a later explicit order",
        ));
    }
    let before = before(at);
    for scene in active(world).filter(|scene| Some(scene.id.as_str()) != exclude) {
        let mut endings = BTreeSet::new();
        for id in ids {
            if prospective_present(world, scene, id, &before)? {
                let row = list(scene, "participants")
                    .iter()
                    .find(|row| row["character"].as_str() == Some(id))
                    .unwrap();
                if optional_point(world, row.get("until"))?
                    .as_ref()
                    .is_none_or(|until| !until.not_after(&before))
                {
                    endings.insert(id.as_str());
                }
            }
        }
        if !endings.is_empty() {
            changes.touch(scene)?;
            let mut rows = list(scene, "participants").to_vec();
            for row in &mut rows {
                if endings.contains(row["character"].as_str().unwrap()) {
                    row["until"] = source_point(&before);
                }
            }
            changes.set(scene, "participants", participants_for_patch(rows))?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct ObjectLinks {
    holder: Option<String>,
    container: Option<String>,
    location: Option<String>,
}

fn entity_link(
    world: &World,
    kind: &str,
    key: &str,
    values: &BTreeMap<String, Yaml>,
) -> Option<String> {
    let definition = state_schema::field_definition(world, kind, key).ok()?;
    if definition.profile.is_some() || definition.value_type != state_schema::StateType::Entity {
        return None;
    }
    let id = values.get(key)?.get("entity")?.as_str()?;
    world.records.get(id).map(|record| record.id.clone())
}

fn placement_links(world: &World, id: &str, at: &StoryTime) -> Result<ObjectLinks> {
    let owner = world
        .records
        .get(id)
        .ok_or_else(|| Error::NotFound(format!("unknown exact container ID {id:?}")))?;
    let values = runtime::resolved_state(world, id, at)?;
    // Listed inventory is validated as objects. Recursive references obey the
    // authored declaration's optional kind constraint, without inventing one.
    Ok(ObjectLinks {
        holder: entity_link(world, &owner.kind, "holder", &values),
        container: entity_link(world, &owner.kind, "container", &values),
        location: entity_link(world, &owner.kind, "location", &values),
    })
}

fn transfer_objects(
    world: &World,
    changes: &mut Changes,
    ids: &[String],
    at: &StoryTime,
    exclude: Option<&str>,
) -> Result<Vec<String>> {
    let before = before(at);
    let mut moved = BTreeSet::new();
    for scene in active(world).filter(|scene| Some(scene.id.as_str()) != exclude) {
        let mut cast = BTreeSet::new();
        for id in ids {
            if prospective_present(world, scene, id, &before)? {
                cast.insert(id.as_str());
            }
        }
        if cast.is_empty() {
            continue;
        }
        let mut states = BTreeMap::new();
        for value in list(scene, "objects") {
            let id = value.as_str().expect("validated object ID");
            states.insert(id, placement_links(world, id, &before)?);
        }
        let mut transferred: BTreeSet<&str> = states
            .iter()
            .filter(|(_, state)| state.holder.as_deref().is_some_and(|id| cast.contains(id)))
            .map(|(id, _)| *id)
            .collect();
        loop {
            let additional: Vec<_> = states
                .iter()
                .filter(|(id, state)| {
                    !transferred.contains(**id)
                        && state
                            .container
                            .as_deref()
                            .is_some_and(|container| transferred.contains(container))
                })
                .map(|(id, _)| *id)
                .collect();
            if additional.is_empty() {
                break;
            }
            transferred.extend(additional);
        }
        if !transferred.is_empty() {
            let kept = list(scene, "objects")
                .iter()
                .filter(|id| !transferred.contains(id.as_str().unwrap()))
                .cloned()
                .collect::<Vec<_>>();
            changes.set(scene, "objects", kept.into())?;
            moved.extend(transferred.into_iter().map(str::to_owned));
        }
    }
    Ok(moved.into_iter().collect())
}

fn stationary_objects(
    world: &World,
    scene: &Record,
    cast: &[String],
    at: &StoryTime,
) -> Result<Vec<String>> {
    let mut blocked = Vec::new();
    let mut states = BTreeMap::new();
    for value in list(scene, "objects") {
        let id = value.as_str().expect("validated object ID");
        let state = placement_links(world, id, at)?;
        let at_old_location = state.location.as_deref() == scene.string("location");
        states.insert(id.to_owned(), state);
        if !at_old_location {
            continue;
        }
        let mut next = Some(id.to_owned());
        let mut seen = BTreeSet::new();
        let mut carried = false;
        while let Some(id) = next {
            if !seen.insert(id.clone()) {
                break;
            }
            if !states.contains_key(&id) {
                states.insert(id.clone(), placement_links(world, &id, at)?);
            }
            let state = &states[&id];
            if state.holder.as_ref().is_some_and(|id| cast.contains(id)) {
                carried = true;
                break;
            }
            next = state.container.clone();
        }
        if !carried {
            blocked.push(id.to_owned());
        }
    }
    blocked.sort();
    Ok(blocked)
}

fn new_participant(id: &str, at: &StoryTime) -> Yaml {
    Mapping::from_iter([
        ("character".into(), id.into()),
        ("from".into(), source_point(at)),
    ])
    .into()
}

fn join_scene(
    world: &World,
    changes: &mut Changes,
    scene: &Record,
    ids: &[String],
    objects: Vec<String>,
    at: &StoryTime,
) -> Result<()> {
    let existing: BTreeSet<_> = list(scene, "participants")
        .iter()
        .map(|row| row["character"].as_str().unwrap())
        .collect();
    for id in ids {
        if existing.contains(id.as_str()) && !prospective_present(world, scene, id, at)? {
            return Err(invalid(
                "character.move cannot re-enter an ended scene presence; use a new scene or an explicit record plan",
            ));
        }
    }
    let new: Vec<_> = ids
        .iter()
        .filter(|id| !existing.contains(id.as_str()))
        .collect();
    if !new.is_empty() {
        changes.touch(scene)?;
        let mut rows = list(scene, "participants").to_vec();
        rows.extend(new.into_iter().map(|id| new_participant(id, at)));
        changes.set(scene, "participants", participants_for_patch(rows))?;
    }
    let old_objects = list(scene, "objects");
    let additions: Vec<_> = objects
        .into_iter()
        .filter(|id| !old_objects.iter().any(|value| value.as_str() == Some(id)))
        .collect();
    if !additions.is_empty() {
        changes.touch(scene)?;
        let mut objects = old_objects.to_vec();
        objects.extend(additions.into_iter().map(Yaml::from));
        changes.set(scene, "objects", objects.into())?;
    }
    Ok(())
}

fn effect_id(event: &str, character: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"wedl-scene-movement-effect\0");
    hash.update(event.as_bytes());
    hash.update(b"\0");
    hash.update(character.as_bytes());
    let digest = hash.finalize();
    let mut value = u128::from_be_bytes(digest[..16].try_into().expect("128 digest bits"));
    let mut suffix = [b'0'; 26];
    for digit in suffix.iter_mut().rev() {
        *digit = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ"[(value & 31) as usize];
        value >>= 5;
    }
    format!(
        "effect_{}",
        std::str::from_utf8(&suffix).expect("Crockford digits")
    )
}

fn movement_event(
    world: &World,
    event: &MovementEvent,
    cast: &[String],
    location: &str,
    at: &StoryTime,
) -> Result<Mapping> {
    if world.records.contains_key(&event.id) {
        return Err(Error::Conflict(format!(
            "movement event ID {:?} already exists",
            event.id
        )));
    }
    let definition = state_schema::field_definition(world, "character", "location")?;
    if definition.profile.is_some() || definition.value_type != state_schema::StateType::Entity {
        return Err(invalid(
            "movement requires an ordinary entity-valued character.location state key",
        ));
    }
    let destination: Yaml = Mapping::from_iter([("entity".into(), location.into())]).into();
    state_schema::validate_value(world, &definition, &destination)?;
    let mut participants = Vec::new();
    let mut effects = Vec::new();
    for id in cast {
        exact(world, id, "character")?;
        participants.push(Yaml::Mapping(Mapping::from_iter([
            ("character".into(), id.clone().into()),
            ("role".into(), "moving character".into()),
        ])));
        effects.push(Yaml::Mapping(Mapping::from_iter([
            ("id".into(), effect_id(&event.id, id).into()),
            ("target".into(), id.clone().into()),
            ("key".into(), "location".into()),
            ("operation".into(), "set".into()),
            ("value".into(), destination.clone()),
        ])));
    }
    Ok(Mapping::from_iter([
        ("kind".into(), "event".into()),
        ("id".into(), event.id.clone().into()),
        ("title".into(), event.title.clone().into()),
        ("status".into(), "canonical".into()),
        ("time".into(), source_point(at)),
        ("location".into(), location.into()),
        ("participants".into(), participants.into()),
        ("effects".into(), effects.into()),
        ("body".into(), format!("# {}\n", event.title).into()),
    ]))
}

fn construct(world: &World, request: &Request) -> Result<consequences::Plan> {
    validate_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the scene-control request snapshot".into(),
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "base snapshot failed validation; run validate for diagnostics".into(),
        ));
    }
    world.story_time(
        request.at.tick,
        Some(&request.at.timeline),
        request.at.order,
    )?;
    let at = &request.at;
    let mut changes = Changes::default();
    match &request.action {
        Action::SceneCreate {
            scene_id,
            title,
            location_id,
            character_ids,
            event,
        } => {
            exact(world, location_id, "location")?;
            for id in character_ids {
                exact(world, id, "character")?;
            }
            if world.records.contains_key(scene_id) {
                return Err(Error::Conflict(format!(
                    "scene ID {scene_id:?} already exists"
                )));
            }
            not_before_authored_cursor(world, at)?;
            detach(world, &mut changes, character_ids, at, None)?;
            let objects = transfer_objects(world, &mut changes, character_ids, at, None)?;
            changes.shared_clock(world, at)?;
            changes.create(Mapping::from_iter([
                ("kind".into(), "scene".into()),
                ("id".into(), scene_id.clone().into()),
                ("title".into(), title.clone().into()),
                ("status".into(), "active".into()),
                ("location".into(), location_id.clone().into()),
                (
                    "time".into(),
                    Mapping::from_iter([
                        ("start".into(), source_point(at)),
                        ("current".into(), source_point(at)),
                    ])
                    .into(),
                ),
                (
                    "participants".into(),
                    character_ids
                        .iter()
                        .map(|id| new_participant(id, at))
                        .collect::<Vec<_>>()
                        .into(),
                ),
                (
                    "objects".into(),
                    objects
                        .into_iter()
                        .map(Yaml::from)
                        .collect::<Vec<_>>()
                        .into(),
                ),
                ("body".into(), format!("# {title}\n").into()),
            ]))?;
            changes.create(movement_event(
                world,
                event,
                character_ids,
                location_id,
                at,
            )?)?;
        }
        Action::SceneAdvance {
            scene_id,
            location_id,
            event,
        } => {
            let scene = active_scene(world, scene_id, at)?;
            let cast = present_characters(world, scene, at)?;
            if let Some(location) = location_id {
                exact(world, location, "location")?;
                let blocked = stationary_objects(world, scene, &cast, &before(at))?;
                if !blocked.is_empty() {
                    return Err(invalid(format!(
                        "scene.advance cannot relocate stationary listed objects: {}; author their object state changes first",
                        blocked.join(", ")
                    )));
                }
                if cast.is_empty() {
                    if event.is_some() {
                        return Err(invalid(
                            "scene.advance has no present characters for the supplied movement event",
                        ));
                    }
                } else {
                    let event = event.as_ref().ok_or_else(|| {
                        invalid("scene.advance relocation requires an explicit movement event")
                    })?;
                    changes.create(movement_event(world, event, &cast, location, at)?)?;
                }
                changes.set(scene, "location", location.clone().into())?;
            }
            changes.shared_clock(world, at)?;
        }
        Action::SceneClose { scene_id } => {
            let scene = active_scene(world, scene_id, at)?;
            changes.clock(world, scene, at, true)?;
            changes.set(scene, "status", "closed".into())?;
        }
        Action::CharacterMove {
            character_ids,
            location_id,
            scene_id,
            event,
        } => {
            exact(world, location_id, "location")?;
            for id in character_ids {
                exact(world, id, "character")?;
            }
            if let Some(scene_id) = scene_id {
                let scene = active_scene(world, scene_id, at)?;
                if scene.string("location") != Some(location_id) {
                    return Err(invalid(
                        "character.move destination must match the active scene location",
                    ));
                }
                detach(world, &mut changes, character_ids, at, Some(scene_id))?;
                let objects =
                    transfer_objects(world, &mut changes, character_ids, at, Some(scene_id))?;
                changes.shared_clock(world, at)?;
                join_scene(world, &mut changes, scene, character_ids, objects, at)?;
            }
            changes.create(movement_event(
                world,
                event,
                character_ids,
                location_id,
                at,
            )?)?;
        }
    }
    changes.finish(&request.expected_fingerprint)
}

fn prepare(
    world: &World,
    request: &Request,
    maximum: usize,
) -> Result<(consequences::Plan, consequences::Prepared)> {
    let plan = construct(world, request)?;
    let prepared = consequences::prepare(world, &plan, maximum)?;
    if prepared.report["validation"]["valid"] != true {
        return Err(Error::Validation(format!(
            "scene-control candidate failed validation: {}",
            prepared.report["validation"]["diagnostics"]
        )));
    }
    reconciliation::validate(&prepared.candidate)?;
    Ok((plan, prepared))
}

/// Return a complete candidate-validated Plan. Invalid coordinated outcomes
/// are refused; ordinary consequence preview still supplies raw diagnostics.
pub fn plan(world: &World, request: &Request) -> Result<consequences::Plan> {
    prepare(world, request, MAX_CHARACTERS).map(|(plan, _)| plan)
}

fn bounded_result(value: &Json, maximum: usize) -> Result<()> {
    struct Bounded {
        bytes: Vec<u8>,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > consequences::MAX_PREVIEW_BYTES.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("complete scene-control JSON bound"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded { bytes: Vec::new() };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| invalid("scene-control preview exceeds 8 MiB of complete JSON"))?;
    let characters = std::str::from_utf8(&writer.bytes)
        .expect("JSON is UTF-8")
        .chars()
        .count();
    if characters > maximum {
        return Err(invalid(format!(
            "complete scene-control preview requires {characters} characters; maxCharacters is {maximum}"
        )));
    }
    Ok(())
}

/// Export the exact Plan and its complete ordinary consequence preview. The
/// wrapper, source/diffs and Plan all count toward one output bound.
pub fn preview(world: &World, request: &Request, max_characters: usize) -> Result<Json> {
    if !(MIN_CHARACTERS..=MAX_CHARACTERS).contains(&max_characters) {
        return Err(invalid(
            "scene-control preview maxCharacters must be 256..1000000",
        ));
    }
    let (plan, prepared) = prepare(world, request, max_characters)?;
    let result = json!({
        "protocol": crate::CONTRACT, "operation":"scene-control-preview",
        "scope":"local-author-scene-control", "request":request,
        "previousTime":previous_time(world)?, "plan":plan, "preview":prepared.report,
        "maxCharacters":max_characters, "complete":true, "validationComplete":false,
        "writesPerformed":false, "canApply":false,
    });
    bounded_result(&result, max_characters)?;
    Ok(result)
}
