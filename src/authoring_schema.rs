//! Repository-free guidance for the existing native authoring codecs.
//!
//! The self-contained JSON Schema describes a consequence Plan's JSON shape.
//! Embedded YAML, exact typed operands, complete byte budgets and source/domain
//! validity still belong to the native decoders and candidate preview. This
//! module neither supplies source choices nor constructs or publishes a Plan.
use crate::{consequence_semantics, consequences, generational, record_edit, source, validation};
use serde_json::{Map, Value as Json, json};

fn reference(name: &str) -> Json {
    json!({"$ref":format!("#/$defs/{name}")})
}

fn object(properties: &[(&str, Json)], required: &[&str]) -> Json {
    json!({
        "type":"object", "additionalProperties":false, "required":required,
        "properties":properties.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect::<Map<_,_>>(),
    })
}

fn text(maximum_bytes: usize, nonblank: bool) -> Json {
    let mut result = json!({
        "type":"string", "maxLength":maximum_bytes,
        "x-nativeMaxUtf8Bytes":maximum_bytes,
    });
    if nonblank {
        result["minLength"] = json!(1);
        // JSON Schema's regex whitespace and length rules are not Rust's
        // Unicode trim and UTF-8 byte rules. Do not reject valid native text
        // by claiming an approximate regular expression is equivalent.
        result["x-nativeNonblank"] = json!(true);
    }
    result
}

/// Positive canonical decimal integers no larger than a literal bound. The
/// alternatives enumerate shorter lengths and the first smaller decimal digit;
/// no floating-point conversion enters the generated schema.
fn positive_decimal(maximum: u64) -> String {
    let digits = maximum.to_string();
    let mut alternatives = Vec::new();
    if digits.len() > 1 {
        alternatives.push(format!("[1-9][0-9]{{0,{}}}", digits.len() - 2));
    }
    for (index, digit) in digits.bytes().enumerate() {
        let first = if index == 0 { b'1' } else { b'0' };
        if digit > first {
            let range = if first + 1 == digit {
                char::from(first).to_string()
            } else {
                format!("[{}-{}]", char::from(first), char::from(digit - 1))
            };
            let remaining = digits.len() - index - 1;
            let tail = if remaining == 0 {
                String::new()
            } else {
                format!("[0-9]{{{remaining}}}")
            };
            alternatives.push(format!("{}{range}{tail}", &digits[..index]));
        }
    }
    alternatives.push(digits);
    format!("(?:{})", alternatives.join("|"))
}

fn decimal_integer(minimum: i64, maximum: i64) -> Json {
    json!({
        "type":"string",
        "pattern":format!("^(?:0|{}|-{})(?![\\s\\S])", positive_decimal(maximum as u64), positive_decimal(minimum.unsigned_abs())),
        "maxLength":minimum.to_string().len(),
        "description":"Canonical signed decimal integer string; no plus sign, leading zeros, negative zero or surrounding whitespace.",
        "x-nativeMinimum":minimum.to_string(), "x-nativeMaximum":maximum.to_string(),
    })
}

fn predicate(kind: &str, properties: &[(&str, Json)]) -> Json {
    let mut fields = vec![("kind", json!({"const":kind}))];
    fields.extend_from_slice(properties);
    let required = fields.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    object(&fields, &required)
}

