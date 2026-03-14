//! Multi-agent supervisor: one orchestrator agent that delegates to sub-agents
//! registered as [`AgentTool`](crate::tools::AgentTool)s.
//!
//! # Example
//! ```rust,no_run
//! use agentflow_agents::supervisor::SupervisorBuilder;
//! use agentflow_agents::react::{ReActAgent, ReActConfig};
//! use agentflow_memory::SessionMemory;
//! use agentflow_tools::ToolRegistry;
//! use std::sync::Arc;
//!
//! let sub = ReActAgent::new(
//!     ReActConfig::new("gpt-4o"),
//!     Box::new(SessionMemory::default_window()),
//!     Arc::new(ToolRegistry::new()),
//! );
//!
//! let mut supervisor = SupervisorBuilder::new("gpt-4o")
//!     .add_sub_agent("researcher", "Find factual information", sub)
//!     .build();
//!
//! // supervisor.run("...").await.unwrap();
//! ```

use std::sync::Arc;

use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;

use crate::react::{ReActAgent, ReActConfig};
use crate::tools::AgentTool;

// ── Default orchestrator persona ──────────────────────────────────────────────

const DEFAULT_ORCHESTRATOR_PERSONA: &str = "\
You are an orchestrator AI that decomposes complex tasks and delegates them to \
specialised sub-agents via tool calls. You must:\n\
1. Analyse the user's request.\n\
2. Break the task into sub-tasks.\n\
3. Delegate each sub-task to the appropriate sub-agent tool.\n\
4. Synthesise the results into a coherent final answer.\n\
Never attempt to answer from your own knowledge alone when a sub-agent can help.";

// ── Public types ──────────────────────────────────────────────────────────────

/// A supervisor that owns one orchestrator [`ReActAgent`] which can delegate
/// to registered sub-agents as tools.
pub struct Supervisor {
    orchestrator: ReActAgent,
}

impl Supervisor {
    /// Run the orchestrator on a task and return its final answer.
    pub async fn run(&mut self, task: &str) -> Result<String, crate::react::ReActError> {
        self.orchestrator.run(task).await
    }

    /// The unique session ID of the orchestrator agent.
    pub fn session_id(&self) -> &str {
        &self.orchestrator.session_id
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Builder for [`Supervisor`].
///
/// Register sub-agents with [`add_sub_agent`](SupervisorBuilder::add_sub_agent),
/// then call [`build`](SupervisorBuilder::build) to create the supervisor.
pub struct SupervisorBuilder {
    model: String,
    persona: Option<String>,
    sub_agents: Vec<(String, String, ReActAgent)>, // (name, description, agent)
    max_iterations: usize,
    budget_tokens: Option<u32>,
}

impl SupervisorBuilder {
    /// Create a builder for a supervisor that uses `model` as the orchestrator LLM.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            persona: None,
            sub_agents: Vec::new(),
            max_iterations: 20,
            budget_tokens: Some(80_000),
        }
    }

    /// Override the orchestrator model.
    pub fn orchestrator_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Override the orchestrator system persona.
    pub fn orchestrator_persona(mut self, persona: impl Into<String>) -> Self {
        self.persona = Some(persona.into());
        self
    }

    /// Set the maximum number of tool-call iterations for the orchestrator.
    pub fn max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Set the token budget for the orchestrator session.
    pub fn budget_tokens(mut self, tokens: u32) -> Self {
        self.budget_tokens = Some(tokens);
        self
    }

    /// Register a sub-agent.
    ///
    /// * `name` — tool name the orchestrator uses to call this agent.
    /// * `description` — capability description shown in the orchestrator's system prompt.
    /// * `agent` — the [`ReActAgent`] that handles the delegated sub-task.
    pub fn add_sub_agent(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        agent: ReActAgent,
    ) -> Self {
        self.sub_agents.push((name.into(), description.into(), agent));
        self
    }

    /// Build the [`Supervisor`].
    ///
    /// Each registered sub-agent is wrapped in an [`AgentTool`] and inserted
    /// into the orchestrator's [`ToolRegistry`].
    pub fn build(self) -> Supervisor {
        let mut registry = ToolRegistry::new();

        for (name, description, agent) in self.sub_agents {
            registry.register(Arc::new(AgentTool::new(name, description, agent)));
        }

        let persona = self
            .persona
            .unwrap_or_else(|| DEFAULT_ORCHESTRATOR_PERSONA.to_string());

        let config = ReActConfig::new(self.model)
            .with_persona(persona)
            .with_max_iterations(self.max_iterations)
            .with_budget_tokens(self.budget_tokens.unwrap_or(80_000));

        let orchestrator = ReActAgent::new(
            config,
            Box::new(SessionMemory::default_window()),
            Arc::new(registry),
        );

        Supervisor { orchestrator }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agentflow_tools::ToolRegistry;

    fn make_sub_agent() -> ReActAgent {
        ReActAgent::new(
            ReActConfig::new("gpt-4o"),
            Box::new(SessionMemory::default_window()),
            Arc::new(ToolRegistry::new()),
        )
    }

    // ── Builder constructs without panicking ─────────────────────────────────

    #[test]
    fn builder_builds_with_no_sub_agents() {
        let supervisor = SupervisorBuilder::new("gpt-4o").build();
        // session_id is a UUID, so it must be non-empty
        assert!(
            !supervisor.session_id().is_empty(),
            "session_id should be non-empty"
        );
    }

    #[test]
    fn builder_builds_with_one_sub_agent() {
        let supervisor = SupervisorBuilder::new("gpt-4o")
            .add_sub_agent("researcher", "Finds information on the web", make_sub_agent())
            .build();
        assert!(!supervisor.session_id().is_empty());
    }

    #[test]
    fn builder_builds_with_multiple_sub_agents() {
        let supervisor = SupervisorBuilder::new("gpt-4o")
            .add_sub_agent("rust_expert", "Expert in Rust programming", make_sub_agent())
            .add_sub_agent("code_reviewer", "Reviews code for bugs", make_sub_agent())
            .add_sub_agent("tech_writer", "Writes clear documentation", make_sub_agent())
            .build();
        assert!(!supervisor.session_id().is_empty());
    }

    // ── Custom persona and model ──────────────────────────────────────────────

    #[test]
    fn builder_accepts_custom_persona() {
        let supervisor = SupervisorBuilder::new("gpt-4o")
            .orchestrator_persona("You are a strict project manager.")
            .build();
        assert!(!supervisor.session_id().is_empty());
    }

    #[test]
    fn builder_accepts_custom_model() {
        let supervisor = SupervisorBuilder::new("gpt-3.5-turbo")
            .orchestrator_model("claude-3-5-sonnet")
            .build();
        assert!(!supervisor.session_id().is_empty());
    }

    // ── Builder ergonomics ────────────────────────────────────────────────────

    #[test]
    fn builder_chain_is_fluent() {
        let supervisor = SupervisorBuilder::new("gpt-4o")
            .orchestrator_persona("Task decomposer")
            .max_iterations(10)
            .budget_tokens(40_000)
            .add_sub_agent("a", "Agent A", make_sub_agent())
            .add_sub_agent("b", "Agent B", make_sub_agent())
            .build();
        assert!(!supervisor.session_id().is_empty());
    }

    // ── session_id uniqueness ─────────────────────────────────────────────────

    #[test]
    fn two_supervisors_have_different_session_ids() {
        let s1 = SupervisorBuilder::new("gpt-4o").build();
        let s2 = SupervisorBuilder::new("gpt-4o").build();
        assert_ne!(
            s1.session_id(),
            s2.session_id(),
            "each supervisor should have a unique session_id"
        );
    }
}
