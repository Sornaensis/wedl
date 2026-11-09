//! Specialized character families admit before selecting, traversing or bounding.
#[path = "support/family_context.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value as Json, json};
use wedl::{environment, social};

fn social_read(world: &wedl::model::World) -> Json {
    social::character_context(
        world,
        VIEWER,
        &social_selection(),
        &[id("assertion", 90)],
        &at(5, 0),
    )
    .unwrap()
}
fn environment_read(world: &wedl::model::World) -> Json {
    environment::character_context(
        world,
        VIEWER,
        &[concept_id(10)],
        &[id("know", 90)],
        &at(5, 0),
    )
    .unwrap()
}
fn records(value: &Json) -> Vec<&str> {
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["item"]["conceptId"].as_str().unwrap())
        .collect()
}

#[test]
fn social_selects_known_language_forms_and_separate_explicit_evidence() {
    let world = world();
    let value = social_read(&world);
    assert_eq!(value["count"], 6);
    assert_eq!(value["sections"].as_array().unwrap().len(), 3);
    assert_eq!(value["beliefs"][0]["result"]["count"], 1);
    assert_eq!(
        value["beliefs"][0]["result"]["items"][0]["statement"],
        "I was told the kiln keeps its heat."
    );
    assert_eq!(
        value["beliefs"][0]["result"]["items"][0]["confidence"],
        0.75
    );
    let text = value.to_string();
    for forbidden in [
        "AUTHOR_",
        "fingerprint",
        "revision",
        "sourceField",
        "sourcePath",
        "classification",
        "attributedTo",
        "provenance",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}: {text}");
    }
    let mut form = social_selection();
    form.form = Some("BANK".into());
    let filtered = social::character_context(&world, VIEWER, &form, &[], &at(5, 0)).unwrap();
    assert!(!records(&filtered).contains(&concept_id(3).as_str()));
    assert!(records(&filtered).contains(&concept_id(2).as_str()));
}

#[test]
fn environment_uses_one_admitted_hop_without_canonical_summaries() {
    let value = environment_read(&world());
    assert_eq!(
        records(&value),
        vec![concept_id(10), concept_id(11), concept_id(13)]
    );
    assert_eq!(value["links"].as_array().unwrap().len(), 2);
    assert_eq!(value["sections"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["records"][0]["item"]["values"]["literal"]["id"],
        concept_id(12)
    );
    assert_eq!(
        value["records"][1]["item"]["values"]["dependencies"][0]["id"],
        concept_id(14)
    );
    for field in [
        "canon",
        "unknowns",
        "competingAccounts",
        "fingerprint",
        "revision",
    ] {
        assert!(value.get(field).is_none());
    }
    let text = value.to_string();
    for forbidden in [
        "AUTHOR_",
        "sourceField",
        "ordinal",
        "\"citation\":",
        "PRIVATE_RESOURCE",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}: {text}");
    }
}

#[test]
fn selection_never_teaches_roots_and_acquisition_is_exact() {
    let source = world();
    assert!(
        social::character_context(&source, VIEWER, &social_selection(), &[], &at(1, 0)).is_err()
    );
    let before = social::character_context(
        &source,
        VIEWER,
        &social_selection(),
        &[id("assertion", 90)],
        &at(2, 0),
    )
    .unwrap();
    assert_eq!(before["beliefs"][0]["result"]["count"], 0);
    let after = social::character_context(
        &source,
        VIEWER,
        &social_selection(),
        &[id("assertion", 90)],
        &at(3, 0),
    )
    .unwrap();
    assert_eq!(after["beliefs"][0]["result"]["count"], 1);
    assert!(
        environment::character_context(&source, VIEWER, &[concept_id(18)], &[], &at(5, 0)).is_err()
    );
    assert!(
        social::character_context(&source, OTHER, &social_selection(), &[], &at(5, 0)).is_err()
    );
    let forgotten = edit(&source, &id("know", 1), |data| {
        data["transitions"].as_sequence_mut().unwrap().push(serde_yaml::to_value(json!({"id":"forgot","time":point(5,0),"state":"forgotten","confidence":0.0,"acquisition":"heard"})).unwrap());
    });
    valid(&forgotten);
    assert!(
        social::character_context(&forgotten, VIEWER, &social_selection(), &[], &at(5, 0)).is_err()
    );
    assert!(
        social::character_context(&forgotten, VIEWER, &social_selection(), &[], &at(4, 0)).is_ok()
    );
}

