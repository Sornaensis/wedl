//! Exact private reveal sources, literal occurrence milestones, bounded whole
//! source validation and the shared private store. Reader authority is tested
//! separately by the projection and isolated service suites.
#[path = "support/actions.rs"]
mod fixture;

use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use wedl::{
    manuscript::{
        self as m, Admission, DetailRequest, ListRequest, RevealResolveRequest, SaveRequest,
    },
    model::{Record, World},
    retcon_impact, retcon_reviews, source, validation,
};

const FIRST: &str = "segment_00000000000000000000000001";
const SECOND: &str = "segment_00000000000000000000000002";
const OTHER: &str = "segment_00000000000000000000000003";
const WORK: &str = "work_00000000000000000000000001";
const CHAPTER: &str = "chapter_00000000000000000000000001";
const NEXT_CHAPTER: &str = "chapter_00000000000000000000000002";
const PLAN: &str = "reveal_00000000000000000000000001";
const TEXT: &str = "revealtext_00000000000000000000000001";
const NEXT_TEXT: &str = "revealtext_00000000000000000000000002";
const NOTES: &str = "\r\n  Exact private plan notes 🌲\r\nTrailing  ";
const REVEALED: &str = "\r\n  Exact revealed text æ🌲\r\n\r\nTrailing  ";

fn raw(fields: &Json, body: &str) -> String {
    format!("---\n{}---\n{body}", serde_yaml::to_string(fields).unwrap())
}
fn record(fields: &Json, body: &str) -> wedl::Result<Record> {
    let path = m::document_path(
        fields["kind"].as_str().unwrap_or("reveal-plan"),
        fields["id"].as_str().unwrap_or(PLAN),
    )?;
    m::parse_document(raw(fields, body).as_bytes(), &path)
}
fn segment(id: &str, tick: Option<i64>) -> Record {
    let mut fields = json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":id,"title":"Unrevealed private segment","source_links":[]});
    if let Some(tick) = tick {
        fields["placement"] = json!({"timeline":"main","tick":tick,"order":0});
    }
    record(
        &fields,
        "Private manuscript body that is never a reveal admission.",
    )
    .unwrap()
}
fn chapter(id: &str, segments: &[&str]) -> Json {
    json!({"id":id,"title":"Private chapter title","segments":segments.iter().map(|id|json!({"kind":"author-record","id":id})).collect::<Vec<_>>()})
}
fn work(chapters: Vec<Json>) -> Record {
    record(&json!({"schema":"wedl-author/v1","kind":"work","id":WORK,"title":"Private work title","chapters":chapters,"duplicate_policy":"allow-explicit-reuse"}),"Private work notes.").unwrap()
}
fn milestone(chapter: &str, index: usize) -> Json {
    json!({"chapter_id":chapter,"segment_index":index})
}
fn entry(chapter: &str, index: usize, admissions: Vec<Json>) -> Json {
    json!({"milestone":milestone(chapter,index),"admissions":admissions})
}
fn text(id: &str, markdown: &str) -> Json {
    json!({"kind":"text","id":id,"markdown":markdown})
}
fn record_admission(id: &str) -> Json {
    json!({"kind":"record","target":{"kind":"record","id":id}})
}
fn section(owner: &str, id: &str) -> Json {
    json!({"kind":"section","target":{"kind":"section","owner_id":owner,"id":id}})
}
fn fields(entries: Vec<Json>) -> Json {
    json!({"schema":"wedl-author/v1","kind":"reveal-plan","id":PLAN,"title":"Private spoiler plan","work":{"kind":"author-record","id":WORK},"entries":entries})
}
fn plan() -> Json {
    fields(vec![
        entry(
            CHAPTER,
            0,
            vec![
                record_admission(fixture::ACTOR),
                section(fixture::ACTOR, fixture::SECTION),
            ],
        ),
        entry(CHAPTER, 1, vec![text(TEXT, REVEALED)]),
        entry(
            NEXT_CHAPTER,
            0,
            vec![text(NEXT_TEXT, "Literal prequel reveal")],
        ),
    ])
}
fn records(plan: &Json) -> Vec<Record> {
    vec![
        segment(FIRST, Some(20)),
        segment(SECOND, Some(-20)),
        work(vec![
            chapter(CHAPTER, &[FIRST, SECOND]),
            chapter(NEXT_CHAPTER, &[FIRST]),
        ]),
        record(plan, NOTES).unwrap(),
    ]
}
fn snapshot(plan: &Json) -> m::Snapshot {
    m::snapshot_from_records(records(plan), None).unwrap()
}
fn put(world: &World, record: &Record) {
    let path = world.root.join(&record.source_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &record.raw_bytes).unwrap();
}
fn saved() -> (tempfile::TempDir, World) {
    let (directory, world) = fixture::fixture();
    for record in records(&plan()) {
        put(&world, &record);
    }
    (directory, world)
}
fn list(world: &World, revision: Option<&str>) -> Json {
    m::list(
        &world.root,
        &ListRequest {
            revision: revision.map(Into::into),
            kind: None,
            max_items: 1024,
        },
    )
    .unwrap()
}
fn detail(world: &World, id: &str, revision: Option<&str>) -> Json {
    m::detail(
        &world.root,
        &DetailRequest {
            revision: revision.map(Into::into),
            id: id.into(),
            expected_sha256: None,
        },
    )
    .unwrap()
}
fn resolve_request(revision: Option<&str>) -> RevealResolveRequest {
    RevealResolveRequest {
        revision: revision.map(Into::into),
        plan_id: PLAN.into(),
        expected_author_fingerprint: None,
        max_occurrences: 4096,
        max_characters: 1_000_000,
    }
}
fn save_request(world: &World, source: String, sha: Option<String>) -> SaveRequest {
    SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_author_fingerprint: list(world, None)["authorFingerprint"]
            .as_str()
            .unwrap()
            .into(),
        expected_sha256: sha,
        source,
    }
}
fn source_image(world: &World) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    fn walk(
        root: &Path,
        dir: &Path,
        image: &mut BTreeMap<String, (Vec<u8>, std::time::SystemTime)>,
    ) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                walk(root, &path, image);
            } else if metadata.is_file() {
                image.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (fs::read(&path).unwrap(), metadata.modified().unwrap()),
                );
            }
        }
    }
    let mut image = BTreeMap::new();
    walk(&world.root, &world.root, &mut image);
    image
}
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
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
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn commit(world: &World) -> String {
    git(&world.root, &["add", "--", "story", "author"]);
    git(
        &world.root,
        &["commit", "-q", "-m", "Pinned reveal sources"],
    );
    git(&world.root, &["rev-parse", "HEAD"])
}

