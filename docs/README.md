# WEDL documentation

The [README](../README.md) covers installation and the basic ideas. Use the
guides below to create a world, read and edit its records, and explore the
packaged stories. This manual also links to historical reports. Read and write
architecture through ADRAI against the WEDL development/source checkout.

## Usage guides

- [Command line, completion and preview/apply](guides/command-line.md)
- [Local HTTP API and browser sessions](guides/http-api.md)
- [Migration and recovery](guides/migration-and-recovery.md)
- [Testing and runner usage](guides/testing.md)
- [Chronology](guides/chronology.md)
- [Generational reads and authoring](guides/generational.md)
- [Search profiles](guides/search-profiles.md)
- [Context writing packets](guides/context-briefs.md)
- [Conversations and recollections](guides/conversations.md)
- [Consequence preview](guides/event-consequence-preview.md)
- [Consequence reads](guides/event-consequences-reader.md)

## Packaged stories

Tideglass is a compact 64-record native v0.7 repair story with authored spatial
and generational facts. Start with its [world guide](stories/tideglass/world-guide.md),
[walkthrough](stories/tideglass/walkthrough.md) and
[query recipes](stories/tideglass/queries.md).

The maintained Ash Archive is a completed 262-record story ending at tick 208.
Its [walkthrough](stories/ash-archive/walkthrough.md) and
[novella](stories/ash-archive/novella.md) accompany
[authoring notes](stories/ash-archive/authoring-notes.md).
Earlier material remains explicitly historical: the
[unresolved walkthrough](stories/ash-archive/historical-unresolved-walkthrough.md),
[seed overview](stories/ash-archive/seed-overview.md),
[119-ID seed manifest](stories/ash-archive/seed-manifest.md),
[seed story guide](stories/ash-archive/seed-story-guide.md) and
[seed changeset illustration](stories/ash-archive/seed-changeset.md).

The maintained Frontiersmen package has 309 records, 19 conversations and
248 turns, with Southward Cut active at main 210:0. Use its
[world guide](stories/frontiersmen/world-guide.md),
[walkthrough](stories/frontiersmen/walkthrough.md),
[queries](stories/frontiersmen/queries.md) and
[authoring notes](stories/frontiersmen/authoring-notes.md).
The [chronicle through pursuit](stories/frontiersmen/chronicle-through-pursuit.md)
is an earlier manuscript.

## Historical reports

Dated observations and raw receipts preserve their original inputs, counts,
timings, failures and limitations. They do not certify the current release.
See the [performance baselines](reports/performance-baselines.md),
[0.5.0 validation](reports/validation-0.5.0.md),
[0.5.1 validation](reports/validation-0.5.1.md),
[Frontiersmen interaction report](reports/frontiersmen-interaction-historical.md),
[Frontiersmen performance report](reports/frontiersmen-performance-historical.md),
[spatial measurement report](reports/spatial-index-benchmark.md) and
[generational measurement report](reports/generational-benchmark.md).
The [report retention ledger](reports/retention-and-dispositions.md) records
custody, fingerprints and missing historical archive limitations.
The [documentation migration reconciliation](reports/documentation-cleanup-dispositions.md)
accounts for all 69 original documentation paths.

## Architecture with ADRAI

Replace WEDL_SOURCE_CHECKOUT with the exact development repository root.
Use that checkout even when a WEDL command is inspecting a separate world
repository. Architecture decisions are CLI-managed under architecture/adrai.
Small practical snippets are in [examples](examples/README.md).

Discover repository-wide and path-specific decisions, then read the returned
stable ADR IDs and their history:

```text
adrai --repo WEDL_SOURCE_CHECKOUT search --mode fts --file .hmem.workspace --include-obsolete --limit 100 --json
adrai --repo WEDL_SOURCE_CHECKOUT search --mode fts --file src/wedl/cli.py --include-obsolete --limit 100 --json
adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json
adrai --repo WEDL_SOURCE_CHECKOUT history ADR_ID --limit 100 --json
adrai --repo WEDL_SOURCE_CHECKOUT doctor --json
```

Use the affected repository-relative path for scoped discovery. Read the actual
bodies, scope, domains, status and supersession relationships; an obsolete
decision is historical context. A limit-sized search response needs additional
bounded discovery before assuming completeness.

For a new architectural decision, draft a body in a caller-owned temporary
file and review its content, domains, scope and intended mutation sequence.
Replace the example attribution with the actual author:

```text
adrai --repo WEDL_SOURCE_CHECKOUT create --title "Decision title" --summary "Decision summary" --domain architecture --applies-to src/wedl/cli.py --actor human:YOUR_IDENTIFIER --body-file APPROVED_BODY_FILE --json
```

For an amendment, read show again and copy its exact current state_token into
STATE_TOKEN. Review the replacement body and rationale before writing:

```text
adrai --repo WEDL_SOURCE_CHECKOUT amend ADR_ID --expect STATE_TOKEN --change-summary "Reason for the amendment" --actor human:YOUR_IDENTIFIER --body-file APPROVED_BODY_FILE --json
```

Inspect create/amend --help for attribution and digest options. An LLM actor
uses its truthful kind:identifier and model. ADRAI mutations can create Git
commits automatically: inspect the returned record, commit, status and complete
patch afterward. Keep obsolete/supersession changes and their intended final
state in the reviewed sequence; consult the relevant installed command help.
Remove only the temporary body file you own when the task is complete.
Managed decision files are changed through the CLI. Use scoped CLI discovery
for current architecture rather than maintaining a second decision/status
registry here.

