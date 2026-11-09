use serde_json::{Value, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, character,
    model::{Record, StoryTime, World},
    source::{self, SOURCE_SCHEMA},
    validation,
};

const HERO: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const THIRD: &str = "char_00000000000000000000000003";
const PLACE: &str = "loc_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const CONV: &str = "conv_00000000000000000000000001";
const PRIVATE_CONV: &str = "conv_00000000000000000000000002";
const KNOW: &str = "know_00000000000000000000000001";
const PRIVATE_KNOW: &str = "know_00000000000000000000000002";
const FUTURE_KNOW: &str = "know_00000000000000000000000003";
const HEARD: &str = "turn_00000000000000000000000001";
const HIDDEN: &str = "turn_00000000000000000000000002";

fn record(id: &str, kind: &str, extra: &str) -> Record {
    let markdown = format!(
        "---\nschema: {SOURCE_SCHEMA}\nid: {id}\nkind: {kind}\ntitle: AUTHOR_TITLE_{id}\nstatus: canonical\naliases: []\ntags: []\n{extra}---\nAUTHOR_BODY_SECRET\n"
    );
    source::parse_record(markdown.as_bytes(), &format!("story/{id}.md")).unwrap()
}

fn fresh(world: World) -> World {
    let records = world
        .records
        .values()
        .map(|record| {
            let raw = format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&record.frontmatter).unwrap(),
                record.body
            );
            let mut parsed = source::parse_record(raw.as_bytes(), &record.source_path).unwrap();
            parsed.blob_oid = record.blob_oid.clone();
            parsed
        })
        .collect();
    source::world_from_records(world.root, records, world.revision).unwrap()
}

