//! Application operations shared by the command line and the local HTTP adapter.
//! These are the Rust prototype's contracts, not the Python transport protocol.
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use serde_json::{Value, json};

use crate::{
    Error, Result, action_drafts, action_outcomes, action_requirements, action_store, assertion,
    authoring, briefs, catalogue, character, chronology, compiler, consequences, current_time,
    description, edit_drafts, environment, event_consequences, existence, generational, geometry,
    governance, hypotheses, learned_assertions, lore, lore_state, magic, manuscript,
    model::{StoryTime, World},
    narrative, navigation, retcon_impact, retcon_reviews, retrieval, revision_review, runtime,
    scene, social, spatial, spatial_history, species, story, validation, writing_frames,
};

pub const PROTOCOL: &str = crate::CONTRACT;

fn action_budget(max_characters: usize) -> Result<()> {
    if !(256..=100_000).contains(&max_characters) {
        return Err(Error::InvalidInput(
            "maxCharacters must be between 256 and 100000".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct TimeInput {
    pub tick: i64,
    pub order: i32,
    pub timeline: Option<String>,
}

/// Exact thread grouping selectors; membership is checked after ranking.
pub fn thread_ids_json(raw: &str) -> Result<Vec<String>> {
    if raw.len() > 16_384 {
        return Err(Error::InvalidInput(
            "thread selection exceeds 16 KiB".into(),
        ));
    }
    let ids: Vec<String> = serde_json::from_str(raw)
        .map_err(|_| Error::InvalidInput("threads must be a JSON string array".into()))?;
    if ids.is_empty() || ids.len() > 32 {
        return Err(Error::InvalidInput(
            "select between 1 and 32 exact thread IDs".into(),
        ));
    }
    Ok(ids)
}

/// Bounded exact selectors used by navigation reads. Sorting does not admit a
/// record; each query applies its own source/status/time rules afterwards.
pub(crate) fn navigation_ids_json(raw: &str, label: &str, max: usize) -> Result<Vec<String>> {
    if raw.len() > 16_384 {
        return Err(Error::InvalidInput(format!(
            "{label} selection exceeds 16 KiB"
        )));
    }
    let mut ids: Vec<String> = serde_json::from_str(raw)
        .map_err(|_| Error::InvalidInput(format!("{label} must be a JSON string array")))?;
    if ids.is_empty() || ids.len() > max || ids.iter().any(|id| id.trim().is_empty()) {
        return Err(Error::InvalidInput(format!(
            "select between 1 and {max} exact {label} IDs"
        )));
    }
    ids.sort();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::InvalidInput(format!("duplicate {label} selection")));
    }
    Ok(ids)
}

/// Closed role mappings use JSON string rules and reject repeated object keys.
pub fn species_fields_json(value: &str) -> Result<BTreeMap<String, String>> {
    if value.len() > 16_384 {
        return Err(Error::InvalidInput(
            "species field mapping exceeds 16 KiB".into(),
        ));
    }
    let mut decoder = serde_json::Deserializer::from_str(value);
    let _: serde_yaml::Value = serde::Deserialize::deserialize(&mut decoder).map_err(|error| {
        Error::InvalidInput(format!(
            "invalid or duplicate species field mapping: {error}"
        ))
    })?;
    decoder
        .end()
        .map_err(|error| Error::InvalidInput(format!("invalid species field JSON: {error}")))?;
    serde_json::from_str(value).map_err(|error| {
        Error::InvalidInput(format!(
            "species fields require a JSON object of role to root-field strings: {error}"
        ))
    })
}

/// Magic role selections are exact JSON objects; repeated keys are invalid.
pub fn magic_selection_json(
    profiles: &str,
    links: Option<&str>,
    restrictions: Option<&str>,
    exceptions: Option<&str>,
) -> Result<magic::Selection> {
    fn mapping<T: serde::de::DeserializeOwned>(value: &str, name: &str) -> Result<T> {
        if value.len() > 16_384 {
            return Err(Error::InvalidInput(format!(
                "magic {name} mapping exceeds 16 KiB"
            )));
        }
        let mut decoder = serde_json::Deserializer::from_str(value);
        let _: serde_yaml::Value =
            serde::Deserialize::deserialize(&mut decoder).map_err(|error| {
                Error::InvalidInput(format!(
                    "invalid or duplicate magic {name} mapping: {error}"
                ))
            })?;
        decoder
            .end()
            .map_err(|error| Error::InvalidInput(format!("invalid magic {name} JSON: {error}")))?;
        serde_json::from_str(value)
            .map_err(|error| Error::InvalidInput(format!("invalid magic {name} mapping: {error}")))
    }
    let selection = magic::Selection {
        profiles: mapping(profiles, "profiles")?,
        links: mapping(links.unwrap_or("{}"), "links")?,
        restrictions: mapping(restrictions.unwrap_or("{}"), "restrictions")?,
        exceptions: mapping(exceptions.unwrap_or("{}"), "exceptions")?,
    };
    selection.validate()?;
    Ok(selection)
}

/// Governance bindings require all six explicit roles and reject repeated JSON keys.
pub fn governance_binding_json(profile: &str, roles: &str) -> Result<governance::Binding> {
    if roles.len() > 16_384 {
        return Err(Error::InvalidInput(
            "governance role mapping exceeds 16 KiB".into(),
        ));
    }
    let mut decoder = serde_json::Deserializer::from_str(roles);
    let _: serde_yaml::Value = serde::Deserialize::deserialize(&mut decoder).map_err(|error| {
        Error::InvalidInput(format!(
            "invalid or duplicate governance role mapping: {error}"
        ))
    })?;
    decoder
        .end()
        .map_err(|error| Error::InvalidInput(format!("invalid governance role JSON: {error}")))?;
    let binding = governance::Binding {
        profile: profile.into(),
        roles: serde_json::from_str(roles).map_err(|error| {
            Error::InvalidInput(format!(
                "governance roles require an object of field strings or null: {error}"
            ))
        })?,
    };
    binding.validate()?;
    Ok(binding)
}

/// Explicit family context roots use a closed JSON array of exact ID strings.
pub fn context_roots_json(value: &str) -> Result<Vec<String>> {
    if value.len() > 16_384 {
        return Err(Error::InvalidInput("context roots exceed 16 KiB".into()));
    }
    serde_json::from_str(value).map_err(|_| {
        Error::InvalidInput("contexts must be a JSON array of exact concept ID strings".into())
    })
}

/// Writing context pins are an explicit JSON array; they do not expand the frame's scope.
pub fn writing_briefs_json(value: &str) -> Result<Vec<String>> {
    if value.len() > 16_384 {
        return Err(Error::InvalidInput(
            "writing brief selections exceed 16 KiB".into(),
        ));
    }
    serde_json::from_str(value).map_err(|_| {
        Error::InvalidInput("briefs must be a JSON array of exact brief ID strings".into())
    })
}

/// Explicit learned targets are selectors, never grants for canonical subjects.
pub fn belief_targets_json(value: &str) -> Result<Vec<String>> {
    if value.len() > 16_384 {
        return Err(Error::InvalidInput("belief targets exceed 16 KiB".into()));
    }
    serde_json::from_str(value).map_err(|_| {
        Error::InvalidInput(
            "beliefTargets must be a JSON array of exact assertion or knowledge IDs".into(),
        )
    })
}

#[derive(Clone, Debug)]
pub enum Operation {
    AuthoringSchema,
    AuthoringContext {
        max_items: usize,
    },
    AuthoringScaffold {
        request: crate::authoring_tools::ScaffoldRequest,
    },
    Status,
    Validate,
    Compile,
    HypothesisList {
        status: Option<String>,
        text: Option<String>,
        max_items: usize,
        max_characters: usize,
    },
    HypothesisDetail {
        hypothesis_id: String,
        max_characters: usize,
    },
    HypothesisPreview {
        request: hypotheses::Request,
        max_characters: usize,
    },
    Catalogue {
        request: catalogue::Request,
    },
    ActionDraftList,
    EditDraftList,
    EditDraftDetail {
        id: String,
        expected_sha256: Option<String>,
    },
    EditDraftPreview {
        request: edit_drafts::SaveRequest,
    },
    EditDraftSave {
        request: edit_drafts::SaveRequest,
    },
    EditDraftReview {
        request: edit_drafts::ReviewRequest,
    },
    EditDraftApply {
        request: edit_drafts::ApplyRequest,
    },
    RevisionReview {
        request: revision_review::Request,
    },
    RevealResolve {
        request: manuscript::RevealResolveRequest,
    },
    ReaderPreview {
        request: String,
    },
    ManuscriptList {
        request: manuscript::ListRequest,
    },
    ManuscriptDetail {
        request: manuscript::DetailRequest,
    },
    ManuscriptResolve {
        request: manuscript::ResolveRequest,
    },
    ManuscriptSavePreview {
        request: manuscript::SaveRequest,
    },
    ManuscriptSave {
        request: manuscript::SaveRequest,
    },
    RetconImpactPreview {
        request: retcon_impact::PreviewRequest,
    },
    RetconReviewPreview {
        request: retcon_reviews::PreviewRequest,
    },
    RetconReviewSavePreview {
        request: retcon_reviews::SaveRequest,
    },
    RetconReviewSave {
        request: retcon_reviews::SaveRequest,
    },
    RetconReviewQueue {
        request: retcon_reviews::QueueRequest,
    },
    RetconReviewDetail {
        request: retcon_reviews::DetailRequest,
    },
    EventConsequences {
        request: event_consequences::EventRequest,
    },
    GroupedConsequences {
        request: event_consequences::GroupedRequest,
    },
    ActionDraftDetail {
        id: String,
        expected_sha256: Option<String>,
    },
    ActionDraftPreview {
        request: action_drafts::SaveRequest,
    },
    ActionDraftSave {
        request: action_drafts::SaveRequest,
    },
    ActionDraftReview {
        request: action_drafts::ReviewRequest,
    },
    ActionProposalPreview {
        request: action_store::ProposalPreviewRequest,
    },
    ActionOutcomeReview {
        request: action_outcomes::ReviewRequest,
    },
    ActionOutcomeApply {
        request: action_outcomes::ApplyRequest,
    },
    ActionOutcomeVerify {
        request: action_outcomes::VerifyRequest,
    },
    ConsequencesPreview {
        plan: consequences::Plan,
        max_characters: usize,
    },
    ConsequencesCheck {
        request: crate::consequence_semantics::CheckRequest,
    },
    ActionRequirementCapture {
        frame: writing_frames::Frame,
        families: writing_frames::FamilySelection,
        max_characters: usize,
    },
    ActionRequirementCompare {
        frame: writing_frames::Frame,
        families: writing_frames::FamilySelection,
        requirement: action_requirements::Requirement,
        max_characters: usize,
    },
    EntityList {
        kind: Option<String>,
        text: Option<String>,
    },
    EntityShow {
        entity: String,
    },
    BriefCompose {
        brief: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    WritingResolve {
        frame: writing_frames::Frame,
    },
    CurrentTimePreview {
        request: current_time::Request,
        max_characters: usize,
    },
    SceneControlPreview {
        request: crate::scene_control::Request,
        max_characters: usize,
    },
    ConversationControlPreview {
        request: crate::conversation_control::Request,
        max_characters: usize,
    },
    WritingChoices {
        frame: writing_frames::Frame,
        limit: usize,
    },
    WritingContext {
        frame: writing_frames::Frame,
        briefs: Vec<String>,
        max_characters: usize,
    },
    WritingFamilyContext {
        frame: writing_frames::Frame,
        briefs: Vec<String>,
        families: writing_frames::FamilySelection,
        max_characters: usize,
    },
    DescriptionList {
        owner: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    DescriptionSection {
        owner: String,
        section: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
    },
    AssertionPredicates {
        limit: usize,
    },
    AssertionList {
        target: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    ExistenceList {
        character: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    GovernanceQuery {
        target: String,
        binding: governance::Binding,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    SocialContext {
        selected: social::Selection,
        at: TimeInput,
        audience: String,
        view: String,
    },
    EnvironmentContext {
        roots: Vec<String>,
        at: TimeInput,
        audience: String,
        view: String,
    },
    LoreProfiles {
        limit: usize,
    },
    LoreProfile {
        profile: String,
    },
    LoreCatalog {
        at: Option<TimeInput>,
        audience: String,
        view: String,
        domain: Option<String>,
        limit: usize,
    },
    LoreConcept {
        concept: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
    },
    LoreLinks {
        record: String,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    LoreState {
        record: String,
        at: TimeInput,
        audience: String,
        view: String,
    },
    SpeciesCompose {
        individual: String,
        selection: species::Selection,
        at: Option<TimeInput>,
        audience: String,
        view: String,
    },
    SpeciesAffected {
        common: String,
        selection: species::Selection,
        at: Option<TimeInput>,
        audience: String,
        view: String,
        limit: usize,
    },
    MagicCompose {
        root: String,
        selection: magic::Selection,
        at: Option<TimeInput>,
        audience: String,
        view: String,
    },
    GenerationalFamily {
        character: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalLineage {
        character: String,
        at: TimeInput,
        direction: generational::LineageDirection,
        max_depth: usize,
        limit: usize,
    },
    GenerationalPath {
        from: String,
        to: String,
        at: TimeInput,
        max_hops: usize,
        limit: usize,
    },
    GenerationalHistory {
        record: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalOrganization {
        organization: String,
        at: TimeInput,
        max_depth: usize,
        include_former: bool,
        limit: usize,
    },
    GenerationalLegacy {
        legacy: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalOrganizationLegacies {
        organization: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalVital {
        character: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalLearnedAssertions {
        viewer: String,
        at: TimeInput,
        kind: Option<String>,
        endpoint: Option<String>,
        held_history: bool,
        limit: usize,
    },
    GenerationalLearnedFamily {
        viewer: String,
        character: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalLearnedLineage {
        viewer: String,
        character: String,
        at: TimeInput,
        direction: generational::LineageDirection,
        max_depth: usize,
        limit: usize,
    },
    GenerationalLearnedPath {
        viewer: String,
        from: String,
        to: String,
        at: TimeInput,
        max_hops: usize,
        limit: usize,
    },
    GenerationalLearnedOrganization {
        viewer: String,
        organization: String,
        at: TimeInput,
        max_depth: usize,
        limit: usize,
    },
    GenerationalLearnedLegacy {
        viewer: String,
        legacy: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalLearnedVital {
        viewer: String,
        character: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalLabels {
        viewer: Option<String>,
        ids: Vec<String>,
        at: TimeInput,
        limit: usize,
    },
    GenerationalDiscover {
        viewer: Option<String>,
        kind: String,
        prefix: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalSearch {
        viewer: Option<String>,
        text: String,
        at: TimeInput,
        limit: usize,
    },
    GenerationalContext {
        viewer: Option<String>,
        subject: String,
        at: TimeInput,
        max_depth: usize,
        limit: usize,
        max_chars: usize,
    },
    SpatialLocation {
        entity: String,
        mode: Option<String>,
        limit: usize,
    },
    SpatialHistory {
        record: String,
        at: Option<TimeInput>,
        limit: usize,
    },
    SpatialRoute {
        route: String,
        at: Option<TimeInput>,
    },
    SpatialPath {
        from: String,
        to: String,
        mode: Option<String>,
        metric: String,
        unit: Option<String>,
        limit: usize,
    },
    SpatialReachable {
        from: String,
        mode: Option<String>,
        limit: usize,
    },
    SpatialOverlays {
        location: String,
        at: TimeInput,
        audience: String,
        view: String,
        overlay: Option<String>,
        limit: usize,
    },
    SpatialMaps {
        limit: usize,
    },
    SpatialAnchors {
        from_map: Option<String>,
        to_map: Option<String>,
        limit: usize,
    },
    SpatialBbox {
        map: String,
        min: Vec<f64>,
        max: Vec<f64>,
        relation: String,
        limit: usize,
    },
    SpatialNearby {
        map: String,
        point: Vec<f64>,
        radius: f64,
        limit: usize,
    },
    SpatialBboxAt {
        map: String,
        min: Vec<f64>,
        max: Vec<f64>,
        relation: String,
        at: TimeInput,
        limit: usize,
    },
    SpatialNearbyAt {
        map: String,
        point: Vec<f64>,
        radius: f64,
        at: TimeInput,
        limit: usize,
    },
    ChronologyCalendars {
        limit: usize,
    },
    ChronologyDate {
        calendar: String,
        value: chronology::CivilDate,
        era: Option<String>,
    },
    ChronologyConvert {
        from: String,
        to: String,
        value: chronology::CivilDate,
    },
    ChronologyToStory {
        calendar: String,
        value: chronology::CivilDate,
        timeline: Option<String>,
    },
    ChronologyFromStory {
        calendar: String,
        at: TimeInput,
    },
    ChronologyAnnotations {
        record: Option<String>,
        calendar: Option<String>,
        era: Option<String>,
        limit: usize,
    },
    ChronologyCompare {
        left_record: String,
        left_id: String,
        right_record: String,
        right_id: String,
    },
    ChronologyFormat {
        value: chronology::DateInput,
    },
    ChronologyConvertDate {
        value: chronology::DateInput,
        target: chronology::DateTarget,
    },
    ChronologySearch {
        query: chronology::AnnotationQuery,
    },
    ChronologyStoryTimes {
        value: chronology::DateInput,
        upper: Option<chronology::DateInput>,
        timeline: Option<String>,
        limit: usize,
    },
    Scene {
        entity: String,
        at: TimeInput,
    },
    CharacterSpeciesCompose {
        character: String,
        individual: String,
        selection: species::Selection,
        belief_targets: Vec<String>,
        at: TimeInput,
    },
    CharacterSpeciesAffected {
        character: String,
        common: String,
        selection: species::Selection,
        at: TimeInput,
        limit: usize,
    },
    CharacterMagic {
        character: String,
        belief_targets: Vec<String>,
        at: TimeInput,
    },
    CharacterSocialContext {
        character: String,
        selected: social::Selection,
        belief_targets: Vec<String>,
        at: TimeInput,
    },
    CharacterEnvironmentContext {
        character: String,
        roots: Vec<String>,
        belief_targets: Vec<String>,
        at: TimeInput,
    },
    CharacterContext {
        character: String,
        at: TimeInput,
        max_characters: usize,
    },
    CharacterBrief {
        character: String,
        brief: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterSearch {
        character: String,
        query: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterSearchWithMode {
        character: String,
        query: String,
        at: TimeInput,
        limit: usize,
        mode: retrieval::Mode,
    },
    CharacterConversation {
        character: String,
        conversation: String,
        at: TimeInput,
    },
    CharacterAssertions {
        character: String,
        target: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterDescriptions {
        character: String,
        owner: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterSection {
        character: String,
        owner: String,
        section: String,
        at: TimeInput,
    },
    CharacterLore {
        character: String,
        domain: Option<String>,
        at: TimeInput,
        limit: usize,
    },
    CharacterConcept {
        character: String,
        target: String,
        at: TimeInput,
    },
    CharacterLinks {
        character: String,
        owner: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterSpatialHistory {
        character: String,
        owner: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterExistence {
        character: String,
        target: String,
        at: TimeInput,
        limit: usize,
    },
    CharacterGovernance {
        character: String,
        target: String,
        at: TimeInput,
        limit: usize,
    },
    State {
        entity: String,
        at: TimeInput,
    },
    Knowledge {
        entity: String,
        at: TimeInput,
        include_forgotten: bool,
    },
    Relationships {
        entity: String,
        at: TimeInput,
    },
    StoryPoints {
        at: TimeInput,
        scene: Option<String>,
        limit: usize,
    },
    StoryPointTrail {
        story_point: String,
        at: TimeInput,
        scene: Option<String>,
        limit: usize,
    },
    Causal {
        event: String,
        at: TimeInput,
        direction: String,
        limit: usize,
    },
    Whereabouts {
        at: TimeInput,
        characters: Option<Vec<String>>,
        limit: usize,
    },
    Interactions {
        first: String,
        second: String,
        at: TimeInput,
        limit: usize,
    },
    ThreadMemberships {
        record_ids: Vec<String>,
        thread_ids: Vec<String>,
        at: Option<TimeInput>,
        limit: usize,
    },
    Environments {
        entity: String,
        at: TimeInput,
    },
    Timeline {
        timeline: Option<String>,
        at: Option<TimeInput>,
        limit: usize,
    },
    Conversation {
        entity: String,
        at: TimeInput,
    },
    Context {
        entity: String,
        at: TimeInput,
        max_characters: usize,
    },
    Search {
        query: String,
        limit: usize,
        kind: Option<String>,
    },
    SearchWithMode {
        query: String,
        limit: usize,
        kind: Option<String>,
        mode: retrieval::Mode,
    },
    SearchWithSelection {
        query: String,
        limit: usize,
        kind: Option<String>,
        mode: retrieval::Mode,
        at: Option<TimeInput>,
        thread_ids: Option<Vec<String>>,
    },
    SearchWithHypotheses {
        query: String,
        limit: usize,
        kind: Option<String>,
        mode: retrieval::Mode,
        at: Option<TimeInput>,
        thread_ids: Option<Vec<String>>,
    },
    Threads,
    PreviewBody {
        entity: String,
        body: String,
        expected_fingerprint: String,
    },
    ApplyBody {
        entity: String,
        body: String,
        expected_fingerprint: String,
        expected_after_sha256: String,
    },
    PreviewRecordCreate {
        draft: String,
        expected_fingerprint: String,
    },
    ApplyRecordCreate {
        draft: String,
        expected_fingerprint: String,
        expected_after_sha256: String,
    },
    PreviewRecordUpdate {
        entity: String,
        patch: String,
        expected_fingerprint: String,
    },
    ApplyRecordUpdate {
        entity: String,
        patch: String,
        expected_fingerprint: String,
        expected_after_sha256: String,
    },
}

impl Operation {
    /// These operations are static or select and capture their own original/
    /// private source. A malformed current world must not substitute for, or
    /// prevent access to, source-independent guidance or explicit originals.
    pub(crate) fn independent_source(&self) -> bool {
        matches!(
            self,
            Operation::AuthoringSchema
                | Operation::RevealResolve { .. }
                | Operation::ReaderPreview { .. }
                | Operation::RevisionReview { .. }
                | Operation::RetconReviewPreview { .. }
                | Operation::RetconReviewQueue { .. }
                | Operation::RetconReviewDetail { .. }
                | Operation::ManuscriptList { .. }
                | Operation::ManuscriptDetail { .. }
                | Operation::ManuscriptResolve { .. }
        )
    }
}

#[derive(Clone)]
pub struct Service {
    source_root: Arc<PathBuf>,
    world: Arc<World>,
    validation: Arc<validation::ValidationReport>,
    reader: Arc<crate::reader::PresentationService>,
}

impl Service {
    pub fn new(world: World) -> Self {
        let validation = Arc::new(validation::validate_world(&world));
        let reader = Arc::new(crate::reader::PresentationService::new(Some(
            world.root.clone(),
        )));
        Self {
            source_root: Arc::new(world.root.clone()),
            world: Arc::new(world),
            validation,
            reader,
        }
    }

    pub fn reader(&self) -> &crate::reader::PresentationService {
        &self.reader
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    /// Capture an immutable worktree snapshot or reuse a byte-identical bounded
    /// capture, retaining installed reader policy and disclosure state.
    /// HTTP owns request serialization and the previous bounded capture.
    pub(crate) fn capture_worktree(&self, bounded: bool) -> Result<Self> {
        if self.world.revision.is_some() {
            return Err(Error::InvalidInput(
                "live author requests require the current worktree".into(),
            ));
        }
        let previous = bounded.then_some(self.world());
        match crate::historical::capture_current_world(&self.source_root, previous)? {
            Some(world) => Ok(Self {
                source_root: self.source_root.clone(),
                validation: Arc::new(validation::validate_world(&world)),
                world: Arc::new(world),
                reader: self.reader.clone(),
            }),
            None => Ok(self.clone()),
        }
    }

    fn time(&self, input: &TimeInput) -> Result<StoryTime> {
        self.world
            .story_time(input.tick, input.timeline.as_deref(), input.order)
    }

    pub fn execute(&self, operation: Operation) -> Result<Value> {
        if matches!(operation, Operation::AuthoringSchema) {
            return Ok(json!({"protocol":PROTOCOL,"result":crate::authoring_schema::schema()}));
        }
        let world = self.world();
        let independent_source = operation.independent_source();
        let character_view = matches!(
            &operation,
            Operation::CharacterContext { .. }
                | Operation::CharacterBrief { .. }
                | Operation::CharacterSpeciesCompose { .. }
                | Operation::CharacterSpeciesAffected { .. }
                | Operation::CharacterMagic { .. }
                | Operation::CharacterSocialContext { .. }
                | Operation::CharacterEnvironmentContext { .. }
                | Operation::CharacterSearch { .. }
                | Operation::CharacterSearchWithMode { .. }
                | Operation::CharacterConversation { .. }
                | Operation::CharacterAssertions { .. }
                | Operation::CharacterDescriptions { .. }
                | Operation::CharacterSection { .. }
                | Operation::CharacterLore { .. }
                | Operation::CharacterConcept { .. }
                | Operation::CharacterLinks { .. }
                | Operation::CharacterSpatialHistory { .. }
                | Operation::CharacterExistence { .. }
                | Operation::CharacterGovernance { .. }
                | Operation::GenerationalLearnedAssertions { .. }
                | Operation::GenerationalLearnedFamily { .. }
                | Operation::GenerationalLearnedLineage { .. }
                | Operation::GenerationalLearnedPath { .. }
                | Operation::GenerationalLearnedOrganization { .. }
                | Operation::GenerationalLearnedLegacy { .. }
                | Operation::GenerationalLearnedVital { .. }
                | Operation::GenerationalLabels {
                    viewer: Some(_),
                    ..
                }
                | Operation::GenerationalDiscover {
                    viewer: Some(_),
                    ..
                }
                | Operation::GenerationalSearch {
                    viewer: Some(_),
                    ..
                }
                | Operation::GenerationalContext {
                    viewer: Some(_),
                    ..
                }
        );
        if !independent_source
            && !matches!(operation, Operation::Status | Operation::Validate)
            && !self.validation.valid
        {
            return Err(Error::Validation(
                "source validation failed; run validate for diagnostics".into(),
            ));
        }
        let result = match operation {
            Operation::AuthoringSchema => {
                unreachable!("static guidance returned before source access")
            }
            Operation::AuthoringContext { max_items } => {
                crate::authoring_tools::context(world, max_items)?
            }
            Operation::AuthoringScaffold { request } => {
                crate::authoring_tools::scaffold(world, &request)?
            }
            Operation::HypothesisList {
                status,
                text,
                max_items,
                max_characters,
            } => hypotheses::list(
                world,
                status.as_deref(),
                text.as_deref(),
                max_items,
                max_characters,
            )?,
            Operation::HypothesisDetail {
                hypothesis_id,
                max_characters,
            } => hypotheses::detail(world, &hypothesis_id, max_characters)?,
            Operation::HypothesisPreview {
                request,
                max_characters,
            } => hypotheses::preview(world, &request, max_characters)?,
            Operation::Catalogue { request } => catalogue::read(world, &request)?,
            Operation::ActionDraftList => action_store::list(world)?,
            Operation::EditDraftList => edit_drafts::list(world)?,
            Operation::EditDraftDetail {
                id,
                expected_sha256,
            } => edit_drafts::detail(world, &id, expected_sha256.as_deref())?,
            Operation::EditDraftPreview { request } => edit_drafts::preview(world, &request)?,
            Operation::EditDraftSave { request } => edit_drafts::save(world, &request)?,
            Operation::EditDraftReview { request } => edit_drafts::review(world, &request)?,
            Operation::EditDraftApply { request } => edit_drafts::apply(world, &request)?,
            Operation::RevisionReview { request } => {
                revision_review::review(&self.source_root, &request)?
            }
            Operation::RevealResolve { request } => {
                manuscript::resolve_reveal(&self.source_root, &request)?
            }
            Operation::ReaderPreview { request } => self.reader.preview(request.as_bytes())?,
            Operation::ManuscriptList { request } => manuscript::list(&self.source_root, &request)?,
            Operation::ManuscriptDetail { request } => {
                manuscript::detail(&self.source_root, &request)?
            }
            Operation::ManuscriptResolve { request } => {
                manuscript::resolve(&self.source_root, &request)?
            }
            Operation::ManuscriptSavePreview { request } => {
                manuscript::save_preview(world, &request)?
            }
            Operation::ManuscriptSave { request } => manuscript::save(world, &request)?,
            Operation::RetconImpactPreview { request } => retcon_impact::preview(world, &request)?,
            Operation::RetconReviewPreview { request } => {
                retcon_reviews::preview(&self.source_root, &request)?
            }
            Operation::RetconReviewSavePreview { request } => {
                retcon_reviews::save_preview(world, &request)?
            }
            Operation::RetconReviewSave { request } => retcon_reviews::save(world, &request)?,
            Operation::RetconReviewQueue { request } => {
                retcon_reviews::queue(&self.source_root, &request)?
            }
            Operation::RetconReviewDetail { request } => {
                retcon_reviews::detail(&self.source_root, &request)?
            }
            Operation::EventConsequences { request } => event_consequences::read(world, &request)?,
            Operation::GroupedConsequences { request } => {
                event_consequences::preview(world, &request)?
            }
            Operation::ActionDraftDetail {
                id,
                expected_sha256,
            } => action_store::detail(world, &id, expected_sha256.as_deref())?,
            Operation::ActionDraftPreview { request } => action_store::preview(world, &request)?,
            Operation::ActionDraftSave { request } => action_store::save(world, &request)?,
            Operation::ActionDraftReview { request } => action_drafts::review(world, &request)?,
            Operation::ActionProposalPreview { request } => {
                action_store::proposal_preview(world, &request)?
            }
            Operation::ActionOutcomeReview { request } => action_outcomes::review(world, &request)?,
            Operation::ActionOutcomeApply { request } => action_outcomes::apply(world, &request)?,
            Operation::ActionOutcomeVerify { request } => action_outcomes::verify(world, &request)?,
            Operation::ConsequencesPreview {
                plan,
                max_characters,
            } => consequences::preview(world, &plan, max_characters)?,
            Operation::ConsequencesCheck { request } => {
                crate::consequence_semantics::check_request(world, &request)?
            }
            Operation::ActionRequirementCapture {
                frame,
                families,
                max_characters,
            } => {
                action_budget(max_characters)?;
                action_requirements::capture(world, &frame, &families, max_characters)?
            }
            Operation::ActionRequirementCompare {
                frame,
                families,
                requirement,
                max_characters,
            } => {
                action_budget(max_characters)?;
                action_requirements::compare(
                    world,
                    &frame,
                    &families,
                    &requirement,
                    max_characters,
                )?
            }
            Operation::BriefCompose {
                brief,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                briefs::compose(world, &brief, at.as_ref(), &audience, &view, limit)?
            }
            Operation::CurrentTimePreview {
                request,
                max_characters,
            } => {
                action_budget(max_characters)?;
                current_time::preview(world, &request, max_characters)?
            }
            Operation::SceneControlPreview {
                request,
                max_characters,
            } => {
                action_budget(max_characters)?;
                crate::scene_control::preview(world, &request, max_characters)?
            }
            Operation::ConversationControlPreview {
                request,
                max_characters,
            } => {
                action_budget(max_characters)?;
                crate::conversation_control::preview(world, &request, max_characters)?
            }
            Operation::WritingResolve { frame } => {
                writing_frames::with_author_snapshot(world, &frame, false, |authors| {
                    writing_frames::resolve(world, authors, &frame)
                })?
            }
            Operation::WritingChoices { frame, limit } => {
                writing_frames::with_author_snapshot(world, &frame, true, |authors| {
                    writing_frames::choices(world, authors, &frame, limit)
                })?
            }
            Operation::WritingContext {
                frame,
                briefs,
                max_characters,
            } => {
                if !(256..=100_000).contains(&max_characters) {
                    return Err(Error::InvalidInput(
                        "maxCharacters must be between 256 and 100000".into(),
                    ));
                }
                writing_frames::with_author_snapshot(world, &frame, false, |authors| {
                    writing_frames::context(world, authors, &frame, &briefs, max_characters)
                })?
            }
            Operation::WritingFamilyContext {
                frame,
                briefs,
                families,
                max_characters,
            } => {
                if !(256..=100_000).contains(&max_characters) {
                    return Err(Error::InvalidInput(
                        "maxCharacters must be between 256 and 100000".into(),
                    ));
                }
                writing_frames::with_author_snapshot(world, &frame, false, |authors| {
                    writing_frames::context_with_families(
                        world,
                        authors,
                        &frame,
                        &briefs,
                        &families,
                        max_characters,
                    )
                })?
            }
            Operation::CharacterSocialContext {
                character,
                selected,
                belief_targets,
                at,
            } => social::character_context(
                world,
                &character,
                &selected,
                &belief_targets,
                &self.time(&at)?,
            )?,
            Operation::CharacterEnvironmentContext {
                character,
                roots,
                belief_targets,
                at,
            } => environment::character_context(
                world,
                &character,
                &roots,
                &belief_targets,
                &self.time(&at)?,
            )?,
            Operation::CharacterSpeciesCompose {
                character,
                individual,
                selection,
                belief_targets,
                at,
            } => species::character_compose(
                world,
                &character,
                &individual,
                &selection,
                &belief_targets,
                &self.time(&at)?,
            )?,
            Operation::CharacterSpeciesAffected {
                character,
                common,
                selection,
                at,
                limit,
            } => species::character_affected(
                world,
                &character,
                &common,
                &selection,
                &self.time(&at)?,
                limit,
            )?,
            Operation::CharacterMagic {
                character,
                belief_targets,
                at,
            } => magic::character(world, &character, &belief_targets, &self.time(&at)?)?,
            Operation::Status => {
                let counts = world.records.values().fold(
                    std::collections::BTreeMap::<String, usize>::new(),
                    |mut counts, r| {
                        *counts.entry(r.kind.clone()).or_default() += 1;
                        counts
                    },
                );
                json!({
                    "world": world.world_record()?.title,
                    "recordCount": world.records.len(),
                    "counts": counts,
                    "sourceSchema": crate::source::SOURCE_SCHEMA,
                    "snapshot": true,
                    "defaultTimeline": world.default_timeline(),
                    "currentTime": world.current_time().ok(),
                    "timeModel": {"ticks": "signed ordinal", "intervalEndpoints": "inclusive", "coordinates": "decimal strings"},
                    "searchScope": "author-all-time",
                    "cache": compiler::status(world)?,
                    "validation": self.validation.as_ref(),
                    "limitations": ["Prototype contracts; not a drop-in replacement for Python WEDL", "Each author response uses one immutable source snapshot", "Full domain conformance is not certified; automatic Git commits are not implemented", "Reviewed saved outcomes and edits use recoverable publication; external editors must coordinate with the native writer lock"]
                })
            }
            Operation::Validate => serde_json::to_value(self.validation.as_ref())?,
            Operation::Compile => serde_json::to_value(compiler::compile(world)?)?,
            Operation::EntityList { kind, text } => {
                let query = text.map(|s| s.to_lowercase());
                let mut records: Vec<_> = world
                    .records
                    .values()
                    .filter(|record| {
                        kind.as_ref().is_none_or(|k| k == &record.kind)
                            && query.as_ref().is_none_or(|q| {
                                record.id.to_lowercase().contains(q)
                                    || record.title.to_lowercase().contains(q)
                            })
                    })
                    .collect();
                records.sort_by(|a, b| {
                    (&a.kind, a.title.to_lowercase(), &a.id).cmp(&(
                        &b.kind,
                        b.title.to_lowercase(),
                        &b.id,
                    ))
                });
                json!({"scope":"author-all-time-source", "items": records.iter().map(|record| json!({
                    "id":record.id, "kind":record.kind, "title":record.title,
                    "path":record.source_path, "nonCanonical":record.kind=="hypothesis"
                })).collect::<Vec<_>>()})
            }
            Operation::EntityShow { entity } => {
                let record = world.resolve(&entity)?;
                let raw = std::str::from_utf8(&record.raw_bytes)
                    .map_err(|_| Error::Data("loaded source is not UTF-8".into()))?;
                let yaml_start = raw
                    .find('\n')
                    .ok_or_else(|| Error::Data("loaded source has no envelope".into()))?
                    + 1;
                let prefix = &raw[..raw.len() - record.body.len()];
                let close_len = if prefix.ends_with("---\r\n") { 5 } else { 4 };
                let yaml_end = prefix
                    .len()
                    .checked_sub(close_len)
                    .filter(|end| *end >= yaml_start)
                    .ok_or_else(|| Error::Data("loaded source envelope is inconsistent".into()))?;
                json!({"scope":"author-all-time-source", "id":record.id, "kind":record.kind,
                    "title":record.title, "path":record.source_path, "blobOid":record.blob_oid,
                    "frontmatterYaml":&raw[yaml_start..yaml_end], "rawSource":raw, "bodyMarkdown":record.body,
                    "nonCanonical":record.kind=="hypothesis"})
            }
            Operation::GenerationalFamily {
                character,
                at,
                limit,
            } => generational::family(world, &character, &self.time(&at)?, limit)?,
            Operation::GenerationalLineage {
                character,
                at,
                direction,
                max_depth,
                limit,
            } => generational::lineage(
                world,
                &character,
                &self.time(&at)?,
                direction,
                max_depth,
                limit,
            )?,
            Operation::GenerationalPath {
                from,
                to,
                at,
                max_hops,
                limit,
            } => generational::kinship_path(world, &from, &to, &self.time(&at)?, max_hops, limit)?,
            Operation::GenerationalHistory { record, at, limit } => {
                generational::history(world, &record, &self.time(&at)?, limit)?
            }
            Operation::GenerationalOrganization {
                organization,
                at,
                max_depth,
                include_former,
                limit,
            } => generational::organization(
                world,
                &organization,
                &self.time(&at)?,
                max_depth,
                include_former,
                limit,
            )?,
            Operation::GenerationalLegacy { legacy, at, limit } => {
                generational::legacy(world, &legacy, &self.time(&at)?, limit)?
            }
            Operation::GenerationalOrganizationLegacies {
                organization,
                at,
                limit,
            } => {
                generational::organization_legacies(world, &organization, &self.time(&at)?, limit)?
            }
            Operation::GenerationalVital {
                character,
                at,
                limit,
            } => generational::vital(world, &character, &self.time(&at)?, limit)?,
            Operation::GenerationalLearnedAssertions {
                viewer,
                at,
                kind,
                endpoint,
                held_history,
                limit,
            } => {
                let at = self.time(&at)?;
                let endpoint = endpoint
                    .as_deref()
                    .map(|selector| {
                        generational::discovery::resolve_learned(
                            world, &viewer, selector, &at, None,
                        )
                    })
                    .transpose()?;
                generational::knowledge::assertions(
                    world,
                    &viewer,
                    &at,
                    kind.as_deref(),
                    endpoint.as_deref(),
                    held_history,
                    limit,
                )?
            }
            Operation::GenerationalLearnedFamily {
                viewer,
                character,
                at,
                limit,
            } => {
                let at = self.time(&at)?;
                let character = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &character,
                    &at,
                    Some("character"),
                )?;
                generational::knowledge::family(world, &viewer, &character, &at, limit)?
            }
            Operation::GenerationalLearnedLineage {
                viewer,
                character,
                at,
                direction,
                max_depth,
                limit,
            } => {
                let at = self.time(&at)?;
                let character = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &character,
                    &at,
                    Some("character"),
                )?;
                generational::knowledge::lineage(
                    world, &viewer, &character, &at, direction, max_depth, limit,
                )?
            }
            Operation::GenerationalLearnedPath {
                viewer,
                from,
                to,
                at,
                max_hops,
                limit,
            } => {
                let at = self.time(&at)?;
                let from = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &from,
                    &at,
                    Some("character"),
                )?;
                let to = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &to,
                    &at,
                    Some("character"),
                )?;
                generational::knowledge::kinship_path(
                    world, &viewer, &from, &to, &at, max_hops, limit,
                )?
            }
            Operation::GenerationalLearnedOrganization {
                viewer,
                organization,
                at,
                max_depth,
                limit,
            } => {
                let at = self.time(&at)?;
                let organization = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &organization,
                    &at,
                    Some("organization"),
                )?;
                generational::knowledge::organization(
                    world,
                    &viewer,
                    &organization,
                    &at,
                    max_depth,
                    limit,
                )?
            }
            Operation::GenerationalLearnedLegacy {
                viewer,
                legacy,
                at,
                limit,
            } => {
                let at = self.time(&at)?;
                let legacy = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &legacy,
                    &at,
                    Some("legacy"),
                )?;
                generational::knowledge::legacy(world, &viewer, &legacy, &at, limit)?
            }
            Operation::GenerationalLearnedVital {
                viewer,
                character,
                at,
                limit,
            } => {
                let at = self.time(&at)?;
                let character = generational::discovery::resolve_learned(
                    world,
                    &viewer,
                    &character,
                    &at,
                    Some("character"),
                )?;
                generational::knowledge::vital(world, &viewer, &character, &at, limit)?
            }
            Operation::GenerationalLabels {
                viewer,
                ids,
                at,
                limit,
            } => {
                let at = self.time(&at)?;
                if let Some(viewer) = viewer {
                    generational::discovery::learned_labels(world, &viewer, &ids, &at, limit)?
                } else {
                    generational::discovery::labels(world, &ids, &at, limit)?
                }
            }
            Operation::GenerationalDiscover {
                viewer,
                kind,
                prefix,
                at,
                limit,
            } => {
                let at = self.time(&at)?;
                if let Some(viewer) = viewer {
                    generational::discovery::learned_discover(
                        world, &viewer, &kind, &prefix, &at, limit,
                    )?
                } else {
                    generational::discovery::discover(world, &kind, &prefix, &at, limit)?
                }
            }
            Operation::GenerationalSearch {
                viewer,
                text,
                at,
                limit,
            } => {
                let at = self.time(&at)?;
                if let Some(viewer) = viewer {
                    generational::discovery::learned_search(world, &viewer, &text, &at, limit)?
                } else {
                    generational::discovery::search(world, &text, &at, limit)?
                }
            }
            Operation::GenerationalContext {
                viewer,
                subject,
                at,
                max_depth,
                limit,
                max_chars,
            } => {
                let at = self.time(&at)?;
                if let Some(viewer) = viewer {
                    generational::discovery::learned_context(
                        world, &viewer, &subject, &at, max_depth, limit, max_chars,
                    )?
                } else {
                    generational::discovery::context(
                        world, &subject, &at, max_depth, limit, max_chars,
                    )?
                }
            }
            Operation::DescriptionList {
                owner,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                description::list(world, &owner, at.as_ref(), &audience, &view, limit)?
            }
            Operation::DescriptionSection {
                owner,
                section,
                at,
                audience,
                view,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                description::section(world, &owner, &section, at.as_ref(), &audience, &view)?
            }
            Operation::AssertionPredicates { limit } => assertion::predicates(world, limit)?,
            Operation::AssertionList {
                target,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                assertion::as_of(world, &target, at.as_ref(), &audience, &view, limit)?
            }
            Operation::ExistenceList {
                character,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                existence::list(world, &character, at.as_ref(), &audience, &view, limit)?
            }
            Operation::GovernanceQuery {
                target,
                binding,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                governance::query(
                    world,
                    &target,
                    &binding,
                    at.as_ref(),
                    &audience,
                    &view,
                    limit,
                )?
            }
            Operation::SocialContext {
                selected,
                at,
                audience,
                view,
            } => {
                let at = self.time(&at)?;
                social::context(world, &selected, &at, &audience, &view)?
            }
            Operation::EnvironmentContext {
                roots,
                at,
                audience,
                view,
            } => {
                let at = self.time(&at)?;
                environment::context(world, &roots, &at, &audience, &view)?
            }
            Operation::LoreProfiles { limit } => lore::profiles(world, limit)?,
            Operation::LoreProfile { profile } => lore::profile(world, &profile)?,
            Operation::LoreCatalog {
                at,
                audience,
                view,
                domain,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                lore::catalog(
                    world,
                    at.as_ref(),
                    &audience,
                    &view,
                    domain.as_deref(),
                    limit,
                )?
            }
            Operation::LoreConcept {
                concept,
                at,
                audience,
                view,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                lore::concept(world, &concept, at.as_ref(), &audience, &view)?
            }
            Operation::LoreLinks {
                record,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                lore::links(world, &record, at.as_ref(), &audience, &view, limit)?
            }
            Operation::LoreState {
                record,
                at,
                audience,
                view,
            } => lore_state::state(world, &record, &self.time(&at)?, &audience, &view)?,
            Operation::SpeciesCompose {
                individual,
                selection,
                at,
                audience,
                view,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                species::compose(
                    world,
                    &individual,
                    &selection,
                    at.as_ref(),
                    &audience,
                    &view,
                )?
            }
            Operation::SpeciesAffected {
                common,
                selection,
                at,
                audience,
                view,
                limit,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                species::affected(
                    world,
                    &common,
                    &selection,
                    at.as_ref(),
                    &audience,
                    &view,
                    limit,
                )?
            }
            Operation::MagicCompose {
                root,
                selection,
                at,
                audience,
                view,
            } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                magic::compose(world, &root, &selection, at.as_ref(), &audience, &view)?
            }
            Operation::SpatialLocation {
                entity,
                mode,
                limit,
            } => spatial::location(world, &entity, mode.as_deref(), limit)?,
            Operation::SpatialHistory { record, at, limit } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                spatial_history::history(world, &record, at.as_ref(), limit)?
            }
            Operation::SpatialRoute { route, at } => {
                let at = at.as_ref().map(|at| self.time(at)).transpose()?;
                spatial::route_info(world, &route, at.as_ref())?
            }
            Operation::SpatialPath {
                from,
                to,
                mode,
                metric,
                unit,
                limit,
            } => spatial::path(
                world,
                &from,
                &to,
                mode.as_deref(),
                &metric,
                unit.as_deref(),
                limit,
            )?,
            Operation::SpatialMaps { limit } => geometry::maps(world, limit)?,
            Operation::SpatialAnchors {
                from_map,
                to_map,
                limit,
            } => geometry::anchors(world, from_map.as_deref(), to_map.as_deref(), limit)?,
            Operation::SpatialReachable { from, mode, limit } => {
                spatial::reachable(world, &from, mode.as_deref(), limit)?
            }
            Operation::SpatialOverlays {
                location,
                at,
                audience,
                view,
                overlay,
                limit,
            } => spatial::overlays(
                world,
                &location,
                &self.time(&at)?,
                &audience,
                &view,
                overlay.as_deref(),
                limit,
            )?,
            Operation::SpatialBbox {
                map,
                min,
                max,
                relation,
                limit,
            } => geometry::bbox_relation(world, &map, &min, &max, &relation, limit)?,
            Operation::SpatialNearby {
                map,
                point,
                radius,
                limit,
            } => geometry::nearby(world, &map, point, radius, limit)?,
            Operation::SpatialBboxAt {
                map,
                min,
                max,
                relation,
                at,
                limit,
            } => geometry::bbox_at(world, &map, &min, &max, &relation, &self.time(&at)?, limit)?,
            Operation::SpatialNearbyAt {
                map,
                point,
                radius,
                at,
                limit,
            } => geometry::nearby_at(world, &map, &point, radius, &self.time(&at)?, limit)?,
            Operation::ChronologyCalendars { limit } => chronology::calendars(world, limit)?,
            Operation::ChronologyDate {
                calendar,
                value,
                era,
            } => chronology::date(world, &calendar, value, era.as_deref())?,
            Operation::ChronologyConvert { from, to, value } => {
                chronology::convert(world, &from, &to, value)?
            }
            Operation::ChronologyToStory {
                calendar,
                value,
                timeline,
            } => chronology::to_story(world, &calendar, value, timeline.as_deref())?,
            Operation::ChronologyFromStory { calendar, at } => {
                chronology::from_story(world, &calendar, &self.time(&at)?)?
            }
            Operation::ChronologyAnnotations {
                record,
                calendar,
                era,
                limit,
            } => chronology::annotations(
                world,
                &chronology::AnnotationFilter {
                    record,
                    calendar,
                    era,
                    limit,
                },
            )?,
            Operation::ChronologyCompare {
                left_record,
                left_id,
                right_record,
                right_id,
            } => chronology::compare_annotations(
                world,
                &left_record,
                &left_id,
                &right_record,
                &right_id,
            )?,
            Operation::ChronologyFormat { value } => chronology::format_date(world, &value)?,
            Operation::ChronologyConvertDate { value, target } => {
                chronology::convert_date(world, &value, &target)?
            }
            Operation::ChronologySearch { query } => chronology::search_annotations(world, &query)?,
            Operation::ChronologyStoryTimes {
                value,
                upper,
                timeline,
                limit,
            } => {
                chronology::story_times(world, &value, upper.as_ref(), timeline.as_deref(), limit)?
            }
            Operation::Scene { entity, at } => scene::query(world, &entity, &self.time(&at)?)?,
            Operation::CharacterContext {
                character,
                at,
                max_characters,
            } => {
                if !(256..=100_000).contains(&max_characters) {
                    return Err(Error::InvalidInput(
                        "maxCharacters must be between 256 and 100000".into(),
                    ));
                }
                character::context(world, &character, &self.time(&at)?, max_characters)?
            }
            Operation::CharacterBrief {
                character,
                brief,
                at,
                limit,
            } => briefs::character(world, &character, &brief, &self.time(&at)?, limit)?,
            Operation::CharacterSearch {
                character,
                query,
                at,
                limit,
            } => {
                if query.trim().is_empty() || query.len() > 4096 {
                    return Err(Error::InvalidInput(
                        "q must contain 1 to 4096 bytes of nonblank search text".into(),
                    ));
                }
                if !(1..=100).contains(&limit) {
                    return Err(Error::InvalidInput(
                        "limit must be between 1 and 100".into(),
                    ));
                }
                character::search(world, &character, &query, &self.time(&at)?, limit)?
            }
            Operation::CharacterSearchWithMode {
                character,
                query,
                at,
                limit,
                mode,
            } => {
                if query.trim().is_empty() || query.len() > 4096 {
                    return Err(Error::InvalidInput(
                        "q must contain 1 to 4096 bytes of nonblank search text".into(),
                    ));
                }
                if !(1..=100).contains(&limit) {
                    return Err(Error::InvalidInput(
                        "limit must be between 1 and 100".into(),
                    ));
                }
                character::search_with_mode(
                    world,
                    &character,
                    &query,
                    &self.time(&at)?,
                    limit,
                    mode,
                )?
            }
            Operation::CharacterConversation {
                character,
                conversation,
                at,
            } => character::conversation(world, &character, &conversation, &self.time(&at)?)?,
            Operation::CharacterAssertions {
                character,
                target,
                at,
                limit,
            } => learned_assertions::list(world, &character, &target, &self.time(&at)?, limit)?,
            Operation::CharacterDescriptions {
                character,
                owner,
                at,
                limit,
            } => narrative::descriptions(world, &character, &owner, &self.time(&at)?, limit)?,
            Operation::CharacterSection {
                character,
                owner,
                section,
                at,
            } => narrative::section(world, &character, &owner, &section, &self.time(&at)?)?,
            Operation::CharacterLore {
                character,
                domain,
                at,
                limit,
            } => narrative::lore_catalog(
                world,
                &character,
                domain.as_deref(),
                &self.time(&at)?,
                limit,
            )?,
            Operation::CharacterConcept {
                character,
                target,
                at,
            } => narrative::lore_concept(world, &character, &target, &self.time(&at)?)?,
            Operation::CharacterLinks {
                character,
                owner,
                at,
                limit,
            } => narrative::lore_links(world, &character, &owner, &self.time(&at)?, limit)?,
            Operation::CharacterSpatialHistory {
                character,
                owner,
                at,
                limit,
            } => narrative::spatial_history(world, &character, &owner, &self.time(&at)?, limit)?,
            Operation::CharacterExistence {
                character,
                target,
                at,
                limit,
            } => narrative::existence(world, &character, &target, &self.time(&at)?, limit)?,
            Operation::CharacterGovernance {
                character,
                target,
                at,
                limit,
            } => narrative::governance(world, &character, &target, &self.time(&at)?, limit)?,
            Operation::State { entity, at } => runtime::state(world, &entity, &self.time(&at)?)?,
            Operation::Knowledge {
                entity,
                at,
                include_forgotten,
            } => runtime::knowledge(world, &entity, &self.time(&at)?, include_forgotten)?,
            Operation::Relationships { entity, at } => {
                runtime::relationships(world, &entity, &self.time(&at)?)?
            }
            Operation::StoryPoints { at, scene, limit } => {
                navigation::story::story_points(world, &self.time(&at)?, scene.as_deref(), limit)?
            }
            Operation::StoryPointTrail {
                story_point,
                at,
                scene,
                limit,
            } => navigation::story::story_point_trail(
                world,
                &story_point,
                &self.time(&at)?,
                scene.as_deref(),
                limit,
            )?,
            Operation::Causal {
                event,
                at,
                direction,
                limit,
            } => navigation::causal::causal(world, &event, &self.time(&at)?, &direction, limit)?,
            Operation::Whereabouts {
                at,
                characters,
                limit,
            } => navigation::whereabouts::whereabouts(
                world,
                &self.time(&at)?,
                characters.as_deref(),
                limit,
            )?,
            Operation::Interactions {
                first,
                second,
                at,
                limit,
            } => navigation::relations::interactions(
                world,
                &first,
                &second,
                &self.time(&at)?,
                limit,
            )?,
            Operation::ThreadMemberships {
                record_ids,
                thread_ids,
                at,
                limit,
            } => {
                let at = at.map(|input| self.time(&input)).transpose()?;
                navigation::relations::thread_memberships(
                    world,
                    &record_ids,
                    &thread_ids,
                    at.as_ref(),
                    limit,
                )?
            }
            Operation::Environments { entity, at } => {
                story::environments(world, &entity, &self.time(&at)?)?
            }
            Operation::Timeline {
                timeline,
                at,
                limit,
            } => {
                let timeline = timeline.unwrap_or_else(|| world.default_timeline());
                world.story_time(0, Some(&timeline), 0)?;
                let at = at.map(|input| self.time(&input)).transpose()?;
                navigation::timeline::timeline(world, &timeline, at.as_ref(), limit)?
            }
            Operation::Conversation { entity, at } => {
                runtime::conversation(world, &entity, &self.time(&at)?)?
            }
            Operation::Context {
                entity,
                at,
                max_characters,
            } => {
                if !(256..=100_000).contains(&max_characters) {
                    return Err(Error::InvalidInput(
                        "maxCharacters must be between 256 and 100000".into(),
                    ));
                }
                runtime::context(world, &entity, &self.time(&at)?, max_characters)?
            }
            Operation::Search { query, limit, kind } => {
                if query.trim().is_empty() || query.len() > 4096 {
                    return Err(Error::InvalidInput(
                        "q must contain 1 to 4096 bytes of nonblank search text".into(),
                    ));
                }
                if !(1..=100).contains(&limit) {
                    return Err(Error::InvalidInput(
                        "limit must be between 1 and 100".into(),
                    ));
                }
                json!({"scope":"author-all-time", "matches":compiler::search(world, &query, limit, kind.as_deref())?})
            }
            Operation::SearchWithMode {
                query,
                limit,
                kind,
                mode,
            } => {
                if query.trim().is_empty() || query.len() > 4096 {
                    return Err(Error::InvalidInput(
                        "q must contain 1 to 4096 bytes of nonblank search text".into(),
                    ));
                }
                if !(1..=100).contains(&limit) {
                    return Err(Error::InvalidInput(
                        "limit must be between 1 and 100".into(),
                    ));
                }
                json!({"scope":"author-all-time", "matches":compiler::search_with_mode(world, &query, limit, kind.as_deref(), mode)?})
            }
            Operation::SearchWithSelection {
                query,
                limit,
                kind,
                mode,
                at,
                thread_ids,
            } => {
                let selection = compiler::AuthorSearchSelection {
                    at: at.as_ref().map(|at| self.time(at)).transpose()?,
                    thread_ids,
                };
                let matches = compiler::search_with_selection(
                    world,
                    &query,
                    limit,
                    kind.as_deref(),
                    mode,
                    &selection,
                )?;
                json!({"scope":matches["scope"],"matches":matches})
            }
            Operation::Threads => {
                let value = world
                    .world_record()?
                    .get("threads")
                    .cloned()
                    .unwrap_or(serde_yaml::Value::Sequence(Vec::new()));
                json!({"scope":"authored-grouping-labels", "items":yaml_json(&value)?})
            }
            Operation::SearchWithHypotheses {
                query,
                limit,
                kind,
                mode,
                at,
                thread_ids,
            } => {
                let selection = compiler::AuthorSearchSelection {
                    at: at.as_ref().map(|at| self.time(at)).transpose()?,
                    thread_ids,
                };
                compiler::search_with_hypotheses(
                    world,
                    &query,
                    limit,
                    kind.as_deref(),
                    mode,
                    &selection,
                )?
            }
            Operation::PreviewBody {
                entity,
                body,
                expected_fingerprint,
            } => authoring::preview_body_edit(world, &entity, &body, &expected_fingerprint)?,
            Operation::ApplyBody {
                entity,
                body,
                expected_fingerprint,
                expected_after_sha256,
            } => authoring::apply_body_edit(
                world,
                &entity,
                &body,
                &expected_fingerprint,
                &expected_after_sha256,
            )?,
            Operation::PreviewRecordCreate {
                draft,
                expected_fingerprint,
            } => authoring::preview_record_create(world, &draft, &expected_fingerprint)?,
            Operation::ApplyRecordCreate {
                draft,
                expected_fingerprint,
                expected_after_sha256,
            } => authoring::apply_record_create(
                world,
                &draft,
                &expected_fingerprint,
                &expected_after_sha256,
            )?,
            Operation::PreviewRecordUpdate {
                entity,
                patch,
                expected_fingerprint,
            } => authoring::preview_record_update(world, &entity, &patch, &expected_fingerprint)?,
            Operation::ApplyRecordUpdate {
                entity,
                patch,
                expected_fingerprint,
                expected_after_sha256,
            } => authoring::apply_record_update(
                world,
                &entity,
                &patch,
                &expected_fingerprint,
                &expected_after_sha256,
            )?,
        };
        if character_view || independent_source {
            Ok(json!({"protocol":PROTOCOL, "result":result}))
        } else {
            Ok(
                json!({"protocol":PROTOCOL, "fingerprint":world.fingerprint, "revision":world.revision, "result":result}),
            )
        }
    }
}

/// Source integers are emitted as strings so a browser cannot round story coordinates.
/// This is a deliberately explicit author-source projection, not source round-tripping.
pub fn yaml_json(value: &serde_yaml::Value) -> Result<Value> {
    Ok(match value {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(value) => Value::Bool(*value),
        serde_yaml::Value::Number(value) if value.is_i64() || value.is_u64() => {
            Value::String(value.to_string())
        }
        serde_yaml::Value::Number(value) => serde_json::to_value(value)?,
        serde_yaml::Value::String(value) => Value::String(value.clone()),
        serde_yaml::Value::Sequence(values) => {
            Value::Array(values.iter().map(yaml_json).collect::<Result<Vec<_>>>()?)
        }
        serde_yaml::Value::Mapping(values) => {
            let mut out = serde_json::Map::new();
            for (key, value) in values {
                let Some(key) = key.as_str() else {
                    return Err(Error::Validation(
                        "source mapping keys must be strings".into(),
                    ));
                };
                out.insert(key.into(), yaml_json(value)?);
            }
            Value::Object(out)
        }
        serde_yaml::Value::Tagged(_) => {
            return Err(Error::Unsupported(
                "tagged YAML is not an HTTP source value".into(),
            ));
        }
    })
}
