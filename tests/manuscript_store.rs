use serde_json::{Value as Json, json};
use std::{fs, path::Path, process::Command};
use wedl::{
    Error,
    manuscript::{self as manuscripts, DetailRequest, ListRequest, ResolveRequest, SaveRequest},
    model::World,
    source,
};
#[path = "support/actions.rs"]
mod support;
use support::{ACTOR, PLACE, SECTION, TICK};

const SEGMENT: &str = "segment_00000000000000000000000001";
const OTHER_SEGMENT: &str = "segment_00000000000000000000000002";
const WORK: &str = "work_00000000000000000000000001";
const OTHER_WORK: &str = "work_00000000000000000000000002";
const CHAPTER: &str = "chapter_00000000000000000000000001";

fn document(kind: &str, id: &str, fields: Json, body: &str) -> String {
    let mut data =
        json!({"schema":"wedl-author/v1","kind":kind,"id":id,"title":"Private source title"});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    format!("---\n{}---\n{body}", serde_yaml::to_string(&data).unwrap())
}
fn segment(id: &str, body: &str) -> String {
    document(
        "manuscript-segment",
        id,
        json!({"source_links":[
        {"kind":"record","id":PLACE},
        {"kind":"section","owner_id":ACTOR,"id":SECTION}
    ],"placement":{"timeline":"main","tick":TICK,"order":-1},"pov_character_id":ACTOR}),
        body,
    )
}
fn work(id: &str, references: &[&str]) -> String {
    document(
        "work",
        id,
        json!({"duplicate_policy":"allow-explicit-reuse","chapters":[
            {"id":CHAPTER,"title":"Authored first chapter","segments":references.iter().map(|id| json!({"kind":"author-record","id":id})).collect::<Vec<_>>()}
        ]}),
        "Private assembly notes.\n",
    )
}
fn list(world: &World) -> Json {
    manuscripts::list(
        &world.root,
        &ListRequest {
            revision: None,
            kind: None,
            max_items: 1024,
        },
    )
    .unwrap()
}
fn detail(world: &World, id: &str) -> Json {
    manuscripts::detail(
        &world.root,
        &DetailRequest {
            revision: None,
            id: id.into(),
            expected_sha256: None,
        },
    )
    .unwrap()
}
fn request(world: &World, source: String, expected_sha256: Option<String>) -> SaveRequest {
    SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_author_fingerprint: list(world)["authorFingerprint"].as_str().unwrap().into(),
        expected_sha256,
        source,
    }
}
fn create(world: &World, source: String) -> Json {
    manuscripts::save(world, &request(world, source, None)).unwrap()
}
fn put(world: &World, kind: &str, id: &str, raw: &str) {
    let path = world
        .root
        .join(manuscripts::document_path(kind, id).unwrap());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, raw).unwrap();
}
fn resolve_request() -> ResolveRequest {
    ResolveRequest {
        revision: None,
        work_id: WORK.into(),
        expected_author_fingerprint: None,
        max_occurrences: 4096,
        max_characters: 1_000_000,
    }
}
fn assembled() -> (tempfile::TempDir, World) {
    let (directory, world) = support::fixture();
    create(&world, segment(SEGMENT, "Private segment prose 🌲\r\n"));
    create(&world, work(WORK, &[SEGMENT, SEGMENT]));
    (directory, world)
}
fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn init_git(world: &World) {
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["config", "user.name", "Manuscript tests"]);
    git(
        &world.root,
        &["config", "user.email", "manuscript@example.invalid"],
    );
}
fn commit(world: &World) -> String {
    git(&world.root, &["add", "--", "story", "author"]);
    git(
        &world.root,
        &["commit", "-q", "-m", "Pinned private originals"],
    );
    git(&world.root, &["rev-parse", "HEAD"])
}

