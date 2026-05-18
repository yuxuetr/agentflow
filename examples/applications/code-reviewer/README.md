# A2 — code-reviewer

**Status**: WIP — live ✅ as L3 skill (2026-05-18 first run on commit
`11b3707`, kimi-k2.6, ~200s wall clock, high-quality review).
**Tracking entry**: [`EXAMPLES_TODOs.md` § A2](../../../EXAMPLES_TODOs.md#a2--code-reviewer)
**Sibling**: this is L3's second validation case (A1.5 was first).

## Business

Read-only code reviewer. Input: a git commit hash, a git ref range
(`HEAD~3..HEAD`), or a GitHub PR identifier (`#42`, `owner/repo#42`,
full PR URL). Output: structured markdown review grouped by severity
(🔴 Critical / 🟡 Important / 🔵 Minor) + Strengths + Verdict
(Approve / Approve-with-comments / Request-changes).

## Architecture: L3 skill (ReAct + shell tool with `git`+`gh`)

```
┌─────────────────────────┐  spawns        ┌────────────────────┐
│ agentflow skill run     │ ──shell tool──▶ │ git / gh subprocs  │
│ kimi-k2.6 ReAct loop    │ ◀──stdout──── │ (admission-gated)  │
│  - reads diff           │                 │                    │
│  - decides what matters │                 │                    │
│  - composes review      │                 │                    │
└─────────────────────────┘                 └────────────────────┘
       │
       ▼
  user prompt →
  "Review commit <ref>"  /  "Review PR #N in owner/repo"
       │
       ▼
  agent loop (2-N tool calls per session):
  1. shell: git show --stat <ref>     OR  gh pr view <ref>
  2. shell: git show <ref>            OR  gh pr diff <ref>
  3. (optional) shell: git show <ref>:<path>  for context on hunks
  → markdown review as final_answer
```

## Why L3 skill works here (vs A7's L1 binary)

A7's changelog-writer was L1 binary because the task is fixed-pipeline
pass-through (no agent decisions, just thread the range through `git
log` → LLM → file). A2 is the opposite — every input requires the
agent to:

- Classify the reference type (commit hash vs ref range vs GitHub PR)
- Pick the right shell command (`git show` vs `git diff` vs `gh pr ...`)
- Decide whether to fetch additional context files based on diff hunks
- Decide which findings are worth flagging at each severity
- Compose a structured review

This is **genuine agent-decides territory** → L3 skill is the right
tier per the L1+L3 reflection rule.

## Files

```
code-reviewer/
├── README.md                              # ← this file
├── skill.toml                             # persona + model + shell tool
└── sample-reviews/                        # real review outputs as fixtures
    └── commit-11b3707-A1-podcast.md       # first dogfooding output
```

## External dependencies

| Dep | How to satisfy |
| --- | --- |
| `git` | System install (already on PATH for any dev box). |
| `gh` CLI | `brew install gh` / `apt install gh`. For GitHub PR reviews, also `gh auth login` once. |
| `MOONSHOT_API_KEY` | Auto-loaded from `~/.agentflow/.env` (P9.3). Default model is `kimi-k2.6` so it must be in the user's models.yml (use the workspace template's entry as reference; A7 dogfooding added it). |

## Run

```bash
# Review a local commit (no GitHub network needed):
/Users/hal/.target/release/agentflow skill run \
  examples/applications/code-reviewer \
  --message "Review commit 11b3707"

# Review a ref range:
/Users/hal/.target/release/agentflow skill run \
  examples/applications/code-reviewer \
  --message "Review changes in HEAD~3..HEAD"

# Review a GitHub PR (needs gh auth):
/Users/hal/.target/release/agentflow skill run \
  examples/applications/code-reviewer \
  --message "Review PR #42 in owner/repo"

# Capture the review markdown (avoids the F-A2-1 display bug below):
/Users/hal/.target/release/agentflow skill run \
  examples/applications/code-reviewer \
  --message "Review commit <hash>" --trace 2>&1 \
  | python3 -c "
import json, sys
raw = sys.stdin.read()
i = raw.find('Runtime Trace:')
j = raw.find('{', i)
depth = 0; k = j
while k < len(raw):
  if raw[k] == '{': depth += 1
  elif raw[k] == '}':
    depth -= 1
    if depth == 0: k += 1; break
  k += 1
print(json.loads(raw[j:k]).get('answer', ''))
"
```

## First-run observations (2026-05-18, commit 11b3707)

- **End-to-end wall clock: ~200s** for a 1166-line diff via kimi-k2.6.
  Two shell tool calls (`git show --stat 11b3707`, `git show 11b3707`)
  + significant LLM thinking on the long diff.
