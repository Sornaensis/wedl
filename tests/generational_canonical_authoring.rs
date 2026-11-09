//! The six canonical history kinds share the existing one-record save boundary.
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use std::{fs, path::Path, process::Command};
use wedl::{
    Error, authoring, generational,
    model::{StoryTime, World},
    record_edit, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const JULES: &str = "char_00000000000000000000000003";
const ORG: &str = "organization_00000000000000000000000001";
const AFF: &str = "affiliation_00000000000000000000000001";
const LEGACY: &str = "legacy_00000000000000000000000001";
const TENURE: &str = "tenure_00000000000000000000000001";
const CLAIM: &str = "claim_00000000000000000000000001";
const VITAL: &str = "vital_00000000000000000000000001";
const CREATES: [&str; 6] = [
    include_str!("../examples/authoring/27-organization.yaml"),
    include_str!("../examples/authoring/28-affiliation.yaml"),
    include_str!("../examples/authoring/29-legacy.yaml"),
    include_str!("../examples/authoring/30-tenure.yaml"),
    include_str!("../examples/authoring/31-claim.yaml"),
    include_str!("../examples/authoring/32-vital.yaml"),
];
const UPDATES: [(&str, &str); 3] = [
    (
        ORG,
        include_str!("../examples/authoring/33-organization-rename.yaml"),
    ),
    (
        TENURE,
        include_str!("../examples/authoring/34-tenure-vacancy.yaml"),
    ),
    (
        AFF,
        include_str!("../examples/authoring/35-affiliation-end.yaml"),
    ),
];

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}
fn unchanged(world: &World) {
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}
fn save(world: &World, id: Option<&str>, request: &str) -> World {
    let preview = match id {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    match id {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    }
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert!(validation::validate_world(&saved).valid);
    saved
}
fn refused(world: &World, id: Option<&str>, request: &str) {
    let preview = match id {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], false, "accepted {request}: {preview}");
    let hash = preview["afterSha256"].as_str().unwrap();
    let result = match id {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    };
    assert!(matches!(result, Err(Error::Validation(_))), "{result:?}");
    unchanged(world);
}
fn fixture(capability: bool, histories: bool) -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world =
        source::init_world(&dir.path().join("world"), "Canonical history authoring").unwrap();
    if capability {
        let path = world.root.join("story/world.md");
        fs::write(
            &path,
            fs::read_to_string(&path)
                .unwrap()
                .replace("capabilities: []", "capabilities: [generational-core-v1]"),
        )
        .unwrap();
        world = source::load_world(&world.root, None).unwrap();
    }
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/21-rowan.yaml"),
        include_str!("../examples/authoring/22-jules.yaml"),
    ] {
        world = save(&world, None, request);
    }
    if histories {
        for request in CREATES {
            world = save(&world, None, request);
        }
    }
    (dir, world)
}
fn patch(kind: &str, key: &str, value: Yaml) -> String {
    let mut fields = serde_yaml::Mapping::new();
    fields.insert("kind".into(), kind.into());
    fields.insert(key.into(), value);
    serde_yaml::to_string(&fields).unwrap()
}

#[test]
fn six_kinds_roundtrip_and_claims_remain_separate_from_holders() {
    let (_dir, world) = fixture(true, true);
    let early = generational::organization(&world, ORG, &at(2, -1), 8, false, 100).unwrap();
    assert_eq!(early["roles"][0]["value"]["role"], Json::Null);
    let later = generational::organization(&world, ORG, &at(2, 0), 8, false, 100).unwrap();
    assert_eq!(later["roles"][0]["value"]["role"], "watch keeper");
    let legacy = generational::legacy(&world, LEGACY, &at(20, 0), 100).unwrap();
    assert_eq!(legacy["holders"][0]["value"]["holderId"], ANA);
    assert_eq!(legacy["claims"][0]["value"]["claimantId"], JULES);
    assert_eq!(legacy["claims"][0]["lifecycle"], "disputed");
    assert_eq!(legacy["holders"].as_array().unwrap().len(), 1);
    assert_eq!(
        generational::organization_legacies(&world, ORG, &at(0, 0), 100).unwrap()["count"],
        1
    );
    assert_eq!(
        generational::vital(&world, ANA, &at(-30, -1), 100).unwrap()["available"],
        false
    );
    assert_eq!(
        generational::vital(&world, ANA, &at(-30, 0), 100).unwrap()["vital"],
        "living"
    );
    for id in [ORG, AFF, LEGACY, TENURE, CLAIM, VITAL] {
        let record = &world.records[id];
        assert_eq!(record.status(), "canonical");
        assert_eq!(
            generational::history(&world, id, &at(20, 0), 100).unwrap()["available"],
            true
        );
    }
}

