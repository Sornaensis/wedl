//! Specialized descriptions use the same independent character admission gate.
#[path = "support/family_character.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value as Json, json};
use wedl::{Error, species};

fn compose(world: &wedl::model::World, targets: &[String], time: &wedl::model::StoryTime) -> Json {
    species::character_compose(
        world,
        VIEWER,
        &id("concept", 2),
        &species_selection(),
        targets,
        time,
    )
    .unwrap()
}

#[test]
fn independently_known_common_and_individual_statements_keep_exact_safe_values() {
    let w = base();
    valid(&w);
    let value = compose(&w, &[], &at(6, 0));
    assert_eq!(value["perspective"], "character");
    assert_eq!(
        value["common"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["conceptId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![id("concept", 1), id("concept", 4)]
    );
    assert_eq!(
        value["individual"]["fields"]["traits"]["value"],
        "Individual words"
    );
    assert_eq!(
        value["common"][0]["fields"]["life_stage"]["value"],
        "9007199254740993"
    );
    assert_eq!(
        value["common"][0]["fields"]["language"]["valueType"]["scalarType"],
        "number"
    );
    assert_eq!(
        value["individual"]["fields"]["language"]["valueType"]["scalarType"],
        "string"
    );
    assert!(value["individual"]["fields"]["culture"]["value"].is_null());
    assert_eq!(
        value["individual"]["fields"]["habitat"]["value"],
        json!([rr(&id("concept", 4))])
    );
    assert_eq!(
        value["individual"]["fields"]["habitat"]["references"],
        json!([{"entityId":id("concept",4),"kind":"lore-concept","target":rr(&id("concept",4))}])
    );
    assert_eq!(value["sections"].as_array().unwrap().len(), 3);
    assert_eq!(value["beliefs"], json!([]));
    assert_eq!(value["linkageComplete"], true);
    assert_eq!(value["complete"], true);
    no_author_material(&value);
}

#[test]
fn hidden_array_insertions_cycles_and_canonical_metadata_leave_the_whole_packet_unchanged() {
    let w = base();
    let before = compose(&w, &[], &at(6, 0));
    let w = edit(&w, &id("concept", 2), |data| {
        data["values"]["parents"] = serde_yaml::to_value(json!([
            rr(&id("concept", 3)),
            rr(&id("concept", 3)),
            rr(&id("concept", 4))
        ]))
        .unwrap();
        data["title"] = serde_yaml::Value::String("New private current title".into());
        data["provenance"] = serde_yaml::Value::String("Changed private source provenance".into());
    });
    let w = edit(&w, &id("concept", 1), |data| {
        let mut links = vec![rr(&id("concept", 3)); 1023];
        links.push(rr(&id("concept", 4)));
        data["values"]["parents"] = serde_yaml::to_value(links).unwrap();
    });
    valid(&w);
    assert_eq!(compose(&w, &[], &at(6, 0)), before);
}

#[test]
fn selected_profiles_and_links_do_not_acquire_the_individual_or_a_common_target() {
    let w = base();
    for (viewer, time) in [(VIEWER, at(4, 0)), (OTHER, at(6, 0))] {
        assert!(matches!(
            species::character_compose(
                &w,
                viewer,
                &id("concept", 2),
                &species_selection(),
                &[],
                &time
            ),
            Err(Error::NotFound(_))
        ));
    }
    let w = set(
        &w,
        &id("know", 1),
        "transitions",
        json!([transition(5, "accepted"), transition(6, "forgotten")]),
    );
    valid(&w);
    let value = compose(&w, &[], &at(6, 0));
    assert_eq!(value["common"], json!([]));
    assert_eq!(
        value["unknowns"],
        json!([{"ownerId":id("concept",2),"field":"common","reason":"absent-or-unavailable"}])
    );
    no_author_material(&value);
}

#[test]
fn explicit_belief_targets_stay_separate_from_current_canonical_ownership_and_wording() {
    let w = base();
    let targets = vec![id("assertion", 1), id("know", 101)];
    let before = compose(&w, &targets, &at(6, 0));
    assert_eq!(
        before["beliefs"][0]["result"]["items"][0]["statement"],
        "A retained mistaken explanation"
    );
    assert_eq!(
        before["beliefs"][0]["result"]["items"][0]["state"],
        "rejected"
    );
    assert_eq!(
        before["beliefs"][1]["result"]["items"][0]["statement"],
        "The learned magic explanation"
    );
    let changed = edit(&w, &id("assertion", 1), |data| {
        data["subject"] = serde_yaml::to_value(rr(&id("concept", 3))).unwrap();
        data["value"] = serde_yaml::Value::String("A different current canonical account".into());
        data["status"] = serde_yaml::Value::String("draft".into());
        data["valid"] = serde_yaml::to_value(interval(20, 30)).unwrap();
    });
    valid(&changed);
    assert_eq!(compose(&changed, &targets, &at(6, 0)), before);
    assert_eq!(
        compose(&w, &targets, &at(8, 0))["beliefs"][0]["result"]["count"],
        0
    );
    assert_eq!(
        compose(&w, &targets, &at(10, 0))["beliefs"][0]["result"]["items"][0]["state"],
        "remembered"
    );
    no_author_material(&before);
}

#[test]
fn sections_apply_own_scopes_corrections_and_inclusive_exact_times() {
    let w = base();
    let mut previous = section(1, "Original dated words");
    previous["valid"] = json!({"kind":"timeless"});
    let mut replacement = section(2, "Corrected dated words");
    replacement["valid"] = json!({"kind":"timeless"});
    replacement["replaces"] = json!(id("description", 1));
    let mut private = section(3, "PRIVATE_SECTION");
    private["audience"] = json!(["private"]);
    let w = set(
        &w,
        &id("concept", 1),
        "descriptions",
        json!([previous, replacement, private]),
    );
    valid(&w);
    let value = compose(&w, &[], &at(20, 0));
    assert_eq!(
        value["sections"][1]["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        value["sections"][1]["result"]["items"][0]["markdown"],
        "Corrected dated words"
    );
    assert_eq!(
        value["sections"][1]["result"]["items"][0]["replaces"],
        id("description", 1)
    );
    assert_eq!(value["sections"][0]["result"]["count"], 1);
    assert_eq!(
        compose(&w, &[], &at(20, 1))["sections"][0]["result"]["count"],
        0
    );
    no_author_material(&value);
}

#[test]
fn affected_lists_count_only_independently_admitted_variations_and_never_grant_them() {
    let w = base();
    let before = species::character_affected(
        &w,
        VIEWER,
        &id("concept", 1),
        &species_selection(),
        &at(6, 0),
        1,
    )
    .unwrap();
    assert_eq!(before["count"], 1);
    let mut records = w.records.values().cloned().collect::<Vec<_>>();
    for n in 20..120 {
        records.push(concept(
            n,
            "individual-variation",
            json!({"traits":"Unlearned variation","common":rr(&id("concept",1))}),
        ));
    }
    let w = from(records);
    valid(&w);
    assert_eq!(
        species::character_affected(
            &w,
            VIEWER,
            &id("concept", 1),
            &species_selection(),
            &at(6, 0),
            1
        )
        .unwrap(),
        before
    );
    let w = add(&w, acquisition(20, &id("concept", 20)));
    assert!(matches!(
        species::character_affected(
            &w,
            VIEWER,
            &id("concept", 1),
            &species_selection(),
            &at(6, 0),
            1
        ),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        species::character_affected(
            &w,
            VIEWER,
            &id("concept", 1),
            &species_selection(),
            &at(6, 0),
            2
        )
        .unwrap()["count"],
        2
    );
    no_author_material(&before);
}

#[test]
fn admitted_cycles_refuse_complete_results_while_unlearned_cycles_supply_no_edges() {
    let w = base();
    assert_eq!(compose(&w, &[], &at(6, 0))["linkageComplete"], true);
    let w = add(&w, acquisition(3, &id("concept", 3)));
    valid(&w);
    assert!(matches!(
        species::character_compose(
            &w,
            VIEWER,
            &id("concept", 2),
            &species_selection(),
            &[],
            &at(6, 0)
        ),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        species::character_affected(
            &w,
            VIEWER,
            &id("concept", 1),
            &species_selection(),
            &at(6, 0),
            100
        ),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn large_coordinates_remain_exact_and_different_timelines_supply_no_learning() {
    let tick = 9_007_199_254_740_993;
    let w = set(&base(), &id("concept", 1), "valid", interval(tick, tick));
    valid(&w);
    let value = compose(&w, &[], &at(tick, 0));
    assert_eq!(value["effectiveTime"]["tick"], tick.to_string());
    assert_eq!(value["common"].as_array().unwrap().len(), 2);
    assert_eq!(compose(&w, &[], &at(tick, 1))["common"], json!([]));
    let other = wedl::model::StoryTime::new("other", tick, 0).unwrap();
    assert!(matches!(
        species::character_compose(
            &w,
            VIEWER,
            &id("concept", 2),
            &species_selection(),
            &[],
            &other
        ),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn author_compositions_attach_only_native_admitted_evidence_for_returned_owners() {
    let w = set(&base(), &id("concept", 3), "audience", json!(["private"]));
    valid(&w);
    let value = species::compose(
        &w,
        &id("concept", 2),
        &species_selection(),
        Some(&at(6, 0)),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(
        value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["ownerId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![id("concept", 2), id("concept", 1), id("concept", 4)]
    );
    assert_eq!(
        value["canon"][1]["result"]["items"][0]["value"],
        "CANONICAL_TRUE"
    );
    assert_eq!(
        value["canon"][1]["result"]["items"][0]["citation"]["entityId"],
        id("assertion", 1)
    );
    assert_eq!(
        value["sections"][1]["result"]["items"][0]["markdown"],
        "Explicit section 1\r\n"
    );
    let affected = species::affected(
        &w,
        &id("concept", 1),
        &species_selection(),
        Some(&at(6, 0)),
        "author",
        "author",
        100,
    )
    .unwrap();
    assert_eq!(affected["sections"].as_array().unwrap().len(), 2);
    assert!(
        !affected["sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|stream| stream["ownerId"] == id("concept", 4))
    );
    let history = species::compose(
        &w,
        &id("concept", 2),
        &species_selection(),
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
fn complete_character_report_and_evidence_bounds_filter_hidden_material_first() {
    let w = base();
    let huge = edit(&w, &id("concept", 2), |data| {
        data["values"]["traits"] = serde_yaml::Value::String("x".repeat(262_144))
    });
    valid(&huge);
    let error = species::character_compose(
        &huge,
        VIEWER,
        &id("concept", 2),
        &species_selection(),
        &[],
        &at(6, 0),
    )
    .unwrap_err();
    assert!(error.to_string().contains("262144"));
    let mut large = w;
    for n in [1, 2, 4] {
        large = set(
            &large,
            &id("concept", n),
            "descriptions",
            json!((1..=171).map(|n| section(n, "tiny")).collect::<Vec<_>>()),
        );
    }
    valid(&large);
    let error = species::character_compose(
        &large,
        VIEWER,
        &id("concept", 2),
        &species_selection(),
        &[],
        &at(6, 0),
    )
    .unwrap_err();
    assert!(error.to_string().contains("512"));
    for n in [1, 2, 4] {
        large = edit(&large, &id("concept", n), |data| {
            for item in data["descriptions"].as_sequence_mut().unwrap() {
                item["audience"] = serde_yaml::to_value(json!(["private"])).unwrap();
            }
        });
    }
    valid(&large);
    let value = compose(&large, &[], &at(6, 0));
    assert!(
        value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["result"]["count"] == 0)
    );
}

#[test]
fn selected_beliefs_and_captured_source_use_closed_coherent_inputs() {
    let w = base();
    for targets in [
        vec![id("concept", 1)],
        vec![id("assertion", 1); 2],
        (1..=9).map(|n| id("assertion", n)).collect(),
    ] {
        assert!(matches!(
            species::character_compose(
                &w,
                VIEWER,
                &id("concept", 2),
                &species_selection(),
                &targets,
                &at(6, 0)
            ),
            Err(Error::InvalidInput(_))
        ));
    }
    let unknown = compose(&w, &[id("assertion", 99)], &at(6, 0));
    assert_eq!(unknown["beliefs"][0]["result"]["count"], 0);
    let bytes = w
        .records
        .values()
        .map(|r| r.raw_bytes.clone())
        .collect::<Vec<_>>();
    compose(&w, &[id("assertion", 1)], &at(6, 0));
    assert_eq!(
        w.records
            .values()
            .map(|r| r.raw_bytes.clone())
            .collect::<Vec<_>>(),
        bytes
    );
    let mut invalid = w.clone();
    invalid
        .records
        .get_mut(&id("concept", 2))
        .unwrap()
        .frontmatter["title"] = serde_yaml::Value::String("mismatched source".into());
    assert!(matches!(
        species::character_compose(
            &invalid,
            VIEWER,
            &id("concept", 2),
            &species_selection(),
            &[],
            &at(6, 0)
        ),
        Err(Error::Validation(_))
    ));
}
