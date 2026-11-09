use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};
use wedl::{
    Error,
    consequence_semantics::Time,
    consequences::{Operation, Plan},
    model::World,
    retcon_impact::{Reference, Request},
    retcon_reviews::{
        self as reviews, Decision, DetailRequest, Disposition, Document, PreviewRequest,
        QueueRequest, SaveRequest,
    },
    source, validation,
};
#[path = "support/actions.rs"]
mod support;
use support::{ACTOR, PLACE, TICK};

const ID: &str = "review_00000000000000000000000001";
const SEGMENT: &str = "segment_00000000000000000000000001";
const WORK: &str = "work_00000000000000000000000001";
const UNRELATED_SEGMENT: &str = "segment_00000000000000000000000002";
const FUTURE_SEGMENT: &str = "segment_00000000000000000000000003";

fn manuscript_source(kind: &str, id: &str, fields: &str, body: &str) -> String {
    format!(
        "---\nschema: wedl-author/v1\nkind: {kind}\nid: {id}\ntitle: Private source\n{fields}---\n{body}"
    )
}
fn write_manuscript(world: &World, kind: &str, id: &str, fields: &str, body: &str) -> String {
    let path = wedl::manuscript::document_path(kind, id).unwrap();
    let source = manuscript_source(kind, id, fields, body);
    wedl::manuscript::parse_document(source.as_bytes(), &path).unwrap();
    fs::create_dir_all(world.root.join(&path).parent().unwrap()).unwrap();
    fs::write(world.root.join(&path), source).unwrap();
    path
}
fn manuscript_fixture(section: bool) -> (tempfile::TempDir, World, PreviewRequest) {
    let (directory, world, mut request) = fixture();
    let target = if section {
        format!(
            "{{kind: section, owner_id: {ACTOR}, id: {}}}",
            support::SECTION
        )
    } else {
        format!("{{kind: record, id: {PLACE}}}")
    };
    write_manuscript(
        &world,
        "manuscript-segment",
        SEGMENT,
        &format!("source_links: [{target}]\n"),
        "Selected manuscript\r\nExact trailing  ",
    );
    write_manuscript(
        &world,
        "work",
        WORK,
        &format!(
            "duplicate_policy: allow-explicit-reuse\nchapters:\n - id: chapter_00000000000000000000000001\n   title: First\n   segments: [{{kind: author-record, id: {SEGMENT}}}, {{kind: author-record, id: {SEGMENT}}}]\n"
        ),
        "Private work notes",
    );
    write_manuscript(
        &world,
        "manuscript-segment",
        UNRELATED_SEGMENT,
        "source_links: []\n",
        "Unrelated source",
    );
    write_manuscript(
        &world,
        "manuscript-segment",
        FUTURE_SEGMENT,
        &format!(
            "source_links: [{{kind: record, id: {PLACE}}}]\nplacement: {{timeline: main, tick: {}, order: 0}}\n",
            TICK + 1
        ),
        "Future source",
    );
    git(
        &world.root,
        &["add", "--", "author/segments", "author/works"],
    );
    request.base_revision = commit(&world.root, "original manuscript source");
    if section {
        request.impact_request.target = Reference::Record { id: ACTOR.into() };
    }
    (directory, world, request)
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "--", "story"]);
    git(root, &["commit", "-q", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}
fn fixture() -> (tempfile::TempDir, World, PreviewRequest) {
    let (directory, world) = support::fixture();
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["config", "user.name", "Review tests"]);
    git(
        &world.root,
        &["config", "user.email", "review@example.invalid"],
    );
    let head = commit(&world.root, "original source");
    let request = PreviewRequest {
        base_revision: head,
        plan: Plan {
            expected_fingerprint: world.fingerprint.clone(),
            operations: vec![Operation::Update {
                entity_id: PLACE.into(),
                patch: "kind: location\nbody: Revised landing.\n".into(),
            }],
            analysis: None,
        },
        impact_request: Request {
            target: Reference::Record { id: PLACE.into() },
            at: Time {
                timeline: "main".into(),
                tick: TICK.to_string(),
                order: "1".into(),
            },
            depth: 8,
            max_items: 1000,
        },
    };
    (directory, world, request)
}
fn document(world: &World, request: &PreviewRequest) -> Document {
    let result = reviews::preview(&world.root, request).unwrap();
    Document {
        id: ID.into(),
        title: "Original dependency review".into(),
        analysis: serde_json::from_value(result["analysis"].clone()).unwrap(),
        decisions: vec![],
        body_markdown: "\r\nPrivate review 🌲\r\n\nTrailing space  ".into(),
    }
}
fn save_request(world: &World, document: Document, expected_sha256: Option<String>) -> SaveRequest {
    SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256,
        document,
    }
}
fn detail(world: &World, revision: Option<String>) -> Json {
    reviews::detail(
        &world.root,
        &DetailRequest {
            id: ID.into(),
            revision,
            expected_sha256: None,
        },
    )
    .unwrap()
}
fn create(world: &World, request: &PreviewRequest) -> (Document, Json) {
    let document = document(world, request);
    let saved = reviews::save(world, &save_request(world, document.clone(), None)).unwrap();
    (document, saved)
}
fn changed(
    world: &World,
    id: &str,
    edit: impl FnOnce(&mut serde_yaml::Value, &mut String),
) -> World {
    let record = &world.records[id];
    let mut fields = record.frontmatter.clone();
    let mut body = record.body.clone();
    edit(&mut fields, &mut body);
    fs::write(
        world.root.join(&record.source_path),
        format!(
            "---\n{}---\n{body}",
            serde_yaml::to_string(&fields).unwrap()
        ),
    )
    .unwrap();
    let result = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&result).valid);
    result
}
fn rehash(domain: &[u8], value: &Json) -> String {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(serde_json::to_vec(value).unwrap());
    hex::encode(h.finalize())
}
fn private_path(world: &World) -> std::path::PathBuf {
    world.root.join(format!("author/retcon-reviews/{ID}.json"))
}
fn raw_document(world: &World, document: &Document) {
    fs::write(private_path(world), serde_json::to_vec(document).unwrap()).unwrap();
}