#[test]
fn empty_inspection_and_preview_create_no_private_or_cache_files() {
    let (_directory, world) = support::fixture();
    let listed = list(&world);
    assert_eq!(listed["items"], json!([]));
    assert_eq!(listed["writesPerformed"], false);
    let request = request(&world, segment(SEGMENT, "Exact prospective prose"), None);
    let preview = manuscripts::save_preview(&world, &request).unwrap();
    assert_eq!(preview["operation"], "manuscripts-preview-save");
    assert_eq!(preview["canSave"], true);
    assert_eq!(preview["beforeSource"], Json::Null);
    assert_eq!(preview["afterSource"], request.source);
    assert_eq!(preview["authorFingerprint"], listed["authorFingerprint"]);
    assert_ne!(
        preview["beforeAuthorFingerprint"],
        preview["afterAuthorFingerprint"]
    );
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn exact_source_lifecycle_preserves_crlf_unicode_large_clock_and_canonical_isolation() {
    let (_directory, world) = support::fixture();
    let canonical = support::source_image(&world);
    let original = segment(SEGMENT, "\nExact 🌲\nTrailing space  ").replace('\n', "\r\n");
    let created = create(&world, original.clone());
    assert_eq!(created["writesPerformed"], true);
    assert_eq!(created["gitCommit"], Json::Null);
    let first = detail(&world, SEGMENT);
    assert_eq!(first["document"]["rawSource"], original);
    assert_eq!(
        first["document"]["bodyMarkdown"],
        "\r\nExact 🌲\r\nTrailing space  "
    );
    assert!(
        first["document"]["frontmatterYaml"]
            .as_str()
            .unwrap()
            .contains(&TICK.to_string())
    );
    assert_eq!(first["document"]["blobOid"], Json::Null);
    assert_eq!(
        first["document"]["frontmatter"]["placement"]["tick"],
        TICK.to_string()
    );
    assert_eq!(first["document"]["frontmatter"]["placement"]["order"], "-1");
    assert_eq!(
        first["authorFingerprint"],
        created["afterAuthorFingerprint"]
    );
    let updated_source = format!("{original}\r\nRevised ending.");
    let update = request(
        &world,
        updated_source.clone(),
        first["document"]["sourceSha256"]
            .as_str()
            .map(str::to_owned),
    );
    let preview = manuscripts::save_preview(&world, &update).unwrap();
    assert_eq!(preview["beforeSource"], original);
    assert_eq!(detail(&world, SEGMENT), first);
    let saved = manuscripts::save(&world, &update).unwrap();
    assert_eq!(
        saved["afterAuthorFingerprint"],
        preview["afterAuthorFingerprint"]
    );
    assert_eq!(
        detail(&world, SEGMENT)["document"]["rawSource"],
        updated_source
    );
    assert_eq!(support::source_image(&world), canonical);
    let fresh = source::load_world(&world.root, None).unwrap();
    assert_eq!(fresh.fingerprint, world.fingerprint);
    assert_eq!(fresh.records.len(), world.records.len());
    assert!(fresh.records.get(SEGMENT).is_none());
    assert_eq!(
        fs::read_dir(world.root.join(".wedl-rust")).unwrap().count(),
        1
    );
}

#[test]
fn same_content_is_no_write_and_still_requires_all_three_current_pins() {
    let (_directory, world) = support::fixture();
    let raw = segment(SEGMENT, "Retained exact text");
    let saved = create(&world, raw.clone());
    let request = request(
        &world,
        raw.clone(),
        saved["sourceSha256"].as_str().map(str::to_owned),
    );
    let path = world
        .root
        .join(manuscripts::document_path("manuscript-segment", SEGMENT).unwrap());
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let no_op = manuscripts::save(&world, &request).unwrap();
    assert_eq!(no_op["changed"], false);
    assert_eq!(no_op["writesPerformed"], false);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    for index in 0..3 {
        let mut stale = request.clone();
        match index {
            0 => stale.expected_fingerprint = "0".repeat(64),
            1 => stale.expected_author_fingerprint = "0".repeat(64),
            _ => stale.expected_sha256 = Some("0".repeat(64)),
        }
        assert!(matches!(
            manuscripts::save(&world, &stale),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            manuscripts::save_preview(&world, &stale),
            Err(Error::Conflict(_))
        ));
    }
    assert_eq!(fs::read_to_string(path).unwrap(), raw);
}

#[test]
fn create_update_and_whole_store_compare_and_swap_refuse_stale_sources() {
    let (_directory, world) = support::fixture();
    let create_request = request(&world, segment(SEGMENT, "First"), None);
    manuscripts::save(&world, &create_request).unwrap();
    assert!(matches!(
        manuscripts::save(&world, &create_request),
        Err(Error::Conflict(_))
    ));
    let first = detail(&world, SEGMENT);
    let update = request(
        &world,
        segment(SEGMENT, "Second"),
        first["document"]["sourceSha256"]
            .as_str()
            .map(str::to_owned),
    );
    create(&world, segment(OTHER_SEGMENT, "Unrelated source changed"));
    assert!(matches!(
        manuscripts::save(&world, &update),
        Err(Error::Conflict(_))
    ));
    let mut absent = request(&world, work(OTHER_WORK, &[SEGMENT]), Some("0".repeat(64)));
    assert!(matches!(
        manuscripts::save(&world, &absent),
        Err(Error::Conflict(_))
    ));
    absent.source = segment(SEGMENT, "Wrong create");
    absent.expected_sha256 = None;
    assert!(matches!(
        manuscripts::save(&world, &absent),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        detail(&world, SEGMENT)["document"]["rawSource"],
        first["document"]["rawSource"]
    );
}

#[test]
fn complete_candidate_validation_requires_work_dependencies_and_all_segment_links() {
    let (_directory, world) = support::fixture();
    let absent = request(&world, work(WORK, &[SEGMENT]), None);
    assert!(manuscripts::save_preview(&world, &absent).is_err());
    assert!(manuscripts::save(&world, &absent).is_err());
    assert!(!world.root.join("author").exists());
    create(&world, segment(SEGMENT, "Valid source"));
    create(&world, work(WORK, &[SEGMENT]));
    let invalid =
        segment(SEGMENT, "Still private").replace(PLACE, "loc_00000000000000000000000099");
    put(&world, "manuscript-segment", SEGMENT, &invalid);
    assert_eq!(detail(&world, SEGMENT)["document"]["rawSource"], invalid);
    assert!(manuscripts::resolve(&world.root, &resolve_request()).is_err());
    let unrelated = request(
        &world,
        segment(OTHER_SEGMENT, "Cannot bless broken store"),
        None,
    );
    assert!(manuscripts::save(&world, &unrelated).is_err());
    assert!(
        !world
            .root
            .join(manuscripts::document_path("manuscript-segment", OTHER_SEGMENT).unwrap())
            .exists()
    );
}

#[test]
fn selected_work_resolve_keeps_reuse_and_does_not_validate_unrelated_private_links() {
    let (_directory, world) = assembled();
    let invalid = segment(OTHER_SEGMENT, "Unreferenced private prose")
        .replace(PLACE, "loc_00000000000000000000000099");
    put(&world, "manuscript-segment", OTHER_SEGMENT, &invalid);
    let result = manuscripts::resolve(&world.root, &resolve_request()).unwrap();
    assert_eq!(result["occurrenceCount"], 2);
    assert_eq!(result["uniqueSegmentCount"], 1);
    assert_eq!(result["chapters"][0]["occurrences"][0]["segmentIndex"], 0);
    assert_eq!(result["chapters"][0]["occurrences"][1]["segmentIndex"], 1);
    assert_eq!(result["segments"][0]["placement"]["tick"], TICK.to_string());
    assert_eq!(result["readerAuthorized"], false);
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["validationComplete"], false);
    assert!(!result.to_string().contains("Unreferenced private prose"));
}

#[test]
fn private_inspection_remains_available_when_canonical_source_is_invalid() {
    let (_directory, world) = assembled();
    let before = detail(&world, SEGMENT);
    let original_list = list(&world);
    fs::write(
        world.root.join(&world.records[PLACE].source_path),
        "Not a canonical Markdown envelope",
    )
    .unwrap();
    assert_eq!(list(&world), original_list);
    assert_eq!(detail(&world, SEGMENT), before);
    assert!(manuscripts::resolve(&world.root, &resolve_request()).is_err());
    let request = request(
        &world,
        segment(OTHER_SEGMENT, "Cannot save against invalid source"),
        None,
    );
    assert!(manuscripts::save(&world, &request).is_err());
}

#[test]
fn saves_reject_forged_world_projections_and_current_canonical_drift() {
    let (_directory, world) = support::fixture();
    let request = request(&world, segment(SEGMENT, "Prospective source"), None);
    let mut forged = world.clone();
    forged.records.get_mut(PLACE).unwrap().body = "Changed only in memory".into();
    assert!(matches!(
        manuscripts::save(&forged, &request),
        Err(Error::Conflict(_))
    ));
    let mut forged = world.clone();
    forged.records.get_mut(PLACE).unwrap().frontmatter["title"] = "Forged title".into();
    assert!(matches!(
        manuscripts::save_preview(&forged, &request),
        Err(Error::Conflict(_))
    ));
    let record = &world.records[PLACE];
    fs::write(
        world.root.join(&record.source_path),
        [record.raw_bytes.as_slice(), b"\nExternal change"].concat(),
    )
    .unwrap();
    assert!(matches!(
        manuscripts::save(&world, &request),
        Err(Error::Conflict(_))
    ));
    assert!(!world.root.join("author").exists());
}

#[test]
fn historical_inspection_and_resolve_use_exact_original_canonical_and_private_blobs() {
    let (_directory, world) = assembled();
    init_git(&world);
    let revision = commit(&world);
    let original = detail(&world, SEGMENT);
    let original_author_fingerprint = list(&world)["authorFingerprint"].clone();
    put(
        &world,
        "manuscript-segment",
        SEGMENT,
        &segment(SEGMENT, "Changed live private prose"),
    );
    fs::write(
        world.root.join(&world.records[PLACE].source_path),
        "Invalid live canonical source",
    )
    .unwrap();
    let selected = manuscripts::detail(
        &world.root,
        &DetailRequest {
            revision: Some(revision.clone()),
            id: SEGMENT.into(),
            expected_sha256: original["document"]["sourceSha256"]
                .as_str()
                .map(str::to_owned),
        },
    )
    .unwrap();
    assert_eq!(
        selected["document"]["rawSource"],
        original["document"]["rawSource"]
    );
    assert_eq!(selected["authorFingerprint"], original_author_fingerprint);
    assert_eq!(selected["revision"], revision);
    let path = manuscripts::document_path("manuscript-segment", SEGMENT).unwrap();
    assert_eq!(
        selected["document"]["blobOid"],
        git(&world.root, &["rev-parse", &format!("{revision}:{path}")])
    );
    let mut request = resolve_request();
    request.revision = Some(revision.clone());
    request.expected_author_fingerprint = original_author_fingerprint.as_str().map(str::to_owned);
    let result = manuscripts::resolve(&world.root, &request).unwrap();
    assert_eq!(result["revision"], revision);
    assert!(result.to_string().contains("Private segment prose"));
    assert!(!result.to_string().contains("Changed live private prose"));
    assert_eq!(git(&world.root, &["rev-parse", "HEAD"]), revision);
}

#[test]
fn missing_historical_manuscripts_never_fall_back_to_current_files() {
    let (_directory, world) = support::fixture();
    init_git(&world);
    git(&world.root, &["add", "--", "story"]);
    git(
        &world.root,
        &["commit", "-q", "-m", "Before private source"],
    );
    let revision = git(&world.root, &["rev-parse", "HEAD"]);
    create(&world, segment(SEGMENT, "Live-only private source"));
    create(&world, work(WORK, &[SEGMENT]));
    let result = manuscripts::list(
        &world.root,
        &ListRequest {
            revision: Some(revision.clone()),
            kind: None,
            max_items: 1024,
        },
    )
    .unwrap();
    assert_eq!(result["items"], json!([]));
    assert!(matches!(
        manuscripts::detail(
            &world.root,
            &DetailRequest {
                revision: Some(revision.clone()),
                id: SEGMENT.into(),
                expected_sha256: None
            }
        ),
        Err(Error::NotFound(_))
    ));
    let mut request = resolve_request();
    request.revision = Some(revision);
    assert!(manuscripts::resolve(&world.root, &request).is_err());
}

#[test]
fn historical_reads_support_literal_sha256_git_commit_and_blob_ids() {
    let (_directory, world) = assembled();
    git(&world.root, &["init", "-q", "--object-format=sha256"]);
    git(&world.root, &["config", "user.name", "Manuscript tests"]);
    git(
        &world.root,
        &["config", "user.email", "manuscript@example.invalid"],
    );
    let revision = commit(&world);
    assert_eq!(revision.len(), 64);
    let selected = manuscripts::detail(
        &world.root,
        &DetailRequest {
            revision: Some(revision.clone()),
            id: SEGMENT.into(),
            expected_sha256: None,
        },
    )
    .unwrap();
    assert_eq!(selected["document"]["blobOid"].as_str().unwrap().len(), 64);
    let mut request = resolve_request();
    request.revision = Some(revision);
    assert_eq!(
        manuscripts::resolve(&world.root, &request).unwrap()["occurrenceCount"],
        2
    );
}

#[test]
fn other_private_stores_do_not_join_manuscript_identity_or_validation() {
    let (_directory, world) = assembled();
    let before = list(&world);
    for folder in ["actions", "edits", "retcon-reviews", "unrelated"] {
        fs::create_dir_all(world.root.join("author").join(folder)).unwrap();
        fs::write(
            world
                .root
                .join("author")
                .join(folder)
                .join("malformed.private"),
            "arbitrary unrelated source",
        )
        .unwrap();
    }
    assert_eq!(list(&world), before);
    assert!(manuscripts::resolve(&world.root, &resolve_request()).is_ok());
    init_git(&world);
    let revision = commit(&world);
    assert_eq!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: Some(revision),
                kind: None,
                max_items: 1024
            }
        )
        .unwrap()["authorFingerprint"],
        before["authorFingerprint"]
    );
}

