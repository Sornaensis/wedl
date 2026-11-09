//! Independently dated source members must be admitted before retrieval work.
use serde_json::{Value as Json, json};
use std::path::Path;
use wedl::{
    character,
    compiler::{self, AuthorSearchSelection},
    model::{Record, StoryTime, World},
    retrieval::Mode,
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const FRIEND: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const ENV: &str = "env_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const KNOW: &str = "know_00000000000000000000000001";
const REL: &str = "rel_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const CONV: &str = "conv_00000000000000000000000001";
const T1: &str = "thread_00000000000000000000000001";
const T2: &str = "thread_00000000000000000000000002";
const MODES: [Mode; 3] = [Mode::Lexical, Mode::Semantic, Mode::Hybrid];

fn record(kind: &str, id: &str, title: &str, fields: Json, body: &str) -> Record {
    let mut fields = fields.as_object().unwrap().clone();
    fields.extend(
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":title})
            .as_object()
            .unwrap()
            .clone(),
    );
    source::parse_record(
        format!(
            "---\n{}---\n{body}",
            serde_yaml::to_string(&fields).unwrap()
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}

fn assemble(root: &Path, records: Vec<Record>) -> World {
    let world = source::world_from_records(root.to_path_buf(), records, None).unwrap();
    let checked = validation::validate_world(&world);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    world
}

fn fixture(root: &Path) -> World {
    assemble(
        root,
        vec![
            record(
                "world",
                WORLD,
                "Dated corpus",
                json!({
                    "capabilities":["lore-core-v1"],"default_timeline":"main",
                    "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
                    "threads":[{"id":T1,"label":"First"},{"id":T2,"label":"Second"}],
                    "state_keys":{"character":{"note":{"type":"string"},"tally":{"type":"integer"},"secret":{"type":"string"}}},
                    "state_profile_bindings":{"character":{"secret":"sealed"}},
                    "profiles":[{"id":"sealed","label":"Bound state","fields":{"type":"object","required":{"text":{"type":"string"}},"optional":{}}}],
                    "relationship_metrics":{"trust":{}}
                }),
                "",
            ),
            record(
                "character",
                HERO,
                "Astronomer",
                json!({"initial_state":{"note":"calm","tally":0}}),
                "",
            ),
            record(
                "character",
                FRIEND,
                "Companion",
                json!({"initial_state":{}}),
                "",
            ),
            record("location", PLACE, "Room", json!({}), ""),
            record(
                "environment",
                ENV,
                "Weather",
                json!({
                    "time":{"start":{"tick":6,"order":3},"end":{"tick":9,"order":2}},
                    "targets":[PLACE],"threads":[T1],
                    "conditions":{"weather":"environmentneedle sharedneedle","exact":9_007_199_254_740_993_i64,"unknown":null,"present":true},
                    "sensory":["sensoryneedle"]
                }),
                "",
            ),
            record(
                "event",
                EVENT,
                "Change",
                json!({
                    "time":{"tick":6,"order":3},"threads":[T1],
                    "aliases":["aliasneedle"],"tags":["tagneedle"],"x-search":"opaqueneedle",
                    "effects":[
                        {"id":"effect_00000000000000000000000001","target":HERO,"key":"secret","operation":"set","value":{"text":"boundneedle"}},
                        {"id":"effect_00000000000000000000000002","target":HERO,"key":"note","operation":"set","value":"effectneedle sharedneedle"},
                        {"id":"effect_00000000000000000000000003","target":HERO,"key":"tally","operation":"set","value":9_007_199_254_740_993_i64}
                    ]
                }),
                "",
            ),
            record(
                "knowledge",
                KNOW,
                "Account",
                json!({
                    "knower":HERO,"threads":[T1],"claim":{"key":"account","statement":"knowledgeneedle sharedneedle"},
                    "transitions":[
                        {"id":"learned","time":{"tick":6,"order":3},"state":"accepted","confidence":0.8,"acquisition":"observed","note":"learningneedle"},
                        {"id":"forgotten","time":{"tick":12},"state":"forgotten","confidence":1,"acquisition":"observed","note":"forgettingneedle"}
                    ]
                }),
                "",
            ),
            record(
                "relationship",
                REL,
                "Colleagues",
                json!({
                    "from":HERO,"to":FRIEND,"relationship_kind":"colleagues","threads":[T2],
                    "transitions":[
                        {"id":"met","time":{"tick":6,"order":3},"relationship_status":"active","metrics":{"trust":7.5},"facets":["facetneedle"],"note":"relationneedle sharedneedle"},
                        {"id":"left","time":{"tick":12},"relationship_status":"estranged","metrics":{},"facets":[],"note":"endedneedle"}
                    ]
                }),
                "",
            ),
            record(
                "story-point",
                POINT,
                "Question",
                json!({
                    "activation_policy":"manual","threads":[T2],
                    "lifecycle":{"initial_state":"dormant","transitions":[
                        {"id":"activated","time":{"tick":6,"order":3},"state":"active","note":"lifecycleneedle sharedneedle"},
                        {"id":"resolved","time":{"tick":13},"state":"resolved","note":"resolvedneedle"}
                    ]}
                }),
                "",
            ),
            record(
                "scene",
                SCENE,
                "Meeting",
                json!({
                    "status":"active","location":PLACE,"threads":[T2],
                    "time":{"start":{"tick":0},"current":{"tick":10}},
                    "participants":[{"character":HERO},{"character":FRIEND}],
                    "observations":[
                        {"id":"later","at":{"tick":10},"text":"futureobservationneedle","audience":["author"]},
                        {"id":"noticed","at":{"tick":6,"order":3},"until":{"tick":9,"order":2},"text":"observationneedle sharedneedle","audience":["author"]},
                        {"id":"lasting","at":{"tick":7},"text":"persistentneedle","audience":["author"]}
                    ]
                }),
                "summaryneedle",
            ),
            record(
                "conversation",
                CONV,
                "Discussion",
                json!({
                    "status":"active","location":PLACE,"threads":[T2],
                    "time":{"start":{"tick":0}},"participants":[{"character":HERO},{"character":FRIEND}],
                    "turns":[
                        {"id":"later","at":{"tick":10},"speaker":FRIEND,"text":"futureturnneedle","audience":["author"]},
                        {"id":"spoken","at":{"tick":6,"order":3},"speaker":HERO,"text":"speechneedle sharedneedle","audience":["author"]},
                        {"id":"acted","kind":"action","at":{"tick":6,"order":4},"actors":[FRIEND],"text":"actionneedle sharedneedle","audience":["author"]}
                    ],
                    "recollections":[
                        {"id":"remembered","character":HERO,"at":{"tick":8},"state":"remembered","confidence":0.5,
                            "summary":"recollectionneedle sharedneedle","interpretation":"interpretationneedle","emotional_impression":"emotionneedle",
                            "exact_turns":["spoken","acted"],"remembered_quotes":[{"source_turn":"spoken","speaker":FRIEND,"text":"approximationneedle","fidelity":"approximate"}]},
                        {"id":"forgotten","character":HERO,"at":{"tick":12},"state":"forgotten","confidence":1,"summary":"forgottenneedle"}
                    ]
                }),
                "summaryneedle",
            ),
        ],
    )
}

fn edit(base: &World, id: &str, update: impl FnOnce(&mut Json)) -> World {
    let original = &base.records[id];
    let mut fields = serde_json::to_value(&original.frontmatter).unwrap();
    update(&mut fields);
    let replacement = record(&original.kind, id, &original.title, fields, &original.body);
    assemble(
        &base.root,
        base.records
            .values()
            .map(|record| {
                if record.id == id {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
    )
}

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}

fn selected(at: StoryTime) -> AuthorSearchSelection {
    AuthorSearchSelection {
        at: Some(at),
        thread_ids: None,
    }
}

fn search(world: &World, query: &str, at: StoryTime, mode: Mode) -> Json {
    compiler::search_with_selection(world, query, 100, None, mode, &selected(at)).unwrap()
}

fn has(value: &Json, kind: &str, member: Option<&str>) -> bool {
    value["items"].as_array().unwrap().iter().any(|item| {
        item["documentKind"] == kind && member.is_none_or(|member| item["memberId"] == member)
    })
}

fn rankings(value: &Json) -> Json {
    json!({
        "total":value["total"],"rankedTotal":value["rankedTotal"],"retrieval":value["retrieval"],
        "items":value["items"].as_array().unwrap().iter().map(|item| json!({
            "id":item["id"],"documentKey":item["documentKey"],"documentKind":item["documentKind"],
            "memberId":item["memberId"],"snippet":item["snippet"],"score":item["score"],
            "lexicalRank":item["lexicalRank"],"lexicalScore":item["lexicalScore"],
            "semanticRank":item["semanticRank"],"semanticScore":item["semanticScore"]
        })).collect::<Vec<_>>()
    })
}

#[test]
fn structured_lanes_enter_each_retrieval_mode_at_their_exact_member_time() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for mode in MODES {
        for (query, kind, member, tick, order) in [
            ("environmentneedle", "environment", None, 6, 3),
            (
                "effectneedle",
                "event-effect",
                Some("effect_00000000000000000000000002"),
                6,
                3,
            ),
            ("relationneedle", "transition", Some("met"), 6, 3),
            ("lifecycleneedle", "transition", Some("activated"), 6, 3),
            ("knowledgeneedle", "knowledge-claim", None, 6, 3),
            (
                "observationneedle",
                "scene-observation",
                Some("noticed"),
                6,
                3,
            ),
            ("speechneedle", "conversation-turn", Some("spoken"), 6, 3),
            ("actionneedle", "conversation-action", Some("acted"), 6, 4),
            (
                "recollectionneedle",
                "conversation-recollection",
                Some("remembered"),
                8,
                0,
            ),
        ] {
            let before = search(&world, query, at(tick, order - 1), mode);
            assert_eq!(
                before["items"],
                json!([]),
                "{mode}: {query} before its origin"
            );
            let visible = search(&world, query, at(tick, order), mode);
            assert!(
                has(&visible, kind, member),
                "{mode}: missing {kind} for {query}: {visible}"
            );
            assert_eq!(visible["projection"], "native-author-dated");
            let foreign = search(
                &world,
                query,
                StoryTime::new("other", tick, order).unwrap(),
                mode,
            );
            assert_eq!(
                foreign["items"],
                json!([]),
                "{mode}: {query} crossed timelines"
            );
        }
    }
}

#[test]
fn environment_and_observation_endpoints_are_inclusive_without_expiring_other_members() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for mode in MODES {
        for query in ["environmentneedle", "sensoryneedle", "observationneedle"] {
            assert!(
                !search(&world, query, at(9, 2), mode)["items"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(search(&world, query, at(9, 3), mode)["items"], json!([]));
        }
        assert!(has(
            &search(&world, "persistentneedle", at(100, 0), mode),
            "scene-observation",
            Some("lasting")
        ));
    }
}

#[test]
fn open_aggregate_bodies_do_not_block_dated_members_and_closed_bodies_start_at_end() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let closed = edit(&world, SCENE, |fields| {
        fields["status"] = json!("closed");
        fields["time"]["end"] = json!({"tick":10});
        fields["observations"][1]["until"] = json!({"tick":20});
    });
    let closed = edit(&closed, CONV, |fields| {
        fields["status"] = json!("closed");
        fields["time"]["end"] = json!({"tick":10});
    });
    for mode in MODES {
        for tick in [8, 100] {
            assert_eq!(
                search(&world, "summaryneedle", at(tick, 0), mode)["items"],
                json!([])
            );
            assert!(has(
                &search(&world, "speechneedle", at(tick, 0), mode),
                "conversation-turn",
                Some("spoken")
            ));
        }
        assert_eq!(
            search(&closed, "summaryneedle", at(9, i32::MAX), mode)["items"],
            json!([])
        );
        let summaries = search(&closed, "summaryneedle", at(10, 0), mode);
        for id in [SCENE, CONV] {
            assert!(
                summaries["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["id"] == id && item["bodyIncluded"] == true)
            );
        }
        assert!(has(
            &search(&closed, "observationneedle", at(15, 0), mode),
            "scene-observation",
            Some("noticed")
        ));
        assert!(has(
            &search(&closed, "recollectionneedle", at(15, 0), mode),
            "conversation-recollection",
            Some("remembered")
        ));
    }
}

#[test]
fn inactive_scene_and_conversation_statuses_do_not_supply_structured_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for (id, status, query) in [
        (SCENE, "planned", "observationneedle"),
        (SCENE, "abandoned", "observationneedle"),
        (CONV, "draft", "speechneedle"),
        (CONV, "retired", "recollectionneedle"),
    ] {
        let changed = edit(&world, id, |fields| fields["status"] = json!(status));
        for mode in MODES {
            assert_eq!(
                search(&changed, query, at(8, 0), mode)["items"],
                json!([]),
                "{status}: {mode}"
            );
        }
    }
}

#[test]
fn member_citations_keep_source_ordinals_after_earlier_members_are_withheld() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for (query, kind, id, member, path) in [
        (
            "environmentneedle",
            "environment",
            ENV,
            None,
            "conditions/sensory",
        ),
        (
            "effectneedle",
            "event-effect",
            EVENT,
            Some("effect_00000000000000000000000002"),
            "effects[1]",
        ),
        (
            "knowledgeneedle",
            "knowledge-claim",
            KNOW,
            None,
            "claim.statement",
        ),
        (
            "lifecycleneedle",
            "transition",
            POINT,
            Some("activated"),
            "lifecycle.transitions[0]",
        ),
        (
            "observationneedle",
            "scene-observation",
            SCENE,
            Some("noticed"),
            "observations[1]",
        ),
        (
            "speechneedle",
            "conversation-turn",
            CONV,
            Some("spoken"),
            "turns[1]",
        ),
        (
            "actionneedle",
            "conversation-action",
            CONV,
            Some("acted"),
            "turns[2]",
        ),
        (
            "recollectionneedle",
            "conversation-recollection",
            CONV,
            Some("remembered"),
            "recollections[0]",
        ),
    ] {
        let result = search(&world, query, at(8, 0), Mode::Lexical);
        let item = result["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["documentKind"] == kind)
            .unwrap();
        assert_eq!(item["id"], id);
        assert_eq!(item["memberId"].as_str(), member);
        assert_eq!(item["status"], world.records[id].status());
        assert!(
            item["documentKey"]
                .as_str()
                .is_some_and(|key| !key.is_empty())
        );
        assert_eq!(item["citation"]["entityId"], id);
        assert_eq!(item["citation"]["section"], path);
        assert_eq!(
            item["citation"]["sourcePath"],
            world.records[id].source_path
        );
        assert_eq!(item["citation"]["fingerprint"], world.fingerprint);
        assert!(item["at"]["tick"].is_string());
        assert!(item["at"]["order"].is_string());
        if id == ENV || id == SCENE {
            assert_eq!(
                item["until"],
                json!({"timeline":"main","tick":"9","order":"2"})
            );
        } else {
            assert!(item["until"].is_null());
        }
    }
}

#[test]
fn author_history_survives_later_forgetting_without_adding_character_memory() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for mode in MODES {
        assert!(has(
            &search(&world, "knowledgeneedle", at(15, 0), mode),
            "knowledge-claim",
            None
        ));
        assert!(has(
            &search(&world, "learningneedle", at(15, 0), mode),
            "transition",
            Some("learned")
        ));
        assert!(has(
            &search(&world, "forgettingneedle", at(15, 0), mode),
            "transition",
            Some("forgotten")
        ));
        assert!(has(
            &search(&world, "recollectionneedle", at(15, 0), mode),
            "conversation-recollection",
            Some("remembered")
        ));
        assert_eq!(
            search(&world, "forgottenneedle", at(15, 0), mode)["items"],
            json!([])
        );
    }
    for query in [
        "knowledgeneedle",
        "recollectionneedle",
        "effectneedle",
        "persistentneedle",
    ] {
        assert_eq!(
            character::search(&world, HERO, query, &at(15, 0), 100).unwrap()["items"],
            json!([]),
            "author search taught {query}"
        );
    }
}

#[test]
fn undated_authored_claim_remains_timeless_without_inventing_an_acquisition() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let world = edit(&world, KNOW, |fields| fields["transitions"] = json!([]));
    for mode in MODES {
        for horizon in [
            at(i64::MIN, i32::MIN),
            StoryTime::new("other", 0, 0).unwrap(),
        ] {
            let result = search(&world, "knowledgeneedle", horizon, mode);
            let claim = result["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["documentKind"] == "knowledge-claim")
                .unwrap();
            assert!(claim["at"].is_null());
            assert!(claim["until"].is_null());
        }
    }
    assert_eq!(
        character::search(&world, HERO, "knowledgeneedle", &at(20, 0), 100).unwrap()["items"],
        json!([])
    );
}

#[test]
fn recollections_keep_authored_quotes_and_exact_speech_but_do_not_turn_actions_into_dialogue() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for query in [
        "interpretationneedle",
        "emotionneedle",
        "approximationneedle",
        "speechneedle",
    ] {
        let found = search(&world, query, at(8, 0), Mode::Lexical);
        assert!(
            has(&found, "conversation-recollection", Some("remembered")),
            "{query}"
        );
    }
    let action = search(&world, "actionneedle", at(8, 0), Mode::Lexical);
    assert!(has(&action, "conversation-action", Some("acted")));
    assert!(!has(&action, "conversation-recollection", None));
    assert!(!has(&action, "conversation-turn", None));
}

#[test]
fn profile_bound_effects_and_opaque_fields_cannot_change_ranking_or_consume_retrieval_budget() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let huge = edit(&world, EVENT, |fields| {
        fields["effects"][0]["value"]["text"] = json!("boundneedle sharedneedle ".repeat(30_000));
        fields["x-search"] = json!("opaqueneedle sharedneedle ".repeat(30_000));
    });
    for mode in MODES {
        assert_eq!(
            rankings(&search(&world, "sharedneedle", at(8, 0), mode)),
            rankings(&search(&huge, "sharedneedle", at(8, 0), mode))
        );
        for query in ["boundneedle", "opaqueneedle", "aliasneedle", "tagneedle"] {
            assert_eq!(
                search(&huge, query, at(8, 0), mode)["items"],
                json!([]),
                "{mode}: {query}"
            );
        }
    }
}

#[test]
fn future_members_are_excluded_before_statistics_and_complete_input_budgets() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let changed = edit(&world, CONV, |fields| {
        fields["turns"][0]["text"] = json!("futureturnneedle sharedneedle ".repeat(25_000));
        fields["recollections"][1]["summary"] =
            json!("forgottenneedle sharedneedle ".repeat(25_000));
    });
    let changed = edit(&changed, SCENE, |fields| {
        fields["observations"][0]["text"] =
            json!("futureobservationneedle sharedneedle ".repeat(25_000));
    });
    for mode in MODES {
        assert_eq!(
            rankings(&search(&world, "sharedneedle", at(8, 0), mode)),
            rankings(&search(&changed, "sharedneedle", at(8, 0), mode))
        );
        assert!(
            compiler::search_with_selection(
                &changed,
                "sharedneedle",
                1,
                None,
                mode,
                &selected(at(10, 0))
            )
            .is_err()
        );
    }
}

