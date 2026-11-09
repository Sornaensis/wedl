//! Independent original-evidence boundary checks through saved outcome review.
#[path = "support/actions.rs"]
mod actions;

use actions::*;
use serde_json::json;
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::SystemTime};
use wedl::{
    action_drafts::{self, Draft, Proposal, QuestionAnswer},
    action_outcomes::{self, ApplyRequest, ReviewRequest, VerifyRequest},
    action_store, consequences,
    historical::{Citation, Target},
    model::World,
    source, validation,
};

const PROPOSAL: &str = "proposal_00000000000000000000000001";
const PRIOR: &str = "proposal_00000000000000000000000000";
const OTHER: &str = "proposal_00000000000000000000000002";
const HYPOTHESIS: &str = "hyp_00000000000000000000000001";

fn git(root: &Path, arguments: &[&str]) -> String {
    let mut command = Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(root)
        .args(arguments);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .env("GIT_AUTHOR_NAME", "Original evidence audit")
        .env("GIT_AUTHOR_EMAIL", "evidence@example.invalid")
        .env("GIT_COMMITTER_NAME", "Original evidence audit")
        .env("GIT_COMMITTER_EMAIL", "evidence@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn commit(world: &World, label: &str) -> String {
    git(&world.root, &["add", "--", "story"]);
    git(
        &world.root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            label,
        ],
    );
    git(&world.root, &["rev-parse", "HEAD"])
}

fn original(world: &World) -> String {
    git(&world.root, &["init", "--quiet"]);
    commit(world, "Original source")
}

fn captured(world: &World) -> World {
    let world = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn citation(world: &World, revision: &str, owner: &str, target: Target) -> Citation {
    let path = world.records[owner].source_path.clone();
    let blob_oid = git(&world.root, &["rev-parse", &format!("{revision}:{path}")]);
    Citation {
        revision: revision.into(),
        path,
        blob_oid,
        target,
    }
}

fn record_citation(world: &World, revision: &str, owner: &str) -> Citation {
    citation(world, revision, owner, Target::Record { id: owner.into() })
}

fn proposed(world: &World, evidence: Vec<Citation>, selected: Vec<Citation>) -> Draft {
    let mut value = draft(world, &id(1));
    value.evidence = evidence;
    value.questions[0].answer = Some("Explicit authored answer".into());
    value.phase = "proposed".into();
    value.proposals.push(Proposal {
        id: PROPOSAL.into(),
        summary: "Exact authored outcome".into(),
        context_fingerprint: world.fingerprint.clone(),
        evidence: selected,
        question_answers: vec![QuestionAnswer {
            question_id: value.questions[0].id.clone(),
            answer: value.questions[0].answer.clone().unwrap(),
        }],
        resolver: "human".into(),
        supersedes: None,
        consequences: consequences::decode(
            &json!({"expectedFingerprint":world.fingerprint,
            "operations":[{"type":"record.update","entityId":ACTOR,
                "patch":"kind: character\nbody: Explicit published outcome.\n"}]})
            .to_string(),
        )
        .unwrap(),
    });
    value
}

fn install(world: &World, draft: &Draft) -> ReviewRequest {
    action_drafts::validate(world, draft).unwrap();
    let bytes = action_drafts::serialize(draft).unwrap();
    let file = path(world, &draft.id);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, &bytes).unwrap();
    ReviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: draft.id.clone(),
        expected_sha256: action_drafts::sha256(&bytes),
        proposal_id: PROPOSAL.into(),
        max_characters: 100_000,
    }
}

fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .map(|entry| entry.unwrap())
        .filter(|entry| !entry.file_type().is_dir())
        .map(|entry| {
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            let bytes = if metadata.file_type().is_symlink() {
                fs::read_link(entry.path())
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec()
            } else {
                fs::read(entry.path()).unwrap()
            };
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                (bytes, metadata.modified().unwrap()),
            )
        })
        .collect()
}

fn rewrite(world: &World, owner: &str, fields: &Yaml, body: &str) {
    fs::write(
        world.root.join(&world.records[owner].source_path),
        format!("---\n{}---\n{body}", serde_yaml::to_string(fields).unwrap()),
    )
    .unwrap();
}

