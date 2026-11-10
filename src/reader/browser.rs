//! Closed, policy-bound presentation for the isolated Reader UI.
use super::*;
use std::collections::BTreeMap;

const BROWSER_PROTOCOL: &str = "wedl-reader-browser/v1";
const PAGE_SIZE: usize = 20;
const DEFAULT_EDITION: &str = "Published edition";
const DEFAULT_POSITION: &str = "Reading position";

/// Optional names explicitly published by the operator for this policy's
/// committed work and reveal plan. An unnamed occurrence remains navigable.
#[derive(Clone, Default, Serialize)]
pub struct PublicLabels {
    pub edition: Option<String>,
    pub positions: BTreeMap<usize, String>,
}

impl PublicLabels {
    pub(crate) fn validate(&self) -> Result<()> {
        let label = |value: &str| {
            text(value, 120) && value.len() <= 480 && !value.chars().any(char::is_control)
        };
        if self.edition.as_deref().is_some_and(|value| !label(value))
            || self.positions.len() > MAX_OCCURRENCES
            || self
                .positions
                .iter()
                .any(|(&position, value)| position >= MAX_OCCURRENCES || !label(value))
        {
            return Err(invalid_policy());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadingInput {
    edition: Option<String>,
    position: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    cursor: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    query: Option<String>,
}

pub(super) enum BrowserRequest {
    Choices,
    Reading {
        operation: Operation,
        edition: Option<[u8; 32]>,
        position: Option<[u8; 32]>,
        cursor: Option<Cursor>,
        query: Option<String>,
    },
}

fn selector(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut mac = [0; 32];
    hex::decode_to_slice(value, &mut mac).ok()?;
    Some(mac)
}

impl BrowserRequest {
    pub(super) fn reading(raw: &[u8], operation: Operation) -> Result<Self> {
        if raw.len() > MAX_REQUEST_BYTES {
            return Err(invalid_request());
        }
        let input: ReadingInput = serde_json::from_slice(raw).map_err(|_| invalid_request())?;
        if (operation == Operation::Search) != input.query.is_some()
            || input
                .query
                .as_ref()
                .is_some_and(|query| !text(query, 256) || query.len() > 1024)
        {
            return Err(invalid_request());
        }
        Ok(Self::Reading {
            operation,
            edition: input.edition.as_deref().and_then(selector),
            position: input.position.as_deref().and_then(selector),
            cursor: decode_cursor(input.cursor)?,
            query: input.query,
        })
    }
}

struct Selectors<'a> {
    policy: &'a PublicPolicy,
    labels: &'a PublicLabels,
    scope: [u8; 32],
}

impl<'a> Selectors<'a> {
    fn new(
        policy: &'a PublicPolicy,
        labels: &'a PublicLabels,
    ) -> std::result::Result<Self, ProjectionError> {
        let binding = json!({"repository":policy.repository,"revision":policy.revision,
            "work":policy.work_id,"plan":policy.plan_id,"scope":policy.canonical_scope,
            "generation":policy.generation,"ceiling":policy.max_occurrence,"publication":policy.publication_id,
            "horizon":policy.at,"textPolicy":policy.text_policy,"labels":labels});
        let scope = Sha256::digest(bounded_bytes(&binding, MAX_RESPONSE_BYTES)?).into();
        Ok(Self {
            policy,
            labels,
            scope,
        })
    }

    fn mac(&self, occurrence: Option<usize>) -> [u8; 32] {
        let mut context = Vec::with_capacity(BROWSER_PROTOCOL.len() + 35);
        context.extend_from_slice(BROWSER_PROTOCOL.as_bytes());
        context.extend_from_slice(&self.scope);
        if let Some(occurrence) = occurrence {
            context.extend_from_slice(&(occurrence as u16).to_be_bytes());
        }
        hmac_sha256(&self.policy.cursor_secret, &context)
    }

    fn occurrence(&self, edition: Option<[u8; 32]>, position: Option<[u8; 32]>) -> Option<usize> {
        let edition = edition?;
        let position = position?;
        if !equal_mac(&edition, &self.mac(None)) {
            return None;
        }
        let mut occurrence = None;
        for index in 0..MAX_OCCURRENCES {
            if equal_mac(&position, &self.mac(Some(index))) {
                occurrence = Some(index);
            }
        }
        occurrence.filter(|&index| index <= self.policy.max_occurrence)
    }

    fn position(&self, occurrence: usize, fallback: &'static str) -> Choice<'_> {
        Choice {
            selector: hex::encode(self.mac(Some(occurrence))),
            label: self
                .labels
                .positions
                .get(&occurrence)
                .map_or(fallback, String::as_str),
        }
    }

    fn choices(&self, count: usize) -> std::result::Result<Json, ProjectionError> {
        if count == 0 {
            return Err(ProjectionError::Unavailable);
        }
        available(Choices {
            edition: Choice {
                selector: hex::encode(self.mac(None)),
                label: self.labels.edition.as_deref().unwrap_or(DEFAULT_EDITION),
            },
            entry: self.position(0, "Start reading"),
            positions: self
                .labels
                .positions
                .range(..count)
                .map(|(&index, _)| self.position(index, DEFAULT_POSITION))
                .collect(),
        })
    }
}

#[derive(Serialize)]
struct Choice<'a> {
    selector: String,
    label: &'a str,
}

#[derive(Serialize)]
struct Choices<'a> {
    edition: Choice<'a>,
    entry: Choice<'a>,
    positions: Vec<Choice<'a>>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum PublishedKind {
    Record,
    Section,
    Text,
}

#[derive(Deserialize, Serialize)]
struct PublishedItem {
    kind: PublishedKind,
    title: Option<String>,
    markdown: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedItems {
    items: Vec<PublishedItem>,
    count: usize,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
struct Reading<'a> {
    position: Choice<'a>,
    previous: Option<Choice<'a>>,
    next: Option<Choice<'a>>,
    #[serde(flatten)]
    items: PublishedItems,
}

#[derive(Serialize)]
struct Available<T> {
    protocol: &'static str,
    outcome: &'static str,
    #[serde(flatten)]
    payload: T,
}

fn available<T: Serialize>(payload: T) -> std::result::Result<Json, ProjectionError> {
    let response = Available {
        protocol: BROWSER_PROTOCOL,
        outcome: "available",
        payload,
    };
    let bytes = bounded_bytes(&response, MAX_RESPONSE_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|_| ProjectionError::Invalid)
}

impl PresentationService {
    pub(super) fn prepare_browser(&self, request: BrowserRequest) -> Prepared {
        self.prepare_browser_using(|policy, labels| self.project_browser(request, policy, labels))
    }

