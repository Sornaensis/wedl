//! Learned words never adopt current canonical assertion contents or metadata.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, character, learned_assertions,
    model::{Record, StoryTime, World},
    runtime, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const CLAIM: &str = "assertion_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const WORDS: &str = "The archive taught me blue.";

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("AUTHOR_TITLE_{id}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!(
            "---\n{}---\nAUTHOR_BODY_NOT_LEARNED\r\n",
            serde_yaml::to_string(&data).unwrap()
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn transition(tick: i64, order: i32, state: &str) -> Json {
    json!({"id":format!("learning-{tick}-{order}"),"time":{"timeline":"main","tick":tick,"order":order},
        "state":state,"confidence":0.4,"acquisition":"heard","note":"AUTHOR_NOTE_NOT_LEARNED"})
}
fn knowledge(number: usize, viewer: &str, claim: &str, transitions: Json) -> Record {
    record(
        "knowledge",
        &format!("know_{number:026}"),
        json!({"knower":viewer,"claim":{"key":"learned-rule","statement":WORDS,"assertion_id":claim},
            "transitions":transitions}),
    )
}
fn records_world(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, Some("AUTHOR_REVISION".into())).unwrap()
}
fn base() -> World {
    let world = records_world(vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":["lore-core-v1","assertions-v1"],"default_timeline":"main",
            "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{},
            "predicates":[{"id":"rule","subject_kinds":["character"],"value_type":{"type":"string"}}]}),
        ),
        record(
            "character",
            PERSON,
            json!({"aliases":["Known character title"]}),
        ),
        record("character", OTHER, json!({})),
        record(
            "assertion",
            CLAIM,
            json!({"domain":"authored.rule","subject":{"kind":"record","id":PERSON},
            "predicate":"rule","value":"AUTHOR_CANONICAL_VALUE","classification":"author-fact","evidence":[],
            "valid":{"kind":"interval","timeline":"main","end":{"timeline":"main","tick":-1,"order":0}},
            "audience":["author"],"perspectives":["author"]}),
        ),
        knowledge(
            1,
            PERSON,
            CLAIM,
            json!([
                transition(0, 0, "rejected"),
                transition(5, 0, "suspected"),
                transition(6, 0, "rejected"),
                transition(7, 0, "forgotten"),
                transition(8, 0, "remembered"),
                transition(9, 0, "forgotten"),
                transition(10, 0, "rejected")
            ]),
        ),
    ]);
    valid(&world);
    world
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut data = old.frontmatter.clone();
    change(&mut data);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        old.body
    );
    let updated = source::parse_record(raw.as_bytes(), &old.source_path).unwrap();
    records_world(
        world
            .records
            .values()
            .map(|record| {
                if record.id == id {
                    updated.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
    )
}
fn set(world: &World, id: &str, field: &str, value: Json) -> World {
    edit(world, id, |data| {
        data[field] = serde_yaml::to_value(value).unwrap()
    })
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    let world = records_world(records);
    valid(&world);
    world
}
fn read(world: &World, tick: i64, order: i32) -> Json {
    learned_assertions::list(world, PERSON, CLAIM, &at(tick, order), 100).unwrap()
}
fn invalid(world: &World) {
    assert!(!validation::validate_world(world).valid);
    let error = learned_assertions::list(world, PERSON, CLAIM, &at(6, 0), 100)
        .unwrap_err()
        .to_string();
    assert!(
        !error.contains("AUTHOR_") && !error.contains("story/"),
        "{error}"
    );
}

#[test]
fn closed_linked_claim_is_valid_and_does_not_require_affirmative_source_history() {
    let source = base();
    assert!(learned_assertions::validate(&source).is_empty());
    for transitions in [json!([]), json!([transition(0, 0, "rejected")])] {
        let candidate = set(&source, KNOWLEDGE, "transitions", transitions);
        valid(&candidate);
        assert_eq!(read(&candidate, 100, 0)["count"], 0);
    }
    for extra in [
        "genealogy",
        "subject",
        "predicate",
        "object",
        "truth",
        "extra",
    ] {
        let candidate = edit(&source, KNOWLEDGE, |data| {
            data["claim"][extra] = Yaml::String("not a linked-claim field".into());
        });
        invalid(&candidate);
    }
    for field in ["key", "statement", "assertion_id"] {
        let candidate = edit(&source, KNOWLEDGE, |data| {
            data["claim"].as_mapping_mut().unwrap().remove(field);
        });
        if field == "assertion_id" {
            // A complete replacement without the link is an ordinary claim.
            valid(&candidate);
            assert_eq!(read(&candidate, 6, 0)["count"], 0);
        } else {
            invalid(&candidate);
        }
        for value in [json!(" "), Json::Null, json!(3), json!({})] {
            let candidate = edit(&source, KNOWLEDGE, |data| {
                data["claim"][field] = serde_yaml::to_value(value).unwrap();
            });
            invalid(&candidate);
        }
    }
}

#[test]
fn exact_assertion_endpoint_and_capabilities_are_source_dependencies_for_every_status() {
    let source = base();
    for target in [
        PERSON,
        "assertion_00000000000000000000000999",
        "AUTHOR_TITLE",
    ] {
        let candidate = edit(&source, KNOWLEDGE, |data| {
            data["claim"]["assertion_id"] = Yaml::String(target.into());
        });
        invalid(&candidate);
        invalid(&set(&candidate, KNOWLEDGE, "status", json!("retired")));
    }
    for capabilities in [json!([]), json!(["assertions-v1"]), json!(["lore-core-v1"])] {
        let candidate = set(&source, WORLD, "capabilities", capabilities);
        invalid(&candidate);
        assert!(!learned_assertions::validate(&candidate).is_empty());
    }
    let retired = set(&source, KNOWLEDGE, "status", json!("retired"));
    valid(&retired);
    assert_eq!(read(&retired, 6, 0)["count"], 0);
}

#[test]
fn learning_rejection_forgetting_and_later_explicit_held_state_follow_exact_history() {
    let source = base();
    for (tick, order, state) in [
        (-1, 0, None),
        (0, 0, None),
        (5, -1, None),
        (5, 0, Some("suspected")),
        (6, 0, Some("rejected")),
        (7, -1, Some("rejected")),
        (7, 0, None),
        (8, 0, Some("remembered")),
        (9, 0, None),
        (10, 0, Some("rejected")),
    ] {
        let value = read(&source, tick, order);
        assert_eq!(
            value["count"],
            usize::from(state.is_some()),
            "{tick}/{order}: {value}"
        );
        if let Some(state) = state {
            assert_eq!(value["items"][0]["state"], state);
            assert_eq!(value["items"][0]["statement"], WORDS);
        }
    }
}

#[test]
fn every_affirmative_state_can_admit_learning_and_rejection_is_not_an_acquisition() {
    let source = base();
    for state in ["accepted", "suspected", "uncertain", "remembered"] {
        let candidate = set(
            &source,
            KNOWLEDGE,
            "transitions",
            json!([transition(5, 0, state), transition(6, 0, "rejected")]),
        );
        valid(&candidate);
        assert_eq!(read(&candidate, 5, 0)["items"][0]["state"], state);
        assert_eq!(read(&candidate, 6, 0)["items"][0]["state"], "rejected");
    }
    let candidate = set(
        &source,
        KNOWLEDGE,
        "transitions",
        json!([transition(0, 0, "rejected"), transition(1, 0, "forgotten")]),
    );
    valid(&candidate);
    assert_eq!(read(&candidate, 0, 0)["count"], 0);
    assert_eq!(read(&candidate, 1, 0)["count"], 0);
}

#[test]
fn native_large_times_and_other_timelines_never_coerce_or_cross_admit() {
    let source = base();
    let tick = 9_007_199_254_740_993;
    let candidate = set(
        &source,
        KNOWLEDGE,
        "transitions",
        json!([
            transition(tick, i32::MIN, "accepted"),
            transition(tick, i32::MAX, "rejected")
        ]),
    );
    valid(&candidate);
    let first = read(&candidate, tick, i32::MIN);
    assert_eq!(
        first["items"][0]["time"],
        json!({"timeline":"main","tick":tick.to_string(),"order":i32::MIN.to_string()})
    );
    assert_eq!(read(&candidate, tick - 1, i32::MAX)["count"], 0);
    assert_eq!(
        read(&candidate, tick, i32::MAX)["items"][0]["state"],
        "rejected"
    );
    let other = StoryTime::new("other", tick, i32::MAX).unwrap();
    assert_eq!(
        learned_assertions::list(&candidate, PERSON, CLAIM, &other, 100).unwrap()["count"],
        0
    );
}

#[test]
fn latest_transition_validation_remains_the_ordinary_core_contract() {
    let source = base();
    for transitions in [
        json!([transition(5, 0, "accepted"), transition(5, 0, "rejected")]),
        json!([transition(6, 0, "accepted"), transition(5, 0, "rejected")]),
        json!([transition(5, 0, "canonical")]),
    ] {
        invalid(&set(&source, KNOWLEDGE, "transitions", transitions));
    }
    for (field, value) in [
        ("confidence", json!(2)),
        ("confidence", json!("0.4")),
        ("acquisition", json!("author secret prose")),
    ] {
        let candidate = edit(&source, KNOWLEDGE, |data| {
            data["transitions"][0][field] = serde_yaml::to_value(value).unwrap();
        });
        invalid(&candidate);
    }
    for value in [json!(5.0), json!("5"), json!(true)] {
        let candidate = edit(&source, KNOWLEDGE, |data| {
            data["transitions"][1]["time"]["tick"] = serde_yaml::to_value(value).unwrap();
        });
        invalid(&candidate);
    }
}

#[test]
fn scopes_admit_only_own_canonical_words_and_never_supply_caller_grants() {
    let source = base();
    for (audience, perspectives, admitted) in [
        (json!(["public"]), json!(["ordinary"]), true),
        (json!([PERSON]), json!([PERSON]), true),
        (json!(["author"]), json!(["ordinary"]), false),
        (json!(["public"]), json!(["author"]), false),
        (json!([]), json!(["ordinary"]), false),
        (json!(["public"]), json!([]), false),
        (json!([OTHER]), json!([OTHER]), false),
        (json!(["public", PERSON]), json!(["ordinary", PERSON]), true),
    ] {
        let scoped = set(
            &set(&source, KNOWLEDGE, "audience", audience),
            KNOWLEDGE,
            "perspectives",
            perspectives,
        );
        valid(&scoped);
        assert_eq!(read(&scoped, 6, 0)["count"], usize::from(admitted));
    }
    let other = set(&source, KNOWLEDGE, "knower", json!(OTHER));
    valid(&other);
    assert_eq!(read(&other, 6, 0)["count"], 0);
    let other_read = learned_assertions::list(&other, OTHER, CLAIM, &at(6, 0), 100).unwrap();
    assert_eq!(other_read["count"], 1);
}

#[test]
fn malformed_scope_arrays_fail_closed_but_empty_arrays_are_valid_withholding() {
    let source = base();
    for field in ["audience", "perspectives"] {
        for value in [
            Json::Null,
            json!("public"),
            json!([3]),
            json!([""]),
            json!(["public", "public"]),
        ] {
            invalid(&set(&source, KNOWLEDGE, field, value));
        }
        let withheld = set(&source, KNOWLEDGE, field, json!([]));
        valid(&withheld);
        assert_eq!(read(&withheld, 6, 0)["count"], 0);
    }
}

#[test]
fn payload_contains_only_literal_learning_and_transition_provenance() {
    let source = base();
    let result = read(&source, 6, 0);
    assert_eq!(
        result["items"][0],
        json!({
            "knowledgeId":KNOWLEDGE,"transitionId":"learning-6-0","assertionId":CLAIM,
            "claim":{"key":"learned-rule","statement":WORDS,"assertion_id":CLAIM},
            "statement":WORDS,"state":"rejected","confidence":0.4,
            "time":{"timeline":"main","tick":"6","order":"0"},
            "target":{"kind":"transition","ownerId":KNOWLEDGE,"id":"learning-6-0"},"citations":[]
        })
    );
    let text = serde_json::to_string(&result).unwrap();
    for forbidden in [
        "AUTHOR_",
        "story/",
        "fingerprint",
        "revision",
        "blobOid",
        "acquisition",
        "sourceEntityId",
        "causingEventId",
        "classification",
        "provenance",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}: {text}");
    }
    assert_eq!(result["perspective"], "character");
    assert_eq!(result["basis"], "explicitly-learned-assertions");
    assert_eq!(result["complete"], true);
}

#[test]
fn late_learning_survives_canonical_expiry_hidden_labels_retirement_and_correction() {
    let source = base();
    let before = read(&source, 6, 0);
    let mut changed = set(&source, CLAIM, "title", json!("REPLACED_CANONICAL_TITLE"));
    changed = set(&changed, CLAIM, "value", json!("REPLACED_CANONICAL_VALUE"));
    changed = set(&changed, CLAIM, "audience", json!(["private"]));
    changed = set(&changed, CLAIM, "perspectives", json!(["private"]));
    changed = set(&changed, CLAIM, "status", json!("retired"));
    valid(&changed);
    assert_eq!(read(&changed, 6, 0), before);
    let correction = "assertion_00000000000000000000000002";
    let mut fields = source.records[CLAIM].frontmatter.clone();
    fields["id"] = Yaml::String(correction.into());
    fields["title"] = Yaml::String("NEW_AUTHOR_CORRECTION".into());
    fields["value"] = Yaml::String("NEW_AUTHOR_CORRECTION_VALUE".into());
    fields["replaces"] = Yaml::String(CLAIM.into());
    let raw = format!("---\n{}---\n", serde_yaml::to_string(&fields).unwrap());
    let replacement =
        source::parse_record(raw.as_bytes(), &format!("story/{correction}.md")).unwrap();
    changed = add(&changed, replacement);
    assert_eq!(read(&changed, 6, 0), before);
    assert_eq!(
        learned_assertions::list(&changed, PERSON, correction, &at(6, 0), 100).unwrap()["count"],
        0
    );
}

#[test]
fn canonical_and_unadmitted_edits_leave_complete_context_search_scores_and_budgets_unchanged() {
    let source = base();
    let before = character::context(&source, PERSON, &at(6, 0), 100_000).unwrap();
    let search = character::search(&source, PERSON, "archive", &at(6, 0), 100).unwrap();
    let budget = character::context(&source, PERSON, &at(6, 0), 1)
        .unwrap_err()
        .to_string();
    assert!(serde_json::to_string(&before).unwrap().contains(WORDS));
    assert!(!serde_json::to_string(&before).unwrap().contains("AUTHOR_"));
    let mut changed = set(&source, CLAIM, "value", json!("CANONICAL_CHANGED_DOCTRINE"));
    changed = set(
        &changed,
        KNOWLEDGE,
        "title",
        json!("CHANGED_AUTHOR_KNOWLEDGE_TITLE"),
    );
    changed = edit(&changed, KNOWLEDGE, |data| {
        data["transitions"][2]["note"] = Yaml::String("HIDDEN_ACQUISITION_NOTE".into());
        data["transitions"][2]["acquisition"] = Yaml::String("read".into());
    });
    let future = knowledge(2, PERSON, CLAIM, json!([transition(30, 0, "accepted")]));
    changed = add(&changed, future);
    let private = knowledge(3, OTHER, CLAIM, json!([transition(1, 0, "accepted")]));
    changed = add(&changed, private);
    valid(&changed);
    assert_eq!(
        character::context(&changed, PERSON, &at(6, 0), 100_000).unwrap(),
        before
    );
    assert_eq!(
        character::search(&changed, PERSON, "archive", &at(6, 0), 100).unwrap(),
        search
    );
    assert_eq!(
        character::context(&changed, PERSON, &at(6, 0), 1)
            .unwrap_err()
            .to_string(),
        budget
    );
    assert!(
        !serde_json::to_string(&character::context(&source, PERSON, &at(0, 0), 100_000).unwrap())
            .unwrap()
            .contains(WORDS)
    );
    assert!(
        !serde_json::to_string(&character::context(&source, PERSON, &at(7, 0), 100_000).unwrap())
            .unwrap()
            .contains(WORDS)
    );
}

#[test]
fn exact_assertion_or_knowledge_targets_do_not_resolve_subjects_titles_or_guessed_links() {
    let source = base();
    let assertion = read(&source, 6, 0);
    let knowledge = learned_assertions::list(&source, PERSON, KNOWLEDGE, &at(6, 0), 100).unwrap();
    assert_eq!(assertion["items"], knowledge["items"]);
    for target in [
        PERSON,
        "AUTHOR_TITLE",
        "assertion_00000000000000000000000999",
        "know_00000000000000000000000999",
        "missing",
    ] {
        let value = learned_assertions::list(&source, PERSON, target, &at(6, 0), 100).unwrap();
        assert_eq!(value["items"], json!([]));
        assert_eq!(value["count"], 0);
        assert_eq!(value["targetId"], target);
    }
    for viewer in [OTHER, PERSON] {
        assert_eq!(
            learned_assertions::list(&source, viewer, CLAIM, &at(0, 0), 100).unwrap()["items"],
            json!([])
        );
    }
}

#[test]
fn unknown_wrong_kind_and_title_selected_viewers_share_one_unavailable_error() {
    let source = base();
    let errors = [
        CLAIM,
        KNOWLEDGE,
        "Known character title",
        "char_00000000000000000000000999",
    ]
    .map(|viewer| {
        learned_assertions::list(&source, viewer, CLAIM, &at(6, 0), 100)
            .unwrap_err()
            .to_string()
    });
    assert!(errors.iter().all(|error| error == &errors[0]));
    assert!(!errors[0].contains("AUTHOR_"));
}

#[test]
fn overlapping_learned_claims_remain_separate_and_sort_by_held_time_then_identity() {
    let source = base();
    let mut other = knowledge(2, PERSON, CLAIM, json!([transition(5, 0, "accepted")]));
    let mut data = other.frontmatter.clone();
    data["claim"]["statement"] = Yaml::String("I learned a contrary red account.".into());
    other = source::parse_record(
        format!("---\n{}---\n", serde_yaml::to_string(&data).unwrap()).as_bytes(),
        &other.source_path,
    )
    .unwrap();
    let world = add(&source, other);
    let world = add(
        &world,
        knowledge(3, PERSON, CLAIM, json!([transition(6, 0, "uncertain")])),
    );
    let result = read(&world, 6, 0);
    assert_eq!(result["count"], 3);
    assert_eq!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["knowledgeId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "know_00000000000000000000000002",
            KNOWLEDGE,
            "know_00000000000000000000000003"
        ]
    );
    assert_eq!(result["items"][0]["state"], "accepted");
    assert_eq!(result["items"][1]["state"], "rejected");
    assert_eq!(result["items"][2]["state"], "uncertain");
}