#[test]
fn only_selected_evidence_resolves_and_cross_array_duplicates_keep_their_order() {
    let (_directory, world) = fixture();
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, ACTOR);
    let mut value = proposed(&world, vec![evidence.clone()], vec![evidence.clone()]);
    let mut unavailable = value.proposals[0].clone();
    unavailable.id = PRIOR.into();
    unavailable.evidence[0].revision = "e".repeat(40);
    let mut other = unavailable.clone();
    other.id = OTHER.into();
    value.proposals[0].supersedes = Some(PRIOR.into());
    value.proposals.insert(0, unavailable);
    value.proposals.push(other);
    let request = install(&world, &value);
    let before = image(&world.root);
    let report = action_outcomes::review(&world, &request).unwrap();
    let expected = json!({"citation":evidence,
        "sourceSha256":action_drafts::sha256(&world.records[ACTOR].raw_bytes)});
    assert_eq!(report["evidence"], json!([expected.clone(), expected]));
    assert_eq!(report["canApply"], true);
    assert!(
        action_outcomes::review(
            &world,
            &ReviewRequest {
                proposal_id: OTHER.into(),
                ..request.clone()
            }
        )
        .is_err()
    );
    assert!(
        action_outcomes::review(
            &world,
            &ReviewRequest {
                proposal_id: PRIOR.into(),
                ..request
            }
        )
        .is_err()
    );
    assert_eq!(image(&world.root), before);
}

#[test]
fn original_section_survives_live_member_removal_and_owner_path_rename() {
    let (_directory, world) = fixture();
    let revision = original(&world);
    let evidence = citation(
        &world,
        &revision,
        ACTOR,
        Target::Section {
            owner_id: ACTOR.into(),
            id: SECTION.into(),
        },
    );
    let original_bytes = world.records[ACTOR].raw_bytes.clone();
    let mut fields = world.records[ACTOR].frontmatter.clone();
    fields
        .as_mapping_mut()
        .unwrap()
        .remove(Yaml::String("descriptions".into()));
    fields["title"] = Yaml::String("Current unrelated title".into());
    rewrite(&world, ACTOR, &fields, "Different current prose.\r\n");
    fs::rename(
        world.root.join(&world.records[ACTOR].source_path),
        world.root.join("story/moved-actor.md"),
    )
    .unwrap();
    let world = captured(&world);
    assert_ne!(world.records[ACTOR].source_path, evidence.path);
    let request = install(&world, &proposed(&world, vec![evidence.clone()], vec![]));
    let before = image(&world.root);
    let report = action_outcomes::review(&world, &request).unwrap();
    assert_eq!(
        report["evidence"],
        json!([{"citation":evidence,
        "sourceSha256":action_drafts::sha256(&original_bytes)}])
    );
    assert_eq!(report["canApply"], true);
    assert_eq!(image(&world.root), before);
}

#[test]
fn a_valid_cited_owner_cannot_hide_invalid_unrelated_historical_source() {
    let (_directory, world) = fixture();
    let broken = world.root.join("story/broken-location.md");
    fs::write(&broken, "---\nschema: wedl/v1\nkind: location\nid: loc_00000000000000000000000999\ntitle: Broken old source\nparent: loc_00000000000000000000000998\n---\nOld unrelated body.\n").unwrap();
    let invalid = source::load_world(&world.root, None).unwrap();
    assert!(!validation::validate_world(&invalid).valid);
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, ACTOR);
    fs::remove_file(broken).unwrap();
    let world = captured(&world);
    let request = install(&world, &proposed(&world, vec![evidence], vec![]));
    let before = image(&world.root);
    assert!(action_outcomes::review(&world, &request).is_err());
    assert_eq!(image(&world.root), before);
}