    fn prepare_browser_using(
        &self,
        project: impl FnOnce(
            &CapturedPolicy,
            &PublicLabels,
        ) -> std::result::Result<Json, ProjectionError>,
    ) -> Prepared {
        let captured = self.0.installed.read().ok().and_then(|installed| {
            Some((
                CapturedPolicy::Public(installed.public.clone()?),
                installed.public_labels.clone(),
            ))
        });
        let unavailable = || failure(Mode::Reader, Failure::Unavailable);
        match captured {
            Some((policy, labels)) => Prepared {
                mode: Mode::Reader,
                value: project(&policy, &labels).unwrap_or_else(|_| unavailable()),
                policy: Some(policy),
            },
            None => Prepared {
                mode: Mode::Reader,
                policy: None,
                value: unavailable(),
            },
        }
    }

    fn project_browser(
        &self,
        request: BrowserRequest,
        captured: &CapturedPolicy,
        labels: &PublicLabels,
    ) -> std::result::Result<Json, ProjectionError> {
        let CapturedPolicy::Public(policy) = captured else {
            return Err(ProjectionError::Unavailable);
        };
        let selectors = Selectors::new(policy, labels)?;
        let selection = match request {
            BrowserRequest::Choices => None,
            BrowserRequest::Reading {
                operation,
                edition,
                position,
                cursor,
                query,
            } => {
                let occurrence = selectors
                    .occurrence(edition, position)
                    .ok_or(ProjectionError::Unavailable)?;
                Some(Selection {
                    revision: policy.revision.clone(),
                    publication_id: Some(policy.publication_id.clone()),
                    work_id: None,
                    plan_id: None,
                    occurrence,
                    operation,
                    at: None,
                    text_policy: None,
                    limit: PAGE_SIZE,
                    cursor,
                    query,
                })
            }
        };
        let root = self.0.root.as_deref().ok_or(ProjectionError::Unavailable)?;
        if repository_identity(root).ok().as_ref() != Some(&policy.repository) {
            return Err(ProjectionError::Unavailable);
        }
        let result = (|| {
            let source = ReaderSource::load(
                root,
                &captured.binding(),
                selection.as_ref().map(|request| request.occurrence),
            )?;
            Self::present_browser(&source, captured, &selectors, selection.as_ref())
        })();
        if repository_identity(root).ok().as_ref() != Some(&policy.repository) {
            return Err(ProjectionError::Unavailable);
        }
        result
    }

