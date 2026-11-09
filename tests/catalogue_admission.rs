//! Independent catalogue admission cases: hidden source cannot become labels,
//! training text, facets or continuation authority.
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    catalogue,
    model::{Record, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const LATE: &str = "char_00000000000000000000000003";
const PLACE: &str = "loc_00000000000000000000000001";
const ACQUIRED: &str = "loc_00000000000000000000000002";
const PRIVATE: &str = "loc_00000000000000000000000003";
const UNKNOWN: &str = "loc_00000000000000000000000009";
const SCENE: &str = "scene_00000000000000000000000001";
const CLOSED: &str = "scene_00000000000000000000000002";
const OBJECT1: &str = "obj_00000000000000000000000001";
const OBJECT2: &str = "obj_00000000000000000000000002";
const OBJECT3: &str = "obj_00000000000000000000000003";
const KNOW1: &str = "know_00000000000000000000000001";
const KNOW2: &str = "know_00000000000000000000000002";
const LEARNED: &str = "know_00000000000000000000000003";
const ASSERTION: &str = "assertion_00000000000000000000000001";
const THREAD1: &str = "thread_00000000000000000000000001";
const THREAD2: &str = "thread_00000000000000000000000002";
const THREAD3: &str = "thread_00000000000000000000000003";
const CALENDAR: &str = "calendar_00000000000000000000000001";
const ERA1: &str = "era_00000000000000000000000001";
const ERA2: &str = "era_00000000000000000000000002";

fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn wire(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick.to_string(),"order":order.to_string()})
}
fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Title {id}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        body
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn build(records: Vec<Record>) -> World {
    let world = source::world_from_records(PathBuf::new(), records, None).unwrap();
    let checked = validation::validate_world(&world);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    world
}
fn change(world: &World, id: &str, edit: impl FnOnce(&mut Json, &mut String)) -> World {
    let original = &world.records[id];
    let mut fields = serde_json::to_value(&original.frontmatter).unwrap();
    let mut body = original.body.clone();
    edit(&mut fields, &mut body);
    let changed = record(&original.kind, id, fields, &body);
    build(
        world
            .records
            .values()
            .map(|r| {
                if r.id == id {
                    changed.clone()
                } else {
                    r.clone()
                }
            })
            .collect(),
    )
}
fn add(world: &World, extra: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(extra);
    build(records)
}
fn learning(id: &str, tick: i64, order: i32, state: &str) -> Json {
    json!({"id":id,"time":point(tick,order),"state":state,"confidence":0.8,"acquisition":"heard"})
}
fn section(id: &str, heading: &str, markdown: &str) -> Json {
    json!({"id":id,"heading":heading,"markdown":markdown,"valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]})
}
fn fixture() -> World {
    build(vec![
        record(
            "world",
            WORLD,
            json!({"title":"Catalogue admission fixture",
            "capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "state_keys":{},"predicates":[{"id":"report","subject_kinds":["object"],"value_type":{"type":"string"}}],
            "threads":[{"id":THREAD1,"label":"Selected"},{"id":THREAD2,"label":"Other"},{"id":THREAD3,"label":"UNSELECTED_THREAD_LABEL"}]}),
            "World prose\n",
        ),
        record(
            "character",
            VIEWER,
            json!({"title":"Viewer"}),
            "UNTIMED_VIEWER_BODY\n",
        ),
        record(
            "character",
            OTHER,
            json!({"title":"Other observer"}),
            "UNTIMED_OTHER_BODY\n",
        ),
        record(
            "character",
            LATE,
            json!({"title":"Late arrival"}),
            "UNTIMED_LATE_BODY\n",
        ),
        record(
            "location",
            PLACE,
            json!({"title":"Harbor","descriptions":[section("description_00000000000000000000000001","Shared place","A public timeless cedar harbor account.")]}),
            "UNTIMED_PLACE_BODY\n",
        ),
        record(
            "location",
            ACQUIRED,
            json!({"title":"Acquired refuge","descriptions":[section("description_00000000000000000000000001","Known refuge","An acquired cedar refuge account.")]}),
            "UNTIMED_ACQUIRED_BODY\n",
        ),
        record(
            "location",
            PRIVATE,
            json!({"title":"PRIVATE_CANONICAL_TITLE","aliases":["PRIVATE_ALIAS"],"audience":["secret"],"threads":[THREAD1]}),
            "PRIVATE_OPAQUE_BODY\n",
        ),
        record(
            "object",
            OBJECT1,
            json!({"title":"Amber ledger","threads":[THREAD1,THREAD3],"descriptions":[section("description_00000000000000000000000001","Ledger","Remember the amber cedar ledger.")]}),
            "Amber cedar ledger and harbor signal.\n",
        ),
        record(
            "object",
            OBJECT2,
            json!({"title":"Blue ledger","threads":[THREAD2],"descriptions":[section("description_00000000000000000000000001","Blue ledger","Amber cedar fragments in a blue ledger.")]}),
            "Amber amber cedar ledger.\n",
        ),
        record(
            "object",
            OBJECT3,
            json!({"title":"Copper ledger","threads":[THREAD1]}),
            "Amber and cedar.\n",
        ),
        record(
            "scene",
            SCENE,
            json!({"title":"Open inspection","status":"active","location":PLACE,
            "time":{"start":point(0,0),"current":point(10,0)},
            "participants":[{"character":VIEWER},{"character":OTHER},{"character":LATE,"from":point(5,4),"until":point(8,0)}],
            "objects":[OBJECT1,OBJECT2]}),
            "OPEN_AGGREGATE_SECRET\n",
        ),
        record(
            "scene",
            CLOSED,
            json!({"title":"Closed inspection","status":"closed","location":PLACE,
            "time":{"start":point(0,0),"current":point(10,0),"end":point(10,0)},"participants":[{"character":LATE,"from":point(9,0)}]}),
            "CLOSED_AGGREGATE_SECRET\n",
        ),
        record(
            "knowledge",
            KNOW1,
            json!({"title":"Refuge memory","knower":VIEWER,
            "claim":{"key":"known-refuge","statement":"There is a refuge","object":{"entity":ACQUIRED}},
            "transitions":[learning("learned",5,3,"accepted"),learning("forgot",9,0,"forgotten")]}),
            "UNTIMED_KNOWLEDGE_BODY\n",
        ),
        record(
            "knowledge",
            KNOW2,
            json!({"title":"Private place memory","knower":VIEWER,
            "claim":{"key":"private-refuge","statement":"A private place exists","object":{"entity":PRIVATE}},
            "transitions":[learning("learned",0,0,"accepted")]}),
            "UNTIMED_PRIVATE_KNOWLEDGE_BODY\n",
        ),
        record(
            "assertion",
            ASSERTION,
            json!({"title":"AUTHOR_ONLY_ASSERTION_HEADER","domain":"catalogue.report",
            "subject":{"kind":"record","id":OBJECT1},"predicate":"report","value":"AUTHOR_ONLY_CURRENT_VALUE",
            "classification":"author-fact","valid":{"kind":"timeless"},"evidence":[],"audience":["public"],"perspectives":["ordinary"]}),
            "AUTHOR_ONLY_ASSERTION_BODY\n",
        ),
        record(
            "knowledge",
            LEARNED,
            json!({"title":"AUTHOR_ONLY_BELIEF_HEADER","aliases":["AUTHOR_ONLY_BELIEF_ALIAS"],"knower":VIEWER,
            "claim":{"key":"remembered-ledger","statement":"Remembered amber signal","assertion_id":ASSERTION},
            "transitions":[learning("heard",5,3,"accepted"),learning("doubted",7,0,"rejected"),learning("forgot",9,0,"forgotten")]}),
            "AUTHOR_ONLY_BELIEF_PROSE\n",
        ),
    ])
}
fn page(world: &World) -> Json {
    json!({"protocol":"wedl-core-catalogue/v1","operation":"page","fingerprint":world.fingerprint,
        "at":null,"lens":{"kind":"author-all-time","timeline":"main"},
        "filters":{"kinds":[],"profiles":[],"eras":[],"name":""},"sort":"kind-name",
        "q":null,"searchMode":null,"threadIds":[],"limit":100,"cursor":null})
}
fn character_page(world: &World, tick: i64, order: i32) -> Json {
    let mut request = page(world);
    request["at"] = wire(tick, order);
    request["lens"] = json!({"kind":"character","characterId":VIEWER,"sceneId":SCENE});
    request
}
fn labels(mut request: Json, ids: &[&str]) -> Json {
    for key in [
        "filters",
        "sort",
        "q",
        "searchMode",
        "threadIds",
        "limit",
        "cursor",
    ] {
        request.as_object_mut().unwrap().remove(key);
    }
    request["operation"] = json!("labels");
    request["targets"] = json!(
        ids.iter()
            .map(|id| json!({"kind":"record","id":id}))
            .collect::<Vec<_>>()
    );
    request
}
fn read(world: &World, request: &Json) -> Json {
    let decoded = catalogue::decode(&request.to_string()).unwrap();
    catalogue::read(world, &decoded).unwrap()
}
fn refused(world: &World, request: &Json) {
    let decoded = catalogue::decode(&request.to_string()).unwrap();
    assert!(catalogue::read(world, &decoded).is_err());
}
fn ids(value: &Json) -> Vec<String> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}
fn card<'a>(value: &'a Json, id: &str) -> &'a Json {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap()
}
fn public_value(mut value: Json) -> Json {
    fn scrub(value: &mut Json) {
        match value {
            Json::Object(fields) => {
                for key in ["fingerprint", "revision", "nextCursor"] {
                    fields.remove(key);
                }
                fields.values_mut().for_each(scrub);
            }
            Json::Array(values) => values.iter_mut().for_each(scrub),
            _ => {}
        }
    }
    scrub(&mut value);
    value
}

