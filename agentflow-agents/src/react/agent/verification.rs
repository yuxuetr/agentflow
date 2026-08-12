use chrono::Utc;
use tracing::warn;

use agentflow_memory::Message;

use crate::runtime::{AgentEvent, AgentStep, AgentStepKind};
use crate::verification::{VerificationContext, VerificationOutcome};

use super::config::ReActError;
use super::core::{ReActAgent, push_step};
use super::turn_driven::LoopState;

impl ReActAgent {
  /// Validate a candidate final answer against `config.output_schema`
  /// (V2.1), if configured. Returns `true` when `run_one_turn` should loop
  /// back around for another attempt instead of stopping.
  ///
  /// Structural (does this even parse as JSON matching the schema) rather
  /// than semantic, so it runs *before* [`Self::gate_candidate_answer`] —
  /// no point running a domain `VerificationStrategy` against an answer
  /// that doesn't even conform to the caller's declared shape. Tracked with
  /// its own attempt budget
  /// ([`ReActConfig::max_schema_correction_attempts`]), independent of
  /// `verification_attempts`: unlike verification rejection (which
  /// force-accepts on exhaustion), exhausting the schema budget is a hard
  /// [`ReActError::SchemaValidationFailed`] — see `output_schema`'s doc
  /// comment for why. Reuses the existing `AgentStepKind::Verify`/
  /// `AgentEvent::VerificationCompleted` shapes rather than adding new
  /// variants (`AgentStepKind`'s doc comment asks custom runtimes to reuse
  /// existing variants rather than fork the enum); the feedback text
  /// itself makes the schema origin unambiguous to trace readers.
  pub(crate) async fn gate_schema_answer(
    &mut self,
    answer: &str,
    st: &mut LoopState,
  ) -> Result<bool, ReActError> {
    let Some(schema) = self.config.output_schema.clone() else {
      return Ok(false);
    };

    let attempt = st.schema_correction_attempts + 1;
    let validation = agentflow_agent_spi::validate_json_str_against_schema(&schema, answer);
    let approved = validation.is_ok();

    let current_step = st.step_index;
    let feedback = validation.as_ref().err().map(|errors| {
      format!(
        "Your final answer did not match the required output schema: {}. \
         Correct it and provide the final answer again.",
        errors.join("; ")
      )
    });
    push_step!(
      self.live_sink,
      st.steps,
      st.events,
      self.session_id,
      current_step,
      AgentStepKind::Verify {
        approved,
        feedback: feedback.clone(),
        attempt,
      }
    );
    st.events.push(AgentEvent::VerificationCompleted {
      session_id: self.session_id.clone(),
      step_index: current_step,
      approved,
      timestamp: Utc::now(),
    });
    st.step_index += 1;

    let Err(errors) = validation else {
      return Ok(false);
    };

    if attempt > self.config.max_schema_correction_attempts {
      warn!(
        attempt,
        max_attempts = self.config.max_schema_correction_attempts,
        "schema correction attempts exhausted"
      );
      return Err(ReActError::SchemaValidationFailed {
        errors,
        attempts: attempt,
      });
    }

    self
      .add_memory_message(Message::tool_result_with_counter(
        &self.session_id,
        "schema_validator",
        feedback.unwrap_or_default(),
        &*self.message_counter,
      ))
      .await?;
    st.schema_correction_attempts = attempt;
    st.iteration += 1;
    Ok(true)
  }

  /// Run the attached `VerificationStrategy` (if any) against a candidate
  /// final answer. Returns `true` when `run_one_turn` should loop back
  /// around for another attempt instead of stopping.
  ///
  /// Shared by the `AgentResponse::Answer` and `AgentResponse::Malformed`
  /// branches of [`Self::run_one_turn`] — both reach a candidate final
  /// answer and must apply the same gate.
  pub(crate) async fn gate_candidate_answer(
    &mut self,
    answer: &str,
    st: &mut LoopState,
  ) -> Result<bool, ReActError> {
    let attempt = st.verification_attempts + 1;
    let outcome = self
      .record_verification(
        VerificationContext::for_answer(
          &self.session_id,
          st.step_index,
          &st.user_input,
          answer,
          attempt,
        ),
        &mut st.step_index,
        &mut st.steps,
        &mut st.events,
      )
      .await?;

    let VerificationOutcome::Rejected { .. } = outcome else {
      return Ok(false);
    };
    if attempt >= self.config.max_verification_attempts {
      warn!(
        attempt,
        max_attempts = self.config.max_verification_attempts,
        "verification attempts exhausted; force-accepting candidate answer"
      );
      return Ok(false);
    }

    st.verification_attempts = attempt;
    st.iteration += 1;
    Ok(true)
  }

