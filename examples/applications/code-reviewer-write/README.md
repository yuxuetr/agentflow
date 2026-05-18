# A2 follow-up — code-reviewer-write

**Status**: live ✅ (2026-05-18, Harness Mode approval gate validated
end-to-end via captured `approval_requested` / `approval_decided`
events; model-loop and CLI gaps separately tracked).
**Tracking entry**: [`EXAMPLES_TODOs.md` § A2](../../../EXAMPLES_TODOs.md#a2--code-reviewer)
(closes F-A2-9; introduces F-A2-11 / F-A2-12 / F-A2-13).

## Business

A2's original spec called for a code reviewer that BOTH reads PR diffs
AND posts review comments back to GitHub — with the write side gated
by the Harness Mode approval flow (`agentflow-harness`'s P-H.2
`HookedTool` + `ApprovalProvider`). The original A2
[code-reviewer skill](../code-reviewer/) covered only the read side
because the write side requires the approval gate.

This binary covers the write side: read a local commit via
`git show`, analyse the diff, and write a structured review ledger to
disk via `file:write` — both tool calls intercepted by the approval
gate so the operator can allow / deny / scope decisions before any
mutation happens.

## Why not `agentflow harness run --skill`?

Investigation while building this surfaced **F-A2-11**:
`agentflow harness run` CLI today builds the agent via
`SkillBuilder::build` but does **NOT** call `wrap_registry(...)` to
install the `HookedTool` + `ApprovalProvider` pipeline. Only
`agentflow-server`'s `LiveHarnessExecutor` wires it. So if you want
to dogfood Harness approval from CLI today, you have to wire the
pipeline yourself — which is what this binary does. The end-result
is essentially the reduced form of what the CLI SHOULD eventually do
for skills with write tools.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      ReActAgent loop                         │
│                                                              │
│  step 1                              step 2                  │
│  shell `git show <commit>`           file:write ledger.json  │
│     │                                   │                    │
│     ▼                                   ▼                    │
│  ┌──────────────────┐               ┌──────────────────┐     │
│  │  HookedTool      │               │  HookedTool      │     │
│  │  pre-hook + ── ▶ │ ApprovalProvider (Cli / AutoAllow) │   │
│  │  policy check    │               │                  │     │
│  │  + escalation    │               │  Production:     │     │
│  │  to Critical     │               │  NonIdempotent → │     │
│  │  for NonIdempo   │               │  RequireApproval │     │
│  └──────────────────┘               └──────────────────┘     │
│     │ allow                            │ allow               │
│     ▼                                  ▼                     │
│  inner ShellTool                    inner FileTool           │
│  (SandboxPolicy:                    (SandboxPolicy:          │
│   only `git`)                        only paths under /tmp)  │
│                                                              │
│  approval_requested / approval_decided events →              │
│      SinkChain → StdoutEventSink (jsonl on stdout)           │
└──────────────────────────────────────────────────────────────┘
```

Key wiring (see `src/main.rs`):

```rust
let policy = Arc::new(SandboxPolicy {
  allowed_commands: vec!["git".to_string()],
  allowed_paths:    vec![PathBuf::from("/tmp")],
  ..SandboxPolicy::default()
});
let mut registry = ToolRegistry::new();
registry.register(Arc::new(ShellTool::new(policy.clone())));
registry.register(Arc::new(FileTool::new(policy.clone())));

let approval: Arc<dyn ApprovalProvider> = if args.auto_approve {
  Arc::new(AutoAllowApprovalProvider::new())
} else {
  Arc::new(CliApprovalProvider::stdin())
};
let sinks = SinkChain::new().push(Arc::new(StdoutEventSink::new()));
let hook_config = HookConfig::new(session_id, approval, sinks)
  .with_profile(HarnessProfile::Production);   // ← F-A2-12

let wrapped_registry = wrap_registry(registry, hook_config);
let agent = ReActAgent::new(config, memory, Arc::new(wrapped_registry));
```

The `with_profile(HarnessProfile::Production)` is **load-bearing** —
without it, the default `Local` profile silently auto-allows
NonIdempotent calls and the approval gate never fires (F-A2-12).

## What this validates in AgentFlow

- `agentflow-harness::wrap_registry` + `HookConfig` end-to-end:
  every registered tool wrapped with hook + approval pipeline.
- `HarnessProfile::Production` auto-escalation of `NonIdempotent`
  tools (shell, file:write) → `RequireApproval`.
- `ApprovalRequest` payload completeness: `tool`, `source`,
  `permissions`, `idempotency`, `params_summary`, `risk`, `reason`,
  `expires_at` all populated and round-tripped through the JSONL
  event stream.
- `AutoAllowApprovalProvider` for CI smoke and
  `CliApprovalProvider::stdin()` for interactive operator flow —
  both wired without changing the rest of the agent setup.
- `ApprovalDecision` with `scope` (once / session / run) and
  `decided_by` carried through `approval_decided` events.
- `StdoutEventSink` → JSONL on stdout, suitable for piping into
  monitors / dashboards / `agentflow trace replay`.

## External dependencies

| Dep | How to satisfy |
| --- | --- |
| LLM API key | Default model `moonshot-v1-128k`; needs `MOONSHOT_API_KEY`. Auto-loaded from `~/.agentflow/.env` (P9.3). |
| git | Required (binary calls `git show`). Any local repo with at least one commit. |

## Files

```
code-reviewer-write/
├── README.md     # ← this file
├── Cargo.toml    # standalone Cargo project, path deps to agentflow-*
└── src/main.rs   # CLI + registry wrap + ReActAgent
```

## Run

```bash
cd examples/applications/code-reviewer-write
# MOONSHOT_API_KEY auto-loaded from ~/.agentflow/.env

# Auto-approve mode — CI smoke; both tool calls auto-allowed,
# approval_* events still fire and are captured on stdout JSONL.
cargo run --release -- \
  --commit 11b3707 \
  --ledger /tmp/pr-review.json \
  --auto-approve

# Interactive mode — operator gets stderr prompts for each tool call.
# Stdin accepts y/yes (once), s/session, r/run, n/no (deny),
# q/quit (deny-and-stop).
cargo run --release -- --commit 11b3707 --ledger /tmp/pr-review.json

# Recommended for moonshot-v1-128k (F-A2-13): pre-fetch the diff out
# of band, register only FileTool. Reliably reaches file:write.
cargo run --release -- --commit 11b3707 \
  --ledger /tmp/pr-review.json --prefetch-diff --auto-approve
# ── Harness approval request ──
#   tool: shell (step=0)
#   risk: Critical   idempotency: NonIdempotent
#   source: Builtin
#   permissions: [ProcessExec]
#   reason: production profile: mutating tool requires explicit approval
#   params: {"command":"git show 11b3707"}
# Allow this call? [y]es / [s]ession / [r]un / [n]o / [q]uit:

# Stdout: JSONL stream of approval_requested / approval_decided events.
```

CLI flags:

| Flag | Default | Notes |
| --- | --- | --- |
| `--commit <ref>` | (required) | git ref / hash, passed verbatim to `git show` |
| `--ledger <path>` | `/tmp/pr-review-ledger.json` | output JSON path (must be under /tmp by sandbox policy) |
| `--model <name>` | `moonshot-v1-128k` | any agentflow-llm-registered model |
| `--auto-approve` | off | bypass interactive approval (uses `AutoAllowApprovalProvider`) |
| `--prefetch-diff` | off | run `git show` outside the agent, inline diff into the prompt, register only `FileTool` — isolates the file:write approval path from F-A2-13's shell-loop pathology |

## Validated end-to-end (2026-05-18 iter 2)

Four scenarios exercised; all evidence captured in `/tmp/stdout*.log`
JSONL streams or stderr prompts.

### 1. AutoAllow + shell tool — approval gate fires per call

```bash
printf "" | cargo run --release -- --commit 11b3707 \
  --ledger /tmp/x.json --auto-approve
```

Result: 4× `approval_requested` for `shell` (`{"command":"git show 11b3707"}`)
+ 4× `approval_decided` (`{"decision":"allow","scope":"once","decided_by":"auto:allow"}`),
all params identical (no hallucination after persona inlining). Model
loops on shell instead of advancing to file:write — see F-A2-13 —
so this run stops at `MaxToolCalls{max_tool_calls:4}`. The approval
gate works flawlessly regardless of whether the model reaches step 2.

### 2. AutoAllow + `--prefetch-diff` — full happy path with ledger on disk

```bash
printf "" | cargo run --release -- --commit 11b3707 \
  --ledger /tmp/pr-ledger-prefetch.json --auto-approve --prefetch-diff
```

`--prefetch-diff` runs `git show` outside the agent and inlines the
diff into the user prompt; only `FileTool` stays registered. Bypasses
F-A2-13. Result:

- 1× `approval_requested` for `file` with `params: {"path": "/tmp/pr-ledger-prefetch.json", ...}`
- 1× `approval_decided` (auto:allow)
- Ledger written:
  ```json
  {
    "commit": "11b3707",
    "reviewed_at": "2024-03-05T14:37:00Z",
    "findings": [],
    "verdict": "approve"
  }
  ```
- Stop reason: `FinalAnswer` (clean exit)

(Findings empty + hallucinated reviewed_at timestamp are LLM-quality
issues from `moonshot-v1-128k`, not Harness-side concerns.)

### 3. Cli approval + session-scope allow — caching works

```bash
printf "s\n" | cargo run --release -- --commit 11b3707 --ledger /tmp/x.json
```

Result: stderr printed the full `── Harness approval request ──`
block (tool / risk / permissions / source / params / reason), stdin
`s` parsed as session-scope allow, subsequent identical `git show
11b3707` calls within the session did **not** re-prompt — confirming
the session-scope cache works.

### 4. Cli approval + deny — ledger NOT written

```bash
printf "n\n" | cargo run --release -- --commit 11b3707 \
  --ledger /tmp/pr-ledger-deny.json --prefetch-diff
```

Result:
- 1× `approval_requested` for `file`
- 1× `approval_decided` (`decision:"deny", scope:"once"`)
- `/tmp/pr-ledger-deny.json` does **not** exist
- Stop reason: `FinalAnswer` (agent gracefully reported deny reason)

This proves the gate is fail-closed: the inner `FileTool` never
executes when the operator says no, and the agent doesn't crash —
it observes the deny and threads the reason into its final answer.

## What's not in iteration 1

- **Real GitHub PR comment posting** (the original A2 stretch goal). Today the "write side" is a local JSON ledger; the conceptual equivalent of `gh pr review --comment ...` would just swap `FileTool` for an `HttpTool` POST to GitHub's API. The approval flow doesn't care which `NonIdempotent` tool is on the other side.
- **PlanExecuteAgent wrapping**. ReAct's free-form loop hit the F-A2-13 model-loop pathology; a `PlanExecuteAgent` with a pre-committed 2-step plan would force the model through the script without re-deciding on each tool call.
- **Receipt rendering**. Today the agent's `final_answer` is the only operator-facing summary; a follow-up could render a short stdout receipt after the ledger write.

## Findings during dogfooding

See [`EXAMPLES_TODOs.md` § A2](../../../EXAMPLES_TODOs.md#a2--code-reviewer):

- **F-A2-9 — Harness approval gate** ✅ CLOSED end-to-end via this binary.
- **F-A2-11** — `agentflow harness run` CLI doesn't wire the
  approval pipeline; manual wrap_registry needed today.
- **F-A2-12** — `HarnessProfile::Local` (default) silently
  auto-allows; need `Production` (or explicit pre-hook) for the
  approval gate to fire.
- **F-A2-13** — `moonshot-v1-128k` loops on identical tool calls;
  needs persona-side commit inlining + stronger model or
  `PlanExecuteAgent` wrapping for production use.