#[test]
fn requested_labels_preserve_order_and_do_not_distinguish_hidden_from_missing() {
    let world = fixture();
    let result = read(
        &world,
        &labels(
            character_page(&world, 5, 3),
            &[PRIVATE, PLACE, UNKNOWN, ACQUIRED],
        ),
    );
    assert_eq!(result["coverage"], "requested-targets");
    assert_eq!(result["total"], 4);
    assert_eq!(result["facets"], Json::Null);
    assert_eq!(result["searchState"], Json::Null);
    assert_eq!(result["nextCursor"], Json::Null);
    for (index, id) in [PRIVATE, PLACE, UNKNOWN, ACQUIRED].iter().enumerate() {
        assert_eq!(
            result["items"][index]["target"],
            json!({"kind":"record","id":id})
        );
    }
    for index in [0, 2] {
        assert_eq!(result["items"][index]["status"], "unavailable");
        assert_eq!(result["items"][index]["title"], Json::Null);
        assert_eq!(result["items"][index]["kind"], Json::Null);
    }
    assert_eq!(result["items"][1]["title"], "Harbor");
    assert_eq!(result["items"][3]["title"], "Acquired refuge");
    assert!(!result.to_string().contains("PRIVATE_"));
}

#[test]
fn held_entity_acquisition_uses_exact_order_and_forgetting_removes_its_grant() {
    let world = fixture();
    for (tick, order, available) in [(5, 2, false), (5, 3, true), (8, 0, true), (9, 0, false)] {
        let result = read(
            &world,
            &labels(character_page(&world, tick, order), &[ACQUIRED]),
        );
        assert_eq!(
            result["items"][0]["status"],
            if available {
                "available"
            } else {
                "unavailable"
            }
        );
    }
    let changed = change(&world, KNOW1, |data, _| {
        data["claim"]["object"] = json!({"text":ACQUIRED});
    });
    let result = read(
        &changed,
        &labels(character_page(&changed, 5, 3), &[ACQUIRED]),
    );
    assert_eq!(result["items"][0]["status"], "unavailable");
}

