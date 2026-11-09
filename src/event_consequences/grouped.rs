//! Source-section attribution is deliberately independent of value equality.
//! Only changed explicit contributions supply event owners; no final writer or
//! temporal proximity is treated as an explanation for an indirect difference.
use super::*;
use crate::revision_review::{self, Node};

const MAX_ALIGNMENT_WORK: usize = 2_000_000;

struct Section {
    value: Yaml,
    citations: Vec<Json>,
}
type Sections = BTreeMap<String, Section>;

fn section_name(field: &str) -> String {
    if [
        "effects",
        "transitions",
        "causes",
        "related_story_points",
        "outcome_events",
        "requires",
        "when",
    ]
    .contains(&field)
    {
        field.into()
    } else {
        format!("frontmatter.{field}")
    }
}

fn section(world: &World, record: &Record, name: &str, node: &Node) -> Result<Section> {
    let value = node.value();
    let mut citations = Vec::new();
    if let Node::List(members) = node {
        for (ordinal, member) in members {
            let member = member.value();
            let time = if matches!(name, "transitions" | "lifecycle.transitions")
                && member.get("time").is_some()
            {
                Some(point(world, &member["time"])?)
            } else if matches!(name, "effects" | "causes") && record.kind == "event" {
                Some(event_time(world, record)?)
            } else {
                None
            };
            citations.push(citation(
                world,
                record,
                name,
                Some(*ordinal),
                member
                    .get("id")
                    .or_else(|| member.get("transition_id"))
                    .and_then(Yaml::as_str),
                time.as_ref(),
            ));
        }
    }
    if citations.is_empty() {
        citations.push(citation(world, record, name, None, None, None));
    }
    Ok(Section { value, citations })
}

fn sections(world: &World, record: &Record, node: &Node) -> Result<Sections> {
    let Node::Map(fields) = node else {
        return Err(invalid("admitted source is not a mapping"));
    };
    let mut values = BTreeMap::new();
    for (field, node) in fields {
        if matches!(field.as_str(), "initial_state" | "lifecycle") {
            if let Node::Map(children) = node {
                for (key, node) in children {
                    let name = format!("{field}.{key}");
                    values.insert(name.clone(), section(world, record, &name, node)?);
                }
                continue;
            }
        }
        let name = section_name(field);
        values.insert(name.clone(), section(world, record, &name, node)?);
    }
    values.insert(
        "bodyMarkdown".into(),
        Section {
            value: Yaml::String(record.body.clone()),
            citations: vec![citation(world, record, "bodyMarkdown", None, None, None)],
        },
    );
    Ok(values)
}

fn literal(value: &Yaml) -> Result<String> {
    // JSON's ordered map gives literal typed comparison independent of YAML
    // mapping insertion order, while retaining integer/float and string kinds.
    Ok(serde_json::to_string(&serde_json::to_value(value)?)?)
}

/// Stable owner-local IDs align inserted history without treating every shifted
/// ordinal as a change. An actual move remains a changed contribution. Full
/// alignment work is bounded before allocating a matrix; no prefix is returned.
fn changed<'a>(old: &'a [Yaml], new: &'a [Yaml], work: &mut usize) -> Result<Vec<&'a Yaml>> {
    let cells = old
        .len()
        .saturating_add(1)
        .saturating_mul(new.len().saturating_add(1));
    *work = work.saturating_add(cells);
    if *work > MAX_ALIGNMENT_WORK {
        return Err(invalid(
            "complete consequence alignment exceeds 2000000 work units",
        ));
    }
    let tokens = |values: &[Yaml]| -> Result<Vec<(String, String)>> {
        values
            .iter()
            .map(|value| {
                let literal = literal(value)?;
                let key = value
                    .get("id")
                    .and_then(Yaml::as_str)
                    .map(|id| format!("id:{id}"))
                    .unwrap_or_else(|| format!("value:{literal}"));
                Ok((key, literal))
            })
            .collect()
    };
    let before = tokens(old)?;
    let after = tokens(new)?;
    let width = new.len() + 1;
    let mut table = vec![0u32; cells];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i * width + j] = if before[i].0 == after[j].0 {
                1 + table[(i + 1) * width + j + 1]
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut values = Vec::new();
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && before[i].0 == after[j].0 {
            if before[i].1 != after[j].1 {
                values.push(&old[i]);
                values.push(&new[j]);
            }
            i += 1;
            j += 1;
        } else if i < old.len()
            && (j == new.len() || table[(i + 1) * width + j] >= table[i * width + j + 1])
        {
            values.push(&old[i]);
            i += 1;
        } else {
            values.push(&new[j]);
            j += 1;
        }
    }
    Ok(values)
}

