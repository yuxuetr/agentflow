//! Turn-driven runtime contracts (RFC_HARNESS_LOOP_OWNERSHIP §6).
//!
//! The object-safe façade that lets an external owner (the Harness) drive any
//! runtime one turn at a time — `begin()` a [`TurnDrivenRuntime`], then pump the
//! returned [`LoopSession`], performing context engineering between turns. The
//! concrete `ReActAgent` / `ReActLoopSession` implementations live in
//! `agentflow-agents`; these contracts live here so the harness governs a
//! runtime through `Box<dyn TurnDrivenRuntime>` without depending on the
//! `agents` impl crate (P-A2.1).

use crate::runtime::{AgentContext, AgentRunResult, AgentRuntimeError};
use agentflow_store_spi::MemoryStore;
use async_trait::async_trait;

/// Outcome of one driven turn.
#[derive(Debug)]
pub enum TurnProgress {
  /// The agent advanced; call [`LoopSession::next_turn`] again.
  Continued,
  /// The agent reached a terminal state; the run result is attached.
  Finished(AgentRunResult),
}

/// A runtime that can be **driven one turn at a time** by an external owner.
/// The owner calls [`begin`](TurnDrivenRuntime::begin) and pumps the returned
/// [`LoopSession`], performing its own context engineering between turns. This
/// is the object-safe, runtime-agnostic façade so the Harness can drive any
/// turn-driven runtime through `Box<dyn TurnDrivenRuntime>`.
#[async_trait]
pub trait TurnDrivenRuntime: Send {
  /// Begin a turn-driven run and return the session to pump.
  async fn begin(
    &mut self,
    context: AgentContext,
  ) -> Result<Box<dyn LoopSession + Send + '_>, AgentRuntimeError>;

  /// Stable, machine-readable runtime identifier (e.g. `"react"`).
  fn runtime_name(&self) -> &'static str;
}

/// One turn-driven session: pump [`next_turn`](LoopSession::next_turn) until it
/// returns [`TurnProgress::Finished`]. Between turns the owner may inspect or
/// rewrite [`memory`](LoopSession::memory).
#[async_trait]
pub trait LoopSession: Send {
  /// Advance exactly one turn.
  async fn next_turn(&mut self) -> Result<TurnProgress, AgentRuntimeError>;
  /// The run's conversation memory (for caller-owned context engineering).
  fn memory(&self) -> &dyn MemoryStore;
  /// 0-based index of the turn `next_turn` will run next.
  fn turn_index(&self) -> usize;
}
