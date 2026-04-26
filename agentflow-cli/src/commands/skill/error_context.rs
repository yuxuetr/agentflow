use agentflow_skills::SkillManifest;

pub fn mcp_context(action: &str, manifest: &SkillManifest) -> String {
  if manifest.mcp_servers.is_empty() {
    return action.to_string();
  }

  let servers = manifest
    .mcp_servers
    .iter()
    .map(|server| {
      let command = std::iter::once(server.command.as_str())
        .chain(server.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
      format!("server '{}' via `{}`", server.name, command)
    })
    .collect::<Vec<_>>()
    .join("; ");

  format!(
    "{} for skill '{}' ({}). MCP tool names are exposed as mcp_<server>_<tool>.",
    action, manifest.skill.name, servers
  )
}
