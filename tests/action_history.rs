//! Historical private sources are original captures, not current author input.
#[path = "support/actions.rs"]
mod actions;

use actions::*;
use serde_json::{Value as Json, json};
use std::{fs, path::Path, process::Command};
use wedl::{
    Error,
    action_drafts::{self, Draft, Proposal, QuestionAnswer},
    action_outcomes, action_store, consequences,
    historical::Citation,
    source,
};

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_AUTHOR_NAME", "History test")
        .env("GIT_AUTHOR_EMAIL", "history@example.invalid")
        .env("GIT_COMMITTER_NAME", "History test")
        .env("GIT_COMMITTER_EMAIL", "history@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn init(root: &Path) {
    git(root, &["init", "--quiet"]);
}

fn commit(root: &Path, label: &str) -> String {
    if root.join("author").exists() {
        git(root, &["add", "--", "story", "author"]);
    } else {
        git(root, &["add", "--", "story"]);
    }
    git(root, &["commit", "--quiet", "--allow-empty", "-m", label]);
    git(root, &["rev-parse", "HEAD"])
}

fn image(root: &Path) -> Vec<(String, String, std::time::SystemTime)> {
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(root).sort_by_file_name() {
        let entry = entry.unwrap();
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        if metadata.is_file() || metadata.file_type().is_symlink() {
            let bytes = if metadata.file_type().is_symlink() {
                fs::read_link(entry.path())
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec()
            } else {
                fs::read(entry.path()).unwrap()
            };
            result.push((
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                action_drafts::sha256(&bytes),
                metadata.modified().unwrap(),
            ));
        }
    }
    result
}

fn citation(target: Json) -> Json {
    json!({"revision":"a".repeat(40),"path":"story/old/PRIVATE_HISTORICAL_EVIDENCE.md",
        "blob_oid":"b".repeat(40),"target":target})
}

fn proposed(world: &wedl::model::World) -> Draft {
    let mut value = draft(world, &id(1));
    value.questions[0].answer = Some("Explicit reviewed answer".into());
    value.phase = "proposed".into();
    value.proposals.push(Proposal {
        id: "proposal_00000000000000000000000001".into(),
        summary: "Explicit candidate".into(),
        context_fingerprint: world.fingerprint.clone(),
        evidence: vec![],
        question_answers: vec![QuestionAnswer {
            question_id: value.questions[0].id.clone(),
            answer: value.questions[0].answer.clone().unwrap(),
        }],
        resolver: "human".into(),
        consequences: consequences::decode(
            &json!({"expectedFingerprint":world.fingerprint,
            "operations":[{"type":"record.update","entityId":ACTOR,
                "patch":"kind: character\nbody: Explicitly proposed replacement.\n"}]})
            .to_string(),
        )
        .unwrap(),
        supersedes: None,
    });
    value
}

fn write_private(world: &wedl::model::World, value: &Draft) -> Vec<u8> {
    fs::create_dir_all(world.root.join("author/actions")).unwrap();
    let bytes = action_drafts::serialize(value).unwrap();
    fs::write(path(world, &value.id), &bytes).unwrap();
    bytes
}

#[test]
fn action_and_proposal_evidence_are_required_closed_syntax_without_git_resolution() {
    let (_directory, world) = fixture();
    let mut value = json!(proposed(&world));
    let pointers = json!([
        citation(json!({"kind":"record","id":"not-a-current-record-id"})),
        citation(json!({"kind":"section","owner_id":"historical owner","id":"old section"})),
        citation(json!({"kind":"transition","owner_id":"historical owner","id":"old transition"})),
    ]);
    value["evidence"] = pointers.clone();
    value["proposals"][0]["evidence"] = pointers.clone();
    let decoded = action_drafts::decode(&value.to_string()).unwrap();
    assert_eq!(json!(decoded.evidence), pointers);
    assert_eq!(json!(decoded.proposals[0].evidence), pointers);
    assert!(!world.root.join(".git").exists());
    for proposal in [false, true] {
        let mut missing = value.clone();
        let object = if proposal {
            &mut missing["proposals"][0]
        } else {
            &mut missing
        };
        object.as_object_mut().unwrap().remove("evidence");
        assert!(action_drafts::decode(&missing.to_string()).is_err());
    }
    for target in [
        Json::Null,
        json!({}),
        json!({"kind":"record","id":"old","extra":false}),
        json!({"kind":"section","ownerId":"old","id":"member"}),
        json!({"kind":"transition","owner_id":"old","id":" "}),
    ] {
        let mut bad = value.clone();
        bad["evidence"][0]["target"] = target;
        assert!(action_drafts::decode(&bad.to_string()).is_err());
    }
}

