//! Learned assertions use the existing reviewed writer and admitted evidence boundary.
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use std::{fs, path::Path, process::Command};
use wedl::{
    Error, authoring, character,
    generational::{self, discovery, knowledge},
    model::{StoryTime, World},
    record_edit, runtime, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const ROWAN: &str = "char_00000000000000000000000002";
const JULES: &str = "char_00000000000000000000000003";
const ORG: &str = "organization_00000000000000000000000001";
const OLD: &str = "know_00000000000000000000000002";
const NEW: &str = "know_00000000000000000000000009";
const CREATES: [&str; 7] = [
    include_str!("../examples/authoring/36-learned-parentage.yaml"),
    include_str!("../examples/authoring/37-learned-union.yaml"),
    include_str!("../examples/authoring/38-learned-organization.yaml"),
    include_str!("../examples/authoring/39-learned-affiliation.yaml"),
    include_str!("../examples/authoring/40-learned-tenure.yaml"),
    include_str!("../examples/authoring/41-learned-claim.yaml"),
    include_str!("../examples/authoring/42-learned-vital.yaml"),
];
const REJECT: &str = include_str!("../examples/authoring/43-reject-parentage.yaml");
const REPLACE: &str = include_str!("../examples/authoring/44-replacement-parentage.yaml");

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
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
    let report = validation::validate_world(&saved);
    assert!(report.valid, "{report:?}");
    saved
}
fn refused(world: &World, request: &str) {
    let preview = authoring::preview_record_create(world, request, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false, "{preview}");
    assert!(matches!(
        authoring::apply_record_create(
            world,
            request,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Validation(_))
    ));
    unchanged(world);
}
fn fixture(learned_capability: bool) -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world =
        source::init_world(&dir.path().join("world"), "Learned generational authoring").unwrap();
    let path = world.root.join("story/world.md");
    let caps = if learned_capability {
        "capabilities: [generational-core-v1, generational-knowledge-v1]"
    } else {
        "capabilities: [generational-core-v1]"
    };
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("capabilities: []", caps),
    )
    .unwrap();
    world = source::load_world(&world.root, None).unwrap();
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/21-rowan.yaml"),
        include_str!("../examples/authoring/22-jules.yaml"),
        include_str!("../examples/authoring/23-parentage.yaml"),
        include_str!("../examples/authoring/27-organization.yaml"),
        include_str!("../examples/authoring/29-legacy.yaml"),
    ] {
        world = save(&world, None, request);
    }
    (dir, world)
}
fn assertion(world: &World, tick: i64, order: i32, history: bool) -> Json {
    knowledge::assertions(
        world,
        ANA,
        &at(tick, order),
        Some("parentage"),
        None,
        history,
        100,
    )
    .unwrap()
}