#[test]
fn private_owners_are_excluded_before_member_text_and_training() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let private = edit(&world, CONV, |fields| {
        fields["audience"] = json!(["editor"]);
        fields["perspectives"] = json!(["author"]);
    });
    let changed = edit(&private, CONV, |fields| {
        fields["turns"][1]["text"] = json!("speechneedle sharedneedle ".repeat(30_000));
        fields["recollections"][0]["summary"] =
            json!("recollectionneedle sharedneedle ".repeat(30_000));
    });
    for mode in MODES {
        assert_eq!(
            rankings(&search(&private, "sharedneedle", at(8, 0), mode)),
            rankings(&search(&changed, "sharedneedle", at(8, 0), mode))
        );
        assert_eq!(
            search(&changed, "speechneedle", at(8, 0), mode)["items"],
            json!([])
        );
    }
}

#[test]
fn kind_filter_precedes_structured_work_but_threads_and_output_limits_do_not() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let huge = edit(&world, EVENT, |fields| {
        fields["effects"][1]["value"] = json!("effectneedle ".repeat(50_000))
    });
    for mode in MODES {
        let selection = selected(at(8, 0));
        let read = |world: &World| {
            compiler::search_with_selection(
                world,
                "speechneedle",
                100,
                Some("conversation"),
                mode,
                &selection,
            )
            .unwrap()
        };
        assert_eq!(rankings(&read(&world)), rankings(&read(&huge)));
        assert!(
            compiler::search_with_selection(&huge, "speechneedle", 1, None, mode, &selection)
                .is_err()
        );
        let grouped = AuthorSearchSelection {
            thread_ids: Some(vec![T2.into()]),
            ..selection
        };
        assert!(
            compiler::search_with_selection(&huge, "speechneedle", 1, None, mode, &grouped)
                .is_err()
        );
    }
}

