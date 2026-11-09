//! Literal offices, holdings, competing claims and disclosed vital histories.
//! No claim appoints a holder, and a transfer never supplies the target's hold.
use super::canonical::{
    Entry, LiteralFold, LiteralHistory, canonical_ref, effective_indices, fold_literal,
    history_json, optional_ref, ordered_text, parse_literal, require_valid, state_json,
};
use super::*;

const KINDS: &[&str] = &["legacy", "tenure", "claim", "vital-history"];

fn tenure_next(entry: &Entry<'_>, before: Option<&str>) -> Option<&'static str> {
    match (entry.kind, before) {
        ("tenure-initialize", None) => Some(if matches!(&entry.payload["holder_id"], Yaml::Null) {
            "vacant"
        } else {
            "holding"
        }),
        ("tenure-designate", Some("vacant")) => Some("designated"),
        ("tenure-hold", Some("vacant" | "designated")) => Some("holding"),
        ("tenure-vacate", Some("designated" | "holding")) => Some("vacant-interval"),
        ("tenure-transfer", Some("holding")) => Some("vacant"),
        ("tenure-end", Some("vacant" | "designated" | "holding")) => Some("ended"),
        _ => None,
    }
}

fn check(
    world: &World,
    record: &Record,
    entry: &Entry<'_>,
    before: Option<&str>,
) -> Result<String> {
    let label = format!("{}.payload", entry.field);
    match entry.kind {
        "legacy-initialize" | "legacy-rename" => {
            let value = closed(entry.payload, &["title", "aliases"], &[], &label)?;
            text(&value["title"], &format!("{label}.title"))?;
            ordered_text(&value["aliases"], &format!("{label}.aliases"))?;
        }
        "tenure-initialize" | "tenure-designate" | "tenure-hold" => {
            let value = closed(entry.payload, &["holder_id", "basis"], &[], &label)?;
            let basis = text(&value["basis"], &format!("{label}.basis"))?;
            if !["legal", "de-facto"].contains(&basis) {
                return Err(invalid(
                    &format!("{label}.basis"),
                    "expected legal or de-facto",
                ));
            }
            if entry.kind != "tenure-initialize" || !matches!(&value["holder_id"], Yaml::Null) {
                canonical_ref(
                    world,
                    &value["holder_id"],
                    "character",
                    &format!("{label}.holder_id"),
                )?;
            }
        }
        "tenure-vacate" => {
            let value = closed(entry.payload, &["holder_id"], &[], &label)?;
            if !matches!(&value["holder_id"], Yaml::Null) {
                return Err(invalid(
                    &format!("{label}.holder_id"),
                    "vacancy requires explicit null holder",
                ));
            }
        }
        "tenure-transfer" => {
            let value = closed(
                entry.payload,
                &["from_tenure_id", "to_tenure_id"],
                &[],
                &label,
            )?;
            let from = canonical_ref(
                world,
                &value["from_tenure_id"],
                "tenure",
                &format!("{label}.from_tenure_id"),
            )?;
            let to = canonical_ref(
                world,
                &value["to_tenure_id"],
                "tenure",
                &format!("{label}.to_tenure_id"),
            )?;
            if from.id == to.id {
                return Err(invalid(&label, "transfer endpoints must differ"));
            }
        }
        "claim-initialize" | "claim-dispute" => {
            let value = closed(entry.payload, &["competes_with"], &[], &label)?;
            ordered_text(&value["competes_with"], &format!("{label}.competes_with"))?;
            for (index, value) in value["competes_with"]
                .as_sequence()
                .unwrap()
                .iter()
                .enumerate()
            {
                canonical_ref(
                    world,
                    value,
                    "claim",
                    &format!("{label}.competes_with[{index}]"),
                )?;
            }
        }
        "legacy-dormant"
        | "legacy-dissolve"
        | "tenure-end"
        | "claim-recognize"
        | "claim-withdraw"
        | "claim-reject"
        | "vital-initialize"
        | "vital-birth"
        | "vital-death"
        | "vital-existence-start"
        | "vital-existence-end" => {
            closed(entry.payload, &[], &[], &label)?;
        }
        _ => {
            return Err(invalid(
                &format!("{}.transition_kind", entry.field),
                "unsupported transition for this record",
            ));
        }
    }
    let state_error = || {
        invalid(
            &format!("{}.transition_kind", entry.field),
            "transition is not allowed from the current lifecycle state",
        )
    };
    if record.kind == "tenure" {
        return tenure_next(entry, before)
            .map(str::to_owned)
            .ok_or_else(state_error);
    }
    let state = match (record.kind.as_str(), entry.kind, before) {
        ("legacy", "legacy-initialize", None) => "active",
        ("legacy", "legacy-rename", Some(state @ ("active" | "dormant"))) => state,
        ("legacy", "legacy-dormant", Some("active")) => "dormant",
        ("legacy", "legacy-dissolve", Some("active" | "dormant")) => "dissolved",
        ("claim", "claim-initialize", None) => "proposed",
        ("claim", "claim-dispute", Some("proposed" | "disputed")) => "disputed",
        ("claim", "claim-recognize", Some("proposed" | "disputed")) => "recognized",
        ("claim", "claim-withdraw", Some("proposed" | "disputed")) => "withdrawn",
        ("claim", "claim-reject", Some("proposed" | "disputed")) => "rejected",
        ("vital-history", "vital-initialize", None) => "unknown",
        ("vital-history", "vital-birth", Some("unknown")) => "living",
        ("vital-history", "vital-death", Some("living")) => "dead",
        ("vital-history", "vital-existence-start", Some("unknown")) => "existing",
        ("vital-history", "vital-existence-end", Some("existing")) => "ended",
        _ => return Err(state_error()),
    };
    Ok(state.to_owned())
}

fn parse<'a>(world: &'a World, record: &'a Record) -> Result<LiteralHistory<'a>> {
    match record.kind.as_str() {
        "legacy" => {
            let kind = text(field(record, "legacy_kind")?, "legacy_kind")?;
            if !["office", "estate", "title", "other"].contains(&kind) {
                return Err(invalid(
                    "legacy_kind",
                    "expected office, estate, title or other",
                ));
            }
            optional_ref(world, record, "organization_id", "organization")?;
        }
        "tenure" => {
            canonical_ref(world, field(record, "legacy_id")?, "legacy", "legacy_id")?;
            for key in ["predecessor_tenure_id", "successor_tenure_id"] {
                if optional_ref(world, record, key, "tenure")?
                    .is_some_and(|target| target.id == record.id)
                {
                    return Err(invalid(key, "tenure cannot link to itself"));
                }
            }
        }
        "claim" => {
            canonical_ref(world, field(record, "legacy_id")?, "legacy", "legacy_id")?;
            canonical_ref(
                world,
                field(record, "claimant_id")?,
                "character",
                "claimant_id",
            )?;
        }
        "vital-history" => {
            canonical_ref(
                world,
                field(record, "character_id")?,
                "character",
                "character_id",
            )?;
            let disclosure = text(field(record, "disclosure")?, "disclosure")?;
            if !["known", "unknown", "withheld"].contains(&disclosure) {
                return Err(invalid("disclosure", "expected known, unknown or withheld"));
            }
        }
        _ => {
            return Err(invalid(
                "kind",
                "expected legacy, tenure, claim or vital-history",
            ));
        }
    }
    parse_literal(world, record, check)
}

fn identity(record: &Record) -> Json {
    match record.kind.as_str() {
        "legacy" => {
            json!({"legacyKind":record.string("legacy_kind"),"organizationId":record.string("organization_id")})
        }
        "tenure" => {
            json!({"legacyId":record.string("legacy_id"),"predecessorTenureId":record.string("predecessor_tenure_id"),"successorTenureId":record.string("successor_tenure_id")})
        }
        "claim" => {
            json!({"legacyId":record.string("legacy_id"),"claimantId":record.string("claimant_id")})
        }
        _ => {
            json!({"characterId":record.string("character_id"),"disclosure":record.string("disclosure")})
        }
    }
}
fn apply(entry: &Entry<'_>, at: &StoryTime, folded: &mut LiteralFold) {
    match entry.kind {
        "legacy-initialize" | "legacy-rename" => {
            folded.value["title"] = json!(entry.payload["title"].as_str());
            folded.value["aliases"] = serde_json::to_value(&entry.payload["aliases"]).unwrap();
            if entry.kind == "legacy-initialize" {
                folded.lifecycle = "active".into();
            }
        }
        "legacy-dormant" => folded.lifecycle = "dormant".into(),
        "legacy-dissolve" => folded.lifecycle = "dissolved".into(),
        "tenure-initialize" | "tenure-designate" | "tenure-hold" => {
            let holder = entry.payload["holder_id"].as_str();
            folded.value["holderId"] = json!(holder);
            folded.value["basis"] = json!(entry.payload["basis"].as_str());
            folded.lifecycle = if entry.kind == "tenure-designate" {
                "designated"
            } else if holder.is_some() {
                "holding"
            } else {
                "vacant"
            }
            .into();
        }
        "tenure-vacate" => {
            folded.value["holderId"] = Json::Null;
            let last = entry
                .applicability
                .last
                .as_ref()
                .expect("validated interval");
            folded.lifecycle = if at.not_after(last) {
                "vacant-interval"
            } else {
                "expired"
            }
            .into();
        }
        "tenure-transfer" => {
            folded.value["holderId"] = Json::Null;
            folded.value["fromTenureId"] = json!(entry.payload["from_tenure_id"].as_str());
            folded.value["toTenureId"] = json!(entry.payload["to_tenure_id"].as_str());
            folded.lifecycle = "vacant".into();
        }
        "tenure-end" => folded.lifecycle = "ended".into(),
        "claim-initialize" | "claim-dispute" => {
            folded.value["competesWith"] =
                serde_json::to_value(&entry.payload["competes_with"]).unwrap();
            folded.lifecycle = if entry.kind == "claim-initialize" {
                "proposed"
            } else {
                "disputed"
            }
            .into();
        }
        "claim-recognize" => folded.lifecycle = "recognized".into(),
        "claim-withdraw" => folded.lifecycle = "withdrawn".into(),
        "claim-reject" => folded.lifecycle = "rejected".into(),
        "vital-initialize" => folded.lifecycle = "unknown".into(),
        "vital-birth" => folded.lifecycle = "living".into(),
        "vital-death" => folded.lifecycle = "dead".into(),
        "vital-existence-start" => folded.lifecycle = "existing".into(),
        "vital-existence-end" => folded.lifecycle = "ended".into(),
        _ => unreachable!("validated transition"),
    }
    folded.active = match entry.kind.split('-').next().unwrap() {
        "legacy" => matches!(folded.lifecycle.as_str(), "active" | "dormant"),
        "tenure" => matches!(
            folded.lifecycle.as_str(),
            "vacant" | "designated" | "holding" | "vacant-interval"
        ),
        "claim" => matches!(
            folded.lifecycle.as_str(),
            "proposed" | "disputed" | "recognized"
        ),
        _ => matches!(folded.lifecycle.as_str(), "living" | "existing"),
    };
}
fn fold(history: &LiteralHistory<'_>, at: &StoryTime) -> Option<LiteralFold> {
    fold_literal(history, at, identity(history.record), apply)
}
fn before(history: &LiteralHistory<'_>, at: &StoryTime) -> Option<LiteralFold> {
    if !canonical(history.record)
        || !history.entries[0].applicability.first.not_after(at)
        || history.entries[0].applicability.first == *at
    {
        return None;
    }
    let visible = history
        .entries
        .iter()
        .take_while(|entry| {
            entry.applicability.first.not_after(at) && entry.applicability.first != *at
        })
        .count();
    let superseded = history.entries[..visible]
        .iter()
        .filter_map(|entry| entry.replaces)
        .collect();
    let mut folded = LiteralFold {
        visible,
        superseded,
        lifecycle: String::new(),
        value: identity(history.record),
        active: false,
    };
    for index in effective_indices(history, visible) {
        let entry = &history.entries[index];
        // Transfer eligibility has a stricter replay than the public literal
        // fold: surviving fork leaves apply only from an allowed current state.
        // The source contract derives vacancy expiry directly at this point.
        let prior = if index == 0 {
            None
        } else {
            Some(folded.lifecycle.as_str())
        };
        if entry.kind == "tenure-vacate" || tenure_next(entry, prior).is_some() {
            apply(entry, at, &mut folded);
        }
    }
    Some(folded)
}

fn inspect(world: &World) -> (BTreeMap<&str, LiteralHistory<'_>>, Vec<Diagnostic>) {
    let mut histories = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| KINDS.contains(&record.kind.as_str()))
    {
        match parse(world, record) {
            Ok(history) => {
                histories.insert(record.id.as_str(), history);
            }
            Err(error) => diagnostics.push(diagnostic(record, "GEN-HISTORY", error)),
        }
    }
    for history in histories.values() {
        let record = history.record;
        if record.kind == "vital-history"
            && record.string("disclosure") == Some("unknown")
            && history
                .entries
                .iter()
                .any(|entry| entry.kind != "vital-initialize")
        {
            diagnostics.push(diagnostic(
                record,
                "GEN-VITAL-002",
                invalid(
                    "transitions",
                    "unknown vital history cannot contain affirmative transitions",
                ),
            ));
        }
        if record.kind == "claim" {
            // Every authored competition must be reciprocal, including a
            // superseded source entry; the target's surviving history supplies
            // its reciprocal assertion, exactly as the source contract states.
            for entry in &history.entries {
                let Some(competitors) = entry
                    .payload
                    .get("competes_with")
                    .and_then(Yaml::as_sequence)
                else {
                    continue;
                };
                for (index, target) in competitors.iter().enumerate() {
                    let target_id = target.as_str().unwrap();
                    let reciprocal = histories.get(target_id).is_some_and(|target| {
                        target.record.kind == "claim"
                            && target.record.string("legacy_id") == record.string("legacy_id")
                            && effective_indices(target, target.entries.len())
                                .into_iter()
                                .any(|index| {
                                    let candidate = &target.entries[index];
                                    candidate.applicability == entry.applicability
                                        && candidate
                                            .payload
                                            .get("competes_with")
                                            .and_then(Yaml::as_sequence)
                                            .is_some_and(|values| {
                                                values.iter().any(|value| {
                                                    value.as_str() == Some(record.id.as_str())
                                                })
                                            })
                                })
                    });
                    if target_id == record.id || !reciprocal {
                        diagnostics.push(diagnostic(record,"GEN-CLAIM-001",invalid(
                            &format!("{}.payload.competes_with[{index}]",entry.field),
                            "competing claims must reciprocally cite the same legacy and exact applicability")));
                    }
                }
            }
        }
        if record.kind != "tenure" || !canonical(record) {
            continue;
        }
        let start = &history.entries[0].applicability.first;
        for key in ["predecessor_tenure_id", "successor_tenure_id"] {
            let Some(target) = record.string(key).and_then(|id| histories.get(id)) else {
                continue;
            };
            let target_start = &target.entries[0].applicability.first;
            let ordered = if key == "predecessor_tenure_id" {
                target_start.not_after(start) && target_start != start
            } else {
                start.not_after(target_start) && start != target_start
            };
            if target.record.kind != "tenure"
                || target.record.string("legacy_id") != record.string("legacy_id")
                || !ordered
            {
                diagnostics.push(diagnostic(record,"GEN-TENURE-LINK-001",invalid(key,
                    "tenure link requires the same legacy and strict initial time order on one timeline")));
            }
        }
        for entry in history
            .entries
            .iter()
            .filter(|entry| entry.kind == "tenure-transfer")
        {
            if entry.payload["from_tenure_id"].as_str() != Some(record.id.as_str()) {
                diagnostics.push(diagnostic(
                    record,
                    "GEN-TENURE-TRANSFER-001",
                    invalid(
                        &format!("{}.payload.from_tenure_id", entry.field),
                        "transfer source must be this holding tenure",
                    ),
                ));
            }
            let target_id = entry.payload["to_tenure_id"].as_str().unwrap();
            let compatible = histories.get(target_id).is_some_and(|target| {
                target.record.kind == "tenure"
                    && target.record.string("legacy_id") == record.string("legacy_id")
                    && before(target, &entry.applicability.first).is_some_and(|fold| {
                        matches!(fold.lifecycle.as_str(), "vacant" | "designated")
                    })
            });
            if !compatible {
                diagnostics.push(diagnostic(record,"GEN-TENURE-TRANSFER-002",invalid(
                    &format!("{}.payload.to_tenure_id",entry.field),
                    "transfer target must already be vacant or designated on this timeline for the same legacy")));
            }
        }
    }
    (histories, diagnostics)
}
pub(super) fn validate(world: &World) -> Vec<Diagnostic> {
    inspect(world).1
}