#[test]
fn hidden_future_noncanonical_wrong_role_and_missing_roots_are_equivalent() {
    let source = world();
    let read = |world: &wedl::model::World, roots: Vec<String>| {
        environment::character_context(world, VIEWER, &roots, &[], &at(5, 0))
            .unwrap_err()
            .to_string()
    };
    let missing = read(&source, vec![concept_id(999)]);
    assert_eq!(read(&source, vec![concept_id(1)]), missing);
    assert_eq!(read(&source, vec![concept_id(12)]), missing);
    for (field, value) in [
        ("status", json!("retired")),
        ("status", json!("draft")),
        ("valid", interval(6, 10)),
        ("audience", json!(["author"])),
        ("perspectives", json!(["author"])),
    ] {
        let candidate = set(&source, &concept_id(10), field, value);
        valid(&candidate);
        assert_eq!(read(&candidate, vec![concept_id(10)]), missing);
    }
}

#[test]
fn canonical_assertion_metadata_never_changes_selected_literal_beliefs() {
    let source = world();
    let social_before = social_read(&source);
    let environment_before = environment_read(&source);
    let candidate = edit(&source, &id("assertion", 90), |data| {
        data["title"] = serde_yaml::to_value("PRIVATE_NEW_ASSERTION_TITLE").unwrap();
        data["subject"] = serde_yaml::to_value(reference(6)).unwrap();
        data["value"] = serde_yaml::to_value("PRIVATE_DIFFERENT_CANON").unwrap();
        data["classification"] = serde_yaml::to_value("unresolved").unwrap();
        data["valid"] = serde_yaml::to_value(interval(100, 200)).unwrap();
        data["status"] = serde_yaml::to_value("retired").unwrap();
    });
    valid(&candidate);
    assert_eq!(social_read(&candidate), social_before);
    assert_eq!(environment_read(&candidate), environment_before);
}

#[test]
fn private_reference_positions_and_large_unlearned_catalogues_do_not_change_response() {
    let source = world();
    let before = environment_read(&source);
    let mut records = source.records.values().cloned().collect::<Vec<_>>();
    for n in 100..150 {
        records.push(concept(
            n,
            "context-resource",
            json!({"note":"UNLEARNED_CATALOGUE"}),
        ));
    }
    let candidate = from_records(records);
    let refs = (100..150)
        .map(reference)
        .chain([reference(12), reference(11)])
        .collect::<Vec<_>>();
    let candidate = edit(&candidate, &concept_id(10), |data| {
        data["values"]["resources"] = serde_yaml::to_value(refs).unwrap()
    });
    valid(&candidate);
    assert_eq!(environment_read(&candidate), before);
    let text = before.to_string();
    assert!(!text.contains("resources["));
}

#[test]
fn unknown_belief_targets_remain_empty_and_grant_no_concept() {
    let source = world();
    let targets = [id("assertion", 999), id("know", 999)];
    let value =
        environment::character_context(&source, VIEWER, &[concept_id(10)], &targets, &at(5, 0))
            .unwrap();
    assert_eq!(value["beliefs"].as_array().unwrap().len(), 2);
    for stream in value["beliefs"].as_array().unwrap() {
        assert_eq!(stream["result"]["count"], 0);
        assert_eq!(stream["result"]["items"], json!([]));
    }
    let hidden = environment::character_context(
        &source,
        VIEWER,
        &[concept_id(12)],
        &[id("assertion", 90)],
        &at(5, 0),
    );
    assert!(hidden.is_err());
}

