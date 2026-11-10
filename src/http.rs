//! Loopback author HTTP: immutable read-only snapshots and explicit live
//! worktree publication through the existing reviewed native writers.
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use crate::{
    Error, Result,
    chronology::{AnnotationQuery, CivilDate, DateInput, DateTarget, EraMatchMode},
    compendium::{self, Browser as CompendiumBrowser},
    service::{CompendiumSource, Operation, Service, TimeInput},
    validation, workbench,
};

pub const INDEX: &str = include_str!("../static/index.html");
const COMPENDIUM: &str = include_str!("../static/compendium.html");

fn workbench_assets<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/workbench",
            get(|| async { Html(include_str!("../static/workbench.html")) }),
        )
        .route(
            "/workbench.js",
            get(|| async {
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/javascript; charset=utf-8",
                    )],
                    include_str!("../static/workbench.js"),
                )
            }),
        )
        .route(
            "/workbench-core.js",
            get(|| async {
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/javascript; charset=utf-8",
                    )],
                    include_str!("../static/workbench-core.js"),
                )
            }),
        )
        .route(
            "/workbench-archive.js",
            get(|| async {
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/javascript; charset=utf-8",
                    )],
                    include_str!("../static/workbench-archive.js"),
                )
            }),
        )
}

pub fn router(service: Service) -> Router {
    let service = service.with_compendium_authority();
    Router::new()
        .merge(workbench_assets())
        .route("/", get(|| async { Html(INDEX) }))
        .route("/compendium", get(|| async { Html(COMPENDIUM) }))
        .route("/api/compendium/{endpoint}", get(fixed_compendium))
        .route("/api/workbench/{endpoint}", get(fixed_workbench))
        .route("/api/{*path}", get(api))
        .fallback(|| async { ApiError(Error::NotFound("route does not exist".into())) })
        .method_not_allowed_fallback(|| async { (StatusCode::METHOD_NOT_ALLOWED, Json(json!({"code":"method_not_allowed","message":"This snapshot server accepts GET requests only","details":{}}))) })
        .layer(Extension(Arc::new(CompendiumBrowser::new())))
        .layer(Extension(Arc::new(workbench::Browser::default())))
        .layer(middleware::from_fn(local_request))
        .with_state(service)
}

/// Current-worktree author router. Historical callers retain the immutable
/// read-only contract even if they explicitly select this constructor.
pub fn live_router(service: Service) -> Router {
    if service.world().revision.is_some() {
        return router(service);
    }
    worktree_router(service, true)
}

fn worktree_router(service: Service, live_author: bool) -> Router {
    let service = service.with_compendium_authority();
    let mut app = Router::new()
        .merge(workbench_assets())
        .route("/", get(|| async { Html(INDEX) }))
        .route("/compendium", get(|| async { Html(COMPENDIUM) }))
        .route("/api/compendium/{endpoint}", get(current_compendium))
        .route("/api/workbench/{endpoint}", get(current_workbench))
        .route("/api/session", get(worktree_capabilities))
        .route("/api/{*path}", get(live_api));
    if live_author {
        app = app
            .route("/api/action-drafts/save", post(live_write))
            .route("/api/action-drafts/apply-outcome", post(live_write))
            .route("/api/edit-drafts/save", post(live_write))
            .route("/api/edit-drafts/apply", post(live_write))
            .route("/api/manuscripts/save", post(live_write))
            .route("/api/retcon-reviews/save", post(live_write));
    }
    app.fallback(|| async { ApiError(Error::NotFound("route does not exist".into())) })
        .method_not_allowed_fallback(|| async {
            refusal(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "This route does not accept the requested method",
            )
        })
        .layer(Extension(Arc::new(CompendiumBrowser::new())))
        .layer(Extension(Arc::new(workbench::Browser::default())))
        .layer(middleware::from_fn(local_request))
        .with_state(LiveSession {
            live_author,
            reader: service.reader().clone(),
            gate: Arc::new(Mutex::new(LiveState {
                service,
                bounded: false,
            })),
        })
}

#[derive(Clone)]
struct LiveSession {
    live_author: bool,
    reader: crate::reader::PresentationService,
    gate: Arc<Mutex<LiveState>>,
}

struct LiveState {
    service: Service,
    bounded: bool,
}

impl LiveSession {
    fn run<T>(&self, current: bool, work: impl FnOnce(Service) -> Result<T>) -> Result<T> {
        #[cfg(feature = "snapshot-baseline")]
        let measurement = crate::snapshot_baseline::Measurement::start();
        let mut state = self
            .gate
            .lock()
            .map_err(|_| Error::Data("live author request worker failed".into()))?;
        #[cfg(feature = "snapshot-baseline")]
        measurement.acquired_gate();
        // A load failure or pending publication never falls back to an older
        // snapshot. Source capture itself uses the native publication fence.
        if current {
            state.service = state.service.capture_worktree(state.bounded)?;
            state.bounded = true;
        }
        // Static and independently captured original/private reads dispatch
        // without refreshing the worktree or using its outer source pins.
        work(state.service.clone())
    }
}

fn compendium_request(
    endpoint: &str,
    query: Option<&str>,
) -> std::result::Result<compendium::Request, compendium::Failure> {
    let query = parse_query_bounded(query.unwrap_or(""), compendium::MAX_QUERY_BYTES)
        .map_err(|_| compendium::Failure::Invalid)?;
    compendium::Request::decode(endpoint, query)
}

fn compendium_response(
    result: std::result::Result<serde_json::Value, compendium::Failure>,
) -> Response {
    match result {
        Ok(result) => Json(result).into_response(),
        Err(failure) => {
            let (status, code, message) = failure.presentation();
            refusal(
                StatusCode::from_u16(status).expect("known Compendium HTTP status"),
                code,
                message,
            )
        }
    }
}

async fn fixed_compendium(
    State(service): State<Service>,
    Extension(browser): Extension<Arc<CompendiumBrowser>>,
    Path(endpoint): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let request = match compendium_request(&endpoint, query.as_deref()) {
        Ok(request) => request,
        Err(failure) => return compendium_response(Err(failure)),
    };
    compendium_response(
        tokio::task::spawn_blocking(move || {
            browser.execute(&service, CompendiumSource::Fixed, request)
        })
        .await
        .unwrap_or(Err(compendium::Failure::Source)),
    )
}

async fn current_compendium(
    State(session): State<LiveSession>,
    Extension(browser): Extension<Arc<CompendiumBrowser>>,
    Path(endpoint): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let request = match compendium_request(&endpoint, query.as_deref()) {
        Ok(request) => request,
        Err(failure) => return compendium_response(Err(failure)),
    };
    compendium_response(
        tokio::task::spawn_blocking(move || {
            // The pin performs native current-source capture while this request
            // holds the same gate as a live-author write. Do not capture twice.
            session
                .run(false, |service| {
                    Ok(browser.execute(&service, CompendiumSource::Current, request))
                })
                .unwrap_or(Err(compendium::Failure::Source))
        })
        .await
        .unwrap_or(Err(compendium::Failure::Source)),
    )
}

fn workbench_response(result: Result<serde_json::Value>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(Error::Conflict(_)) => refusal(
            StatusCode::CONFLICT,
            "stale",
            "The source changed. Reload the selection.",
        ),
        Err(Error::InvalidInput(_) | Error::Ambiguous(_)) => refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "Choose a supported Workbench selection.",
        ),
        Err(Error::Unsupported(_)) => refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported",
            "This selection needs the native file workflow.",
        ),
        Err(_) => refusal(
            StatusCode::NOT_FOUND,
            "unavailable",
            "This selection is unavailable.",
        ),
    }
}

async fn fixed_workbench(
    State(service): State<Service>,
    Extension(browser): Extension<Arc<workbench::Browser>>,
    Path(endpoint): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    if endpoint == "archive-simulation" {
        let reader = service.reader().clone();
        let result = tokio::task::spawn_blocking(move || {
            let query =
                parse_query_bounded(query.as_deref().unwrap_or(""), workbench::MAX_QUERY_BYTES)?;
            browser.prepare_simulation(&service, &query)
        })
        .await
        .unwrap_or_else(|_| Err(workbench::unavailable()));
        return workbench_response(
            result.and_then(|prepared| workbench::present_simulation(reader.emit(prepared))),
        );
    }
    workbench_response(
        tokio::task::spawn_blocking(move || {
            let query =
                parse_query_bounded(query.as_deref().unwrap_or(""), workbench::MAX_QUERY_BYTES)?;
            browser.execute(&service, &endpoint, &query, false)
        })
        .await
        .unwrap_or_else(|_| Err(workbench::unavailable())),
    )
}

async fn current_workbench(
    State(session): State<LiveSession>,
    Extension(browser): Extension<Arc<workbench::Browser>>,
    Path(endpoint): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    if endpoint == "archive-simulation" {
        let reader = session.reader.clone();
        let result = tokio::task::spawn_blocking(move || {
            let query =
                parse_query_bounded(query.as_deref().unwrap_or(""), workbench::MAX_QUERY_BYTES)?;
            session.run(false, |service| {
                browser.prepare_simulation(&service, &query)
            })
        })
        .await
        .unwrap_or_else(|_| Err(workbench::unavailable()));
        return workbench_response(
            result.and_then(|prepared| workbench::present_simulation(reader.emit(prepared))),
        );
    }
    workbench_response(
        tokio::task::spawn_blocking(move || {
            let query =
                parse_query_bounded(query.as_deref().unwrap_or(""), workbench::MAX_QUERY_BYTES)?;
            session.run(workbench::needs_current(&endpoint, &query), |service| {
                browser.execute(&service, &endpoint, &query, session.live_author)
            })
        })
        .await
        .unwrap_or_else(|_| Err(workbench::unavailable())),
    )
}

