//! Bridge between `agentflow-llm`'s model-aware tokenizer and the
//! `agentflow-memory::TokenCounter` trait that `Message::new_with_counter`
//! expects (P10.3.3-FU1).
//!
//! The memory crate doesn't depend on `agentflow-llm`, so its
//! `TokenCounter` trait is a local definition with the same `&str
//! -> u32` shape as `agentflow_llm::TokenCounter`. This module
//! defines the small adapter the agent layer uses to plug one
//! into the other without forcing a workspace-wide trait merge.
//!
//! Usage:
//!
//! ```ignore
//! let counter = build_message_counter(&context.model);
//! let msg = Message::user_with_counter(session_id, input, &*counter);
//! ```
//!
//! The counter is built once per agent run (in
//! `ReActAgent::run_with_context` and `PlanExecuteAgent::run`)
//! and shared across every message that flows through the
//! conversation memory.

use agentflow_llm::counter_for_model;
use agentflow_memory::TokenCounter as MemoryTokenCounter;

/// Adapter that exposes an `agentflow-llm` BPE tokenizer to the
/// memory crate's local `TokenCounter` surface. Internally holds
/// the boxed `agentflow_llm::TokenCounter`; counting forwards
/// directly to it.
pub struct LlmTokenCounter {
  inner: Box<dyn agentflow_llm::TokenCounter>,
}

impl std::fmt::Debug for LlmTokenCounter {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("LlmTokenCounter")
      .field("name", &self.inner.name())
      .finish()
  }
}

impl LlmTokenCounter {
  /// Build the counter that matches the named model. Falls back
  /// to the heuristic when the model isn't recognized — see
  /// `agentflow_llm::counter_for_model` for the family mapping.
  pub fn for_model(model_id: &str) -> Self {
    Self {
      inner: counter_for_model(model_id),
    }
  }

  /// Stable name of the underlying tokenizer
  /// (`"tiktoken/cl100k_base"`, `"heuristic/4-chars"`, etc.). Used
  /// in telemetry / tests to verify the right counter was picked.
  pub fn name(&self) -> &'static str {
    self.inner.name()
  }
}

impl MemoryTokenCounter for LlmTokenCounter {
  fn count_tokens(&self, text: &str) -> u32 {
    self.inner.count_tokens(text)
  }
}

/// Convenience constructor that returns the adapter as a boxed
/// memory counter so call sites can stash it on the agent state
/// behind one type.
pub fn build_message_counter(model_id: &str) -> Box<dyn MemoryTokenCounter> {
  Box::new(LlmTokenCounter::for_model(model_id))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn adapter_routes_to_real_tiktoken_for_openai_family() {
    let counter = LlmTokenCounter::for_model("gpt-4o-mini");
    assert_eq!(counter.name(), "tiktoken/o200k_base");
    // "hello world" is 2 tokens under o200k_base — pinned in
    // `agentflow_llm::tokenizer::tests`.
    assert_eq!(counter.count_tokens("hello world"), 2);
  }

  #[test]
  fn adapter_falls_back_to_heuristic_for_non_bpe_families() {
    let counter = LlmTokenCounter::for_model("claude-sonnet-4-6");
    assert_eq!(counter.name(), "heuristic/4-chars");
    // "hello world" is 11 bytes / 4 = 2 tokens under the
    // heuristic.
    assert_eq!(counter.count_tokens("hello world"), 2);
  }

  #[test]
  fn adapter_satisfies_memory_counter_trait_via_dyn() {
    // The signature `Message::new_with_counter` takes
    // `&dyn agentflow_memory::TokenCounter`. The adapter
    // implements that trait via the impl above. This test pins
    // the trait routing so a future refactor of either side
    // catches the breakage at compile time.
    let counter: Box<dyn MemoryTokenCounter> = build_message_counter("gpt-4o-mini");
    let n = counter.count_tokens("hello world");
    assert_eq!(n, 2);
  }
}
