//! Family selections share one immutable frame without becoming frame fields.
#[path = "support/family_context.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value as Json, json};
use wedl::{
    environment,
    writing_frames::{self, FamilySelection, Frame, Lens},
};

fn frame(world: &wedl::model::World, lens: Lens) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: Some(SCENE.into()),
        pov_id: Some(VIEWER.into()),
        at: at(5, 0),
        lens,
    }
}
fn decode(value: Json) -> FamilySelection {
    writing_frames::decode_families(&value.to_string()).unwrap()
}
fn read(world: &wedl::model::World, lens: Lens, families: &FamilySelection) -> Json {
    writing_frames::context_with_families(
        world,
        None,
        &frame(world, lens),
        &[],
        families,
        1_000_000,
    )
    .unwrap()
}

#[test]
fn family_json_is_separate_closed_bounded_and_duplicate_safe() {
    let accepted = families();
    assert!(writing_frames::decode_families(&accepted.to_string()).is_ok());
    assert!(writing_frames::decode_families("{}").is_ok());
    for invalid in [
        json!([]),
        json!({"work":{}}),
        json!({"species":null}),
        json!({"magic":{"author":null}}),
        json!({"environment":{"contexts":[concept_id(10)],"knownIds":[concept_id(12)]}}),
        json!({"environment":{"contexts":[concept_id(10)],"beliefTargets":[concept_id(10)]}}),
        json!({"environment":{"contexts":[concept_id(10)],"beliefTargets":[id("know",90),id("know",90)]}}),
        json!({"social":{"language":concept_id(1),"form":null,"contexts":[]}}),
        json!({"magic":{"rootId":concept_id(30),"selection":magic_selection()}}),
    ] {
        assert!(
            writing_frames::decode_families(&invalid.to_string()).is_err(),
            "{invalid}"
        );
    }
    for raw in [
        "{\"magic\":{},\"magic\":{}}",
        "{\"environment\":{\"contexts\":[],\"contexts\":[]}}",
    ] {
        assert!(writing_frames::decode_families(raw).is_err());
    }
    let mut invalid = accepted;
    invalid["species"]["selection"]["known"] = json!([concept_id(20)]);
    assert!(writing_frames::decode_families(&invalid.to_string()).is_err());
    assert!(writing_frames::decode_families(&format!("{}{}", " ".repeat(16_384), "{}")).is_err());
}

#[test]
fn no_family_selection_preserves_all_existing_frame_outputs() {
    let world = world();
    for lens in [Lens::Author, Lens::Character, Lens::DramaticIrony] {
        let selected = frame(&world, lens);
        assert_eq!(
            writing_frames::context(&world, None, &selected, &[], 1_000_000).unwrap(),
            writing_frames::context_with_families(
                &world,
                None,
                &selected,
                &[],
                &FamilySelection::default(),
                1_000_000
            )
            .unwrap()
        );
    }
}

