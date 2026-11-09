use std::path::PathBuf;
use wedl::{character, model::World, source, validation};

const PERSON: &str = "char_00000000000000000000000001";
const ROOT: &str = "world_00000000000000000000000000";

fn world(audiences: &str, body: &str) -> World {
    let root = format!(
        "---\nschema: {}\nkind: world\nid: {ROOT}\ntitle: Audience world\ncapabilities: []\ntimelines: [{{id: main, label: Main}}]\n---\n",
        source::SOURCE_SCHEMA
    );
    let person = format!(
        "---\nschema: {}\nkind: character\nid: {PERSON}\ntitle: Person\nsection_audiences: {audiences}\n---\n{body}",
        source::SOURCE_SCHEMA
    );
    source::world_from_records(
        PathBuf::new(),
        vec![
            source::parse_record(root.as_bytes(), "story/world.md").unwrap(),
            source::parse_record(person.as_bytes(), "story/person.md").unwrap(),
        ],
        None,
    )
    .unwrap()
}

#[test]
fn explicit_section_audiences_allow_known_tokens_and_character_ids() {
    let world = world(
        &format!("{{Public: [public, author], Voice: [self, {PERSON}], Hidden: []}}"),
        "# Person\n",
    );
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
}

#[test]
fn invalid_audience_declarations_fail_instead_of_widening_access() {
    for audience in [
        "null",
        "[public]",
        "{public: public}",
        "{public: [true]}",
        "{public: [public, public]}",
        "{Public: [public], public: [author]}",
        "{'': [public]}",
        "{public: [unknown]}",
        "{public: [char_00000000000000000000000002]}",
        "{public: [world_00000000000000000000000000]}",
        "{public: [char:char_00000000000000000000000001]}",
        "{public: [participants]}",
        "!custom {public: [public]}",
        "{public: !custom [public]}",
        "{public: [!custom public]}",
    ] {
        let report = validation::validate_world(&world(audience, ""));
        assert!(!report.valid, "accepted {audience}");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "WDL-AUDIENCE-001"),
            "{:?}",
            report.diagnostics
        );
    }
}

#[test]
fn ordinary_prose_stays_out_of_character_views_even_when_marked_public() {
    let public = world(
        "{public: [public], voice: [self]}",
        "## Public\nPRIVATE_SENTINEL\n## Voice\nSelf prose\n",
    );
    let private = world(
        "{public: [author], voice: []}",
        "## Public\nChanged future text\n## Voice\nDifferent text\n",
    );
    assert!(validation::validate_world(&public).valid);
    assert!(validation::validate_world(&private).valid);
    assert_ne!(public.fingerprint, private.fingerprint);
    let at = public.story_time(20, None, i32::MAX).unwrap();
    assert_eq!(
        character::context(&public, PERSON, &at, 16000).unwrap(),
        character::context(&private, PERSON, &at, 16000).unwrap()
    );
    assert_eq!(
        character::search(&public, PERSON, "PRIVATE_SENTINEL", &at, 20).unwrap(),
        character::search(&private, PERSON, "PRIVATE_SENTINEL", &at, 20).unwrap()
    );
}
