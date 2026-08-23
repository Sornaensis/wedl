# Change-Set Example

The following illustrates temporary IDs and a scene outcome. It is deliberately marked for preview because the chosen container may not match the author’s intended prose; this demonstrates that validation/preview is not merely ceremonial.

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
