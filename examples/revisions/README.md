# Original Git revision review v1

This fixture creates a base commit and two branches, then compares the three original sources at story time `main/10/0`. It produces one changed character and three conflicts: its literal body, its authored initial-state value, and its native folded state. The reviewer does not construct or apply a merge.

Prerequisites: Git with SHA-1 repositories, Python 3, and the current `wedl-rs` binary on `PATH`. Run the block in one shell. It creates a new temporary fixture; the source repository is not modified. The commit identity, source bytes and timestamps are fixed so all three full original pins are reproducible.

| Original | Full commit pin |
| --- | --- |
| Base | `eb4e4672316025a63cc31bf52118dc4c369c9290` |
| Left | `893add06ffc7a279f0bac908b42349fd28f23502` |
| Right | `65439949e405b6dffb267a2451776c5387b06a4b` |

```bash
set -eu
demo="$(mktemp -d)"
git -C "$demo" init -q --object-format=sha1 --initial-branch=main
git -C "$demo" config user.name 'WEDL revision example'
git -C "$demo" config user.email 'revision@example.invalid'
git -C "$demo" config core.autocrlf false
git -C "$demo" config commit.gpgsign false
mkdir "$demo/story"
cat > "$demo/story/world.md" <<'WORLD'
---
schema: wedl/v1
kind: world
id: world_00000000000000000000000000
title: Revision example
capabilities: []
default_timeline: main
timelines: [{id: main, label: Main}]
threads: []
state_keys:
  character:
    score: {type: integer}
relationship_metrics: {}
---
A three-way source comparison.
WORLD
cat > "$demo/story/ana.md" <<'ANA'
---
schema: wedl/v1
kind: character
id: char_00000000000000000000000001
title: Ana
initial_state: {score: 0}
---
Original account.
ANA
git -C "$demo" add story
GIT_AUTHOR_DATE='2001-01-01T00:00:00Z' GIT_COMMITTER_DATE='2001-01-01T00:00:00Z' git -C "$demo" commit -qm base
base="$(git -C "$demo" rev-parse HEAD)"
git -C "$demo" switch -qc left "$base"
python3 - "$demo/story/ana.md" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace('score: 0', 'score: 1').replace('Original account.', 'Left account.'))
PY
git -C "$demo" add story
GIT_AUTHOR_DATE='2001-01-01T00:00:00Z' GIT_COMMITTER_DATE='2001-01-01T00:00:00Z' git -C "$demo" commit -qm left
left="$(git -C "$demo" rev-parse HEAD)"
git -C "$demo" switch -qc right "$base"
python3 - "$demo/story/ana.md" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace('score: 0', 'score: 2').replace('Original account.', 'Right account.'))
PY
git -C "$demo" add story
GIT_AUTHOR_DATE='2001-01-01T00:00:00Z' GIT_COMMITTER_DATE='2001-01-01T00:00:00Z' git -C "$demo" commit -qm right
right="$(git -C "$demo" rev-parse HEAD)"
test "$base" = eb4e4672316025a63cc31bf52118dc4c369c9290
test "$left" = 893add06ffc7a279f0bac908b42349fd28f23502
test "$right" = 65439949e405b6dffb267a2451776c5387b06a4b
cat > "$demo/review.json" <<JSON
{"baseRevision":"$base","leftRevision":"$left","rightRevision":"$right","at":{"timeline":"main","tick":"10","order":"0"},"maxItems":100,"maxCharacters":100000}
JSON
printf '%s\n' 'invalid uncommitted source' > "$demo/story/world.md"
wedl-rs --repo "$demo" --compact revision-review --file "$demo/review.json" > "$demo/report.json"
python3 - "$demo/report.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1]))['result']
assert report['writesPerformed'] is False and report['canApply'] is False
assert len(report['requestSha256']) == 64 and len(report['reportSha256']) == 64
assert len(report['changes']) == 1 and len(report['conflicts']) == 3
assert sorted(row['kind'] for row in report['conflicts']) == ['state', 'text', 'text']
print('One changed record; three conflicts (state, source value, literal Markdown).')
PY
printf 'Fixture: %s\nBase: %s\nLeft: %s\nRight: %s\n' "$demo" "$base" "$left" "$right"
```

The final request uses three explicit full commit IDs. Branch names and `HEAD` are used only while constructing the demonstration history. The example then makes the live world file invalid before invoking review, demonstrating that original Git blobs supply the comparison. The shell writes `review.json` and redirects the returned transport envelope to `report.json`; the reviewer itself performs no writes.

Open the printed fixture path to inspect `review.json` and `report.json`. Each conflict carries original source paths, commit pins and blob IDs. `sourceOrdinal` is the original array index when the selected source is an array member. The result's `requestSha256` and `reportSha256` bind the complete request and result, including their budgets.

The [v1 revision-review contract](../../docs/REVISION_REVIEW.md) defines all five conflict families, native author admission, source/semantic equality, complete bounds and explicit unsupported boundaries. This example exercises the standalone original-source read. Separate
[retcon decisions](../retcons/README.md) and
[event/grouped reports](../consequences/README.md) have executable walkthroughs.
None creates a merge candidate or grants canonical approval.