#[test]
fn scene_roots_follow_actual_presence_and_never_follow_literal_body_references() {
    let world = fixture();
    for (tick, order, available) in [(5, 3, false), (5, 4, true), (8, 0, true), (8, 1, false)] {
        let result = read(
            &world,
            &labels(
                character_page(&world, tick, order),
                &[LATE, OBJECT1, OBJECT3],
            ),
        );
        assert_eq!(
            result["items"][0]["status"],
            if available {
                "available"
            } else {
                "unavailable"
            }
        );
        assert_eq!(result["items"][1]["status"], "available");
        assert_eq!(result["items"][2]["status"], "unavailable");
    }
    let changed = change(&world, SCENE, |_, body| {
        *body = format!("Secret scenery mentions {OBJECT3}.")
    });
    let result = read(
        &changed,
        &labels(character_page(&changed, 5, 3), &[OBJECT3]),
    );
    assert_eq!(result["items"][0]["status"], "unavailable");
    for (tick, order) in [(-1, 0), (10, 1)] {
        refused(&world, &character_page(&world, tick, order));
    }
}

#[test]
fn hidden_and_missing_selected_scenes_have_the_same_unavailable_scope() {
    let world = fixture();
    let hidden = change(&world, SCENE, |data, _| data["audience"] = json!([]));
    let request = character_page(&hidden, 7, 0);
    let first =
        catalogue::read(&hidden, &catalogue::decode(&request.to_string()).unwrap()).unwrap_err();
    let mut request = character_page(&world, 7, 0);
    request["lens"]["sceneId"] = json!("scene_00000000000000000000000009");
    let second =
        catalogue::read(&world, &catalogue::decode(&request.to_string()).unwrap()).unwrap_err();
    assert_eq!(first.to_string(), second.to_string());
    assert!(!first.to_string().contains(SCENE));
}

