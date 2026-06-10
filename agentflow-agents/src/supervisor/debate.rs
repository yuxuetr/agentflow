//! Debate multi-agent collaboration: N participants propose answers in
//! parallel; over optional further rounds they revise their proposals after
//! seeing each other's; finally a judge agent renders the verdict.
//!
//! # Why?
//!
//! Useful when one model's answer is too risky to trust on its own — fact
//! checking, multi-perspective summarisation, code review with N reviewers,
//! etc. The judge can either pick a winner (`winner = Some(name)`) or
//! synthesise a merged answer (`winner = None`).
//!
//! # Lifecycle
//!
//! 1. The supervisor records the user's input as an `Observe` step.
//! 2. For each round 1..=`rounds`:
//!    - emit `DebateRoundStarted` event.
//!    - run every participant concurrently with the round's prompt
//!      (round 1 = the original input; later rounds include each
//!      participant's prior-round proposal so they can revise).
//!    - record one `DebateProposal` step per participant.
//! 3. Run the judge with all final-round proposals; record a
//!    `DebateVerdict` step + `DebateVerdictRendered` event.
//! 4. The judge's `answer` becomes the supervisor's `answer`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::react::ReActAgent;
use crate::runtime::{
  AgentContext, AgentEvent, AgentRunResult, AgentRuntime, AgentRuntimeError, AgentStep,
  AgentStepKind, AgentStopReason,
};

const DEFAULT_JUDGE_PROMPT: &str = "\
Several specialist agents have independently considered the following user \
request and produced their own answers. As the judge, read each proposal \
carefully, identify points of agreement and disagreement, then produce a \
single best answer. If one proposal is clearly superior you may pick it \
verbatim; otherwise synthesise the strongest combined answer.";

/// One participant's proposal for a single debate round.
#[derive(Debug, Clone)]
struct ProposalRecord {
  agent: String,
  /// `None` means the agent failed to produce an answer this round.
  proposal: Option<String>,
  /// Captured for diagnostic logging in future revisions; intentionally
  /// retained even though the current judge prompt does not surface it.
  #[allow(dead_code)]
  stop_reason: AgentStopReason,
}

#[derive(Debug, thiserror::Error)]
pub enum DebateSupervisorError {
  #[error("DebateSupervisor needs at least one participant")]
  NoParticipants,
  #[error("DebateSupervisor needs a judge agent")]
  NoJudge,
  #[error("Duplicate participant name '{0}'")]
  DuplicateParticipant(String),
  #[error("rounds must be ≥ 1")]
  ZeroRounds,
}

/// A multi-agent runtime where participants propose answers in parallel and
/// a judge produces the final verdict.
///
/// Implements [`AgentRuntime`] so it can be embedded in [`AgentNode`].
///
/// [`AgentNode`]: crate::nodes::AgentNode
pub struct DebateSupervisor {
  /// Participants in registration order so traces are deterministic.
  participants: Vec<(String, Arc<AsyncMutex<ReActAgent>>)>,
  judge: Arc<AsyncMutex<ReActAgent>>,
  rounds: usize,
  judge_prompt_template: String,
  session_id: String,
}

impl std::fmt::Debug for DebateSupervisor {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let participants: Vec<&str> = self.participants.iter().map(|(n, _)| n.as_str()).collect();
    f.debug_struct("DebateSupervisor")
      .field("session_id", &self.session_id)
      .field("rounds", &self.rounds)
      .field("participants", &participants)
      .finish()
  }
}

impl DebateSupervisor {
  pub fn session_id(&self) -> &str {
    &self.session_id
  }

  pub fn participant_names(&self) -> Vec<&str> {
    self.participants.iter().map(|(n, _)| n.as_str()).collect()
  }

  /// Convenience: run a one-shot task and return the judge's answer.
  pub async fn run(&mut self, task: &str) -> Result<String, AgentRuntimeError> {
    let context = AgentContext::new(self.session_id.clone(), task, "");
    let result = AgentRuntime::run(self, context).await?;
    result
      .answer
      .ok_or_else(|| AgentRuntimeError::ExecutionFailed {
        message: format!(
          "DebateSupervisor stopped without a final answer: {:?}",
          result.stop_reason
        ),
      })
  }