- **Output quality**: high. Caught 5 genuinely real issues in A1's
  code:
  - 🔴 phonon path deps point outside the repo
    (`../../../../../rustspace/phonon/...`) — non-portable across hosts/CI
  - 🟡 `info!(summary = %json!({"status":"ok"}), ...)` hack to silence
    dead-code warning (the comment "Touch json! to keep the dep used"
    is fair admission)
  - 🟡 3 duplicated match arms in `render_audio` (MiniMax/Edge/OpenAi)
    could be `Box<dyn TtsProvider>` to reduce duplication
  - 🟡 `unsafe { env::set_var/remove_var }` in tests is UB on
    multi-thread despite the "single-threaded" comment
  - 🟡 `override_script_voices_for_tts` matches speaker name verbatim,
    fragile to LLM-emitted whitespace/case variants
- **Severity calibration**: reasonable — distinguishes "this will
  break on CI" from "this is style polish".
- **Output rendering bug**: top-level `🤖 Agent: <answer>` line printed
  EMPTY despite the answer being present in the trace JSON (see
  F-A2-1 in findings). The Python extraction snippet above is the
  current workaround.

## What this validates in AgentFlow

- L3 ReAct + shell tool path with multi-command admission
  (`allowed_commands = ["git", "gh"]`) works end-to-end.
- kimi-k2.6 honours persona's anti-substitution instruction
  ("严格使用用户给的这个 reference 原文") when the input is unambiguous
  — no repeat of A7's hallucination failure mode.
- `--trace` JSON dump is the system of record for the actual agent
  output when the human-readable summary line is missing.

## Operating practice: LLM review is non-deterministic (F-A2-5)

Two runs of this skill on the same commit (`11b3707`, identical
persona, identical model `kimi-k2.6`) produced **almost-disjoint
finding sets** — both correct, both useful, only 1 of ~11 unique
issues was flagged by both passes:

| Pass | Findings caught (unique) |
| --- | --- |
| Run 1 (live stderr trace) | unsafe `env::set_var` in tests · `/tmp` hardcoded paths · `edition = 2024` compatibility · return-vs-panic skip pattern · `media_type` case sensitivity |
| Run 2 (saved sample fixture) | phonon path deps pointing outside the repo · `json!` hack to silence dead-code warning · 3 duplicated match arms in `render_audio` · unsafe `env::set_var` in tests · fragile speaker-name verbatim match · `&Path` vs `&PathBuf` idiom · lost error chain in `io::Error` mapping |

Shared finding: `unsafe env::set_var` (1 / ~11). **The two reviews are
each genuinely correct but explore different facets of the same diff.**
This is intrinsic to LLM sampling; raising the temperature or
swapping models won't make it deterministic in the way `cargo clippy`
is.

**Practice for this skill:**

- For ad-hoc human review, run the skill **3–5 times on the same
  commit** and union the findings before triaging. Iterations 3+
  typically surface fewer new issues; coverage curves flatten quickly.
- For PR gates, **don't fail on a single run**. Run N times and
  accept "≥2 of N flag the same issue" as the bar for "must
  address before merge"; treat single-pass findings as advisory.
- Two passes that share zero findings doesn't mean either is wrong
  — it means the model sampled different attention slices. Read both
  and act on the union.
- If you need true determinism, this is the wrong tool — use
  `cargo clippy` / `rust-analyzer` diagnostics / a linter ruleset.
  LLM review complements those by catching things they can't
  (intent, design taste, comment vs code drift), at the cost of
  reproducibility.

The same caveat applies to any LLM-judgement skill (content
moderation, evaluation, scoring). See `examples/README.md`
§ Conventions for the cross-cutting version.

## Findings during dogfooding

See [`EXAMPLES_TODOs.md` § A2 Findings](../../../EXAMPLES_TODOs.md#a2--code-reviewer)
for the live list.

## Future iterations (not in this commit)

- **Harness Mode approval gate validation**: add a write-side
  `add_review_comment` tool, gated by Harness's approval flow.
  Requires running via `agentflow harness run` instead of
  `skill run`; covers the third pillar of A2's original spec.
- **MCP GitHub server alternative**: swap `gh` shell calls for
  the official `@modelcontextprotocol/server-github` MCP server
  via `[[mcp_servers]]`. Validates the same skill against MCP-
  based instead of shell-based tool surface.
- **Multi-PR batching**: review N PRs in one run, output a
  consolidated digest. Tests memory limits + parallel-like
  patterns.