#[test]
fn complete_limits_count_only_the_selected_admitted_claims() {
    let source = base();
    let world = add(
        &source,
        knowledge(2, PERSON, CLAIM, json!([transition(5, 0, "accepted")])),
    );
    assert!(learned_assertions::list(&world, PERSON, CLAIM, &at(6, 0), 1).is_err());
    assert_eq!(
        learned_assertions::list(&world, PERSON, KNOWLEDGE, &at(6, 0), 1).unwrap()["count"],
        1
    );
    let hidden = set(
        &world,
        "know_00000000000000000000000002",
        "audience",
        json!([]),
    );
    valid(&hidden);
    assert_eq!(
        learned_assertions::list(&hidden, PERSON, CLAIM, &at(6, 0), 1).unwrap()["count"],
        1
    );
    assert_eq!(
        learned_assertions::list(&world, PERSON, CLAIM, &at(7, 0), 1).unwrap()["count"],
        1
    );
    for limit in [0, 1001] {
        assert!(matches!(
            learned_assertions::list(&world, PERSON, CLAIM, &at(6, 0), limit),
            Err(Error::InvalidInput(_))
        ));
    }
}

#[test]
fn native_reads_refuse_forged_parsed_bytes_fingerprints_and_invalid_composed_source() {
    let source = base();
    let mut forged = source.clone();
    forged.records.get_mut(KNOWLEDGE).unwrap().frontmatter["claim"]["statement"] =
        Yaml::String("forged words".into());
    invalid(&forged);
    let mut forged = source.clone();
    forged
        .records
        .get_mut(KNOWLEDGE)
        .unwrap()
        .raw_bytes
        .push(b' ');
    invalid(&forged);
    let mut forged = source.clone();
    forged.fingerprint = "forged-source-fingerprint".into();
    invalid(&forged);
    let invalid_source = set(&source, CLAIM, "value", json!(false));
    invalid(&invalid_source);
}

