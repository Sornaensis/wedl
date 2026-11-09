//! Scene/conversation drafts use the existing validated one-record writer.
use std::{fs, path::PathBuf};

use serde_json::Value as Json;
use wedl::{
    Error, authoring, character,
    model::{Record, World},
    record_edit, source, validation,
};

const HERO: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const CONVERSATION: &str = "conv_00000000000000000000000001";

struct Fixture {
    _directory: tempfile::TempDir,
    world: World,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut world = source::init_world(&directory.path().join("world"), "Scenes").unwrap();
        for (kind, id, title) in [
            ("character", HERO, "Ana"),
            ("character", OTHER, "Bo"),
            ("location", PLACE, "Harbor"),
        ] {
            let record =
                record_edit::create(&world, &format!("kind: {kind}\nid: {id}\ntitle: {title}\n"))
                    .unwrap();
            let path = world.root.join(&record.source_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, record.raw_bytes).unwrap();
            world = source::load_world(&world.root, None).unwrap();
        }
        assert!(validation::validate_world(&world).valid);
        Self {
            _directory: directory,
            world,
        }
    }

    fn create(&mut self, request: &str) -> Json {
        let preview =
            authoring::preview_record_create(&self.world, request, &self.world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], true, "{preview}");
        assert_eq!(
            source::load_world(&self.world.root, None)
                .unwrap()
                .fingerprint,
            self.world.fingerprint
        );
        let result = authoring::apply_record_create(
            &self.world,
            request,
            &self.world.fingerprint,
            preview["afterSha256"].as_str().unwrap(),
        )
        .unwrap();
        self.world = source::load_world(&self.world.root, None).unwrap();
        assert_eq!(preview["candidateFingerprint"], self.world.fingerprint);
        assert_eq!(result["fingerprint"], self.world.fingerprint);
        let record = &self.world.records[preview["entityId"].as_str().unwrap()];
        assert_eq!(
            record.raw_bytes,
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
        preview
    }

    fn update(&mut self, id: &str, request: &str) -> Json {
        let preview =
            authoring::preview_record_update(&self.world, id, request, &self.world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], true, "{preview}");
        assert_eq!(
            source::load_world(&self.world.root, None)
                .unwrap()
                .fingerprint,
            self.world.fingerprint
        );
        authoring::apply_record_update(
            &self.world,
            id,
            request,
            &self.world.fingerprint,
            preview["afterSha256"].as_str().unwrap(),
        )
        .unwrap();
        self.world = source::load_world(&self.world.root, None).unwrap();
        assert_eq!(preview["candidateFingerprint"], self.world.fingerprint);
        preview
    }
}

fn scene_request(id: &str) -> String {
    format!(
        "kind: scene\nid: {id}\ntitle: Harbor Meeting\nlocation: {PLACE}\ntime: {{start: {{tick: 0}}, current: {{tick: 0}}, end: {{tick: 10}}}}\nparticipants: [{{character: {HERO}, from: {{tick: 0}}, until: {{tick: 10}}, role: witness, point_of_view: true}}]\nobservations: [{{id: lantern, at: {{tick: 0}}, until: {{tick: 10}}, text: The lantern is lit, audience: [participants], salience: 1.5}}]\nbody: |\n  # Harbor Meeting\n  Original scene prose.\n"
    )
}

fn conversation_request() -> String {
    format!(
        "kind: conversation\nid: {CONVERSATION}\ntitle: A Distant Caller\nlocation: {PLACE}\ntime: {{start: {{tick: 0}}, end: {{tick: 10}}}}\nparticipants: [{{character: {HERO}}}, {{character: {OTHER}}}]\nturns:\n- {{id: greeting, at: {{tick: 0}}, speaker: {OTHER}, text: Bring the lantern, audience: [participants], delivery: Quietly, addressee: {HERO}}}\n- {{id: wave, kind: action, at: {{tick: 1}}, actors: [{HERO}], text: Waves goodbye, audience: [participants]}}\nrecollections:\n- id: memory\n  character: {HERO}\n  at: {{tick: 11}}\n  state: remembered\n  confidence: 0.8\n  summary: We discussed a lantern\n  interpretation: A useful signal\n  emotional_impression: Hopeful\n  exact_turns: [greeting]\n  remembered_quotes: [{{source_turn: greeting, speaker: {OTHER}, text: Bring a light, fidelity: approximate}}]\nbody: |\n  # A Distant Caller\n  Original conversation prose.\n"
    )
}

