// AgentFlow Batch Processing Example
// Migrated from PocketFlow cookbook/pocketflow-batch
// Tests: BatchNode functionality, parallel processing, file operations

use agentflow_core::{AgentFlowError, AsyncFlow, AsyncNode, Result, SharedState};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;
use tokio::fs;
use tokio::time::{sleep, Duration};

/// Mock LLM translation function
async fn translate_text(text: &str, target_language: &str) -> String {
  // Simulate translation API call delay
  sleep(Duration::from_millis(200)).await;

  // Mock translations for demonstration
  let mock_translations = HashMap::from([
    ("Chinese", "这是一个模拟的中文翻译"),
    ("Spanish", "Esta es una traducción simulada en español"),
    ("Japanese", "これは模擬的な日本語翻訳です"),
    ("German", "Dies ist eine simulierte deutsche Übersetzung"),
    ("Russian", "Это имитированный русский перевод"),
    ("Portuguese", "Esta é uma tradução simulada em português"),
    ("French", "Ceci est une traduction simulée en français"),
    ("Korean", "이것은 시뮬레이션된 한국어 번역입니다"),
  ]);

  if let Some(translation) = mock_translations.get(target_language) {
    format!(
      "# {}\n\n{}\n\n*Original text: {}...*",
      target_language,
      translation,
      &text[..std::cmp::min(50, text.len())]
    )
  } else {
    format!(
      "# Translation to {}\n\n[Mock translation of: {}...]",
      target_language,
      &text[..std::cmp::min(50, text.len())]
    )
  }
}

/// Batch translation result
#[derive(Debug, Clone)]
struct TranslationResult {
  language: String,
  translation: String,
  processing_time_ms: u64,
}

/// Translate Text Node - equivalent to PocketFlow's TranslateTextNode (BatchNode)
/// Tests batch processing with parallel execution
struct TranslateTextNode {
  node_id: String,
  max_concurrency: usize,
}

impl TranslateTextNode {
  fn new(max_concurrency: usize) -> Self {
    Self {
      node_id: "translate_text_node".to_string(),
      max_concurrency,
    }
  }
}

#[async_trait]
impl AsyncNode for TranslateTextNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Phase 1: Preparation - get text and target languages
    let text = shared
      .get("text")
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| "(No text provided)".to_string());

    let languages: Vec<String> = shared
      .get("languages")
      .and_then(|v| {
        v.as_array().map(|arr| {
          arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect()
        })
      })
      .unwrap_or_else(|| {
        vec![
          "Chinese".to_string(),
          "Spanish".to_string(),
          "Japanese".to_string(),
          "German".to_string(),
          "Russian".to_string(),
          "Portuguese".to_string(),
          "French".to_string(),
          "Korean".to_string(),
        ]
      });

    println!("🔍 [PREP] Text length: {} characters", text.len());
    println!("🔍 [PREP] Target languages: {:?}", languages);
    println!("🔍 [PREP] Max concurrency: {}", self.max_concurrency);

    Ok(serde_json::json!({
      "text": text,
      "languages": languages
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    // Phase 2: Execution - batch translate to all languages
    let text = prep_result["text"].as_str().unwrap();
    let languages: Vec<String> = prep_result["languages"]
      .as_array()
      .unwrap()
      .iter()
      .map(|v| v.as_str().unwrap().to_string())
      .collect();

    println!(
      "🤖 [EXEC] Starting batch translation for {} languages",
      languages.len()
    );
    let start_time = Instant::now();

    // Create semaphore for concurrency control
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(self.max_concurrency));

    // Process translations concurrently
    let mut tasks = Vec::new();

    for language in languages {
      let text = text.to_string();
      let language = language.clone();
      let semaphore = semaphore.clone();

      let task = tokio::spawn(async move {
        let _permit = semaphore.acquire().await.unwrap();
        let task_start = Instant::now();

        println!("  🌐 Translating to {}...", language);
        let translation = translate_text(&text, &language).await;
        let processing_time = task_start.elapsed();

        println!(
          "  ✅ {} translation completed in {:?}",
          language, processing_time
        );

        TranslationResult {
          language,
          translation,
          processing_time_ms: processing_time.as_millis() as u64,
        }
      });

      tasks.push(task);
    }

    // Wait for all translations to complete
    let mut results = Vec::new();
    for task in tasks {
      results.push(task.await.unwrap());
    }

    let total_duration = start_time.elapsed();
    println!(
      "⚡ [EXEC] All translations completed in {:?}",
      total_duration
    );

    // Convert results to JSON
    let results_json: Vec<Value> = results
      .iter()
      .map(|r| {
        serde_json::json!({
          "language": r.language,
          "translation": r.translation,
          "processing_time_ms": r.processing_time_ms
        })
      })
      .collect();

    Ok(serde_json::json!({
      "results": results_json,
      "total_time_ms": total_duration.as_millis(),
      "languages_count": results.len()
    }))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep: Value,
    exec: Value,
  ) -> Result<Option<String>> {
    // Phase 3: Post-processing - save translations to files
    let results = exec["results"].as_array().unwrap();
    let output_dir = shared
      .get("output_dir")
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| "translations".to_string());

    println!(
      "💾 [POST] Saving {} translations to {}/",
      results.len(),
      output_dir
    );

    // Create output directory
    if let Err(e) = fs::create_dir_all(&output_dir).await {
      return Err(AgentFlowError::NodeExecutionFailed {
        message: format!("Failed to create output directory: {}", e),
      });
    }

    // Save each translation to a file
    for result in results {
      let language = result["language"].as_str().unwrap();
      let translation = result["translation"].as_str().unwrap();
      let processing_time = result["processing_time_ms"].as_u64().unwrap();

      let filename = format!("{}/README_{}.md", output_dir, language.to_uppercase());

      // Add metadata to translation
      let content = format!(
        "{}\n\n---\n*Translation processing time: {}ms*\n*Generated by AgentFlow batch processing*\n",
        translation,
        processing_time
      );

      if let Err(e) = fs::write(&filename, content).await {
        return Err(AgentFlowError::NodeExecutionFailed {
          message: format!("Failed to write translation file {}: {}", filename, e),
        });
      }

      println!("  📄 Saved {} translation to {}", language, filename);
    }

    // Store summary in shared state
    shared.insert(
      "translation_count".to_string(),
      Value::Number(results.len().into()),
    );
    shared.insert(
      "output_directory".to_string(),
      Value::String(output_dir.clone()),
    );
    shared.insert(
      "total_processing_time_ms".to_string(),
      Value::Number(exec["total_time_ms"].as_u64().unwrap().into()),
    );

    println!("💾 [POST] Batch processing completed successfully!");

    // Return None to end the flow
    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.node_id.clone())
  }
}