#[test]
fn numbered_updates_preserve_historical_names_memberships_and_inclusive_vacancy() {
    let (_dir, mut world) = fixture(true, true);
    for (id, request) in UPDATES {
        world = save(&world, Some(id), request);
    }
    assert_eq!(world.records[ORG].title, "Harbor Guild");
    assert_eq!(
        generational::organization(&world, ORG, &at(3, -1), 8, false, 100).unwrap()["organization"]
            ["value"]["title"],
        "Harbor Guild"
    );
    assert_eq!(
        generational::organization(&world, ORG, &at(3, 0), 8, false, 100).unwrap()["organization"]
            ["value"]["title"],
        "Harbor Watch"
    );
    for (point, state) in [
        (at(10, -1), "holding"),
        (at(10, 0), "vacant-interval"),
        (at(12, 0), "vacant-interval"),
        (at(12, 1), "expired"),
    ] {
        assert_eq!(
            generational::history(&world, TENURE, &point, 100).unwrap()["lifecycle"],
            state
        );
    }
    assert!(
        generational::legacy(&world, LEGACY, &at(20, 0), 100).unwrap()["holders"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let current = generational::organization(&world, ORG, &at(15, 0), 8, false, 100).unwrap();
    assert!(current["roles"].as_array().unwrap().is_empty());
    let former = generational::organization(&world, ORG, &at(15, 0), 8, true, 100).unwrap();
    assert_eq!(former["formerRoles"][0]["recordId"], AFF);
    assert_eq!(former["formerRoles"][0]["lifecycle"], "ended");
    assert_eq!(
        generational::organization(&world, ORG, &at(15, -1), 8, false, 100).unwrap()["roles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn nullable_payloads_are_data_but_unknown_shapes_and_top_level_nulls_are_not_edits() {
    let (_dir, mut world) = fixture(true, true);
    for (id, kind) in [
        (ORG, "organization"),
        (AFF, "affiliation"),
        (LEGACY, "legacy"),
        (TENURE, "tenure"),
        (CLAIM, "claim"),
        (VITAL, "vital-history"),
    ] {
        let original = world.records[id].clone();
        let path = world.root.join(&original.source_path);
        let raw = fs::read_to_string(&path).unwrap().replacen(
            "\n---\n",
            "\nx-note: {chronology: preserved}\n---\n",
            1,
        );
        fs::write(&path, raw).unwrap();
        world = source::load_world(&world.root, None).unwrap();
        let update = format!(
            "kind: {kind}\nchronology:\n  - id: chronology_00000000000000000000000001\n    role: source-duration\n    value: {{duration: {{value: 2, unit: year}}}}\n    provenance: [ledger]\n"
        );
        world = save(&world, Some(id), &update);
        let record = &world.records[id];
        assert_eq!(record.body, original.body);
        for key in ["initialization", "transitions"] {
            assert_eq!(record.get(key), original.get(key));
        }
        assert_eq!(
            record.get("x-note").unwrap()["chronology"].as_str(),
            Some("preserved")
        );
        let no_op =
            authoring::preview_record_update(&world, id, &update, &world.fingerprint).unwrap();
        assert_eq!(no_op["changed"], false);
        assert_eq!(
            no_op["afterSource"].as_str().unwrap().as_bytes(),
            record.raw_bytes
        );
        for field in ["initialization", "transitions", "status", "title"] {
            let bad = format!("kind: {kind}\n{field}: null\n");
            assert!(matches!(
                record_edit::update(&world, id, &bad),
                Err(Error::InvalidInput(_))
            ));
        }
        let tagged = format!("kind: {kind}\ninitialization: !tag {{}}\n");
        assert!(record_edit::update(&world, id, &tagged).is_err());
    }
    // A null reparent payload explicitly detaches; it is not a field deletion.
    let reparent = "kind: organization\ntransitions:\n- transition_id: transition_00000000000000000000000040\n  transition_kind: organization-reparent\n  applicability: {applicability_kind: instant, point: {timeline: main, tick: 4, order: 0}}\n  payload: {parent_id: null}\n";
    world = save(&world, Some(ORG), reparent);
    assert_eq!(
        generational::organization(&world, ORG, &at(4, 0), 8, false, 100).unwrap()["organization"]
            ["value"]["parentId"],
        Json::Null
    );
    refused(
        &world,
        Some(ORG),
        &reparent.replace("parent_id: null", "unexpected: null"),
    );
    assert!(record_edit::update(&world, ORG, "kind: organization\nparent_id: null\n").is_err());
}

#[test]
fn capabilities_cross_record_references_and_invalid_status_edits_cannot_publish() {
    let (_dir, world) = fixture(false, false);
    refused(&world, None, CREATES[0]);
    let (_dir, mut world) = fixture(true, true);
    for (id, request) in [
        (ORG, format!("kind: organization\nparent_id: {ORG}\n")),
        (
            AFF,
            "kind: affiliation\ncharacter_id: char_00000000000000000000000009\n".into(),
        ),
        (
            LEGACY,
            "kind: legacy\norganization_id: organization_00000000000000000000000009\n".into(),
        ),
        (
            TENURE,
            format!("kind: tenure\nsuccessor_tenure_id: {TENURE}\n"),
        ),
        (VITAL, "kind: vital-history\ndisclosure: unknown\n".into()),
        (ORG, "kind: organization\nstatus: retired\n".into()),
        (LEGACY, "kind: legacy\nstatus: retired\n".into()),
    ] {
        refused(&world, Some(id), &request);
    }
    let mut init = world.records[CLAIM].get("initialization").unwrap().clone();
    init["payload"]["competes_with"] = Yaml::Sequence(vec![CLAIM.into()]);
    refused(&world, Some(CLAIM), &patch("claim", "initialization", init));
    // An unreferenced noncanonical record remains valid but supplies no facts.
    world = save(&world, Some(TENURE), "kind: tenure\nstatus: retired\n");
    world = save(&world, Some(CLAIM), "kind: claim\nstatus: draft\n");
    let legacy = generational::legacy(&world, LEGACY, &at(20, 0), 100).unwrap();
    assert!(legacy["holders"].as_array().unwrap().is_empty());
    assert!(legacy["claims"].as_array().unwrap().is_empty());
    world = save(
        &world,
        Some(VITAL),
        "kind: vital-history\ndisclosure: withheld\n",
    );
    assert_eq!(
        generational::vital(&world, ANA, &at(20, 0), 100).unwrap()["available"],
        false
    );
    assert_eq!(
        generational::history(&world, VITAL, &at(20, 0), 100).unwrap()["reason"],
        "undisclosed"
    );
}

#[test]
fn reciprocal_claim_edits_refuse_half_a_pair_and_keep_the_current_source() {
    let (_dir, world) = fixture(true, true);
    let other = "claim_00000000000000000000000002";
    let mut create: Yaml = serde_yaml::from_str(CREATES[4]).unwrap();
    create["id"] = other.into();
    create["title"] = "Another claim".into();
    create["initialization"]["transition_id"] = "transition_00000000000000000000000042".into();
    create["transitions"] = Yaml::Sequence(vec![]);
    let world = save(&world, None, &serde_yaml::to_string(&create).unwrap());
    for (id, target) in [(CLAIM, other), (other, CLAIM)] {
        let mut init = world.records[id].get("initialization").unwrap().clone();
        init["payload"]["competes_with"] = Yaml::Sequence(vec![target.into()]);
        refused(&world, Some(id), &patch("claim", "initialization", init));
    }
    assert_eq!(
        generational::legacy(&world, LEGACY, &at(0, 0), 100).unwrap()["holders"][0]["value"]["holderId"],
        ANA
    );
}

fn cli(root: &Path, args: &[&str]) -> Json {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn all_nine_new_numbered_requests_preview_and_save_through_cli() {
    let (dir, mut world) = fixture(true, false);
    for (id, request) in CREATES
        .into_iter()
        .map(|request| (None, request))
        .chain(UPDATES.into_iter().map(|(id, request)| (Some(id), request)))
    {
        let path = dir.path().join("request.yaml");
        fs::write(&path, request).unwrap();
        let mut args = vec!["record", if id.is_some() { "update" } else { "create" }];
        if let Some(id) = id {
            args.push(id);
        }
        args.extend([
            "--file",
            path.to_str().unwrap(),
            "--expected-fingerprint",
            &world.fingerprint,
        ]);
        let preview = cli(&world.root, &args)["result"].clone();
        assert_eq!(preview["canApply"], true, "{preview}");
        unchanged(&world);
        args.extend([
            "--apply",
            "--expected-after-sha256",
            preview["afterSha256"].as_str().unwrap(),
        ]);
        cli(&world.root, &args);
        world = source::load_world(&world.root, None).unwrap();
        assert_eq!(world.fingerprint, preview["candidateFingerprint"]);
        assert_eq!(
            fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
    }
    assert_eq!(
        cli(
            &world.root,
            &[
                "generational",
                "organization",
                ORG,
                "--tick",
                "3",
                "--order",
                "0"
            ]
        )["result"]["organization"]["value"]["title"],
        "Harbor Watch"
    );
    assert_eq!(
        cli(
            &world.root,
            &[
                "generational",
                "history",
                TENURE,
                "--tick",
                "12",
                "--order",
                "1"
            ]
        )["result"]["lifecycle"],
        "expired"
    );
}