async fn worktree_capabilities(
    State(session): State<LiveSession>,
    RawQuery(raw): RawQuery,
) -> std::result::Result<Response, ApiError> {
    allowed(&parse_query(raw.as_deref().unwrap_or(""))?, &[])?;
    let writes_enabled = session.live_author;
    let mode = if writes_enabled {
        "live-author"
    } else {
        "read-only-author"
    };
    let result = tokio::task::spawn_blocking(move || {
        session.run(true, |service| {
            Ok(json!({"protocol":crate::service::PROTOCOL,
            "fingerprint":service.world().fingerprint,"revision":null,
            "result":{"mode":mode,"writesEnabled":writes_enabled}}))
        })
    })
    .await
    .map_err(|_| ApiError(Error::Data("request worker failed".into())))??;
    Ok(Json(result).into_response())
}

async fn live_api(
    State(session): State<LiveSession>,
    path: std::result::Result<Path<String>, axum::extract::rejection::PathRejection>,
    RawQuery(raw): RawQuery,
) -> std::result::Result<Response, ApiError> {
    let Path(path) =
        path.map_err(|_| ApiError(Error::InvalidInput("route path is not valid UTF-8".into())))?;
    let operation = operation(&path, raw.as_deref())?;
    if let Operation::ReaderPreview { request } = operation {
        let reader = session.reader.clone();
        let worker = reader.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            session.run(false, |_| worker.prepare_preview(request.as_bytes()))
        })
        .await
        .map_err(|_| ApiError(Error::Data("request worker failed".into())))??;
        // Preserve the reader's policy fence at the HTTP response boundary.
        return Ok(reader_response(reader.emit(prepared)));
    }
    let current = !operation.independent_source();
    let result = tokio::task::spawn_blocking(move || {
        session.run(current, |service| service.execute(operation))
    })
    .await
    .map_err(|_| ApiError(Error::Data("request worker failed".into())))??;
    Ok(Json(result).into_response())
}

#[derive(Clone, Copy)]
enum WriteRoute {
    ActionSave,
    ActionApply,
    EditSave,
    EditApply,
    ManuscriptSave,
    RetconSave,
}

impl WriteRoute {
    fn at(path: &str) -> Option<Self> {
        match path {
            "/api/action-drafts/save" => Some(Self::ActionSave),
            "/api/action-drafts/apply-outcome" => Some(Self::ActionApply),
            "/api/edit-drafts/save" => Some(Self::EditSave),
            "/api/edit-drafts/apply" => Some(Self::EditApply),
            "/api/manuscripts/save" => Some(Self::ManuscriptSave),
            "/api/retcon-reviews/save" => Some(Self::RetconSave),
            _ => None,
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::ActionSave | Self::EditSave => 512 * 1024,
            Self::ActionApply | Self::EditApply => 16 * 1024,
            Self::ManuscriptSave => 8 * 1024 * 1024,
            Self::RetconSave => crate::retcon_reviews::MAX_SAVE_BYTES,
        }
    }

    fn decode(self, raw: &str) -> Result<Operation> {
        Ok(match self {
            Self::ActionSave => Operation::ActionDraftSave {
                request: crate::action_drafts::decode_save(raw)?,
            },
            Self::ActionApply => Operation::ActionOutcomeApply {
                request: crate::action_outcomes::decode_apply(raw)?,
            },
            Self::EditSave => Operation::EditDraftSave {
                request: crate::edit_drafts::decode_save(raw)?,
            },
            Self::EditApply => Operation::EditDraftApply {
                request: crate::edit_drafts::decode_apply(raw)?,
            },
            Self::ManuscriptSave => Operation::ManuscriptSave {
                request: crate::manuscript::decode_save(raw)?,
            },
            Self::RetconSave => Operation::RetconReviewSave {
                request: crate::retcon_reviews::decode_save(raw)?,
            },
        })
    }
}

fn refusal(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"code":code,"message":message,"details":{}})),
    )
        .into_response()
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

fn canonical_loopback_host(value: &str) -> bool {
    let Ok(authority) = value.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    let host = match authority.host() {
        "localhost" => "localhost",
        "127.0.0.1" => "127.0.0.1",
        "[::1]" | "::1" => "[::1]",
        _ => return false,
    };
    let expected = if authority.port().is_some() {
        let Some(port) = authority.port_u16() else {
            return false;
        };
        format!("{host}:{port}")
    } else {
        host.to_owned()
    };
    value == expected
}

fn write_headers(headers: &HeaderMap) -> std::result::Result<(), Response> {
    let host = single_header(headers, "host")
        .filter(|value| canonical_loopback_host(value))
        .ok_or_else(|| {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden_host",
                "Writes require one canonical loopback Host",
            )
        })?;
    if single_header(headers, "origin") != Some(format!("http://{host}").as_str()) {
        return Err(refusal(
            StatusCode::FORBIDDEN,
            "forbidden_origin",
            "Writes require one exact same-origin Origin",
        ));
    }
    if single_header(headers, "x-wedl-write") != Some("1") {
        return Err(refusal(
            StatusCode::FORBIDDEN,
            "write_header_required",
            "Writes require X-WEDL-Write: 1",
        ));
    }
    let json = single_header(headers, "content-type").is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    });
    if !json {
        return Err(refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_required",
            "Writes require a single application/json Content-Type",
        ));
    }
    Ok(())
}

async fn live_write(
    State(session): State<LiveSession>,
    request: axum::extract::Request,
) -> std::result::Result<Response, ApiError> {
    let receipt = single_header(request.headers(), "accept") == Some(workbench::RECEIPT_MEDIA);
    let before_dispatch = |response: Response| {
        if receipt {
            write_receipt_error(response.status(), false)
        } else {
            response
        }
    };
    if let Err(response) = write_headers(request.headers()) {
        return Ok(before_dispatch(response));
    }
    if request.uri().query().is_some() {
        return Ok(before_dispatch(
            ApiError(Error::InvalidInput(
                "write routes do not accept query parameters".into(),
            ))
            .into_response(),
        ));
    }
    let Some(route) = WriteRoute::at(request.uri().path()) else {
        return Ok(before_dispatch(
            ApiError(Error::NotFound("write route does not exist".into())).into_response(),
        ));
    };
    let bytes = match axum::body::to_bytes(request.into_body(), route.limit()).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return Ok(before_dispatch(refusal(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
                "Write body exceeds the route byte bound or could not be read",
            )));
        }
    };
    let Ok(raw) = String::from_utf8(bytes.to_vec()) else {
        return Ok(before_dispatch(
            ApiError(Error::InvalidInput("write body must be UTF-8 JSON".into())).into_response(),
        ));
    };
    let (dispatched, result) = tokio::task::spawn_blocking(move || {
        let operation = match route.decode(&raw) {
            Ok(operation) => operation,
            Err(error) => return (false, Err(error)),
        };
        // Keep exact native review/hash/approval admission, including terminal
        // retries whose original expected fingerprint predates publication.
        // The capture fence can itself encounter a pending native publication.
        (
            true,
            session.run(true, |service| service.execute(operation)),
        )
    })
    .await
    .unwrap_or_else(|_| (true, Err(Error::Data("request worker failed".into()))));
    match result {
        Ok(result) => Ok(Json(if receipt {
            workbench::receipt(&result)
        } else {
            result
        })
        .into_response()),
        Err(error) if receipt => Ok(write_receipt_error(
            ApiError(error).into_response().status(),
            dispatched,
        )),
        Err(error) => Err(ApiError(error)),
    }
}

fn write_receipt_error(status: StatusCode, may_have_effects: bool) -> Response {
    // Native writer errors can follow durable effects. Only failures proven
    // to precede native dispatch can safely release the submission lock.
    let (outcome, message) = if may_have_effects {
        ("unconfirmed", "The operation could not be confirmed.")
    } else {
        (
            "refused",
            "The operation was refused before native dispatch.",
        )
    };
    (status, Json(json!({"outcome":outcome,"message":message}))).into_response()
}

fn reader_response(result: serde_json::Value) -> Response {
    let status = match result["outcome"].as_str() {
        Some("available") => StatusCode::OK,
        Some("unavailable") => StatusCode::CONFLICT,
        Some("invalid") => StatusCode::BAD_REQUEST,
        Some("limit") => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(json!({"protocol":crate::service::PROTOCOL,"result":result})),
    )
        .into_response()
}

