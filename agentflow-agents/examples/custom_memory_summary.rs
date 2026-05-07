//! Custom memory-summary backend example.
//!
//! Demonstrates the `MemorySummaryBackend` extension point. The backend is
//! invoked by the ReAct loop only when the prompt-memory token estimate
//! exceeds the configured `memory_prompt_token_budget`. To make the example
//! deterministic and dependency-free, we exercise the backend directly via
//! a constructed `MemorySummaryContext` — the same input the runtime would
//! supply at run time. The final block shows how to wire the backend into
//! a `ReActAgent` for production use.
//!
//! Run:
//!
//! ```sh
//! cargo run -p agentflow-agents --example custom_memory_summary
//! ```

use std::sync::Arc;

use agentflow_agents::react::{
  CompactMemorySummary, MemorySummaryBackend, MemorySummaryContext, MemorySummaryStrategy,
  ReActAgent, ReActConfig, ReActError,
};
use agentflow_memory::{Message, SessionMemory};
use agentflow_tools::ToolRegistry;
use async_trait::async_trait;

// ── Custom backend: keeps only one bullet per omitted speaker ─────────────

/// A deterministic backend that condenses every omitted message to a single
/// `<role>: <head>...` bullet, capped at `head_chars` characters per line.
struct BulletSummaryBackend {
  head_chars: usize,
}

impl BulletSummaryBackend {
  fn new(head_chars: usize) -> Self {
    Self { head_chars }
  }
}

#[async_trait]
impl MemorySummaryBackend for BulletSummaryBackend {
  fn name(&self) -> &'static str {
    "bullet_summary"
  }

  async fn summarize(&self, context: MemorySummaryContext) -> Result<Option<String>, ReActError> {
    if context.omitted_messages.is_empty() {
      return Ok(None);
    }
    let mut lines = Vec::with_capacity(context.omitted_messages.len() + 1);
    lines.push(format!(
      "[Memory Summary] {} older messages condensed (~{} tokens, budget {}).",
      context.omitted_messages.len(),
      context.omitted_tokens,
      context.budget_tokens,
    ));
    for message in &context.omitted_messages {
      let head: String = message.content.chars().take(self.head_chars).collect();
      let suffix = if message.content.chars().count() > self.head_chars {
        "…"
      } else {
        ""
      };
      lines.push(format!("- {:?}: {head}{suffix}", message.role));
    }
    Ok(Some(lines.join("\n")))
  }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  // ── 1. Show the trait contract directly. ────────────────────────────────
  let session_id = "custom-memory-summary-example".to_string();
  let mut older1 = Message::user(&session_id, "Long discussion about retrieval strategy");
  older1.token_count = 32;
  let mut older2 = Message::assistant(&session_id, "Recommended hybrid BM25 + dense rerank");
  older2.token_count = 18;
  let mut kept = Message::user(&session_id, "Now please summarise the plan");
  kept.token_count = 6;

  let context = MemorySummaryContext {
    session_id: session_id.clone(),
    budget_tokens: 8,
    omitted_tokens: older1.token_count + older2.token_count,
    omitted_messages: vec![older1, older2],
    kept_messages: vec![kept],
  };

  let backend = BulletSummaryBackend::new(32);
  let summary = backend.summarize(context).await?;
  println!("--- Direct backend invocation ---");
  println!("{}", summary.as_deref().unwrap_or("<no summary>"));

  // ── 2. Show how the same backend plugs into a ReActAgent. ───────────────
  // (This block does not start a run; it just demonstrates the wiring.)
  let registry = Arc::new(ToolRegistry::new());
  let _agent = ReActAgent::new(
    ReActConfig::new("gpt-4o")
      .with_memory_prompt_token_budget(8)
      .with_memory_summary_strategy(MemorySummaryStrategy::Compact),
    Box::new(SessionMemory::default_window()),
    registry,
  )
  .with_memory_summary_backend(Arc::new(BulletSummaryBackend::new(48)));

  println!();
  println!("Agent constructed with bullet_summary backend (no run executed).");
  println!(
    "Built-in alternatives: {} / {} / disabled.",
    agentflow_agents::RecentOnlyMemorySummary.name(),
    CompactMemorySummary.name(),
  );

  Ok(())
}