#[test]
fn reveal_source_preserves_exact_private_notes_and_literal_text() {
    let (_, world) = fixture::fixture();
    for newline in ["\n", "\r\n"] {
        let header = serde_yaml::to_string(&plan())
            .unwrap()
            .replace('\n', newline);
        let source = format!("---{newline}{header}---{newline}{NOTES}");
        let path = m::document_path("reveal-plan", PLAN).unwrap();
        assert_eq!(path, format!("author/reveals/{PLAN}.md"));
        let parsed = m::parse_document(source.as_bytes(), &path).unwrap();
        assert_eq!(parsed.raw_bytes, source.as_bytes());
        assert_eq!(parsed.body, NOTES);
        let mut docs = records(&plan());
        docs.pop();
        docs.push(parsed);
        let authors = m::snapshot_from_records(docs, None).unwrap();
        let resolved = m::resolve_reveal_plan(&world, &authors, PLAN, 4096).unwrap();
        assert_eq!(
            resolved.entries[1].admissions,
            vec![Admission::Text {
                id: TEXT.into(),
                markdown: REVEALED.into()
            }]
        );
    }
    let empty = fields(vec![entry(CHAPTER, 0, vec![text(TEXT, "")])]);
    assert!(m::resolve_reveal_plan(&world, &snapshot(&empty), PLAN, 4096).is_ok());
}

