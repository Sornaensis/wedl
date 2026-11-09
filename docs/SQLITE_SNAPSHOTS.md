# SQLite snapshots

This contract covers the SQLite projection used to validate and query WEDL world source. Markdown with YAML frontmatter and original Git objects retain authority for world content and provenance. The SQLite files are replaceable build output.

This contract follows the active decisions [Markdown and Git authority](../architecture/adrai/decisions/R01M/R01M70QB923BA8WW0J7DB1QVZKY--keep-markdown-and-git-authoritative-for-world-state.decision.md), [SQLite views](../architecture/adrai/decisions/R01M/R01M70QD1PHS8ENV9C5QFMAYF72--compile-world-views-for-browsing-search-and-validation-in-sqlite.decision.md), [incremental publication](../architecture/adrai/decisions/R01M/R01M70QDD10FZ6500HADMBT3ZXZ--incrementally-publish-sqlite-snapshots-after-git-commits.decision.md), and [read-only cards](../architecture/adrai/decisions/R01M/R01M70QDS2K4Z3WSTRVAGKVE31A--serve-the-world-through-read-only-searchable-cards.decision.md).

## Identity and trust

- An immutable snapshot key contains the verified repository scope, object hash algorithm, full original commit ID, and an internal projection compatibility ID. The compatibility ID changes when any stored interpretation becomes incompatible, including source parsing, validation, query semantics, or the SQLite schema. The public WEDL source and API versions follow their existing contracts.
- A worktree snapshot key contains the worktree scope, compatibility ID, and the existing exact source fingerprint. HEAD can identify a starting point for reuse. A worktree may have no HEAD, and a file may change while HEAD, length, and modification time stay the same. The live API keeps its existing null revision and current-source pins.
- Preserve the exact path-framed fingerprint, including case-sensitive path spelling, original bytes, and ordering. Retain exact record IDs, raw frontmatter/body, canonical relative paths, and original blob/tree/commit provenance where the source was Git. Git reuse requires an authenticated target manifest built from original objects. Verify repository ownership and object availability according to the existing original-source contract, including SHA-1 and SHA-256. Reuse by exact source bytes or authenticated blob identity and compatible parse context. Rebuild path-dependent rows and citations on moves, and reconcile removals and case collisions.
- Keep a cache manifest with source identity, compatibility, bounds, validated image identity, and build state. Cache trust requires a local operator-owned directory and an artifact produced by the trusted builder or fully verified before use. Refuse unsafe paths, links, unexpected schemas, damaged images, and mutable or unsupported cache locations. A self-declared SQL metadata row alone grants no trust. Perform bounded integrity and schema checks at admission. Reuse the result only while the verified immutable file identity is retained. Validate data and query bounds independently of SQL storage.
- Publish validated generations through a staging transaction and atomic activation. Readers pin a single generation for their whole operation. Competing builders share a lock or a single-flight result for the same key, with independent requests using retained read transactions. Failed builds never enter the ready namespace. Bound temporary files and retained generations, and evict only disposable, unpinned images. The writer lock, recovery journal, and private receipts are separate protected state.

## Refresh and commit ordering

A WEDL-produced Git commit triggers an incremental build for the exact new commit ID after Git has created it. A commit and an SQLite file cannot be published in one transaction. If projection publication fails, return the committed identity and the projection failure distinctly, and allow a safe retry for that same ID. An existing commit may be compiled on demand. Retained older generations remain available through their explicit historical selectors.

For live worktrees, honor the native writer lock and publication fence. Take a coherent source manifest, reconcile additions, edits, moves, and deletions, and recheck the source and publication identity before activation. Trusted change evidence may narrow inspection. When the observation history is incomplete, including watcher overflow, restart, branch switch, or uncoordinated editor saves, inspect the affected source or conservatively verify the full file image. Unchanged raw-byte identities may reuse parsed records and their approved validation evidence. Preserve whole-world validation for changed generations wherever dependency coverage is unproven. Preserve native parse and capture errors, including refusal and recovery for mixed captures or pending publication. When a coherent source capture fails semantic validation, retain its native status and validation envelopes and diagnostics. Such a capture cannot become a ready browse generation. A stale or absent current cache may trigger a build from verified source. Successful browse, search, and facet responses require a ready generation for the selected source. If freshness cannot be established or a build fails, return the appropriate refusal.

