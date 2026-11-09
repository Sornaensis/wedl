//! Author horizon admission and exact grouping remain independent of ranking.
use serde_json::{Value, json};
use std::{fs, path::Path};
use wedl::{
    compiler::{self, AuthorSearchSelection},
    model::{Record, StoryTime, World},
    retrieval::Mode,
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PIER: &str = "obj_00000000000000000000000001";
const SHIP: &str = "obj_00000000000000000000000002";
const LIBRARY: &str = "obj_00000000000000000000000003";
const PLACE: &str = "loc_00000000000000000000000004";
const EVENT: &str = "event_00000000000000000000000005";
const HERO: &str = "char_00000000000000000000000006";
const KNOWLEDGE: &str = "know_00000000000000000000000007";
const HYPOTHESIS: &str = "hyp_00000000000000000000000008";
const T1: &str = "thread_00000000000000000000000001";
const T2: &str = "thread_00000000000000000000000002";
const T3: &str = "thread_00000000000000000000000003";
const MODES: [Mode; 3] = [Mode::Lexical, Mode::Semantic, Mode::Hybrid];

fn record(kind: &str, id: &str, title: &str, body: &str, extra: Value) -> Record {
    let mut fields = json!({"schema":"wedl/v1","id":id,"kind":kind,"title":title});
    fields
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let text = format!(
        "---\n{}---\n{body}",
        serde_yaml::to_string(&fields).unwrap()
    );
    source::parse_record(text.as_bytes(), &format!("story/{id}.md")).unwrap()
}

fn fixture(root: &Path) -> World {
    let records = vec![
        record(
            "world",
            WORLD,
            "Corpus",
            "",
            json!({
                "capabilities":["descriptions-v1"],"default_timeline":"main",
                "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
                "threads":[{"id":T1,"label":"First"},{"id":T2,"label":"Second"},{"id":T3,"label":"Empty"}]
            }),
        ),
        record(
            "object",
            PIER,
            "Pier",
            "harbor pier vessel",
            json!({"threads":[T1]}),
        ),
        record(
            "object",
            SHIP,
            "Ship",
            "ship vessel harbor",
            json!({"threads":[T2]}),
        ),
        record(
            "object",
            LIBRARY,
            "Library",
            "library book scroll",
            json!({"threads":[T1,T2]}),
        ),
        record(
            "location",
            PLACE,
            "Elsewhere",
            "unrelated location prose",
            json!({}),
        ),
        record(
            "event",
            EVENT,
            "Dated identity",
            "futurebody ship",
            json!({"time":{"tick":20},"threads":[T1]}),
        ),
        record(
            "character",
            HERO,
            "Witness",
            "",
            json!({"initial_state":{}}),
        ),
        record(
            "hypothesis",
            HYPOTHESIS,
            "Possibility",
            "hypothesisonly ship",
            json!({"status":"open","subjects":[PIER],"statement":"An authored possibility.",
                "context":"Author reasoning.","alternatives":["One","Two"],
                "placement":{"context":"Author reasoning."}}),
        ),
    ];
    world(root, records)
}

fn world(root: &Path, records: Vec<Record>) -> World {
    let world = source::world_from_records(root.to_path_buf(), records, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn replace(base: &World, id: &str, body: Option<&str>, edit: impl FnOnce(&mut Value)) -> World {
    let old = &base.records[id];
    let mut fields = serde_json::to_value(&old.frontmatter).unwrap();
    edit(&mut fields);
    let text = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        body.unwrap_or(&old.body)
    );
    let replacement = source::parse_record(text.as_bytes(), &old.source_path).unwrap();
    let records = base
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
    world(&base.root, records)
}

fn add(base: &World, record: Record) -> World {
    let mut records: Vec<_> = base.records.values().cloned().collect();
    records.push(record);
    world(&base.root, records)
}

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}

fn selection(at: Option<StoryTime>, threads: Option<&[&str]>) -> AuthorSearchSelection {
    AuthorSearchSelection {
        at,
        thread_ids: threads.map(|values| values.iter().map(|value| (*value).to_owned()).collect()),
    }
}

fn search(
    world: &World,
    query: &str,
    kind: Option<&str>,
    mode: Mode,
    selection: &AuthorSearchSelection,
    limit: usize,
) -> Value {
    compiler::search_with_selection(world, query, limit, kind, mode, selection).unwrap()
}

fn section(n: u32, markdown: &str, valid: Value, audience: &[&str], views: &[&str]) -> Value {
    json!({"id":format!("description_{n:026}"),"heading":format!("Section {n}"),"markdown":markdown,
        "valid":valid,"audience":audience,"perspectives":views})
}

fn ranks(value: &Value) -> Vec<Value> {
    value["items"].as_array().unwrap().iter().map(|item| json!({
        "id":item["id"],"sectionId":item["sectionId"],"snippet":item["snippet"],"score":item["score"],
        "lexicalRank":item["lexicalRank"],"lexicalScore":item["lexicalScore"],
        "semanticRank":item["semanticRank"],"semanticScore":item["semanticScore"]
    })).collect()
}

#[test]
fn omitted_author_selection_preserves_every_existing_mode_exactly() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    for mode in MODES {
        assert_eq!(
            search(
                &base,
                "ship",
                Some("object"),
                mode,
                &AuthorSearchSelection::default(),
                10
            ),
            compiler::search_with_mode(&base, "ship", 10, Some("object"), mode).unwrap()
        );
    }
}