#[test]
fn historical_citation_validation_rejects_duplicates_paths_aliases_and_complete_bounds() {
    let (_directory, world) = fixture();
    let mut value = json!(draft(&world, &id(1)));
    let pointer = citation(json!({"kind":"record","id":"old"}));
    value["evidence"] = json!([pointer.clone()]);
    for (key, bad) in [
        ("revision", json!("HEAD")),
        ("revision", json!("A".repeat(40))),
        ("blob_oid", json!("b".repeat(39))),
        ("path", json!("../outside.md")),
        ("path", json!("story//old.md")),
        ("path", json!("story/./old.md")),
        ("path", json!("/absolute.md")),
        ("path", json!("story\\old.md")),
        ("path", json!("story:old.md")),
        ("path", json!("story/old\0.md")),
        ("path", json!(" ")),
        ("extra", json!(true)),
    ] {
        let mut candidate = value.clone();
        candidate["evidence"][0][key] = bad;
        assert!(
            action_drafts::decode(&candidate.to_string()).is_err(),
            "{key}"
        );
    }
    for bad in [
        Json::Null,
        json!({}),
        json!([pointer.clone(), pointer.clone()]),
        json!(
            (0..1025)
                .map(|n| {
                    let mut value = pointer.clone();
                    value["target"]["id"] = json!(format!("old-{n}"));
                    value
                })
                .collect::<Vec<_>>()
        ),
    ] {
        let mut candidate = value.clone();
        candidate["evidence"] = bad;
        assert!(action_drafts::decode(&candidate.to_string()).is_err());
    }
    let raw = value
        .to_string()
        .replace("\"blob_oid\":", "\"blob_oid\":\"b\",\"blob_oi\\u0064\":");
    assert!(action_drafts::decode(&raw).is_err());
    value["evidence"][0]["revision"] = json!("a".repeat(64));
    value["evidence"][0]["blob_oid"] = json!("b".repeat(64));
    assert!(action_drafts::decode(&value.to_string()).is_ok());
}

#[test]
fn retained_evidence_binds_review_but_never_enters_admitted_context_or_source() {
    let (_directory, world) = fixture();
    let mut value = draft(&world, &id(1));
    let selection = action_drafts::ReviewRequest {
        draft: value.clone(),
        frame: frame(&world, "character"),
        briefs: vec![],
        families: wedl::writing_frames::FamilySelection::default(),
        max_characters: 100_000,
    };
    let before = action_drafts::review(&world, &selection).unwrap();
    let source_before = source_image(&world);
    value.evidence.push(
        serde_json::from_value::<Citation>(citation(
            json!({"kind":"record","id":"PRIVATE_UNKNOWN_TARGET"}),
        ))
        .unwrap(),
    );
    let after = action_drafts::review(
        &world,
        &action_drafts::ReviewRequest {
            draft: value.clone(),
            ..selection
        },
    )
    .unwrap();
    assert_eq!(after["context"], before["context"]);
    assert_ne!(after["draftSha256"], before["draftSha256"]);
    assert_ne!(after["reviewSha256"], before["reviewSha256"]);
    assert!(!after["context"].to_string().contains("PRIVATE_"));
    action_store::save(&world, &request(&world, value.clone(), None)).unwrap();
    let retained = action_store::detail(&world, &id(1), None).unwrap();
    assert_eq!(retained["draft"]["evidence"], json!(value.evidence));
    assert_eq!(source_image(&world), source_before);
    assert!(!world.root.join(".git").exists());
}

