//! Explicit authored role selection, complete admission and independent accounts.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, assertion, character, governance,
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const ORG: &str = "organization_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const ELSEWHERE: &str = "loc_00000000000000000000000002";
const OVERLAY: &str = "overlay_00000000000000000000000001";
const ROLES: [&str; 6] = [
    "territory",
    "party",
    "basis",
    "subjects",
    "obligations",
    "supersedes",
];

fn id(number: usize) -> String {
    format!("concept_{number:026}")
}
fn assertion_id(number: usize) -> String {
    format!("assertion_{number:026}")
}
fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn interval(start: (i64, i32), end: (i64, i32)) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start.0,start.1),"end":point(end.0,end.1)})
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nExact authored source.\r\nNo inferred authority.  \n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn concept(number: usize, values: Json) -> Record {
    record(
        "lore-concept",
        &id(number),
        json!({"domain":"authored.governance","profile":"authored-policy",
        "values":values,"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
    )
}
fn values(basis: &str) -> Json {
    json!({"area":reference(PLACE),"issuer":reference(ORG),"basis":basis,
        "subjects":[reference(PERSON),reference(ORG),reference(&id(50))],
        "duties":{"amount":9007199254740993i64,"nullable":null}})
}
fn statement(number: usize, subject: &str, predicate: &str, value: Json) -> Record {
    record(
        "assertion",
        &assertion_id(number),
        json!({"domain":"authored.governance","subject":reference(subject),
        "predicate":predicate,"value":value,"classification":"author-fact","valid":{"kind":"timeless"},
        "evidence":[],"audience":["author"],"perspectives":["author"]}),
    )
}
fn world(mut extra: Vec<Record>) -> World {
    let party = json!({"type":"entity","kinds":["character","organization","lore-concept"]});
    let literal = json!({"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}});
    let nested = json!({"type":"object","required":{"targets":{"type":"array","items":{"type":"entity","kinds":["lore-concept"]},"max_items":8}},"optional":{}});
    let fields = json!({"type":"object","required":{},"optional":{
        "area":{"type":"entity","kinds":["location","overlay"]},"issuer":party,
        "basis":{"type":"enum","values":["legal","de-facto","claimed","contested"]},
        "subjects":{"type":"array","items":party,"max_items":8},
        "duties":{"type":"object","required":{"amount":{"type":"number"}},"optional":{
            "nullable":{"type":"null"},"beneficiary":party,"targets":{"type":"array","items":party,"max_items":8}}},
        "previous":{"type":"entity","kinds":["lore-concept"]},"unselected":party,
        "note":{"type":"string"},"number":{"type":"number"},"null_value":{"type":"null"},
        "literal":literal,"mixed_enum":{"type":"enum","values":[9007199254740993i64,"9007199254740993",null]},
        "locations":{"type":"array","items":{"type":"entity","kinds":["location"]},"max_items":8},
        "party_any":{"type":"entity","kinds":["character","organization","lore-concept","location"]},
        "bad_basis":{"type":"enum","values":["legal","sovereign"]}
    }});
    extra.push(record("world",WORLD,json!({"capabilities":["generational-core-v1","lore-core-v1","assertions-v1","collective-parties-v1"],
        "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"default_timeline":"main","state_keys":{},
        "profiles":[{"id":"authored-policy","label":"An arbitrary author definition","fields":fields},
            {"id":"collective","label":"An authored collective","fields":{"type":"object","required":{},"optional":{"note":{"type":"string"}}}}],
        "predicates":[
            {"id":"says-rule","subject_kinds":["character","organization","lore-concept"],"value_type":{"type":"string"}},
            {"id":"points-rule","subject_kinds":["character","organization","lore-concept"],"value_type":{"type":"entity","kinds":["lore-concept"]}},
            {"id":"literal-rule","subject_kinds":["character","organization","lore-concept"],"value_type":literal},
            {"id":"nested-rule","subject_kinds":["character","organization","lore-concept"],"value_type":nested}
        ]})));
    extra.push(record("character", PERSON, json!({"initial_state":{}})));
    extra.push(record("character", OTHER, json!({"initial_state":{}})));
    extra.push(record("location", PLACE, json!({})));
    extra.push(record("location", ELSEWHERE, json!({})));
    extra.push(record("organization",ORG,json!({"organization_kind":"institution","parent_id":null,"location_id":PLACE,
        "initialization":{"transition_id":"transition_00000000000000000000000001","transition_kind":"organization-initialize",
            "applicability":{"applicability_kind":"instant","point":point(0,0)},"payload":{"title":"Authored organization","aliases":[]}},"transitions":[]})));
    extra.push(record("lore-concept",&id(50),json!({"domain":"authored.governance","profile":"collective","values":{"note":"An authored group"},
        "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})));
    source::world_from_records(PathBuf::new(), extra, Some("governance-fixture".into())).unwrap()
}
fn base() -> World {
    world(vec![concept(1, values("legal"))])
}
fn edit(world: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let original = &world.records[target];
    let mut data = original.frontmatter.clone();
    change(&mut data);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        world
            .records
            .values()
            .map(|record| {
                if record.id == target {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn field(world: &World, target: &str, name: &str, value: Json) -> World {
    edit(world, target, |data| {
        data[name] = serde_yaml::to_value(value).unwrap()
    })
}
fn add(world: &World, value: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(value);
    source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap()
}
fn binding() -> governance::Binding {
    governance::Binding {
        profile: "authored-policy".into(),
        roles: [
            ("territory", Some("area")),
            ("party", Some("issuer")),
            ("basis", Some("basis")),
            ("subjects", Some("subjects")),
            ("obligations", Some("duties")),
            ("supersedes", Some("previous")),
        ]
        .into_iter()
        .map(|(role, field)| (role.into(), field.map(str::to_owned)))
        .collect(),
    }
}
fn read(
    world: &World,
    target: &str,
    binding: &governance::Binding,
    at: Option<&StoryTime>,
) -> Json {
    governance::query(world, target, binding, at, "author", "author", 1000).unwrap()
}
fn query(world: &World) -> Json {
    read(world, PLACE, &binding(), Some(&at(5, 0)))
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn ids(result: &Json) -> Vec<&str> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["conceptId"].as_str().unwrap())
        .collect()
}
fn statement_ids(item: &Json) -> Vec<&str> {
    item["statements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["assertionId"].as_str().unwrap())
        .collect()
}

#[test]
fn six_roles_retain_all_bases_party_order_and_actual_source_citations() {
    let mut records = Vec::new();
    for (index, basis) in ["legal", "de-facto", "claimed", "contested"]
        .into_iter()
        .enumerate()
    {
        let mut authored = values(basis);
        if index > 0 {
            authored["previous"] = reference(&id(index));
        }
        records.push(concept(index + 1, authored));
    }
    let world = world(records);
    valid(&world);
    let result = query(&world);
    assert_eq!(result["count"], 4);
    assert_eq!(result["scope"], "author-as-of-governance");
    for (index, item) in result["items"].as_array().unwrap().iter().enumerate() {
        assert_eq!(item["conceptId"], id(index + 1));
        assert_eq!(
            item["roles"]["basis"],
            ["legal", "de-facto", "claimed", "contested"][index]
        );
        assert_eq!(item["jurisdiction"], "authored");
        assert_eq!(
            item["parties"],
            json!([
            {"kind":"organization","id":ORG},{"kind":"character","id":PERSON},
            {"kind":"organization","id":ORG},{"kind":"concept","id":id(50)}])
        );
        assert_eq!(item["roles"]["obligations"]["amount"], "9007199254740993");
        assert_eq!(
            item["roleTypes"]["obligations"]["fields"]["amount"]["type"],
            "number"
        );
        assert_eq!(item["roleKnown"]["supersedes"], index > 0);
        assert_eq!(item["roleCitations"]["party"]["section"], "values.issuer");
        let reference = item["references"]
            .as_array()
            .unwrap()
            .iter()
            .find(|reference| reference["sourceField"] == "values.subjects[2].id")
            .unwrap();
        assert_eq!(reference["entityId"], id(50));
        assert_eq!(reference["citation"]["section"], "values.subjects[2].id");
        assert_eq!(reference["targetCitation"]["entityId"], id(50));
        assert_eq!(
            reference["targetCitation"]["fingerprint"],
            world.fingerprint
        );
        if index > 0 {
            assert_eq!(item["supersedesCitation"]["entityId"], id(index));
        }
    }
}

#[test]
fn unbound_absent_and_explicit_null_roles_remain_distinguishable() {
    let world = world(vec![
        concept(1, json!({"null_value":null})),
        concept(2, json!({})),
    ]);
    let mut map = governance::Binding {
        profile: "authored-policy".into(),
        roles: ROLES.into_iter().map(|role| (role.into(), None)).collect(),
    };
    map.roles
        .insert("obligations".into(), Some("null_value".into()));
    let result = read(&world, PLACE, &map, None);
    assert_eq!(result["count"], 2);
    for item in result["items"].as_array().unwrap() {
        assert_eq!(item["jurisdiction"], "unknown");
        assert_eq!(item["roles"]["territory"], Json::Null);
        assert_eq!(item["roleKnown"]["territory"], false);
        assert_eq!(item["parties"], json!([]));
    }
    assert_eq!(result["items"][0]["roles"]["obligations"], Json::Null);
    assert_eq!(result["items"][0]["roleKnown"]["obligations"], true);
    assert_eq!(
        result["items"][0]["roleTypes"]["obligations"],
        json!({"type":"null"})
    );
    assert_eq!(result["items"][1]["roleKnown"]["obligations"], false);
    assert_eq!(result["items"][1]["roleTypes"]["obligations"], Json::Null);
    assert_eq!(
        result["items"][1]["roleCitations"]["obligations"],
        Json::Null
    );
}

#[test]
fn role_binding_is_closed_distinct_and_never_guessed_from_names() {
    let world = base();
    for (role, field) in [
        ("territory", "issuer"),
        ("party", "area"),
        ("party", "party_any"),
        ("basis", "note"),
        ("basis", "mixed_enum"),
        ("basis", "bad_basis"),
        ("subjects", "locations"),
        ("subjects", "issuer"),
        ("supersedes", "issuer"),
        ("territory", "missing"),
    ] {
        let mut map = binding();
        for selected in map.roles.values_mut() {
            if selected.as_deref() == Some(field) {
                *selected = None;
            }
        }
        map.roles.insert(role.into(), Some(field.into()));
        assert_eq!(
            read(&world, PLACE, &map, None)["count"],
            0,
            "{role}: {field}"
        );
    }
    let mut map = binding();
    map.profile = "law".into();
    assert_eq!(read(&world, PLACE, &map, None)["count"], 0);
    let mut invalid = vec![];
    let mut missing = binding();
    missing.roles.remove("party");
    invalid.push(missing);
    let mut extra = binding();
    extra.roles.insert("authority".into(), None);
    invalid.push(extra);
    let mut duplicate = binding();
    duplicate
        .roles
        .insert("obligations".into(), Some("area".into()));
    invalid.push(duplicate);
    for token in [
        "",
        "Upper",
        "nested.field",
        "field-name",
        "with space",
        "_field",
    ] {
        let mut bad = binding();
        bad.roles.insert("party".into(), Some(token.into()));
        invalid.push(bad);
    }
    for token in ["", "Upper", "with_underscore", "policy@1"] {
        let mut bad = binding();
        bad.profile = token.into();
        invalid.push(bad);
    }
    for map in invalid {
        assert!(matches!(map.validate(), Err(Error::InvalidInput(_))));
    }
}

#[test]
fn absent_wrong_kind_and_hidden_targets_return_the_same_empty_author_shape() {
    let world = base();
    let missing = read(&world, "loc_00000000000000000000000099", &binding(), None);
    for target in [PERSON, "not-an-exact-id", &world.records[PLACE].title] {
        if target == PLACE {
            continue;
        }
        let mut result = read(&world, target, &binding(), None);
        result["targetId"] = missing["targetId"].clone();
        assert_eq!(result, missing);
    }
    for scope in ["audience", "perspectives"] {
        let hidden = field(&world, PLACE, scope, json!(["private"]));
        let mut result = read(&hidden, PLACE, &binding(), None);
        result["targetId"] = missing["targetId"].clone();
        assert_eq!(result, missing);
        assert!(!result.to_string().contains("title"));
    }
}

#[test]
fn any_hidden_selected_typed_reference_withholds_the_entire_policy() {
    let world = base();
    let hidden = field(&world, OTHER, "audience", json!(["private"]));
    let mutations = [
        ("issuer", reference(OTHER)),
        ("subjects", json!([reference(PERSON), reference(OTHER)])),
        ("duties", json!({"amount":1,"beneficiary":reference(OTHER)})),
        (
            "duties",
            json!({"amount":1,"targets":[reference(PERSON),reference(OTHER)]}),
        ),
    ];
    for (name, value) in mutations {
        let changed = edit(&hidden, &id(1), |data| {
            data["values"][name] = serde_yaml::to_value(value).unwrap()
        });
        valid(&changed);
        assert_eq!(query(&changed)["count"], 0, "{name}");
    }
    for (scope, target) in [("audience", id(50)), ("perspectives", ORG.to_owned())] {
        assert_eq!(
            query(&field(&world, &target, scope, json!(["private"])))["count"],
            0
        );
    }
    let prior = edit(&world, &id(1), |data| {
        data["values"]["previous"] = serde_yaml::to_value(reference(&id(50))).unwrap()
    });
    let prior = field(&prior, &id(50), "status", json!("retired"));
    assert_eq!(query(&prior)["count"], 0);
    let prior = field(&world, &id(50), "valid", interval((7, 0), (8, 0)));
    assert_eq!(query(&prior)["count"], 0);
    assert_eq!(read(&prior, PLACE, &binding(), None)["count"], 1);
}

#[test]
fn hidden_unselected_fields_and_ref_shaped_literals_do_not_become_typed_edges() {
    let world = edit(&base(), &id(1), |data| {
        data["values"]["unselected"] = serde_yaml::to_value(reference(OTHER)).unwrap();
        data["values"]["literal"] = serde_yaml::to_value(reference(OTHER)).unwrap();
    });
    let world = field(&world, OTHER, "audience", json!(["private"]));
    assert_eq!(query(&world)["count"], 1);
    let mut map = binding();
    map.roles
        .insert("obligations".into(), Some("literal".into()));
    let result = read(&world, PLACE, &map, Some(&at(5, 0)));
    assert_eq!(result["count"], 1);
    assert_eq!(result["items"][0]["roles"]["obligations"], reference(OTHER));
    assert!(
        result["items"][0]["references"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["entityId"] != OTHER)
    );
    map.roles
        .insert("obligations".into(), Some("unselected".into()));
    assert_eq!(read(&world, PLACE, &map, None)["count"], 0);
}

#[test]
fn finite_obligation_types_preserve_exact_enum_scalar_identity() {
    let mut map = binding();
    map.roles
        .insert("obligations".into(), Some("mixed_enum".into()));
    for (value, scalar) in [
        (json!(9007199254740993i64), "number"),
        (json!("9007199254740993"), "string"),
        (Json::Null, "null"),
    ] {
        let world = edit(&base(), &id(1), |data| {
            data["values"]["mixed_enum"] = serde_yaml::to_value(value).unwrap()
        });
        let result = read(&world, PLACE, &map, None);
        let item = &result["items"][0];
        assert_eq!(
            item["roleTypes"]["obligations"],
            json!({"type":"enum","scalarType":scalar})
        );
        assert_eq!(item["roleKnown"]["obligations"], true);
        if scalar != "null" {
            assert_eq!(item["roles"]["obligations"], "9007199254740993");
        }
    }
    let world = edit(&base(), &id(1), |data| {
        data["values"]["mixed_enum"] = serde_yaml::to_value(9007199254740992i64).unwrap()
    });
    assert!(governance::query(&world, PLACE, &map, None, "author", "author", 100).is_err());
}

fn spatial(world: &World) -> World {
    field(
        world,
        WORLD,
        "capabilities",
        json!([
            "generational-core-v1",
            "spatial-core-v1",
            "overlay-v1",
            "lore-core-v1",
            "assertions-v1",
            "collective-parties-v1"
        ]),
    )
}
fn overlay(world: &World, timed: bool) -> World {
    let mut fields = json!({"lifecycle":"static","membership":{"location_ids":[PLACE]},"audience":["author"],"perspectives":["author"]});
    if timed {
        fields["lifecycle"] = json!("time-bounded");
        fields["valid"] = json!({"start":point(5,-1),"end":point(5,1)});
    }
    let world = add(&spatial(world), record("overlay", OVERLAY, fields));
    edit(&world, &id(1), |data| {
        data["values"]["area"] = serde_yaml::to_value(reference(OVERLAY)).unwrap()
    })
}

#[test]
fn location_jurisdiction_is_exact_and_never_inherited_through_containment() {
    let world = field(&spatial(&base()), ELSEWHERE, "parent_id", json!(PLACE));
    valid(&world);
    assert_eq!(query(&world)["count"], 1);
    assert_eq!(read(&world, ELSEWHERE, &binding(), None)["count"], 0);
    let world = edit(&world, &id(1), |data| {
        data["values"].as_mapping_mut().unwrap().remove("area");
    });
    assert_eq!(
        read(&world, ELSEWHERE, &binding(), None)["items"][0]["jurisdiction"],
        "unknown"
    );
}

#[test]
fn static_overlay_membership_is_direct_canonical_and_scoped() {
    let world = overlay(&base(), false);
    valid(&world);
    for target in [PLACE, OVERLAY] {
        assert_eq!(read(&world, target, &binding(), None)["count"], 1);
        assert_eq!(
            read(&world, target, &binding(), Some(&at(500, 0)))["count"],
            1
        );
    }
    assert_eq!(read(&world, ELSEWHERE, &binding(), None)["count"], 0);
    for (name, value) in [
        ("status", json!("retired")),
        ("audience", json!(["private"])),
        ("perspectives", json!(["private"])),
    ] {
        let changed = field(&world, OVERLAY, name, value);
        assert_eq!(read(&changed, PLACE, &binding(), None)["count"], 0);
    }
    let no_cap = field(
        &world,
        WORLD,
        "capabilities",
        json!([
            "generational-core-v1",
            "lore-core-v1",
            "assertions-v1",
            "collective-parties-v1"
        ]),
    );
    assert!(governance::query(&no_cap, PLACE, &binding(), None, "author", "author", 100).is_err());
}

#[test]
fn timed_overlays_require_an_exact_inclusive_horizon_even_when_querying_the_overlay() {
    let world = overlay(&base(), true);
    valid(&world);
    for target in [PLACE, OVERLAY] {
        for (order, count) in [(-2, 0), (-1, 1), (0, 1), (1, 1), (2, 0)] {
            assert_eq!(
                read(&world, target, &binding(), Some(&at(5, order)))["count"],
                count
            );
        }
        assert_eq!(read(&world, target, &binding(), None)["count"], 0);
        assert_eq!(
            read(
                &world,
                target,
                &binding(),
                Some(&StoryTime::new("other", 5, 0).unwrap())
            )["count"],
            0
        );
    }
}

#[test]
fn policy_scopes_status_and_exact_time_apply_before_counts_and_all_time_keeps_timelines() {
    let coordinate = 9007199254740993i64;
    let world = world(vec![
        concept(1, values("legal")),
        concept(2, values("contested")),
    ]);
    let world = field(
        &world,
        &id(1),
        "valid",
        interval((coordinate, -1), (coordinate, 1)),
    );
    let world = field(
        &world,
        &id(2),
        "valid",
        json!({"kind":"interval","timeline":"other"}),
    );
    valid(&world);
    for (order, count) in [(-2, 0), (-1, 1), (1, 1), (2, 0)] {
        let result = read(&world, PLACE, &binding(), Some(&at(coordinate, order)));
        assert_eq!(result["count"], count);
        assert_eq!(result["at"]["tick"], coordinate.to_string());
    }
    assert_eq!(
        ids(&read(
            &world,
            PLACE,
            &binding(),
            Some(&StoryTime::new("other", 0, 0).unwrap())
        )),
        [id(2)]
    );
    let all = read(&world, PLACE, &binding(), None);
    assert_eq!(all["scope"], "author-all-time-governance");
    assert_eq!(all["count"], 2);
    for (field_name, value) in [
        ("status", json!("draft")),
        ("status", json!("retired")),
        ("audience", json!(["private"])),
        ("perspectives", json!(["private"])),
    ] {
        let changed = field(&world, &id(1), field_name, value);
        assert_eq!(read(&changed, PLACE, &binding(), None)["count"], 1);
    }
}

#[test]
fn actual_assertions_link_by_subject_or_root_entity_value_and_retain_classification() {
    let world = world(vec![
        concept(1, values("legal")),
        statement(1, &id(1), "says-rule", json!("Direct words")),
        statement(2, ORG, "points-rule", reference(&id(1))),
        statement(3, ORG, "literal-rule", reference(&id(1))),
        statement(
            4,
            ORG,
            "nested-rule",
            json!({"targets":[reference(&id(1))]}),
        ),
        statement(5, ORG, "says-rule", json!(id(1))),
    ]);
    let world = field(
        &world,
        &assertion_id(2),
        "classification",
        json!("attributed-claim"),
    );
    let world = field(&world, &assertion_id(2), "attributed_to", reference(PERSON));
    valid(&world);
    let result = query(&world);
    assert_eq!(
        statement_ids(&result["items"][0]),
        [assertion_id(1), assertion_id(2)]
    );
    let native = assertion::as_of(&world, ORG, Some(&at(5, 0)), "author", "author", 100).unwrap();
    assert_eq!(result["items"][0]["statements"][1], native["items"][0]);
    assert_eq!(
        result["items"][0]["statements"][1]["classification"],
        "attributed-claim"
    );
    assert_eq!(
        result["items"][0]["statements"][1]["attributedTo"],
        reference(PERSON)
    );
    assert_eq!(result["items"][0]["statements"][1]["evidence"], json!([]));
    let hidden = field(&world, PERSON, "audience", json!(["private"]));
    let mut map = binding();
    map.roles.insert("subjects".into(), None);
    let result = read(&hidden, PLACE, &map, Some(&at(5, 0)));
    assert_eq!(statement_ids(&result["items"][0]), [assertion_id(1)]);
}

#[test]
fn assertion_corrections_forks_and_partial_projection_use_the_native_admission_path() {
    let world = world(vec![
        concept(1, values("legal")),
        statement(1, &id(1), "says-rule", json!("First")),
        statement(2, &id(1), "says-rule", json!("Fork A")),
        statement(3, &id(1), "says-rule", json!("Fork B")),
        statement(
            4,
            &id(1),
            "nested-rule",
            json!({"targets":[reference(&id(1)),reference(&id(50))]}),
        ),
    ]);
    let world = field(&world, &assertion_id(2), "replaces", json!(assertion_id(1)));
    let world = field(&world, &assertion_id(3), "replaces", json!(assertion_id(1)));
    let world = field(&world, &id(50), "audience", json!(["private"]));
    let mut map = binding();
    map.roles.insert("subjects".into(), None);
    let result = read(&world, PLACE, &map, Some(&at(5, 0)));
    assert_eq!(
        statement_ids(&result["items"][0]),
        [assertion_id(2), assertion_id(3), assertion_id(4)]
    );
    assert_eq!(
        result["items"][0]["statements"][2]["value"]["targets"],
        json!([reference(&id(1))])
    );
    let history = read(&world, PLACE, &map, None);
    assert_eq!(
        statement_ids(&history["items"][0]),
        [
            assertion_id(1),
            assertion_id(2),
            assertion_id(3),
            assertion_id(4)
        ]
    );
    let hidden = field(&world, &assertion_id(2), "audience", json!(["private"]));
    let hidden = field(&hidden, &assertion_id(3), "audience", json!(["private"]));
    let result = read(&hidden, PLACE, &map, Some(&at(5, 0)));
    assert_eq!(
        statement_ids(&result["items"][0]),
        [assertion_id(1), assertion_id(4)]
    );
}

#[test]
fn supersedes_is_only_a_cited_pointer_and_policy_cycles_do_not_adjudicate() {
    let mut first = values("legal");
    first["previous"] = reference(&id(2));
    let mut second = values("contested");
    second["previous"] = reference(&id(1));
    let world = world(vec![concept(1, first), concept(2, second)]);
    valid(&world);
    assert_eq!(query(&world)["count"], 2);
    assert_eq!(read(&world, PLACE, &binding(), None)["count"], 2);
    assert_eq!(
        query(&world)["items"][0]["supersedesCitation"]["entityId"],
        id(2)
    );
}

#[test]
fn complete_limits_follow_policy_admission_and_scope() {
    let world = world(vec![
        concept(1, values("legal")),
        concept(2, values("contested")),
    ]);
    assert!(matches!(
        governance::query(&world, PLACE, &binding(), None, "author", "author", 1),
        Err(Error::InvalidInput(_))
    ));
    let hidden = field(&world, &id(2), "audience", json!(["private"]));
    assert_eq!(
        governance::query(&hidden, PLACE, &binding(), None, "author", "author", 1).unwrap()["count"],
        1
    );
    for limit in [0, 1001] {
        assert!(matches!(
            governance::query(&world, PLACE, &binding(), None, "author", "author", limit),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        governance::query(&world, PLACE, &binding(), None, "", "author", 100),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        governance::query(
            &world,
            PLACE,
            &binding(),
            Some(&StoryTime::new("absent", 0, 0).unwrap()),
            "author",
            "author",
            100
        ),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn statement_bound_counts_linked_occurrences_after_admission_not_unrelated_source() {
    let mut records = vec![concept(1, values("legal")), concept(2, values("claimed"))];
    records.extend((1..=501).map(|n| statement(n, &id(1), "points-rule", reference(&id(2)))));
    let shared = world(records);
    let error =
        governance::query(&shared, PLACE, &binding(), None, "author", "author", 1000).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("1000 linked statement occurrences")
    );
    let hidden = field(&shared, &id(2), "audience", json!(["private"]));
    assert_eq!(query(&hidden)["items"][0]["statements"], json!([]));
    let mut records = vec![concept(1, values("legal"))];
    records.extend((1..=1001).map(|n| statement(n, ORG, "says-rule", json!("Unrelated words"))));
    let unrelated = world(records);
    assert_eq!(query(&unrelated)["count"], 1);
}

#[test]
fn collective_requires_lore_and_generational_but_optional_capabilities_stay_optional() {
    let world = edit(&base(), WORLD, |data| {
        data["capabilities"] = serde_yaml::to_value(json!([
            "generational-core-v1",
            "lore-core-v1",
            "collective-parties-v1"
        ]))
        .unwrap();
        data.as_mapping_mut().unwrap().remove("predicates");
    });
    valid(&world);
    assert_eq!(query(&world)["count"], 1);
    assert_eq!(query(&world)["items"][0]["statements"], json!([]));
    for capability in [
        "generational-core-v1",
        "lore-core-v1",
        "collective-parties-v1",
    ] {
        let changed = edit(&world, WORLD, |data| {
            data["capabilities"]
                .as_sequence_mut()
                .unwrap()
                .retain(|v| v.as_str() != Some(capability))
        });
        assert!(
            governance::query(&changed, PLACE, &binding(), None, "author", "author", 100).is_err()
        );
    }
}

#[test]
fn source_validation_is_composed_and_exact_captured_bytes_cannot_be_forged() {
    let world = base();
    let mut forged = world.clone();
    forged.records.get_mut(&id(1)).unwrap().frontmatter["values"]["basis"] =
        Yaml::String("contested".into());
    assert!(governance::query(&forged, PLACE, &binding(), None, "author", "author", 100).is_err());
    let mut forged = world.clone();
    forged.fingerprint = "forged".into();
    assert!(governance::query(&forged, PLACE, &binding(), None, "author", "author", 100).is_err());
    for (name, value) in [
        ("issuer", json!({"kind":"organization","id":ORG})),
        ("subjects", json!([reference(PLACE)])),
        ("basis", json!("sovereign")),
        ("duties", json!({"amount":true})),
        ("previous", reference(PERSON)),
    ] {
        let bad = edit(&world, &id(1), |data| {
            data["values"][name] = serde_yaml::to_value(value).unwrap()
        });
        assert!(
            governance::query(&bad, PLACE, &binding(), None, "author", "author", 100).is_err(),
            "{name}"
        );
    }
}

#[test]
fn governance_reads_and_canonical_edits_leave_character_knowledge_and_source_clock_unchanged() {
    let world = base();
    let before = world
        .records
        .iter()
        .map(|(id, r)| {
            (
                id.clone(),
                (r.raw_bytes.clone(), r.body.clone(), r.source_path.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let context = character::context(&world, PERSON, &at(5, 0), 100000).unwrap();
    let search = character::search(&world, PERSON, "Authored", &at(5, 0), 100).unwrap();
    query(&world);
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (
                id.clone(),
                (r.raw_bytes.clone(), r.body.clone(), r.source_path.clone())
            ))
            .collect::<BTreeMap<_, _>>(),
        before
    );
    assert!(world.world_record().unwrap().get("current_time").is_none());
    let changed = edit(&world, &id(1), |data| {
        data["values"]["basis"] = Yaml::String("contested".into())
    });
    assert_eq!(
        character::context(&changed, PERSON, &at(5, 0), 100000).unwrap(),
        context
    );
    assert_eq!(
        character::search(&changed, PERSON, "Authored", &at(5, 0), 100).unwrap(),
        search
    );
}