#[test]
fn learned_assertion_cards_use_literal_words_without_author_metadata_or_canonical_grants() {
    let world = fixture();
    let plain = read(&world, &character_page(&world, 7, 0));
    assert!(
        !ids(&plain)
            .iter()
            .any(|id| id == LEARNED || id == ASSERTION)
    );
    let label = read(
        &world,
        &labels(character_page(&world, 7, 0), &[LEARNED, ASSERTION]),
    );
    assert_eq!(label["items"][0]["title"], "Remembered amber signal");
    assert_eq!(label["items"][1]["status"], "unavailable");
    let mut request = character_page(&world, 7, 0);
    request["q"] = json!("amber");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    let searched = read(&world, &request);
    let learned = card(&searched, LEARNED);
    assert_eq!(learned["title"], "Remembered amber signal");
    assert_eq!(learned["eraIds"], json!([]));
    assert_eq!(learned["profile"], Json::Null);
    assert_eq!(learned["citations"][0]["kind"], "learned");
    assert_eq!(learned["citations"][0]["knowledgeId"], LEARNED);
    assert_eq!(learned["citations"][0]["transitionId"], "doubted");
    assert_eq!(learned["citations"][0]["state"], "rejected");
    assert_eq!(learned["citations"][0]["at"], wire(7, 0));
    assert!(!searched.to_string().contains("AUTHOR_ONLY_"));
    for text in [
        "AUTHOR_ONLY_BELIEF_HEADER",
        "AUTHOR_ONLY_BELIEF_ALIAS",
        "AUTHOR_ONLY_BELIEF_PROSE",
        "AUTHOR_ONLY_CURRENT_VALUE",
    ] {
        request["q"] = json!(text);
        assert_eq!(read(&world, &request)["total"], 0);
    }
}

#[test]
fn learned_cards_support_selected_threads_without_exposing_other_memberships() {
    let world = fixture();
    let world = change(&world, LEARNED, |data, _| {
        data["threads"] = json!([THREAD1, THREAD3]);
    });
    let mut request = character_page(&world, 7, 0);
    request["q"] = json!("amber");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    let unfiltered = read(&world, &request);
    request["threadIds"] = json!([THREAD1]);
    let selected = read(&world, &request);
    let learned = card(&selected, LEARNED);
    assert_eq!(learned["title"], "Remembered amber signal");
    assert_eq!(learned["threadIds"], json!([THREAD1]));
    assert_eq!(learned["eraIds"], json!([]));
    assert_eq!(learned["match"], card(&unfiltered, LEARNED)["match"]);
    assert!(!selected.to_string().contains(THREAD3));
    assert!(!selected.to_string().contains("AUTHOR_ONLY_"));
}

