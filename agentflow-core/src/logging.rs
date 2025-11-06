//! Structured logging infrastructure for AgentFlow.
//!
//! Provides structured, contextual logging with support for JSON output,
//! log level filtering, and distributed tracing integration.
//!
//! # Features
//!
//! - Structured JSON logging for production
//! - Pretty-printed logging for development
//! - Environment-based configuration
//! - Context propagation (workflow_id, node_id, etc.)
//! - Log level filtering per module
//! - Integration with tracing ecosystem
//!
//! # Environment Variables
//!
//! - `RUST_LOG`: Set log level filtering (e.g., `info`, `agentflow_core=debug`)
//! - `LOG_FORMAT`: Set output format (`json` or `pretty`, default: `pretty`)
//! - `LOG_LEVEL`: Global log level (if RUST_LOG not set)
//!
//! # Examples
//!
//! ## Basic Setup
//!
//! ```rust,no_run
//! use agentflow_core::logging;
//!
//! // Initialize logging at application start
//! logging::init();
//!
//! // Or with custom configuration
//! logging::init_with_config(logging::LogConfig {
//!     format: logging::LogFormat::Json,
//!     default_level: logging::LogLevel::Info,
//!     ..Default::default()
//! });
//! ```
//!
//! ## Using Structured Logging
//!
//! ```rust,no_run
//! use agentflow_core::logging::prelude::*;
//! use agentflow_core::error::Result;
//!
//! #[instrument]
//! async fn execute_workflow(workflow_id: &str) -> Result<()> {
//!     info!(workflow_id, "Starting workflow execution");
//!
//!     // ... do work ...
//!
//!     info!(workflow_id, "Workflow completed successfully");
//!     Ok(())
//! }
//!
//! #[instrument]
//! async fn execute_with_error_handling(workflow_id: &str) -> Result<String> {
//!     match perform_operation().await {
//!         Ok(result) => {
//!             info!(workflow_id, "Operation completed successfully");
//!             Ok(result)
//!         }
//!         Err(e) => {
//!             error!(workflow_id, error = %e, "Operation failed");
//!             Err(e)
//!         }
//!     }
//! }
//!
//! # async fn perform_operation() -> Result<String> { Ok("result".to_string()) }
//! ```

#[cfg(feature = "observability")]
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use std::io;

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
  /// Pretty-printed human-readable format (for development)
  Pretty,
  /// JSON structured format (for production)
  Json,
  /// Compact format
  Compact,
}

impl Default for LogFormat {
  fn default() -> Self {
    Self::Pretty
  }
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
  /// Trace level (most verbose)
  Trace,
  /// Debug level
  Debug,
  /// Info level
  Info,
  /// Warn level
  Warn,
  /// Error level (least verbose)
  Error,
}

impl Default for LogLevel {
  fn default() -> Self {
    Self::Info
  }
}

impl From<LogLevel> for tracing::Level {
  fn from(level: LogLevel) -> Self {
    match level {
      LogLevel::Trace => tracing::Level::TRACE,
      LogLevel::Debug => tracing::Level::DEBUG,
      LogLevel::Info => tracing::Level::INFO,
      LogLevel::Warn => tracing::Level::WARN,
      LogLevel::Error => tracing::Level::ERROR,
    }
  }
}

/// Logging configuration.
#[derive(Debug, Clone)]
pub struct LogConfig {
  /// Output format
  pub format: LogFormat,

  /// Default log level
  pub default_level: LogLevel,

  /// Enable ANSI colors (only for Pretty format)
  pub enable_colors: bool,

  /// Include source file and line numbers
  pub include_location: bool,

  /// Include thread names
  pub include_thread_names: bool,

  /// Include thread IDs
  pub include_thread_ids: bool,

  /// Target width for formatting (only for Pretty format)
  pub target_width: Option<usize>,
}

