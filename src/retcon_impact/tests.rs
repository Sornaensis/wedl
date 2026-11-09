use super::*;

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const OTHER: &str = "obj_00000000000000000000000002";
const EVENT: &str = "event_00000000000000000000000001";
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SEGMENT: &str = "segment_00000000000000000000000001";
const WORK: &str = "work_00000000000000000000000001";

fn author_record(kind: &str, id: &str, fields: &str, body: &str) -> Record {
    manuscript::parse_document(
        format!("---\nschema: wedl-author/v1\nkind: {kind}\nid: {id}\ntitle: Private {kind}\n{fields}---\n{body}").as_bytes(),
        &manuscript::document_path(kind, id).unwrap(),
    ).unwrap()
}
fn authors(records: Vec<Record>) -> manuscript::Snapshot {
    manuscript::snapshot_from_records(records, None).unwrap()
}
fn segment(id: &str, target: &str, extra: &str, body: &str) -> Record {
    author_record(
        "manuscript-segment",
        id,
        &format!("source_links: [{{kind: record, id: {target}}}]\n{extra}"),
        body,
    )
}
fn work(segment_id: &str) -> Record {
    author_record(
        "work",
        WORK,
        &format!(
            "duplicate_policy: allow-explicit-reuse\nchapters:\n - id: chapter_00000000000000000000000001\n   title: First\n   segments: [{{kind: author-record, id: {segment_id}}}, {{kind: author-record, id: {segment_id}}}]\n - id: chapter_00000000000000000000000002\n   title: Second\n   segments: [{{kind: author-record, id: {segment_id}}}]\n"
        ),
        "Work notes.",
    )
}

