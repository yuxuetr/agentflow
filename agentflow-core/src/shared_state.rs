use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct SharedState {
  inner: Arc<RwLock<HashMap<String, Value>>>,
}

impl SharedState {
  pub fn new() -> Self {
    Self {
      inner: Arc::new(RwLock::new(HashMap::new())),
    }
  }

  pub fn insert(&self, key: String, value: Value) {
    let mut map = self.inner.write().unwrap();
    map.insert(key, value);
  }

  pub fn get(&self, key: &str) -> Option<Value> {
    let map = self.inner.read().unwrap();
    map.get(key).cloned()
  }

  pub fn contains_key(&self, key: &str) -> bool {
    let map = self.inner.read().unwrap();
    map.contains_key(key)
  }

  pub fn remove(&self, key: &str) -> Option<Value> {
    let mut map = self.inner.write().unwrap();
    map.remove(key)
  }

  pub fn is_empty(&self) -> bool {
    let map = self.inner.read().unwrap();
    map.is_empty()
  }
}

impl Default for SharedState {
  fn default() -> Self {
    Self::new()
  }
}

impl Serialize for SharedState {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    let map = self.inner.read().unwrap();
    map.serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for SharedState {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let map = HashMap::<String, Value>::deserialize(deserializer)?;
    Ok(Self {
      inner: Arc::new(RwLock::new(map)),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::thread;
  use std::sync::Arc;
  use serde_json::Value;

  #[test]
  fn test_shared_state_creation() {
    // Test creating a new SharedState
    let state = SharedState::new();
    assert!(state.is_empty());
  }

  #[test]
  fn test_shared_state_insert_and_get() {
    // Test inserting and retrieving values
    let state = SharedState::new();
    state.insert("key1".to_string(), Value::String("value1".to_string()));
    
    let retrieved = state.get("key1").unwrap();
    assert_eq!(retrieved, Value::String("value1".to_string()));
  }

  #[test]
  fn test_shared_state_thread_safety() {
    // Test thread-safe operations
    let state = Arc::new(SharedState::new());
    let mut handles = vec![];

    // Spawn multiple threads that modify the state
    for i in 0..10 {
      let state_clone = Arc::clone(&state);
      let handle = thread::spawn(move || {
        state_clone.insert(
          format!("key{}", i),
          Value::Number(serde_json::Number::from(i))
        );
      });
      handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
      handle.join().unwrap();
    }

    // Verify all values were inserted
    for i in 0..10 {
      let key = format!("key{}", i);
      assert!(state.contains_key(&key));
    }
  }

  #[test]
  fn test_shared_state_concurrent_read_write() {
    // Test concurrent reads and writes
    let state = Arc::new(SharedState::new());
    state.insert("counter".to_string(), Value::Number(serde_json::Number::from(0)));

    let state_writer = Arc::clone(&state);
    let state_reader = Arc::clone(&state);

    let writer = thread::spawn(move || {
      for i in 1..=100 {
        state_writer.insert(
          "counter".to_string(),
          Value::Number(serde_json::Number::from(i))
        );
      }
    });

    let reader = thread::spawn(move || {
      for _ in 0..50 {
        let _value = state_reader.get("counter");
        thread::sleep(std::time::Duration::from_millis(1));
      }
    });

    writer.join().unwrap();
    reader.join().unwrap();

    // Verify final state
    let final_value = state.get("counter").unwrap();
    assert_eq!(final_value, Value::Number(serde_json::Number::from(100)));
  }

  #[test]
  fn test_shared_state_remove() {
    // Test removing values
    let state = SharedState::new();
    state.insert("key1".to_string(), Value::String("value1".to_string()));
    
    assert!(state.contains_key("key1"));
    let removed = state.remove("key1").unwrap();
    assert_eq!(removed, Value::String("value1".to_string()));
    assert!(!state.contains_key("key1"));
  }

  #[test]
  fn test_shared_state_clone() {
    // Test cloning SharedState (Arc clone shares the same data)
    let state1 = SharedState::new();
    state1.insert("key1".to_string(), Value::String("value1".to_string()));
    
    let state2 = state1.clone();
    assert_eq!(state1.get("key1"), state2.get("key1"));
    
    // With Arc, modifications to one affect the other (this is the intended behavior)
    state1.insert("key2".to_string(), Value::String("value2".to_string()));
    assert!(state1.contains_key("key2"));
    assert!(state2.contains_key("key2")); // Both should see the change
  }

  #[test]
  fn test_shared_state_serialization() {
    // Test serializing and deserializing SharedState
    let state = SharedState::new();
    state.insert("key1".to_string(), Value::String("value1".to_string()));
    state.insert("key2".to_string(), Value::Number(serde_json::Number::from(42)));

    let serialized = serde_json::to_string(&state).unwrap();
    let deserialized: SharedState = serde_json::from_str(&serialized).unwrap();

    assert_eq!(state.get("key1"), deserialized.get("key1"));
    assert_eq!(state.get("key2"), deserialized.get("key2"));
  }
}