A request must observe changes that completed before its source fence, including a successful external CLI write without a commit. A write racing a capture can return a retryable conflict. Any separately retained generation must be explicitly identified as historical. No repair or cleanup of authoring state follows from a cache miss. Writes still recheck exact source, approval, private-file pins, and receipts under the native writer's existing contract. SQLite build or cache publication grants no write approval.

## Projection and query boundaries

Use versioned SQLite projections for browse, lexical search, and eligible validation. Preserve the native parser, path restrictions, source budgets, and semantic diagnostics. Use indexed record identity, catalogue facets, timeline and relationship keys, and full-text indexes for migrated reads. Validation can use proved compiled facts with exact diagnostic parity. An initially changed generation may still run full native cross-record validation before its report becomes eligible for reuse. Specialized semantic, hybrid, and character-scoped retrieval keep their existing policy, admission, and ranking rules. Browser implementations may run compatible native query logic on data from the pinned SQLite generation. A new SQL execution path must prove equivalent results before replacing it.

The production `serve` target is read-only and intended for human browsing. Provide searchable cards for characters, locations, items, lore, spatial relationships, and other supported world records, with explicit character POV, story tick, and calendar filters. Apply visibility before deriving cards, search matches, counts, navigation, and filter facets. Route all filters through the existing chronology and admission rules, including exact timeline, integer tick/order, calendar conversions, and withheld evidence. Test that each admitted result and citation reflects the selected viewpoint and instant. Reject ambiguous time selectors with structured diagnostics. Browser requests and controls expose no mutation to Markdown, private authoring state, or Git. A separate builder may publish disposable cache artifacts. Browser publishing in the current baseline is legacy behavior for comparison. Authoring commands keep their native review, apply, and recovery semantics.

| Existing consumer | Contract for migration |
| --- | --- |
| Original Git source and historical review | Verify literal original commit, tree and blob identity. Preserve historical availability and authorization checks. A multi-revision review must use each of its explicit revisions and retain their distinct evidence. |
| Worktree loader, validation and status | Keep source identity, file paths, validation flags, diagnostics and ordering. Cache eligibility never bypasses dirty source, capability validation, publication state or the path/byte/work budgets. |
| HTTP and CLI author queries | Keep normal entry points and public envelopes, status codes, fingerprints, revision fields, explicit horizons, ordering and limits. Source-show retains the exact authored bytes. Run exact and alias selectors, catalogue, chronology, spatial and lexical parity against the source path. |
| Existing lexical and specialized retrieval | Preserve literal query semantics, scope, admission, BM25 and stable ties, snippets, pagination, citations and limits. Compare cold and reused generations, including source-dependent search ranking and the existing independent retrieval modes. |
| Authoring and public writer boundaries | Preserve expected source and before/after hashes, explicit approvals, private-file checks, structured stale-pin conflicts, recovery and terminal retry semantics. Verify the intended removal of mutation routes from read-only serve separately from CLI parity. |
| Manuscript, reveal, retcon, revision and reader policy | Preserve each independent source selection, private-store isolation, and policy emission fence. These calls retain their own authority even when live canon is invalid. Never put private inputs or withheld policy results in the shared author index. |

## Existing limits and adoption gates

The bounded canonical loader permits up to 4,096 records, 8 MiB per file, 32 MiB of raw source, and 8,192 walked entries or Git tree objects. Its additional path, YAML nesting/node, expanded scalar and original Git tree/ancestry budgets remain effective. Individual operations also keep their existing semantic work, selector, byte, result and policy limits. The near-limit performance fixture reaches the record ceiling and almost the total byte ceiling. It uses simple valid YAML and cannot establish a limit increase for dense YAML, high relationship fan-out, large individual files, complex validation, or reader policy queries.