#[test]
fn all_seven_kinds_publish_without_changing_canonical_relationships() {
    let (_dir, mut world) = fixture(true);
    let source_before = world.records["kinship_00000000000000000000000001"]
        .raw_bytes
        .clone();
    for request in CREATES {
        world = save(&world, None, request);
    }
    assert_eq!(
        world.records["kinship_00000000000000000000000001"].raw_bytes,
        source_before
    );
    assert_eq!(
        knowledge::assertions(&world, ANA, &at(4, 0), None, None, false, 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        knowledge::family(&world, ANA, ANA, &at(4, 0), 100).unwrap()["parents"][0]["value"]["parent_id"],
        JULES
    );
    let canonical = generational::family(&world, ANA, &at(4, 0), 100).unwrap();
    assert!(canonical.to_string().contains(ROWAN));
    assert!(!canonical.to_string().contains(JULES));
    let author = runtime::knowledge(&world, ANA, &at(4, 0), false).unwrap();
    let typed = author["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["knowledgeId"] == OLD)
        .unwrap();
    assert_eq!(typed["claim"]["genealogy"]["valid"]["from"]["tick"], "-40");
    assert_eq!(typed["claim"]["genealogy"]["valid"]["until"]["order"], "0");
}

#[test]
fn expiry_rejection_and_replacement_preserve_earlier_belief_history() {
    let (_dir, world) = fixture(true);
    let world = save(&world, None, CREATES[0]);
    assert!(
        assertion(&world, 4, -1, false)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        assertion(&world, 8, 0, false)["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        assertion(&world, 8, 1, false)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        assertion(&world, 8, 1, true)["items"][0]["applicable"],
        false
    );
    assert_eq!(
        discovery::resolve_learned(&world, ANA, "Jules the Elder", &at(8, 1), Some("character"))
            .unwrap(),
        JULES
    );
    let world = save(&world, Some(OLD), REJECT);
    assert!(
        assertion(&world, 9, 0, true)["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        discovery::resolve_learned(&world, ANA, "Jules the Elder", &at(9, 0), Some("character"))
            .is_err()
    );
    let world = save(&world, None, REPLACE);
    assert_eq!(
        assertion(&world, 9, 1, false)["items"][0]["knowledgeId"],
        NEW
    );
    assert_eq!(
        assertion(&world, 8, 0, false)["items"][0]["knowledgeId"],
        OLD
    );
    assert_eq!(
        world.records[OLD].get("claim").unwrap()["genealogy"]["payload"]["parent_id"].as_str(),
        Some(JULES)
    );
}

#[test]
fn generic_character_context_and_search_admit_typed_literals_only() {
    let (_dir, world) = fixture(true);
    let mut request: Yaml = serde_yaml::from_str(CREATES[0]).unwrap();
    request["claim"]["key"] = "SECRET_AUTHOR_KEY".into();
    request["claim"]["statement"] = "SECRET_AUTHOR_STATEMENT".into();
    request["title"] = "SECRET_AUTHOR_TITLE".into();
    request["body"] = "SECRET_AUTHOR_BODY".into();
    let world = save(&world, None, &serde_yaml::to_string(&request).unwrap());
    let packet = character::context(&world, ANA, &at(4, 0), 100_000).unwrap();
    assert!(
        packet["beliefs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["knowledgeId"] == OLD)
    );
    assert!(!packet.to_string().contains("SECRET_AUTHOR"));
    assert_eq!(
        character::search(&world, ANA, "Jules", &at(4, 0), 20).unwrap()["total"],
        1
    );
    assert_eq!(
        character::search(&world, ANA, "SECRET_AUTHOR_STATEMENT", &at(4, 0), 20).unwrap()["total"],
        0
    );
    assert_eq!(
        character::search(&world, ANA, "Jules", &at(8, 1), 20).unwrap()["total"],
        0
    );
    let mut hidden = world.clone();
    hidden.records.get_mut(OLD).unwrap().frontmatter["claim"]["statement"] =
        "REPLACED_AUTHOR_STATEMENT".into();
    hidden.records.get_mut(JULES).unwrap().title = "REPLACED_CANONICAL_NAME".into();
    hidden.fingerprint = "HIDDEN_FINGERPRINT".into();
    hidden.revision = Some("HIDDEN_REVISION".into());
    // Refresh a valid snapshot rather than relying on inconsistent raw bytes.
    let records = hidden
        .records
        .values()
        .map(|record| {
            let mut fields = record.frontmatter.clone();
            fields["title"] = record.title.clone().into();
            let bytes = format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&fields).unwrap(),
                record.body
            );
            let mut parsed = source::parse_record(bytes.as_bytes(), &record.source_path).unwrap();
            parsed.blob_oid = Some("HIDDEN_BLOB".into());
            parsed
        })
        .collect();
    let hidden = source::world_from_records(hidden.root, records, hidden.revision).unwrap();
    assert!(validation::validate_world(&hidden).valid);
    assert_eq!(
        packet,
        character::context(&hidden, ANA, &at(4, 0), 100_000).unwrap()
    );
    assert_eq!(
        character::search(&world, ANA, "Jules", &at(4, 0), 20).unwrap(),
        character::search(&hidden, ANA, "Jules", &at(4, 0), 20).unwrap()
    );
    assert_eq!(
        character::context(&world, ANA, &at(4, 0), 1)
            .unwrap_err()
            .to_string(),
        character::context(&hidden, ANA, &at(4, 0), 1)
            .unwrap_err()
            .to_string()
    );
}

#[test]
fn invalid_claim_semantics_are_reviewable_and_never_published() {
    let (_dir, world) = fixture(true);
    let base: Yaml = serde_yaml::from_str(CREATES[0]).unwrap();
    let mut variants = Vec::new();
    let mut value = base.clone();
    value["claim"]["genealogy"]["valid"]["until"] = Yaml::Null;
    variants.push(value);
    let mut value = base.clone();
    value["claim"]["genealogy"]["payload"]["parent_id"] = ORG.into();
    variants.push(value);
    let mut value = base.clone();
    value["claim"]["genealogy"]["labels"][ORG] = "Unasserted name".into();
    variants.push(value);
    let mut value = base.clone();
    value["claim"]["genealogy"]["payload"]["chronology"] = Yaml::Sequence(vec![]);
    variants.push(value);
    let mut value = base.clone();
    value["transitions"] = Yaml::Sequence(vec![]);
    variants.push(value);
    let mut value = base.clone();
    value["transitions"][0]["state"] = "rejected".into();
    variants.push(value);
    for value in variants {
        refused(&world, &serde_yaml::to_string(&value).unwrap());
    }
    let (_dir, without_capability) = fixture(false);
    refused(&without_capability, CREATES[0]);
    unchanged(&world);
}

#[test]
fn typed_claims_keep_closed_shapes_and_nullable_domain_values() {
    let (_dir, mut world) = fixture(true);
    for request in [CREATES[2], CREATES[3], CREATES[4]] {
        world = save(&world, None, request);
    }
    for request in [
        "kind: knowledge\nclaim: null\n",
        "kind: knowledge\nclaim: {key: x, statement: x, genealogy: {}, truth: unknown}\n",
    ] {
        assert!(record_edit::update(&world, "know_00000000000000000000000004", request).is_err());
    }
    let plain = "kind: knowledge\nknower: char_00000000000000000000000001\nclaim: {key: plain, statement: plain, object: null}\n";
    assert!(record_edit::create(&world, plain).is_err());
    assert!(
        world
            .records
            .values()
            .filter(|record| record.kind == "knowledge"
                && record.id != "know_00000000000000000000000001")
            .all(|record| record.get("claim").unwrap().get("genealogy").is_some())
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
fn all_nine_learned_requests_preview_and_save_through_cli() {
    let (dir, mut world) = fixture(true);
    for (id, request) in CREATES
        .into_iter()
        .map(|request| (None, request))
        .chain([(Some(OLD), REJECT), (None, REPLACE)])
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
        assertion(&world, 9, 1, false)["items"][0]["knowledgeId"],
        NEW
    );
    assert!(validation::validate_world(&world).valid);
}