#[test]
fn threads_keep_structured_rows_as_an_unchanged_post_ranked_subsequence() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for mode in MODES {
        let selection = selected(at(8, 0));
        let all =
            compiler::search_with_selection(&world, "sharedneedle", 100, None, mode, &selection)
                .unwrap();
        for thread in [T1, T2] {
            let grouping = AuthorSearchSelection {
                thread_ids: Some(vec![thread.into()]),
                ..selection.clone()
            };
            let selected =
                compiler::search_with_selection(&world, "sharedneedle", 100, None, mode, &grouping)
                    .unwrap();
            let expected: Vec<_> = all["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| {
                    world.records[item["id"].as_str().unwrap()]
                        .list("threads")
                        .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(thread)))
                })
                .cloned()
                .collect();
            assert!(!expected.is_empty());
            assert_eq!(selected["items"], json!(expected));
            assert_eq!(selected["retrieval"], all["retrieval"]);
            assert_eq!(selected["rankedTotal"], all["rankedTotal"]);
        }
    }
}

#[test]
fn structured_document_limit_applies_to_complete_admitted_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let many = edit(&world, REL, |fields| {
        fields["transitions"] =
            json!((0..1100).map(|index| json!({
            "id":format!("history-{index}"),"time":{"tick":100+index},"note":"countedneedle"
        })).collect::<Vec<_>>());
    });
    for mode in MODES {
        assert_eq!(
            rankings(&search(&world, "Room", at(0, 0), mode)),
            rankings(&search(&many, "Room", at(0, 0), mode))
        );
        assert!(
            compiler::search_with_selection(
                &many,
                "countedneedle",
                1,
                None,
                mode,
                &selected(at(2000, 0))
            )
            .is_err()
        );
    }
}