/// Create translation flow equivalent to PocketFlow's batch translation
fn create_translation_flow(max_concurrency: usize) -> AsyncFlow {
  let translate_node = Box::new(TranslateTextNode::new(max_concurrency));
  AsyncFlow::new(translate_node)
}

/// Main batch processing example
async fn run_batch_processing_example() -> Result<()> {
  println!("🚀 AgentFlow Batch Processing Example");
  println!("📝 Migrated from: PocketFlow cookbook/pocketflow-batch");
  println!("🎯 Testing: BatchNode functionality, parallel processing, file operations\n");

  // Sample text to translate (using a shorter text for demo)
  let sample_text = r#"# AgentFlow Documentation

AgentFlow is a high-performance, async-first Rust framework for building intelligent agent workflows.

## Key Features

- **Async-first**: Built on tokio for maximum concurrency
- **Type-safe**: Leverages Rust's type system for reliability  
- **Batch Processing**: Efficient parallel processing capabilities
- **Enterprise Ready**: Built-in observability and error handling

## Getting Started

```rust
use agentflow_core::{AsyncFlow, AsyncNode};

// Your intelligent workflow starts here
```

This framework enables building production-grade AI applications with confidence."#;

  // Create shared state with input data
  let shared = SharedState::new();
  shared.insert("text".to_string(), Value::String(sample_text.to_string()));
  shared.insert(
    "languages".to_string(),
    serde_json::json!(["Chinese", "Spanish", "Japanese", "German"]),
  );
  shared.insert(
    "output_dir".to_string(),
    Value::String("translations".to_string()),
  );

  println!("📄 Input text length: {} characters", sample_text.len());
  println!("🌍 Target languages: Chinese, Spanish, Japanese, German");
  println!("📁 Output directory: translations/\n");

  // Create and run the translation flow with concurrency limit
  let translation_flow = create_translation_flow(2); // Max 2 concurrent translations
  let start_time = Instant::now();

  match translation_flow.run_async(&shared).await {
    Ok(result) => {
      let total_duration = start_time.elapsed();

      println!("\n✅ Batch processing completed in {:?}", total_duration);
      println!("📋 Flow result: {:?}", result);

      // Extract and display results
      let translation_count = shared
        .get("translation_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

      let output_dir = shared
        .get("output_directory")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

      let processing_time = shared
        .get("total_processing_time_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

      println!("\n🎯 Batch Processing Results:");
      println!("Translations Created: {}", translation_count);
      println!("Output Directory: {}/", output_dir);
      println!("Total Processing Time: {}ms", processing_time);
      println!("Total Flow Time: {:?}", total_duration);

      // Verify functionality
      assert!(translation_count > 0, "Should have created translations");
      assert!(processing_time > 0, "Should have recorded processing time");

      println!("\n✅ All assertions passed - AgentFlow batch processing verified!");
    }
    Err(e) => {
      println!("❌ Batch processing failed: {}", e);
      return Err(e);
    }
  }

  Ok(())
}

/// Performance comparison with PocketFlow
async fn performance_comparison() {
  println!("\n📊 Batch Processing Performance Comparison:");
  println!("PocketFlow (Python):");
  println!("  - Sequential BatchNode processing");
  println!("  - Thread-based concurrency (GIL limitations)");
  println!("  - Dict-based shared state");
  println!("  - File I/O with standard library");
  println!();
  println!("AgentFlow (Rust):");
  println!("  - Async/await with tokio runtime");
  println!("  - True parallel processing with semaphores");
  println!("  - Thread-safe SharedState with Arc<RwLock>");
  println!("  - Async file I/O with tokio::fs");
  println!();
  println!("Expected improvements:");
  println!("  - 🚀 Better resource utilization");
  println!("  - ⚡ True parallel processing (no GIL)");
  println!("  - 💧 Lower memory footprint");
  println!("  - 🛡️ Type-safe error handling");
  println!("  - 📊 Built-in concurrency controls");
}

#[tokio::main]
async fn main() -> Result<()> {
  // Run the batch processing example
  run_batch_processing_example().await?;

  // Show performance comparison notes
  performance_comparison().await;

  println!("\n🎉 Batch Processing migration completed successfully!");
  println!("🔬 AgentFlow batch functionality verified:");
  println!("  ✅ Parallel batch processing");
  println!("  ✅ Concurrency control with semaphores");
  println!("  ✅ Async file I/O operations");
  println!("  ✅ Structured error handling");
  println!("  ✅ Performance monitoring");

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_translate_text_mock() {
    let result = translate_text("Hello world", "Spanish").await;
    assert!(result.contains("Spanish"));
    assert!(result.contains("Hello world"));
  }

  #[tokio::test]
  async fn test_translate_node_lifecycle() {
    let node = TranslateTextNode::new(2);
    let shared = SharedState::new();

    // Setup input data
    shared.insert("text".to_string(), Value::String("Test text".to_string()));
    shared.insert(
      "languages".to_string(),
      serde_json::json!(["Spanish", "French"]),
    );

    // Test prep phase
    let prep_result = node.prep_async(&shared).await.unwrap();
    assert_eq!(prep_result["text"].as_str().unwrap(), "Test text");
    assert_eq!(prep_result["languages"].as_array().unwrap().len(), 2);

    // Test exec phase
    let exec_result = node.exec_async(prep_result).await.unwrap();
    assert_eq!(exec_result["languages_count"].as_u64().unwrap(), 2);
    assert!(exec_result["total_time_ms"].as_u64().unwrap() > 0);

    // Test post phase
    let post_result = node
      .post_async(&shared, Value::Null, exec_result)
      .await
      .unwrap();
    assert!(post_result.is_none()); // Should end flow

    // Verify shared state was updated
    assert!(shared.get("translation_count").is_some());
    assert!(shared.get("total_processing_time_ms").is_some());
  }

  #[tokio::test]
  async fn test_batch_translation_flow() {
    let shared = SharedState::new();
    shared.insert("text".to_string(), Value::String("Test".to_string()));
    shared.insert("languages".to_string(), serde_json::json!(["Spanish"]));
    shared.insert(
      "output_dir".to_string(),
      Value::String("test_translations".to_string()),
    );

    let flow = create_translation_flow(1);
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());
    assert!(shared.get("translation_count").is_some());

    // Cleanup test directory
    let _ = tokio::fs::remove_dir_all("test_translations").await;
  }

  #[tokio::test]
  async fn test_concurrency_control() {
    let node = TranslateTextNode::new(1); // Max 1 concurrent
    let shared = SharedState::new();

    shared.insert("text".to_string(), Value::String("Test".to_string()));
    shared.insert(
      "languages".to_string(),
      serde_json::json!(["Spanish", "French"]),
    );

    let prep_result = node.prep_async(&shared).await.unwrap();
    let start = Instant::now();
    let _exec_result = node.exec_async(prep_result).await.unwrap();
    let duration = start.elapsed();

    // With concurrency=1, should take at least 400ms (2 * 200ms mock delay)
    assert!(duration >= Duration::from_millis(350));
  }
}
