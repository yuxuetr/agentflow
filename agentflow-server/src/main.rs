use agentflow_db::Database;
use agentflow_server::{AppState, create_router};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize tracing
  tracing_subscriber::registry()
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
    .with(tracing_subscriber::fmt::layer())
    .init();

  // Load .env if present
  let _ = dotenvy::dotenv();

  // Setup database connection
  let db_url = std::env::var("DATABASE_URL")
    .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/agentflow".to_string());

  info!("Initializing database connection...");

  // Using a simple connect that would error gracefully via the custom ApiError later
  // but in main() we bubble up to Box<dyn Error>.
  let db = match Database::connect(&db_url, 5).await {
    Ok(d) => d,
    Err(e) => {
      error!("Failed to connect to database: {}", e);
      return Err(e.into());
    }
  };

  let state = AppState { db };
  let app = create_router(state);

  let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
  let addr = format!("0.0.0.0:{}", port);

  info!("Starting AgentFlow Gateway on {}", addr);
  let listener = tokio::net::TcpListener::bind(&addr).await?;

  axum::serve(listener, app).await?;

  Ok(())
}