#[test]
fn exact_integer_scalars_remain_distinguishable_in_structured_text() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let exact = search(&world, "9007199254740993", at(8, 0), Mode::Lexical);
    assert!(has(&exact, "environment", None));
    assert!(has(
        &exact,
        "event-effect",
        Some("effect_00000000000000000000000003")
    ));
    assert_eq!(
        search(&world, "9007199254740992", at(8, 0), Mode::Lexical)["items"],
        json!([])
    );
}

#[test]
fn accepted_raw_relationship_values_keep_nested_nonstring_keys_as_literal_search_text() {
    let directory = tempfile::tempdir().unwrap();
    let base = fixture(directory.path());
    let original = &base.records[REL];
    let mut fields = original.frontmatter.clone();
    let metric: serde_yaml::Value = serde_yaml::from_str("{1: rawneedle}").unwrap();
    fields["transitions"][0]["metrics"]["trust"] = metric.clone();
    let replacement = source::parse_record(
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            original.body
        )
        .as_bytes(),
        &original.source_path,
    )
    .unwrap();
    let mut records = base.records.clone();
    records.insert(REL.into(), replacement);
    let world = assemble(&base.root, records.into_values().collect());
    let authored = world.records[REL].raw_bytes.clone();
    let state_before = wedl::runtime::resolved_state(&base, HERO, &at(8, 0)).unwrap();
    for mode in MODES {
        let result = search(&world, "rawneedle", at(8, 0), mode);
        assert!(has(&result, "transition", Some("met")));
        assert_eq!(world.records[REL].raw_bytes, authored);
        assert_eq!(
            world.records[REL].get("transitions").unwrap()[0]["metrics"]["trust"],
            metric
        );
        assert_eq!(
            wedl::runtime::resolved_state(&world, HERO, &at(8, 0)).unwrap(),
            state_before
        );
    }
}

