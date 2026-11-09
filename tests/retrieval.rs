use serde_json::Value;
use std::{fs, path::Path, str::FromStr};
use wedl::{
    compiler,
    model::{Record, World},
    retrieval::Mode,
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PIER: &str = "obj_00000000000000000000000001";
const SHIP: &str = "obj_00000000000000000000000002";
const LIBRARY: &str = "obj_00000000000000000000000003";
const PLACE: &str = "loc_00000000000000000000000004";
const HYPOTHESIS: &str = "hyp_00000000000000000000000005";

fn record(id: &str, kind: &str, title: &str, body: &str, extra: &str) -> Record {
    let status = if kind == "hypothesis" {
        "open"
    } else {
        "canonical"
    };
    let text = format!(
        "---\nschema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {title}\nstatus: {status}\naliases: [MetadataAliasOnly]\ntags: [metadatatagonly]\n{extra}---\n{body}"
    );
    source::parse_record(text.as_bytes(), &format!("story/{id}.md")).unwrap()
}

fn fixture(root: &Path) -> World {
    let records = vec![
        record(
            WORLD,
            "world",
            "Corpus",
            "",
            "capabilities: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nthreads: []\n",
        ),
        record(PIER, "object", "Pier", "harbor pier vessel", ""),
        record(SHIP, "object", "Ship", "ship vessel harbor", ""),
        record(LIBRARY, "object", "Library", "library book scroll", ""),
        record(
            PLACE,
            "location",
            "Hidden elsewhere",
            "unrelated location words",
            "",
        ),
        record(
            HYPOTHESIS,
            "hypothesis",
            "Author reasoning",
            "hypothesisonly sentinel",
            "subjects: [obj_00000000000000000000000001]\nstatement: An authored possibility.\ncontext: Author reasoning.\nalternatives: [One, Two]\nplacement: {context: Author reasoning.}\n",
        ),
    ];
    let world = source::world_from_records(root.to_path_buf(), records, None).unwrap();
    let validation = validation::validate_world(&world);
    assert!(validation.valid, "{validation:?}");
    world
}

fn changed(world: &World, id: &str, body: &str) -> World {
    let old = &world.records[id];
    let source_text = format!(
        "---\n{}---\n{body}",
        serde_yaml::to_string(&old.frontmatter).unwrap()
    );
    let replacement = source::parse_record(source_text.as_bytes(), &old.source_path).unwrap();
    let records = world
        .records
        .values()
        .map(|record| {
            if record.id == id {
                replacement.clone()
            } else {
                record.clone()
            }
        })
        .collect();
    source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap()
}

fn search(world: &World, query: &str, mode: Mode, limit: usize) -> Value {
    compiler::search_with_mode(world, query, limit, Some("object"), mode).unwrap()
}

#[test]
fn mode_names_are_exact_and_legacy_lexical_results_are_unchanged() {
    assert_eq!(Mode::default(), Mode::Lexical);
    for (text, mode) in [
        ("lexical", Mode::Lexical),
        ("semantic", Mode::Semantic),
        ("hybrid", Mode::Hybrid),
    ] {
        assert_eq!(Mode::from_str(text).unwrap(), mode);
        assert_eq!(mode.as_str(), text);
    }
    for invalid in ["", "fts", "vector", "Semantic", " hybrid", "hybrid "] {
        assert!(Mode::from_str(invalid).is_err());
    }
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    assert_eq!(
        search(&world, "ship", Mode::Lexical, 10),
        compiler::search(&world, "ship", 10, Some("object")).unwrap()
    );
}

#[test]
fn semantic_and_hybrid_expose_real_nonliteral_recall_and_lane_scores() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let semantic = search(&world, "ship", Mode::Semantic, 10);
    assert_eq!(semantic["mode"], "semantic");
    assert_eq!(semantic["retrieval"]["provider"], "lsa");
    assert_eq!(semantic["retrieval"]["solver"], "gram-jacobi");
    assert_eq!(semantic["retrieval"]["dimensions"], 2);
    let pier = semantic["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == PIER)
        .unwrap();
    assert!(pier["semanticScore"].as_f64().unwrap() > 0.8);
    assert!(pier["lexicalRank"].is_null());
    assert!(!pier["snippet"].as_str().unwrap().contains("ship"));
    let hybrid = search(&world, "ship", Mode::Hybrid, 10);
    let ship = hybrid["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == SHIP)
        .unwrap();
    let semantic_rank = ship["semanticRank"].as_f64().unwrap();
    assert_eq!(
        ship["score"].as_f64().unwrap(),
        1.0 / 61.0 + 1.0 / (60.0 + semantic_rank)
    );
    assert!(ship["lexicalScore"].as_f64().unwrap() > 0.0);
    assert!(
        hybrid["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == PIER && item["lexicalRank"].is_null())
    );
    assert!(!directory.path().join(".wedl-rust").exists());
}

#[test]
fn kind_admission_and_hypothesis_exclusion_precede_fit_and_input_bounds() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let before = search(&world, "ship", Mode::Hybrid, 100);
    let with_other_kind = changed(&world, PLACE, &"excludedkind ".repeat(45_000));
    let after = search(&with_other_kind, "ship", Mode::Hybrid, 100);
    assert_eq!(before["items"], after["items"]);
    assert_eq!(before["retrieval"], after["retrieval"]);
    let all = compiler::search_with_mode(&world, "ship", 100, None, Mode::Semantic).unwrap();
    let with_hypothesis = changed(&world, HYPOTHESIS, &"hypothesisonly ".repeat(40_000));
    let after =
        compiler::search_with_mode(&with_hypothesis, "ship", 100, None, Mode::Semantic).unwrap();
    assert_eq!(all["items"], after["items"]);
    assert_eq!(all["retrieval"], after["retrieval"]);
    assert!(
        compiler::search_with_mode(&with_hypothesis, "hypothesisonly", 100, None, Mode::Hybrid)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        compiler::search_with_mode(&world, "ship", 100, Some("hypothesis"), Mode::Semantic)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn limit_and_repeat_requests_preserve_order_scores_and_do_not_touch_index() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let report = compiler::compile(&world).unwrap();
    let original_index = fs::read(&report.path).unwrap();
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let full = search(&world, "ship", mode, 100);
        let limited = search(&world, "ship", mode, 1);
        assert_eq!(limited["items"][0], full["items"][0]);
        assert_eq!(limited["total"], full["total"]);
        assert_eq!(full, search(&world, "ship", mode, 100));
    }
    assert_eq!(fs::read(&report.path).unwrap(), original_index);
    assert_eq!(
        fs::read_dir(report.path.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn out_of_vocabulary_queries_and_empty_kind_have_honest_empty_results() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for mode in [Mode::Semantic, Mode::Hybrid] {
        for query in [
            "unseenneedle",
            "MetadataAliasOnly",
            "metadatatagonly",
            "the and",
        ] {
            assert!(
                search(&world, query, mode, 100)["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        let result = compiler::search_with_mode(&world, "ship", 10, Some("event"), mode).unwrap();
        assert_eq!(result["total"], 0);
        assert_eq!(result["retrieval"]["dimensions"], 0);
    }
}

#[test]
fn semantic_requests_obey_complete_byte_and_query_limits_without_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let large = changed(&world, PIER, &"vessel ".repeat(40_000));
    for mode in [Mode::Semantic, Mode::Hybrid] {
        assert!(
            compiler::search_with_mode(&large, "unseenneedle", 1, Some("object"), mode).is_err()
        );
        for (query, limit) in [("", 1), ("ship", 0), ("ship", 101)] {
            assert!(compiler::search_with_mode(&world, query, limit, None, mode).is_err());
        }
        assert!(compiler::search_with_mode(&world, &"x".repeat(4097), 1, None, mode).is_err());
        assert!(compiler::search_with_mode(&world, &"x ".repeat(65), 1, None, mode).is_err());
    }
    assert!(!directory.path().join(".wedl-rust").exists());
}

#[test]
fn invalid_source_is_rejected_before_either_new_lane() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = fixture(directory.path());
    world.records.get_mut(PIER).unwrap().title = "out of sync with source".into();
    for mode in [Mode::Semantic, Mode::Hybrid] {
        assert!(matches!(
            compiler::search_with_mode(&world, "ship", 10, Some("object"), mode),
            Err(wedl::Error::Validation(_))
        ));
    }
    assert!(!directory.path().join(".wedl-rust").exists());
}
