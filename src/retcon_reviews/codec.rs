//! Closed, duplicate-safe JSON admission with budgets charged before nodes are
//! allocated. Original source values are retained as canonical valueRaw strings.
use super::*;
use serde::de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt, io::Write};

struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("retcon JSON byte bound exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn bytes(value: &impl Serialize, maximum: usize) -> Result<Vec<u8>> {
    let mut writer = Writer {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        invalid(format!(
            "complete retcon review JSON exceeds {maximum} bytes or is invalid"
        ))
    })?;
    Ok(writer.bytes)
}
struct Budget {
    nodes: usize,
    scalars: usize,
    maximum: usize,
}
struct Scan<'a> {
    budget: &'a mut Budget,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = Json;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Json, D::Error> {
        self.budget.nodes += 1;
        if self.depth > 64 || self.budget.nodes > 131_072 {
            return Err(serde::de::Error::custom(
                "retcon JSON structural bound exceeded",
            ));
        }
        decoder.deserialize_any(self)
    }
}
impl Scan<'_> {
    fn string<E: serde::de::Error>(&mut self, value: &str) -> std::result::Result<(), E> {
        self.budget.scalars = self.budget.scalars.saturating_add(value.len());
        if self.budget.scalars > self.budget.maximum {
            return Err(E::custom("retcon JSON scalar bound exceeded"));
        }
        Ok(())
    }
}
impl<'de> Visitor<'de> for Scan<'_> {
    type Value = Json;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded duplicate-free retcon JSON")
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
            .ok_or_else(|| E::custom("non-finite retcon JSON"))
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Json, E> {
        Ok(Json::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Json, E> {
        Ok(Json::Null)
    }
    fn visit_str<E: serde::de::Error>(mut self, value: &str) -> std::result::Result<Json, E> {
        self.string(value)?;
        Ok(value.into())
    }
    fn visit_string<E: serde::de::Error>(mut self, value: String) -> std::result::Result<Json, E> {
        self.string(&value)?;
        Ok(value.into())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<Json, A::Error> {
        let mut result = Vec::new();
        while let Some(value) = values.next_element_seed(Scan {
            budget: &mut *self.budget,
            depth: self.depth + 1,
        })? {
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
            self.string::<A::Error>(&key)?;
            if result.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate retcon JSON field"));
            }
            let value = values.next_value_seed(Scan {
                budget: &mut *self.budget,
                depth: self.depth + 1,
            })?;
            result.insert(key, value);
        }
        Ok(Json::Object(result))
    }
}
fn value(raw: &str, maximum: usize) -> Result<Json> {
    if raw.len() > maximum {
        return Err(invalid("retcon JSON exceeds its complete byte bound"));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let result = Scan {
        budget: &mut Budget {
            nodes: 0,
            scalars: 0,
            maximum,
        },
        depth: 0,
    }
    .deserialize(&mut decoder)
    .map_err(|_| invalid("retcon JSON is invalid, duplicate or exceeds structural bounds"))?;
    decoder
        .end()
        .map_err(|_| invalid("retcon JSON has trailing input"))?;
    Ok(result)
}
pub(super) fn decode<T: DeserializeOwned>(raw: &str, maximum: usize) -> Result<T> {
    serde_json::from_value(value(raw, maximum)?)
        .map_err(|error| invalid(format!("invalid closed retcon review fields: {error}")))
}
fn keys(value: &Json, fields: &[&str]) -> Result<()> {
    let object = value.as_object().ok_or_else(binding)?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(binding());
    }
    Ok(())
}
fn array<'a>(value: &'a Json, maximum: usize) -> Result<&'a [Json]> {
    value
        .as_array()
        .filter(|items| items.len() <= maximum)
        .map(Vec::as_slice)
        .ok_or_else(binding)
}
fn text(value: &Json, maximum: usize, blank: bool) -> Result<&str> {
    value
        .as_str()
        .filter(|text| text.len() <= maximum && (blank || !text.trim().is_empty()))
        .ok_or_else(binding)
}
fn optional_text(value: &Json, maximum: usize) -> Result<()> {
    if !value.is_null() {
        text(value, maximum, false)?;
    }
    Ok(())
}
fn ordinal(value: &Json, nullable: bool) -> Result<()> {
    if nullable && value.is_null() {
        return Ok(());
    }
    if value.as_u64().is_none_or(|number| number > 1_000_000) {
        return Err(binding());
    }
    Ok(())
}
fn reference(value: &Json) -> Result<retcon_impact::Reference> {
    let reference: retcon_impact::Reference =
        serde_json::from_value(value.clone()).map_err(|_| binding())?;
    let request = retcon_impact::Request {
        target: reference.clone(),
        at: crate::consequence_semantics::Time {
            timeline: "main".into(),
            tick: "0".into(),
            order: "0".into(),
        },
        depth: 1,
        max_items: 1,
    };
    retcon_impact::validate_request(&request)?;
    Ok(reference)
}
fn time(value: &Json, nullable: bool) -> Result<()> {
    if nullable && value.is_null() {
        return Ok(());
    }
    let time: crate::consequence_semantics::Time =
        serde_json::from_value(value.clone()).map_err(|_| binding())?;
    time.point()?;
    Ok(())
}
fn safe_path(value: &Json, nullable: bool) -> Result<()> {
    if nullable && value.is_null() {
        return Ok(());
    }
    let value = text(value, 4096, false)?;
    if value.starts_with('/')
        || value.contains(['\\', '\0'])
        || value.split('/').count() > 64
        || value.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(binding());
    }
    Ok(())
}
fn raw(value_: &Json) -> Result<()> {
    let raw = text(value_, retcon_impact::MAX_WITNESS_BYTES, true)?;
    // Keep exact scalar tokens opaque. Original reconstruction proves the
    // canonical generated string; a second float parse/format is not an
    // authority for source numeric identity.
    value(raw, retcon_impact::MAX_WITNESS_BYTES)?;
    Ok(())
}
fn same_request(value: &Json, analysis: &Analysis) -> Result<()> {
    if value["target"] != serde_json::to_value(&analysis.impact_request.target)?
        || value["at"] != serde_json::to_value(&analysis.impact_request.at)?
        || value["depth"] != analysis.impact_request.depth
        || value["maxItems"] != analysis.impact_request.max_items
    {
        return Err(binding());
    }
    Ok(())
}
pub(super) fn report(report: &Json, analysis: &Analysis) -> Result<()> {
    bytes(report, retcon_impact::MAX_REPORT_BYTES)?;
    keys(
        report,
        &[
            "protocol",
            "outcome",
            "target",
            "at",
            "depth",
            "maxItems",
            "base",
            "candidate",
            "items",
            "mentions",
            "semanticSuggestions",
            "canApply",
            "writesPerformed",
            "scope",
            "notice",
        ],
    )?;
    same_request(report, analysis)?;
    if report["protocol"] != crate::CONTRACT
        || report["outcome"] != "ok"
        || report["canApply"] != false
        || report["writesPerformed"] != false
    {
        return Err(binding());
    }
    keys(&report["base"], &["revision"])?;
    keys(&report["candidate"], &["baseRevision", "planSha256"])?;
    if report["base"]["revision"] != analysis.base_revision
        || report["candidate"]["baseRevision"] != analysis.base_revision
        || report["candidate"]["planSha256"] != analysis.plan_sha256
    {
        return Err(binding());
    }
    if !array(&report["mentions"], 0)?.is_empty()
        || !array(&report["semanticSuggestions"], 0)?.is_empty()
    {
        return Err(binding());
    }
    keys(
        &report["scope"],
        &["audiences", "perspectives", "unsupported"],
    )?;
    for field in ["audiences", "perspectives", "unsupported"] {
        for item in array(&report["scope"][field], 32)? {
            text(item, 1024, false)?;
        }
    }
    text(&report["notice"], 4096, false)?;
    for item in array(&report["items"], analysis.impact_request.max_items)? {
        keys(
            item,
            &[
                "snapshot", "owner", "target", "relation", "distance", "citation",
            ],
        )?;
        let snapshot = text(&item["snapshot"], 16, false)?;
        if !["base", "candidate"].contains(&snapshot) {
            return Err(binding());
        }
        reference(&item["owner"])?;
        reference(&item["target"])?;
        text(&item["relation"], 256, false)?;
        if item["distance"]
            .as_u64()
            .is_none_or(|v| v == 0 || v > analysis.impact_request.depth as u64)
        {
            return Err(binding());
        }
        let citation = &item["citation"];
        keys(
            citation,
            &[
                "entityId",
                "sourcePath",
                "section",
                "sourceOrdinal",
                "memberId",
                "time",
                "provenance",
            ],
        )?;
        text(&citation["entityId"], 256, false)?;
        safe_path(&citation["sourcePath"], false)?;
        text(&citation["section"], 4096, false)?;
        ordinal(&citation["sourceOrdinal"], true)?;
        optional_text(&citation["memberId"], 256)?;
        time(&citation["time"], true)?;
        let provenance = &citation["provenance"];
        if snapshot == "base" || item["owner"]["kind"] == "author-record" {
            keys(provenance, &["kind", "revision", "blobOid"])?;
            if provenance["kind"] != "source" || provenance["revision"] != analysis.base_revision {
                return Err(binding());
            }
            check_revision(text(&provenance["blobOid"], 64, false)?)?;
        } else {
            keys(provenance, &["kind", "baseRevision", "planSha256"])?;
            if provenance["kind"] != "candidate"
                || provenance["baseRevision"] != analysis.base_revision
                || provenance["planSha256"] != analysis.plan_sha256
            {
                return Err(binding());
            }
        }
    }
    // Match the native consequence digest's typed field order, not a caller's
    // object key order. The complete reconstruction remains authoritative.
    let mut hash = Sha256::new();
    hash.update(b"wedl-consequence-plan\0");
    hash.update(bytes(&analysis.plan, consequences::MAX_PLAN_BYTES)?);
    if hex::encode(hash.finalize()) != analysis.plan_sha256 {
        return Err(binding());
    }
    Ok(())
}
pub(super) fn witness(witness: &Json, analysis: &Analysis) -> Result<()> {
    bytes(witness, retcon_impact::MAX_WITNESS_BYTES)?;
    text(&witness["version"], 256, false)?;
    if witness["version"] != retcon_impact::WITNESS_VERSION {
        return Ok(());
    }
    keys(
        witness,
        &[
            "version",
            "worldId",
            "target",
            "at",
            "depth",
            "maxItems",
            "seeds",
            "inputs",
            "edges",
            "consulted",
        ],
    )?;
    same_request(witness, analysis)?;
    if witness["worldId"] != analysis.world_id {
        return Err(binding());
    }
    let seeds = array(&witness["seeds"], retcon_impact::MAX_WITNESS_POSITIONS)?;
    let mut unique = BTreeSet::new();
    for seed in seeds {
        if !unique.insert(reference(seed)?) {
            return Err(binding());
        }
    }
    if !unique.contains(&analysis.impact_request.target) {
        return Err(binding());
    }
    let inputs = array(&witness["inputs"], retcon_impact::MAX_WITNESS_POSITIONS)?;
    let edges = array(&witness["edges"], retcon_impact::MAX_WITNESS_POSITIONS)?;
    let consulted = array(&witness["consulted"], retcon_impact::MAX_WITNESS_POSITIONS)?;
    let mut count = inputs
        .len()
        .saturating_add(edges.len())
        .saturating_add(consulted.len());
    let mut inputs_seen = BTreeSet::new();
    for input in inputs {
        keys(input, &["ref", "path", "present", "sections"])?;
        if !inputs_seen.insert(reference(&input["ref"])?) {
            return Err(binding());
        }
        safe_path(&input["path"], true)?;
        let present = input["present"].as_bool().ok_or_else(binding)?;
        let sections = array(&input["sections"], retcon_impact::MAX_WITNESS_POSITIONS)?;
        if present && input["path"].is_null() || !present && !sections.is_empty() {
            return Err(binding());
        }
        count = count.saturating_add(sections.len());
        for (index, section) in sections.iter().enumerate() {
            keys(section, &["field", "ordinal", "valueRaw"])?;
            text(&section["field"], 4096, false)?;
            if section["ordinal"].as_u64() != Some(index as u64) {
                return Err(binding());
            }
            raw(&section["valueRaw"])?;
        }
    }
    if unique.iter().any(|seed| !inputs_seen.contains(seed))
        || count > retcon_impact::MAX_WITNESS_POSITIONS
    {
        return Err(binding());
    }
    let mut edges_seen = BTreeSet::new();
    for edge in edges {
        keys(
            edge,
            &[
                "owner", "target", "relation", "field", "ordinal", "memberId", "time",
            ],
        )?;
        reference(&edge["owner"])?;
        reference(&edge["target"])?;
        text(&edge["relation"], 256, false)?;
        text(&edge["field"], 4096, false)?;
        ordinal(&edge["ordinal"], true)?;
        optional_text(&edge["memberId"], 256)?;
        time(&edge["time"], true)?;
        if !edges_seen.insert(bytes(edge, retcon_impact::MAX_WITNESS_BYTES)?) {
            return Err(binding());
        }
    }
    let mut consulted_seen = BTreeSet::new();
    for item in consulted {
        keys(item, &["path", "field", "valueRaw"])?;
        safe_path(&item["path"], false)?;
        text(&item["field"], 4096, false)?;
        raw(&item["valueRaw"])?;
        if !consulted_seen.insert((item["path"].as_str(), item["field"].as_str())) {
            return Err(binding());
        }
    }
    Ok(())
}