#[test]
fn manuscript_original_and_historical_review_keep_exact_source_citations() {
    let (_dir, world, request) = manuscript_fixture(false);
    let (document, _) = create(&world, &request);
    let items = document.analysis.report["items"].as_array().unwrap();
    let private: Vec<_> = items
        .iter()
        .filter(|item| item["owner"]["kind"] == "author-record")
        .collect();
    assert_eq!(private.len(), 6);
    for item in &private {
        let path = item["citation"]["sourcePath"].as_str().unwrap();
        let oid = git(
            &world.root,
            &["rev-parse", &format!("{}:{path}", request.base_revision)],
        );
        assert_eq!(
            item["citation"]["provenance"],
            json!({"kind":"source","revision":request.base_revision,"blobOid":oid})
        );
    }
    let wire = serde_json::to_string(&save_request(&world, document.clone(), None)).unwrap();
    assert!(reviews::decode_save(&wire).is_ok());
    git(
        &world.root,
        &["add", "--", &format!("author/retcon-reviews/{ID}.json")],
    );
    let review_revision = commit(&world.root, "retained review");
    fs::write(
        world
            .root
            .join(wedl::manuscript::document_path("manuscript-segment", SEGMENT).unwrap()),
        "broken private source",
    )
    .unwrap();
    let historical = detail(&world, Some(review_revision.clone()));
    assert_eq!(historical["revision"], review_revision);
    assert_eq!(
        historical["document"]["analysis"],
        serde_json::to_value(document.analysis).unwrap()
    );
    assert_eq!(historical["currentness"]["status"], "unknown");
    assert_eq!(
        historical["document"]["bodyMarkdown"],
        document.body_markdown
    );
}

#[test]
fn unrelated_and_future_manuscript_edits_stay_current_but_new_incoming_links_stale() {
    let (_dir, world, request) = manuscript_fixture(false);
    let (document, _) = create(&world, &request);
    write_manuscript(
        &world,
        "manuscript-segment",
        UNRELATED_SEGMENT,
        "source_links: []\n",
        &format!("Opaque prose mentioning {PLACE}"),
    );
    write_manuscript(
        &world,
        "manuscript-segment",
        FUTURE_SEGMENT,
        &format!(
            "source_links: [{{kind: record, id: {PLACE}}}]\nplacement: {{timeline: main, tick: {}, order: 0}}\n",
            TICK + 1
        ),
        "Changed future manuscript",
    );
    assert_eq!(detail(&world, None)["currentness"]["status"], "current");
    assert!(
        !document
            .analysis
            .witness
            .to_string()
            .contains("fingerprint")
    );
    let incoming = write_manuscript(
        &world,
        "manuscript-segment",
        "segment_00000000000000000000000004",
        &format!("source_links: [{{kind: record, id: {PLACE}}}]\n"),
        "New explicit incoming source",
    );
    assert_eq!(detail(&world, None)["currentness"]["status"], "stale");
    fs::remove_file(world.root.join(incoming)).unwrap();
    assert_eq!(detail(&world, None)["currentness"]["status"], "current");
    write_manuscript(
        &world,
        "work",
        "work_00000000000000000000000002",
        &format!(
            "duplicate_policy: reject\nchapters:\n - id: chapter_00000000000000000000000001\n   title: Newly collected\n   segments: [{{kind: author-record, id: {SEGMENT}}}]\n"
        ),
        "New incoming work",
    );
    assert_eq!(detail(&world, None)["currentness"]["status"], "stale");
}

#[test]
fn unchanged_original_manuscript_links_survive_deleted_canonical_targets_as_stale_evidence() {
    let (_dir, world, request) = manuscript_fixture(true);
    create(&world, &request);
    let path = world
        .root
        .join(wedl::manuscript::document_path("manuscript-segment", SEGMENT).unwrap());
    let original = fs::read(&path).unwrap();
    let changed_world = changed(&world, ACTOR, |fields, _| {
        fields.as_mapping_mut().unwrap().remove("descriptions");
    });
    assert_eq!(
        detail(&changed_world, None)["currentness"]["status"],
        "stale"
    );
    let mut revised = original.clone();
    revised.extend_from_slice(b"Changed private prose");
    fs::write(&path, revised).unwrap();
    assert_eq!(
        detail(&changed_world, None)["currentness"]["status"],
        "unknown"
    );
    fs::write(&path, original).unwrap();
    assert_eq!(
        detail(&changed_world, None)["currentness"]["status"],
        "stale"
    );
}

