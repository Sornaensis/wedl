use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    process::Command,
};

const WORK: &str = "work_00000000000000000000000001";
const PLAN: &str = "reveal_00000000000000000000000001";
fn request() -> Json {
    json!({"protocol":PROTOCOL,"publicationId":"publication","revision":"a".repeat(40),
        "occurrence":0,"limit":1,"cursor":null})
}
fn preview_request() -> Json {
    let mut value = request();
    value.as_object_mut().unwrap().remove("publicationId");
    value["workId"] = WORK.into();
    value["planId"] = PLAN.into();
    value["operation"] = "page".into();
    value["at"] = json!({"timeline":"main","tick":"20","order":"0"});
    value["textPolicy"] = "all-at-pinned-plan".into();
    value
}
fn time() -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick: 20,
        order: 0,
    }
}
fn scope() -> CanonicalScope {
    CanonicalScope {
        world_id: "world".into(),
        records: BTreeSet::from(["location".into()]),
        sections: None,
    }
}
fn policy(repository: RepositoryIdentity) -> PublicPolicy {
    PublicPolicy {
        repository,
        revision: "a".repeat(40),
        work_id: WORK.into(),
        plan_id: PLAN.into(),
        canonical_scope: scope(),
        generation: "generation".into(),
        max_occurrence: 2,
        cursor_secret: [17; 32],
        publication_id: "publication".into(),
        at: time(),
        text_policy: TextPolicy::AllAtPinnedPlan,
    }
}
fn simulation(policy: &PublicPolicy) -> SimulationPolicy {
    SimulationPolicy {
        repository: policy.repository.clone(),
        revision: policy.revision.clone(),
        work_id: policy.work_id.clone(),
        plan_id: policy.plan_id.clone(),
        canonical_scope: policy.canonical_scope.clone(),
        generation: policy.generation.clone(),
        max_occurrence: policy.max_occurrence,
        cursor_secret: policy.cursor_secret,
        principal_id: "operator".into(),
        author_permissions: scope(),
    }
}
fn empty_repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(root.path())
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );
    root
}
fn installed() -> (tempfile::TempDir, PresentationService, PublicPolicy) {
    let root = empty_repository();
    let policy = policy(repository_identity(root.path()).unwrap());
    let service = PresentationService::new(Some(root.path().into()));
    service.install_public(Some(policy.clone())).unwrap();
    (root, service, policy)
}
fn item(id: usize, markdown: &str) -> ReaderItem {
    ReaderItem {
        kind: "text".into(),
        id: format!("revealtext_{id:026}"),
        owner_id: None,
        title: None,
        markdown: markdown.into(),
    }
}
fn selection(input: &Json, operation: Operation) -> Selection {
    decode(input.to_string().as_bytes(), operation, Mode::Reader).unwrap()
}
fn page(
    items: Vec<ReaderItem>,
    input: &Json,
    context: Json,
    secret: &[u8; 32],
    mode: Mode,
) -> std::result::Result<Json, ProjectionError> {
    let operation = if input.get("query").is_some() {
        Operation::Search
    } else {
        Operation::Page
    };
    paginated(
        items,
        3,
        2,
        &time(),
        secret,
        context,
        &selection(input, operation),
        mode,
    )
}
fn assert_unavailable(value: Json, mode: Mode) {
    assert_eq!(value, failure(mode, Failure::Unavailable));
}
fn assert_limit(result: std::result::Result<Json, ProjectionError>) {
    assert!(matches!(result, Err(ProjectionError::Limit)));
}

