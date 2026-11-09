//! Private work identity, exact source, owner-scoped links and complete order.
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    manuscript,
    model::{Record, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const WORK: &str = "work_00000000000000000000000001";
const SEGMENT: &str = "segment_00000000000000000000000001";
const OTHER: &str = "segment_00000000000000000000000002";
const SECTION: &str = "description_00000000000000000000000001";

fn raw(fields: &Json, body: &str) -> String {
    format!("---\n{}---\n{body}", serde_yaml::to_string(fields).unwrap())
}
fn segment(id: &str, fields: Json, body: &str) -> Record {
    let mut value = json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":id,"title":"Private segment","source_links":[]});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    parse(&value, body).unwrap()
}
fn parse(fields: &Json, body: &str) -> wedl::Result<Record> {
    let kind = fields["kind"].as_str().unwrap_or("manuscript-segment");
    let id = fields["id"].as_str().unwrap_or(SEGMENT);
    let path =
        manuscript::document_path(kind, id).unwrap_or_else(|_| format!("author/segments/{id}.md"));
    manuscript::parse_document(raw(fields, body).as_bytes(), &path)
}
fn chapter(id: usize, segments: &[&str]) -> Json {
    json!({"id":format!("chapter_{id:026}"),"title":format!("Chapter {id}"),"segments":segments.iter().map(|id|json!({"kind":"author-record","id":id})).collect::<Vec<_>>()})
}
fn work(chapters: Vec<Json>, policy: &str) -> Record {
    parse(&json!({"schema":"wedl-author/v1","kind":"work","id":WORK,"title":"Private work","duplicate_policy":policy,"chapters":chapters}), "Private work notes\r\n").unwrap()
}
fn canonical(kind: &str, id: &str, extra: Json) -> Record {
    let mut fields = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    fields
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    source::parse_record(
        raw(&fields, "Opaque canonical prose\n").as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn world() -> World {
    let description = json!({"id":SECTION,"heading":"Original description","markdown":"Old account","valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]});
    let mut replacement = description.clone();
    replacement["id"] = json!("description_00000000000000000000000002");
    replacement["replaces"] = SECTION.into();
    replacement["markdown"] = "Corrected account".into();
    let records = vec![
        canonical(
            "world",
            WORLD,
            json!({"capabilities":["descriptions-v1"],"default_timeline":"main","timelines":[{"id":"main","label":"Main"}],"state_keys":{}}),
        ),
        canonical("character", PERSON, json!({})),
        canonical(
            "location",
            PLACE,
            json!({"descriptions":[description,replacement]}),
        ),
        canonical(
            "story-point",
            POINT,
            json!({"activation_policy":"suggest","lifecycle":{"initial_state":"dormant","transitions":[{"id":"resolved","state":"resolved","time":{"tick":20}}]}}),
        ),
        canonical(
            "knowledge",
            KNOWLEDGE,
            json!({"knower":PERSON,"claim":{"key":"place.open","statement":"The place is open"},"transitions":[{"id":"learned","time":{"tick":10},"state":"accepted","confidence":0.8,"acquisition":"observed"}]}),
        ),
    ];
    let world = source::world_from_records(PathBuf::new(), records, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}
fn snapshot(records: Vec<Record>) -> manuscript::Snapshot {
    manuscript::snapshot_from_records(records, None).unwrap()
}
fn resolve(world: &World, authors: &manuscript::Snapshot) -> Json {
    manuscript::resolve_work(world, authors, WORK, 4096, 1_000_000).unwrap()
}
fn fields(record: &Record) -> Json {
    serde_json::to_value(&record.frontmatter).unwrap()
}

#[test]
fn exact_markdown_source_preserves_lf_crlf_unicode_and_absent_final_newline() {
    let body = "\r\n  Private 🌲 prose  \r\nTrailing space  ";
    let original = segment(SEGMENT, json!({}), body);
    assert_eq!(original.body, body);
    for newline in ["\n", "\r\n"] {
        let header = serde_yaml::to_string(&fields(&original))
            .unwrap()
            .replace('\n', newline);
        let text = format!("---{newline}{header}---{newline}{body}");
        let parsed = manuscript::parse_document(text.as_bytes(), &original.source_path).unwrap();
        assert_eq!(parsed.body, body);
        assert_eq!(parsed.raw_bytes, text.as_bytes());
    }
    assert!(parse(&fields(&original), "").unwrap().body.is_empty());
}

#[test]
fn envelope_fields_id_path_types_and_duplicate_keys_are_closed() {
    let original = segment(SEGMENT, json!({}), "");
    let base = fields(&original);
    for (key, value) in [
        ("schema", json!("wedl/v1")),
        ("kind", json!("segment")),
        ("id", json!("segment_bad")),
        ("title", json!(" ")),
        ("title", json!("é".repeat(257))),
        ("scene_id", json!(PLACE)),
        ("reveal_all", json!(true)),
        ("source_links", Json::Null),
    ] {
        let mut candidate = base.clone();
        candidate[key] = value;
        assert!(parse(&candidate, "").is_err(), "{key}");
    }
    for key in ["schema", "kind", "id", "title", "source_links"] {
        let mut candidate = base.clone();
        candidate.as_object_mut().unwrap().remove(key);
        assert!(parse(&candidate, "").is_err(), "missing {key}");
    }
    let mut unicode_title = base.clone();
    unicode_title["title"] = json!("é".repeat(256));
    assert!(parse(&unicode_title, "").is_ok());
    for path in [
        format!("author/segments/{OTHER}.md"),
        format!("author/works/{SEGMENT}.md"),
        format!("author/segments/../{SEGMENT}.md"),
        format!("Author/segments/{SEGMENT}.md"),
    ] {
        assert!(manuscript::parse_document(&original.raw_bytes, &path).is_err());
    }
    let text = String::from_utf8(original.raw_bytes.clone()).unwrap();
    for extra in [
        "schema: wedl-author/v1\n",
        "title: !custom Private\n",
        "? [one, two]\n: value\n",
        "true: value\n",
    ] {
        let candidate = text.replacen("---\n", &format!("---\n{extra}"), 1);
        assert!(
            manuscript::parse_document(candidate.as_bytes(), &original.source_path).is_err(),
            "{extra}"
        );
    }
}

#[test]
fn bounded_expansion_accepts_finite_aliases_and_refuses_cycles_depth_and_bombs() {
    let original = segment(SEGMENT, json!({}), "");
    let header = format!(
        "---\nschema: wedl-author/v1\nkind: manuscript-segment\nid: {SEGMENT}\ntitle: Alias\n"
    );
    let finite =
        format!("{header}source_links:\n  - &link {{kind: record, id: {PLACE}}}\n  - *link\n---\n");
    let parsed = manuscript::parse_document(finite.as_bytes(), &original.source_path).unwrap();
    assert_eq!(parsed.list("source_links").unwrap().len(), 2);
    manuscript::validate_segment(&world(), &parsed).unwrap();
    for extra in [
        "source_links: &loop [*loop]\n".to_owned(),
        format!("source_links: {}null{}\n", "[".repeat(64), "]".repeat(64)),
        "source_links: .nan\n".into(),
        "source_links: !custom []\n".into(),
    ] {
        assert!(
            manuscript::parse_document(
                format!("{header}{extra}---\n").as_bytes(),
                &original.source_path
            )
            .is_err()
        );
    }
    let mut bomb = String::new();
    bomb.push_str("a: &a [x, x, x, x, x, x, x, x]\n");
    for (previous, next) in [('a', 'b'), ('b', 'c'), ('c', 'd'), ('d', 'e'), ('e', 'f')] {
        bomb.push_str(&format!("{next}: &{next} [*{previous}, *{previous}, *{previous}, *{previous}, *{previous}, *{previous}, *{previous}, *{previous}]\n"));
    }
    let error = manuscript::parse_document(
        format!("{header}{bomb}source_links: []\n---\n").as_bytes(),
        &original.source_path,
    )
    .unwrap_err();
    assert!(error.to_string().contains("bound"), "{error}");
}

#[test]
fn source_body_document_array_and_graph_bounds_are_complete() {
    let original = segment(SEGMENT, json!({}), "");
    assert!(parse(&fields(&original), &"x".repeat(manuscript::MAX_BODY_BYTES)).is_ok());
    assert!(
        parse(
            &fields(&original),
            &"x".repeat(manuscript::MAX_BODY_BYTES + 1)
        )
        .is_err()
    );
    assert!(
        manuscript::parse_document(
            &vec![b'x'; manuscript::MAX_DOCUMENT_BYTES + 1],
            &original.source_path
        )
        .is_err()
    );
    let mut value = fields(&original);
    value["source_links"] = json!(
        (0..1025)
            .map(|_| json!({"kind":"record","id":PLACE}))
            .collect::<Vec<_>>()
    );
    assert!(parse(&value, "").is_err());
    let empty_work = work(vec![], "reject");
    let mut value = fields(&empty_work);
    value["chapters"] = json!(
        (0..1024)
            .map(|n| chapter(n, &[SEGMENT, SEGMENT]))
            .collect::<Vec<_>>()
    );
    value["duplicate_policy"] = "allow-explicit-reuse".into();
    assert!(
        parse(&value, "")
            .unwrap_err()
            .to_string()
            .contains("structural")
    );
}

#[test]
fn segment_links_are_ordered_exact_owner_members_without_reveal_or_scene_inference() {
    let world = world();
    let links = json!([
        {"kind":"record","id":PLACE},
        {"kind":"section","owner_id":PLACE,"id":SECTION},
        {"kind":"transition","owner_id":POINT,"id":"resolved"},
        {"kind":"transition","owner_id":KNOWLEDGE,"id":"learned"},
        {"kind":"record","id":PLACE}
    ]);
    let record = segment(
        SEGMENT,
        json!({"source_links":links}),
        "Private text mentioning unrelated scene names",
    );
    manuscript::validate_segment(&world, &record).unwrap();
    assert_eq!(fields(&record)["source_links"], links);
    assert!(record.get("placement").is_none());
    assert!(record.get("pov_character_id").is_none());
    for link in [
        json!({"kind":"record","id":"A place alias"}),
        json!({"kind":"record","id":WORK}),
        json!({"kind":"section","owner_id":PERSON,"id":SECTION}),
        json!({"kind":"section","owner_id":PLACE,"id":"Original description"}),
        json!({"kind":"transition","owner_id":PLACE,"id":"learned"}),
        json!({"kind":"transition","owner_id":POINT,"id":"learned"}),
    ] {
        assert!(
            manuscript::validate_segment(
                &world,
                &segment(SEGMENT, json!({"source_links":[link]}), "")
            )
            .is_err()
        );
    }
    for link in [
        json!({"kind":"record","id":PLACE,"owner_id":PERSON}),
        json!({"kind":"section","id":SECTION}),
        json!({"kind":"author-record","id":SEGMENT}),
        json!({"kind":"record","id":null}),
    ] {
        let mut value = fields(&record);
        value["source_links"] = json!([link]);
        assert!(parse(&value, "").is_err());
    }
}

#[test]
fn placement_requires_declared_exact_signed_clocks_and_character_pov() {
    let world = world();
    let exact = json!({"timeline":"main","tick":i64::MIN,"order":i32::MAX});
    manuscript::validate_segment(
        &world,
        &segment(
            SEGMENT,
            json!({"placement":exact,"pov_character_id":PERSON}),
            "",
        ),
    )
    .unwrap();
    for placement in [
        Json::Null,
        json!({"timeline":"main","tick":1}),
        json!({"timeline":"main","tick":true,"order":0}),
        json!({"timeline":"main","tick":"9007199254740993","order":0}),
        json!({"timeline":"main","tick":9.5,"order":0}),
        json!({"timeline":"main","tick":9223372036854775808_u64,"order":0}),
        json!({"timeline":"main","tick":0,"order":2147483648_i64}),
        json!({"timeline":"main","tick":0,"order":0,"extra":0}),
    ] {
        let mut fields = fields(&segment(SEGMENT, json!({}), ""));
        fields["placement"] = placement;
        assert!(parse(&fields, "").is_err());
    }
    for extra in [
        json!({"placement":{"timeline":"missing","tick":0,"order":0}}),
        json!({"pov_character_id":PLACE}),
        json!({"pov_character_id":"Narrator"}),
    ] {
        assert!(manuscript::validate_segment(&world, &segment(SEGMENT, extra, "")).is_err());
    }
}

#[test]
fn work_chapters_are_closed_empty_work_valid_empty_chapter_invalid() {
    let world = world();
    let empty = work(vec![], "reject");
    assert_eq!(
        resolve(&world, &snapshot(vec![empty.clone()]))["chapters"],
        json!([])
    );
    let base = fields(&empty);
    for chapter in [
        json!({}),
        chapter(1, &[]),
        json!({"id":"chapter_bad","title":"Bad","segments":[{"kind":"author-record","id":SEGMENT}]}),
        json!({"id":"chapter_00000000000000000000000001","title":" ","segments":[{"kind":"author-record","id":SEGMENT}]}),
        json!({"id":"chapter_00000000000000000000000001","title":"Bad","segments":[{"kind":"record","id":SEGMENT}]}),
        json!({"id":"chapter_00000000000000000000000001","title":"Bad","segments":[{"kind":"author-record","id":WORK}]}),
    ] {
        let mut value = base.clone();
        value["chapters"] = json!([chapter]);
        assert!(parse(&value, "").is_err());
    }
    let mut value = base.clone();
    value["chapters"] = json!([chapter(1, &[SEGMENT]), chapter(1, &[OTHER])]);
    assert!(parse(&value, "").is_err());
    for policy in [json!("infer"), Json::Null, json!([])] {
        let mut value = base.clone();
        value["duplicate_policy"] = policy;
        assert!(parse(&value, "").is_err());
    }
}

#[test]
fn occurrence_identity_order_reuse_and_large_clocks_survive_shared_segment_report() {
    let world = world();
    let authors = snapshot(vec![
        segment(
            SEGMENT,
            json!({"placement":{"timeline":"main","tick":9007199254740993_i64,"order":-4},"pov_character_id":PERSON,"source_links":[{"kind":"record","id":PLACE}]}),
            "Later 🌲 prose\r\n  ",
        ),
        segment(
            OTHER,
            json!({"placement":{"timeline":"main","tick":-20,"order":0}}),
            "Earlier prose",
        ),
        work(
            vec![chapter(7, &[SEGMENT, OTHER]), chapter(2, &[SEGMENT])],
            "allow-explicit-reuse",
        ),
    ]);
    manuscript::validate_snapshot(&world, &authors).unwrap();
    let result = resolve(&world, &authors);
    assert_eq!(result["occurrenceCount"], 3);
    assert_eq!(result["uniqueSegmentCount"], 2);
    assert_eq!(
        result["chapters"][0]["id"],
        "chapter_00000000000000000000000007"
    );
    assert_eq!(
        result["chapters"][1]["occurrences"][0],
        json!({"chapterId":"chapter_00000000000000000000000002","segmentIndex":0,"ordinal":2,"segmentId":SEGMENT})
    );
    assert_eq!(result["chapters"][0]["occurrences"][1]["segmentId"], OTHER);
    assert_eq!(
        result["segments"][0]["bodyMarkdown"],
        authors.documents[SEGMENT].body
    );
    assert_eq!(
        result["segments"][0]["placement"]["tick"],
        "9007199254740993"
    );
    assert_eq!(result["segments"][0]["placement"]["order"], "-4");
    assert_eq!(result["complete"], true);
    assert_eq!(result["readerAuthorized"], false);
    assert_eq!(result["validationComplete"], false);
    assert_eq!(result["writesPerformed"], false);
    assert!(!result["chapters"].to_string().contains("Later"));
    let canonical_ids: Vec<_> = world.records.keys().cloned().collect();
    assert!(!canonical_ids.contains(&SEGMENT.into()));
    assert_eq!(world.fingerprint, self::world().fingerprint);
}

#[test]
fn duplicate_policy_is_work_wide_and_independent_works_can_reuse_segments() {
    let world = world();
    let mut value = fields(&work(vec![], "reject"));
    value["chapters"] = json!([chapter(1, &[SEGMENT]), chapter(2, &[SEGMENT])]);
    assert!(parse(&value, "").is_err());
    let first = work(vec![chapter(1, &[SEGMENT])], "reject");
    let mut other = fields(&first);
    other["id"] = "work_00000000000000000000000002".into();
    let authors = snapshot(vec![
        segment(SEGMENT, json!({}), ""),
        first,
        parse(&other, "").unwrap(),
    ]);
    manuscript::validate_snapshot(&world, &authors).unwrap();
}

#[test]
fn per_array_source_bounds_allow_more_than_1024_complete_work_occurrences() {
    let world = world();
    let authors = snapshot(vec![
        segment(SEGMENT, json!({}), ""),
        work(
            vec![chapter(1, &vec![SEGMENT; 1024]), chapter(2, &[SEGMENT])],
            "allow-explicit-reuse",
        ),
    ]);
    manuscript::validate_snapshot(&world, &authors).unwrap();
    assert_eq!(resolve(&world, &authors)["occurrenceCount"], 1025);
    assert!(manuscript::resolve_work(&world, &authors, WORK, 1024, 1_000_000).is_err());
    let mut value = fields(&authors.documents[WORK]);
    value["chapters"][0] = chapter(1, &vec![SEGMENT; 1025]);
    assert!(parse(&value, "").is_err());
}

#[test]
fn structural_inspection_and_selected_work_resolution_do_not_mask_broken_dependencies() {
    let world = world();
    let missing = "segment_00000000000000000000000009";
    let missing_work = snapshot(vec![work(vec![chapter(1, &[missing])], "reject")]);
    assert!(manuscript::validate_snapshot(&world, &missing_work).is_err());
    assert!(manuscript::resolve_work(&world, &missing_work, WORK, 10, 10000).is_err());
    let authors = snapshot(vec![
        segment(SEGMENT, json!({}), "Valid selected prose"),
        segment(
            OTHER,
            json!({"source_links":[{"kind":"record","id":"absent"}]}),
            "Inspect this broken source",
        ),
        work(vec![chapter(1, &[SEGMENT])], "reject"),
    ]);
    assert!(manuscript::validate_snapshot(&world, &authors).is_err());
    assert_eq!(resolve(&world, &authors)["uniqueSegmentCount"], 1);
    let mut bad_work = fields(&authors.documents[WORK]);
    bad_work["chapters"] = json!([chapter(1, &[OTHER])]);
    let selected_bad = snapshot(vec![
        authors.documents[OTHER].clone(),
        parse(&bad_work, "").unwrap(),
    ]);
    assert!(manuscript::resolve_work(&world, &selected_bad, WORK, 10, 10000).is_err());
}

#[test]
fn snapshots_bind_exact_source_projection_fingerprint_and_selected_revision() {
    let mut world = world();
    let authors = snapshot(vec![
        segment(SEGMENT, json!({}), "Original"),
        work(vec![chapter(1, &[SEGMENT])], "reject"),
    ]);
    for mutate in [0, 1, 2, 3] {
        let mut forged = authors.clone();
        match mutate {
            0 => forged.documents.get_mut(SEGMENT).unwrap().body = "Forged".into(),
            1 => forged.fingerprint = "a".repeat(64),
            2 => {
                forged.documents.get_mut(SEGMENT).unwrap().frontmatter["title"] =
                    serde_yaml::Value::String("Forged".into())
            }
            _ => {
                let record = forged.documents.remove(SEGMENT).unwrap();
                forged.documents.insert(OTHER.into(), record);
            }
        }
        assert!(manuscript::resolve_work(&world, &forged, WORK, 10, 10000).is_err());
    }
    for revision in ["HEAD", "WORKTREE", "abcd", &"A".repeat(40)] {
        assert!(manuscript::snapshot_from_records(vec![], Some(revision.into())).is_err());
    }
    for length in [40, 64] {
        let revision = "a".repeat(length);
        let mut records: Vec<_> = authors.documents.values().cloned().collect();
        for record in &mut records {
            record.blob_oid = Some("b".repeat(length));
        }
        let historical =
            manuscript::snapshot_from_records(records, Some(revision.clone())).unwrap();
        assert!(manuscript::resolve_work(&world, &historical, WORK, 10, 10000).is_err());
        world.revision = Some(revision.clone());
        assert_eq!(resolve(&world, &historical)["revision"], revision);
        world.revision = None;
    }
}

#[test]
fn aggregate_snapshot_document_and_byte_bounds_reject_before_semantic_resolution() {
    let original = segment(SEGMENT, json!({}), "");
    assert!(manuscript::snapshot_from_records(vec![original.clone(); 1025], None).is_err());
    assert!(manuscript::snapshot_from_records(vec![original.clone(), original], None).is_err());
    let mut records = Vec::new();
    for n in 0..16 {
        records.push(segment(
            &format!("segment_{n:026}"),
            json!({}),
            &"x".repeat(manuscript::MAX_BODY_BYTES),
        ));
    }
    assert!(
        manuscript::snapshot_from_records(records, None)
            .unwrap_err()
            .to_string()
            .contains("16 MiB")
    );
}

#[test]
fn complete_snapshot_bounds_count_expanded_alias_text_across_documents() {
    let payload = "x".repeat(240_000);
    let mut records = Vec::new();
    for number in 0..10 {
        let id = format!("segment_{number:026}");
        let raw = format!(
            "---\nschema: wedl-author/v1\nkind: manuscript-segment\nid: {id}\ntitle: Expanded\nsource_links:\n  - &link {{kind: record, id: {payload}}}\n{}---\n",
            "  - *link\n".repeat(7)
        );
        records.push(
            manuscript::parse_document(
                raw.as_bytes(),
                &manuscript::document_path("manuscript-segment", &id).unwrap(),
            )
            .unwrap(),
        );
    }
    assert!(
        records
            .iter()
            .map(|record| record.raw_bytes.len())
            .sum::<usize>()
            < manuscript::MAX_SNAPSHOT_BYTES
    );
    let error = manuscript::snapshot_from_records(records, None).unwrap_err();
    assert!(
        error.to_string().contains("complete expanded text"),
        "{error}"
    );
}

#[test]
fn report_bounds_count_complete_serialized_unicode_and_never_truncate_or_repeat_prose() {
    let world = world();
    let authors = snapshot(vec![
        segment(SEGMENT, json!({}), &"🌲".repeat(200)),
        work(
            vec![chapter(1, &[SEGMENT, SEGMENT])],
            "allow-explicit-reuse",
        ),
    ]);
    let report = resolve(&world, &authors);
    assert_eq!(report["proseCharacters"], 400);
    let mut expected = report.clone();
    let mut limit = serde_json::to_string(&expected).unwrap().chars().count();
    for _ in 0..4 {
        expected["bounds"]["maxCharacters"] = limit.into();
        limit = serde_json::to_string(&expected).unwrap().chars().count();
    }
    let exact = manuscript::resolve_work(&world, &authors, WORK, 4096, limit).unwrap();
    assert_eq!(
        serde_json::to_string(&exact).unwrap().chars().count(),
        limit
    );
    assert!(manuscript::resolve_work(&world, &authors, WORK, 4096, limit - 1).is_err());
    assert!(manuscript::resolve_work(&world, &authors, WORK, 1, 1_000_000).is_err());
    for (occurrences, characters) in [(0, 10000), (4097, 10000), (10, 255), (10, 1_000_001)] {
        assert!(manuscript::resolve_work(&world, &authors, WORK, occurrences, characters).is_err());
    }
    assert!(manuscript::resolve_work(&world, &authors, SEGMENT, 10, 10000).is_err());
}
