# RFC: Code-Execution Trust Classification

- Status: **Decided** — adopted as the design basis for the S0 quick-fix wave.
- Parent: 2026-07-23 sandbox review (conversational code walkthrough, not a
  full audit). Scope of the review: `agentflow-tools/src/sandbox/*` +
  `builtin/{shell,script,file}.rs` + `agentflow-skills/src/builder.rs` +
  `agentflow-agents/src/dynamic.rs`.
- Tracking: `TODOs.md` §S0–S4. S0 items cite this RFC (`Refs S0.x`) in their
  commits; S1–S4 build on the model defined here.
- Scope: threat model + design principle only. Concrete remediations are
  tracked as separate TODO items (S0.2, S0.3, S1, ...); this document is the
  shared reference they point back to, not an implementation plan by itself.

## Problem

`agentflow-tools`' sandbox has two real layers today — the in-process
[`SandboxPolicy`](../agentflow-tools/src/sandbox/policy.rs) (allow-lists +
deny-by-default checks) and the OS-level `SandboxBackend`
(`sandbox-exec` / seccomp, opt-in via `os_sandbox`). Both layers answer the
question **"what is this tool allowed to touch?"** Neither layer answers a
different, equally important question: **"who produced the content this tool
is about to execute or write?"**

That second question matters because the tools are not symmetric. `file`
*produces* content (whatever the LLM decides to write, this turn, based on
the conversation). `script` and `shell` *execute* content. A sandbox that
gates both by the same path/command allow-list, but never asks whether the
content at that path was placed there by a human before the run started or
by the model during the run, cannot distinguish "run the vetted script the
skill author shipped" from "run whatever the model just wrote to disk" — even
though those are completely different risk profiles. The allow-list keeps
you inside a directory; it says nothing about what's allowed to land there
and then be interpreted as code.

Concretely, `agentflow-skills::builder::build_tool_registry` merges every
declared tool's constraints into **one shared `Arc<SandboxPolicy>`**
(`build_sandbox_policy`, `builder.rs:535`), and when a skill declares a
`script` tool, that merge step injects two script-specific defaults into the
*shared* policy so the script tool can find its own scripts:

- `allowed_commands` gains `python3` / `bash` / `node` (`builder.rs:584-594`).
- `allowed_paths` gains `<skill_dir>/scripts` when nothing else populated it
  (`builder.rs:596-598`).

Because `file` and `shell` read from the *same* policy object, both defaults
leak into tools whose content is LLM-generated:

- A skill declaring `file` + `script` (no other `allowed_paths`): the LLM can
  `file.write` a new `.py`/`.sh`/`.js` file into `scripts/`, then `script`
  will execute it — `ScriptTool::execute` only checks that the target is
  *some* file inside `scripts_dir` with a supported extension
  (`script.rs:149-194`); it does not check who wrote it or when.
- A skill declaring `file` + `shell` + `script` together: `shell`'s argv
  parser checks only the first token against `allowed_commands`
  (`shell.rs:183-192`) — it never validates path *arguments*. Once `bash` /
  `python3` / `node` leak into the shared `allowed_commands` (because
  `script` is present), `shell` can run `bash <any allowed path>`,
  bypassing every one of `script`'s own containment checks (extension
  allow-list, `scripts_dir` prefix, symlink-escape rejection) entirely.

Both are instances of the same root cause: **a tool's own default privileges
were merged into a policy object other tools also read**, and the design
comment that justifies sharing the merged policy — "each built-in tool only
checks its relevant policy field, so merging is safe" (`builder.rs:250`) —
is only true if no tool's *own* defaults populate a field another tool
*does* check. `script`'s defaults populate exactly the two fields (`paths`,
`commands`) that `file` and `shell` respectively check. The sharing
invariant was violated by construction, not by a missing allow-list entry.

## Non-goals

This RFC does not re-litigate the existing allow-list mechanics
(`SandboxPolicy`, `SandboxBackend`) — those stay as the *reachability*
layer. It does not propose a full code-signing or capability-token system.
It defines one classification, one placement rule, and leaves the concrete
per-tool fixes to the TODO items that reference it.

## Trust levels

Three levels, ordered by how much the runtime should be willing to
*interpret* content at that level as code:

1. **Author-signed** — content that shipped with the skill package or was
   declared in its manifest, and existed *before* the run started: files
   under `<skill_dir>/scripts/` at install time, the skill's declared MCP
   server binaries + argv (`security.mcp_command_allowlist`), plugin
   binaries. A human (the skill author, or the operator who installed the
   skill) reviewed or at minimum chose to trust this content prior to any
   agent turn. This is the **only** level eligible for direct interpretation
   (interpreter exec, MCP JSON-RPC dispatch, plugin subprocess).