A limit increase requires separate measurements with high-entropy prose and YAML, many tiny files, at-bound individual files, dense linked and dated data, and the existing worst-case operation budgets. Check peak RSS and disk footprint during cold/incremental builds, simultaneous readers, cache recovery and eviction. Require parity and verified fail-closed behavior at the old and proposed boundaries before changing a limit.

Before adoption, compare the same benchmark tiers on the same target hardware using release binaries. Include missing cache, exact commit restart, cache hit, one-record commits, same-mtime dirty saves, add/move/delete, and parallel readers, with corrupt cache and interrupted publication exercised separately. Warm unchanged requests must have zero source parses, full validations, projection builds and whole-database integrity scans. Repeated exact-commit reads and restart must reuse the ready artifact. Single-file edits must reuse the unchanged records and projections, with physical SQLite writes reported separately from logical row work. Measure trusted and conservative worktree refresh independently.

For the synthetic near-limit tier on the baseline host, the initial adoption budgets for simple point and lexical reads are p95 at most 100 ms serial, p95 at most 200 ms with four clients, and at least 20 requests per second with four clients. These apply to a ready commit snapshot and a live generation whose freshness can be checked without a full scan. An exact-commit restart with an existing artifact targets readiness within 2 seconds. Report conservative full verification separately and compare it with the corresponding current-server result.

Target at most 512 MiB of peak server and builder RSS combined and 256 MiB per published generation for this fixture. Start with a 1 GiB per-world cache budget including build staging. Track staged bytes, physical SQLite writes, and active-reader retention separately. Keep the budget finite when readers pin old generations. A build that cannot fit should refuse safely rather than expose an incomplete generation. These are acceptance budgets for the comparison workload. Additional load and limit-increase tests must cover other content shapes and host classes.

## Current serve baseline

Measured the production live author server with benchmark counters and the Windows path correction in this change, before introducing SQLite world snapshots. Before the correction the server reached readiness, then returned HTTP 400 for a live entity read because the bounded worktree loader rejected Windows path separators. The benchmark verifies that the initial API fingerprint matches the startup fingerprint and that the entity path uses canonical slashes.

The release binary was built from base `e5e21a9856ad8f901c32e503041ea9324c6c68f3` plus the instrumentation and Windows correction. Binary SHA-256 was `e30184bdd81fec382f03c5fe8b225d654596e2358d8b4c26cdc899f74324f034`. The host ran Windows 11 build 26200 with an Intel Core Ultra 9 285K, 24 logical CPUs, 63.5 GiB of usable RAM, and local NTFS scratch. The toolchain was Rust 1.97.1, x86_64-pc-windows-msvc, with release optimization and locked dependencies.

Set $scratch to an existing writable scratch directory. Run from the repository root with Python 3.11 or newer.

```powershell
$env:CARGO_TARGET_DIR = Join-Path $scratch 'target'
$env:CARGO_INCREMENTAL = '0'
cargo build --locked --release --features snapshot-baseline
python tools/benchmark_snapshots.py --binary (Join-Path $env:CARGO_TARGET_DIR 'release/wedl-rs.exe') --scratch $scratch --samples 12 --workers 4
```

The runner uses a disposable world per tier with deterministic plain Markdown and simple object frontmatter. It launches an actual server on an ephemeral IPv4 loopback port. It times launch to the server's readiness line, then issues an initial point read, 12 serial point reads, 12 point reads across four workers, an initial lexical query with no application index, and 12 lexical cache hits. It atomically replaces one object file while preserving byte length and modification time. It then verifies the changed fingerprint, entity body, and search result without a restart or Git commit. Cold describes the application cache. Filesystem caches were left under OS control.

