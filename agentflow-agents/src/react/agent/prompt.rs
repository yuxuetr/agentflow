use agentflow_llm::{MultimodalMessage, ToolSpec};
use agentflow_memory::Role;
use chrono::Utc;

use crate::runtime::{AgentContext, AgentEvent, AgentRunResult, AgentStopReason, RuntimeLimits};

use super::config::{ASK_USER_TOOL_NAME, FINAL_ANSWER_TOOL_NAME, ReActError};
use super::core::ReActAgent;
use super::support::{
  format_preference_for_prompt, format_project_facts_for_prompt, format_task_summary_for_prompt,
};

impl ReActAgent {
  /// `pub(crate)` (rather than private) so [`crate::nodes::AgentNode`] can
  /// build the same config-derived default context this uses internally,
  /// then layer in parent-flow governance (W2.3) before calling
  /// [`Self::run_with_context`] directly instead of [`Self::run_with_trace`].
  pub(crate) fn context_for_input(&self, user_input: &str) -> AgentContext {
    let mut context = AgentContext::new(&self.session_id, user_input, &self.config.model)
      .with_limits(RuntimeLimits {
        max_steps: Some(self.config.max_iterations),
        max_tool_calls: None,
        timeout_ms: None,
        token_budget: self.config.budget_tokens,
        cost_limit_usd: self.config.cost_limit_usd,
      });
    if let Some(persona) = &self.config.persona {
      context = context.with_persona(persona.clone());
    }
    context
  }

  pub(crate) fn apply_context(&mut self, context: &AgentContext) {
    self.session_id = context.session_id.clone();
    if !context.model.trim().is_empty() {
      self.config.model = context.model.clone();
      // Rebuild the per-message tokenizer when the model changes
      // so the precision claims in `apply_memory_prompt_budget`
      // match the model the run actually targets (P10.3.3-FU1).
      self.message_counter = crate::token_counter_adapter::build_message_counter(&context.model);
    }
    if let Some(persona) = &context.persona {
      self.config.persona = Some(persona.clone());
    }
  }

  /// Build a `Vec<ToolSpec>` from the registered tools so it can be passed to
  /// the LLM as a native `tools` field. Returns an empty vector when no
  /// tools are registered, in which case the LLM call is unchanged.
  pub(crate) fn collect_tool_specs(&self) -> Vec<ToolSpec> {
    let mut specs: Vec<ToolSpec> = self
      .tools
      .list()
      .into_iter()
      .map(|tool| ToolSpec::new(tool.name(), tool.description(), tool.parameters_schema()))
      .collect();
    // V2.1: offer the schema-constrained final-answer tool alongside real
    // tools whenever `output_schema` is configured, so providers with
    // native tool calling enforce the shape directly instead of relying on
    // prompt-only constraint. See `FINAL_ANSWER_TOOL_NAME`'s doc comment.
    if let Some(schema) = &self.config.output_schema {
      specs.push(ToolSpec::new(
        FINAL_ANSWER_TOOL_NAME,
        "Call this with your final answer once you have all the information you need. \
         The answer must match the required schema.",
        schema.clone(),
      ));
    }
    // V2.3: unconditional — see ASK_USER_TOOL_NAME's doc comment for why
    // this doesn't need an opt-in config flag the way `final_answer` does.
    specs.push(ToolSpec::new(
      ASK_USER_TOOL_NAME,
      "Call this to ask the user a question when you need information only \
       they can provide, then wait for their answer before continuing. Do \
       not use this for information you can find yourself.",
      serde_json::json!({
        "type": "object",
        "properties": {
          "question": {"type": "string", "description": "The question to ask the user."}
        },
        "required": ["question"]
      }),
    ));
    specs
  }