fn world() -> World {
    let participants = format!(
        "participants:\n- {{character: {HERO}, from: {{tick: 5}}, until: {{tick: 15, order: 0}}}}\n- {{character: {OTHER}, from: {{tick: 0}}, until: {{tick: 30}}}}\n- {{character: {THIRD}, from: {{tick: 0}}, until: {{tick: 30}}}}\n"
    );
    let mut scene = record(
        SCENE,
        "scene",
        &format!(
            "location: {PLACE}\ntime: {{start: {{tick: 0}}, current: {{tick: 30}}, end: {{tick: 30}}}}\n{participants}objects: []\nenvironments: []\nconversations: [{CONV}]\nstory_points: []\noutcome_events: []\nobservations:\n- {{id: obs_before, at: {{tick: 4}}, audience: [participants], text: Before joining}}\n- {{id: obs_join, at: {{tick: 5}}, until: {{tick: 10}}, audience: [participants], text: Lantern courier signal appears}}\n- {{id: obs_hidden, at: {{tick: 6}}, audience: [{OTHER}], text: PRIVATE_OBSERVATION}}\n- {{id: obs_recorded, at: {{tick: 10}}, audience: [{HERO}], text: A bell sounds nearby}}\n- {{id: obs_after, at: {{tick: 16}}, audience: [participants], text: After leaving}}\n"
        ),
    );
    scene.frontmatter["status"] = Yaml::String("closed".into());
    let mut conversation = record(
        CONV,
        "conversation",
        &format!(
            "location: {PLACE}\nscene: {SCENE}\ntime: {{start: {{tick: 0}}, end: {{tick: 30}}}}\n{participants}turns:\n- {{id: turn_before, kind: speech, at: {{tick: 4}}, speaker: {OTHER}, audience: [participants], text: Before joining}}\n- {{id: {HEARD}, kind: speech, at: {{tick: 5}}, speaker: {OTHER}, audience: [participants], text: Lantern courier carries the key}}\n- {{id: {HIDDEN}, kind: speech, at: {{tick: 6}}, speaker: {OTHER}, audience: [{THIRD}], text: PRIVATE_BEAT_SECRET}}\n- {{id: turn_now, kind: speech, at: {{tick: 10}}, speaker: {OTHER}, audience: [participants], text: Lantern courier waits outside}}\n- {{id: turn_self, kind: speech, at: {{tick: 12}}, speaker: {HERO}, audience: [{OTHER}], text: I will meet the courier}}\n- {{id: turn_exit, kind: action, at: {{tick: 15, order: 0}}, actors: [{OTHER}], audience: [participants], text: The courier waves}}\n- {{id: turn_after, kind: speech, at: {{tick: 15, order: 1}}, speaker: {OTHER}, audience: [participants], text: After leaving}}\n- {{id: turn_future, kind: speech, at: {{tick: 20}}, speaker: {OTHER}, audience: [participants], text: FUTURE_BEAT_SECRET}}\nrecollections:\n- {{id: recol_other, character: {OTHER}, at: {{tick: 10}}, state: remembered, confidence: 1, summary: PRIVATE_MEMORY_SECRET}}\n- id: recol_hero\n  character: {HERO}\n  at: {{tick: 14}}\n  state: remembered\n  confidence: 0.7\n  summary: I remember the lantern courier\n  interpretation: I may have misunderstood the destination\n  emotional_impression: Cautious\n  exact_turns: [{HEARD}, {HIDDEN}]\n  remembered_quotes:\n  - {{source_turn: {HIDDEN}, speaker: {OTHER}, text: I recall a quiet remark, fidelity: approximate}}\n- {{id: recol_forgotten, character: {HERO}, at: {{tick: 25}}, state: forgotten, confidence: 0, summary: It has faded}}\n"
        ),
    );
    conversation.frontmatter["status"] = Yaml::String("closed".into());
    let mut private_conversation = record(
        PRIVATE_CONV,
        "conversation",
        &format!(
            "location: {PLACE}\ntime: {{start: {{tick: 0}}, end: {{tick: 30}}}}\nparticipants: [{{character: {OTHER}, from: {{tick: 0}}, until: {{tick: 30}}}}]\nturns: [{{id: private_turn, kind: speech, at: {{tick: 5}}, speaker: {OTHER}, audience: [participants], text: PRIVATE_CONVERSATION_SECRET}}]\nrecollections: []\n"
        ),
    );
    private_conversation.frontmatter["status"] = Yaml::String("closed".into());
    let records = vec![
        record(
            "world_00000000000000000000000000",
            "world",
            "default_timeline: main\ntimelines: [{id: main, label: Main}, {id: alternate, label: Alternate}]\ncapabilities: []\nthreads: []\nstate_keys: {}\n",
        ),
        record(HERO, "character", "initial_state: {}\n"),
        record(OTHER, "character", "initial_state: {}\n"),
        record(THIRD, "character", "initial_state: {}\n"),
        record(PLACE, "location", ""),
        scene,
        conversation,
        private_conversation,
        record(
            KNOW,
            "knowledge",
            &format!(
                "knower: {HERO}\nclaim: {{key: AUTHOR_TAXONOMY_SECRET, statement: The lantern courier has a key, truth: 'false'}}\ntransitions:\n- {{id: acquired, time: {{tick: 5}}, state: suspected, acquisition: told, confidence: 0.4, note: AUTHOR_NOTE_SECRET}}\n- {{id: forgot, time: {{tick: 20}}, state: forgotten, acquisition: remembered, confidence: 0}}\n"
            ),
        ),
        record(
            PRIVATE_KNOW,
            "knowledge",
            &format!(
                "knower: {OTHER}\nclaim: {{key: private, statement: PRIVATE_KNOWLEDGE_SECRET, truth: unknown}}\ntransitions: [{{id: private_acquired, time: {{tick: 5}}, state: accepted, acquisition: observed, confidence: 1}}]\n"
            ),
        ),
        record(
            FUTURE_KNOW,
            "knowledge",
            &format!(
                "knower: {HERO}\nclaim: {{key: future, statement: FUTURE_KNOWLEDGE_SECRET, truth: unknown}}\ntransitions: [{{id: future_acquired, time: {{tick: 40}}, state: accepted, acquisition: observed, confidence: 1}}]\n"
            ),
        ),
    ];
    fresh(
        source::world_from_records(
            PathBuf::new(),
            records,
            Some("GLOBAL_REVISION_SECRET".into()),
        )
        .unwrap(),
    )
}

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}
fn assert_valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
fn context(world: &World, tick: i64, order: i32) -> Value {
    character::context(world, HERO, &at(tick, order), 100_000).unwrap()
}