fn record(kind: &str, id: &str, fields: &str, body: &str) -> Record {
    crate::source::parse_record(
        format!(
            "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Test {kind}\n{fields}---\n{body}"
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn fixture(extra: Vec<Record>) -> World {
    let mut records = vec![
        record(
            "world",
            WORLD,
            "capabilities: [lore-core-v1, descriptions-v1, briefs-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nthreads: []\nstate_keys:\n  character:\n    score: {type: integer}\n    number: {type: number}\n    unused: {type: string}\n  object:\n    label: {type: string}\n    holder: {type: entity, kind: character}\nrelationship_metrics: {}\n",
            "World body.\n",
        ),
        record(
            "character",
            ANA,
            "initial_state: {score: 9007199254740993, number: 1.0}\n",
            "# 20 - no parsed date heading\n\r\nOpaque body  \n",
        ),
    ];
    records.extend(extra);
    let world = crate::source::world_from_records(
        std::env::temp_dir().join("wedl-retcon-witness-test"),
        records,
        None,
    )
    .unwrap();
    let valid = validation::validate_world(&world);
    assert!(
        valid.valid,
        "{:?}",
        valid
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| (&d.code, &d.message))
            .take(5)
            .collect::<Vec<_>>()
    );
    world
}
fn request() -> Request {
    Request {
        target: record_ref(ANA),
        at: Time {
            timeline: "main".into(),
            tick: "10".into(),
            order: "0".into(),
        },
        depth: 32,
        max_items: 1000,
    }
}
fn replace(world: &World, record: Record) -> World {
    let mut records: Vec<_> = world
        .records
        .values()
        .filter(|item| item.id != record.id)
        .cloned()
        .collect();
    records.push(record);
    crate::source::world_from_records(world.root.clone(), records, None).unwrap()
}
fn edit(world: &World, id: &str, f: impl FnOnce(&mut Yaml, &mut String)) -> World {
    let original = &world.records[id];
    let mut fields = original.frontmatter.clone();
    let mut body = original.body.clone();
    f(&mut fields, &mut body);
    let record = crate::source::parse_record(
        format!(
            "---\n{}---\n{body}",
            serde_yaml::to_string(&fields).unwrap()
        )
        .as_bytes(),
        &original.source_path,
    )
    .unwrap();
    replace(world, record)
}
fn captured(world: &World) -> Captured {
    valid(world);
    capture(world, world, &request(), &[ANA.into()], DIGEST).unwrap()
}
fn current(world: &World, original: &Captured) -> Json {
    valid(world);
    witness(world, &request(), &original.seeds, &original.path_hints).unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(
        report.valid,
        "{:?}",
        report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| (&d.code, &d.message))
            .take(5)
            .collect::<Vec<_>>()
    );
}
fn section<'a>(witness: &'a Json, owner: &str, field: &str) -> &'a str {
    witness["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["ref"] == json!({"kind":"record","id":owner}))
        .unwrap()["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["field"] == field)
        .unwrap()["valueRaw"]
        .as_str()
        .unwrap()
}

#[test]
fn witness_discovers_new_incoming_references_from_retained_original_seeds() {
    let base = fixture(vec![]);
    let original = captured(&base);
    let added = replace(
        &base,
        record(
            "object",
            OBJECT,
            &format!("initial_state: {{holder: {{entity: {ANA}}}}}\n"),
            "New dependency\n",
        ),
    );
    let now = current(&added, &original);
    assert_ne!(now, original.witness);
    assert!(
        now["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["ref"]["id"] == OBJECT && item["present"] == true)
    );
    assert!(
        now["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["owner"]["id"] == OBJECT && item["target"]["id"] == ANA)
    );
    assert!(
        !original
            .seeds
            .iter()
            .any(|reference| reference.owner() == OBJECT)
    );
}

#[test]
fn witness_values_are_browser_exact_and_markdown_stays_opaque() {
    let world = fixture(vec![]);
    let original = captured(&world);
    assert_eq!(
        section(&original.witness, ANA, "initial_state"),
        "{\"number\":1.0,\"score\":9007199254740993}"
    );
    assert_eq!(
        serde_json::from_str::<String>(section(&original.witness, ANA, "bodyMarkdown")).unwrap(),
        world.records[ANA].body
    );
    let changed = edit(&world, ANA, |front, _| {
        front["initial_state"]["number"] = Yaml::Number(1.into())
    });
    assert_ne!(current(&changed, &original), original.witness);
    for input in original.witness["inputs"].as_array().unwrap() {
        for cell in input["sections"].as_array().unwrap() {
            assert!(cell["valueRaw"].is_string());
            assert!(cell.get("value").is_none());
        }
    }
    for declaration in original.witness["consulted"].as_array().unwrap() {
        assert!(declaration["valueRaw"].is_string());
        assert!(declaration.get("value").is_none());
    }
}

#[test]
fn unrelated_source_unused_declarations_and_snapshot_identities_do_not_change_witness() {
    let base = fixture(vec![record(
        "object",
        OTHER,
        "initial_state: {label: unrelated}\n",
        "Unrelated\n",
    )]);
    let original = captured(&base);
    let changed = edit(&base, OTHER, |front, body| {
        front["title"] = "Unrelated rename".into();
        *body = "Unrelated changed\n".into();
    });
    let mut changed = edit(&changed, WORLD, |front, _| {
        front["state_keys"]["character"]["unused"] = serde_yaml::from_str("type: number").unwrap()
    });
    changed.revision = Some("b".repeat(40));
    assert_ne!(changed.fingerprint, base.fingerprint);
    for record in changed.records.values_mut() {
        record.blob_oid = Some("d".repeat(40));
    }
    assert_eq!(current(&changed, &original), original.witness);
    assert!(
        original.witness["consulted"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["field"] == "state_keys.character.score")
    );
    assert!(
        !original.witness["consulted"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["field"] == "state_keys.character.unused")
    );
}

fn descriptions(world: &World, future_first: bool, reverse_admitted: bool) -> World {
    edit(world, ANA, |front, _| {
        let first: Yaml = serde_yaml::from_str("id: description_00000000000000000000000001\nheading: First\nmarkdown: First source\nvalid: {kind: timeless}\naudience: [author]\nperspectives: [author]\n").unwrap();
        let correction: Yaml = serde_yaml::from_str("id: description_00000000000000000000000002\nheading: Correction\nmarkdown: Corrected source\nvalid: {kind: timeless}\naudience: [author]\nperspectives: [author]\nreplaces: description_00000000000000000000000001\n").unwrap();
        let alternative: Yaml = serde_yaml::from_str("id: description_00000000000000000000000003\nheading: Alternative\nmarkdown: Alternative source\nvalid: {kind: timeless}\naudience: [author]\nperspectives: [author]\n").unwrap();
        let mut values = if reverse_admitted {
            vec![first, alternative, correction]
        } else {
            vec![first, correction, alternative]
        };
        if future_first {
            values.insert(0, serde_yaml::from_str("id: description_00000000000000000000000004\nheading: Future\nmarkdown: Future source\nvalid: {kind: interval, timeline: main, start: {timeline: main, tick: 20, order: 0}}\naudience: [author]\nperspectives: [author]\n").unwrap());
        }
        front["descriptions"] = Yaml::Sequence(values);
    })
}

#[test]
fn withheld_insertion_normalizes_witness_positions_but_report_keeps_original_ordinals() {
    let base = descriptions(&fixture(vec![]), false, false);
    let original = captured(&base);
    let shifted = descriptions(&base, true, false);
    assert_eq!(current(&shifted, &original), original.witness);
    let shifted_report = evaluate(&shifted, &shifted, &request(), DIGEST).unwrap();
    assert_ne!(shifted_report["items"], original.report["items"]);
    assert_eq!(original.report["items"][0]["citation"]["sourceOrdinal"], 1);
    assert_eq!(shifted_report["items"][0]["citation"]["sourceOrdinal"], 2);
    let reordered = descriptions(&base, true, true);
    assert_ne!(current(&reordered, &original), original.witness);
}

#[test]
fn retained_member_disappearance_and_native_path_moves_change_witness() {
    let base = descriptions(&fixture(vec![]), false, false);
    let original = captured(&base);
    let renamed = edit(&base, ANA, |front, _| {
        front["descriptions"][2]["id"] = "description_00000000000000000000000005".into();
    });
    let now = current(&renamed, &original);
    assert!(
        now["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |item| item["ref"]["id"] == "description_00000000000000000000000003"
                    && item["present"] == false
                    && item["path"] == format!("story/{ANA}.md")
            )
    );
    let mut moved = base.clone();
    moved.records.get_mut(ANA).unwrap().source_path = format!("story/moved-{ANA}.md");
    moved.fingerprint = crate::source::fingerprint_for_records(&moved.records);
    assert_ne!(current(&moved, &original), original.witness);
}

#[test]
fn original_candidate_touched_new_record_stays_as_an_absent_seed() {
    let base = fixture(vec![]);
    let candidate = replace(
        &base,
        record(
            "object",
            OBJECT,
            "initial_state: {label: created}\n",
            "New source\n",
        ),
    );
    let original = capture(&base, &candidate, &request(), &[OBJECT.into()], DIGEST).unwrap();
    assert!(
        original.witness["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["ref"]["id"] == OBJECT
                && item["present"] == false
                && item["path"] == format!("story/{OBJECT}.md"))
    );
    assert_ne!(current(&candidate, &original), original.witness);
}

#[test]
fn withheld_effect_values_never_select_consulted_declarations() {
    let base = fixture(vec![
        record(
            "object",
            OBJECT,
            "audience: [gm]\ninitial_state: {label: hidden}\n",
            "Hidden source\n",
        ),
        record(
            "event",
            EVENT,
            &format!(
                "time: {{tick: 5}}\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: score, operation: set, value: 1}}, {{id: effect_00000000000000000000000002, target: {OBJECT}, key: label, operation: set, value: hidden}}]\n"
            ),
            "",
        ),
    ]);
    let original = captured(&base);
    assert!(
        !original.witness["consulted"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["field"] == "state_keys.object.label")
    );
    let changed = edit(&base, EVENT, |front, _| {
        front["effects"][1]["value"] = "Changed hidden payload".into();
    });
    assert_eq!(current(&changed, &original), original.witness);
    assert!(!section(&original.witness, EVENT, "effects").contains(OBJECT));
}

#[test]
fn required_brief_member_admission_withholds_whole_brief() {
    let base = descriptions(&fixture(vec![]), true, false);
    let brief = "brief_00000000000000000000000001";
    let world = replace(
        &base,
        record(
            "context-brief",
            brief,
            &format!(
                "domain: test.brief\nvalid: {{kind: timeless}}\naudience: [author]\nperspectives: [author]\nscope: world\nrequired_refs: [{{kind: section, owner_id: {ANA}, id: description_00000000000000000000000004}}]\n"
            ),
            "Withheld brief\n",
        ),
    );
    let original = captured(&world);
    assert!(
        !original.witness["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["ref"]["id"] == brief)
    );
    assert!(
        !original.report["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["owner"]["id"] == brief)
    );
}

#[test]
fn complete_witness_byte_bound_never_returns_a_truncated_value() {
    let base = fixture(vec![]);
    let large = edit(&base, ANA, |_, body| *body = "a".repeat(MAX_WITNESS_BYTES));
    assert!(
        capture(&large, &large, &request(), &[ANA.into()], DIGEST)
            .unwrap_err()
            .to_string()
            .contains("1048576")
    );
}

#[test]
fn private_source_edges_keep_exact_positions_pov_reuse_and_original_provenance() {
    let know = "know_00000000000000000000000001";
    let world = descriptions(
        &fixture(vec![record(
            "knowledge",
            know,
            &format!(
                "knower: {ANA}\nclaim: {{key: fixture, statement: Known., truth: unknown}}\ntransitions: [{{id: learned, time: {{tick: 5}}, state: uncertain, acquisition: heard, confidence: 0.5}}]\n"
            ),
            "",
        )]),
        false,
        false,
    );
    let source = author_record(
        "manuscript-segment",
        SEGMENT,
        &format!(
            "source_links:\n - {{kind: record, id: {ANA}}}\n - {{kind: section, owner_id: {ANA}, id: description_00000000000000000000000001}}\n - {{kind: transition, owner_id: {know}, id: learned}}\npov_character_id: {ANA}\nplacement: {{timeline: main, tick: 10, order: 0}}\n"
        ),
        "Opaque author body.",
    );
    let authors = authors(vec![source, work(SEGMENT)]);
    let report = evaluate_with_authors(&world, &world, &authors, &request(), DIGEST).unwrap();
    let private: Vec<_> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["owner"]["kind"] == "author-record")
        .collect();
    assert_eq!(private.len(), 14);
    assert!(
        private
            .iter()
            .any(|item| item["relation"] == "manuscript-source"
                && item["citation"]["section"] == "source_links[2]"
                && item["target"]["kind"] == "transition")
    );
    assert!(
        private
            .iter()
            .any(|item| item["relation"] == "manuscript-pov"
                && item["citation"]["sourceOrdinal"].is_null())
    );
    for item in &private {
        assert_eq!(
            item["citation"]["provenance"],
            json!({"kind":"source","revision":null,"blobOid":null})
        );
        assert_eq!(item["citation"]["time"], Json::Null);
        assert_eq!(
            item["citation"]["sourcePath"],
            manuscript::document_path(
                if item["owner"]["id"] == WORK {
                    "work"
                } else {
                    "manuscript-segment"
                },
                item["owner"]["id"].as_str().unwrap()
            )
            .unwrap()
        );
    }
    let work_rows: Vec<_> = private
        .iter()
        .filter(|item| item["snapshot"] == "base" && item["relation"] == "work-segment")
        .collect();
    assert_eq!(work_rows.len(), 3);
    assert_eq!(
        work_rows[0]["citation"]["section"],
        "chapters[0].segments[0]"
    );
    assert_eq!(work_rows[1]["citation"]["sourceOrdinal"], 1);
    assert_eq!(
        work_rows[2]["citation"]["memberId"],
        "chapter_00000000000000000000000002"
    );
    let mut author_request = request();
    author_request.target = Reference::AuthorRecord { id: SEGMENT.into() };
    assert_eq!(
        evaluate_with_authors(&world, &world, &authors, &author_request, DIGEST).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(matches!(
        evaluate(&world, &world, &author_request, DIGEST),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn private_placement_excludes_future_sources_and_cannot_bridge_to_work() {
    let world = fixture(vec![]);
    let future = authors(vec![
        segment(
            SEGMENT,
            ANA,
            "placement: {timeline: main, tick: 10, order: 1}\n",
            "Future private prose",
        ),
        work(SEGMENT),
    ]);
    let report = evaluate_with_authors(&world, &world, &future, &request(), DIGEST).unwrap();
    assert_eq!(report["items"], json!([]));
    let mut author_request = request();
    author_request.target = Reference::AuthorRecord { id: SEGMENT.into() };
    assert!(matches!(
        evaluate_with_authors(&world, &world, &future, &author_request, DIGEST),
        Err(Error::NotFound(_))
    ));
    let admitted = authors(vec![
        segment(
            SEGMENT,
            ANA,
            "placement: {timeline: main, tick: 10, order: 0}\n",
            "Current private prose",
        ),
        work(SEGMENT),
    ]);
    assert_eq!(
        evaluate_with_authors(&world, &world, &admitted, &request(), DIGEST).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
}

#[test]
fn candidate_member_removal_keeps_private_original_without_revalidating_its_link() {
    let world = descriptions(&fixture(vec![]), false, false);
    let source = author_record(
        "manuscript-segment",
        SEGMENT,
        &format!(
            "source_links: [{{kind: section, owner_id: {ANA}, id: description_00000000000000000000000001}}]\n"
        ),
        "Retained source evidence.",
    );
    let authors = authors(vec![source, work(SEGMENT)]);
    let candidate = edit(&world, ANA, |fields, _| {
        fields.as_mapping_mut().unwrap().remove("descriptions");
    });
    assert!(manuscript::validate_snapshot(&candidate, &authors).is_err());
    let report = evaluate_with_authors(&world, &candidate, &authors, &request(), DIGEST).unwrap();
    assert!(
        report["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["owner"]["kind"] == "author-record")
            .all(|item| item["snapshot"] == "base")
    );
    let captured = capture_with_authors(
        &world,
        &candidate,
        &authors,
        &request(),
        &[ANA.into()],
        DIGEST,
    )
    .unwrap();
    let changed = witness_with_authors(
        &candidate,
        &authors,
        &request(),
        &captured.seeds,
        &captured.path_hints,
    )
    .unwrap();
    assert_ne!(changed, captured.witness);
    assert!(
        changed["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input["ref"]["id"] == SEGMENT && input["present"] == true)
    );
}

#[test]
fn private_witness_changes_only_selected_documents_and_new_incoming_dependencies() {
    let world = fixture(vec![]);
    let unrelated = "segment_00000000000000000000000002";
    let future = "segment_00000000000000000000000003";
    let incoming = "segment_00000000000000000000000004";
    let source = segment(SEGMENT, ANA, "", "Selected body\r\nTrailing  ");
    let make = |unrelated_body: &str, future_body: &str| {
        authors(vec![
            source.clone(),
            work(SEGMENT),
            author_record(
                "manuscript-segment",
                unrelated,
                "source_links: []\n",
                unrelated_body,
            ),
            segment(
                future,
                ANA,
                "placement: {timeline: main, tick: 20, order: 0}\n",
                future_body,
            ),
        ])
    };
    let base = make("Old unrelated", "Old future");
    let captured =
        capture_with_authors(&world, &world, &base, &request(), &[ANA.into()], DIGEST).unwrap();
    let changed = make(&format!("A prose mention of {ANA}"), "Changed future");
    assert_ne!(base.fingerprint, changed.fingerprint);
    assert_eq!(
        witness_with_authors(
            &world,
            &changed,
            &request(),
            &captured.seeds,
            &captured.path_hints
        )
        .unwrap(),
        captured.witness
    );
    let selected_input = captured.witness["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["ref"]["id"] == SEGMENT)
        .unwrap();
    assert_eq!(
        selected_input["sections"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["valueRaw"],
        serde_json::to_string(&source.body).unwrap()
    );
    let mut records: Vec<_> = changed.documents.values().cloned().collect();
    records.push(segment(incoming, ANA, "", "New declared incoming link"));
    let changed = authors(records);
    assert_ne!(
        witness_with_authors(
            &world,
            &changed,
            &request(),
            &captured.seeds,
            &captured.path_hints
        )
        .unwrap(),
        captured.witness
    );
    let changed = authors(vec![
        segment(SEGMENT, ANA, "", "Changed selected body"),
        work(SEGMENT),
    ]);
    assert_ne!(
        witness_with_authors(
            &world,
            &changed,
            &request(),
            &captured.seeds,
            &captured.path_hints
        )
        .unwrap(),
        captured.witness
    );
}

#[test]
fn private_witness_and_occurrence_bounds_refuse_complete_oversized_results() {
    let world = fixture(vec![]);
    let large = authors(vec![segment(
        SEGMENT,
        ANA,
        "",
        &"x".repeat(manuscript::MAX_BODY_BYTES),
    )]);
    assert!(evaluate_with_authors(&world, &world, &large, &request(), DIGEST).is_ok());
    assert!(
        capture_with_authors(&world, &world, &large, &request(), &[ANA.into()], DIGEST)
            .unwrap_err()
            .to_string()
            .contains("1048576")
    );
    let repeated = author_record(
        "manuscript-segment",
        SEGMENT,
        &format!(
            "source_links: [{}]\n",
            vec![format!("{{kind: record, id: {ANA}}}"); 1000].join(",")
        ),
        "",
    );
    let many = authors(vec![repeated]);
    assert!(
        evaluate_with_authors(&world, &world, &many, &request(), DIGEST)
            .unwrap_err()
            .to_string()
            .contains("bound")
    );
}

#[test]
fn private_graph_refuses_forged_raw_projection_path_and_revision() {
    let world = fixture(vec![]);
    let base = authors(vec![segment(SEGMENT, ANA, "", "Exact bytes")]);
    assert!(evaluate_with_authors(&world, &world, &base, &request(), DIGEST).is_ok());
    let mut forged = base.clone();
    forged.documents.get_mut(SEGMENT).unwrap().body = "Invented projection".into();
    assert!(evaluate_with_authors(&world, &world, &forged, &request(), DIGEST).is_err());
    let mut forged = base.clone();
    forged.documents.get_mut(SEGMENT).unwrap().source_path = "author/segments/../wrong.md".into();
    assert!(evaluate_with_authors(&world, &world, &forged, &request(), DIGEST).is_err());
    let mut forged = base;
    forged.revision = Some("a".repeat(40));
    assert!(evaluate_with_authors(&world, &world, &forged, &request(), DIGEST).is_err());
}