#[test]
fn hmac_sha256_matches_rfc4231_including_a_key_larger_than_one_block() {
    for (key, input, expected) in [
        (
            vec![0x0b; 20],
            b"Hi There".as_slice(),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        ),
        (
            b"Jefe".to_vec(),
            b"what do ya want for nothing?".as_slice(),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
        ),
        (
            vec![0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First".as_slice(),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
        ),
    ] {
        assert_eq!(hex::encode(hmac_sha256(&key, input)), expected);
    }
}

#[test]
fn full_default_unicode_casefold_handles_expansions_final_sigma_and_supplementary_letters() {
    for (source, folded) in [
        ("Straße ẞ", "strasse ss"),
        ("Σςσ", "σσσ"),
        ("ﬃ ﬄ ﬀ", "ffi ffl ff"),
        ("İIı", "i\u{307}iı"),
        ("Kſ", "ks"),
        ("\u{10400}", "\u{10428}"),
        ("Æ🌲", "æ🌲"),
    ] {
        assert_eq!(casefold::fold(source), folded);
    }
    // No normalization or locale-specific Turkic fold is silently added.
    assert_ne!(casefold::fold("É"), casefold::fold("e\u{301}"));
    assert_eq!(casefold::fold("\u{10ffff}"), "\u{10ffff}");
}

#[test]
fn closed_public_requests_reject_unknown_or_duplicate_fields_and_coercions() {
    let good = request();
    assert!(decode(good.to_string().as_bytes(), Operation::Page, Mode::Reader).is_ok());
    for (field, value) in [
        ("protocol", json!("wedl-reader-presentation/v2")),
        ("publicationId", json!("_bad")),
        ("revision", json!("HEAD")),
        ("revision", json!("A".repeat(40))),
        ("occurrence", json!(true)),
        ("occurrence", json!(-1)),
        ("occurrence", json!(0.0)),
        ("occurrence", json!(1024)),
        ("limit", json!(0)),
        ("limit", json!(101)),
        ("limit", json!("1")),
        ("cursor", json!([])),
        ("query", Json::Null),
        ("query", json!("not allowed on page")),
        ("at", json!({})),
        ("grant", json!("all")),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        assert!(
            decode(bad.to_string().as_bytes(), Operation::Page, Mode::Reader).is_err(),
            "accepted {field}"
        );
    }
    for field in [
        "protocol",
        "publicationId",
        "revision",
        "occurrence",
        "limit",
        "cursor",
    ] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            decode(
                missing.to_string().as_bytes(),
                Operation::Page,
                Mode::Reader
            )
            .is_err(),
            "missing {field}"
        );
    }
    let duplicate = good
        .to_string()
        .replacen("\"limit\":1", "\"limit\":1,\"li\\u006dit\":2", 1);
    assert!(decode(duplicate.as_bytes(), Operation::Page, Mode::Reader).is_err());
    assert!(
        decode(
            format!("{} {{}}", good).as_bytes(),
            Operation::Page,
            Mode::Reader
        )
        .is_err()
    );
    assert!(
        decode(
            &[b' '; MAX_REQUEST_BYTES + 1],
            Operation::Page,
            Mode::Reader
        )
        .is_err()
    );
}

#[test]
fn search_query_bounds_are_exact_and_page_query_omission_is_significant() {
    let mut input = request();
    assert!(
        decode(
            input.to_string().as_bytes(),
            Operation::Search,
            Mode::Reader
        )
        .is_err()
    );
    for query in ["".into(), "\t\n".into(), "a".repeat(257), "🌲".repeat(257)] {
        input["query"] = query.into();
        assert!(
            decode(
                input.to_string().as_bytes(),
                Operation::Search,
                Mode::Reader
            )
            .is_err()
        );
    }
    input["query"] = "🌲".repeat(256).into();
    assert!(
        decode(
            input.to_string().as_bytes(),
            Operation::Search,
            Mode::Reader
        )
        .is_ok()
    );
    input["query"] = Json::Null;
    assert!(
        decode(
            input.to_string().as_bytes(),
            Operation::Search,
            Mode::Reader
        )
        .is_err()
    );
}