#[test]
fn reveal_envelope_and_nested_admission_shapes_are_closed() {
    let original = plan();
    for key in ["schema", "kind", "id", "title", "work", "entries"] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(key);
        assert!(record(&value, NOTES).is_err(), "missing {key}");
    }
    for (key, value) in [
        ("unknown", json!(true)),
        ("work", json!({"kind":"record","id":WORK})),
        ("work", json!({"kind":"author-record","id":FIRST})),
        ("work", json!({"kind":"author-record","id":WORK,"extra":1})),
        ("entries", Json::Null),
    ] {
        let mut candidate = original.clone();
        candidate[key] = value;
        assert!(record(&candidate, NOTES).is_err(), "{key}");
    }
    for value in [
        json!({}),
        json!({"kind":"transition","target":{"kind":"transition","owner_id":fixture::ACTOR,"id":"x"}}),
        json!({"kind":"record","target":{"kind":"author-record","id":WORK}}),
        json!({"kind":"section","target":{"kind":"section","id":fixture::SECTION}}),
        json!({"kind":"text","id":"revealtext_bad","markdown":""}),
        json!({"kind":"text","id":TEXT,"markdown":null}),
        json!({"kind":"text","id":TEXT,"markdown":"","grant":true}),
    ] {
        assert!(
            record(&fields(vec![entry(CHAPTER, 0, vec![value.clone()])]), NOTES).is_err(),
            "{value}"
        );
    }
    for value in [
        json!(true),
        json!(-1),
        json!(1.0),
        json!("0"),
        json!(1024),
        Json::Null,
    ] {
        let mut candidate = original.clone();
        candidate["entries"][0]["milestone"]["segment_index"] = value;
        assert!(record(&candidate, NOTES).is_err());
    }
    for patch in [
        json!({"chapter_id":CHAPTER}),
        json!({"chapter_id":"chapter_bad","segment_index":0}),
        json!({"chapter_id":CHAPTER,"segment_index":0,"extra":false}),
    ] {
        let mut candidate = original.clone();
        candidate["entries"][0]["milestone"] = patch;
        assert!(record(&candidate, NOTES).is_err());
    }
}

#[test]
fn reveal_duplicate_milestones_and_admission_identity_are_global() {
    let first = entry(CHAPTER, 0, vec![text(TEXT, REVEALED)]);
    assert!(record(&fields(vec![first.clone(), first]), NOTES).is_err());
    for admission in [
        record_admission(fixture::ACTOR),
        section(fixture::ACTOR, fixture::SECTION),
        text(TEXT, REVEALED),
    ] {
        assert!(
            record(
                &fields(vec![
                    entry(CHAPTER, 0, vec![admission.clone()]),
                    entry(CHAPTER, 1, vec![admission.clone()])
                ]),
                NOTES
            )
            .is_err()
        );
        assert!(
            record(
                &fields(vec![entry(CHAPTER, 0, vec![admission.clone(), admission])]),
                NOTES
            )
            .is_err()
        );
    }
    assert!(
        record(
            &fields(vec![
                entry(CHAPTER, 0, vec![text(TEXT, "one")]),
                entry(CHAPTER, 1, vec![text(TEXT, "changed text")])
            ]),
            NOTES
        )
        .is_err()
    );
    assert!(record(&plan(), NOTES).is_ok()); // Overlapping record/section admissions remain distinct inputs.
}

#[test]
fn reveal_order_follows_occurrences_with_explicit_reuse() {
    let (_, world) = fixture::fixture();
    let resolved = m::resolve_reveal_plan(&world, &snapshot(&plan()), PLAN, 4096).unwrap();
    assert_eq!(resolved.plan_id, PLAN);
    assert_eq!(resolved.work_id, WORK);
    assert_eq!(
        resolved
            .occurrences
            .iter()
            .map(|item| item.segment_id.as_str())
            .collect::<Vec<_>>(),
        [FIRST, SECOND, FIRST]
    );
    assert_eq!(
        resolved
            .occurrences
            .iter()
            .map(|item| item.segment_index)
            .collect::<Vec<_>>(),
        [0, 1, 0]
    );
    assert_eq!(
        resolved
            .entries
            .iter()
            .map(|entry| entry.ordinal)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    let mut backwards = plan();
    backwards["entries"].as_array_mut().unwrap().reverse();
    let authors = snapshot(&backwards);
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 4096).is_err());
}