  fn build_participant_input(&self, original: &str, prior: Option<&[ProposalRecord]>) -> String {
    let Some(prior) = prior else {
      return original.to_string();
    };
    let mut buf = String::from(original);
    buf.push_str("\n\nOther agents previously proposed:\n");
    for record in prior {
      let body = record.proposal.as_deref().unwrap_or("(no answer)");
      buf.push_str(&format!("- {}: {}\n", record.agent, body));
    }
    buf.push_str(
      "\nReview these proposals critically, then produce your own revised answer. \
       Respond with the final improved answer only.",
    );
    buf
  }

  fn build_judge_input(&self, original: &str, finals: &[ProposalRecord]) -> String {
    let mut buf = self.judge_prompt_template.clone();
    buf.push_str("\n\nUser request:\n");
    buf.push_str(original);
    buf.push_str("\n\nProposals:\n");
    for record in finals {
      let body = record.proposal.as_deref().unwrap_or("(no answer)");
      buf.push_str(&format!("- {}: {}\n", record.agent, body));
    }
    buf
  }
}

#[async_trait]
impl AgentRuntime for DebateSupervisor {
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
    let session_id = context.session_id.clone();
    let mut steps: Vec<AgentStep> = Vec::new();
    let mut events: Vec<AgentEvent> = Vec::new();
    let mut step_index = 0usize;

    events.push(AgentEvent::RunStarted {
      session_id: session_id.clone(),
      model: format!(
        "multi_agent:debate(participants={},rounds={})",
        self.participants.len(),
        self.rounds
      ),
      timestamp: context.started_at,
    });
    steps.push(AgentStep::new(
      step_index,
      AgentStepKind::Observe {
        input: context.input.clone(),
      },
    ));
    step_index += 1;

    let cancellation = context.cancellation_token.clone();
    if cancellation.as_ref().is_some_and(|t| t.is_cancelled()) {
      return Ok(stopped(
        session_id,
        None,
        AgentStopReason::Cancelled {
          message: "cancellation token signalled".into(),
        },
        steps,
        events,
      ));
    }

    let mut last_round_proposals: Vec<ProposalRecord> = Vec::new();

