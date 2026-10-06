# Review an explicit consequence preview

This Ash Archive walkthrough rescues a ledger and records a belief, directional
trust, a later plot resolution and an outcome link. It also includes a separate
prose edit so you can compare what the preview attributes to the rescue.

Replace the zero `expectedHead` with the exact HEAD of your world repository.
Check the installed schema at that revision before using the example:

```powershell
wedl changeset schema --repo PATH --revision HEAD_SHA
```

Use your world's names and supported source version in the source `schema`
members. Save the following request as `rescue.json`:

```json
{
  "action": "consequence.batch",
  "expectedHead": "0000000000000000000000000000000000000000",
  "idempotencyKey": "explicit-ledger-rescue",
  "summary": "Rescue the ledger and record explicit consequences",
  "consequenceRequest": {
    "protocol": "wedl-event-consequence-delta/v1",
    "at": {
      "timeline": "main",
      "tick": "302",
      "order": "0"
    },
    "limit": 1000
  },
  "operations": [
    {
      "type": "entity.create",
      "temporaryId": "tmp:ledger",
      "value": {
        "frontmatter": {
          "schema": "wedl/v0.3",
          "kind": "object",
          "title": "Rescued ledger",
          "domain": "story",
          "status": "canonical",
          "tags": [],
          "aliases": [],
          "initial_state": {}
        },
        "bodyMarkdown": "Rescued ledger\n"
      }
    },
    {
      "type": "entity.create",
      "temporaryId": "tmp:plot",
      "value": {
        "frontmatter": {
          "schema": "wedl/v0.3",
          "kind": "story-point",
          "title": "Recover the rescued ledger",
          "domain": "story",
          "status": "canonical",
          "tags": [],
          "aliases": [],
          "lifecycle": {
            "initial_state": "dormant",
            "transitions": []
          },
          "dependencies": {},
          "trigger": {},
          "outcome_events": []
        },
        "bodyMarkdown": "Recover the rescued ledger\n"
      }
    },
    {
      "type": "entity.create",
      "temporaryId": "tmp:scene",
      "value": {
        "frontmatter": {
          "schema": "wedl/v0.3",
          "kind": "scene",
          "title": "Ledger rescue scene",
          "domain": "story",
          "status": "closed",
          "tags": [],
          "aliases": [],
          "location": "South-Bank Flood Stair",
          "time": {
            "start": {
              "timeline": "main",
              "tick": 299,
              "order": 0
            },
            "current": {
              "timeline": "main",
              "tick": 302,
              "order": 0
            },
            "end": {
              "timeline": "main",
              "tick": 303,
              "order": 0
            }
          },
          "participants": [],
          "objects": [],
          "environments": [],
          "conversations": [],
          "story_points": [],
          "observations": [],
          "outcome_events": []
        },
        "bodyMarkdown": "Ledger rescue scene\n"
      }
    },
    {
      "type": "event.create",
      "temporaryId": "tmp:rescue",
      "title": "Ledger rescue",
      "time": {
        "timeline": "main",
        "tick": "300",
        "order": "0"
      },
      "effects": [
        {
          "id": "tmp:custody",
          "target": "tmp:ledger",
          "key": "holder",
          "operation": "set",
          "value": {
            "entity": "Mara Vale"
          }
        }
      ]
    },
    {
      "type": "knowledge.create",
      "temporaryId": "tmp:belief",
      "value": {
        "frontmatter": {
          "schema": "wedl/v0.3",
          "kind": "knowledge",
          "title": "Mara believes the rescued ledger safe",
          "domain": "story",
          "status": "canonical",
          "tags": [],
          "aliases": [],
          "knower": "Mara Vale",
          "claim": {
            "key": "rescued-ledger-safe",
            "statement": "The rescued ledger is safe."
          },
          "transitions": []
        },
        "bodyMarkdown": "Mara believes the rescued ledger safe\n"
      }
    },
    {
      "type": "knowledge.transition.append",
      "knowledge": "tmp:belief",
      "transition": {
        "id": "tmp:learned",
        "time": {
          "timeline": "main",
          "tick": "300",
          "order": "0"
        },
        "state": "accepted",
        "causing_event": "tmp:rescue"
      }
    },
    {
      "type": "relationship.create",
      "temporaryId": "tmp:trust",
      "value": {
        "frontmatter": {
          "schema": "wedl/v0.3",
          "kind": "relationship",
          "title": "Mara trusts Oren after ledger rescue",
          "domain": "story",
          "status": "canonical",
          "tags": [],
          "aliases": [],
          "from": "Mara Vale",
          "to": "Oren Thane",
          "relationship_kind": "trust",
          "transitions": []
        },
        "bodyMarkdown": "Mara trusts Oren after ledger rescue\n"
      }
    },
    {
      "type": "relationship.transition.append",
      "relationship": "tmp:trust",
      "transition": {
        "id": "tmp:trust-raised",
        "time": {
          "timeline": "main",
          "tick": "300",
          "order": "0"
        },
        "metrics": {
          "trust": 0.8
        },
        "causing_event": "tmp:rescue"
      }
    },
    {
      "type": "story-point.transition.append",
      "storyPoint": "tmp:plot",
      "transition": {
        "id": "tmp:resolved",
        "time": {
          "timeline": "main",
          "tick": "301",
          "order": "0"
        },
        "state": "resolved",
        "causing_event": "tmp:rescue"
      }
    },
    {
      "type": "outcome.link",
      "event": "tmp:rescue",
      "storyPoints": [
        "tmp:plot"
      ],
      "scenes": [
        "tmp:scene"
      ]
    },
    {
      "type": "entity.update",
      "entity": "Mara Vale",
      "bodyMarkdown": "An unrelated blue-coat description.\n"
    },
    {
      "type": "expectation.check",
      "event": "tmp:rescue",
      "at": {
        "timeline": "main",
        "tick": "302",
        "order": "0"
      },
      "policy": "required",
      "items": [
        {
          "id": "custody",
          "predicate": {
            "kind": "state.equals",
            "target": "tmp:ledger",
            "key": "holder",
            "value": {
              "entity": "Mara Vale"
            }
          }
        },
        {
          "id": "belief",
          "predicate": {
            "kind": "knowledge.state",
            "knowledge": "tmp:belief",
            "state": "accepted"
          }
        },
        {
          "id": "trust",
          "predicate": {
            "kind": "relationship.matches",
            "relationship": "tmp:trust",
            "values": {
              "metrics": {
                "trust": 0.8
              }
            }
          }
        },
        {
          "id": "plot",
          "predicate": {
            "kind": "story-point.state",
            "storyPoint": "tmp:plot",
            "state": "resolved"
          }
        },
        {
          "id": "scene",
          "predicate": {
            "kind": "outcome.linked",
            "event": "tmp:rescue",
            "target": "tmp:scene"
          }
        }
      ]
    }
  ]
}
```