#[test]
fn list_detail_and_resolve_limits_refuse_partial_or_stale_results() {
    let (_directory, world) = assembled();
    assert!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: None,
                kind: None,
                max_items: 1
            }
        )
        .is_err()
    );
    let selected = manuscripts::list(
        &world.root,
        &ListRequest {
            revision: None,
            kind: Some("work".into()),
            max_items: 1,
        },
    )
    .unwrap();
    assert_eq!(selected["items"].as_array().unwrap().len(), 1);
    assert_eq!(selected["items"][0]["id"], WORK);
    assert!(matches!(
        manuscripts::detail(
            &world.root,
            &DetailRequest {
                revision: None,
                id: SEGMENT.into(),
                expected_sha256: Some("0".repeat(64))
            }
        ),
        Err(Error::Conflict(_))
    ));
    let mut request = resolve_request();
    request.max_occurrences = 1;
    assert!(manuscripts::resolve(&world.root, &request).is_err());
    request = resolve_request();
    request.max_characters = 256;
    assert!(manuscripts::resolve(&world.root, &request).is_err());
    request = resolve_request();
    request.expected_author_fingerprint = Some("0".repeat(64));
    assert!(matches!(
        manuscripts::resolve(&world.root, &request),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn requests_are_closed_duplicate_safe_and_require_explicit_nullable_fields() {
    let list = json!({"revision":null,"kind":null,"maxItems":1024});
    let detail = json!({"revision":null,"id":SEGMENT,"expectedSha256":null});
    let resolve = json!({"revision":null,"workId":WORK,"expectedAuthorFingerprint":null,"maxOccurrences":4096,"maxCharacters":1_000_000});
    let save = json!({"expectedFingerprint":"0".repeat(64),"expectedAuthorFingerprint":"0".repeat(64),"expectedSha256":null,"source":segment(SEGMENT,"Raw source")});
    assert!(manuscripts::decode_list(&list.to_string()).is_ok());
    assert!(manuscripts::decode_detail(&detail.to_string()).is_ok());
    assert!(manuscripts::decode_resolve(&resolve.to_string()).is_ok());
    assert!(manuscripts::decode_save(&save.to_string()).is_ok());
    for (value, names) in [
        (&list, vec!["revision", "kind"]),
        (&detail, vec!["revision", "expectedSha256"]),
        (&resolve, vec!["revision", "expectedAuthorFingerprint"]),
        (&save, vec!["expectedSha256"]),
    ] {
        for name in names {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(name);
            let raw = missing.to_string();
            let accepted = if value == &list {
                manuscripts::decode_list(&raw).is_ok()
            } else if value == &detail {
                manuscripts::decode_detail(&raw).is_ok()
            } else if value == &resolve {
                manuscripts::decode_resolve(&raw).is_ok()
            } else {
                manuscripts::decode_save(&raw).is_ok()
            };
            assert!(!accepted, "missing {name} accepted");
        }
    }
    assert!(
        manuscripts::decode_list(
            r#"{"revision":null,"revi\u0073ion":null,"kind":null,"maxItems":1}"#
        )
        .is_err()
    );
    assert!(manuscripts::decode_detail(&format!(r#"{{"revision":null,"id":"{SEGMENT}","expectedSha256":null,"expected\u0053ha256":null}}"#)).is_err());
    let mut unknown = list.clone();
    unknown["readerAuthorized"] = true.into();
    assert!(manuscripts::decode_list(&unknown.to_string()).is_err());
    let mut unknown = save.clone();
    unknown["path"] = "story/injection.md".into();
    assert!(manuscripts::decode_save(&unknown.to_string()).is_err());
}

#[test]
fn request_bounds_and_literal_identity_checks_apply_before_repository_access() {
    for revision in ["HEAD", "main", "1234567", &"A".repeat(40), &"0".repeat(41)] {
        assert!(
            manuscripts::decode_list(
                &json!({"revision":revision,"kind":null,"maxItems":1}).to_string()
            )
            .is_err()
        );
    }
    for limit in [0, 1025] {
        assert!(
            manuscripts::decode_list(
                &json!({"revision":null,"kind":null,"maxItems":limit}).to_string()
            )
            .is_err()
        );
    }
    for id in [
        "../segment_00000000000000000000000001",
        "segment_bad",
        PLACE,
    ] {
        assert!(
            manuscripts::decode_detail(
                &json!({"revision":null,"id":id,"expectedSha256":null}).to_string()
            )
            .is_err()
        );
    }
    assert!(manuscripts::decode_resolve(&json!({"revision":null,"workId":WORK,"expectedAuthorFingerprint":null,"maxOccurrences":4097,"maxCharacters":1_000_000}).to_string()).is_err());
    assert!(manuscripts::decode_resolve(&json!({"revision":null,"workId":WORK,"expectedAuthorFingerprint":null,"maxOccurrences":1,"maxCharacters":255}).to_string()).is_err());
    assert!(manuscripts::decode_resolve(&json!({"revision":null,"workId":WORK,"expectedAuthorFingerprint":null,"maxOccurrences":1.0,"maxCharacters":1_000_000}).to_string()).is_err());
    assert!(
        manuscripts::list(
            Path::new("/unavailable-manuscript-root"),
            &ListRequest {
                revision: Some("HEAD".into()),
                kind: None,
                max_items: 1
            }
        )
        .is_err()
    );
}

#[test]
fn escaped_source_request_uses_its_own_byte_bound_without_weakening_decoded_source_limit() {
    let source = segment(SEGMENT, &"\u{1}".repeat(manuscripts::MAX_BODY_BYTES));
    let raw = json!({"expectedFingerprint":"0".repeat(64),"expectedAuthorFingerprint":"0".repeat(64),"expectedSha256":null,"source":source}).to_string();
    assert!(raw.len() > 4 * 1024 * 1024);
    assert_eq!(manuscripts::decode_save(&raw).unwrap().source, source);
    let too_large = json!({"expectedFingerprint":"0".repeat(64),"expectedAuthorFingerprint":"0".repeat(64),"expectedSha256":null,"source":"x".repeat(manuscripts::MAX_DOCUMENT_BYTES + 1)}).to_string();
    assert!(manuscripts::decode_save(&too_large).is_err());
    assert!(manuscripts::decode_save(&" ".repeat(8 * 1024 * 1024 + 1)).is_err());
}

#[test]
fn exact_paths_and_complete_structural_store_are_required_even_with_kind_filter() {
    let (_directory, world) = assembled();
    let path = world
        .root
        .join(manuscripts::document_path("manuscript-segment", SEGMENT).unwrap());
    fs::write(
        &path,
        segment(OTHER_SEGMENT, "Wrong identity in existing filename"),
    )
    .unwrap();
    assert!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: None,
                kind: Some("work".into()),
                max_items: 1
            }
        )
        .is_err()
    );
    fs::write(&path, segment(SEGMENT, "Correct source")).unwrap();
    fs::write(
        world.root.join("author/segments/unexpected.txt"),
        "Not source",
    )
    .unwrap();
    assert!(
        manuscripts::detail(
            &world.root,
            &DetailRequest {
                revision: None,
                id: WORK.into(),
                expected_sha256: None
            }
        )
        .is_err()
    );
}

#[test]
fn snapshot_bounds_are_combined_across_both_private_directories_before_reads() {
    let (_directory, world) = support::fixture();
    let source = segment(SEGMENT, "Small source");
    put(&world, "manuscript-segment", SEGMENT, &source);
    let path = world
        .root
        .join(manuscripts::document_path("manuscript-segment", SEGMENT).unwrap());
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len((manuscripts::MAX_DOCUMENT_BYTES + 1) as u64)
        .unwrap();
    assert!(
        manuscripts::list(
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
    let large_body = "x".repeat(manuscripts::MAX_BODY_BYTES);
    for index in 0..17 {
        let id = format!("segment_{index:026}");
        put(
            &world,
            "manuscript-segment",
            &id,
            &segment(&id, &large_body),
        );
    }
    assert!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: None,
                kind: None,
                max_items: 1024
            }
        )
        .is_err()
    );
}

#[test]
fn combined_document_count_is_bounded_before_parsing_source() {
    let (_directory, world) = support::fixture();
    fs::create_dir_all(world.root.join("author/segments")).unwrap();
    fs::create_dir_all(world.root.join("author/works")).unwrap();
    for index in 0..1025 {
        let (folder, prefix) = if index < 512 {
            ("segments", "segment")
        } else {
            ("works", "work")
        };
        fs::write(
            world
                .root
                .join(format!("author/{folder}/{prefix}_{index:026}.md")),
            "Unparsed source",
        )
        .unwrap();
    }
    let error = manuscripts::list(
        &world.root,
        &ListRequest {
            revision: None,
            kind: None,
            max_items: 1024,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("complete file"));
}

#[test]
fn small_yaml_alias_sources_share_one_complete_expansion_budget_current_and_historical() {
    let (_directory, world) = support::fixture();
    let long_id = "x".repeat(1024);
    let references = format!(
        "  - &pointer {{kind: record, id: {long_id}}}\n{}",
        "  - *pointer\n".repeat(999)
    );
    let mut raw_bytes = 0usize;
    for index in 0..17 {
        let id = format!("segment_{index:026}");
        let raw = format!(
            "---\nschema: wedl-author/v1\nkind: manuscript-segment\nid: {id}\ntitle: Bounded aliases\nsource_links:\n{references}---\n"
        );
        let path = manuscripts::document_path("manuscript-segment", &id).unwrap();
        assert!(manuscripts::parse_document(raw.as_bytes(), &path).is_ok());
        raw_bytes += raw.len();
        put(&world, "manuscript-segment", &id, &raw);
    }
    assert!(raw_bytes < 256 * 1024);
    let request = ListRequest {
        revision: None,
        kind: None,
        max_items: 1024,
    };
    let error = manuscripts::list(&world.root, &request).unwrap_err();
    assert!(error.to_string().contains("expanded"), "{error}");
    init_git(&world);
    let revision = commit(&world);
    let error = manuscripts::list(
        &world.root,
        &ListRequest {
            revision: Some(revision),
            ..request
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("expanded"), "{error}");
}

#[test]
fn writer_lock_contention_prevents_private_publication() {
    let (_directory, world) = support::fixture();
    let request = request(&world, segment(SEGMENT, "Cannot race another writer"), None);
    fs::create_dir(world.root.join(".wedl-rust")).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(world.root.join(".wedl-rust/write.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(manuscripts::save(&world, &request).is_err());
    assert!(!world.root.join("author").exists());
}

#[test]
fn pending_canonical_publication_fences_resolve_and_saves_but_preserves_private_inspection() {
    let (_directory, world) = assembled();
    let original = detail(&world, SEGMENT);
    fs::write(
        world.root.join(".wedl-rust/pending-outcome.json"),
        "Publication fence",
    )
    .unwrap();
    assert_eq!(detail(&world, SEGMENT), original);
    assert!(manuscripts::resolve(&world.root, &resolve_request()).is_err());
    let request = request(
        &world,
        segment(OTHER_SEGMENT, "Cannot save while fenced"),
        None,
    );
    assert!(manuscripts::save(&world, &request).is_err());
}

#[cfg(unix)]
#[test]
fn private_parent_and_document_symlinks_are_rejected_without_following_targets() {
    use std::os::unix::fs::symlink;
    let (_directory, world) = support::fixture();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), world.root.join("author")).unwrap();
    assert!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: None,
                kind: None,
                max_items: 1024
            }
        )
        .is_err()
    );
    fs::remove_file(world.root.join("author")).unwrap();
    fs::create_dir(world.root.join("author")).unwrap();
    symlink(outside.path(), world.root.join("author/segments")).unwrap();
    assert!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: None,
                kind: None,
                max_items: 1024
            }
        )
        .is_err()
    );
    fs::remove_file(world.root.join("author/segments")).unwrap();
    fs::create_dir(world.root.join("author/segments")).unwrap();
    let external = outside.path().join("source.md");
    let raw = segment(SEGMENT, "External original remains unchanged");
    fs::write(&external, &raw).unwrap();
    symlink(
        &external,
        world
            .root
            .join(manuscripts::document_path("manuscript-segment", SEGMENT).unwrap()),
    )
    .unwrap();
    assert!(
        manuscripts::detail(
            &world.root,
            &DetailRequest {
                revision: None,
                id: SEGMENT.into(),
                expected_sha256: None
            }
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(external).unwrap(), raw);
}

