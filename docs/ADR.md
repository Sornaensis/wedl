# Architecture decisions with ADRAI

Read and write ADRs through the `adrai` CLI. Its managed records under
`architecture/adrai/decisions` and `architecture/adrai/connections` contain the
decision bodies and their evolution. This page provides commands and historical
alias navigation only. Read current scope, domains, status, and replacement from
the CLI; historical approval dates and rationale remain in the managed bodies.

Run these commands from the repository root. `--repo .` selects this repository;
from another directory, pass its exact absolute Git root.

```powershell
adrai --repo . search --mode fts --file .hmem.workspace --limit 100 --json
adrai --repo . search --mode fts --file src/wedl/schema.py --limit 100 --json
adrai --repo . search 'global StoryTime' --mode fts --include-obsolete --limit 100 --json
adrai --repo . show <ADR-ID> --json
adrai --repo . history <ADR-ID> --limit 100 --json
adrai --repo . doctor --json
```

File-filtered searches include repository-wide `**` decisions. Query results are
ranked and limited; increase the limit or use additional bounded reads when it
may truncate applicable decisions. `--include-obsolete` includes historical
records, and `--at <revision>` pins reads to a Git revision. `show` returns the
current `state_token` used by guarded edits.

Prepare and review the complete decision body before writing. Supply an actor
that identifies the writer; historical approval recorded in a migrated body is
separate from the actor performing the migration. The following are templates,
with placeholders replaced by reviewed values:

```powershell
adrai --repo . create --title '<title>' --summary '<summary>' --domain '<domain>' --applies-to '<glob>' --actor human:<name> --body-file <body-file> --json
adrai --repo . amend <ADR-ID> --change-summary '<reason>' --expect <state-token> --actor human:<name> --body-file <body-file> --json
adrai --repo . scope <ADR-ID> --set '<glob>' --reason '<reason>' --expect <state-token> --actor human:<name> --json
adrai --repo . domain <ADR-ID> --set '<domain>' --reason '<reason>' --expect <state-token> --actor human:<name> --json
adrai --repo . obsolete <ADR-ID> --replacement <replacement-ADR-ID> --reason '<reason>' --expect <state-token> --actor human:<name> --json
adrai --repo . reactivate <ADR-ID> --reason '<reason>' --expect <state-token> --actor human:<name> --json
```

These writes automatically commit their managed records. Check the exact
repository and index before writing, inspect the returned commit and full patch,
then read back with `show` and `history`. Use a fresh state token for each edit.
Amendments preserve earlier editions; use the CLI to evolve a decision instead
of editing or deleting generated records directly. An LLM writer supplies
`--actor llm:<identifier>` and `--model <model>`.

The historical aliases below remain stable for existing references. Each entry
links to its managed source and names its returned ADRAI ID; it carries no
separate status or approval authority. Conformance vectors live under
`architecture/adrai/examples`.

## ADR 0001

[0001 — Multi-strand chronology and fictional continuities](../architecture/adrai/decisions/R01M/R01M48NJHZTGCYRTBBJQ9D7XG9M--multi-strand-chronology-and-fictional-continuities.decision.md)
— `A01M48NJH26YJTG9XWA0SKCPR2Z`. Read with `adrai --repo . show A01M48NJH26YJTG9XWA0SKCPR2Z --json`.

## ADR 0002

[0002 — Shared-world concurrent narrative threads](../architecture/adrai/decisions/R01M/R01M48NHJBX1HWE7RG8G2S3DD9H--shared-world-concurrent-narrative-threads.decision.md)
— `A01M48NHJ5Z6AX617K56WRVFYWT`. Read with `adrai --repo . show A01M48NHJ5Z6AX617K56WRVFYWT --json`.

## ADR 0003

[0003 — Calendar and historical chronology semantics](../architecture/adrai/decisions/R01M/R01M48NNHA0JJP2Y13F9FPEK43C--calendar-and-historical-chronology-semantics.decision.md)
— `A01M48NNH43QCFRPC68NQQTFT63`. Read with `adrai --repo . show A01M48NNH43QCFRPC68NQQTFT63 --json`.

## ADR 0004

[0004 — Spatial domain, queries, and schema-version contract](../architecture/adrai/decisions/R01M/R01M48NPY7BSP5A8EMP5MFVD8MN--spatial-domain-queries-and-schema-version-contract.decision.md)
— `A01M48NPY19KENPF8EWB8A4W95W`. Read with `adrai --repo . show A01M48NPY19KENPF8EWB8A4W95W --json`.

## ADR 0005

[0005 — First-class generational history and coordinated v0.7 contract](../architecture/adrai/decisions/R01M/R01M48NQZAYXMAHXD49NWZ9M8YZ--first-class-generational-history-and-coordinated-v0-7-contract.decision.md)
— `A01M48NQZ50KH9V094Q5A3138R0`. Read with `adrai --repo . show A01M48NQZ50KH9V094Q5A3138R0 --json`.

## ADR 0006

[0006 — Preserve authored object affordances in v0.7](../architecture/adrai/decisions/R01M/R01M48NSE20KVVH8NHZ794W57YB--preserve-authored-object-affordances-in-v0-7.decision.md)
— `A01M48NSD0Q3SVF3RRXVR520H9G`. Read with `adrai --repo . show A01M48NSD0Q3SVF3RRXVR520H9G --json`.

## Development workflow

[Bounded development scratch and test signals](../architecture/adrai/decisions/R01M/R01M48NTY3JQB1XGHTVQB4V60EX--bounded-development-scratch-and-test-signals.decision.md)
— `A01M3Y3S5QMJWMBMQT42HDQPDJS`. Read with
`adrai --repo . show A01M3Y3S5QMJWMBMQT42HDQPDJS --json`.
