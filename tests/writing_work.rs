//! A private work binds exact author source without changing Writing admission.
#[path = "support/family_character.rs"]
mod family;
#[path = "support/actions.rs"]
mod support;

use serde_json::{Value as Json, json};
use std::{fs, path::Path, process::Command};
use wedl::{
    Error, action_drafts, action_requirements,
    manuscript::{self, Snapshot},
    model::{Record, StoryTime, World},
    source,
    writing_frames::{self, FamilySelection, Frame, Lens, WorkSelection},
};

const WORK: &str = "work_00000000000000000000000001";
const OTHER_WORK: &str = "work_00000000000000000000000002";
const SEGMENT: &str = "segment_00000000000000000000000001";
const OTHER_SEGMENT: &str = "segment_00000000000000000000000002";
const HIDDEN: &str = "char_00000000000000000000000009";

fn document(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut fields = fields.as_object().unwrap().clone();
    fields.extend(
        json!({"schema":"wedl-author/v1","kind":kind,"id":id})
            .as_object()
            .unwrap()
            .clone(),
    );
    let raw = format!(
        "---\n{}---\n{body}",
        serde_yaml::to_string(&fields).unwrap()
    );
    manuscript::parse_document(
        raw.as_bytes(),
        &manuscript::document_path(kind, id).unwrap(),
    )
    .unwrap()
}

fn segment(id: &str, fields: Json, body: &str) -> Record {
    let mut data = json!({"title":"PRIVATE_SEGMENT_TITLE","source_links":[]});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    document("manuscript-segment", id, data, body)
}