  /// L4.4: run the attached [`CitationChecker`] (if any) against an
  /// approved candidate final answer. Returns the answer unchanged when
  /// there's no checker configured, no `rag_search` result to check
  /// against, or every referenced citation is supported. Otherwise
  /// downgrades the answer (strips every referenced citation marker),
  /// records the outcome as a `Verify` step + `VerificationCompleted`
  /// event (reusing the existing verification step/event kinds rather
  /// than introducing new ones — `AgentStepKind` is closed by design),
  /// and returns the downgraded text.
  ///
  /// A checker error is logged and treated as non-fatal (the original
  /// answer passes through unchanged) — same failure philosophy as
  /// `record_verification`'s strategy-error handling: a broken checker
  /// must not block the run.
  pub(crate) async fn apply_citation_check(
    &mut self,
    answer: String,
    st: &mut LoopState,
  ) -> String {
    let Some(checker) = self.citation_checker.clone() else {
      return answer;
    };
    let report = match crate::citation::verify_citations(&st.steps, &answer, checker.as_ref()).await
    {
      Ok(report) => report,
      Err(err) => {
        warn!(
          checker = checker.name(),
          error = %err,
          "citation checker failed; keeping candidate answer unchanged"
        );
        return answer;
      }
    };
    let Some(report) = report else {
      return answer;
    };
    if report.all_supported() {
      return answer;
    }

    let markers = report.markers();
    let unsupported: Vec<String> = report
      .verdicts
      .iter()
      .filter_map(|(c, v)| match v {
        crate::citation::CitationVerdict::Unsupported { reason } => {
          Some(format!("{} ({reason})", c.marker))
        }
        crate::citation::CitationVerdict::Supported => None,
      })
      .collect();
    let feedback = format!(
      "citation check failed for {}; answer downgraded to a citation-free version",
      unsupported.join(", ")
    );
    let downgraded = crate::citation::downgrade_answer(&answer, &markers);

    let current_step = st.step_index;
    push_step!(
      self.live_sink,
      st.steps,
      st.events,
      self.session_id,
      current_step,
      AgentStepKind::Verify {
        approved: false,
        feedback: Some(feedback),
        attempt: 0,
      }
    );
    st.events.push(AgentEvent::VerificationCompleted {
      session_id: self.session_id.clone(),
      step_index: current_step,
      approved: false,
      timestamp: Utc::now(),
    });
    st.step_index += 1;

    downgraded
  }

  /// Gate a candidate final answer through the attached
  /// [`VerificationStrategy`], if any. Always records a `Verify` step
  /// (and `VerificationCompleted` event) when a strategy is attached and
  /// enabled, so the trace shows every verdict. A [`VerificationOutcome::Rejected`]
  /// feeds its feedback into memory as the next observation, mirroring
  /// how `dispatch_single_tool_call` feeds tool results back for the
  /// following turn.
  ///
  /// Returns [`VerificationOutcome::Approved`] when no strategy is
  /// attached, verification is disabled, or the strategy call itself
  /// fails (non-fatal — logged and treated as approved so the run can't
  /// deadlock).
  async fn record_verification(
    &mut self,
    context: VerificationContext,
    step_index: &mut usize,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
  ) -> Result<VerificationOutcome, ReActError> {
    if !self.config.verification_enabled {
      return Ok(VerificationOutcome::Approved);
    }
    let Some(strategy) = self.verification.clone() else {
      return Ok(VerificationOutcome::Approved);
    };

    let attempt = context.attempt;
    let outcome = match strategy.verify(&context).await {
      Ok(outcome) => outcome,
      Err(err) => {
        warn!(
          strategy = strategy.name(),
          error = %err,
          "verification strategy failed; accepting candidate answer"
        );
        VerificationOutcome::Approved
      }
    };

    let approved = matches!(outcome, VerificationOutcome::Approved);
    let feedback = match &outcome {
      VerificationOutcome::Approved => None,
      VerificationOutcome::Rejected { feedback } => Some(feedback.clone()),
    };

    let current_step = *step_index;
    push_step!(
      self.live_sink,
      steps,
      events,
      self.session_id,
      current_step,
      AgentStepKind::Verify {
        approved,
        feedback,
        attempt,
      }
    );
    events.push(AgentEvent::VerificationCompleted {
      session_id: self.session_id.clone(),
      step_index: current_step,
      approved,
      timestamp: Utc::now(),
    });
    *step_index += 1;

    if let VerificationOutcome::Rejected { feedback } = &outcome {
      self
        .add_memory_message(Message::tool_result_with_counter(
          &self.session_id,
          "verifier",
          feedback,
          &*self.message_counter,
        ))
        .await?;
    }

    Ok(outcome)
  }
}