#[test]
fn newly_invalid_private_inputs_make_currentness_unknown_without_replacing_original() {
    let (_dir, world, request) = manuscript_fixture(false);
    let (document, _) = create(&world, &request);
    write_manuscript(
        &world,
        "manuscript-segment",
        UNRELATED_SEGMENT,
        "source_links: [{kind: record, id: loc_00000000000000000000000099}]\n",
        "Changed invalid source",
    );
    let observed = detail(&world, None);
    assert_eq!(observed["currentness"]["status"], "unknown");
    assert_eq!(
        observed["document"]["analysis"],
        serde_json::to_value(document.analysis).unwrap()
    );
}

#[test]
fn committed_manuscript_change_and_revert_remains_stale() {
    let (_dir, world, request) = manuscript_fixture(false);
    create(&world, &request);
    let relative = wedl::manuscript::document_path("manuscript-segment", SEGMENT).unwrap();
    let path = world.root.join(&relative);
    let original = fs::read(&path).unwrap();
    let mut revised = original.clone();
    revised.extend_from_slice(b"Committed relevant revision");
    fs::write(&path, revised).unwrap();
    git(&world.root, &["add", "--", &relative]);
    commit(&world.root, "private change");
    fs::write(&path, original).unwrap();
    git(&world.root, &["add", "--", &relative]);
    commit(&world.root, "private revert");
    assert_eq!(detail(&world, None)["currentness"]["status"], "stale");
}

#[test]
fn review_preview_requires_current_private_source_to_match_the_committed_base() {
    let (_dir, world, mut request) = fixture();
    write_manuscript(
        &world,
        "manuscript-segment",
        SEGMENT,
        &format!("source_links: [{{kind: record, id: {PLACE}}}]\n"),
        "Uncommitted private source",
    );
    assert!(matches!(
        reviews::preview(&world.root, &request),
        Err(Error::Conflict(_))
    ));
    git(&world.root, &["add", "--", "author/segments"]);
    request.base_revision = commit(&world.root, "committed private source");
    assert!(reviews::preview(&world.root, &request).is_ok());
}