fn request_schema() -> Json {
    let fingerprint =
        json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"});
    let record_id = json!({
        "type":"string", "minLength":1, "x-nativeRecordId":true,
        "description":"An exact native record ID. The native decoder checks kind prefixes, Crockford IDs and namespaced spatial paths, including reserved path components. Actual target kinds and existence are checked during candidate analysis.",
    });
    let mut raw_yaml = text(record_edit::MAX_BYTES, true);
    raw_yaml["contentMediaType"] = json!("application/yaml");
    raw_yaml["description"] = json!(
        "Retained typed YAML request text. Creation requires kind/id/title and kind-specific fields; update requires kind. See recordKinds guidance; native parsing and complete candidate validation remain required."
    );

    let mut raw_value = text(65_536, false);
    raw_value["minLength"] = json!(1);
    raw_value["contentMediaType"] = json!("application/json");
    raw_value["description"] = json!(
        "Exact authored JSON operand text, not an already-decoded JSON value. Native decoding rejects duplicate keys, non-finite numbers and numeric tokens that lose precision in the native representation."
    );
    let metric = json!({
        "type":"string", "minLength":1, "maxLength":256,
        "pattern":"^-?(?:0|[1-9][0-9]*)(?:\\.[0-9]+)?(?:[eE][+-]?[0-9]+)?(?![\\s\\S])",
        "description":"Retained decimal JSON-number token. Native validation additionally requires exponent -4096..4096 and finite f64 conversion.",
    });
    let mut relationship_values = object(
        &[
            ("status", text(256, true)),
            (
                "metrics",
                json!({"type":"object","maxProperties":256,"propertyNames":text(256,true),"additionalProperties":metric}),
            ),
            (
                "facets",
                json!({"type":"array","maxItems":1024,"items":text(4096,false)}),
            ),
        ],
        &[],
    );
    relationship_values["minProperties"] = json!(1);
    relationship_values["description"] = json!(
        "Select at least one field. Omission differs from null; null is refused. Empty metric maps and facet arrays are valid selected values. Facets preserve duplicates and empty strings."
    );

    let variants = [
        (
            "StateEquals",
            predicate(
                "state.equals",
                &[
                    ("targetId", reference("RecordId")),
                    ("key", text(256, true)),
                    ("expectedValueRaw", raw_value),
                ],
            ),
        ),
        (
            "StateAbsent",
            predicate(
                "state.absent",
                &[
                    ("targetId", reference("RecordId")),
                    ("key", text(256, true)),
                ],
            ),
        ),
        (
            "KnowledgeState",
            predicate(
                "knowledge.state",
                &[
                    ("knowledgeId", reference("RecordId")),
                    ("state", json!({"enum":record_edit::KNOWLEDGE_STATES})),
                ],
            ),
        ),
        (
            "RelationshipMatches",
            predicate(
                "relationship.matches",
                &[
                    ("relationshipId", reference("RecordId")),
                    ("values", reference("RelationshipValues")),
                ],
            ),
        ),
        (
            "StoryPointState",
            predicate(
                "story-point.state",
                &[
                    ("storyPointId", reference("RecordId")),
                    ("state", json!({"enum":record_edit::STORY_STATES})),
                ],
            ),
        ),
        (
            "OutcomeLinked",
            predicate(
                "outcome.linked",
                &[
                    ("eventId", reference("RecordId")),
                    ("targetId", reference("RecordId")),
                ],
            ),
        ),
        (
            "TransitionCaused",
            predicate(
                "transition.caused",
                &[
                    ("recordId", reference("RecordId")),
                    ("transitionId", text(256, true)),
                    ("eventId", reference("RecordId")),
                ],
            ),
        ),
    ];
    let mut definitions = Map::new();
    definitions.insert("SourceFingerprint".into(), fingerprint);
    definitions.insert("RecordId".into(), record_id);
    definitions.insert(
        "Signed64Decimal".into(),
        decimal_integer(i64::MIN, i64::MAX),
    );
    definitions.insert(
        "Signed32Decimal".into(),
        decimal_integer(i64::from(i32::MIN), i64::from(i32::MAX)),
    );
    definitions.insert(
        "StoryTime".into(),
        object(
            &[
                ("timeline", text(128, true)),
                ("tick", reference("Signed64Decimal")),
                ("order", reference("Signed32Decimal")),
            ],
            &["timeline", "tick", "order"],
        ),
    );
    definitions.insert("RelationshipValues".into(), relationship_values);
    definitions.insert(
        "ExpectationPredicate".into(),
        json!({"oneOf":variants.iter().map(|(name,_)|reference(name)).collect::<Vec<_>>()}),
    );
    for (name, value) in variants {
        definitions.insert(name.into(), value);
    }
    definitions.insert(
        "Expectation".into(),
        object(
            &[
                ("id", text(128, true)),
                ("predicate", reference("ExpectationPredicate")),
            ],
            &["id", "predicate"],
        ),
    );
    definitions.insert("Analysis".into(),object(&[
        ("at",reference("StoryTime")),
        ("policy",json!({"enum":[consequence_semantics::Policy::Required,consequence_semantics::Policy::Advisory]})),
        ("expectations",json!({"type":"array","maxItems":consequence_semantics::MAX_EXPECTATIONS,"uniqueItems":true,"x-nativeUniqueBy":"id","items":reference("Expectation")})),
    ], &["at","policy","expectations"]));
    definitions.insert(
        "RecordCreate".into(),
        object(
            &[
                ("type", json!({"const":"record.create"})),
                ("draft", raw_yaml.clone()),
            ],
            &["type", "draft"],
        ),
    );
    definitions.insert(
        "RecordUpdate".into(),
        object(
            &[
                ("type", json!({"const":"record.update"})),
                ("entityId", reference("RecordId")),
                ("patch", raw_yaml),
            ],
            &["type", "entityId", "patch"],
        ),
    );
    definitions.insert(
        "ConsequenceOperation".into(),
        json!({"oneOf":[reference("RecordCreate"),reference("RecordUpdate")]}),
    );
    definitions.insert("ConsequencePlan".into(),object(&[
        ("expectedFingerprint",reference("SourceFingerprint")),
        ("operations",json!({"type":"array","minItems":1,"maxItems":consequences::MAX_OPERATIONS,"items":reference("ConsequenceOperation")})),
        ("analysis",reference("Analysis")),
    ], &["expectedFingerprint","operations"]));
    json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "title":"Native WEDL consequence Plan",
        "description":"Structural JSON guidance. Native decoding and candidate preview additionally enforce the checks listed by authoring.schema.nativeChecks. Omitted analysis is allowed; explicit null is not.",
        "$ref":"#/$defs/ConsequencePlan", "$defs":definitions,
    })
}

