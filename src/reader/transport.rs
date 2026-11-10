//! Dedicated reader-only loopback origin. No author router, source discovery,
//! private assets, policy installation endpoint or session/token route is mounted.
use super::{browser::BrowserRequest, *};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};

const INDEX: &str = include_str!("../../static/reader.html");
const SCRIPT: &str = include_str!("../../static/reader.js");
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

fn usage() -> Response {
    (StatusCode::BAD_REQUEST, Json(invalid_request().json())).into_response()
}
fn unavailable_response() -> Response {
    (
        StatusCode::CONFLICT,
        Json(failure(Mode::Reader, Failure::Unavailable)),
    )
        .into_response()
}
fn result_response(value: serde_json::Value) -> Response {
    let status = match value["outcome"].as_str() {
        Some("available") => StatusCode::OK,
        Some("unavailable") => StatusCode::CONFLICT,
        Some("invalid") => StatusCode::BAD_REQUEST,
        Some("limit") => StatusCode::UNPROCESSABLE_ENTITY,
        _ => return unavailable_response(),
    };
    (status, Json(value)).into_response()
}
fn route_failure(status: StatusCode) -> Response {
    (
        status,
        Json(
            json!({"code":if status==StatusCode::NOT_FOUND {"not_found"}else{"method_not_allowed"},
        "message":"Reader route unavailable.","details":{}}),
        ),
    )
        .into_response()
}

pub fn router(service: PresentationService) -> Router {
    Router::new()
        .route("/", get(|| async { Html(INDEX) }))
        .route(
            "/reader.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    SCRIPT,
                )
            }),
        )
        .route("/api/reader-presentation/page", post(page))
        .route("/api/reader-presentation/search", post(search))
        .route("/api/reader-presentation/choices", get(choices))
        .route("/api/reader-presentation/read", post(read_position))
        .route("/api/reader-presentation/find", post(find_revealed))
        .fallback(|| async { route_failure(StatusCode::NOT_FOUND) })
        .method_not_allowed_fallback(|| async { route_failure(StatusCode::METHOD_NOT_ALLOWED) })
        .layer(middleware::from_fn(local_request))
        .with_state(service)
}
async fn local_request(request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let allowed_host = request.headers().get_all(header::HOST).iter().count() == 1
        && host
            .and_then(|value| value.parse::<axum::http::uri::Authority>().ok())
            .is_some_and(|value| {
                if !matches!(value.host(), "127.0.0.1" | "localhost" | "[::1]") {
                    return false;
                }
                let suffix = value
                    .as_str()
                    .strip_prefix(value.host())
                    .unwrap_or("invalid");
                suffix.is_empty()
                    || suffix.strip_prefix(':').is_some_and(|port| {
                        !port.is_empty()
                            && port.bytes().all(|byte| byte.is_ascii_digit())
                            && port.parse::<u16>().is_ok()
                    })
            });
    let allowed_origin = request.headers().get_all(header::ORIGIN).iter().count() <= 1
        && request.headers().get(header::ORIGIN).is_none_or(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<axum::http::Uri>().ok())
                .is_some_and(|value| {
                    value.scheme_str() == Some("http")
                        && value.authority().map(|value| value.as_str()) == host
                        && value.path() == "/"
                        && value.query().is_none()
                })
        });
    let mut response = if allowed_host && allowed_origin {
        next.run(request).await
    } else {
        (StatusCode::FORBIDDEN,Json(json!({"code":"forbidden_origin","message":"Reader requests require this loopback origin.","details":{}}))).into_response()
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("content-security-policy", HeaderValue::from_static(CSP));
    response
}
async fn page(State(service): State<PresentationService>, request: Request) -> Response {
    read(service, request, Operation::Page).await
}
async fn search(State(service): State<PresentationService>, request: Request) -> Response {
    read(service, request, Operation::Search).await
}
async fn request_json(request: Request) -> Result<axum::body::Bytes> {
    if request.uri().query().is_some() {
        return Err(invalid_request());
    }
    let json = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        });
    if !json {
        return Err(invalid_request());
    }
    to_bytes(request.into_body(), MAX_REQUEST_BYTES)
        .await
        .map_err(|_| invalid_request())
}
async fn read(service: PresentationService, request: Request, operation: Operation) -> Response {
    let raw = match request_json(request).await {
        Ok(raw) => raw,
        Err(_) => return usage(),
    };
    let worker = service.clone();
    let prepared =
        tokio::task::spawn_blocking(move || worker.prepare(&raw, operation, Mode::Reader)).await;
    match prepared {
        Ok(Ok(prepared)) => result_response(service.emit(prepared)),
        Ok(Err(_)) => usage(),
        Err(_) => unavailable_response(),
    }
}
async fn choices(State(service): State<PresentationService>, request: Request) -> Response {
    if request.uri().query().is_some() || to_bytes(request.into_body(), 0).await.is_err() {
        return usage();
    }
    browser_read(service, BrowserRequest::Choices).await
}
async fn read_position(State(service): State<PresentationService>, request: Request) -> Response {
    browser_position(service, request, Operation::Page).await
}
async fn find_revealed(State(service): State<PresentationService>, request: Request) -> Response {
    browser_position(service, request, Operation::Search).await
}
async fn browser_position(
    service: PresentationService,
    request: Request,
    operation: Operation,
) -> Response {
    let request = match request_json(request)
        .await
        .and_then(|raw| BrowserRequest::reading(&raw, operation))
    {
        Ok(request) => request,
        Err(_) => return usage(),
    };
    browser_read(service, request).await
}
async fn browser_read(service: PresentationService, request: BrowserRequest) -> Response {
    let worker = service.clone();
    match tokio::task::spawn_blocking(move || worker.prepare_browser(request)).await {
        Ok(prepared) => result_response(service.emit(prepared)),
        Err(_) => unavailable_response(),
    }
}

