use axum::Router;
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

pub fn create_router(state: AppState) -> Router {
    let routes = super::routes::all_routes();

    let mut router = Router::new();
    for (path, method_router) in routes {
        router = router.route(&path, method_router);
    }

    router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