fn candidate(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}

fn refused_update(fixture: &Fixture, id: &str, request: &str) -> Json {
    let world = &fixture.world;
    let preview = authoring::preview_record_update(world, id, request, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false, "accepted {request}: {preview}");
    assert!(matches!(
        authoring::apply_record_update(
            world,
            id,
            request,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Validation(_))
    ));
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    preview
}

#[test]
fn minimal_drafts_default_to_planned_or_draft_without_inventing_evidence() {
    let fixture = Fixture::new();
    for (kind, id, status, directory, time, collections) in [
        (
            "scene",
            SCENE,
            "planned",
            "scenes",
            "{start: {tick: 0}, current: {tick: 0}}",
            &[
                "participants",
                "objects",
                "environments",
                "conversations",
                "story_points",
                "outcome_events",
                "observations",
            ][..],
        ),
        (
            "conversation",
            CONVERSATION,
            "draft",
            "conversations",
            "{start: {tick: 0}}",
            &["participants", "turns", "recollections"][..],
        ),
    ] {
        let request =
            format!("kind: {kind}\nid: {id}\ntitle: Empty\nlocation: {PLACE}\ntime: {time}\n");
        let record = record_edit::create(&fixture.world, &request).unwrap();
        assert_eq!(record.status(), status);
        assert_eq!(record.source_path, format!("story/{directory}/{id}.md"));
        assert_eq!(record.string("schema"), Some(source::SOURCE_SCHEMA));
        assert!(record.body.is_empty());
        for field in collections {
            assert!(record.list(field).unwrap().is_empty(), "{field}");
        }
        assert!(record.get("scene").is_none());
        let world = candidate(&fixture.world, record);
        let report = validation::validate_world(&world);
        assert!(report.valid, "{:?}", report.diagnostics);
        let context =
            character::context(&world, HERO, &world.story_time(20, None, 0).unwrap(), 16000)
                .unwrap();
        for field in ["beliefs", "observations", "conversations"] {
            assert!(context[field].as_array().unwrap().is_empty());
        }
        for missing in ["location", "time"] {
            let request: String = request
                .lines()
                .filter(|line| !line.starts_with(&format!("{missing}:")))
                .map(|line| format!("{line}\n"))
                .collect();
            assert!(matches!(
                record_edit::create(&fixture.world, &request),
                Err(Error::InvalidInput(_))
            ));
        }
    }
    assert!(!fixture.world.root.join("story/scenes").exists());
    assert!(!fixture.world.root.join("story/conversations").exists());
    assert!(!fixture.world.root.join(".wedl-rust").exists());
}