#[test]
fn dramatic_character_family_component_exactly_equals_standalone() {
    let world = world();
    let selected = decode(families());
    let character = read(&world, Lens::Character, &selected);
    let author = read(&world, Lens::Author, &selected);
    let dramatic = read(&world, Lens::DramaticIrony, &selected);
    assert_eq!(dramatic["characterPrompt"], character["characterPrompt"]);
    assert_eq!(dramatic["authorMargin"], author["authorMargin"]);
    assert_eq!(
        character["characterPrompt"]["familyContexts"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
    assert!(
        author["authorMargin"]
            .to_string()
            .contains("AUTHOR_CANON_VALUE")
    );
    let text = character["characterPrompt"].to_string();
    for forbidden in [
        "AUTHOR_",
        "PRIVATE_RESOURCE",
        "PRIVATE_COMMON_TRAIT",
        "fingerprint",
        "revision",
        "classification",
        "sourceField",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}: {text}");
    }
}

#[test]
fn magic_character_branch_never_reads_optional_canonical_author_selectors() {
    let world = world();
    let basic = decode(json!({"magic":{"beliefTargets":[id("assertion",90)]}}));
    let expected = read(&world, Lens::Character, &basic);
    let mut extra = json!({"magic":{"author":{"rootId":concept_id(999),"selection":magic_selection()},"beliefTargets":[id("assertion",90)]}});
    extra["magic"]["author"]["selection"]["profiles"]["tradition"] = json!("absent-profile");
    assert_eq!(read(&world, Lens::Character, &decode(extra)), expected);
    let no_beliefs =
        decode(json!({"magic":{"author":{"rootId":concept_id(30),"selection":magic_selection()}}}));
    assert!(
        writing_frames::context_with_families(
            &world,
            None,
            &frame(&world, Lens::Character),
            &[],
            &no_beliefs,
            1_000_000
        )
        .is_err()
    );
    assert!(
        writing_frames::context_with_families(
            &world,
            None,
            &frame(&world, Lens::Author),
            &[],
            &basic,
            1_000_000
        )
        .is_err()
    );
    assert!(
        read(&world, Lens::Author, &no_beliefs)["authorMargin"]["familyContexts"]["magic"]
            .is_object()
    );
}

#[test]
fn author_family_component_does_not_consult_selected_character_beliefs() {
    let world = world();
    let before = read(&world, Lens::Author, &decode(families()));
    let mut changed = families();
    for family in ["species", "magic", "social", "environment"] {
        changed[family]["beliefTargets"] = json!([id("assertion", 999)]);
    }
    assert_eq!(read(&world, Lens::Author, &decode(changed)), before);
}

#[test]
fn only_selected_scene_scope_supplies_scene_subject_family_roots() {
    let world = world();
    assert!(
        environment::character_context(&world, VIEWER, &[concept_id(18)], &[], &at(5, 0)).is_err()
    );
    let families = decode(json!({"environment":{"contexts":[concept_id(18)]}}));
    let value = read(&world, Lens::Character, &families);
    assert_eq!(
        value["characterPrompt"]["familyContexts"]["environment"]["count"],
        1
    );
    assert!(
        environment::character_context(&world, VIEWER, &[concept_id(18)], &[], &at(5, 0)).is_err()
    );
    let mut absent = frame(&world, Lens::Character);
    absent.at = at(20, 1);
    assert!(
        writing_frames::context_with_families(&world, None, &absent, &[], &families, 1_000_000)
            .is_err()
    );
}

#[test]
fn private_edits_leave_complete_character_family_packet_unchanged_after_explicit_repin() {
    let source = world();
    let selected = decode(families());
    let before = read(&source, Lens::Character, &selected);
    let changed = edit(&source, &id("assertion", 90), |data| {
        data["value"] = serde_yaml::to_value("PRIVATE_CANON_CORRECTION").unwrap();
        data["subject"] = serde_yaml::to_value(reference(30)).unwrap();
        data["classification"] = serde_yaml::to_value("unresolved").unwrap();
    });
    let changed = edit(&changed, &concept_id(12), |data| {
        data["values"]["note"] = serde_yaml::to_value("PRIVATE_DIFFERENT_RESOURCE").unwrap();
        data["title"] = serde_yaml::to_value("PRIVATE_DIFFERENT_TITLE").unwrap();
    });
    valid(&changed);
    let stale = writing_frames::context_with_families(
        &changed,
        None,
        &frame(&source, Lens::Character),
        &[],
        &selected,
        1_000_000,
    )
    .unwrap_err()
    .to_string();
    assert!(!stale.contains(&changed.fingerprint));
    assert_eq!(
        read(&changed, Lens::Character, &selected)["characterPrompt"],
        before["characterPrompt"]
    );
    assert_eq!(read(&source, Lens::Character, &selected), before);
}

#[test]
fn family_horizon_comes_only_from_exact_frame_coordinates() {
    let source = world();
    let selected = decode(families());
    let mut early = frame(&source, Lens::Character);
    early.at = at(2, 0);
    let value =
        writing_frames::context_with_families(&source, None, &early, &[], &selected, 1_000_000)
            .unwrap();
    assert_eq!(
        value["characterPrompt"]["familyContexts"]["magic"]["beliefs"][0]["result"]["count"],
        0
    );
    let huge = 9_007_199_254_740_993_i64;
    let source = set(
        &source,
        SCENE,
        "time",
        json!({"start":point(0,0),"current":point(huge,0),"end":point(huge+1,0)}),
    );
    valid(&source);
    let mut later = frame(&source, Lens::Character);
    later.at = at(huge, 0);
    let value =
        writing_frames::context_with_families(&source, None, &later, &[], &selected, 1_000_000)
            .unwrap();
    assert_eq!(value["frame"]["at"]["tick"], huge.to_string());
    assert_eq!(
        value["characterPrompt"]["familyContexts"]["environment"]["effectiveTime"]["tick"],
        huge.to_string()
    );
}

#[test]
fn unavailable_required_family_and_brief_precede_context_budget_failures() {
    let source = world();
    let source = set(
        &source,
        &concept_id(1),
        "descriptions",
        json!([section(1, &"é".repeat(140_000))]),
    );
    valid(&source);
    let selected = decode(
        json!({"social":{"language":concept_id(1),"sense":concept_id(2),"form":null,"contexts":[]},
        "environment":{"contexts":[concept_id(999)]}}),
    );
    let unavailable = writing_frames::context_with_families(
        &source,
        None,
        &frame(&source, Lens::Character),
        &[],
        &selected,
        1,
    )
    .unwrap_err()
    .to_string();
    assert!(
        unavailable.contains("writing frame is unavailable"),
        "{unavailable}"
    );
    let required_brief = writing_frames::context_with_families(
        &source,
        None,
        &frame(&source, Lens::Character),
        &[id("brief", 999)],
        &decode(families()),
        1,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(required_brief, unavailable);
}

#[test]
fn complete_family_context_budget_counts_unicode_scalars_and_all_metadata() {
    let source = world();
    let source = set(
        &source,
        &concept_id(10),
        "descriptions",
        json!([section(10, "Exact é♥ family wording")]),
    );
    valid(&source);
    let families = decode(families());
    let selected = frame(&source, Lens::DramaticIrony);
    let mut expected = writing_frames::context_with_families(
        &source,
        None,
        &selected,
        &[id("brief", 80)],
        &families,
        1_000_000,
    )
    .unwrap();
    let mut bound = expected.to_string().chars().count();
    loop {
        expected["maxCharacters"] = json!(bound);
        let actual = expected.to_string().chars().count();
        if actual == bound {
            break;
        }
        bound = actual;
    }
    assert!(expected.to_string().len() > bound);
    assert_eq!(
        writing_frames::context_with_families(
            &source,
            None,
            &selected,
            &[id("brief", 80)],
            &families,
            bound
        )
        .unwrap(),
        expected
    );
    assert!(
        writing_frames::context_with_families(
            &source,
            None,
            &selected,
            &[id("brief", 80)],
            &families,
            bound - 1
        )
        .is_err()
    );
}

#[test]
fn independent_family_selections_never_modify_frame_or_world() {
    let world = world();
    let selected = frame(&world, Lens::Character);
    let before = serde_json::to_value(&selected).unwrap();
    let source_hash = world.fingerprint.clone();
    let social =
        decode(json!({"social":{"language":concept_id(1),"sense":null,"form":null,"contexts":[]}}));
    let environment = decode(json!({"environment":{"contexts":[concept_id(10)]}}));
    let first =
        writing_frames::context_with_families(&world, None, &selected, &[], &social, 1_000_000)
            .unwrap();
    let second = writing_frames::context_with_families(
        &world,
        None,
        &selected,
        &[],
        &environment,
        1_000_000,
    )
    .unwrap();
    assert!(
        first["characterPrompt"]["familyContexts"]
            .get("environment")
            .is_none()
    );
    assert!(
        second["characterPrompt"]["familyContexts"]
            .get("social")
            .is_none()
    );
    assert_eq!(
        writing_frames::resolve(&world, None, &selected).unwrap(),
        before
    );
    assert_eq!(before.as_object().unwrap().len(), 6);
    assert_eq!(world.fingerprint, source_hash);
}

#[test]
fn complete_native_unknown_linkage_is_retained_without_claiming_action_readiness() {
    let world = world();
    let mut families = families();
    families["species"]["selection"]["common_link"] = json!("traits");
    let families = decode(families);
    let value = read(&world, Lens::DramaticIrony, &families);
    for component in ["characterPrompt", "authorMargin"] {
        let species = &value[component]["familyContexts"]["species"];
        assert_eq!(species["linkageComplete"], false);
        assert!(!species["unknowns"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        value["characterPrompt"],
        read(&world, Lens::Character, &families)["characterPrompt"]
    );
}

#[test]
fn unavailable_family_precedes_a_separately_overlimit_required_brief() {
    let source = world();
    let mut records = source.records.values().cloned().collect::<Vec<_>>();
    for n in 1000..2000 {
        records.push(record("object", &id("obj", n), json!({})));
    }
    let source = from_records(records);
    let source = set(
        &source,
        &id("brief", 80),
        "required_refs",
        json!(
            (1000..2000)
                .map(|n| json!({"kind":"record","id":id("obj",n)}))
                .collect::<Vec<_>>()
        ),
    );
    valid(&source);
    let selected = frame(&source, Lens::Author);
    let pins = [id("brief", 80)];
    let brief_error = writing_frames::context_with_families(
        &source,
        None,
        &selected,
        &pins,
        &FamilySelection::default(),
        1_000_000,
    )
    .unwrap_err()
    .to_string();
    assert!(brief_error.contains("1000 items"), "{brief_error}");
    let missing = decode(json!({"environment":{"contexts":[concept_id(999)]}}));
    let unavailable =
        writing_frames::context_with_families(&source, None, &selected, &[], &missing, 1_000_000)
            .unwrap_err()
            .to_string();
    assert_eq!(
        writing_frames::context_with_families(&source, None, &selected, &pins, &missing, 1_000_000)
            .unwrap_err()
            .to_string(),
        unavailable
    );
    assert!(
        unavailable.contains("writing frame is unavailable"),
        "{unavailable}"
    );
}
