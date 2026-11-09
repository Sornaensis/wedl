use std::{io::Read, path::PathBuf, process::ExitCode};

use clap::{Args, CommandFactory, Parser, Subcommand};
use wedl::{
    Error, Result,
    chronology::{AnnotationQuery, CivilDate, DateInput, DatePredicate, DateTarget, EraMatchMode},
    generational::LineageDirection,
    http,
    service::{Operation, Service, TimeInput},
    source,
};

mod completion;

#[derive(Parser)]
#[command(
    name = "wedl-rs",
    version,
    about = "Rust WEDL: native source, temporal reads, character views and a local author browser",
    long_about = "Breadth-first Rust implementation of native WEDL source. JSON output uses the Rust implementation's v1 contract. Source inspection and author search span all story time. Character commands use exact IDs and an explicit story tick."
)]
struct Cli {
    #[arg(
        long,
        global = true,
        default_value = ".",
        value_name = "PATH",
        help = "World root (containing story/)"
    )]
    repo: PathBuf,
    #[arg(
        long,
        global = true,
        value_name = "COMMIT",
        help = "Read an immutable Git commit instead of the working source tree"
    )]
    revision: Option<String>,
    #[arg(long, global = true, help = "Print JSON on one line")]
    compact: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct TimeArgs {
    #[arg(
        long,
        allow_hyphen_values = true,
        help = "Signed ordinal story tick (not elapsed duration)"
    )]
    tick: i64,
    #[arg(long, allow_hyphen_values=true, default_value_t=i32::MAX, help="Order within the tick; defaults to the end of the tick")]
    order: i32,
    #[arg(long)]
    timeline: Option<String>,
}
impl From<TimeArgs> for TimeInput {
    fn from(time: TimeArgs) -> Self {
        Self {
            tick: time.tick,
            order: time.order,
            timeline: time.timeline,
        }
    }
}

#[derive(Args)]
struct OptionalTimeArgs {
    #[arg(
        long,
        allow_hyphen_values = true,
        help = "Select an exact historical horizon; omit for the static/all-time read"
    )]
    tick: Option<i64>,
    #[arg(
        long,
        allow_hyphen_values = true,
        requires = "tick",
        help = "Order within the tick; defaults to the end of the tick"
    )]
    order: Option<i32>,
    #[arg(long, requires = "tick")]
    timeline: Option<String>,
}
impl OptionalTimeArgs {
    fn input(self) -> Option<TimeInput> {
        self.tick.map(|tick| TimeInput {
            tick,
            order: self.order.unwrap_or(i32::MAX),
            timeline: self.timeline,
        })
    }
}

fn signed_date_integer(value: &str) -> std::result::Result<i64, String> {
    value
        .parse::<i64>()
        .ok()
        .filter(|number| number.to_string() == value)
        .ok_or_else(|| "use a canonical signed 64-bit decimal integer".into())
}

fn date_component(value: &str) -> std::result::Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|number| number.to_string() == value && *number > 0)
        .ok_or_else(|| "use a positive decimal integer without leading zeroes".into())
}

fn anchor_order(value: &str) -> std::result::Result<i32, String> {
    value
        .parse::<i32>()
        .ok()
        .filter(|number| number.to_string() == value)
        .ok_or_else(|| "use a canonical signed 32-bit decimal integer".into())
}

#[derive(Args)]
struct DateArgs {
    #[arg(long, allow_hyphen_values = true, value_parser = signed_date_integer)]
    year: i64,
    #[arg(long, value_parser = date_component)]
    month: u32,
    #[arg(long, value_parser = date_component)]
    day: u32,
}

impl From<DateArgs> for CivilDate {
    fn from(value: DateArgs) -> Self {
        Self {
            year: value.year,
            month: value.month,
            day: value.day,
        }
    }
}

#[derive(Args)]
struct DateInputArgs {
    #[arg(long, conflicts_with_all = ["calendar", "era", "year", "month", "day"],
        help = "Advanced tagged date JSON with decimal-string coordinates")]
    value: Option<String>,
    #[arg(long, conflicts_with = "era", value_name = "CALENDAR_ID")]
    calendar: Option<String>,
    #[arg(long, value_name = "ERA_ID")]
    era: Option<String>,
    #[arg(long, allow_hyphen_values = true, value_parser = signed_date_integer)]
    year: Option<i64>,
    #[arg(long, value_parser = date_component)]
    month: Option<u32>,
    #[arg(long, value_parser = date_component, requires = "month")]
    day: Option<u32>,
}

impl DateInputArgs {
    fn input(self) -> Result<DateInput> {
        if let Some(value) = self.value {
            return DateInput::parse_json(&value);
        }
        let year = self.year.ok_or_else(|| {
            Error::InvalidInput("--year is required with a civil or era date".into())
        })?;
        match (self.calendar, self.era) {
            (Some(calendar), None) => Ok(DateInput::Civil {
                calendar,
                year,
                month: self.month,
                day: self.day,
            }),
            (None, Some(era)) => Ok(DateInput::Era {
                era,
                year,
                month: self.month,
                day: self.day,
            }),
            _ => Err(Error::InvalidInput(
                "choose exactly one of --calendar, --era or --value".into(),
            )),
        }
    }
}

#[derive(Args)]
struct UpperDateArgs {
    #[arg(long, conflicts_with_all = ["upper_calendar", "upper_era", "upper_year", "upper_month", "upper_day"],
        help = "Advanced upper operand JSON for an inclusive interval")]
    upper_value: Option<String>,
    #[arg(
        long,
        conflicts_with = "upper_era",
        requires = "upper_year",
        value_name = "CALENDAR_ID"
    )]
    upper_calendar: Option<String>,
    #[arg(long, requires = "upper_year", value_name = "ERA_ID")]
    upper_era: Option<String>,
    #[arg(long, allow_hyphen_values = true, value_parser = signed_date_integer,
        help = "Upper endpoint year; defaults to the input calendar or era")]
    upper_year: Option<i64>,
    #[arg(long, value_parser = date_component, requires = "upper_year")]
    upper_month: Option<u32>,
    #[arg(long, value_parser = date_component, requires = "upper_month")]
    upper_day: Option<u32>,
}

impl UpperDateArgs {
    fn input(self, lower: &DateInput) -> Result<Option<DateInput>> {
        if let Some(value) = self.upper_value {
            return DateInput::parse_json(&value).map(Some);
        }
        let Some(year) = self.upper_year else {
            return Ok(None);
        };
        let calendar = self.upper_calendar;
        let era = self.upper_era;
        let (calendar, era) = if calendar.is_none() && era.is_none() {
            match lower {
                DateInput::Civil { calendar, .. } => (Some(calendar.clone()), None),
                DateInput::Era { era, .. } => (None, Some(era.clone())),
                _ => {
                    return Err(Error::InvalidInput(
                        "an extended lower operand requires --upper-calendar or --upper-era".into(),
                    ));
                }
            }
        } else {
            (calendar, era)
        };
        match (calendar, era) {
            (Some(calendar), None) => Ok(Some(DateInput::Civil {
                calendar,
                year,
                month: self.upper_month,
                day: self.upper_day,
            })),
            (None, Some(era)) => Ok(Some(DateInput::Era {
                era,
                year,
                month: self.upper_month,
                day: self.upper_day,
            })),
            _ => Err(Error::InvalidInput(
                "choose one upper calendar or era".into(),
            )),
        }
    }
}

#[derive(Subcommand)]
enum ChronologyCommand {
    /// Format an authored civil or era date, including partial dates.
    Format {
        #[command(flatten)]
        date: DateInputArgs,
    },
    /// Convert an exact civil or era date into a target calendar or era.
    ConvertDate {
        #[command(flatten)]
        date: DateInputArgs,
        #[arg(
            long,
            conflicts_with = "target_era",
            required_unless_present = "target_era"
        )]
        target_calendar: Option<String>,
        #[arg(long)]
        target_era: Option<String>,
    },
    /// Search authored annotations using inclusive calendar-date predicates.
    Search {
        #[arg(
            value_name = "PREDICATE",
            help = "on_date, before, after, overlaps or between"
        )]
        predicate: DatePredicate,
        #[command(flatten)]
        date: DateInputArgs,
        #[command(flatten)]
        upper: UpperDateArgs,
        #[arg(long)]
        record: Option<String>,
        #[arg(long)]
        calendar_filter: Option<String>,
        #[arg(long)]
        era_filter: Option<String>,
        #[arg(
            long,
            requires = "era_filter",
            help = "authored (default) or overlaps_bounds"
        )]
        era_mode: Option<EraMatchMode>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// List exact authored story anchors inside a date interval; never interpolate.
    StoryTimes {
        #[command(flatten)]
        date: DateInputArgs,
        #[command(flatten)]
        upper: UpperDateArgs,
        #[arg(long)]
        timeline: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// List authored record date annotations and their declared date bounds.
    Annotations {
        #[arg(value_name = "RECORD")]
        record: Option<String>,
        #[arg(long, value_name = "CALENDAR_ID")]
        calendar: Option<String>,
        #[arg(long, value_name = "ERA_ID")]
        era: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Compare two stored annotations without resolving uncertain claims.
    Compare {
        #[arg(value_name = "LEFT_RECORD")]
        left_record: String,
        #[arg(value_name = "LEFT_ANNOTATION_ID")]
        left_id: String,
        #[arg(value_name = "RIGHT_RECORD")]
        right_record: String,
        #[arg(value_name = "RIGHT_ANNOTATION_ID")]
        right_id: String,
    },
    /// List authored calendar declarations and eras (author, all time).
    Calendars {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Validate and format one exact civil date, optionally displaying an era year.
    Date {
        #[arg(value_name = "CALENDAR_ID")]
        calendar: String,
        #[command(flatten)]
        date: DateArgs,
        #[arg(long, value_name = "ERA_ID")]
        era: Option<String>,
    },
    /// Convert an exact civil date using authored shared-axis epochs.
    Convert {
        #[arg(value_name = "FROM_CALENDAR_ID")]
        from: String,
        #[arg(value_name = "TO_CALENDAR_ID")]
        to: String,
        #[command(flatten)]
        date: DateArgs,
    },
    /// Find an explicitly anchored story position for an exact civil date.
    ToStory {
        #[arg(value_name = "CALENDAR_ID")]
        calendar: String,
        #[command(flatten)]
        date: DateArgs,
        #[arg(long)]
        timeline: Option<String>,
    },
    /// Find a calendar date at an exact authored story anchor; never interpolate.
    FromStory {
        #[arg(value_name = "CALENDAR_ID")]
        calendar: String,
        #[arg(long, allow_hyphen_values = true, value_parser = signed_date_integer)]
        tick: i64,
        #[arg(long, allow_hyphen_values = true, default_value_t = 0, value_parser = anchor_order,
            help = "Exact anchor order; defaults to 0, not the end of the tick")]
        order: i32,
        #[arg(long)]
        timeline: Option<String>,
    },
}