#[test]
fn malformed_nested_requests_fail_before_a_candidate_is_produced() {
    let mut fixture = Fixture::new();
    fixture.create(&scene_request(SCENE));
    fixture.create(&conversation_request());
    let bad = [
        (SCENE, "kind: scene\nstatus: canonical\n".to_owned()),
        (CONVERSATION, "kind: conversation\nstatus: planned\n".to_owned()),
        (SCENE, "kind: scene\ntime: {start: {tick: 0}}\n".to_owned()),
        (SCENE, "kind: scene\ntime: {start: {tick: 0}, current: {tick: 1.0}}\n".to_owned()),
        (CONVERSATION, "kind: conversation\ntime: {start: {tick: 0}, current: {tick: 0}}\n".to_owned()),
        (SCENE, format!("kind: scene\nparticipants: [{{character: {HERO}, to: {{tick: 5}}}}]\n")),
        (SCENE, format!("kind: scene\nparticipants: [{{character: {HERO}, point_of_view: yes}}]\n")),
        (SCENE, "kind: scene\nobservations: [{id: o, text: Missing time}]\n".to_owned()),
        (SCENE, "kind: scene\nobservations: [{id: o, at: {tick: 0}, text: Bad, salience: .nan}]\n".to_owned()),
        (SCENE, "kind: scene\nobservations: [{id: o, at: {tick: 0}, text: Bad, audience: [everyone]}]\n".to_owned()),
        (SCENE, format!("kind: scene\nobjects: [{HERO}]\n")),
        (SCENE, format!("kind: scene\nconversations: [{SCENE}]\n")),
        (CONVERSATION, "kind: conversation\nturns: [{id: a, kind: action, at: {tick: 0}, text: Waves}]\n".to_owned()),
        (CONVERSATION, format!("kind: conversation\nturns: [{{id: s, at: {{tick: 0}}, speaker: {OTHER}, text: Hello, acoustics: loud}}]\n")),
        (CONVERSATION, format!("kind: conversation\nrecollections: [{{id: m, character: {HERO}, at: {{tick: 11}}, state: remembered, confidence: 2, summary: Memory}}]\n")),
        (SCENE, "kind: scene\nsection_audiences: {Public: [unknown-token]}\n".to_owned()),
        (CONVERSATION, format!("kind: conversation\nid: {CONVERSATION}\n")),
        (HERO, "kind: character\nstatus: draft\n".to_owned()),
    ];
    for (id, request) in bad {
        assert!(
            matches!(
                record_edit::update(&fixture.world, id, &request),
                Err(Error::InvalidInput(_))
            ),
            "accepted {request}"
        );
    }
}

#[test]
fn requests_refuse_nulls_and_tags_even_where_source_allows_optional_absence() {
    let mut fixture = Fixture::new();
    fixture.create(&scene_request(SCENE));
    fixture.create(&conversation_request());
    for (id, request) in [
        (SCENE, "kind: scene\nobservations: null\n".to_owned()),
        (
            SCENE,
            format!("kind: scene\nparticipants: [{{character: {HERO}, until: null}}]\n"),
        ),
        (
            SCENE,
            "kind: scene\ntime: {start: {tick: 0}, current: {tick: 0}, end: null}\n".to_owned(),
        ),
        (SCENE, "kind: scene\nobservations: !custom []\n".to_owned()),
        (CONVERSATION, "kind: conversation\nscene: null\n".to_owned()),
        (
            CONVERSATION,
            format!(
                "kind: conversation\nturns: [{{id: s, at: {{tick: 0}}, speaker: {OTHER}, text: !custom hello}}]\n"
            ),
        ),
        (
            CONVERSATION,
            format!(
                "kind: conversation\nrecollections: [{{id: m, character: {HERO}, at: {{tick: 11}}, state: remembered, confidence: 1, summary: Memory, interpretation: null}}]\n"
            ),
        ),
    ] {
        assert!(
            matches!(
                record_edit::update(&fixture.world, id, &request),
                Err(Error::InvalidInput(_))
            ),
            "accepted {request}"
        );
    }
}

#[test]
fn semantic_errors_are_reviewable_but_never_written() {
    let mut fixture = Fixture::new();
    fixture.create(&scene_request(SCENE));
    fixture.create(&conversation_request());
    for (id, request) in [
        (
            SCENE,
            "kind: scene\nlocation: loc_00000000000000000000000099\n",
        ),
        (
            SCENE,
            "kind: scene\ntime: {start: {tick: 0}, current: {tick: 11}, end: {tick: 10}}\n",
        ),
        (
            SCENE,
            "kind: scene\nstatus: active\ntime: {start: {tick: 0}, current: {tick: 5}, end: {tick: 10}}\n",
        ),
        (
            CONVERSATION,
            "kind: conversation\nstatus: closed\ntime: {start: {tick: 0}}\n",
        ),
        (
            CONVERSATION,
            "kind: conversation\nscene: scene_00000000000000000000000099\n",
        ),
    ] {
        refused_update(&fixture, id, request);
    }
    refused_update(
        &fixture,
        CONVERSATION,
        &format!("kind: conversation\nparticipants: [{{character: {HERO}}}]\n"),
    );
    refused_update(
        &fixture,
        CONVERSATION,
        &format!(
            "kind: conversation\nrecollections: [{{id: m, character: {HERO}, at: {{tick: 11}}, state: remembered, confidence: 1, summary: Memory, exact_turns: [missing]}}]\n"
        ),
    );
    refused_update(
        &fixture,
        CONVERSATION,
        &format!(
            "kind: conversation\nrecollections: [{{id: m, character: {HERO}, at: {{tick: 0}}, state: remembered, confidence: 1, summary: Memory, exact_turns: [wave]}}]\n"
        ),
    );
}