#[test]
fn prior_proposal_evidence_is_immutable_and_draft_evidence_obeys_the_existing_phase_gate() {
    let (_directory, world) = fixture();
    let previous = proposed(&world);
    let evidence: Citation =
        serde_json::from_value(citation(json!({"kind":"record","id":"old"}))).unwrap();
    let mut next = previous.clone();
    next.evidence.push(evidence.clone());
    assert!(
        action_drafts::validate_save(
            &world,
            &request(&world, next.clone(), Some(&"a".repeat(64))),
            Some(&previous)
        )
        .is_err()
    );
    next.phase = "stale".into();
    action_drafts::validate_save(
        &world,
        &request(&world, next.clone(), Some(&"a".repeat(64))),
        Some(&previous),
    )
    .unwrap();
    next.proposals[0].evidence.push(evidence);
    assert!(
        action_drafts::validate_save(
            &world,
            &request(&world, next, Some(&"a".repeat(64))),
            Some(&previous)
        )
        .is_err()
    );
}

#[test]
fn historical_action_reads_return_original_private_bytes_while_live_source_is_unavailable() {
    let (_directory, world) = fixture();
    init(&world.root);
    let mut value = draft(&world, &id(1));
    value.evidence.push(
        serde_json::from_value(citation(
            json!({"kind":"transition","owner_id":"old","id":"transition"}),
        ))
        .unwrap(),
    );
    fs::create_dir_all(world.root.join("author/actions")).unwrap();
    let raw = format!(
        "{}\r\n",
        serde_json::to_string_pretty(&value)
            .unwrap()
            .replace('\n', "\r\n")
    );
    fs::write(path(&world, &id(1)), raw.as_bytes()).unwrap();
    let revision = commit(&world.root, "original private source");
    let selected = source::load_world(&world.root, Some(&revision)).unwrap();
    let oid = git(
        &world.root,
        &[
            "rev-parse",
            &format!("{revision}:author/actions/{}.json", id(1)),
        ],
    );
    fs::write(path(&world, &id(1)), "invalid live private bytes").unwrap();
    fs::write(
        world.root.join(&world.world_record().unwrap().source_path),
        "invalid live canonical bytes",
    )
    .unwrap();
    fs::create_dir_all(world.root.join(".wedl-rust")).unwrap();
    fs::write(
        world.root.join(".wedl-rust/pending-outcome.json"),
        "unrelated pending current transaction",
    )
    .unwrap();
    let before = image(&world.root);
    let detail = action_store::detail(
        &selected,
        &id(1),
        Some(&action_drafts::sha256(raw.as_bytes())),
    )
    .unwrap();
    assert_eq!(detail["source"], raw);
    assert_eq!(detail["draft"], json!(value));
    assert_eq!(
        detail["sourceSha256"],
        action_drafts::sha256(raw.as_bytes())
    );
    assert_eq!(detail["blobOid"], oid);
    assert_eq!(detail["revision"], revision);
    assert_eq!(detail["historical"], true);
    assert_eq!(detail["writesPerformed"], false);
    let listed = action_store::list(&selected).unwrap();
    assert_eq!(listed["items"][0]["blobOid"], detail["blobOid"]);
    assert_eq!(listed["items"][0]["revision"], detail["revision"]);
    assert!(source::load_world(&world.root, None).is_err());
    assert_eq!(image(&world.root), before);
}

#[test]
fn missing_historical_private_sources_never_fall_back_to_existing_live_drafts() {
    let (_directory, world) = fixture();
    init(&world.root);
    let revision = commit(&world.root, "source without private drafts");
    write_private(&world, &draft(&world, &id(1)));
    let selected = source::load_world(&world.root, Some(&revision)).unwrap();
    let before = image(&world.root);
    assert_eq!(action_store::list(&selected).unwrap()["items"], json!([]));
    assert!(matches!(
        action_store::detail(&selected, &id(1), None),
        Err(Error::NotFound(_))
    ));
    assert_eq!(image(&world.root), before);
}