#[cfg(unix)]
#[test]
fn current_canonical_symlinks_and_readonly_private_updates_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_directory, world) = assembled();
    let original = detail(&world, SEGMENT);
    let request = request(
        &world,
        segment(SEGMENT, "Update"),
        original["document"]["sourceSha256"]
            .as_str()
            .map(str::to_owned),
    );
    let path = world
        .root
        .join(manuscripts::document_path("manuscript-segment", SEGMENT).unwrap());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(manuscripts::save(&world, &request).is_err());
    assert_eq!(detail(&world, SEGMENT), original);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let canonical = world.root.join(&world.records[PLACE].source_path);
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), &world.records[PLACE].raw_bytes).unwrap();
    fs::remove_file(&canonical).unwrap();
    symlink(outside.path(), &canonical).unwrap();
    assert_eq!(detail(&world, SEGMENT), original);
    assert!(manuscripts::resolve(&world.root, &resolve_request()).is_err());
    assert!(manuscripts::save(&world, &request).is_err());
}

#[cfg(unix)]
#[test]
fn historical_symlinks_in_selected_private_tree_are_rejected() {
    use std::os::unix::fs::symlink;
    let (_directory, world) = assembled();
    init_git(&world);
    let path = world
        .root
        .join(manuscripts::document_path("manuscript-segment", SEGMENT).unwrap());
    fs::remove_file(&path).unwrap();
    symlink("../../story/unknown.md", &path).unwrap();
    let revision = commit(&world);
    assert!(
        manuscripts::list(
            &world.root,
            &ListRequest {
                revision: Some(revision),
                kind: None,
                max_items: 1024
            }
        )
        .is_err()
    );
}