fn unavailable(history: &LiteralHistory<'_>, at: &StoryTime) -> &'static str {
    if !canonical(history.record) {
        "noncanonical"
    } else if history.entries[0].applicability.first.timeline != at.timeline {
        "different_timeline"
    } else {
        "before_initialization"
    }
}
fn resolve<'a>(world: &'a World, selector: &str, kind: &str) -> Result<&'a Record> {
    let record = world.resolve(selector)?;
    if record.kind != kind {
        return Err(Error::InvalidInput(format!("this query requires a {kind}")));
    }
    Ok(record)
}

pub fn legacy(world: &World, selector: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    let mut result = envelope(world, at, limit)?;
    require_valid(world)?;
    let selected = resolve(world, selector, "legacy")?;
    let (histories, _) = inspect(world);
    let history = &histories[selected.id.as_str()];
    result["legacyId"] = json!(selected.id);
    result["legacy"] = Json::Null;
    result["tenures"] = json!([]);
    result["holders"] = json!([]);
    result["claims"] = json!([]);
    result["succession"] = json!([]);
    result["count"] = json!(0);
    result["holderCount"] = json!(0);
    result["successionCount"] = json!(0);
    let Some(folded) = fold(history, at) else {
        result["available"] = json!(false);
        result["reason"] = json!(unavailable(history, at));
        return Ok(result);
    };
    result["legacy"] = state_json(world, history, at, &folded, limit)?;
    result["available"] = json!(true);
    result["reason"] = Json::Null;
    let (mut tenures, mut holders, mut claims) = (Vec::new(), Vec::new(), Vec::new());
    for history in histories
        .values()
        .filter(|history| history.record.string("legacy_id") == Some(selected.id.as_str()))
    {
        let Some(folded) = fold(history, at) else {
            continue;
        };
        let value = state_json(world, history, at, &folded, limit)?;
        if history.record.kind == "tenure" {
            bounded(tenures.len() + 1, limit)?;
            if folded.lifecycle == "holding" {
                holders.push(value.clone());
            }
            tenures.push(value);
        } else if history.record.kind == "claim" {
            bounded(claims.len() + 1, limit)?;
            claims.push(value);
        }
    }
    let visible_ids: BTreeSet<_> = tenures
        .iter()
        .filter_map(|row| row["recordId"].as_str())
        .collect();
    let mut succession = Vec::new();
    for row in &tenures {
        let record_id = row["recordId"].as_str().unwrap();
        let source = &histories[record_id];
        for (field, key) in [
            ("predecessor_tenure_id", "predecessorTenureId"),
            ("successor_tenure_id", "successorTenureId"),
        ] {
            let Some(linked) = row["value"][key]
                .as_str()
                .filter(|id| visible_ids.contains(id))
            else {
                continue;
            };
            let (from, to) = if field == "predecessor_tenure_id" {
                (linked, record_id)
            } else {
                (record_id, linked)
            };
            bounded(succession.len() + 1, limit)?;
            succession.push(
                json!({"from":from,"to":to,"recordId":record_id,"field":field,
                "linkCitation":citation(world,source.record,field),"citations":row["citations"]}),
            );
        }
    }
    result["count"] = json!(tenures.len() + claims.len());
    result["holderCount"] = json!(holders.len());
    result["successionCount"] = json!(succession.len());
    result["tenures"] = json!(tenures);
    result["holders"] = json!(holders);
    result["claims"] = json!(claims);
    result["succession"] = json!(succession);
    Ok(result)
}

