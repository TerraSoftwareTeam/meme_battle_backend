use axum::{
    body::{Body, Bytes},
    error_handling::HandleErrorLayer,
    extract::Request,
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
    features::{
        auth::user_auth_routes,
        media::media_routes,
        user::user_routes,
    },
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
        // enforce JWT authentication
        .route_layer(middleware::from_fn(jwt::jwt_auth))
        // attach inspecter
        .layer(middleware::from_fn(make_request_response_inspecter(true)));

    // Create the main router
    Router::new()
        .route("/health", axum::routing::get(health_check))
        .merge(auth_router)
        .merge(protected_routes)
        .merge(create_swagger_ui())
        .merge(create_admin_swagger_ui())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "request",
                        method = %req.method(),
                        uri = %req.uri(),
                    )
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     _span: &tracing::Span| {
                        tracing::info!(
                            "request completed: status = {status}, latency = {latency:?}",
                            status = response.status(),
                            latency = latency
                        );
                     },
                ),
        )
        .fallback(fallback)
        .layer(middleware_stack)
        .with_state(state)
}

async fn health_check() -> &'static str {
    "OK\n"
}

/// Fallback handler for unmatched routes
pub async fn fallback() -> Result<impl IntoResponse, AppError> {
    Ok((StatusCode::NOT_FOUND, "Not Found"))
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
