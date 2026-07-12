use axum::{
    body::{Body, Bytes},
    error_handling::HandleErrorLayer,
    extract::{Request, DefaultBodyLimit},
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use http_body_util::BodyExt;

use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    common::{
        app::{
            state::AppState,
            swagger::{create_admin_swagger_ui, create_swagger_ui},
        },
        http::error::{handle_error, AppError},
        security::jwt,
    },
    features::{auth::user_auth_routes, game::game_routes, media::media_routes, user::user_routes},
};

use once_cell::sync::Lazy;
use regex::Regex;

/// List of regex patterns representing disallowed content to block in requests.
/// These patterns are applied to both request bodies and URL query strings.
/// Used to detect and reject potentially dangerous input (e.g., script tags).
pub static FORBIDDEN_PATTERNS: Lazy<Vec<Regex>> =
    Lazy::new(|| vec![Regex::new(r"(?i)<\s*script\b[^>]*>").unwrap()]);

pub fn create_router(state: AppState) -> Router {
    // Build a CORS layer that applies to everyone
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_origin(Any)
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    // Create a common middleware stack for error handling, timeouts, and CORS.
    let middleware_stack = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(handle_error))
        .timeout(Duration::from_secs(1800))
        .layer(cors);

    // /auth routes (login, register, refresh, etc.) — no logging here
    let auth_router = Router::new()
        .nest("/auth", user_auth_routes())
        .layer(middleware::from_fn(make_request_response_inspecter(false)));

    // Protected API routes
    let protected_routes = Router::new()
        .nest("/user", user_routes())
        .nest("/media", media_routes())
        .nest("/games", game_routes())
        // enforce JWT authentication
        .route_layer(middleware::from_fn_with_state(state.clone(), jwt::jwt_auth))
        // attach inspecter
        .layer(middleware::from_fn(make_request_response_inspecter(true)));

    // Create the API routes router that we want to trace and assign request IDs to
    let api_routes = Router::new()
        .merge(auth_router)
        .merge(protected_routes)
        .merge(create_swagger_ui())
        .merge(create_admin_swagger_ui())
        .layer(middleware::from_fn(request_id_middleware))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<_>| {
                    let user_agent = req
                        .headers()
                        .get(axum::http::header::USER_AGENT)
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("");
                    let client_ip = req
                        .headers()
                        .get("x-forwarded-for")
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.split(',').next())
                        .map(|s| s.trim())
                        .unwrap_or("");

                    tracing::info_span!(
                        "request",
                        method = %req.method(),
                        uri = %req.uri().path(),
                        user_agent = %user_agent,
                        client_ip = %client_ip,
                        status_code = tracing::field::Empty,
                        error = tracing::field::Empty,
                        request_id = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     _latency: std::time::Duration,
                     span: &tracing::Span| {
                        let status_code = response.status().as_u16();
                        span.record("status_code", status_code);
                        if status_code >= 400 {
                            span.record("error", true);
                        }
                    },
                ),
        );

    // Create the main router
    Router::new()
        .route("/health", axum::routing::get(health_check))
        .merge(api_routes)
        .fallback(fallback)
        .layer(middleware_stack)
        .layer(DefaultBodyLimit::max((state.config.max_file_size_mb + 25) as usize * 1024 * 1024))
        .with_state(state)
}

async fn health_check() -> &'static str {
    "OK\n"
}

/// Fallback handler for unmatched routes
pub async fn fallback() -> Result<impl IntoResponse, AppError> {
    Ok((StatusCode::NOT_FOUND, "Not Found ;("))
}

type InspectorFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Response, (StatusCode, String)>> + Send>,
>;

fn make_request_response_inspecter(
    log_enabled: bool,
) -> impl Fn(Request<Body>, Next) -> InspectorFuture + Clone + Send + Sync + 'static {
    move |req, next| {
        let fut = request_response_inspecter(req, next, log_enabled);
        Box::pin(fut)
    }
}

async fn request_response_inspecter(
    req: Request<Body>,
    next: Next,
    log_enabled: bool,
) -> Result<Response, (StatusCode, String)> {
    if let Some(query) = req.uri().query() {
        if FORBIDDEN_PATTERNS.iter().any(|re| re.is_match(query)) {
            return Err((StatusCode::FORBIDDEN, "Forbidden Request".to_string()));
        }
    }

    let (parts, body) = req.into_parts();
    let bytes = request_inspect_print("request", log_enabled, body).await?;
    let req = Request::from_parts(parts, Body::from(bytes));

    let mut res = next.run(req).await;
    if log_enabled && tracing::enabled!(tracing::Level::DEBUG) {
        let (parts, body) = res.into_parts();
        let bytes = response_print("response", body).await?;
        res = Response::from_parts(parts, Body::from(bytes));
    }

    Ok(res)
}

async fn request_inspect_print<B>(
    direction: &str,
    log_enabled: bool,
    body: B,
) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("failed to read {direction} body: {err}"),
            ));
        }
    };

    if let Ok(body_str) = std::str::from_utf8(&bytes) {
        if log_enabled {
            tracing::info!("{} body = {:?}", direction, body_str);
        }

        if FORBIDDEN_PATTERNS.iter().any(|re| re.is_match(body_str)) {
            return Err((StatusCode::FORBIDDEN, "Forbidden Request".to_string()));
        }
    }

    Ok(bytes)
}

async fn response_print<B>(direction: &str, body: B) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("failed to read {direction} body: {err}"),
            ));
        }
    };

    if let Ok(body_str) = std::str::from_utf8(&bytes) {
        tracing::debug!("{} body = {:?}", direction, body_str);
    }

    Ok(bytes)
}

/// Middleware that generates a task-local request ID, records it on the request tracing span,
/// and adds it to the HTTP response headers.
async fn request_id_middleware(req: Request<Body>, next: Next) -> Response {
    let generated_uuid = uuid::Uuid::new_v4().to_string();
    let start_time = std::time::Instant::now();

    // Extract request info for logging later
    let method = req.method().clone();
    let uri = req.uri().path().to_string();
    let user_agent = req
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .unwrap_or("")
        .to_string();

    // Bind the generated UUID to the task-local variable
    let (mut res, final_request_id) = crate::common::http::dto::REQUEST_ID
        .scope(generated_uuid.clone(), async move {
            // Proceed with the request handler chain
            let res = next.run(req).await;

            // Grab the final request ID (either the OTel trace ID or the fallback UUID)
            let final_request_id = crate::common::http::dto::get_current_request_id();

            // Record it on the active tracing span
            tracing::Span::current().record("request_id", &final_request_id);

            (res, final_request_id)
        })
        .await;

    let latency = start_time.elapsed();
    let status_code = res.status().as_u16();

    // Log the request completion with all fields explicitly on the event.
    // This allows OTel tracing bridge to attach them as attributes to the log record,
    // which makes them available as structured metadata in Loki.
    tracing::info!(
        status = status_code,
        latency_ms = latency.as_secs_f64() * 1000.0,
        method = %method,
        uri = %uri,
        user_agent = %user_agent,
        client_ip = %client_ip,
        request_id = %final_request_id,
        "request completed: status = {status}, latency = {latency:?}, user_agent = \"{user_agent}\"",
        status = status_code,
        latency = latency,
        user_agent = user_agent
    );

    // Inject the request ID into the response headers
    if let Ok(hdr_val) = axum::http::HeaderValue::from_str(&final_request_id) {
        res.headers_mut()
            .insert(axum::http::HeaderName::from_static("x-request-id"), hdr_val);
    }

    res
}