  pub(crate) fn answer_from_result(result: AgentRunResult) -> Result<String, ReActError> {
    match result.stop_reason {
      AgentStopReason::FinalAnswer | AgentStopReason::StopCondition { .. } => {
        Ok(result.answer.unwrap_or_default())
      }
      AgentStopReason::MaxSteps { max_steps } => Err(ReActError::MaxIterationsReached(max_steps)),
      AgentStopReason::TokenBudgetExceeded { used, budget } => {
        Err(ReActError::BudgetExceeded { used, budget })
      }
      AgentStopReason::MaxToolCalls { max_tool_calls } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!("max tool calls ({}) reached", max_tool_calls),
      }),
      AgentStopReason::Timeout { timeout_ms } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!("timeout after {}ms", timeout_ms),
      }),
      AgentStopReason::Cancelled { message } => Err(ReActError::Cancelled { reason: message }),
      AgentStopReason::CostLimitExceeded {
        used_usd,
        budget_usd,
      } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!(
          "cost limit exceeded: ${:.4} (budget ${:.4})",
          used_usd, budget_usd
        ),
      }),
      AgentStopReason::LoopDetected { tool, repeats } => Err(ReActError::ToolError {
        tool,
        message: format!("loop detected: repeated {repeats} times with identical params"),
      }),
      AgentStopReason::Error { message } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message,
      }),
      // V2.3: a delegated sub-agent has no user to ask — nested HITL
      // through delegation is out of scope; surface as an error to the
      // parent rather than silently bubbling the question up.
      AgentStopReason::AwaitingInput { question } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!(
          "delegated sub-agent asked a question but delegation does not support HITL: {question}"
        ),
      }),
      // W0.5: a delegated sub-agent hit a DenyAndStop tool denial —
      // surface it to the parent the same way as any other terminal
      // runtime failure.
      AgentStopReason::ApprovalDenied { message } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message,
      }),
    }
  }

  /// Build the system prompt injected at the start of every LLM call.
  pub(crate) fn build_system_prompt(&self) -> String {
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

    // V2.1: when `output_schema` is configured, `collect_tool_specs` (used
    // for the native LLM tool-calling channel) offers the `final_answer`
    // tool alongside real tools — providers with native tool calling
    // enforce the shape directly there. This section is the prompt-only
    // fallback: it still applies when a provider doesn't honour native
    // tool calling (e.g. `MockProvider`, or a provider outage), so the
    // textual `answer` JSON convention below stays schema-aware too.
    let schema_section = if let Some(schema) = &self.config.output_schema {
      format!(
        "\n\nYour final answer must conform to this JSON Schema:\n{}\n\
                Prefer calling the `{}` tool with your answer. If no tool-calling channel is \
                available, use the JSON convention below but make `answer` a JSON-encoded \
                string of a value matching this schema.\n",
        serde_json::to_string_pretty(schema).unwrap_or_default(),
        FINAL_ANSWER_TOOL_NAME,
      )
    } else {
      String::new()
    };

    format!(
      "{}{}{}\n\
            To give a final answer, respond ONLY with this JSON:\n\
            {{\"thought\": \"<your final reasoning>\", \"answer\": \"<your answer>\"}}\n\n\
            Respond ONLY with valid JSON matching one of the formats above. \
            No additional text, no markdown, no explanation outside the JSON.",
      persona, tools_section, schema_section
    )
  }

  /// Assemble the full message list to send to the LLM.
  pub(crate) async fn build_llm_messages(
    &self,
    system_prompt: &str,
  ) -> Result<Vec<MultimodalMessage>, ReActError> {
    let history = self.read_memory_history(&self.session_id).await?;
    let (memory_summary, history) = self.apply_memory_prompt_budget(history).await?;

    // Phase 2b (RFC_HARNESS_LOOP_OWNERSHIP): the agent compacts prompt
    // memory every turn when it exceeds budget. Surface that *mid-run*
    // compaction live so the Harness bridge turns it into a
    // `MemorySummaryAdded` envelope — previously this between-turn
    // context engineering was invisible. Live-only (the summary is a
    // transient prompt artifact, not a recorded step); `None` live_sink
    // is a no-op, so non-harness runs are unaffected.
    if let Some(summary) = &memory_summary
      && let Some(handle) = &self.live_sink
    {
      let token_estimate =
        agentflow_llm::tokenizer::count_tokens_for_model(&self.config.model, summary) as usize;
      handle
        .0
        .emit(&AgentEvent::MemorySummaryAdded {
          session_id: self.session_id.clone(),
          layer: "session".to_string(),
          summary: summary.clone(),
          token_estimate,
          timestamp: Utc::now(),
        })
        .await;
    }

    let mut messages = Vec::with_capacity(history.len() + 1);

    // Always start with the system prompt
    messages.push(MultimodalMessage::system().add_text(system_prompt).build());

    // U2.2: user preferences come first among the injected context
    // blocks — per `docs/MEMORY_LAYERING.md`'s stated precedence
    // (Session, then Preference, then Entity facts, then Semantic),
    // preference is "always small, always inserted into the persona."
    // It's about the user, not any one project or session, so it's more
    // foundational than the project-facts/task-summary blocks below.
    if let Some(store) = &self.preference_store
      && let Some(scope) = &self.preference_scope
    {
      let prefs = store.list_preferences(scope).await?;
      if !prefs.is_empty() {
        messages.push(
          MultimodalMessage::system()
            .add_text(format_preference_for_prompt(&prefs))
            .build(),
        );
      }
    }

    // L3.1: project facts (if any) come right after the system prompt —
    // even more foundational than the L2.1 task summary below, since
    // they're project-wide rather than scoped to this one session.
    if let Some(store) = &self.project_memory_store
      && let Some(project_key) = &self.project_key
    {
      let facts = store.get_project_facts(project_key).await?;
      if !facts.is_empty() {
        messages.push(
          MultimodalMessage::system()
            .add_text(format_project_facts_for_prompt(&facts))
            .build(),
        );
      }
    }

    // L2.1: the persisted task-summary checkpoint (if any) comes right
    // after the system prompt and before the transient per-turn
    // compaction note below — it's the durable "big picture," read fresh
    // every turn so a resumed run, or a fresh run reusing this session
    // id, sees it exactly like a run that's been going the whole time.
    if let Some(store) = &self.task_summary_store
      && let Some(summary) = store.get_task_summary(&self.session_id).await?
    {
      messages.push(
        MultimodalMessage::system()
          .add_text(format_task_summary_for_prompt(&summary))
          .build(),
      );
    }

    if let Some(summary) = memory_summary {
      messages.push(MultimodalMessage::system().add_text(summary).build());
    }

    // Map memory roles to LLM message roles
    for msg in &history {
      let llm_msg = match msg.role {
        Role::System => continue, // Skip — we inject our own system prompt
        Role::User => MultimodalMessage::user().add_text(&msg.content).build(),
        Role::Assistant => MultimodalMessage::assistant()
          .add_text(&msg.content)
          .build(),
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
}