Client latency includes loopback connection and full response transfer. Throughput is completed requests divided by batch wall time. P50 and p95 use nearest rank, so p95 is the maximum of the 12 measured samples. First and post-edit reads have one sample each. Gate wait covers the live server mutex from the worker's entry through acquisition and excludes Tokio dispatch and client time. Peak RSS is the server process peak working set across the complete tier. Index bytes are the persisted SQLite file size. The feature counts logical source reads and published SQL rows. Physical device reads and SQLite pages written require separate instrumentation.

### Browse and contention

All latencies are milliseconds. The tier byte counts include the world record.

| Records | Source bytes | Ready | First entity | Warm entity p50 / p95 | Serial req/s | Four-reader p50 / p95 | Four-reader gate p95 | Four-reader req/s | Edited entity |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 | 32,645 | 401.1 | 49.9 | 58.7 / 65.3 | 17.986 | 177.9 / 182.7 | 137.3 | 22.061 | 74.9 |
| 1,024 | 2,095,493 | 4,631.0 | 794.1 | 794.6 / 1,200.9 | 1.199 | 3,186.7 / 3,384.3 | 2,593.6 | 1.246 | 748.1 |
| 4,096 | 32,498,309 | 19,067.1 | 3,025.5 | 3,170.3 / 3,690.3 | 0.312 | 13,130.4 / 13,936.4 | 10,575.0 | 0.295 | 3,441.8 |

### Lexical search and resources

| Records | First FTS (ms) | Warm FTS p50 / p95 (ms) | Warm FTS req/s | Edited FTS (ms) | Peak server RSS (bytes) | FTS index (bytes) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 | 82.8 | 63.1 / 79.1 | 15.390 | 73.1 | 13,094,912 | 126,976 |
| 1,024 | 863.7 | 797.1 / 846.2 | 1.251 | 852.0 | 31,567,872 | 6,737,920 |
| 4,096 | 4,638.9 | 3,513.3 / 3,937.9 | 0.277 | 4,781.2 | 183,848,960 | 74,031,104 |

Each measured request walked the source tree three times and read every Markdown file twice. The near-limit fixture recorded 12,294 walked entries, 8,192 file reads, 64,996,618 source bytes read, and 4,096 source preflights per request. Cache size was the same before and after the edit. The near-limit operation counters were stable across samples.

| Per-request work at 4,096 records | Entity (all batches) | Warm FTS | First or edited FTS |
| --- | ---: | ---: | ---: |
| Record parses | 8,192 | 12,288 | 16,384 |
| Full validations | 1 | 2 | 3 |
| Fingerprint passes | 2 | 3 | 4 |
| Fingerprinted source bytes | 64,996,618 | 97,494,927 | 129,993,236 |
| FTS index probes / matches | 0 / 0 | 1 / 1 | 2 / 1 |
| FTS quick checks / builds | 0 / 0 | 1 / 0 | 1 / 1 |
| Published record rows / FTS rows | 0 / 0 | 0 / 0 | 4,096 / 4,096 |

These are single-host measurements on a synthetic world. Historical Git loads, the future read-only browser, complex semantics, and physical SQLite I/O need separate comparison runs.

## Live worktree reuse comparison

The existing live server retains its latest bounded parsed world and validation report. It reuses them only after the source path set and bytes match on both reads and the publication fence passes. Changed source still receives the full parse and validation. The server replaces its startup world on the first bounded capture, keeps the existing request gate, and continues to use the separate lexical index and its integrity checks.

This comparison used the same Windows 11 build 26200 host, Intel Core Ultra 9 285K, Rust 1.97.1, release profile, fixtures, sample count, and benchmark runner as the baseline. Use the commands above with `--offline` added to the Cargo build. The inputs were base `2ba30125f4e164a111a08728decf3ca837b90941` plus the live reuse changes in `src/historical.rs`, `src/http.rs`, and `src/service.rs`. Binary SHA-256 was `784107d1b7e494e1294178d1751c7a909ae80ed57ed4e269385dbff4510ff30a`.

All values below are milliseconds, shown as baseline → live reuse. The edited columns are single requests. The warm and four-reader columns use the same 12-request nearest-rank p95 as the baseline. The runner issues the edited entity request before the edited FTS request, so that search starts from a worktree view the entity read has already refreshed.