#[test]
fn rejected_without_acquisition_and_forgotten_linked_claims_do_not_get_cards() {
    let world = fixture();
    let changed = change(&world, LEARNED, |data, _| {
        data["transitions"] = json!([learning("never-learned", 0, 0, "rejected")]);
    });
    for (world, tick) in [(&world, 5), (&changed, 7), (&world, 9)] {
        let result = read(world, &labels(character_page(world, tick, 0), &[LEARNED]));
        assert_eq!(result["items"][0]["status"], "unavailable");
    }
}

#[test]
fn author_only_changes_to_a_learned_record_do_not_change_its_public_card_or_ranking() {
    let world = fixture();
    let changed = change(&world, LEARNED, |data, body| {
        data["title"] = json!("HIDDEN_REVISED_HEADER");
        data["aliases"] = json!(["HIDDEN_REVISED_ALIAS"]);
        data["transitions"][1]["acquisition"] = json!("observed");
        *body = "HIDDEN_REVISED_BODY amber amber amber".into();
    });
    let mut request = character_page(&world, 7, 0);
    request["q"] = json!("amber");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    let before = read(&world, &request);
    request["fingerprint"] = json!(changed.fingerprint);
    let after = read(&changed, &request);
    assert_eq!(public_value(before), public_value(after));
}

#[test]
fn hidden_only_source_changes_preserve_selection_facets_scores_and_labels() {
    let world = fixture();
    let changed = change(&world, PRIVATE, |data, body| {
        data["title"] = json!("DIFFERENT_PRIVATE_TITLE");
        data["aliases"] = json!(["amber cedar ledger"]);
        *body = "amber cedar ledger ".repeat(500);
    });
    for character in [false, true] {
        let mut request = if character {
            character_page(&world, 7, 0)
        } else {
            page(&world)
        };
        request["q"] = json!("cedar");
        request["searchMode"] = json!("lexical");
        request["sort"] = json!("relevance");
        let before = read(&world, &request);
        request["fingerprint"] = json!(changed.fingerprint);
        let after = read(&changed, &request);
        assert_eq!(public_value(before), public_value(after));
    }
}

#[test]
fn thread_grouping_preserves_scores_and_exposes_only_selected_memberships() {
    let world = fixture();
    let mut request = page(&world);
    request["filters"]["kinds"] = json!(["object"]);
    request["q"] = json!("amber");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    let all = read(&world, &request);
    assert_eq!(all["total"], 3);
    assert!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["threadIds"] == json!([]))
    );
    request["threadIds"] = json!([THREAD1]);
    let selected = read(&world, &request);
    assert_eq!(selected["total"], 2);
    for id in [OBJECT1, OBJECT3] {
        assert_eq!(card(&selected, id)["match"], card(&all, id)["match"]);
        assert_eq!(card(&selected, id)["threadIds"], json!([THREAD1]));
    }
    assert!(!selected.to_string().contains(THREAD3));
    assert!(!selected.to_string().contains("UNSELECTED_THREAD_LABEL"));
    assert_eq!(
        selected["facets"]["kinds"],
        json!([{"value":"object","count":2}])
    );
}

#[test]
fn paging_keeps_full_selection_totals_and_facets_without_duplicates() {
    let world = fixture();
    let mut request = page(&world);
    request["filters"]["kinds"] = json!(["object"]);
    request["limit"] = json!(1);
    let mut found = Vec::new();
    for page_number in 0..3 {
        let result = read(&world, &request);
        assert_eq!(result["total"], 3);
        assert_eq!(
            result["facets"]["kinds"],
            json!([{"value":"object","count":3}])
        );
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        found.extend(ids(&result));
        if page_number < 2 {
            assert!(result["nextCursor"].is_string());
        } else {
            assert_eq!(result["nextCursor"], Json::Null);
        }
        request["cursor"] = result["nextCursor"].clone();
    }
    assert_eq!(found, vec![OBJECT1, OBJECT2, OBJECT3]);
}

