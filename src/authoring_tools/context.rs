//! Full-author choices from the already admitted immutable source image.
//! Editable declarations remain YAML text; a browser must not collapse their
//! integer/string alternatives or invent an absent declaration as an empty one.
use super::{Family, MAX_RESPONSE_BYTES, invalid};
use crate::{
    Result, briefs,
    model::{Record, World},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

const DECLARATIONS: &[&str] = &[
    "profiles",
    "predicates",
    "state_profile_bindings",
    "state_keys",
    "relationship_metrics",
];

struct Budget {
    limit: usize,
    items: usize,
    text_bytes: usize,
}
impl Budget {
    fn row(&mut self, text: &[&str]) -> Result<()> {
        if self.items == self.limit {
            return Err(invalid(format!(
                "authoring context exceeds {} complete selection rows; refusing an incomplete result",
                self.limit
            )));
        }
        self.items += 1;
        self.text(text)
    }
    fn text(&mut self, values: &[&str]) -> Result<()> {
        for value in values {
            if value.len() > MAX_RESPONSE_BYTES.saturating_sub(self.text_bytes) {
                return Err(invalid("complete authoring context text exceeds 1 MiB"));
            }
            self.text_bytes += value.len();
        }
        Ok(())
    }
    fn yaml(&mut self, value: &Yaml) -> Result<String> {
        let mut writer = YamlText {
            bytes: Vec::new(),
            maximum: MAX_RESPONSE_BYTES.saturating_sub(self.text_bytes),
            exceeded: false,
        };
        let encoded = serde_yaml::to_writer(&mut writer, value);
        if writer.exceeded {
            return Err(invalid("complete authoring context text exceeds 1 MiB"));
        }
        encoded?;
        self.text_bytes += writer.bytes.len();
        String::from_utf8(writer.bytes).map_err(|_| invalid("invalid internal declaration UTF-8"))
    }
}

struct YamlText {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl Write for YamlText {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "complete authoring context exceeds 1 MiB",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn text<'a>(value: Option<&'a Yaml>, field: &str) -> Result<&'a str> {
    value
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid(format!("authoring context cannot represent {field}")))
}

fn sorted_mapping<'a>(value: Option<&'a Yaml>, field: &str) -> Result<BTreeMap<&'a str, &'a Yaml>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let fields = value
        .as_mapping()
        .ok_or_else(|| invalid(format!("authoring context cannot represent {field}")))?;
    fields
        .iter()
        .map(|(key, value)| Ok((text(Some(key), field)?, value)))
        .collect()
}

fn named_declarations(owner: &Record, field: &str, budget: &mut Budget) -> Result<Vec<Json>> {
    let mut rows = Vec::new();
    let Some(value) = owner.get(field) else {
        return Ok(rows);
    };
    let values = value
        .as_sequence()
        .ok_or_else(|| invalid(format!("authoring context cannot represent {field}")))?;
    for (index, value) in values.iter().enumerate() {
        let id = text(value.get("id"), field)?;
        let source_field = format!("{field}[{index}]");
        budget.row(&[id, &source_field])?;
        let mut row =
            json!({"id":id,"sourceField":source_field,"definitionYaml":budget.yaml(value)?});
        if field == "profiles" {
            let label = text(value.get("label"), "profile label")?;
            budget.text(&[label])?;
            row["label"] = json!(label);
        }
        rows.push(row);
    }
    Ok(rows)
}

fn member_kind(kind: &str, field: &str) -> Result<&'static str> {
    if kind == "transition" {
        return Ok("transition");
    }
    for (prefix, name) in [
        ("descriptions[", "description"),
        ("chronology.calendars[", "calendar"),
        ("chronology.eras[", "era"),
        ("chronology.anchors[", "anchor"),
    ] {
        if field.starts_with(prefix) {
            return Ok(name);
        }
    }
    Err(invalid(
        "authoring context encountered an unsupported member namespace",
    ))
}

