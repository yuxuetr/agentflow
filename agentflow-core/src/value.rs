use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde_json::Value;
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

// We implement custom serialization and deserialization to represent `FlowValue`
// as a tagged JSON object for persistence, while allowing `Json(Value)` to be
// serialized transparently where possible.

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PersistentFlowValue {
    File {
        #[serde(rename = "$type")]
        type_tag: String,
        path: PathBuf,
        mime_type: Option<String>,
    },
    Url {
        #[serde(rename = "$type")]
        type_tag: String,
        url: String,
        mime_type: Option<String>,
    },
    Json(Value),
}

impl Serialize for FlowValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            FlowValue::Json(v) => v.serialize(serializer),
            FlowValue::File { path, mime_type } => {
                let persistent = PersistentFlowValue::File {
                    type_tag: "file".to_string(),
                    path: path.clone(),
                    mime_type: mime_type.clone(),
                };
                persistent.serialize(serializer)
            }
            FlowValue::Url { url, mime_type } => {
                let persistent = PersistentFlowValue::Url {
                    type_tag: "url".to_string(),
                    url: url.clone(),
                    mime_type: mime_type.clone(),
                };
                persistent.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for FlowValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let persistent = PersistentFlowValue::deserialize(deserializer)?;
        match persistent {
            PersistentFlowValue::Json(v) => Ok(FlowValue::Json(v)),
            PersistentFlowValue::File { path, mime_type, .. } => {
                Ok(FlowValue::File { path, mime_type })
            }
            PersistentFlowValue::Url { url, mime_type, .. } => {
                Ok(FlowValue::Url { url, mime_type })
            }
        }
    }
}
