//! Saved outcomes bind the exact original source of their selected evidence.
#[path = "support/actions.rs"]
mod support;

use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::SystemTime};
use support::*;
use wedl::{
    action_drafts::{self, Draft, Proposal, QuestionAnswer},
    action_outcomes::{self, ApplyRequest, ReviewRequest},
    action_store, consequences,
    historical::{Citation, Target},
    model::World,
    source, validation,
};

const PROPOSAL: &str = "proposal_00000000000000000000000001";
const PLOT: &str = "sp_00000000000000000000000001";

fn git(root: &Path, arguments: &[&str]) -> String {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(arguments);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .env("GIT_AUTHOR_NAME", "Original evidence test")
        .env("GIT_AUTHOR_EMAIL", "evidence@example.invalid")
        .env("GIT_COMMITTER_NAME", "Original evidence test")
        .env("GIT_COMMITTER_EMAIL", "evidence@example.invalid")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn initialize(world: &World, format: &str) {
    git(
        &world.root,
        &["init", "--quiet", &format!("--object-format={format}")],
    );
}

fn commit(world: &World, label: &str) -> String {
    git(&world.root, &["add", "--", "story"]);
    git(
        &world.root,
        &["commit", "--quiet", "--allow-empty", "-m", label],
    );
    git(&world.root, &["rev-parse", "HEAD"])
}

fn valid_current(world: &World) -> World {
    let current = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&current);
    assert!(report.valid, "{:?}", report.diagnostics);
    current
}

fn replace_body(world: &World, id: &str, body: &str) -> World {
    let record = &world.records[id];
    let source = format!(
        "---\n{}---\n{body}",
        serde_yaml::to_string(&record.frontmatter).unwrap()
    );
    fs::write(world.root.join(&record.source_path), source).unwrap();
    valid_current(world)
}

fn historical_fixture(format: &str) -> (tempfile::TempDir, World, String) {
    let (temp, world) = fixture();
    let record = json!({"schema":"wedl/v1","kind":"story-point","id":PLOT,"title":"Original trail",
    "activation_policy":"manual","lifecycle":{"initial_state":"dormant","transitions":[
        {"id":"earlier","time":point(TICK,-1),"state":"active","note":"Original earlier words"},
        {"id":"selected","time":point(TICK,0),"state":"resolved","note":"Original selected words"}
    ]}});
    fs::write(
        world.root.join("story").join(format!("{PLOT}.md")),
        format!(
            "---\n{}---\nOriginal plot body.\n",
            serde_yaml::to_string(&record).unwrap()
        ),
    )
    .unwrap();
    let world = valid_current(&world);
    initialize(&world, format);
    let revision = commit(&world, "original canonical evidence");
    (temp, world, revision)
}

fn citation(world: &World, revision: &str, owner: &str, target: Target) -> Citation {
    let path = world.records[owner].source_path.clone();
    Citation {
        revision: revision.into(),
        blob_oid: git(&world.root, &["rev-parse", &format!("{revision}:{path}")]),
        path,
        target,
    }
}

fn record_citation(world: &World, revision: &str, owner: &str) -> Citation {
    citation(world, revision, owner, Target::Record { id: owner.into() })
}

fn proposed(world: &World, evidence: Vec<Citation>, proposal_evidence: Vec<Citation>) -> Draft {
    let mut value = draft(world, &id(1));
    value.phase = "proposed".into();
    value.evidence = evidence;
    value.questions[0].answer = Some("Explicitly answered".into());
    value.proposals.push(Proposal {
        id: PROPOSAL.into(), summary: "One exact reviewed update".into(),
        context_fingerprint: world.fingerprint.clone(), evidence: proposal_evidence,
        question_answers: vec![QuestionAnswer { question_id: value.questions[0].id.clone(), answer: "Explicitly answered".into() }],
        resolver: "human".into(), supersedes: None,
        consequences: consequences::decode(&json!({"expectedFingerprint":world.fingerprint,
            "operations":[{"type":"record.update","entityId":ACTOR,"patch":"kind: character\nbody: Exact reviewed outcome.\n"}]}).to_string()).unwrap(),
    });
    value
}

