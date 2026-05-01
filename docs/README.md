# AgentFlow Docs

Last updated: 2026-05-01

This directory contains current user and maintainer documentation. Historical
phase reports, old TODO trackers, implementation summaries, and stale backups
have been removed from the active docs tree so this directory stays focused on
working references.

## Start Here

- [ARCHITECTURE.md](ARCHITECTURE.md): current workspace layout and runtime model.
- [CONFIGURATION.md](CONFIGURATION.md): CLI config, secrets, workflow YAML, and run directories.
- [WORKFLOW_SCHEMA.md](WORKFLOW_SCHEMA.md): implemented config-first workflow validation contract.
- [AGENT_RUNTIME.md](AGENT_RUNTIME.md): agent runtime behavior and event model.
- [SKILLS.md](SKILLS.md): Skill authoring, tools, MCP integration, memory, and CLI commands.
- [MCP_SKILLS.md](MCP_SKILLS.md): MCP tool usage from Skills.
- [TRACING_USAGE.md](TRACING_USAGE.md): trace capture, replay, and inspection.
- [CHECKPOINT_RECOVERY.md](CHECKPOINT_RECOVERY.md): checkpoint and resume behavior.

## Topic Guides

- Workflow execution: [WORKFLOW_DEBUGGING.md](WORKFLOW_DEBUGGING.md), [HYBRID_WORKFLOW.md](HYBRID_WORKFLOW.md), [TIMEOUT_CONTROL.md](TIMEOUT_CONTROL.md), [RETRY_MECHANISM.md](RETRY_MECHANISM.md), [RESOURCE_MANAGEMENT.md](RESOURCE_MANAGEMENT.md)
- Production operations: [DEPLOYMENT.md](DEPLOYMENT.md), [KUBERNETES_DEPLOYMENT.md](KUBERNETES_DEPLOYMENT.md), [HEALTH_CHECKS.md](HEALTH_CHECKS.md), [SECRET_MANAGEMENT.md](SECRET_MANAGEMENT.md), [TOOL_PERMISSIONS.md](TOOL_PERMISSIONS.md)
- Skills and ecosystem: [SKILL_FORMAT.md](SKILL_FORMAT.md), [SKILL_REGISTRY.md](SKILL_REGISTRY.md), [MCP_SKILLS_INTEGRATION.md](MCP_SKILLS_INTEGRATION.md), [MCP_PRODUCTION_DESIGN.md](MCP_PRODUCTION_DESIGN.md)
- LLM and multimodal: [GRANULAR_MODEL_TYPES.md](GRANULAR_MODEL_TYPES.md), [MULTIMODAL_GUIDE.md](MULTIMODAL_GUIDE.md)
- Tracing internals: [TRACING_DESIGN.md](TRACING_DESIGN.md), [TRACE_PERSISTENCE_SCHEMA.md](TRACE_PERSISTENCE_SCHEMA.md)
- Examples: [examples/README.md](examples/README.md)

## Historical Release References

These remain as release/migration references, not as current design documents:

- [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md)
- [MIGRATION_GUIDE_v0.2.0.md](MIGRATION_GUIDE_v0.2.0.md)

## Maintenance Rule

Keep active docs tied to implemented behavior. Put time-boxed plans, phase
completion notes, audit reports, and TODO trackers outside `docs/` or delete them
after their decisions have been folded into stable guides.