pub(super) fn build(world: &World, limit: usize) -> Result<Json> {
    let owner = world.world_record()?;
    let mut budget = Budget {
        limit,
        items: 0,
        text_bytes: 0,
    };
    let mut families = Vec::new();
    for family in Family::ALL {
        budget.row(&[family.name()])?;
        families.push(json!({
            "family":family.name(), "available":family.available(world),
            "requiredCapabilities":family.required_capabilities(),
            "mode":if family == Family::Generational { "create" } else { "update" },
        }));
    }

    let mut timelines = Vec::new();
    for (index, value) in owner
        .list("timelines")
        .ok_or_else(|| invalid("authoring context requires timelines"))?
        .iter()
        .enumerate()
    {
        let id = text(value.get("id"), "timeline id")?;
        let label = text(value.get("label"), "timeline label")?;
        let field = format!("timelines[{index}]");
        budget.row(&[id, label, &field])?;
        timelines.push(
            json!({"id":id,"label":label,"sourceField":field,"definitionYaml":budget.yaml(value)?}),
        );
    }

    let mut records = Vec::new();
    let mut members = Vec::new();
    for record in world.records.values() {
        let scaffold_families = Family::ALL
            .into_iter()
            .filter(|family| family.available(world) && family.supports_kind(&record.kind))
            .map(Family::name)
            .collect::<Vec<_>>();
        budget.row(&[
            &record.id,
            &record.title,
            &record.kind,
            record.status(),
            &record.source_path,
        ])?;
        budget.text(&scaffold_families)?;
        records.push(json!({
            "target":{"kind":"record","id":record.id}, "label":record.title,
            "kind":record.kind, "status":record.status(), "sourcePath":record.source_path,
            "scaffoldFamilies":scaffold_families,
        }));
        if record.kind == "hypothesis" {
            continue;
        }
        for kind in ["section", "transition"] {
            let mut seen = BTreeSet::new();
            for (id, value, field) in briefs::members(record, kind) {
                // Reference identity is owner + kind + ID. Different fields in
                // one owner cannot advertise the same ambiguous reference.
                if !seen.insert(id) {
                    return Err(invalid(format!(
                        "authoring context requires unique {kind} member {id:?} in {}",
                        record.id
                    )));
                }
                let member_kind = member_kind(kind, &field)?;
                let label = value
                    .get("heading")
                    .or_else(|| value.get("label"))
                    .and_then(Yaml::as_str)
                    .unwrap_or(id);
                budget.row(&[
                    &record.id,
                    id,
                    &record.title,
                    label,
                    " — ",
                    &record.kind,
                    &field,
                ])?;
                members.push(json!({
                    "target":{"kind":kind,"owner_id":record.id,"id":id},
                    "label":format!("{} — {label}",record.title),
                    "memberKind":member_kind,"sourceField":field,"ownerKind":record.kind,
                }));
            }
        }
    }

    let mut declarations = serde_json::Map::new();
    for field in DECLARATIONS {
        declarations.insert(
            (*field).into(),
            match owner.get(field) {
                Some(value) => json!({"present":true,"yaml":budget.yaml(value)?}),
                None => json!({"present":false,"yaml":null}),
            },
        );
    }
    let profiles = named_declarations(owner, "profiles", &mut budget)?;
    let predicates = named_declarations(owner, "predicates", &mut budget)?;

    let mut bindings = Vec::new();
    for (kind, fields) in sorted_mapping(
        owner.get("state_profile_bindings"),
        "state_profile_bindings",
    )? {
        for (key, value) in sorted_mapping(Some(fields), "bound state keys")? {
            let profile = text(Some(value), "bound profile id")?;
            budget.row(&[kind, key, profile])?;
            bindings.push(json!({"entityKind":kind,"key":key,"profileId":profile}));
        }
    }
    let mut state_keys = Vec::new();
    for (kind, fields) in sorted_mapping(owner.get("state_keys"), "state_keys")? {
        for (key, definition) in sorted_mapping(Some(fields), "state key definitions")? {
            let value_type = text(definition.get("type"), "state value type")?;
            budget.row(&[kind, key, value_type])?;
            state_keys.push(json!({
                "entityKind":kind,"key":key,"valueType":value_type,
                "definitionYaml":budget.yaml(definition)?,
            }));
        }
    }
    let mut metrics = Vec::new();
    for (name, value) in sorted_mapping(owner.get("relationship_metrics"), "relationship_metrics")?
    {
        budget.row(&[name])?;
        metrics.push(json!({"name":name,"definitionYaml":budget.yaml(value)?}));
    }
    let capabilities = owner
        .list("capabilities")
        .ok_or_else(|| invalid("authoring context requires capabilities"))?
        .iter()
        .map(|value| text(Some(value), "capability"))
        .collect::<Result<Vec<_>>>()?;
    budget.text(&capabilities)?;
    let default_timeline = world.default_timeline();
    let current_time = world.current_time()?;
    budget.text(&[&owner.id, &default_timeline, &current_time.timeline])?;
    Ok(json!({
        "perspective":"author","scope":"author-captured-source",
        "worldId":owner.id,"defaultTimeline":default_timeline,"currentTime":current_time,
        "capabilities":capabilities,"families":families,"timelines":timelines,
        "records":records,"members":members,"declarations":declarations,
        "profiles":profiles,"predicates":predicates,"stateProfileBindings":bindings,
        "stateKeys":state_keys,"relationshipMetrics":metrics,
        "selectionCount":budget.items,"maxItems":limit,
    }))
}
