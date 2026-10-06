+++
schema = "adrai/decision/v1"
adr = "A01M48XXBREJY497SNXZVCW85EC"
record = "R01M48XXBY8T83QFCD3FWHTG6J8"
title = "Chronology validation and exact diagnostic catalog"
summary = "Migrated CHRONOLOGY_VALIDATION.md; current implementation boundaries and original contract semantics."
domains = ["chronology"]
+++

## Authority and provenance

Migrated from `docs/CHRONOLOGY_VALIDATION.md` at Git revision `daf78b54ebfa40a7e9757bb7cdb1179db779b7b3`. This records the existing contract and corrects current implementation descriptions; it does not create a historical approval or replace the authority of the original calendar chronology decision A01M48NNH43QCFRPC68NQQTFT63. Earlier approval-stage wording and immutable measured evidence retain their historical meaning.

# Chronology validation (`wedl/v0.6` candidates)

`upgrade-v06` validates pinned legacy input and the complete v0.6 candidate
before any backup ref, commit, receipt, or cache rebuild. Calendar conversion
requires declared anchors; StoryTime never supplies duration.

`validate_v06_candidate(world)` is a pre-commit source-boundary validator. It
accepts only homogeneous v0.6 candidate records. Valid candidates load and
compile into the internal disposable chronology read model; public chronology
CLI/HTTP reads and `chronology.replace` authoring are active, and `upgrade-v06`
is the local confirmed migration route. The UI presents that capability but
does not migrate source. It returns deterministic error diagnostics
with the owning source path, entity ID, and exact frontmatter field.

Validation is ordered as declaration envelope, calendars, eras, anchors, then
non-world annotations. A failed prerequisite gates dependent diagnostics.
Chronology references are owned here rather than by generic reference scanning;
uncertainty, conflicts, null epochs, open ranges, duplicate era aliases, and
`x-*` extensions are valid where the schema permits them. Table calendars
require one through 500 explicit signed-year rows.

Chronology is valid only at the documented world and record-level locations.
The validator recursively rejects a descendant `chronology` key in operational
objects, including descendants carried inside opaque `x-*` extensions, and
reports that exact descendant field rather than collapsing it to `chronology`.
Semantic kernel failures are replayed only to select the authored leaf for the
published diagnostic: calendar mechanics/epoch coordinates, era display/bounds
coordinates, and date/range/approximation endpoints retain their source
provenance. The kernel remains the semantic authority. Source IDs that collapse
to the same canonical kernel ID (including case variants) are rejected at the
second authored `.id` before definitions or maps are constructed.

Leaf attribution evaluates the selected signed-year layout. Cycle rules use the
kernel's exact `divmod` quotient and residue: checked `quotient * cycle_days`,
then the selected cycle prefix, then the month prefix and final day offset.
The cycle-prefix operation is one checked sum, so a transient wide-integer
product may be accepted when its selected residue prefix returns it to signed
i64 range. This matches the kernel for both positive and negative boundaries.
Table rules use only an exactly matching signed row and never aggregate
neighbouring rows. Consequently a table gap reports the year, an absent
effective month reports the month, and an overlong effective day reports the
day. Full calendar preparation completes before era construction or endpoint
inspection, so a calendar diagnostic always gates a dependent era diagnostic.
Epoch mapping failures that remain after a valid selected civil coordinate report
`axis_day`. Era display-to-machine conversion checks subtraction and addition as
separate i64 stages and retains the annotation `year` leaf. Era bounds check
lower then upper endpoint layouts; a reversed pair reports `bounds.lower.year`.

The schema vector binds every negative-document category to its complete
ordered diagnostic list, including code, severity, message, entity ID, source
path, and field. The executable conformance tests compare those lists exactly;
the raw YAML and Markdown registry rows are first checked for equal counts and
duplicates before either is compared with the production registry. Definition replay reports the individual cycle/table
operand, epoch coordinate, era display or bounds coordinate, and date/range or
approximation endpoint rather than a reconstructed enclosing object.

Every non-null range and approximation endpoint is validated as a complete
selected-year civil coordinate before the pair is compared. Era-bound endpoints
are deliberately local: they stop after signed-year layout, month/day, and
checked ordinal arithmetic, and never require an epoch or shared-axis mapping.
Evaluation is lower then upper: shape/calendar agreement, signed-year layout,
month/day, checked ordinal arithmetic, and (for claims) the shared-axis epoch
offset all precede a lower-before-upper ordering check. `NO_EPOCH` remains a
valid local-calendar result. Era display conversion reports an unrepresentable machine
year before inspecting its month or day, then validates the resulting civil
endpoint by the same complete path.
Thus an invalid `lower.month` wins over a simultaneous reversed interval. The
same replay keeps overflow attribution on the operand that supplied it: civil
ordinal start/final stages use the authored precision (`year`, `month`, or
`day`), epoch-offset subtraction uses `epoch.axis_day`, era display-to-machine
arithmetic uses the era `year`, and era-bound ordinal stages use the exact
`bounds.lower` or `bounds.upper` coordinate.

## Diagnostic catalog

This table is the complete production wire registry. Every row is an exact
`code`, `severity`, and `message` tuple; kernel exceptions are translated to
the stable semantic rows below. The YAML catalog is deliberately identical.

