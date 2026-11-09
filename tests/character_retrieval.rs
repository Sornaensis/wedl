//! Local retrieval scores only admitted evidence in a separate public basis.
#[path = "support/retrieval.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use wedl::{character, model::World, retrieval::Mode};

fn search(world: &World, query: &str, mode: Mode, limit: usize) -> Json {
    character::search_with_mode(world, VIEWER, query, &horizon(), limit, mode).unwrap()
}
fn identity(item: &Json) -> String {
    format!("{}:{}:{}", item["kind"], item["recordId"], item["memberId"])
}
fn scores(value: &Json) -> BTreeMap<String, Json> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| (identity(item), item["semanticScore"].clone()))
        .collect()
}

#[test]
fn explicit_lexical_mode_preserves_existing_result_bytes() {
    let world = world();
    for query in [
        "ferry",
        "messenger river",
        "vessel",
        "ferry OR PRIVATE_BEAT",
        "\"ferry\"",
        "!!!",
    ] {
        for limit in [1, 20, 100] {
            let before = character::search(&world, VIEWER, query, &horizon(), limit).unwrap();
            let after = search(&world, query, Mode::Lexical, limit);
            assert_eq!(
                serde_json::to_vec(&before).unwrap(),
                serde_json::to_vec(&after).unwrap()
            );
            assert!(after.get("mode").is_none());
            assert!(after.get("retrieval").is_none());
        }
    }
}

#[test]
fn semantic_queries_recall_only_existing_admitted_targets() {
    let world = world();
    assert_eq!(search(&world, "vessel", Mode::Lexical, 100)["total"], 0);
    let result = search(&world, "vessel", Mode::Semantic, 100);
    assert!(result["total"].as_u64().unwrap() > 0);
    assert_eq!(result["retrieval"]["provider"], "lsa");
    assert_eq!(result["retrieval"]["projection"], "truncated-svd");
    for item in result["items"].as_array().unwrap() {
        assert!(
            [KNOWLEDGE, LINKED_KNOWLEDGE, VIEWER, SCENE, CONVERSATION]
                .contains(&item["recordId"].as_str().unwrap())
        );
        assert!(!item["value"].to_string().contains("vessel"));
        assert!(!item["snippet"].as_str().unwrap().contains("vessel"));
        assert!(item["lexicalRank"].is_null());
        let score = item["semanticScore"].as_f64().unwrap();
        assert!(score.is_finite() && (-1.000000001..=1.000000001).contains(&score));
        assert_eq!(item["score"], item["semanticScore"]);
    }
    let encoded = result["items"].to_string();
    for secret in [
        "AUTHOR_",
        "PRIVATE_",
        "FUTURE_",
        "sourcePath",
        "blobOid",
        "classification",
        "fingerprint",
        "revision",
    ] {
        assert!(!encoded.contains(secret), "{secret}: {encoded}");
    }
}

#[test]
fn all_new_modes_are_fully_invariant_under_private_future_and_metadata_edits() {
    let before = world();
    let after = private_edit(&before);
    assert_ne!(before.fingerprint, after.fingerprint);
    for mode in [Mode::Semantic, Mode::Hybrid] {
        for query in [
            "ferry",
            "vessel",
            "messenger",
            "PRIVATE_CHANGED",
            "FUTURE_BEAT_CHANGED",
            "AUTHOR_BODY_PRIVATE",
        ] {
            for limit in [1, 100] {
                assert_eq!(
                    search(&before, query, mode, limit),
                    search(&after, query, mode, limit),
                    "{mode}: {query}"
                );
            }
        }
    }
}

