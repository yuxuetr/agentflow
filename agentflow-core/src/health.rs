use crate::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

type HealthCheckFuture = Pin<Box<dyn Future<Output = Result<HealthStatus>> + Send>>;
type HealthCheckFn = Arc<dyn Fn() -> HealthCheckFuture + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
  Healthy,
  Degraded,
  Unhealthy,
}

#[derive(Debug, Clone)]
pub struct HealthCheckResult {
  pub name: String,
  pub status: HealthStatus,
  pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HealthReport {
  pub is_healthy: bool,
  pub checks: Vec<HealthCheckResult>,
}

#[derive(Clone, Default)]
pub struct HealthChecker {
  checks: Arc<RwLock<Vec<(String, HealthCheckFn)>>>,
}

impl HealthChecker {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn add_check<F>(&self, name: impl Into<String>, check: F)
  where
    F: Fn() -> HealthCheckFuture + Send + Sync + 'static,
  {
    self
      .checks
      .write()
      .await
      .push((name.into(), Arc::new(check)));
  }

  pub async fn check_health(&self) -> HealthReport {
    let checks = self.checks.read().await.clone();
    let mut results = Vec::with_capacity(checks.len());

    for (name, check) in checks {
      match check().await {
        Ok(status) => results.push(HealthCheckResult {
          name,
          status,
          message: None,
        }),
        Err(error) => results.push(HealthCheckResult {
          name,
          status: HealthStatus::Unhealthy,
          message: Some(error.to_string()),
        }),
      }
    }

    let is_healthy = results
      .iter()
      .all(|result| !matches!(result.status, HealthStatus::Unhealthy));

    HealthReport {
      is_healthy,
      checks: results,
    }
  }
}
