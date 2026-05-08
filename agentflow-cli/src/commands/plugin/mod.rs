//! `agentflow plugin` CLI sub-commands.
//!
//! Mirrors the layout of `commands/skill/`: each verb lives in its own
//! module, and the parent re-exports them. The whole module is gated on
//! `feature = "plugin"` so default builds stay free of the subprocess
//! plugin runtime dependency footprint.

pub mod inspect;
pub mod install;
pub mod list;
pub mod uninstall;
