//! Real immutable source, storage routing, and complete reader result bounds.
#[path = "support/actions.rs"]
mod base;
use serde_json::{Value as Json, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};
use wedl::{
    manuscript as m,
    model::{StoryTime, World},
    reader::{self, Operation, PresentationService, PublicPolicy, SimulationPolicy, TextPolicy},
    reveal::CanonicalScope,
};
const SEGMENT: &str = "segment_00000000000000000000000001";
const WORK: &str = "work_00000000000000000000000001";
const PLAN: &str = "reveal_00000000000000000000000001";
const CHAPTER: &str = "chapter_00000000000000000000000001";
fn raw(fields: Json, body: &str) -> String {
    format!(
        "---\n{}---\n{body}",
        serde_yaml::to_string(&fields).unwrap()
    )
}
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn commit(world: &World) -> String {
    git(&world.root, &["add", "--", "story", "author"]);
    git(&world.root, &["commit", "-q", "-m", "Original sources"]);
    git(&world.root, &["rev-parse", "HEAD"])
}
fn put(world: &World, kind: &str, id: &str, fields: Json, body: &str) {
    let path = world.root.join(m::document_path(kind, id).unwrap());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, raw(fields, body)).unwrap();
}
fn work(world: &World, count: usize) {
    let chapters=(0..count).collect::<Vec<_>>().chunks(1024).enumerate().map(|(index,items)|json!({
        "id":format!("chapter_{:026}",index+1),"title":"Private chapter",
        "segments":items.iter().map(|_|json!({"kind":"author-record","id":SEGMENT})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    put(
        world,
        "work",
        WORK,
        json!({"schema":"wedl-author/v1","kind":"work","id":WORK,"title":"Private work","chapters":chapters,"duplicate_policy":"allow-explicit-reuse"}),
        "Private notes",
    );
}
fn plan(world: &World, admissions: Vec<Json>) {
    put(
        world,
        "reveal-plan",
        PLAN,
        json!({"schema":"wedl-author/v1","kind":"reveal-plan","id":PLAN,"title":"Private plan",
        "work":{"kind":"author-record","id":WORK},"entries":[{"milestone":{"chapter_id":CHAPTER,"segment_index":0},"admissions":admissions}]}),
        "Private plan body",
    );
}
fn text(id: usize, markdown: &str) -> Json {
    json!({"kind":"text","id":format!("revealtext_{id:026}"),"markdown":markdown})
}
fn setup(hash: &str) -> (tempfile::TempDir, World, String) {
    let (directory, world) = base::fixture();
    put(
        &world,
        "manuscript-segment",
        SEGMENT,
        json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":SEGMENT,"title":"Private segment","source_links":[]}),
        "Private segment body",
    );
    work(&world, 3);
    plan(
        &world,
        vec![
            text(1, "first revealed text"),
            text(2, "second revealed text"),
        ],
    );
    git(
        &world.root,
        &["init", "-q", &format!("--object-format={hash}")],
    );
    let revision = commit(&world);
    (directory, world, revision)
}
fn public(world: &World, revision: &str) -> PublicPolicy {
    PublicPolicy {
        repository: reader::repository_identity(&world.root).unwrap(),
        revision: revision.into(),
        work_id: WORK.into(),
        plan_id: PLAN.into(),
        canonical_scope: CanonicalScope {
            world_id: world.world_record().unwrap().id.clone(),
            records: BTreeSet::from([base::PLACE.into()]),
            sections: None,
        },
        generation: "one".into(),
        max_occurrence: 2,
        cursor_secret: [17; 32],
        publication_id: "publication".into(),
        at: StoryTime::new("main", 20, 0).unwrap(),
        text_policy: TextPolicy::AllAtPinnedPlan,
    }
}
fn install(world: &World, revision: &str) -> PresentationService {
    let policy = public(world, revision);
    let service = PresentationService::new(Some(world.root.clone()));
    service
        .install_simulation(Some(SimulationPolicy {
            repository: policy.repository.clone(),
            revision: policy.revision.clone(),
            work_id: policy.work_id.clone(),
            plan_id: policy.plan_id.clone(),
            canonical_scope: policy.canonical_scope.clone(),
            generation: policy.generation.clone(),
            max_occurrence: policy.max_occurrence,
            cursor_secret: policy.cursor_secret,
            principal_id: "operator".into(),
            author_permissions: policy.canonical_scope.clone(),
        }))
        .unwrap();
    service.install_public(Some(policy)).unwrap();
    service
}
fn request(revision: &str) -> Json {
    json!({"protocol":reader::PROTOCOL,"publicationId":"publication","revision":revision,"occurrence":0,"limit":1,"cursor":null})
}
fn preview(revision: &str) -> Json {
    let mut input = request(revision);
    input.as_object_mut().unwrap().remove("publicationId");
    input["workId"] = WORK.into();
    input["planId"] = PLAN.into();
    input["operation"] = "page".into();
    input["at"] = json!({"timeline":"main","tick":"20","order":"0"});
    input["textPolicy"] = "all-at-pinned-plan".into();
    input
}
fn result(service: &PresentationService, revision: &str) -> Json {
    service
        .execute(request(revision).to_string().as_bytes(), Operation::Page)
        .unwrap()
}
fn closed_failure(value: &Json, outcome: &str) {
    assert_eq!(value["outcome"], outcome, "{value}");
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        ["protocol", "mode", "outcome", "code", "message"].into()
    );
}

#[test]
fn raw_canonical_and_private_blob_limits_refuse_before_parsing_or_projection() {
    for private in [false, true] {
        let (_directory, world, _) = setup("sha1");
        let path = if private {
            world
                .root
                .join(m::document_path("reveal-plan", PLAN).unwrap())
        } else {
            world.root.join(&world.records[base::PLACE].source_path)
        };
        fs::write(path, "x".repeat(2 * 1024 * 1024 + 1)).unwrap();
        let revision = commit(&world);
        let service = install(&world, &revision);
        closed_failure(&result(&service, &revision), "limit");
        closed_failure(
            &service
                .preview(preview(&revision).to_string().as_bytes())
                .unwrap(),
            "limit",
        );
    }
}

#[test]
fn complete_canonical_and_private_source_byte_budgets_precede_blob_parse() {
    for (directory, count) in [("story", 33), ("author/segments", 17)] {
        let (_directory, world, _) = setup("sha1");
        for index in 0..count {
            fs::write(
                world.root.join(directory).join(format!("extra-{index}.md")),
                "x".repeat(1024 * 1024),
            )
            .unwrap();
        }
        let revision = commit(&world);
        closed_failure(&result(&install(&world, &revision), &revision), "limit");
    }
}

#[test]
fn complete_canonical_and_private_file_counts_are_not_hidden_by_selected_plan() {
    for (directory, count) in [("story", 4097), ("author/segments", 1025)] {
        let (_directory, world, _) = setup("sha1");
        for index in 0..count {
            fs::write(
                world.root.join(directory).join(format!("extra-{index}.md")),
                "invalid extra source",
            )
            .unwrap();
        }
        let revision = commit(&world);
        closed_failure(&result(&install(&world, &revision), &revision), "limit");
    }
}

#[test]
fn malformed_pinned_source_is_public_unavailable_and_author_invalid() {
    for private in [false, true] {
        let (_directory, world, _) = setup("sha1");
        let path = if private {
            world
                .root
                .join(m::document_path("reveal-plan", PLAN).unwrap())
        } else {
            world.root.join(&world.records[base::PLACE].source_path)
        };
        fs::write(path, "unparseable private source sentinel").unwrap();
        let revision = commit(&world);
        let service = install(&world, &revision);
        closed_failure(&result(&service, &revision), "unavailable");
        closed_failure(
            &service
                .preview(preview(&revision).to_string().as_bytes())
                .unwrap(),
            "invalid",
        );
    }
}

#[test]
fn reader_work_limit_refuses_otherwise_valid_author_source_without_partial_page() {
    let (_directory, world, _) = setup("sha1");
    work(&world, 1025);
    let revision = commit(&world);
    let resolved = m::resolve_reveal(
        &world.root,
        &m::RevealResolveRequest {
            revision: Some(revision.clone()),
            plan_id: PLAN.into(),
            expected_author_fingerprint: None,
            max_occurrences: 4096,
            max_characters: 1_000_000,
        },
    )
    .unwrap();
    assert_eq!(resolved["occurrenceCount"], 1025);
    let service = install(&world, &revision);
    closed_failure(&result(&service, &revision), "limit");
    closed_failure(
        &service
            .preview(preview(&revision).to_string().as_bytes())
            .unwrap(),
        "limit",
    );
}

#[test]
fn literal_text_policy_filters_before_whole_admitted_markdown_limit_or_search() {
    let (_directory, world, _) = setup("sha1");
    let owner = &world.records[base::PLACE];
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&owner.frontmatter).unwrap(),
            "x".repeat(1_200_000)
        ),
    )
    .unwrap();
    plan(
        &world,
        vec![
            json!({"kind":"record","target":{"kind":"record","id":base::PLACE}}),
            text(1, &"a".repeat(600_000)),
            text(2, &"b".repeat(600_000)),
        ],
    );
    let revision = commit(&world);
    let service = install(&world, &revision);
    closed_failure(&result(&service, &revision), "limit");
    let mut policy = public(&world, &revision);
    policy.text_policy = TextPolicy::None;
    service.install_public(Some(policy)).unwrap();
    assert_eq!(result(&service, &revision)["count"], 1);
    let mut input = request(&revision);
    input["query"] = "bbb".into();
    let value = service
        .execute(input.to_string().as_bytes(), Operation::Search)
        .unwrap();
    assert_eq!(value["outcome"], "available");
    assert_eq!(value["count"], 0);
}

