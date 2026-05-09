//! Debate multi-agent example: two participants propose answers in parallel
//! about a code-review question; a judge synthesises the verdict.
//!
//! # Run
//! ```sh
//! OPENAI_API_KEY=sk-... cargo run --example multi_agent_debate -p agentflow-agents
//! ```

use std::sync::Arc;

use agentflow_agents::supervisor::DebateSupervisorBuilder;
use agentflow_agents::{
  AgentContext, AgentRuntime, AgentStepKind,
  react::{ReActAgent, ReActConfig},
};
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;

fn agent(model: &str, persona: &str) -> ReActAgent {
  ReActAgent::new(
    ReActConfig::new(model)
      .with_persona(persona)
      .with_max_iterations(3),
    Box::new(SessionMemory::default_window()),
    Arc::new(ToolRegistry::new()),
  )
}

#[tokio::main]
async fn main() {
  if let Err(e) = agentflow_llm::AgentFlow::init().await {
    eprintln!("LLM init failed (no API key?): {e}");
    return;
  }
  let model = "gpt-4o";

  let mut supervisor = DebateSupervisorBuilder::new()
    .add_participant(
      "performance",
      agent(
        model,
        "You are a senior engineer focused on performance. Critique the code \
         purely from a runtime-cost / allocation / branch-prediction angle.",
      ),
    )
    .add_participant(
      "readability",
      agent(
        model,
        "You are a senior engineer focused on readability. Critique the code \
         from a maintainability / naming / abstraction angle.",
      ),
    )
    .judge(agent(
      model,
      "You are a tech-lead judge. Read the proposals and produce a single \
       prioritised review comment that combines both perspectives. Be concise.",
    ))
    .rounds(1)
    .build()
    .expect("supervisor must build");

  let task = r#"
Review this Rust function:

```rust
fn sum_evens(xs: Vec<i32>) -> i32 {
    let mut s = 0;
    for x in &xs {
        if *x % 2 == 0 { s = s + *x; }
    }
    s
}
```
"#;

  let context = AgentContext::new(supervisor.session_id().to_string(), task, model);
  match AgentRuntime::run(&mut supervisor, context).await {
    Ok(result) => {
      println!(
        "=== Verdict ===\n{}\n",
        result.answer.as_deref().unwrap_or("")
      );
      println!("=== Proposals ===");
      for step in &result.steps {
        if let AgentStepKind::DebateProposal {
          round,
          agent,
          proposal,
        } = &step.kind
        {
          println!("[round {round}] {agent}: {proposal}");
        }
      }
    }
    Err(e) => eprintln!("Error: {e}"),
  }
}
