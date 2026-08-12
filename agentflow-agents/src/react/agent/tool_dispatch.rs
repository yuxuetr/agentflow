use std::time::Instant;

use agentflow_async_util::{RaceOutcome, race_with_limits};
use agentflow_memory::Message;
use chrono::Utc;
use tracing::{info, warn};

use crate::reflection::ReflectionContext;
use crate::runtime::{
  AgentCancellationToken, AgentEvent, AgentStep, AgentStepKind, AgentStopReason,
};

use super::config::{LoopDetectionConfig, ReActError};
use super::core::{ReActAgent, emit_and_push, push_step};
use super::support::{
  annotate_tool_params_for_resume, is_cancelled, remaining_timeout, tool_event_metadata,
  truncate_str_at_char_boundary,
};
use super::turn_driven::{ToolExecOutcome, TurnStep};

impl ReActAgent {
  /// Execute one tool call under the run's timeout/cancellation limits,
  /// racing the tool future against the deadline and the cancellation
  /// token. Returns `Output(tool_output)` on completion (success or a
  /// tool-level error, which is surfaced as an error `ToolOutput`), or
  /// `Stop(result)` when the run must terminate (timeout / cancellation).
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 3b): the gnarly tool-execute `select!` block lifted out of the
  /// `Action` arm, mirroring `run_turn_llm_call`. The future is created
  /// inside so the borrow of `self.tools` does not outlive this call.
  /// `steps`/`events` are consumed (via `mem::take`) only on stop paths.
  #[allow(clippy::too_many_arguments)]
  async fn execute_tool_with_limits(
    &self,
    tool: &str,
    params: serde_json::Value,
    tool_step_index: usize,
    tool_source: &Option<String>,
    tool_permissions: &[String],
    started_at: Instant,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: &Option<AgentCancellationToken>,
  ) -> Result<ToolExecOutcome, ReActError> {
    let tool_call = self.tools.execute(tool, params);
    // Race the tool execution against the wall-clock budget and the
    // cancellation token. On a limit, both the timeout and cancel paths emit a
    // failed `ToolCallCompleted` event before stopping the run.
    let cancel = cancellation_token.as_ref().map(|token| token.cancelled());
    let tool_output = match race_with_limits(
      tool_call,
      remaining_timeout(run_started_at, timeout_ms),
      cancel,
    )
    .await
    {
      RaceOutcome::Completed(Ok(out)) => out,
      // W0.5: `PolicyDeniedAndStop` means the approval layer wants the
      // whole run to stop, not just this call skipped — every other
      // `Err` here degrades to an observation the LLM sees and can
      // route around; this one must actually end the loop, otherwise
      // `DenyAndStop` only ever stalls into `MaxSteps`/`MaxToolCalls`
      // once every remaining tool call in the session gets the same
      // denial (see the stop-after-deny gate in
      // `agentflow-harness::hooks_runtime`).
      RaceOutcome::Completed(Err(agentflow_tool::ToolError::PolicyDeniedAndStop { message })) => {
        let duration_ms = started_at.elapsed().as_millis() as u64;
        emit_and_push!(
          self.live_sink,
          events,
          AgentEvent::ToolCallCompleted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.to_string(),
            is_error: true,
            duration_ms,
            source: tool_source.clone(),
            permissions: tool_permissions.to_vec(),
            timestamp: Utc::now(),
          }
        );
        return Ok(ToolExecOutcome::Stop(Self::stopped_result(
          &self.session_id,
          None,
          AgentStopReason::ApprovalDenied { message },
          std::mem::take(steps),
          std::mem::take(events),
        )));
      }
      RaceOutcome::Completed(Err(e)) => {
        warn!(tool = %tool, error = %e, "Tool execution failed");
        agentflow_tool::ToolOutput::error(e.to_string())
      }
      RaceOutcome::TimedOut => {
        let duration_ms = started_at.elapsed().as_millis() as u64;
        emit_and_push!(
          self.live_sink,
          events,
          AgentEvent::ToolCallCompleted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.to_string(),
            is_error: true,
            duration_ms,
            source: tool_source.clone(),
            permissions: tool_permissions.to_vec(),
            timestamp: Utc::now(),
          }
        );
        self
          .record_reflection(
            ReflectionContext::failure(
              &self.session_id,
              *step_index,
              format!(
                "runtime timed out after {}ms",
                timeout_ms.unwrap_or_default()
              ),
            ),
            step_index,
            steps,
            events,
          )
          .await?;
        return Ok(ToolExecOutcome::Stop(Self::stopped_result(
          &self.session_id,
          None,
          AgentStopReason::Timeout {
            timeout_ms: timeout_ms.unwrap_or_default(),
          },
          std::mem::take(steps),
          std::mem::take(events),
        )));
      }
      RaceOutcome::Cancelled => {
        emit_and_push!(
          self.live_sink,
          events,
          AgentEvent::ToolCallCompleted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.to_string(),
            is_error: true,
            duration_ms: started_at.elapsed().as_millis() as u64,
            source: tool_source.clone(),
            permissions: tool_permissions.to_vec(),
            timestamp: Utc::now(),
          }
        );
        return Ok(ToolExecOutcome::Stop(Self::cancelled_result(
          &self.session_id,
          "cancellation token signalled",
          std::mem::take(steps),
          std::mem::take(events),
        )));
      }
    };
    Ok(ToolExecOutcome::Output(tool_output))
  }

  /// Process one `AgentResponse::Action`: the max-tool-call guard, the
  /// plan step, tool policy/capability events, the tool execution (via
  /// [`Self::execute_tool_with_limits`]), the result step + observation,
  /// the F-A2-13 repeat-call steering note, and the memory write.
  /// Returns `TurnStep::Continue` to advance to the next turn, or
  /// `TurnStep::Stop` on a terminal condition (max tool calls / cancel /
  /// timeout).
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 3c): the `Action` arm body lifted whole out of the loop. Pure
  /// relocation; `steps`/`events` are consumed via `mem::take` only on
  /// stop paths, and the loop now owns the `iteration += 1` increment.
  #[allow(clippy::too_many_arguments)]
  pub(crate) async fn dispatch_single_tool_call(
    &mut self,
    thought: String,
    tool: String,
    params: serde_json::Value,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    tool_calls: &mut usize,
    last_tool_call: &mut Option<(String, serde_json::Value)>,
    recent_tool_calls: &mut std::collections::VecDeque<(String, serde_json::Value)>,
    loop_detection: Option<LoopDetectionConfig>,
    iteration: usize,
    max_tool_calls: Option<usize>,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: &Option<AgentCancellationToken>,
  ) -> Result<TurnStep, ReActError> {
    info!(iteration, tool = %tool, thought = %thought, "Tool call");
    // F-A2-13: detect the (tool, params) == previous call shape BEFORE
    // we touch `params` (it gets moved into `self.tools.execute` later).
    let is_repeat_tool_call = matches!(
      &*last_tool_call,
      Some((prev_tool, prev_params))
        if prev_tool == &tool && prev_params == &params
    );
    if is_repeat_tool_call {
      warn!(
        iteration,
        tool = %tool,
        "Repeat tool call detected (identical params as prior iteration); appending steering note (F-A2-13)"
      );
    }
    if let Some(max_tool_calls) = max_tool_calls
      && *tool_calls >= max_tool_calls
    {
      self
        .record_reflection(
          ReflectionContext::failure(
            &self.session_id,
            *step_index,
            format!("max tool calls ({}) reached", max_tool_calls),
          ),
          step_index,
          steps,
          events,
        )
        .await?;
      return Ok(TurnStep::Stop(Self::stopped_result(
        &self.session_id,
        None,
        AgentStopReason::MaxToolCalls { max_tool_calls },
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    if !thought.trim().is_empty() {
      push_step!(
        self.live_sink,
        steps,
        events,
        self.session_id,
        *step_index,
        AgentStepKind::Plan {
          thought: thought.clone(),
        }
      );
      *step_index += 1;
    }

    if is_cancelled(cancellation_token) {
      return Ok(TurnStep::Stop(Self::cancelled_result(
        &self.session_id,
        "cancellation token signalled",
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    let tool_step_index = *step_index;
    let metadata = self.tools.tool_metadata(&tool);
    let (tool_source, tool_permissions) = tool_event_metadata(metadata.as_ref());
    let trace_params =
      annotate_tool_params_for_resume(params.clone(), self.tools.tool_idempotency(&tool, &params));
    if let Ok(decision) = self.tools.evaluate_policy(&tool, &params) {
      events.push(AgentEvent::ToolPolicyDecision {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        allowed: decision.allowed,
        matched_rule: decision.matched_rule,
        deny_reason: decision.deny_reason,
        source: decision.source,
        permissions: decision.permissions,
        params_summary: decision.params_summary,
        timestamp: Utc::now(),
      });
    }
    if let Ok(effective) = self.tools.evaluate_capabilities(&tool) {
      events.push(AgentEvent::ToolCapabilityDecision {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        allowed: effective.allowed,
        required: effective.required,
        effective: effective.effective,
        denied: effective.denied,
        deny_reason: effective.deny_reason,
        trace: effective.trace,
        sandbox: effective.sandbox,
        timestamp: Utc::now(),
      });
    }
    emit_and_push!(
      self.live_sink,
      events,
      AgentEvent::ToolCallStarted {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        params: trace_params.clone(),
        source: tool_source.clone(),
        permissions: tool_permissions.clone(),
        timestamp: Utc::now(),
      }
    );
    push_step!(
      self.live_sink,
      steps,
      events,
      self.session_id,
      tool_step_index,
      AgentStepKind::ToolCall {
        tool: tool.clone(),
        params: trace_params,
      }
    );
    *step_index += 1;

    let started_at = std::time::Instant::now();
    // F-A2-13: snapshot now so we can compare on the next iteration even
    // after `params` moves into `execute`.
    let params_snapshot = params.clone();
    let tool_output = match self
      .execute_tool_with_limits(
        &tool,
        params,
        tool_step_index,
        &tool_source,
        &tool_permissions,
        started_at,
        steps,
        events,
        step_index,
        run_started_at,
        timeout_ms,
        cancellation_token,
      )
      .await?
    {
      ToolExecOutcome::Output(output) => output,
      ToolExecOutcome::Stop(result) => return Ok(TurnStep::Stop(result)),
    };
    *tool_calls += 1;
    let duration_ms = started_at.elapsed().as_millis() as u64;

    let observation = if tool_output.is_error {
      format!("[ERROR] {}", tool_output.content)
    } else {
      tool_output.content.clone()
    };

    info!(
      tool = %tool,
      "Observation: {}",
      truncate_str_at_char_boundary(&observation, 200)
    );
    push_step!(
      self.live_sink,
      steps,
      events,
      self.session_id,
      *step_index,
      AgentStepKind::ToolResult {
        tool: tool.clone(),
        content: tool_output.content.clone(),
        is_error: tool_output.is_error,
        parts: tool_output.parts.clone(),
      }
    );
    emit_and_push!(
      self.live_sink,
      events,
      AgentEvent::ToolCallCompleted {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        is_error: tool_output.is_error,
        duration_ms,
        source: tool_source.clone(),
        permissions: tool_permissions.clone(),
        timestamp: Utc::now(),
      }
    );
    *step_index += 1;
    if tool_output.is_error {
      self
        .record_reflection(
          ReflectionContext::failure(&self.session_id, *step_index, &observation),
          step_index,
          steps,
          events,
        )
        .await?;
    }

    // F-A2-13: when this iteration is a repeat of the prior, append a
    // steering note ONLY to the memory the model sees on its next turn.
    let observation_for_memory = if is_repeat_tool_call {
      format!(
        "{observation}\n\n\
         [agentflow steering note (F-A2-13): this is your 2nd consecutive call to tool `{tool}` with identical parameters. The observation above is unchanged from the prior call — calling `{tool}` again with these params will not yield new information. To make progress, choose one of: (a) draw conclusions from the observation and emit a final answer, (b) call a different tool, or (c) call `{tool}` with materially different parameters.]"
      )
    } else {
      observation.clone()
    };

    self
      .add_memory_message(Message::tool_result_with_counter(
        &self.session_id,
        &tool,
        &observation_for_memory,
        &*self.message_counter,
      ))
      .await?;

    // Track the call so the next iteration's check can run.
    *last_tool_call = Some((tool.clone(), params_snapshot.clone()));

    // L1.2: feed the sliding window; checked at the top of the next turn.
    if let Some(cfg) = loop_detection {
      recent_tool_calls.push_back((tool.clone(), params_snapshot));
      while recent_tool_calls.len() > cfg.window {
        recent_tool_calls.pop_front();
      }
    }

    Ok(TurnStep::Continue)
  }
}
