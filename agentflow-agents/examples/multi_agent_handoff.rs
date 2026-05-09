//! Handoff multi-agent example: a customer-service triage agent transfers
//! the conversation to a billing or technical specialist via a shared
//! `handoff` tool.
//!
//! # Run
//! ```sh
//! OPENAI_API_KEY=sk-... cargo run --example multi_agent_handoff -p agentflow-agents
//! ```
//!
//! Requires a live LLM API key. Set `OPENAI_API_KEY` (or any other provider
//! you have configured under `~/.agentflow/models.yml`).

use std::sync::Arc;

use agentflow_agents::supervisor::HandoffSupervisorBuilder;
use agentflow_agents::{
  AgentContext, AgentEvent, AgentRuntime, AgentStepKind, RuntimeLimits,
  react::{ReActAgent, ReActConfig},
};
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;

#[tokio::main]
async fn main() {
  if let Err(e) = agentflow_llm::AgentFlow::init().await {
    eprintln!("LLM init failed (no API key?): {e}");
    eprintln!("Set OPENAI_API_KEY to run this example against a live model.");
    return;
  }

  let model = "gpt-4o";
  let triage_model = model.to_string();
  let billing_model = model.to_string();
  let tech_model = model.to_string();

  // Each closure receives the shared HandoffTool and is responsible for
  // putting it into the agent's tool registry alongside the agent's own
  // domain tools.
  let mut supervisor = HandoffSupervisorBuilder::new()
    .add_agent(
      "triage",
      "Front-desk agent that classifies the user's request and hands off \
       to the right specialist.",
      move |handoff| {
        let mut registry = ToolRegistry::new();
        registry.register(handoff);
        ReActAgent::new(
          ReActConfig::new(&triage_model)
            .with_persona(
              "You are a customer-service triage agent. Identify whether the \
               user has a billing or a technical issue and call \
               handoff(to=\"billing\"|\"tech\", message=...) immediately. Do \
               not try to solve the issue yourself.",
            )
            .with_max_iterations(3),
          Box::new(SessionMemory::default_window()),
          Arc::new(registry),
        )
      },
    )
    .add_agent(
      "billing",
      "Specialist for refunds, payments, and account disputes.",
      move |handoff| {
        let mut registry = ToolRegistry::new();
        registry.register(handoff);
        ReActAgent::new(
          ReActConfig::new(&billing_model)
            .with_persona(
              "You are a billing specialist. Resolve the user's billing issue \
               concisely. If the request is technical, hand off to \
               handoff(to=\"tech\", ...). Otherwise produce the final answer.",
            )
            .with_max_iterations(4),
          Box::new(SessionMemory::default_window()),
          Arc::new(registry),
        )
      },
    )
    .add_agent(
      "tech",
      "Specialist for product, integration, and outage questions.",
      move |handoff| {
        let mut registry = ToolRegistry::new();
        registry.register(handoff);
        ReActAgent::new(
          ReActConfig::new(&tech_model)
            .with_persona(
              "You are a technical support specialist. Diagnose and resolve \
               product/integration questions. If the request is about \
               billing, hand off via handoff(to=\"billing\", ...).",
            )
            .with_max_iterations(4),
          Box::new(SessionMemory::default_window()),
          Arc::new(registry),
        )
      },
    )
    .initial_agent("triage")
    .max_handoffs(3)
    .build()
    .expect("supervisor must build");

  println!("Supervisor session: {}", supervisor.session_id());

  let task = "Hi, I was charged twice for last month's subscription. Can you \
              refund the duplicate charge?";

  let context = AgentContext::new(supervisor.session_id().to_string(), task, model).with_limits(
    RuntimeLimits {
      max_steps: Some(20),
      ..RuntimeLimits::default()
    },
  );

  match AgentRuntime::run(&mut supervisor, context).await {
    Ok(result) => {
      println!("\n=== Final answer ===");
      println!("{}", result.answer.as_deref().unwrap_or("(no answer)"));
      println!("\nStop reason: {:?}", result.stop_reason);

      println!("\n=== Handoff chain ===");
      let mut chain = vec!["triage".to_string()];
      for step in &result.steps {
        if let AgentStepKind::Handoff { from: _, to, .. } = &step.kind {
          chain.push(to.clone());
        }
      }
      println!("{}", chain.join(" -> "));

      println!("\n=== Events ===");
      for event in &result.events {
        if let AgentEvent::HandoffOccurred { from, to, .. } = event {
          println!("handoff: {from} -> {to}");
        }
      }
    }
    Err(e) => eprintln!("\n=== Error ===\n{e}"),
  }
}
