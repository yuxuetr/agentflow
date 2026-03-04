use std::sync::Arc;

use agentflow_llm::{AgentFlow, MultimodalMessage};
use agentflow_memory::{MemoryStore, Message, Role};
use agentflow_tools::ToolRegistry;
use tracing::{debug, info, warn};

use crate::react::parser::AgentResponse;

/// Error type for ReAct agent operations
#[derive(Debug, thiserror::Error)]
pub enum ReActError {
    #[error("LLM error: {0}")]
    LlmError(#[from] agentflow_llm::LLMError),

    #[error("Memory error: {0}")]
    MemoryError(#[from] agentflow_memory::MemoryError),

    #[error("Tool error: {tool} — {message}")]
    ToolError { tool: String, message: String },

    #[error("Max iterations ({0}) reached without a final answer")]
    MaxIterationsReached(usize),

    #[error("Token budget exceeded: {used} / {budget}")]
    BudgetExceeded { used: u32, budget: u32 },
}

/// Configuration for a [`ReActAgent`].
#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// LLM model identifier (e.g. `"gpt-4o"`, `"claude-3-5-sonnet"`)
    pub model: String,

    /// Optional persona / task description prepended to the system prompt.
    pub persona: Option<String>,

    /// Maximum number of tool-call iterations before giving up.
    pub max_iterations: usize,

    /// Stop after the session accumulates more than this many estimated tokens.
    /// `None` disables the token budget guard.
    pub budget_tokens: Option<u32>,

    /// Terminate if any of these strings appear in the LLM response.
    pub stop_conditions: Vec<String>,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            persona: None,
            max_iterations: 15,
            budget_tokens: Some(50_000),
            stop_conditions: vec![],
        }
    }
}

impl ReActConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn with_persona(mut self, persona: impl Into<String>) -> Self {
        self.persona = Some(persona.into());
        self
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    pub fn with_budget_tokens(mut self, tokens: u32) -> Self {
        self.budget_tokens = Some(tokens);
        self
    }

    pub fn with_stop_conditions(mut self, conditions: Vec<String>) -> Self {
        self.stop_conditions = conditions;
        self
    }
}

/// An autonomous ReAct (Reasoning + Acting) agent.
///
/// On each call to [`run`], the agent:
/// 1. Stores the user message in memory.
/// 2. Iterates: builds a prompt from memory, calls the LLM, parses the response.
/// 3. If the LLM returns a tool call, executes it and appends the result to memory.
/// 4. If the LLM returns a final answer, stores it and returns.
///
/// ## Example
/// ```rust,no_run
/// use agentflow_agents::react::{ReActAgent, ReActConfig};
/// use agentflow_memory::SessionMemory;
/// use agentflow_tools::{ToolRegistry, SandboxPolicy};
/// use agentflow_tools::builtin::ShellTool;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     agentflow_llm::AgentFlow::init().await.unwrap();
///
///     let policy = Arc::new(SandboxPolicy::default());
///     let mut registry = ToolRegistry::new();
///     registry.register(Arc::new(ShellTool::new(policy)));
///
///     let mut agent = ReActAgent::new(
///         ReActConfig::new("gpt-4o"),
///         Box::new(SessionMemory::default_window()),
///         Arc::new(registry),
///     );
///
///     let answer = agent.run("What is today's date?").await.unwrap();
///     println!("{}", answer);
/// }
/// ```
pub struct ReActAgent {
    config: ReActConfig,
    memory: Box<dyn MemoryStore>,
    tools: Arc<ToolRegistry>,
    /// Stable identifier for this agent's conversation session
    pub session_id: String,
}

impl ReActAgent {
    pub fn new(
        config: ReActConfig,
        memory: Box<dyn MemoryStore>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        Self {
            config,
            memory,
            tools,
            session_id,
        }
    }