#[test]
fn historical_effective_phase_uses_the_selected_snapshot_and_not_the_current_head() {
    let (_directory, world) = fixture();
    init(&world.root);
    write_private(&world, &proposed(&world));
    let original = commit(&world.root, "proposed at original source");
    let changed = change_source(&world);
    let later = commit(&world.root, "different canonical source");
    let old = source::load_world(&world.root, Some(&original)).unwrap();
    let new = source::load_world(&world.root, Some(&later)).unwrap();
    let before = image(&world.root);
    let original_detail = action_store::detail(&old, &id(1), None).unwrap();
    let later_detail = action_store::detail(&new, &id(1), None).unwrap();
    assert_eq!(original_detail["effectivePhase"], "proposed");
    assert_eq!(original_detail["sourceChanged"], false);
    assert_eq!(later_detail["phase"], "proposed");
    assert_eq!(later_detail["effectivePhase"], "stale");
    assert_eq!(later_detail["sourceChanged"], true);
    assert_eq!(later_detail["baseFingerprint"], world.fingerprint);
    assert_eq!(later_detail["fingerprint"], changed.fingerprint);
    assert_eq!(original_detail["source"], later_detail["source"]);
    assert_eq!(image(&world.root), before);
}

#[test]
fn historical_private_reads_refuse_stale_hashes_and_forged_captures() {
    let (_directory, world) = fixture();
    init(&world.root);
    write_private(&world, &draft(&world, &id(1)));
    let revision = commit(&world.root, "valid original capture");
    let selected = source::load_world(&world.root, Some(&revision)).unwrap();
    let before = image(&world.root);
    assert!(matches!(
        action_store::detail(&selected, &id(1), Some(&"0".repeat(64))),
        Err(Error::Conflict(_))
    ));
    for change in 0..6 {
        let mut forged = selected.clone();
        match change {
            0 => forged.fingerprint = "0".repeat(64),
            1 => forged
                .records
                .get_mut(ACTOR)
                .unwrap()
                .body
                .push_str("forged parsed projection"),
            2 => forged.records.get_mut(ACTOR).unwrap().blob_oid = Some("a".repeat(40)),
            3 => forged.revision = Some("HEAD".into()),
            4 => forged.revision = Some("a".repeat(40)),
            _ => forged.root = world.root.join("story"),
        }
        assert!(action_store::list(&forged).is_err(), "forgery {change}");
        assert!(
            action_store::detail(&forged, &id(1), None).is_err(),
            "forgery {change}"
        );
    }
    assert_eq!(image(&world.root), before);
}

#[test]
fn historical_private_documents_must_match_their_exact_path_and_selected_world() {
    for variant in 0..3 {
        let (_directory, world) = fixture();
        init(&world.root);
        let mut value = draft(&world, &id(1));
        let filename = if variant == 0 { id(2) } else { id(1) };
        if variant == 1 {
            value.world_id = "world_00000000000000000000000001".into();
        }
        fs::create_dir_all(world.root.join("author/actions")).unwrap();
        let target = if variant == 2 {
            world.root.join("author/actions/unexpected.tmp")
        } else {
            path(&world, &filename)
        };
        fs::write(target, action_drafts::serialize(&value).unwrap()).unwrap();
        let revision = commit(&world.root, "mismatched private identity");
        let selected = source::load_world(&world.root, Some(&revision)).unwrap();
        let before = image(&world.root);
        assert!(action_store::list(&selected).is_err(), "identity {variant}");
        assert!(
            action_store::detail(&selected, &filename, None).is_err(),
            "identity {variant}"
        );
        assert_eq!(image(&world.root), before);
    }
}

