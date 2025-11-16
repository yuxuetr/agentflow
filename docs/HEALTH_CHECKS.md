# Health Check System

**Since:** v0.2.0+
**Status:** Production-Ready
**Performance:** < 1ms per single check, < 10ms for multiple checks

## Overview

The Health Check System provides comprehensive health and readiness monitoring for AgentFlow applications, compatible with Kubernetes liveness and readiness probes and other orchestration platforms.

## Features

- **Kubernetes Compatible**: Works with liveness and readiness probes
- **Custom Health Checks**: Add domain-specific health checks
- **Built-in Checks**: Pre-configured checks for common components
- **Async Support**: All health checks are async-first
- **Lightweight**: < 1ms overhead per check
- **Rich Metadata**: Include custom metrics and diagnostics

## Quick Start

### Basic Usage

```rust
use agentflow_core::health::{HealthChecker, HealthStatus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let checker = HealthChecker::new();

    // Add a simple health check
    checker.add_check("api", || {
        Box::pin(async {
            // Check if API is responding
            match check_api_connection().await {
                Ok(_) => Ok(HealthStatus::Healthy),
                Err(_) => Err("API connection failed".to_string()),
            }
        })
    }).await;

    // Perform health check
    let report = checker.check_health().await;

    if report.is_healthy {
        println!("System is healthy!");
    } else {
        println!("System is unhealthy: {:?}", report);
    }

    Ok(())
}
```

### With Built-in Checks

```rust
use agentflow_core::health::HealthChecker;

let checker = HealthChecker::new();

// Add built-in memory health check
checker.add_memory_check(0.9).await; // Warn if > 90% memory used

// Add built-in metrics health check
checker.add_metrics_check().await;

// Check overall health
let report = checker.check_health().await;
println!("Health status: {}", report.status());
```

## Health Status Types

The health check system supports three status levels:

### HealthStatus::Healthy

The component is operating normally with no issues.

```rust
Ok(HealthStatus::Healthy)
```

### HealthStatus::Degraded

The component is functional but experiencing issues that may affect performance.

```rust
Ok(HealthStatus::Degraded)
```

### HealthStatus::Unhealthy

The component is not functioning properly.

```rust
Err("Database connection lost".to_string())
```

## API Reference

### HealthChecker

Main interface for managing health checks.

#### Constructor

```rust
pub fn new() -> Self
```

Creates a new health checker instance.

**Example:**
```rust
let checker = HealthChecker::new();
```

#### Adding Checks

```rust
pub async fn add_check<F>(&self, name: impl Into<String>, check: F)
where
    F: Fn() -> Pin<Box<dyn Future<Output = Result<HealthStatus, String>> + Send>>
       + Send + Sync + 'static,
```

Adds a custom health check.

**Parameters:**
- `name`: Unique identifier for the health check
- `check`: Async function that returns health status

**Example:**
```rust
checker.add_check("database", || {
    Box::pin(async {
        match db_pool.get_conn().await {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(e) => Err(format!("DB connection failed: {}", e)),
        }
    })
}).await;
```

#### Built-in Checks

```rust
pub async fn add_memory_check(&self, threshold: f64)
pub async fn add_metrics_check(&self)
```

Add pre-configured health checks for common scenarios.

**Example:**
```rust
// Alert if memory usage > 85%
checker.add_memory_check(0.85).await;

// Check metrics collection system
checker.add_metrics_check().await;
```

#### Checking Health

```rust
pub async fn check_health(&self) -> HealthReport
```

Performs all registered health checks and returns a comprehensive report.

**Returns:** `HealthReport` containing:
- Overall health status
- Individual check results
- System metadata
- Timestamp

**Example:**
```rust
let report = checker.check_health().await;

if report.is_healthy {
    println!("All systems operational");
} else {
    for result in report.checks {
        if !result.status.is_healthy() {
            println!("Unhealthy: {} - {:?}", result.name, result.message);
        }
    }
}
```

#### Managing Checks

```rust
pub async fn remove_check(&self, name: &str) -> bool
pub async fn set_metadata(&self, key: impl Into<String>, value: impl Into<String>)
```

Remove health checks or add system metadata.