#[test]
fn preview_time_is_closed_exact_decimal_and_never_a_public_request_grant() {
    let good = preview_request();
    assert!(decode(good.to_string().as_bytes(), Operation::Page, Mode::Preview).is_ok());
    for bad in [
        json!({"timeline":"main","tick":20,"order":"0"}),
        json!({"timeline":"main","tick":"+20","order":"0"}),
        json!({"timeline":"main","tick":"020","order":"0"}),
        json!({"timeline":"main","tick":"-0","order":"0"}),
        json!({"timeline":"main","tick":"9223372036854775808","order":"0"}),
        json!({"timeline":"main","tick":"20","order":"2147483648"}),
        json!({"timeline":" ","tick":"20","order":"0"}),
        json!({"timeline":"main","tick":"20","order":"0","scope":"all"}),
    ] {
        let mut input = good.clone();
        input["at"] = bad;
        assert!(decode(input.to_string().as_bytes(), Operation::Page, Mode::Preview).is_err());
    }
    let mut input = good.clone();
    input["at"] =
        json!({"timeline":"main","tick":i64::MIN.to_string(),"order":i32::MAX.to_string()});
    assert!(decode(input.to_string().as_bytes(), Operation::Page, Mode::Preview).is_ok());
    assert!(decode(good.to_string().as_bytes(), Operation::Page, Mode::Reader).is_err());
    let duplicate =
        good.to_string()
            .replacen("\"tick\":\"20\"", "\"tick\":\"20\",\"tick\":\"30\"", 1);
    assert!(decode(duplicate.as_bytes(), Operation::Page, Mode::Preview).is_err());
}

#[test]
fn opaque_cursor_syntax_has_one_exact_bounded_shape() {
    for input in [
        "".into(),
        "0".repeat(71),
        "0".repeat(73),
        "A".repeat(72),
        format!("00000401{}", "0".repeat(64)),
        format!("00000000{}=", "0".repeat(63)),
    ] {
        assert!(decode_cursor(Some(input)).is_err());
    }
    assert_eq!(
        decode_cursor(Some(format!("00000400{}", "0".repeat(64))))
            .unwrap()
            .unwrap()
            .offset,
        1024
    );
    assert!(decode_cursor(None).unwrap().is_none());
}

#[test]
fn no_installed_authority_refuses_before_projection_or_repository_existence() {
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("absent");
    for service in [
        PresentationService::default(),
        PresentationService::new(Some(absent.clone())),
    ] {
        for (input, mode) in [
            (request(), Mode::Reader),
            (preview_request(), Mode::Preview),
        ] {
            let prepared = service
                .prepare_using(
                    input.to_string().as_bytes(),
                    Operation::Page,
                    mode,
                    |_, _| panic!("source lookup without authority"),
                )
                .unwrap();
            assert_unavailable(service.emit(prepared), mode);
        }
    }
    assert!(!absent.exists());
}

#[test]
fn installed_policy_identity_revision_and_ceiling_checks_precede_projection() {
    let (_root, service, policy) = installed();
    for (field, value) in [
        ("publicationId", json!("other")),
        ("revision", json!("b".repeat(40))),
        ("occurrence", json!(3)),
    ] {
        let mut input = request();
        input[field] = value;
        let prepared = service
            .prepare_using(
                input.to_string().as_bytes(),
                Operation::Page,
                Mode::Reader,
                |_, _| panic!("source lookup for {field} mismatch"),
            )
            .unwrap();
        assert_unavailable(service.emit(prepared), Mode::Reader);
    }
    service
        .install_simulation(Some(simulation(&policy)))
        .unwrap();
    for field in ["workId", "planId"] {
        let mut input = preview_request();
        input[field] = input[field]
            .as_str()
            .unwrap()
            .replacen("00001", "00002", 1)
            .into();
        let prepared = service
            .prepare_using(
                input.to_string().as_bytes(),
                Operation::Page,
                Mode::Preview,
                |_, _| panic!("source lookup for {field} mismatch"),
            )
            .unwrap();
        assert_unavailable(service.emit(prepared), Mode::Preview);
    }
}

#[test]
fn clones_share_revocation_and_reinstalling_equal_policy_invalidates_inflight_result() {
    let (_root, service, policy) = installed();
    let clone = service.clone();
    let raw = request().to_string();
    for replace in [false, true] {
        service.install_public(Some(policy.clone())).unwrap();
        let prepared = service
            .prepare_using(raw.as_bytes(), Operation::Page, Mode::Reader, |_, _| {
                clone
                    .install_public(if replace { Some(policy.clone()) } else { None })
                    .unwrap();
                Ok(json!({"private":"must not emit"}))
            })
            .unwrap();
        assert_unavailable(service.emit(prepared), Mode::Reader);
    }
}

