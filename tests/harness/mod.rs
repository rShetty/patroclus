use axum::Router;
use patroclus::api::server::create_router;
use patroclus::api::state::AppState;
use patroclus::config::Config;

pub struct TestServer {
    pub app: Router,
    pub state: AppState,
}

impl TestServer {
    pub fn new() -> anyhow::Result<Self> {
        let state = AppState::new_test()?;
        let app = create_router(state.clone());
        Ok(TestServer { app, state })
    }

    pub fn new_with_policy(yaml: &str) -> anyhow::Result<Self> {
        let state = AppState::new_test()?;
        state.db.create_policy("test", "yaml", yaml)?;
        let yaml = state.db.load_active_policy_yaml()?;
        let engine = patroclus::policy::create_engine("yaml", Some(&yaml))?;
        let state = AppState {
            db: state.db.clone(),
            config: state.config.clone(),
            policy_engine: std::sync::Arc::from(engine),
            token_issuer: state.token_issuer.clone(),
            token_verifier: state.token_verifier.clone(),
        };
        let app = create_router(state.clone());
        Ok(TestServer { app, state })
    }
}
