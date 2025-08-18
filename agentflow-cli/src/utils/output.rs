use anyhow::Result;
use serde_json::Value;

pub struct OutputFormatter {
  max_length: usize,
}

impl OutputFormatter {
  pub fn new() -> Self {
    Self {
      max_length: 200, // Truncate long values for display
    }
  }

  pub fn format_value(&self, data: &Value) -> String {
    let formatted = self.format_value_internal(data);
    if formatted.len() > self.max_length {
      format!("{}...", &formatted[..self.max_length])
    } else {
      formatted
    }
  }

  fn format_value_internal(&self, data: &Value) -> String {
    match data {
      Value::String(s) => s.clone(),
      Value::Number(n) => n.to_string(),
      Value::Bool(b) => b.to_string(),
      Value::Array(arr) => {
        if arr.len() <= 3 {
          format!(
            "[{}]",
            arr
              .iter()
              .map(|v| self.format_value_internal(v))
              .collect::<Vec<_>>()
              .join(", ")
          )
        } else {
          format!("[{} items]", arr.len())
        }
      }
      Value::Object(obj) => {
        if obj.len() <= 2 {
          format!(
            "{{{}}}",
            obj
              .iter()
              .map(|(k, v)| format!("{}: {}", k, self.format_value_internal(v)))
              .collect::<Vec<_>>()
              .join(", ")
          )
        } else {
          format!("{{object with {} keys}}", obj.len())
        }
      }
      Value::Null => "null".to_string(),
    }
  }
}

pub fn format_output(data: &Value, format: &str) -> Result<String> {
  match format {
    "json" => Ok(serde_json::to_string_pretty(data)?),
    "yaml" => Ok(serde_yaml::to_string(data)?),
    "text" => Ok(format_as_text(data)),
    _ => Ok(data.to_string()),
  }
}

fn format_as_text(data: &Value) -> String {
  match data {
    Value::String(s) => s.clone(),
    Value::Number(n) => n.to_string(),
    Value::Bool(b) => b.to_string(),
    Value::Array(arr) => arr
      .iter()
      .map(format_as_text)
      .collect::<Vec<_>>()
      .join("\n"),
    Value::Object(obj) => obj
      .iter()
      .map(|(k, v)| format!("{}: {}", k, format_as_text(v)))
      .collect::<Vec<_>>()
      .join("\n"),
    Value::Null => "null".to_string(),
  }
}
