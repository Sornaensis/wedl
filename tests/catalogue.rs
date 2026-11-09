//! Request, finite selection, cursor and retrieval boundaries for the native catalogue.
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    catalogue,
    model::{Record, World},
    source, validation,
};

const W: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const B: &str = "char_00000000000000000000000002";
const C: &str = "char_00000000000000000000000003";
const L: &str = "loc_00000000000000000000000001";
const T: &str = "thread_00000000000000000000000001";
fn id(prefix: &str, n: usize) -> String {
    format!("{prefix}_{n:026}")
}
fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!("---\n{}---\n{body}", serde_yaml::to_string(&value).unwrap());
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn build(extra: Vec<Record>, world_fields: Json) -> World {
    let mut fields = json!({"title":"World","capabilities":[],"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"current_time":{"timeline":"main","tick":20,"order":0},"state_keys":{},"relationship_metrics":{},"threads":[{"id":T,"label":"Selected"}]});
    fields
        .as_object_mut()
        .unwrap()
        .extend(world_fields.as_object().unwrap().clone());
    let mut records = vec![record("world", W, fields, "")];
    records.extend(extra);
    let world = source::world_from_records(PathBuf::new(), records, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}
fn locations(count: usize) -> World {
    build(
        (1..=count)
            .map(|n| {
                record(
                    "location",
                    &id("loc", n),
                    json!({"title":format!("Location {n:05}")}),
                    "Cedar ledger.",
                )
            })
            .collect(),
        json!({}),
    )
}
fn page(world: &World) -> Json {
    json!({"protocol":catalogue::PROTOCOL,"operation":"page","fingerprint":world.fingerprint,"at":null,"lens":{"kind":"author-all-time","timeline":"main"},
        "filters":{"kinds":["location"],"profiles":[],"eras":[],"name":""},"sort":"name-asc","q":null,"searchMode":null,"threadIds":[],"limit":10,"cursor":null})
}
fn read(world: &World, request: &Json) -> Result<Json, wedl::Error> {
    catalogue::read(world, &catalogue::decode(&request.to_string())?)
}
fn labels(world: &World, targets: Vec<String>) -> Json {
    json!({"protocol":catalogue::PROTOCOL,"operation":"labels","fingerprint":world.fingerprint,"at":null,"lens":{"kind":"author-all-time","timeline":"main"},"targets":targets.iter().map(|id|json!({"kind":"record","id":id})).collect::<Vec<_>>()})
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect()
}
fn error(world: &World, request: &Json, code: &str) {
    let e = read(world, request).unwrap_err();
    assert!(e.to_string().contains(code), "{e}");
}

#[test]
fn decoder_is_closed_at_every_object_and_rejects_duplicate_fields() {
    let world = locations(2);
    let original = page(&world);
    for path in ["outer", "filters", "lens"] {
        let mut value = original.clone();
        match path {
            "outer" => {
                value["scope"] = json!({});
            }
            "filters" => {
                value["filters"]["grant"] = json!(true);
            }
            _ => {
                value["lens"]["knownRecords"] = json!([]);
            }
        }
        assert!(catalogue::decode(&value.to_string()).is_err());
    }
    for field in [
        "protocol",
        "operation",
        "fingerprint",
        "at",
        "lens",
        "filters",
        "sort",
        "q",
        "searchMode",
        "threadIds",
        "limit",
        "cursor",
    ] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            catalogue::decode(&value.to_string()).is_err(),
            "missing {field}"
        );
    }
    for raw in [
        original
            .to_string()
            .replace("\"limit\":10", "\"limit\":10,\"limit\":20"),
        original
            .to_string()
            .replace("\"name\":\"\"", "\"name\":\"\",\"name\":\"shadow\""),
        original.to_string().replace(
            "\"kind\":\"author-all-time\"",
            "\"kind\":\"author-all-time\",\"kind\":\"character\"",
        ),
    ] {
        assert!(catalogue::decode(&raw).is_err());
    }
    for raw in [
        "null",
        "[]",
        "{",
        "{\"protocol\":NaN}",
        "{\"protocol\":\"\\ud800\"}",
    ] {
        assert!(catalogue::decode(raw).is_err());
    }
}

#[test]
fn request_limit_counts_utf8_bytes_and_allows_exact_bound_whitespace() {
    let world = locations(1);
    let raw = page(&world).to_string();
    let exact = format!(
        "{raw}{}",
        " ".repeat(catalogue::MAX_REQUEST_BYTES - raw.len())
    );
    assert!(catalogue::decode(&exact).is_ok());
    assert!(catalogue::decode(&(exact + " ")).is_err());
    let mut value = page(&world);
    value["filters"]["name"] = json!("é".repeat(257));
    assert!(catalogue::decode(&value.to_string()).is_err());
    value["filters"]["name"] = json!("🙂".repeat(256));
    assert!(catalogue::decode(&value.to_string()).is_ok());
}

#[test]
fn exact_decimal_time_accepts_integer_extrema_and_rejects_coercions() {
    let world = locations(1);
    let mut value = page(&world);
    value["lens"] = json!({"kind":"author-as-of"});
    value["at"] =
        json!({"timeline":"main","tick":i64::MIN.to_string(),"order":i32::MIN.to_string()});
    assert_eq!(read(&world, &value).unwrap()["context"]["at"], value["at"]);
    value["at"]["tick"] = json!(i64::MAX.to_string());
    value["at"]["order"] = json!(i32::MAX.to_string());
    assert!(read(&world, &value).is_ok());
    for bad in [
        json!(0),
        json!(1.0),
        json!("+1"),
        json!("01"),
        json!("-0"),
        json!("1e3"),
        json!(" 1"),
        json!("9223372036854775808"),
        json!(true),
    ] {
        let mut bad_value = value.clone();
        bad_value["at"]["tick"] = bad;
        assert!(catalogue::decode(&bad_value.to_string()).is_err());
    }
    value["at"]["order"] = json!("2147483648");
    assert!(catalogue::decode(&value.to_string()).is_err());
    value["at"]["order"] = json!("0");
    value["at"]["timeline"] = json!("missing");
    error(&world, &value, "CAT_TIME");
}

#[test]
fn paired_search_inputs_closed_modes_and_character_prominence_are_checked_before_source() {
    let world = locations(1);
    let original = page(&world);
    for (q, mode) in [
        (json!("word"), Json::Null),
        (Json::Null, json!("lexical")),
        (json!("word"), json!("fts")),
        (json!(""), json!("hybrid")),
    ] {
        let mut value = original.clone();
        value["q"] = q;
        value["searchMode"] = mode;
        assert!(catalogue::decode(&value.to_string()).is_err());
    }
    for mode in ["lexical", "semantic", "hybrid"] {
        let mut value = original.clone();
        value["q"] = json!("word");
        value["searchMode"] = json!(mode);
        value["sort"] = json!("relevance");
        assert!(catalogue::decode(&value.to_string()).is_ok());
    }
    let mut value = original.clone();
    value["sort"] = json!("relevance");
    assert!(catalogue::decode(&value.to_string()).is_err());
    for sort in ["prominence-low", "prominence-high"] {
        value = original.clone();
        value["lens"] = json!({"kind":"character","characterId":A,"sceneId":id("scene",1)});
        value["at"] = json!({"timeline":"main","tick":"0","order":"0"});
        value["sort"] = json!(sort);
        assert!(catalogue::decode(&value.to_string()).is_err());
    }
}

#[test]
fn selection_arrays_are_unique_bounded_and_profiles_use_current_ids() {
    let world = locations(1);
    let original = page(&world);
    for field in ["kinds", "profiles", "eras"] {
        let mut value = original.clone();
        value["filters"][field] = json!(["same", "same"]);
        assert!(catalogue::decode(&value.to_string()).is_err());
        value["filters"][field] = json!((0..9).map(|n| format!("name{n}")).collect::<Vec<_>>());
        assert!(catalogue::decode(&value.to_string()).is_err());
    }
    let mut value = original.clone();
    value["filters"]["profiles"] = json!([{"id":"notes","version":1}]);
    assert!(catalogue::decode(&value.to_string()).is_err());
    value["filters"]["profiles"] = json!(["notes"]);
    assert!(catalogue::decode(&value.to_string()).is_ok());
    for limit in [json!(0), json!(101), json!(true), json!(1.0), json!("10")] {
        value = original.clone();
        value["limit"] = limit;
        assert!(catalogue::decode(&value.to_string()).is_err());
    }
    value = original.clone();
    value["threadIds"] = json!([id("thread", 2), T]);
    assert!(catalogue::decode(&value.to_string()).is_err());
    value["threadIds"] = json!([id("thread", 2)]);
    error(&world, &value, "CAT_REQUEST");
}

#[test]
fn requested_labels_allow_empty_and_sixty_four_unique_targets_only() {
    let world = locations(1);
    assert_eq!(
        read(&world, &labels(&world, vec![])).unwrap()["items"],
        json!([])
    );
    assert!(
        read(
            &world,
            &labels(&world, (1..=64).map(|n| id("loc", n)).collect())
        )
        .is_ok()
    );
    assert!(
        catalogue::decode(&labels(&world, (1..=65).map(|n| id("loc", n)).collect()).to_string())
            .is_err()
    );
    assert!(catalogue::decode(&labels(&world, vec![L.into(), L.into()]).to_string()).is_err());
    let mut value = labels(&world, vec![L.into()]);
    value["targets"][0]["kind"] = json!("section");
    assert!(catalogue::decode(&value.to_string()).is_err());
    value["targets"][0]["kind"] = json!("record");
    value["targets"][0]["scope"] = json!({});
    assert!(catalogue::decode(&value.to_string()).is_err());
}

#[test]
fn complete_pagination_keeps_order_totals_facets_and_snapshot_without_duplicates() {
    let world = locations(257);
    let mut value = page(&world);
    value["limit"] = json!(37);
    let first = read(&world, &value).unwrap();
    let mut collected = Vec::new();
    let mut result = first.clone();
    loop {
        assert_eq!(result["total"], 257);
        assert_eq!(result["facets"], first["facets"]);
        collected.extend(ids(&result).into_iter().map(str::to_owned));
        if result["nextCursor"].is_null() {
            break;
        }
        value["cursor"] = result["nextCursor"].clone();
        result = read(&world, &value).unwrap();
    }
    assert_eq!(
        collected,
        (1..=257).map(|n| id("loc", n)).collect::<Vec<_>>()
    );
    assert_eq!(result["items"].as_array().unwrap().len(), 35);
    assert_eq!(read(&world, &value).unwrap(), result);
}

#[test]
fn cursor_is_bound_to_page_size_filters_sort_query_lens_and_threads() {
    let world = locations(3);
    let mut base = page(&world);
    base["limit"] = json!(1);
    let first = read(&world, &base).unwrap();
    base["cursor"] = first["nextCursor"].clone();
    let mut changes = Vec::new();
    for (key, value) in [
        ("limit", json!(2)),
        ("sort", json!("name-desc")),
        ("threadIds", json!([T])),
    ] {
        let mut changed = base.clone();
        changed[key] = value;
        changes.push(changed);
    }
    let mut changed = base.clone();
    changed["filters"]["name"] = json!("Location");
    changes.push(changed);
    changed = base.clone();
    changed["q"] = json!("ledger");
    changed["searchMode"] = json!("lexical");
    changes.push(changed);
    changed = base.clone();
    changed["lens"]["timeline"] = json!("other");
    changes.push(changed);
    changed = base.clone();
    changed["at"] = json!({"timeline":"main","tick":"0","order":"0"});
    changed["lens"] = json!({"kind":"author-as-of"});
    changes.push(changed);
    for changed in changes {
        error(&world, &changed, "CAT_CURSOR");
    }
    for cursor in ["", "!", "e30", "a", &"a".repeat(4097)] {
        let mut changed = base.clone();
        changed["cursor"] = json!(cursor);
        assert!(catalogue::decode(&changed.to_string()).is_err());
    }
}

#[test]
fn selection_bound_precedes_pages_and_threads_but_follows_filters() {
    let world = locations(4097);
    let mut value = page(&world);
    value["limit"] = json!(1);
    error(&world, &value, "CAT_SELECTION_LIMIT");
    value["threadIds"] = json!([T]);
    error(&world, &value, "CAT_SELECTION_LIMIT");
    value["threadIds"] = json!([]);
    value["filters"]["name"] = json!(id("loc", 4097));
    assert_eq!(read(&world, &value).unwrap()["total"], 1);
    let result = read(&world, &labels(&world, vec![id("loc", 4097)])).unwrap();
    assert_eq!(result["items"][0]["status"], "available");
}

#[test]
fn oversized_response_and_selected_metadata_refuse_complete_results() {
    let title = "x".repeat(300_000);
    let world = build(
        vec![record("location", L, json!({"title":title}), "")],
        json!({}),
    );
    error(&world, &page(&world), "CAT_RESPONSE_LIMIT");
    let title = "x".repeat(320_000);
    let world = build(
        (1..=27)
            .map(|n| record("location", &id("loc", n), json!({"title":title}), ""))
            .collect(),
        json!({}),
    );
    let mut value = page(&world);
    value["limit"] = json!(1);
    error(&world, &value, "CAT_TEXT_LIMIT");
    let result = read(&world, &labels(&world, vec!["missing".into()])).unwrap();
    assert_eq!(result["items"][0]["status"], "unavailable");
}

#[test]
fn facet_bound_applies_to_complete_selection_before_pagination() {
    let profiles=(0..65).map(|n|json!({"id":format!("p{n}"),"label":format!("Profile {n}"),"fields":{"type":"object","required":{"note":{"type":"string"}},"optional":{}}})).collect::<Vec<_>>();
    let records=(0..65).map(|n|record("lore-concept",&id("concept",n+1),json!({"profile":format!("p{n}"),"domain":"story.notes","values":{"note":"cedar"},"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),"")).collect();
    let world = build(
        records,
        json!({"capabilities":["lore-core-v1"],"profiles":profiles}),
    );
    let mut value = page(&world);
    value["filters"]["kinds"] = json!(["lore-concept"]);
    value["limit"] = json!(1);
    error(&world, &value, "CAT_FACET_LIMIT");
    value["filters"]["profiles"] = json!(["p1"]);
    assert_eq!(read(&world, &value).unwrap()["total"], 1);
}

#[test]
fn search_input_bounds_follow_admission_and_filters_and_never_truncate() {
    let world = locations(1025);
    let mut value = page(&world);
    value["q"] = json!("cedar");
    value["searchMode"] = json!("lexical");
    error(&world, &value, "CAT_DOCUMENT_LIMIT");
    value["filters"]["name"] = json!(L);
    assert_eq!(read(&world, &value).unwrap()["total"], 1);
    let oversized = "cedar ".repeat(90_000);
    let world = build(
        vec![record("location", L, json!({}), &oversized)],
        json!({}),
    );
    let mut value = page(&world);
    value["q"] = json!("cedar");
    value["searchMode"] = json!("lexical");
    error(&world, &value, "CAT_TEXT_LIMIT");
    let hidden = record(
        "location",
        &id("loc", 2),
        json!({"audience":[]}),
        &oversized,
    );
    let world = build(
        vec![record("location", L, json!({}), "cedar ledger"), hidden],
        json!({}),
    );
    value["fingerprint"] = json!(world.fingerprint);
    assert_eq!(read(&world, &value).unwrap()["total"], 1);
}

fn normalized(mut value: Json) -> Json {
    fn strip(value: &mut Json) {
        match value {
            Json::Object(values) => {
                values.remove("fingerprint");
                for value in values.values_mut() {
                    strip(value);
                }
            }
            Json::Array(values) => {
                for value in values {
                    strip(value);
                }
            }
            _ => {}
        }
    }
    strip(&mut value);
    value
}
#[test]
fn all_three_search_lanes_ignore_valid_hidden_corpus_mutations() {
    let visible = (1..=4)
        .map(|n| {
            record(
                "location",
                &id("loc", n),
                json!({"title":format!("Cedar ledger {n}")}),
                &format!("Cedar ledger harbor signal maintenance account {n}."),
            )
        })
        .collect::<Vec<_>>();
    let mut one = visible.clone();
    one.push(record(
        "location",
        &id("loc", 5),
        json!({"title":"HIDDEN_TITLE","audience":["secret"]}),
        "hidden private poem",
    ));
    let one = build(one, json!({}));
    let mut two = visible;
    two.push(record(
        "location",
        &id("loc", 5),
        json!({"title":"CHANGED_HIDDEN_TITLE","audience":["secret"],"aliases":["cedar"]}),
        &"cedar ledger hidden ".repeat(5000),
    ));
    let two = build(two, json!({}));
    for mode in ["lexical", "semantic", "hybrid"] {
        let mut value = page(&one);
        value["q"] = json!("cedar ledger");
        value["searchMode"] = json!(mode);
        value["sort"] = json!("relevance");
        let a = read(&one, &value).unwrap();
        assert!(!a["items"].as_array().unwrap().is_empty(), "{mode}");
        value["fingerprint"] = json!(two.fingerprint);
        let b = read(&two, &value).unwrap();
        assert_eq!(normalized(a), normalized(b), "{mode}");
    }
}

#[test]
fn prominence_uses_full_cohort_and_shared_current_without_location_journey_bounds() {
    let mut records = vec![
        record("location", L, json!({}), ""),
        record(
            "character",
            A,
            json!({"title":"Amber","aliases":["focus"]}),
            "",
        ),
        record(
            "character",
            B,
            json!({"title":"Blue","aliases":["focus"]}),
            "",
        ),
        record("character", C, json!({"title":"Cedar"}), ""),
        record("object", &id("obj", 1), json!({"title":"Focus object"}), ""),
    ];
    for n in 1..=6 {
        records.push(record("scene",&id("scene",n),json!({"status":"closed","location":L,"time":{"start":{"tick":n as i64},"current":{"tick":n as i64},"end":{"tick":n as i64}},"participants":[{"character":if n==1{A}else{C}}]}),""));
    }
    for n in 1..=2 {
        records.push(record(
            "event",
            &id("event", n),
            json!({"time":{"tick":n as i64},"location":L,"participants":[{"character":B}]}),
            "",
        ));
    }
    let world = build(records, json!({}));
    let mut value = page(&world);
    value["filters"]["kinds"] = json!([]);
    value["filters"]["name"] = json!("focus");
    value["sort"] = json!("prominence-high");
    let result = read(&world, &value).unwrap();
    assert_eq!(ids(&result), vec![B, A, id("obj", 1).as_str()]);
    assert_eq!(result["context"]["prominenceOrigin"], "shared-current");
    assert_eq!(result["context"]["prominenceAt"]["tick"], "20");
    value["sort"] = json!("prominence-low");
    assert_eq!(
        ids(&read(&world, &value).unwrap()),
        vec![A, B, id("obj", 1).as_str()]
    );
    value["sort"] = json!("prominence-high");
    value["lens"] = json!({"kind":"author-as-of"});
    value["at"] = json!({"timeline":"main","tick":"0","order":"0"});
    let result = read(&world, &value).unwrap();
    assert_eq!(ids(&result), vec![A, B, id("obj", 1).as_str()]);
    assert_eq!(result["context"]["prominenceOrigin"], "explicit-horizon");
}
