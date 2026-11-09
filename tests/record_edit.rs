use sha2::{Digest, Sha256};
use std::path::PathBuf;
use wedl::{
    Error,
    model::{Record, World},
    record_edit,
    source::{self, SOURCE_SCHEMA},
    validation,
};

const CHARACTER: &str = "char_00000000000000000000000001";
const LOCATION: &str = "loc_00000000000000000000000002";
const OBJECT: &str = "obj_00000000000000000000000003";
const EVENT: &str = "event_00000000000000000000000004";

fn record(kind: &str, id: &str, title: &str, fields: &str, path: &str) -> Record {
    let text = format!(
        "---\nschema: {SOURCE_SCHEMA}\nkind: {kind}\nid: {id}\ntitle: {title}\nstatus: canonical\n{fields}---\n"
    );
    source::parse_record(text.as_bytes(), path).unwrap()
}

fn world() -> World {
    let world = record(
        "world",
        "world_00000000000000000000000000",
        "Draft tests",
        "capabilities: []\nthreads: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys:\n  character:\n    condition: {type: string}\n    location: {type: entity, kind: location}\n  object:\n    condition: {type: string}\n    holder: {type: entity, kind: character}\n  location:\n    label: {type: string}\n",
        "story/world.md",
    );
    let header = format!("---\n# Keep this authored header comment on no-op and body-only edits.\nschema: {SOURCE_SCHEMA}\nkind: character\nid: {CHARACTER}\ntitle: Ana\nstatus: canonical\naliases: [Annie]\ntags: [first, second]\nrole: Observer\npronouns: {{subject: she, object: her, possessive: her}}\ninitial_state:\n  condition: well\n  location: {{entity: {LOCATION}}}\nx-opaque: {{1: [true, 1, 1.0, null, '1'], note: null}}\n---\n").replace('\n', "\r\n");
    let mut character = source::parse_record(
        format!("{header}\r\n\n  Original — α  \r\n\t").as_bytes(),
        "story/characters/original-name.md",
    )
    .unwrap();
    character.blob_oid = Some("a".repeat(40));
    let location = record("location", LOCATION, "Pier", "", "story/locations/pier.md");
    let object = record(
        "object",
        OBJECT,
        "Letter",
        "initial_state: {condition: sealed}\n",
        "story/objects/letter.md",
    );
    let event = record(
        "event",
        EVENT,
        "Seed",
        "time: {tick: 0}\nparticipants: []\ncauses: []\neffects: []\n",
        "story/events/seed.md",
    );
    source::world_from_records(
        PathBuf::from("."),
        vec![world, character, location, object, event],
        None,
    )
    .unwrap()
}

fn candidate(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}

#[test]
fn all_five_creates_are_native_typed_records_with_deterministic_paths() {
    let world = world();
    let drafts = [
        ("character", "char_00000000000000000000000011", "characters", "role: Visitor\npronouns: {subject: they, object: them}\ninitial_state: {condition: rested}\n".into()),
        ("location", "loc_00000000000000000000000012", "locations", "initial_state: {label: North quay}\n".into()),
        ("object", "obj_00000000000000000000000013", "objects", format!("initial_state: {{condition: new, holder: {{entity: {CHARACTER}}}}}\n")),
        ("event", "event_00000000000000000000000014", "events", format!("time: {{tick: 1, order: 0}}\nlocation: {LOCATION}\nparticipants: [{{character: {CHARACTER}, role: observer}}]\ncauses: [{EVENT}]\neffects: [{{id: effect_00000000000000000000000014, target: {OBJECT}, key: condition, operation: set, value: open}}]\n")),
        ("knowledge", "know_00000000000000000000000015", "knowledge", format!("knower: {CHARACTER}\nclaim: {{key: letter.present, statement: The letter is present, subject: {CHARACTER}, predicate: sees, object: {{entity: {OBJECT}}}, truth: 'true'}}\ntransitions: [{{id: learned-letter, time: {{tick: 1}}, state: accepted, acquisition: observed, confidence: 0.9, causing_event: {EVENT}, source_entity: {OBJECT}, note: ''}}]\n")),
    ];
    for (kind, id, directory, fields) in drafts {
        let draft = format!("kind: {kind}\nid: {id}\ntitle: New {kind}\n{fields}");
        let record = record_edit::create(&world, &draft).unwrap();
        assert_eq!(record.string("schema"), Some(SOURCE_SCHEMA));
        assert_eq!(record.status(), "canonical");
        assert_eq!(record.source_path, format!("story/{directory}/{id}.md"));
        assert_eq!(record.body, "");
        assert_eq!(record.list("aliases").unwrap().len(), 0);
        assert_eq!(record.list("tags").unwrap().len(), 0);
        let report = validation::validate_world(&candidate(&world, record));
        assert!(report.valid, "{kind}: {:?}", report.diagnostics);
    }
}

