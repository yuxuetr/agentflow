//! Harness governance contracts.
//!
//! The wire/protocol surface a Harness runtime and its governors agree on,
//! extracted from `agentflow-harness` in P-A1.1 (RFC §4) so that operations
//! crates (`agentflow-tracing`, `agentflow-server`, UIs / SDKs) can depend on
//! the contract in the kernel instead of the `agentflow-harness` runtime crate.
//!
//! What lives here (contract only — no execution logic, no side effects):
//!
//! - [`HarnessEvent`] — the line-delimited JSON envelope.
//! - [`ApprovalRequest`] / [`ApprovalDecision`] and the [`ApprovalProvider`]
//!   trait.
//! - [`PreToolHook`] / [`PostToolHook`].
//! - [`ContextProvider`] plus the session descriptor ([`HarnessContext`] /
//!   [`HarnessProfile`] / [`HarnessRuntimeKind`]).
//! - [`HarnessEventSink`] — the persistence-sink trait.
//! - [`HarnessError`] — the shared contract error.
//!
//! Concrete implementations (`HarnessRuntime`, `HookConfig` / `wrap_registry`,
//! the default providers, the file/stdout/in-memory sinks, the tracing bridge)
//! stay in `agentflow-harness`, which re-exports everything here under its
//! original paths so existing consumers compile unchanged.

pub mod approval;
pub mod context;
pub mod error;
pub mod event;
pub mod hooks;
pub mod sink;

pub use approval::{
  ApprovalDecision, ApprovalOutcome, ApprovalProvider, ApprovalRequest, ApprovalRisk, ApprovalScope,
};
pub use context::{
  ContextItem, ContextPriority, ContextProvider, HarnessContext, HarnessProfile, HarnessRuntimeKind,
};
pub use error::HarnessError;
pub use event::{
  ApprovalDecidedPayload, ApprovalRequestedPayload, BackgroundTaskStatus,
  BackgroundTaskUpdatedPayload, HarnessEvent, HarnessEventBody, MemorySummaryAddedPayload,
  SessionStartedPayload, StepStartedPayload, StopReason, StoppedPayload, ToolCallCompletedPayload,
  ToolCallRequestedPayload,
};
pub use hooks::{CompletedToolCall, PendingToolCall, PostToolHook, PreToolDecision, PreToolHook};
pub use sink::HarnessEventSink;