/// Deterministic, repository-free guidance. It contains no selected source,
/// fingerprint, entity choices or implied permission to publish.
pub fn schema() -> Json {
    let histories = generational::GENERATIONAL_KINDS.iter().map(|kind| {
        let names = generational::transition_kinds(kind);
        ((*kind).to_owned(),json!({
            "initialization":names[0], "transitions":&names[1..],
            "applicability":"instant, except tenure-vacate uses inclusive-interval",
            "coordinates":"YAML integer tick (i64) and order (i32), not the analysis JSON decimal-string representation",
        }))
    }).collect::<Map<_,_>>();
    json!({
        "protocol":crate::CONTRACT, "operation":"authoring.schema", "sourceSchema":source::SOURCE_SCHEMA,
        "sourceIndependent":true, "writesPerformed":false, "validationComplete":false,
        "requestSchema":request_schema(),
        "recordKinds":record_edit::kind_guidance(),
        "vocabularies":{
            "capabilities":validation::CAPABILITIES,
            "knowledgeStates":record_edit::KNOWLEDGE_STATES,
            "knowledgeAcquisitions":record_edit::ACQUISITIONS,
            "storyPointStates":record_edit::STORY_STATES,
            "generational":histories,
        },
        "bounds":{
            "maxPlanBytes":consequences::MAX_PLAN_BYTES, "maxPlanOperations":consequences::MAX_OPERATIONS,
            "maxRecordInputBytes":record_edit::MAX_BYTES, "maxExpandedYamlBytes":record_edit::MAX_BYTES,
            "maxExpandedYamlNodes":record_edit::MAX_YAML_NODES, "maxExpandedYamlDepth":128,
            "maxAnalysisBytes":consequence_semantics::MAX_ANALYSIS_BYTES,
            "maxExpectations":consequence_semantics::MAX_EXPECTATIONS,
            "maxPreviewBytes":consequences::MAX_PREVIEW_BYTES,
            "minPreviewCharacters":consequences::MIN_CHARACTERS,"maxPreviewCharacters":consequences::MAX_CHARACTERS,
        },
        "nativeChecks":[
            "JSON Schema describes decoded JSON. The native decoder also rejects duplicate object keys, invalid UTF-8 and trailing input.",
            "Complete encoded JSON byte limits and aggregate operation-input budgets are enforced natively. JSON Schema maxLength counts Unicode characters; x-nativeMaxUtf8Bytes describes the stricter UTF-8 byte bound. Nonblank text uses native Unicode trim rules.",
            "RecordId syntax is checked by the native decoder, including portable kind prefixes, Crockford identifiers and namespaced spatial paths. A syntactically valid ID does not prove existence, target kind or admission.",
            "Expectation IDs are unique across the complete list. Raw JSON operands retain exact numeric tokens and require bounded duplicate-free, finite, representable values; metric tokens also have exponent and finite-conversion checks.",
            "Retained draft/patch strings use the current typed YAML codec. Unknown fields, duplicate keys, unsupported nulls/tags and complete expanded-source bounds are checked before candidate construction. recordKinds lists top-level fields and literal creation defaults, not a complete nested domain schema.",
            "Create requires the listed fields; domain validation may impose conditional requirements such as an overlay validity interval. World records are update-only. Update requires kind, preserves omitted fields, replaces complete supplied values and does not accept id/schema/path changes.",
            "The current source fingerprint, declared timelines, typed state/profile definitions, references, capabilities, temporal/lifecycle rules and complete final candidate still require native preview validation. A schema match never approves or publishes a change.",
        ],
    })
}