#[test]
fn emission_rechecks_installation_after_work_finished_and_errors_do_not_override_revocation() {
    let (_root, service, policy) = installed();
    let raw = request().to_string();
    let prepared = service
        .prepare_using(raw.as_bytes(), Operation::Page, Mode::Reader, |_, _| {
            Ok(json!({"must":"disappear"}))
        })
        .unwrap();
    service.install_public(None).unwrap();
    assert_unavailable(service.emit(prepared), Mode::Reader);
    for error in [
        ProjectionError::Invalid,
        ProjectionError::Limit,
        ProjectionError::Unavailable,
    ] {
        service.install_public(Some(policy.clone())).unwrap();
        let prepared = service
            .prepare_using(raw.as_bytes(), Operation::Page, Mode::Reader, |_, _| {
                service.install_public(None).unwrap();
                Err(error)
            })
            .unwrap();
        assert_unavailable(service.emit(prepared), Mode::Reader);
    }
}

#[test]
fn simulation_revocation_is_independent_from_public_and_source_errors_are_mode_specific() {
    let (_root, service, policy) = installed();
    service
        .install_simulation(Some(simulation(&policy)))
        .unwrap();
    let prepared = service
        .prepare_using(
            request().to_string().as_bytes(),
            Operation::Page,
            Mode::Reader,
            |_, _| {
                service.install_simulation(None).unwrap();
                Ok(json!({"result":"public still installed"}))
            },
        )
        .unwrap();
    assert_eq!(service.emit(prepared)["result"], "public still installed");
    for (mode, input, expected) in [
        (Mode::Reader, request(), Failure::Unavailable),
        (Mode::Preview, preview_request(), Failure::Invalid),
    ] {
        service
            .install_simulation(Some(simulation(&policy)))
            .unwrap();
        let prepared = service
            .prepare_using(
                input.to_string().as_bytes(),
                Operation::Page,
                mode,
                |_, _| Err(ProjectionError::Invalid),
            )
            .unwrap();
        assert_eq!(service.emit(prepared), failure(mode, expected));
    }
    service
        .install_simulation(Some(simulation(&policy)))
        .unwrap();
    let prepared = service
        .prepare_using(
            preview_request().to_string().as_bytes(),
            Operation::Page,
            Mode::Preview,
            |_, _| {
                service
                    .install_simulation(Some(simulation(&policy)))
                    .unwrap();
                Err(ProjectionError::Invalid)
            },
        )
        .unwrap();
    assert_unavailable(service.emit(prepared), Mode::Preview);
}

#[test]
fn installation_copies_trusted_values_and_rejects_invalid_replacement_without_losing_current_policy()
 {
    let (_root, service, mut policy) = installed();
    policy.publication_id = "mutated-copy".into();
    policy.cursor_secret = [99; 32];
    let prepared = service
        .prepare_using(
            request().to_string().as_bytes(),
            Operation::Page,
            Mode::Reader,
            |_, captured| {
                assert_eq!(captured.binding().secret, &[17; 32]);
                Ok(json!({"same":true}))
            },
        )
        .unwrap();
    assert_eq!(service.emit(prepared)["same"], true);
    policy.max_occurrence = 1024;
    assert!(service.install_public(Some(policy)).is_err());
    let prepared = service
        .prepare_using(
            request().to_string().as_bytes(),
            Operation::Page,
            Mode::Reader,
            |_, _| Ok(json!({"retained":true})),
        )
        .unwrap();
    assert_eq!(service.emit(prepared)["retained"], true);
}

#[test]
fn repository_identity_distinguishes_clones_and_missing_or_nested_roots() {
    let first = empty_repository();
    let second = empty_repository();
    assert!(
        repository_identity(first.path()).unwrap() != repository_identity(second.path()).unwrap()
    );
    let nested = first.path().join("nested");
    fs::create_dir(&nested).unwrap();
    assert!(repository_identity(&nested).is_err());
    assert!(repository_identity(&first.path().join("missing")).is_err());
    let service = PresentationService::new(Some(second.path().into()));
    assert!(
        service
            .install_public(Some(policy(repository_identity(first.path()).unwrap())))
            .is_err()
    );
}