fn list(value: Option<&Section>) -> &[Yaml] {
    value
        .and_then(|section| section.value.as_sequence())
        .map_or(&[], Vec::as_slice)
}

fn owners(
    record: &Record,
    name: &str,
    before: Option<&Section>,
    after: Option<&Section>,
    focus: &mut BTreeSet<String>,
    work: &mut usize,
) -> Result<BTreeSet<String>> {
    let mut owners = BTreeSet::new();
    if record.kind == "event"
        && matches!(
            name,
            "effects" | "frontmatter.time" | "related_story_points"
        )
    {
        owners.insert(record.id.clone());
    } else if matches!(name, "transitions" | "lifecycle.transitions")
        && matches!(
            record.kind.as_str(),
            "knowledge" | "relationship" | "story-point"
        )
    {
        let mut uncaused = false;
        for member in changed(list(before), list(after), work)? {
            if let Some(event) = member.get("causing_event").and_then(Yaml::as_str) {
                owners.insert(event.to_owned());
                focus.insert(event.to_owned());
            } else {
                uncaused = true;
            }
        }
        if uncaused || owners.len() != 1 {
            owners.clear();
        }
    } else if name == "outcome_events" {
        let before: BTreeSet<_> = list(before).iter().filter_map(Yaml::as_str).collect();
        let after: BTreeSet<_> = list(after).iter().filter_map(Yaml::as_str).collect();
        owners.extend(
            before
                .symmetric_difference(&after)
                .map(|id| (*id).to_owned()),
        );
        if owners.len() != 1 {
            owners.clear();
        }
    }
    focus.extend(owners.iter().cloned());
    Ok(owners)
}

fn observed(section: Option<&Section>) -> Result<Json> {
    Ok(match section {
        Some(section) => {
            json!({"presence":"present","value":service::yaml_json(&section.value)?,"valueRaw":literal(&section.value)?})
        }
        None => json!({"presence":"absent"}),
    })
}

struct RecordChange {
    row: Json,
    sections: Vec<(Json, BTreeSet<String>, Vec<Json>)>,
}