#[test]
fn minimal_creates_have_explicit_empty_collections_without_invented_content() {
    let world = world();
    let event = record_edit::create(
        &world,
        "kind: event\nid: event_00000000000000000000000012\ntitle: Bell\ntime: {tick: 2}\n",
    )
    .unwrap();
    for field in ["participants", "causes", "effects"] {
        assert!(event.list(field).unwrap().is_empty());
    }
    let knowledge = record_edit::create(&world, &format!("kind: knowledge\nid: know_00000000000000000000000013\ntitle: A claim\nknower: {CHARACTER}\nclaim: {{key: note, statement: An authored statement}}\n")).unwrap();
    assert!(knowledge.list("transitions").unwrap().is_empty());
    assert!(knowledge.get("claim").unwrap().get("truth").is_none());
    assert!(knowledge.body.is_empty());
}

#[test]
fn namespaced_location_ids_cannot_control_the_source_path() {
    let world = world();
    let id = "location:town/north-quay";
    let record = record_edit::create(
        &world,
        &format!("kind: location\nid: {id}\ntitle: North quay\n"),
    )
    .unwrap();
    assert_eq!(record.id, id);
    assert_eq!(
        record.source_path,
        format!(
            "story/locations/location-{}.md",
            hex::encode(Sha256::digest(id.as_bytes()))
        )
    );
    for bad in [
        "location:../outside",
        "location:town/../../outside",
        "location:con",
        "location:Town",
        "location:town\\outside",
    ] {
        assert!(
            record_edit::create(
                &world,
                &format!("kind: location\nid: '{bad}'\ntitle: Bad\n")
            )
            .is_err()
        );
    }
    assert!(record_edit::create(&world, "kind: character\nid: char_00000000000000000000000012\ntitle: Safe\npath: /tmp/outside.md\n").is_err());
}

#[test]
fn no_op_updates_preserve_every_original_byte_and_provenance() {
    let world = world();
    let before = world.resolve(CHARACTER).unwrap();
    for patch in [
        "kind: character\n".into(),
        "kind: character\ntitle: Ana\naliases: [Annie]\ntags: [first, second]\n".into(),
        format!(
            "kind: character\ninitial_state: {{location: {{entity: {LOCATION}}}, condition: well}}\n"
        ),
        format!(
            "kind: character\nbody: {}\n",
            serde_json::to_string(&before.body).unwrap()
        ),
    ] {
        let result = record_edit::update(&world, "Ana", &patch).unwrap();
        assert_eq!(result.raw_bytes, before.raw_bytes);
        assert_eq!(result.body, before.body);
        assert_eq!(result.blob_oid, before.blob_oid);
        assert_eq!(result.source_path, before.source_path);
    }
}

#[test]
fn body_only_update_preserves_header_exactly_including_crlf_comments() {
    let world = world();
    let before = world.resolve(CHARACTER).unwrap();
    let prefix = &before.raw_bytes[..before.raw_bytes.len() - before.body.len()];
    for body in ["", "\nReplacement — β\r\n \t"] {
        let patch = format!(
            "kind: character\nbody: {}\n",
            serde_json::to_string(body).unwrap()
        );
        let result = record_edit::update(&world, CHARACTER, &patch).unwrap();
        assert_eq!(result.body, body);
        assert_eq!(&result.raw_bytes[..prefix.len()], prefix);
        assert_eq!(&result.raw_bytes[prefix.len()..], body.as_bytes());
        assert!(result.blob_oid.is_none());
    }
}

#[test]
fn structured_updates_preserve_omitted_body_and_unknown_existing_metadata() {
    let world = world();
    let before = world.resolve(CHARACTER).unwrap();
    let result =
        record_edit::update(&world, CHARACTER, "kind: character\ntitle: Ana Revised\n").unwrap();
    assert_eq!(result.title, "Ana Revised");
    assert_eq!(result.id, before.id);
    assert_eq!(result.source_path, before.source_path);
    assert_eq!(result.body.as_bytes(), before.body.as_bytes());
    assert_eq!(result.get("x-opaque"), before.get("x-opaque"));
    assert_eq!(result.get("pronouns"), before.get("pronouns"));
    assert_eq!(result.get("role"), before.get("role"));
    assert!(result.raw_bytes.starts_with(b"---\r\n"));
    assert!(result.raw_bytes.ends_with(before.body.as_bytes()));
    let report = validation::validate_world(&candidate(&world, result));
    assert!(report.valid, "{:?}", report.diagnostics);
}