2. **User-provided** — content a human operator supplies at *invocation*
   time and outside the model's control: a `--allow-path` grant, a file the
   operator explicitly attaches, CLI arguments. Eligible for the reachable
   *data* operations (read, and — where the operator's own grant says so —
   write), but never auto-promoted to execution. If a human wants to run a
   user-provided file, that has to be an explicit, separately-authorized
   action, not a side effect of an LLM tool call sequence.
3. **LLM-generated** — anything produced by model output during a run:
   `file.write` content, `script`/`shell` tool-call *arguments*, dynamic
   workflow `WorkflowPlan` step params. This is the level every prompt
   injection or model mistake writes at. It is eligible to be *stored*
   (written to an allowed path) and to be *passed as data* into an
   already-trusted channel (e.g. piped to a script's stdin, per
   `script.rs:219-222`, or serialized as a tool-call argument to a
   pre-registered MCP tool) — but it must never itself become the thing
   that gets `exec()`'d, loaded as an interpreter's script argument, or
   dispatched as a new MCP/plugin command line. Promoting llm-generated
   content to executable requires a level transition performed by a human
   (see S4, gated behind its own RFC) — never an automatic one.

## Design principle

> **A tool's execution channel may only interpret content whose trust level
> was fixed at registration time (author-signed) or explicit grant time
> (user-provided). Content that entered storage during the run
> (llm-generated) may be data to an execution channel; it may never become
> the channel's own code path.**

Two corollaries this RFC uses to judge every fix in S0–S4:

- **Don't let one tool's execution-boundary defaults leak into a policy
  object another tool reads.** If `script` needs `python3`/`bash`/`node` and
  `scripts/` reachable to do its own job, those defaults belong to a policy
  scoped to `script` alone — never to a policy `file` or `shell` also
  consult, even implicitly via a "merge everything" convenience path. This
  is what makes the current `build_sandbox_policy` merge unsafe: it treats
  "safe to share because fields are disjoint" as a static property, when in
  practice one tool's *defaults* (not just explicit author config) populate
  fields the sharing tools do check.
- **Execution boundaries are content-addressed by trust level, not just by
  path.** `scripts_dir` being on an allow-list is necessary but not
  sufficient for `script` to be safe — the deeper invariant is "only files
  that existed before this run's first LLM turn, in `scripts_dir`, with the
  skill's declared extensions, are execution-eligible." Path allow-listing
  approximates that invariant only as long as nothing LLM-controlled can
  write into the same path. S1 (manifest script inventory + content hash)
  makes the invariant exact instead of approximate; S0 is the interim fix
  that stops the approximation from being silently false.

## How this maps onto the S-track TODOs

| Item | What it fixes, in this vocabulary |
|---|---|
| S0.2 | Stop `script`'s own defaults (interpreters, `scripts/`) from populating the policy object `file`/`shell` read; `file` additionally never gets `scripts/` even from explicit config, since an LLM writing there always breaks the author-signed invariant regardless of who configured the allow-list. |
| S0.3 | Confirm the dynamic-workflow tool registry (LLM-authored plan, `agentflow-agents::dynamic`) has no llm-generated → execution path today (no `script`/`shell`/MCP tool registered by the shipped CLI surface), and pin that as a regression test rather than an incidental fact. |
| S1 | Replace "any file under `scripts_dir`" with "a file listed, by content hash, in the manifest at install time" — turns the approximate path-based boundary into an exact author-signed boundary. |
| S2 | Per-skill dependency environments are still author-signed content (declared in the manifest, materialized at install time) — same trust level as S1, different concern (environment isolation, not content provenance). |
| S3 | OS-backend hardening raises the cost of a successful escape *after* something at the wrong trust level got executed; it's defense in depth, not a substitute for keeping llm-generated content out of the execution channel in the first place. |
| S4 | The only place this RFC's default ("llm-generated is never directly executable") is meant to be lifted — and only behind its own RFC, with an isolation backend strong enough that "the model's code runs" is an accepted, contained outcome rather than an accident. |

## Alternatives considered

- **Capability tokens / code signing per file.** Rejected for S0 scope: real
  fix for S1, but too heavy for a "quick fix wave" whose job is to stop the
  *implicit* leak, not build a new trust infrastructure. S1 picks this up
  (manifest hash list) at the right scope.
- **Leave the shared `SandboxPolicy` merge as-is and rely on `os_sandbox`.**
  Rejected: `os_sandbox` is opt-in and defaults to `false`
  (`manifest.rs:239`); the in-process policy is the only enforcement most
  skills get today, and S3 (raising the OS backend to a safe default) is
  explicitly sequenced *after* S0 in the TODO dependency chain
  (`S0 → S1 → (S2 ∥ S3) → S4`) — it cannot be the thing S0 leans on.
- **Block `file` writes to `scripts_dir` only when `allowed_paths` was
  empty (i.e., only patch the implicit-default case).** Rejected in favor of
  an unconditional exclusion: an explicit author grant of `scripts/` to
  `file` has the same effect (LLM-writable execution directory) as the
  implicit leak; the trust argument against it doesn't depend on how the
  policy got populated.