#[test]
fn continuation_is_bound_to_selected_character_and_hidden_source_fingerprint() {
    let world = fixture();
    let mut request = character_page(&world, 7, 0);
    request["filters"]["kinds"] = json!(["object"]);
    request["limit"] = json!(1);
    let first = read(&world, &request);
    request["cursor"] = first["nextCursor"].clone();
    assert!(request["cursor"].is_string());
    let mut other = request.clone();
    other["lens"]["characterId"] = json!(OTHER);
    refused(&world, &other);
    let changed = change(&world, PRIVATE, |data, _| {
        data["title"] = json!("Other hidden title")
    });
    refused(&changed, &request);
    request["fingerprint"] = json!(changed.fingerprint);
    refused(&changed, &request);
    request["cursor"] = Json::Null;
    assert_eq!(read(&changed, &request)["total"], first["total"]);
}

#[test]
fn canonical_unicode_names_and_aliases_match_without_rewriting_authored_titles() {
    let world = fixture();
    let world = change(&world, OBJECT1, |data, _| {
        data["title"] = json!("Éclat");
        data["aliases"] = json!(["A\u{30a}ngstro\u{308}m", "Ångström table", "Cedar__ledger"]);
    });
    let world = change(&world, OBJECT2, |data, _| {
        data["title"] = json!("E\u{301}clat")
    });
    let world = change(&world, OBJECT3, |data, _| data["title"] = json!("Straße"));
    let mut request = page(&world);
    request["filters"]["kinds"] = json!(["object"]);
    request["filters"]["name"] = json!("éCLAT");
    request["sort"] = json!("name-asc");
    let result = read(&world, &request);
    assert_eq!(ids(&result), vec![OBJECT1, OBJECT2]);
    assert_eq!(result["items"][0]["title"], "Éclat");
    assert_eq!(result["items"][1]["title"], "E\u{301}clat");
    request["sort"] = json!("name-desc");
    assert_eq!(ids(&read(&world, &request)), vec![OBJECT2, OBJECT1]);
    request["filters"]["name"] = json!("ÅNGSTRÖM");
    assert_eq!(ids(&read(&world, &request)), vec![OBJECT1]);
    request["filters"]["name"] = json!("STRASSE");
    assert_eq!(ids(&read(&world, &request)), vec![OBJECT3]);
    for name in ["ngstr-m-table", "cedar-ledger"] {
        request["filters"]["name"] = json!(name);
        assert_eq!(ids(&read(&world, &request)), vec![OBJECT1]);
    }
}

#[test]
fn completed_aggregate_labels_start_at_end_and_open_metadata_does_not_admit_prose() {
    let world = fixture();
    let mut request = page(&world);
    request["at"] = wire(5, 3);
    request["lens"] = json!({"kind":"author-as-of"});
    let before = read(&world, &labels(request.clone(), &[SCENE, CLOSED]));
    assert_eq!(before["items"][0]["status"], "available");
    assert_eq!(before["items"][1]["status"], "unavailable");
    request["at"] = wire(10, 0);
    let after = read(&world, &labels(request.clone(), &[SCENE, CLOSED]));
    assert_eq!(after["items"][1]["status"], "available");
    request["q"] = json!("OPEN_AGGREGATE_SECRET");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    assert_eq!(read(&world, &request)["total"], 0);
    request["q"] = json!("CLOSED_AGGREGATE_SECRET");
    assert_eq!(ids(&read(&world, &request)), vec![CLOSED]);
    request["at"] = Json::Null;
    request["lens"] = json!({"kind":"author-all-time","timeline":"main"});
    request["q"] = json!("OPEN_AGGREGATE_SECRET");
    assert_eq!(read(&world, &request)["total"], 0);
    request["q"] = json!("CLOSED_AGGREGATE_SECRET");
    assert_eq!(ids(&read(&world, &request)), vec![CLOSED]);
}