#[derive(Subcommand)]
enum CatalogueCommand {
    /// Read one admitted catalogue page from an exact closed JSON request.
    Page {
        #[arg(
            long,
            value_name = "FILE",
            default_value = "-",
            help = "JSON request file, or - for stdin (at most 16 KiB)"
        )]
        file: PathBuf,
    },
    /// Resolve only the requested record labels under an explicit catalogue lens.
    Labels {
        #[arg(
            long,
            value_name = "FILE",
            default_value = "-",
            help = "JSON request file, or - for stdin (at most 16 KiB)"
        )]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum EntityCommand {
    /// List the complete local-author source inventory.
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        text: Option<String>,
    },
    /// Read source metadata and prose (author, all time).
    Show { entity: String },
}

#[derive(Args)]
struct RecordWriteArgs {
    #[arg(
        long,
        value_name = "FILE",
        help = "Typed YAML request file, or - for stdin (at most 1 MiB)"
    )]
    file: PathBuf,
    #[arg(long)]
    expected_fingerprint: String,
    #[arg(
        long,
        requires = "expected_after_sha256",
        help = "Apply the request instead of previewing it"
    )]
    apply: bool,
    #[arg(
        long,
        requires = "apply",
        help = "Exact afterSha256 returned by preview"
    )]
    expected_after_sha256: Option<String>,
}

#[derive(Subcommand)]
enum RecordCommand {
    /// Preview a typed record creation; add --apply with its preview hash to save it.
    Create {
        #[command(flatten)]
        request: RecordWriteArgs,
    },
    /// Preview a typed partial update; omitted fields and body remain unchanged.
    Update {
        entity: String,
        #[command(flatten)]
        request: RecordWriteArgs,
    },
}

#[derive(Subcommand)]
enum CharacterCommand {
    /// Compose admitted species descriptions with optional explicitly selected learned accounts.
    Species {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "INDIVIDUAL_ID")]
        individual: String,
        #[command(flatten)]
        selection: SpeciesSelectionArgs,
        #[arg(long = "belief-target", value_name = "ASSERTION_OR_KNOWLEDGE_ID")]
        belief_targets: Vec<String>,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// List admitted individuals explicitly linked to an admitted common description.
    SpeciesAffected {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "COMMON_ID")]
        common: String,
        #[command(flatten)]
        selection: SpeciesSelectionArgs,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read explicitly learned magic accounts without canonical profile composition.
    Magic {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(
            long = "belief-target",
            value_name = "ASSERTION_OR_KNOWLEDGE_ID",
            required = true
        )]
        belief_targets: Vec<String>,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Compose admitted language and culture at an exact character horizon.
    Social {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "LANGUAGE_ID")]
        language: String,
        #[arg(long, value_name = "SENSE_ID")]
        sense: Option<String>,
        #[arg(long, value_name = "LITERAL_TEXT", allow_hyphen_values = true)]
        form: Option<String>,
        #[arg(long = "context", value_name = "CONCEPT_ID")]
        contexts: Vec<String>,
        #[arg(long = "belief-target", value_name = "ASSERTION_OR_KNOWLEDGE_ID")]
        belief_targets: Vec<String>,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Compose admitted environmental roots and their explicit one-hop context.
    Environment {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(long = "context", value_name = "CONCEPT_ID", required = true)]
        contexts: Vec<String>,
        #[arg(long = "belief-target", value_name = "ASSERTION_OR_KNOWLEDGE_ID")]
        belief_targets: Vec<String>,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Compose an independently admitted brief at an exact character horizon.
    Brief {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "BRIEF_ID")]
        brief: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read admitted authored evidence for an exact character ID at a story horizon.
    Context {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 16000)]
        max_characters: usize,
    },
    /// Search only evidence admitted to this character at the requested horizon.
    Search {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        query: String,
        #[arg(
            long,
            value_name = "MODE",
            help = "lexical (default), semantic, or hybrid"
        )]
        mode: Option<wedl::retrieval::Mode>,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Read admitted conversation beats and this character's own recollection.
    Conversation {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "CONVERSATION_ID")]
        conversation: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Read explicitly learned assertion wording without joining current canon.
    Assertions {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "ASSERTION_OR_KNOWLEDGE_ID")]
        target: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read dated sections admitted to the exact character at the horizon.
    Descriptions {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "OWNER_ID")]
        owner: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read one admitted section by its exact owner and local member IDs.
    Section {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "OWNER_ID")]
        owner: String,
        #[arg(value_name = "SECTION_ID")]
        section: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// List currently admitted typed lore concepts without source metadata.
    Lore {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(long)]
        domain: Option<String>,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read one admitted typed lore concept by exact ID.
    Concept {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "CONCEPT_ID")]
        target: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Read explicit concept links whose owner and targets are admitted.
    Links {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "OWNER_ID")]
        owner: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read admitted authored spatial members at an exact horizon.
    SpatialHistory {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "OWNER_ID")]
        owner: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Inspect the explicit unknown boundary for typed character existence.
    Existence {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "TARGET_CHARACTER_ID")]
        target: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read learned governance words with applicability left unassigned.
    Governance {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[arg(value_name = "LOCATION_OR_OVERLAY_ID")]
        target: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum BriefCommand {
    /// Compose explicit required pins after owner and target admission.
    Compose {
        #[arg(value_name = "BRIEF_ID")]
        brief: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum WritingCommand {
    /// Validate an exact captured frame without selecting a replacement.
    Resolve {
        #[arg(long, value_name = "FILE", help = "Closed frame JSON; use - for stdin")]
        file: PathBuf,
    },
    /// List complete admitted work, scene, point-of-view and moment choices for a valid frame.
    Choices {
        #[arg(long, value_name = "FILE", help = "Closed frame JSON; use - for stdin")]
        file: PathBuf,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Render complete author and/or character context from one captured frame.
    Context {
        #[arg(long, value_name = "FILE", help = "Closed frame JSON; use - for stdin")]
        file: PathBuf,
        #[arg(long = "brief", value_name = "BRIEF_ID")]
        briefs: Vec<String>,
        #[arg(
            long,
            value_name = "JSON",
            help = "Closed optional family selections; inherits this frame's lens and horizon"
        )]
        families: Option<String>,
        #[arg(long, default_value_t = 16000)]
        max_characters: usize,
    },
}

#[derive(Subcommand)]
enum CurrentTimeCommand {
    /// Preview an explicit plan setting the world and every active scene cursor.
    Preview {
        #[arg(
            long,
            value_name = "FILE",
            help = "Closed current-time request JSON; use - for stdin"
        )]
        file: PathBuf,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
}

#[derive(Subcommand)]
enum SceneControlCommand {
    /// Preview coordinated scene, attendance or movement changes as an exact Plan.
    Preview {
        #[arg(
            long,
            value_name = "FILE",
            help = "Closed scene-control request JSON; use - for stdin"
        )]
        file: PathBuf,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
}

#[derive(Subcommand)]
enum ConversationControlCommand {
    /// Preview a linked conversation or appended speech/action beat as an exact Plan.
    Preview {
        #[arg(
            long,
            value_name = "FILE",
            help = "Closed conversation-control request JSON; use - for stdin"
        )]
        file: PathBuf,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
}

#[derive(Subcommand)]
enum HypothesisCommand {
    /// List complete, noncanonical possibilities across all story time.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        text: Option<String>,
        #[arg(long, default_value_t = 100)]
        max_items: usize,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
    /// Inspect one exact hypothesis and its authored references.
    Detail {
        hypothesis_id: String,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
    /// Preview an explicit create, adopt-existing, or reject request as an ordinary Plan.
    Preview {
        #[arg(
            long,
            value_name = "FILE",
            help = "Closed hypothesis request JSON; use - for stdin"
        )]
        file: PathBuf,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
}

#[derive(Args)]
struct LoreScopeArgs {
    #[command(flatten)]
    at: OptionalTimeArgs,
    #[arg(long, default_value = "author")]
    audience: String,
    #[arg(long, default_value = "author")]
    view: String,
}

