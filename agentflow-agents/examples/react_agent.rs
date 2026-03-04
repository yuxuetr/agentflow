//! # ReAct Agent Example
//!
//! Demonstrates building and running a [`ReActAgent`] with:
//! - Built-in file and HTTP tools
//! - In-memory session storage
//! - Persistent SQLite memory (optional, uncomment to enable)
//!
//! Run with:
//! ```bash
//! cargo run --example react_agent -p agentflow-agents
//! ```
//!
//! Set your LLM provider key first, e.g.:
//! ```bash
//! export OPENAI_API_KEY=sk-...
//! ```

use std::sync::Arc;

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_memory::SessionMemory;
use agentflow_tools::builtin::{FileTool, HttpTool};
use agentflow_tools::{SandboxPolicy, ToolRegistry};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ─────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    // ── Initialise AgentFlow (loads provider config from env / config file) ─
    agentflow_llm::AgentFlow::init().await?;

    // ── Build a sandbox policy ───────────────────────────────────────────────
    let policy = Arc::new(SandboxPolicy {
        // Allow reading any path under /tmp for the demo
        allowed_paths: vec![std::path::PathBuf::from("/tmp")],
        // Allow HTTP requests to httpbin.org
        allowed_domains: vec!["httpbin.org".to_string()],
        ..SandboxPolicy::default()
    });

    // ── Register tools ───────────────────────────────────────────────────────
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FileTool::new(policy.clone())));
    registry.register(Arc::new(HttpTool::new(policy.clone())));
    let registry = Arc::new(registry);

    // ── In-memory session store (sliding 4 096-token window) ─────────────────
    let memory = Box::new(SessionMemory::default_window());

    // ── (Optional) persistent SQLite memory — uncomment to persist history ───
    // use agentflow_memory::SqliteMemory;
    // let memory = Box::new(SqliteMemory::open("/tmp/agentflow_demo.db").await?);

    // ── Create the agent ─────────────────────────────────────────────────────
    let config = ReActConfig::new("gpt-4o")
        .with_persona(
            "You are a helpful assistant that can read files and make HTTP requests. \
             Always reason step-by-step before acting.",
        )
        .with_max_iterations(10)
        .with_budget_tokens(20_000);

    let mut agent = ReActAgent::new(config, memory, registry);

    println!("Session ID: {}", agent.session_id);
    println!("─────────────────────────────────────────");

    // ── First turn ───────────────────────────────────────────────────────────
    let question = "Fetch https://httpbin.org/get and summarise the response in one sentence.";
    println!("User: {}", question);
    let answer = agent.run(question).await?;
    println!("Agent: {}\n", answer);

    // ── Second turn (multi-turn conversation) ────────────────────────────────
    let question2 = "What was the URL you fetched in your previous response?";
    println!("User: {}", question2);
    let answer2 = agent.run(question2).await?;
    println!("Agent: {}\n", answer2);

    // ── Token usage ──────────────────────────────────────────────────────────
    let tokens = agent.token_count().await?;
    println!("Estimated session tokens used: {}", tokens);

    Ok(())
}