#[test]
fn commit_and_blob_replace_refs_cannot_substitute_original_evidence() {
    let (_directory, world) = fixture();
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, ACTOR);
    let original_bytes = world.records[ACTOR].raw_bytes.clone();
    rewrite(
        &world,
        ACTOR,
        &world.records[ACTOR].frontmatter,
        "Replacement-only body.\n",
    );
    let replacement = commit(&world, "Replacement source");
    let replacement_blob = git(
        &world.root,
        &["rev-parse", &format!("{replacement}:{}", evidence.path)],
    );
    git(
        &world.root,
        &["replace", &evidence.blob_oid, &replacement_blob],
    );
    git(&world.root, &["replace", &revision, &replacement]);
    let world = captured(&world);
    let request = install(&world, &proposed(&world, vec![evidence.clone()], vec![]));
    let before = image(&world.root);
    let report = action_outcomes::review(&world, &request).unwrap();
    assert_eq!(
        report["evidence"],
        json!([{"citation":evidence,
        "sourceSha256":action_drafts::sha256(&original_bytes)}])
    );
    assert_eq!(image(&world.root), before);
}

#[test]
fn an_unavailable_original_blob_is_not_rescued_by_replacements_or_live_source() {
    let (_directory, world) = fixture();
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, ACTOR);
    rewrite(
        &world,
        ACTOR,
        &world.records[ACTOR].frontmatter,
        "Available current source.\n",
    );
    let replacement = commit(&world, "Available alternate source");
    let replacement_blob = git(
        &world.root,
        &["rev-parse", &format!("{replacement}:{}", evidence.path)],
    );
    git(
        &world.root,
        &["replace", &evidence.blob_oid, &replacement_blob],
    );
    fs::remove_file(
        world
            .root
            .join(".git/objects")
            .join(&evidence.blob_oid[..2])
            .join(&evidence.blob_oid[2..]),
    )
    .unwrap();
    let world = captured(&world);
    let request = install(&world, &proposed(&world, vec![evidence], vec![]));
    let before = image(&world.root);
    assert!(action_outcomes::review(&world, &request).is_err());
    assert_eq!(image(&world.root), before);
}

#[test]
fn whole_owner_bytes_and_authored_evidence_order_change_the_evidence_digest() {
    let (_directory, first) = fixture();
    let revision = original(&first);
    let old = citation(
        &first,
        &revision,
        ACTOR,
        Target::Section {
            owner_id: ACTOR.into(),
            id: SECTION.into(),
        },
    );
    let mut changed = String::from_utf8(first.records[ACTOR].raw_bytes.clone())
        .unwrap()
        .replacen("---\n", "---\n# Different literal source spelling.\n", 1)
        .replace('\n', "\r\n");
    changed.push_str("Unrelated owner-body addition.  ");
    fs::write(first.root.join(&first.records[ACTOR].source_path), changed).unwrap();
    let next_revision = commit(&first, "Literal byte change");
    let world = captured(&first);
    assert_eq!(
        first.records[ACTOR].frontmatter,
        world.records[ACTOR].frontmatter
    );
    let new = citation(
        &world,
        &next_revision,
        ACTOR,
        Target::Section {
            owner_id: ACTOR.into(),
            id: SECTION.into(),
        },
    );
    let mut value = proposed(&world, vec![old.clone(), new.clone()], vec![]);
    let request = install(&world, &value);
    let before = image(&world.root);
    let report = action_outcomes::review(&world, &request).unwrap();
    assert_eq!(
        report["evidence"],
        json!([
        {"citation":old,"sourceSha256":action_drafts::sha256(&first.records[ACTOR].raw_bytes)},
        {"citation":new,"sourceSha256":action_drafts::sha256(&world.records[ACTOR].raw_bytes)}])
    );
    assert_ne!(
        report["evidence"][0]["sourceSha256"],
        report["evidence"][1]["sourceSha256"]
    );
    assert_eq!(image(&world.root), before);
    value.evidence.reverse();
    let reversed = install(&world, &value);
    let before = image(&world.root);
    let reversed = action_outcomes::review(&world, &reversed).unwrap();
    assert_eq!(reversed["evidence"][0], report["evidence"][1]);
    assert_eq!(reversed["evidence"][1], report["evidence"][0]);
    assert_ne!(reversed["evidenceSha256"], report["evidenceSha256"]);
    assert_ne!(reversed["reviewSha256"], report["reviewSha256"]);
    assert_eq!(image(&world.root), before);
}

