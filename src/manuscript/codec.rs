//! Bound expanded source before building a YAML value, and bound complete JSON
//! reports before transport. Finite YAML aliases are a native source adaptation;
//! every expanded node and scalar is charged, including repeated aliases.
use super::*;
use serde::{
    Serialize,
    de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::{collections::BTreeSet, fmt, io::Write};

struct Budget {
    nodes: usize,
    scalar_bytes: usize,
    maximum: usize,
}
struct Scan<'a> {
    budget: &'a mut Budget,
    depth: usize,
    key: bool,
}
impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = Option<String>;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        self.budget.nodes += 1;
        if self.depth > 16 || self.budget.nodes > 16_384 {
            return Err(serde::de::Error::custom(
                "manuscript expanded source structural bound exceeded",
            ));
        }
        decoder.deserialize_any(self)
    }
}
impl Scan<'_> {
    fn scalar<E: serde::de::Error>(&self) -> std::result::Result<Option<String>, E> {
        if self.key {
            return Err(E::custom("manuscript mapping keys must be strings"));
        }
        Ok(None)
    }
}
impl<'de> Visitor<'de> for Scan<'_> {
    type Value = Option<String>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded untagged manuscript source")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_i128<E: serde::de::Error>(self, _: i128) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_u128<E: serde::de::Error>(self, _: u128) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<Self::Value, E> {
        if !value.is_finite() {
            return Err(E::custom("manuscript numbers must be finite"));
        }
        self.scalar()
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
        self.scalar()
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
        self.budget.scalar_bytes = self.budget.scalar_bytes.saturating_add(value.len());
        if value.len() > MAX_BODY_BYTES || self.budget.scalar_bytes > self.budget.maximum {
            return Err(E::custom(
                "manuscript expanded source scalar byte bound exceeded",
            ));
        }
        Ok(self.key.then(|| value.to_owned()))
    }
    fn visit_string<E: serde::de::Error>(
        self,
        value: String,
    ) -> std::result::Result<Self::Value, E> {
        self.visit_str(&value)
    }
    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut values: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        self.scalar::<A::Error>()?;
        let mut count = 0;
        while values
            .next_element_seed(Scan {
                budget: &mut *self.budget,
                depth: self.depth + 1,
                key: false,
            })?
            .is_some()
        {
            count += 1;
            if count > 1024 {
                return Err(serde::de::Error::custom(
                    "manuscript arrays exceed 1024 entries",
                ));
            }
        }
        Ok(None)
    }
    fn visit_map<A: MapAccess<'de>>(
        self,
        mut values: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        self.scalar::<A::Error>()?;
        let mut keys = BTreeSet::new();
        while let Some(key) = values.next_key_seed(Scan {
            budget: &mut *self.budget,
            depth: self.depth + 1,
            key: true,
        })? {
            let key = key.expect("string key scanner");
            if keys.len() >= 256 || !keys.insert(key) {
                return Err(serde::de::Error::custom(
                    "manuscript mapping has duplicate keys or exceeds 256 entries",
                ));
            }
            values.next_value_seed(Scan {
                budget: &mut *self.budget,
                depth: self.depth + 1,
                key: false,
            })?;
        }
        Ok(None)
    }
    // Tags arrive as enums; no enum visitor is admitted.
}

pub(super) fn yaml(raw: &str) -> Result<Yaml> {
    let mut documents = serde_yaml::Deserializer::from_str(raw);
    let decoder = documents
        .next()
        .ok_or_else(|| invalid("manuscript source has no YAML document"))?;
    Scan {
        budget: &mut Budget {
            nodes: 0,
            scalar_bytes: 0,
            maximum: MAX_DOCUMENT_BYTES,
        },
        depth: 1,
        key: false,
    }
    .deserialize(decoder)
    .map_err(|error| invalid(format!("invalid bounded manuscript YAML: {error}")))?;
    if documents.next().is_some() {
        return Err(invalid("manuscript source requires one YAML document"));
    }
    serde_yaml::from_str(raw).map_err(|error| invalid(format!("invalid manuscript YAML: {error}")))
}

struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "complete manuscript JSON exceeds its byte bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn json_bytes(value: &impl Serialize, maximum: usize) -> Result<Vec<u8>> {
    let mut writer = Writer {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| invalid("complete manuscript JSON exceeds its byte bound"))?;
    Ok(writer.bytes)
}

struct JsonScan<'a> {
    budget: &'a mut Budget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for JsonScan<'_> {
    type Value = Json;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Json, D::Error> {
        self.budget.nodes += 1;
        if self.depth > 16 || self.budget.nodes > 16_384 {
            return Err(serde::de::Error::custom(
                "manuscript JSON structural bound exceeded",
            ));
        }
        decoder.deserialize_any(self)
    }
}
impl JsonScan<'_> {
    fn text<E: serde::de::Error>(&mut self, value: &str) -> std::result::Result<(), E> {
        self.budget.scalar_bytes = self.budget.scalar_bytes.saturating_add(value.len());
        if self.budget.scalar_bytes > self.budget.maximum {
            return Err(E::custom("manuscript JSON scalar byte bound exceeded"));
        }
        Ok(())
    }
}
impl<'de> Visitor<'de> for JsonScan<'_> {
    type Value = Json;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded duplicate-free manuscript JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> std::result::Result<Json, E> {
        Ok(value.into())
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Json, E> {
        Ok(value.into())
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Json, E> {
        Ok(value.into())
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<Json, E> {
        serde_json::Number::from_f64(value)
            .map(Json::Number)
            .ok_or_else(|| E::custom("manuscript JSON numbers must be finite"))
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Json, E> {
        Ok(Json::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Json, E> {
        Ok(Json::Null)
    }
    fn visit_str<E: serde::de::Error>(mut self, value: &str) -> std::result::Result<Json, E> {
        self.text(value)?;
        Ok(value.into())
    }
    fn visit_string<E: serde::de::Error>(mut self, value: String) -> std::result::Result<Json, E> {
        self.text(&value)?;
        Ok(value.into())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<Json, A::Error> {
        let mut result = Vec::new();
        while let Some(value) = values.next_element_seed(JsonScan {
            budget: &mut *self.budget,
            depth: self.depth + 1,
        })? {
            if result.len() >= 1024 {
                return Err(serde::de::Error::custom(
                    "manuscript JSON array bound exceeded",
                ));
            }
            result.push(value);
        }
        Ok(Json::Array(result))
    }
    fn visit_map<A: MapAccess<'de>>(
        mut self,
        mut values: A,
    ) -> std::result::Result<Json, A::Error> {
        let mut result = serde_json::Map::new();
        while let Some(key) = values.next_key::<String>()? {
            self.text::<A::Error>(&key)?;
            self.budget.nodes += 1;
            if result.len() >= 256 || result.contains_key(&key) {
                return Err(serde::de::Error::custom(
                    "duplicate manuscript JSON field or mapping bound exceeded",
                ));
            }
            let value = values.next_value_seed(JsonScan {
                budget: &mut *self.budget,
                depth: self.depth + 1,
            })?;
            result.insert(key, value);
        }
        Ok(Json::Object(result))
    }
}
pub(super) fn decode_json<T: DeserializeOwned>(raw: &str, maximum: usize) -> Result<T> {
    if raw.len() > maximum {
        return Err(invalid(
            "manuscript request exceeds its complete JSON byte bound",
        ));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let value = JsonScan {
        budget: &mut Budget {
            nodes: 0,
            scalar_bytes: 0,
            maximum,
        },
        depth: 1,
    }
    .deserialize(&mut decoder)
    .map_err(|error| invalid(format!("invalid bounded manuscript request: {error}")))?;
    decoder
        .end()
        .map_err(|_| invalid("manuscript JSON request has trailing input"))?;
    serde_json::from_value(value)
        .map_err(|error| invalid(format!("invalid closed manuscript request: {error}")))
}
