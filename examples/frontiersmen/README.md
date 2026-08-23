# Frontiersmen authoring artifacts

The files in this directory are the actual changesets, previews, apply receipts, validations, and interactive query outputs used to build the packaged Frontiersmen world.

## Act commits

| Index | Slug | Narrative movement |
|---:|---|---|
| 0 | `foundation` | Frontier geography, cast, amber economy, Blackroot author truth |
| 1 | `arrival-eastroad` | Keldmouth hiring, amber wages, eastroad caravan, Harrowcross arrival |
| 2 | `harrowcross-investigation` | abandonment, wretch recurrence, amber ledger, Saint Orra lead |
| 3 | `under-the-frontier` | mine, cave-in, two underground days, Sunken Hall, surface |
| 4 | `tree-king-and-pursuit` | masked camp, capture, blood sport, Moth's intervention, escape |
| 5 | `pursuit-perspectives` | timed recollections, distant audibility, context cleanup |
| 6 | `drowned-waymark` | immediate pursuit broken, bounded Blackroot evidence, uncertain southward road |

The Act 6 payload is followed by the separately receipted
`act-06-boundary-tightening` changeset, which closes the inclusive Hunt/Running
interval at `main 195:99`, removes the Hunt's stale `active` tag, and preserves
the identity and unresolved-fate boundaries at the active `main 210:0` edge.
Recollections use stable conversation-and-slug identifiers, so a clean rebuild
of all seven acts reproduces the 307 non-world canonical and packaged Markdown
records byte-for-byte. The target's `story/world.md` keeps the repository-local
ID generated during initialization, so that record is byte-for-byte identical
only when the target starts with the same world ID.

A fresh empty wedl repository can be rebuilt with:

```bash
PYTHONPATH=src python tools/build_frontiersmen.py /path/to/world
```

Resume from an act boundary with:

```bash
PYTHONPATH=src python tools/build_frontiersmen.py /path/to/world --start 3
```

PowerShell uses a semicolon-separated `PYTHONPATH` when there is more than one
entry. For this single-entry build, set it for the current terminal and invoke
the workspace venv directly:

```powershell
$env:PYTHONPATH = "src"
& .\.venv\Scripts\python.exe tools\build_frontiersmen.py .\frontiersmen-rebuilt
& .\.venv\Scripts\python.exe tools\build_frontiersmen.py .\frontiersmen-rebuilt --start 3
```

`interactive/` contains the contexts, searches, conversation views, leakage probes, and story-point inspections used between acts.