**Example:**
```rust
// Remove a check
if checker.remove_check("old_api").await {
    println!("Check removed");
}

// Add metadata
checker.set_metadata("version", "0.2.0").await;
checker.set_metadata("environment", "production").await;
```

### HealthReport

Result of a health check operation.

```rust
pub struct HealthReport {
    pub is_healthy: bool,
    pub checks: Vec<HealthCheckResult>,
    pub metadata: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
}
```

#### Methods

```rust
pub fn status(&self) -> &'static str
```

Returns a string representation of overall health status:
- `"healthy"` - All checks passed
- `"degraded"` - Some checks degraded but functional
- `"unhealthy"` - One or more checks failed

**Example:**
```rust
let report = checker.check_health().await;
println!("Status: {}", report.status());
```

### HealthCheckResult

Individual health check result.

```rust
pub struct HealthCheckResult {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub metrics: HashMap<String, String>,
}
```

#### Constructors

```rust
pub fn healthy(name: impl Into<String>) -> Self
pub fn degraded(name: impl Into<String>, message: impl Into<String>) -> Self
pub fn unhealthy(name: impl Into<String>, message: impl Into<String>) -> Self
```

**Example:**
```rust
// Creating results manually (usually done automatically)
let result = HealthCheckResult::healthy("api");
let result = HealthCheckResult::degraded("cache", "High latency detected");
let result = HealthCheckResult::unhealthy("database", "Connection timeout");
```

#### Adding Metrics

```rust
pub fn with_metric(mut self, key: impl Into<String>, value: impl Into<String>) -> Self
```

Attach custom metrics to health check results.

**Example:**
```rust
let result = HealthCheckResult::healthy("api")
    .with_metric("response_time_ms", "45")
    .with_metric("requests_per_second", "1250");
```

## Integration Patterns

### Kubernetes Liveness Probe

Kubernetes uses liveness probes to determine if a container should be restarted.

**HTTP Endpoint Example:**

```rust
use axum::{Router, routing::get, Json};
use agentflow_core::health::HealthChecker;

async fn liveness_handler(
    checker: Arc<HealthChecker>
) -> (StatusCode, Json<HealthReport>) {
    let report = checker.check_health().await;

    let status = if report.is_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(report))
}

#[tokio::main]
async fn main() {
    let checker = Arc::new(HealthChecker::new());

    // Add health checks
    checker.add_memory_check(0.9).await;
    checker.add_metrics_check().await;

    let app = Router::new()
        .route("/health/live", get({
            let checker = checker.clone();
            move || liveness_handler(checker.clone())
        }));

    // Serve on 0.0.0.0:8080
    axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
```

**Kubernetes Deployment:**

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: agentflow-app
spec:
  containers:
  - name: agentflow
    image: agentflow:latest
    livenessProbe:
      httpGet:
        path: /health/live
        port: 8080
      initialDelaySeconds: 30
      periodSeconds: 10
      timeoutSeconds: 5
      failureThreshold: 3
```

### Kubernetes Readiness Probe

Readiness probes determine if a container is ready to accept traffic.

```rust
async fn readiness_handler(
    checker: Arc<HealthChecker>
) -> (StatusCode, Json<HealthReport>) {
    let report = checker.check_health().await;

    // Readiness requires all checks to be at least functional
    let is_ready = report.checks.iter()
        .all(|check| check.status.is_functional());

    let status = if is_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(report))
}
```

**Kubernetes Configuration:**

```yaml
readinessProbe:
  httpGet:
    path: /health/ready
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 5
  timeoutSeconds: 3
  failureThreshold: 3
```

### Startup Probe

For applications with slow startup times.

```yaml
startupProbe:
  httpGet:
    path: /health/startup
    port: 8080
  initialDelaySeconds: 0
  periodSeconds: 10
  timeoutSeconds: 3
  failureThreshold: 30  # 5 minutes total
```

## Custom Health Checks

### Database Health Check

```rust
use sqlx::PgPool;
use agentflow_core::health::{HealthChecker, HealthStatus};