#[test]
fn supplied_arrays_and_maps_replace_instead_of_merging() {
    let world = world();
    let result = record_edit::update(&world, CHARACTER,
        "kind: character\naliases: []\ntags: []\ninitial_state: {}\nrole: ''\npronouns: {subject: they, object: them}\n").unwrap();
    assert!(result.list("aliases").unwrap().is_empty());
    assert!(result.list("tags").unwrap().is_empty());
    assert!(
        result
            .get("initial_state")
            .unwrap()
            .as_mapping()
            .unwrap()
            .is_empty()
    );
    assert_eq!(result.string("role"), Some(""));
    assert!(result.get("pronouns").unwrap().get("possessive").is_none());
    assert_eq!(
        result.get("x-opaque"),
        world.resolve(CHARACTER).unwrap().get("x-opaque")
    );
}

#[test]
fn edits_refuse_identity_changes_unknown_fields_nulls_and_wrong_types() {
    let world = world();
    for patch in [
        "title: Missing kind",
        "kind: object",
        "kind: character\nid: char_00000000000000000000000001",
        "kind: character\nschema: forbidden",
        "kind: character\nsource_path: story/other.md",
        "kind: character\nstatus: draft",
        "kind: character\ntitlle: Typo",
        "kind: character\ntitle: null",
        "kind: character\naliases: [null]",
        "kind: character\ninitial_state: {condition: null}",
        "kind: character\npronouns: {subject: they, object: them, possessive: null}",
        "kind: character\npronouns: {subject: they, objcet: them}",
        "kind: character\npronouns: {}",
        "kind: character\ntitle: 123",
        "kind: character\naliases: [same, same]",
        "kind: character\ntags: [true]",
        "kind: character\nbody: !custom text",
        "kind: character\nkind: character",
    ] {
        assert!(
            record_edit::update(&world, CHARACTER, patch).is_err(),
            "accepted {patch}"
        );
    }
    for draft in [
        "kind: character",
        "kind: character\nid: char_00000000000000000000000012",
        "kind: world\nid: world_00000000000000000000000012\ntitle: Nope",
        "kind: event\nid: event_00000000000000000000000012\ntitle: Missing time",
    ] {
        assert!(
            record_edit::create(&world, draft).is_err(),
            "accepted {draft}"
        );
    }
}

#[test]
fn request_size_is_bounded_and_duplicate_create_ids_fail() {
    let world = world();
    let oversized = " ".repeat(1024 * 1024 + 1);
    assert!(record_edit::create(&world, &oversized).is_err());
    assert!(record_edit::update(&world, CHARACTER, &oversized).is_err());
    assert!(matches!(
        record_edit::create(
            &world,
            &format!("kind: character\nid: {CHARACTER}\ntitle: Collision\n")
        ),
        Err(Error::Conflict(_))
    ));
    let body = "é".repeat(1000);
    let result = record_edit::update(
        &world,
        CHARACTER,
        &format!(
            "kind: character\nbody: {}",
            serde_json::to_string(&body).unwrap()
        ),
    )
    .unwrap();
    assert_eq!(result.body.as_bytes(), body.as_bytes());
}

