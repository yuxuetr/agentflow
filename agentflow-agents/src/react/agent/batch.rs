use std::time::Instant;

use agentflow_async_util::{RaceOutcome, race_with_limits};
use agentflow_llm::ToolCallRequest;
use agentflow_memory::Message;
use agentflow_tool::ToolIdempotency;
use chrono::Utc;
use serde_json::Value;
use tracing::{info, warn};

use crate::reflection::ReflectionContext;
use crate::runtime::{
  AgentCancellationToken, AgentEvent, AgentRunResult, AgentStep, AgentStepKind, AgentStopReason,
};

use super::config::{LoopDetectionConfig, ReActError};
use super::core::{ReActAgent, emit_and_push, push_step};
use super::support::{
  annotate_tool_params_for_resume, is_cancelled, remaining_timeout, tool_event_metadata,
  truncate_str_at_char_boundary,
};

/// Internal staging record for one tool call in a multi-call batch.
struct PreparedToolCall {
  tool: String,
  params: Value,
  call_step_idx: usize,
  idempotency: ToolIdempotency,
  source: Option<String>,
  permissions: Vec<String>,
}

/// Outcome of `dispatch_native_tool_calls_batch`. `Stop` boxes the
/// full `AgentRunResult` (large struct; boxing keeps the enum
/// variants similarly sized).
pub(crate) enum BatchOutcome {
  Continue,
  Stop(Box<AgentRunResult>),
}