/// Serving this isolated loopback origin never initializes an author world or
/// creates an implicit policy. Installed policy replacement is a native API.
pub async fn serve(service: PresentationService, host: &str, port: u16) -> Result<()> {
    let address = crate::http::loopback_address(host, port)?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!(
        "Ready: http://{} — isolated reader presentation (Ctrl+C to stop)",
        listener.local_addr()?
    );
    axum::serve(listener, router(service))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn request() -> String {
        json!({"protocol":PROTOCOL,"publicationId":"publication","revision":"a".repeat(40),"occurrence":0,"limit":10,"cursor":null}).to_string()
    }
    async fn send(
        host: Option<&str>,
        origin: Option<&str>,
        path: &str,
        method: &str,
        body: String,
    ) -> Response {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(host) = host {
            request = request.header(header::HOST, host);
        }
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        router(PresentationService::default())
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap()
    }
    fn security_headers(response: &Response) {
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::VARY], "Origin");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["x-frame-options"], "DENY");
        assert_eq!(response.headers()["content-security-policy"], CSP);
        assert!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }
    #[tokio::test]
    async fn loopback_host_and_exact_origin_are_required_even_on_error_or_asset_routes() {
        for (host, origin) in [
            (None, None),
            (Some("attacker.invalid"), None),
            (Some("127.0.0.1.evil"), None),
            (Some("localhost:"), None),
            (Some("localhost:bad"), None),
            (Some("localhost:999999"), None),
            (Some("user@localhost:8766"), None),
            (Some("localhost:8766"), Some("http://localhost:8765")),
            (Some("localhost:8766"), Some("https://localhost:8766")),
            (Some("localhost:8766"), Some("null")),
            (Some("localhost:8766"), Some("http://localhost:8766/path")),
        ] {
            for path in [
                "/",
                "/reader.js",
                "/api/reader-presentation/page",
                "/api/session",
            ] {
                let response = send(host, origin, path, "POST", request()).await;
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "host={host:?}, origin={origin:?}"
                );
                security_headers(&response);
            }
        }
        for host in [
            "localhost",
            "localhost:8766",
            "127.0.0.1:8766",
            "[::1]:8766",
        ] {
            let response = send(
                Some(host),
                Some(&format!("http://{host}")),
                "/api/reader-presentation/page",
                "POST",
                request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CONFLICT, "{host}");
            security_headers(&response);
        }
    }
    #[tokio::test]
    async fn duplicate_origin_or_host_headers_are_refused() {
        for duplicate in [header::HOST, header::ORIGIN] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/reader-presentation/page")
                .header(header::HOST, "localhost:8766")
                .header(header::ORIGIN, "http://localhost:8766")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(request()))
                .unwrap();
            let value = request.headers()[&duplicate].clone();
            request.headers_mut().append(duplicate, value);
            let response = router(PresentationService::default())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            security_headers(&response);
        }
    }
    #[tokio::test]
    async fn reader_has_no_author_route_session_grant_or_preview_surface() {
        for path in [
            "/api/session",
            "/api/status",
            "/api/entities",
            "/api/search",
            "/api/reader-presentation/preview",
            "/api/manuscripts/list",
            "/api/reveal-plans/resolve",
            "/api/policy",
            "/author.js",
            "/index.html",
        ] {
            for method in ["GET", "POST"] {
                let response = send(Some("localhost:8766"), None, path, method, request()).await;
                assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
                security_headers(&response);
            }
        }
        let response = send(
            Some("localhost:8766"),
            None,
            "/api/reader-presentation/page",
            "GET",
            String::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        security_headers(&response);
    }
    #[tokio::test]
    async fn malformed_transport_is_generic_and_request_body_is_bounded() {
        let reference = invalid_request().json();
        for (path, body) in [
            ("/api/reader-presentation/page?", request()),
            ("/api/reader-presentation/page?grant=all", request()),
            (
                "/api/reader-presentation/page",
                " ".repeat(MAX_REQUEST_BYTES + 1),
            ),
            ("/api/reader-presentation/page", "{".into()),
        ] {
            let response = send(Some("localhost:8766"), None, path, "POST", body).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            security_headers(&response);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                reference
            );
        }
        let request = Request::builder()
            .method("POST")
            .uri("/api/reader-presentation/page")
            .header(header::HOST, "localhost:8766")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(request()))
            .unwrap();
        let response = router(PresentationService::default())
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        security_headers(&response);
    }
    #[tokio::test]
    async fn default_reader_page_cannot_be_granted_by_headers_or_cookies() {
        let request = Request::builder()
            .method("POST")
            .uri("/api/reader-presentation/page")
            .header(header::HOST, "localhost:8766")
            .header(header::CONTENT_TYPE, "application/json")
            .header("authorization", "Bearer fabricated-author-token")
            .header("cookie", "readerId=author; scope=all")
            .header("x-reader-grant", "all")
            .body(Body::from(request()))
            .unwrap();
        let response = router(PresentationService::default())
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        security_headers(&response);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            failure(Mode::Reader, Failure::Unavailable)
        );
    }

    #[tokio::test]
    async fn browser_routes_require_public_policy_and_closed_loopback_requests() {
        let host = Some("localhost:8766");
        let choices = "/api/reader-presentation/choices";
        let read = "/api/reader-presentation/read";
        let find = "/api/reader-presentation/find";
        let raw =
            json!({"edition":"a".repeat(64),"position":"b".repeat(64),"cursor":null}).to_string();
        let query = json!({"edition":"a".repeat(64),"position":"b".repeat(64),"cursor":null,"query":"story"}).to_string();
        for (path, method, body) in [
            (choices, "GET", String::new()),
            (read, "POST", raw.clone()),
            (read, "POST", r#"{"cursor":null}"#.into()),
            (find, "POST", query),
        ] {
            let response = send(host, None, path, method, body).await;
            assert_eq!(response.status(), StatusCode::CONFLICT);
            security_headers(&response);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                failure(Mode::Reader, Failure::Unavailable)
            );
        }
        for (path, method, body) in [
            (
                "/api/reader-presentation/choices?grant=all",
                "GET",
                String::new(),
            ),
            (choices, "GET", "x".into()),
            (read, "POST", request()),
            (find, "POST", raw.clone()),
            (read, "POST", " ".repeat(MAX_REQUEST_BYTES + 1)),
        ] {
            let response = send(host, None, path, method, body).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            security_headers(&response);
        }
        let response = send(host, Some("http://localhost:8765"), read, "POST", raw).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        security_headers(&response);
    }
}