async fn setup_database_health(checker: &HealthChecker, pool: Arc<PgPool>) {
    let pool_clone = pool.clone();

    checker.add_check("database", move || {
        let pool = pool_clone.clone();
        Box::pin(async move {
            match sqlx::query("SELECT 1").fetch_one(pool.as_ref()).await {
                Ok(_) => {
                    let size = pool.size();
                    let idle = pool.num_idle();

                    if idle < size / 10 {
                        Ok(HealthStatus::Degraded)
                    } else {
                        Ok(HealthStatus::Healthy)
                    }
                }
                Err(e) => Err(format!("Database query failed: {}", e)),
            }
        })
    }).await;
}
```

### Redis Cache Health Check

```rust
use redis::Client;

async fn setup_redis_health(checker: &HealthChecker, redis_url: String) {
    checker.add_check("redis", move || {
        let url = redis_url.clone();
        Box::pin(async move {
            let client = Client::open(url.as_str())
                .map_err(|e| format!("Redis connection error: {}", e))?;

            let mut conn = client.get_async_connection().await
                .map_err(|e| format!("Redis connection error: {}", e))?;

            redis::cmd("PING").query_async(&mut conn).await
                .map_err(|e| format!("Redis ping failed: {}", e))?;

            Ok(HealthStatus::Healthy)
        })
    }).await;
}
```

### External API Health Check

```rust
use reqwest::Client;

