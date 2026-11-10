//! Explicit operator configuration for installed reader policies. These codecs
//! are never exposed as HTTP requests and never derive authority from a reader.
use crate::{Error, Result, model::StoryTime, reader, reveal::CanonicalScope};
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    path::Path,
};

pub const SCHEMA: &str = "wedl-reader-policy/v1";
pub const MAX_BYTES: usize = 1024 * 1024;
fn invalid() -> Error {
    Error::InvalidInput("invalid trusted reader policy configuration".into())
}
fn required_nullable<'de, D, T>(decoder: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(decoder)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScopeInput {
    world_id: String,
    record_ids: Vec<String>,
    #[serde(deserialize_with = "required_nullable")]
    sections: Option<BTreeMap<String, Vec<String>>>,
}
impl ScopeInput {
    fn into_scope(self) -> Result<CanonicalScope> {
        let records: BTreeSet<_> = self.record_ids.iter().cloned().collect();
        if records.len() != self.record_ids.len() || records.len() > 4096 {
            return Err(invalid());
        }
        let sections = self
            .sections
            .map(|items| {
                items
                    .into_iter()
                    .map(|(id, paths)| {
                        let unique: BTreeSet<_> = paths.iter().cloned().collect();
                        if unique.len() != paths.len() || paths.len() > 4096 {
                            return Err(invalid());
                        }
                        Ok((id, unique))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()
            })
            .transpose()?;
        let scope = CanonicalScope {
            world_id: self.world_id,
            records,
            sections,
        };
        scope.validate()?;
        Ok(scope)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeInput {
    timeline: String,
    tick: String,
    order: String,
}
impl TimeInput {
    fn into_time(self) -> Result<StoryTime> {
        let tick = self.tick.parse::<i64>().map_err(|_| invalid())?;
        let order = self.order.parse::<i32>().map_err(|_| invalid())?;
        if tick.to_string() != self.tick
            || order.to_string() != self.order
            || self.timeline.chars().count() > 256
        {
            return Err(invalid());
        }
        StoryTime::new(self.timeline, tick, order)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PublicInput {
    schema: String,
    kind: String,
    revision: String,
    work_id: String,
    plan_id: String,
    canonical_scope: ScopeInput,
    generation: String,
    max_occurrence: usize,
    cursor_secret: String,
    publication_id: String,
    at: TimeInput,
    text_policy: String,
    #[serde(default)]
    presentation: Option<PresentationInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationInput {
    edition: Option<String>,
    #[serde(default)]
    positions: Vec<PositionInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionInput {
    occurrence: usize,
    label: String,
}
impl PresentationInput {
    fn into_labels(self) -> Result<reader::PublicLabels> {
        let count = self.positions.len();
        let positions = self
            .positions
            .into_iter()
            .map(|item| (item.occurrence, item.label))
            .collect::<BTreeMap<_, _>>();
        if count != positions.len() {
            return Err(invalid());
        }
        let labels = reader::PublicLabels {
            edition: self.edition,
            positions,
        };
        labels.validate()?;
        Ok(labels)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SimulationInput {
    schema: String,
    kind: String,
    revision: String,
    work_id: String,
    plan_id: String,
    canonical_scope: ScopeInput,
    generation: String,
    max_occurrence: usize,
    cursor_secret: String,
    principal_id: String,
    author_permissions: ScopeInput,
}
fn secret(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let bytes = hex::decode(value).map_err(|_| invalid())?;
    bytes.try_into().map_err(|_| invalid())
}
fn text_policy(value: &str) -> Result<reader::TextPolicy> {
    match value {
        "none" => Ok(reader::TextPolicy::None),
        "all-at-pinned-plan" => Ok(reader::TextPolicy::AllAtPinnedPlan),
        _ => Err(invalid()),
    }
}

// Count and reject duplicate map keys before constructing the typed policy.
// JSON has no aliases; the byte/depth/node/collection ceilings bound allocations.
struct Budget {
    nodes: usize,
}
struct Seed<'a> {
    budget: &'a mut Budget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<(), D::Error> {
        self.budget.nodes += 1;
        if self.depth > 16 || self.budget.nodes > 131072 {
            return Err(de::Error::custom("policy structure bound"));
        }
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded policy JSON")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<(), E> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(E::custom("nonfinite policy value"))
        }
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<(), E> {
        if value.len() <= 4096 {
            Ok(())
        } else {
            Err(E::custom("policy string bound"))
        }
    }
    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<(), E> {
        self.visit_str(&value)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<(), A::Error> {
        let mut count = 0;
        while values
            .next_element_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?
            .is_some()
        {
            count += 1;
            if count > 4096 {
                return Err(de::Error::custom("policy array bound"));
            }
        }
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut fields: A) -> std::result::Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = fields.next_key::<String>()? {
            self.budget.nodes += 1;
            if key.len() > 512 || keys.len() >= 4096 || !keys.insert(key) {
                return Err(de::Error::custom("duplicate or over-bound policy field"));
            }
            fields.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}
fn decode<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    if raw.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    Seed {
        budget: &mut Budget { nodes: 0 },
        depth: 0,
    }
    .deserialize(&mut decoder)
    .map_err(|_| invalid())?;
    decoder.end().map_err(|_| invalid())?;
    serde_json::from_str(raw).map_err(|_| invalid())
}
/// Read only the explicitly supplied operator file; no default policy is inferred.
pub fn read(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_BYTES as u64 {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    String::from_utf8(bytes).map_err(|_| invalid())
}
pub fn decode_public(root: &Path, raw: &str) -> Result<reader::PublicPolicy> {
    decode_public_with_labels(root, raw).map(|(policy, _)| policy)
}
/// Decode optional browser labels alongside the unchanged machine policy.
pub fn decode_public_with_labels(
    root: &Path,
    raw: &str,
) -> Result<(reader::PublicPolicy, reader::PublicLabels)> {
    let input: PublicInput = decode(raw)?;
    if input.schema != SCHEMA || input.kind != "public" {
        return Err(invalid());
    }
    let canonical_scope = input.canonical_scope.into_scope()?;
    let cursor_secret = secret(&input.cursor_secret)?;
    let at = input.at.into_time()?;
    let text_policy = text_policy(&input.text_policy)?;
    let labels = input
        .presentation
        .map(PresentationInput::into_labels)
        .transpose()?
        .unwrap_or_default();
    Ok((
        reader::PublicPolicy {
            repository: reader::repository_identity(root)?,
            revision: input.revision,
            work_id: input.work_id,
            plan_id: input.plan_id,
            canonical_scope,
            generation: input.generation,
            max_occurrence: input.max_occurrence,
            cursor_secret,
            publication_id: input.publication_id,
            at,
            text_policy,
        },
        labels,
    ))
}
pub fn decode_simulation(root: &Path, raw: &str) -> Result<reader::SimulationPolicy> {
    let input: SimulationInput = decode(raw)?;
    if input.schema != SCHEMA || input.kind != "author-simulation" {
        return Err(invalid());
    }
    let canonical_scope = input.canonical_scope.into_scope()?;
    let author_permissions = input.author_permissions.into_scope()?;
    let cursor_secret = secret(&input.cursor_secret)?;
    Ok(reader::SimulationPolicy {
        repository: reader::repository_identity(root)?,
        revision: input.revision,
        work_id: input.work_id,
        plan_id: input.plan_id,
        canonical_scope,
        generation: input.generation,
        max_occurrence: input.max_occurrence,
        cursor_secret,
        principal_id: input.principal_id,
        author_permissions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_labels_are_optional_closed_bounded_operator_input() {
        let labels = decode::<PresentationInput>(
            r#"{"edition":"Published story","positions":[{"occurrence":0,"label":"Arrival"}]}"#,
        )
        .unwrap()
        .into_labels()
        .unwrap();
        assert_eq!(labels.edition.as_deref(), Some("Published story"));
        assert_eq!(
            labels.positions.get(&0).map(String::as_str),
            Some("Arrival")
        );
        let defaults = decode::<PresentationInput>("{}")
            .unwrap()
            .into_labels()
            .unwrap();
        assert!(defaults.edition.is_none() && defaults.positions.is_empty());
        for raw in [
            r#"{"edition":""}"#,
            r#"{"edition":"bad\nlabel"}"#,
            r#"{"positions":[{"occurrence":1024,"label":"Future"}]}"#,
            r#"{"positions":[{"occurrence":0,"label":"A"},{"occurrence":0,"label":"B"}]}"#,
            r#"{"positions":[{"occurrence":1,"label":"A","private":true}]}"#,
            r#"{"edition":"A","edition":"B"}"#,
        ] {
            assert!(
                decode::<PresentationInput>(raw)
                    .and_then(PresentationInput::into_labels)
                    .is_err(),
                "accepted {raw}"
            );
        }
    }
}