#[test]
fn pagination_and_navigation_only_describe_the_admitted_order_and_installed_ceiling() {
    let units = vec![item(1, "first"), item(2, "second"), item(3, "third")];
    let mut input = request();
    let first = page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader).unwrap();
    assert_eq!(first["count"], 3);
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        first["navigation"],
        json!({"previousOccurrence":null,"nextOccurrence":1})
    );
    let cursor = first["nextCursor"].as_str().unwrap();
    assert_eq!(cursor.len(), 72);
    assert!(
        cursor
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
    input["cursor"] = cursor.into();
    let next = page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader).unwrap();
    assert_eq!(next["items"][0]["markdown"], "second");
    input["cursor"] = next["nextCursor"].clone();
    let last = page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader).unwrap();
    assert_eq!(last["items"][0]["markdown"], "third");
    assert_eq!(last["nextCursor"], Json::Null);
    let selection = selection(&request(), Operation::Page);
    let limited = paginated(
        units,
        100,
        0,
        &time(),
        &[1; 32],
        json!({}),
        &selection,
        Mode::Reader,
    )
    .unwrap();
    assert_eq!(
        limited["navigation"],
        json!({"previousOccurrence":null,"nextOccurrence":null})
    );
    assert_eq!(
        first
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        [
            "protocol",
            "mode",
            "operation",
            "outcome",
            "code",
            "revision",
            "occurrence",
            "at",
            "navigation",
            "items",
            "count",
            "nextCursor"
        ]
        .map(String::from)
        .into()
    );
}