#[test]
fn changed_storage_routing_is_unavailable_before_even_author_source_diagnostics() {
    let (_directory, world, revision) = setup("sha1");
    let service = install(&world, &revision);
    assert_eq!(result(&service, &revision)["outcome"], "available");
    let other = tempfile::tempdir().unwrap();
    git(other.path(), &["init", "-q"]);
    fs::rename(world.root.join(".git"), world.root.join("original-git")).unwrap();
    fs::write(
        world.root.join(".git"),
        format!("gitdir: {}\n", other.path().join(".git").display()),
    )
    .unwrap();
    closed_failure(&result(&service, &revision), "unavailable");
    closed_failure(
        &service
            .preview(preview(&revision).to_string().as_bytes())
            .unwrap(),
        "unavailable",
    );
}

#[test]
fn cloned_storage_with_identical_original_objects_cannot_reuse_reader_cursor() {
    let (directory, world, revision) = setup("sha1");
    let service = install(&world, &revision);
    let mut input = request(&revision);
    let first = result(&service, &revision);
    input["cursor"] = first["nextCursor"].clone();
    assert!(input["cursor"].as_str().is_some());
    let clone = directory.path().join("clone");
    git(
        directory.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            world.root.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    let other = PresentationService::new(Some(clone.clone()));
    let mut policy = public(&world, &revision);
    policy.repository = reader::repository_identity(&clone).unwrap();
    other.install_public(Some(policy)).unwrap();
    closed_failure(
        &other
            .execute(input.to_string().as_bytes(), Operation::Page)
            .unwrap(),
        "unavailable",
    );
    // An independent first page does not advance another client's valid cursor.
    assert_eq!(result(&service, &revision), first);
    assert_eq!(
        service
            .execute(input.to_string().as_bytes(), Operation::Page)
            .unwrap()["items"][0]["markdown"],
        "second revealed text"
    );
}

#[test]
fn literal_sha256_git_revision_is_a_supported_immutable_reader_pin() {
    let (_directory, world, revision) = setup("sha256");
    assert_eq!(revision.len(), 64);
    let service = install(&world, &revision);
    let value = result(&service, &revision);
    assert_eq!(value["outcome"], "available");
    assert_eq!(value["revision"], revision);
    assert_eq!(value["items"][0]["markdown"], "first revealed text");
}
