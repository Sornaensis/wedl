//! Existence and governance use the reviewed writer without altering learned or vital answers.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeSet, fs};
use wedl::{
    Error, assertion, authoring, character, chronology, description, existence, generational,
    governance,
    model::{Record, StoryTime, World},
    record_edit, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const HARBOR: &str = "loc_00000000000000000000000001";
const GUILD: &str = "organization_00000000000000000000000001";
const EPISODE: &str = "existence_00000000000000000000000001";
const OPEN: &str = "existence_00000000000000000000000003";
const REQUESTS: [&str; 17] = [
    include_str!("../examples/authoring/126-world-governance-profile.yaml"),
    include_str!("../examples/authoring/127-embodied-episode.yaml"),
    include_str!("../examples/authoring/128-disembodied-episode.yaml"),
    include_str!("../examples/authoring/129-constructed-episode.yaml"),
    include_str!("../examples/authoring/130-unbounded-other-episode.yaml"),
    include_str!("../examples/authoring/131-unknown-episode.yaml"),
    include_str!("../examples/authoring/132-alternative-embodied-episode.yaml"),
    include_str!("../examples/authoring/133-episode-description-and-date.yaml"),
    include_str!("../examples/authoring/134-legal-harbor-policy.yaml"),
    include_str!("../examples/authoring/135-de-facto-overlay-policy.yaml"),
    include_str!("../examples/authoring/136-claimed-warning-policy.yaml"),
    include_str!("../examples/authoring/137-contested-harbor-policy.yaml"),
    include_str!("../examples/authoring/138-policy-account.yaml"),
    include_str!("../examples/authoring/139-guild-policy-endorsement.yaml"),
    include_str!("../examples/authoring/140-contrary-policy-account.yaml"),
    include_str!("../examples/authoring/141-correct-policy-account.yaml"),
    include_str!("../examples/authoring/142-literal-policy-note.yaml"),
];
fn id(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn request(value: &impl serde::Serialize) -> String {
    serde_yaml::to_string(value).unwrap()
}
fn unchanged(world: &World) {
    let loaded = source::load_world(&world.root, None).unwrap();
    assert_eq!(loaded.fingerprint, world.fingerprint);
    for (id, record) in &world.records {
        assert_eq!(loaded.records[id].raw_bytes, record.raw_bytes);
    }
}
fn retained(before: &Record, after: &Record, changed: &[&str]) {
    assert_eq!(after.body, before.body);
    assert_eq!(after.id, before.id);
    assert_eq!(after.source_path, before.source_path);
    for (key, value) in before.frontmatter.as_mapping().unwrap() {
        if !changed.contains(&key.as_str().unwrap()) {
            assert_eq!(after.frontmatter.get(key), Some(value), "{key:?}");
        }
    }
}
fn save(world: &World, target: Option<&str>, input: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    }
    .unwrap_or_else(|error| {
        panic!(
            "{error}; base diagnostics: {:?}",
            validation::validate_world(world).diagnostics
        )
    });
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(preview["writesPerformed"], false);
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    let receipt = match target {
        Some(id) => authoring::apply_record_update(world, id, input, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, input, &world.fingerprint, hash),
    }
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(receipt["fingerprint"], saved.fingerprint);
    let edited = preview["entityId"].as_str().unwrap();
    let path = preview["sourcePath"].as_str().unwrap();
    assert_eq!(
        fs::read(world.root.join(path)).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert_eq!(saved.records[edited].source_path, path);
    for (id, record) in &world.records {
        if id != edited {
            assert_eq!(saved.records[id].raw_bytes, record.raw_bytes);
        }
    }
    let report = validation::validate_world(&saved);
    assert!(report.valid, "{:?}", report.diagnostics);
    saved
}
fn rejected(world: &World, target: Option<&str>, input: &str) {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    };
    match preview {
        Ok(preview) => {
            assert_eq!(preview["canApply"], false, "{preview}");
            let hash = preview["afterSha256"].as_str().unwrap();
            let applied = match target {
                Some(id) => {
                    authoring::apply_record_update(world, id, input, &world.fingerprint, hash)
                }
                None => authoring::apply_record_create(world, input, &world.fingerprint, hash),
            };
            assert!(matches!(applied, Err(Error::Validation(_))), "{applied:?}");
        }
        Err(error) => assert!(
            matches!(error, Error::InvalidInput(_) | Error::Validation(_)),
            "{error}"
        ),
    }
    unchanged(world);
}
fn direct_source(world: &World, target: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let original = &world.records[target];
    let mut fields = original.frontmatter.clone();
    edit(&mut fields);
    fs::write(
        world.root.join(&original.source_path),
        format!("---\n{}---\n{}", request(&fields), original.body),
    )
    .unwrap();
    source::load_world(&world.root, None).unwrap()
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let world = source::init_world(&dir.path().join("world"), "Reviewed facts").unwrap();
    let world_id = world.world_record().unwrap().id.clone();
    let mut world = direct_source(&world, &world_id, |v| {
        v["capabilities"] = yaml(
            "[generational-core-v1, spatial-core-v1, overlay-v1, lore-core-v1, descriptions-v1, assertions-v1, existence-v1, collective-parties-v1]",
        );
        v["author_metadata"] = yaml(
            "{integer: 9007199254740993, decimal: 1.0, chronology: literal-note, entity: missing-record, optional: null}",
        );
    });
    for input in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/27-organization.yaml"),
        include_str!("../examples/authoring/32-vital.yaml"),
    ] {
        world = save(&world, None, input);
    }
    world = save(
        &world,
        Some(&world_id),
        include_str!("../examples/authoring/19-world-chronology.yaml"),
    );
    (dir, world)
}
fn governance_contexts(mut world: World) -> World {
    for identifier in ["location:harbor/boardwalk", "location:harbor/landing"] {
        world = save(
            &world,
            None,
            &request(&json!({"kind":"location","id":identifier,"title":identifier})),
        );
    }
    for input in [
        include_str!("../examples/authoring/54-static-overlay.yaml"),
        include_str!("../examples/authoring/55-dated-overlay.yaml"),
    ] {
        world = save(&world, None, input);
    }
    for (n, profile) in [(23, "social-culture"), (24, "social-religion")] {
        world = save(
            &world,
            None,
            &request(&json!({"kind":"lore-concept","id":id("concept",n),
            "title":profile,"domain":"authored.context","profile":profile,"values":{},
            "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})),
        );
    }
    world
}
fn binding() -> governance::Binding {
    governance::Binding {
        profile: "harbor-policy".into(),
        roles: [
            ("territory", "jurisdiction"),
            ("party", "issuer"),
            ("basis", "basis"),
            ("subjects", "subjects"),
            ("obligations", "duties"),
            ("supersedes", "supersedes"),
        ]
        .into_iter()
        .map(|(role, field)| (role.into(), Some(field.into())))
        .collect(),
    }
}
fn without_fingerprint(value: &mut Json) {
    match value {
        Json::Object(fields) => {
            fields.remove("fingerprint");
            for value in fields.values_mut() {
                without_fingerprint(value);
            }
        }
        Json::Array(values) => values.iter_mut().for_each(without_fingerprint),
        _ => {}
    }
}
fn ids(value: &Json, field: &str) -> Vec<String> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v[field].as_str().unwrap().to_owned())
        .collect()
}
fn annotations(world: &World) -> Json {
    chronology::annotations(
        world,
        &chronology::AnnotationFilter {
            record: Some(EPISODE.into()),
            calendar: None,
            era: None,
            limit: 100,
        },
    )
    .unwrap()
}

#[test]
fn all_seventeen_examples_publish_exact_bytes_and_preserve_independent_accounts_and_learning() {
    let (_dir, before) = fixture();
    let context = character::context(&before, ANA, &at(5, 0), 100_000).unwrap();
    let search = character::search(&before, ANA, "bell", &at(5, 0), 100).unwrap();
    let budget = character::context(&before, ANA, &at(5, 0), 1).map_err(|e| e.to_string());
    let mut vital = generational::vital(&before, ANA, &at(5, 0), 100).unwrap();
    without_fingerprint(&mut vital);
    let world_id = before.world_record().unwrap().id.clone();
    let mut world = save(&before, Some(&world_id), REQUESTS[0]);
    retained(
        before.world_record().unwrap(),
        world.world_record().unwrap(),
        &["profiles", "predicates"],
    );
    world = governance_contexts(world);
    let count_before = world.records.len();
    for (index, input) in REQUESTS.iter().enumerate().skip(1) {
        let prior = world.records.get(EPISODE).cloned();
        world = save(&world, (index == 7).then_some(EPISODE), input);
        if index == 7 {
            retained(
                prior.as_ref().unwrap(),
                &world.records[EPISODE],
                &["descriptions", "chronology"],
            );
        }
    }
    assert_eq!(world.records.len(), count_before + 15);
    assert_eq!(
        world.records[EPISODE].source_path,
        format!("story/existence/{EPISODE}.md")
    );
    let episodes = existence::list(&world, ANA, Some(&at(5, 0)), "author", "author", 100).unwrap();
    assert_eq!(
        ids(&episodes, "episodeId"),
        [1, 2, 4, 5, 6].map(|n| id("existence", n))
    );
    assert_eq!(
        existence::list(&world, ANA, None, "author", "author", 100).unwrap()["count"],
        6
    );
    assert_eq!(annotations(&world)["count"], 1);
    assert_eq!(
        description::list(&world, EPISODE, Some(&at(5, 0)), "author", "author", 100).unwrap()["count"],
        1
    );
    assert!(matches!(
        description::list(&world, EPISODE, Some(&at(10, 0)), "author", "author", 100),
        Err(Error::NotFound(_))
    ));
    let policies = governance::query(
        &world,
        HARBOR,
        &binding(),
        Some(&at(5, 0)),
        "author",
        "author",
        100,
    )
    .unwrap();
    assert_eq!(
        ids(&policies, "conceptId"),
        [33, 34, 36].map(|n| id("concept", n))
    );
    let legal = &policies["items"][0];
    assert_eq!(legal["roles"]["obligations"]["amount"], "9007199254740993");
    assert!(legal["roles"]["obligations"]["exception"].is_null());
    assert_eq!(legal["roleKnown"]["obligations"], true);
    assert_eq!(policies["items"][1]["roleKnown"]["obligations"], false);
    assert_eq!(
        legal["parties"]
            .as_array()
            .unwrap()
            .iter()
            .map(|party| party["kind"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        ["character", "organization", "concept"]
            .into_iter()
            .collect()
    );
    let statement_ids = |item: &Json| {
        item["statements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["assertionId"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        statement_ids(legal),
        [id("assertion", 13), id("assertion", 15)]
    );
    assert_eq!(statement_ids(&policies["items"][2]), [id("assertion", 14)]);
    assert_eq!(
        policies["items"][2]["roles"]["supersedes"]["id"],
        id("concept", 33)
    );
    assert_eq!(
        policies["items"][2]["supersedesCitation"]["entityId"],
        id("concept", 33)
    );
    assert_eq!(
        governance::query(
            &world,
            HARBOR,
            &binding(),
            Some(&at(10, 0)),
            "author",
            "author",
            100
        )
        .unwrap()["count"],
        4
    );
    assert_eq!(
        governance::query(
            &world,
            HARBOR,
            &binding(),
            Some(&at(12, 1)),
            "author",
            "author",
            100
        )
        .unwrap()["count"],
        3
    );
    let history =
        governance::query(&world, HARBOR, &binding(), None, "author", "author", 100).unwrap();
    // A time-bounded overlay needs an explicit membership horizon.
    assert_eq!(history["count"], 3);
    assert_eq!(
        statement_ids(&history["items"][0]),
        [12, 13, 15].map(|n| id("assertion", n))
    );
    assert!(!policies.to_string().contains(&id("assertion", 16)));
    assert!(
        ids(
            &assertion::as_of(&world, GUILD, Some(&at(5, 0)), "author", "author", 100).unwrap(),
            "assertionId"
        )
        .contains(&id("assertion", 16))
    );
    assert_eq!(
        character::context(&world, ANA, &at(5, 0), 100_000).unwrap(),
        context
    );
    assert_eq!(
        character::search(&world, ANA, "bell", &at(5, 0), 100).unwrap(),
        search
    );
    assert_eq!(
        character::context(&world, ANA, &at(5, 0), 1).map_err(|e| e.to_string()),
        budget
    );
    let mut after_vital = generational::vital(&world, ANA, &at(5, 0), 100).unwrap();
    without_fingerprint(&mut after_vital);
    assert_eq!(after_vital, vital);
    assert_eq!(
        world.current_time().unwrap(),
        before.current_time().unwrap()
    );
    for (id, record) in &before.records {
        if id != &world_id {
            assert_eq!(world.records[id].raw_bytes, record.raw_bytes);
        }
    }
}

#[test]
fn episode_updates_preserve_common_fields_extensions_body_and_noop_bytes() {
    let (_dir, base) = fixture();
    let created = save(&base, None, REQUESTS[1]);
    let original = &created.records[EPISODE];
    let mut fields = original.frontmatter.clone();
    fields["author_metadata"] = yaml(
        "{integer: 9007199254740993, decimal: 1.0, chronology: literal, target: nonexistent-id, optional: null}",
    );
    let body = "\r\n# Untimed source\r\nDo not infer a date from 2040.  \rTail\n";
    let raw = format!(
        "---\r\n# Keep this header on no-op updates.\r\n{}---\r\n{body}",
        request(&fields).replace('\n', "\r\n")
    );
    fs::write(created.root.join(&original.source_path), raw.as_bytes()).unwrap();
    let world = source::load_world(&created.root, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    assert_eq!(
        record_edit::update(&world, EPISODE, "kind: existence-episode\n")
            .unwrap()
            .raw_bytes,
        raw.as_bytes()
    );
    let described = save(&world, Some(EPISODE), REQUESTS[7]);
    retained(
        &world.records[EPISODE],
        &described.records[EPISODE],
        &["descriptions", "chronology"],
    );
    assert_eq!(described.records[EPISODE].body, body);
    assert!(described.records[EPISODE].raw_bytes.starts_with(b"---\r\n"));
    let renamed = save(
        &described,
        Some(EPISODE),
        "kind: existence-episode\ntitle: A revised authored label\nstatus: retired\naliases: [An account]\n",
    );
    retained(
        &described.records[EPISODE],
        &renamed.records[EPISODE],
        &["title", "status", "aliases"],
    );
    assert_eq!(annotations(&renamed)["count"], 1);
    assert_eq!(
        existence::list(&renamed, ANA, None, "author", "author", 100).unwrap()["count"],
        0
    );
    let cleared = save(
        &renamed,
        Some(EPISODE),
        "kind: existence-episode\nchronology: []\ndescriptions: []\n",
    );
    retained(
        &renamed.records[EPISODE],
        &cleared.records[EPISODE],
        &["chronology", "descriptions"],
    );
    assert_eq!(annotations(&cleared)["count"], 0);
    assert_eq!(
        cleared.records[EPISODE].get("descriptions"),
        Some(&yaml("[]"))
    );
    let before = &cleared.records[EPISODE];
    let header_len = before.raw_bytes.len() - before.body.len();
    let rewritten = save(
        &cleared,
        Some(EPISODE),
        &request(
            &json!({"kind":"existence-episode","body":"Replacement body.\r\nExact trailing spaces.  \n"}),
        ),
    );
    assert_eq!(
        &rewritten.records[EPISODE].raw_bytes[..header_len],
        &before.raw_bytes[..header_len]
    );
    assert_eq!(rewritten.records[EPISODE].frontmatter, before.frontmatter);
}

#[test]
fn interval_and_evidence_replacements_are_complete_and_optional_removal_is_explicit() {
    let (_dir, base) = fixture();
    let world = save(&base, None, REQUESTS[3]);
    let mut bounded = yaml(REQUESTS[1])["valid"].clone();
    bounded.as_mapping_mut().unwrap().remove("start");
    rejected(
        &world,
        Some(OPEN),
        &request(&json!({"kind":"existence-episode","valid":bounded})),
    );
    let evidence = yaml(REQUESTS[2])["end_evidence"].clone();
    rejected(
        &world,
        Some(OPEN),
        &request(&json!({"kind":"existence-episode","end_evidence":evidence})),
    );
    let changed = save(
        &world,
        Some(OPEN),
        &request(&json!({"kind":"existence-episode","valid":bounded,"end_evidence":evidence})),
    );
    assert!(
        changed.records[OPEN]
            .get("valid")
            .unwrap()
            .get("start")
            .is_none()
    );
    assert_eq!(
        changed.records[OPEN].get("start_evidence"),
        world.records[OPEN].get("start_evidence")
    );
    retained(
        &world.records[OPEN],
        &changed.records[OPEN],
        &["valid", "end_evidence"],
    );
    let alternate = yaml(REQUESTS[4])["start_evidence"].clone();
    let replaced = save(
        &changed,
        Some(OPEN),
        &request(&json!({"kind":"existence-episode","start_evidence":alternate})),
    );
    assert_eq!(
        replaced.records[OPEN].get("start_evidence"),
        Some(&alternate)
    );
    retained(
        &changed.records[OPEN],
        &replaced.records[OPEN],
        &["start_evidence"],
    );
    let omitted = save(
        &replaced,
        Some(OPEN),
        "kind: existence-episode\nprovenance: A changed provenance note\n",
    );
    retained(
        &replaced.records[OPEN],
        &omitted.records[OPEN],
        &["provenance"],
    );
    let open = yaml("{kind: interval, timeline: main}");
    for patch in [
        json!({"kind":"existence-episode","valid":open}),
        json!({"kind":"existence-episode","valid":open,"end_evidence":[]}),
        json!({"kind":"existence-episode","valid":open,"end_evidence":null}),
        json!({"kind":"existence-episode","remove":["end_evidence"]}),
    ] {
        rejected(&omitted, Some(OPEN), &request(&patch));
    }
    let direct = direct_source(&omitted, OPEN, |v| {
        v["valid"] = open;
        v.as_mapping_mut().unwrap().remove("end_evidence");
    });
    assert!(validation::validate_world(&direct).valid);
    let saved = save(
        &direct,
        Some(OPEN),
        "kind: existence-episode\ntitle: An explicitly unbounded interval\n",
    );
    assert!(saved.records[OPEN].get("end_evidence").is_none());
    assert!(
        existence::list(&saved, ANA, None, "author", "author", 100).unwrap()["items"][0]
            .get("endEvidence")
            .is_none()
    );
    assert_eq!(saved.records[OPEN].body, world.records[OPEN].body);
}

#[test]
fn reviewed_source_checks_required_shapes_and_semantic_dependencies_for_every_status() {
    let (_dir, base) = fixture();
    for field in [
        "domain",
        "character_id",
        "episode_kind",
        "valid",
        "start_evidence",
        "audience",
        "perspectives",
    ] {
        let mut input = yaml(REQUESTS[1]);
        input.as_mapping_mut().unwrap().remove(field);
        rejected(&base, None, &request(&input));
    }
    for (field, value) in [
        ("episode_kind", json!("resurrected")),
        ("episode_kind", json!([])),
        ("character_id", json!(HARBOR)),
        ("character_id", json!("char_00000000000000000000000999")),
        ("valid", json!({"kind":"timeless"})),
        (
            "valid",
            json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":"9007199254740993","order":0}}),
        ),
        ("start_evidence", Json::Null),
        ("start_evidence", json!({})),
        ("audience", json!(["z", "a"])),
        ("status", json!("active")),
        ("unknown_edit_field", json!("x")),
    ] {
        let mut input = yaml(REQUESTS[1]);
        input[field] = serde_yaml::to_value(value).unwrap();
        rejected(&base, None, &request(&input));
    }
    for status in ["canonical", "draft", "retired"] {
        let mut input = yaml(REQUESTS[1]);
        input["status"] = yaml(status);
        input["start_evidence"] = yaml("[]");
        rejected(&base, None, &request(&input));
    }
    let world = save(&base, None, REQUESTS[5]);
    rejected(
        &world,
        Some(&id("existence", 5)),
        "kind: existence-episode\nepisode_kind: embodied\n",
    );
    rejected(
        &world,
        Some(&id("existence", 5)),
        "kind: existence-episode\nend_evidence: []\n",
    );
    let mut unknown = yaml(REQUESTS[5]);
    unknown["id"] = yaml(&id("existence", 9));
    unknown["valid"]["start"] = yaml("{timeline: main, tick: 0, order: 0}");
    rejected(&world, None, &request(&unknown));
}

#[test]
fn reviewed_evidence_keeps_historical_targets_inert_and_rejects_malformed_citations() {
    let (_dir, base) = fixture();
    let world = save(&base, None, REQUESTS[1]);
    let evidence = json!([{"revision":"a".repeat(40),"path":"lost/accounts/old.md","blob_oid":"b".repeat(64),
        "target":{"kind":"transition","owner_id":"no-current-owner","id":"no-current-member"}}]);
    let saved = save(
        &world,
        Some(EPISODE),
        &request(
            &json!({"kind":"existence-episode","start_evidence":evidence,"end_evidence":evidence}),
        ),
    );
    let item = &existence::list(&saved, ANA, None, "author", "author", 100).unwrap()["items"][0];
    assert_eq!(item["startEvidence"], evidence);
    assert_eq!(item["endEvidence"], evidence);
    assert_eq!(item["evidenceStatus"], "unverified-historical");
    assert_eq!(
        item["citation"]["sourcePath"],
        saved.records[EPISODE].source_path
    );
    assert!(!saved.records.contains_key("no-current-owner"));
    for (field, value) in [
        ("revision", json!("f".repeat(39))),
        ("path", json!("../outside.md")),
        (
            "target",
            json!({"kind":"transition","owner_id":"old-owner","id":"old-member","extra":"x"}),
        ),
    ] {
        let mut invalid = evidence.clone();
        invalid[0][field] = value;
        for endpoint in ["start_evidence", "end_evidence"] {
            let mut patch = json!({"kind":"existence-episode"});
            patch[endpoint] = invalid.clone();
            rejected(&saved, Some(EPISODE), &request(&patch));
        }
    }
    let duplicate = json!([evidence[0].clone(), evidence[0].clone()]);
    rejected(
        &saved,
        Some(EPISODE),
        &request(&json!({"kind":"existence-episode","start_evidence":duplicate})),
    );
    let bad_confirmation = authoring::preview_record_update(
        &saved,
        EPISODE,
        "kind: existence-episode\ntitle: First review\n",
        &saved.fingerprint,
    )
    .unwrap();
    assert!(matches!(
        authoring::apply_record_update(
            &saved,
            EPISODE,
            "kind: existence-episode\ntitle: Changed after review\n",
            &saved.fingerprint,
            bad_confirmation["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Conflict(_))
    ));
    unchanged(&saved);
}

#[test]
fn source_validation_reserves_only_operational_episode_chronology_and_requires_capabilities() {
    let (_dir, base) = fixture();
    let world = save(&base, None, REQUESTS[1]);
    let id = world.world_record().unwrap().id.clone();
    for capability in ["existence-v1", "generational-core-v1"] {
        let missing = direct_source(&world, &id, |v| {
            v["capabilities"]
                .as_sequence_mut()
                .unwrap()
                .retain(|v| v.as_str() != Some(capability));
        });
        let report = validation::validate_world(&missing);
        assert!(!report.valid);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "WDL-EXISTENCE-001")
        );
        rejected(
            &missing,
            Some(EPISODE),
            "kind: existence-episode\ntitle: Unavailable capability\n",
        );
        fs::write(
            world.root.join(&world.records[&id].source_path),
            &world.records[&id].raw_bytes,
        )
        .unwrap();
    }
    for path in [
        ["valid", "", ""],
        ["valid", "start", ""],
        ["valid", "end", ""],
        ["start_evidence", "0", ""],
        ["start_evidence", "0", "target"],
        ["end_evidence", "0", ""],
        ["end_evidence", "0", "target"],
    ] {
        let malformed = direct_source(&world, EPISODE, |v| {
            let mut selected = &mut v[path[0]];
            if path[1] == "0" {
                selected = &mut selected[0];
            } else if !path[1].is_empty() {
                selected = &mut selected[path[1]];
            }
            if !path[2].is_empty() {
                selected = &mut selected[path[2]];
            }
            selected["chronology"] = yaml("[]");
        });
        let report = validation::validate_world(&malformed);
        assert!(!report.valid);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "WDL-CHRON-003" && d.entity_id.as_deref() == Some(EPISODE)),
            "{:?}",
            report.diagnostics
        );
        fs::write(
            world.root.join(&world.records[EPISODE].source_path),
            &world.records[EPISODE].raw_bytes,
        )
        .unwrap();
    }
    unchanged(&world);
}