impl Default for LogConfig {
  fn default() -> Self {
    Self {
      format: LogFormat::Pretty,
      default_level: LogLevel::Info,
      enable_colors: true,
      include_location: false,
      include_thread_names: false,
      include_thread_ids: false,
      target_width: Some(40),
    }
  }
}

impl LogConfig {
  /// Create a configuration suitable for production environments.
  pub fn production() -> Self {
    Self {
      format: LogFormat::Json,
      default_level: LogLevel::Info,
      enable_colors: false,
      include_location: true,
      include_thread_names: true,
      include_thread_ids: true,
      target_width: None,
    }
  }

  /// Create a configuration suitable for development environments.
  pub fn development() -> Self {
    Self {
      format: LogFormat::Pretty,
      default_level: LogLevel::Debug,
      enable_colors: true,
      include_location: true,
      include_thread_names: false,
      include_thread_ids: false,
      target_width: Some(40),
    }
  }

  /// Create a configuration from environment variables.
  pub fn from_env() -> Self {
    let format = std::env::var("LOG_FORMAT")
      .ok()
      .and_then(|f| match f.to_lowercase().as_str() {
        "json" => Some(LogFormat::Json),
        "pretty" => Some(LogFormat::Pretty),
        "compact" => Some(LogFormat::Compact),
        _ => None,
      })
      .unwrap_or_default();

    let default_level = std::env::var("LOG_LEVEL")
      .ok()
      .and_then(|l| match l.to_lowercase().as_str() {
        "trace" => Some(LogLevel::Trace),
        "debug" => Some(LogLevel::Debug),
        "info" => Some(LogLevel::Info),
        "warn" => Some(LogLevel::Warn),
        "error" => Some(LogLevel::Error),
        _ => None,
      })
      .unwrap_or_default();

    Self {
      format,
      default_level,
      ..Default::default()
    }
  }
}

/// Initialize logging with default configuration.
///
/// Reads configuration from environment variables:
/// - `RUST_LOG`: Filter directives
/// - `LOG_FORMAT`: Output format (json/pretty/compact)
/// - `LOG_LEVEL`: Default level if RUST_LOG not set
#[cfg(feature = "observability")]
pub fn init() {
  let config = LogConfig::from_env();
  init_with_config(config);
}

/// Initialize logging with custom configuration.
#[cfg(feature = "observability")]
pub fn init_with_config(config: LogConfig) {
  // Build env filter
  let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
    EnvFilter::new(format!("agentflow_core={}", level_to_str(config.default_level)))
  });

  match config.format {
    LogFormat::Json => {
      let fmt_layer = fmt::layer()
        .json()
        .with_file(config.include_location)
        .with_line_number(config.include_location)
        .with_thread_names(config.include_thread_names)
        .with_thread_ids(config.include_thread_ids)
        .with_writer(io::stderr)
        .with_filter(env_filter);

      tracing_subscriber::registry().with(fmt_layer).init();
    }
    LogFormat::Pretty => {
      let fmt_layer = fmt::layer()
        .pretty()
        .with_ansi(config.enable_colors)
        .with_file(config.include_location)
        .with_line_number(config.include_location)
        .with_thread_names(config.include_thread_names)
        .with_thread_ids(config.include_thread_ids)
        .with_writer(io::stderr)
        .with_filter(env_filter);

      tracing_subscriber::registry().with(fmt_layer).init();
    }
    LogFormat::Compact => {
      let fmt_layer = fmt::layer()
        .compact()
        .with_ansi(config.enable_colors)
        .with_file(config.include_location)
        .with_line_number(config.include_location)
        .with_thread_names(config.include_thread_names)
        .with_thread_ids(config.include_thread_ids)
        .with_writer(io::stderr)
        .with_filter(env_filter);

      tracing_subscriber::registry().with(fmt_layer).init();
    }
  }
}

#[cfg(not(feature = "observability"))]
pub fn init() {
  // No-op when observability feature is disabled
}

