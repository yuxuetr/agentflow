use serde_json::Value;
use anyhow::Result;

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
        Value::Array(arr) => {
            arr.iter()
                .map(format_as_text)
                .collect::<Vec<_>>()
                .join("\n")
        }
        Value::Object(obj) => {
            obj.iter()
                .map(|(k, v)| format!("{}: {}", k, format_as_text(v)))
                .collect::<Vec<_>>()
                .join("\n")
        }
        Value::Null => "null".to_string(),
    }
}