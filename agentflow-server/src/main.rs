use agentflow_db::Database;
use agentflow_server::{
  AppState, SkillCatalog, create_router, resolve_auth_config_from_env,
  server_security_defaults_from_env,
};
use agentflow_tools::{SECURITY_PROFILE_ENV, SecurityProfile};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::registry()
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
    .with(tracing_subscriber::fmt::layer())
    .init();

  let _ = dotenvy::dotenv();

  let db_url = std::env::var("DATABASE_URL")
    .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/agentflow".to_string());

  info!("Initializing database connection and applying migrations…");
  let db = match Database::connect_and_migrate(&db_url, 8).await {
    Ok(d) => d,
    Err(e) => {
      error!("Failed to connect to database: {}", e);
      return Err(e.into());
    }
  };

  let security_profile = match SecurityProfile::from_env() {
    Ok(profile) => profile,
    Err(err) => {
      error!("Invalid {SECURITY_PROFILE_ENV}: {err}");
      return Err(err.into());
    }
  };
  let security_defaults = match server_security_defaults_from_env(security_profile) {
    Ok(defaults) => defaults,
    Err(err) => {
      error!("{err}");
      return Err(err.into());
    }
  };
  info!("Using '{}' security profile", security_profile);

  let auth = match resolve_auth_config_from_env(security_profile) {
    Ok(auth) => auth,
    Err(err) => {
      error!("{err}");
      return Err(err.into());
    }
  };
  if auth.is_none() && !security_defaults.auth.require_api_token {
    warn!(
      "AGENTFLOW_API_TOKEN is not set; the gateway is running without bearer auth. \
       Set AGENTFLOW_API_TOKEN before exposing this server outside trusted networks."
    );
  }

  let state = AppState::new(db)
    .with_security_defaults(security_defaults)
    .with_auth(auth)
    .with_skills(SkillCatalog::from_env());
  let app = create_router(state);

  let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
  let addr = format!("0.0.0.0:{}", port);

  info!("Starting AgentFlow Gateway on {}", addr);
  let listener = tokio::net::TcpListener::bind(&addr).await?;

  axum::serve(listener, app).await?;

  Ok(())
}
