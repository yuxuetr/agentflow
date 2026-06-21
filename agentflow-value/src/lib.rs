//! `agentflow-value` — the universal data contract for AgentFlow.
//!
//! [`FlowValue`] is the single value type passed between nodes in a workflow
//! and through the agent runtime. It is the lowest leaf of the contract kernel
//! (`docs/RFC_CRATE_ARCHITECTURE.md` §4): it depends on nothing but `serde` and
//! is depended on by almost everything. Extracted from `agentflow-core` in
//! P-A1.5; `agentflow_core::FlowValue` re-exports it for backward compatibility.

use serde::{
  Deserialize, Deserializer, Serialize, Serializer,
  de::{self, MapAccess, Visitor},
};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// A unified data wrapper for all values passed between nodes in a workflow.
///
/// `FlowValue` allows for handling heterogeneous, multi-modal data in a type-safe
/// and efficient manner. It follows a principle of passing large data (like files)
/// by reference (path) and small, simple data by value.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowValue {
  /// Represents any data that is directly serializable to a JSON value.
  /// This includes text, numbers, booleans, lists, and objects.
  Json(Value),

  /// Represents a reference to a file on the local filesystem.
  /// This is used to pass large binary data without loading it into memory.
  File {
    path: PathBuf,
    mime_type: Option<String>,
  },

  /// Represents a reference to a remote resource via a URL.
  Url {
    url: String,
    mime_type: Option<String>,
  },
}

impl FlowValue {
  /// Best-effort estimate of the serialized size of this value, in bytes.
  ///
  /// Used by the live-state size gauge (P10.14.2-FU6) to summarise an
  /// active workflow's state pool without persisting the actual contents.
  /// `Json` variants are sized via `serde_json::to_vec` (compact form);
  /// `File` and `Url` variants only count the path/url string plus the
  /// optional mime tag — the underlying blob lives elsewhere and is not
  /// part of the in-memory pool.
  ///
  /// Returns `0` if the JSON encoder errors (shouldn't happen for any
  /// `serde_json::Value` round-tripped through this enum, but fall back
  /// to zero rather than panic).
  pub fn estimated_size_bytes(&self) -> usize {
    match self {
      FlowValue::Json(value) => serde_json::to_vec(value).map(|b| b.len()).unwrap_or(0),
      FlowValue::File { path, mime_type } => {
        path.as_os_str().len() + mime_type.as_deref().map(str::len).unwrap_or(0)
      }
      FlowValue::Url { url, mime_type } => {
        url.len() + mime_type.as_deref().map(str::len).unwrap_or(0)
      }
    }
  }
}

impl Serialize for FlowValue {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match self {
      FlowValue::Json(value) => {
        #[derive(Serialize)]
        struct JsonFlowValue<'a> {
          #[serde(rename = "type")]
          type_tag: &'static str,
          value: &'a Value,
        }

        JsonFlowValue {
          type_tag: "json",
          value,
        }
        .serialize(serializer)
      }
      FlowValue::File { path, mime_type } => {
        #[derive(Serialize)]
        struct FileFlowValue<'a> {
          #[serde(rename = "type")]
          type_tag: &'static str,
          path: &'a PathBuf,
          mime_type: &'a Option<String>,
        }

        FileFlowValue {
          type_tag: "file",
          path,
          mime_type,
        }
        .serialize(serializer)
      }
      FlowValue::Url { url, mime_type } => {
        #[derive(Serialize)]
        struct UrlFlowValue<'a> {
          #[serde(rename = "type")]
          type_tag: &'static str,
          url: &'a str,
          mime_type: &'a Option<String>,
        }

        UrlFlowValue {
          type_tag: "url",
          url,
          mime_type,
        }
        .serialize(serializer)
      }
    }
  }
}

impl<'de> Deserialize<'de> for FlowValue {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    deserializer.deserialize_any(FlowValueVisitor)
  }
}

struct FlowValueVisitor;

impl<'de> Visitor<'de> for FlowValueVisitor {
  type Value = FlowValue;

  fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    formatter.write_str("a tagged FlowValue object or a raw JSON value")
  }

  fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
  where
    A: MapAccess<'de>,
  {
    let mut map = Map::new();
    while let Some((key, value)) = access.next_entry::<String, Value>()? {
      map.insert(key, value);
    }

    match flow_value_from_object(&map).map_err(de::Error::custom)? {
      Some(flow_value) => Ok(flow_value),
      None => Ok(FlowValue::Json(Value::Object(map))),
    }
  }

  fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::Bool(value)))
  }

  fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::from(value)))
  }

  fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::from(value)))
  }

  fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::from(value)))
  }

  fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::String(value.to_string())))
  }

  fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::String(value)))
  }

  fn visit_none<E>(self) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::Null))
  }

  fn visit_unit<E>(self) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    Ok(FlowValue::Json(Value::Null))
  }

  fn visit_seq<A>(self, access: A) -> Result<Self::Value, A::Error>
  where
    A: de::SeqAccess<'de>,
  {
    let values = Vec::<Value>::deserialize(de::value::SeqAccessDeserializer::new(access))?;
    Ok(FlowValue::Json(Value::Array(values)))
  }
}

