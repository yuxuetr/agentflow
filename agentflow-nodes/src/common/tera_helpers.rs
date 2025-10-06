use agentflow_core::value::FlowValue;
use serde_json::{Value as JsonValue, Map as JsonMap};
use tera::{Tera, Value as TeraValue, Result as TeraResult};
use std::collections::HashMap;

/// Convert FlowValue to Tera-compatible value
pub fn flow_value_to_tera_value(value: &FlowValue) -> TeraValue {
    match value {
        FlowValue::Json(json) => json_to_tera_value(json),
        FlowValue::File { path, .. } => TeraValue::String(path.to_string_lossy().to_string()),
        FlowValue::Url { url, .. } => TeraValue::String(url.clone()),
    }
}

/// Convert serde_json::Value to tera::Value
fn json_to_tera_value(json: &JsonValue) -> TeraValue {
    match json {
        JsonValue::Null => TeraValue::Null,
        JsonValue::Bool(b) => TeraValue::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                TeraValue::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                TeraValue::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                TeraValue::Number(serde_json::Number::from_f64(f).unwrap().into())
            } else {
                TeraValue::Null
            }
        }
        JsonValue::String(s) => TeraValue::String(s.clone()),
        JsonValue::Array(arr) => {
            let tera_arr: Vec<TeraValue> = arr.iter().map(json_to_tera_value).collect();
            TeraValue::Array(tera_arr)
        }
        JsonValue::Object(obj) => {
            let mut tera_obj = JsonMap::new();
            for (k, v) in obj {
                tera_obj.insert(k.clone(), json_to_tera_value(v));
            }
            TeraValue::Object(tera_obj)
        }
    }
}

/// Register custom filters for AgentFlow
pub fn register_custom_filters(tera: &mut Tera) {
    // Filter to get file path from FlowValue
    tera.register_filter("flow_path", |value: &TeraValue, _args: &HashMap<String, TeraValue>| -> TeraResult<TeraValue> {
        if let TeraValue::String(s) = value {
            Ok(TeraValue::String(s.clone()))
        } else {
            Ok(TeraValue::String(format!("{:?}", value)))
        }
    });

    // Filter to format JSON beautifully
    tera.register_filter("json_pretty", |value: &TeraValue, _args: &HashMap<String, TeraValue>| -> TeraResult<TeraValue> {
        let json_str = serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| format!("{:?}", value));
        Ok(TeraValue::String(json_str))
    });

    // Filter to convert to JSON string
    tera.register_filter("to_json", |value: &TeraValue, _args: &HashMap<String, TeraValue>| -> TeraResult<TeraValue> {
        let json_str = serde_json::to_string(value)
            .unwrap_or_else(|_| format!("{:?}", value));
        Ok(TeraValue::String(json_str))
    });
}

/// Register custom functions for AgentFlow
pub fn register_custom_functions(tera: &mut Tera) {
    // Function to get current timestamp
    tera.register_function("now", |_args: &HashMap<String, TeraValue>| -> TeraResult<TeraValue> {
        let now = chrono::Utc::now();
        Ok(TeraValue::String(now.to_rfc3339()))
    });

    // Function to generate UUID
    tera.register_function("uuid", |_args: &HashMap<String, TeraValue>| -> TeraResult<TeraValue> {
        let id = uuid::Uuid::new_v4();
        Ok(TeraValue::String(id.to_string()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_to_tera_value_primitives() {
        assert_eq!(json_to_tera_value(&json!(null)), TeraValue::Null);
        assert_eq!(json_to_tera_value(&json!(true)), TeraValue::Bool(true));
        assert_eq!(json_to_tera_value(&json!(42)), TeraValue::Number(42.into()));
        assert_eq!(json_to_tera_value(&json!("hello")), TeraValue::String("hello".to_string()));
    }

    #[test]
    fn test_json_to_tera_value_array() {
        let json_arr = json!([1, 2, 3]);
        let tera_val = json_to_tera_value(&json_arr);

        if let TeraValue::Array(arr) = tera_val {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], TeraValue::Number(1.into()));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_json_to_tera_value_object() {
        let json_obj = json!({"name": "Alice", "age": 30});
        let tera_val = json_to_tera_value(&json_obj);

        if let TeraValue::Object(obj) = tera_val {
            assert_eq!(obj.len(), 2);
            assert_eq!(obj.get("name"), Some(&TeraValue::String("Alice".to_string())));
            assert_eq!(obj.get("age"), Some(&TeraValue::Number(30.into())));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_flow_value_to_tera_value() {
        let flow_val = FlowValue::Json(json!({"test": "value"}));
        let tera_val = flow_value_to_tera_value(&flow_val);

        if let TeraValue::Object(obj) = tera_val {
            assert_eq!(obj.get("test"), Some(&TeraValue::String("value".to_string())));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_custom_filters() {
        let mut tera = Tera::default();
        register_custom_filters(&mut tera);

        // Test that filters are registered
        assert!(tera.get_filter("flow_path").is_ok());
        assert!(tera.get_filter("json_pretty").is_ok());
        assert!(tera.get_filter("to_json").is_ok());
    }

    #[test]
    fn test_custom_functions() {
        let mut tera = Tera::default();
        register_custom_functions(&mut tera);

        // Test that functions are registered
        assert!(tera.get_function("now").is_ok());
        assert!(tera.get_function("uuid").is_ok());
    }
}
