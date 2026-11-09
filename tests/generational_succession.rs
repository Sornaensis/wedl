//! Closed succession/vital source rules and author-horizon projections.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    generational as genealogy,
    model::{Record, StoryTime, World},
    source,
};

fn id(kind: &str, number: u32) -> String {
    let prefix = match kind {
        "character" => "char",
        "vital-history" => "vital",
        _ => kind,
    };
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn tr(number: u32, kind: &str, tick: i64, order: i32, payload: Json) -> Json {
    json!({"transition_id":id("transition",number),"transition_kind":kind,
      "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":tick,"order":order}},"payload":payload})
}
fn vacancy(number: u32, first: (i64, i32), last: (i64, i32)) -> Json {
    let mut value = tr(
        number,
        "tenure-vacate",
        first.0,
        first.1,
        json!({"holder_id":null}),
    );
    value["applicability"] = json!({"applicability_kind":"inclusive-interval",
      "first":{"timeline":"main","tick":first.0,"order":first.1},"last":{"timeline":"main","tick":last.0,"order":last.1}});
    value
}
fn organization(number: u32) -> Json {
    json!({"kind":"organization","id":id("organization",number),"title":format!("Organization {number}"),"organization_kind":"institution",
      "initialization":tr(1000+number*100,"organization-initialize",-30,0,json!({"title":format!("Organization {number}"),"aliases":[]})),"transitions":[]})
}
fn legacy(number: u32) -> Json {
    json!({"kind":"legacy","id":id("legacy",number),"title":format!("Legacy {number}"),"legacy_kind":"office","organization_id":id("organization",1),
      "initialization":tr(10000+number*100,"legacy-initialize",-20,0,json!({"title":format!("Authored legacy {number}"),"aliases":[]})),"transitions":[]})
}
fn tenure(number: u32, holder: Option<u32>, basis: &str, tick: i64, order: i32) -> Json {
    json!({"kind":"tenure","id":id("tenure",number),"title":format!("Tenure {number}"),"legacy_id":id("legacy",1),
      "initialization":tr(20000+number*100,"tenure-initialize",tick,order,json!({"holder_id":holder.map(|number|id("character",number)),"basis":basis})),"transitions":[]})
}
fn claim(number: u32, claimant: u32, competitors: &[u32]) -> Json {
    json!({"kind":"claim","id":id("claim",number),"title":format!("Claim {number}"),"legacy_id":id("legacy",1),"claimant_id":id("character",claimant),
      "initialization":tr(30000+number*100,"claim-initialize",0,0,json!({"competes_with":competitors.iter().map(|number|id("claim",*number)).collect::<Vec<_>>()})),"transitions":[]})
}
fn vital(number: u32, character: u32, disclosure: &str) -> Json {
    json!({"kind":"vital-history","id":id("vital-history",number),"title":format!("Vital history {number}"),"character_id":id("character",character),"disclosure":disclosure,
      "initialization":tr(40000+number*100,"vital-initialize",-10,0,json!({})),"transitions":[]})
}
fn event(tick: i64, order: i32) -> Json {
    json!({"kind":"event","id":id("event",1),"title":"Transfer cause","time":{"timeline":"main","tick":tick,"order":order}})
}
fn record(mut value: Json) -> Record {
    value["schema"] = json!(source::SOURCE_SCHEMA);
    let raw = format!(
        "---\n{}---\nPreserved narrative prose.\n",
        serde_yaml::to_string(&value).unwrap()
    );
    source::parse_record(
        raw.as_bytes(),
        &format!("story/{}.md", value["id"].as_str().unwrap()),
    )
    .unwrap()
}
fn fixture(extra: Vec<Json>) -> World {
    let mut values = vec![
        json!({"kind":"world","id":id("world",1),"title":"Succession test","capabilities":["generational-core-v1"],
      "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
        organization(1),
        legacy(1),
    ];
    values.extend((1..=6).map(|number|json!({"kind":"character","id":id("character",number),"title":format!("Character {number}"),"aliases":[format!("Person {number}")]})));
    values.extend(extra);
    let records: BTreeMap<_, _> = values
        .into_iter()
        .map(record)
        .map(|record| (record.id.clone(), record))
        .collect();
    source::world_from_records(
        PathBuf::new(),
        records.into_values().collect(),
        Some("succession-revision".into()),
    )
    .unwrap()
}
fn edit(base: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let original = &base.records[target];
    let mut value = original.frontmatter.clone();
    change(&mut value);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&value).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        base.root.clone(),
        base.records
            .values()
            .map(|record| {
                if record.id == target {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
        base.revision.clone(),
    )
    .unwrap()
}
fn yaml(value: Json) -> Yaml {
    serde_yaml::to_value(value).unwrap()
}
fn valid(world: &World) {
    let errors = genealogy::validate(world);
    assert!(errors.is_empty(), "{errors:#?}");
}
fn invalid(world: &World) {
    assert!(!genealogy::validate(world).is_empty());
    assert!(genealogy::legacy(world, &id("legacy", 1), &at(20, 0), 100).is_err());
}
fn view(world: &World, tick: i64, order: i32) -> Json {
    genealogy::legacy(world, &id("legacy", 1), &at(tick, order), 100).unwrap()
}
fn history(world: &World, kind: &str, number: u32, tick: i64, order: i32) -> Json {
    genealogy::history(world, &id(kind, number), &at(tick, order), 100).unwrap()
}
fn transfer_fixture() -> World {
    let mut from = tenure(1, Some(1), "legal", 0, 0);
    let mut transfer = tr(
        20101,
        "tenure-transfer",
        3,
        0,
        json!({"from_tenure_id":id("tenure",1),"to_tenure_id":id("tenure",2)}),
    );
    transfer["cause_event_id"] = json!(id("event", 1));
    from["transitions"] = json!([transfer]);
    let mut target = tenure(2, None, "legal", 1, 0);
    target["predecessor_tenure_id"] = json!(id("tenure", 1));
    target["transitions"] = json!([tr(
        20201,
        "tenure-hold",
        3,
        1,
        json!({"holder_id":id("character",2),"basis":"legal"})
    )]);
    fixture(vec![from, target, event(2, 0)])
}

#[test]
fn legacy_names_and_lifecycle_are_literal_inclusive_and_do_not_change_holders() {
    let mut source = legacy(1);
    source["transitions"] = json!([
        tr(10101, "legacy-dormant", 1, 0, json!({})),
        tr(
            10102,
            "legacy-rename",
            2,
            3,
            json!({"title":"Later office","aliases":["Later alias"]})
        ),
        tr(10103, "legacy-dissolve", 4, 0, json!({}))
    ]);
    let world = fixture(vec![source, tenure(1, Some(1), "legal", 0, 0)]);
    valid(&world);
    let report = wedl::validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    let before = view(&world, 2, 2);
    assert_eq!(before["legacy"]["lifecycle"], "dormant");
    assert_eq!(before["legacy"]["value"]["title"], "Authored legacy 1");
    assert!(!before.to_string().contains("Later office"));
    let renamed = view(&world, 2, 3);
    assert_eq!(renamed["legacy"]["lifecycle"], "dormant");
    assert_eq!(renamed["legacy"]["value"]["title"], "Later office");
    assert_eq!(renamed["legacy"]["citations"].as_array().unwrap().len(), 3);
    assert_eq!(renamed["legacy"]["citations"][2]["field"], "transitions[1]");
    assert_eq!(
        renamed["legacy"]["citations"][2]["revision"],
        "succession-revision"
    );
    let dissolved = view(&world, 4, 0);
    assert_eq!(dissolved["legacy"]["lifecycle"], "dissolved");
    assert_eq!(dissolved["legacy"]["active"], false);
    assert_eq!(dissolved["holders"].as_array().unwrap().len(), 1);
    let future = view(&world, -21, i32::MAX);
    assert_eq!(future["available"], false);
    assert_eq!(future["reason"], "before_initialization");
    let other = genealogy::legacy(
        &world,
        &id("legacy", 1),
        &StoryTime::new("other", 100, 0).unwrap(),
        100,
    )
    .unwrap();
    assert_eq!(other["reason"], "different_timeline");
    assert_eq!(other["tenures"], json!([]));
}

#[test]
fn coholding_designation_and_claims_remain_separate_without_a_winner() {
    let mut designated = tenure(3, None, "legal", 0, 0);
    designated["transitions"] = json!([tr(
        20301,
        "tenure-designate",
        1,
        0,
        json!({"holder_id":id("character",3),"basis":"legal"})
    )]);
    let mut recognized = claim(1, 4, &[]);
    recognized["transitions"] = json!([tr(30101, "claim-recognize", 1, 0, json!({}))]);
    let world = fixture(vec![
        tenure(1, Some(1), "legal", 0, 0),
        tenure(2, Some(2), "de-facto", 0, 0),
        designated,
        recognized,
    ]);
    valid(&world);
    let result = view(&world, 1, 0);
    assert_eq!(result["tenures"].as_array().unwrap().len(), 3);
    assert_eq!(result["holders"].as_array().unwrap().len(), 2);
    assert_eq!(result["holders"][0]["value"]["basis"], "legal");
    assert_eq!(result["holders"][1]["value"]["basis"], "de-facto");
    assert_eq!(result["tenures"][2]["lifecycle"], "designated");
    assert_eq!(result["claims"][0]["lifecycle"], "recognized");
    assert_eq!(
        result["claims"][0]["value"]["claimantId"],
        id("character", 4)
    );
    assert_eq!(result["succession"], json!([]));
}

#[test]
fn explicit_vacancy_expires_after_its_inclusive_last_without_reopening() {
    let start = 9_007_199_254_740_993_i64;
    let mut source = tenure(1, Some(1), "legal", 0, 0);
    source["transitions"] = json!([vacancy(20101, (start, 2), (start + 1, -1))]);
    let world = fixture(vec![source]);
    valid(&world);
    let bytes = world.records[&id("tenure", 1)].raw_bytes.clone();
    for (tick, order, state, holder) in [
        (start, 1, "holding", true),
        (start, 2, "vacant-interval", false),
        (start + 1, -1, "vacant-interval", false),
        (start + 1, 0, "expired", false),
    ] {
        let result = history(&world, "tenure", 1, tick, order);
        assert_eq!(result["lifecycle"], state);
        assert_eq!(
            view(&world, tick, order)["holders"]
                .as_array()
                .unwrap()
                .len(),
            usize::from(holder)
        );
        if !holder {
            assert_eq!(result["value"]["holderId"], Json::Null);
        }
    }
    let expired = history(&world, "tenure", 1, start + 1, 0);
    assert_eq!(
        expired["history"][1]["applicability"]["last"]["tick"],
        (start + 1).to_string()
    );
    assert_eq!(
        expired["history"][1]["applicability"]["last"]["order"],
        "-1"
    );
    assert_eq!(expired["appliedTransitionIds"].as_array().unwrap().len(), 2);
    assert_eq!(bytes, world.records[&id("tenure", 1)].raw_bytes);
    for (kind, payload) in [
        (
            "tenure-hold",
            json!({"holder_id":id("character",2),"basis":"legal"}),
        ),
        (
            "tenure-designate",
            json!({"holder_id":id("character",2),"basis":"legal"}),
        ),
        ("tenure-end", json!({})),
    ] {
        invalid(&edit(&world, &id("tenure", 1), |record| {
            record["transitions"]
                .as_sequence_mut()
                .unwrap()
                .push(yaml(tr(20102, kind, start + 2, 0, payload)))
        }));
    }
}

#[test]
fn vacancies_require_closed_intervals_and_exact_replacement_intervals() {
    let mut source = tenure(1, Some(1), "legal", 0, 0);
    source["transitions"] = json!([vacancy(20101, (1, 0), (2, 0))]);
    let base = fixture(vec![source]);
    valid(&base);
    for app in [
        json!({"applicability_kind":"instant","point":{"timeline":"main","tick":1,"order":0}}),
        json!({"applicability_kind":"inclusive-interval","first":{"timeline":"main","tick":2,"order":0},"last":{"timeline":"main","tick":1,"order":0}}),
        json!({"applicability_kind":"inclusive-interval","first":{"timeline":"main","tick":1,"order":0},"last":{"timeline":"other","tick":2,"order":0}}),
    ] {
        invalid(&edit(&base, &id("tenure", 1), |record| {
            record["transitions"][0]["applicability"] = yaml(app)
        }));
    }
    let mut correction = vacancy(20102, (1, 0), (2, 0));
    correction["replaces_transition_id"] = json!(id("transition", 20101));
    let corrected = edit(&base, &id("tenure", 1), |record| {
        record["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push(yaml(correction))
    });
    valid(&corrected);
    let result = history(&corrected, "tenure", 1, 2, 0);
    assert_eq!(result["history"][1]["superseded"], true);
    assert_eq!(result["citations"].as_array().unwrap().len(), 3);
    invalid(&edit(&corrected, &id("tenure", 1), |record| {
        record["transitions"][1]["applicability"]["last"]["order"] = yaml(json!(1))
    }));
}

#[test]
fn caused_transfer_vacates_only_source_until_an_independent_target_hold() {
    let world = transfer_fixture();
    valid(&world);
    assert_eq!(
        view(&world, 2, i32::MAX)["holders"][0]["recordId"],
        id("tenure", 1)
    );
    let exact = view(&world, 3, 0);
    assert_eq!(exact["holders"], json!([]));
    assert_eq!(exact["tenures"][0]["lifecycle"], "vacant");
    assert_eq!(exact["tenures"][0]["value"]["holderId"], Json::Null);
    assert_eq!(exact["tenures"][1]["lifecycle"], "vacant");
    assert_eq!(
        exact["tenures"][0]["citations"][1]["causeCitation"]["entityId"],
        id("event", 1)
    );
    assert_eq!(exact["succession"][0]["from"], id("tenure", 1));
    assert_eq!(exact["succession"][0]["to"], id("tenure", 2));
    assert_eq!(
        exact["succession"][0]["linkCitation"]["field"],
        "predecessor_tenure_id"
    );
    assert_eq!(
        view(&world, 3, 1)["holders"][0]["recordId"],
        id("tenure", 2)
    );
    // An independently authored target hold at the transfer point is allowed;
    // target eligibility is evaluated strictly before that exact point.
    let same = edit(&world, &id("tenure", 2), |record| {
        record["transitions"][0]["applicability"]["point"]["order"] = yaml(json!(0))
    });
    valid(&same);
    assert_eq!(view(&same, 3, 0)["holders"][0]["recordId"], id("tenure", 2));
}

#[test]
fn transfers_require_a_prior_cause_and_an_extant_eligible_same_legacy_target() {
    let world = transfer_fixture();
    for time in [
        json!({"timeline":"main","tick":3,"order":0}),
        json!({"timeline":"other","tick":1,"order":0}),
        json!({"tick":1}),
    ] {
        invalid(&edit(&world, &id("event", 1), |record| {
            record["time"] = yaml(time)
        }));
    }
    invalid(&edit(&world, &id("event", 1), |record| {
        record["status"] = yaml(json!("draft"))
    }));
    invalid(&edit(&world, &id("tenure", 1), |record| {
        record["transitions"][0]
            .as_mapping_mut()
            .unwrap()
            .remove("cause_event_id");
    }));
    invalid(&edit(&world, &id("tenure", 1), |record| {
        record["transitions"][0]["cause_event_id"] = Yaml::Null
    }));
    invalid(&edit(&world, &id("tenure", 1), |record| {
        record["transitions"][0]["payload"]["from_tenure_id"] = yaml(json!(id("tenure", 2)))
    }));
    let mut target = tenure(2, None, "legal", 3, 0); // Not extant strictly before transfer.
    let mut from = tenure(1, Some(1), "legal", 0, 0);
    let mut transition = tr(
        20101,
        "tenure-transfer",
        3,
        0,
        json!({"from_tenure_id":id("tenure",1),"to_tenure_id":id("tenure",2)}),
    );
    transition["cause_event_id"] = json!(id("event", 1));
    from["transitions"] = json!([transition]);
    invalid(&fixture(vec![from.clone(), target, event(2, 0)]));
    target = tenure(2, Some(2), "legal", 1, 0);
    invalid(&fixture(vec![from.clone(), target.clone(), event(2, 0)]));
    target["transitions"] = json!([vacancy(20201, (1, 1), (2, 0))]);
    invalid(&fixture(vec![from.clone(), target, event(2, 0)])); // Expired vacancy is not vacant.
    let mut target = tenure(2, None, "legal", 1, 0);
    target["legacy_id"] = json!(id("legacy", 2));
    invalid(&fixture(vec![from.clone(), target, legacy(2), event(2, 0)]));
    let mut target = tenure(2, None, "legal", 1, 0);
    target["initialization"]["applicability"]["point"]["timeline"] = json!("other");
    invalid(&fixture(vec![from, target, event(2, 0)]));
}

#[test]
fn transfer_target_fork_replay_cannot_reopen_a_terminal_eligibility_state() {
    let mut target = tenure(2, None, "legal", 0, 0);
    let hold = tr(
        20201,
        "tenure-hold",
        1,
        0,
        json!({"holder_id":id("character",2),"basis":"legal"}),
    );
    let mut end = tr(20202, "tenure-end", 1, 0, json!({}));
    end["replaces_transition_id"] = json!(id("transition", 20201));
    let mut designate = tr(
        20203,
        "tenure-designate",
        1,
        0,
        json!({"holder_id":id("character",2),"basis":"legal"}),
    );
    designate["replaces_transition_id"] = json!(id("transition", 20201));
    target["transitions"] = json!([hold, end, designate]);
    let target_only = fixture(vec![target.clone()]);
    valid(&target_only);
    assert_eq!(
        history(&target_only, "tenure", 2, 1, 0)["lifecycle"],
        "designated"
    );
    let mut source = tenure(1, Some(1), "legal", 0, 0);
    let mut transfer = tr(
        20101,
        "tenure-transfer",
        2,
        0,
        json!({"from_tenure_id":id("tenure",1),"to_tenure_id":id("tenure",2)}),
    );
    transfer["cause_event_id"] = json!(id("event", 1));
    source["transitions"] = json!([transfer]);
    let candidate = fixture(vec![source, target, event(1, 1)]);
    assert!(
        genealogy::validate(&candidate)
            .iter()
            .any(|error| error.code == "GEN-TENURE-TRANSFER-002")
    );
    invalid(&candidate);
}

#[test]
fn transfer_accepts_designation_without_promoting_the_designated_character() {
    let world = transfer_fixture();
    let designated = edit(&world, &id("tenure", 2), |record| {
        record["transitions"] = yaml(json!([tr(
            20201,
            "tenure-designate",
            2,
            0,
            json!({"holder_id":id("character",2),"basis":"legal"})
        )]))
    });
    valid(&designated);
    let result = view(&designated, 3, 0);
    assert_eq!(result["holders"], json!([]));
    assert_eq!(result["tenures"][1]["lifecycle"], "designated");
}

#[test]
fn tenure_links_require_strict_initial_order_but_not_reciprocal_headers() {
    let mut first = tenure(1, Some(1), "legal", 0, 0);
    first["successor_tenure_id"] = json!(id("tenure", 2));
    let target = tenure(2, None, "legal", 1, 0);
    let base = fixture(vec![first, target]);
    valid(&base);
    assert_eq!(view(&base, 0, 0)["succession"], json!([]));
    assert!(
        view(&base, 0, 0)["tenures"][0]["value"]
            .get("successorTenureId")
            .is_none()
    );
    assert_eq!(
        view(&base, 1, 0)["tenures"][0]["value"]["successorTenureId"],
        id("tenure", 2)
    );
    assert_eq!(view(&base, 1, 0)["succession"].as_array().unwrap().len(), 1);
    invalid(&edit(&base, &id("tenure", 2), |record| {
        record["initialization"]["applicability"]["point"]["tick"] = yaml(json!(0))
    }));
    invalid(&edit(&base, &id("tenure", 2), |record| {
        record["initialization"]["applicability"]["point"]["timeline"] = yaml(json!("other"))
    }));
    invalid(&edit(&base, &id("tenure", 1), |record| {
        record["successor_tenure_id"] = yaml(json!(id("tenure", 1)))
    }));
    let mut other = tenure(2, None, "legal", 1, 0);
    other["legacy_id"] = json!(id("legacy", 2));
    let mut first = tenure(1, None, "legal", 0, 0);
    first["successor_tenure_id"] = json!(id("tenure", 2));
    invalid(&fixture(vec![first, other, legacy(2)]));
}

#[test]
fn typed_references_obey_target_initialization_and_timeline_without_changing_source() {
    let mut later_organization = organization(1);
    later_organization["initialization"]["applicability"]["point"]["tick"] = json!(20);
    let world = fixture(vec![later_organization]);
    valid(&world);
    let original = world.records[&id("legacy", 1)].raw_bytes.clone();
    assert!(
        view(&world, 0, 0)["legacy"]["value"]
            .get("organizationId")
            .is_none()
    );
    assert_eq!(
        view(&world, 20, 0)["legacy"]["value"]["organizationId"],
        id("organization", 1)
    );
    assert_eq!(original, world.records[&id("legacy", 1)].raw_bytes);
    let mut future = legacy(1);
    future["initialization"]["applicability"]["point"]["tick"] = json!(20);
    let world = fixture(vec![future, tenure(1, None, "legal", 0, 0)]);
    valid(&world);
    let before = history(&world, "tenure", 1, 0, 0);
    assert!(before["value"].get("legacyId").is_none());
    assert_eq!(before["value"]["predecessorTenureId"], Json::Null);
    assert_eq!(
        history(&world, "tenure", 1, 20, 0)["value"]["legacyId"],
        id("legacy", 1)
    );
    let other = edit(&world, &id("legacy", 1), |record| {
        record["initialization"]["applicability"]["point"]["timeline"] = yaml(json!("other"))
    });
    valid(&other);
    assert!(
        history(&other, "tenure", 1, 100, 0)["value"]
            .get("legacyId")
            .is_none()
    );
}

#[test]
fn claims_compete_reciprocally_at_exact_applicability_without_appointing_holders() {
    let mut first = claim(1, 1, &[2]);
    let mut second = claim(2, 2, &[1]);
    first["transitions"] = json!([tr(
        30101,
        "claim-dispute",
        2,
        3,
        json!({"competes_with":[id("claim",2)]})
    )]);
    second["transitions"] = json!([tr(
        30201,
        "claim-dispute",
        2,
        3,
        json!({"competes_with":[id("claim",1)]})
    )]);
    let world = fixture(vec![first.clone(), second.clone()]);
    valid(&world);
    let result = view(&world, 2, 3);
    assert_eq!(result["claims"].as_array().unwrap().len(), 2);
    assert_eq!(result["holders"], json!([]));
    assert_eq!(result["claims"][0]["lifecycle"], "disputed");
    assert_eq!(
        result["claims"][0]["value"]["competesWith"],
        json!([id("claim", 2)])
    );
    invalid(&edit(&world, &id("claim", 2), |record| {
        record["transitions"][0]["applicability"]["point"]["order"] = yaml(json!(4))
    }));
    invalid(&edit(&world, &id("claim", 2), |record| {
        record["initialization"]["payload"]["competes_with"] = yaml(json!([]))
    }));
    second["legacy_id"] = json!(id("legacy", 2));
    invalid(&fixture(vec![first, second, legacy(2)]));
    invalid(&fixture(vec![claim(1, 1, &[1])]));
    invalid(&edit(&world, &id("claim", 2), |record| {
        record["status"] = yaml(json!("draft"))
    }));
}

#[test]
fn claim_reciprocity_uses_effective_target_history_and_every_authored_source_entry() {
    let mut first = claim(1, 1, &[]);
    let mut second = claim(2, 2, &[]);
    let first_dispute = tr(
        30101,
        "claim-dispute",
        2,
        0,
        json!({"competes_with":[id("claim",2)]}),
    );
    let second_dispute = tr(
        30201,
        "claim-dispute",
        2,
        0,
        json!({"competes_with":[id("claim",1)]}),
    );
    first["transitions"] = json!([first_dispute.clone()]);
    second["transitions"] = json!([second_dispute.clone()]);
    valid(&fixture(vec![first.clone(), second.clone()]));
    let mut replacement = tr(30202, "claim-dispute", 2, 0, json!({"competes_with":[]}));
    replacement["replaces_transition_id"] = json!(id("transition", 30201));
    second["transitions"] = json!([second_dispute, replacement]);
    invalid(&fixture(vec![first.clone(), second]));
    let mut replacement = tr(30102, "claim-dispute", 2, 0, json!({"competes_with":[]}));
    replacement["replaces_transition_id"] = json!(id("transition", 30101));
    first["transitions"] = json!([first_dispute, replacement]);
    invalid(&fixture(vec![first, claim(2, 2, &[])]));
}

#[test]
fn closed_terminal_lifecycle_rules_cover_legacy_tenure_and_claims() {
    for (kind, payload) in [
        ("legacy-rename", json!({"title":"No reopen","aliases":[]})),
        ("legacy-dormant", json!({})),
        ("legacy-dissolve", json!({})),
    ] {
        let mut source = legacy(1);
        source["transitions"] = json!([
            tr(10101, "legacy-dissolve", 0, 0, json!({})),
            tr(10102, kind, 1, 0, payload)
        ]);
        invalid(&fixture(vec![source]));
    }
    for terminal in ["claim-recognize", "claim-withdraw", "claim-reject"] {
        let mut source = claim(1, 1, &[]);
        source["transitions"] = json!([tr(30101, terminal, 1, 0, json!({}))]);
        valid(&fixture(vec![source.clone()]));
        source["transitions"].as_array_mut().unwrap().push(tr(
            30102,
            "claim-dispute",
            2,
            0,
            json!({"competes_with":[]}),
        ));
        invalid(&fixture(vec![source]));
    }
    let mut source = tenure(1, None, "legal", 0, 0);
    source["transitions"] = json!([
        tr(20101, "tenure-end", 1, 0, json!({})),
        tr(
            20102,
            "tenure-hold",
            2,
            0,
            json!({"holder_id":id("character",1),"basis":"legal"})
        )
    ]);
    invalid(&fixture(vec![source]));
    let mut source = tenure(1, Some(1), "legal", 0, 0);
    source["transitions"] = json!([tr(
        20101,
        "tenure-hold",
        1,
        0,
        json!({"holder_id":id("character",2),"basis":"legal"})
    )]);
    invalid(&fixture(vec![source]));
    let mut source = tenure(1, None, "legal", 0, 0);
    source["transitions"] = json!([vacancy(20101, (1, 0), (2, 0))]);
    invalid(&fixture(vec![source]));
}

#[test]
fn known_vital_state_begins_at_affirmative_events_and_undisclosed_literals_stay_unavailable() {
    let mut source = vital(1, 1, "known");
    source["transitions"] = json!([
        tr(40101, "vital-birth", 0, 2, json!({})),
        tr(40102, "vital-death", 10, 3, json!({}))
    ]);
    let mut concealed = vital(2, 2, "withheld");
    concealed["transitions"] = json!([
        tr(40201, "vital-birth", 0, 0, json!({})),
        tr(40202, "vital-death", 1, 0, json!({}))
    ]);
    let world = fixture(vec![source, concealed, vital(3, 3, "unknown")]);
    valid(&world);
    for (tick, order, state) in [
        (-11, 0, None),
        (-10, 0, None),
        (0, 1, None),
        (0, 2, Some("living")),
        (10, 2, Some("living")),
        (10, 3, Some("dead")),
    ] {
        let result = genealogy::vital(&world, "Person 1", &at(tick, order), 100).unwrap();
        assert_eq!(result["vital"], json!(state));
        assert_eq!(result["available"], state.is_some());
    }
    assert_eq!(
        history(&world, "vital-history", 1, -10, 0)["lifecycle"],
        "unknown"
    );
    for number in [2, 3, 4] {
        let result = genealogy::vital(&world, &id("character", number), &at(20, 0), 1).unwrap();
        assert_eq!(result["reason"], "unknown");
        assert_eq!(result["items"], json!([]));
        assert_eq!(result["states"], json!([]));
        assert_eq!(result["vital"], Json::Null);
    }
    for number in [2, 3] {
        let result =
            genealogy::history(&world, &id("vital-history", number), &at(20, 0), 1).unwrap();
        assert_eq!(result["available"], false);
        assert_eq!(result["reason"], "undisclosed");
        assert_eq!(result["history"], json!([]));
        assert!(result.get("lifecycle").is_none());
        assert!(result.get("citations").is_none());
    }
    let other = genealogy::vital(
        &world,
        "Person 1",
        &StoryTime::new("other", 100, 0).unwrap(),
        100,
    )
    .unwrap();
    assert_eq!(other["available"], false);
}

#[test]
fn unknown_vital_source_cannot_contain_affirmative_transitions_even_in_drafts() {
    for status in ["canonical", "draft", "retired"] {
        for kind in ["vital-birth", "vital-existence-start"] {
            let mut source = vital(1, 1, "unknown");
            source["status"] = json!(status);
            source["transitions"] = json!([tr(40101, kind, 0, 0, json!({}))]);
            let candidate = fixture(vec![source]);
            assert!(
                genealogy::validate(&candidate)
                    .iter()
                    .any(|error| error.code == "GEN-VITAL-002"
                        && error.field.as_deref() == Some("transitions"))
            );
            invalid(&candidate);
        }
    }
    let mut source = vital(1, 1, "known");
    source["transitions"] = json!([tr(40101, "vital-birth", 0, 0, json!({}))]);
    let base = fixture(vec![source]);
    invalid(&edit(&base, &id("vital-history", 1), |record| {
        record["disclosure"] = yaml(json!("unknown"))
    }));
    valid(&edit(&base, &id("vital-history", 1), |record| {
        record["disclosure"] = yaml(json!("withheld"))
    }));
}

#[test]
fn multiple_known_vital_histories_preserve_all_assertions_without_picking_a_winner() {
    let mut living = vital(1, 1, "known");
    living["transitions"] = json!([tr(40101, "vital-birth", 0, 0, json!({}))]);
    let mut dead = vital(2, 1, "known");
    dead["transitions"] = json!([
        tr(40201, "vital-birth", 0, 0, json!({})),
        tr(40202, "vital-death", 1, 0, json!({}))
    ]);
    let world = fixture(vec![living, dead]);
    valid(&world);
    let earlier = genealogy::vital(&world, "Person 1", &at(0, 0), 100).unwrap();
    assert_eq!(earlier["vital"], "living");
    assert_eq!(earlier["ambiguous"], false);
    assert_eq!(earlier["count"], 2);
    let later = genealogy::vital(&world, "Person 1", &at(1, 0), 100).unwrap();
    assert_eq!(later["vital"], Json::Null);
    assert_eq!(later["states"], json!(["dead", "living"]));
    assert_eq!(later["ambiguous"], true);
    assert_eq!(later["items"][0]["recordId"], id("vital-history", 1));
    assert_eq!(later["items"][1]["recordId"], id("vital-history", 2));
}

#[test]
fn vital_branches_are_closed_and_corrections_replay_from_the_target_prestate() {
    for (first, second) in [
        ("vital-birth", "vital-existence-start"),
        ("vital-existence-start", "vital-birth"),
        ("vital-birth", "vital-existence-end"),
        ("vital-existence-start", "vital-death"),
        ("vital-death", "vital-birth"),
    ] {
        let mut source = vital(1, 1, "known");
        source["transitions"] = json!([
            tr(40101, first, 0, 0, json!({})),
            tr(40102, second, 1, 0, json!({}))
        ]);
        invalid(&fixture(vec![source]));
    }
    let mut correction = tr(40102, "vital-existence-start", 0, 1, json!({}));
    correction["replaces_transition_id"] = json!(id("transition", 40101));
    let mut source = vital(1, 1, "known");
    source["transitions"] = json!([
        tr(40101, "vital-birth", 0, 1, json!({})),
        correction,
        tr(40103, "vital-existence-end", 2, 0, json!({}))
    ]);
    let world = fixture(vec![source]);
    valid(&world);
    assert_eq!(
        genealogy::vital(&world, "Person 1", &at(0, 0), 100).unwrap()["vital"],
        Json::Null
    );
    assert_eq!(
        genealogy::vital(&world, "Person 1", &at(0, 1), 100).unwrap()["vital"],
        "existing"
    );
    let result = history(&world, "vital-history", 1, 0, 1);
    assert_eq!(result["history"][1]["superseded"], true);
    assert_eq!(
        result["appliedTransitionIds"],
        json!([id("transition", 40100), id("transition", 40102)])
    );
    assert_eq!(
        genealogy::vital(&world, "Person 1", &at(2, 0), 100).unwrap()["vital"],
        "ended"
    );
}

#[test]
fn reverse_legacies_include_visible_terminal_states_and_filter_before_limits() {
    let mut dormant = legacy(2);
    dormant["transitions"] = json!([tr(10201, "legacy-dormant", 0, 0, json!({}))]);
    let mut dissolved = legacy(3);
    dissolved["transitions"] = json!([tr(10301, "legacy-dissolve", 0, 0, json!({}))]);
    let mut future = legacy(4);
    future["initialization"]["applicability"]["point"]["tick"] = json!(30);
    let mut draft = legacy(5);
    draft["status"] = json!("draft");
    let world = fixture(vec![dormant, dissolved, future, draft]);
    valid(&world);
    let result = genealogy::organization_legacies(&world, "Organization 1", &at(0, 0), 3).unwrap();
    assert_eq!(result["count"], 3);
    assert_eq!(result["items"][0]["lifecycle"], "active");
    assert_eq!(result["items"][1]["lifecycle"], "dormant");
    assert_eq!(result["items"][2]["lifecycle"], "dissolved");
    assert!(genealogy::organization_legacies(&world, "Organization 1", &at(0, 0), 2).is_err());
    let before =
        genealogy::organization_legacies(&world, "Organization 1", &at(-31, 0), 1).unwrap();
    assert_eq!(before["available"], false);
    let mut org = organization(2);
    org["transitions"] = json!([tr(
        1201,
        "organization-rename",
        0,
        0,
        json!({"title":"Longer source history","aliases":["A","B"]})
    )]);
    let empty = fixture(vec![org]);
    assert_eq!(
        genealogy::organization_legacies(&empty, &id("organization", 2), &at(0, 0), 1).unwrap()["count"],
        0
    );
}

#[test]
fn complete_limits_cover_each_collection_and_visible_nested_history() {
    let mut with_aliases = legacy(1);
    with_aliases["initialization"]["payload"]["aliases"] = json!(["A", "B", "C"]);
    let world = fixture(vec![with_aliases]);
    assert!(genealogy::legacy(&world, "Legacy 1", &at(0, 0), 2).is_err());
    let world = fixture(vec![
        tenure(1, None, "legal", 0, 0),
        tenure(2, None, "legal", 0, 0),
        claim(1, 1, &[]),
        claim(2, 2, &[]),
    ]);
    assert!(genealogy::legacy(&world, "Legacy 1", &at(0, 0), 1).is_err());
    let result = genealogy::legacy(&world, "Legacy 1", &at(0, 0), 2).unwrap();
    assert_eq!(result["count"], 4);
    assert_eq!(result["complete"], true);
    let mut source = vital(1, 1, "known");
    source["transitions"] = json!([
        tr(40101, "vital-birth", 0, 0, json!({})),
        tr(40102, "vital-death", 1, 0, json!({}))
    ]);
    let world = fixture(vec![source]);
    assert!(genealogy::vital(&world, "Person 1", &at(1, 0), 2).is_err());
    assert!(genealogy::history(&world, &id("vital-history", 1), &at(1, 0), 2).is_err());
    for limit in [0, 1001] {
        assert!(genealogy::legacy(&world, "Legacy 1", &at(0, 0), limit).is_err());
        assert!(genealogy::vital(&world, "Person 1", &at(0, 0), limit).is_err());
    }
}

#[test]
fn source_grammar_rejects_bad_discriminators_references_and_nullable_payloads() {
    for (kind, key, value) in [
        ("legacy", "legacy_kind", json!("crown")),
        ("legacy", "organization_id", json!(id("character", 1))),
        ("tenure", "legacy_id", json!(id("legacy", 99))),
        ("claim", "claimant_id", json!("Character 1")),
        ("vital-history", "disclosure", json!("public")),
    ] {
        let mut record = match kind {
            "legacy" => legacy(1),
            "tenure" => tenure(1, None, "legal", 0, 0),
            "claim" => claim(1, 1, &[]),
            _ => vital(1, 1, "known"),
        };
        record[key] = value;
        invalid(&fixture(vec![record]));
    }
    for (kind, payload) in [
        ("tenure-hold", json!({"holder_id":null,"basis":"legal"})),
        (
            "tenure-designate",
            json!({"holder_id":null,"basis":"legal"}),
        ),
        (
            "tenure-hold",
            json!({"holder_id":id("character",1),"basis":"inherited"}),
        ),
        ("tenure-end", json!({"holder_id":null})),
    ] {
        let mut source = tenure(1, None, "legal", 0, 0);
        source["transitions"] = json!([tr(20101, kind, 1, 0, payload)]);
        invalid(&fixture(vec![source]));
    }
    let mut source = tenure(1, Some(1), "legal", 0, 0);
    let mut entry = vacancy(20101, (1, 0), (2, 0));
    entry["payload"] = json!({});
    source["transitions"] = json!([entry]);
    invalid(&fixture(vec![source]));
    let mut source = legacy(1);
    source["initialization"]["payload"]["aliases"] = json!(["B", "A"]);
    invalid(&fixture(vec![source]));
    let mut source = claim(1, 1, &[]);
    source["initialization"]["payload"]["competes_with"] = Json::Null;
    invalid(&fixture(vec![source]));
    let mut source = vital(1, 1, "known");
    source["initialization"]["payload"] = json!({"state":"living"});
    invalid(&fixture(vec![source]));
    let mut source = tenure(1, None, "legal", 0, 0);
    source["predecessor_tenure_id"] = Json::Null;
    valid(&fixture(vec![source]));
}

#[test]
fn canonical_status_controls_results_while_drafts_remain_validated() {
    let mut tenure = tenure(1, Some(1), "legal", 0, 0);
    tenure["status"] = json!("draft");
    let mut claim = claim(1, 2, &[]);
    claim["status"] = json!("retired");
    let mut vital = vital(1, 1, "known");
    vital["status"] = json!("draft");
    vital["transitions"] = json!([tr(40101, "vital-birth", 0, 0, json!({}))]);
    let world = fixture(vec![tenure, claim, vital]);
    valid(&world);
    assert_eq!(view(&world, 1, 0)["count"], 0);
    assert_eq!(
        genealogy::vital(&world, "Person 1", &at(1, 0), 100).unwrap()["available"],
        false
    );
    for kind in ["tenure", "claim", "vital-history"] {
        let result = history(&world, kind, 1, 1, 0);
        assert_eq!(result["reason"], "noncanonical");
    }
    invalid(&edit(&world, &id("tenure", 1), |record| {
        record["initialization"]["payload"]["basis"] = yaml(json!("illegitimate"))
    }));
    let source = fixture(vec![]);
    let draft = edit(&source, &id("legacy", 1), |record| {
        record["status"] = yaml(json!("draft"))
    });
    assert_eq!(view(&draft, 0, 0)["reason"], "noncanonical");
    assert!(genealogy::legacy(&source, "Person 1", &at(0, 0), 100).is_err());
    assert!(genealogy::vital(&source, "Legacy 1", &at(0, 0), 100).is_err());
}

#[test]
fn all_bundled_canonical_domains_validate_and_queries_preserve_source_bytes() {
    let world = source::load_world(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass"),
        None,
    )
    .unwrap();
    valid(&world);
    let bytes: BTreeMap<_, _> = world
        .records
        .iter()
        .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
        .collect();
    let office = "legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ";
    let result = genealogy::legacy(&world, office, &at(20, 0), 100).unwrap();
    assert_eq!(result["holders"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["holders"][0]["value"]["holderId"],
        "char_46VTBJZ9TNMTF7G14N5GKYQ9C1"
    );
    assert_eq!(result["claims"][0]["lifecycle"], "disputed");
    assert_eq!(result["succession"], json!([]));
    let linked = genealogy::organization_legacies(
        &world,
        "organization_4YNRVZSRYTGBB27YKD7NW1T7CZ",
        &at(20, 0),
        100,
    )
    .unwrap();
    assert_eq!(linked["items"][0]["recordId"], office);
    let vital =
        genealogy::vital(&world, "char_39E9EWWC017E40519033S4RW4X", &at(20, 0), 100).unwrap();
    assert_eq!(vital["vital"], "living");
    assert_eq!(
        bytes,
        world
            .records
            .iter()
            .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
            .collect()
    );
}