#[test]
fn reveal_missing_work_segments_and_occurrences_refuse_whole_resolution() {
    let (_, world) = fixture::fixture();
    let mut missing_work = plan();
    missing_work["work"]["id"] = "work_00000000000000000000000002".into();
    assert!(m::resolve_reveal_plan(&world, &snapshot(&missing_work), PLAN, 4096).is_err());
    for milestone_value in [
        milestone(CHAPTER, 2),
        milestone(NEXT_CHAPTER, 1),
        milestone("chapter_00000000000000000000000003", 0),
    ] {
        let mut value = fields(vec![entry(CHAPTER, 0, vec![])]);
        value["entries"][0]["milestone"] = milestone_value;
        assert!(m::resolve_reveal_plan(&world, &snapshot(&value), PLAN, 4096).is_err());
    }
    let mut docs = records(&fields(vec![]));
    docs.retain(|record| record.id != SECOND);
    let authors = m::snapshot_from_records(docs, None).unwrap();
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 4096).is_err());
}

#[test]
fn reveal_section_links_resolve_authored_members_including_replaced_sections() {
    let (_directory, world) = fixture::fixture();
    let owner = &world.records[fixture::ACTOR];
    let mut changed = owner.frontmatter.clone();
    let mut replacement = changed["descriptions"][0].clone();
    replacement["id"] = "description_00000000000000000000000002".into();
    replacement["replaces"] = fixture::SECTION.into();
    changed["descriptions"]
        .as_sequence_mut()
        .unwrap()
        .push(replacement);
    let source = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&changed).unwrap(),
        owner.body
    );
    let changed = source::parse_record(source.as_bytes(), &owner.source_path).unwrap();
    let records = world
        .records
        .values()
        .map(|record| {
            if record.id == changed.id {
                changed.clone()
            } else {
                record.clone()
            }
        })
        .collect();
    let world = source::world_from_records(world.root.clone(), records, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    assert!(m::resolve_reveal_plan(&world, &snapshot(&plan()), PLAN, 4096).is_ok());
    for reference in [
        section(fixture::PLACE, fixture::SECTION),
        section(fixture::ACTOR, "Known context"),
        section(fixture::ACTOR, "chronology_00000000000000000000000001"),
        record_admission("missing"),
    ] {
        let input = fields(vec![entry(CHAPTER, 0, vec![reference])]);
        assert!(m::resolve_reveal_plan(&world, &snapshot(&input), PLAN, 4096).is_err());
    }
}

#[test]
fn reveal_selected_closure_checks_every_segment_without_implicit_admissions() {
    let (_, world) = fixture::fixture();
    let empty = fields(vec![]);
    let mut docs = records(&empty);
    let mut bad = serde_json::to_value(segment(OTHER, None).frontmatter).unwrap();
    bad["source_links"] = json!([{"kind":"record","id":"missing"}]);
    docs.push(record(&bad, "").unwrap());
    let authors = m::snapshot_from_records(docs.clone(), None).unwrap();
    let compiled = m::resolve_reveal_plan(&world, &authors, PLAN, 4096).unwrap();
    assert!(compiled.entries.is_empty());
    assert_eq!(compiled.occurrences.len(), 3);
    bad["id"] = FIRST.into();
    docs.retain(|record| record.id != FIRST);
    docs.push(record(&bad, "").unwrap());
    let authors = m::snapshot_from_records(docs, None).unwrap();
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 4096).is_err());
}

#[test]
fn reveal_resolution_requires_exact_projection_and_matching_revision() {
    let (_, world) = fixture::fixture();
    let authors = snapshot(&plan());
    let mut forged = authors.clone();
    forged
        .documents
        .get_mut(PLAN)
        .unwrap()
        .body
        .push_str("forged");
    assert!(m::resolve_reveal_plan(&world, &forged, PLAN, 4096).is_err());
    let mut forged = authors.clone();
    forged.fingerprint = "f".repeat(64);
    assert!(m::resolve_reveal_plan(&world, &forged, PLAN, 4096).is_err());
    let revision = "a".repeat(40);
    let mut docs = records(&plan());
    for record in &mut docs {
        record.blob_oid = Some("b".repeat(40));
    }
    let historical = m::snapshot_from_records(docs, Some(revision.clone())).unwrap();
    assert!(m::resolve_reveal_plan(&world, &historical, PLAN, 4096).is_err());
    let mut selected = world.clone();
    selected.revision = Some(revision);
    assert!(m::resolve_reveal_plan(&selected, &historical, PLAN, 4096).is_ok());
    selected.revision = Some("c".repeat(40));
    assert!(m::resolve_reveal_plan(&selected, &historical, PLAN, 4096).is_err());
}