#[test]
fn limit_prefixes_scores_and_full_lane_ranks_are_stable() {
    let world = world();
    for mode in [Mode::Semantic, Mode::Hybrid] {
        for query in ["ferry", "vessel", "courier"] {
            let full = search(&world, query, mode, 100);
            let small = search(&world, query, mode, 1);
            assert_eq!(small["total"], full["total"]);
            assert_eq!(small["retrieval"], full["retrieval"]);
            assert_eq!(
                small["items"],
                json!(
                    full["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .take(1)
                        .collect::<Vec<_>>()
                )
            );
        }
    }
    let lexical = search(&world, "ferry", Mode::Lexical, 100);
    let hybrid = search(&world, "ferry", Mode::Hybrid, 100);
    for (index, item) in lexical["items"].as_array().unwrap().iter().enumerate() {
        let hit = hybrid["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|hit| identity(hit) == identity(item))
            .unwrap();
        assert_eq!(hit["lexicalRank"], index + 1);
        assert!(
            (hit["lexicalScore"].as_f64().unwrap() + item["score"].as_f64().unwrap()).abs() < 1e-12
        );
    }
    let operators = search(&world, "ferry OR vessel", Mode::Hybrid, 100);
    assert!(
        operators["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["lexicalRank"].is_null())
    );
}

#[test]
fn admitted_targets_and_current_character_sections_never_fit_the_basis() {
    let source = world();
    let before = search(&source, "ferry", Mode::Semantic, 100);
    let changed = edit(&source, VIEWER, |fields| {
        fields["descriptions"].as_sequence_mut().unwrap().push(
            serde_yaml::to_value(section(
                100,
                "Vessel ferry",
                "Ferry vessel messenger courier ferry vessel",
            ))
            .unwrap(),
        )
    });
    let changed = add(
        &changed,
        record(
            "knowledge",
            &id("know", 20),
            json!({"knower":VIEWER,"claim":{"key":"new","statement":"Ferry ferry ferry vessel"},
        "transitions":[{"id":"learned","time":point(TICK-1,0),"state":"accepted","confidence":1.0,"acquisition":"heard"}]}),
        ),
    );
    valid(&changed);
    let after = search(&changed, "ferry", Mode::Semantic, 100);
    assert_eq!(before["retrieval"], after["retrieval"]);
    let after_scores = scores(&after);
    for (key, score) in scores(&before) {
        assert_eq!(after_scores[&key], score, "{key}");
    }
    assert_eq!(
        after["total"].as_u64().unwrap(),
        before["total"].as_u64().unwrap() + 2
    );
}

#[test]
fn missing_prior_is_unavailable_even_with_zero_targets_and_never_falls_back() {
    let source = world();
    let absent = without_prior(&source);
    valid(&absent);
    assert!(
        search(&absent, "ferry", Mode::Lexical, 100)["total"]
            .as_u64()
            .unwrap()
            > 0
    );
    let mut empty = from_records(
        absent
            .records
            .values()
            .filter(|r| r.id == WORLD || r.id == VIEWER)
            .cloned()
            .collect(),
    );
    empty = edit(&empty, VIEWER, |fields| {
        fields.as_mapping_mut().unwrap().remove("descriptions");
    });
    valid(&empty);
    for world in [&absent, &empty] {
        for mode in [Mode::Semantic, Mode::Hybrid] {
            let error = character::search_with_mode(world, VIEWER, "ferry", &horizon(), 100, mode)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("eligible public timeless description prior"),
                "{error}"
            );
        }
    }
}

#[test]
fn a_public_prior_with_no_targets_and_zero_projections_yields_honest_empty_results() {
    let source = world();
    let empty = from_records(
        source
            .records
            .values()
            .filter(|r| r.id == WORLD || r.id == VIEWER)
            .cloned()
            .collect(),
    );
    let empty = edit(&empty, VIEWER, |fields| {
        fields.as_mapping_mut().unwrap().remove("descriptions");
    });
    valid(&empty);
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let result = search(&empty, "ferry", mode, 100);
        assert_eq!(result["total"], 0);
        assert_eq!(result["items"], json!([]));
        assert!(result["retrieval"]["dimensions"].as_u64().unwrap() > 0);
        for query in ["zzqxvbytnr", "!!!"] {
            let result = search(&source, query, mode, 100);
            assert_eq!(result["total"], 0);
            assert_eq!(result["items"], json!([]));
        }
    }
    let featureless = without_prior(&source);
    let featureless = set(
        &featureless,
        WORLD,
        "descriptions",
        json!([section(500, "!!!", "...")]),
    );
    valid(&featureless);
    assert_eq!(
        search(&featureless, "ferry", Mode::Semantic, 100)["total"],
        0
    );
    assert!(
        search(&featureless, "ferry", Mode::Hybrid, 100)["total"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn prior_requires_explicit_public_timeless_sections_and_unqualified_eligible_owners() {
    let source = without_prior(&world());
    let base = section(500, "River travel", "A ferry vessel crosses water.");
    for (field, value) in [
        ("audience", json!([VIEWER])),
        ("perspectives", json!([VIEWER])),
        ("status", json!("retired")),
        ("valid", json!({"kind":"timeless"})),
        (
            "time",
            json!({"start":point(TICK-1,0),"end":point(TICK+1,0)}),
        ),
    ] {
        let candidate = set(&source, OBJECT, "descriptions", json!([base]));
        let candidate = set(&candidate, OBJECT, field, value);
        valid(&candidate);
        assert!(
            character::search_with_mode(
                &candidate,
                VIEWER,
                "ferry",
                &horizon(),
                100,
                Mode::Semantic
            )
            .is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("audience", json!([VIEWER])),
        ("perspectives", json!([VIEWER])),
        ("valid", interval(TICK - 5, TICK + 5)),
        ("valid", interval(TICK + 1, TICK + 5)),
    ] {
        let mut section = base.clone();
        section[field] = value;
        let candidate = set(&source, OBJECT, "descriptions", json!([section]));
        valid(&candidate);
        assert!(
            character::search_with_mode(
                &candidate,
                VIEWER,
                "ferry",
                &horizon(),
                100,
                Mode::Semantic
            )
            .is_err(),
            "{field}"
        );
    }
    let character_only = set(&source, VIEWER, "descriptions", json!([base]));
    valid(&character_only);
    assert!(
        character::search_with_mode(
            &character_only,
            VIEWER,
            "ferry",
            &horizon(),
            100,
            Mode::Semantic
        )
        .is_err()
    );
}

#[test]
fn eligible_public_prior_corrections_suppress_predecessors_before_training() {
    let source = without_prior(&world());
    let old = section(500, "Quasar", "quasar quasar");
    let mut replacement = section(501, "Ferry vessel", "A ferry vessel crosses water.");
    replacement["replaces"] = json!(id("description", 500));
    let corrected = set(&source, WORLD, "descriptions", json!([old, replacement]));
    valid(&corrected);
    let changed = edit(&corrected, WORLD, |data| {
        data["descriptions"][0]["markdown"] =
            serde_yaml::to_value("Changed superseded public source ferry vessel").unwrap()
    });
    valid(&changed);
    assert_eq!(
        search(&corrected, "quasar", Mode::Semantic, 100)["total"],
        0
    );
    for mode in [Mode::Semantic, Mode::Hybrid] {
        for query in ["quasar", "ferry", "vessel"] {
            assert_eq!(
                search(&corrected, query, mode, 100),
                search(&changed, query, mode, 100)
            );
        }
    }
}

#[test]
fn learned_wrong_words_rejection_and_forgetting_keep_the_existing_projection() {
    let world = world();
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let current = search(&world, "ferry", mode, 100);
        let linked = current["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["kind"] == "assertion-belief")
            .unwrap();
        assert_eq!(linked["value"]["state"], "rejected");
        assert_eq!(
            linked["value"]["statement"],
            "A messenger promised the ferry sails at night."
        );
        assert!(!linked.to_string().contains("AUTHOR_CANON_VALUE"));
        let forgotten =
            character::search_with_mode(&world, VIEWER, "ferry", &at(TICK + 2, 0), 100, mode)
                .unwrap();
        assert!(
            forgotten["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["recordId"] != LINKED_KNOWLEDGE)
        );
        let memory = current["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["kind"] == "recollection")
            .unwrap();
        assert_eq!(
            memory["value"]["rememberedQuotes"][0]["text"],
            "I recall a quiet promise."
        );
        assert!(
            memory["value"]["rememberedQuotes"][0]
                .get("sourceTurnId")
                .is_none()
        );
    }
}

#[test]
fn exact_time_and_native_description_corrections_precede_candidate_ranking() {
    let world = world();
    let mut replacement = section(
        22,
        "Ferry habit",
        "I wait for the ferry by a different gate.",
    );
    replacement["valid"] = interval(TICK - 10, TICK + 10);
    replacement["replaces"] = json!(id("description", 20));
    let world = edit(&world, VIEWER, |data| {
        data["descriptions"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::to_value(replacement).unwrap())
    });
    valid(&world);
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let exact = search(&world, "ferry", mode, 100);
        assert_eq!(exact["effectiveTime"]["tick"], TICK.to_string());
        assert!(
            exact["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["kind"] == "observation")
        );
        assert!(
            exact["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["memberId"] != id("description", 20))
        );
        assert!(
            exact["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["memberId"] == id("description", 22))
        );
        let later =
            character::search_with_mode(&world, VIEWER, "ferry", &at(TICK, 1), 100, mode).unwrap();
        assert!(
            later["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["kind"] != "observation")
        );
        assert_eq!(exact["retrieval"], later["retrieval"]);
        let branch = wedl::model::StoryTime::new("branch", TICK, 0).unwrap();
        let other =
            character::search_with_mode(&world, VIEWER, "ferry", &branch, 100, mode).unwrap();
        assert_eq!(other["total"], 0);
        assert_eq!(other["retrieval"], exact["retrieval"]);
    }
}

#[test]
fn complete_input_bounds_count_only_admitted_targets_and_eligible_prior() {
    let source = world();
    let huge = section(500, "Private text", &"x".repeat(524_288));
    let private = set(&source, PRIVATE_OBJECT, "descriptions", json!([huge]));
    valid(&private);
    for mode in [Mode::Semantic, Mode::Hybrid] {
        assert_eq!(
            search(&source, "ferry", mode, 100),
            search(&private, "ferry", mode, 100)
        );
    }
    let huge = section(500, "Public prior", &"é".repeat(262_144));
    let oversized = set(&source, OBJECT, "descriptions", json!([huge]));
    valid(&oversized);
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let error =
            character::search_with_mode(&oversized, VIEWER, "zzqxvbytnr", &horizon(), 1, mode)
                .unwrap_err()
                .to_string();
        assert!(error.contains("complete input or work bounds"), "{error}");
    }
    let too_many = set(
        &source,
        WORLD,
        "descriptions",
        json!(
            (1000..1512)
                .map(|n| section(n, "Ferry vessel", "Ferry vessel across water."))
                .collect::<Vec<_>>()
        ),
    );
    let too_many = set(
        &too_many,
        PLACE,
        "descriptions",
        json!(
            (2000..2512)
                .map(|n| section(n, "Ferry vessel", "Ferry vessel across water."))
                .collect::<Vec<_>>()
        ),
    );
    valid(&too_many);
    assert!(
        character::search_with_mode(
            &too_many,
            VIEWER,
            "zzqxvbytnr",
            &horizon(),
            1,
            Mode::Semantic
        )
        .unwrap_err()
        .to_string()
        .contains("complete input or work bounds")
    );
}

#[test]
fn retrieval_reads_preserve_source_and_write_no_persistent_index() {
    let mut world = world();
    let directory = save(&world);
    world.root = directory.path().into();
    let fingerprint = world.fingerprint.clone();
    let before = world
        .records
        .iter()
        .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
        .collect::<Vec<_>>();
    for mode in [Mode::Semantic, Mode::Hybrid] {
        search(&world, "ferry", mode, 100);
    }
    assert_eq!(world.fingerprint, fingerprint);
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
            .collect::<Vec<_>>(),
        before
    );
    assert!(!directory.path().join(".wedl-rust").exists());
}