#[test]
fn event_shapes_require_exact_times_and_ids_and_closed_effects() {
    let world = world();
    let base = "kind: event\nid: event_00000000000000000000000012\ntitle: Event\n";
    for fields in [
        "time: {tick: true}".into(),
        "time: {tick: 1.0}".into(),
        "time: {tick: '1'}".into(),
        "time: {tick: 9223372036854775808}".into(),
        "time: {tick: 0, order: 2147483648}".into(),
        "time: {tick: 0, seconds: 1}".into(),
        "time: {tick: 0}\nlocation: Pier".into(),
        format!("time: {{tick: 0}}\nparticipants: [{{character: {OBJECT}}}]"),
        format!("time: {{tick: 0}}\nparticipants: [{{character: {CHARACTER}, roel: actor}}]"),
        format!("time: {{tick: 0}}\ncauses: [{CHARACTER}]"),
        format!(
            "time: {{tick: 0}}\neffects: [{{id: arbitrary, target: {OBJECT}, key: condition, operation: clear}}]"
        ),
        format!(
            "time: {{tick: 0}}\neffects: [{{id: effect_00000000000000000000000012, target: {OBJECT}, key: condition, operation: clear, value: ignored}}]"
        ),
        format!(
            "time: {{tick: 0}}\neffects: [{{id: effect_00000000000000000000000012, target: Letter, key: condition, operation: set, value: opened}}]"
        ),
        format!(
            "time: {{tick: 0}}\neffects: [{{id: effect_00000000000000000000000012, target: {OBJECT}, key: condition, operation: add-to-set, value: [wrong]}}]"
        ),
    ] {
        assert!(
            record_edit::create(&world, &format!("{base}{fields}\n")).is_err(),
            "accepted {fields}"
        );
    }
    let record = record_edit::create(
        &world,
        &format!("{base}time: {{tick: -9223372036854775808, order: -2147483648}}\n"),
    )
    .unwrap();
    assert_eq!(record.get("time").unwrap()["tick"].as_i64(), Some(i64::MIN));
}

#[test]
fn knowledge_shapes_are_closed_and_confidence_and_enums_are_checked() {
    let world = world();
    let base = format!(
        "kind: knowledge\nid: know_00000000000000000000000012\ntitle: Knowledge\nknower: {CHARACTER}\n"
    );
    for claim in [
        "{}",
        "{key: note}",
        "{key: note, statement: Text, truth: true}",
        "{key: note, statement: Text, truth: maybe}",
        "{key: note, statement: Text, author_truth_status: unknown}",
        "{key: note, statement: Text, object: literal}",
        "{key: note, statement: Text, object: {text: ''}}",
        "{key: note, statement: Text, subject: Ana}",
    ] {
        assert!(
            record_edit::create(&world, &format!("{base}claim: {claim}\n")).is_err(),
            "accepted {claim}"
        );
    }
    for fields in [
        "id: learned, time: {tick: 1}, state: accepted, acquisition: observed", // confidence missing
        "id: learned, time: {tick: 1}, state: certain, acquisition: observed, confidence: 1",
        "id: learned, time: {tick: 1}, state: accepted, acquisition: magic, confidence: 1",
        "id: learned, time: {tick: 1}, state: accepted, acquisition: observed, confidence: true",
        "id: learned, time: {tick: 1}, state: accepted, acquisition: observed, confidence: .nan",
        "id: learned, time: {tick: 1}, state: accepted, acquisition: observed, confidence: 1.1",
        "id: learned, time: {tick: 1}, state: accepted, acquisition: observed, confidence: 0, causing_event: null",
        "id: learned, time: {tick: 1}, state: accepted, acquisition: observed, confidence: 1, source_entity: Ana",
    ] {
        let draft =
            format!("{base}claim: {{key: note, statement: Text}}\ntransitions: [{{{fields}}}]\n");
        assert!(
            record_edit::create(&world, &draft).is_err(),
            "accepted {fields}"
        );
    }
}

#[test]
fn initial_state_and_effect_values_have_only_supported_literal_shapes() {
    let world = world();
    for state in [
        "[]",
        "{condition: .nan}",
        "{condition: {arbitrary: map}}",
        "{location: {entity: Pier}}",
        "{condition: [duplicate, duplicate]}",
        "{condition: [text, {entity: loc_00000000000000000000000002}]}",
    ] {
        assert!(
            record_edit::update(
                &world,
                CHARACTER,
                &format!("kind: character\ninitial_state: {state}\n")
            )
            .is_err(),
            "accepted {state}"
        );
    }
    // The builder does not pretend that shape checks replace candidate semantics.
    let record = record_edit::create(&world, "kind: object\nid: obj_00000000000000000000000012\ntitle: Missing holder\ninitial_state: {holder: {entity: char_00000000000000000000000099}}\n").unwrap();
    assert!(!validation::validate_world(&candidate(&world, record)).valid);
}

#[test]
fn an_inconsistent_public_record_cannot_supply_a_body_boundary() {
    let mut world = world();
    world.records.get_mut(CHARACTER).unwrap().body = "fabricated".into();
    assert!(
        record_edit::update(&world, CHARACTER, "kind: character\nbody: replacement\n").is_err()
    );
}
