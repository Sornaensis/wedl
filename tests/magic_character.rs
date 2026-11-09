//! Character magic retains selected learned words without canonical role joins.
#[path = "support/family_character.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value as Json, json};
use wedl::{Error, magic};

fn read(world: &wedl::model::World, targets: &[String], tick: i64) -> Json {
    magic::character(world, VIEWER, targets, &at(tick, 0)).unwrap()
}

#[test]
fn character_magic_contains_only_explicit_literal_learned_targets() {
    let w = base();
    valid(&w);
    let value = read(&w, &[id("assertion", 2)], 6);
    assert_eq!(value["basis"], "explicitly-learned-magic-explanations");
    assert_eq!(value["outcome"], "available");
    assert_eq!(value["count"], 1);
    assert_eq!(
        value["beliefs"][0]["result"]["items"][0]["statement"],
        "The learned magic explanation"
    );
    assert_eq!(
        value["beliefs"][0]["result"]["items"][0]["state"],
        "rejected"
    );
    assert_eq!(value["beliefs"][0]["result"]["items"][0]["confidence"], 1.0);
    assert!(value.get("records").is_none());
    assert!(value.get("selection").is_none());
    assert!(value.get("canon").is_none());
    assert_eq!(value["complete"], true);
    no_author_material(&value);
}

#[test]
fn current_canonical_profiles_subjects_applicability_and_status_never_rewrite_magic_learning() {
    let w = base();
    let before = read(&w, &[id("assertion", 2)], 6);
    let w = edit(&w, &id("assertion", 2), |data| {
        data["subject"] = serde_yaml::to_value(rr(&id("concept", 3))).unwrap();
        data["value"] = serde_yaml::Value::String("A changed canonical explanation".into());
        data["valid"] = serde_yaml::to_value(interval(40, 50)).unwrap();
        data["status"] = serde_yaml::Value::String("retired".into());
    });
    let w = edit(&w, &id("concept", 10), |data| {
        data["values"]["restrictions"] =
            serde_yaml::Value::String("A changed current canonical requirement".into());
        data["profile"] = serde_yaml::Value::String("other-profile".into());
        data["status"] = serde_yaml::Value::String("draft".into());
        data["title"] = serde_yaml::Value::String("A changed private title".into());
    });
    valid(&w);
    assert_eq!(read(&w, &[id("assertion", 2)], 6), before);
}

#[test]
fn acquisition_forgetting_remembering_and_private_learning_keep_unknown_results_closed() {
    let w = base();
    let target = vec![id("assertion", 2)];
    for tick in [4, 8, 9] {
        let value = read(&w, &target, tick);
        assert_eq!(value["outcome"], "unknown");
        assert_eq!(value["count"], 0);
        assert_eq!(value["beliefs"][0]["result"]["items"], json!([]));
    }
    assert_eq!(read(&w, &target, 5)["count"], 1);
    assert_eq!(
        read(&w, &target, 10)["beliefs"][0]["result"]["items"][0]["state"],
        "remembered"
    );
    let w = set(&w, &id("know", 101), "audience", json!(["private"]));
    valid(&w);
    let before = read(&w, &target, 6);
    let w = edit(&w, &id("know", 101), |data| {
        data["claim"]["statement"] =
            serde_yaml::Value::String("Different hidden learned words".into())
    });
    assert_eq!(read(&w, &target, 6), before);
    no_author_material(&before);
}

#[test]
fn multiple_explicit_targets_preserve_request_identity_and_do_not_include_unselected_accounts() {
    let w = base();
    let targets = vec![id("assertion", 99), id("know", 101), id("assertion", 1)];
    let value = read(&w, &targets, 6);
    assert_eq!(value["count"], 2);
    assert_eq!(value["beliefs"][0]["targetId"], id("assertion", 99));
    assert_eq!(value["beliefs"][0]["result"]["count"], 0);
    assert_eq!(
        value["beliefs"][1]["result"]["items"][0]["knowledgeId"],
        id("know", 101)
    );
    assert_eq!(
        value["beliefs"][2]["result"]["items"][0]["knowledgeId"],
        id("know", 100)
    );
    let single = read(&w, &[id("know", 101)], 6);
    assert!(
        !serde_json::to_string(&single)
            .unwrap()
            .contains("retained mistaken")
    );
    no_author_material(&value);
}

#[test]
fn target_selection_is_required_closed_unique_bounded_and_never_an_alias_lookup() {
    let w = base();
    for targets in [
        vec![],
        vec![id("concept", 10)],
        vec!["The learned magic explanation".into()],
        vec![id("assertion", 2); 2],
        (1..=9).map(|n| id("assertion", n)).collect(),
    ] {
        assert!(matches!(
            magic::character(&w, VIEWER, &targets, &at(6, 0)),
            Err(Error::InvalidInput(_))
        ));
    }
    let targets = (20..28).map(|n| id("assertion", n)).collect::<Vec<_>>();
    assert_eq!(read(&w, &targets, 6)["outcome"], "unknown");
    assert_eq!(
        read(&w, &[id("know", 10)], 6)["count"],
        0,
        "ordinary concept acquisition is not a magic explanation"
    );
    assert_eq!(
        magic::character(&w, OTHER, &[id("assertion", 2)], &at(6, 0)).unwrap()["count"],
        0
    );
}

