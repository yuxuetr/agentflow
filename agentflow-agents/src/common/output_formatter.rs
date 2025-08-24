//! Output formatting utilities for agents

use serde_json::Value;
use std::path::Path;

/// Format JSON output with pretty printing
pub fn format_json_pretty(value: &Value) -> crate::AgentResult<String> {
  let formatted = serde_json::to_string_pretty(value)?;
  Ok(formatted)
}

/// Format analysis results as markdown report
pub fn format_analysis_as_markdown(
  title: &str,
  summary: Option<&str>,
  insights: Option<&Value>,
  additional_sections: &[(&str, &str)]
) -> String {
  let mut markdown = String::new();
  
  markdown.push_str(&format!("# {}\n\n", title));
  
  if let Some(summary) = summary {
    markdown.push_str("## Summary\n\n");
    markdown.push_str(summary);
    markdown.push_str("\n\n");
  }
  
  if let Some(insights) = insights {
    markdown.push_str("## Key Insights\n\n");
    if let Some(obj) = insights.as_object() {
      for (key, value) in obj {
        markdown.push_str(&format!("**{}**: {}\n\n", key, format_value_for_markdown(value)));
      }
    }
  }
  
  for (section_title, content) in additional_sections {
    markdown.push_str(&format!("## {}\n\n", section_title));
    markdown.push_str(content);
    markdown.push_str("\n\n");
  }
  
  markdown
}

/// Format JSON value for markdown display
fn format_value_for_markdown(value: &Value) -> String {
  match value {
    Value::String(s) => s.clone(),
    Value::Number(n) => n.to_string(),
    Value::Bool(b) => b.to_string(),
    Value::Array(arr) => {
      let items: Vec<String> = arr.iter()
        .map(format_value_for_markdown)
        .collect();
      format!("[{}]", items.join(", "))
    }
    Value::Object(_) => serde_json::to_string_pretty(value).unwrap_or_default(),
    Value::Null => "null".to_string(),
  }
}

/// Create comprehensive output structure
pub async fn save_comprehensive_output<P: AsRef<Path>>(
  output_dir: P,
  title: &str,
  results: &[(String, String, String)]  // (filename, content, extension)
) -> crate::AgentResult<()> {
  use crate::common::file_utils::{save_content, create_timestamped_output_dir};
  
  let final_output_dir = if output_dir.as_ref().exists() {
    output_dir.as_ref().to_path_buf()
  } else {
    create_timestamped_output_dir(output_dir, "analysis").await?
  };
  
  for (filename, content, extension) in results {
    let file_path = final_output_dir.join(format!("{}.{}", filename, extension));
    save_content(file_path, content).await?;
  }
  
  println!("✅ {} results saved to: {}", title, final_output_dir.display());
  Ok(())
}