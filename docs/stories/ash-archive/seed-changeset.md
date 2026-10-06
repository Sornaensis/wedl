# Change-Set Example

> Historical tick121 seed material. Maintained packaged Ash is completed at tick208; this page preserves the earlier narrative/IDs, not a current active-world inventory.
This is a historical, preview-only tick121 seed illustration of temporary IDs and a scene outcome. The JSON is preserved unchanged. It closes After the Exchange at tick122, but the maintained completed package already closes that scene at tick124 and has later events; this payload does not promise a valid apply there. The satchel choice also requires author review. Do not change story source to make this old illustration pass.

```json
{
  "protocol": "wedl-changeset/v1",
  "expectedHead": "<replace-with-current-head>",
  "idempotencyKey": "ash-archive-after-exchange-hide-letter-v1",
  "summary": "Mara hides the sealed letter as Nessa enters the Reading Room.",
  "operations": [
    {
      "type": "event.create",
      "temporaryId": "$tmp.event.hide-letter",
      "title": "Mara Conceals the Heron Letter",
      "domain": "plot.letter",
      "time": {
        "timeline": "main",
        "tick": 122,
        "order": 10
      },
      "location": "loc_0E434X5XGQVW7S6T5Z8H5RAP68",
      "participants": [
        {
          "character": "char_00HBQM4T4CMF11RKMNDBRP92QC",
          "role": "actor"
        },
        {
          "character": "char_0PWVJ91SCDP5TFAJRAGNPXDAEB",
          "role": "witness"
        },
        {
          "character": "char_0T8KVZX33X5FQJFYPMH70FADD9",
          "role": "arrival"
        }
      ],
      "causes": [
        "event_0H62XBG85QV4B5ZJR8DAS3PBYD"
      ],
      "effects": [
        {
          "target": "obj_0AZ9FQZCZC8ZR175BPRGDFQHEJ",
          "key": "holder",
          "operation": "clear"
        },
        {
          "target": "obj_0AZ9FQZCZC8ZR175BPRGDFQHEJ",
          "key": "container",
          "operation": "set",
          "value": {
            "entity": "obj_0RQ5A8VJT08HHSJ8FM6KJX0KJM"
          }
        }
      ],
      "body": "Nessa enters as Mara moves the sealed letter out of sight. This fixture operation intentionally requires review because placing it in Oren’s satchel may conflict with the intended prose; a valid test variant can instead create a dedicated archive case."
    },
    {
      "type": "knowledge.transition",
      "knowledge": {
        "create": {
          "temporaryId": "$tmp.knowledge.nessa-mara-hiding",
          "knower": "char_0T8KVZX33X5FQJFYPMH70FADD9",
          "claim": {
            "key": "mara.hiding.archive-evidence",
            "statement": "Mara is hiding archive evidence from Nessa.",
            "subject": "char_00HBQM4T4CMF11RKMNDBRP92QC",
            "predicate": "hiding",
            "object": {
              "text": "archive evidence"
            },
            "authorTruthStatus": "unknown"
          }
        }
      },
      "transition": {
        "state": "suspected",
        "confidence": 0.55,
        "acquisition": "observed",
        "causingEvent": "$tmp.event.hide-letter",
        "sourceEntity": "char_00HBQM4T4CMF11RKMNDBRP92QC"
      }
    },
    {
      "type": "scene.update",
      "scene": "scene_0FVWCX929V1WYY31NEP6Q1B0BX",
      "patch": {
        "status": "closed",
        "end": {
          "timeline": "main",
          "tick": 122,
          "order": 30
        }
      }
    }
  ]
}
```

For a current authoring session, start from the actual repository HEAD:

```text
wedl changeset scaffold --repo PATH --output change.json
wedl changeset schema --repo PATH
wedl changeset preview change.json --repo PATH
wedl changeset apply change.json --repo PATH --confirm PREVIEW_TOKEN
```

Inspect and edit the scaffold's operation for the current scene while retaining its current-HEAD guard; choose an appropriate idempotency key and use the exact token from the unchanged successful preview. See the [command guide](../../guides/command-line.md).
