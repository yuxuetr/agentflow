//! Graph renderers for different output formats
//!
//! This module provides the `GraphRenderer` trait and implementations for
//! various output formats including Mermaid, Graphviz DOT, and JSON.

mod dot;
mod json;
mod mermaid;

pub use dot::DotRenderer;
pub use json::JsonRenderer;
pub use mermaid::MermaidRenderer;

use crate::graph::VisualGraph;

/// Output format for graph rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
  /// Mermaid diagram format (for Markdown, GitHub, etc.)
  Mermaid,
  /// Graphviz DOT format (for generating images)
  Dot,
  /// JSON format (for web frontends)
  Json,
}

impl OutputFormat {
  /// Get the file extension for this format
  pub fn extension(&self) -> &'static str {
    match self {
      OutputFormat::Mermaid => "mmd",
      OutputFormat::Dot => "dot",
      OutputFormat::Json => "json",
    }
  }

  /// Get the MIME type for this format
  pub fn mime_type(&self) -> &'static str {
    match self {
      OutputFormat::Mermaid => "text/plain",
      OutputFormat::Dot => "text/vnd.graphviz",
      OutputFormat::Json => "application/json",
    }
  }
}

impl std::str::FromStr for OutputFormat {
  type Err = RenderError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "mermaid" | "mmd" => Ok(OutputFormat::Mermaid),
      "dot" | "graphviz" => Ok(OutputFormat::Dot),
      "json" => Ok(OutputFormat::Json),
      _ => Err(RenderError::UnsupportedFormat(s.to_string())),
    }
  }
}

/// Errors that can occur during rendering
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
  #[error("Unsupported output format: {0}")]
  UnsupportedFormat(String),

  #[error("Rendering failed: {0}")]
  RenderFailed(String),

  #[error("Invalid graph structure: {0}")]
  InvalidGraph(String),

  #[error("IO error: {0}")]
  IoError(#[from] std::io::Error),

  #[error("Serialization error: {0}")]
  SerializationError(String),
}

/// Trait for graph renderers
pub trait GraphRenderer: Send + Sync {
  /// Render the graph to a string
  fn render(&self, graph: &VisualGraph) -> Result<String, RenderError>;

  /// Get the output format of this renderer
  fn format(&self) -> OutputFormat;
}

/// Configuration options for rendering
#[derive(Debug, Clone)]
pub struct RenderConfig {
  /// Include node status styling
  pub show_status: bool,
  /// Include edge labels
  pub show_edge_labels: bool,
  /// Graph direction (for formats that support it)
  pub direction: GraphDirection,
  /// Include metadata in output (for JSON)
  pub include_metadata: bool,
  /// Pretty print output (for JSON)
  pub pretty_print: bool,
}

impl Default for RenderConfig {
  fn default() -> Self {
    Self {
      show_status: true,
      show_edge_labels: true,
      direction: GraphDirection::TopToBottom,
      include_metadata: true,
      pretty_print: true,
    }
  }
}

/// Direction of the graph layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDirection {
  TopToBottom,
  BottomToTop,
  LeftToRight,
  RightToLeft,
}

impl GraphDirection {
  /// Get the Mermaid direction code
  pub fn mermaid_code(&self) -> &'static str {
    match self {
      GraphDirection::TopToBottom => "TD",
      GraphDirection::BottomToTop => "BT",
      GraphDirection::LeftToRight => "LR",
      GraphDirection::RightToLeft => "RL",
    }
  }

  /// Get the Graphviz rankdir value
  pub fn dot_rankdir(&self) -> &'static str {
    match self {
      GraphDirection::TopToBottom => "TB",
      GraphDirection::BottomToTop => "BT",
      GraphDirection::LeftToRight => "LR",
      GraphDirection::RightToLeft => "RL",
    }
  }
}

/// Create a renderer for the specified format
pub fn create_renderer(format: OutputFormat) -> Box<dyn GraphRenderer> {
  create_renderer_with_config(format, RenderConfig::default())
}

/// Create a renderer with custom configuration
pub fn create_renderer_with_config(
  format: OutputFormat,
  config: RenderConfig,
) -> Box<dyn GraphRenderer> {
  match format {
    OutputFormat::Mermaid => Box::new(MermaidRenderer::new(config)),
    OutputFormat::Dot => Box::new(DotRenderer::new(config)),
    OutputFormat::Json => Box::new(JsonRenderer::new(config)),
  }
}

/// Convenience function to render a graph to a specific format
pub fn render(graph: &VisualGraph, format: OutputFormat) -> Result<String, RenderError> {
  let renderer = create_renderer(format);
  renderer.render(graph)
}

/// Convenience function to render with custom config
pub fn render_with_config(
  graph: &VisualGraph,
  format: OutputFormat,
  config: RenderConfig,
) -> Result<String, RenderError> {
  let renderer = create_renderer_with_config(format, config);
  renderer.render(graph)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_output_format_from_str() {
    assert_eq!(
      "mermaid".parse::<OutputFormat>().unwrap(),
      OutputFormat::Mermaid
    );
    assert_eq!("dot".parse::<OutputFormat>().unwrap(), OutputFormat::Dot);
    assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
    assert!("invalid".parse::<OutputFormat>().is_err());
  }

  #[test]
  fn test_output_format_extension() {
    assert_eq!(OutputFormat::Mermaid.extension(), "mmd");
    assert_eq!(OutputFormat::Dot.extension(), "dot");
    assert_eq!(OutputFormat::Json.extension(), "json");
  }
}
