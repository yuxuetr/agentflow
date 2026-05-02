use agentflow_db::Database;
use agentflow_server::{AppState, AuthConfig, create_router};
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

  let auth = AuthConfig::from_env();
  if auth.is_none() {
    warn!(
      "AGENTFLOW_API_TOKEN is not set; the gateway is running without bearer auth. \
       Set AGENTFLOW_API_TOKEN before exposing this server outside trusted networks."
    );
  }

  let state = AppState::new(db).with_auth(auth);
  let app = create_router(state);

  let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
  let addr = format!("0.0.0.0:{}", port);

  info!("Starting AgentFlow Gateway on {}", addr);
  let listener = tokio::net::TcpListener::bind(&addr).await?;

  axum::serve(listener, app).await?;

  Ok(())
}