#[test]
fn historical_hypothesis_links_do_not_become_canonical_evidence() {
    let (_directory, world) = fixture();
    fs::write(world.root.join("story/hypothesis.md"), format!(
        "---\nschema: wedl/v1\nkind: hypothesis\nid: {HYPOTHESIS}\ntitle: Private possibility\nstatus: open\nstatement: This is only a possibility.\nsubjects: [{ACTOR}]\nalternatives: [One possibility]\ncontext: Authored private reasoning.\nplacement: {{context: Authored private placement}}\n---\nNoncanonical body.\n")).unwrap();
    let world = captured(&world);
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, HYPOTHESIS);
    let mut value = proposed(&world, vec![evidence], vec![]);
    value
        .hypothesis_refs
        .push(action_drafts::Reference::Record {
            id: HYPOTHESIS.into(),
        });
    let request = install(&world, &value);
    let before = image(&world.root);
    // The same actual owner is a valid private link and an invalid canonical pin.
    assert!(action_outcomes::review(&world, &request).is_err());
    assert_eq!(image(&world.root), before);
}

#[test]
fn terminal_replay_and_source_verification_keep_the_review_time_evidence_binding() {
    let (_directory, world) = fixture();
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, ACTOR);
    let request = install(&world, &proposed(&world, vec![evidence], vec![]));
    let report = action_outcomes::review(&world, &request).unwrap();
    assert_eq!(report["evidenceSha256"].as_str().unwrap().len(), 64);
    let apply = ApplyRequest {
        expected_fingerprint: request.expected_fingerprint,
        action_id: request.action_id,
        expected_sha256: request.expected_sha256,
        proposal_id: request.proposal_id,
        max_characters: request.max_characters,
        review_sha256: report["reviewSha256"].as_str().unwrap().into(),
        approved: true,
    };
    let applied = action_outcomes::apply(&world, &apply).unwrap();
    let world = captured(&world);
    assert_eq!(
        applied["receipt"]["evidenceSha256"],
        report["evidenceSha256"]
    );
    fs::remove_dir_all(world.root.join(".git")).unwrap();
    let before = image(&world.root);
    let replay = action_outcomes::apply(&world, &apply).unwrap();
    assert_eq!(replay["alreadyApplied"], true);
    assert_eq!(replay["writesPerformed"], false);
    assert_eq!(replay["receipt"], applied["receipt"]);
    let verify = action_outcomes::verify(
        &world,
        &VerifyRequest {
            expected_fingerprint: world.fingerprint.clone(),
            action_id: id(1),
            expected_sha256: applied["sourceSha256"].as_str().unwrap().into(),
            max_characters: 100_000,
        },
    )
    .unwrap();
    assert_eq!(verify["verified"], true);
    assert_eq!(verify["verificationScope"], "exact-source-publication");
    assert_eq!(
        verify["receipt"]["evidenceSha256"],
        report["evidenceSha256"]
    );
    assert_eq!(
        action_store::detail(&world, &id(1), None).unwrap()["draft"]["evidence"],
        report["draft"]["evidence"]
    );
    assert_eq!(image(&world.root), before);
    assert!(!world.root.join(".git").exists());
}

#[cfg(unix)]
#[test]
fn an_unrelated_original_symlink_is_not_ignored_by_citation_resolution() {
    use std::os::unix::fs::symlink;
    let (_directory, world) = fixture();
    let linked = world.root.join("story/ignored-owner.md");
    symlink("does-not-exist", &linked).unwrap();
    let revision = original(&world);
    let evidence = record_citation(&world, &revision, ACTOR);
    fs::remove_file(linked).unwrap();
    let world = captured(&world);
    let request = install(&world, &proposed(&world, vec![evidence], vec![]));
    let before = image(&world.root);
    assert!(action_outcomes::review(&world, &request).is_err());
    assert_eq!(image(&world.root), before);
}