    fn present_browser(
        source: &ReaderSource,
        captured: &CapturedPolicy,
        selectors: &Selectors<'_>,
        selection: Option<&Selection>,
    ) -> std::result::Result<Json, ProjectionError> {
        let count = source
            .plan
            .occurrences
            .len()
            .min(selectors.policy.max_occurrence + 1);
        let Some(request) = selection else {
            check_horizon(&source.world, &selectors.policy.at)?;
            return selectors.choices(count);
        };
        let occurrence = request.occurrence;
        if occurrence >= count {
            return Err(ProjectionError::Unavailable);
        }
        let projected = Self::project_loaded(
            source,
            request,
            captured,
            Mode::Reader,
            Some(&selectors.scope),
        )?;
        let items = serde_json::from_value(projected).map_err(|_| ProjectionError::Invalid)?;
        available(Reading {
            position: selectors.position(occurrence, DEFAULT_POSITION),
            previous: occurrence
                .checked_sub(1)
                .map(|index| selectors.position(index, "Previous position")),
            next: (occurrence + 1 < count)
                .then(|| selectors.position(occurrence + 1, "Next position")),
            items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manuscript::{Admission, Milestone, Occurrence, RevealEntry, RevealPlan};

    const WORK: &str = "work_00000000000000000000000001";
    const PLAN: &str = "reveal_00000000000000000000000001";

    fn fixture() -> (tempfile::TempDir, ReaderSource, PublicPolicy, PublicLabels) {
        let root = tempfile::tempdir().unwrap();
        let mut world =
            crate::source::init_world(&root.path().join("world"), "Private world").unwrap();
        let revision = "a".repeat(40);
        world.revision = Some(revision.clone());
        let policy = PublicPolicy {
            repository: RepositoryIdentity([
                "private root".into(),
                "private Git".into(),
                "private common".into(),
            ]),
            revision,
            work_id: WORK.into(),
            plan_id: PLAN.into(),
            canonical_scope: CanonicalScope {
                world_id: world.world_record().unwrap().id.clone(),
                records: Default::default(),
                sections: None,
            },
            generation: "private generation".into(),
            max_occurrence: 1,
            cursor_secret: [17; 32],
            publication_id: "private-publication".into(),
            at: world.current_time().unwrap(),
            text_policy: TextPolicy::AllAtPinnedPlan,
        };
        let entry = |ordinal, admissions| RevealEntry {
            milestone: Milestone {
                chapter_id: "private chapter".into(),
                segment_index: ordinal,
            },
            ordinal,
            admissions,
        };
        let text = |id: usize, markdown: String| Admission::Text {
            id: format!("revealtext_{id:026}"),
            markdown,
        };
        let plan = RevealPlan {
            plan_id: PLAN.into(),
            work_id: WORK.into(),
            occurrences: (0..3)
                .map(|ordinal| Occurrence {
                    chapter_id: "private chapter".into(),
                    segment_index: ordinal,
                    ordinal,
                    segment_id: "private segment".into(),
                })
                .collect(),
            entries: vec![
                entry(
                    0,
                    (0..21)
                        .map(|index| text(index, format!("Admitted quay text {index}")))
                        .collect(),
                ),
                entry(2, vec![text(99, "WITHHELD REVEAL".into())]),
            ],
        };
        let labels = PublicLabels {
            edition: Some("Public edition name".into()),
            positions: [
                (0, "Arrival at the Quay".into()),
                (2, "WITHHELD LABEL".into()),
                (9, "MISSING LABEL".into()),
            ]
            .into(),
        };
        let source = ReaderSource {
            world,
            plan,
            occurrence_count: 3,
            work_blob: Some("private work blob".into()),
            plan_blob: Some("private plan blob".into()),
        };
        (root, source, policy, labels)
    }

    fn request(policy: &PublicPolicy, occurrence: usize) -> Selection {
        Selection {
            revision: policy.revision.clone(),
            publication_id: Some(policy.publication_id.clone()),
            work_id: None,
            plan_id: None,
            occurrence,
            operation: Operation::Page,
            at: None,
            text_policy: None,
            limit: PAGE_SIZE,
            cursor: None,
            query: None,
        }
    }

    #[test]
    fn named_choices_and_pages_exclude_unpublished_labels_reveals_and_source_fields() {
        let (_root, mut source, policy, labels) = fixture();
        let captured = CapturedPolicy::Public(Arc::new(policy.clone()));
        let selectors = Selectors::new(&policy, &labels).unwrap();
        let choices =
            PresentationService::present_browser(&source, &captured, &selectors, None).unwrap();
        assert_eq!(choices["edition"]["label"], "Public edition name");
        assert_eq!(choices["positions"].as_array().unwrap().len(), 1);
        assert_eq!(choices["positions"][0], choices["entry"]);
        assert_eq!(choices["entry"]["label"], "Arrival at the Quay");
        let first = request(&policy, 0);
        let page =
            PresentationService::present_browser(&source, &captured, &selectors, Some(&first))
                .unwrap();
        assert_eq!(page["position"], choices["entry"]);
        assert_eq!(page["previous"], Json::Null);
        assert_eq!(page["next"]["label"], "Next position");
        assert_eq!(page["count"], 21);
        assert_eq!(page["items"].as_array().unwrap().len(), 20);
        let edition = selector(choices["edition"]["selector"].as_str().unwrap());
        let next = selector(page["next"]["selector"].as_str().unwrap());
        assert_eq!(selectors.occurrence(edition, next), Some(1));
        assert_eq!(
            selectors.occurrence(edition, Some(selectors.mac(Some(2)))),
            None
        );
        assert_eq!(
            selectors.occurrence(edition, Some(selectors.mac(Some(9)))),
            None
        );
        assert_eq!(selectors.occurrence(Some([0; 32]), next), None);
        let mut second = request(&policy, 1);
        second.operation = Operation::Search;
        second.query = Some("withheld".into());
        let found =
            PresentationService::present_browser(&source, &captured, &selectors, Some(&second))
                .unwrap();
        assert_eq!(found["count"], 0);
        assert_eq!(found["next"], Json::Null);
        for value in [&choices, &page, &found] {
            let raw = value.to_string();
            for forbidden in [
                "WITHHELD",
                "MISSING",
                "private",
                "occurrence",
                "revision",
                "ownerId",
                "revealtext_",
                WORK,
                PLAN,
            ] {
                assert!(!raw.contains(forbidden), "leaked {forbidden}");
            }
        }
        source.plan.entries.pop();
        assert_eq!(
            choices,
            PresentationService::present_browser(&source, &captured, &selectors, None).unwrap()
        );
        assert_eq!(
            page,
            PresentationService::present_browser(&source, &captured, &selectors, Some(&first))
                .unwrap()
        );
    }

    #[test]
    fn position_handles_and_continuations_are_bound_to_browser_policy_and_operation() {
        let (_root, source, policy, labels) = fixture();
        let captured = CapturedPolicy::Public(Arc::new(policy.clone()));
        let selectors = Selectors::new(&policy, &labels).unwrap();
        let first = request(&policy, 0);
        let page =
            PresentationService::present_browser(&source, &captured, &selectors, Some(&first))
                .unwrap();
        let raw_cursor = page["nextCursor"].as_str().unwrap().to_owned();
        let mut next = request(&policy, 0);
        next.cursor = decode_cursor(Some(raw_cursor.clone())).unwrap();
        let last =
            PresentationService::present_browser(&source, &captured, &selectors, Some(&next))
                .unwrap();
        assert_eq!(last["items"].as_array().unwrap().len(), 1);
        assert_eq!(last["nextCursor"], Json::Null);
        assert!(matches!(
            PresentationService::project_loaded(&source, &next, &captured, Mode::Reader, None),
            Err(ProjectionError::Unavailable)
        ));
        next.operation = Operation::Search;
        next.query = Some("Admitted".into());
        assert!(matches!(
            PresentationService::present_browser(&source, &captured, &selectors, Some(&next)),
            Err(ProjectionError::Unavailable)
        ));
        let mut changed_labels = labels.clone();
        changed_labels.edition = Some("New edition".into());
        let changed = Selectors::new(&policy, &changed_labels).unwrap();
        assert_eq!(
            changed.occurrence(Some(selectors.mac(None)), Some(selectors.mac(Some(0)))),
            None
        );
        assert_eq!(
            changed.occurrence(Some(changed.mac(None)), Some(selectors.mac(Some(0)))),
            None
        );
        let without_names = PublicLabels::default();
        let defaults = Selectors::new(&policy, &without_names)
            .unwrap()
            .choices(2)
            .unwrap();
        assert_eq!(defaults["edition"]["label"], DEFAULT_EDITION);
        assert_eq!(defaults["entry"]["label"], "Start reading");
        assert_eq!(defaults["positions"], json!([]));
    }

    #[test]
    fn replacing_or_revoking_public_policy_suppresses_prepared_browser_responses() {
        let (_root, source, policy, labels) = fixture();
        let service = PresentationService::default();
        let prepare = |selection: Option<&Selection>| {
            service.prepare_browser_using(|captured, labels| {
                let CapturedPolicy::Public(policy) = captured else {
                    panic!("public policy only")
                };
                let selectors = Selectors::new(policy, labels)?;
                PresentationService::present_browser(&source, captured, &selectors, selection)
            })
        };
        let mut installed = service.0.installed.write().unwrap();
        installed.public = Some(Arc::new(policy.clone()));
        installed.public_labels = Arc::new(labels);
        drop(installed);
        assert_eq!(service.emit(prepare(None))["outcome"], "available");
        let old_choices = prepare(None);
        let first = request(&policy, 0);
        let old_read = prepare(Some(&first));
        service.0.installed.write().unwrap().public = Some(Arc::new(policy));
        let unavailable = failure(Mode::Reader, Failure::Unavailable);
        assert_eq!(service.emit(old_choices), unavailable);
        assert_eq!(service.emit(old_read), unavailable);
        let prepared = prepare(None);
        service.install_public(None).unwrap();
        assert_eq!(service.emit(prepared), unavailable);
        assert_eq!(
            service.emit(
                service.prepare_browser_using(|_, _| panic!("no policy must not access source"))
            ),
            unavailable
        );
    }
}
