//! Linked possibilities remain private reasoning and never become context grants.
#[path = "support/actions.rs"]
mod actions;

use actions::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{fs, path::Path, process::Command, time::SystemTime};
use tower::ServiceExt;
use wedl::{
    action_drafts::{self, Draft, Reference, ReviewConfirmation, ReviewRequest},
    action_store, character, compiler, consequences,
    model::World,
    runtime,
    service::{Operation, Service},
    source, validation,
    writing_frames::FamilySelection,
};

const SECRET: &str = "obj_00000000000000000000000999";
const PROPOSAL: &str = "proposal_00000000000000000000000001";

fn hypothesis(n: usize) -> String {
    format!("hyp_{n:026}")
}
fn reference(n: usize) -> Reference {
    Reference::Record { id: hypothesis(n) }
}
fn write_record(world: &World, kind: &str, id: &str, fields: Json, body: &str) {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":"LINKED_PRIVATE_TITLE"});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    fs::write(
        world.root.join("story").join(format!("{id}.md")),
        format!("---\n{}---\n{body}", serde_yaml::to_string(&value).unwrap()),
    )
    .unwrap();
}
fn with_hypotheses() -> (tempfile::TempDir, World) {
    let (directory, world) = fixture();
    write_record(
        &world,
        "object",
        SECRET,
        json!({"title":"UNSEEN_HYPOTHESIS_SUBJECT","status":"canonical"}),
        "Unseen subject prose.\r\n",
    );
    for (n, status) in [(1, "open"), (2, "adopted"), (3, "rejected")] {
        let mut fields = json!({"status":status,"domain":"author.possibilities",
            "aliases":["LINKED_PRIVATE_ALIAS"],"statement":"LINKED_PRIVATE_STATEMENT",
            "subjects":[SECRET],"alternatives":["Maybe A","Maybe B"],"context":"LINKED_PRIVATE_CONTEXT",
            "placement":{"context":"LINKED_PRIVATE_PLACEMENT"}});
        if status == "adopted" {
            fields["resolution"] = json!({"canonical_entities":[PLACE]});
        }
        if status == "rejected" {
            fields["resolution"] = json!({"note":"Literal rejection"});
        }
        write_record(
            &world,
            "hypothesis",
            &hypothesis(n),
            fields,
            "LINKED_PRIVATE_BODY 🌲\r\nNo final newline  ",
        );
    }
    let world = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    (directory, world)
}
fn review_request(world: &World, value: Draft, lens: &str) -> ReviewRequest {
    ReviewRequest {
        draft: value,
        frame: frame(world, lens),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    }
}
fn confirmation(world: &World, value: Draft) -> ReviewConfirmation {
    let input = review_request(world, value, "author");
    let report = action_drafts::review(world, &input).unwrap();
    ReviewConfirmation {
        frame: input.frame,
        briefs: input.briefs,
        families: input.families,
        max_characters: input.max_characters,
        review_sha256: report["reviewSha256"].as_str().unwrap().into(),
        sufficient: true,
    }
}
fn image(root: &Path) -> Vec<(String, Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let metadata = entry.metadata().unwrap();
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                fs::read(entry.path()).unwrap(),
                metadata.modified().unwrap(),
            )
        })
        .collect()
}
fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_AUTHOR_NAME", "Hypothesis links test")
        .env("GIT_AUTHOR_EMAIL", "links@example.invalid")
        .env("GIT_COMMITTER_NAME", "Hypothesis links test")
        .env("GIT_COMMITTER_EMAIL", "links@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn required_closed_record_links_preserve_order_and_full_array_bounds() {
    let (_directory, world) = with_hypotheses();
    let mut value = json!(draft(&world, &id(1)));
    value["hypothesisRefs"] = json!([reference(3), reference(1), reference(2)]);
    let decoded = action_drafts::decode(&value.to_string()).unwrap();
    assert_eq!(
        decoded.hypothesis_refs,
        vec![reference(3), reference(1), reference(2)]
    );
    assert_eq!(
        action_drafts::decode(
            std::str::from_utf8(&action_drafts::serialize(&decoded).unwrap()).unwrap()
        )
        .unwrap(),
        decoded
    );
    for invalid in [
        Json::Null,
        json!({}),
        json!([hypothesis(1)]),
        json!([{"kind":"section","ownerId":hypothesis(1),"id":SECTION}]),
        json!([{"kind":"record","id":hypothesis(1),"title":"Do not trust"}]),
        json!([{"kind":"record","id":"LINKED_PRIVATE_ALIAS"}]),
        json!([reference(1), reference(1)]),
    ] {
        let mut bad = value.clone();
        bad["hypothesisRefs"] = invalid;
        assert!(action_drafts::decode(&bad.to_string()).is_err());
    }
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("hypothesisRefs");
    assert!(action_drafts::decode(&missing.to_string()).is_err());
    let raw = value.to_string().replace(
        "\"hypothesisRefs\":",
        "\"hypothesisRefs\":[],\"hypothesisRe\\u0066s\":",
    );
    assert!(action_drafts::decode(&raw).is_err());
    value["hypothesisRefs"] = json!((0..1024).map(reference).collect::<Vec<_>>());
    assert_eq!(
        action_drafts::decode(&value.to_string())
            .unwrap()
            .hypothesis_refs
            .len(),
        1024
    );
    value["hypothesisRefs"]
        .as_array_mut()
        .unwrap()
        .push(json!(reference(1024)));
    assert!(action_drafts::decode(&value.to_string()).is_err());
}