    /// Continue an existing session by reusing a known `session_id`.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = session_id.into();
        self
    }

    /// Run the agent on a new user message and return the final answer.
    pub async fn run(&mut self, user_input: &str) -> Result<String, ReActError> {
        info!(
            session = %self.session_id,
            model = %self.config.model,
            "ReActAgent starting"
        );

        // 1. Store user message
        self.memory
            .add_message(Message::user(&self.session_id, user_input))
            .await?;

        // 2. Inject system prompt if this is the first user message
        // (We prepend it to the conversation each time we call the LLM)
        let system_prompt = self.build_system_prompt();

        let mut iteration = 0;

        loop {
            // --- Guard: max iterations ---
            if iteration >= self.config.max_iterations {
                return Err(ReActError::MaxIterationsReached(self.config.max_iterations));
            }

            // --- Guard: token budget ---
            if let Some(budget) = self.config.budget_tokens {
                let used = self.memory.session_token_count(&self.session_id).await?;
                if used > budget {
                    return Err(ReActError::BudgetExceeded { used, budget });
                }
            }

            // --- Build LLM messages from memory ---
            let messages = self.build_llm_messages(&system_prompt).await?;

            // --- Call LLM ---
            debug!(iteration, "Calling LLM");
            let raw_response = AgentFlow::model(&self.config.model)
                .multimodal_messages(messages)
                .execute()
                .await?;

            debug!(response = %raw_response, "LLM responded");

            // --- Check stop conditions ---
            for cond in &self.config.stop_conditions {
                if raw_response.contains(cond.as_str()) {
                    info!("Stop condition matched: '{}'", cond);
                    self.memory
                        .add_message(Message::assistant(&self.session_id, &raw_response))
                        .await?;
                    return Ok(raw_response);
                }
            }

            // --- Parse response ---
            let parsed = AgentResponse::parse(&raw_response);

            // Store the assistant turn
            self.memory
                .add_message(Message::assistant(&self.session_id, &raw_response))
                .await?;

            match parsed {
                AgentResponse::Action {
                    thought,
                    tool,
                    params,
                } => {
                    info!(iteration, tool = %tool, thought = %thought, "Tool call");

                    let tool_output = match self.tools.execute(&tool, params).await {
                        Ok(out) => out,
                        Err(e) => {
                            warn!(tool = %tool, error = %e, "Tool execution failed");
                            agentflow_tools::ToolOutput::error(e.to_string())
                        }
                    };

                    let observation = if tool_output.is_error {
                        format!("[ERROR] {}", tool_output.content)
                    } else {
                        tool_output.content
                    };

                    info!(tool = %tool, "Observation: {}", &observation[..observation.len().min(200)]);

                    self.memory
                        .add_message(Message::tool_result(
                            &self.session_id,
                            &tool,
                            &observation,
                        ))
                        .await?;

                    iteration += 1;
                }

                AgentResponse::Answer { thought, answer } => {
                    info!(thought = %thought, "Final answer reached");
                    return Ok(answer);
                }

                AgentResponse::Malformed(text) => {
                    // Treat unstructured text as a final answer
                    warn!("LLM returned non-JSON text; treating as final answer");
                    return Ok(text);
                }
            }
        }
    }

    /// Build the system prompt injected at the start of every LLM call.
    fn build_system_prompt(&self) -> String {
        let persona = self
            .config
            .persona
            .as_deref()
            .unwrap_or("You are a helpful autonomous AI assistant.");

        let tools_desc = self.tools.prompt_tools_description();
        let has_tools = !tools_desc.is_empty();

        let tools_section = if has_tools {
            format!(
                "\n\n## Available Tools\n{}\n\n\
                To call a tool, respond ONLY with this JSON:\n\
                {{\"thought\": \"<your reasoning>\", \"action\": {{\"tool\": \"<tool_name>\", \"params\": {{<parameters>}}}}}}\n",
                tools_desc
            )
        } else {
            String::new()
        };

        format!(
            "{}{}\n\
            To give a final answer, respond ONLY with this JSON:\n\
            {{\"thought\": \"<your final reasoning>\", \"answer\": \"<your answer>\"}}\n\n\
            Respond ONLY with valid JSON matching one of the formats above. \
            No additional text, no markdown, no explanation outside the JSON.",
            persona, tools_section
        )
    }

    /// Assemble the full message list to send to the LLM.
    async fn build_llm_messages(
        &self,
        system_prompt: &str,
    ) -> Result<Vec<MultimodalMessage>, ReActError> {
        let history = self
            .memory
            .get_all(&self.session_id)
            .await?;

        let mut messages = Vec::with_capacity(history.len() + 1);

        // Always start with the system prompt
        messages.push(
            MultimodalMessage::system()
                .add_text(system_prompt)
                .build(),
        );

        // Map memory roles to LLM message roles
        for msg in &history {
            let llm_msg = match msg.role {
                Role::System => continue, // Skip — we inject our own system prompt
                Role::User => MultimodalMessage::user().add_text(&msg.content).build(),
                Role::Assistant => {
                    MultimodalMessage::assistant()
                        .add_text(&msg.content)
                        .build()
                }
                Role::Tool => {
                    // Represent tool results as user messages with a clear prefix
                    let tool_name = msg.tool_name.as_deref().unwrap_or("tool");
                    let content = format!("[Tool Result: {}]\n{}", tool_name, msg.content);
                    MultimodalMessage::user().add_text(&content).build()
                }
            };
            messages.push(llm_msg);
        }

        Ok(messages)
    }

    /// Clear the current session's memory.
    pub async fn reset(&mut self) -> Result<(), ReActError> {
        self.memory.clear_session(&self.session_id).await?;
        self.session_id = uuid::Uuid::new_v4().to_string();
        Ok(())
    }

    /// Estimated tokens used in the current session.
    pub async fn token_count(&self) -> Result<u32, ReActError> {
        Ok(self.memory.session_token_count(&self.session_id).await?)
    }
}
