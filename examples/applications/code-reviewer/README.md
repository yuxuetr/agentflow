# A2 — code-reviewer

**Status**: TODO (scaffold only)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A2](../../../EXAMPLES_TODOs.md#a2--code-reviewer)

## Business

Input: a GitHub PR URL (or local diff file).
Output: structured review comments grouped by file + sorted by severity.
Optional: push the comments back to GitHub via the API.

## Architecture (planned)

This application ships as a **Skill** so it can be installed and run
through `agentflow skill run code-reviewer`. Under the hood:

- A `ReActAgent` reads the diff and decides which file(s) deserve
  closer attention.
- Tools available to the agent:
  - `get_pr_diff(pr_url)` — via GitHub MCP server or `gh pr diff`
  - `get_file_content(repo, ref, path)` — for context beyond the diff hunks
  - `add_review_comment(pr_url, file, line, body, severity)` — write-side
- Persona enforces the review style (focus on correctness > style,
  flag silent-failure patterns, suggest tests).
- Tool admission gates `add_review_comment` behind Harness Mode
  approval (or `--auto-approve` flag for CI use).

## External dependencies

| Dep | Why |
| --- | --- |
| `gh` CLI or GitHub MCP server | Fetch PR diff + post comments |
| `GITHUB_TOKEN` env | Authenticate gh / MCP |
| Strong LLM (Anthropic / OpenAI flagship) | Review quality is sensitive to model strength |

## What this validates in AgentFlow

- `ReActAgent` main loop with native tool calling
- MCP server integration as agent tools
- Skill packaging (`SKILL.md` / `skill.toml`) + persona + tool allowlist
- Harness Mode approval gate for write-side tools (a key safety surface)
- OS sandbox if shell-calling `gh` directly

## Findings during dogfooding

_Pending implementation._