#[test]
fn actual_hypotheses_of_every_status_are_links_without_canonical_reference_admission() {
    let (_directory, world) = with_hypotheses();
    let mut value = draft(&world, &id(1));
    value.hypothesis_refs = vec![reference(3), reference(1), reference(2)];
    action_drafts::validate(&world, &value).unwrap();
    for unavailable in [ACTOR.to_owned(), PLACE.to_owned(), hypothesis(999)] {
        let mut bad = value.clone();
        bad.hypothesis_refs = vec![Reference::Record { id: unavailable }];
        let decoded = action_drafts::decode(&json!(bad).to_string()).unwrap();
        assert!(action_drafts::validate(&world, &decoded).is_err());
    }
    for actor in [false, true] {
        let mut bad = value.clone();
        if actor {
            bad.actor_refs = vec![reference(1)];
        } else {
            bad.target_refs = vec![reference(1)];
        }
        assert!(action_drafts::validate(&world, &bad).is_err());
    }
}

#[test]
fn links_bind_readiness_but_never_add_author_character_or_dramatic_context() {
    let (_directory, world) = with_hypotheses();
    let before = image(&world.root);
    for lens in ["author", "character", "dramatic-irony"] {
        let value = draft(&world, &id(1));
        let initial =
            action_drafts::review(&world, &review_request(&world, value.clone(), lens)).unwrap();
        let mut linked = value.clone();
        linked.hypothesis_refs = vec![reference(1), reference(2), reference(3)];
        let selected =
            action_drafts::review(&world, &review_request(&world, linked.clone(), lens)).unwrap();
        assert_eq!(selected["context"], initial["context"]);
        assert_eq!(selected["requiredAnswered"], initial["requiredAnswered"]);
        assert_eq!(
            selected["unansweredRequired"],
            initial["unansweredRequired"]
        );
        assert_ne!(selected["draftSha256"], initial["draftSha256"]);
        assert_ne!(selected["reviewSha256"], initial["reviewSha256"]);
        assert!(!selected["context"].to_string().contains("LINKED_PRIVATE"));
        if lens == "character" {
            assert!(
                !selected["context"]
                    .to_string()
                    .contains("UNSEEN_HYPOTHESIS_SUBJECT")
            );
        }
        linked.hypothesis_refs.reverse();
        let reordered =
            action_drafts::review(&world, &review_request(&world, linked, lens)).unwrap();
        assert_eq!(reordered["context"], selected["context"]);
        assert_ne!(reordered["reviewSha256"], selected["reviewSha256"]);
    }
    assert_eq!(image(&world.root), before);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn private_save_preserves_exact_links_without_changing_source_state_or_retrieval() {
    let (_directory, world) = with_hypotheses();
    let source_before = image(&world.root.join("story"));
    let state = runtime::state(&world, ACTOR, &horizon()).unwrap();
    let context = character::context(&world, ACTOR, &horizon(), 100_000).unwrap();
    let search =
        character::search(&world, ACTOR, "LINKED_PRIVATE_STATEMENT", &horizon(), 100).unwrap();
    let author_search = compiler::search(&world, "LINKED_PRIVATE_STATEMENT", 100, None).unwrap();
    assert!(search["items"].as_array().unwrap().is_empty());
    assert!(author_search["items"].as_array().unwrap().is_empty());
    let mut value = draft(&world, &id(1));
    value.hypothesis_refs = vec![reference(3), reference(1)];
    let save = action_store::save(&world, &request(&world, value.clone(), None)).unwrap();
    assert_eq!(save["writesPerformed"], true);
    let captured = source::load_world(&world.root, None).unwrap();
    assert_eq!(captured.fingerprint, world.fingerprint);
    assert_eq!(runtime::state(&captured, ACTOR, &horizon()).unwrap(), state);
    assert_eq!(
        character::context(&captured, ACTOR, &horizon(), 100_000).unwrap(),
        context
    );
    assert_eq!(
        character::search(
            &captured,
            ACTOR,
            "LINKED_PRIVATE_STATEMENT",
            &horizon(),
            100
        )
        .unwrap(),
        search
    );
    assert_eq!(
        compiler::search(&captured, "LINKED_PRIVATE_STATEMENT", 100, None).unwrap(),
        author_search
    );
    let detail = action_store::detail(&captured, &value.id, save["sourceSha256"].as_str()).unwrap();
    assert_eq!(detail["draft"], json!(value));
    assert_eq!(
        detail["draft"]["hypothesisRefs"],
        json!([reference(3), reference(1)])
    );
    assert_eq!(image(&world.root.join("story")), source_before);
}

#[test]
fn dangling_stored_links_remain_inspectable_but_cannot_be_reviewed_or_resaved() {
    let (_directory, world) = with_hypotheses();
    let mut value = draft(&world, &id(1));
    value.hypothesis_refs = vec![reference(1)];
    let saved = action_store::save(&world, &request(&world, value.clone(), None)).unwrap();
    let hash = saved["sourceSha256"].as_str().unwrap();
    fs::remove_file(world.root.join(&world.records[&hypothesis(1)].source_path)).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&current).valid);
    let before = image(&world.root);
    let detail = action_store::detail(&current, &value.id, Some(hash)).unwrap();
    assert_eq!(detail["draft"], json!(value));
    assert_eq!(
        action_store::list(&current).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let retained = action_drafts::decode(detail["source"].as_str().unwrap()).unwrap();
    assert!(
        action_drafts::review(
            &current,
            &review_request(&current, retained.clone(), "author")
        )
        .is_err()
    );
    assert!(
        action_store::preview(&current, &request(&current, retained.clone(), Some(hash))).is_err()
    );
    assert!(action_store::save(&current, &request(&current, retained, Some(hash))).is_err());
    assert_eq!(image(&world.root), before);
}