#[test]
fn explicit_status_edits_control_admission_without_creating_beliefs_or_scene_attendance() {
    let mut fixture = Fixture::new();
    fixture.create(&scene_request(SCENE));
    fixture.create(&format!("{}scene: {SCENE}\n", conversation_request()));
    fixture.update(
        SCENE,
        &format!("kind: scene\nconversations: [{CONVERSATION}]\n"),
    );
    let at = fixture.world.story_time(1, None, 0).unwrap();
    let context = character::context(&fixture.world, HERO, &at, 16000).unwrap();
    assert!(context["observations"].as_array().unwrap().is_empty());
    assert!(context["conversations"].as_array().unwrap().is_empty());

    fixture.update(SCENE, "kind: scene\nstatus: active\n");
    fixture.update(CONVERSATION, "kind: conversation\nstatus: closed\n");
    let context = character::context(&fixture.world, HERO, &at, 16000).unwrap();
    assert_eq!(context["observations"].as_array().unwrap().len(), 1);
    assert_eq!(
        context["conversations"][0]["beats"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(context["beliefs"].as_array().unwrap().is_empty());
    assert!(context["conversations"][0]["recollection"].is_null());
    let scene = &fixture.world.records[SCENE];
    assert_eq!(
        scene.list("participants").unwrap().len(),
        1,
        "distant caller must not be added to linked scene"
    );
    let memory = character::conversation(
        &fixture.world,
        HERO,
        CONVERSATION,
        &fixture.world.story_time(11, None, 0).unwrap(),
    )
    .unwrap();
    assert_eq!(
        memory["recollection"]["rememberedQuotes"][0]["text"],
        "Bring a light"
    );
    assert_eq!(memory["beats"][0]["text"], "Bring the lantern");

    fixture.update(SCENE, "kind: scene\nstatus: abandoned\n");
    fixture.update(CONVERSATION, "kind: conversation\nstatus: retired\n");
    let context = character::context(&fixture.world, HERO, &at, 16000).unwrap();
    assert!(context["observations"].as_array().unwrap().is_empty());
    assert!(context["conversations"].as_array().unwrap().is_empty());
}

#[test]
fn replacing_turns_preserves_omitted_memories_and_blocks_dangling_references() {
    let mut fixture = Fixture::new();
    fixture.create(&conversation_request());
    let original = fixture.world.records[CONVERSATION].clone();
    refused_update(&fixture, CONVERSATION, "kind: conversation\nturns: []\n");
    let built = record_edit::update(
        &fixture.world,
        CONVERSATION,
        "kind: conversation\nturns: []\n",
    )
    .unwrap();
    assert!(built.list("turns").unwrap().is_empty());
    assert_eq!(built.get("recollections"), original.get("recollections"));
    fixture.update(
        CONVERSATION,
        "kind: conversation\nturns: []\nrecollections: []\n",
    );
    let record = &fixture.world.records[CONVERSATION];
    assert!(record.list("turns").unwrap().is_empty());
    assert!(record.list("recollections").unwrap().is_empty());
    assert_eq!(record.get("participants"), original.get("participants"));
    assert_eq!(record.body, original.body);
}

#[test]
fn supplied_time_maps_replace_the_whole_map_including_omitted_end() {
    let mut fixture = Fixture::new();
    fixture.create(&scene_request(SCENE));
    fixture.create(&conversation_request());
    fixture.update(
        SCENE,
        "kind: scene\ntime: {start: {tick: 0}, current: {tick: 0}}\n",
    );
    fixture.update(
        CONVERSATION,
        "kind: conversation\ntime: {start: {tick: 0}}\n",
    );
    for id in [SCENE, CONVERSATION] {
        assert!(
            fixture.world.records[id]
                .get("time")
                .unwrap()
                .get("end")
                .is_none()
        );
    }
    refused_update(
        &fixture,
        CONVERSATION,
        "kind: conversation\nstatus: closed\n",
    );
    fixture.update(
        CONVERSATION,
        "kind: conversation\nstatus: closed\ntime: {start: {tick: 0}, end: {tick: 10}}\n",
    );
    assert_eq!(fixture.world.records[CONVERSATION].status(), "closed");
}

#[test]
fn no_op_body_only_and_structured_edits_preserve_unsubmitted_source() {
    let mut fixture = Fixture::new();
    fixture.create(&scene_request(SCENE));
    fixture.create(&conversation_request());
    for (id, kind) in [(SCENE, "scene"), (CONVERSATION, "conversation")] {
        let original = &fixture.world.records[id];
        let prefix = std::str::from_utf8(
            &original.raw_bytes[..original.raw_bytes.len() - original.body.len()],
        )
        .unwrap();
        let prefix = prefix.strip_suffix("---\n").unwrap();
        let header = format!("{}x-author-note: {{value: null, numbers: [1, 1.0, '1']}}\nsection_audiences: {{Private: [author]}}\n# Keep exact comments on no-op/body-only edits.\n---\n", prefix).replace('\n', "\r\n");
        let body = "\r\n  Exact authored prose — α  \n\t";
        fs::write(
            fixture.world.root.join(&original.source_path),
            format!("{header}{body}"),
        )
        .unwrap();
        fixture.world = source::load_world(&fixture.world.root, None).unwrap();
        let original = fixture.world.records[id].clone();
        let no_op = fixture.update(
            id,
            &format!("kind: {kind}\nstatus: {}\n", original.status()),
        );
        assert_eq!(no_op["changed"], false);
        assert_eq!(fixture.world.records[id].raw_bytes, original.raw_bytes);

        let replacement = "New body\r\n\n  Kept spacing\t";
        let body_edit = fixture.update(
            id,
            &format!(
                "kind: {kind}\nbody: {}\n",
                serde_json::to_string(replacement).unwrap()
            ),
        );
        assert_eq!(
            body_edit["afterSource"].as_str().unwrap(),
            format!("{header}{replacement}")
        );
        fixture.update(
            id,
            &format!("kind: {kind}\nstatus: closed\naliases: [Changed Alias]\n"),
        );
        let record = &fixture.world.records[id];
        assert_eq!(record.body, replacement);
        assert_eq!(record.source_path, original.source_path);
        assert_eq!(record.get("x-author-note"), original.get("x-author-note"));
        assert_eq!(
            record.get("section_audiences"),
            original.get("section_audiences")
        );
        assert_eq!(record.get("time"), original.get("time"));
        assert_eq!(record.get("participants"), original.get("participants"));
    }
}

#[test]
fn all_packaged_scenes_and_conversations_support_lossless_no_ops_and_partial_edits() {
    let mut checked = 0;
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name);
        let world = source::load_world(&root, None).unwrap();
        let mut changed = world.records.clone();
        for original in world
            .records
            .values()
            .filter(|record| ["scene", "conversation"].contains(&record.kind.as_str()))
        {
            let patch = format!("kind: {}\nstatus: {}\n", original.kind, original.status());
            assert_eq!(
                record_edit::update(&world, &original.id, &patch)
                    .unwrap()
                    .raw_bytes,
                original.raw_bytes
            );
            let patch = format!(
                "kind: {}\ntitle: {}\n",
                original.kind,
                serde_json::to_string(&format!("{} revised", original.title)).unwrap()
            );
            let record = record_edit::update(&world, &original.id, &patch).unwrap();
            assert_eq!(record.body, original.body);
            assert_eq!(record.source_path, original.source_path);
            for (key, value) in original.frontmatter.as_mapping().unwrap() {
                if key.as_str() != Some("title") {
                    assert_eq!(record.frontmatter.get(key), Some(value));
                }
            }
            changed.insert(record.id.clone(), record);
            checked += 1;
        }
        let changed =
            source::world_from_records(world.root, changed.into_values().collect(), None).unwrap();
        let report = validation::validate_world(&changed);
        assert!(report.valid, "{name}: {:?}", report.diagnostics);
    }
    assert_eq!(checked, 80);
}