fn records(
    base: &World,
    candidate: &World,
    before: &BTreeMap<String, Node>,
    after: &BTreeMap<String, Node>,
    operations: &Json,
    focus: &mut BTreeSet<String>,
    work: &mut usize,
) -> Result<Vec<RecordChange>> {
    let mut targets = BTreeMap::<String, Vec<usize>>::new();
    for operation in operations
        .as_array()
        .ok_or_else(|| invalid("prepared operation identities are unavailable"))?
    {
        let id = operation["entityId"]
            .as_str()
            .ok_or_else(|| invalid("prepared operation target is unavailable"))?;
        let index = operation["index"]
            .as_u64()
            .ok_or_else(|| invalid("prepared operation index is unavailable"))?
            as usize;
        targets.entry(id.to_owned()).or_default().push(index);
    }
    let mut rows = Vec::new();
    for (id, indexes) in targets {
        // An excluded extant owner is neither a creation nor a deletion at H.
        if base.records.contains_key(&id) && !before.contains_key(&id)
            || candidate.records.contains_key(&id) && !after.contains_key(&id)
        {
            continue;
        }
        let left = before
            .get(&id)
            .map(|node| sections(base, &base.records[&id], node))
            .transpose()?
            .unwrap_or_default();
        let right = after
            .get(&id)
            .map(|node| sections(candidate, &candidate.records[&id], node))
            .transpose()?
            .unwrap_or_default();
        let Some(record) = candidate.records.get(&id).or_else(|| base.records.get(&id)) else {
            continue;
        };
        let names: BTreeSet<_> = left.keys().chain(right.keys()).cloned().collect();
        let mut changes = Vec::new();
        for name in names {
            let old = left.get(&name);
            let new = right.get(&name);
            if let Some((left, right)) = old.zip(new)
                && literal(&left.value)? == literal(&right.value)?
            {
                continue;
            }
            let owners = owners(record, &name, old, new, focus, work)?;
            let citations = old
                .into_iter()
                .chain(new)
                .flat_map(|section| section.citations.iter().cloned())
                .collect();
            changes.push((
                json!({"section":name,"before":observed(old)?,"after":observed(new)?}),
                owners,
                citations,
            ));
        }
        changes.sort_by_key(|(value, _, _)| {
            (
                value["section"] == "bodyMarkdown",
                value["section"].as_str().unwrap().to_owned(),
            )
        });
        let status = if !base.records.contains_key(&id) {
            "created"
        } else if !candidate.records.contains_key(&id) {
            "deleted"
        } else if changes.is_empty() {
            "noop"
        } else {
            "updated"
        };
        if record.kind == "event" && status != "noop" {
            focus.insert(id.clone());
        }
        let mut citations = Vec::new();
        if changes.is_empty() {
            for world in [base, candidate] {
                if let Some(record) = world.records.get(&id) {
                    citations.push(citation(
                        world,
                        record,
                        "frontmatter.title",
                        None,
                        None,
                        None,
                    ));
                }
            }
        }
        rows.push(RecordChange {
            row: json!({"recordId":id,"kind":record.kind,"change":status,
            "before":before.contains_key(&id).then(||reference(&base.records[&id])),
            "after":after.contains_key(&id).then(||reference(&candidate.records[&id])),
            "operationIndexes":indexes,"sections":[],"citations":citations}),
            sections: changes,
        });
    }
    rows.sort_by_cached_key(|value| {
        (
            value.row["kind"].as_str().unwrap().to_owned(),
            value.row["recordId"].as_str().unwrap().to_owned(),
        )
    });
    Ok(rows)
}

fn contributions(world: &World, subject: &Json, at: &StoryTime) -> Result<Vec<Yaml>> {
    let mut values = Vec::new();
    if subject["kind"] == "state" {
        let id = subject["entityId"]
            .as_str()
            .ok_or_else(|| invalid("state subject has no exact owner"))?;
        let key = subject["key"]
            .as_str()
            .ok_or_else(|| invalid("state subject has no exact key"))?;
        let mut events = Vec::new();
        for record in world
            .records
            .values()
            .filter(|record| record.kind == "event")
        {
            if projection::admitted(world, record, at)? {
                let point = event_time(world, record)?;
                events.push(((point.tick, point.order, record.id.clone()), record));
            }
        }
        events.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (_, event) in events {
            for effect in event.list("effects").into_iter().flatten() {
                if effect["target"].as_str() == Some(id) && effect["key"].as_str() == Some(key) {
                    let mut value = serde_yaml::Mapping::new();
                    value.insert("event".into(), Yaml::String(event.id.clone()));
                    value.insert(
                        "time".into(),
                        event.get("time").expect("validated event time").clone(),
                    );
                    value.insert("effect".into(), effect.clone());
                    values.push(Yaml::Mapping(value));
                }
            }
        }
    } else {
        let key = match subject["kind"].as_str() {
            Some("knowledge") => "knowledgeId",
            Some("relationship") => "relationshipId",
            Some("story-point") => "storyPointId",
            _ => return Ok(values),
        };
        let Some(record) = subject[key].as_str().and_then(|id| world.records.get(id)) else {
            return Ok(values);
        };
        if !projection::admitted(world, record, at)? {
            return Ok(values);
        }
        if let Some((_, history)) = members(record) {
            for member in history {
                if point(world, &member["time"])?.not_after(at) {
                    values.push(member.clone());
                }
            }
        }
    }
    Ok(values)
}