#[test]
fn original_private_history_retains_links_even_when_that_snapshot_lacks_the_target() {
    for format in ["sha1", "sha256"] {
        let (_directory, world) = with_hypotheses();
        let mut value = draft(&world, &id(1));
        value.hypothesis_refs = vec![reference(1)];
        action_store::save(&world, &request(&world, value.clone(), None)).unwrap();
        fs::remove_file(world.root.join(&world.records[&hypothesis(1)].source_path)).unwrap();
        git(
            &world.root,
            &["init", "--quiet", &format!("--object-format={format}")],
        );
        git(&world.root, &["add", "--", "story", "author"]);
        git(
            &world.root,
            &["commit", "--quiet", "-m", "Retained dangling private link"],
        );
        let revision = git(&world.root, &["rev-parse", "HEAD"]);
        let historical = source::load_world(&world.root, Some(&revision)).unwrap();
        assert!(!historical.records.contains_key(&hypothesis(1)));
        let raw = fs::read(path(&world, &value.id)).unwrap();
        fs::write(
            path(&world, &value.id),
            "Unrelated broken live private source",
        )
        .unwrap();
        fs::write(
            world.root.join(&world.world_record().unwrap().source_path),
            "Unrelated broken live canonical source",
        )
        .unwrap();
        let before = image(&world.root);
        let detail =
            action_store::detail(&historical, &value.id, Some(&action_drafts::sha256(&raw)))
                .unwrap();
        assert_eq!(detail["source"], String::from_utf8(raw).unwrap());
        assert_eq!(detail["draft"]["hypothesisRefs"], json!([reference(1)]));
        assert_eq!(detail["historical"], true);
        assert_eq!(detail["writesPerformed"], false);
        assert_eq!(image(&world.root), before);
    }
}

