//! Interruption references retain exact local identity without granting access
//! to the interrupted speech or expanding literal retrieval text.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeSet, path::PathBuf};
use wedl::{
    character,
    compiler::{self, AuthorSearchSelection},
    consequence_semantics::Time,
    manuscript,
    model::{Record, StoryTime, World},
    record_edit,
    retcon_impact::{self, Reference, Request},
    retrieval::Mode,
    reveal::{self, CanonicalScope, Lens, ReaderGrant},
    runtime, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const SPEAKER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const CONVERSATION: &str = "conv_00000000000000000000000001";
const NEW_CONVERSATION: &str = "conv_00000000000000000000000002";
const REVISION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TICK: i64 = 9_007_199_254_740_993;
const MODES: [Mode; 3] = [Mode::Lexical, Mode::Semantic, Mode::Hybrid];

fn point(order: i32) -> Json {
    json!({"timeline":"main","tick":TICK,"order":order})
}
fn at(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}
fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut value =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Fixture {kind}")});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!("---\n{}---\n{body}", serde_yaml::to_string(&value).unwrap()).as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn fixture(hidden_target: bool) -> World {
    let world = source::world_from_records(
        PathBuf::from("/conversation-interruptions-no-filesystem"),
        vec![
            record("world", WORLD, json!({"capabilities":["descriptions-v1"],"default_timeline":"main","timelines":[{"id":"main","label":"Main"}],"state_keys":{}}), ""),
            record("character", VIEWER, json!({"initial_state":{}}), ""),
            record("character", SPEAKER, json!({"initial_state":{}}), ""),
            record("location", PLACE, json!({"descriptions":[{"id":"description_00000000000000000000000002","heading":"Public vocabulary","markdown":"A lantern illuminates a conversation, a reply, and a raised hand.","valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]}]}), ""),
            record("conversation", CONVERSATION, json!({
                "status":"closed","location":PLACE,"time":{"start":point(0),"end":point(10)},
                "participants":[{"character":VIEWER},{"character":SPEAKER}],
                "turns":[
                    {"id":"earlier","at":point(1),"speaker":SPEAKER,"text":"Earlier lantern speech","audience":[if hidden_target {"author"} else {"participants"}]},
                    {"id":"gesture","kind":"action","at":point(2),"actors":[SPEAKER],"text":"A hand rises","audience":["participants"]},
                    {"id":"reply","kind":"speech","at":point(3),"speaker":VIEWER,"text":"Lantern reply","audience":["participants"],"interrupts":"earlier"}
                ],"recollections":[],
                "descriptions":[{"id":"description_00000000000000000000000001","heading":"Discussion","markdown":"A lantern was discussed.","valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]}]
            }), "Visible conversation prose.\n"),
        ],
        None,
    ).unwrap();
    assert_valid(&world);
    world
}
fn assert_valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn changed(world: &World, edit: impl FnOnce(&mut Yaml)) -> World {
    let original = &world.records[CONVERSATION];
    let mut fields = original.frontmatter.clone();
    edit(&mut fields);
    let updated = source::parse_record(
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            original.body
        )
        .as_bytes(),
        &original.source_path,
    )
    .unwrap();
    let mut records = world.records.clone();
    records.insert(updated.id.clone(), updated);
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn patch_turns(world: &World) -> String {
    serde_yaml::to_string(
        &json!({"kind":"conversation","turns":world.records[CONVERSATION].get("turns").unwrap()}),
    )
    .unwrap()
}
fn beat<'a>(packet: &'a Json, key: &str, id: &str) -> &'a Json {
    packet["beats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|beat| beat[key] == id)
        .unwrap()
}
fn without_fingerprints(mut value: Json) -> Json {
    fn remove(value: &mut Json) {
        match value {
            Json::Object(fields) => {
                fields.remove("fingerprint");
                for value in fields.values_mut() {
                    remove(value);
                }
            }
            Json::Array(values) => values.iter_mut().for_each(remove),
            _ => {}
        }
    }
    remove(&mut value);
    value
}

#[test]
fn literal_interruption_survives_typed_construction_and_author_transcript() {
    let world = fixture(false);
    let updated = record_edit::update(&world, CONVERSATION, &patch_turns(&world)).unwrap();
    assert_eq!(updated.raw_bytes, world.records[CONVERSATION].raw_bytes);

    let mut draft = world.records[CONVERSATION].frontmatter.clone();
    draft.as_mapping_mut().unwrap().remove("schema");
    draft["id"] = NEW_CONVERSATION.into();
    let created = record_edit::create(&world, &serde_yaml::to_string(&draft).unwrap()).unwrap();
    assert_eq!(created.get("turns").unwrap()[2]["interrupts"], "earlier");

    let author = runtime::conversation(&world, CONVERSATION, &at(3)).unwrap();
    assert_eq!(beat(&author, "id", "reply")["interrupts"], "earlier");
    assert_eq!(beat(&author, "id", "reply")["text"], "Lantern reply");
    assert!(beat(&author, "id", "earlier").get("interrupts").is_none());
    assert!(beat(&author, "id", "gesture").get("interrupts").is_none());
    assert!(
        runtime::conversation(&world, CONVERSATION, &at(2)).unwrap()["beats"]
            .as_array()
            .unwrap()
            .iter()
            .all(|beat| beat["id"] != "reply")
    );

    let character = character::conversation(&world, VIEWER, CONVERSATION, &at(3)).unwrap();
    assert_eq!(beat(&character, "turnId", "reply")["interrupts"], "earlier");
}

#[test]
fn interruption_targets_are_nonnull_exact_local_speech_references() {
    let world = fixture(false);
    for target in [
        json!(null),
        json!(false),
        json!(42),
        json!(["earlier"]),
        json!(""),
        json!("missing"),
        json!("Earlier"),
        json!("gesture"),
        json!("reply"),
        json!(SPEAKER),
    ] {
        let invalid = changed(&world, |fields| {
            fields["turns"][2]["interrupts"] = serde_yaml::to_value(&target).unwrap()
        });
        assert!(
            !validation::validate_world(&invalid).valid,
            "accepted {target}"
        );
        if !target.is_string() || target == "" {
            assert!(record_edit::update(&world, CONVERSATION, &patch_turns(&invalid)).is_err());
        }
    }
    let action_reference = changed(&world, |fields| {
        fields["turns"][1]["interrupts"] = "earlier".into()
    });
    assert!(!validation::validate_world(&action_reference).valid);
    assert!(record_edit::update(&world, CONVERSATION, &patch_turns(&action_reference)).is_err());
    let unknown = changed(&world, |fields| {
        fields["turns"][2]["interruption"] = "earlier".into()
    });
    assert!(record_edit::update(&world, CONVERSATION, &patch_turns(&unknown)).is_err());
}

#[test]
fn interruption_requires_both_earlier_source_position_and_strictly_earlier_exact_time() {
    let world = fixture(false);
    for order in [3, 4] {
        let invalid = changed(&world, |fields| {
            fields["turns"][0]["at"] = serde_yaml::to_value(point(order)).unwrap()
        });
        assert!(!validation::validate_world(&invalid).valid);
    }
    let forward = changed(&world, |fields| {
        fields["turns"].as_sequence_mut().unwrap().swap(0, 2)
    });
    assert!(!validation::validate_world(&forward).valid);
    // This addition does not impose a new ordering rule on legacy arrays that
    // contain no interruption reference.
    let legacy = changed(&forward, |fields| {
        fields["turns"][0]
            .as_mapping_mut()
            .unwrap()
            .remove("interrupts");
    });
    assert_valid(&legacy);
}

#[test]
fn replacing_turns_preserves_native_optional_nulls_without_granting_an_audience() {
    let world = fixture(false);
    let nullable = changed(&world, |fields| {
        for field in ["audience", "delivery", "addressee"] {
            fields["turns"][0][field] = Yaml::Null;
        }
        fields["turns"][1]["audience"] = Yaml::Null;
    });
    assert_valid(&nullable);
    let rebuilt = record_edit::update(&nullable, CONVERSATION, &patch_turns(&nullable)).unwrap();
    assert_eq!(rebuilt.raw_bytes, nullable.records[CONVERSATION].raw_bytes);
    let packet = character::conversation(&nullable, VIEWER, CONVERSATION, &at(3)).unwrap();
    assert_eq!(packet["beats"].as_array().unwrap().len(), 1);
    assert!(beat(&packet, "turnId", "reply").get("interrupts").is_none());
    for field in ["id", "kind", "at", "speaker", "text", "interrupts"] {
        let invalid = changed(&nullable, |fields| fields["turns"][2][field] = Yaml::Null);
        assert!(record_edit::update(&nullable, CONVERSATION, &patch_turns(&invalid)).is_err());
    }
    for field in ["audience", "addressee"] {
        let invalid = changed(&nullable, |fields| {
            fields["turns"][2][field] = Yaml::Sequence(vec![Yaml::Null]);
        });
        assert!(record_edit::update(&nullable, CONVERSATION, &patch_turns(&invalid)).is_err());
    }
}

#[test]
fn hidden_target_identity_and_content_never_change_character_packets_or_retrieval() {
    let world = fixture(true);
    let hidden_id = format!("hidden_{}", "x".repeat(80_000));
    let renamed = changed(&world, |fields| {
        fields["turns"][0]["id"] = hidden_id.clone().into();
        fields["turns"][0]["text"] = "UNHEARD_NEEDLE unrelated private wording".into();
        fields["turns"][2]["interrupts"] = hidden_id.clone().into();
    });
    let omitted = changed(&world, |fields| {
        fields["turns"][2]
            .as_mapping_mut()
            .unwrap()
            .remove("interrupts");
    });
    for variant in [&renamed, &omitted] {
        assert_valid(variant);
        assert_eq!(
            character::conversation(&world, VIEWER, CONVERSATION, &at(3)).unwrap(),
            character::conversation(variant, VIEWER, CONVERSATION, &at(3)).unwrap(),
        );
        let context = character::context(&world, VIEWER, &at(3), 100_000).unwrap();
        assert_eq!(
            context,
            character::context(variant, VIEWER, &at(3), 100_000).unwrap()
        );
        for mode in MODES {
            assert_eq!(
                character::search_with_mode(&world, VIEWER, "lantern", &at(3), 10, mode).unwrap(),
                character::search_with_mode(variant, VIEWER, "lantern", &at(3), 10, mode).unwrap(),
            );
        }
    }
    let heard_by_speaker = character::conversation(&world, SPEAKER, CONVERSATION, &at(3)).unwrap();
    assert_eq!(
        beat(&heard_by_speaker, "turnId", "reply")["interrupts"],
        "earlier"
    );
}

#[test]
fn dated_author_retrieval_ignores_interruption_metadata_and_future_members() {
    let world = fixture(false);
    let omitted = changed(&world, |fields| {
        fields["turns"][2]
            .as_mapping_mut()
            .unwrap()
            .remove("interrupts");
    });
    let future = changed(&world, |fields| {
        fields["turns"].as_sequence_mut().unwrap().extend([
            serde_yaml::to_value(json!({"id":"future_target","at":point(8),"speaker":SPEAKER,"text":"UNADMITTED_NEEDLE lantern","audience":["participants"]})).unwrap(),
            serde_yaml::to_value(json!({"id":"future_reply","at":point(9),"speaker":VIEWER,"text":"UNADMITTED_NEEDLE reply","audience":["participants"],"interrupts":"future_target"})).unwrap(),
        ]);
    });
    let selection = AuthorSearchSelection {
        at: Some(at(3)),
        thread_ids: None,
    };
    for mode in MODES {
        let baseline =
            compiler::search_with_selection(&world, "lantern", 10, None, mode, &selection).unwrap();
        for variant in [&omitted, &future] {
            assert_valid(variant);
            let compared =
                compiler::search_with_selection(variant, "lantern", 10, None, mode, &selection)
                    .unwrap();
            assert_eq!(
                without_fingerprints(baseline.clone()),
                without_fingerprints(compared)
            );
        }
    }
}

#[test]
fn retcon_impact_resolves_interruption_as_an_exact_local_speech_dependency() {
    let world = fixture(false);
    let request = Request {
        target: Reference::Turn {
            entity_id: CONVERSATION.into(),
            turn_id: "earlier".into(),
        },
        at: Time {
            timeline: "main".into(),
            tick: TICK.to_string(),
            order: "3".into(),
        },
        depth: 4,
        max_items: 100,
    };
    let report = retcon_impact::evaluate(&world, &world, &request, &"a".repeat(64)).unwrap();
    let edges: Vec<_> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["citation"]["section"] == "turns[2].interrupts")
        .collect();
    assert!(!edges.is_empty());
    for edge in edges {
        assert_eq!(
            edge["target"],
            serde_json::to_value(&request.target).unwrap()
        );
        assert_eq!(edge["relation"], "source-member");
        assert_eq!(edge["citation"]["memberId"], "reply");
        assert_eq!(edge["citation"]["sourceOrdinal"], 2);
    }
    let mut before = request;
    before.at.order = "2".into();
    let report = retcon_impact::evaluate(&world, &world, &before, &"a".repeat(64)).unwrap();
    assert!(
        report["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["citation"]["section"] != "turns[2].interrupts")
    );
}

#[test]
fn reveal_record_and_description_admissions_never_expand_interruption_references() {
    const SEGMENT: &str = "segment_00000000000000000000000001";
    const WORK: &str = "work_00000000000000000000000001";
    const CHAPTER: &str = "chapter_00000000000000000000000001";
    const PLAN: &str = "reveal_00000000000000000000000001";
    let base = fixture(true);
    // Hearing a beat does not itself acquire the conversation's source prose.
    // Supply the explicit ordinary record acquisition required by this lens.
    let mut records: Vec<_> = base.records.values().cloned().collect();
    records.push(record(
        "knowledge",
        "know_00000000000000000000000001",
        json!({"knower":VIEWER,"claim":{"key":"known-conversation","statement":"I know this conversation.","object":{"entity":CONVERSATION}},"transitions":[{"id":"learned","time":point(3),"state":"accepted","confidence":1,"acquisition":"experienced"}]}),
        "",
    ));
    let world =
        source::world_from_records(base.root.clone(), records, Some(REVISION.into())).unwrap();
    assert_valid(&world);
    let changed = changed(&world, |fields| {
        fields["turns"][0]["id"] = "hidden_reference_change".into();
        fields["turns"][0]["text"] = "UNHEARD_NEEDLE".into();
        fields["turns"][2]["interrupts"] = "hidden_reference_change".into();
    });
    let documents = [
        json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":SEGMENT,"title":"A segment","source_links":[],"placement":point(10),"pov_character_id":VIEWER}),
        json!({"schema":"wedl-author/v1","kind":"work","id":WORK,"title":"A work","duplicate_policy":"allow-explicit-reuse","chapters":[{"id":CHAPTER,"title":"A chapter","segments":[{"kind":"author-record","id":SEGMENT}]}]}),
        json!({"schema":"wedl-author/v1","kind":"reveal-plan","id":PLAN,"title":"A plan","work":{"kind":"author-record","id":WORK},"entries":[{"milestone":{"chapter_id":CHAPTER,"segment_index":0},"admissions":[{"kind":"record","target":{"kind":"record","id":CONVERSATION}}]}]}),
    ].into_iter().map(|fields| {
        let path = manuscript::document_path(fields["kind"].as_str().unwrap(), fields["id"].as_str().unwrap()).unwrap();
        let mut record = manuscript::parse_document(format!("---\n{}---\n", serde_yaml::to_string(&fields).unwrap()).as_bytes(), &path).unwrap();
        record.blob_oid = Some("b".repeat(40));
        record
    }).collect();
    let authors = manuscript::snapshot_from_records(documents, Some(REVISION.into())).unwrap();
    for lens in [
        Lens::AuthorAsOf,
        Lens::Character {
            character_id: VIEWER.into(),
        },
    ] {
        let grant = ReaderGrant {
            reader_id: "reader:interruptions".into(),
            plan_id: Some(PLAN.into()),
            revision: REVISION.into(),
            canonical_scope: CanonicalScope {
                world_id: WORLD.into(),
                records: BTreeSet::from([CONVERSATION.into(), VIEWER.into()]),
                sections: None,
            },
            at: at(10),
            lens,
            audiences: BTreeSet::from(["public".into()]),
            perspectives: BTreeSet::from(["ordinary".into()]),
        };
        let present = |world| {
            reveal::present(
                world,
                &authors,
                Some(PLAN),
                Some(0),
                "reader:interruptions",
                &grant,
            )
            .unwrap()
        };
        let baseline = present(&world);
        assert!(!baseline.is_empty());
        assert_eq!(baseline, present(&changed));
        let wire = serde_json::to_string(&baseline).unwrap();
        assert!(!wire.contains("interrupts"));
        assert!(!wire.contains("UNHEARD_NEEDLE"));
        assert!(!wire.contains("hidden_reference_change"));
    }
}