#[test]
fn all_time_lenses_still_isolate_scoped_record_and_description_timelines() {
    const MAIN_CONCEPT: &str = "concept_00000000000000000000000001";
    const OTHER_CONCEPT: &str = "concept_00000000000000000000000002";
    let world = fixture();
    let world = change(&world, WORLD, |data, _| {
        data["profiles"] = json!([
            {"id":"catalogue-note","label":"Notes","fields":{"type":"object","required":{"note":{"type":"string"}},"optional":{}}}
        ])
    });
    let mut world = world;
    for (id, timeline) in [(MAIN_CONCEPT, "main"), (OTHER_CONCEPT, "other")] {
        world = add(
            &world,
            record(
                "lore-concept",
                id,
                json!({"title":format!("{timeline} concept"),"domain":"catalogue.note",
            "profile":"catalogue-note","values":{"note":"Safe typed words"},"audience":["public"],"perspectives":["ordinary"],
            "valid":{"kind":"interval","timeline":timeline,"start":{"timeline":timeline,"tick":50,"order":0}}}),
                "Concept body\n",
            ),
        );
    }
    let world = change(&world, ASSERTION, |data, _| {
        data["valid"] = json!({"kind":"interval","timeline":"other"})
    });
    let world = change(&world, OBJECT2, |data, _| {
        data["descriptions"][0]["valid"] = json!({"kind":"interval","timeline":"other"});
        data["descriptions"][0]["markdown"] = json!("Otherdescriptionword");
    });
    let mut request = page(&world);
    let main = read(
        &world,
        &labels(request.clone(), &[MAIN_CONCEPT, OTHER_CONCEPT, ASSERTION]),
    );
    assert_eq!(main["items"][0]["status"], "available");
    assert_eq!(main["items"][1]["status"], "unavailable");
    assert_eq!(main["items"][2]["status"], "unavailable");
    request["lens"]["timeline"] = json!("other");
    let other = read(
        &world,
        &labels(request.clone(), &[MAIN_CONCEPT, OTHER_CONCEPT, ASSERTION]),
    );
    assert_eq!(other["items"][0]["status"], "unavailable");
    assert_eq!(other["items"][1]["status"], "available");
    assert_eq!(other["items"][2]["status"], "available");
    request["q"] = json!("Otherdescriptionword");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    assert_eq!(ids(&read(&world, &request)), vec![OBJECT2]);
    request["lens"]["timeline"] = json!("main");
    assert_eq!(read(&world, &request)["total"], 0);
}

#[test]
fn all_time_search_keeps_finite_structured_evidence_without_a_synthetic_late_horizon() {
    const ENV: &str = "env_00000000000000000000000001";
    let world = fixture();
    let world = add(
        &world,
        record(
            "environment",
            ENV,
            json!({"title":"Earlier weather",
        "time":{"start":point(0,0),"end":point(6,0)},"targets":[PLACE],"conditions":{"weather":"Earliervaporword"}}),
            "Environment body\n",
        ),
    );
    let world = change(&world, SCENE, |data, _| {
        data["observations"] = json!([
            {"id":"early-observation","at":point(2,0),"until":point(3,0),"text":"Earlierobservationword","audience":["participants"]}
        ])
    });
    let mut request = page(&world);
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    for (query, id) in [("Earliervaporword", ENV), ("Earlierobservationword", SCENE)] {
        request["q"] = json!(query);
        request["at"] = Json::Null;
        request["lens"] = json!({"kind":"author-all-time","timeline":"main"});
        assert_eq!(ids(&read(&world, &request)), vec![id]);
        request["lens"] = json!({"kind":"author-as-of"});
        request["at"] = wire(3, 0);
        assert_eq!(ids(&read(&world, &request)), vec![id]);
        request["at"] = wire(7, 0);
        assert_eq!(read(&world, &request)["total"], 0);
        request["at"] = Json::Null;
        request["lens"] = json!({"kind":"author-all-time","timeline":"other"});
        assert_eq!(read(&world, &request)["total"], 0);
    }
}