#[test]
fn historical_private_reads_enforce_complete_document_count_and_byte_bounds() {
    for variant in 0..3 {
        let (_directory, world) = fixture();
        init(&world.root);
        fs::create_dir_all(world.root.join("author/actions")).unwrap();
        let (count, bytes) = match variant {
            0 => (1, 262_145),
            1 => (1025, 0),
            _ => (65, 262_144),
        };
        for n in 1..=count {
            fs::write(path(&world, &id(n)), vec![b' '; bytes]).unwrap();
        }
        let revision = commit(&world.root, "complete bound refusal");
        let selected = source::load_world(&world.root, Some(&revision)).unwrap();
        let before = image(&world.root);
        assert!(action_store::list(&selected).is_err(), "bound {variant}");
        assert!(
            action_store::detail(&selected, &id(1), None).is_err(),
            "bound {variant}"
        );
        assert_eq!(image(&world.root), before);
    }
}

#[test]
fn historical_discovery_does_not_authorize_current_private_or_canonical_workflows() {
    let (_directory, world) = fixture();
    init(&world.root);
    let value = proposed(&world);
    let bytes = write_private(&world, &value);
    let revision = commit(&world.root, "valid retained proposal");
    let selected = source::load_world(&world.root, Some(&revision)).unwrap();
    let hash = action_drafts::sha256(&bytes);
    let preview = action_store::ProposalPreviewRequest {
        expected_fingerprint: selected.fingerprint.clone(),
        action_id: value.id.clone(),
        expected_sha256: hash.clone(),
        proposal_id: value.proposals[0].id.clone(),
        max_characters: 100_000,
    };
    let before = image(&world.root);
    assert!(action_store::detail(&selected, &id(1), Some(&hash)).is_ok());
    assert!(action_store::proposal_preview(&selected, &preview).is_err());
    assert!(action_outcomes::review(&selected, &preview).is_err());
    let apply = action_outcomes::decode_apply(
        &json!({"expectedFingerprint":selected.fingerprint,
        "actionId":value.id,"expectedSha256":hash,"proposalId":value.proposals[0].id,
        "maxCharacters":100000,"reviewSha256":"0".repeat(64),"approved":true})
        .to_string(),
    )
    .unwrap();
    assert!(action_outcomes::apply(&selected, &apply).is_err());
    let verify = action_outcomes::decode_verify(
        &json!({"expectedFingerprint":selected.fingerprint,
        "actionId":value.id,"expectedSha256":hash,"maxCharacters":100000})
        .to_string(),
    )
    .unwrap();
    assert!(action_outcomes::verify(&selected, &verify).is_err());
    let save = request(&selected, value.clone(), Some(&hash));
    assert!(action_store::preview(&selected, &save).is_err());
    assert!(action_store::save(&selected, &save).is_err());
    assert!(
        action_drafts::review(
            &selected,
            &action_drafts::ReviewRequest {
                draft: value,
                frame: frame(&selected, "author"),
                briefs: vec![],
                families: wedl::writing_frames::FamilySelection::default(),
                max_characters: 100_000,
            }
        )
        .is_err()
    );
    assert_eq!(image(&world.root), before);
}

#[cfg(unix)]
#[test]
fn historical_private_symlink_files_and_parent_trees_are_never_followed() {
    use std::os::unix::fs::symlink;
    for parent in [false, true] {
        let (_directory, world) = fixture();
        init(&world.root);
        let other = world.root.join("unrelated");
        fs::create_dir_all(other.join("actions")).unwrap();
        fs::write(
            other.join("actions").join(format!("{}.json", id(1))),
            action_drafts::serialize(&draft(&world, &id(1))).unwrap(),
        )
        .unwrap();
        if parent {
            symlink("unrelated", world.root.join("author")).unwrap();
        } else {
            fs::create_dir_all(world.root.join("author/actions")).unwrap();
            symlink(
                format!("../../unrelated/actions/{}.json", id(1)),
                path(&world, &id(1)),
            )
            .unwrap();
        }
        let revision = commit(&world.root, "private symlink refusal");
        let selected = source::load_world(&world.root, Some(&revision)).unwrap();
        let before = image(&world.root);
        assert!(action_store::list(&selected).is_err());
        assert!(action_store::detail(&selected, &id(1), None).is_err());
        assert_eq!(image(&world.root), before);
    }
}