fn work(id: &str, title: &str, segments: &[&str]) -> Record {
    let chapters = segments.chunks(1024).enumerate().map(|(index, segments)| {
        json!({"id":format!("chapter_{:026}",index+1),"title":"PRIVATE_CHAPTER_TITLE",
            "segments":segments.iter().map(|id|json!({"kind":"author-record","id":id})).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    document(
        "work",
        id,
        json!({"title":title,"duplicate_policy":"allow-explicit-reuse","chapters":chapters}),
        "PRIVATE_WORK_NOTES\r\n",
    )
}

fn authors() -> Snapshot {
    manuscript::snapshot_from_records(
        vec![
            segment(
                SEGMENT,
                json!({}),
                "PRIVATE_PROSE 🌲\r\n  Exact trailing spaces.  \n",
            ),
            work(WORK, "PRIVATE_WORK_TITLE", &[SEGMENT, SEGMENT]),
        ],
        None,
    )
    .unwrap()
}

fn frame(world: &World, authors: Option<&Snapshot>, lens: Lens) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: authors.map(|authors| WorkSelection {
            work_id: WORK.into(),
            author_fingerprint: authors.fingerprint.clone(),
        }),
        scene_id: Some(support::SCENE.into()),
        pov_id: Some(support::ACTOR.into()),
        at: support::horizon(),
        lens,
    }
}

fn put(world: &World, authors: &Snapshot) {
    for record in authors.documents.values() {
        let path = world.root.join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &record.raw_bytes).unwrap();
    }
}

fn unavailable<T: std::fmt::Debug>(result: wedl::Result<T>) {
    let error = result.unwrap_err();
    assert!(matches!(error, Error::NotFound(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        "writing frame is unavailable; select it again"
    );
}

fn read(world: &World, frame: &Frame) -> wedl::Result<Json> {
    writing_frames::with_author_snapshot(world, frame, false, |authors| {
        writing_frames::context(world, authors, frame, &[], 100_000)
    })
}

fn image(root: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
    let mut files = walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                fs::read(entry.path()).unwrap(),
                fs::metadata(entry.path()).unwrap().modified().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
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

fn commit(world: &World) -> String {
    git(&world.root, &["add", "--", "story", "author"]);
    git(
        &world.root,
        &["commit", "-q", "-m", "Original writing sources"],
    );
    git(&world.root, &["rev-parse", "HEAD"])
}

#[test]
fn work_selection_is_required_nullable_closed_and_exact() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    let selected = frame(&world, Some(&authors), Lens::Character);
    let wire = json!(selected);
    assert_eq!(writing_frames::decode(&wire.to_string()).unwrap(), selected);
    let mut unbound = wire.clone();
    unbound["work"] = Json::Null;
    assert!(
        writing_frames::decode(&unbound.to_string())
            .unwrap()
            .work
            .is_none()
    );
    let mut absent = wire.clone();
    absent.as_object_mut().unwrap().remove("work");
    assert!(writing_frames::decode(&absent.to_string()).is_err());
    for bad in [
        json!(WORK),
        json!([]),
        json!({"workId":WORK}),
        json!({"workId":null,"authorFingerprint":authors.fingerprint}),
        json!({"workId":SEGMENT,"authorFingerprint":authors.fingerprint}),
        json!({"workId":"work_missing","authorFingerprint":authors.fingerprint}),
        json!({"workId":WORK,"authorFingerprint":"A".repeat(64)}),
        json!({"workId":WORK,"authorFingerprint":0}),
        json!({"workId":WORK,"authorFingerprint":authors.fingerprint,"occurrence":0}),
    ] {
        let mut candidate = wire.clone();
        candidate["work"] = bad;
        assert!(
            writing_frames::decode(&candidate.to_string()).is_err(),
            "{candidate}"
        );
    }
    let duplicate = wire.to_string().replace(
        &format!("\"workId\":\"{WORK}\""),
        &format!("\"workId\":\"{WORK}\",\"workId\":\"{WORK}\""),
    );
    assert!(writing_frames::decode(&duplicate).is_err());
}

#[test]
fn work_order_placement_pov_and_links_never_change_coordinates_or_admission() {
    let (_directory, original) = support::fixture();
    let hidden = source::parse_record(
        format!("---\nschema: wedl/v1\nkind: character\nid: {HIDDEN}\ntitle: PRIVATE_LINKED_PERSON\ninitial_state: {{}}\n---\nPrivate linked body\n").as_bytes(),
        &format!("story/{HIDDEN}.md"),
    ).unwrap();
    let mut records = original.records.values().cloned().collect::<Vec<_>>();
    records.push(hidden);
    let world = source::world_from_records(original.root, records, None).unwrap();
    let authors = manuscript::snapshot_from_records(
        vec![
            segment(
                SEGMENT,
                json!({"placement":{"timeline":"main","tick":support::TICK+50,"order":7},
            "pov_character_id":HIDDEN,"source_links":[{"kind":"record","id":HIDDEN}]}),
                "PRIVATE_PROSE",
            ),
            segment(
                OTHER_SEGMENT,
                json!({"placement":{"timeline":"main","tick":-support::TICK,"order":-17}}),
                "PRIVATE_EARLIER_PROSE",
            ),
            work(
                WORK,
                "PRIVATE_WORK_TITLE",
                &[SEGMENT, OTHER_SEGMENT, SEGMENT],
            ),
        ],
        None,
    )
    .unwrap();
    let before = authors
        .documents
        .values()
        .map(|r| r.raw_bytes.clone())
        .collect::<Vec<_>>();
    for lens in [Lens::Character, Lens::Author, Lens::DramaticIrony] {
        let bound = frame(&world, Some(&authors), lens);
        let unbound = frame(&world, None, lens);
        assert_eq!(
            writing_frames::resolve(&world, Some(&authors), &bound).unwrap(),
            json!(bound)
        );
        let context =
            writing_frames::context(&world, Some(&authors), &bound, &[], 100_000).unwrap();
        let plain = writing_frames::context(&world, None, &unbound, &[], 100_000).unwrap();
        assert_eq!(context["characterPrompt"], plain["characterPrompt"]);
        assert_eq!(context["authorMargin"], plain["authorMargin"]);
        assert_eq!(context["frame"]["at"], json!(support::horizon()));
        for needle in [
            "PRIVATE_WORK_TITLE",
            "PRIVATE_CHAPTER_TITLE",
            "PRIVATE_SEGMENT_TITLE",
            "PRIVATE_PROSE",
            "PRIVATE_EARLIER_PROSE",
            "PRIVATE_WORK_NOTES",
        ] {
            assert!(!context.to_string().contains(needle), "{needle}");
        }
        if lens != Lens::Author {
            assert!(!context["characterPrompt"].to_string().contains(HIDDEN));
        }
    }
    let mut without_coordinates = frame(&world, Some(&authors), Lens::Author);
    without_coordinates.scene_id = None;
    without_coordinates.pov_id = None;
    assert_eq!(
        writing_frames::resolve(&world, Some(&authors), &without_coordinates).unwrap(),
        json!(without_coordinates)
    );
    assert!(
        writing_frames::context(&world, Some(&authors), &without_coordinates, &[], 100_000)
            .is_err()
    );
    assert_eq!(
        before,
        authors
            .documents
            .values()
            .map(|r| r.raw_bytes.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn selected_work_requires_both_exact_source_pins_and_the_same_revision() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    let selected = frame(&world, Some(&authors), Lens::Character);
    unavailable(writing_frames::resolve(&world, None, &selected));
    let mut missing = selected.clone();
    missing.work.as_mut().unwrap().work_id = OTHER_WORK.into();
    unavailable(writing_frames::resolve(&world, Some(&authors), &missing));
    let mut stale = selected.clone();
    stale.work.as_mut().unwrap().author_fingerprint = "f".repeat(64);
    unavailable(writing_frames::resolve(&world, Some(&authors), &stale));
    stale = selected.clone();
    stale.fingerprint = "f".repeat(64);
    unavailable(writing_frames::resolve(&world, Some(&authors), &stale));
    let mut historical = authors.clone();
    historical.revision = Some("a".repeat(40));
    unavailable(writing_frames::resolve(
        &world,
        Some(&historical),
        &selected,
    ));
    let mut incorrect_kind = selected.clone();
    incorrect_kind.work.as_mut().unwrap().work_id = SEGMENT.into();
    assert!(matches!(
        writing_frames::resolve(&world, Some(&authors), &incorrect_kind),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn native_private_projections_and_fingerprint_cannot_forge_a_selected_work() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    let selected = frame(&world, Some(&authors), Lens::Character);
    for mutation in 0..6 {
        let mut forged = authors.clone();
        match mutation {
            0 => forged.documents.get_mut(WORK).unwrap().title = "Forged title".into(),
            1 => forged
                .documents
                .get_mut(SEGMENT)
                .unwrap()
                .body
                .push_str("Forged prose"),
            2 => forged
                .documents
                .get_mut(SEGMENT)
                .unwrap()
                .raw_bytes
                .push(b' '),
            3 => {
                forged.documents.get_mut(WORK).unwrap().source_path =
                    "author/works/elsewhere.md".into()
            }
            4 => forged.documents.get_mut(WORK).unwrap().blob_oid = Some("a".repeat(40)),
            _ => {
                forged.fingerprint = "f".repeat(64);
            }
        }
        let mut matching = selected.clone();
        matching.work.as_mut().unwrap().author_fingerprint = forged.fingerprint.clone();
        unavailable(writing_frames::resolve(&world, Some(&forged), &matching));
        unavailable(writing_frames::choices(
            &world,
            Some(&forged),
            &matching,
            100,
        ));
        unavailable(writing_frames::context(
            &world,
            Some(&forged),
            &matching,
            &[],
            100_000,
        ));
    }
}

#[test]
fn selected_work_validates_the_complete_closure_without_unrelated_orphans() {
    let (_directory, world) = support::fixture();
    let mut records = authors().documents.into_values().collect::<Vec<_>>();
    records.push(segment(
        OTHER_SEGMENT,
        json!({"placement":{"timeline":"main","tick":support::TICK+100,"order":0},
        "source_links":[{"kind":"record","id":HIDDEN}]}),
        "UNAVAILABLE_LATER_SEGMENT",
    ));
    let authors = manuscript::snapshot_from_records(records.clone(), None).unwrap();
    let selected = frame(&world, Some(&authors), Lens::Character);
    writing_frames::resolve(&world, Some(&authors), &selected).unwrap();
    records.retain(|record| record.id != WORK);
    records.push(work(WORK, "Changed work", &[SEGMENT, OTHER_SEGMENT]));
    let changed = manuscript::snapshot_from_records(records, None).unwrap();
    let selected = frame(&world, Some(&changed), Lens::Character);
    unavailable(writing_frames::resolve(&world, Some(&changed), &selected));
    unavailable(writing_frames::context(
        &world,
        Some(&changed),
        &selected,
        &[],
        1,
    ));
    let unbound = frame(&world, None, Lens::Character);
    writing_frames::context(&world, Some(&changed), &unbound, &[], 100_000).unwrap();
    let author = frame(&world, None, Lens::Author);
    unavailable(writing_frames::choices(
        &world,
        Some(&changed),
        &author,
        100,
    ));
}

#[test]
fn missing_segments_placement_timelines_and_private_povs_fail_the_whole_work() {
    let (_directory, world) = support::fixture();
    for fields in [
        json!({"placement":{"timeline":"absent","tick":0,"order":0}}),
        json!({"pov_character_id":HIDDEN}),
        json!({"source_links":[{"kind":"section","owner_id":support::ACTOR,"id":"description_00000000000000000000000009"}]}),
    ] {
        let authors = manuscript::snapshot_from_records(
            vec![
                segment(SEGMENT, fields, "Private body"),
                work(WORK, "Invalid closure", &[SEGMENT]),
            ],
            None,
        )
        .unwrap();
        unavailable(writing_frames::resolve(
            &world,
            Some(&authors),
            &frame(&world, Some(&authors), Lens::Character),
        ));
    }
    let authors =
        manuscript::snapshot_from_records(vec![work(WORK, "Missing segment", &[SEGMENT])], None)
            .unwrap();
    unavailable(writing_frames::resolve(
        &world,
        Some(&authors),
        &frame(&world, Some(&authors), Lens::Author),
    ));
}

#[test]
fn author_work_choices_are_minimal_folded_complete_and_independently_pinned() {
    let (_directory, world) = support::fixture();
    let authors = manuscript::snapshot_from_records(
        vec![
            work(WORK, "Straße", &[]),
            work(OTHER_WORK, "STRASSE", &[]),
            work("work_00000000000000000000000003", "Strasße", &[]),
        ],
        None,
    )
    .unwrap();
    let selected = frame(&world, None, Lens::Author);
    let choices = writing_frames::choices(&world, Some(&authors), &selected, 100).unwrap();
    assert_eq!(
        choices["works"],
        json!([
            {"id":WORK,"label":"Straße"},
            {"id":OTHER_WORK,"label":"STRASSE"},
            {"id":"work_00000000000000000000000003","label":"Strasße"},
        ])
    );
    assert_eq!(choices["authorFingerprint"], authors.fingerprint);
    assert!(!choices.to_string().contains("PRIVATE_WORK_NOTES"));
    assert!(matches!(
        writing_frames::choices(&world, Some(&authors), &selected, 2),
        Err(Error::InvalidInput(_))
    ));
    let empty = manuscript::snapshot_from_records(vec![], None).unwrap();
    for provided in [None, Some(&empty)] {
        let choices = writing_frames::choices(&world, provided, &selected, 100).unwrap();
        assert_eq!(choices["works"], json!([]));
        assert!(choices["authorFingerprint"].is_null());
    }
}

#[test]
fn unbound_character_and_dramatic_reads_ignore_private_changes_and_invalid_snapshots() {
    let (_directory, mut world) = support::fixture();
    world.root = world.root.join("does-not-exist");
    let mut invalid = authors();
    invalid.fingerprint = "malformed".into();
    invalid.revision = Some("unrelated".into());
    invalid.documents.get_mut(WORK).unwrap().title = "PRIVATE_FORGED_NAME".into();
    for lens in [Lens::Character, Lens::DramaticIrony] {
        let selected = frame(&world, None, lens);
        let plain = writing_frames::choices(&world, None, &selected, 100).unwrap();
        assert_eq!(
            writing_frames::choices(&world, Some(&invalid), &selected, 100).unwrap(),
            plain
        );
        assert_eq!(plain["works"], json!([]));
        assert!(plain["authorFingerprint"].is_null());
        assert_eq!(
            writing_frames::context(&world, Some(&invalid), &selected, &[], 100_000).unwrap(),
            read(&world, &selected).unwrap(),
        );
        let discovered = writing_frames::with_author_snapshot(&world, &selected, true, |authors| {
            assert!(authors.is_none());
            writing_frames::choices(&world, authors, &selected, 100)
        })
        .unwrap();
        assert_eq!(discovered, plain);
    }
}

#[test]
fn bound_character_choices_validate_work_but_never_disclose_private_labels_or_pin_catalogues() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    for lens in [Lens::Character, Lens::DramaticIrony] {
        let selected = frame(&world, Some(&authors), lens);
        let choices = writing_frames::choices(&world, Some(&authors), &selected, 100).unwrap();
        assert_eq!(choices["works"], json!([]));
        assert!(choices["authorFingerprint"].is_null());
        assert_eq!(choices["frame"]["work"], json!(selected.work));
        assert!(!choices.to_string().contains("PRIVATE_"));
    }
}

#[test]
fn work_binding_uses_source_bounds_without_reader_occurrence_or_prose_report_limits() {
    let (_directory, world) = support::fixture();
    let authors = manuscript::snapshot_from_records(
        vec![
            segment(SEGMENT, json!({}), &"p".repeat(1_000_001)),
            work(WORK, "Large private work", &vec![SEGMENT; 1_025]),
        ],
        None,
    )
    .unwrap();
    let selected = frame(&world, Some(&authors), Lens::Character);
    writing_frames::resolve(&world, Some(&authors), &selected).unwrap();
    let context = writing_frames::context(&world, Some(&authors), &selected, &[], 100_000).unwrap();
    assert!(context.to_string().len() < 100_000);
    assert!(!context.to_string().contains("Large private work"));
}

#[test]
fn current_capture_is_read_only_and_private_edits_require_a_new_private_pin() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    put(&world, &authors);
    let selected = frame(&world, Some(&authors), Lens::Character);
    let before = image(&world.root);
    let original = read(&world, &selected).unwrap();
    let author = frame(&world, None, Lens::Author);
    let choices = writing_frames::with_author_snapshot(&world, &author, true, |authors| {
        writing_frames::choices(&world, authors, &author, 100)
    })
    .unwrap();
    assert_eq!(choices["authorFingerprint"], authors.fingerprint);
    assert_eq!(before, image(&world.root));
    let replacement = segment(SEGMENT, json!({}), "Changed private prose\r\n");
    fs::write(
        world.root.join(&replacement.source_path),
        &replacement.raw_bytes,
    )
    .unwrap();
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    unavailable(read(&world, &selected));
    assert_eq!(
        selected.work.as_ref().unwrap().author_fingerprint,
        authors.fingerprint
    );
    let fresh =
        manuscript::snapshot_from_records(vec![replacement, authors.documents[WORK].clone()], None)
            .unwrap();
    let refreshed = frame(&world, Some(&fresh), Lens::Character);
    assert_eq!(
        read(&world, &refreshed).unwrap()["characterPrompt"],
        original["characterPrompt"]
    );
    assert_eq!(
        read(&world, &frame(&world, None, Lens::Character)).unwrap()["characterPrompt"],
        original["characterPrompt"]
    );
}

#[test]
fn current_capture_rechecks_private_source_and_canonical_source_before_emission() {
    for private_change in [true, false] {
        let (_directory, world) = support::fixture();
        let authors = authors();
        put(&world, &authors);
        let selected = frame(&world, Some(&authors), Lens::Character);
        let result = writing_frames::with_author_snapshot(&world, &selected, false, |snapshot| {
            let result = writing_frames::context(&world, snapshot, &selected, &[], 100_000);
            if private_change {
                let path = world.root.join(&authors.documents[SEGMENT].source_path);
                let mut bytes = fs::read(&path).unwrap();
                bytes.extend_from_slice(b"\nConcurrent private edit\n");
                fs::write(path, bytes).unwrap();
            } else {
                support::change_source(&world);
            }
            result
        });
        unavailable(result);
    }
}

#[test]
fn current_private_capture_refuses_an_already_stale_loaded_world() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    put(&world, &authors);
    support::change_source(&world);
    let mut called = false;
    let selected = frame(&world, Some(&authors), Lens::Character);
    unavailable(writing_frames::with_author_snapshot(
        &world,
        &selected,
        false,
        |_| {
            called = true;
            Ok(Json::Null)
        },
    ));
    assert!(!called);
}

#[test]
fn historical_work_frames_use_original_same_commit_sources_in_both_git_formats() {
    for format in ["sha1", "sha256"] {
        let (_directory, current) = support::fixture();
        let authors = authors();
        put(&current, &authors);
        git(
            &current.root,
            &["init", "-q", &format!("--object-format={format}")],
        );
        git(
            &current.root,
            &["config", "user.name", "Writing work tests"],
        );
        git(
            &current.root,
            &["config", "user.email", "writing@example.invalid"],
        );
        let revision = commit(&current);
        let original = source::load_world(&current.root, Some(&revision)).unwrap();
        let selected = frame(&original, Some(&authors), Lens::Character);
        let original_context = read(&original, &selected).unwrap();
        fs::remove_file(current.root.join(&authors.documents[WORK].source_path)).unwrap();
        fs::write(
            current.root.join(&authors.documents[SEGMENT].source_path),
            b"invalid live private source",
        )
        .unwrap();
        support::change_source(&current);
        commit(&current);
        let before = image(&current.root);
        assert_eq!(read(&original, &selected).unwrap(), original_context);
        assert_eq!(image(&current.root), before);
        unavailable(read(
            &source::load_world(&current.root, None).unwrap(),
            &frame(&current, Some(&authors), Lens::Character),
        ));
        unavailable(writing_frames::resolve(
            &original,
            Some(&authors),
            &selected,
        ));
        let mut supplied = None;
        writing_frames::with_author_snapshot(&original, &selected, false, |authors| {
            supplied = authors.cloned();
            writing_frames::resolve(&original, authors, &selected)
        })
        .unwrap();
        let captured = supplied.unwrap();
        assert_eq!(captured.revision.as_deref(), Some(revision.as_str()));
        assert!(
            captured
                .documents
                .values()
                .all(|record| record.blob_oid.as_ref().unwrap().len() == revision.len())
        );
        let mut elsewhere = original.clone();
        elsewhere.root = current.root.join("no-live-root");
        assert_eq!(
            writing_frames::context(&elsewhere, Some(&captured), &selected, &[], 100_000).unwrap(),
            original_context
        );
        let mut unavailable_revision = original.clone();
        unavailable_revision.revision = Some("f".repeat(revision.len()));
        unavailable(read(&unavailable_revision, &selected));
    }
}

#[test]
fn action_reviews_bind_the_work_without_admitting_private_source_as_facts() {
    let (_directory, world) = support::fixture();
    let authors = authors();
    put(&world, &authors);
    let mut selected = frame(&world, Some(&authors), Lens::Character);
    selected.at = StoryTime::new("main", support::TICK - 1, 0).unwrap();
    let request = action_drafts::ReviewRequest {
        draft: support::draft(&world, &support::id(1)),
        frame: selected.clone(),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    };
    let bound = action_drafts::review(&world, &request).unwrap();
    let mut plain_request = request.clone();
    plain_request.frame.work = None;
    let plain = action_drafts::review(&world, &plain_request).unwrap();
    assert_eq!(
        bound["context"]["characterPrompt"],
        plain["context"]["characterPrompt"]
    );
    assert_eq!(bound["frame"]["at"], json!(request.draft.horizon));
    assert_ne!(bound["reviewSha256"], plain["reviewSha256"]);
    assert!(!bound.to_string().contains("PRIVATE_WORK_TITLE"));
    assert_eq!(request.frame.at, selected.at);
    fs::remove_file(world.root.join(&authors.documents[WORK].source_path)).unwrap();
    unavailable(action_drafts::review(&world, &request));
    action_drafts::review(&world, &plain_request).unwrap();
}

#[test]
fn requirement_capture_and_comparison_validate_bound_work_without_changing_magic_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("story")).unwrap();
    for record in family::base().records.values() {
        fs::write(root.join(&record.source_path), &record.raw_bytes).unwrap();
    }
    let world = source::load_world(root, None).unwrap();
    let authors = authors();
    put(&world, &authors);
    let mut selected = frame(&world, Some(&authors), Lens::Author);
    selected.scene_id = None;
    selected.at = family::at(6, 0);
    let families = writing_frames::decode_families(
        &json!({"magic":{
            "author":{"rootId":family::id("concept",10),"selection":family::magic_selection()},
            "beliefTargets":[family::id("assertion",2)]
        }})
        .to_string(),
    )
    .unwrap();
    let requirement = action_requirements::Requirement {
        target_id: family::id("concept", 10),
        selector: action_requirements::Selector::Field {
            field: "restrictions".into(),
        },
        expected_value_raw: "\"Current canonical practice\"".into(),
    };
    for lens in [Lens::Author, Lens::Character, Lens::DramaticIrony] {
        selected.lens = lens;
        let mut plain = selected.clone();
        plain.work = None;
        let bound = action_requirements::capture(&world, &selected, &families, 100_000).unwrap();
        let unbound = action_requirements::capture(&world, &plain, &families, 100_000).unwrap();
        assert_eq!(bound["context"], unbound["context"]);
        assert_ne!(bound["captureSha256"], unbound["captureSha256"]);
        let comparison =
            action_requirements::compare(&world, &selected, &families, &requirement, 100_000)
                .unwrap();
        assert_eq!(
            comparison["outcome"],
            if lens == Lens::Author {
                "matches"
            } else {
                "unknown"
            }
        );
        assert!(!comparison.to_string().contains("PRIVATE_WORK_TITLE"));
    }
    fs::remove_file(world.root.join(&authors.documents[WORK].source_path)).unwrap();
    unavailable(action_requirements::capture(
        &world, &selected, &families, 100_000,
    ));
    unavailable(action_requirements::compare(
        &world,
        &selected,
        &families,
        &requirement,
        100_000,
    ));
}