#[derive(Subcommand)]
enum DescriptionCommand {
    /// Read authored section history, or applicable sections at an exact horizon.
    List {
        #[arg(value_name = "OWNER_ID")]
        owner: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read one stable local section by exact owner and section IDs.
    Section {
        #[arg(value_name = "OWNER_ID")]
        owner: String,
        #[arg(value_name = "SECTION_ID")]
        section: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
    },
}

#[derive(Subcommand)]
enum AssertionCommand {
    /// List the world's authored predicate declarations.
    Predicates {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read assertion history or applicable assertions for an exact assertion or subject ID.
    List {
        #[arg(value_name = "TARGET_ID")]
        target: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum ExistenceCommand {
    /// Read independent authored existence episodes for an exact character ID.
    List {
        #[arg(value_name = "CHARACTER_ID")]
        character: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum GovernanceCommand {
    /// Read authored authority and obligations through explicit profile-role bindings.
    Query {
        #[arg(value_name = "TARGET_ID")]
        target: String,
        #[arg(long, value_name = "PROFILE_ID")]
        profile: String,
        #[arg(long, value_name = "JSON")]
        roles: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Args)]
struct FamilyScopeArgs {
    #[command(flatten)]
    at: TimeArgs,
    #[arg(long, default_value = "author")]
    audience: String,
    #[arg(long, default_value = "author")]
    view: String,
}

#[derive(Subcommand)]
enum SocialCommand {
    /// Compose authored language, spellings and selected cultural context at an exact horizon.
    Context {
        #[arg(value_name = "LANGUAGE_ID")]
        language: String,
        #[arg(long, value_name = "SENSE_ID")]
        sense: Option<String>,
        #[arg(long, value_name = "LITERAL_TEXT", allow_hyphen_values = true)]
        form: Option<String>,
        #[arg(long = "context", value_name = "CONCEPT_ID")]
        contexts: Vec<String>,
        #[command(flatten)]
        scope: FamilyScopeArgs,
    },
}

#[derive(Subcommand)]
enum EnvironmentCommand {
    /// Compose authored environment, material and technology context at an exact horizon.
    Context {
        #[arg(long = "context", value_name = "CONCEPT_ID", required = true)]
        contexts: Vec<String>,
        #[command(flatten)]
        scope: FamilyScopeArgs,
    },
}

#[derive(Subcommand)]
enum LoreCommand {
    /// List the world's authored profile definitions.
    Profiles {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Inspect one authored profile by its exact ID.
    Profile { profile: String },
    /// Read canonical concepts within author presentation filters and an optional horizon.
    Catalog {
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Inspect a concept admitted by the selected author filters.
    Concept {
        concept: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
    },
    /// Read a record's explicit links to admitted lore concepts.
    Links {
        record: String,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read explicitly profile-bound state at an exact story horizon.
    State {
        record: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value = "author")]
        audience: String,
        #[arg(long, default_value = "author")]
        view: String,
    },
}

#[derive(Args)]
struct SpeciesSelectionArgs {
    #[arg(long)]
    common_profile: String,
    #[arg(long)]
    individual_profile: String,
    #[arg(long)]
    individual_link: String,
    #[arg(long)]
    common_link: Option<String>,
    #[arg(
        long,
        help = "JSON object mapping common-description roles to exact root fields"
    )]
    common_fields: Option<String>,
    #[arg(
        long,
        help = "JSON object mapping individual-description roles to exact root fields"
    )]
    individual_fields: Option<String>,
}
impl SpeciesSelectionArgs {
    fn input(self) -> Result<wedl::species::Selection> {
        let selection = wedl::species::Selection {
            common_profile: self.common_profile,
            individual_profile: self.individual_profile,
            individual_link: self.individual_link,
            common_link: self.common_link,
            common_fields: self
                .common_fields
                .as_deref()
                .map(wedl::service::species_fields_json)
                .transpose()?
                .unwrap_or_default(),
            individual_fields: self
                .individual_fields
                .as_deref()
                .map(wedl::service::species_fields_json)
                .transpose()?
                .unwrap_or_default(),
        };
        selection.validate()?;
        Ok(selection)
    }
}

#[derive(Subcommand)]
enum SpeciesCommand {
    /// Read explicit common descriptions and individual variation separately.
    Compose {
        individual: String,
        #[command(flatten)]
        selection: SpeciesSelectionArgs,
        #[command(flatten)]
        scope: LoreScopeArgs,
    },
    /// List admitted variation concepts explicitly linked to one common concept.
    Affected {
        common: String,
        #[command(flatten)]
        selection: SpeciesSelectionArgs,
        #[command(flatten)]
        scope: LoreScopeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum MagicCommand {
    /// Read explicitly linked traditions, practices, realms and artifacts.
    Compose {
        root: String,
        #[arg(long, help = "JSON object mapping selected roles to exact profile IDs")]
        profiles: String,
        #[arg(
            long,
            help = "JSON object mapping roles to ordered root reference-field arrays"
        )]
        links: Option<String>,
        #[arg(
            long,
            help = "JSON object mapping roles to authored restriction root fields"
        )]
        restrictions: Option<String>,
        #[arg(
            long,
            help = "JSON object mapping roles to authored exception root fields"
        )]
        exceptions: Option<String>,
        #[command(flatten)]
        scope: LoreScopeArgs,
    },
}

#[derive(Subcommand)]
enum SpatialCommand {
    /// Read authored containment and open outgoing routes (author, all time).
    Location {
        entity: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read authored spatial history, retaining corrections in the all-time view.
    History {
        record: String,
        #[command(flatten)]
        at: OptionalTimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Inspect authored route fields, optionally at an exact historical horizon.
    Route {
        route: String,
        #[command(flatten)]
        at: OptionalTimeArgs,
    },
    /// Find a directed path using hops or one authored metric and exact unit.
    Path {
        from: String,
        to: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value = "hops", value_parser = ["hops", "route_distance", "travel_cost", "duration"])]
        metric: String,
        #[arg(long)]
        unit: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read the complete set reachable through open routes and location portals.
    Reachable {
        from: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read overlay memberships at an exact horizon with authored presentation filters.
    Overlays {
        location: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value = "author")]
        audience: String,
        #[arg(long, default_value = "author")]
        view: String,
        #[arg(long)]
        overlay: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// List authored local planar and EPSG:4326 maps (author, all time).
    Maps {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Inspect authored map anchor correspondences without inferring a transform.
    Anchors {
        #[arg(long)]
        from_map: Option<String>,
        #[arg(long)]
        to_map: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Match authored geometry bounds against an inclusive two/three-axis box.
    Bbox {
        map: String,
        #[arg(long, allow_hyphen_values = true)]
        min_x: f64,
        #[arg(long, allow_hyphen_values = true)]
        min_y: f64,
        #[arg(long, allow_hyphen_values = true)]
        max_x: f64,
        #[arg(long, allow_hyphen_values = true)]
        max_y: f64,
        #[arg(long, allow_hyphen_values = true, requires = "max_z")]
        min_z: Option<f64>,
        #[arg(long, allow_hyphen_values = true, requires = "min_z")]
        max_z: Option<f64>,
        #[arg(long, default_value = "intersects", value_parser = ["intersects", "within"])]
        relation: String,
        #[command(flatten)]
        at: OptionalTimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Find geometry within Euclidean coordinate distance in one map's units.
    Nearby {
        map: String,
        #[arg(long, allow_hyphen_values = true)]
        x: f64,
        #[arg(long, allow_hyphen_values = true)]
        y: f64,
        #[arg(long, allow_hyphen_values = true)]
        z: Option<f64>,
        #[arg(long, allow_hyphen_values = true)]
        radius: f64,
        #[command(flatten)]
        at: OptionalTimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Args)]
struct GenerationalLineageArgs {
    character: String,
    #[command(flatten)]
    at: TimeArgs,
    #[arg(
        long,
        default_value_t = 8,
        help = "Complete traversal depth limit, from 0 to 32"
    )]
    max_depth: usize,
    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Args)]
struct GenerationalLabelsArgs {
    /// One or more exact endpoint IDs.
    #[arg(required = true, num_args = 1..)]
    ids: Vec<String>,
    #[command(flatten)]
    at: TimeArgs,
    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Args)]
struct GenerationalDiscoverArgs {
    /// Endpoint kind: character, organization, legacy or event.
    kind: String,
    #[arg(long, default_value = "")]
    prefix: String,
    #[command(flatten)]
    at: TimeArgs,
    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Args)]
struct GenerationalSearchArgs {
    /// One to eight word prefixes over visible generational evidence.
    text: String,
    #[command(flatten)]
    at: TimeArgs,
    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Args)]
struct GenerationalContextArgs {
    subject: String,
    #[command(flatten)]
    at: TimeArgs,
    #[arg(long, default_value_t = 8)]
    max_depth: usize,
    #[arg(long, default_value_t = 100)]
    limit: usize,
    /// Complete rendered context budget, from 80 to 1000000 characters.
    #[arg(long, default_value_t = 12_000)]
    max_chars: usize,
}