impl ReActAgent {
  /// Dispatch a batch of native tool calls (`>=2`) produced by one
  /// LLM turn (P-H.3). Idempotent calls run concurrently;
  /// `NonIdempotent` / `Unknown` calls run serially, in array order.
  /// `ToolCallStarted` events fire in the LLM-returned array order
  /// before any execution begins; `ToolCallCompleted` and the
  /// `ToolResult` step rows also follow that order so trace replay
  /// remains deterministic across runs.
  #[allow(clippy::too_many_arguments)]
  pub(crate) async fn dispatch_native_tool_calls_batch(
    &mut self,
    tool_calls: &[ToolCallRequest],
    raw_response: &str,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    tool_calls_counter: &mut usize,
    recent_tool_calls: &mut std::collections::VecDeque<(String, serde_json::Value)>,
    loop_detection: Option<LoopDetectionConfig>,
    max_tool_calls: Option<usize>,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: Option<&AgentCancellationToken>,
  ) -> Result<BatchOutcome, ReActError> {
    let n = tool_calls.len();
    debug_assert!(n >= 2, "batch path expects >=2 native tool calls");

    // 1. Max-tool-calls precondition: refuse to start a batch that
    //    would put the counter over the limit. We treat the whole
    //    batch atomically so the agent never sees a partial trace.
    if let Some(max) = max_tool_calls
      && *tool_calls_counter + n > max
    {
      self
        .record_reflection(
          ReflectionContext::failure(
            &self.session_id,
            *step_index,
            format!(
              "batch of {n} tool calls would exceed max_tool_calls={max}; refusing to dispatch"
            ),
          ),
          step_index,
          steps,
          events,
        )
        .await?;
      return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
        &self.session_id,
        None,
        AgentStopReason::MaxToolCalls {
          max_tool_calls: max,
        },
        std::mem::take(steps),
        std::mem::take(events),
      ))));
    }

    // 2. Cancellation precheck.
    if is_cancelled(&cancellation_token.cloned()) {
      return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
        &self.session_id,
        "cancellation token signalled",
        std::mem::take(steps),
        std::mem::take(events),
      ))));
    }

    // 3. Persist the assistant turn that triggered this batch.
    self
      .add_memory_message(Message::assistant_with_counter(
        &self.session_id,
        raw_response,
        &*self.message_counter,
      ))
      .await?;

    // 4. Pre-assign step indexes and emit `ToolPolicyDecision`,
    //    `ToolCapabilityDecision`, and `ToolCallStarted` for every
    //    call before dispatching anything. The trace is therefore
    //    deterministic regardless of completion order.
    let mut prepared: Vec<PreparedToolCall> = Vec::with_capacity(n);
    for call in tool_calls.iter() {
      let metadata = self.tools.tool_metadata(&call.name);
      let idempotency = self
        .tools
        .tool_idempotency(&call.name, &call.arguments)
        .unwrap_or(ToolIdempotency::Unknown);
      let (source, permissions) = tool_event_metadata(metadata.as_ref());
      let trace_params = annotate_tool_params_for_resume(call.arguments.clone(), Some(idempotency));
      let call_step_idx = *step_index;
      *step_index += 1;

      if let Ok(decision) = self.tools.evaluate_policy(&call.name, &call.arguments) {
        events.push(AgentEvent::ToolPolicyDecision {
          session_id: self.session_id.clone(),
          step_index: call_step_idx,
          tool: call.name.clone(),
          allowed: decision.allowed,
          matched_rule: decision.matched_rule,
          deny_reason: decision.deny_reason,
          source: decision.source,
          permissions: decision.permissions,
          params_summary: decision.params_summary,
          timestamp: Utc::now(),
        });
      }
      if let Ok(effective) = self.tools.evaluate_capabilities(&call.name) {
        events.push(AgentEvent::ToolCapabilityDecision {
          session_id: self.session_id.clone(),
          step_index: call_step_idx,
          tool: call.name.clone(),
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
          step_index: call_step_idx,
          tool: call.name.clone(),
          params: trace_params.clone(),
          source: source.clone(),
          permissions: permissions.clone(),
          timestamp: Utc::now(),
        }
      );
      push_step!(
        self.live_sink,
        steps,
        events,
        self.session_id,
        call_step_idx,
        AgentStepKind::ToolCall {
          tool: call.name.clone(),
          params: trace_params,
        }
      );

      // L1.2: feed the sliding window for every call in the batch, in
      // dispatch order; checked at the top of the next turn.
      if let Some(cfg) = loop_detection {
        recent_tool_calls.push_back((call.name.clone(), call.arguments.clone()));
        while recent_tool_calls.len() > cfg.window {
          recent_tool_calls.pop_front();
        }
      }

      prepared.push(PreparedToolCall {
        tool: call.name.clone(),
        params: call.arguments.clone(),
        call_step_idx,
        idempotency,
        source,
        permissions,
      });
    }

    // 5. Partition by idempotency. Idempotent → concurrent group.
    //    Non-idempotent / Unknown → serial group, evaluated in LLM
    //    order. The harness `HookedTool` wrapper is responsible for
    //    approval gating; the agent only worries about safety
    //    relative to repeating the call.
    let concurrent_idxs: Vec<usize> = (0..n)
      .filter(|&i| matches!(prepared[i].idempotency, ToolIdempotency::Idempotent))
      .collect();
    let serial_idxs: Vec<usize> = (0..n)
      .filter(|&i| !matches!(prepared[i].idempotency, ToolIdempotency::Idempotent))
      .collect();

    let mut outputs: Vec<Option<(agentflow_tool::ToolOutput, u64)>> =
      (0..n).map(|_| None).collect();

    // 5a. Concurrent group.
    if !concurrent_idxs.is_empty() {
      let mut futs = Vec::with_capacity(concurrent_idxs.len());
      for &i in &concurrent_idxs {
        let tools = self.tools.clone();
        let tool = prepared[i].tool.clone();
        let params = prepared[i].params.clone();
        let started = Instant::now();
        futs.push(async move {
          let result = tools.execute(&tool, params).await;
          (i, result, started.elapsed().as_millis() as u64)
        });
      }
      let batch_fut = futures::future::join_all(futs);

      let timeout = remaining_timeout(run_started_at, timeout_ms);
      let cancel = cancellation_token.as_ref().map(|token| token.cancelled());
      let result_set = match race_with_limits(batch_fut, timeout, cancel).await {
        RaceOutcome::Completed(results) => Some(results),
        RaceOutcome::TimedOut => {
          self
            .emit_batch_timeout(&prepared, &concurrent_idxs, events)
            .await;
          return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::Timeout {
              timeout_ms: timeout_ms.unwrap_or_default(),
            },
            std::mem::take(steps),
            std::mem::take(events),
          ))));
        }
        RaceOutcome::Cancelled => {
          self
            .emit_batch_cancelled(&prepared, &concurrent_idxs, events)
            .await;
          return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
            &self.session_id,
            "cancellation token signalled",
            std::mem::take(steps),
            std::mem::take(events),
          ))));
        }
      };

      if let Some(results) = result_set {
        // W0.5: a `DenyAndStop` denial can land anywhere in the
        // concurrent group — scan for it before committing outputs, so
        // the whole batch stops instead of the other calls' results
        // quietly winning the race. Only the message is cloned (not
        // the non-`Clone` `ToolError`), so `results` is still movable
        // for the normal-path loop below.
        let stop_message: Option<String> = results.iter().find_map(|(_, result, _)| match result {
          Err(agentflow_tool::ToolError::PolicyDeniedAndStop { message }) => Some(message.clone()),
          _ => None,
        });
        if let Some(message) = stop_message {
          // Emit each call's real outcome/duration (not a synthetic
          // 0ms like the timeout/cancel helpers use) so the trace
          // still reflects what actually happened concurrently.
          for (i, result, dur) in &results {
            emit_and_push!(
              self.live_sink,
              events,
              AgentEvent::ToolCallCompleted {
                session_id: self.session_id.clone(),
                step_index: prepared[*i].call_step_idx,
                tool: prepared[*i].tool.clone(),
                is_error: result.is_err(),
                duration_ms: *dur,
                source: prepared[*i].source.clone(),
                permissions: prepared[*i].permissions.clone(),
                timestamp: Utc::now(),
              }
            );
          }
          return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::ApprovalDenied { message },
            std::mem::take(steps),
            std::mem::take(events),
          ))));
        }

        for (i, result, dur) in results {
          let output = match result {
            Ok(out) => out,
            Err(e) => {
              warn!(tool = %prepared[i].tool, error = %e, "tool execution failed");
              agentflow_tool::ToolOutput::error(e.to_string())
            }
          };
          outputs[i] = Some((output, dur));
        }
      }
    }

    // 5b. Serial group. Each call is independently subject to
    //     cancellation + timeout.
    for &i in &serial_idxs {
      if is_cancelled(&cancellation_token.cloned()) {
        // Skip remaining calls; emit completion events for the rest
        // so the trace stays balanced.
        for &j in serial_idxs.iter().skip_while(|&&j| j != i) {
          if outputs[j].is_none() {
            emit_and_push!(
              self.live_sink,
              events,
              AgentEvent::ToolCallCompleted {
                session_id: self.session_id.clone(),
                step_index: prepared[j].call_step_idx,
                tool: prepared[j].tool.clone(),
                is_error: true,
                duration_ms: 0,
                source: prepared[j].source.clone(),
                permissions: prepared[j].permissions.clone(),
                timestamp: Utc::now(),
              }
            );
          }
        }
        return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
          &self.session_id,
          "cancellation token signalled",
          std::mem::take(steps),
          std::mem::take(events),
        ))));
      }
      let started = Instant::now();
      let tools = self.tools.clone();
      let tool = prepared[i].tool.clone();
      let params = prepared[i].params.clone();
      let call_fut = async move { tools.execute(&tool, params).await };
      let timeout = remaining_timeout(run_started_at, timeout_ms);
      let cancel = cancellation_token.as_ref().map(|token| token.cancelled());
      let output = match race_with_limits(call_fut, timeout, cancel).await {
        RaceOutcome::Completed(Ok(out)) => out,
        // W0.5: same stop semantics as the single-call path — see the
        // comment on the equivalent arm in `execute_tool_with_limits`.
        RaceOutcome::Completed(Err(agentflow_tool::ToolError::PolicyDeniedAndStop { message })) => {
          emit_and_push!(
            self.live_sink,
            events,
            AgentEvent::ToolCallCompleted {
              session_id: self.session_id.clone(),
              step_index: prepared[i].call_step_idx,
              tool: prepared[i].tool.clone(),
              is_error: true,
              duration_ms: started.elapsed().as_millis() as u64,
              source: prepared[i].source.clone(),
              permissions: prepared[i].permissions.clone(),
              timestamp: Utc::now(),
            }
          );
          return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::ApprovalDenied { message },
            std::mem::take(steps),
            std::mem::take(events),
          ))));
        }
        RaceOutcome::Completed(Err(e)) => {
          warn!(tool = %prepared[i].tool, error = %e, "tool execution failed");
          agentflow_tool::ToolOutput::error(e.to_string())
        }
        RaceOutcome::TimedOut => {
          emit_and_push!(
            self.live_sink,
            events,
            AgentEvent::ToolCallCompleted {
              session_id: self.session_id.clone(),
              step_index: prepared[i].call_step_idx,
              tool: prepared[i].tool.clone(),
              is_error: true,
              duration_ms: started.elapsed().as_millis() as u64,
              source: prepared[i].source.clone(),
              permissions: prepared[i].permissions.clone(),
              timestamp: Utc::now(),
            }
          );
          return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::Timeout {
              timeout_ms: timeout_ms.unwrap_or_default(),
            },
            std::mem::take(steps),
            std::mem::take(events),
          ))));
        }
        RaceOutcome::Cancelled => {
          return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
            &self.session_id,
            "cancellation token signalled",
            std::mem::take(steps),
            std::mem::take(events),
          ))));
        }
      };
      outputs[i] = Some((output, started.elapsed().as_millis() as u64));
    }

    // 6. Emit completions + push ToolResult steps in LLM order;
    //    append tool results to memory. Reflection is recorded for
    //    the batch once if any call errored, so the next LLM turn
    //    sees a single reflective summary rather than n reflections.
    let mut error_summary = String::new();
    for (i, prep) in prepared.iter().enumerate() {
      // Q2.9.1: previous code `expect`ed every prepared call to have
      // an output set by the earlier loop. The invariant should
      // hold (the previous loop fills `outputs[i]` for every i in
      // 0..prepared.len()) but a panic here would crash the entire
      // ReAct runtime mid-batch. Fall back to a synthetic error
      // output + warning so the rest of the batch still completes
      // and the operator sees the inconsistency in the trace.
      let (output, duration_ms) = match outputs[i].take() {
        Some(pair) => pair,
        None => {
          warn!(
            tool = %prep.tool,
            index = i,
            "internal invariant violation: prepared call has no recorded output; emitting synthetic error"
          );
          (
            agentflow_tool::ToolOutput::error(
              "internal invariant violation: tool call has no output recorded".to_string(),
            ),
            0,
          )
        }
      };
      let observation = if output.is_error {
        format!("[ERROR] {}", output.content)
      } else {
        output.content.clone()
      };
      info!(
        tool = %prep.tool,
        "Batch observation [{}]: {}",
        i,
        truncate_str_at_char_boundary(&observation, 200)
      );
      let result_step_idx = *step_index;
      *step_index += 1;
      push_step!(
        self.live_sink,
        steps,
        events,
        self.session_id,
        result_step_idx,
        AgentStepKind::ToolResult {
          tool: prep.tool.clone(),
          content: output.content.clone(),
          is_error: output.is_error,
          parts: output.parts.clone(),
        }
      );
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallCompleted {
          session_id: self.session_id.clone(),
          step_index: prep.call_step_idx,
          tool: prep.tool.clone(),
          is_error: output.is_error,
          duration_ms,
          source: prep.source.clone(),
          permissions: prep.permissions.clone(),
          timestamp: Utc::now(),
        }
      );
      if output.is_error {
        if !error_summary.is_empty() {
          error_summary.push_str("; ");
        }
        error_summary.push_str(&format!("{}: {}", prep.tool, observation));
      }
      *tool_calls_counter += 1;
      self
        .add_memory_message(Message::tool_result_with_counter(
          &self.session_id,
          &prep.tool,
          &observation,
          &*self.message_counter,
        ))
        .await?;
    }
    if !error_summary.is_empty() {
      self
        .record_reflection(
          ReflectionContext::failure(&self.session_id, *step_index, error_summary),
          step_index,
          steps,
          events,
        )
        .await?;
    }

    Ok(BatchOutcome::Continue)
  }

  async fn emit_batch_timeout(
    &self,
    prepared: &[PreparedToolCall],
    idxs: &[usize],
    events: &mut Vec<AgentEvent>,
  ) {
    for &i in idxs {
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallCompleted {
          session_id: self.session_id.clone(),
          step_index: prepared[i].call_step_idx,
          tool: prepared[i].tool.clone(),
          is_error: true,
          duration_ms: 0,
          source: prepared[i].source.clone(),
          permissions: prepared[i].permissions.clone(),
          timestamp: Utc::now(),
        }
      );
    }
  }

  async fn emit_batch_cancelled(
    &self,
    prepared: &[PreparedToolCall],
    idxs: &[usize],
    events: &mut Vec<AgentEvent>,
  ) {
    for &i in idxs {
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallCompleted {
          session_id: self.session_id.clone(),
          step_index: prepared[i].call_step_idx,
          tool: prepared[i].tool.clone(),
          is_error: true,
          duration_ms: 0,
          source: prepared[i].source.clone(),
          permissions: prepared[i].permissions.clone(),
          timestamp: Utc::now(),
        }
      );
    }
  }
}
