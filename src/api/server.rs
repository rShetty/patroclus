use std::sync::Arc;

use axum::{
    routing::{get, post},
    Json, Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::state::AppState;
use crate::config::Config;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let state = AppState::new(config.clone()).await?;
    let app = create_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Patroclus starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_router(state: AppState) -> Router {
    let routes = super::routes::all_routes();

    let mut router = Router::new();
    for (path, method_router) in routes {
        router = router.route(&path, method_router);
    }

    router
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