#[test]
fn exact_character_context_contains_only_explicit_dated_evidence() {
    let world = world();
    assert_valid(&world);
    let packet = context(&world, 10, 0);
    assert_eq!(packet["characterId"], HERO);
    assert_eq!(packet["beliefs"].as_array().unwrap().len(), 1);
    assert_eq!(
        packet["beliefs"][0],
        json!({"knowledgeId":KNOW,"transitionId":"acquired","statement":"The lantern courier has a key","state":"suspected","confidence":0.4,"acquisition":"told","time":{"timeline":"main","tick":"5","order":"0"}})
    );
    assert_eq!(packet["observations"].as_array().unwrap().len(), 2);
    assert_eq!(packet["conversations"].as_array().unwrap().len(), 1);
    assert_eq!(
        packet["conversations"][0]["beats"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(packet["conversations"][0]["recollection"].is_null());
    let serialized = serde_json::to_string(&packet).unwrap();
    for forbidden in [
        "SECRET",
        "AUTHOR_",
        "truth",
        "claimKey",
        "title",
        "alias",
        "fingerprint",
        "revision",
        "blobOid",
        "sourcePath",
        "causingEvent",
        "sourceEntity",
        "[0]",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "leaked {forbidden}: {serialized}"
        );
    }
    let mut alias_world = world.clone();
    alias_world.records.get_mut(HERO).unwrap().frontmatter["aliases"] =
        serde_yaml::from_str("[Hero]").unwrap();
    assert!(matches!(
        character::context(&alias_world, "Hero", &at(10, 0), 100_000),
        Err(Error::NotFound(_))
    ));
    assert!(
        character::context(
            &world,
            HERO,
            &StoryTime {
                timeline: "unknown".into(),
                tick: 10,
                order: 0
            },
            100_000
        )
        .is_err()
    );
}

#[test]
fn full_outputs_and_search_statistics_ignore_private_future_and_source_metadata() {
    let baseline = world();
    let mut changed = baseline.clone();
    changed.revision = Some("completely different revision".into());
    for record in changed.records.values_mut() {
        record.frontmatter["title"] = Yaml::String(format!("OTHER_PRIVATE_TITLE_{}", record.id));
        record.frontmatter["aliases"] = serde_yaml::from_str("[Private alias]").unwrap();
        record.body =
            "Entirely rewritten untimed private prose lantern courier lantern courier".into();
        record.source_path = format!("another/private-{}.md", record.id);
        record.blob_oid = Some("private-blob-hash".into());
    }
    changed.records.get_mut(KNOW).unwrap().frontmatter["claim"]["truth"] =
        Yaml::String("true".into());
    changed.records.get_mut(KNOW).unwrap().frontmatter["claim"]["key"] =
        Yaml::String("different.hidden.taxonomy".into());
    changed.records.get_mut(KNOW).unwrap().frontmatter["transitions"][0]["note"] =
        Yaml::String("Changed hidden note lantern courier".into());
    for id in [PRIVATE_KNOW, FUTURE_KNOW] {
        changed.records.get_mut(id).unwrap().frontmatter["claim"]["statement"] =
            Yaml::String("lantern courier lantern courier PRIVATE_CHANGED".into());
    }
    changed.records.get_mut(CONV).unwrap().frontmatter["turns"][2]["text"] =
        Yaml::String("lantern courier PRIVATE_CHANGED".into());
    changed.records.get_mut(CONV).unwrap().frontmatter["turns"][7]["text"] =
        Yaml::String("lantern courier FUTURE_CHANGED".into());
    changed.records.get_mut(CONV).unwrap().frontmatter["recollections"][0]["summary"] =
        Yaml::String("lantern courier PRIVATE_MEMORY_CHANGED".into());
    let earlier:Yaml=serde_yaml::from_str(&format!("{{id: turn_extra_hidden, kind: speech, at: {{tick: 1}}, speaker: {OTHER}, audience: [participants], text: lantern courier HIDDEN_INSERTION}}" )).unwrap();
    changed.records.get_mut(CONV).unwrap().frontmatter["turns"]
        .as_sequence_mut()
        .unwrap()
        .insert(0, earlier);
    let extra = record(
        "know_00000000000000000000000004",
        "knowledge",
        &format!(
            "knower: {OTHER}\nclaim: {{key: added, statement: lantern courier added hidden corpus, truth: unknown}}\ntransitions: [{{id: added, time: {{tick: 0}}, state: accepted, acquisition: authored, confidence: 1}}]\n"
        ),
    );
    changed.records.insert(extra.id.clone(), extra);
    let changed = fresh(changed);
    assert_valid(&baseline);
    assert_valid(&changed);
    assert_ne!(baseline.fingerprint, changed.fingerprint);
    for horizon in [at(10, 0), at(14, 0)] {
        assert_eq!(
            bytes(&character::context(&baseline, HERO, &horizon, 100_000).unwrap()),
            bytes(&character::context(&changed, HERO, &horizon, 100_000).unwrap())
        );
        assert_eq!(
            bytes(&character::conversation(&baseline, HERO, CONV, &horizon).unwrap()),
            bytes(&character::conversation(&changed, HERO, CONV, &horizon).unwrap())
        );
        for query in ["lantern courier", "PRIVATE_CHANGED", "SECRET", "key"] {
            assert_eq!(
                bytes(&character::search(&baseline, HERO, query, &horizon, 2).unwrap()),
                bytes(&character::search(&changed, HERO, query, &horizon, 2).unwrap())
            );
        }
        let before = character::context(&baseline, HERO, &horizon, 200).unwrap_err();
        let after = character::context(&changed, HERO, &horizon, 200).unwrap_err();
        assert_eq!(before.json(), after.json());
    }
}

#[test]
fn validated_author_chronology_never_changes_character_responses() {
    use wedl::service::{Operation, Service, TimeInput};

    let baseline = world();
    let mut changed = baseline.clone();
    let declarations: Yaml = serde_yaml::from_str(include_str!(
        "../examples/authoring/19-world-chronology.yaml"
    ))
    .unwrap();
    let annotations: Yaml = serde_yaml::from_str(
        "- id: chronology_00000000000000000000000001\n  value: {civil: {calendar_id: calendar_00000000000000000000000001, year: 500}}\n  display: AUTHOR_UNDATED_DATE_SECRET\n  provenance: [AUTHOR_CHRONOLOGY_SECRET]\n",
    )
    .unwrap();
    for record in changed.records.values_mut() {
        record.frontmatter["chronology"] = if record.kind == "world" {
            declarations["chronology"].clone()
        } else {
            annotations.clone()
        };
    }
    let changed = fresh(changed);
    assert_valid(&baseline);
    assert_valid(&changed);
    assert_ne!(baseline.fingerprint, changed.fingerprint);
    let authored = wedl::chronology::annotations(
        &changed,
        &wedl::chronology::AnnotationFilter {
            record: Some(HERO.into()),
            calendar: None,
            era: None,
            limit: 100,
        },
    )
    .unwrap();
    assert!(authored.to_string().contains("AUTHOR_UNDATED_DATE_SECRET"));

    let before = Service::new(baseline);
    let after = Service::new(changed);
    for tick in [10, 14, 25] {
        let horizon = TimeInput {
            timeline: Some("main".into()),
            tick,
            order: 0,
        };
        for operation in [
            Operation::CharacterContext {
                character: HERO.into(),
                at: horizon.clone(),
                max_characters: 100_000,
            },
            Operation::CharacterSearch {
                character: HERO.into(),
                query: "lantern courier".into(),
                at: horizon.clone(),
                limit: 100,
            },
            Operation::CharacterConversation {
                character: HERO.into(),
                conversation: CONV.into(),
                at: horizon,
            },
        ] {
            assert_eq!(
                bytes(&before.execute(operation.clone()).unwrap()),
                bytes(&after.execute(operation).unwrap())
            );
        }
    }
}

#[test]
fn membership_and_observation_expiry_use_exact_inclusive_boundaries() {
    let world = world();
    let before = context(&world, 4, 0);
    assert!(before["observations"].as_array().unwrap().is_empty());
    assert!(before["conversations"].as_array().unwrap().is_empty());
    let joined = context(&world, 5, 0);
    assert_eq!(joined["observations"][0]["observationId"], "obs_join");
    assert_eq!(joined["conversations"][0]["beats"][0]["turnId"], HEARD);
    let exact = context(&world, 10, 0);
    assert!(
        exact["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["observationId"] == "obs_join")
    );
    let expired = context(&world, 10, 1);
    assert!(
        !expired["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["observationId"] == "obs_join")
    );
    let later = context(&world, 16, 0);
    let turns = later["conversations"][0]["beats"].as_array().unwrap();
    assert!(turns.iter().any(|item| item["turnId"] == "turn_self"));
    assert!(turns.iter().any(|item| item["turnId"] == "turn_exit"));
    assert!(!turns.iter().any(|item| item["turnId"] == "turn_after"));
    // An explicitly recorded observation without until persists, even after departure.
    assert_eq!(later["observations"].as_array().unwrap().len(), 1);
    assert_eq!(later["observations"][0]["observationId"], "obs_recorded");
}

#[test]
fn recollections_do_not_expand_or_name_inaccessible_transcript_members() {
    let world = world();
    let packet = character::conversation(&world, HERO, CONV, &at(14, 0)).unwrap();
    let memory = &packet["recollection"];
    assert_eq!(memory["exactTurns"], json!([HEARD]));
    assert_eq!(
        memory["rememberedQuotes"][0]["text"],
        "I recall a quiet remark"
    );
    assert_eq!(memory["rememberedQuotes"][0]["fidelity"], "approximate");
    assert!(memory["rememberedQuotes"][0].get("sourceTurnId").is_none());
    let encoded = serde_json::to_string(&packet).unwrap();
    assert!(!encoded.contains(HIDDEN));
    assert!(!encoded.contains("PRIVATE_BEAT_SECRET"));
    let search = character::search(&world, HERO, "quiet remark", &at(14, 0), 10).unwrap();
    assert_eq!(search["total"], 1);
    assert_eq!(search["items"][0]["kind"], "recollection");
}

#[test]
fn valid_parentage_and_union_histories_do_not_create_character_evidence() {
    use wedl::service::{Operation, Service, TimeInput};

    let baseline = world();
    let mut changed = baseline.clone();
    let owner = changed.world_record().unwrap().id.clone();
    changed.records.get_mut(&owner).unwrap().frontmatter["capabilities"] =
        serde_yaml::from_str("[generational-core-v1]").unwrap();
    for (id, kind, extra) in [
        (
            "kinship_00000000000000000000000001",
            "parentage",
            format!(
                "child_id: {HERO}\nparent_id: {OTHER}\ninitialization:\n  transition_id: transition_00000000000000000000000001\n  transition_kind: parentage-initialize\n  applicability: {{applicability_kind: instant, point: {{timeline: main, tick: 0, order: 0}}}}\n  payload: {{basis: adoptive}}\ntransitions: []\n"
            ),
        ),
        (
            "union_00000000000000000000000001",
            "union",
            format!(
                "participant_ids: [{HERO}, {THIRD}]\ninitialization:\n  transition_id: transition_00000000000000000000000002\n  transition_kind: union-initialize\n  applicability: {{applicability_kind: instant, point: {{timeline: main, tick: 0, order: 0}}}}\n  payload: {{participant_ids: [{HERO}, {THIRD}]}}\ntransitions: []\n"
            ),
        ),
    ] {
        let mut item = record(id, kind, &extra);
        item.body = "AUTHOR_GENEALOGY_SECRET lantern courier lantern courier".into();
        changed.records.insert(id.into(), item);
    }
    let changed = fresh(changed);
    assert_valid(&baseline);
    assert_valid(&changed);
    let family = wedl::generational::family(&changed, HERO, &at(10, 0), 100).unwrap();
    assert!(
        family
            .to_string()
            .contains("kinship_00000000000000000000000001")
    );
    assert!(
        family
            .to_string()
            .contains("union_00000000000000000000000001")
    );
    let before = Service::new(baseline);
    let after = Service::new(changed);
    for tick in [10, 14, 25] {
        let horizon = TimeInput {
            timeline: Some("main".into()),
            tick,
            order: 0,
        };
        for operation in [
            Operation::CharacterContext {
                character: HERO.into(),
                at: horizon.clone(),
                max_characters: 100_000,
            },
            Operation::CharacterSearch {
                character: HERO.into(),
                query: "lantern courier".into(),
                at: horizon.clone(),
                limit: 100,
            },
            Operation::CharacterConversation {
                character: HERO.into(),
                conversation: CONV.into(),
                at: horizon,
            },
        ] {
            assert_eq!(
                bytes(&before.execute(operation.clone()).unwrap()),
                bytes(&after.execute(operation).unwrap())
            );
        }
    }
}

#[test]
fn canonical_organizations_succession_and_vitals_do_not_create_character_evidence() {
    use wedl::service::{Operation, Service, TimeInput};

    let baseline = world();
    let mut changed = baseline.clone();
    let owner = changed.world_record().unwrap().id.clone();
    changed.records.get_mut(&owner).unwrap().frontmatter["capabilities"] =
        serde_yaml::from_str("[generational-core-v1]").unwrap();
    for request in [
        include_str!("../examples/authoring/27-organization.yaml"),
        include_str!("../examples/authoring/28-affiliation.yaml"),
        include_str!("../examples/authoring/29-legacy.yaml"),
        include_str!("../examples/authoring/30-tenure.yaml"),
        include_str!("../examples/authoring/31-claim.yaml"),
        include_str!("../examples/authoring/32-vital.yaml"),
    ] {
        let mut request: Yaml = serde_yaml::from_str(request).unwrap();
        for transition in request["transitions"].as_sequence_mut().unwrap() {
            transition
                .as_mapping_mut()
                .unwrap()
                .remove("cause_event_id");
        }
        let mut item =
            wedl::record_edit::create(&changed, &serde_yaml::to_string(&request).unwrap()).unwrap();
        item.body = "AUTHOR_HISTORY_SECRET lantern courier lantern courier".into();
        changed.records.insert(item.id.clone(), item);
    }
    let changed = fresh(changed);
    assert_valid(&baseline);
    assert_valid(&changed);
    let legacy = wedl::generational::legacy(
        &changed,
        "legacy_00000000000000000000000001",
        &at(10, 0),
        100,
    )
    .unwrap();
    assert_eq!(legacy["holders"][0]["value"]["holderId"], HERO);
    assert_eq!(
        wedl::generational::vital(&changed, HERO, &at(10, 0), 100).unwrap()["vital"],
        "living"
    );
    let before = Service::new(baseline);
    let after = Service::new(changed);
    for tick in [10, 14, 25] {
        let horizon = TimeInput {
            timeline: Some("main".into()),
            tick,
            order: 0,
        };
        for operation in [
            Operation::CharacterContext {
                character: HERO.into(),
                at: horizon.clone(),
                max_characters: 100_000,
            },
            Operation::CharacterSearch {
                character: HERO.into(),
                query: "lantern courier".into(),
                at: horizon.clone(),
                limit: 100,
            },
            Operation::CharacterConversation {
                character: HERO.into(),
                conversation: CONV.into(),
                at: horizon,
            },
        ] {
            assert_eq!(
                bytes(&before.execute(operation.clone()).unwrap()),
                bytes(&after.execute(operation).unwrap())
            );
        }
    }
}

#[test]
fn latest_forgotten_belief_and_recollection_are_absent() {
    let world = world();
    assert_eq!(
        context(&world, 19, 0)["beliefs"].as_array().unwrap().len(),
        1
    );
    assert!(
        context(&world, 20, 0)["beliefs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!context(&world, 24, 0)["conversations"][0]["recollection"].is_null());
    let after = context(&world, 25, 0);
    assert!(after["conversations"][0]["recollection"].is_null());
    assert_eq!(
        character::search(&world, HERO, "misunderstood", &at(25, 0), 10).unwrap()["total"],
        0
    );
}

#[test]
fn inaccessible_missing_and_wrong_kind_conversations_share_one_error() {
    let world = world();
    let expected = character::conversation(&world, HERO, PRIVATE_CONV, &at(10, 0))
        .unwrap_err()
        .json();
    for id in [
        "conv_00000000000000000000000999",
        OTHER,
        "AUTHOR_TITLE_conv_00000000000000000000000001",
    ] {
        let result = character::conversation(&world, HERO, id, &at(10, 0)).unwrap_err();
        assert!(matches!(result, Error::NotFound(_)));
        assert_eq!(result.json(), expected);
    }
    assert_eq!(
        character::conversation(&world, HERO, CONV, &at(4, 0))
            .unwrap_err()
            .json(),
        expected
    );
}

#[test]
fn search_is_literal_bounded_and_writes_no_author_cache() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world();
    world.root = directory.path().into();
    let result = character::search(&world, HERO, "lantern courier", &at(10, 0), 1).unwrap();
    assert_eq!(result["total"], 4);
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert!(!world.root.join(".wedl-rust").exists());
    assert_eq!(
        character::search(
            &world,
            HERO,
            "lantern OR PRIVATE_BEAT_SECRET",
            &at(10, 0),
            10
        )
        .unwrap()["total"],
        0
    );
    for (query, limit) in [("", 10), ("   ", 10), ("lantern", 0), ("lantern", 101)] {
        assert!(character::search(&world, HERO, query, &at(10, 0), limit).is_err());
    }
    assert!(character::search(&world, HERO, &"a".repeat(4097), &at(10, 0), 10).is_err());
    assert!(character::search(&world, HERO, &vec!["a"; 65].join(" "), &at(10, 0), 10).is_err());
    let packet = character::context(&world, HERO, &at(10, 0), 100_000).unwrap();
    let generous_size = serde_json::to_string(&packet).unwrap().chars().count();
    // The budget field itself changes width; a generous bound succeeds, and a
    // bound below the payload fails with no silent evidence truncation.
    assert!(character::context(&world, HERO, &at(10, 0), generous_size + 10).is_ok());
    assert!(character::context(&world, HERO, &at(10, 0), generous_size - 100).is_err());
}

#[test]
fn real_tideglass_character_projection_has_no_author_source_metadata() {
    let world = source::load_world(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass"),
        None,
    )
    .unwrap();
    let mina = "char_4JVNDN6QW0BAFFV379AJJSMEPZ";
    let packet = character::context(&world, mina, &at(20, 1), 100_000).unwrap();
    assert!(!packet["conversations"].as_array().unwrap().is_empty());
    let encoded = serde_json::to_string(&packet).unwrap();
    for key in [
        "sourcePath",
        "fingerprint",
        "revision",
        "blobOid",
        "claimKey",
        "truth",
        "title",
    ] {
        assert!(!encoded.contains(&format!("\"{key}\"")));
    }
    let matches = character::search(&world, mina, "check", &at(20, 1), 100).unwrap();
    assert!(matches["total"].as_u64().unwrap() > 0);
}

#[test]
fn real_memories_stay_distinct_from_speech_and_distant_callers_use_conversation_presence() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let ash = source::load_world(examples.join("ash_archive"), None).unwrap();
    let memory = character::conversation(
        &ash,
        "char_0T8KVZX33X5FQJFYPMH70FADD9",
        "conv_3RZAWPJ5WT20F3T5NAQ91Y4NCB",
        &at(124, 0),
    )
    .unwrap();
    let quote = &memory["recollection"]["rememberedQuotes"][0];
    assert_eq!(quote["text"], "Caldrin will know the road.");
    assert_eq!(quote["fidelity"], "approximate");
    assert_eq!(quote["sourceTurnId"], "turn_245XBXVYGX2NV0KFSJYEAHJ2F9");
    let actual = memory["beats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|beat| beat["turnId"] == quote["sourceTurnId"])
        .unwrap();
    assert_ne!(actual["text"], quote["text"]);

    let frontier = source::load_world(examples.join("frontiersmen"), None).unwrap();
    let conversation_id = "conv_6CAEJ0NV40CEBK9A8BGVK3CAFG";
    let caller = "char_5P1T0F31DJHKZHBC49XEHCX891";
    assert!(matches!(
        character::conversation(&frontier, caller, conversation_id, &at(193, 9)),
        Err(Error::NotFound(_))
    ));
    let heard = character::conversation(&frontier, caller, conversation_id, &at(193, 10)).unwrap();
    assert_eq!(heard["beats"].as_array().unwrap().len(), 1);
    assert_eq!(heard["beats"][0]["speakerId"], caller);
    assert_eq!(
        heard["beats"][0]["text"],
        "[a distant boar-like bellow through the rain]"
    );
}