    for round in 1..=self.rounds {
      let participant_names: Vec<String> =
        self.participants.iter().map(|(n, _)| n.clone()).collect();

      events.push(AgentEvent::DebateRoundStarted {
        session_id: session_id.clone(),
        round,
        participants: participant_names.clone(),
        timestamp: Utc::now(),
      });

      let prior_for_input = if round == 1 {
        None
      } else {
        Some(last_round_proposals.as_slice())
      };
      let round_input = self.build_participant_input(&context.input, prior_for_input);

      // Spawn all participants concurrently.
      let mut handles: Vec<(
        String,
        tokio::task::JoinHandle<Result<AgentRunResult, AgentRuntimeError>>,
      )> = Vec::with_capacity(self.participants.len());
      for (name, handle) in &self.participants {
        let agent_handle = handle.clone();
        let child_ctx = build_child_context(&context, name, &round_input);
        let task = tokio::spawn(async move {
          let mut guard = agent_handle.lock().await;
          AgentRuntime::run(&mut *guard, child_ctx).await
        });
        handles.push((name.clone(), task));
      }

      // Collect results in registration order so the trace is deterministic.
      let mut round_proposals: Vec<ProposalRecord> = Vec::with_capacity(handles.len());
      for (name, handle) in handles {
        match handle.await {
          Ok(Ok(child_result)) => {
            step_index =
              merge_child_into(&mut steps, &mut events, step_index, child_result.clone());
            let proposal = child_result.answer.clone();
            let proposal_step_index = step_index;
            steps.push(AgentStep::new(
              proposal_step_index,
              AgentStepKind::DebateProposal {
                round,
                agent: name.clone(),
                proposal: proposal.clone().unwrap_or_default(),
              },
            ));
            step_index += 1;
            round_proposals.push(ProposalRecord {
              agent: name,
              proposal,
              stop_reason: child_result.stop_reason,
            });
          }
          Ok(Err(e)) => {
            // Surfaced agent error: record an empty proposal, keep debating.
            round_proposals.push(ProposalRecord {
              agent: name.clone(),
              proposal: None,
              stop_reason: AgentStopReason::Error {
                message: e.to_string(),
              },
            });
            let proposal_step_index = step_index;
            steps.push(AgentStep::new(
              proposal_step_index,
              AgentStepKind::DebateProposal {
                round,
                agent: name,
                proposal: String::new(),
              },
            ));
            step_index += 1;
          }
          Err(join_err) => {
            return Err(AgentRuntimeError::ExecutionFailed {
              message: format!("DebateSupervisor: participant join failed: {join_err}"),
            });
          }
        }
      }

      last_round_proposals = round_proposals;

      if cancellation.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Ok(stopped(
          session_id,
          None,
          AgentStopReason::Cancelled {
            message: "cancellation token signalled".into(),
          },
          steps,
          events,
        ));
      }
    }

    // Run the judge with the final-round proposals.
    let judge_input = self.build_judge_input(&context.input, &last_round_proposals);
    let judge_ctx = build_child_context(&context, "judge", &judge_input);
    let judge_result = {
      let mut guard = self.judge.lock().await;
      AgentRuntime::run(&mut *guard, judge_ctx).await?
    };
    step_index = merge_child_into(&mut steps, &mut events, step_index, judge_result.clone());

    let verdict_index = step_index;
    let answer = judge_result.answer.clone();
    let rationale = answer.clone().unwrap_or_else(|| {
      format!(
        "judge stopped without an answer: {:?}",
        judge_result.stop_reason
      )
    });
    steps.push(AgentStep::new(
      verdict_index,
      AgentStepKind::DebateVerdict {
        winner: None, // judge synthesises rather than voting
        rationale,
      },
    ));
    events.push(AgentEvent::DebateVerdictRendered {
      session_id: session_id.clone(),
      step_index: verdict_index,
      winner: None,
      timestamp: Utc::now(),
    });
    step_index += 1;
    let _ = step_index;

    Ok(stopped(
      session_id,
      answer,
      AgentStopReason::FinalAnswer,
      steps,
      events,
    ))
  }

  fn runtime_name(&self) -> &'static str {
    "debate"
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_child_context(parent: &AgentContext, agent_name: &str, input: &str) -> AgentContext {
  let session = format!("{}::{}", parent.session_id, agent_name);
  let mut ctx =
    AgentContext::new(session, input, parent.model.clone()).with_limits(parent.limits.clone());
  if let Some(token) = parent.cancellation_token.clone() {
    ctx = ctx.with_cancellation_token(token);
  }
  ctx.metadata = parent.metadata.clone();
  ctx
}

fn merge_child_into(
  steps: &mut Vec<AgentStep>,
  events: &mut Vec<AgentEvent>,
  mut next_index: usize,
  child: AgentRunResult,
) -> usize {
  let mut index_map: HashMap<usize, usize> = HashMap::new();
  for mut step in child.steps {
    let original = step.index;
    step.index = next_index;
    index_map.insert(original, next_index);
    steps.push(step);
    next_index += 1;
  }
  for mut event in child.events {
    rewrite_event_step_index(&mut event, &index_map);
    events.push(event);
  }
  next_index
}