#[test]
fn impact_preview_selects_same_revision_private_source_without_live_fallback() {
    let (_dir, world, request) = manuscript_fixture(false);
    let preview = wedl::retcon_impact::PreviewRequest {
        plan: request.plan.clone(),
        impact_request: request.impact_request.clone(),
    };
    let historical = source::load_world(&world.root, Some(&request.base_revision)).unwrap();
    write_manuscript(
        &world,
        "manuscript-segment",
        SEGMENT,
        "source_links: []\n",
        "Changed current manuscript",
    );
    let current = wedl::retcon_impact::preview(&world, &preview).unwrap();
    assert!(
        !current["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["owner"]["kind"] == "author-record")
    );
    let selected = wedl::retcon_impact::preview(&historical, &preview).unwrap();
    assert!(
        selected["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["owner"]["id"] == SEGMENT
                && item["citation"]["provenance"]["revision"] == request.base_revision)
    );
    fs::remove_dir_all(world.root.join("author/segments")).unwrap();
    assert_eq!(
        wedl::retcon_impact::preview(&historical, &preview).unwrap(),
        selected
    );
}

#[test]
fn private_candidate_citation_codec_requires_original_provenance_and_safe_paths() {
    let (_dir, world, request) = manuscript_fixture(false);
    let document = document(&world, &request);
    let source = serde_json::to_value(save_request(&world, document, None)).unwrap();
    assert!(reviews::decode_save(&source.to_string()).is_ok());
    let mut forged = source.clone();
    let item = forged["document"]["analysis"]["report"]["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["snapshot"] == "candidate" && item["owner"]["kind"] == "author-record")
        .unwrap();
    item["citation"]["provenance"] = json!({"kind":"candidate","baseRevision":request.base_revision,"planSha256":source["document"]["analysis"]["planSha256"]});
    assert!(reviews::decode_save(&forged.to_string()).is_err());
    let mut forged = source;
    let item = forged["document"]["analysis"]["report"]["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["owner"]["kind"] == "author-record")
        .unwrap();
    item["citation"]["sourcePath"] = "author/segments/../outside.md".into();
    assert!(reviews::decode_save(&forged.to_string()).is_err());
}

#[test]
fn preview_is_pure_and_pins_exact_original_candidate_and_large_clock() {
    let (_dir, world, request) = fixture();
    let original = fs::read(world.root.join(&world.records[PLACE].source_path)).unwrap();
    let report = reviews::preview(&world.root, &request).unwrap();
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(report["canApply"], false);
    assert_eq!(report["comparedRevision"], request.base_revision);
    assert_eq!(report["comparedFingerprint"], world.fingerprint);
    let analysis = &report["analysis"];
    assert_eq!(analysis["baseRevision"], request.base_revision);
    assert_eq!(analysis["impactRequest"]["at"]["tick"], TICK.to_string());
    assert!(analysis["report"]["items"].as_array().unwrap().len() >= 2);
    assert_eq!(
        analysis["report"]["base"]["revision"],
        request.base_revision
    );
    assert_eq!(
        analysis["report"]["candidate"]["planSha256"],
        analysis["planSha256"]
    );
    assert!(!world.root.join("author/retcon-reviews").exists());
    assert_eq!(
        original,
        fs::read(world.root.join(&world.records[PLACE].source_path)).unwrap()
    );
    assert_eq!(
        git(&world.root, &["rev-parse", "HEAD"]),
        request.base_revision
    );
    assert_eq!(report, reviews::preview(&world.root, &request).unwrap());
}

#[test]
fn private_lifecycle_preserves_exact_body_and_ordered_dispositions() {
    let (_dir, world, request) = fixture();
    let mut document = document(&world, &request);
    let analysis = document.analysis.clone();
    let count = document.analysis.item_digests.len();
    let preview =
        reviews::save_preview(&world, &save_request(&world, document.clone(), None)).unwrap();
    assert_eq!(preview["canSave"], true);
    assert_eq!(preview["beforeSource"], Json::Null);
    assert!(!private_path(&world).exists());
    let saved = reviews::save(&world, &save_request(&world, document.clone(), None)).unwrap();
    assert_eq!(saved["writesPerformed"], true);
    assert_eq!(saved["gitCommit"], Json::Null);
    let first = detail(&world, None);
    assert_eq!(first["pendingCount"], count);
    assert_eq!(first["document"]["bodyMarkdown"], document.body_markdown);
    assert_eq!(first["currentness"]["status"], "current");
    assert_eq!(first["source"], preview["afterSource"]);
    document.decisions.push(Decision {
        item_digest: document.analysis.item_digests[0].clone(),
        disposition: Disposition::IntentionallyRetained,
        note: "Deliberate historical ambiguity.\r\n".into(),
    });
    document.title = "Read with annotations".into();
    document.body_markdown.push_str("Updated");
    let updated = reviews::save(
        &world,
        &save_request(
            &world,
            document.clone(),
            saved["sourceSha256"].as_str().map(str::to_owned),
        ),
    )
    .unwrap();
    assert_eq!(updated["writesPerformed"], true);
    let actual = detail(&world, None);
    assert_eq!(
        actual["document"]["analysis"],
        serde_json::to_value(analysis).unwrap()
    );
    assert_eq!(actual["pendingCount"], count - 1);
    assert_eq!(
        actual["document"]["decisions"][0]["note"],
        "Deliberate historical ambiguity.\r\n"
    );
    let queued = reviews::queue(
        &world.root,
        &QueueRequest {
            revision: None,
            max_items: 10,
        },
    )
    .unwrap();
    assert_eq!(queued["items"][0]["sourceSha256"], actual["sourceSha256"]);
    assert_eq!(queued["items"][0]["pendingCount"], count - 1);
    assert_eq!(queued["comparedRevision"], request.base_revision);
    assert_eq!(
        git(&world.root, &["rev-parse", "HEAD"]),
        request.base_revision
    );
}

#[test]
fn private_cas_rejects_old_pins_and_current_pin_identical_retry_does_not_write() {
    let (_dir, world, request) = fixture();
    let (document, saved) = create(&world, &request);
    let metadata = fs::metadata(private_path(&world))
        .unwrap()
        .modified()
        .unwrap();
    let exact = save_request(
        &world,
        document.clone(),
        saved["sourceSha256"].as_str().map(str::to_owned),
    );
    let retry = reviews::save(&world, &exact).unwrap();
    assert_eq!(retry["changed"], false);
    assert_eq!(retry["writesPerformed"], false);
    assert_eq!(
        metadata,
        fs::metadata(private_path(&world))
            .unwrap()
            .modified()
            .unwrap()
    );
    assert!(matches!(
        reviews::save(&world, &save_request(&world, document.clone(), None)),
        Err(Error::Conflict(_))
    ));
    let mut stale = exact.clone();
    stale.expected_sha256 = Some("0".repeat(64));
    assert!(matches!(
        reviews::save(&world, &stale),
        Err(Error::Conflict(_))
    ));
    stale = exact;
    stale.expected_fingerprint = "0".repeat(64);
    assert!(matches!(
        reviews::save(&world, &stale),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        reviews::detail(
            &world.root,
            &DetailRequest {
                id: ID.into(),
                revision: None,
                expected_sha256: Some("0".repeat(64))
            }
        ),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn preview_requires_current_committed_source_but_note_save_pin_is_separate() {
    let (_dir, world, request) = fixture();
    let (mut document, saved) = create(&world, &request);
    let changed = changed(&world, ACTOR, |_, body| {
        body.push_str("Unrelated current prose.\n")
    });
    assert!(matches!(
        reviews::preview(&world.root, &request),
        Err(Error::Conflict(_))
    ));
    document
        .body_markdown
        .push_str("New note on an old capture.");
    assert!(matches!(
        reviews::save(
            &world,
            &save_request(
                &world,
                document.clone(),
                saved["sourceSha256"].as_str().map(str::to_owned)
            )
        ),
        Err(Error::Conflict(_))
    ));
    let result = reviews::save(
        &changed,
        &save_request(
            &changed,
            document,
            saved["sourceSha256"].as_str().map(str::to_owned),
        ),
    )
    .unwrap();
    assert_eq!(result["fingerprint"], changed.fingerprint);
    assert_eq!(
        detail(&changed, None)["document"]["analysis"]["baseRevision"],
        request.base_revision
    );
    commit(&world.root, "unrelated later source");
    let mut rebound = request.clone();
    rebound.plan.expected_fingerprint = changed.fingerprint;
    assert!(matches!(
        reviews::preview(&world.root, &rebound),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn relevant_uncommitted_changes_stale_and_uncommitted_reverts_have_no_retained_history() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    let original = world.records[PLACE].raw_bytes.clone();
    let changed = changed(&world, PLACE, |_, body| {
        body.push_str("Current relevant edit.\n")
    });
    let current = detail(&changed, None);
    assert_eq!(current["currentness"]["status"], "stale");
    assert_eq!(current["comparedRevision"], request.base_revision);
    assert_eq!(current["comparedFingerprint"], changed.fingerprint);
    fs::write(world.root.join(&world.records[PLACE].source_path), original).unwrap();
    assert_eq!(detail(&world, None)["currentness"]["status"], "current");
}

#[test]
fn committed_change_then_revert_never_revives_review() {
    let (_dir, world, request) = fixture();
    let (document, _) = create(&world, &request);
    changed(&world, PLACE, |_, body| {
        body.push_str("A committed relevant change.\n")
    });
    commit(&world.root, "relevant change");
    fs::write(
        world.root.join(&world.records[PLACE].source_path),
        &world.records[PLACE].raw_bytes,
    )
    .unwrap();
    let reverted = commit(&world.root, "revert source bytes");
    let result = detail(&world, None);
    assert_eq!(result["currentness"]["status"], "stale");
    assert_eq!(result["comparedRevision"], reverted);
    assert_eq!(
        result["document"]["analysis"],
        serde_json::to_value(document.analysis).unwrap()
    );
}

#[test]
fn unrelated_committed_and_hidden_future_changes_do_not_stale() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    let unrelated = changed(&world, ACTOR, |_, body| {
        body.push_str("Unrelated actor prose.\n")
    });
    commit(&world.root, "unrelated source");
    assert_eq!(detail(&unrelated, None)["currentness"]["status"], "current");
    let hidden = "event_00000000000000000000000007";
    let future = "event_00000000000000000000000008";
    for (id, fields) in [
        (
            hidden,
            format!("audience: [gm]\ntime: {{tick: {}}}\n", TICK - 2),
        ),
        (future, format!("time: {{tick: {}}}\n", TICK + 2)),
    ] {
        fs::write(world.root.join(format!("story/{id}.md")), format!("---\nschema: wedl/v1\nkind: event\nid: {id}\ntitle: Excluded\n{fields}location: {PLACE}\neffects: []\n---\nExcluded source\n")).unwrap();
    }
    let live = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&live).valid);
    commit(&world.root, "hidden and future originals");
    assert_eq!(detail(&live, None)["currentness"]["status"], "current");
}

#[test]
fn newly_incoming_dependency_stales_even_when_original_inputs_are_unchanged() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    let id = "event_00000000000000000000000002";
    fs::write(world.root.join(format!("story/{id}.md")), format!("---\nschema: wedl/v1\nkind: event\nid: {id}\ntitle: Newly incoming\ntime: {{tick: {TICK}}}\nlocation: {PLACE}\neffects: []\n---\nNew incoming dependency.\n")).unwrap();
    let live = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&live).valid);
    assert_eq!(detail(&live, None)["currentness"]["status"], "stale");
}

#[test]
fn historical_private_bytes_are_exact_and_independent_of_live_store_and_source() {
    let (_dir, world, request) = fixture();
    let (document, saved) = create(&world, &request);
    let source_bytes = fs::read(private_path(&world)).unwrap();
    git(&world.root, &["add", "--", "author/retcon-reviews"]);
    git(&world.root, &["commit", "-q", "-m", "private review"]);
    let revision = git(&world.root, &["rev-parse", "HEAD"]);
    let historical = detail(&world, Some(revision.clone()));
    assert_eq!(
        historical["source"],
        String::from_utf8(source_bytes).unwrap()
    );
    assert_eq!(historical["sourceSha256"], saved["sourceSha256"]);
    assert_eq!(
        historical["document"],
        serde_json::to_value(document).unwrap()
    );
    assert_eq!(historical["revision"], revision);
    assert!(historical["blobOid"].as_str().is_some());
    assert_eq!(historical["currentness"]["status"], "current");
    fs::write(private_path(&world), "invalid private live data").unwrap();
    fs::write(
        world.root.join(&world.records[PLACE].source_path),
        "invalid canonical live data",
    )
    .unwrap();
    let read = detail(&world, Some(revision.clone()));
    assert_eq!(read["source"], historical["source"]);
    assert_eq!(read["document"], historical["document"]);
    assert_eq!(read["currentness"]["status"], "unknown");
    assert_eq!(read["comparedFingerprint"], Json::Null);
    let queue = reviews::queue(
        &world.root,
        &QueueRequest {
            revision: Some(revision),
            max_items: 10,
        },
    )
    .unwrap();
    assert_eq!(queue["items"].as_array().unwrap().len(), 1);
    assert!(
        reviews::detail(
            &world.root,
            &DetailRequest {
                revision: None,
                id: ID.into(),
                expected_sha256: None
            }
        )
        .is_err()
    );
}

#[test]
fn rehashed_forgery_is_rejected_and_original_analysis_cannot_change_in_place() {
    let (_dir, world, request) = fixture();
    let (mut document, saved) = create(&world, &request);
    document.analysis.report["notice"] = "Forged but rehashed original report".into();
    document.analysis.report_sha256 =
        rehash(b"wedl-retcon-review-report\0", &document.analysis.report);
    let request = save_request(
        &world,
        document.clone(),
        saved["sourceSha256"].as_str().map(str::to_owned),
    );
    assert!(
        reviews::save_preview(&world, &request)
            .unwrap_err()
            .to_string()
            .contains("immutable")
    );
    document.id = "review_00000000000000000000000002".into();
    assert!(
        reviews::save_preview(&world, &save_request(&world, document.clone(), None))
            .unwrap_err()
            .to_string()
            .contains("REVIEW-BINDING-001")
    );
    document.id = ID.into();
    raw_document(&world, &document);
    assert!(
        reviews::detail(
            &world.root,
            &DetailRequest {
                id: ID.into(),
                revision: None,
                expected_sha256: None
            }
        )
        .unwrap_err()
        .to_string()
        .contains("REVIEW-BINDING-001")
    );
}

#[test]
fn unsupported_witness_is_opaque_unknown_but_nonwitness_bindings_still_verify() {
    let (_dir, world, request) = fixture();
    let (mut document, _) = create(&world, &request);
    document.analysis.witness =
        json!({"version":"unknown-format/v1","opaque":{"old":["source","positions"]}});
    document.analysis.witness_sha256 =
        rehash(b"wedl-retcon-review-witness\0", &document.analysis.witness);
    raw_document(&world, &document);
    let read = detail(&world, None);
    assert_eq!(read["currentness"]["status"], "unknown");
    assert_eq!(
        read["document"]["analysis"]["witness"],
        document.analysis.witness
    );
    document.body_markdown.push_str("Note");
    assert!(
        reviews::save(
            &world,
            &save_request(
                &world,
                document.clone(),
                read["sourceSha256"].as_str().map(str::to_owned)
            )
        )
        .is_err()
    );
    document.analysis.report["notice"] = "Tampered unrelated field".into();
    document.analysis.report_sha256 =
        rehash(b"wedl-retcon-review-report\0", &document.analysis.report);
    raw_document(&world, &document);
    assert!(
        reviews::detail(
            &world.root,
            &DetailRequest {
                id: ID.into(),
                revision: None,
                expected_sha256: None
            }
        )
        .unwrap_err()
        .to_string()
        .contains("REVIEW-BINDING-001")
    );
}

#[test]
fn closed_codecs_require_explicit_null_options_and_reject_duplicates_and_bad_order() {
    assert!(reviews::decode_queue(r#"{"maxItems":10}"#).is_err());
    assert!(reviews::decode_queue(r#"{"revision":null,"maxItems":10,"maxItems":20}"#).is_err());
    assert!(reviews::decode_queue(r#"{"revision":null,"maxItems":10,"extra":0}"#).is_err());
    assert!(reviews::decode_queue(r#"{"revision":"HEAD","maxItems":10}"#).is_err());
    assert!(reviews::decode_queue(r#"{"revision":null,"maxItems":0}"#).is_err());
    assert!(reviews::decode_queue(r#"{"revision":null,"maxItems":1001}"#).is_err());
    assert!(reviews::decode_detail(&json!({"id":ID,"revision":null}).to_string()).is_err());
    let (_dir, world, request) = fixture();
    let document = document(&world, &request);
    let saved = save_request(&world, document.clone(), None);
    assert_eq!(
        reviews::decode_save(&serde_json::to_string(&saved).unwrap()).unwrap(),
        saved
    );
    let mut value = serde_json::to_value(&saved).unwrap();
    value.as_object_mut().unwrap().remove("expectedSha256");
    assert!(reviews::decode_save(&value.to_string()).is_err());
    for digests in [
        vec![document.analysis.item_digests[0].clone(); 2],
        vec![
            document.analysis.item_digests[1].clone(),
            document.analysis.item_digests[0].clone(),
        ],
        vec!["0".repeat(64)],
    ] {
        let mut document = document.clone();
        document.decisions = digests
            .into_iter()
            .map(|item_digest| Decision {
                item_digest,
                disposition: Disposition::Reviewed,
                note: "".into(),
            })
            .collect();
        assert!(
            reviews::decode_save(
                &serde_json::to_string(&save_request(&world, document, None)).unwrap()
            )
            .is_err()
        );
    }
    let raw = serde_json::to_string(&saved)
        .unwrap()
        .replace("\"notice\":", "\"notice\":\"duplicate\",\"notice\":");
    assert!(reviews::decode_save(&raw).is_err());
    let mut value = serde_json::to_value(&saved).unwrap();
    value["document"]["bodyMarkdown"] = "x".repeat(128 * 1024 + 1).into();
    assert!(reviews::decode_save(&value.to_string()).is_err());
    assert!(!world.root.join("author/retcon-reviews").exists());
}

#[test]
fn large_source_numbers_remain_literal_value_raw_strings_through_review_json() {
    let (_dir, world, mut request) = fixture();
    let owner = world.world_record().unwrap().id.clone();
    let world = changed(&world, &owner, |fields, _| {
        fields["state_keys"] =
            serde_yaml::from_str("location: {wide: {type: integer}, mixed: {type: number}}\n")
                .unwrap();
    });
    let world = changed(&world, PLACE, |fields, _| {
        fields["initial_state"] =
            serde_yaml::from_str("wide: 9007199254740993\nmixed: 1.0\n").unwrap();
    });
    request.base_revision = commit(&world.root, "exact native numeric values");
    request.plan.expected_fingerprint = world.fingerprint.clone();
    let doc = document(&world, &request);
    let input = doc.analysis.witness["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["ref"]["id"] == PLACE)
        .unwrap();
    let section = input["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["field"] == "initial_state")
        .unwrap();
    assert_eq!(
        section["valueRaw"],
        "{\"mixed\":1.0,\"wide\":9007199254740993}"
    );
    let encoded = serde_json::to_string(&save_request(&world, doc, None)).unwrap();
    assert!(reviews::decode_save(&encoded).is_ok());
}

#[test]
fn missing_head_and_non_first_parent_base_are_unknown() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    git(
        &world.root,
        &["symbolic-ref", "HEAD", "refs/heads/unborn-comparison"],
    );
    let unavailable = detail(&world, None);
    assert_eq!(unavailable["comparedRevision"], Json::Null);
    assert_eq!(unavailable["currentness"]["status"], "unknown");
    let tree = git(
        &world.root,
        &["rev-parse", &format!("{}^{{tree}}", request.base_revision)],
    );
    let other = git(
        &world.root,
        &["commit-tree", &tree, "-m", "independent root"],
    );
    let merge = git(
        &world.root,
        &[
            "commit-tree",
            &tree,
            "-p",
            &other,
            "-p",
            &request.base_revision,
            "-m",
            "base is second parent only",
        ],
    );
    git(&world.root, &["update-ref", "HEAD", &merge]);
    let unavailable = detail(&world, None);
    assert_eq!(unavailable["comparedRevision"], merge);
    assert_eq!(unavailable["currentness"]["status"], "unknown");
}

#[test]
fn history_limit_is_complete_before_a_stale_prefix_can_be_trusted() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    changed(&world, PLACE, |_, body| {
        body.push_str("Relevant current change.\n")
    });
    let tree = git(
        &world.root,
        &["rev-parse", &format!("{}^{{tree}}", request.base_revision)],
    );
    let mut previous = request.base_revision.clone();
    for i in 0..257 {
        previous = git(
            &world.root,
            &[
                "commit-tree",
                &tree,
                "-p",
                &previous,
                "-m",
                &format!("bounded history {i}"),
            ],
        );
    }
    git(&world.root, &["update-ref", "HEAD", &previous]);
    let result = detail(&world, None);
    assert_eq!(result["currentness"]["status"], "unknown");
    assert_eq!(result["comparedRevision"], previous);
}

#[test]
fn malformed_live_source_remains_readable_as_unknown_and_oversize_files_are_bounded() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    let file = fs::File::create(world.root.join("story/oversized.md")).unwrap();
    file.set_len(8 * 1024 * 1024 + 1).unwrap();
    let result = detail(&world, None);
    assert_eq!(result["currentness"]["status"], "unknown");
    assert_eq!(result["comparedFingerprint"], Json::Null);
    assert_eq!(
        result["document"]["analysis"]["baseRevision"],
        request.base_revision
    );
    fs::remove_file(world.root.join("story/oversized.md")).unwrap();
    fs::write(
        world.root.join(&world.records[PLACE].source_path),
        "---\nunterminated: [\n",
    )
    .unwrap();
    assert_eq!(detail(&world, None)["currentness"]["status"], "unknown");
    assert!(reviews::preview(&world.root, &request).is_err());
}

#[test]
fn queue_limit_refuses_complete_result_and_missing_historical_store_is_empty() {
    let (_dir, world, request) = fixture();
    let (mut document, _) = create(&world, &request);
    let empty = reviews::queue(
        &world.root,
        &QueueRequest {
            revision: Some(request.base_revision.clone()),
            max_items: 1,
        },
    )
    .unwrap();
    assert_eq!(empty["items"], json!([]));
    document.id = "review_00000000000000000000000002".into();
    reviews::save(&world, &save_request(&world, document, None)).unwrap();
    assert!(
        reviews::queue(
            &world.root,
            &QueueRequest {
                revision: None,
                max_items: 1
            }
        )
        .is_err()
    );
    let all = reviews::queue(
        &world.root,
        &QueueRequest {
            revision: None,
            max_items: 2,
        },
    )
    .unwrap();
    assert_eq!(all["items"].as_array().unwrap().len(), 2);
    for item in all["items"].as_array().unwrap() {
        assert_eq!(
            item["currentness"]["comparedRevision"],
            all["comparedRevision"]
        );
        assert_eq!(
            item["currentness"]["comparedFingerprint"],
            all["comparedFingerprint"]
        );
    }
}

#[cfg(unix)]
#[test]
fn symlinked_private_parents_files_and_canonical_sources_are_refused_without_writes() {
    use std::os::unix::fs::symlink;
    let (directory, world, request) = fixture();
    let document = document(&world, &request);
    let outside = directory.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::create_dir_all(world.root.join("author")).unwrap();
    symlink(&outside, world.root.join("author/retcon-reviews")).unwrap();
    assert!(reviews::save_preview(&world, &save_request(&world, document.clone(), None)).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::remove_file(world.root.join("author/retcon-reviews")).unwrap();
    reviews::save(&world, &save_request(&world, document, None)).unwrap();
    let original = fs::read(private_path(&world)).unwrap();
    let other = outside.join("source.json");
    fs::write(&other, &original).unwrap();
    fs::remove_file(private_path(&world)).unwrap();
    symlink(&other, private_path(&world)).unwrap();
    assert!(
        reviews::queue(
            &world.root,
            &QueueRequest {
                revision: None,
                max_items: 10
            }
        )
        .is_err()
    );
    fs::remove_file(private_path(&world)).unwrap();
    fs::write(private_path(&world), original).unwrap();
    let canonical = world.root.join(&world.records[PLACE].source_path);
    let canonical_copy = outside.join("canonical.md");
    fs::rename(&canonical, &canonical_copy).unwrap();
    symlink(canonical_copy, canonical).unwrap();
    assert_eq!(detail(&world, None)["currentness"]["status"], "unknown");
}

#[test]
fn rehashed_impossible_plan_is_binding_error_when_original_source_is_available() {
    let (_dir, world, request) = fixture();
    let (document, _) = create(&world, &request);
    for wrong_fingerprint in [true, false] {
        let mut forged = document.clone();
        if wrong_fingerprint {
            forged.analysis.plan.expected_fingerprint = "0".repeat(64);
        } else {
            forged.analysis.plan.operations = vec![Operation::Update {
                entity_id: PLACE.into(),
                patch: "body: missing required native kind\n".into(),
            }];
        }
        let mut hash = Sha256::new();
        hash.update(b"wedl-consequence-plan\0");
        hash.update(serde_json::to_vec(&forged.analysis.plan).unwrap());
        forged.analysis.plan_sha256 = hex::encode(hash.finalize());
        forged.analysis.report["candidate"]["planSha256"] =
            forged.analysis.plan_sha256.clone().into();
        for item in forged.analysis.report["items"].as_array_mut().unwrap() {
            if item["snapshot"] == "candidate" {
                item["citation"]["provenance"]["planSha256"] =
                    forged.analysis.plan_sha256.clone().into();
            }
        }
        forged.analysis.report_sha256 =
            rehash(b"wedl-retcon-review-report\0", &forged.analysis.report);
        forged.analysis.item_digests = forged.analysis.report["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| rehash(b"wedl-retcon-review-item\0", item))
            .collect();
        // The envelope hashes are self-consistent; only original reconstruction
        // can disprove the retained candidate/plan binding.
        assert!(
            reviews::decode_save(
                &serde_json::to_string(&save_request(&world, forged.clone(), None)).unwrap()
            )
            .is_ok()
        );
        raw_document(&world, &forged);
        let error = reviews::detail(
            &world.root,
            &DetailRequest {
                id: ID.into(),
                revision: None,
                expected_sha256: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("REVIEW-BINDING-001"), "{error}");
    }
}

#[test]
fn pending_publication_fences_current_comparison_and_saves_without_losing_private_original() {
    let (_dir, world, request) = fixture();
    let (mut document, saved) = create(&world, &request);
    let original = fs::read(private_path(&world)).unwrap();
    fs::write(
        world.root.join(".wedl-rust/pending-outcome.json"),
        "pending external publication",
    )
    .unwrap();
    let result = detail(&world, None);
    assert_eq!(result["currentness"]["status"], "unknown");
    assert_eq!(result["comparedFingerprint"], Json::Null);
    assert_eq!(
        result["source"],
        String::from_utf8(original.clone()).unwrap()
    );
    assert!(reviews::preview(&world.root, &request).is_err());
    document
        .body_markdown
        .push_str("Cannot race pending source publication.");
    assert!(
        reviews::save(
            &world,
            &save_request(
                &world,
                document,
                saved["sourceSha256"].as_str().map(str::to_owned)
            )
        )
        .is_err()
    );
    assert_eq!(fs::read(private_path(&world)).unwrap(), original);
}

#[test]
fn replacement_refs_do_not_hide_committed_change_then_revert() {
    let (_dir, world, request) = fixture();
    create(&world, &request);
    changed(&world, PLACE, |_, body| {
        body.push_str("Original committed change.\n")
    });
    let relevant = commit(&world.root, "relevant original");
    fs::write(
        world.root.join(&world.records[PLACE].source_path),
        &world.records[PLACE].raw_bytes,
    )
    .unwrap();
    let reverted = commit(&world.root, "original revert");
    let original_tree = git(
        &world.root,
        &["rev-parse", &format!("{}^{{tree}}", request.base_revision)],
    );
    let forged_relevant = git(
        &world.root,
        &[
            "commit-tree",
            &original_tree,
            "-p",
            &request.base_revision,
            "-m",
            "replacement hides the original change",
        ],
    );
    git(&world.root, &["replace", &relevant, &forged_relevant]);
    let result = detail(&world, None);
    assert_eq!(result["comparedRevision"], reverted);
    assert_eq!(result["currentness"]["status"], "stale");
}