| Code | Severity | Message |
| --- | --- | --- |
| WDL-ANCHOR-001 | error | anchor IDs must be unique and ascending |
| WDL-ANCHOR-001 | error | anchors must be an array of at most 500 |
| WDL-ANCHOR-001 | error | invalid anchor definition |
| WDL-ANCHOR-002 | error | axis day must be signed i64 |
| WDL-ANCHOR-003 | error | anchor StoryTime must be closed and use a declared timeline |
| WDL-ANCHOR-004 | error | anchor provenance must be an array of strings |
| WDL-ANCHOR-007 | error | anchors must map monotonically between StoryTime and axis day |
| WDL-CAL-001 | error | calendar IDs must be unique and ascending |
| WDL-CAL-001 | error | calendar must have a valid closed definition |
| WDL-CAL-002 | error | calendar months must contain 1..64 rows |
| WDL-CAL-002 | error | invalid calendar month |
| WDL-CAL-002 | error | month numbers must be unique and ascending |
| WDL-CAL-003 | error | calendar rule kind must be cycle or table |
| WDL-CAL-004 | error | invalid cycle rule |
| WDL-CAL-005 | error | invalid cycle override |
| WDL-CAL-006 | error | overrides must be unique and ordered |
| WDL-CAL-007 | error | invalid table year row |
| WDL-CAL-007 | error | table years must be unique and ascending |
| WDL-CAL-007 | error | table years must contain 1..500 rows |
| WDL-CAL-008 | error | invalid table override |
| WDL-CAL-008 | error | table overrides must be unique and ordered |
| WDL-CAL-009 | error | calendar mechanics exceed 500 rows |
| WDL-CAL-010 | error | invalid calendar epoch |
| WDL-CAL-011 | error | calendar definition is semantically invalid |
| WDL-CHRON-001 | error | chronology calendars must be an array of at most 500 |
| WDL-CHRON-001 | error | world chronology must be a closed declaration |
| WDL-CHRON-002 | error | non-world chronology must be an array of at most 500 |
| WDL-CHRON-003 | error | chronology is forbidden in nested operational objects |
| WDL-CHRON-004 | error | invalid chronology annotation envelope |
| WDL-CHRON-005 | error | annotation IDs must be unique per record |
| WDL-CHRON-007 | error | annotation value must contain exactly one tag |
| WDL-CHRON-007 | error | unknown annotation value tag |
| WDL-DATE-001 | error | civil date must have a known calendar and valid precision |
| WDL-DATE-001 | error | civil endpoint must be exact |
| WDL-DATE-001 | error | invalid civil precision |
| WDL-DATE-002 | error | calendar date is unavailable for the declared year |
| WDL-DATE-003 | error | civil date is semantically invalid |
| WDL-DATE-003 | error | invalid civil range |
| WDL-DATE-003 | error | range endpoint calendar differs |
| WDL-DATE-003 | error | range is reversed |
| WDL-DATE-004 | error | approximation bounds must share an ordered calendar |
| WDL-DATE-004 | error | invalid approximation |
| WDL-DATE-005 | error | conflict requires 2..64 claims within depth 64 |
| WDL-DATE-006 | error | relative date must use declared source-record references |
| WDL-DATE-007 | error | invalid display-only duration |
| WDL-ERA-001 | error | era IDs must be unique and ascending |
| WDL-ERA-001 | error | era must have a valid closed definition |
| WDL-ERA-001 | error | eras must be an array of at most 500 |
| WDL-ERA-002 | error | era calendar is unknown |
| WDL-ERA-003 | error | invalid era display metadata |
| WDL-ERA-004 | error | invalid era display epoch |
| WDL-ERA-005 | error | era bounds must be exact civil endpoints |
| WDL-ERA-005 | error | era definition is semantically invalid |
| WDL-ERA-005 | error | invalid era bounds |
| WDL-ERA-006 | error | invalid era annotation |
| WDL-ERA-006 | error | invalid era precision |
| WDL-ERA-008 | error | era date is semantically invalid |
| WDL-SRC-001 | error | v0.6 candidate must be homogeneous |
| WDL-SRC-008 | error | v0.6 candidate must be homogeneous |
| WDL-WORLD-001 | error | world must contain exactly one world record |

## Current source and delivery boundary

The generic loader, validator, and compiler accept homogeneous `wedl/v0.7` worlds. Their chronology projection accepts validated v0.6 and v0.7 declarations; legacy v0.3/v0.5 worlds have no chronology projection. This extends compiled read-model coverage without rewriting the v0.6 grammar or the local `upgrade-v06` input policy.

The existing public adapter has a narrower advertised capability: `chronology_capability` enables calendar chronology only for v0.6, and the catalogue returns empty definitions for v0.7. `chronology.replace` also requires v0.6 and rejects v0.7 before making a changeset. In contrast, format, convert, search, and story-times obtain the compiled chronology store without that capability gate, so a valid v0.7 projection can be evaluated by those operations. This asymmetry is the current implementation boundary, not a new uniform public v0.7 admission policy. Source references: `src/wedl/chronology_api.py`, `src/wedl/chronology_index.py`, and `src/wedl/authoring.py`.

Executable contract vectors: [chronology-validation-v06.yaml](../../examples/chronology-validation-v06.yaml). Their semantic cases are preserved unchanged.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiZDIxMzZkMGE2MDRmZDZmZGEzYTIwZGQzOTIyOTljYjE0NDBhMjYzMyIsImkiOiJzaGEyNTY6c2JBWXkwX3FWX25Td3BSeXhqcEFHRFdRazFSLXlCQU5DQktGbmlRU3p0dyIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WFhCWThUODNRRkNEM0ZXSFRHNko4Iiwib3AiOiJPMDFNNDhYWEJZOFQ4M1FGQ0QzRldIVEc2SjgiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1Njp0N1NoQzJCcU82dGhvT1d1Z1BvLWpPVzdOUlR3NFA2bnVNVVdud0V4b1pFIiwidCI6MTc5MTMwMTE2OTA5NiwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
