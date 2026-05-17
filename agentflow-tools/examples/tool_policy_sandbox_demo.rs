//! Tool policy + sandbox capability decision demo (P3.1 row #12).
//!
//! Shows the three-way decision pipeline a tool call goes through
//! inside an AgentFlow runtime:
//!
//! 1. The tool advertises its required capabilities via `ToolMetadata`.
//! 2. `ToolPolicy` decides whether the tool may be invoked at all, based
//!    on tool-name allowlists and `ToolPermission` allowlists.
//! 3. `SandboxPolicy` constrains what the tool can touch at runtime
//!    (allowed shell commands, file paths, network domains).
//!
//! Run offline:
//! ```bash
//! cargo run -p agentflow-tools --example tool_policy_sandbox_demo
//! ```
//!
//! The demo never makes a real shell call — it shows the *decisions*
//! the runtime would produce when fed the same inputs, so it is safe
//! to run on any developer machine.

use std::sync::Arc;

use agentflow_tools::builtin::{HttpTool, ShellTool};
use agentflow_tools::sandbox::SandboxPolicy;
use agentflow_tools::{Tool, ToolPermission, ToolPolicy};
use serde_json::json;

fn main() {
  println!("=== AgentFlow tool policy + sandbox demo ===\n");

  // ── 1. Tool registration with default sandbox ────────────────────────
  let sandbox = Arc::new(SandboxPolicy::default());
  let shell = ShellTool::new(sandbox.clone());
  let http = HttpTool::new(sandbox.clone());

  println!("Built-in tools registered:");
  for tool in [&shell as &dyn Tool, &http as &dyn Tool] {
    let metadata = tool.metadata();
    let permissions: Vec<&str> = metadata
      .permissions
      .permissions
      .iter()
      .map(|p| p.as_str())
      .collect();
    println!(
      "  - {}\n      source: {}\n      permissions: {}",
      tool.name(),
      metadata.source.as_str(),
      if permissions.is_empty() {
        "(none)".to_string()
      } else {
        permissions.join(", ")
      }
    );
  }
  println!();

  // ── 2. Tool policy decisions: allowlist by name ──────────────────────
  let allowlist_policy = ToolPolicy::allow_tools(["http"]);
  println!("Policy = allow_tools([\"http\"]):");
  for tool in [&shell as &dyn Tool, &http as &dyn Tool] {
    let decision = allowlist_policy.evaluate(tool.name(), &tool.metadata(), &json!({}));
    println!(
      "  - {} → {} (rule={}{})",
      tool.name(),
      if decision.allowed { "ALLOWED" } else { "DENIED" },
      decision.matched_rule,
      decision
        .deny_reason
        .as_ref()
        .map(|r| format!(", reason={r}"))
        .unwrap_or_default(),
    );
  }
  println!();

  // ── 3. Tool policy decisions: allowlist by permission ────────────────
  let permission_policy = ToolPolicy::allow_permissions([ToolPermission::Network]);
  println!("Policy = allow_permissions([Network]):");
  for tool in [&shell as &dyn Tool, &http as &dyn Tool] {
    let decision = permission_policy.evaluate(tool.name(), &tool.metadata(), &json!({}));
    println!(
      "  - {} → {} (rule={}{})",
      tool.name(),
      if decision.allowed { "ALLOWED" } else { "DENIED" },
      decision.matched_rule,
      decision
        .deny_reason
        .as_ref()
        .map(|r| format!(", reason={r}"))
        .unwrap_or_default(),
    );
  }
  println!();

  // ── 4. Sandbox policy constraints (runtime layer) ────────────────────
  // The sandbox policy lives below the tool policy: even when a tool is
  // admitted, the sandbox decides what it can actually touch at runtime.
  let strict_sandbox = SandboxPolicy {
    allowed_commands: vec!["echo".into()],
    allowed_domains: vec!["example.com".into()],
    allow_loopback_network_access: false,
    ..SandboxPolicy::default()
  };
  println!("Strict sandbox policy:");
  println!(
    "  allowed_commands: {:?}",
    strict_sandbox.allowed_commands
  );
  println!("  allowed_domains:  {:?}", strict_sandbox.allowed_domains);
  println!(
    "  allow_loopback:   {}",
    strict_sandbox.allow_loopback_network_access
  );
  println!(
    "\nUnder this sandbox, ShellTool will block any command outside\n\
     {{echo}}, and HttpTool will block any request whose host doesn't\n\
     end with `example.com` (including loopback addresses).\n"
  );

  println!("=== Done. See docs/TOOL_PERMISSIONS.md for the full policy model. ===");
}
