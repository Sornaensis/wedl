# Source and build anomaly investigation

Investigated 5 November 2026. The build failure is now explained by an older
library archive being reused alongside newer build outputs. The operation that
created that inconsistent set is still unidentified. The earlier source-file
restorations also remain unattributed.

This investigation found no defect in the currently delivered WEDL runtime that
warrants a speculative source-restoration workaround. It did find and correct a
local investigation-wrapper bug that defeated environment-selected build
directories. That wrapper is outside the WEDL source package.

The source archive includes an adjacent [investigation directory](../../investigation/README.md)
with reports, compiler metadata, source reconstructions, logs and the controlled
reproducer. The original frozen executables and library archive remain in
[wedl-build-mismatch-evidence.zip](https://chatgpt.com/library?file_id=libfile_719e87356da08191bbe5c42ece59f045&account_id=personal).

## 1. The failed test linked an older source generation

The frozen `authoring_context` test still exits 101 when `decoded()` unwraps the
missing `timelines[0].definitionYaml` string, before YAML parsing or the equality
comparison. The panic is at `tests/authoring_context.rs:128:41`. Its linked
`context::build` implementation is 43,380 bytes, matching the object in the saved
library archive. The working
CLI and both retained incremental context objects contain the newer 45,352-byte
implementation.

Compiler metadata identifies the source used for those artifacts. Reversing only
the recorded formatting edits and timeline-field addition reconstructs three
exact source identities:

| Artifact generation | Context source bytes | Embedded source MD5 | Timeline fields |
| --- | ---: | --- | --- |
| Saved library archive | 9,875 | `a00f663f04313e7b9ddd5896c5a50759` | `id`, `label` |
| Earlier incremental metadata | 10,019 | `c003834becc05e5312f1b2ecb6a13ff9` | Also `sourceField`, `definitionYaml` |
| Adjacent/later metadata | 10,802 | `366e2689b8828cb65fe827e3b12352cb` | Same new fields; formatted source |

Across 98 library source records, the archive and adjacent metadata differ in
only four files: `authoring_tools/context.rs`, `authoring_tools.rs`, `http.rs` and
`service.rs`. The last three differences are exactly their recorded formatting
changes. The archive and earlier incremental metadata differ only in the
context file. Their external dependency identities match, but their compiler
crate hashes differ.

Of 256 archived object members, 255 match the earlier incremental session and
248 match the later session. Both incremental context code sections are
byte-identical to each other. The older archive therefore represents a real
earlier source generation; this is not just a misleading assertion message or
one malformed JSON response. See the [forensic report](../../investigation/archive-forensics/report.json).

The archive's recorded modification time is 14:17:31.750218509 and its change
time is 14:20:14.910908656 on 5 November, Copenhagen time (UTC+1). The good CLI
existed by 14:17:36; the failing test linked at 14:23:59. The later change time
is a lead, not a write attribution: permissions and hard-link changes can also
change it. No retained trace names the operation responsible for this archive.

All 43 monitored original/evidence inputs retained their hashes, size, mtime,
ctime, inode, permissions and link count during the new inspection and replay.

## 2. Cargo can propagate this inconsistent state

A small independent crate demonstrates the failure mechanism without any WEDL
code or dependencies:

1. Build a library and CLI that return `1`.
2. Change the library to return `2`, then rebuild. The CLI returns `2`.
3. **Deliberately** replace the new library archive with the old bytes while
   preserving the new archive's modification time. Leave current source,
   standalone compiler metadata and Cargo fingerprints unchanged.
4. Build a new test expecting `2`. Cargo reports the library filesystem state
   as up to date; the test receives `1`, while the unchanged CLI still returns `2`.
5. Build the unchanged current source in a fresh directory. The test passes.

Incremental compilation was disabled throughout. The standalone reproducer was
also run successfully in a second new directory. This shows that an
incremental-compiler defect is not required to propagate an already inconsistent
archive. It does **not** show that Cargo spontaneously created the historical
inconsistency: the old archive substitution in this experiment is explicit.

This behavior is consistent with Cargo's documented
[mtime-based freshness checks](https://doc.rust-lang.org/stable/nightly-rustc/cargo/compiler/fingerprint/index.html).
The preserved logs record the exact library path, freshness decision, failing
test and unchanged CLI/source hashes. See [control results](../../investigation/cargo-control/report.json).

The reproducible control creates a new temporary fixture and leaves real project
sources and existing build directories alone:

```sh
python investigation/reproduce_archive_reuse.py --cargo cargo
```

Run from the extracted archive root. Its inner test failure is intentional; the
outer script succeeds only when the expected mismatch and fresh-build control
both occur. It prints the new evidence directory. `--output PATH` optionally
selects a directory that must not already exist.

## 3. One local wrapper bug was corrected

The investigation's `tooling/cargo.sh` unconditionally exported its shared
`CARGO_TARGET_DIR`, overwriting a caller's environment choice. A metadata query
confirmed that a requested isolated target was ignored. The wrapper now keeps a
supplied nonempty value and uses its existing default only when no value is
provided; both cases were verified. Before/after wrapper bytes and results are
preserved in the [wrapper report](../../investigation/cargo-control/wrapper-report.json).

This was a real isolation hazard. No retained command trace establishes that an
ignored override or overlapping build caused the older archive. The final WEDL
build already used an explicit `--target-dir` and a new directory. Cargo's
[target-directory configuration](https://doc.rust-lang.org/stable/cargo/reference/config.html#buildtarget-dir)
supports both environment and command-line selection.

## 4. Source restoration remains unattributed

Earlier notes record unrelated canonical source returning to older bytes and a
deleted private work reappearing. The latest retained description of the work
restoration records its exact original bytes and nanosecond mtime, a new inode
and later ctime. The original failing-run directories and command transcript are
not available in this workspace or the starting source archive. The older
browser readiness check is an attribution weakness, but cannot explain the
separate frozen CLI observation by itself.

The current source audit found:

- Historical readers use Git object reads; no checkout, reset, restore or
  automatic publication recovery was found in those paths.
- Publication recovery is explicit-only and its target whitelist excludes
  manuscript work, segment and reveal files.
- Native canonical/private writers use temporary files and atomic replacement,
  preserve permissions, and do not restore original timestamps. Ordinary
  private saves also require a valid current canonical snapshot.
- Twelve reviewed source files match the delivered manifest; six normal
  writer/recovery files also match the earliest available workspace baseline.
  This does not establish the identity of an unavailable earlier executable.

A bounded native probe passed 26 checks across 19 CLI calls. Two canonical
updates and one private save changed only their specified targets and produced
fresh mtimes. After an explicit work deletion and canonical corruption,
historical list/detail/resolve and Writing reads retained that exact damaged
state, with zero watched source-mutation events. The historical phase logged
347 Git invocations limited to object-reading operations; `hash-object` did not
use its write flag. See the [writer audit](../../investigation/writers-audit.json)
and [phase report](../../investigation/writer-control/report.json).

A separately identified positive-control helper then restored the work with an
explicit timestamp setter and rename. It produced the old bytes and old
nanosecond mtime with a new inode and later ctime while leaving canonical
corruption intact. That establishes that a metadata-preserving replacement can
produce the recorded signature. It is an intentional control, not an unexplained
restoration.

## 5. What the environment allowed us to observe

The kernel denied `strace` before its driver started. The fallback records
inotify events, file identities, phase boundaries, native subprocesses and the
disclosed Git logging wrapper. It is not a full syscall/descendant-process trace;
inotify does not identify the writer.

Executor calls used different mount, PID and user namespaces. A separate
concurrent root/agent control nevertheless observed the same two file states
and nine mutation events, respected the held file lock, and retained the exact
edit, deletion and replacement after both observers exited. A fresh call saw
the same hashes and nanosecond metadata. This specific execution was coherent;
different namespaces do not establish an environment fault. See the
[workspace control](../../investigation/workspace-control/report.json).

## Conclusion and remaining evidence

The stale library generation and its path into the failed test are established.
The original archive producer/replacer and the earlier source-restoration writer
are not. Those two observation families must not be assigned one common cause
without evidence.

A recurrence needs an attributable trace from the failing execution, captured
before cleanup or rebuilding: the affected path's old/new identity, the process
and namespace performing the mutation, and the exact source, library and linked
executable hashes. A clean run from another execution cannot supply that missing
attribution.

No WEDL runtime, registered test, dependency or v1 contract was changed for this
investigation. All 1,158 build/test/static/example hashes still match the source
that passed 1,823 native tests and 216 browser checks in the preceding review;
those complete suites were not rerun for documentation-only changes. The
confirmed feature estimate remains zero further named iterations. Causal
diagnosis remains unbounded by the available evidence.