#[test]
fn request_time_and_target_arguments_reject_invalid_inputs() {
    let source = base();
    for target in ["", " ", "line\nbreak", "line\rbreak"] {
        assert!(matches!(
            learned_assertions::list(&source, PERSON, target, &at(6, 0), 100),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        learned_assertions::list(&source, PERSON, &"x".repeat(257), &at(6, 0), 100),
        Err(Error::InvalidInput(_))
    ));
    let unknown = StoryTime::new("unknown", 6, 0).unwrap();
    assert!(learned_assertions::list(&source, PERSON, CLAIM, &unknown, 100).is_err());
}

#[test]
fn author_knowledge_keeps_its_own_details_and_reads_never_mutate_source_or_clock() {
    let source = base();
    let before = source
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<Vec<_>>();
    let fingerprint = source.fingerprint.clone();
    let author = runtime::knowledge(&source, PERSON, &at(6, 0), false).unwrap();
    assert_eq!(author["items"][0]["claim"]["assertion_id"], CLAIM);
    assert_eq!(author["items"][0]["acquisition"], "heard");
    assert_eq!(author["items"][0]["note"], "AUTHOR_NOTE_NOT_LEARNED");
    assert!(
        author["items"][0]["citation"]["sourcePath"]
            .as_str()
            .unwrap()
            .starts_with("story/")
    );
    assert_eq!(
        runtime::knowledge(&source, PERSON, &at(7, 0), true).unwrap()["items"][0]["state"],
        "forgotten"
    );
    assert_eq!(read(&source, 7, 0)["count"], 0);
    assert_eq!(source.fingerprint, fingerprint);
    assert_eq!(
        source
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<Vec<_>>(),
        before
    );
    assert!(source.world_record().unwrap().get("current_time").is_none());
}