fn install(world: &World, value: &Draft) -> ReviewRequest {
    let bytes = action_drafts::serialize(value).unwrap();
    fs::create_dir_all(world.root.join("author/actions")).unwrap();
    fs::write(path(world, &value.id), &bytes).unwrap();
    ReviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: value.id.clone(),
        expected_sha256: action_drafts::sha256(&bytes),
        proposal_id: PROPOSAL.into(),
        max_characters: 1_000_000,
    }
}

fn approved(request: &ReviewRequest, report: &Json) -> ApplyRequest {
    let mut value = json!(request);
    value["reviewSha256"] = report["reviewSha256"].clone();
    value["approved"] = json!(true);
    action_outcomes::decode_apply(&value.to_string()).unwrap()
}

fn image(world: &World) -> BTreeMap<String, (String, SystemTime)> {
    walkdir::WalkDir::new(&world.root)
        .into_iter()
        .map(|item| item.unwrap())
        .filter(|item| item.file_type().is_file())
        .map(|item| {
            (
                item.path()
                    .strip_prefix(&world.root)
                    .unwrap()
                    .to_string_lossy()
                    .into(),
                (
                    action_drafts::sha256(&fs::read(item.path()).unwrap()),
                    item.metadata().unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

fn evidence_digest(evidence: &Json) -> String {
    let mut bytes = b"wedl-action-outcome-evidence\0".to_vec();
    bytes.extend(serde_json::to_vec(evidence).unwrap());
    action_drafts::sha256(&bytes)
}

#[test]
fn original_record_section_and_transition_evidence_is_ordered_in_both_git_formats() {
    for format in ["sha1", "sha256"] {
        let (_temp, original, revision) = historical_fixture(format);
        let record = record_citation(&original, &revision, ACTOR);
        let section = citation(
            &original,
            &revision,
            ACTOR,
            Target::Section {
                owner_id: ACTOR.into(),
                id: SECTION.into(),
            },
        );
        let transition = citation(
            &original,
            &revision,
            PLOT,
            Target::Transition {
                owner_id: PLOT.into(),
                id: "selected".into(),
            },
        );
        let current = replace_body(&original, ACTOR, "Different live source.\n");
        let value = proposed(
            &current,
            vec![section.clone(), record.clone()],
            vec![transition.clone(), record.clone()],
        );
        let request = install(&current, &value);
        let before = image(&current);
        let report = action_outcomes::review(&current, &request).unwrap();
        let actor_digest = action_drafts::sha256(&original.records[ACTOR].raw_bytes);
        let plot_digest = action_drafts::sha256(&original.records[PLOT].raw_bytes);
        let expected = json!([
            {"citation":section,"sourceSha256":actor_digest},
            {"citation":record,"sourceSha256":actor_digest},
            {"citation":transition,"sourceSha256":plot_digest},
            {"citation":record,"sourceSha256":actor_digest},
        ]);
        assert_eq!(report["evidence"], expected);
        assert_eq!(report["evidenceSha256"], evidence_digest(&expected));
        assert_eq!(
            report["reviewBinding"]["evidenceSha256"],
            report["evidenceSha256"]
        );
        assert_ne!(
            actor_digest,
            action_drafts::sha256(&current.records[ACTOR].raw_bytes)
        );
        assert_eq!(report["canApply"], true);
        assert_eq!(image(&current), before);
        assert!(!current.root.join(".wedl-rust").exists());
    }
}

#[test]
fn exact_owner_member_kind_path_and_blob_are_all_required() {
    let (_temp, world, revision) = historical_fixture("sha1");
    let valid = citation(
        &world,
        &revision,
        ACTOR,
        Target::Section {
            owner_id: ACTOR.into(),
            id: SECTION.into(),
        },
    );
    let mut variants = Vec::new();
    let mut value = valid.clone();
    value.target = Target::Section {
        owner_id: PLACE.into(),
        id: SECTION.into(),
    };
    variants.push(value);
    let mut value = valid.clone();
    value.target = Target::Section {
        owner_id: ACTOR.into(),
        id: "absent".into(),
    };
    variants.push(value);
    let mut value = valid.clone();
    value.target = Target::Transition {
        owner_id: ACTOR.into(),
        id: SECTION.into(),
    };
    variants.push(value);
    let mut value = valid.clone();
    value.target = Target::Record {
        id: "missing-original-owner".into(),
    };
    variants.push(value);
    let mut value = valid.clone();
    value.path = world.records[PLACE].source_path.clone();
    variants.push(value);
    let mut value = valid.clone();
    value.blob_oid = record_citation(&world, &revision, PLACE).blob_oid;
    variants.push(value);
    let mut value = valid.clone();
    value.path = "story/moved-owner.md".into();
    variants.push(value);
    for candidate in variants {
        let request = install(&world, &proposed(&world, vec![candidate], vec![]));
        let before = image(&world);
        let error = action_outcomes::review(&world, &request)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("exact historical owner and member"),
            "{error}"
        );
        assert_eq!(image(&world), before);
        assert!(!world.root.join(".wedl-rust").exists());
    }
}

#[test]
fn historical_revisions_must_be_original_commits_not_blob_or_tree_objects() {
    let (_temp, world, revision) = historical_fixture("sha1");
    let original = record_citation(&world, &revision, ACTOR);
    for not_commit in [
        original.blob_oid.clone(),
        git(&world.root, &["rev-parse", "HEAD^{tree}"]),
        "f".repeat(40),
    ] {
        let mut evidence = original.clone();
        evidence.revision = not_commit;
        let request = install(&world, &proposed(&world, vec![evidence], vec![]));
        let before = image(&world);
        assert!(action_outcomes::review(&world, &request).is_err());
        assert_eq!(image(&world), before);
    }
}

#[test]
fn changing_evidence_order_changes_the_complete_binding_without_changing_the_plan() {
    let (_temp, world, revision) = historical_fixture("sha1");
    let actor = record_citation(&world, &revision, ACTOR);
    let place = record_citation(&world, &revision, PLACE);
    let a = proposed(&world, vec![actor.clone(), place.clone()], vec![]);
    let first = action_outcomes::review(&world, &install(&world, &a)).unwrap();
    let b = proposed(&world, vec![place, actor], vec![]);
    let second = action_outcomes::review(&world, &install(&world, &b)).unwrap();
    assert_ne!(first["evidenceSha256"], second["evidenceSha256"]);
    assert_ne!(first["reviewSha256"], second["reviewSha256"]);
    assert_eq!(first["consequences"], second["consequences"]);
    assert_eq!(first["proposalSha256"], second["proposalSha256"]);
}

#[test]
fn publication_retains_the_bound_original_evidence_digest_in_its_required_receipt() {
    let (_temp, world, revision) = historical_fixture("sha256");
    let request = install(
        &world,
        &proposed(
            &world,
            vec![record_citation(&world, &revision, ACTOR)],
            vec![],
        ),
    );
    let report = action_outcomes::review(&world, &request).unwrap();
    let result = action_outcomes::apply(&world, &approved(&request, &report)).unwrap();
    assert_eq!(
        result["receipt"]["evidenceSha256"],
        report["evidenceSha256"]
    );
    let source = fs::read_to_string(path(&world, &id(1))).unwrap();
    let retained: Json = serde_json::from_str(&source).unwrap();
    assert_eq!(retained["evidence"], report["draft"]["evidence"]);
    assert_eq!(
        retained["appliedReceipt"]["evidenceSha256"],
        report["evidenceSha256"]
    );
    let mut modified = retained.clone();
    modified["appliedReceipt"]["evidenceSha256"] = json!("a".repeat(64));
    assert!(action_drafts::decode(&modified.to_string()).is_err());
    modified = retained.clone();
    modified["appliedReceipt"]
        .as_object_mut()
        .unwrap()
        .remove("evidenceSha256");
    assert!(action_drafts::decode(&modified.to_string()).is_err());
    let duplicate = source.replacen(
        "\"evidenceSha256\":",
        "\"evidenceSha256\":\"a\",\"evidenceSha256\":",
        1,
    );
    assert!(action_drafts::decode(&duplicate).is_err());
}

#[test]
fn an_original_disappearing_after_review_blocks_first_application_without_writer_state() {
    let (_temp, world, revision) = historical_fixture("sha1");
    let evidence = record_citation(&world, &revision, ACTOR);
    let request = install(&world, &proposed(&world, vec![evidence.clone()], vec![]));
    let report = action_outcomes::review(&world, &request).unwrap();
    fs::remove_file(
        world
            .root
            .join(".git/objects")
            .join(&evidence.blob_oid[..2])
            .join(&evidence.blob_oid[2..]),
    )
    .unwrap();
    let before = image(&world);
    assert!(action_outcomes::apply(&world, &approved(&request, &report)).is_err());
    assert_eq!(image(&world), before);
    assert!(!world.root.join(".wedl-rust").exists());
    assert_eq!(
        action_drafts::decode(&fs::read_to_string(path(&world, &id(1))).unwrap())
            .unwrap()
            .phase,
        "proposed"
    );
}

#[test]
fn the_complete_evidence_report_cannot_exceed_the_requested_budget() {
    let (_temp, world, revision) = historical_fixture("sha1");
    let request = install(
        &world,
        &proposed(
            &world,
            vec![record_citation(&world, &revision, ACTOR)],
            vec![record_citation(&world, &revision, PLACE)],
        ),
    );
    let report = action_outcomes::review(&world, &request).unwrap();
    let preview = action_store::proposal_preview(&world, &request).unwrap();
    let too_small = report.to_string().chars().count() - 500;
    assert!(preview.to_string().chars().count() < too_small);
    let request = ReviewRequest {
        max_characters: too_small,
        ..request
    };
    let before = image(&world);
    let error = action_outcomes::review(&world, &request)
        .unwrap_err()
        .to_string();
    assert!(error.contains("complete outcome report exceeds"), "{error}");
    assert_eq!(image(&world), before);
}

#[test]
fn distinct_revision_allowance_is_complete_and_duplicate_array_occurrences_do_not_spend_it() {
    let (_temp, world) = fixture();
    initialize(&world, "sha1");
    let mut citations = Vec::new();
    for n in 0..65 {
        let revision = commit(&world, &format!("original revision {n}"));
        citations.push(record_citation(&world, &revision, ACTOR));
    }
    let value = proposed(&world, citations[..64].to_vec(), vec![citations[0].clone()]);
    let request = install(&world, &value);
    let before = image(&world);
    let report = action_outcomes::review(&world, &request).unwrap();
    assert_eq!(report["evidence"].as_array().unwrap().len(), 65);
    assert_eq!(image(&world), before);
    let value = proposed(&world, citations, vec![]);
    let request = install(&world, &value);
    let before = image(&world);
    let error = action_outcomes::review(&world, &request)
        .unwrap_err()
        .to_string();
    assert!(error.contains("64 original revisions"), "{error}");
    assert_eq!(image(&world), before);
}

#[test]
fn source_bytes_are_bounded_across_distinct_original_revisions() {
    let (_temp, world) = fixture();
    let original = replace_body(&world, PLACE, &"x".repeat(7 * 1024 * 1024));
    initialize(&original, "sha1");
    let mut citations = Vec::new();
    for n in 0..5 {
        let revision = commit(&original, &format!("large original revision {n}"));
        citations.push(record_citation(&original, &revision, ACTOR));
    }
    let current = replace_body(&original, PLACE, "Current small body.\n");
    let value = proposed(&current, citations[..4].to_vec(), vec![]);
    let request = install(&current, &value);
    action_outcomes::review(&current, &request).unwrap();
    let value = proposed(&current, citations, vec![]);
    let request = install(&current, &value);
    let before = image(&current);
    let error = action_outcomes::review(&current, &request)
        .unwrap_err()
        .to_string();
    assert!(error.contains("byte bound"), "{error}");
    assert_eq!(image(&current), before);
    assert!(!current.root.join(".wedl-rust").exists());
}
