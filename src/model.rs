//! Canonical records retain YAML and original source bytes; projections are disposable.
use crate::error::{Error, Result};
use serde::{Serialize, Serializer, ser::SerializeStruct};
use serde_yaml::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StoryTime {
    pub timeline: String,
    pub tick: i64,
    pub order: i32,
}

impl StoryTime {
    pub fn new(timeline: impl Into<String>, tick: i64, order: i32) -> Result<Self> {
        let timeline = timeline.into();
        if timeline.trim().is_empty() {
            return Err(Error::InvalidInput(
                "timeline must be a nonempty string".into(),
            ));
        }
        Ok(Self {
            timeline,
            tick,
            order,
        })
    }

    pub fn from_yaml(value: &Value, default_timeline: &str) -> Result<Self> {
        let map = value
            .as_mapping()
            .ok_or_else(|| Error::Validation("story time must be a mapping".into()))?;
        let timeline = match map.get("timeline") {
            Some(value) => value
                .as_str()
                .ok_or_else(|| Error::Validation("timeline must be a string".into()))?,
            None => default_timeline,
        };
        let tick = map.get("tick").and_then(Value::as_i64).ok_or_else(|| {
            Error::Validation("tick must be an exact signed 64-bit integer".into())
        })?;
        let order = match map.get("order") {
            None => 0,
            Some(value) => value
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| {
                    Error::Validation("order must be an exact signed 32-bit integer".into())
                })?,
        };
        Self::new(timeline, tick, order)
    }

    pub fn not_after(&self, other: &Self) -> bool {
        self.timeline == other.timeline && (self.tick, self.order) <= (other.tick, other.order)
    }
}

impl Serialize for StoryTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut item = serializer.serialize_struct("StoryTime", 3)?;
        item.serialize_field("timeline", &self.timeline)?;
        item.serialize_field("tick", &self.tick.to_string())?;
        item.serialize_field("order", &self.order.to_string())?;
        item.end()
    }
}

#[derive(Clone, Debug)]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub source_path: String,
    pub frontmatter: Value,
    pub body: String,
    pub raw_bytes: Vec<u8>,
    pub blob_oid: Option<String>,
}

impl Record {
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.frontmatter.get(field)
    }
    pub fn string(&self, field: &str) -> Option<&str> {
        self.get(field).and_then(Value::as_str)
    }
    pub fn list(&self, field: &str) -> Option<&Vec<Value>> {
        self.get(field).and_then(Value::as_sequence)
    }
    pub fn status(&self) -> &str {
        self.string("status").unwrap_or("canonical")
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub root: PathBuf,
    pub records: BTreeMap<String, Record>,
    pub fingerprint: String,
    pub revision: Option<String>,
}

impl World {
    pub fn world_record(&self) -> Result<&Record> {
        let mut worlds = self
            .records
            .values()
            .filter(|record| record.kind == "world");
        match (worlds.next(), worlds.next()) {
            (Some(world), None) => Ok(world),
            _ => Err(Error::Validation(
                "world must contain exactly one world record".into(),
            )),
        }
    }

    pub fn default_timeline(&self) -> String {
        self.world_record()
            .ok()
            .and_then(|record| record.string("default_timeline"))
            .unwrap_or("main")
            .to_owned()
    }

    pub fn story_time(&self, tick: i64, timeline: Option<&str>, order: i32) -> Result<StoryTime> {
        let default = self.default_timeline();
        let point = StoryTime::new(timeline.unwrap_or(&default), tick, order)?;
        let declared = self
            .world_record()?
            .list("timelines")
            .is_some_and(|values| {
                values.iter().any(|value| {
                    value.get("id").and_then(Value::as_str) == Some(point.timeline.as_str())
                })
            });
        if !declared {
            return Err(Error::InvalidInput(format!(
                "unknown timeline {:?}",
                point.timeline
            )));
        }
        Ok(point)
    }

    pub fn current_time(&self) -> Result<StoryTime> {
        let world = self.world_record()?;
        let default = self.default_timeline();
        if let Some(value) = world.get("current_time") {
            let time = StoryTime::from_yaml(value, &default)?;
            return self.story_time(time.tick, Some(&time.timeline), time.order);
        }
        let active: Vec<_> = self
            .records
            .values()
            .filter(|record| record.kind == "scene" && record.status() == "active")
            .collect();
        if active.len() > 1 {
            return Err(Error::Validation(
                "multiple active scenes require world.current_time".into(),
            ));
        }
        if let Some(scene) = active.first() {
            let value = scene
                .get("time")
                .and_then(|value| value.get("current"))
                .ok_or_else(|| Error::Validation("an active scene requires time.current".into()))?;
            let time = StoryTime::from_yaml(value, &default)?;
            return self.story_time(time.tick, Some(&time.timeline), time.order);
        }
        self.story_time(0, None, 0)
    }

    pub fn resolve(&self, reference: &str) -> Result<&Record> {
        if let Some(record) = self.records.get(reference) {
            return Ok(record);
        }
        let folded = reference.to_lowercase();
        let matches: Vec<_> = self
            .records
            .values()
            .filter(|record| {
                record.title.to_lowercase() == folded
                    || record.list("aliases").is_some_and(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|alias| alias.to_lowercase() == folded)
                    })
            })
            .collect();
        match matches.as_slice() {
            [record] => Ok(record),
            [] => Err(Error::NotFound(format!("unknown record {reference:?}"))),
            _ => Err(Error::Ambiguous(format!(
                "ambiguous record {reference:?}; use an ID: {}",
                matches
                    .iter()
                    .map(|record| record.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}