pub fn organization_legacies(
    world: &World,
    selector: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    let mut result = envelope(world, at, limit)?;
    require_valid(world)?;
    let organization = resolve(world, selector, "organization")?;
    result["organizationId"] = json!(organization.id);
    result["items"] = json!([]);
    result["count"] = json!(0);
    // The organization only gates existence at this horizon; its history is
    // not returned and does not spend the linked-legacy result budget.
    if super::canonical::organization_at(world, organization, at, MAX_HISTORY)?.is_none() {
        result["available"] = json!(false);
        result["reason"] = json!("unavailable");
        return Ok(result);
    }
    let (histories, _) = inspect(world);
    let mut items = Vec::new();
    for history in histories.values().filter(|history| {
        history.record.kind == "legacy"
            && history.record.string("organization_id") == Some(organization.id.as_str())
    }) {
        let Some(folded) = fold(history, at) else {
            continue;
        };
        bounded(items.len() + 1, limit)?;
        items.push(state_json(world, history, at, &folded, limit)?);
    }
    result["available"] = json!(true);
    result["reason"] = Json::Null;
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}

pub fn vital(world: &World, character: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    let mut result = envelope(world, at, limit)?;
    require_valid(world)?;
    let character = query_character(world, character)?;
    let (histories, _) = inspect(world);
    let mut states = BTreeSet::new();
    let mut items = Vec::new();
    for history in histories.values().filter(|history| {
        history.record.kind == "vital-history"
            && history.record.string("character_id") == Some(character.id.as_str())
            && history.record.string("disclosure") == Some("known")
    }) {
        let Some(folded) = fold(history, at).filter(|fold| {
            matches!(
                fold.lifecycle.as_str(),
                "living" | "dead" | "existing" | "ended"
            )
        }) else {
            continue;
        };
        bounded(items.len() + 1, limit)?;
        states.insert(folded.lifecycle.clone());
        items.push(state_json(world, history, at, &folded, limit)?);
    }
    result["characterId"] = json!(character.id);
    result["available"] = json!(!items.is_empty());
    result["reason"] = if items.is_empty() {
        json!("unknown")
    } else {
        Json::Null
    };
    result["vital"] = if states.len() == 1 {
        json!(states.first())
    } else {
        Json::Null
    };
    result["ambiguous"] = json!(states.len() > 1);
    result["states"] = json!(states);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}

pub(super) fn history(world: &World, selector: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    require_valid(world)?;
    let record = world.resolve(selector)?;
    let history = parse(world, record)?;
    if record.kind == "vital-history" && record.string("disclosure") != Some("known") {
        let mut result = history_json(world, &history, at, None, limit)?;
        result["reason"] = json!("undisclosed");
        return Ok(result);
    }
    let folded = fold(&history, at);
    history_json(world, &history, at, folded.as_ref(), limit)
}
