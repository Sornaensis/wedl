use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use wedl::{
    Error, authoring,
    consequences::{self, Operation, Plan},
    model::{Record, World},
    record_edit, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ACTOR: &str = "char_00000000000000000000000001";
const NEW: &str = "char_00000000000000000000000002";
const OTHER: &str = "char_00000000000000000000000003";
const EVENT: &str = "event_00000000000000000000000001";
const TICK: i64 = 9_007_199_254_740_993;

fn record(kind: &str, id: &str, fields: &str, body: &str, path: &str) -> Record {
    source::parse_record(
        format!(
            "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Fixture {kind}\nstatus: canonical\n{fields}---\n{body}"
        )
        .as_bytes(),
        path,
    )
    .unwrap()
}

fn world() -> World {
    let world = record(
        "world",
        WORLD,
        "capabilities: [lore-core-v1]\nthreads: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nprofiles: []\nstate_keys:\n  character:\n    partner: {type: entity, kind: character}\n    condition: {type: string}\n    measure: {type: number}\n",
        "Unrelated authored world prose.\n",
        "story/world.md",
    );
    let header = format!(
        "---\n# Retain this exact header and its line endings.\nschema: wedl/v1\nkind: character\nid: {ACTOR}\ntitle: Ana\nstatus: canonical\naliases: [Annie]\ninitial_state: {{condition: well}}\nx-private: original\n---\n"
    )
    .replace('\n', "\r\n");
    let actor = source::parse_record(
        format!("{header}Original 🌲 prose  \r\nwithout final newline").as_bytes(),
        "story/characters/retained.md",
    )
    .unwrap();
    let world = source::world_from_records(
        PathBuf::from("/consequence-preview-does-not-read-this-directory"),
        vec![world, actor],
        None,
    )
    .unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn create(id: &str, extra: &str) -> Operation {
    Operation::Create {
        draft: format!("kind: character\nid: {id}\ntitle: Proposed actor\n{extra}"),
    }
}

fn update(id: &str, patch: &str) -> Operation {
    Operation::Update {
        entity_id: id.into(),
        patch: patch.into(),
    }
}

fn plan(world: &World, operations: Vec<Operation>) -> Plan {
    Plan {
        expected_fingerprint: world.fingerprint.clone(),
        operations,
        analysis: None,
    }
}

fn preview(world: &World, plan: &Plan) -> Json {
    consequences::preview(world, plan, consequences::MAX_CHARACTERS).unwrap()
}

fn candidate(world: &World, report: &Json) -> World {
    let mut records = world.records.clone();
    for file in report["files"].as_array().unwrap() {
        let record = source::parse_record(
            file["afterSource"].as_str().unwrap().as_bytes(),
            file["sourcePath"].as_str().unwrap(),
        )
        .unwrap();
        records.insert(record.id.clone(), record);
    }
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}

fn with_records(world: &World, replacements: Vec<Record>) -> World {
    let mut records = world.records.clone();
    for record in replacements {
        records.insert(record.id.clone(), record);
    }
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}

fn hash(domain: &[u8], bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(bytes);
    hex::encode(hash.finalize())
}

#[test]
fn closed_plan_codec_preserves_literal_yaml_and_rejects_duplicate_keys() {
    let world = world();
    let yaml = format!(
        "kind: character\r\nid: {NEW}\r\ntitle: 'A literal name'\r\nbody: |\r\n  Unchanged temp:new references and 🌲.  \r\n"
    );
    let value = json!({"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.create","draft":yaml}]});
    let decoded = consequences::decode(&value.to_string()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
    assert_eq!(
        consequences::decode(&serde_json::to_string_pretty(&value).unwrap()).unwrap(),
        decoded
    );
    for text in [
        format!(
            "{{\"expectedFingerprint\":\"{}\",\"expectedFingerprint\":\"{}\",\"operations\":[]}}",
            world.fingerprint, world.fingerprint
        ),
        format!(
            "{{\"expectedFingerprint\":\"{}\",\"operations\":[{{\"type\":\"record.create\",\"draft\":\"a\",\"dr\\u0061ft\":\"b\"}}]}}",
            world.fingerprint
        ),
    ] {
        assert!(consequences::decode(&text).is_err(), "{text}");
    }
    let mut top = value.clone();
    top["unknown"] = true.into();
    let mut member = value.clone();
    member["operations"][0]["unknown"] = true.into();
    let mut missing = value.clone();
    missing["operations"][0]
        .as_object_mut()
        .unwrap()
        .remove("type");
    let mut extra = value.clone();
    extra["operations"][0]["entityId"] = ACTOR.into();
    for invalid in [top, member, missing, extra] {
        assert!(consequences::decode(&invalid.to_string()).is_err());
    }
    for invalid in [
        json!({"expectedFingerprint":world.fingerprint,"operations":[],"summary":"placeholder"}),
        json!({"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.delete","entityId":ACTOR}]}),
        json!({"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.create","draft":null}]}),
        json!({"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.update","entityId":"Ana","patch":"kind: character"}]}),
    ] {
        assert!(
            consequences::decode(&invalid.to_string()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn historical_validation_has_no_current_reference_or_source_grammar_dependency() {
    let world = world();
    let plan = plan(
        &world,
        vec![update(OTHER, "kind: character\nbody: old alternative\n")],
    );
    consequences::validate_plan(&plan).unwrap();
    consequences::decode(&serde_json::to_string(&plan).unwrap()).unwrap();
    assert!(matches!(
        consequences::preview(&world, &plan, consequences::MAX_CHARACTERS),
        Err(Error::NotFound(_))
    ));
    let malformed = Plan {
        operations: vec![update(ACTOR, "kind: character\nbody: [unclosed")],
        ..plan
    };
    consequences::validate_plan(&malformed).unwrap();
    let error =
        consequences::preview(&world, &malformed, consequences::MAX_CHARACTERS).unwrap_err();
    assert!(error.to_string().contains("operation 0"));
    assert!(error.to_string().contains("captured base"));
}

#[test]
fn complete_plan_bounds_apply_to_native_values_and_encoded_escapes() {
    let world = world();
    let mut plan = plan(&world, vec![]);
    assert!(consequences::validate_plan(&plan).is_err());
    plan.operations = (0..64).map(|_| create(NEW, "")).collect();
    consequences::validate_plan(&plan).unwrap();
    plan.operations.push(create(NEW, ""));
    assert!(consequences::validate_plan(&plan).is_err());
    plan.operations = vec![create(NEW, &"\0".repeat(50_000))];
    assert!(matches!(
        consequences::validate_plan(&plan),
        Err(Error::InvalidInput(message)) if message.contains("encoded bytes")
    ));
    plan.operations = vec![create(NEW, &"x".repeat(1_048_577))];
    assert!(consequences::validate_plan(&plan).is_err());
    plan.operations = vec![create(NEW, "")];
    plan.expected_fingerprint = "A".repeat(64);
    assert!(consequences::validate_plan(&plan).is_err());
    let nested = format!("{}0{}", "[".repeat(40), "]".repeat(40));
    assert!(consequences::decode(&nested).is_err());
}

#[test]
fn single_record_candidates_are_identical_to_existing_authoring_codec_previews() {
    let world = world();
    let create = create(NEW, "body: |\n  Authored literal prose.\n");
    let Operation::Create { draft } = &create else {
        unreachable!()
    };
    let old = authoring::preview_record_create(&world, draft, &world.fingerprint).unwrap();
    let new = preview(&world, &plan(&world, vec![create]));
    for field in [
        "afterSource",
        "afterSha256",
        "sourcePath",
        "beforeSource",
        "beforeSha256",
    ] {
        assert_eq!(new["files"][0][field], old[field]);
    }
    assert_eq!(new["candidateFingerprint"], old["candidateFingerprint"]);
    assert_eq!(new["validation"], old["validation"]);
    assert_eq!(new["canApply"], false);
    let patch = "kind: character\ntitle: Ana Reed\n";
    let old = authoring::preview_record_update(&world, ACTOR, patch, &world.fingerprint).unwrap();
    let new = preview(&world, &plan(&world, vec![update(ACTOR, patch)]));
    assert_eq!(new["files"][0]["afterSource"], old["afterSource"]);
    assert_eq!(new["candidateFingerprint"], old["candidateFingerprint"]);
    assert_eq!(
        new["files"][0]["beforeSource"].as_str().unwrap().as_bytes(),
        world.records[ACTOR].raw_bytes
    );
}

#[test]
fn mutual_creates_are_validated_together_and_keep_order_only_in_plan_identity() {
    let world = world();
    let operations = vec![
        create(
            NEW,
            &format!("initial_state: {{partner: {{entity: {OTHER}}}}}\n"),
        ),
        create(
            OTHER,
            &format!("initial_state: {{partner: {{entity: {NEW}}}}}\n"),
        ),
    ];
    let first = preview(&world, &plan(&world, operations.clone()));
    let reverse = preview(
        &world,
        &plan(&world, operations.into_iter().rev().collect()),
    );
    assert_eq!(first["validation"]["valid"], true, "{first}");
    assert_eq!(
        first["candidateFingerprint"],
        reverse["candidateFingerprint"]
    );
    assert_ne!(first["planSha256"], reverse["planSha256"]);
    assert_ne!(first["previewSha256"], reverse["previewSha256"]);
    assert_eq!(first["operations"][0]["index"], 0);
    assert_eq!(first["operations"][0]["entityId"], NEW);
    assert_eq!(reverse["operations"][0]["entityId"], OTHER);
    assert_eq!(first["changedFiles"], 2);
    assert_eq!(
        first["candidateFingerprint"],
        candidate(&world, &first).fingerprint
    );
    assert_eq!(world.records.len(), 2);
}

#[test]
fn interdependent_create_and_update_use_final_source_for_references_and_schema() {
    let world = world();
    let mixed = preview(
        &world,
        &plan(
            &world,
            vec![
                update(
                    ACTOR,
                    &format!("kind: character\ninitial_state: {{partner: {{entity: {NEW}}}}}\n"),
                ),
                create(
                    NEW,
                    &format!("initial_state: {{partner: {{entity: {ACTOR}}}}}\n"),
                ),
            ],
        ),
    );
    assert_eq!(mixed["validation"]["valid"], true, "{mixed}");
    // An ordinary new scalar declaration can be checked at final-candidate time.
    let schema = "kind: world\nstate_keys:\n  character:\n    condition: {type: string}\n    fresh: {type: boolean}\n";
    let report = preview(
        &world,
        &plan(
            &world,
            vec![
                create(NEW, "initial_state: {fresh: true}\n"),
                update(WORLD, schema),
            ],
        ),
    );
    assert_eq!(report["validation"]["valid"], true, "{report}");
}

#[test]
fn event_and_knowledge_forward_references_keep_exact_large_clocks() {
    let world = world();
    let knowledge = format!(
        "kind: knowledge\nid: know_00000000000000000000000001\ntitle: Authored report\nknower: {NEW}\nclaim: {{key: observed, statement: A literal report, subject: {NEW}, predicate: observes, object: {{text: dawn}}, truth: unknown}}\ntransitions: [{{id: first, time: {{timeline: main, tick: {TICK}, order: 2147483647}}, state: accepted, acquisition: observed, confidence: 0.9, causing_event: {EVENT}}}]\n"
    );
    let event = format!(
        "kind: event\nid: {EVENT}\ntitle: Explicit event\ntime: {{timeline: main, tick: {TICK}, order: 2147483647}}\nparticipants: [{{character: {NEW}}}]\neffects: [{{id: effect_00000000000000000000000001, target: {NEW}, key: measure, operation: set, value: {TICK}}}]\n"
    );
    let report = preview(
        &world,
        &plan(
            &world,
            vec![
                Operation::Create { draft: knowledge },
                Operation::Create { draft: event },
                create(NEW, ""),
            ],
        ),
    );
    assert_eq!(report["validation"]["valid"], true, "{report}");
    let final_world = candidate(&world, &report);
    assert_eq!(
        final_world.records[EVENT].get("time").unwrap()["tick"].as_i64(),
        Some(TICK)
    );
    assert_eq!(
        final_world.records[EVENT].get("effects").unwrap()[0]["value"].as_i64(),
        Some(TICK)
    );
    assert!(
        report["files"][1]["afterSource"]
            .as_str()
            .unwrap()
            .contains("9007199254740993")
    );
}

#[test]
fn invalid_complete_candidate_returns_all_diagnostics_and_exact_changes() {
    let world = world();
    let report = preview(
        &world,
        &plan(
            &world,
            vec![
                create(
                    NEW,
                    &format!("initial_state: {{partner: {{entity: {OTHER}}}}}\n"),
                ),
                update(
                    ACTOR,
                    "kind: character\ninitial_state: {undeclared: true}\n",
                ),
            ],
        ),
    );
    assert_eq!(report["validation"]["valid"], false);
    assert_eq!(report["validation"]["complete"], false);
    let diagnostics = report["validation"]["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["entityId"] == NEW && d["severity"] == "error")
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d["entityId"] == ACTOR && d["severity"] == "error")
    );
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
    assert_eq!(report["changedFiles"], 2);
    assert_eq!(report["canApply"], false);
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(
        report["validation"],
        serde_json::to_value(validation::validate_world(&candidate(&world, &report))).unwrap()
    );
}

#[test]
fn final_world_declaration_edit_can_invalidate_an_otherwise_constructible_create() {
    let world = world();
    let report = preview(
        &world,
        &plan(
            &world,
            vec![
                create(NEW, "initial_state: {condition: well}\n"),
                update(
                    WORLD,
                    "kind: world\nstate_keys: {character: {condition: {type: boolean}}}\n",
                ),
            ],
        ),
    );
    assert_eq!(report["validation"]["valid"], false);
    assert!(
        report["validation"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["entityId"] == NEW && d["severity"] == "error")
    );
}

#[test]
fn newly_bound_profile_values_and_new_profile_effect_targets_refuse_contextual_construction() {
    let world = world();
    let binding = "kind: world\nprofiles:\n- id: notes\n  label: Notes\n  fields: {type: object, required: {text: {type: string}}, optional: {}}\nstate_keys: {character: {condition: {type: string}, notes: {type: string}}}\nstate_profile_bindings: {character: {notes: notes}}\n";
    let new_actor = create(NEW, "initial_state: {notes: {text: explicit}}\n");
    let request = plan(&world, vec![update(WORLD, binding), new_actor.clone()]);
    let error = consequences::preview(&world, &request, consequences::MAX_CHARACTERS).unwrap_err();
    assert!(error.to_string().contains("operation 1"), "{error}");
    assert!(error.to_string().contains("captured base"));
    let bound = with_records(
        &world,
        vec![record_edit::update(&world, WORLD, binding).unwrap()],
    );
    assert!(validation::validate_world(&bound).valid);
    let report = preview(&bound, &plan(&bound, vec![new_actor.clone()]));
    assert_eq!(report["validation"]["valid"], true, "{report}");
    let event = Operation::Create {
        draft: format!(
            "kind: event\nid: {EVENT}\ntitle: Explicit bound effect\ntime: {{tick: 2}}\neffects: [{{id: effect_00000000000000000000000001, target: {NEW}, key: notes, operation: set, value: {{text: changed}}}}]\n"
        ),
    };
    let request = plan(&bound, vec![new_actor, event.clone()]);
    let error = consequences::preview(&bound, &request, consequences::MAX_CHARACTERS).unwrap_err();
    assert!(error.to_string().contains("operation 1"));
    let with_actor = candidate(&bound, &report);
    assert_eq!(
        preview(&with_actor, &plan(&with_actor, vec![event]))["validation"]["valid"],
        true
    );
}

#[test]
fn duplicate_write_ids_missing_exact_updates_and_aliases_never_form_a_candidate() {
    let world = world();
    for operations in [
        vec![
            create(NEW, ""),
            create(NEW, "body: Duplicate alternative\n"),
        ],
        vec![
            update(ACTOR, "kind: character\n"),
            update(ACTOR, "kind: character\nbody: changed\n"),
        ],
        vec![
            create(NEW, ""),
            update(NEW, "kind: character\nbody: changed\n"),
        ],
        vec![update(ACTOR, "kind: object\nbody: wrong-kind\n")],
        vec![create(ACTOR, "")],
    ] {
        assert!(
            consequences::preview(
                &world,
                &plan(&world, operations),
                consequences::MAX_CHARACTERS
            )
            .is_err()
        );
    }
    for alias in ["Ana", "Annie", "ana", "../story/world.md"] {
        assert!(
            consequences::validate_plan(&plan(&world, vec![update(alias, "kind: character\n")]))
                .is_err()
        );
    }
    assert!(matches!(
        consequences::preview(
            &world,
            &plan(&world, vec![create(NEW, ""), create(NEW, "body: second\n")]),
            consequences::MAX_CHARACTERS
        ),
        Err(Error::Conflict(_))
    ));
    let mut occupied = world.records[ACTOR].clone();
    occupied.source_path = format!("story/characters/{}.md", NEW.to_uppercase());
    let world = with_records(&world, vec![occupied]);
    assert!(validation::validate_world(&world).valid);
    assert!(matches!(
        consequences::preview(
            &world,
            &plan(&world, vec![create(NEW, "")]),
            consequences::MAX_CHARACTERS
        ),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn no_op_preserves_exact_header_body_hash_and_report_members() {
    let world = world();
    let plan = plan(&world, vec![update(ACTOR, "kind: character\ntitle: Ana\n")]);
    let report = preview(&world, &plan);
    let file = &report["files"][0];
    assert_eq!(report["canonicalNoOp"], true);
    assert_eq!(report["changedFiles"], 0);
    assert_eq!(report["candidateFingerprint"], world.fingerprint);
    assert_eq!(file["changed"], false);
    assert_eq!(file["beforeSource"], file["afterSource"]);
    assert_eq!(file["beforeSha256"], file["afterSha256"]);
    assert_eq!(file["diff"], "");
    assert_eq!(report["operations"][0]["changed"], false);
    assert_eq!(
        file["afterSource"].as_str().unwrap().as_bytes(),
        world.records[ACTOR].raw_bytes
    );
    assert_eq!(preview(&world, &plan), report);
}

#[test]
fn body_only_diffs_preserve_endings_mark_missing_newlines_and_keep_literal_prose() {
    let world = world();
    let body = "Proposed 🌲 prose\r\nLiteral temp:record and action prose; no inferred effects.  ";
    let patch = serde_yaml::to_string(&json!({"kind":"character","body":body})).unwrap();
    let report = preview(&world, &plan(&world, vec![update(ACTOR, &patch)]));
    let file = &report["files"][0];
    let expected = record_edit::update(&world, ACTOR, &patch).unwrap();
    assert_eq!(
        file["afterSource"].as_str().unwrap().as_bytes(),
        expected.raw_bytes
    );
    let diff = file["diff"].as_str().unwrap();
    assert!(diff.starts_with(
        "--- a/story/characters/retained.md\n+++ b/story/characters/retained.md\n@@ "
    ));
    assert!(diff.contains("-Original 🌲 prose  \r\n"));
    assert!(diff.contains("+Proposed 🌲 prose\r\n"));
    assert_eq!(diff.matches("\\ No newline at end of file\n").count(), 2);
    assert_eq!(candidate(&world, &report).records[ACTOR].body, body);
    assert_eq!(
        candidate(&world, &report).records.len(),
        world.records.len()
    );
}

#[test]
fn line_diff_removes_equal_prefix_and_suffix_and_keeps_complete_changed_middle() {
    let world = world();
    let replacement = record_edit::update(
        &world,
        ACTOR,
        "kind: character\nbody: |\n  prefix\n  left\n  middle\n  gone\n  suffix\n",
    )
    .unwrap();
    let world = with_records(&world, vec![replacement]);
    let report = preview(
        &world,
        &plan(
            &world,
            vec![update(
                ACTOR,
                "kind: character\nbody: |\n  prefix\n  right\n  middle\n  new\n  suffix\n",
            )],
        ),
    );
    let diff = report["files"][0]["diff"].as_str().unwrap();
    assert!(diff.contains("-left\n-middle\n-gone\n+right\n+middle\n+new\n"));
    assert!(!diff.contains("prefix"));
    assert!(!diff.contains("suffix"));
    assert!(diff.contains(",3 +"));
    let create = preview(&world, &plan(&world, vec![create(NEW, "")]));
    assert!(
        create["files"][0]["diff"]
            .as_str()
            .unwrap()
            .starts_with("--- /dev/null\n")
    );
    assert!(
        create["files"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("@@ -0,0 +1,")
    );
}

#[test]
fn unusual_valid_source_paths_are_quoted_without_injecting_diff_headers() {
    let world = world();
    let mut actor = world.records[ACTOR].clone();
    actor.source_path = "story/characters/a\n+++ b/injected\t\r\".md".into();
    let path = actor.source_path.clone();
    let world = with_records(&world, vec![actor]);
    let validation = validation::validate_world(&world);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    let report = preview(
        &world,
        &plan(
            &world,
            vec![update(ACTOR, "kind: character\nbody: changed\n")],
        ),
    );
    assert_eq!(report["files"][0]["sourcePath"], path);
    let diff = report["files"][0]["diff"].as_str().unwrap();
    let expected = format!(
        "--- {}\n+++ {}\n@@ ",
        serde_json::to_string(&format!("a/{path}")).unwrap(),
        serde_json::to_string(&format!("b/{path}")).unwrap()
    );
    assert!(diff.starts_with(&expected), "{diff}");
    assert_eq!(
        diff.lines().filter(|line| line.starts_with("+++ ")).count(),
        1
    );
}

#[test]
fn plan_and_preview_digests_bind_literal_requests_complete_result_and_budget() {
    let world = world();
    let first = plan(&world, vec![create(NEW, "")]);
    let mut second = first.clone();
    let Operation::Create { draft } = &mut second.operations[0] else {
        unreachable!()
    };
    *draft = draft.replace("title: Proposed actor", "title: 'Proposed actor'");
    let one = preview(&world, &first);
    let two = preview(&world, &second);
    assert_eq!(one["candidateFingerprint"], two["candidateFingerprint"]);
    assert_ne!(one["planSha256"], two["planSha256"]);
    assert_ne!(one["previewSha256"], two["previewSha256"]);
    assert_eq!(
        one["planSha256"],
        hash(
            b"wedl-consequence-plan\0",
            &serde_json::to_vec(&first).unwrap()
        )
    );
    let mut unhashed = one.clone();
    let digest = unhashed
        .as_object_mut()
        .unwrap()
        .remove("previewSha256")
        .unwrap();
    assert_eq!(
        digest,
        hash(
            b"wedl-consequence-preview\0",
            &serde_json::to_vec(&unhashed).unwrap()
        )
    );
    assert_eq!(
        one["files"][0]["afterSha256"],
        hash(
            b"",
            one["files"][0]["afterSource"].as_str().unwrap().as_bytes()
        )
    );
    let budget = consequences::preview(&world, &first, 100_000).unwrap();
    assert_eq!(budget["candidateFingerprint"], one["candidateFingerprint"]);
    assert_eq!(budget["planSha256"], one["planSha256"]);
    assert_ne!(budget["previewSha256"], one["previewSha256"]);
}

#[test]
fn complete_unicode_budget_includes_hashes_diffs_metadata_and_no_op_sources() {
    let world = world();
    let request = plan(
        &world,
        vec![create(NEW, &format!("body: {}\n", "🌲é".repeat(500)))],
    );
    let mut budget = consequences::MAX_CHARACTERS;
    for _ in 0..3 {
        let report = consequences::preview(&world, &request, budget).unwrap();
        budget = serde_json::to_string(&report).unwrap().chars().count();
    }
    let report = consequences::preview(&world, &request, budget).unwrap();
    let encoded = serde_json::to_string(&report).unwrap();
    assert_eq!(encoded.chars().count(), budget);
    assert!(encoded.len() > budget);
    assert!(consequences::preview(&world, &request, budget - 1).is_err());
    assert!(consequences::preview(&world, &request, 255).is_err());
    assert!(consequences::preview(&world, &request, consequences::MAX_CHARACTERS + 1).is_err());
    let large = record_edit::update(
        &world,
        ACTOR,
        &format!("kind: character\nbody: {}\n", "x".repeat(4096)),
    )
    .unwrap();
    let large = with_records(&world, vec![large]);
    assert!(
        consequences::preview(
            &large,
            &plan(&large, vec![update(ACTOR, "kind: character\n")]),
            4096
        )
        .is_err()
    );
}

#[test]
fn hard_source_budget_is_complete_even_when_the_patch_is_a_tiny_no_op() {
    let world = world();
    let before = &world.records[ACTOR];
    let boundary = before.raw_bytes.len() - before.body.len();
    let mut bytes = before.raw_bytes[..boundary].to_vec();
    bytes.extend(std::iter::repeat_n(b'x', consequences::MAX_PREVIEW_BYTES));
    let large = source::parse_record(&bytes, &before.source_path).unwrap();
    let world = with_records(&world, vec![large]);
    assert!(validation::validate_world(&world).valid);
    let error = consequences::preview(
        &world,
        &plan(&world, vec![update(ACTOR, "kind: character\n")]),
        consequences::MAX_CHARACTERS,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("8 MiB of complete source"),
        "{error}"
    );
}

#[test]
fn source_binding_and_base_integrity_are_checked_even_for_a_no_op() {
    let world = world();
    let mut plan = plan(&world, vec![update(ACTOR, "kind: character\n")]);
    plan.expected_fingerprint = "0".repeat(64);
    assert!(matches!(
        consequences::preview(&world, &plan, consequences::MAX_CHARACTERS),
        Err(Error::Conflict(_))
    ));
    plan.expected_fingerprint = world.fingerprint.clone();
    let mut forged = world.clone();
    forged
        .records
        .get_mut(ACTOR)
        .unwrap()
        .body
        .push_str("projection-only mutation");
    assert!(matches!(
        consequences::preview(&forged, &plan, consequences::MAX_CHARACTERS),
        Err(Error::Validation(_))
    ));
    let mut revision = world.clone();
    revision.revision = Some("1".repeat(40));
    let report = preview(&revision, &plan);
    assert_eq!(report["baseRevision"], "1".repeat(40));
    assert_eq!(report["canApply"], false);
}

#[test]
fn captured_world_preview_does_not_read_disk_or_modify_source_private_history_or_cache() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = world();
    world.root = directory.path().join("unopened-world");
    fs::create_dir_all(world.root.join("author/actions")).unwrap();
    fs::create_dir_all(world.root.join(".wedl-rust")).unwrap();
    fs::write(
        world.root.join("author/actions/retained.json"),
        b"private exact history\r\n",
    )
    .unwrap();
    fs::write(
        world.root.join(".wedl-rust/index.sqlite3"),
        b"unopened cache",
    )
    .unwrap();
    for record in world.records.values() {
        let path = world.root.join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &record.raw_bytes).unwrap();
    }
    let plan = plan(&world, vec![create(NEW, "body: An explicit alternative\n")]);
    let expected = preview(&world, &plan);
    let original_world = format!("{world:?}");
    // A captured preview stays tied to its supplied immutable World. Filesystem
    // current-source checks are the private-store save boundary's responsibility.
    fs::write(
        world.root.join("story/world.md"),
        b"disk changed outside snapshot",
    )
    .unwrap();
    assert_eq!(preview(&world, &plan), expected);
    assert_eq!(format!("{world:?}"), original_world);
    assert_eq!(
        fs::read(world.root.join("story/world.md")).unwrap(),
        b"disk changed outside snapshot"
    );
    assert_eq!(
        fs::read(world.root.join("author/actions/retained.json")).unwrap(),
        b"private exact history\r\n"
    );
    assert_eq!(
        fs::read(world.root.join(".wedl-rust/index.sqlite3")).unwrap(),
        b"unopened cache"
    );
    assert!(
        !world
            .root
            .join("story/characters")
            .join(format!("{NEW}.md"))
            .exists()
    );
    assert!(consequences::preview(&world, &plan, 256).is_err());
    fs::remove_dir_all(&world.root).unwrap();
    assert_eq!(preview(&world, &plan), expected);
    assert!(!world.root.exists());
}