#[test]
fn author_magic_attaches_native_sections_and_classified_assertions_at_each_returned_owner() {
    let w = base();
    let result = magic::compose(
        &w,
        &id("concept", 10),
        &magic_selection(),
        Some(&at(6, 0)),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(
        result["canon"]
            .as_array()
            .unwrap()
            .iter()
            .map(|stream| stream["ownerId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![id("concept", 10), id("concept", 11)]
    );
    assert_eq!(
        result["canon"][0]["result"]["items"][0]["classification"],
        "author-fact"
    );
    assert_eq!(
        result["canon"][0]["result"]["items"][0]["value"],
        "CANONICAL_MAGIC"
    );
    assert_eq!(
        result["sections"][0]["result"]["items"][0]["markdown"],
        "Explicit section 10\r\n"
    );
    assert_eq!(
        result["sections"][1]["result"]["items"][0]["citation"]["entityId"],
        id("concept", 11)
    );
    let expired = magic::compose(
        &w,
        &id("concept", 10),
        &magic_selection(),
        Some(&at(20, 1)),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(expired["sections"][0]["result"]["count"], 0);
    let history = magic::compose(
        &w,
        &id("concept", 10),
        &magic_selection(),
        None,
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(
        history["sections"][0]["result"]["scope"],
        "author-all-time-descriptions"
    );
}

#[test]
fn author_corrections_keep_all_time_history_and_literal_untimed_body_separate() {
    let w = base();
    let mut replacement = section(2, "Revised explicit practice section");
    replacement["replaces"] = json!(id("description", 1));
    let w = set(
        &w,
        &id("concept", 10),
        "descriptions",
        json!([section(1, "Old explicit practice section"), replacement]),
    );
    valid(&w);
    let dated = magic::compose(
        &w,
        &id("concept", 10),
        &magic_selection(),
        Some(&at(6, 0)),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(dated["sections"][0]["result"]["count"], 1);
    assert_eq!(
        dated["sections"][0]["result"]["items"][0]["markdown"],
        "Revised explicit practice section"
    );
    assert_eq!(dated["records"][0]["body"], "PRIVATE_CANONICAL_BODY\r\n");
    assert_eq!(dated["records"][0]["bodyKind"], "undated-authored-prose");
    let history = magic::compose(
        &w,
        &id("concept", 10),
        &magic_selection(),
        None,
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(history["sections"][0]["result"]["count"], 2);
}

#[test]
fn exact_large_learning_coordinates_preserve_order_and_timeline_independence() {
    let tick = 9_007_199_254_740_993;
    let mut learned_at = transition(tick, "accepted");
    learned_at["time"] = point(tick, 2);
    let w = set(
        &base(),
        &id("know", 101),
        "transitions",
        json!([learned_at]),
    );
    valid(&w);
    assert_eq!(
        magic::character(&w, VIEWER, &[id("assertion", 2)], &at(tick, 1)).unwrap()["count"],
        0
    );
    let value = magic::character(&w, VIEWER, &[id("assertion", 2)], &at(tick, 2)).unwrap();
    assert_eq!(
        value["beliefs"][0]["result"]["items"][0]["time"]["tick"],
        tick.to_string()
    );
    let other = wedl::model::StoryTime::new("other", tick, 2).unwrap();
    assert_eq!(
        magic::character(&w, VIEWER, &[id("assertion", 2)], &other).unwrap()["count"],
        0
    );
}

#[test]
fn selected_admitted_evidence_and_complete_report_caps_ignore_unselected_or_private_claims() {
    let w = base();
    let huge = edit(&w, &id("know", 101), |data| {
        data["claim"]["statement"] = serde_yaml::Value::String("z".repeat(262_144))
    });
    valid(&huge);
    let error = magic::character(&huge, VIEWER, &[id("assertion", 2)], &at(6, 0)).unwrap_err();
    assert!(error.to_string().contains("262144"));
    assert_eq!(
        read(&huge, &[id("assertion", 1)], 6),
        read(&w, &[id("assertion", 1)], 6)
    );
    let mut records = w.records.values().cloned().collect::<Vec<_>>();
    for n in 200..712 {
        records.push(learned(n, &id("assertion", 1), "Separate held report"));
    }
    let large = from(records);
    valid(&large);
    let error = magic::character(&large, VIEWER, &[id("assertion", 1)], &at(6, 0)).unwrap_err();
    assert!(error.to_string().contains("512"));
    assert_eq!(
        read(&large, &[id("assertion", 2)], 6),
        read(&w, &[id("assertion", 2)], 6)
    );
}

#[test]
fn immutable_captured_source_is_validated_without_exposing_author_diagnostics() {
    let w = base();
    let bytes = w
        .records
        .values()
        .map(|r| r.raw_bytes.clone())
        .collect::<Vec<_>>();
    let before = read(&w, &[id("assertion", 2)], 6);
    assert_eq!(
        w.records
            .values()
            .map(|r| r.raw_bytes.clone())
            .collect::<Vec<_>>(),
        bytes
    );
    let mut broken = w.clone();
    broken.fingerprint = "f".repeat(64);
    assert!(matches!(
        magic::character(&broken, VIEWER, &[id("assertion", 2)], &at(6, 0)),
        Err(Error::Validation(_))
    ));
    let mut changed = w.clone();
    changed
        .records
        .get_mut(&id("know", 101))
        .unwrap()
        .frontmatter["title"] = serde_yaml::Value::String("incoherent title".into());
    let error = magic::character(&changed, VIEWER, &[id("assertion", 2)], &at(6, 0)).unwrap_err();
    assert!(!error.to_string().contains("incoherent title"));
    assert_eq!(read(&w, &[id("assertion", 2)], 6), before);
}
