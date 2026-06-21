pub mod commands;
// `config` (workflow schema) + `executor` (YAML -> Flow assembly) moved to the
// shared `agentflow-config` crate in P-A2.4 so the server can assemble workflows
// without depending on the CLI. Re-exported under their original paths.
pub use agentflow_config::{config, executor};
pub mod json_envelope;
pub mod redaction;
pub mod server_client;
pub mod shutdown;