fn flow_value_from_object(map: &Map<String, Value>) -> Result<Option<FlowValue>, String> {
  let type_tag = map
    .get("type")
    .or_else(|| map.get("$type"))
    .and_then(Value::as_str);

  match type_tag {
    Some("json") => Ok(Some(FlowValue::Json(
      map.get("value").cloned().unwrap_or(Value::Null),
    ))),
    Some("file") => {
      let path = map
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "FlowValue file is missing string field 'path'".to_string())?;
      let mime_type = optional_string_field(map, "mime_type")?;
      Ok(Some(FlowValue::File {
        path: PathBuf::from(path),
        mime_type,
      }))
    }
    Some("url") => {
      let url = map
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "FlowValue url is missing string field 'url'".to_string())?;
      let mime_type = optional_string_field(map, "mime_type")?;
      Ok(Some(FlowValue::Url {
        url: url.to_string(),
        mime_type,
      }))
    }
    Some(other) => Err(format!("unknown FlowValue type '{}'", other)),
    None => Ok(None),
  }
}

fn optional_string_field(map: &Map<String, Value>, field: &str) -> Result<Option<String>, String> {
  match map.get(field) {
    Some(Value::String(value)) => Ok(Some(value.clone())),
    Some(Value::Null) | None => Ok(None),
    Some(_) => Err(format!(
      "FlowValue field '{}' must be a string or null",
      field
    )),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use proptest::prelude::*;
  use serde_json::json;

  prop_compose! {
    fn flow_value_strategy()(
      tag in 0u8..3,
      text in "[a-zA-Z0-9_./:-]{0,64}",
      mime in prop::option::of("[a-z]{1,16}/[a-z0-9.+-]{1,24}"),
      json_value in any::<i64>(),
    ) -> FlowValue {
      match tag {
        0 => FlowValue::Json(json!({ "value": json_value, "text": text })),
        1 => FlowValue::File {
          path: PathBuf::from(format!("/tmp/{}", text)),
          mime_type: mime,
        },
        _ => FlowValue::Url {
          url: format!("https://example.test/{}", text),
          mime_type: mime,
        },
      }
    }
  }

  proptest! {
    #[test]
    fn flow_value_json_roundtrip_preserves_variant(value in flow_value_strategy()) {
      let encoded = serde_json::to_value(&value).unwrap();
      let decoded: FlowValue = serde_json::from_value(encoded).unwrap();
      prop_assert_eq!(decoded, value);
    }
  }

  #[test]
  fn flow_value_uses_stable_tagged_schema() {
    let value = FlowValue::Json(json!({"ok": true}));
    assert_eq!(
      serde_json::to_value(value).unwrap(),
      json!({"type": "json", "value": {"ok": true}})
    );
  }

  #[test]
  fn estimated_size_bytes_json_matches_compact_encoding() {
    let value = FlowValue::Json(json!({"k": "v"}));
    let expected = serde_json::to_vec(&json!({"k": "v"})).unwrap().len();
    assert_eq!(value.estimated_size_bytes(), expected);
    assert!(value.estimated_size_bytes() >= "{\"k\":\"v\"}".len());
  }

  #[test]
  fn estimated_size_bytes_file_counts_path_and_mime_only() {
    let value = FlowValue::File {
      path: PathBuf::from("/tmp/blob.bin"),
      mime_type: Some("application/octet-stream".to_string()),
    };
    assert_eq!(
      value.estimated_size_bytes(),
      "/tmp/blob.bin".len() + "application/octet-stream".len()
    );

    let value_no_mime = FlowValue::File {
      path: PathBuf::from("/tmp/blob.bin"),
      mime_type: None,
    };
    assert_eq!(value_no_mime.estimated_size_bytes(), "/tmp/blob.bin".len());
  }

  #[test]
  fn estimated_size_bytes_url_counts_url_and_mime_only() {
    let value = FlowValue::Url {
      url: "https://example.test/r".to_string(),
      mime_type: Some("text/plain".to_string()),
    };
    assert_eq!(
      value.estimated_size_bytes(),
      "https://example.test/r".len() + "text/plain".len()
    );
  }

  #[test]
  fn flow_value_reads_legacy_checkpoint_tags() {
    let value: FlowValue = serde_json::from_value(json!({
      "$type": "file",
      "path": "/tmp/legacy.txt",
      "mime_type": "text/plain"
    }))
    .unwrap();

    assert_eq!(
      value,
      FlowValue::File {
        path: PathBuf::from("/tmp/legacy.txt"),
        mime_type: Some("text/plain".to_string())
      }
    );
  }
}