#[test]
fn explicit_horizon_gates_event_body_at_exact_large_tick_and_order_but_keeps_identity() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let tick = 9_007_199_254_740_993;
    let base = replace(
        &base,
        EVENT,
        Some("futurebody futurebody futurebody ship"),
        |fields| fields["time"] = json!({"timeline":"main","tick":tick,"order":2}),
    );
    for mode in MODES {
        let before = selection(Some(at(tick, 1)), None);
        assert!(
            search(&base, "futurebody", Some("event"), mode, &before, 100)["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let identity = search(&base, "dated", Some("event"), mode, &before, 100);
        assert_eq!(identity["items"][0]["id"], EVENT);
        assert_eq!(identity["items"][0]["bodyIncluded"], false);
        assert_eq!(identity["at"]["tick"], tick.to_string());
        assert_eq!(identity["at"]["order"], "1");
        let after = search(
            &base,
            "futurebody",
            Some("event"),
            mode,
            &selection(Some(at(tick, 2)), None),
            100,
        );
        assert_eq!(after["scope"], "author-as-of");
        assert_eq!(after["projection"], "native-author-dated");
        assert_eq!(after["items"][0]["bodyIncluded"], true);
        let other = selection(Some(StoryTime::new("other", tick, 2).unwrap()), None);
        assert!(
            search(&base, "futurebody", Some("event"), mode, &other, 100)["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn future_body_is_absent_from_all_statistics_training_and_byte_budgets() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let huge = replace(
        &base,
        EVENT,
        Some(&"excludedfuture ship ".repeat(40_000)),
        |_| {},
    );
    let selected = selection(Some(at(10, 0)), None);
    for mode in MODES {
        let before = search(&base, "ship", None, mode, &selected, 100);
        let after = search(&huge, "ship", None, mode, &selected, 100);
        assert_eq!(ranks(&before), ranks(&after));
        assert_eq!(before["retrieval"], after["retrieval"]);
        assert_eq!(before["total"], after["total"]);
        assert!(
            compiler::search_with_selection(
                &huge,
                "ship",
                1,
                None,
                mode,
                &selection(Some(at(20, 0)), None)
            )
            .is_err()
        );
    }
    assert!(!directory.path().join(".wedl-rust").exists());
}

#[test]
fn closed_scene_and_conversation_aggregates_start_at_end_and_open_ones_stay_absent() {
    let directory = tempfile::tempdir().unwrap();
    let mut base = fixture(directory.path());
    for (kind, id) in [
        ("scene", "scene_00000000000000000000000001"),
        ("conversation", "conv_00000000000000000000000001"),
    ] {
        let mut fields = json!({"status":"closed","location":PLACE,"participants":[],"time":{"start":{"tick":0},"end":{"tick":10,"order":2}}});
        if kind == "scene" {
            fields["time"]["current"] = json!({"tick":10,"order":2});
            fields["observations"] = json!([]);
        } else {
            fields["turns"] = json!([]);
            fields["recollections"] = json!([]);
        }
        base = add(
            &base,
            record(
                kind,
                id,
                "Aggregate",
                "completedsummary completedsummary completedsummary",
                fields,
            ),
        );
        for mode in MODES {
            assert!(
                search(
                    &base,
                    "completedsummary",
                    Some(kind),
                    mode,
                    &selection(Some(at(10, 1)), None),
                    100
                )["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                search(
                    &base,
                    "completedsummary",
                    Some(kind),
                    mode,
                    &selection(Some(at(10, 2)), None),
                    100
                )["items"][0]["id"],
                id
            );
        }
        let open = replace(&base, id, None, |fields| {
            fields["status"] = json!("active");
            fields["time"].as_object_mut().unwrap().remove("end");
        });
        for mode in MODES {
            assert!(
                search(
                    &open,
                    "completedsummary",
                    Some(kind),
                    mode,
                    &selection(Some(at(100, 0)), None),
                    100
                )["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn environment_body_uses_same_timeline_and_inclusive_interval_endpoints() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let base = add(
        &base,
        record(
            "environment",
            "env_00000000000000000000000001",
            "Weather",
            "intervalweather intervalweather intervalweather",
            json!({
                "time":{"start":{"tick":2,"order":3},"end":{"tick":5,"order":7}},"targets":[PLACE]
            }),
        ),
    );
    for mode in MODES {
        for (point, included) in [
            (at(2, 2), false),
            (at(2, 3), true),
            (at(5, 7), true),
            (at(5, 8), false),
            (StoryTime::new("other", 3, 0).unwrap(), false),
        ] {
            let result = search(
                &base,
                "intervalweather",
                Some("environment"),
                mode,
                &selection(Some(point), None),
                100,
            );
            assert_eq!(!result["items"].as_array().unwrap().is_empty(), included);
        }
    }
}

#[test]
fn authored_transition_bounds_prose_and_independently_dates_transition_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let base = add(
        &base,
        record(
            "knowledge",
            KNOWLEDGE,
            "Learned account",
            "transitionbody transitionbody transitionbody",
            json!({
                "knower":HERO,"claim":{"key":"account","statement":"datedstatement"},
                "transitions":[{"id":"learned","time":{"tick":7,"order":3},"state":"accepted","confidence":0.8,"acquisition":"observed","note":"datednote"}]
            }),
        ),
    );
    for mode in MODES {
        assert!(
            search(
                &base,
                "transitionbody",
                Some("knowledge"),
                mode,
                &selection(Some(at(7, 2)), None),
                100
            )["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            search(
                &base,
                "transitionbody",
                Some("knowledge"),
                mode,
                &selection(Some(at(7, 3)), None),
                100
            )["items"][0]["id"],
            KNOWLEDGE
        );
        for (query, document_kind) in [
            ("datedstatement", "knowledge-claim"),
            ("datednote", "transition"),
        ] {
            assert!(
                search(
                    &base,
                    query,
                    Some("knowledge"),
                    mode,
                    &selection(Some(at(7, 2)), None),
                    100
                )["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            let admitted = search(
                &base,
                query,
                Some("knowledge"),
                mode,
                &selection(Some(at(7, 3)), None),
                100,
            );
            assert!(
                admitted["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["documentKind"] == document_kind)
            );
        }
    }
}

#[test]
fn dated_sections_use_author_union_scope_corrections_and_member_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let timeless = json!({"kind":"timeless"});
    let old = section(
        1,
        "supersededneedle",
        timeless.clone(),
        &["author"],
        &["author"],
    );
    let mut current = section(
        2,
        "currentneedle",
        timeless.clone(),
        &["author"],
        &["author"],
    );
    current["replaces"] = old["id"].clone();
    let public = section(
        3,
        "publicneedle",
        timeless.clone(),
        &["public"],
        &["ordinary"],
    );
    let private = section(4, "privateneedle", timeless.clone(), &[HERO], &[HERO]);
    let future = section(
        5,
        "futureneedle",
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":10,"order":1}}),
        &["author"],
        &["ordinary"],
    );
    let base = replace(&base, PIER, None, |fields| {
        fields["descriptions"] = json!([old, current, public, private, future])
    });
    let selected = selection(Some(at(10, 0)), None);
    for mode in MODES {
        for needle in ["supersededneedle", "privateneedle", "futureneedle"] {
            assert!(
                search(&base, needle, Some("object"), mode, &selected, 100)["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        for (needle, n) in [("currentneedle", 2), ("publicneedle", 3)] {
            let result = search(&base, needle, Some("object"), mode, &selected, 100);
            let item = result["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["sectionId"] == format!("description_{n:026}"))
                .unwrap();
            assert_eq!(item["id"], PIER);
            assert_eq!(item["documentKind"], "description");
            assert_eq!(
                item["citation"]["section"],
                format!("descriptions[{}]", n - 1)
            );
        }
        let later = search(
            &base,
            "futureneedle",
            Some("object"),
            mode,
            &selection(Some(at(10, 1)), None),
            100,
        );
        assert!(
            later["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["sectionId"] == "description_00000000000000000000000005")
        );
    }
}

#[test]
fn excluded_sections_and_owner_scope_do_not_affect_fit_or_input_budget() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let huge = "excludedsection ship ".repeat(35_000);
    let private = section(1, &huge, json!({"kind":"timeless"}), &[HERO], &[HERO]);
    let future = section(
        2,
        &huge,
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":20,"order":0}}),
        &["author"],
        &["author"],
    );
    let changed = replace(&base, PIER, None, |fields| {
        fields["descriptions"] = json!([private, future])
    });
    let selected = selection(Some(at(10, 0)), None);
    for mode in MODES {
        let before = search(&base, "ship", Some("object"), mode, &selected, 100);
        let after = search(&changed, "ship", Some("object"), mode, &selected, 100);
        assert_eq!(ranks(&before), ranks(&after));
        assert_eq!(before["retrieval"], after["retrieval"]);
    }
    let restricted = replace(&base, PLACE, None, |fields| {
        fields["audience"] = json!([HERO])
    });
    let expanded = replace(&restricted, PLACE, Some(&huge), |_| {});
    for mode in MODES {
        assert_eq!(
            ranks(&search(&restricted, "ship", None, mode, &selected, 100)),
            ranks(&search(&expanded, "ship", None, mode, &selected, 100))
        );
    }
}

#[test]
fn exact_kind_and_hypothesis_admission_precede_every_dated_lane() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let huge = "excludedkind ship ".repeat(45_000);
    let changed = replace(
        &replace(&base, PLACE, Some(&huge), |_| {}),
        HYPOTHESIS,
        Some(&huge),
        |_| {},
    );
    let selected = selection(Some(at(10, 0)), None);
    for mode in MODES {
        let before = search(&base, "ship", Some("object"), mode, &selected, 100);
        let after = search(&changed, "ship", Some("object"), mode, &selected, 100);
        assert_eq!(ranks(&before), ranks(&after));
        assert_eq!(before["retrieval"], after["retrieval"]);
        let none = search(&changed, "ship", Some("hypothesis"), mode, &selected, 100);
        assert_eq!(none["total"], 0);
        assert_eq!(none["retrieval"]["dimensions"], 0);
    }
}

#[test]
fn typed_owner_descriptions_keep_owner_applicability_without_admitting_raw_body_or_identity() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let base = replace(&base, WORLD, None, |fields| {
        fields["capabilities"] = json!(["lore-core-v1", "descriptions-v1"]);
        fields["profiles"] = json!([{"id":"prose-owner","label":"Prose owner","fields":{"type":"object","required":{},"optional":{}}}]);
    });
    let owner = "concept_00000000000000000000000001";
    let prose = section(
        1,
        "selecteddescription selecteddescription selecteddescription",
        json!({"kind":"timeless"}),
        &["author"],
        &["author"],
    );
    let base = add(
        &base,
        record(
            "lore-concept",
            owner,
            "Typed identity",
            &"rawtypedbody ".repeat(45_000),
            json!({
                "profile":"prose-owner","domain":"test.prose","values":{},
                "valid":{"kind":"interval","timeline":"main","start":{"timeline":"main","tick":10,"order":0},"end":{"timeline":"main","tick":20,"order":0}},
                "audience":["author"],"perspectives":["author"],"threads":[T1],"descriptions":[prose]
            }),
        ),
    );
    for mode in MODES {
        for time in [at(9, 0), at(20, 1), StoryTime::new("other", 15, 0).unwrap()] {
            let unavailable = search(
                &base,
                "selecteddescription",
                Some("lore-concept"),
                mode,
                &selection(Some(time), None),
                100,
            );
            assert_eq!(unavailable["total"], 0);
            assert_eq!(unavailable["retrieval"]["dimensions"], 0);
        }
        let current = search(
            &base,
            "selecteddescription",
            Some("lore-concept"),
            mode,
            &selection(Some(at(10, 0)), None),
            100,
        );
        assert_eq!(current["items"].as_array().unwrap().len(), 1);
        assert_eq!(current["items"][0]["documentKind"], "description");
        assert_eq!(current["items"][0]["id"], owner);
        assert!(
            search(
                &base,
                "rawtypedbody",
                Some("lore-concept"),
                mode,
                &selection(Some(at(10, 0)), None),
                100
            )["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let grouped = search(
            &base,
            "selecteddescription",
            Some("lore-concept"),
            mode,
            &selection(Some(at(10, 0)), Some(&[T2])),
            100,
        );
        assert_eq!(grouped["total"], 0);
        assert_eq!(grouped["rankedTotal"], current["total"]);
        assert_eq!(grouped["retrieval"], current["retrieval"]);
        let no_sections = replace(&base, owner, None, |fields| {
            fields["descriptions"] = json!([])
        });
        assert_eq!(
            search(
                &no_sections,
                "typed",
                Some("lore-concept"),
                mode,
                &selection(Some(at(10, 0)), None),
                100
            )["total"],
            0
        );
        for status in ["draft", "retired"] {
            let hidden = replace(&base, owner, None, |fields| {
                fields["status"] = json!(status)
            });
            assert_eq!(
                search(
                    &hidden,
                    "selecteddescription",
                    Some("lore-concept"),
                    mode,
                    &selection(Some(at(10, 0)), None),
                    100
                )["total"],
                0
            );
        }
    }
}

#[test]
fn thread_selection_is_a_stable_any_subsequence_with_unchanged_scores_and_training() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    for horizon in [None, Some(at(10, 0))] {
        for mode in MODES {
            let full = search(
                &base,
                "ship",
                Some("object"),
                mode,
                &selection(horizon.clone(), None),
                100,
            );
            for threads in [&[T1][..], &[T2][..], &[T1, T2][..], &[T3][..]] {
                let filtered = search(
                    &base,
                    "ship",
                    Some("object"),
                    mode,
                    &selection(horizon.clone(), Some(threads)),
                    100,
                );
                let expected: Vec<_> = full["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| {
                        let owner = &base.records[item["id"].as_str().unwrap()];
                        owner
                            .list("threads")
                            .unwrap()
                            .iter()
                            .any(|id| threads.contains(&id.as_str().unwrap()))
                    })
                    .cloned()
                    .collect();
                assert_eq!(filtered["items"], json!(expected));
                assert_eq!(filtered["total"], expected.len());
                assert_eq!(filtered["threadIds"], json!(threads));
                assert_eq!(filtered["candidateWindow"], "complete-admitted-corpus");
                assert_eq!(filtered["retrieval"], full["retrieval"]);
                let limited = search(
                    &base,
                    "ship",
                    Some("object"),
                    mode,
                    &selection(horizon.clone(), Some(threads)),
                    1,
                );
                assert_eq!(
                    limited["items"],
                    json!(expected.into_iter().take(1).collect::<Vec<_>>())
                );
                assert_eq!(limited["total"], filtered["total"]);
            }
        }
    }
}

#[test]
fn all_time_thread_lexical_kind_filter_preserves_legacy_global_bm25() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let base = replace(&base, PLACE, Some(&"ship unrelated ".repeat(100)), |_| {});
    let full = compiler::search(&base, "ship", 100, Some("object")).unwrap();
    let filtered = search(
        &base,
        "ship",
        Some("object"),
        Mode::Lexical,
        &selection(None, Some(&[T2])),
        100,
    );
    assert_eq!(filtered["items"], full["items"]);
    assert!(filtered["items"][0]["score"].as_f64().unwrap() < 0.0);
    assert!(filtered["retrieval"].is_null());
}

#[test]
fn grouping_and_small_output_limits_never_hide_complete_corpus_overflow() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let huge = replace(&base, PIER, Some(&"oversized ship ".repeat(45_000)), |_| {});
    for horizon in [None, Some(at(10, 0))] {
        for mode in MODES {
            for selected in [
                selection(horizon.clone(), Some(&[T2])),
                selection(horizon.clone(), Some(&[T3])),
            ] {
                assert!(
                    compiler::search_with_selection(
                        &huge,
                        "unseenquery",
                        1,
                        Some("object"),
                        mode,
                        &selected
                    )
                    .is_err()
                );
            }
        }
    }
}

#[test]
fn thread_ids_require_sorted_unique_declared_exact_values() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    for ids in [
        vec![],
        vec![T2.to_owned(), T1.to_owned()],
        vec![T1.to_owned(), T1.to_owned()],
        vec!["First".into()],
        vec!["thread_00000000000000000000000009".into()],
        vec![T1.to_owned(); 33],
    ] {
        assert!(
            compiler::search_with_selection(
                &base,
                "ship",
                10,
                None,
                Mode::Hybrid,
                &AuthorSearchSelection {
                    at: None,
                    thread_ids: Some(ids)
                }
            )
            .is_err()
        );
    }
    let lower = "thread_0000000000000000000000000a";
    assert!(
        compiler::search_with_selection(
            &base,
            "ship",
            10,
            None,
            Mode::Lexical,
            &selection(None, Some(&[lower]))
        )
        .is_err()
    );
    assert!(
        compiler::search_with_selection(
            &base,
            "ship",
            10,
            None,
            Mode::Hybrid,
            &selection(Some(StoryTime::new("undeclared", 0, 0).unwrap()), None)
        )
        .is_err()
    );
}

#[test]
fn selected_search_reads_snapshot_without_creating_or_modifying_fts_cache() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    for mode in MODES {
        search(
            &base,
            "ship",
            None,
            mode,
            &selection(Some(at(10, 0)), Some(&[T1, T2])),
            1,
        );
        search(
            &base,
            "ship",
            None,
            mode,
            &selection(None, Some(&[T1, T2])),
            1,
        );
    }
    assert!(!directory.path().join(".wedl-rust").exists());
    let compiled = compiler::compile(&base).unwrap();
    let bytes = fs::read(&compiled.path).unwrap();
    for mode in MODES {
        search(
            &base,
            "ship",
            None,
            mode,
            &selection(Some(at(10, 0)), Some(&[T1, T2])),
            1,
        );
    }
    assert_eq!(bytes, fs::read(&compiled.path).unwrap());
    let mut forged = base.clone();
    forged.records.get_mut(PIER).unwrap().body = "forged ship".into();
    for mode in MODES {
        assert!(
            compiler::search_with_selection(
                &forged,
                "ship",
                1,
                None,
                mode,
                &selection(Some(at(10, 0)), None)
            )
            .is_err()
        );
    }
    assert_eq!(bytes, fs::read(&compiled.path).unwrap());
}