#[test]
fn reveal_source_allows_1025_occurrences_and_complete_reader_bound_can_refuse() {
    let (_, world) = fixture::fixture();
    let plan = fields(vec![entry(NEXT_CHAPTER, 0, vec![text(TEXT, REVEALED)])]);
    let docs = vec![
        segment(FIRST, None),
        work(vec![
            chapter(CHAPTER, &vec![FIRST; 1024]),
            chapter(NEXT_CHAPTER, &[FIRST]),
        ]),
        record(&plan, NOTES).unwrap(),
    ];
    let authors = m::snapshot_from_records(docs, None).unwrap();
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 1024).is_err());
    let result = m::resolve_reveal_plan(&world, &authors, PLAN, 1025).unwrap();
    assert_eq!(result.occurrences.len(), 1025);
    assert_eq!(result.entries[0].ordinal, 1024);
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 0).is_err());
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 4097).is_err());
}

#[test]
fn reveal_nested_source_budgets_precede_reference_resolution() {
    let oversized = fields(vec![entry(
        CHAPTER,
        0,
        (0..1025)
            .map(|i| text(&format!("revealtext_{i:026}"), ""))
            .collect(),
    )]);
    assert!(record(&oversized, NOTES).is_err());
    let oversized = fields(vec![entry(
        CHAPTER,
        0,
        vec![text(TEXT, &"x".repeat(m::MAX_BODY_BYTES + 1))],
    )]);
    assert!(record(&oversized, NOTES).is_err());
    let mut source = raw(
        &fields(vec![entry(CHAPTER, 0, vec![text(TEXT, "")])]),
        NOTES,
    );
    source = source.replacen("entries:", "entries: []\nentries:", 1);
    assert!(
        m::parse_document(
            source.as_bytes(),
            &m::document_path("reveal-plan", PLAN).unwrap()
        )
        .is_err()
    );
}