#[test]
fn private_future_sections_and_valid_corrections_keep_native_admission() {
    let source = world();
    let before = social_read(&source);
    let mut private = section(201, "PRIVATE_SECTION");
    private["audience"] = json!(["author"]);
    let mut future = section(202, "FUTURE_SECTION");
    future["valid"] = interval(6, 10);
    let candidate = set(
        &source,
        &concept_id(1),
        "descriptions",
        json!([section(1, "Dated family account 1"), private, future]),
    );
    valid(&candidate);
    assert_eq!(social_read(&candidate), before);
    let mut replacement = section(203, "Current replacement");
    replacement["replaces"] = json!(id("description", 1));
    let candidate = set(
        &source,
        &concept_id(1),
        "descriptions",
        json!([section(1, "Dated family account 1"), replacement]),
    );
    valid(&candidate);
    let value = social_read(&candidate);
    assert_eq!(
        value["sections"][0]["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        value["sections"][0]["result"]["items"][0]["markdown"],
        "Current replacement"
    );
}

#[test]
fn root_and_belief_bounds_reject_incomplete_or_ambiguous_selections() {
    let source = world();
    for targets in [
        vec![id("assertion", 90); 2],
        vec![concept_id(10)],
        (100..109).map(|n| id("assertion", n)).collect(),
    ] {
        assert!(
            environment::character_context(&source, VIEWER, &[concept_id(10)], &targets, &at(5, 0))
                .is_err()
        );
    }
    for roots in [
        vec![],
        vec![concept_id(10); 2],
        (100..109).map(concept_id).collect(),
    ] {
        assert!(environment::character_context(&source, VIEWER, &roots, &[], &at(5, 0)).is_err());
    }
    let mut records = source.records.values().cloned().collect::<Vec<_>>();
    for n in 100..132 {
        records.push(concept(n, "context-resource", json!({})));
        records.push(acquire(n, &concept_id(n)));
    }
    let candidate = from_records(records);
    let candidate = edit(&candidate, &concept_id(10), |data| {
        data["values"]["resources"] =
            serde_yaml::to_value((100..132).map(reference).collect::<Vec<_>>()).unwrap()
    });
    valid(&candidate);
    let error =
        environment::character_context(&candidate, VIEWER, &[concept_id(10)], &[], &at(5, 0))
            .unwrap_err()
            .to_string();
    assert!(error.contains("32"), "{error}");
}

#[test]
fn evidence_and_utf8_report_budgets_follow_admission() {
    let source = world();
    let sections = (1000..1513)
        .map(|n| section(n, "Admitted item"))
        .collect::<Vec<_>>();
    let candidate = set(&source, &concept_id(10), "descriptions", json!(sections));
    valid(&candidate);
    let error =
        environment::character_context(&candidate, VIEWER, &[concept_id(10)], &[], &at(5, 0))
            .unwrap_err()
            .to_string();
    assert!(error.contains("512"), "{error}");
    let huge = section(900, &"é".repeat(140_000));
    let candidate = set(&source, &concept_id(10), "descriptions", json!([huge]));
    valid(&candidate);
    let error =
        environment::character_context(&candidate, VIEWER, &[concept_id(10)], &[], &at(5, 0))
            .unwrap_err()
            .to_string();
    assert!(error.contains("262144"), "{error}");
    let mut hidden = section(900, &"é".repeat(140_000));
    hidden["audience"] = json!(["author"]);
    let candidate = set(
        &source,
        &concept_id(10),
        "descriptions",
        json!([section(10, "Dated family account 10"), hidden]),
    );
    valid(&candidate);
    assert_eq!(environment_read(&candidate), environment_read(&source));
}

#[test]
fn character_reads_preserve_author_behavior_and_captured_world() {
    let source = world();
    let fingerprint = source.fingerprint.clone();
    let bytes = source
        .records
        .iter()
        .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
        .collect::<Vec<_>>();
    let before =
        environment::context(&source, &[concept_id(10)], &at(5, 0), "author", "author").unwrap();
    social_read(&source);
    environment_read(&source);
    assert_eq!(
        environment::context(&source, &[concept_id(10)], &at(5, 0), "author", "author").unwrap(),
        before
    );
    assert_eq!(source.fingerprint, fingerprint);
    assert_eq!(
        source
            .records
            .iter()
            .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
            .collect::<Vec<_>>(),
        bytes
    );
}
