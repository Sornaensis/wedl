//! The typed private browser reuses native source pins and the guarded writer.
#[path = "support/actions.rs"]
mod fixture;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wedl::{http, service::Service, source};

const RECEIPT: &str = "application/vnd.wedl.workbench+json";
const EDIT_SAVE: &str = "/api/edit-drafts/save";
const EDIT_APPLY: &str = "/api/edit-drafts/apply";
const ACTION_SAVE: &str = "/api/action-drafts/save";
const ACTION_APPLY: &str = "/api/action-drafts/apply-outcome";

async fn call(app: &Router, method: &str, path: &str, body: &str) -> (StatusCode, Value) {
    call_with_accept(app, method, path, body, RECEIPT).await
}

async fn call_with_accept(
    app: &Router,
    method: &str,
    path: &str,
    body: &str,
    accept: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8765");
    if method == "POST" {
        request = request
            .header("origin", "http://localhost:8765")
            .header("content-type", "application/json")
            .header("x-wedl-write", "1")
            .header("accept", accept);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn read(app: &Router, endpoint: &str, fields: &[(&str, &str)]) -> (StatusCode, Value) {
    let query = fields
        .iter()
        .map(|(key, value)| {
            format!(
                "{key}={}",
                value
                    .bytes()
                    .map(|byte| format!("%{byte:02X}"))
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    call(
        app,
        "GET",
        &format!("/api/workbench/{endpoint}?{query}"),
        "",
    )
    .await
}
async fn get(app: &Router, endpoint: &str, fields: &[(&str, &str)]) -> Value {
    let (status, value) = read(app, endpoint, fields).await;
    assert_eq!(status, StatusCode::OK, "{endpoint}: {value}");
    value
}
fn value<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap()
}
fn token<'a>(items: &'a Value, name: &str) -> &'a str {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|row| value(row, "label").contains(name))
        .map(|row| value(row, "token"))
        .unwrap()
}

async fn assert_inspection_rejects_reformatted_terminal(
    app: &Router,
    world: &wedl::model::World,
    request: &str,
    route: &str,
    kind: &str,
    id: &str,
) {
    let path = world.root.join(format!("author/{kind}/{id}.json"));
    let exact = std::fs::read(&path).unwrap();
    let document = serde_json::from_slice::<Value>(&exact).unwrap();
    let reformatted = serde_json::to_vec(&document).unwrap();
    assert_ne!(reformatted, exact);
    std::fs::write(&path, &reformatted).unwrap();

    let inspected = get(app, "inspect", &[("route", route), ("token", request)]).await;
    assert_eq!(inspected["verified"], false);
    assert!(inspected["semanticVerification"].is_null());
    assert_eq!(std::fs::read(path).unwrap(), reformatted);
}

#[tokio::test]
async fn named_frame_round_trips_exact_time_and_restores_without_exposing_its_pins() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let choices = get(&app, "writing-choices", &[]).await;
    let selected = token(&choices["moments"], "Scene position");
    let scene = token(&choices["scenes"], "Fixture scene");
    let viewpoint = token(&choices["viewpoints"], "Fixture character");
    let frame = get(
        &app,
        "writing-frame",
        &[
            ("moment", selected),
            ("scene", scene),
            ("viewpoint", viewpoint),
            ("lens", "dramatic-irony"),
        ],
    )
    .await;
    assert!(frame["characterPrompt"].is_array() && frame["authorMargin"].is_array());
    for raw in [&choices, &frame] {
        let rendered = raw.to_string();
        for forbidden in [
            world.fingerprint.as_str(),
            fixture::ACTOR,
            fixture::SCENE,
            &fixture::TICK.to_string(),
            "effectiveTime",
            "povId",
        ] {
            assert!(!rendered.contains(forbidden), "leaked {forbidden}");
        }
    }
    let file = get(&app, "file", &[("token", value(&frame, "file"))]).await;
    let decoded = wedl::writing_frames::decode(value(&file, "bytes")).unwrap();
    assert_eq!(decoded.at, fixture::horizon());
    let restored = get(
        &app,
        "writing-frame",
        &[("frame", value(&frame, "restore"))],
    )
    .await;
    assert_eq!(restored["moment"], frame["moment"]);
    let after = get(&app, "file", &[("token", value(&restored, "file"))]).await;
    assert_eq!(after, file);
    fixture::change_source(&world);
    let (status, denied) = read(
        &app,
        "writing-frame",
        &[("moment", selected), ("lens", "author")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!denied.to_string().contains(&world.fingerprint));
}

#[tokio::test]
async fn story_condition_preview_keeps_every_changed_operand() {
    let (_directory, world) = fixture::fixture();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["state_keys"] = serde_yaml::from_str("character:\n  score: {type: integer}\n").unwrap();
    std::fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    let other = "char_00000000000000000000000002";
    let point = "sp_00000000000000000000000001";
    std::fs::write(
        world.root.join("story/other.md"),
        format!("---\nschema: wedl/v1\nkind: character\nid: {other}\ntitle: Second character\ninitial_state: {{}}\n---\n"),
    )
    .unwrap();
    std::fs::write(
        world.root.join("story/point.md"),
        format!("---\nschema: wedl/v1\nkind: story-point\nid: {point}\ntitle: Fixture story point\nactivation_policy: suggest\nlifecycle: {{initial_state: dormant, transitions: []}}\nwhen: {{state: {{entity: {}, key: score, equals: 1}}}}\n---\n", fixture::ACTOR),
    )
    .unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let app = http::live_router(Service::new(world.clone()));
    let plan = json!({"expectedFingerprint":world.fingerprint,
        "operations":[{"type":"record.update","entityId":point,
            "patch":format!("kind: story-point\nwhen: {{state: {{entity: {other}, key: score, equals: 2}}}}\n")}]})
    .to_string();
    let preview = get(
        &app,
        "writing-exchange",
        &[("kind", "plan"), ("raw", &plan)],
    )
    .await;
    let change = &preview["changes"][0];
    assert_eq!(change["complete"], true, "{preview}");
    let condition = &change["fields"][0];
    let members = |side: &str| {
        condition[side]["value"][0]["value"]["value"]
            .as_array()
            .unwrap()
    };
    for (side, entity, equals) in [
        ("before", "Fixture character", "1"),
        ("after", "Second character", "2"),
    ] {
        let members = members(side);
        assert!(
            members
                .contains(&json!({"label":"Entity","value":{"kind":"reference","value":entity}}))
        );
        assert!(members.contains(&json!({"label":"Key","value":{"kind":"text","value":"score"}})));
        assert!(
            members.contains(&json!({"label":"Equals","value":{"kind":"integer","value":equals}}))
        );
    }
    assert!(!preview.to_string().contains(other));
    assert!(!preview.to_string().contains(fixture::ACTOR));
}

#[tokio::test]
async fn retained_edit_requires_current_complete_review_and_emits_typed_native_receipt() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let fixed = http::router(Service::new(world.clone()));
    assert_eq!(get(&fixed, "source", &[]).await["writes"], false);
    assert_eq!(
        call(&fixed, "POST", EDIT_SAVE, "{}").await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(get(&app, "source", &[]).await["writes"], true);
    let choices = get(&app, "edit-list", &[]).await;
    let target = token(&choices["targets"], "Fixture location");
    let prose = "New retained prose 🌲\r\nExact trailing spaces  ";
    let prepared = get(
        &app,
        "edit-save",
        &[
            ("target", target),
            ("operation", "replace-prose"),
            ("text", prose),
        ],
    )
    .await;
    let request = value(&prepared, "request");
    assert_eq!(
        read(
            &app,
            "native-request",
            &[("route", EDIT_APPLY), ("token", request)]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let native = get(
        &app,
        "native-request",
        &[("route", EDIT_SAVE), ("token", request)],
    )
    .await;
    assert_eq!(
        call(&app, "POST", EDIT_SAVE, value(&native, "bytes"))
            .await
            .0,
        StatusCode::OK
    );
    let choices = get(&app, "edit-list", &[]).await;
    let selected = token(&choices["drafts"], "Edit Fixture location");
    let review = get(&app, "edit-review", &[("draft", selected)]).await;
    assert_eq!(review["canApply"], true, "{review}");
    assert_eq!(review["changes"][0]["fields"][0]["label"], "Prose");
    assert_eq!(
        review["changes"][0]["fields"][0]["after"],
        json!({"kind":"text","value":prose})
    );
    assert!(!review.to_string().contains(fixture::PLACE));
    assert!(!review.to_string().contains(&world.fingerprint));
    let ticket = value(&review, "approval");
    assert_eq!(
        read(&app, "approve", &[("candidate", ticket), ("approved", "0")])
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let ready = get(&app, "approve", &[("candidate", ticket), ("approved", "1")]).await;
    assert_eq!(ready["route"], EDIT_APPLY);
    let request = value(&ready, "request");
    let native = get(
        &app,
        "native-request",
        &[("route", EDIT_APPLY), ("token", request)],
    )
    .await;
    let (status, receipt) = call(&app, "POST", EDIT_APPLY, value(&native, "bytes")).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["confirmed"], true);
    assert_eq!(receipt["sourceMatches"], true);
    assert!(receipt.get("fingerprint").is_none() && receipt.get("result").is_none());
    let captured = source::load_world(&world.root, None).unwrap();
    assert_eq!(captured.records[fixture::PLACE].body, prose);
    assert_eq!(
        get(
            &app,
            "inspect",
            &[("route", EDIT_APPLY), ("token", request)]
        )
        .await["verified"],
        true
    );
    assert_eq!(
        read(&app, "approve", &[("candidate", ticket), ("approved", "1")])
            .await
            .0,
        StatusCode::CONFLICT
    );
    let applied = wedl::edit_drafts::decode_apply(value(&native, "bytes")).unwrap();
    assert_inspection_rejects_reformatted_terminal(
        &app,
        &world,
        request,
        EDIT_APPLY,
        "edits",
        &applied.edit_id,
    )
    .await;
}

#[tokio::test]
async fn replacing_saved_candidate_bytes_invalidates_review_before_workbench_approval() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let choices = get(&app, "edit-list", &[]).await;
    let target = token(&choices["targets"], "Fixture location");
    let ready = get(
        &app,
        "edit-save",
        &[
            ("target", target),
            ("operation", "replace-prose"),
            ("text", "First body"),
        ],
    )
    .await;
    let native = get(
        &app,
        "native-request",
        &[("route", EDIT_SAVE), ("token", value(&ready, "request"))],
    )
    .await;
    assert_eq!(
        call(&app, "POST", EDIT_SAVE, value(&native, "bytes"))
            .await
            .0,
        StatusCode::OK
    );
    let choices = get(&app, "edit-list", &[]).await;
    let selected = token(&choices["drafts"], "Edit Fixture location");
    let review = get(&app, "edit-review", &[("draft", selected)]).await;
    assert_eq!(review["canApply"], true);
    let replacement = get(
        &app,
        "edit-save",
        &[
            ("draft", selected),
            ("operation", "replace-prose"),
            ("text", "Different body"),
        ],
    )
    .await;
    let native = get(
        &app,
        "native-request",
        &[
            ("route", EDIT_SAVE),
            ("token", value(&replacement, "request")),
        ],
    )
    .await;
    assert_eq!(
        call(&app, "POST", EDIT_SAVE, value(&native, "bytes"))
            .await
            .0,
        StatusCode::OK
    );
    let (status, _) = read(
        &app,
        "approve",
        &[("candidate", value(&review, "approval")), ("approved", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[tokio::test]
async fn native_conflict_has_unconfirmed_workbench_outcome_and_unchanged_machine_response() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let request = serde_json::to_string(&fixture::request(
        &world,
        fixture::draft(&world, &fixture::id(12)),
        None,
    ))
    .unwrap();
    fixture::change_source(&world);

    let (status, typed) = call(&app, "POST", ACTION_SAVE, &request).await;
    assert_eq!(status, StatusCode::CONFLICT, "{typed}");
    assert_eq!(typed["outcome"], "unconfirmed");
    let (status, machine) =
        call_with_accept(&app, "POST", ACTION_SAVE, &request, "application/json").await;
    assert_eq!(status, StatusCode::CONFLICT, "{machine}");
    assert!(machine["code"].is_string());
    assert!(machine.get("outcome").is_none());

    let (status, refused) = call(&app, "POST", ACTION_SAVE, "{}").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(refused["outcome"], "refused");
}

#[tokio::test]
async fn private_save_inspection_requires_exact_saved_bytes_and_current_submitted_source() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let choices = get(&app, "writing-choices", &[]).await;
    let frame = get(
        &app,
        "writing-frame",
        &[
            ("moment", token(&choices["moments"], "Current story time")),
            ("lens", "author"),
        ],
    )
    .await;
    let choices = get(&app, "action-list", &[]).await;
    let action = get(
        &app,
        "action-save",
        &[
            ("title", "Verify exact action bytes"),
            ("intent", "Retain the exact action"),
            ("actor", token(&choices["actors"], "Fixture character")),
            ("target", token(&choices["targets"], "Fixture location")),
            ("frame", value(&frame, "frame")),
        ],
    )
    .await;
    let choices = get(&app, "edit-list", &[]).await;
    let edit = get(
        &app,
        "edit-save",
        &[
            ("target", token(&choices["targets"], "Fixture location")),
            ("operation", "replace-prose"),
            ("text", "Verify exact retained edit bytes"),
        ],
    )
    .await;
    for (route, prepared, directory) in [
        (ACTION_SAVE, &action, "author/actions"),
        (EDIT_SAVE, &edit, "author/edits"),
    ] {
        let fields = [("route", route), ("token", value(prepared, "request"))];
        let native = get(&app, "native-request", &fields).await;
        let request = value(&native, "bytes");
        let (status, receipt) = call(&app, "POST", route, request).await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert_eq!(get(&app, "inspect", &fields).await["verified"], true);

        let decoded: Value = serde_json::from_str(request).unwrap();
        let file = world
            .root
            .join(directory)
            .join(format!("{}.json", value(&decoded["draft"], "id")));
        let exact = std::fs::read(&file).unwrap();
        let mut reformatted = exact.clone();
        reformatted.extend_from_slice(b"\n ");
        assert_eq!(
            serde_json::from_slice::<Value>(&exact).unwrap(),
            serde_json::from_slice::<Value>(&reformatted).unwrap()
        );
        std::fs::write(&file, reformatted).unwrap();
        assert_eq!(get(&app, "inspect", &fields).await["verified"], false);
        std::fs::write(&file, exact).unwrap();
        assert_eq!(get(&app, "inspect", &fields).await["verified"], true);
    }
    fixture::change_source(&world);
    for (route, prepared) in [(ACTION_SAVE, &action), (EDIT_SAVE, &edit)] {
        let fields = [("route", route), ("token", value(prepared, "request"))];
        assert_eq!(get(&app, "inspect", &fields).await["verified"], false);
    }
}

#[tokio::test]
async fn workbench_action_requires_separate_context_and_publication_approvals() {
    workbench_action_review(false).await;
}

#[tokio::test]
async fn workbench_blocks_unavailable_and_unsupported_advisory_checks() {
    workbench_action_review(true).await;
}

async fn workbench_action_review(incomplete: bool) {
    let (_directory, world) = fixture::fixture();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["state_keys"] = serde_yaml::from_str("character:\n  score: {type: integer}\n").unwrap();
    std::fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let app = http::live_router(Service::new(world.clone()));
    let choices = get(&app, "writing-choices", &[]).await;
    let frame = get(
        &app,
        "writing-frame",
        &[
            ("moment", token(&choices["moments"], "Scene position")),
            ("scene", token(&choices["scenes"], "Fixture scene")),
            (
                "viewpoint",
                token(&choices["viewpoints"], "Fixture character"),
            ),
            ("lens", "author"),
        ],
    )
    .await;
    let frame = value(&frame, "frame");
    let choices = get(&app, "action-list", &[]).await;
    let prepared = get(
        &app,
        "action-save",
        &[
            ("title", "Quay repair"),
            ("intent", "Refresh the landing prose"),
            ("actor", token(&choices["actors"], "Fixture character")),
            ("target", token(&choices["targets"], "Fixture location")),
            ("frame", frame),
        ],
    )
    .await;
    let save = async |prepared: Value| {
        let native = get(
            &app,
            "native-request",
            &[
                ("route", ACTION_SAVE),
                ("token", value(&prepared, "request")),
            ],
        )
        .await;
        assert_eq!(
            call(&app, "POST", ACTION_SAVE, value(&native, "bytes"))
                .await
                .0,
            StatusCode::OK
        );
    };
    save(prepared).await;
    let choices = get(&app, "action-list", &[]).await;
    let draft = token(&choices["drafts"], "Quay repair");
    let readiness = get(
        &app,
        "action-prepare-context",
        &[("draft", draft), ("frame", frame), ("stage", "readiness")],
    )
    .await;
    let ticket = value(&readiness, "candidate");
    assert_eq!(
        read(
            &app,
            "action-confirm-context",
            &[("candidate", ticket), ("sufficient", "0")]
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    save(
        get(
            &app,
            "action-confirm-context",
            &[("candidate", ticket), ("sufficient", "1")],
        )
        .await,
    )
    .await;
    let mut expectations = vec![
        json!({"id":"empty-score","predicate":{"kind":"state.absent","targetId":fixture::ACTOR,"key":"score"}}),
    ];
    if incomplete {
        expectations.extend([
                json!({"id":"missing-record","predicate":{"kind":"state.absent","targetId":"char_00000000000000000000000009","key":"score"}}),
                json!({"id":"undeclared-key","predicate":{"kind":"state.absent","targetId":fixture::ACTOR,"key":"unknown_field"}}),
        ]);
    }
    let choices = get(&app, "action-list", &[]).await;
    let draft = token(&choices["drafts"], "Quay repair");
    let plan = json!({
        "expectedFingerprint": world.fingerprint,
        "operations": [{"type":"record.update","entityId":fixture::PLACE,
            "patch":"kind: location\nbody: The quay is repaired.\n"}],
        "analysis":{"at":{"timeline":"main","tick":fixture::TICK.to_string(),"order":"1"},
            "policy":"advisory","expectations":expectations},
    })
    .to_string();
    let preview = get(
        &app,
        "writing-exchange",
        &[("kind", "plan"), ("raw", &plan)],
    )
    .await;
    let proposal = get(
        &app,
        "action-prepare-context",
        &[
            ("draft", draft),
            ("frame", frame),
            ("stage", "proposal"),
            ("plan", value(&preview, "file")),
            ("summary", "Repair the quay"),
        ],
    )
    .await;
    save(
        get(
            &app,
            "action-confirm-context",
            &[
                ("candidate", value(&proposal, "candidate")),
                ("sufficient", "1"),
            ],
        )
        .await,
    )
    .await;
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    let choices = get(&app, "action-list", &[]).await;
    let draft = token(&choices["drafts"], "Quay repair");
    let opened = get(&app, "action-open", &[("draft", draft)]).await;
    let outcome = get(
        &app,
        "action-review",
        &[
            ("draft", draft),
            ("proposal", token(&opened["proposals"], "Repair the quay")),
        ],
    )
    .await;
    if incomplete {
        assert_eq!(outcome["canApply"], false, "{outcome}");
        assert_eq!(outcome["complete"], false);
        assert!(outcome["approval"].is_null());
        assert_eq!(outcome["validation"][2]["status"], "Unavailable");
        assert_eq!(outcome["validation"][3]["status"], "Unsupported");
        let native = get(&app, "file", &[("token", value(&outcome, "file"))]).await;
        let native: Value = serde_json::from_str(value(&native, "bytes")).unwrap();
        assert_eq!(
            native["canApply"], true,
            "native advisory semantics changed"
        );
        return;
    }
    assert_eq!(outcome["canApply"], true, "{outcome}");
    assert_eq!(outcome["validation"][1]["label"], "State is absent");
    assert_eq!(outcome["validation"][1]["policy"], "Advisory");
    assert_eq!(outcome["validation"][1]["status"], "Passed");
    assert_eq!(
        outcome["validation"][1]["operands"][0]["value"]["value"],
        "Fixture character"
    );
    assert_eq!(
        outcome["validation"][1]["operands"][1]["value"]["value"],
        "score"
    );
    assert!(!outcome.to_string().contains(fixture::ACTOR));
    let prepared = get(
        &app,
        "approve",
        &[
            ("candidate", value(&outcome, "approval")),
            ("approved", "1"),
        ],
    )
    .await;
    assert_eq!(prepared["route"], ACTION_APPLY);
    let request = value(&prepared, "request");
    let native = get(
        &app,
        "native-request",
        &[("route", ACTION_APPLY), ("token", request)],
    )
    .await;
    let (status, receipt) = call(&app, "POST", ACTION_APPLY, value(&native, "bytes")).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["confirmed"], true);
    assert_eq!(
        source::load_world(&world.root, None).unwrap().records[fixture::PLACE].body,
        "The quay is repaired."
    );
    let verified = get(
        &app,
        "inspect",
        &[("route", ACTION_APPLY), ("token", request)],
    )
    .await;
    assert_eq!(verified["verified"], true);
    assert_eq!(
        verified["semanticVerification"][0],
        outcome["validation"][1]
    );
    let applied = wedl::action_outcomes::decode_apply(value(&native, "bytes")).unwrap();
    assert_inspection_rejects_reformatted_terminal(
        &app,
        &world,
        request,
        ACTION_APPLY,
        "actions",
        &applied.action_id,
    )
    .await;
}
