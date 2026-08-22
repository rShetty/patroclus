use std::time::Instant;

use axum::{Router, extract::State, http::Request, middleware::Next, response::Response};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::auth::auth_middleware;
use crate::api::state::AppState;
use crate::config::Config;

pub async fn run(config: Config) -> anyhow::Result<()> {
    // Startup guard: release builds refuse to run without an admin token
    // unless PATROCLUS_INSECURE_DEV=1 was set explicitly.
    let auth = crate::api::auth::AuthConfig::from_env();
    if let Err(message) = auth.ensure_startable(cfg!(not(debug_assertions))) {
        eprintln!("patroclus: {message}");
        anyhow::bail!(message);
    }

    let state = AppState::new(config).await?;
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);
    let app = create_router(state);

    tracing::info!("Patroclus starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Middleware recording per-request latency into the Prometheus histogram,
/// labelled by HTTP method and route path template.
async fn metrics_latency_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    // Use the matched route template when available so cardinality stays
    // bounded; fall back to the raw path for unmatched requests.
    let path = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();

    state
        .metrics
        .request_duration
        .with_label_values(&[method.as_str(), &path])
        .observe(elapsed);

    response
}

pub fn create_router(state: AppState) -> Router {
    let routes = super::routes::all_routes();

    let mut router = Router::new();
    for (path, method_router) in routes {
        router = router.route(&path, method_router);
    }

    // CORS: explicit allowlist only. An empty list means no browser
    // cross-origin access at all (server-to-server callers are unaffected).
    let cors = if state.config.server.cors_allowed_origins.is_empty() {
        CorsLayer::new()
    } else {
        let origins: Vec<axum::http::HeaderValue> = state
            .config
            .server
            .cors_allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new().allow_origin(origins)
    };

    router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            metrics_latency_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