| Records | Warm entity p95 | Four-reader p95 | Four-reader gate p95 | Warm FTS p95 | Edited entity | Edited FTS |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 | 65.3 → 78.6 | 182.7 → 194.3 | 137.3 → 140.7 | 79.1 → 83.1 | 74.9 → 66.3 | 73.1 → 92.9 |
| 1,024 | 1,200.9 → 829.0 | 3,384.3 → 2,053.0 | 2,593.6 → 1,525.0 | 846.2 → 517.5 | 748.1 → 449.7 | 852.0 → 516.5 |
| 4,096 | 3,690.3 → 1,689.4 | 13,936.4 → 6,226.4 | 10,575.0 → 4,622.8 | 3,937.9 → 1,880.3 | 3,441.8 → 1,480.9 | 4,781.2 → 2,142.1 |

At 4,096 records, serial entity throughput rose from 0.312 to 0.679 requests per second and four-reader throughput rose from 0.295 to 0.693. Peak server RSS across the whole tier went from 183,848,960 to 182,353,920 bytes. The persisted lexical index was 74,031,104 bytes in both runs. All three tiers exposed the same-size, same-mtime edit without restarting, with matching updated entity and search fingerprints.

| Work per request at 4,096 records | Warm entity | First or edited entity | Warm FTS | First or edited FTS |
| --- | ---: | ---: | ---: | ---: |
| Source walks | 3 | 3 | 3 | 3 |
| Source files read | 8,192 | 8,192 | 8,192 | 8,192 |
| Logical source bytes read | 64,996,618 | 64,996,618 | 64,996,618 | 64,996,618 |
| Source preflights | 0 | 4,096 | 0 | 0 |
| Record parses | 0 | 8,192 | 4,096 | 8,192 |
| Full validations | 0 | 1 | 1 | 2 |
| Fingerprint passes | 0 | 2 | 1 | 2 |
| FTS quick checks / builds | 0 / 0 | 0 / 0 | 1 / 0 | 1 / 1 |
| Published record rows / FTS rows | 0 / 0 | 0 / 0 | 0 / 0 | 4,096 / 4,096 |

The reduced work is deterministic for unchanged bounded source. Timings come from separate single-host runs on synthetic data, and the small tier did not improve. Edited entity requests performed the same logical work as the baseline, so their elapsed-time difference also reflects run conditions. Conservative file verification and serial request execution still govern live latency. The SQLite generation targets and the existing source limits remain the criteria for that separate implementation.

### Back-to-back near-limit comparison

A follow-up ran the exact Git archive of `2ba30125f4e164a111a08728decf3ca837b90941` first and the reuse implementation second. Build each input with the command above using separate `CARGO_TARGET_DIR` directories, then run the same benchmark with `--tier near-limit --samples 4 --workers 4`. Each run created a fresh 4,096-record fixture. The base binary SHA-256 was `75e6f6ab624d311fb02343552fa67409e75bb3202bc6a2ac7619df26a538b0ba`. The reuse binary SHA-256 was `6d8fc347d5739e556eb3e79c49f094545bde3c3a377cb9957b15750ea90428a7`.

| Measurement (ms) | Exact base | Live reuse |
| --- | ---: | ---: |
| Warm entity p95 | 3,913.7 | 1,570.9 |
| Four-reader entity p95 | 9,593.2 | 5,801.6 |
| Four-reader gate p95 | 7,427.9 | 4,393.9 |
| Warm FTS p95 | 2,536.0 | 2,073.4 |
| Edited entity | 1,837.8 | 1,695.5 |
| Edited FTS | 2,928.4 | 2,247.2 |

The adjacent runs confirmed 8,192 record parses for each warm entity request on the base and zero with reuse. Both parsed 8,192 records on the first edited entity request. Four samples make the reported p95 the slowest sample. These measurements reflect one run per build, including run-order and filesystem variation.