fn rewrite_event_step_index(event: &mut AgentEvent, map: &HashMap<usize, usize>) {
  match event {
    AgentEvent::StepStarted { step_index, .. }
    | AgentEvent::ToolCallStarted { step_index, .. }
    | AgentEvent::ToolPolicyDecision { step_index, .. }
    | AgentEvent::ToolCapabilityDecision { step_index, .. }
    | AgentEvent::ToolCallCompleted { step_index, .. }
    | AgentEvent::LlmCallCompleted { step_index, .. }
    | AgentEvent::ReflectionAdded { step_index, .. }
    | AgentEvent::HandoffOccurred { step_index, .. }
    | AgentEvent::BlackboardWritten { step_index, .. }
    | AgentEvent::DebateVerdictRendered { step_index, .. } => {
      if let Some(remapped) = map.get(step_index) {
        *step_index = *remapped;
      }
    }
    AgentEvent::StepCompleted { step, .. } => {
      if let Some(remapped) = map.get(&step.index) {
        step.index = *remapped;
      }
    }
    AgentEvent::RunStarted { .. }
    | AgentEvent::RunStopped { .. }
    | AgentEvent::MemorySummaryAdded { .. }
    | AgentEvent::DebateRoundStarted { .. } => {}
  }
}

fn stopped(
  session_id: String,
  answer: Option<String>,
  reason: AgentStopReason,
  steps: Vec<AgentStep>,
  mut events: Vec<AgentEvent>,
) -> AgentRunResult {
  events.push(AgentEvent::RunStopped {
    session_id: session_id.clone(),
    reason: reason.clone(),
    timestamp: Utc::now(),
  });
  AgentRunResult {
    session_id,
    answer,
    stop_reason: reason,
    steps,
    events,
  }
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Builder for [`DebateSupervisor`].
pub struct DebateSupervisorBuilder {
  participants: Vec<DebateAgentSpec>,
  judge: Option<ReActAgent>,
  rounds: usize,
  judge_prompt_template: String,
}

struct DebateAgentSpec {
  name: String,
  agent: ReActAgent,
}

impl Default for DebateSupervisorBuilder {
  fn default() -> Self {
    Self {
      participants: Vec::new(),
      judge: None,
      rounds: 1,
      judge_prompt_template: DEFAULT_JUDGE_PROMPT.to_string(),
    }
  }
}

impl DebateSupervisorBuilder {
  pub fn new() -> Self {
    Self::default()
  }

  /// Register a participant. Names must be unique.
  pub fn add_participant(mut self, name: impl Into<String>, agent: ReActAgent) -> Self {
    self.participants.push(DebateAgentSpec {
      name: name.into(),
      agent,
    });
    self
  }

  /// Set the judge agent. Required.
  pub fn judge(mut self, agent: ReActAgent) -> Self {
    self.judge = Some(agent);
    self
  }

  /// Number of proposal rounds. Defaults to 1.
  pub fn rounds(mut self, rounds: usize) -> Self {
    self.rounds = rounds;
    self
  }

  /// Override the judge's system prompt template.
  pub fn judge_prompt(mut self, template: impl Into<String>) -> Self {
    self.judge_prompt_template = template.into();
    self
  }

  pub fn build(self) -> Result<DebateSupervisor, DebateSupervisorError> {
    if self.participants.is_empty() {
      return Err(DebateSupervisorError::NoParticipants);
    }
    if self.rounds == 0 {
      return Err(DebateSupervisorError::ZeroRounds);
    }
    let mut seen = std::collections::HashSet::new();
    for spec in &self.participants {
      if !seen.insert(spec.name.clone()) {
        return Err(DebateSupervisorError::DuplicateParticipant(
          spec.name.clone(),
        ));
      }
    }
    let judge = self.judge.ok_or(DebateSupervisorError::NoJudge)?;

    let participants: Vec<(String, Arc<AsyncMutex<ReActAgent>>)> = self
      .participants
      .into_iter()
      .map(|s| (s.name, Arc::new(AsyncMutex::new(s.agent))))
      .collect();

    Ok(DebateSupervisor {
      participants,
      judge: Arc::new(AsyncMutex::new(judge)),
      rounds: self.rounds,
      judge_prompt_template: self.judge_prompt_template,
      session_id: Uuid::new_v4().to_string(),
    })
  }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;

  use agentflow_llm::AgentFlow;
  use agentflow_memory::SessionMemory;
  use agentflow_tools::ToolRegistry;

  use crate::react::{ReActAgent, ReActConfig};

  fn solo_agent(model: &str) -> ReActAgent {
    ReActAgent::new(
      ReActConfig::new(model).with_max_iterations(2),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    )
  }

  async fn init_mock_model(model: &str) {
    let path = std::env::temp_dir().join(format!(
      "agentflow-debate-mock-{}.yml",
      uuid::Uuid::new_v4()
    ));
    std::fs::write(
      &path,
      format!(
        r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
      ),
    )
    .unwrap();
    AgentFlow::init_with_config(path.to_str().unwrap())
      .await
      .unwrap();
  }

  fn set_mock_responses(responses: Vec<&str>) {
    let s = serde_json::to_string(&responses).unwrap();
    // SAFETY: callers hold crate::LLM_TEST_LOCK to serialise env mutation.
    unsafe {
      std::env::set_var("AGENTFLOW_MOCK_RESPONSES", s);
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
    }
  }

  // ── Builder validation ────────────────────────────────────────────────────

  #[tokio::test]
  async fn builder_rejects_empty_participants() {
    let err = DebateSupervisorBuilder::new()
      .judge(solo_agent("mock"))
      .build()
      .unwrap_err();
    assert!(matches!(err, DebateSupervisorError::NoParticipants));
  }

  #[tokio::test]
  async fn builder_rejects_missing_judge() {
    let err = DebateSupervisorBuilder::new()
      .add_participant("a", solo_agent("mock"))
      .build()
      .unwrap_err();
    assert!(matches!(err, DebateSupervisorError::NoJudge));
  }

  #[tokio::test]
  async fn builder_rejects_duplicate_participant_names() {
    let err = DebateSupervisorBuilder::new()
      .add_participant("a", solo_agent("mock"))
      .add_participant("a", solo_agent("mock"))
      .judge(solo_agent("mock"))
      .build()
      .unwrap_err();
    assert!(matches!(
      err,
      DebateSupervisorError::DuplicateParticipant(_)
    ));
  }

  #[tokio::test]
  async fn builder_rejects_zero_rounds() {
    let err = DebateSupervisorBuilder::new()
      .add_participant("a", solo_agent("mock"))
      .judge(solo_agent("mock"))
      .rounds(0)
      .build()
      .unwrap_err();
    assert!(matches!(err, DebateSupervisorError::ZeroRounds));
  }

  // ── Helper formatting ────────────────────────────────────────────────────

  #[test]
  fn judge_input_includes_user_request_and_all_proposals() {
    let supervisor = DebateSupervisorBuilder::new()
      .add_participant("a", solo_agent("mock"))
      .add_participant("b", solo_agent("mock"))
      .judge(solo_agent("mock"))
      .build()
      .unwrap();
    let proposals = vec![
      ProposalRecord {
        agent: "a".into(),
        proposal: Some("answer-a".into()),
        stop_reason: AgentStopReason::FinalAnswer,
      },
      ProposalRecord {
        agent: "b".into(),
        proposal: None,
        stop_reason: AgentStopReason::Error {
          message: "boom".into(),
        },
      },
    ];
    let prompt = supervisor.build_judge_input("explain rust", &proposals);
    assert!(prompt.contains("explain rust"));
    assert!(prompt.contains("a: answer-a"));
    assert!(prompt.contains("b: (no answer)"));
  }

  // ── End-to-end via mock LLM ───────────────────────────────────────────────

  #[tokio::test]
  async fn one_round_two_participants_and_judge_returns_judge_answer() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-debate-{}", uuid::Uuid::new_v4());
    set_mock_responses(vec![
      // The mock provider serves responses in FIFO order; both participants
      // run concurrently so their consumption order is non-deterministic.
      // We make all participant responses interchangeable so tests are stable.
      r#"{"thought":"propose","answer":"proposal-1"}"#,
      r#"{"thought":"propose","answer":"proposal-2"}"#,
      // Judge:
      r#"{"thought":"verdict","answer":"final synthesised answer"}"#,
    ]);
    init_mock_model(&model).await;

    let mut supervisor = DebateSupervisorBuilder::new()
      .add_participant("alpha", solo_agent(&model))
      .add_participant("beta", solo_agent(&model))
      .judge(solo_agent(&model))
      .build()
      .unwrap();

    let context = AgentContext::new("session-1", "what is rust?", &model);
    let result = AgentRuntime::run(&mut supervisor, context).await.unwrap();

    assert_eq!(result.answer.as_deref(), Some("final synthesised answer"));
    assert!(matches!(result.stop_reason, AgentStopReason::FinalAnswer));

    // Two DebateProposal steps + one DebateVerdict step expected.
    let proposals: Vec<&AgentStep> = result
      .steps
      .iter()
      .filter(|s| matches!(s.kind, AgentStepKind::DebateProposal { .. }))
      .collect();
    assert_eq!(proposals.len(), 2);

    let verdicts: Vec<&AgentStep> = result
      .steps
      .iter()
      .filter(|s| matches!(s.kind, AgentStepKind::DebateVerdict { .. }))
      .collect();
    assert_eq!(verdicts.len(), 1);
    if let AgentStepKind::DebateVerdict { rationale, .. } = &verdicts[0].kind {
      assert_eq!(rationale, "final synthesised answer");
    }

    // Round-started + verdict-rendered events both present.
    assert!(
      result
        .events
        .iter()
        .any(|e| matches!(e, AgentEvent::DebateRoundStarted { round: 1, .. })),
      "DebateRoundStarted for round 1 must be present"
    );
    assert!(
      result
        .events
        .iter()
        .any(|e| matches!(e, AgentEvent::DebateVerdictRendered { .. })),
      "DebateVerdictRendered must be present"
    );
  }

  #[tokio::test]
  async fn two_rounds_emit_two_round_started_events() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-debate-2r-{}", uuid::Uuid::new_v4());
    set_mock_responses(vec![
      // Round 1: 2 proposals
      r#"{"thought":"p1","answer":"p1-r1"}"#,
      r#"{"thought":"p2","answer":"p2-r1"}"#,
      // Round 2: 2 revised proposals
      r#"{"thought":"p1r","answer":"p1-r2"}"#,
      r#"{"thought":"p2r","answer":"p2-r2"}"#,
      // Judge
      r#"{"thought":"final","answer":"verdict"}"#,
    ]);
    init_mock_model(&model).await;

    let mut supervisor = DebateSupervisorBuilder::new()
      .add_participant("alpha", solo_agent(&model))
      .add_participant("beta", solo_agent(&model))
      .judge(solo_agent(&model))
      .rounds(2)
      .build()
      .unwrap();

    let context = AgentContext::new("session-1", "topic", &model);
    let result = AgentRuntime::run(&mut supervisor, context).await.unwrap();

    let round_starts: usize = result
      .events
      .iter()
      .filter(|e| matches!(e, AgentEvent::DebateRoundStarted { .. }))
      .count();
    assert_eq!(round_starts, 2);

    // 2 rounds × 2 participants = 4 proposal steps.
    let proposals = result
      .steps
      .iter()
      .filter(|s| matches!(s.kind, AgentStepKind::DebateProposal { .. }))
      .count();
    assert_eq!(proposals, 4);

    assert_eq!(result.answer.as_deref(), Some("verdict"));
  }

  #[tokio::test]
  async fn pre_cancelled_token_short_circuits_debate() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-debate-cancel-{}", uuid::Uuid::new_v4());
    set_mock_responses(vec![]);
    init_mock_model(&model).await;

    let mut supervisor = DebateSupervisorBuilder::new()
      .add_participant("a", solo_agent(&model))
      .judge(solo_agent(&model))
      .build()
      .unwrap();

    let token = crate::runtime::AgentCancellationToken::new();
    token.cancel();
    let context = AgentContext::new("session-1", "x", &model).with_cancellation_token(token);
    let result = AgentRuntime::run(&mut supervisor, context).await.unwrap();
    assert!(matches!(
      result.stop_reason,
      AgentStopReason::Cancelled { .. }
    ));
  }
}