async fn local_request(request: axum::extract::Request, next: Next) -> Response {
    let workbench_page = request.uri().path() == "/workbench";
    if let Some(host) = request.headers().get("host") {
        let valid = host
            .to_str()
            .ok()
            .and_then(|host| host.parse::<axum::http::uri::Authority>().ok())
            .is_some_and(|host| matches!(host.host(), "127.0.0.1" | "localhost" | "[::1]" | "::1"));
        if !valid {
            return (StatusCode::FORBIDDEN, Json(json!({"code":"forbidden_host","message":"Only loopback host names are accepted","details":{}}))).into_response();
        }
    }
    if let Some(origin) = request.headers().get("origin") {
        let same_origin = origin
            .to_str()
            .ok()
            .and_then(|value| value.parse::<axum::http::Uri>().ok())
            .is_some_and(|origin| {
                origin.scheme_str() == Some("http")
                    && origin.authority().map(|v| v.as_str())
                        == request.headers().get("host").and_then(|v| v.to_str().ok())
            });
        if !same_origin {
            return (StatusCode::FORBIDDEN, Json(json!({"code":"forbidden_origin","message":"Cross-origin author requests are not accepted","details":{}}))).into_response();
        }
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert("vary", HeaderValue::from_static("Origin"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert("x-frame-options", HeaderValue::from_static("DENY"));
    let policy = if workbench_page {
        "default-src 'self'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
    } else {
        "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
    };
    response
        .headers_mut()
        .insert("content-security-policy", HeaderValue::from_static(policy));
    response
}

pub fn loopback_address(host: &str, port: u16) -> Result<SocketAddr> {
    let ip = if host == "localhost" {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        host.parse::<IpAddr>()
            .map_err(|_| Error::InvalidInput("host must be 127.0.0.1, localhost, or ::1".into()))?
    };
    if !matches!(ip, IpAddr::V4(v) if v==Ipv4Addr::LOCALHOST)
        && !matches!(ip, IpAddr::V6(v) if v.is_loopback())
    {
        return Err(Error::InvalidInput(
            "the author server binds to loopback only".into(),
        ));
    }
    Ok(SocketAddr::new(ip, port))
}

pub async fn serve(service: Service, host: &str, port: u16) -> Result<()> {
    serve_author(service, host, port, false).await
}

pub async fn serve_live_author(service: Service, host: &str, port: u16) -> Result<()> {
    serve_author(service, host, port, true).await
}

async fn serve_author(service: Service, host: &str, port: u16, live_author: bool) -> Result<()> {
    if !validation::validate_world(service.world()).valid {
        return Err(Error::Validation(
            "source validation failed; run validate before serve".into(),
        ));
    }
    let listener = tokio::net::TcpListener::bind(loopback_address(host, port)?).await?;
    let address = listener.local_addr()?;
    let fingerprint = service.world().fingerprint.clone();
    let (mode, app) = match (service.world().revision.is_some(), live_author) {
        (true, _) => ("fixed read-only author snapshot", router(service)),
        (false, false) => ("read-only author worktree", worktree_router(service, false)),
        (false, true) => ("live local-author worktree", live_router(service)),
    };
    eprintln!("Ready: http://{address} — {mode} {fingerprint} (Ctrl+C to stop)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn api(
    State(service): State<Service>,
    path: std::result::Result<Path<String>, axum::extract::rejection::PathRejection>,
    RawQuery(raw): RawQuery,
) -> std::result::Result<Response, ApiError> {
    let Path(path) =
        path.map_err(|_| ApiError(Error::InvalidInput("route path is not valid UTF-8".into())))?;
    let operation = operation(&path, raw.as_deref())?;
    if let Operation::ReaderPreview { request } = operation {
        let reader = service.reader().clone();
        let worker = reader.clone();
        let prepared =
            tokio::task::spawn_blocking(move || worker.prepare_preview(request.as_bytes()))
                .await
                .map_err(|_| ApiError(Error::Data("request worker failed".into())))??;
        // Source work can outlive its installed policy. Fence the result after
        // the worker returns, at the HTTP response boundary as on the reader router.
        return Ok(reader_response(reader.emit(prepared)));
    }
    let result = tokio::task::spawn_blocking(move || service.execute(operation))
        .await
        .map_err(|_| ApiError(Error::Data("request worker failed".into())))??;
    Ok(Json(result).into_response())
}

/// Route and query decoding is closed: unknown and duplicate parameters fail.
pub fn operation(path: &str, raw: Option<&str>) -> Result<Operation> {
    // A fully percent-encoded closed catalogue request must retain the same
    // decoded 16 KiB limit as the native API and CLI. Other routes keep their
    // existing 16 KiB encoded-query bound.
    let query = if matches!(path, "catalogue/page" | "catalogue/labels") {
        parse_query_bounded(
            raw.unwrap_or(""),
            crate::catalogue::MAX_REQUEST_BYTES * 3 + 8,
        )?
    } else {
        parse_query(raw.unwrap_or(""))?
    };
    let segments: Vec<_> = path.split('/').collect();
    match segments.as_slice() {
        ["status"] => {
            allowed(&query, &[])?;
            Ok(Operation::Status)
        }
        ["validate"] => {
            allowed(&query, &[])?;
            Ok(Operation::Validate)
        }
        ["threads"] => {
            allowed(&query, &[])?;
            Ok(Operation::Threads)
        }
        ["hypotheses"] => {
            allowed(&query, &["status", "text", "maxItems", "maxCharacters"])?;
            Ok(Operation::HypothesisList {
                status: query.get("status").cloned(),
                text: query.get("text").cloned(),
                max_items: bounded_usize(&query, "maxItems", 100, 1, 1000)?,
                max_characters: bounded_usize(&query, "maxCharacters", 100000, 256, 1_000_000)?,
            })
        }
        ["hypothesis"] => {
            allowed(&query, &["hypothesisId", "maxCharacters"])?;
            Ok(Operation::HypothesisDetail {
                hypothesis_id: required(&query, "hypothesisId")?.into(),
                max_characters: bounded_usize(&query, "maxCharacters", 100000, 256, 1_000_000)?,
            })
        }
        ["hypotheses", "preview"] => {
            allowed(&query, &["request", "maxCharacters"])?;
            Ok(Operation::HypothesisPreview {
                request: crate::hypotheses::decode(required(&query, "request")?)?,
                max_characters: bounded_usize(&query, "maxCharacters", 100000, 256, 1_000_000)?,
            })
        }
        ["action-drafts", "list"] => {
            allowed(&query, &[])?;
            Ok(Operation::ActionDraftList)
        }
        ["edit-drafts", "list"] => {
            allowed(&query, &[])?;
            Ok(Operation::EditDraftList)
        }
        ["edit-drafts", "detail"] => {
            allowed(&query, &["id", "sha256"])?;
            Ok(Operation::EditDraftDetail {
                id: required(&query, "id")?.into(),
                expected_sha256: query.get("sha256").cloned(),
            })
        }
        ["edit-drafts", "preview"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::EditDraftPreview {
                request: crate::edit_drafts::decode_save(required(&query, "request")?)?,
            })
        }
        ["edit-drafts", "review"] => {
            allowed(
                &query,
                &["expectedFingerprint", "id", "sha256", "maxCharacters"],
            )?;
            let value = json!({"expectedFingerprint":required(&query,"expectedFingerprint")?,
                "editId":required(&query,"id")?,"expectedSha256":required(&query,"sha256")?,
                "maxCharacters":bounded_usize(&query,"maxCharacters",100000,256,1_000_000)?});
            Ok(Operation::EditDraftReview {
                request: crate::edit_drafts::decode_review(&value.to_string())?,
            })
        }
        ["reveal-plans", "resolve"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RevealResolve {
                request: crate::manuscript::decode_reveal_resolve(required(&query, "request")?)?,
            })
        }
        ["reader-presentation", "preview"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ReaderPreview {
                request: required(&query, "request")?.to_owned(),
            })
        }
        ["manuscripts", "list"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ManuscriptList {
                request: crate::manuscript::decode_list(required(&query, "request")?)?,
            })
        }
        ["manuscripts", "detail"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ManuscriptDetail {
                request: crate::manuscript::decode_detail(required(&query, "request")?)?,
            })
        }
        ["manuscripts", "resolve"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ManuscriptResolve {
                request: crate::manuscript::decode_resolve(required(&query, "request")?)?,
            })
        }
        ["manuscripts", "preview-save"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ManuscriptSavePreview {
                request: crate::manuscript::decode_save(required(&query, "request")?)?,
            })
        }
        ["retcon-impact", "preview"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RetconImpactPreview {
                request: crate::retcon_impact::decode_preview(required(&query, "request")?)?,
            })
        }
        ["retcon-reviews", "preview"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RetconReviewPreview {
                request: crate::retcon_reviews::decode_preview(required(&query, "request")?)?,
            })
        }
        ["retcon-reviews", "preview-save"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RetconReviewSavePreview {
                request: crate::retcon_reviews::decode_save(required(&query, "request")?)?,
            })
        }
        ["retcon-reviews", "queue"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RetconReviewQueue {
                request: crate::retcon_reviews::decode_queue(required(&query, "request")?)?,
            })
        }
        ["retcon-reviews", "detail"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RetconReviewDetail {
                request: crate::retcon_reviews::decode_detail(required(&query, "request")?)?,
            })
        }
        ["event-consequences"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::EventConsequences {
                request: crate::event_consequences::decode_event(required(&query, "request")?)?,
            })
        }
        ["grouped-consequences"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::GroupedConsequences {
                request: crate::event_consequences::decode_grouped(required(&query, "request")?)?,
            })
        }
        ["revision-review"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::RevisionReview {
                request: crate::revision_review::decode(required(&query, "request")?)?,
            })
        }
        ["action-drafts", "detail"] => {
            allowed(&query, &["id", "sha256"])?;
            Ok(Operation::ActionDraftDetail {
                id: required(&query, "id")?.into(),
                expected_sha256: query.get("sha256").cloned(),
            })
        }
        ["action-drafts", "review"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ActionDraftReview {
                request: crate::action_drafts::decode_review(required(&query, "request")?)?,
            })
        }
        [
            "action-drafts",
            name @ ("preview-proposal" | "review-outcome"),
        ] => {
            allowed(
                &query,
                &[
                    "expectedFingerprint",
                    "id",
                    "sha256",
                    "proposalId",
                    "maxCharacters",
                ],
            )?;
            let value = json!({"expectedFingerprint":required(&query,"expectedFingerprint")?,
                "actionId":required(&query,"id")?,"expectedSha256":required(&query,"sha256")?,
                "proposalId":required(&query,"proposalId")?,
                "maxCharacters":bounded_usize(&query,"maxCharacters",100000,256,1_000_000)?});
            Ok(if *name == "review-outcome" {
                Operation::ActionOutcomeReview {
                    request: crate::action_outcomes::decode_review(&value.to_string())?,
                }
            } else {
                Operation::ActionProposalPreview {
                    request: crate::action_store::decode_proposal_preview(&value.to_string())?,
                }
            })
        }
        ["action-drafts", "verify-outcome"] => {
            allowed(
                &query,
                &["expectedFingerprint", "id", "sha256", "maxCharacters"],
            )?;
            let value = json!({"expectedFingerprint":required(&query,"expectedFingerprint")?,
                "actionId":required(&query,"id")?,"expectedSha256":required(&query,"sha256")?,
                "maxCharacters":bounded_usize(&query,"maxCharacters",100000,256,1_000_000)?});
            Ok(Operation::ActionOutcomeVerify {
                request: crate::action_outcomes::decode_verify(&value.to_string())?,
            })
        }
        ["consequences", "check"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::ConsequencesCheck {
                request: crate::consequence_semantics::decode_check(required(&query, "request")?)?,
            })
        }
        ["action-requirements", name @ ("capture" | "compare")] => {
            let keys: &[&str] = if *name == "capture" {
                &["frame", "families", "maxCharacters"]
            } else {
                &["frame", "families", "maxCharacters", "requirement"]
            };
            allowed(&query, keys)?;
            let frame = crate::writing_frames::decode(required(&query, "frame")?)?;
            let families = crate::writing_frames::decode_families(required(&query, "families")?)?;
            let max_characters = bounded_usize(&query, "maxCharacters", 16000, 256, 100_000)?;
            Ok(if *name == "capture" {
                Operation::ActionRequirementCapture {
                    frame,
                    families,
                    max_characters,
                }
            } else {
                Operation::ActionRequirementCompare {
                    frame,
                    families,
                    requirement: crate::action_requirements::decode_requirement(required(
                        &query,
                        "requirement",
                    )?)?,
                    max_characters,
                }
            })
        }
        ["authoring", "schema"] => {
            allowed(&query, &[])?;
            Ok(Operation::AuthoringSchema)
        }
        ["authoring", "context"] => {
            allowed(&query, &["maxItems"])?;
            Ok(Operation::AuthoringContext {
                max_items: bounded_usize(
                    &query,
                    "maxItems",
                    crate::authoring_tools::MAX_ITEMS,
                    1,
                    crate::authoring_tools::MAX_ITEMS,
                )?,
            })
        }
        ["authoring", "scaffold"] => {
            allowed(&query, &["request"])?;
            Ok(Operation::AuthoringScaffold {
                request: crate::authoring_tools::decode_scaffold(required(&query, "request")?)?,
            })
        }
        ["entities"] => {
            allowed(&query, &["kind", "text"])?;
            Ok(Operation::EntityList {
                kind: query.get("kind").cloned(),
                text: query.get("text").cloned(),
            })
        }
        ["catalogue", name @ ("page" | "labels")] => {
            allowed(&query, &["request"])?;
            let request = crate::catalogue::decode(required(&query, "request")?)?;
            if request.operation() != *name {
                return Err(Error::InvalidInput(
                    "catalogue request operation does not match the route".into(),
                ));
            }
            Ok(Operation::Catalogue { request })
        }
        ["entity"] => {
            allowed(&query, &["id"])?;
            Ok(Operation::EntityShow {
                entity: required(&query, "id")?.into(),
            })
        }
        ["briefs", "compose"] => {
            allowed(
                &query,
                &[
                    "brief", "tick", "order", "timeline", "audience", "view", "limit",
                ],
            )?;
            Ok(Operation::BriefCompose {
                brief: required(&query, "brief")?.into(),
                at: optional_time(&query)?,
                audience: query
                    .get("audience")
                    .cloned()
                    .unwrap_or_else(|| "author".into()),
                view: query
                    .get("view")
                    .cloned()
                    .unwrap_or_else(|| "author".into()),
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["writing", name @ ("resolve" | "choices" | "context")] => writing_operation(name, &query),
        ["current-time", "preview"] => {
            allowed(&query, &["request", "maxCharacters"])?;
            Ok(Operation::CurrentTimePreview {
                request: crate::current_time::decode(required(&query, "request")?)?,
                max_characters: bounded_usize(&query, "maxCharacters", 100000, 256, 100000)?,
            })
        }
        ["scene-control", "preview"] => {
            allowed(&query, &["request", "maxCharacters"])?;
            Ok(Operation::SceneControlPreview {
                request: crate::scene_control::decode(required(&query, "request")?)?,
                max_characters: bounded_usize(&query, "maxCharacters", 100000, 256, 100000)?,
            })
        }
        ["conversation-control", "preview"] => {
            allowed(&query, &["request", "maxCharacters"])?;
            Ok(Operation::ConversationControlPreview {
                request: crate::conversation_control::decode(required(&query, "request")?)?,
                max_characters: bounded_usize(&query, "maxCharacters", 100000, 256, 100000)?,
            })
        }
        [
            domain @ ("descriptions" | "assertions" | "existence" | "governance"),
            name,
        ] => narrative_operation(domain, name, &query),
        [domain @ ("social" | "environment"), "context"] => {
            family_context_operation(domain, &query)
        }
        ["lore", name] => lore_operation(name, &query),
        ["species", name] => species_operation(name, &query),
        ["magic", name] => magic_operation(name, &query),
        ["generational", "learned", name] => learned_generational_operation(name, &query),
        [
            "generational",
            name @ ("labels" | "discover" | "search" | "context"),
        ] => generational_view_operation(name, &query, false),
        ["generational", "family"] => {
            allowed(&query, &["character", "tick", "order", "timeline", "limit"])?;
            Ok(Operation::GenerationalFamily {
                character: required(&query, "character")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", direction @ ("ancestors" | "descendants")] => {
            allowed(
                &query,
                &[
                    "character",
                    "tick",
                    "order",
                    "timeline",
                    "max_depth",
                    "limit",
                ],
            )?;
            Ok(Operation::GenerationalLineage {
                character: required(&query, "character")?.into(),
                at: time(&query)?,
                direction: if *direction == "ancestors" {
                    crate::generational::LineageDirection::Ancestors
                } else {
                    crate::generational::LineageDirection::Descendants
                },
                max_depth: bounded_usize(&query, "max_depth", 8, 0, 32)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", "path"] => {
            allowed(
                &query,
                &[
                    "from", "to", "tick", "order", "timeline", "max_hops", "limit",
                ],
            )?;
            Ok(Operation::GenerationalPath {
                from: required(&query, "from")?.into(),
                to: required(&query, "to")?.into(),
                at: time(&query)?,
                max_hops: bounded_usize(&query, "max_hops", 8, 0, 32)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", "history"] => {
            allowed(&query, &["record", "tick", "order", "timeline", "limit"])?;
            Ok(Operation::GenerationalHistory {
                record: required(&query, "record")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", "organization"] => {
            allowed(
                &query,
                &[
                    "organization",
                    "tick",
                    "order",
                    "timeline",
                    "max_depth",
                    "include_former",
                    "limit",
                ],
            )?;
            Ok(Operation::GenerationalOrganization {
                organization: required(&query, "organization")?.into(),
                at: time(&query)?,
                max_depth: bounded_usize(&query, "max_depth", 8, 0, 32)?,
                include_former: boolean(&query, "include_former")?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", "legacy"] => {
            allowed(&query, &["legacy", "tick", "order", "timeline", "limit"])?;
            Ok(Operation::GenerationalLegacy {
                legacy: required(&query, "legacy")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", "organization-legacies"] => {
            allowed(
                &query,
                &["organization", "tick", "order", "timeline", "limit"],
            )?;
            Ok(Operation::GenerationalOrganizationLegacies {
                organization: required(&query, "organization")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["generational", "vital"] => {
            allowed(&query, &["character", "tick", "order", "timeline", "limit"])?;
            Ok(Operation::GenerationalVital {
                character: required(&query, "character")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "location"] => {
            allowed(&query, &["id", "mode", "limit"])?;
            Ok(Operation::SpatialLocation {
                entity: required(&query, "id")?.into(),
                mode: query.get("mode").cloned(),
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "history"] => {
            allowed(&query, &["record", "tick", "order", "timeline", "limit"])?;
            Ok(Operation::SpatialHistory {
                record: required(&query, "record")?.into(),
                at: optional_time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "route"] => {
            allowed(&query, &["id", "tick", "order", "timeline"])?;
            Ok(Operation::SpatialRoute {
                route: required(&query, "id")?.into(),
                at: optional_time(&query)?,
            })
        }
        ["spatial", "path"] => {
            allowed(&query, &["from", "to", "mode", "metric", "unit", "limit"])?;
            Ok(Operation::SpatialPath {
                from: required(&query, "from")?.into(),
                to: required(&query, "to")?.into(),
                mode: query.get("mode").cloned(),
                metric: query
                    .get("metric")
                    .cloned()
                    .unwrap_or_else(|| "hops".into()),
                unit: query.get("unit").cloned(),
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "maps"] => {
            allowed(&query, &["limit"])?;
            Ok(Operation::SpatialMaps {
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "anchors"] => {
            allowed(&query, &["from_map", "to_map", "limit"])?;
            Ok(Operation::SpatialAnchors {
                from_map: query.get("from_map").cloned(),
                to_map: query.get("to_map").cloned(),
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "reachable"] => {
            allowed(&query, &["from", "mode", "limit"])?;
            Ok(Operation::SpatialReachable {
                from: required(&query, "from")?.into(),
                mode: query.get("mode").cloned(),
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "overlays"] => {
            allowed(
                &query,
                &[
                    "location", "tick", "order", "timeline", "audience", "view", "overlay", "limit",
                ],
            )?;
            Ok(Operation::SpatialOverlays {
                location: required(&query, "location")?.into(),
                at: time(&query)?,
                audience: query
                    .get("audience")
                    .cloned()
                    .unwrap_or_else(|| "author".into()),
                view: query
                    .get("view")
                    .cloned()
                    .unwrap_or_else(|| "author".into()),
                overlay: query.get("overlay").cloned(),
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["spatial", "bbox"] => {
            allowed(
                &query,
                &[
                    "map", "min_x", "min_y", "max_x", "max_y", "min_z", "max_z", "relation",
                    "limit", "tick", "order", "timeline",
                ],
            )?;
            if query.contains_key("min_z") != query.contains_key("max_z") {
                return Err(Error::InvalidInput(
                    "min_z and max_z must be supplied together".into(),
                ));
            }
            let map = required(&query, "map")?.into();
            let min = coordinate_input(&query, "min_x", "min_y", "min_z")?;
            let max = coordinate_input(&query, "max_x", "max_y", "max_z")?;
            let relation = query
                .get("relation")
                .cloned()
                .unwrap_or_else(|| "intersects".into());
            let limit = bounded_usize(&query, "limit", 100, 1, 1000)?;
            Ok(match optional_time(&query)? {
                Some(at) => Operation::SpatialBboxAt {
                    map,
                    min,
                    max,
                    relation,
                    at,
                    limit,
                },
                None => Operation::SpatialBbox {
                    map,
                    min,
                    max,
                    relation,
                    limit,
                },
            })
        }
        ["spatial", "nearby"] => {
            allowed(
                &query,
                &[
                    "map", "x", "y", "z", "radius", "limit", "tick", "order", "timeline",
                ],
            )?;
            let map = required(&query, "map")?.into();
            let point = coordinate_input(&query, "x", "y", "z")?;
            let radius = number(&query, "radius")?;
            let limit = bounded_usize(&query, "limit", 100, 1, 1000)?;
            Ok(match optional_time(&query)? {
                Some(at) => Operation::SpatialNearbyAt {
                    map,
                    point,
                    radius,
                    at,
                    limit,
                },
                None => Operation::SpatialNearby {
                    map,
                    point,
                    radius,
                    limit,
                },
            })
        }
        [
            "chronology",
            kind @ ("format" | "convert-date" | "search" | "story-times"),
        ] => {
            let mut keys = DATE_INPUT_KEYS.to_vec();
            match *kind {
                "convert-date" => keys.extend(["target_calendar", "target_era"]),
                "search" => {
                    keys.extend(UPPER_DATE_KEYS);
                    keys.extend([
                        "predicate",
                        "record",
                        "calendar_filter",
                        "era_filter",
                        "era_mode",
                        "limit",
                    ]);
                }
                "story-times" => {
                    keys.extend(UPPER_DATE_KEYS);
                    keys.extend(["timeline", "limit"]);
                }
                _ => {}
            }
            allowed(&query, &keys)?;
            let value = date_input(&query, "", None)?;
            match *kind {
                "format" => Ok(Operation::ChronologyFormat { value }),
                "convert-date" => {
                    let target = match (
                        optional_text(&query, "target_calendar")?,
                        optional_text(&query, "target_era")?,
                    ) {
                        (Some(calendar), None) => DateTarget::Calendar(calendar),
                        (None, Some(era)) => DateTarget::Era(era),
                        _ => {
                            return Err(Error::InvalidInput(
                                "choose exactly one target_calendar or target_era".into(),
                            ));
                        }
                    };
                    Ok(Operation::ChronologyConvertDate { value, target })
                }
                "search" => {
                    let upper = upper_date_input(&query, &value)?;
                    let era = optional_text(&query, "era_filter")?;
                    if query.contains_key("era_mode") && era.is_none() {
                        return Err(Error::InvalidInput("era_mode requires era_filter".into()));
                    }
                    Ok(Operation::ChronologySearch {
                        query: AnnotationQuery {
                            predicate: required(&query, "predicate")?.parse()?,
                            value,
                            upper,
                            record: optional_text(&query, "record")?,
                            calendar: optional_text(&query, "calendar_filter")?,
                            era,
                            era_mode: optional_text(&query, "era_mode")?
                                .map(|value| value.parse())
                                .transpose()?
                                .unwrap_or(EraMatchMode::Authored),
                            limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
                        },
                    })
                }
                "story-times" => {
                    let upper = upper_date_input(&query, &value)?;
                    Ok(Operation::ChronologyStoryTimes {
                        value,
                        upper,
                        timeline: optional_text(&query, "timeline")?,
                        limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
                    })
                }
                _ => unreachable!("matched chronology operation"),
            }
        }
        ["chronology", "annotations"] => {
            allowed(&query, &["record", "calendar", "era", "limit"])?;
            Ok(Operation::ChronologyAnnotations {
                record: optional_text(&query, "record")?,
                calendar: optional_text(&query, "calendar")?,
                era: optional_text(&query, "era")?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["chronology", "compare"] => {
            allowed(
                &query,
                &["left_record", "left_id", "right_record", "right_id"],
            )?;
            Ok(Operation::ChronologyCompare {
                left_record: required(&query, "left_record")?.into(),
                left_id: required(&query, "left_id")?.into(),
                right_record: required(&query, "right_record")?.into(),
                right_id: required(&query, "right_id")?.into(),
            })
        }
        ["chronology", "calendars"] => {
            allowed(&query, &["limit"])?;
            Ok(Operation::ChronologyCalendars {
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        ["chronology", "date"] => {
            allowed(&query, &["calendar", "year", "month", "day", "era"])?;
            Ok(Operation::ChronologyDate {
                calendar: required(&query, "calendar")?.into(),
                value: civil_date(&query)?,
                era: optional_text(&query, "era")?,
            })
        }
        ["chronology", "convert"] => {
            allowed(&query, &["from", "to", "year", "month", "day"])?;
            Ok(Operation::ChronologyConvert {
                from: required(&query, "from")?.into(),
                to: required(&query, "to")?.into(),
                value: civil_date(&query)?,
            })
        }
        ["chronology", "to-story"] => {
            allowed(&query, &["calendar", "year", "month", "day", "timeline"])?;
            Ok(Operation::ChronologyToStory {
                calendar: required(&query, "calendar")?.into(),
                value: civil_date(&query)?,
                timeline: optional_text(&query, "timeline")?,
            })
        }
        ["chronology", "from-story"] => {
            allowed(&query, &["calendar", "tick", "order", "timeline"])?;
            Ok(Operation::ChronologyFromStory {
                calendar: required(&query, "calendar")?.into(),
                at: TimeInput {
                    tick: exact_integer(required(&query, "tick")?, "tick")?,
                    order: query
                        .get("order")
                        .map(|value| exact_integer(value, "order"))
                        .transpose()?
                        .unwrap_or(0),
                    timeline: optional_text(&query, "timeline")?,
                },
            })
        }
        ["scene"] => {
            allowed(&query, &["id", "tick", "order", "timeline"])?;
            Ok(Operation::Scene {
                entity: required(&query, "id")?.into(),
                at: time(&query)?,
            })
        }
        [
            "character",
            name @ ("species" | "species-affected" | "magic" | "social" | "environment"),
        ] => character_family_operation(name, &query),
        ["character", "brief"] => {
            allowed(
                &query,
                &["character", "brief", "tick", "order", "timeline", "limit"],
            )?;
            Ok(Operation::CharacterBrief {
                character: required(&query, "character")?.into(),
                brief: required(&query, "brief")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 100, 1, 1000)?,
            })
        }
        [
            "character",
            name @ ("assertions" | "descriptions" | "section" | "lore" | "concept" | "links"
            | "spatial-history" | "existence" | "governance"),
        ] => character_narrative_operation(name, &query),
        ["character", "context"] => {
            allowed(
                &query,
                &["character", "tick", "order", "timeline", "maxCharacters"],
            )?;
            Ok(Operation::CharacterContext {
                character: required(&query, "character")?.into(),
                at: time(&query)?,
                max_characters: bounded_usize(&query, "maxCharacters", 16000, 256, 100_000)?,
            })
        }
        ["character", "search"] => {
            allowed(
                &query,
                &[
                    "character",
                    "q",
                    "tick",
                    "order",
                    "timeline",
                    "limit",
                    "mode",
                ],
            )?;
            let mode = query.get("mode").map(|value| value.parse()).transpose()?;
            let character = required(&query, "character")?.into();
            let text = required(&query, "q")?.into();
            let at = time(&query)?;
            let limit = bounded_usize(&query, "limit", 20, 1, 100)?;
            Ok(match mode {
                Some(mode) => Operation::CharacterSearchWithMode {
                    character,
                    query: text,
                    at,
                    limit,
                    mode,
                },
                None => Operation::CharacterSearch {
                    character,
                    query: text,
                    at,
                    limit,
                },
            })
        }
        ["character", "conversation"] => {
            allowed(&query, &["character", "id", "tick", "order", "timeline"])?;
            Ok(Operation::CharacterConversation {
                character: required(&query, "character")?.into(),
                conversation: required(&query, "id")?.into(),
                at: time(&query)?,
            })
        }
        ["state"] => {
            allowed(&query, &["entity", "tick", "order", "timeline"])?;
            Ok(Operation::State {
                entity: required(&query, "entity")?.into(),
                at: time(&query)?,
            })
        }
        ["knowledge"] => {
            allowed(
                &query,
                &["character", "tick", "order", "timeline", "includeForgotten"],
            )?;
            Ok(Operation::Knowledge {
                entity: required(&query, "character")?.into(),
                at: time(&query)?,
                include_forgotten: boolean(&query, "includeForgotten")?,
            })
        }
        ["relationships"] => {
            allowed(&query, &["character", "tick", "order", "timeline"])?;
            Ok(Operation::Relationships {
                entity: required(&query, "character")?.into(),
                at: time(&query)?,
            })
        }
        ["conversation"] => {
            allowed(&query, &["id", "tick", "order", "timeline"])?;
            Ok(Operation::Conversation {
                entity: required(&query, "id")?.into(),
                at: time(&query)?,
            })
        }
        ["entities", id] if !id.is_empty() => {
            allowed(&query, &[])?;
            Ok(Operation::EntityShow {
                entity: (*id).into(),
            })
        }
        ["entities", id, "state"] => {
            allowed(&query, &["tick", "order", "timeline"])?;
            Ok(Operation::State {
                entity: (*id).into(),
                at: time(&query)?,
            })
        }
        ["entities", id, "knowledge"] => {
            allowed(&query, &["tick", "order", "timeline", "includeForgotten"])?;
            Ok(Operation::Knowledge {
                entity: (*id).into(),
                at: time(&query)?,
                include_forgotten: boolean(&query, "includeForgotten")?,
            })
        }
        ["entities", id, "relationships"] => {
            allowed(&query, &["tick", "order", "timeline"])?;
            Ok(Operation::Relationships {
                entity: (*id).into(),
                at: time(&query)?,
            })
        }
        ["conversations", id] => {
            allowed(&query, &["tick", "order", "timeline"])?;
            Ok(Operation::Conversation {
                entity: (*id).into(),
                at: time(&query)?,
            })
        }
        ["timeline"] => {
            allowed(&query, &["tick", "order", "timeline", "limit"])?;
            if query.contains_key("order") && !query.contains_key("tick") {
                return Err(Error::InvalidInput("order requires tick".into()));
            }
            Ok(Operation::Timeline {
                timeline: query.get("timeline").cloned(),
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
                at: query
                    .contains_key("tick")
                    .then(|| time(&query))
                    .transpose()?,
            })
        }
        ["story-points"] => {
            allowed(&query, &["tick", "order", "timeline", "scene", "limit"])?;
            Ok(Operation::StoryPoints {
                at: time(&query)?,
                scene: optional_text(&query, "scene")?,
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
            })
        }
        ["story-point"] => {
            allowed(
                &query,
                &["id", "tick", "order", "timeline", "scene", "limit"],
            )?;
            Ok(Operation::StoryPointTrail {
                story_point: required(&query, "id")?.into(),
                at: time(&query)?,
                scene: optional_text(&query, "scene")?,
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
            })
        }
        ["causal"] => {
            allowed(
                &query,
                &["event", "tick", "order", "timeline", "direction", "limit"],
            )?;
            Ok(Operation::Causal {
                event: required(&query, "event")?.into(),
                at: time(&query)?,
                direction: optional_text(&query, "direction")?.unwrap_or_else(|| "upstream".into()),
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
            })
        }
        ["whereabouts"] => {
            allowed(
                &query,
                &["characters", "tick", "order", "timeline", "limit"],
            )?;
            Ok(Operation::Whereabouts {
                at: time(&query)?,
                characters: query
                    .get("characters")
                    .map(|raw| crate::service::navigation_ids_json(raw, "character", 32))
                    .transpose()?,
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
            })
        }
        ["interactions"] => {
            allowed(
                &query,
                &["first", "second", "tick", "order", "timeline", "limit"],
            )?;
            Ok(Operation::Interactions {
                first: required(&query, "first")?.into(),
                second: required(&query, "second")?.into(),
                at: time(&query)?,
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
            })
        }
        ["thread-memberships"] => {
            allowed(
                &query,
                &["records", "threads", "tick", "order", "timeline", "limit"],
            )?;
            Ok(Operation::ThreadMemberships {
                record_ids: crate::service::navigation_ids_json(
                    required(&query, "records")?,
                    "record",
                    256,
                )?,
                thread_ids: crate::service::navigation_ids_json(
                    required(&query, "threads")?,
                    "thread",
                    32,
                )?,
                at: optional_time(&query)?,
                limit: bounded_usize(&query, "limit", 1000, 1, 1000)?,
            })
        }
        ["environments"] => {
            allowed(&query, &["entity", "tick", "order", "timeline"])?;
            Ok(Operation::Environments {
                entity: required(&query, "entity")?.into(),
                at: time(&query)?,
            })
        }
        ["search"] => {
            allowed(
                &query,
                &[
                    "q",
                    "limit",
                    "kind",
                    "mode",
                    "tick",
                    "order",
                    "timeline",
                    "threads",
                    "includeHypotheses",
                ],
            )?;
            let mode = query.get("mode").map(|value| value.parse()).transpose()?;
            let text = required(&query, "q")?.into();
            let limit = bounded_usize(&query, "limit", 20, 1, 100)?;
            let kind = query.get("kind").cloned();
            let at = optional_time(&query)?;
            let thread_ids = query
                .get("threads")
                .map(|raw| crate::service::thread_ids_json(raw))
                .transpose()?;
            if boolean(&query, "includeHypotheses")? {
                return Ok(Operation::SearchWithHypotheses {
                    query: text,
                    limit,
                    kind,
                    mode: mode.unwrap_or(crate::retrieval::Mode::Lexical),
                    at,
                    thread_ids,
                });
            }
            if at.is_some() || thread_ids.is_some() {
                return Ok(Operation::SearchWithSelection {
                    query: text,
                    limit,
                    kind,
                    mode: mode.unwrap_or(crate::retrieval::Mode::Lexical),
                    at,
                    thread_ids,
                });
            }
            Ok(match mode {
                Some(mode) => Operation::SearchWithMode {
                    query: text,
                    limit,
                    kind,
                    mode,
                },
                None => Operation::Search {
                    query: text,
                    limit,
                    kind,
                },
            })
        }
        ["context"] => {
            allowed(
                &query,
                &["character", "tick", "order", "timeline", "maxCharacters"],
            )?;
            Ok(Operation::Context {
                entity: required(&query, "character")?.into(),
                at: time(&query)?,
                max_characters: bounded_usize(&query, "maxCharacters", 16000, 256, 100_000)?,
            })
        }
        _ => Err(Error::NotFound(format!("route /api/{path} does not exist"))),
    }
}

fn required<'a>(query: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    query
        .get(key)
        .filter(|s| !s.trim().is_empty())
        .map(String::as_str)
        .ok_or_else(|| Error::InvalidInput(format!("{key} is required")))
}

fn character_narrative_operation(
    name: &str,
    query: &BTreeMap<String, String>,
) -> Result<Operation> {
    let mut keys = vec!["character", "tick", "order", "timeline"];
    keys.extend_from_slice(match name {
        "assertions" | "existence" | "governance" => &["target", "limit"],
        "descriptions" | "links" | "spatial-history" => &["owner", "limit"],
        "section" => &["owner", "section"],
        "concept" => &["target"],
        "lore" => &["domain", "limit"],
        _ => return Err(Error::NotFound("route does not exist".into())),
    });
    allowed(query, &keys)?;
    let character = required(query, "character")?.into();
    let at = time(query)?;
    let limit = bounded_usize(query, "limit", 100, 1, 1000)?;
    Ok(match name {
        "assertions" => Operation::CharacterAssertions {
            character,
            target: required(query, "target")?.into(),
            at,
            limit,
        },
        "descriptions" => Operation::CharacterDescriptions {
            character,
            owner: required(query, "owner")?.into(),
            at,
            limit,
        },
        "section" => Operation::CharacterSection {
            character,
            owner: required(query, "owner")?.into(),
            section: required(query, "section")?.into(),
            at,
        },
        "lore" => Operation::CharacterLore {
            character,
            domain: query.get("domain").cloned(),
            at,
            limit,
        },
        "concept" => Operation::CharacterConcept {
            character,
            target: required(query, "target")?.into(),
            at,
        },
        "links" => Operation::CharacterLinks {
            character,
            owner: required(query, "owner")?.into(),
            at,
            limit,
        },
        "spatial-history" => Operation::CharacterSpatialHistory {
            character,
            owner: required(query, "owner")?.into(),
            at,
            limit,
        },
        "existence" => Operation::CharacterExistence {
            character,
            target: required(query, "target")?.into(),
            at,
            limit,
        },
        "governance" => Operation::CharacterGovernance {
            character,
            target: required(query, "target")?.into(),
            at,
            limit,
        },
        _ => unreachable!(),
    })
}

fn writing_operation(name: &str, query: &BTreeMap<String, String>) -> Result<Operation> {
    let keys: &[&str] = match name {
        "resolve" => &["frame"],
        "choices" => &["frame", "limit"],
        "context" => &["frame", "briefs", "families", "maxCharacters"],
        _ => return Err(Error::NotFound("route does not exist".into())),
    };
    allowed(query, keys)?;
    let frame = crate::writing_frames::decode(required(query, "frame")?)?;
    Ok(match name {
        "resolve" => Operation::WritingResolve { frame },
        "choices" => Operation::WritingChoices {
            frame,
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        },
        "context" => {
            let briefs = query
                .get("briefs")
                .map(|value| crate::service::writing_briefs_json(value))
                .transpose()?
                .unwrap_or_default();
            let max_characters = bounded_usize(query, "maxCharacters", 16000, 256, 100_000)?;
            if let Some(families) = query.get("families") {
                Operation::WritingFamilyContext {
                    frame,
                    briefs,
                    families: crate::writing_frames::decode_families(families)?,
                    max_characters,
                }
            } else {
                Operation::WritingContext {
                    frame,
                    briefs,
                    max_characters,
                }
            }
        }
        _ => unreachable!(),
    })
}

fn character_family_operation(name: &str, query: &BTreeMap<String, String>) -> Result<Operation> {
    let mut keys = vec!["character", "tick", "order", "timeline"];
    match name {
        "species" | "species-affected" => {
            keys.extend([
                "id",
                "common_profile",
                "individual_profile",
                "individual_link",
                "common_link",
                "common_fields",
                "individual_fields",
            ]);
            keys.push(if name == "species" {
                "beliefTargets"
            } else {
                "limit"
            });
        }
        "magic" => keys.push("beliefTargets"),
        "social" => keys.extend(["language", "sense", "form", "contexts", "beliefTargets"]),
        "environment" => keys.extend(["contexts", "beliefTargets"]),
        _ => return Err(Error::NotFound("route does not exist".into())),
    }
    allowed(query, &keys)?;
    let character = required(query, "character")?.into();
    let at = time(query)?;
    let belief_targets = if name == "magic" {
        crate::service::belief_targets_json(required(query, "beliefTargets")?)?
    } else {
        query
            .get("beliefTargets")
            .map(|value| crate::service::belief_targets_json(value))
            .transpose()?
            .unwrap_or_default()
    };
    Ok(match name {
        "species" | "species-affected" => {
            let selection = crate::species::Selection {
                common_profile: required(query, "common_profile")?.into(),
                individual_profile: required(query, "individual_profile")?.into(),
                individual_link: required(query, "individual_link")?.into(),
                common_link: optional_text(query, "common_link")?,
                common_fields: query
                    .get("common_fields")
                    .map(|value| crate::service::species_fields_json(value))
                    .transpose()?
                    .unwrap_or_default(),
                individual_fields: query
                    .get("individual_fields")
                    .map(|value| crate::service::species_fields_json(value))
                    .transpose()?
                    .unwrap_or_default(),
            };
            selection.validate()?;
            if name == "species" {
                Operation::CharacterSpeciesCompose {
                    character,
                    individual: required(query, "id")?.into(),
                    selection,
                    belief_targets,
                    at,
                }
            } else {
                Operation::CharacterSpeciesAffected {
                    character,
                    common: required(query, "id")?.into(),
                    selection,
                    at,
                    limit: bounded_usize(query, "limit", 100, 1, 1000)?,
                }
            }
        }
        "magic" => Operation::CharacterMagic {
            character,
            belief_targets,
            at,
        },
        "social" => Operation::CharacterSocialContext {
            character,
            selected: crate::social::Selection {
                language: required(query, "language")?.into(),
                sense: query.get("sense").cloned(),
                form: query.get("form").cloned(),
                contexts: crate::service::context_roots_json(
                    query.get("contexts").map(String::as_str).unwrap_or("[]"),
                )?,
            },
            belief_targets,
            at,
        },
        "environment" => Operation::CharacterEnvironmentContext {
            character,
            roots: crate::service::context_roots_json(
                query.get("contexts").map(String::as_str).unwrap_or("[]"),
            )?,
            belief_targets,
            at,
        },
        _ => unreachable!(),
    })
}

fn narrative_operation(
    domain: &str,
    name: &str,
    query: &BTreeMap<String, String>,
) -> Result<Operation> {
    if domain == "assertions" && name == "predicates" {
        allowed(query, &["limit"])?;
        return Ok(Operation::AssertionPredicates {
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        });
    }
    let mut keys = vec!["tick", "order", "timeline", "audience", "view"];
    keys.extend_from_slice(match (domain, name) {
        ("descriptions", "list") => &["owner", "limit"],
        ("descriptions", "section") => &["owner", "section"],
        ("assertions", "list") => &["target", "limit"],
        ("existence", "list") => &["character", "limit"],
        ("governance", "query") => &["target", "profile", "roles", "limit"],
        _ => return Err(Error::NotFound("route does not exist".into())),
    });
    allowed(query, &keys)?;
    let at = optional_time(query)?;
    let audience = query
        .get("audience")
        .cloned()
        .unwrap_or_else(|| "author".into());
    let view = query
        .get("view")
        .cloned()
        .unwrap_or_else(|| "author".into());
    Ok(match (domain, name) {
        ("descriptions", "list") => Operation::DescriptionList {
            owner: required(query, "owner")?.into(),
            at,
            audience,
            view,
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        },
        ("descriptions", "section") => Operation::DescriptionSection {
            owner: required(query, "owner")?.into(),
            section: required(query, "section")?.into(),
            at,
            audience,
            view,
        },
        ("assertions", "list") => Operation::AssertionList {
            target: required(query, "target")?.into(),
            at,
            audience,
            view,
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        },
        ("existence", "list") => Operation::ExistenceList {
            character: required(query, "character")?.into(),
            at,
            audience,
            view,
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        },
        ("governance", "query") => Operation::GovernanceQuery {
            target: required(query, "target")?.into(),
            binding: crate::service::governance_binding_json(
                required(query, "profile")?,
                required(query, "roles")?,
            )?,
            at,
            audience,
            view,
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        },
        _ => unreachable!(),
    })
}

fn family_context_operation(domain: &str, query: &BTreeMap<String, String>) -> Result<Operation> {
    let mut keys = vec!["contexts", "tick", "order", "timeline", "audience", "view"];
    if domain == "social" {
        keys.extend(["language", "sense", "form"]);
    }
    allowed(query, &keys)?;
    let at = time(query)?;
    let audience = query
        .get("audience")
        .cloned()
        .unwrap_or_else(|| "author".into());
    let view = query
        .get("view")
        .cloned()
        .unwrap_or_else(|| "author".into());
    let roots = crate::service::context_roots_json(
        query.get("contexts").map(String::as_str).unwrap_or("[]"),
    )?;
    Ok(if domain == "social" {
        Operation::SocialContext {
            selected: crate::social::Selection {
                language: required(query, "language")?.into(),
                sense: query.get("sense").cloned(),
                form: query.get("form").cloned(),
                contexts: roots,
            },
            at,
            audience,
            view,
        }
    } else {
        Operation::EnvironmentContext {
            roots,
            at,
            audience,
            view,
        }
    })
}

fn lore_operation(name: &str, query: &BTreeMap<String, String>) -> Result<Operation> {
    match name {
        "state" => {
            allowed(
                query,
                &["record", "tick", "order", "timeline", "audience", "view"],
            )?;
            Ok(Operation::LoreState {
                record: required(query, "record")?.into(),
                at: time(query)?,
                audience: query
                    .get("audience")
                    .cloned()
                    .unwrap_or_else(|| "author".into()),
                view: query
                    .get("view")
                    .cloned()
                    .unwrap_or_else(|| "author".into()),
            })
        }
        "profiles" => {
            allowed(query, &["limit"])?;
            Ok(Operation::LoreProfiles {
                limit: bounded_usize(query, "limit", 100, 1, 1000)?,
            })
        }
        "profile" => {
            allowed(query, &["id"])?;
            Ok(Operation::LoreProfile {
                profile: required(query, "id")?.into(),
            })
        }
        "catalog" | "concept" | "links" => {
            let mut keys = vec!["tick", "order", "timeline", "audience", "view"];
            keys.extend_from_slice(match name {
                "catalog" => &["domain", "limit"],
                "concept" => &["id"],
                "links" => &["record", "limit"],
                _ => unreachable!(),
            });
            allowed(query, &keys)?;
            let at = optional_time(query)?;
            let audience = query
                .get("audience")
                .cloned()
                .unwrap_or_else(|| "author".into());
            let view = query
                .get("view")
                .cloned()
                .unwrap_or_else(|| "author".into());
            Ok(match name {
                "catalog" => Operation::LoreCatalog {
                    at,
                    audience,
                    view,
                    domain: optional_text(query, "domain")?,
                    limit: bounded_usize(query, "limit", 100, 1, 1000)?,
                },
                "concept" => Operation::LoreConcept {
                    concept: required(query, "id")?.into(),
                    at,
                    audience,
                    view,
                },
                "links" => Operation::LoreLinks {
                    record: required(query, "record")?.into(),
                    at,
                    audience,
                    view,
                    limit: bounded_usize(query, "limit", 100, 1, 1000)?,
                },
                _ => unreachable!(),
            })
        }
        _ => Err(Error::NotFound("route does not exist".into())),
    }
}

fn magic_operation(name: &str, query: &BTreeMap<String, String>) -> Result<Operation> {
    if name != "compose" {
        return Err(Error::NotFound("route does not exist".into()));
    }
    allowed(
        query,
        &[
            "id",
            "profiles",
            "links",
            "restrictions",
            "exceptions",
            "tick",
            "order",
            "timeline",
            "audience",
            "view",
        ],
    )?;
    Ok(Operation::MagicCompose {
        root: required(query, "id")?.into(),
        selection: crate::service::magic_selection_json(
            required(query, "profiles")?,
            query.get("links").map(String::as_str),
            query.get("restrictions").map(String::as_str),
            query.get("exceptions").map(String::as_str),
        )?,
        at: optional_time(query)?,
        audience: query
            .get("audience")
            .cloned()
            .unwrap_or_else(|| "author".into()),
        view: query
            .get("view")
            .cloned()
            .unwrap_or_else(|| "author".into()),
    })
}

fn species_operation(name: &str, query: &BTreeMap<String, String>) -> Result<Operation> {
    if !["compose", "affected"].contains(&name) {
        return Err(Error::NotFound("route does not exist".into()));
    }
    let mut keys = vec![
        "id",
        "common_profile",
        "individual_profile",
        "individual_link",
        "common_link",
        "common_fields",
        "individual_fields",
        "tick",
        "order",
        "timeline",
        "audience",
        "view",
    ];
    if name == "affected" {
        keys.push("limit");
    }
    allowed(query, &keys)?;
    let selection = crate::species::Selection {
        common_profile: required(query, "common_profile")?.into(),
        individual_profile: required(query, "individual_profile")?.into(),
        individual_link: required(query, "individual_link")?.into(),
        common_link: optional_text(query, "common_link")?,
        common_fields: query
            .get("common_fields")
            .map(|value| crate::service::species_fields_json(value))
            .transpose()?
            .unwrap_or_default(),
        individual_fields: query
            .get("individual_fields")
            .map(|value| crate::service::species_fields_json(value))
            .transpose()?
            .unwrap_or_default(),
    };
    selection.validate()?;
    let at = optional_time(query)?;
    let audience = query
        .get("audience")
        .cloned()
        .unwrap_or_else(|| "author".into());
    let view = query
        .get("view")
        .cloned()
        .unwrap_or_else(|| "author".into());
    if name == "compose" {
        Ok(Operation::SpeciesCompose {
            individual: required(query, "id")?.into(),
            selection,
            at,
            audience,
            view,
        })
    } else {
        Ok(Operation::SpeciesAffected {
            common: required(query, "id")?.into(),
            selection,
            at,
            audience,
            view,
            limit: bounded_usize(query, "limit", 100, 1, 1000)?,
        })
    }
}

fn generational_view_operation(
    name: &str,
    query: &BTreeMap<String, String>,
    learned: bool,
) -> Result<Operation> {
    let mut keys = vec!["tick", "order", "timeline", "limit"];
    if learned {
        keys.push("viewer");
    }
    keys.extend_from_slice(match name {
        "labels" => &["ids"],
        "discover" => &["kind", "prefix"],
        "search" => &["q"],
        "context" => &["subject", "max_depth", "max_chars"],
        _ => return Err(Error::NotFound("route does not exist".into())),
    });
    allowed(query, &keys)?;
    let viewer = learned
        .then(|| required(query, "viewer").map(str::to_owned))
        .transpose()?;
    let at = time(query)?;
    let limit = bounded_usize(query, "limit", 100, 1, 1000)?;
    Ok(match name {
        "labels" => Operation::GenerationalLabels {
            viewer,
            ids: required(query, "ids")?
                .split(',')
                .map(str::to_owned)
                .collect(),
            at,
            limit,
        },
        "discover" => Operation::GenerationalDiscover {
            viewer,
            kind: required(query, "kind")?.into(),
            prefix: query.get("prefix").cloned().unwrap_or_default(),
            at,
            limit,
        },
        "search" => Operation::GenerationalSearch {
            viewer,
            text: required(query, "q")?.into(),
            at,
            limit,
        },
        "context" => Operation::GenerationalContext {
            viewer,
            subject: required(query, "subject")?.into(),
            at,
            max_depth: bounded_usize(query, "max_depth", 8, 0, 32)?,
            limit,
            max_chars: bounded_usize(query, "max_chars", 12_000, 80, 1_000_000)?,
        },
        _ => unreachable!("operation checked above"),
    })
}

fn learned_generational_operation(
    name: &str,
    query: &BTreeMap<String, String>,
) -> Result<Operation> {
    if matches!(name, "labels" | "discover" | "search" | "context") {
        return generational_view_operation(name, query, true);
    }
    let mut keys = vec!["viewer", "tick", "order", "timeline", "limit"];
    keys.extend_from_slice(match name {
        "assertions" => &["kind", "endpoint", "held_history"],
        "history" => &["endpoint"],
        "family" | "vital" => &["character"],
        "ancestors" | "descendants" => &["character", "max_depth"],
        "path" => &["from", "to", "max_hops"],
        "organization" => &["organization", "max_depth"],
        "legacy" => &["legacy"],
        _ => return Err(Error::NotFound("route does not exist".into())),
    });
    allowed(query, &keys)?;
    let viewer = required(query, "viewer")?.to_owned();
    let at = time(query)?;
    let limit = bounded_usize(query, "limit", 100, 1, 1000)?;
    Ok(match name {
        "assertions" => Operation::GenerationalLearnedAssertions {
            viewer,
            at,
            kind: optional_text(query, "kind")?,
            endpoint: optional_text(query, "endpoint")?,
            held_history: boolean(query, "held_history")?,
            limit,
        },
        "history" => Operation::GenerationalLearnedAssertions {
            viewer,
            at,
            kind: None,
            endpoint: Some(required(query, "endpoint")?.into()),
            held_history: true,
            limit,
        },
        "family" => Operation::GenerationalLearnedFamily {
            viewer,
            character: required(query, "character")?.into(),
            at,
            limit,
        },
        "ancestors" | "descendants" => Operation::GenerationalLearnedLineage {
            viewer,
            character: required(query, "character")?.into(),
            at,
            direction: if name == "ancestors" {
                crate::generational::LineageDirection::Ancestors
            } else {
                crate::generational::LineageDirection::Descendants
            },
            max_depth: bounded_usize(query, "max_depth", 8, 0, 32)?,
            limit,
        },
        "path" => Operation::GenerationalLearnedPath {
            viewer,
            from: required(query, "from")?.into(),
            to: required(query, "to")?.into(),
            at,
            max_hops: bounded_usize(query, "max_hops", 8, 0, 32)?,
            limit,
        },
        "organization" => Operation::GenerationalLearnedOrganization {
            viewer,
            organization: required(query, "organization")?.into(),
            at,
            max_depth: bounded_usize(query, "max_depth", 8, 0, 32)?,
            limit,
        },
        "legacy" => Operation::GenerationalLearnedLegacy {
            viewer,
            legacy: required(query, "legacy")?.into(),
            at,
            limit,
        },
        "vital" => Operation::GenerationalLearnedVital {
            viewer,
            character: required(query, "character")?.into(),
            at,
            limit,
        },
        _ => unreachable!("operation checked above"),
    })
}

fn allowed(query: &BTreeMap<String, String>, keys: &[&str]) -> Result<()> {
    for key in query.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(Error::InvalidInput(format!(
                "unknown query parameter: {key}"
            )));
        }
    }
    Ok(())
}

fn time(query: &BTreeMap<String, String>) -> Result<TimeInput> {
    let tick = required(query, "tick")?
        .parse::<i64>()
        .map_err(|_| Error::InvalidInput("tick must be a signed 64-bit integer".into()))?;
    let order = query
        .get("order")
        .map(|s| s.parse::<i32>())
        .transpose()
        .map_err(|_| Error::InvalidInput("order must be a signed 32-bit integer".into()))?
        .unwrap_or(i32::MAX);
    Ok(TimeInput {
        tick,
        order,
        timeline: query.get("timeline").cloned(),
    })
}

fn optional_time(query: &BTreeMap<String, String>) -> Result<Option<TimeInput>> {
    if query.contains_key("tick") {
        return time(query).map(Some);
    }
    if query.contains_key("order") || query.contains_key("timeline") {
        return Err(Error::InvalidInput(
            "order and timeline require tick".into(),
        ));
    }
    Ok(None)
}

fn optional_text(query: &BTreeMap<String, String>, key: &str) -> Result<Option<String>> {
    query
        .get(key)
        .map(|_| required(query, key).map(str::to_owned))
        .transpose()
}

fn exact_integer<T: std::str::FromStr + ToString>(value: &str, key: &str) -> Result<T> {
    value
        .parse::<T>()
        .ok()
        .filter(|number| number.to_string() == value)
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "{key} must be a canonical decimal integer in range"
            ))
        })
}

fn civil_date(query: &BTreeMap<String, String>) -> Result<CivilDate> {
    let value = CivilDate {
        year: exact_integer(required(query, "year")?, "year")?,
        month: exact_integer(required(query, "month")?, "month")?,
        day: exact_integer(required(query, "day")?, "day")?,
    };
    if value.month == 0 || value.day == 0 {
        return Err(Error::InvalidInput(
            "month and day must be positive integers".into(),
        ));
    }
    Ok(value)
}

const DATE_INPUT_KEYS: &[&str] = &["value", "calendar", "era", "year", "month", "day"];
const UPPER_DATE_KEYS: &[&str] = &[
    "upper_value",
    "upper_calendar",
    "upper_era",
    "upper_year",
    "upper_month",
    "upper_day",
];

fn date_input(
    query: &BTreeMap<String, String>,
    prefix: &str,
    inherit: Option<&DateInput>,
) -> Result<DateInput> {
    let key = |name: &str| format!("{prefix}{name}");
    if query.contains_key(&key("value")) {
        if ["calendar", "era", "year", "month", "day"]
            .iter()
            .any(|name| query.contains_key(&key(name)))
        {
            return Err(Error::InvalidInput(format!(
                "{} cannot be combined with date component fields",
                key("value")
            )));
        }
        return DateInput::parse_json(required(query, &key("value"))?);
    }
    let mut calendar = optional_text(query, &key("calendar"))?;
    let mut era = optional_text(query, &key("era"))?;
    let year = exact_integer(required(query, &key("year"))?, &key("year"))?;
    let component = |name: &str| -> Result<Option<u32>> {
        let field = key(name);
        query
            .get(&field)
            .map(|value| {
                let number = exact_integer(value, &field)?;
                if number == 0 {
                    return Err(Error::InvalidInput(format!("{field} must be positive")));
                }
                Ok(number)
            })
            .transpose()
    };
    let month = component("month")?;
    let day = component("day")?;
    if day.is_some() && month.is_none() {
        return Err(Error::InvalidInput(format!(
            "{} requires {}",
            key("day"),
            key("month")
        )));
    }
    if calendar.is_none() && era.is_none() {
        match inherit {
            Some(DateInput::Civil {
                calendar: identifier,
                ..
            }) => calendar = Some(identifier.clone()),
            Some(DateInput::Era {
                era: identifier, ..
            }) => era = Some(identifier.clone()),
            _ => {}
        }
    }
    match (calendar, era) {
        (Some(calendar), None) => Ok(DateInput::Civil {
            calendar,
            year,
            month,
            day,
        }),
        (None, Some(era)) => Ok(DateInput::Era {
            era,
            year,
            month,
            day,
        }),
        _ => Err(Error::InvalidInput(format!(
            "choose exactly one {} or {}",
            key("calendar"),
            key("era")
        ))),
    }
}

fn upper_date_input(
    query: &BTreeMap<String, String>,
    lower: &DateInput,
) -> Result<Option<DateInput>> {
    if UPPER_DATE_KEYS.iter().any(|key| query.contains_key(*key)) {
        date_input(query, "upper_", Some(lower)).map(Some)
    } else {
        Ok(None)
    }
}

fn boolean(query: &BTreeMap<String, String>, key: &str) -> Result<bool> {
    match query.get(key).map(String::as_str) {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        _ => Err(Error::InvalidInput(format!("{key} must be true or false"))),
    }
}

fn number(query: &BTreeMap<String, String>, key: &str) -> Result<f64> {
    required(query, key)?
        .parse()
        .map_err(|_| Error::InvalidInput(format!("{key} must be a number")))
}

fn coordinate_input(
    query: &BTreeMap<String, String>,
    x: &str,
    y: &str,
    z: &str,
) -> Result<Vec<f64>> {
    let mut result = vec![number(query, x)?, number(query, y)?];
    if query.contains_key(z) {
        result.push(number(query, z)?);
    }
    Ok(result)
}

fn bounded_usize(
    query: &BTreeMap<String, String>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize> {
    let value = query
        .get(key)
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| Error::InvalidInput(format!("{key} must be an integer")))?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(Error::InvalidInput(format!(
            "{key} must be between {min} and {max}"
        )));
    }
    Ok(value)
}

fn parse_query(raw: &str) -> Result<BTreeMap<String, String>> {
    parse_query_bounded(raw, 16384)
}

fn parse_query_bounded(raw: &str, maximum: usize) -> Result<BTreeMap<String, String>> {
    if raw.len() > maximum {
        return Err(Error::InvalidInput(format!(
            "query exceeds {maximum} bytes"
        )));
    }
    let mut result = BTreeMap::new();
    for part in raw.split('&').filter(|part| !part.is_empty()) {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        let key = decode(key)?;
        if result.insert(key.clone(), decode(value)?).is_some() {
            return Err(Error::InvalidInput(format!(
                "duplicate query parameter: {key}"
            )));
        }
    }
    Ok(result)
}

fn decode(value: &str) -> Result<String> {
    let mut bytes = Vec::new();
    let mut index = 0;
    let input = value.as_bytes();
    while index < input.len() {
        match input[index] {
            b'+' => bytes.push(b' '),
            b'%' => {
                let pair = input
                    .get(index + 1..index + 3)
                    .ok_or_else(|| Error::InvalidInput("invalid percent encoding".into()))?;
                let digits = std::str::from_utf8(pair)
                    .map_err(|_| Error::InvalidInput("invalid percent encoding".into()))?;
                bytes.push(
                    u8::from_str_radix(digits, 16)
                        .map_err(|_| Error::InvalidInput("invalid percent encoding".into()))?,
                );
                index += 2;
            }
            other => bytes.push(other),
        }
        index += 1;
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidInput("query is not UTF-8".into()))
}

pub struct ApiError(pub Error);
impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Conflict(_) | Error::Ambiguous(_) => StatusCode::CONFLICT,
            Error::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            Error::InvalidInput(_)
            | Error::Parse { .. }
            | Error::Validation(_)
            | Error::Json(_)
            | Error::Yaml(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self.0.json())).into_response()
    }
}
