//! Blackboard multi-agent example: a researcher writes facts to a shared
//! board, then a writer reads them and produces the final report.
//!
//! # Run
//! ```sh
//! OPENAI_API_KEY=sk-... cargo run --example multi_agent_blackboard -p agentflow-agents
//! ```

use std::sync::Arc;

use agentflow_agents::supervisor::{
  BlackboardReadTool, BlackboardSchedule, BlackboardStop, BlackboardSupervisorBuilder,
  BlackboardWriteTool,
};
use agentflow_agents::{
  AgentContext, AgentRuntime, AgentStepKind,
  react::{ReActAgent, ReActConfig},
};
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;

#[tokio::main]
async fn main() {
  if let Err(e) = agentflow_llm::AgentFlow::init().await {
    eprintln!("LLM init failed (no API key?): {e}");
    return;
  }

  let model = "gpt-4o";
  let researcher_model = model.to_string();
  let writer_model = model.to_string();

  let mut supervisor = BlackboardSupervisorBuilder::new()
    .add_agent(
      "researcher",
      "Gathers facts and writes them to the board.",
      move |bb| {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(BlackboardReadTool::new(bb.clone(), "researcher")));
        registry.register(Arc::new(BlackboardWriteTool::new(bb, "researcher")));
        ReActAgent::new(
          ReActConfig::new(&researcher_model)
            .with_persona(
              "You are a researcher. Gather 2-3 key facts about the topic, then \
             call bb_write(key=\"facts\", value=<json list>). Finish with a \
             one-sentence summary.",
            )
            .with_max_iterations(4),
          Box::new(SessionMemory::default_window()),
          Arc::new(registry),
        )
      },
    )
    .add_agent(
      "writer",
      "Reads the facts and produces the final report.",
      move |bb| {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(BlackboardReadTool::new(bb.clone(), "writer")));
        registry.register(Arc::new(BlackboardWriteTool::new(bb, "writer")));
        ReActAgent::new(
          ReActConfig::new(&writer_model)
            .with_persona(
              "You are a technical writer. Call bb_read(key=\"facts\") to fetch \
             the researcher's findings, then write a concise paragraph and \
             call bb_write(key=\"report\", value=<your paragraph>). Finish \
             with the paragraph as your answer.",
            )
            .with_max_iterations(4),
          Box::new(SessionMemory::default_window()),
          Arc::new(registry),
        )
      },
    )
    .schedule(BlackboardSchedule::Sequential(vec![
      "researcher".into(),
      "writer".into(),
    ]))
    .stop_when(BlackboardStop::KeySet("report".into()))
    .answer_from("report")
    .build()
    .expect("supervisor must build");

  let task = "Write a short report on Rust's borrow checker.";
  let context = AgentContext::new(supervisor.session_id().to_string(), task, model);

  match AgentRuntime::run(&mut supervisor, context).await {
    Ok(result) => {
      println!("=== Stop reason ===\n{:?}", result.stop_reason);
      println!("\n=== Final answer ===");
      println!("{}", result.answer.as_deref().unwrap_or("(no answer)"));
      println!("\n=== Blackboard summary ===");
      for step in &result.steps {
        if let AgentStepKind::BlackboardOp { op, key, agent, .. } = &step.kind {
          println!("{:?} {} by {}", op, key, agent);
        }
      }
    }
    Err(e) => eprintln!("Error: {e}"),
  }
}