Preview it with:

```powershell
wedl author request preview rescue.json --repo PATH
```

Python callers use `authoring.preview_intent(repository, intent)`. For HTTP,
obtain `GET /api/session` and POST the JSON object to `/api/authoring/preview`
with `X-Wedl-Token`.

Inspect `preview.files`, `diff`, `diagnostics` and `confirmationToken`.
Read `preview.generatedIds` for `tmp:rescue`, `tmp:custody`, `tmp:learned`,
`tmp:trust-raised` and `tmp:resolved`; use those returned IDs when following up.
In `preview.semanticDelta`, compare the rescue's `eventGroups` with
`unattributedRecordChanges`: the separate blue-coat prose belongs to the latter.
Inspect the five requested results in `preview.expectationChecks` and resolve
a blocked result before applying. To inspect current or delayed consequences
in the reader, follow the [reader guide](event-consequences-reader.md).

Keep the complete original request unchanged and apply with the returned author
preview token:

```powershell
wedl author request apply rescue.json --repo PATH --confirm TOKEN
```

HTTP callers POST the same object to `/api/authoring/apply` with
`X-Wedl-Token` and `X-Wedl-Confirmation`. Use the author preview token returned
for this request. Preview again after editing a name, policy or horizon.

For the architectural contract, search in the WEDL development/source checkout:
`adrai --repo WEDL_SOURCE_CHECKOUT search 'Event consequences and candidate verification' --mode fts --json`.