#[test]
fn reveal_catalogue_detail_and_fingerprints_remain_private() {
    let (_directory, world) = saved();
    let canonical = fixture::source_image(&world);
    let before = source_image(&world);
    let all = list(&world, None);
    assert_eq!(all["items"].as_array().unwrap().len(), 4);
    let filtered = m::list(
        &world.root,
        &ListRequest {
            revision: None,
            kind: Some("reveal-plan".into()),
            max_items: 1,
        },
    )
    .unwrap();
    assert_eq!(filtered["items"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["items"][0]["id"], PLAN);
    assert_eq!(filtered["authorFingerprint"], all["authorFingerprint"]);
    let detail = detail(&world, PLAN, None);
    assert_eq!(detail["document"]["bodyMarkdown"], NOTES);
    assert_eq!(detail["document"]["rawSource"], raw(&plan(), NOTES));
    let report = m::resolve_reveal(&world.root, &resolve_request(None)).unwrap();
    assert_eq!(report["operation"], "reveal-plans-resolve");
    assert_eq!(report["scope"], "private-reveal-plans");
    assert_eq!(report["plan"]["id"], PLAN);
    assert_eq!(report["work"]["id"], WORK);
    assert_eq!(report["entries"][1]["admissions"][0]["markdown"], REVEALED);
    assert_eq!(report["entries"][2]["milestone"]["segmentIndex"], 0);
    assert_eq!(report["entries"][2]["ordinal"], 2);
    assert_eq!(report["readerAuthorized"], false);
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(report["canApply"], false);
    assert_eq!(report["validationComplete"], false);
    assert_eq!(source_image(&world), before);
    assert_eq!(fixture::source_image(&world), canonical);
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[test]
fn reveal_save_pins_cover_whole_private_source_and_same_content_retries() {
    let (_directory, world) = saved();
    let original = detail(&world, PLAN, None);
    let sha = original["document"]["sourceSha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let request = save_request(&world, raw(&plan(), NOTES), Some(sha.clone()));
    let before = source_image(&world);
    let same = m::save(&world, &request).unwrap();
    assert_eq!(same["changed"], false);
    assert_eq!(same["writesPerformed"], false);
    assert_eq!(source_image(&world), before);
    let changed = save_request(
        &world,
        raw(&plan(), &(NOTES.to_owned() + "Changed private note.")),
        Some(sha),
    );
    let preview = m::save_preview(&world, &changed).unwrap();
    assert_eq!(preview["writesPerformed"], false);
    assert_eq!(source_image(&world), before);
    let saved = m::save(&world, &changed).unwrap();
    assert!(m::save(&world, &request).is_err());
    assert_ne!(
        saved["afterAuthorFingerprint"],
        saved["beforeAuthorFingerprint"]
    );
    let old = save_request(
        &world,
        raw(&plan(), NOTES),
        saved["afterSha256"].as_str().map(Into::into),
    );
    let source = detail(&world, FIRST, None);
    let changed = save_request(
        &world,
        source["document"]["rawSource"].as_str().unwrap().to_owned() + "Unrelated prose.",
        source["document"]["sourceSha256"].as_str().map(Into::into),
    );
    m::save(&world, &changed).unwrap();
    assert!(m::save_preview(&world, &old).is_err());
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[test]
fn reveal_complete_candidate_saves_protect_work_milestones_and_canonical_refs() {
    let (_directory, world) = saved();
    let previous = detail(&world, WORK, None);
    let reordered = work(vec![
        chapter(NEXT_CHAPTER, &[FIRST]),
        chapter(CHAPTER, &[FIRST, SECOND]),
    ]);
    let request = save_request(
        &world,
        String::from_utf8(reordered.raw_bytes).unwrap(),
        previous["document"]["sourceSha256"]
            .as_str()
            .map(Into::into),
    );
    let before = source_image(&world);
    assert!(m::save_preview(&world, &request).is_err());
    assert!(m::save(&world, &request).is_err());
    assert_eq!(source_image(&world), before);
    let mut bad = plan();
    bad["entries"][0]["admissions"][0] = record_admission("missing");
    let previous = detail(&world, PLAN, None);
    let request = save_request(
        &world,
        raw(&bad, NOTES),
        previous["document"]["sourceSha256"]
            .as_str()
            .map(Into::into),
    );
    assert!(m::save_preview(&world, &request).is_err());
    assert_eq!(source_image(&world), before);
}

#[test]
fn reveal_original_resolution_uses_the_same_commit_despite_invalid_live_files() {
    for object_format in ["sha1", "sha256"] {
        let (_directory, world) = saved();
        git(
            &world.root,
            &["init", "-q", &format!("--object-format={object_format}")],
        );
        let revision = commit(&world);
        let request = resolve_request(Some(&revision));
        let original = m::resolve_reveal(&world.root, &request).unwrap();
        assert_eq!(original["revision"], revision);
        assert!(original["plan"]["blobOid"].as_str().is_some());
        assert_eq!(
            original["segments"][0]["blobOid"].as_str().unwrap().len(),
            revision.len()
        );
        fs::write(
            world
                .root
                .join(m::document_path("reveal-plan", PLAN).unwrap()),
            "invalid live private plan",
        )
        .unwrap();
        fs::write(
            world.root.join(&world.records[fixture::ACTOR].source_path),
            "invalid live canonical source",
        )
        .unwrap();
        let before = source_image(&world);
        assert_eq!(m::resolve_reveal(&world.root, &request).unwrap(), original);
        assert_eq!(
            detail(&world, PLAN, Some(&revision))["document"]["rawSource"],
            raw(&plan(), NOTES)
        );
        assert!(m::resolve_reveal(&world.root, &resolve_request(None)).is_err());
        assert_eq!(source_image(&world), before);
    }
}

#[test]
fn reveal_requests_and_report_character_limits_are_complete() {
    let (_directory, world) = saved();
    let good = json!({"revision":null,"planId":PLAN,"expectedAuthorFingerprint":null,"maxOccurrences":4096,"maxCharacters":1_000_000});
    assert!(m::decode_reveal_resolve(&good.to_string()).is_ok());
    for key in [
        "revision",
        "planId",
        "expectedAuthorFingerprint",
        "maxOccurrences",
        "maxCharacters",
    ] {
        let mut request = good.clone();
        request.as_object_mut().unwrap().remove(key);
        assert!(m::decode_reveal_resolve(&request.to_string()).is_err());
    }
    for (key, value) in [
        ("revision", json!("HEAD")),
        ("planId", json!(WORK)),
        ("expectedAuthorFingerprint", json!(["a".repeat(64)])),
        ("maxOccurrences", json!(0)),
        ("maxOccurrences", json!(4097)),
        ("maxCharacters", json!(255)),
        ("maxCharacters", json!(1_000_001)),
        ("grant", json!(true)),
    ] {
        let mut request = good.clone();
        request[key] = value;
        assert!(m::decode_reveal_resolve(&request.to_string()).is_err());
    }
    let duplicate = good.to_string().replacen('}', ",\"revision\":null}", 1);
    assert!(m::decode_reveal_resolve(&duplicate).is_err());
    let duplicate = good
        .to_string()
        .replacen('}', ",\"revis\\u0069on\":null}", 1);
    assert!(m::decode_reveal_resolve(&duplicate).is_err());
    let mut request = resolve_request(None);
    request.expected_author_fingerprint = Some("0".repeat(64));
    assert!(m::resolve_reveal(&world.root, &request).is_err());
    request.expected_author_fingerprint = None;
    request.max_occurrences = 2;
    assert!(m::resolve_reveal(&world.root, &request).is_err());
    request.max_occurrences = 4096;
    for _ in 0..4 {
        let report = m::resolve_reveal(&world.root, &request).unwrap();
        let count = report.to_string().chars().count();
        if count == request.max_characters {
            break;
        }
        request.max_characters = count;
    }
    let complete = m::resolve_reveal(&world.root, &request).unwrap();
    assert_eq!(complete.to_string().chars().count(), request.max_characters);
    assert!(complete.to_string().len() > request.max_characters);
    request.max_characters -= 1;
    assert!(m::resolve_reveal(&world.root, &request).is_err());
}

#[test]
fn reveal_changes_do_not_invent_retcon_dependency_nodes() {
    let (_, world) = fixture::fixture();
    let mut docs = records(&plan());
    docs.pop();
    let no_reveals = m::snapshot_from_records(docs.clone(), None).unwrap();
    let mut invalid = plan();
    invalid["work"]["id"] = "work_00000000000000000000000002".into();
    docs.push(record(&invalid, NOTES).unwrap());
    let with_reveal = m::snapshot_from_records(docs, None).unwrap();
    let request=retcon_impact::decode(&json!({"target":{"kind":"record","id":fixture::ACTOR},"at":fixture::horizon(),"depth":32,"maxItems":1000}).to_string()).unwrap();
    let before = retcon_impact::evaluate_with_authors(
        &world,
        &world,
        &no_reveals,
        &request,
        &"a".repeat(64),
    )
    .unwrap();
    let after = retcon_impact::evaluate_with_authors(
        &world,
        &world,
        &with_reveal,
        &request,
        &"a".repeat(64),
    )
    .unwrap();
    assert_eq!(before, after);
    let mut request = request;
    request.target = retcon_impact::Reference::AuthorRecord { id: PLAN.into() };
    assert!(
        retcon_impact::evaluate_with_authors(
            &world,
            &world,
            &with_reveal,
            &request,
            &"a".repeat(64)
        )
        .is_err()
    );
}

#[test]
fn reveal_only_edits_leave_existing_manuscript_retcon_witness_current() {
    let (_directory, world) = saved();
    git(&world.root, &["init", "-q"]);
    let base = commit(&world);
    let input = json!({"baseRevision":base,"plan":{"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.update","entityId":fixture::ACTOR,"patch":"kind: character\ntitle: Candidate actor\n"}]},"impactRequest":{"target":{"kind":"record","id":fixture::ACTOR},"at":fixture::horizon(),"depth":32,"maxItems":1000}});
    let capture = retcon_reviews::preview(
        &world.root,
        &retcon_reviews::decode_preview(&input.to_string()).unwrap(),
    )
    .unwrap();
    let review_id = "review_00000000000000000000000001";
    let input = json!({"expectedFingerprint":world.fingerprint,"expectedSha256":null,"document":{"id":review_id,"title":"Original manuscript impact","analysis":capture["analysis"],"decisions":[],"bodyMarkdown":""}});
    let saved = retcon_reviews::save(
        &world,
        &retcon_reviews::decode_save(&input.to_string()).unwrap(),
    )
    .unwrap();
    let request = retcon_reviews::decode_detail(
        &json!({"revision":null,"id":review_id,"expectedSha256":saved["afterSha256"]}).to_string(),
    )
    .unwrap();
    assert_eq!(
        retcon_reviews::detail(&world.root, &request).unwrap()["currentness"]["status"],
        "current"
    );
    let original = detail(&world, PLAN, None);
    let update = save_request(
        &world,
        raw(&plan(), "Changed private reveal notes"),
        original["document"]["sourceSha256"]
            .as_str()
            .map(Into::into),
    );
    m::save(&world, &update).unwrap();
    assert_eq!(
        retcon_reviews::detail(&world.root, &request).unwrap()["currentness"]["status"],
        "current"
    );
    let mut invalid = plan();
    invalid["entries"][0]["admissions"][0] = record_admission("missing");
    put(&world, &record(&invalid, NOTES).unwrap());
    assert_eq!(
        retcon_reviews::detail(&world.root, &request).unwrap()["currentness"]["status"],
        "current"
    );
    assert!(m::resolve_reveal(&world.root, &resolve_request(None)).is_err());
}

#[test]
fn reveal_source_inspection_survives_broken_semantic_context() {
    let (_directory, world) = saved();
    let mut invalid = plan();
    invalid["work"]["id"] = "work_00000000000000000000000002".into();
    put(&world, &record(&invalid, NOTES).unwrap());
    assert_eq!(
        detail(&world, PLAN, None)["document"]["rawSource"],
        raw(&invalid, NOTES)
    );
    assert_eq!(list(&world, None)["items"].as_array().unwrap().len(), 4);
    assert!(m::resolve_reveal(&world.root, &resolve_request(None)).is_err());
    fs::write(
        world.root.join(&world.records[fixture::ACTOR].source_path),
        "invalid current canonical source",
    )
    .unwrap();
    assert_eq!(
        detail(&world, PLAN, None)["document"]["bodyMarkdown"],
        NOTES
    );
    assert_eq!(list(&world, None)["items"].as_array().unwrap().len(), 4);
}

#[cfg(unix)]
#[test]
fn reveal_directory_and_source_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    let (_directory, world) = saved();
    let path = world
        .root
        .join(m::document_path("reveal-plan", PLAN).unwrap());
    let outside = world.root.parent().unwrap().join("outside-reveal.md");
    fs::rename(&path, &outside).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(
        m::list(
            &world.root,
            &ListRequest {
                revision: None,
                kind: None,
                max_items: 1024
            }
        )
        .is_err()
    );
    fs::remove_file(&path).unwrap();
    fs::rename(&outside, &path).unwrap();
    let directory = path.parent().unwrap();
    let outside = world.root.parent().unwrap().join("outside-reveals");
    fs::rename(directory, &outside).unwrap();
    symlink(&outside, directory).unwrap();
    assert!(
        m::detail(
            &world.root,
            &DetailRequest {
                revision: None,
                id: PLAN.into(),
                expected_sha256: None
            }
        )
        .is_err()
    );
}

#[test]
fn reveal_empty_work_without_milestones_is_valid_private_source() {
    let (_, world) = fixture::fixture();
    let plan = fields(vec![]);
    let authors =
        m::snapshot_from_records(vec![work(vec![]), record(&plan, NOTES).unwrap()], None).unwrap();
    let resolved = m::resolve_reveal_plan(&world, &authors, PLAN, 4096).unwrap();
    assert!(resolved.occurrences.is_empty());
    assert!(resolved.entries.is_empty());
    m::validate_snapshot(&world, &authors).unwrap();
    let plan = fields(vec![entry(CHAPTER, 0, vec![text(TEXT, REVEALED)])]);
    let authors =
        m::snapshot_from_records(vec![work(vec![]), record(&plan, NOTES).unwrap()], None).unwrap();
    assert!(m::resolve_reveal_plan(&world, &authors, PLAN, 4096).is_err());
}