async fn setup_api_health(checker: &HealthChecker, api_url: String) {
    let client = Arc::new(Client::new());

    checker.add_check("external_api", move || {
        let client = client.clone();
        let url = api_url.clone();

        Box::pin(async move {
            let start = std::time::Instant::now();

            let response = client.get(&url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .map_err(|e| format!("API request failed: {}", e))?;

            let latency = start.elapsed();

            if response.status().is_success() {
                // Degraded if latency > 2s
                if latency > Duration::from_secs(2) {
                    Ok(HealthStatus::Degraded)
                } else {
                    Ok(HealthStatus::Healthy)
                }
            } else {
                Err(format!("API returned status: {}", response.status()))
            }
        })
    }).await;
}
```

### Disk Space Health Check

```rust
use sysinfo::{System, SystemExt, DiskExt};

async fn setup_disk_health(checker: &HealthChecker, threshold: f64) {
    checker.add_check("disk_space", move || {
        Box::pin(async move {
            let mut sys = System::new_all();
            sys.refresh_disks_list();

            for disk in sys.disks() {
                let total = disk.total_space();
                let available = disk.available_space();
                let used_percent = 1.0 - (available as f64 / total as f64);

                if used_percent > threshold {
                    return Err(format!(
                        "Disk space critical: {:.1}% used",
                        used_percent * 100.0
                    ));
                } else if used_percent > threshold * 0.9 {
                    return Ok(HealthStatus::Degraded);
                }
            }

            Ok(HealthStatus::Healthy)
        })
    }).await;
}
```

## Best Practices

### 1. Keep Checks Fast

Health checks should complete quickly (< 1 second).

```rust
// ❌ Slow health check
checker.add_check("slow", || {
    Box::pin(async {
        tokio::time::sleep(Duration::from_secs(10)).await; // Too slow!
        Ok(HealthStatus::Healthy)
    })
}).await;

// ✅ Fast health check
checker.add_check("fast", || {
    Box::pin(async {
        // Quick connection test
        db.ping().await?;
        Ok(HealthStatus::Healthy)
    })
}).await;
```

### 2. Use Appropriate Thresholds

```rust
// ❌ Too sensitive
checker.add_memory_check(0.5).await; // 50% is too low

// ✅ Reasonable threshold
checker.add_memory_check(0.85).await; // 85% gives enough headroom
```

### 3. Include Relevant Metrics

```rust
checker.add_check("api", || {
    Box::pin(async {
        let start = Instant::now();
        let response = api_call().await?;
        let latency = start.elapsed();

        let result = HealthCheckResult::healthy("api")
            .with_metric("latency_ms", latency.as_millis().to_string())
            .with_metric("status_code", response.status().to_string());

        Ok(result.status)
    })
}).await;
```

### 4. Separate Liveness and Readiness

```rust
let liveness_checker = HealthChecker::new();
let readiness_checker = HealthChecker::new();

// Liveness: Basic system health
liveness_checker.add_memory_check(0.95).await;

// Readiness: All dependencies healthy
readiness_checker.add_check("database", db_health_check).await;
readiness_checker.add_check("cache", cache_health_check).await;
readiness_checker.add_check("api", api_health_check).await;
```

### 5. Add Metadata for Debugging

```rust
let checker = HealthChecker::new();

// Add useful metadata
checker.set_metadata("version", env!("CARGO_PKG_VERSION")).await;
checker.set_metadata("environment", std::env::var("ENV").unwrap_or_default()).await;
checker.set_metadata("hostname", hostname::get()?.to_string_lossy().to_string()).await;
checker.set_metadata("startup_time", startup_time.to_rfc3339()).await;
```

## Performance Characteristics

Based on benchmark results from v0.2.0:

### Performance Metrics

- **Single health check**: ~3.8μs average
- **Multiple checks (11 checks)**: ~4.0μs average
- **Memory overhead**: Minimal (Arc-wrapped closures)

### Performance Targets

All targets are met in production:

- ✅ Single check: < 1ms (actual: ~3.8μs, **263x faster**)
- ✅ Multiple checks: < 10ms (actual: ~4.0μs, **2500x faster**)

### Benchmark Results

```bash
# Run health check benchmarks
cargo test --test performance_benchmarks benchmark_health_checks -- --nocapture

# Expected output:
# 🏥 Health Check Benchmarks
# Single health check - Avg: 3.819µs
# Multiple health checks (11 checks) - Avg: 4.005µs
# ✅ Health checks meet performance targets
```

## Monitoring and Observability

### Logging Health Check Results

```rust
use tracing::{info, warn, error};

let report = checker.check_health().await;

match report.status() {
    "healthy" => info!("Health check passed"),
    "degraded" => warn!("Health check degraded: {:?}", report),
    "unhealthy" => error!("Health check failed: {:?}", report),
    _ => {}
}
```

### Prometheus Metrics

```rust
use prometheus::{IntGauge, Registry};

// Create Prometheus gauge
let health_gauge = IntGauge::new(
    "agentflow_health_status",
    "Health check status (1=healthy, 0=unhealthy)"
).unwrap();

// Update based on health check
let report = checker.check_health().await;
health_gauge.set(if report.is_healthy { 1 } else { 0 });
```

### Structured Logging

```rust
use serde_json::json;

let report = checker.check_health().await;

let log_entry = json!({
    "timestamp": report.timestamp,
    "is_healthy": report.is_healthy,
    "status": report.status(),
    "checks": report.checks.iter().map(|c| json!({
        "name": c.name,
        "status": format!("{:?}", c.status),
        "message": c.message,
    })).collect::<Vec<_>>(),
});

println!("{}", serde_json::to_string_pretty(&log_entry)?);
```

## Troubleshooting

### Health checks always fail

**Problem:** Health checks consistently return unhealthy status.

**Solutions:**
1. Check the error messages in the health report:
   ```rust
   let report = checker.check_health().await;
   for check in report.checks {
       if !check.status.is_healthy() {
           println!("Failed: {} - {:?}", check.name, check.message);
       }
   }
   ```

2. Verify dependency connectivity
3. Check resource thresholds are appropriate

### Health checks timeout

**Problem:** Health check takes too long to complete.

**Solutions:**
1. Add timeouts to individual checks:
   ```rust
   use agentflow_core::timeout::with_timeout;

   checker.add_check("slow_api", || {
       Box::pin(async {
           with_timeout(
               check_api(),
               Duration::from_secs(5)
           ).await?;
           Ok(HealthStatus::Healthy)
       })
   }).await;
   ```

2. Profile slow checks and optimize
3. Consider removing expensive checks from liveness probe

## See Also

- [Timeout Control](TIMEOUT_CONTROL.md) - Operation timeout management
- [Checkpoint Recovery](CHECKPOINT_RECOVERY.md) - Workflow state persistence
- [Resource Management](RESOURCE_MANAGEMENT.md) - Memory limits and cleanup
- [Retry Mechanism](RETRY_MECHANISM.md) - Automatic retry with backoff

---

**Last Updated:** 2025-11-16
**Version:** 0.2.0+
