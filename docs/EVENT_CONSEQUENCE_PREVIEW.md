# Reviewing an explicit consequence preview

A consequence batch records the facts the author supplies. The rescue below moves
ledger custody through the declared `object.holder` entity key, adds Mara's
accepted belief, records Mara's directional trust toward Oren, resolves a story
point one tick later, and links the event to that story point and a scene.
Acceptance of the belief records Mara's belief; it does not establish its claim as
canonical truth. The independent blue-coat prose edit remains unattributed.

The example names match the Ash Archive seed. Its zero `expectedHead` is a
placeholder, not a Git revision claim. Replace it with the exact session HEAD.
Use authenticated contextual `changeset schema` discovery at that revision to
check the installed source schema, holder reference kind and trust bounds.
Replace the source `schema` members if that world uses a different supported
version. Match names/declarations in another world before previewing.

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

Save that object as `rescue.json`. Direct Python callers use
`authoring.preview_intent(repository, intent)`. CLI callers run
`wedl author request preview rescue.json --repo PATH`. HTTP callers first obtain
`GET /api/session`, then POST the JSON object itself to
`/api/authoring/preview` with `X-Wedl-Token`. The same executable example is
published in OpenAPI; it contains no request-body authorization or scope grants.

An author response retains `wedl-author-preview/v1`, the complete original
`intent`, the resolved raw `changeset`, `authorImpact`, and its nested
`wedl-preview/v1` preview. Ordinary `files`, `diff`, `diagnostics`,
`generatedIds`, `requestHash` and `confirmationToken` remain authoritative.
The optional `preview.semanticDelta` uses
`wedl-event-consequence-delta/v1`. Raw `changeset preview` returns the same
delta under its own `semanticDelta` member.

Read generated references from the response rather than guessing them:
`preview.generatedIds["tmp:rescue"]` is the actual allocated event ID;
`tmp:custody`, `tmp:learned`, `tmp:trust-raised` and `tmp:resolved` similarly
resolve to effect and transition IDs. Each delta citation carries a source
revision/blob identity or a candidate `{baseRevision, requestHash}`. A candidate
has no commit SHA until an actual write. The delta's request hash matches the
returned raw preview hash, while author confirmation also binds the original
name-based intent.

At H = `{"timeline":"main","tick":"302","order":"0"}`, every delta subject
compares the full base and final candidate at that same H. Event groups attribute
those changes only through literal effects, causes and outcome links. The delayed
plot resolution at tick 301 belongs to the rescue group; static creation fields
and the unrelated coat prose remain in `unattributedRecordChanges`. Directional
trust preserves `from=Mara`, `to=Oren`; it creates no reverse relationship.
An empty net semantic change still has an operation record delta where applicable.

A separate event report compares before/after at the event's T = tick 300.
Its before view excludes that event's own effects and transitions literally
caused by it at T, while retaining disjoint equal-coordinate events and uncaused
transitions. It keeps the event record and never calculates a numeric predecessor,
including at signed time extrema. Thus the delayed plot resolution is in the
same-H revision delta and historical caused transitions, but not the event-local
T change. The [shared contract](EVENT_CONSEQUENCES_CONTRACT.md) defines both views.

`preview.expectationChecks` reports this batch's five explicit required checks.
Its ordered groups identify the operation, event, evaluation coordinate and
policy, with typed comparison identities, actuals and citations. A non-pass
required check blocks apply; an advisory non-pass remains visible and permits
otherwise valid accompanying writes. Missing or inaccessible evidence is unknown,
not a false claim of absence. Checks do not prove narrative completeness.

Semantic `invalid`, `unavailable` and `limit` outcomes are closed failures,
containing only protocol, outcome, code and message; they never return partial
assertions. Structural invalidity keeps ordinary source diagnostics and prevents
candidate semantic folding. A valid source preview can still have an unavailable
or limited semantic result. Omit `consequenceRequest` to omit the delta;
expectation-only reporting does not depend on it. A check-only no-op returns
`status:"checked"`, equal previous/new HEAD, no touched records and no compile.

Apply requires the unchanged complete original intent and the returned author
preview token. CLI uses `wedl author request apply rescue.json --repo PATH --confirm TOKEN`;
HTTP uses `POST /api/authoring/apply` with both `X-Wedl-Token` and
`X-Wedl-Confirmation`. Do not substitute the raw changeset token for the author
token. Editing names, policy or H requires another preview. Identical confirmed
retries return the saved receipt without another revision.

The focused parity tests execute this example, validate real responses against
the published shared JSON Schemas, compare direct/CLI/HTTP serialization, check
actual generated IDs and provenance, and verify preview/refusal leaves source and
cache bytes unchanged. No illustration invents an applied commit SHA.