#[test]
fn era_facets_come_from_typed_annotations_on_admitted_owners_only() {
    let world = fixture();
    let world = change(&world, WORLD, |data, _| {
        data["chronology"] = json!({"calendars":[{"id":CALENDAR,"label":"Ledger","months":[{"number":1,"days":30}],
            "rule":{"kind":"cycle","period":1,"overrides":[]},"epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}}],
            "eras":[{"id":ERA1,"label":"Shared era","calendar_id":CALENDAR,"aliases":[],"display_year_zero":true,
                "display_epoch":{"display_year":0,"machine_year":0},"provenance":["annals"]},
                {"id":ERA2,"label":"PRIVATE_ERA_LABEL","calendar_id":CALENDAR,"aliases":[],"display_year_zero":true,
                "display_epoch":{"display_year":0,"machine_year":0},"provenance":["annals"]}],"anchors":[]});
    });
    let world = change(&world, OBJECT1, |data, _| {
        data["chronology"] = json!([
            {"id":"chronology_00000000000000000000000001","provenance":[ERA2],"x-scope":{"eras":[ERA2]},
             "value":{"conflict":{"claims":[{"era":{"era_id":ERA1,"year":0}},{"era":{"era_id":ERA1,"year":1}}]}}}
        ])
    });
    let world = change(&world, PRIVATE, |data, _| {
        data["chronology"] = json!([
            {"id":"chronology_00000000000000000000000001","provenance":["hidden annals"],"value":{"era":{"era_id":ERA2,"year":0}}}
        ])
    });
    let result = read(&world, &page(&world));
    assert_eq!(card(&result, OBJECT1)["eraIds"], json!([ERA1]));
    assert_eq!(
        result["facets"]["eras"],
        json!([{"id":ERA1,"label":"Shared era","count":1}])
    );
    assert!(!result.to_string().contains(ERA2));
    assert!(!result.to_string().contains("PRIVATE_ERA_LABEL"));
    let mut request = page(&world);
    request["filters"]["eras"] = json!([ERA1]);
    assert_eq!(ids(&read(&world, &request)), vec![OBJECT1]);
}

#[test]
fn direct_native_requests_refuse_parsed_source_or_fingerprint_forgery() {
    let world = fixture();
    let request = page(&world);
    let mut forged = world.clone();
    forged.records.get_mut(OBJECT1).unwrap().title = "Forged parsed title".into();
    refused(&forged, &request);
    let mut forged = world.clone();
    forged.fingerprint = "1".repeat(64);
    let mut current = request.clone();
    current["fingerprint"] = json!(forged.fingerprint);
    refused(&forged, &current);
}

#[test]
fn hidden_effect_operands_are_omitted_before_search_text_is_allocated_or_charged() {
    const EVENT: &str = "event_00000000000000000000000001";
    let world = fixture();
    let world = change(&world, WORLD, |data, _| {
        data["state_keys"] = json!({"location":{"note":{"type":"string"}}})
    });
    let world = add(
        &world,
        record(
            "event",
            EVENT,
            json!({"title":"Public event","time":point(4,0),"location":PLACE,
        "participants":[],"effects":[{"id":"effect_00000000000000000000000001","target":PRIVATE,"key":"note","operation":"set","value":"hidden"}]}),
            "Visiblefinding\n",
        ),
    );
    let changed = change(&world, EVENT, |data, _| {
        data["effects"][0]["value"] = json!("x".repeat(600_000))
    });
    let mut request = page(&world);
    request["q"] = json!("Visiblefinding");
    request["searchMode"] = json!("lexical");
    request["sort"] = json!("relevance");
    let before = read(&world, &request);
    assert_eq!(ids(&before), vec![EVENT]);
    request["fingerprint"] = json!(changed.fingerprint);
    let after = read(&changed, &request);
    assert_eq!(public_value(before), public_value(after));
}