#[test]
fn links_obey_proposed_stale_and_terminal_phase_rules() {
    let (_directory, world) = with_hypotheses();
    let mut previous = draft(&world, &id(1));
    previous.questions[0].answer = Some(String::new());
    previous.phase = "proposed".into();
    previous.proposals.push(action_drafts::Proposal {
        id: PROPOSAL.into(), summary: "Exact inert proposal".into(), context_fingerprint: world.fingerprint.clone(), evidence: vec![],
        question_answers: vec![action_drafts::QuestionAnswer { question_id: previous.questions[0].id.clone(), answer: String::new() }],
        resolver: "human".into(), supersedes: None,
        consequences: consequences::decode(&json!({"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.update","entityId":ACTOR,"patch":"kind: character\nbody: Explicit proposed prose.\n"}]}).to_string()).unwrap(),
    });
    let mut changed = previous.clone();
    changed.hypothesis_refs = vec![reference(1)];
    let prior_hash = "a".repeat(64);
    assert!(
        action_drafts::validate_save(
            &world,
            &request(&world, changed.clone(), Some(&prior_hash)),
            Some(&previous)
        )
        .is_err()
    );
    changed.phase = "stale".into();
    action_drafts::validate_save(
        &world,
        &request(&world, changed.clone(), Some(&prior_hash)),
        Some(&previous),
    )
    .unwrap();
    assert_eq!(changed.proposals, previous.proposals);
    let mut terminal = previous.clone();
    terminal.phase = "cancelled".into();
    changed.phase = "cancelled".into();
    assert!(
        action_drafts::validate_save(
            &world,
            &request(&world, changed, Some(&prior_hash)),
            Some(&terminal)
        )
        .is_err()
    );
}

#[test]
fn changing_link_selection_invalidates_existing_readiness_confirmation() {
    let (_directory, world) = with_hypotheses();
    let previous = draft(&world, &id(1));
    let mut ready = previous.clone();
    ready.phase = "resolution-needed".into();
    ready.questions[0].answer = Some(String::new());
    ready.hypothesis_refs = vec![reference(1)];
    let mut save = request(&world, ready.clone(), Some(&"a".repeat(64)));
    save.review = Some(confirmation(&world, ready));
    action_drafts::validate_save(&world, &save, Some(&previous)).unwrap();
    save.draft.hypothesis_refs = vec![reference(2)];
    assert!(matches!(
        action_drafts::validate_save(&world, &save, Some(&previous)),
        Err(wedl::Error::Conflict(_))
    ));
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn cli(world: &World, arguments: &[&str]) -> (i32, Json) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--compact")
        .arg("--repo")
        .arg(&world.root)
        .args(arguments)
        .output()
        .unwrap();
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    (
        output.status.code().unwrap(),
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(bytes))),
    )
}
#[tokio::test]
async fn nonempty_links_round_trip_native_cli_and_http_without_becoming_context() {
    let (_directory, world) = with_hypotheses();
    let before = image(&world.root);
    let mut value = draft(&world, &id(1));
    value.hypothesis_refs = vec![reference(3), reference(1)];
    let input = review_request(&world, value.clone(), "character");
    let native = Service::new(world.clone())
        .execute(Operation::ActionDraftReview {
            request: input.clone(),
        })
        .unwrap();
    let input_file = world.root.parent().unwrap().join("link-review.json");
    fs::write(&input_file, json!(input).to_string()).unwrap();
    assert_eq!(
        cli(
            &world,
            &[
                "action-drafts",
                "review",
                "--file",
                input_file.to_str().unwrap()
            ]
        ),
        (0, native.clone())
    );
    let response = wedl::http::router(Service::new(world.clone()))
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/action-drafts/review?request={}",
                    encode(&json!(input).to_string())
                ))
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let result: Json =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(result, native);
    assert!(
        !result["result"]["context"]
            .to_string()
            .contains("LINKED_PRIVATE")
    );
    assert_eq!(image(&world.root), before);
    let save_file = world.root.parent().unwrap().join("link-save.json");
    fs::write(
        &save_file,
        json!(request(&world, value.clone(), None)).to_string(),
    )
    .unwrap();
    let (status, saved) = cli(
        &world,
        &[
            "action-drafts",
            "save",
            "--file",
            save_file.to_str().unwrap(),
        ],
    );
    assert_eq!(status, 0, "{saved}");
    let native_detail = Service::new(world.clone())
        .execute(Operation::ActionDraftDetail {
            id: value.id.clone(),
            expected_sha256: saved["result"]["sourceSha256"].as_str().map(str::to_owned),
        })
        .unwrap();
    assert_eq!(
        native_detail["result"]["draft"]["hypothesisRefs"],
        json!(value.hypothesis_refs)
    );
    assert_eq!(
        cli(&world, &["action-drafts", "detail", &value.id]),
        (0, native_detail.clone())
    );
    let response = wedl::http::router(Service::new(world))
        .oneshot(
            Request::builder()
                .uri(format!("/api/action-drafts/detail?id={}", value.id))
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Json>(&response.into_body().collect().await.unwrap().to_bytes())
            .unwrap(),
        native_detail
    );
}
