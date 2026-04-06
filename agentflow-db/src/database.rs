use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

use crate::error::DbError;

/// Database connection manager.
#[derive(Clone, Debug)]
pub struct Database {
    pub pool: PgPool,
}

impl Database {
    /// Initialize a new PostgreSQL connection pool.
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, DbError> {
        info!("Connecting to database...");
        
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(3))
            .connect(database_url)
            .await?;

        info!("Successfully connected to database.");
        
        Ok(Self { pool })
    }
}
