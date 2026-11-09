//! Reader disclosure is a pure intersection of ordinary authority and literal
//! occurrence admissions. No canonical or private source envelope is a result.
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use wedl::{
    manuscript::{self, Snapshot},
    model::{Record, StoryTime, World},
    reveal::{self, CanonicalScope, Lens, ReaderGrant, ReaderItem},
    source, validation,
};

const REV: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const HIDDEN: &str = "loc_00000000000000000000000002";
const EVENT: &str = "event_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const CONVERSATION: &str = "conv_00000000000000000000000001";
const ENVIRONMENT: &str = "env_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const FIRST: &str = "segment_00000000000000000000000001";
const SECOND: &str = "segment_00000000000000000000000002";
const WORK: &str = "work_00000000000000000000000001";
const CHAPTER: &str = "chapter_00000000000000000000000001";
const NEXT_CHAPTER: &str = "chapter_00000000000000000000000002";
const PLAN: &str = "reveal_00000000000000000000000001";

fn id(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
}
fn raw(fields: &Json, body: &str) -> String {
    format!("---\n{}---\n{body}", serde_yaml::to_string(fields).unwrap())
}
fn record(kind: &str, id: &str, extra: Json, body: &str) -> Record {
    let mut fields =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Owner title {id}")});
    fields
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    source::parse_record(raw(&fields, body).as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn world(mut records: Vec<Record>) -> World {
    records.insert(0, record("world", WORLD, json!({"capabilities":["descriptions-v1"],"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}), "PRIVATE_WORLD_BODY"));
    if !records.iter().any(|record| record.id == VIEWER) {
        records.push(record(
            "character",
            VIEWER,
            json!({}),
            "PRIVATE_CHARACTER_BODY",
        ));
    }
    let world = source::world_from_records(
        PathBuf::from("/no-reader-filesystem-access"),
        records,
        Some(REV.into()),
    )
    .unwrap();
    let validation = validation::validate_world(&world);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    world
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn interval(tick: i64, order: i32) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(tick,order)})
}
fn section(number: usize, markdown: &str) -> Json {
    json!({"id":id("description",number),"heading":format!("Section {number}"),"markdown":markdown,"valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]})
}
fn owner(sections: Vec<Json>, body: &str) -> Record {
    record("location", PLACE, json!({"descriptions":sections}), body)
}
fn author(fields: &Json, body: &str) -> Record {
    let path = manuscript::document_path(
        fields["kind"].as_str().unwrap(),
        fields["id"].as_str().unwrap(),
    )
    .unwrap();
    let mut record = manuscript::parse_document(raw(fields, body).as_bytes(), &path).unwrap();
    record.blob_oid = Some("b".repeat(40));
    record
}
fn entry(ordinal: usize, admissions: Vec<Json>) -> Json {
    let (chapter, index) = if ordinal < 2 {
        (CHAPTER, ordinal)
    } else {
        (NEXT_CHAPTER, 0)
    };
    json!({"milestone":{"chapter_id":chapter,"segment_index":index},"admissions":admissions})
}
fn admit_record(id: &str) -> Json {
    json!({"kind":"record","target":{"kind":"record","id":id}})
}
fn admit_section(owner: &str, number: usize) -> Json {
    json!({"kind":"section","target":{"kind":"section","owner_id":owner,"id":id("description",number)}})
}
fn admit_text(number: usize, markdown: &str) -> Json {
    json!({"kind":"text","id":id("revealtext",number),"markdown":markdown})
}
fn authors(entries: Vec<Json>) -> Snapshot {
    let segments = [(FIRST,20),(SECOND,-20)].into_iter().map(|(id,tick)| author(&json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":id,"title":"PRIVATE_SEGMENT_TITLE","source_links":[],"placement":point(tick,0),"pov_character_id":VIEWER}), "PRIVATE_MANUSCRIPT_PROSE\r\n")).collect::<Vec<_>>();
    let mut records = segments;
    records.push(author(&json!({"schema":"wedl-author/v1","kind":"work","id":WORK,"title":"PRIVATE_WORK_TITLE","duplicate_policy":"allow-explicit-reuse","chapters":[{"id":CHAPTER,"title":"PRIVATE_CHAPTER_TITLE","segments":[{"kind":"author-record","id":FIRST},{"kind":"author-record","id":SECOND}]},{"id":NEXT_CHAPTER,"title":"PRIVATE_CHAPTER_TITLE","segments":[{"kind":"author-record","id":FIRST}]}]}), "PRIVATE_WORK_NOTES"));
    records.push(author(&json!({"schema":"wedl-author/v1","kind":"reveal-plan","id":PLAN,"title":"PRIVATE_PLAN_TITLE","work":{"kind":"author-record","id":WORK},"entries":entries}), "PRIVATE_PLAN_NOTES"));
    manuscript::snapshot_from_records(records, Some(REV.into())).unwrap()
}
fn grant(world: &World) -> ReaderGrant {
    ReaderGrant {
        reader_id: "reader:fixture".into(),
        plan_id: Some(PLAN.into()),
        revision: REV.into(),
        canonical_scope: CanonicalScope {
            world_id: WORLD.into(),
            records: world.records.keys().cloned().collect(),
            sections: None,
        },
        at: at(30, 0),
        lens: Lens::AuthorAsOf,
        audiences: BTreeSet::from(["public".into()]),
        perspectives: BTreeSet::from(["ordinary".into()]),
    }
}
fn present(
    world: &World,
    authors: &Snapshot,
    ordinal: usize,
    grant: &ReaderGrant,
) -> Vec<ReaderItem> {
    reveal::present(
        world,
        authors,
        Some(PLAN),
        Some(ordinal),
        "reader:fixture",
        grant,
    )
    .unwrap()
}
fn permit(grant: &mut ReaderGrant, owner: &str, paths: &[&str]) {
    grant.canonical_scope.sections = Some(BTreeMap::from([(
        owner.into(),
        paths.iter().map(|path| (*path).into()).collect(),
    )]));
}

#[test]
fn cumulative_disclosure_uses_occurrence_order_and_preserves_literal_text() {
    let world = world(vec![owner(
        vec![section(1, "Visible description")],
        "Visible body",
    )]);
    let exact = "\r\n  Exact æ🌲 text  \r\nNo final newline  ";
    let authors = authors(vec![
        entry(0, vec![admit_text(1, exact)]),
        entry(1, vec![admit_record(PLACE)]),
        entry(2, vec![admit_text(2, "Prequel disclosure")]),
    ]);
    let compiled = manuscript::resolve_reveal_plan(&world, &authors, PLAN, 4096).unwrap();
    assert_eq!(
        compiled
            .occurrences
            .iter()
            .map(|item| item.segment_id.as_str())
            .collect::<Vec<_>>(),
        vec![FIRST, SECOND, FIRST]
    );
    let grant = grant(&world);
    let first = present(&world, &authors, 0, &grant);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].markdown, exact);
    assert_eq!(first[0].title, None);
    let second = present(&world, &authors, 1, &grant);
    assert_eq!(
        second
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["text", "record", "section"]
    );
    let last = present(&world, &authors, 2, &grant);
    assert_eq!(&last[..second.len()], second);
    assert_eq!(last.last().unwrap().markdown, "Prequel disclosure");
    let wire = serde_json::to_value(&last).unwrap();
    for item in wire.as_array().unwrap() {
        assert_eq!(
            item.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["kind", "id", "ownerId", "title", "markdown"])
        );
    }
    let wire = wire.to_string();
    for hidden in [
        "PRIVATE_",
        WORK,
        FIRST,
        CHAPTER,
        PLAN,
        "sourcePath",
        "blobOid",
        "fingerprint",
        "source_links",
    ] {
        assert!(!wire.contains(hidden), "leaked {hidden}");
    }
}

#[test]
fn section_only_has_its_own_heading_and_requires_title_permission_without_emitting_owner_title() {
    let world = world(vec![owner(
        vec![
            section(1, "Selected wording"),
            section(2, "Unselected sibling"),
        ],
        "Unselected owner body",
    )]);
    let authors = authors(vec![entry(0, vec![admit_section(PLACE, 1)])]);
    let mut grant = grant(&world);
    let path = format!("descriptions.{}", id("description", 1));
    permit(&mut grant, PLACE, &["frontmatter.title", &path]);
    let items = present(&world, &authors, 0, &grant);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title.as_deref(), Some("Section 1"));
    assert_eq!(items[0].owner_id.as_deref(), Some(PLACE));
    let wire = serde_json::to_string(&items).unwrap();
    for hidden in ["Owner title", "Unselected owner body", "Unselected sibling"] {
        assert!(!wire.contains(hidden));
    }
    permit(&mut grant, PLACE, &[&path]);
    assert!(present(&world, &authors, 0, &grant).is_empty());
    permit(&mut grant, PLACE, &["frontmatter.title"]);
    assert!(present(&world, &authors, 0, &grant).is_empty());
    grant.canonical_scope.sections = None;
    grant.canonical_scope.records.clear();
    assert!(present(&world, &authors, 0, &grant).is_empty());
}

#[test]
fn overlapping_record_and_section_admissions_deduplicate_in_first_seen_order() {
    let world = world(vec![owner(
        vec![section(1, "First"), section(2, "Second")],
        "Body",
    )]);
    let authors = authors(vec![
        entry(0, vec![admit_section(PLACE, 1), admit_record(PLACE)]),
        entry(1, vec![admit_section(PLACE, 2)]),
    ]);
    let grant = grant(&world);
    let first = present(&world, &authors, 0, &grant);
    assert_eq!(
        first
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["section", "record", "section"]
    );
    assert_eq!(present(&world, &authors, 1, &grant), first);
    let mut limited = grant;
    permit(&mut limited, PLACE, &["frontmatter.title"]);
    let only_title = present(&world, &authors, 1, &limited);
    assert_eq!(only_title.len(), 1);
    assert_eq!(only_title[0].markdown, "");
    assert_eq!(
        only_title[0].title.as_deref(),
        Some(world.records[PLACE].title.as_str())
    );
}

#[test]
fn ordinary_correction_filtering_precedes_exact_reveal_section_matching() {
    let original = section(1, "Old account");
    let mut replacement = section(2, "UNREVEALED_CORRECTION");
    replacement["replaces"] = id("description", 1).into();
    let world = world(vec![owner(vec![original, replacement], "Body")]);
    let authors = authors(vec![entry(0, vec![admit_section(PLACE, 1)])]);
    let mut grant = grant(&world);
    // The replacement is ordinary-visible, so the old admitted section is
    // suppressed even though the replacement is not itself revealed.
    assert!(present(&world, &authors, 0, &grant).is_empty());
    let old = format!("descriptions.{}", id("description", 1));
    permit(&mut grant, PLACE, &["frontmatter.title", &old]);
    let result = present(&world, &authors, 0, &grant);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].markdown, "Old account");
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("UNREVEALED_CORRECTION")
    );
}

#[test]
fn ordinary_audience_view_and_exact_time_filter_before_disclosure() {
    let mut future = section(2, "Future");
    future["valid"] = interval(10, 1);
    let mut author_only = section(3, "Private audience");
    author_only["audience"] = json!(["author"]);
    let mut private_view = section(4, "Private view");
    private_view["perspectives"] = json!(["author"]);
    let mut other_time = section(5, "Other timeline");
    other_time["valid"] = json!({"kind":"interval","timeline":"other","start":{"timeline":"other","tick":0,"order":0}});
    let world = world(vec![
        owner(
            vec![
                section(1, "Timeless"),
                future,
                author_only,
                private_view,
                other_time,
            ],
            "Body",
        ),
        record(
            "location",
            HIDDEN,
            json!({"audience":["author"],"descriptions":[section(6,"Hidden owner wording")]}),
            "Hidden owner body",
        ),
    ]);
    let authors = authors(vec![entry(
        0,
        vec![admit_record(PLACE), admit_record(HIDDEN)],
    )]);
    let mut grant = grant(&world);
    grant.at = at(10, 0);
    let before = present(&world, &authors, 0, &grant);
    assert_eq!(
        before
            .iter()
            .filter(|item| item.kind == "section")
            .map(|item| item.markdown.as_str())
            .collect::<Vec<_>>(),
        vec!["Timeless"]
    );
    grant.at = at(10, 1);
    assert_eq!(
        present(&world, &authors, 0, &grant)
            .iter()
            .filter(|item| item.kind == "section")
            .map(|item| item.markdown.as_str())
            .collect::<Vec<_>>(),
        vec!["Timeless", "Future"]
    );
    grant.at = StoryTime::new("other", 10, 1).unwrap();
    assert_eq!(
        present(&world, &authors, 0, &grant)
            .iter()
            .filter(|item| item.kind == "section")
            .map(|item| item.markdown.as_str())
            .collect::<Vec<_>>(),
        vec!["Timeless", "Other timeline"]
    );
}

#[test]
fn future_event_description_does_not_grant_future_body_or_owner_title() {
    let world = world(vec![record(
        "event",
        EVENT,
        json!({"time":point(10,1),"section_audiences":{"Public":["public"]},"descriptions":[section(1,"Early public account")]}),
        "# Public\nFuture event prose",
    )]);
    let authors = authors(vec![entry(0, vec![admit_record(EVENT)])]);
    let mut grant = grant(&world);
    grant.at = at(10, 0);
    let early = present(&world, &authors, 0, &grant);
    assert_eq!(early.len(), 1);
    assert_eq!(early[0].kind, "section");
    assert!(
        !serde_json::to_string(&early)
            .unwrap()
            .contains("Owner title")
    );
    grant.at = at(10, 1);
    assert_eq!(
        present(&world, &authors, 0, &grant)[0].markdown,
        "# Public\nFuture event prose"
    );
    grant.at = StoryTime::new("other", 100, 0).unwrap();
    assert_eq!(present(&world, &authors, 0, &grant), early);
}

#[test]
fn scene_conversation_and_environment_bodies_keep_their_distinct_temporal_bounds() {
    let interval = json!({"start":point(5,0),"end":point(10,1)});
    let world = world(vec![
        owner(vec![], "Place"),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","location":PLACE,"time":{"start":point(5,0),"current":point(10,1),"end":point(10,1)},"participants":[],"section_audiences":{"Public":["public"]}}),
            "# Public\nCompleted scene",
        ),
        record(
            "conversation",
            CONVERSATION,
            json!({"status":"closed","location":PLACE,"time":interval,"participants":[],"turns":[],"recollections":[],"section_audiences":{"Public":["public"]}}),
            "# Public\nCompleted conversation",
        ),
        record(
            "environment",
            ENVIRONMENT,
            json!({"targets":[VIEWER],"time":interval}),
            "Environment interval",
        ),
    ]);
    let authors = authors(vec![entry(
        0,
        vec![
            admit_record(SCENE),
            admit_record(CONVERSATION),
            admit_record(ENVIRONMENT),
        ],
    )]);
    let mut grant = grant(&world);
    for (time, expected) in [
        (at(4, 0), vec![]),
        (at(5, 0), vec![ENVIRONMENT]),
        (at(10, 0), vec![ENVIRONMENT]),
        (at(10, 1), vec![SCENE, CONVERSATION, ENVIRONMENT]),
        (at(10, 2), vec![SCENE, CONVERSATION]),
    ] {
        grant.at = time;
        assert_eq!(
            present(&world, &authors, 0, &grant)
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
    let mut records = world
        .records
        .values()
        .filter(|record| record.id != WORLD && record.id != SCENE)
        .cloned()
        .collect::<Vec<_>>();
    records.push(record("scene",SCENE,json!({"status":"active","location":PLACE,"time":{"start":point(5,0),"current":point(30,0)},"participants":[],"section_audiences":{"Public":["public"]}}),"# Public\nIncomplete aggregate"));
    let active = world_from_existing(records);
    grant.at = at(30, 0);
    assert!(
        !present(&active, &authors, 0, &grant)
            .iter()
            .any(|item| item.id == SCENE)
    );
}

// Separate name avoids shadowing the fixture constructor in source variations.
fn world_from_existing(records: Vec<Record>) -> World {
    world(records)
}

#[test]
fn inherited_body_is_limited_to_seven_ordinary_kinds() {
    let bodies = vec![
        record("character", VIEWER, json!({}), "# Summary\nCharacter prose"),
        owner(vec![], "Location prose"),
        record("object", OBJECT, json!({}), "Object prose"),
        record(
            "event",
            EVENT,
            json!({"time":point(0,0),"section_audiences":{"Public":["public"]}}),
            "# Public\nEvent prose",
        ),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","location":PLACE,"time":{"start":point(0,0),"current":point(1,0),"end":point(1,0)},"participants":[],"section_audiences":{"Public":["public"]}}),
            "# Public\nScene prose",
        ),
        record(
            "conversation",
            CONVERSATION,
            json!({"status":"closed","location":PLACE,"time":{"start":point(0,0),"end":point(1,0)},"participants":[],"turns":[],"recollections":[],"section_audiences":{"Public":["public"]}}),
            "# Public\nConversation prose",
        ),
        record(
            "environment",
            ENVIRONMENT,
            json!({"targets":[VIEWER],"time":{"start":point(0,0),"end":point(40,0)}}),
            "Environment prose",
        ),
        record(
            "knowledge",
            KNOWLEDGE,
            json!({"knower":VIEWER,"claim":{"key":"learned","statement":"Learning source"},"transitions":[],"descriptions":[section(1,"Knowledge description")]}),
            "PRIVATE_KNOWLEDGE_BODY",
        ),
        record(
            "story-point",
            POINT,
            json!({"activation_policy":"suggest","lifecycle":{"initial_state":"dormant","transitions":[]},"descriptions":[section(2,"Story point description")]}),
            "PRIVATE_STORY_POINT_BODY",
        ),
    ];
    let world = world(bodies);
    let targets = [
        VIEWER,
        PLACE,
        OBJECT,
        EVENT,
        SCENE,
        CONVERSATION,
        ENVIRONMENT,
        KNOWLEDGE,
        POINT,
        WORLD,
    ];
    let authors = authors(vec![entry(
        0,
        targets.iter().map(|target| admit_record(target)).collect(),
    )]);
    let items = present(&world, &authors, 0, &grant(&world));
    assert_eq!(
        items
            .iter()
            .filter(|item| item.kind == "record")
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        targets[..7]
    );
    assert_eq!(
        items.iter().filter(|item| item.kind == "section").count(),
        2
    );
    assert!(!serde_json::to_string(&items).unwrap().contains("PRIVATE_"));
}

#[test]
fn body_audience_subtrees_stay_private_until_explicitly_overridden() {
    let body = "Preamble public\r\n\r\n# Author Notes\r\nPRIVATE_PARENT\r\n## Ordinary child\r\nPRIVATE_CHILD\r\n### Nested child\r\nPRIVATE_GRANDCHILD\r\n### Safe child\r\nVisible child only\r\n# Restrictions\r\nPRIVATE_MAPPED_PARENT\r\n## Plain child\r\nPRIVATE_MAPPED_CHILD\r\n# Unrelated\r\nOrdinary top-level public\r\n";
    let world = world(vec![record(
        "location",
        PLACE,
        json!({"section_audiences":{"Safe child":["public"],"Restrictions":["author"]}}),
        body,
    )]);
    let authors = authors(vec![entry(0, vec![admit_record(PLACE)])]);
    let mut grant = grant(&world);
    let items = present(&world, &authors, 0, &grant);
    assert_eq!(
        items[0].markdown,
        "Preamble public\n\n### Safe child\nVisible child only\n\n# Unrelated\nOrdinary top-level public"
    );
    for hidden in [
        "PRIVATE_",
        "Author Notes",
        "Ordinary child",
        "Nested child",
        "Restrictions",
        "Plain child",
    ] {
        assert!(!items[0].markdown.contains(hidden));
    }
    permit(
        &mut grant,
        PLACE,
        &["frontmatter.title", "bodyMarkdown.Safe child"],
    );
    assert_eq!(
        present(&world, &authors, 0, &grant)[0].markdown,
        "### Safe child\nVisible child only"
    );
    permit(
        &mut grant,
        PLACE,
        &["frontmatter.title", "bodyMarkdown.Safe"],
    );
    assert_eq!(present(&world, &authors, 0, &grant)[0].markdown, "");
}

#[test]
fn character_defaults_and_private_heading_forms_never_fall_back_to_whole_body() {
    let private = [
        "Author's Notes",
        "AUTHOR-ONLY TRUTH",
        "Author only constraints",
        "GM notes",
        "Hidden",
        "Hidden secrets",
        "Spoilers",
        "Internal notes",
    ];
    let mut body = "PRIVATE_PREAMBLE\n\n# Summary\nPublic summary\n".to_string();
    for heading in private {
        body.push_str(&format!(
            "# {heading}\nPRIVATE_HEADING_BODY\n## Appearance\nPRIVATE_DESCENDANT\n"
        ));
    }
    body.push_str("# Appearance\nPublic appearance\n# Memory\nPRIVATE_MEMORY\n");
    let world = world(vec![record("character", VIEWER, json!({}), &body)]);
    let authors = authors(vec![entry(0, vec![admit_record(VIEWER)])]);
    let items = present(&world, &authors, 0, &grant(&world));
    assert_eq!(
        items[0].markdown,
        "# Summary\nPublic summary\n\n# Appearance\nPublic appearance"
    );
    let mut broken = world.clone();
    let original = &world.records[VIEWER];
    let mut fields = serde_json::to_value(&original.frontmatter).unwrap();
    fields["section_audiences"] = json!({"Summary":"public"});
    broken
        .records
        .insert(VIEWER.into(), record("character", VIEWER, fields, &body));
    assert!(
        reveal::present(
            &broken,
            &authors,
            Some(PLAN),
            Some(0),
            "reader:fixture",
            &grant(&world)
        )
        .is_err()
    );
}

#[test]
fn unicode_slug_folding_private_names_and_line_boundaries_retain_restrictions() {
    let body = "# Straße\nHIDDEN_SHARP_S\n# Oﬃce\nHIDDEN_LIGATURE\n# Long ſ\nHIDDEN_LONG_S\n# Author ſecrets\nHIDDEN_REGEX_LONG_S\n# Hıdden\nHIDDEN_DOTLESS_I\n# HİDDEN\nHIDDEN_DOTTED_I\n# ſpoiler\nHIDDEN_SPOILER\n# Public\nVisible";
    let world = world(vec![record(
        "location",
        PLACE,
        json!({"section_audiences":{"strasse":["author"],"office":["author"],"long-s":["author"]}}),
        body,
    )]);
    let authors = authors(vec![entry(0, vec![admit_record(PLACE)])]);
    assert_eq!(
        present(&world, &authors, 0, &grant(&world))[0].markdown,
        "# Public\nVisible"
    );
    for separator in [
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ] {
        let body = format!(
            "# Public\nVisible{separator}# Author secrets{separator}PRIVATE_LINE_BODY{separator}# Unrelated\nLast"
        );
        let world = world_from_existing(vec![owner(vec![], &body)]);
        assert_eq!(
            present(&world, &authors, 0, &grant(&world))[0].markdown,
            "# Public\nVisible\n\n# Unrelated\nLast",
            "separator {separator:?}"
        );
    }
    let world = world_from_existing(vec![owner(
        vec![],
        "#\u{1f}Author\u{1f}secrets\nPRIVATE_SEPARATOR_BODY\n# Public\nVisible",
    )]);
    assert_eq!(
        present(&world, &authors, 0, &grant(&world))[0].markdown,
        "# Public\nVisible"
    );
}

#[test]
fn character_lens_uses_native_learning_and_never_inherits_raw_bodies() {
    let mut private = section(2, "PRIVATE_CHARACTER_DESCRIPTION");
    private["audience"] = json!(["author"]);
    private["perspectives"] = json!(["author"]);
    let world = world(vec![
        owner(
            vec![section(1, "Learned public description"), private],
            "Public location body",
        ),
        record(
            "knowledge",
            KNOWLEDGE,
            json!({"knower":VIEWER,"claim":{"key":"known-place","statement":"Learned this place","object":{"entity":PLACE}},"transitions":[{"id":"learned","time":point(5,1),"state":"accepted","confidence":1,"acquisition":"heard"},{"id":"forgotten","time":point(10,0),"state":"forgotten","confidence":1,"acquisition":"remembered"}]}),
            "PRIVATE_LEARNING_BODY",
        ),
    ]);
    let authors = authors(vec![entry(
        0,
        vec![admit_record(PLACE), admit_text(1, "Explicit reveal text")],
    )]);
    let mut grant = grant(&world);
    grant.lens = Lens::Character {
        character_id: VIEWER.into(),
    };
    grant.audiences.insert("author".into());
    grant.perspectives.insert("author".into());
    grant.at = at(5, 0);
    assert_eq!(
        present(&world, &authors, 0, &grant)
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["text"]
    );
    grant.at = at(5, 1);
    let learned = present(&world, &authors, 0, &grant);
    assert_eq!(
        learned
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["section", "text"]
    );
    assert_eq!(learned[0].markdown, "Learned public description");
    let wire = serde_json::to_string(&learned).unwrap();
    for hidden in ["PRIVATE_", "Public location body", "Owner title"] {
        assert!(!wire.contains(hidden));
    }
    grant.at = at(10, 0);
    assert_eq!(
        present(&world, &authors, 0, &grant)
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["text"]
    );
    grant.at = StoryTime::new("other", 100, 0).unwrap();
    assert_eq!(present(&world, &authors, 0, &grant).len(), 1);
}

#[test]
fn scope_intersection_retains_only_common_records_and_narrower_paths() {
    let base = CanonicalScope {
        world_id: WORLD.into(),
        records: BTreeSet::from([PLACE.into(), HIDDEN.into()]),
        sections: None,
    };
    let limited = CanonicalScope {
        world_id: WORLD.into(),
        records: BTreeSet::from([PLACE.into()]),
        sections: Some(BTreeMap::from([(
            PLACE.into(),
            BTreeSet::from([
                "frontmatter.title".into(),
                "descriptions".into(),
                "bodyMarkdown.Public".into(),
            ]),
        )])),
    };
    assert_eq!(base.intersect(&limited).unwrap(), limited);
    assert_eq!(limited.intersect(&base).unwrap(), limited);
    let specific = CanonicalScope {
        sections: Some(BTreeMap::from([(
            PLACE.into(),
            BTreeSet::from([
                "frontmatter".into(),
                "descriptions.one".into(),
                "bodyMarkdown".into(),
                "bodyMarkdown.Private".into(),
            ]),
        )])),
        ..limited.clone()
    };
    let common = limited.intersect(&specific).unwrap();
    assert!(common.permits(PLACE, "frontmatter.title"));
    assert!(common.permits(PLACE, "descriptions.one.detail"));
    assert!(!common.permits(PLACE, "descriptions.oneMore"));
    assert!(!common.permits(PLACE, "descriptions.two"));
    assert!(common.permits(PLACE, "bodyMarkdown.Public"));
    assert!(!common.permits(PLACE, "bodyMarkdown.Private"));
    assert!(!common.permits(HIDDEN, "frontmatter.title"));
    let absent = CanonicalScope {
        sections: Some(BTreeMap::new()),
        ..base.clone()
    };
    assert!(
        !base
            .intersect(&absent)
            .unwrap()
            .permits(PLACE, "frontmatter.title")
    );
    let different = CanonicalScope {
        world_id: "world_other".into(),
        ..base.clone()
    };
    assert!(base.intersect(&different).is_err());
    let mut invalid = base.clone();
    invalid.records.insert(" ".into());
    assert!(invalid.validate().is_err());
    let mut invalid = base;
    invalid.sections = Some(BTreeMap::from([("not-selected".into(), BTreeSet::new())]));
    assert!(invalid.validate().is_err());
}

#[test]
fn public_projection_checks_identity_plan_clock_revision_and_source_binding() {
    let world = world(vec![owner(vec![section(1, "Public")], "Body")]);
    let authors = authors(vec![entry(0, vec![admit_record(PLACE)])]);
    let valid = grant(&world);
    for mutation in 0..10 {
        let mut grant = valid.clone();
        match mutation {
            0 => grant.reader_id = "other reader".into(),
            1 => grant.plan_id = Some(id("reveal", 2)),
            2 => grant.revision = "main".into(),
            3 => grant.revision = "c".repeat(40),
            4 => grant.canonical_scope.world_id = "world_other".into(),
            5 => grant.at.timeline = "missing".into(),
            6 => grant.audiences.clear(),
            7 => grant.perspectives = BTreeSet::from([" ".into()]),
            8 => {
                grant.lens = Lens::Character {
                    character_id: id("char", 999),
                }
            }
            _ => grant.audiences = (0..17).map(|n| format!("label{n}")).collect(),
        }
        assert!(
            reveal::present(
                &world,
                &authors,
                Some(PLAN),
                Some(0),
                "reader:fixture",
                &grant
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
    assert!(reveal::present(&world, &authors, Some(PLAN), None, "reader:fixture", &valid).is_err());
    assert!(
        reveal::present(
            &world,
            &authors,
            Some(PLAN),
            Some(3),
            "reader:fixture",
            &valid
        )
        .is_err()
    );
    let mut current = world.clone();
    current.revision = None;
    assert!(
        reveal::present(
            &current,
            &authors,
            Some(PLAN),
            Some(0),
            "reader:fixture",
            &valid
        )
        .is_err()
    );
    let mut altered = world.clone();
    altered.records.get_mut(PLACE).unwrap().body = "Injected body".into();
    assert!(
        reveal::present(
            &altered,
            &authors,
            Some(PLAN),
            Some(0),
            "reader:fixture",
            &valid
        )
        .is_err()
    );
    let mut altered = authors.clone();
    altered.documents.get_mut(PLAN).unwrap().body = "Injected notes".into();
    assert!(
        reveal::present(
            &world,
            &altered,
            Some(PLAN),
            Some(0),
            "reader:fixture",
            &valid
        )
        .is_err()
    );
    let mut altered = authors.clone();
    altered.fingerprint = "incorrect".into();
    assert!(
        reveal::present(
            &world,
            &altered,
            Some(PLAN),
            Some(0),
            "reader:fixture",
            &valid
        )
        .is_err()
    );
    let mut none = valid;
    none.plan_id = None;
    assert!(
        reveal::present(&world, &authors, None, None, "reader:fixture", &none)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn hidden_and_later_invalid_references_are_rejected_before_projection() {
    let world = world(vec![owner(vec![], "Body")]);
    let authors = authors(vec![
        entry(0, vec![admit_text(1, "Early text")]),
        entry(2, vec![admit_record(HIDDEN)]),
    ]);
    let mut grant = grant(&world);
    grant.canonical_scope.records.clear();
    assert!(
        reveal::present(
            &world,
            &authors,
            Some(PLAN),
            Some(0),
            "reader:fixture",
            &grant
        )
        .is_err()
    );
}

#[test]
fn hidden_source_changes_do_not_change_minimal_projection() {
    let make = |secret: &str| {
        let mut hidden = section(2, secret);
        hidden["audience"] = json!(["author"]);
        world(vec![owner(
            vec![section(1, "Visible"), hidden],
            &format!("# Public\nVisible body\n# Hidden\n{secret}"),
        )])
    };
    let before = make("PRIVATE_BEFORE");
    let after = make("PRIVATE_AFTER");
    assert_ne!(before.fingerprint, after.fingerprint);
    let authors = authors(vec![entry(0, vec![admit_record(PLACE)])]);
    assert_eq!(
        present(&before, &authors, 0, &grant(&before)),
        present(&after, &authors, 0, &grant(&after))
    );
}

#[test]
fn pure_admission_does_not_apply_service_item_caps_before_installed_text_policy() {
    let world = world(vec![]);
    let authors = authors(vec![
        entry(0, (1..=1024).map(|n| admit_text(n, "x")).collect()),
        entry(1, vec![admit_text(1025, "last")]),
    ]);
    assert_eq!(present(&world, &authors, 1, &grant(&world)).len(), 1025);
}
