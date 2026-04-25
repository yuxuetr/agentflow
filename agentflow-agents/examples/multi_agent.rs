//! Multi-agent example: a Supervisor orchestrates a `rust_expert` and a
//! `code_reviewer` sub-agent to collaboratively analyse a Rust snippet.
//!
//! # Run
//! ```sh
//! OPENAI_API_KEY=sk-... cargo run --example multi_agent -p agentflow-agents
//! ```
//!
//! This example requires a live OpenAI API key.  If none is set it will fail
//! gracefully at the LLM call site.

use std::sync::Arc;

use agentflow_agents::{
  react::{ReActAgent, ReActConfig},
  supervisor::SupervisorBuilder,
};
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;

#[tokio::main]
async fn main() {
  // ── Initialise LLM client (reads OPENAI_API_KEY from env) ────────────────
  if let Err(e) = agentflow_llm::AgentFlow::init().await {
    eprintln!("LLM init failed (no API key?): {e}");
    eprintln!("Set OPENAI_API_KEY to run this example against a live model.");
    return;
  }

  // ── Sub-agent: Rust expert ───────────────────────────────────────────────
  let rust_expert = ReActAgent::new(
    ReActConfig::new("gpt-4o").with_persona(
      "You are an expert Rust programmer. \
             Answer questions about Rust idioms, ownership, lifetimes, and best practices.",
    ),
    Box::new(SessionMemory::default_window()),
    Arc::new(ToolRegistry::new()),
  );

  // ── Sub-agent: Code reviewer ─────────────────────────────────────────────
  let code_reviewer = ReActAgent::new(
    ReActConfig::new("gpt-4o").with_persona(
      "You are a thorough code reviewer. \
             Identify correctness issues, potential panics, and style violations \
             in the provided Rust code.",
    ),
    Box::new(SessionMemory::default_window()),
    Arc::new(ToolRegistry::new()),
  );

  // ── Supervisor ───────────────────────────────────────────────────────────
  let mut supervisor = SupervisorBuilder::new("gpt-4o")
    .add_sub_agent(
      "rust_expert",
      "Expert in Rust programming — answers questions about idioms and best practices",
      rust_expert,
    )
    .add_sub_agent(
      "code_reviewer",
      "Reviews Rust code for correctness, panics, and style",
      code_reviewer,
    )
    .build();

  println!("Supervisor session: {}", supervisor.session_id());

  // ── Task ─────────────────────────────────────────────────────────────────
  let task = r#"
Analyse the following Rust function and answer two questions:
1. Is the implementation idiomatic Rust?
2. Are there any potential runtime panics or logic errors?

```rust
fn divide(a: i32, b: i32) -> i32 {
    a / b
}
```
"#;

  println!("\n=== Task ===\n{task}");

  match supervisor.run(task).await {
    Ok(answer) => println!("\n=== Supervisor Answer ===\n{answer}"),
    Err(e) => eprintln!("\n=== Error ===\n{e}"),
  }
}
