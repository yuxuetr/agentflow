//! `agentflow-config` — shared config schema + config-driven workflow assembly.
//!
//! Holds the YAML workflow [`config`] schema (`FlowDefinitionV2` /
//! `NodeDefinitionV2`) and the [`executor`] that compiles it into an
//! `agentflow-core` `Flow` (`build_flow_from_yaml`). Extracted from
//! `agentflow-cli` in P-A2.4 (RFC §7) so the gateway (`agentflow-server`) can
//! assemble and schedule workflows by depending on this shared crate instead of
//! the CLI binary crate. `agentflow-cli` re-exports both modules under their
//! original `agentflow_cli::{config, executor}` paths.

pub mod config;
pub mod diagnostics;
pub mod executor;