#[derive(Subcommand)]
enum GenerationalCommand {
    /// Read one character's held genealogy assertions, using an exact viewer ID.
    Learned {
        viewer: String,
        #[command(subcommand)]
        command: LearnedGenerationalCommand,
    },
    /// Resolve labels of visible generational endpoints at a story horizon.
    Labels(GenerationalLabelsArgs),
    /// Discover visible generational endpoints by kind and optional name prefix.
    Discover(GenerationalDiscoverArgs),
    /// Search visible generational source at a story horizon.
    Search(GenerationalSearchArgs),
    /// Build a complete bounded generational context for a subject.
    Context(GenerationalContextArgs),
    /// Read active authored parents, children and unions at a story horizon.
    Family {
        character: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Follow active parentage toward ancestors, without inferring family labels.
    Ancestors(GenerationalLineageArgs),
    /// Follow active parentage toward descendants at a story horizon.
    Descendants(GenerationalLineageArgs),
    /// Find a bounded path through authored parent/child links; unions add no edges.
    Path {
        from: String,
        to: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(
            long,
            default_value_t = 8,
            help = "Complete search limit in parent/child steps, from 0 to 32"
        )]
        max_hops: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read a canonical generational record's history through a story horizon.
    History {
        record: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read organization hierarchy and explicit affiliations at a story horizon.
    Organization {
        organization: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(
            long,
            default_value_t = 8,
            help = "Complete hierarchy depth limit, from 0 to 32"
        )]
        max_depth: usize,
        #[arg(long)]
        include_former: bool,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read an authored legacy, its tenures and its claims at a story horizon.
    Legacy {
        legacy: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// List an organization's explicitly associated legacies at a story horizon.
    OrganizationLegacies {
        organization: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read a character's authored vital status at a story horizon.
    Vital {
        character: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

#[derive(Args)]
struct LearnedSubjectArgs {
    /// Exact endpoint ID or an unambiguous label explicitly learned by this viewer.
    subject: String,
    #[command(flatten)]
    at: TimeArgs,
    #[arg(long, default_value_t = 100)]
    limit: usize,
}

#[derive(Subcommand)]
enum LearnedGenerationalCommand {
    /// Read only labels explicitly learned for the supplied exact IDs.
    Labels(GenerationalLabelsArgs),
    /// Discover only endpoint labels explicitly learned by this viewer.
    Discover(GenerationalDiscoverArgs),
    /// Search held assertions and learned labels, without canonical source text.
    Search(GenerationalSearchArgs),
    /// Build a bounded context using only this viewer's held assertions.
    Context(GenerationalContextArgs),
    /// List held assertions applicable at the selected story horizon.
    Assertions {
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        /// Retain held assertions even outside their asserted validity interval.
        #[arg(long)]
        held_history: bool,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read learned parentage and union assertions for a character.
    Family(LearnedSubjectArgs),
    /// Follow learned parentage toward ancestors.
    Ancestors(GenerationalLineageArgs),
    /// Follow learned parentage toward descendants.
    Descendants(GenerationalLineageArgs),
    /// Find a path using only this viewer's admitted parentage assertions.
    Path {
        from: String,
        to: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 8)]
        max_hops: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read held endpoint assertions, including those outside their validity interval.
    History(LearnedSubjectArgs),
    /// Read learned organization hierarchy and affiliation assertions.
    Organization {
        organization: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 8)]
        max_depth: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Read learned tenure and claim assertions without inferring succession.
    Legacy(LearnedSubjectArgs),
    /// Read learned vital assertions without resolving disagreements.
    Vital(LearnedSubjectArgs),
}

fn learned_operation(viewer: String, command: LearnedGenerationalCommand) -> Operation {
    match command {
        LearnedGenerationalCommand::Labels(args) => Operation::GenerationalLabels {
            viewer: Some(viewer),
            ids: args.ids,
            at: args.at.into(),
            limit: args.limit,
        },
        LearnedGenerationalCommand::Discover(args) => Operation::GenerationalDiscover {
            viewer: Some(viewer),
            kind: args.kind,
            prefix: args.prefix,
            at: args.at.into(),
            limit: args.limit,
        },
        LearnedGenerationalCommand::Search(args) => Operation::GenerationalSearch {
            viewer: Some(viewer),
            text: args.text,
            at: args.at.into(),
            limit: args.limit,
        },
        LearnedGenerationalCommand::Context(args) => Operation::GenerationalContext {
            viewer: Some(viewer),
            subject: args.subject,
            at: args.at.into(),
            max_depth: args.max_depth,
            limit: args.limit,
            max_chars: args.max_chars,
        },
        LearnedGenerationalCommand::Assertions {
            at,
            kind,
            endpoint,
            held_history,
            limit,
        } => Operation::GenerationalLearnedAssertions {
            viewer,
            at: at.into(),
            kind,
            endpoint,
            held_history,
            limit,
        },
        LearnedGenerationalCommand::Family(args) => Operation::GenerationalLearnedFamily {
            viewer,
            character: args.subject,
            at: args.at.into(),
            limit: args.limit,
        },
        LearnedGenerationalCommand::Ancestors(args) => Operation::GenerationalLearnedLineage {
            viewer,
            character: args.character,
            at: args.at.into(),
            direction: LineageDirection::Ancestors,
            max_depth: args.max_depth,
            limit: args.limit,
        },
        LearnedGenerationalCommand::Descendants(args) => Operation::GenerationalLearnedLineage {
            viewer,
            character: args.character,
            at: args.at.into(),
            direction: LineageDirection::Descendants,
            max_depth: args.max_depth,
            limit: args.limit,
        },
        LearnedGenerationalCommand::Path {
            from,
            to,
            at,
            max_hops,
            limit,
        } => Operation::GenerationalLearnedPath {
            viewer,
            from,
            to,
            at: at.into(),
            max_hops,
            limit,
        },
        LearnedGenerationalCommand::History(args) => Operation::GenerationalLearnedAssertions {
            viewer,
            at: args.at.into(),
            kind: None,
            endpoint: Some(args.subject),
            held_history: true,
            limit: args.limit,
        },
        LearnedGenerationalCommand::Organization {
            organization,
            at,
            max_depth,
            limit,
        } => Operation::GenerationalLearnedOrganization {
            viewer,
            organization,
            at: at.into(),
            max_depth,
            limit,
        },
        LearnedGenerationalCommand::Legacy(args) => Operation::GenerationalLearnedLegacy {
            viewer,
            legacy: args.subject,
            at: args.at.into(),
            limit: args.limit,
        },
        LearnedGenerationalCommand::Vital(args) => Operation::GenerationalLearnedVital {
            viewer,
            character: args.subject,
            at: args.at.into(),
            limit: args.limit,
        },
    }
}

#[derive(Subcommand)]
enum ActionDraftCommand {
    /// List private drafts in the working world's separate author store.
    List,
    /// Read a private draft, optionally pinned to its exact stored bytes.
    Detail {
        id: String,
        #[arg(long)]
        sha256: Option<String>,
    },
    /// Inspect the exact private-file change without saving it.
    Preview {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Save one private draft with exact source and prior-file checks.
    Save {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Load captured review context at the draft's own exact horizon.
    Review {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Preview the complete consequences of an exact saved active proposal.
    PreviewProposal {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Review the exact saved candidate before explicit outcome approval.
    ReviewOutcome {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Publish an explicitly approved outcome with its exact review binding.
    ApplyOutcome {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Verify an applied receipt against current exact source without writing.
    VerifyOutcome {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum EditDraftCommand {
    /// List retained edits in the selected current or historical source.
    List,
    /// Read exact retained edit bytes without applying them.
    Detail {
        id: String,
        #[arg(long)]
        sha256: Option<String>,
    },
    /// Preview a private buffer save without evaluating its unfinished patch.
    Preview {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Save an unfinished private edit with exact source and file checks.
    Save {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Review the canonical candidate from an exact saved edit.
    Review {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Publish an explicitly approved edit and its terminal receipt together.
    Apply {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum ManuscriptCommand {
    /// Inspect a complete selected private work/segment/reveal catalogue.
    List {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Read exact current or original committed manuscript source.
    Detail {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Assemble complete private work occurrences in authored order.
    Resolve {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Validate a private source save without writing it.
    PreviewSave {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Save one private source with canonical, manuscript and prior-file pins.
    Save {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum RevealCommand {
    /// Resolve an exact private reveal plan and its authored work occurrences.
    Resolve {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum ReaderCommand {
    /// Read disclosed items under an explicitly installed public policy.
    Page {
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Search only disclosed items under an explicitly installed public policy.
    Search {
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Simulate a reader using an explicit operator-owned author policy.
    Preview {
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum RetconImpactCommand {
    /// Read complete typed reverse dependencies for one explicit candidate.
    Preview {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum RetconReviewCommand {
    /// Reconstruct an original committed analysis without saving it.
    Preview {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Inspect an exact private review save without changing source.
    PreviewSave {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Save review decisions and exact commentary with independent source/file pins.
    Save {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Read a complete selected private review queue and independent currentness.
    Queue {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Read an original private review and compare its admitted input witnesses.
    Detail {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum ConsequencesCommand {
    /// Check explicit semantic expectations against one captured source snapshot.
    Check {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Inspect a pending outcome transaction without loading canonical source.
    PublicationStatus,
    /// Recover a pending outcome transaction pinned to the inspected journal.
    Recover {
        #[arg(long)]
        expected_journal_sha256: String,
    },
    /// Plan explicit record creates/updates in memory without publishing source.
    Preview {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        #[arg(long, default_value_t = 100000)]
        max_characters: usize,
    },
}

#[derive(Subcommand)]
enum ActionRequirementCommand {
    /// Capture selected magic context for a separate literal comparison.
    Capture {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        #[arg(long, value_name = "JSON")]
        families: String,
        #[arg(long, default_value_t = 16000)]
        max_characters: usize,
    },
    /// Compare one exact captured field or assertion with an authored JSON literal.
    Compare {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        #[arg(long, value_name = "JSON")]
        families: String,
        #[arg(long, value_name = "JSON")]
        requirement: String,
        #[arg(long, default_value_t = 16000)]
        max_characters: usize,
    },
}

#[derive(Subcommand)]
enum AuthoringCommand {
    /// Read static native Plan schemas and editable kind/field guidance without loading a world.
    Schema,
    /// Discover complete author choices from one validated source snapshot.
    Context {
        #[arg(long, default_value_t = wedl::authoring_tools::MAX_ITEMS)]
        max_items: usize,
    },
    /// Generate an editable, source-pinned Plan without changing source.
    Scaffold {
        #[arg(long, value_enum, default_value = "changeset")]
        family: wedl::authoring_tools::Family,
        /// Exact existing record ID; changeset defaults to the world record.
        #[arg(long)]
        entity: Option<String>,
        /// Exact new organization ID for the generational starter.
        #[arg(long)]
        id: Option<String>,
        /// New organization title for the generational starter.
        #[arg(long)]
        title: Option<String>,
        /// Require the source used for prior discovery; defaults to this capture.
        #[arg(long)]
        expected_fingerprint: Option<String>,
    },
}

#[derive(Subcommand)]
enum Command {
    /// Print static Bash or PowerShell completion for commands, options and fixed choices.
    Completion {
        #[arg(value_enum)]
        shell: completion::Shell,
    },
    #[command(subcommand)]
    Authoring(AuthoringCommand),
    /// Create an empty native world in a new directory.
    Init {
        path: PathBuf,
        #[arg(long)]
        title: Option<String>,
    },
    /// Show the loaded snapshot, source counts, cache status and implementation limits.
    Status,
    /// Run implemented structural and semantic checks; complete=false flags remaining validation work.
    Validate,
    /// Build the disposable SQLite/FTS index for this exact snapshot.
    Compile,
    #[command(subcommand)]
    Catalogue(CatalogueCommand),
    #[command(subcommand)]
    Entity(EntityCommand),
    #[command(subcommand)]
    Record(RecordCommand),
    #[command(subcommand)]
    Character(CharacterCommand),
    #[command(subcommand)]
    Briefs(BriefCommand),
    #[command(subcommand)]
    Writing(WritingCommand),
    #[command(subcommand)]
    CurrentTime(CurrentTimeCommand),
    #[command(subcommand)]
    SceneControl(SceneControlCommand),
    #[command(subcommand)]
    ConversationControl(ConversationControlCommand),
    #[command(subcommand)]
    Hypotheses(HypothesisCommand),
    #[command(subcommand)]
    ActionDrafts(ActionDraftCommand),
    /// Retain unfinished edits and separately review or approve one existing target.
    #[command(subcommand)]
    EditDrafts(EditDraftCommand),
    /// Compare three original Git commits at one horizon without merging or writing.
    RevisionReview {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    #[command(subcommand)]
    RetconImpact(RetconImpactCommand),
    #[command(subcommand)]
    Manuscripts(ManuscriptCommand),
    #[command(subcommand)]
    RevealPlans(RevealCommand),
    #[command(subcommand)]
    Reader(ReaderCommand),
    #[command(subcommand)]
    RetconReviews(RetconReviewCommand),
    /// Inspect one event's explicit local contribution and separately caused history.
    EventConsequences {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    /// Inspect source and semantic groups for an explicit consequence plan.
    GroupedConsequences {
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
    },
    #[command(subcommand)]
    Consequences(ConsequencesCommand),
    #[command(subcommand)]
    ActionRequirements(ActionRequirementCommand),
    #[command(subcommand)]
    Descriptions(DescriptionCommand),
    #[command(subcommand)]
    Assertions(AssertionCommand),
    #[command(subcommand)]
    Existence(ExistenceCommand),
    #[command(subcommand)]
    Governance(GovernanceCommand),
    #[command(subcommand)]
    Social(SocialCommand),
    #[command(subcommand)]
    Environment(EnvironmentCommand),
    #[command(subcommand)]
    Lore(LoreCommand),
    #[command(subcommand)]
    Species(SpeciesCommand),
    #[command(subcommand)]
    Magic(MagicCommand),
    #[command(subcommand)]
    Spatial(SpatialCommand),
    #[command(subcommand)]
    Chronology(ChronologyCommand),
    #[command(subcommand)]
    Generational(GenerationalCommand),
    /// Read a scene's authored participants and evidence at a story horizon.
    Scene {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Replay one entity's explicit state at an ordinal horizon.
    State {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Resolve a character's explicit learned/forgotten knowledge.
    Knowledge {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long)]
        include_forgotten: bool,
    },
    /// Resolve authored directional relationship metrics.
    Relationships {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Resolve authored story-point lifecycles at a story horizon.
    StoryPoints {
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long)]
        scene: Option<String>,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Read one story point's dated lifecycle, authored causes and outcomes.
    StoryPoint {
        id: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long)]
        scene: Option<String>,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Follow explicitly authored causal links to an event at a story horizon.
    Causal {
        event: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value = "upstream", value_parser = ["upstream", "downstream", "both"])]
        direction: String,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Read character journeys, current presence and cohort-relative prominence.
    Whereabouts {
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long = "character", value_name = "CHARACTER_ID")]
        characters: Vec<String>,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Read canonical events explicitly naming both characters as participants.
    Interactions {
        first: String,
        second: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Read only the selected records' memberships in selected threads.
    ThreadMemberships {
        #[arg(long = "record", value_name = "RECORD_ID", required = true)]
        record_ids: Vec<String>,
        #[arg(long = "thread", value_name = "THREAD_ID", required = true)]
        thread_ids: Vec<String>,
        #[command(flatten)]
        at: OptionalTimeArgs,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// List active environment records explicitly targeting an entity.
    Environments {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// List an authored timeline, optionally bounded by a tick.
    Timeline {
        #[arg(long)]
        timeline: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        tick: Option<i64>,
        #[arg(long, allow_hyphen_values = true, requires = "tick")]
        order: Option<i32>,
        #[arg(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Read an author transcript and recollections up to a story horizon.
    Conversation {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
    },
    /// Compose an author packet about a character's state and knowledge.
    Context {
        entity: String,
        #[command(flatten)]
        at: TimeArgs,
        #[arg(long, default_value_t = 16000)]
        max_characters: usize,
    },
    /// Search authored prose with optional story horizon and exact thread grouping.
    Search {
        query: String,
        #[arg(
            long,
            value_name = "MODE",
            help = "lexical (default), semantic, or hybrid"
        )]
        mode: Option<wedl::retrieval::Mode>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        kind: Option<String>,
        #[command(flatten)]
        at: OptionalTimeArgs,
        #[arg(long = "thread", value_name = "THREAD_ID")]
        thread_ids: Vec<String>,
        #[arg(
            long,
            help = "Add a separate all-time lexical hypothesis lane; limit applies per lane"
        )]
        include_hypotheses: bool,
    },
    /// List declared narrative grouping labels.
    Threads,
    /// Preview a body replacement without writing source; retain its afterSha256 for apply-body.
    PreviewBody {
        entity: String,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        #[arg(long)]
        expected_fingerprint: String,
    },
    /// Apply one previewed body replacement to the worktree; does not create a Git commit.
    ApplyBody {
        entity: String,
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        #[arg(long)]
        expected_fingerprint: String,
        #[arg(long)]
        expected_after_sha256: String,
    },
    /// Serve the local-author browser with fresh worktree reads and guarded reviewed writes; revisions are read-only.
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Explicit operator-owned policy for author reader simulation.
        #[arg(long, value_name = "FILE")]
        reader_simulation_policy: Option<PathBuf>,
    },
    /// Serve the isolated anonymous reader UI/API; no policy means no access.
    ServeReader {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8766)]
        port: u16,
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{}", Error::InvalidInput(error.to_string()).json());
            return ExitCode::from(2);
        }
    };
    let compact = cli.compact;
    match run(cli).await {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let output = if compact {
                serde_json::to_string(&error.json())
            } else {
                serde_json::to_string_pretty(&error.json())
            };
            eprintln!("{}",output.unwrap_or_else(|_|"{\"code\":\"internal_error\",\"message\":\"error serialization failed\",\"details\":{}}".into()));
            ExitCode::from(2)
        }
    }
}

async fn run(cli: Cli) -> Result<u8> {
    if let Command::Completion { shell } = &cli.command {
        if cli.compact {
            return Err(Error::InvalidInput(
                "completion emits shell code and cannot be combined with --compact".into(),
            ));
        }
        print!("{}", completion::render(Cli::command(), shell.clone())?);
        return Ok(0);
    }
    if let Command::Authoring(AuthoringCommand::Schema) = &cli.command {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "authoring schema is source-independent and does not accept --revision".into(),
            ));
        }
        print_json(
            &serde_json::json!({"protocol":wedl::CONTRACT,"result":wedl::authoring_schema::schema()}),
            cli.compact,
        )?;
        return Ok(0);
    }
    if let Command::RevealPlans(RevealCommand::Resolve { file }) = &cli.command {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "reveal reads use their explicit request revision, not --revision".into(),
            ));
        }
        let request =
            wedl::manuscript::decode_reveal_resolve(&read_manuscript_input(file.clone())?)?;
        let result = wedl::manuscript::resolve_reveal(&cli.repo, &request)?;
        print_json(
            &serde_json::json!({"protocol":wedl::CONTRACT,"result":result}),
            cli.compact,
        )?;
        return Ok(0);
    }
    if let Command::Reader(command) = &cli.command {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "reader requests use their explicit pinned revision, not --revision".into(),
            ));
        }
        let service = wedl::reader::PresentationService::new(Some(cli.repo.clone()));
        let (policy, file, preview, operation) = match command {
            ReaderCommand::Page { policy, file } => {
                (policy, file, false, wedl::reader::Operation::Page)
            }
            ReaderCommand::Search { policy, file } => {
                (policy, file, false, wedl::reader::Operation::Search)
            }
            ReaderCommand::Preview { policy, file } => {
                (policy, file, true, wedl::reader::Operation::Page)
            }
        };
        if let Some(path) = policy {
            let raw = wedl::reader_config::read(path)?;
            if preview {
                service.install_simulation(Some(wedl::reader_config::decode_simulation(
                    &cli.repo, &raw,
                )?))?;
            } else {
                service
                    .install_public(Some(wedl::reader_config::decode_public(&cli.repo, &raw)?))?;
            }
        }
        let raw = read_bounded_input(file.clone(), 16384, "reader request exceeds 16 KiB")?;
        let result = if preview {
            service.preview(raw.as_bytes())?
        } else {
            service.execute(raw.as_bytes(), operation)?
        };
        let code = if result["outcome"] == "available" {
            0
        } else {
            2
        };
        print_json(&result, cli.compact)?;
        return Ok(code);
    }
    if let Command::ServeReader { host, port, policy } = &cli.command {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "reader server revision is installed in its policy".into(),
            ));
        }
        let service = wedl::reader::PresentationService::new(Some(cli.repo.clone()));
        if let Some(path) = policy {
            service.install_public(Some(wedl::reader_config::decode_public(
                &cli.repo,
                &wedl::reader_config::read(path)?,
            )?))?;
        }
        wedl::reader::serve(service, host, *port).await?;
        return Ok(0);
    }
    if let Command::Manuscripts(
        command @ (ManuscriptCommand::PreviewSave { .. } | ManuscriptCommand::Save { .. }),
    ) = &cli.command
    {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "manuscript saves require the current worktree".into(),
            ));
        }
        let (file, preview) = match command {
            ManuscriptCommand::PreviewSave { file } => (file, true),
            ManuscriptCommand::Save { file } => (file, false),
            _ => unreachable!(),
        };
        let request = wedl::manuscript::decode_save(&read_manuscript_input(file.clone())?)?;
        let world = wedl::historical::load_current_world(&cli.repo)?;
        let operation = if preview {
            Operation::ManuscriptSavePreview { request }
        } else {
            Operation::ManuscriptSave { request }
        };
        print_json(&Service::new(world).execute(operation)?, cli.compact)?;
        return Ok(0);
    }
    if let Command::Manuscripts(
        command @ (ManuscriptCommand::List { .. }
        | ManuscriptCommand::Detail { .. }
        | ManuscriptCommand::Resolve { .. }),
    ) = &cli.command
    {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "manuscript reads use their explicit request revision, not --revision".into(),
            ));
        }
        let result = match command {
            ManuscriptCommand::List { file } => wedl::manuscript::list(
                &cli.repo,
                &wedl::manuscript::decode_list(&read_manuscript_input(file.clone())?)?,
            )?,
            ManuscriptCommand::Detail { file } => wedl::manuscript::detail(
                &cli.repo,
                &wedl::manuscript::decode_detail(&read_manuscript_input(file.clone())?)?,
            )?,
            ManuscriptCommand::Resolve { file } => wedl::manuscript::resolve(
                &cli.repo,
                &wedl::manuscript::decode_resolve(&read_manuscript_input(file.clone())?)?,
            )?,
            _ => unreachable!(),
        };
        print_json(
            &serde_json::json!({"protocol":wedl::CONTRACT,"result":result}),
            cli.compact,
        )?;
        return Ok(0);
    }
    if let Command::RetconReviews(
        command @ (RetconReviewCommand::Preview { .. }
        | RetconReviewCommand::Queue { .. }
        | RetconReviewCommand::Detail { .. }),
    ) = &cli.command
    {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "retcon review reads use explicit request revisions, not --revision".into(),
            ));
        }
        let result = match command {
            RetconReviewCommand::Preview { file } => wedl::retcon_reviews::preview(
                &cli.repo,
                &wedl::retcon_reviews::decode_preview(&read_retcon_input(file.clone())?)?,
            )?,
            RetconReviewCommand::Queue { file } => wedl::retcon_reviews::queue(
                &cli.repo,
                &wedl::retcon_reviews::decode_queue(&read_retcon_input(file.clone())?)?,
            )?,
            RetconReviewCommand::Detail { file } => wedl::retcon_reviews::detail(
                &cli.repo,
                &wedl::retcon_reviews::decode_detail(&read_retcon_input(file.clone())?)?,
            )?,
            _ => unreachable!(),
        };
        print_json(
            &serde_json::json!({"protocol":wedl::CONTRACT,"result":result}),
            cli.compact,
        )?;
        return Ok(0);
    }
    if let Command::RevisionReview { file } = &cli.command {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "revision-review uses its three explicit commit pins, not --revision".into(),
            ));
        }
        let request = wedl::revision_review::decode(&read_input(file.clone())?)?;
        let result = wedl::revision_review::review(&cli.repo, &request)?;
        print_json(
            &serde_json::json!({"protocol":wedl::CONTRACT,"result":result}),
            cli.compact,
        )?;
        return Ok(0);
    }
    if let Command::Consequences(
        command @ (ConsequencesCommand::PublicationStatus | ConsequencesCommand::Recover { .. }),
    ) = &cli.command
    {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "publication status and recovery require the current worktree".into(),
            ));
        }
        let result = match command {
            ConsequencesCommand::PublicationStatus => wedl::publication::status(&cli.repo)?,
            ConsequencesCommand::Recover {
                expected_journal_sha256,
            } => wedl::publication::recover(&cli.repo, expected_journal_sha256)?,
            _ => unreachable!(),
        };
        print_json(
            &serde_json::json!({"protocol":wedl::CONTRACT,"result":result}),
            cli.compact,
        )?;
        return Ok(0);
    }
    if let Command::Init { path, title } = &cli.command {
        if cli.revision.is_some() {
            return Err(Error::InvalidInput(
                "init does not accept --revision".into(),
            ));
        }
        let title = title.as_deref().unwrap_or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled World")
        });
        let world = source::init_world(path, title)?;
        let result = Service::new(world).execute(Operation::Status)?;
        print_json(&result, cli.compact)?;
        return Ok(0);
    }
    let service = Service::new(source::load_world(&cli.repo, cli.revision.as_deref())?);
    let operation = match cli.command {
        Command::Completion { .. } | Command::Authoring(AuthoringCommand::Schema) => {
            unreachable!("static authoring helpers handled before source loading")
        }
        Command::Authoring(AuthoringCommand::Context { max_items }) => {
            Operation::AuthoringContext { max_items }
        }
        Command::Authoring(AuthoringCommand::Scaffold {
            family,
            entity,
            id,
            title,
            expected_fingerprint,
        }) => Operation::AuthoringScaffold {
            request: wedl::authoring_tools::ScaffoldRequest {
                expected_fingerprint: expected_fingerprint
                    .unwrap_or_else(|| service.world().fingerprint.clone()),
                family,
                entity,
                id,
                title,
            },
        },
        Command::Init { .. } => unreachable!("init handled before source loading"),
        Command::RevisionReview { .. } => {
            unreachable!("revision review handled before source loading")
        }
        Command::Reader(_) | Command::RevealPlans(_) | Command::ServeReader { .. } => unreachable!(
            "pinned reader and reveal operations handled before generic source loading"
        ),
        Command::Manuscripts(_) => {
            unreachable!("manuscripts handled before generic source loading")
        }
        Command::RetconReviews(
            RetconReviewCommand::Preview { .. }
            | RetconReviewCommand::Queue { .. }
            | RetconReviewCommand::Detail { .. },
        ) => unreachable!("retcon reads handled before source loading"),
        Command::RetconReviews(RetconReviewCommand::PreviewSave { file }) => {
            Operation::RetconReviewSavePreview {
                request: wedl::retcon_reviews::decode_save(&read_retcon_input(file)?)?,
            }
        }
        Command::RetconReviews(RetconReviewCommand::Save { file }) => Operation::RetconReviewSave {
            request: wedl::retcon_reviews::decode_save(&read_retcon_input(file)?)?,
        },
        Command::RetconImpact(RetconImpactCommand::Preview { file }) => {
            Operation::RetconImpactPreview {
                request: wedl::retcon_impact::decode_preview(&read_input(file)?)?,
            }
        }
        Command::EventConsequences { file } => Operation::EventConsequences {
            request: wedl::event_consequences::decode_event(&read_input(file)?)?,
        },
        Command::GroupedConsequences { file } => Operation::GroupedConsequences {
            request: wedl::event_consequences::decode_grouped(&read_input(file)?)?,
        },
        Command::Status => Operation::Status,
        Command::Validate => Operation::Validate,
        Command::Compile => Operation::Compile,
        Command::ActionDrafts(ActionDraftCommand::List) => Operation::ActionDraftList,
        Command::EditDrafts(EditDraftCommand::List) => Operation::EditDraftList,
        Command::EditDrafts(EditDraftCommand::Detail { id, sha256 }) => {
            Operation::EditDraftDetail {
                id,
                expected_sha256: sha256,
            }
        }
        Command::EditDrafts(EditDraftCommand::Preview { file }) => Operation::EditDraftPreview {
            request: wedl::edit_drafts::decode_save(&read_input(file)?)?,
        },
        Command::EditDrafts(EditDraftCommand::Save { file }) => Operation::EditDraftSave {
            request: wedl::edit_drafts::decode_save(&read_input(file)?)?,
        },
        Command::EditDrafts(EditDraftCommand::Review { file }) => Operation::EditDraftReview {
            request: wedl::edit_drafts::decode_review(&read_input(file)?)?,
        },
        Command::EditDrafts(EditDraftCommand::Apply { file }) => Operation::EditDraftApply {
            request: wedl::edit_drafts::decode_apply(&read_input(file)?)?,
        },
        Command::ActionDrafts(ActionDraftCommand::Detail { id, sha256 }) => {
            Operation::ActionDraftDetail {
                id,
                expected_sha256: sha256,
            }
        }
        Command::ActionDrafts(ActionDraftCommand::Preview { file }) => {
            Operation::ActionDraftPreview {
                request: wedl::action_drafts::decode_save(&read_input(file)?)?,
            }
        }
        Command::ActionDrafts(ActionDraftCommand::Save { file }) => Operation::ActionDraftSave {
            request: wedl::action_drafts::decode_save(&read_input(file)?)?,
        },
        Command::ActionDrafts(ActionDraftCommand::Review { file }) => {
            Operation::ActionDraftReview {
                request: wedl::action_drafts::decode_review(&read_input(file)?)?,
            }
        }
        Command::ActionDrafts(ActionDraftCommand::PreviewProposal { file }) => {
            Operation::ActionProposalPreview {
                request: wedl::action_store::decode_proposal_preview(&read_input(file)?)?,
            }
        }
        Command::ActionDrafts(ActionDraftCommand::ReviewOutcome { file }) => {
            Operation::ActionOutcomeReview {
                request: wedl::action_outcomes::decode_review(&read_input(file)?)?,
            }
        }
        Command::ActionDrafts(ActionDraftCommand::ApplyOutcome { file }) => {
            Operation::ActionOutcomeApply {
                request: wedl::action_outcomes::decode_apply(&read_input(file)?)?,
            }
        }
        Command::ActionDrafts(ActionDraftCommand::VerifyOutcome { file }) => {
            Operation::ActionOutcomeVerify {
                request: wedl::action_outcomes::decode_verify(&read_input(file)?)?,
            }
        }
        Command::Consequences(
            ConsequencesCommand::PublicationStatus | ConsequencesCommand::Recover { .. },
        ) => unreachable!("publication status and recovery handled before source loading"),
        Command::Consequences(ConsequencesCommand::Preview {
            file,
            max_characters,
        }) => Operation::ConsequencesPreview {
            plan: wedl::consequences::decode(&read_input(file)?)?,
            max_characters,
        },
        Command::Consequences(ConsequencesCommand::Check { file }) => {
            Operation::ConsequencesCheck {
                request: wedl::consequence_semantics::decode_check(&read_input(file)?)?,
            }
        }
        Command::ActionRequirements(ActionRequirementCommand::Capture {
            file,
            families,
            max_characters,
        }) => Operation::ActionRequirementCapture {
            frame: wedl::writing_frames::decode(&read_input(file)?)?,
            families: wedl::writing_frames::decode_families(&families)?,
            max_characters,
        },
        Command::ActionRequirements(ActionRequirementCommand::Compare {
            file,
            families,
            requirement,
            max_characters,
        }) => Operation::ActionRequirementCompare {
            frame: wedl::writing_frames::decode(&read_input(file)?)?,
            families: wedl::writing_frames::decode_families(&families)?,
            requirement: wedl::action_requirements::decode_requirement(&requirement)?,
            max_characters,
        },
        Command::Briefs(BriefCommand::Compose {
            brief,
            scope,
            limit,
        }) => Operation::BriefCompose {
            brief,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Writing(WritingCommand::Resolve { file }) => Operation::WritingResolve {
            frame: wedl::writing_frames::decode(&read_input(file)?)?,
        },
        Command::Hypotheses(HypothesisCommand::List {
            status,
            text,
            max_items,
            max_characters,
        }) => Operation::HypothesisList {
            status,
            text,
            max_items,
            max_characters,
        },
        Command::Hypotheses(HypothesisCommand::Detail {
            hypothesis_id,
            max_characters,
        }) => Operation::HypothesisDetail {
            hypothesis_id,
            max_characters,
        },
        Command::Hypotheses(HypothesisCommand::Preview {
            file,
            max_characters,
        }) => Operation::HypothesisPreview {
            request: wedl::hypotheses::decode(&read_bounded_input(
                file,
                wedl::hypotheses::MAX_REQUEST_BYTES,
                "hypothesis request exceeds 262144 encoded bytes",
            )?)?,
            max_characters,
        },
        Command::CurrentTime(CurrentTimeCommand::Preview {
            file,
            max_characters,
        }) => Operation::CurrentTimePreview {
            request: wedl::current_time::decode(&read_bounded_input(
                file,
                16384,
                "current-time request exceeds 16 KiB",
            )?)?,
            max_characters,
        },
        Command::SceneControl(SceneControlCommand::Preview {
            file,
            max_characters,
        }) => Operation::SceneControlPreview {
            request: wedl::scene_control::decode(&read_bounded_input(
                file,
                wedl::scene_control::MAX_REQUEST_BYTES,
                "scene-control request exceeds its complete encoded byte bound",
            )?)?,
            max_characters,
        },
        Command::ConversationControl(ConversationControlCommand::Preview {
            file,
            max_characters,
        }) => Operation::ConversationControlPreview {
            request: wedl::conversation_control::decode(&read_bounded_input(
                file,
                wedl::conversation_control::MAX_REQUEST_BYTES,
                "conversation-control request exceeds its complete encoded byte bound",
            )?)?,
            max_characters,
        },
        Command::Writing(WritingCommand::Choices { file, limit }) => Operation::WritingChoices {
            frame: wedl::writing_frames::decode(&read_input(file)?)?,
            limit,
        },
        Command::Writing(WritingCommand::Context {
            file,
            briefs,
            families,
            max_characters,
        }) => {
            let frame = wedl::writing_frames::decode(&read_input(file)?)?;
            if let Some(families) = families {
                Operation::WritingFamilyContext {
                    frame,
                    briefs,
                    families: wedl::writing_frames::decode_families(&families)?,
                    max_characters,
                }
            } else {
                Operation::WritingContext {
                    frame,
                    briefs,
                    max_characters,
                }
            }
        }
        Command::Catalogue(command) => {
            let (name, file) = match command {
                CatalogueCommand::Page { file } => ("page", file),
                CatalogueCommand::Labels { file } => ("labels", file),
            };
            let request = wedl::catalogue::decode(&read_bounded_input(
                file,
                wedl::catalogue::MAX_REQUEST_BYTES,
                "catalogue request exceeds 16 KiB",
            )?)?;
            if request.operation() != name {
                return Err(Error::InvalidInput(
                    "catalogue request operation does not match the command".into(),
                ));
            }
            Operation::Catalogue { request }
        }
        Command::Entity(EntityCommand::List { kind, text }) => Operation::EntityList { kind, text },
        Command::Entity(EntityCommand::Show { entity }) => Operation::EntityShow { entity },
        Command::Generational(GenerationalCommand::Learned { viewer, command }) => {
            learned_operation(viewer, command)
        }
        Command::Generational(GenerationalCommand::Labels(args)) => Operation::GenerationalLabels {
            viewer: None,
            ids: args.ids,
            at: args.at.into(),
            limit: args.limit,
        },
        Command::Generational(GenerationalCommand::Discover(args)) => {
            Operation::GenerationalDiscover {
                viewer: None,
                kind: args.kind,
                prefix: args.prefix,
                at: args.at.into(),
                limit: args.limit,
            }
        }
        Command::Generational(GenerationalCommand::Search(args)) => Operation::GenerationalSearch {
            viewer: None,
            text: args.text,
            at: args.at.into(),
            limit: args.limit,
        },
        Command::Generational(GenerationalCommand::Context(args)) => {
            Operation::GenerationalContext {
                viewer: None,
                subject: args.subject,
                at: args.at.into(),
                max_depth: args.max_depth,
                limit: args.limit,
                max_chars: args.max_chars,
            }
        }
        Command::Generational(GenerationalCommand::Family {
            character,
            at,
            limit,
        }) => Operation::GenerationalFamily {
            character,
            at: at.into(),
            limit,
        },
        Command::Generational(GenerationalCommand::Ancestors(args)) => {
            Operation::GenerationalLineage {
                character: args.character,
                at: args.at.into(),
                direction: LineageDirection::Ancestors,
                max_depth: args.max_depth,
                limit: args.limit,
            }
        }
        Command::Generational(GenerationalCommand::Descendants(args)) => {
            Operation::GenerationalLineage {
                character: args.character,
                at: args.at.into(),
                direction: LineageDirection::Descendants,
                max_depth: args.max_depth,
                limit: args.limit,
            }
        }
        Command::Generational(GenerationalCommand::Path {
            from,
            to,
            at,
            max_hops,
            limit,
        }) => Operation::GenerationalPath {
            from,
            to,
            at: at.into(),
            max_hops,
            limit,
        },
        Command::Generational(GenerationalCommand::History { record, at, limit }) => {
            Operation::GenerationalHistory {
                record,
                at: at.into(),
                limit,
            }
        }
        Command::Generational(GenerationalCommand::Organization {
            organization,
            at,
            max_depth,
            include_former,
            limit,
        }) => Operation::GenerationalOrganization {
            organization,
            at: at.into(),
            max_depth,
            include_former,
            limit,
        },
        Command::Generational(GenerationalCommand::Legacy { legacy, at, limit }) => {
            Operation::GenerationalLegacy {
                legacy,
                at: at.into(),
                limit,
            }
        }
        Command::Generational(GenerationalCommand::OrganizationLegacies {
            organization,
            at,
            limit,
        }) => Operation::GenerationalOrganizationLegacies {
            organization,
            at: at.into(),
            limit,
        },
        Command::Generational(GenerationalCommand::Vital {
            character,
            at,
            limit,
        }) => Operation::GenerationalVital {
            character,
            at: at.into(),
            limit,
        },
        Command::Descriptions(DescriptionCommand::List {
            owner,
            scope,
            limit,
        }) => Operation::DescriptionList {
            owner,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Descriptions(DescriptionCommand::Section {
            owner,
            section,
            scope,
        }) => Operation::DescriptionSection {
            owner,
            section,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
        },
        Command::Assertions(AssertionCommand::Predicates { limit }) => {
            Operation::AssertionPredicates { limit }
        }
        Command::Assertions(AssertionCommand::List {
            target,
            scope,
            limit,
        }) => Operation::AssertionList {
            target,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Existence(ExistenceCommand::List {
            character,
            scope,
            limit,
        }) => Operation::ExistenceList {
            character,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Governance(GovernanceCommand::Query {
            target,
            profile,
            roles,
            scope,
            limit,
        }) => Operation::GovernanceQuery {
            target,
            binding: wedl::service::governance_binding_json(&profile, &roles)?,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Social(SocialCommand::Context {
            language,
            sense,
            form,
            contexts,
            scope,
        }) => Operation::SocialContext {
            selected: wedl::social::Selection {
                language,
                sense,
                form,
                contexts,
            },
            at: scope.at.into(),
            audience: scope.audience,
            view: scope.view,
        },
        Command::Environment(EnvironmentCommand::Context { contexts, scope }) => {
            Operation::EnvironmentContext {
                roots: contexts,
                at: scope.at.into(),
                audience: scope.audience,
                view: scope.view,
            }
        }
        Command::Lore(LoreCommand::Profiles { limit }) => Operation::LoreProfiles { limit },
        Command::Lore(LoreCommand::Profile { profile }) => Operation::LoreProfile { profile },
        Command::Lore(LoreCommand::Catalog {
            scope,
            domain,
            limit,
        }) => Operation::LoreCatalog {
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            domain,
            limit,
        },
        Command::Lore(LoreCommand::Concept { concept, scope }) => Operation::LoreConcept {
            concept,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
        },
        Command::Lore(LoreCommand::Links {
            record,
            scope,
            limit,
        }) => Operation::LoreLinks {
            record,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Lore(LoreCommand::State {
            record,
            at,
            audience,
            view,
        }) => Operation::LoreState {
            record,
            at: at.into(),
            audience,
            view,
        },
        Command::Species(SpeciesCommand::Compose {
            individual,
            selection,
            scope,
        }) => Operation::SpeciesCompose {
            individual,
            selection: selection.input()?,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
        },
        Command::Species(SpeciesCommand::Affected {
            common,
            selection,
            scope,
            limit,
        }) => Operation::SpeciesAffected {
            common,
            selection: selection.input()?,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
            limit,
        },
        Command::Magic(MagicCommand::Compose {
            root,
            profiles,
            links,
            restrictions,
            exceptions,
            scope,
        }) => Operation::MagicCompose {
            root,
            selection: wedl::service::magic_selection_json(
                &profiles,
                links.as_deref(),
                restrictions.as_deref(),
                exceptions.as_deref(),
            )?,
            at: scope.at.input(),
            audience: scope.audience,
            view: scope.view,
        },
        Command::Spatial(SpatialCommand::Location {
            entity,
            mode,
            limit,
        }) => Operation::SpatialLocation {
            entity,
            mode,
            limit,
        },
        Command::Spatial(SpatialCommand::History { record, at, limit }) => {
            Operation::SpatialHistory {
                record,
                at: at.input(),
                limit,
            }
        }
        Command::Spatial(SpatialCommand::Route { route, at }) => Operation::SpatialRoute {
            route,
            at: at.input(),
        },
        Command::Spatial(SpatialCommand::Path {
            from,
            to,
            mode,
            metric,
            unit,
            limit,
        }) => Operation::SpatialPath {
            from,
            to,
            mode,
            metric,
            unit,
            limit,
        },
        Command::Spatial(SpatialCommand::Maps { limit }) => Operation::SpatialMaps { limit },
        Command::Spatial(SpatialCommand::Anchors {
            from_map,
            to_map,
            limit,
        }) => Operation::SpatialAnchors {
            from_map,
            to_map,
            limit,
        },
        Command::Spatial(SpatialCommand::Reachable { from, mode, limit }) => {
            Operation::SpatialReachable { from, mode, limit }
        }
        Command::Spatial(SpatialCommand::Overlays {
            location,
            at,
            audience,
            view,
            overlay,
            limit,
        }) => Operation::SpatialOverlays {
            location,
            at: at.into(),
            audience,
            view,
            overlay,
            limit,
        },
        Command::Spatial(SpatialCommand::Bbox {
            map,
            min_x,
            min_y,
            max_x,
            max_y,
            min_z,
            max_z,
            relation,
            at,
            limit,
        }) => {
            let min = [Some(min_x), Some(min_y), min_z]
                .into_iter()
                .flatten()
                .collect();
            let max = [Some(max_x), Some(max_y), max_z]
                .into_iter()
                .flatten()
                .collect();
            match at.input() {
                Some(at) => Operation::SpatialBboxAt {
                    map,
                    min,
                    max,
                    relation,
                    at,
                    limit,
                },
                None => Operation::SpatialBbox {
                    map,
                    min,
                    max,
                    relation,
                    limit,
                },
            }
        }
        Command::Spatial(SpatialCommand::Nearby {
            map,
            x,
            y,
            z,
            radius,
            at,
            limit,
        }) => {
            let point = [Some(x), Some(y), z].into_iter().flatten().collect();
            match at.input() {
                Some(at) => Operation::SpatialNearbyAt {
                    map,
                    point,
                    radius,
                    at,
                    limit,
                },
                None => Operation::SpatialNearby {
                    map,
                    point,
                    radius,
                    limit,
                },
            }
        }
        Command::Chronology(ChronologyCommand::Format { date }) => Operation::ChronologyFormat {
            value: date.input()?,
        },
        Command::Chronology(ChronologyCommand::ConvertDate {
            date,
            target_calendar,
            target_era,
        }) => {
            let target = match (target_calendar, target_era) {
                (Some(calendar), None) => DateTarget::Calendar(calendar),
                (None, Some(era)) => DateTarget::Era(era),
                _ => {
                    return Err(Error::InvalidInput(
                        "choose one target calendar or era".into(),
                    ));
                }
            };
            Operation::ChronologyConvertDate {
                value: date.input()?,
                target,
            }
        }
        Command::Chronology(ChronologyCommand::Search {
            predicate,
            date,
            upper,
            record,
            calendar_filter,
            era_filter,
            era_mode,
            limit,
        }) => {
            let value = date.input()?;
            let upper = upper.input(&value)?;
            Operation::ChronologySearch {
                query: AnnotationQuery {
                    predicate,
                    value,
                    upper,
                    record,
                    calendar: calendar_filter,
                    era: era_filter,
                    era_mode: era_mode.unwrap_or(EraMatchMode::Authored),
                    limit,
                },
            }
        }
        Command::Chronology(ChronologyCommand::StoryTimes {
            date,
            upper,
            timeline,
            limit,
        }) => {
            let value = date.input()?;
            let upper = upper.input(&value)?;
            Operation::ChronologyStoryTimes {
                value,
                upper,
                timeline,
                limit,
            }
        }
        Command::Chronology(ChronologyCommand::Annotations {
            record,
            calendar,
            era,
            limit,
        }) => Operation::ChronologyAnnotations {
            record,
            calendar,
            era,
            limit,
        },
        Command::Chronology(ChronologyCommand::Compare {
            left_record,
            left_id,
            right_record,
            right_id,
        }) => Operation::ChronologyCompare {
            left_record,
            left_id,
            right_record,
            right_id,
        },
        Command::Chronology(ChronologyCommand::Calendars { limit }) => {
            Operation::ChronologyCalendars { limit }
        }
        Command::Chronology(ChronologyCommand::Date {
            calendar,
            date,
            era,
        }) => Operation::ChronologyDate {
            calendar,
            value: date.into(),
            era,
        },
        Command::Chronology(ChronologyCommand::Convert { from, to, date }) => {
            Operation::ChronologyConvert {
                from,
                to,
                value: date.into(),
            }
        }
        Command::Chronology(ChronologyCommand::ToStory {
            calendar,
            date,
            timeline,
        }) => Operation::ChronologyToStory {
            calendar,
            value: date.into(),
            timeline,
        },
        Command::Chronology(ChronologyCommand::FromStory {
            calendar,
            tick,
            order,
            timeline,
        }) => Operation::ChronologyFromStory {
            calendar,
            at: TimeInput {
                tick,
                order,
                timeline,
            },
        },
        Command::Scene { entity, at } => Operation::Scene {
            entity,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Species {
            character,
            individual,
            selection,
            belief_targets,
            at,
        }) => Operation::CharacterSpeciesCompose {
            character,
            individual,
            selection: selection.input()?,
            belief_targets,
            at: at.into(),
        },
        Command::Character(CharacterCommand::SpeciesAffected {
            character,
            common,
            selection,
            at,
            limit,
        }) => Operation::CharacterSpeciesAffected {
            character,
            common,
            selection: selection.input()?,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Magic {
            character,
            belief_targets,
            at,
        }) => Operation::CharacterMagic {
            character,
            belief_targets,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Social {
            character,
            language,
            sense,
            form,
            contexts,
            belief_targets,
            at,
        }) => Operation::CharacterSocialContext {
            character,
            selected: wedl::social::Selection {
                language,
                sense,
                form,
                contexts,
            },
            belief_targets,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Environment {
            character,
            contexts,
            belief_targets,
            at,
        }) => Operation::CharacterEnvironmentContext {
            character,
            roots: contexts,
            belief_targets,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Brief {
            character,
            brief,
            at,
            limit,
        }) => Operation::CharacterBrief {
            character,
            brief,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Context {
            character,
            at,
            max_characters,
        }) => Operation::CharacterContext {
            character,
            at: at.into(),
            max_characters,
        },
        Command::Character(CharacterCommand::Search {
            character,
            query,
            mode,
            at,
            limit,
        }) => match mode {
            Some(mode) => Operation::CharacterSearchWithMode {
                character,
                query,
                at: at.into(),
                limit,
                mode,
            },
            None => Operation::CharacterSearch {
                character,
                query,
                at: at.into(),
                limit,
            },
        },
        Command::Character(CharacterCommand::Conversation {
            character,
            conversation,
            at,
        }) => Operation::CharacterConversation {
            character,
            conversation,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Assertions {
            character,
            target,
            at,
            limit,
        }) => Operation::CharacterAssertions {
            character,
            target,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Descriptions {
            character,
            owner,
            at,
            limit,
        }) => Operation::CharacterDescriptions {
            character,
            owner,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Section {
            character,
            owner,
            section,
            at,
        }) => Operation::CharacterSection {
            character,
            owner,
            section,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Lore {
            character,
            domain,
            at,
            limit,
        }) => Operation::CharacterLore {
            character,
            domain,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Concept {
            character,
            target,
            at,
        }) => Operation::CharacterConcept {
            character,
            target,
            at: at.into(),
        },
        Command::Character(CharacterCommand::Links {
            character,
            owner,
            at,
            limit,
        }) => Operation::CharacterLinks {
            character,
            owner,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::SpatialHistory {
            character,
            owner,
            at,
            limit,
        }) => Operation::CharacterSpatialHistory {
            character,
            owner,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Existence {
            character,
            target,
            at,
            limit,
        }) => Operation::CharacterExistence {
            character,
            target,
            at: at.into(),
            limit,
        },
        Command::Character(CharacterCommand::Governance {
            character,
            target,
            at,
            limit,
        }) => Operation::CharacterGovernance {
            character,
            target,
            at: at.into(),
            limit,
        },
        Command::Record(RecordCommand::Create { request }) => {
            let draft = read_input(request.file)?;
            if request.apply {
                Operation::ApplyRecordCreate {
                    draft,
                    expected_fingerprint: request.expected_fingerprint,
                    expected_after_sha256: request.expected_after_sha256.ok_or_else(|| {
                        Error::InvalidInput("--apply requires --expected-after-sha256".into())
                    })?,
                }
            } else {
                Operation::PreviewRecordCreate {
                    draft,
                    expected_fingerprint: request.expected_fingerprint,
                }
            }
        }
        Command::Record(RecordCommand::Update { entity, request }) => {
            let patch = read_input(request.file)?;
            if request.apply {
                Operation::ApplyRecordUpdate {
                    entity,
                    patch,
                    expected_fingerprint: request.expected_fingerprint,
                    expected_after_sha256: request.expected_after_sha256.ok_or_else(|| {
                        Error::InvalidInput("--apply requires --expected-after-sha256".into())
                    })?,
                }
            } else {
                Operation::PreviewRecordUpdate {
                    entity,
                    patch,
                    expected_fingerprint: request.expected_fingerprint,
                }
            }
        }
        Command::State { entity, at } => Operation::State {
            entity,
            at: at.into(),
        },
        Command::Knowledge {
            entity,
            at,
            include_forgotten,
        } => Operation::Knowledge {
            entity,
            at: at.into(),
            include_forgotten,
        },
        Command::Relationships { entity, at } => Operation::Relationships {
            entity,
            at: at.into(),
        },
        Command::StoryPoints { at, scene, limit } => Operation::StoryPoints {
            at: at.into(),
            scene,
            limit,
        },
        Command::StoryPoint {
            id,
            at,
            scene,
            limit,
        } => Operation::StoryPointTrail {
            story_point: id,
            at: at.into(),
            scene,
            limit,
        },
        Command::Causal {
            event,
            at,
            direction,
            limit,
        } => Operation::Causal {
            event,
            at: at.into(),
            direction,
            limit,
        },
        Command::Whereabouts {
            at,
            mut characters,
            limit,
        } => {
            characters.sort();
            Operation::Whereabouts {
                at: at.into(),
                characters: (!characters.is_empty()).then_some(characters),
                limit,
            }
        }
        Command::Interactions {
            first,
            second,
            at,
            limit,
        } => Operation::Interactions {
            first,
            second,
            at: at.into(),
            limit,
        },
        Command::ThreadMemberships {
            mut record_ids,
            mut thread_ids,
            at,
            limit,
        } => {
            record_ids.sort();
            thread_ids.sort();
            Operation::ThreadMemberships {
                record_ids,
                thread_ids,
                at: at.input(),
                limit,
            }
        }
        Command::Environments { entity, at } => Operation::Environments {
            entity,
            at: at.into(),
        },
        Command::Timeline {
            timeline,
            tick,
            order,
            limit,
        } => Operation::Timeline {
            at: tick.map(|tick| TimeInput {
                tick,
                order: order.unwrap_or(i32::MAX),
                timeline: timeline.clone(),
            }),
            timeline,
            limit,
        },
        Command::Conversation { entity, at } => Operation::Conversation {
            entity,
            at: at.into(),
        },
        Command::Context {
            entity,
            at,
            max_characters,
        } => Operation::Context {
            entity,
            at: at.into(),
            max_characters,
        },
        Command::Search {
            query,
            mode,
            limit,
            kind,
            at,
            mut thread_ids,
            include_hypotheses,
        } => {
            let at = at.input();
            thread_ids.sort();
            let thread_ids = (!thread_ids.is_empty()).then_some(thread_ids);
            if include_hypotheses {
                Operation::SearchWithHypotheses {
                    query,
                    limit,
                    kind,
                    mode: mode.unwrap_or(wedl::retrieval::Mode::Lexical),
                    at,
                    thread_ids,
                }
            } else if at.is_some() || thread_ids.is_some() {
                Operation::SearchWithSelection {
                    query,
                    limit,
                    kind,
                    mode: mode.unwrap_or(wedl::retrieval::Mode::Lexical),
                    at,
                    thread_ids,
                }
            } else {
                match mode {
                    Some(mode) => Operation::SearchWithMode {
                        query,
                        limit,
                        kind,
                        mode,
                    },
                    None => Operation::Search { query, limit, kind },
                }
            }
        }
        Command::Threads => Operation::Threads,
        Command::PreviewBody {
            entity,
            file,
            expected_fingerprint,
        } => Operation::PreviewBody {
            entity,
            body: read_input(file)?,
            expected_fingerprint,
        },
        Command::ApplyBody {
            entity,
            file,
            expected_fingerprint,
            expected_after_sha256,
        } => Operation::ApplyBody {
            entity,
            body: read_input(file)?,
            expected_fingerprint,
            expected_after_sha256,
        },
        Command::Serve {
            host,
            port,
            reader_simulation_policy,
        } => {
            if let Some(path) = reader_simulation_policy {
                service.reader().install_simulation(Some(
                    wedl::reader_config::decode_simulation(
                        &cli.repo,
                        &wedl::reader_config::read(&path)?,
                    )?,
                ))?;
            }
            http::serve(service, &host, port).await?;
            return Ok(0);
        }
    };
    let validation = matches!(operation, Operation::Validate);
    let result = service.execute(operation)?;
    let exit_code = if validation && result["result"]["valid"] == false {
        2
    } else {
        0
    };
    print_json(&result, cli.compact)?;
    Ok(exit_code)
}

fn read_input(file: PathBuf) -> Result<String> {
    read_bounded_input(file, 1_048_576, "input exceeds 1 MiB")
}

fn read_manuscript_input(file: PathBuf) -> Result<String> {
    read_bounded_input(file, 8 * 1_048_576, "manuscript input exceeds 8 MiB")
}

fn read_retcon_input(file: PathBuf) -> Result<String> {
    read_bounded_input(file, 8 * 1_048_576, "retcon input exceeds 8 MiB")
}

fn read_bounded_input(file: PathBuf, maximum: usize, message: &str) -> Result<String> {
    let mut body = String::new();
    let limit = maximum as u64 + 1;
    if file.as_os_str() == "-" {
        std::io::stdin().take(limit).read_to_string(&mut body)?;
    } else {
        std::fs::File::open(file)?
            .take(limit)
            .read_to_string(&mut body)?;
    }
    if body.len() > maximum {
        return Err(Error::InvalidInput(message.into()));
    }
    Ok(body)
}

fn print_json(value: &serde_json::Value, compact: bool) -> Result<()> {
    let output = if compact {
        serde_json::to_string(value)?
    } else {
        serde_json::to_string_pretty(value)?
    };
    println!("{output}");
    Ok(())
}