#[test]
fn absent_and_null_optional_recollection_prose_have_identical_retrieval_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let omitted = edit(&world, CONV, |fields| {
        let memory = fields["recollections"][0].as_object_mut().unwrap();
        memory.remove("interpretation");
        memory.remove("emotional_impression");
    });
    let null = edit(&omitted, CONV, |fields| {
        fields["recollections"][0]["interpretation"] = Json::Null;
        fields["recollections"][0]["emotional_impression"] = Json::Null;
    });
    for mode in MODES {
        for query in ["sharedneedle", "recollectionneedle", "null"] {
            assert_eq!(
                rankings(&search(&omitted, query, at(8, 0), mode)),
                rankings(&search(&null, query, at(8, 0), mode)),
                "{mode}: {query}"
            );
        }
    }
    // A null condition is authored data; absent optional prose is not the word "null".
    let literal = search(&null, "null", at(8, 0), Mode::Lexical);
    assert!(has(&literal, "environment", None));
    assert!(!has(&literal, "conversation-recollection", None));
}

#[test]
fn emitted_member_identity_is_bounded_only_after_its_time_admission() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    let huge = edit(&world, SCENE, |fields| {
        fields["observations"][0]["id"] = json!(format!("future-{}", "x".repeat(600_000)));
    });
    for mode in MODES {
        assert_eq!(
            rankings(&search(&world, "sharedneedle", at(8, 0), mode)),
            rankings(&search(&huge, "sharedneedle", at(8, 0), mode))
        );
        let selection = AuthorSearchSelection {
            at: Some(at(10, 0)),
            thread_ids: Some(vec![T1.into()]),
        };
        let error =
            compiler::search_with_selection(&huge, "sharedneedle", 1, None, mode, &selection)
                .unwrap_err();
        assert!(error.to_string().contains("bounds"));
        assert!(error.to_string().len() < 512);
    }
}

#[test]
fn omitted_horizons_retain_existing_all_time_source_search() {
    let directory = tempfile::tempdir().unwrap();
    let world = fixture(directory.path());
    for mode in MODES {
        let plain = compiler::search_with_mode(&world, "effectneedle", 100, None, mode).unwrap();
        let selected = compiler::search_with_selection(
            &world,
            "effectneedle",
            100,
            None,
            mode,
            &AuthorSearchSelection::default(),
        )
        .unwrap();
        assert_eq!(selected, plain);
        assert_eq!(selected["items"], json!([]));
        let grouped = compiler::search_with_selection(
            &world,
            "effectneedle",
            100,
            None,
            mode,
            &AuthorSearchSelection {
                at: None,
                thread_ids: Some(vec![T1.into()]),
            },
        )
        .unwrap();
        assert_eq!(grouped["items"], json!([]));
    }
}