fn change_owners(
    base: &World,
    candidate: &World,
    change: &Json,
    at: &StoryTime,
    work: &mut usize,
) -> Result<BTreeSet<String>> {
    let subject = &change["subject"];
    if subject["kind"] == "outcome" {
        return Ok(subject["eventId"]
            .as_str()
            .into_iter()
            .map(str::to_owned)
            .collect());
    }
    let before = contributions(base, subject, at)?;
    let after = contributions(candidate, subject, at)?;
    Ok(changed(&before, &after, work)?
        .into_iter()
        .filter_map(|member| {
            member
                .get("event")
                .or_else(|| member.get("causing_event"))
                .and_then(Yaml::as_str)
        })
        .map(str::to_owned)
        .collect())
}

struct OutputBytes(usize);
impl OutputBytes {
    fn charge(&mut self, value: &Json) -> Result<()> {
        self.0 = self.0.saturating_add(encoded(value)?.len());
        if self.0 > MAX_REPORT_BYTES {
            return Err(invalid(
                "complete grouped consequence partitions exceed 8 MiB",
            ));
        }
        Ok(())
    }
}

pub(super) fn preview(world: &World, request: &GroupedRequest) -> Result<Json> {
    let at = grouped_request(request)?;
    projection::preflight(world)?;
    // Source construction is shared with the ordinary native consequence
    // preview. This report does not copy its full-source diff into an as-of view.
    let prepared = crate::consequences::prepare(world, &request.plan, MAX_CHARACTERS)?;
    if prepared.report["validation"]["valid"] != true {
        return Err(Error::Validation(
            "grouped consequences require a valid complete candidate".into(),
        ));
    }
    let candidate = prepared.candidate;
    projection::preflight(&candidate)?;
    projection::horizon(world, &at)?;
    projection::horizon(&candidate, &at)?;
    let before = revision_review::consequence_source_view(world, &at)?;
    let after = revision_review::consequence_source_view(&candidate, &at)?;
    let mut focus = BTreeSet::new();
    for id in &request.focus_event_ids {
        let mut available = false;
        for (world, view) in [(world, &before), (&candidate, &after)] {
            if let Some(record) = world.records.get(id) {
                available |= view.contains_key(id)
                    && record.kind == "event"
                    && projection::admitted(world, record, &at)?;
            }
        }
        if !available {
            return Err(Error::NotFound(
                "exact focus event is unavailable at the requested horizon".into(),
            ));
        }
        focus.insert(id.clone());
    }
    let mut work = 0;
    let records = records(
        world,
        &candidate,
        &before,
        &after,
        &prepared.report["operations"],
        &mut focus,
        &mut work,
    )?;
    let delta = projection::delta(world, &candidate, &at)?;
    let changes = delta["changes"].as_array().expect("semantic delta changes");
    let mut owners = Vec::new();
    for change in changes {
        let events = change_owners(world, &candidate, change, &at, &mut work)?;
        focus.extend(events.iter().cloned());
        owners.push(events);
    }
    let mut eligible = Vec::new();
    for id in focus {
        let admitted = if after.contains_key(&id) {
            Some((&candidate, &candidate.records[&id]))
        } else if before.contains_key(&id) {
            Some((world, &world.records[&id]))
        } else {
            None
        };
        let Some((source, event)) = admitted else {
            continue;
        };
        if event.kind != "event" || !projection::admitted(source, event, &at)? {
            continue;
        }
        let time = event_time(source, event)?;
        eligible.push((
            (time.tick, time.order, id.clone()),
            id,
            json!({"event":reference(event),"eventTime":time,"changes":[],"recordChanges":[]}),
        ));
    }
    eligible.sort_by(|(left, _, _), (right, _, _)| left.cmp(right));
    if eligible.len() > MAX_GROUPS {
        return Err(invalid(
            "complete grouped consequence report exceeds 100 event groups",
        ));
    }
    let count = eligible.len()
        + changes.len()
        + records.len()
        + records
            .iter()
            .map(|record| record.sections.len())
            .sum::<usize>()
        + prepared
            .report
            .get("semanticAnalysis")
            .map_or(0, |analysis| {
                analysis["expectations"].as_array().map_or(0, Vec::len)
                    + analysis["delta"]["changes"].as_array().map_or(0, Vec::len)
            });
    if count > request.max_items {
        return Err(invalid(format!(
            "complete grouped consequence report requires {count} items; maxItems is {}",
            request.max_items
        )));
    }
    let order: Vec<_> = eligible.iter().map(|(_, id, _)| id.clone()).collect();
    let mut groups: BTreeMap<_, _> = eligible
        .into_iter()
        .map(|(_, id, value)| (id, value))
        .collect();
    let mut bytes = OutputBytes(0);
    let mut unattributed = Vec::new();
    for (change, owners) in changes.iter().zip(owners) {
        let linked: Vec<_> = owners
            .into_iter()
            .filter(|id| groups.contains_key(id))
            .collect();
        if linked.is_empty() {
            bytes.charge(change)?;
            unattributed.push(change.clone());
        }
        for id in linked {
            bytes.charge(change)?;
            groups.get_mut(&id).unwrap()["changes"]
                .as_array_mut()
                .unwrap()
                .push(change.clone());
        }
    }
    let mut unattributed_records = Vec::new();
    for record in records {
        let mut partitions = BTreeMap::<Option<String>, (Vec<Json>, Vec<Json>)>::new();
        for (section, owners, citations) in record.sections {
            let linked: Vec<_> = owners
                .into_iter()
                .filter(|id| groups.contains_key(id))
                .map(Some)
                .collect();
            for owner in if linked.is_empty() {
                vec![None]
            } else {
                linked
            } {
                let entry = partitions.entry(owner).or_default();
                entry.0.push(section.clone());
                entry.1.extend(citations.iter().cloned());
            }
        }
        if partitions.is_empty() {
            let id = record.row["recordId"].as_str().unwrap();
            let owner =
                (record.row["kind"] == "event" && groups.contains_key(id)).then(|| id.to_owned());
            partitions.insert(
                owner,
                (
                    Vec::new(),
                    record.row["citations"].as_array().unwrap().clone(),
                ),
            );
        }
        for (owner, (sections, citations)) in partitions {
            let mut value = record.row.clone();
            value["sections"] = sections.into();
            value["citations"] = citations.into();
            bytes.charge(&value)?;
            if let Some(owner) = owner {
                groups.get_mut(&owner).unwrap()["recordChanges"]
                    .as_array_mut()
                    .unwrap()
                    .push(value);
            } else {
                unattributed_records.push(value);
            }
        }
    }
    let groups: Vec<_> = order
        .into_iter()
        .map(|id| groups.remove(&id).unwrap())
        .collect();
    let mut report = json!({"protocol":crate::CONTRACT,"operation":"grouped-consequences","outcome":"ok",
        "scope":"local-author","worldId":world.world_record()?.id,"at":at,
        "baseFingerprint":world.fingerprint,"candidateFingerprint":candidate.fingerprint,"baseRevision":world.revision,
        "planSha256":prepared.report["planSha256"],"previewSha256":prepared.report["previewSha256"],
        "eventGroups":groups,"unattributedChanges":unattributed,"unattributedRecordChanges":unattributed_records,
        "changedSubjects":changes.len(),"incomparableSubjects":delta["incomparableSubjects"],
        "complete":true,"validationComplete":false,"writesPerformed":false,"canApply":false,
        "semantics":{"comparison":"same-horizon","source":"independently-admitted-originals",
            "stateScope":"complete-original-author-worlds","stateReferences":"native-values-with-scalar-entity-effect-pruning",
            "futureEntityTargetPruning":true,"attribution":"changed-explicit-contributions",
            "uniqueCausation":false,"sourceValues":"browser-safe-value-and-exact-typed-valueRaw",
            "maxAlignmentWork":MAX_ALIGNMENT_WORK,"alignmentWork":work},
        "notice":"Explicit event contribution groups and source sections are separate from semantic value differences; groups do not infer unique causation or approve source changes."});
    if let Some(analysis) = prepared.report.get("semanticAnalysis") {
        // The existing Plan's explicit checks remain visible, at their own
        // authored horizon, without acquiring approval authority from grouping.
        report["semanticAnalysis"] = analysis.clone();
    }
    finish(
        report,
        request,
        count,
        request.max_items,
        request.max_characters,
    )
}