#[cfg(not(feature = "observability"))]
pub fn init_with_config(_config: LogConfig) {
  // No-op when observability feature is disabled
}

fn level_to_str(level: LogLevel) -> &'static str {
  match level {
    LogLevel::Trace => "trace",
    LogLevel::Debug => "debug",
    LogLevel::Info => "info",
    LogLevel::Warn => "warn",
    LogLevel::Error => "error",
  }
}

/// Common prelude for structured logging.
pub mod prelude {
  #[cfg(feature = "observability")]
  pub use tracing::{debug, error, info, instrument, trace, warn};

  #[cfg(not(feature = "observability"))]
  pub use crate::logging::noop::*;
}

/// No-op logging when observability is disabled.
#[cfg(not(feature = "observability"))]
mod noop {
  /// No-op trace macro
  #[macro_export]
  macro_rules! trace {
    ($($arg:tt)*) => {{}};
  }

  /// No-op debug macro
  #[macro_export]
  macro_rules! debug {
    ($($arg:tt)*) => {{}};
  }

  /// No-op info macro
  #[macro_export]
  macro_rules! info {
    ($($arg:tt)*) => {{}};
  }

  /// No-op warn macro
  #[macro_export]
  macro_rules! warn {
    ($($arg:tt)*) => {{}};
  }

  /// No-op error macro
  #[macro_export]
  macro_rules! error {
    ($($arg:tt)*) => {{}};
  }

  /// No-op instrument attribute
  pub use noop_instrument as instrument;

  pub fn noop_instrument(_fn: &str) -> impl Fn() {
    || {}
  }
}

/// Helper macros for common logging patterns.

/// Log an error with full context.
#[macro_export]
macro_rules! log_error {
  ($err:expr, $($field:tt)*) => {
    #[cfg(feature = "observability")]
    tracing::error!(
      error = %$err,
      error_type = std::any::type_name_of_val(&$err),
      $($field)*
    );
  };
}

/// Log a warning with context.
#[macro_export]
macro_rules! log_warn {
  ($msg:expr, $($field:tt)*) => {
    #[cfg(feature = "observability")]
    tracing::warn!($msg, $($field)*);
  };
}

/// Log an info message with context.
#[macro_export]
macro_rules! log_info {
  ($msg:expr, $($field:tt)*) => {
    #[cfg(feature = "observability")]
    tracing::info!($msg, $($field)*);
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_format_default() {
    assert_eq!(LogFormat::default(), LogFormat::Pretty);
  }

  #[test]
  fn test_log_level_default() {
    assert_eq!(LogLevel::default(), LogLevel::Info);
  }

  #[test]
  fn test_log_level_ordering() {
    assert!(LogLevel::Trace < LogLevel::Debug);
    assert!(LogLevel::Debug < LogLevel::Info);
    assert!(LogLevel::Info < LogLevel::Warn);
    assert!(LogLevel::Warn < LogLevel::Error);
  }

  #[test]
  fn test_config_production() {
    let config = LogConfig::production();
    assert_eq!(config.format, LogFormat::Json);
    assert_eq!(config.default_level, LogLevel::Info);
    assert!(!config.enable_colors);
    assert!(config.include_location);
  }

  #[test]
  fn test_config_development() {
    let config = LogConfig::development();
    assert_eq!(config.format, LogFormat::Pretty);
    assert_eq!(config.default_level, LogLevel::Debug);
    assert!(config.enable_colors);
    assert!(config.include_location);
  }

  #[test]
  fn test_level_to_str() {
    assert_eq!(level_to_str(LogLevel::Trace), "trace");
    assert_eq!(level_to_str(LogLevel::Debug), "debug");
    assert_eq!(level_to_str(LogLevel::Info), "info");
    assert_eq!(level_to_str(LogLevel::Warn), "warn");
    assert_eq!(level_to_str(LogLevel::Error), "error");
  }
}