#[test]
fn cursors_bind_mode_secret_order_complete_data_and_each_private_context_field() {
    let units = vec![item(1, "first"), item(2, "second")];
    let mut input = request();
    let context = json!({"protocol":PROTOCOL,"mode":"reader","operation":"page","repository":["root","git","common"],
        "revision":"a".repeat(40),"work":WORK,"plan":PLAN,"workBlob":"work-blob","planBlob":"plan-blob",
        "principal":"anonymous-public","scope":scope(),"at":time(),"generation":"one","textPolicy":"all-at-pinned-plan",
        "ceiling":2,"occurrence":0,"limit":1,"publication":"publication"});
    input["cursor"] = page(
        units.clone(),
        &input,
        context.clone(),
        &[1; 32],
        Mode::Reader,
    )
    .unwrap()["nextCursor"]
        .clone();
    for key in context.as_object().unwrap().keys() {
        let mut changed = context.clone();
        changed[key] = "different".into();
        assert!(
            matches!(
                page(units.clone(), &input, changed, &[1; 32], Mode::Reader),
                Err(ProjectionError::Unavailable)
            ),
            "unbound {key}"
        );
    }
    assert!(matches!(
        page(
            units.clone(),
            &input,
            context.clone(),
            &[2; 32],
            Mode::Reader
        ),
        Err(ProjectionError::Unavailable)
    ));
    assert!(matches!(
        page(
            units.clone(),
            &input,
            context.clone(),
            &[1; 32],
            Mode::Preview
        ),
        Err(ProjectionError::Unavailable)
    ));
    let mut changed = units.clone();
    changed[1].markdown = "changed unreturned unit".into();
    assert!(matches!(
        page(changed, &input, context.clone(), &[1; 32], Mode::Reader),
        Err(ProjectionError::Unavailable)
    ));
    let mut reversed = units;
    reversed.reverse();
    assert!(matches!(
        page(reversed, &input, context, &[1; 32], Mode::Reader),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn search_uses_full_casefold_but_cursor_binds_original_query_exactly() {
    let units = vec![
        item(1, "Straße ﬃ Σ"),
        item(2, "STRASSE FFI ς"),
        item(3, "other"),
    ];
    let mut input = request();
    input["query"] = "STRASSE FFI σ".into();
    let first = page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader).unwrap();
    assert_eq!(first["count"], 2);
    input["cursor"] = first["nextCursor"].clone();
    assert_eq!(
        page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader).unwrap()["items"][0]["markdown"],
        "STRASSE FFI ς"
    );
    input["query"] = "strasse ffi σ".into();
    assert!(matches!(
        page(units, &input, json!({}), &[1; 32], Mode::Reader),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn malformed_mac_or_offset_tampering_is_uniform_unavailable_after_syntax() {
    let units = vec![item(1, "first"), item(2, "second")];
    let mut input = request();
    let cursor =
        page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader).unwrap()["nextCursor"]
            .as_str()
            .unwrap()
            .to_owned();
    for offset in [0, 2, 1024] {
        input["cursor"] = format!("{offset:08x}{}", &cursor[8..]).into();
        assert!(matches!(
            page(units.clone(), &input, json!({}), &[1; 32], Mode::Reader),
            Err(ProjectionError::Unavailable)
        ));
    }
    let mut bytes = cursor.into_bytes();
    bytes[71] = if bytes[71] == b'0' { b'1' } else { b'0' };
    input["cursor"] = String::from_utf8(bytes).unwrap().into();
    assert!(matches!(
        page(units, &input, json!({}), &[1; 32], Mode::Reader),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn whole_admitted_count_and_markdown_limits_precede_pagination_and_search() {
    for search in [false, true] {
        let mut input = request();
        if search {
            input["query"] = "no matching data".into();
        }
        assert_limit(page(
            (0..=MAX_ITEMS).map(|id| item(id, "")).collect(),
            &input,
            json!({}),
            &[1; 32],
            Mode::Reader,
        ));
        assert_limit(page(
            vec![item(1, &"x".repeat(MAX_MARKDOWN_BYTES)), item(2, "x")],
            &input,
            json!({}),
            &[1; 32],
            Mode::Reader,
        ));
    }
    let input = request();
    assert_eq!(
        page(
            vec![item(1, &"x".repeat(MAX_MARKDOWN_BYTES))],
            &input,
            json!({}),
            &[1; 32],
            Mode::Reader
        )
        .unwrap()["count"],
        1
    );
}

#[test]
fn complete_selected_json_limit_counts_escaping_before_pagination_but_after_search() {
    let mut input = request();
    input["query"] = "unmatched".into();
    // Still under the admitted Markdown byte ceiling, but JSON escaping is >4 MiB.
    let units = vec![item(1, "ordinary"), item(2, &"\0".repeat(800_000))];
    assert_limit(page(
        units.clone(),
        &request(),
        json!({}),
        &[1; 32],
        Mode::Reader,
    ));
    let empty = page(units, &input, json!({}), &[1; 32], Mode::Reader).unwrap();
    assert_eq!(empty["outcome"], "available");
    assert_eq!(empty["count"], 0);
}

#[test]
fn whitelist_size_is_bounded_and_invalid_installation_leaves_no_authority() {
    let root = empty_repository();
    let mut policy = policy(repository_identity(root.path()).unwrap());
    policy.canonical_scope.sections = Some(BTreeMap::from([(
        "location".into(),
        (0..4096)
            .map(|index| format!("{index:04}{}", "x".repeat(507)))
            .collect(),
    )]));
    assert!(policy.canonical_scope.validate().is_ok());
    let service = PresentationService::new(Some(root.path().into()));
    assert!(service.install_public(Some(policy)).is_err());
    assert_unavailable(
        service
            .execute(request().to_string().as_bytes(), Operation::Page)
            .unwrap(),
        Mode::Reader,
    );
}

#[test]
fn prepared_author_preview_is_refused_when_simulation_is_revoked_before_emission() {
    let (_root, service, policy) = installed();
    service
        .install_simulation(Some(simulation(&policy)))
        .unwrap();
    // Actual preparation against the missing immutable source captures an
    // authorized source diagnostic; revocation must suppress that diagnostic.
    let prepared = service
        .prepare_preview(preview_request().to_string().as_bytes())
        .unwrap();
    assert_eq!(prepared.value["outcome"], "invalid");
    service.install_simulation(None).unwrap();
    assert_unavailable(service.emit(prepared), Mode::Preview);
}
