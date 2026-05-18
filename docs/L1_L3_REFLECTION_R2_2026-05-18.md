# L1 + L3 Integration Validation — Reflection R2

**Date**: 2026-05-18 (same day as R1, later in the session)
**Supersedes**: [R1](L1_L3_REFLECTION_2026-05-18.md) for the action queue and
the L1↔L3 selection rule. R1 retained as the first-pass historical
snapshot — its analysis is still correct as far as it went; R2 just
has more evidence.
**Trigger**: A7 (changelog-writer) and A2 (code-reviewer) ran end-to-end
after R1. A7 took an unexpected route (L3 skill attempted and rejected,
pivot to L1 binary); A2 worked as L3 skill first try. Together with R1's
A1 + A1.5, we now have 4 applications spanning both tiers with
contrasting evidence.

---

## 1. What changed since R1

R1 covered 2 applications (A1 L1, A1.5 L3) and 20 findings. R2 adds:

- **A7 changelog-writer**: started as L3 skill, **failed**, pivoted to
  L1 binary, **succeeded**. 10 new findings.
- **A2 code-reviewer**: L3 skill first try, **succeeded**. 10 new
  findings.
- **Total: 4 applications, 40 findings across 4 tiers' worth of
  empirical evidence**.

| App | Intended tier | Actual tier | Outcome | Wall-clock | Findings |
| --- | --- | --- | --- | --- | --- |
| A1 blog-to-podcast | L1 | L1 | ✅ | 19s | 10 |
| A1.5 podcast-mastering | L3 | L3 | ✅ | 37s | 10 |
| A7 changelog-writer | L3 (skill) → L1 (binary) | L1 | ✅ after pivot | 117s | 10 |
| A2 code-reviewer | L3 (skill) | L3 | ✅ first try | 200s | 10 |

The "A7 intended L3 / actual L1" row is the most important data point
in R2 — it falsified the R1 rule's first phrasing and forced a
refinement (see §3 below).

## 2. Refined L1↔L3 selection rule

R1's rule was:

> Fixed pipeline → L1 (no LLM-in-loop tax);
> agent picks tools / branches → L3 (LLM-in-loop tax unavoidable).

R2 keeps this rule but **adds a critical second axis**:

> **Pass-through input forwarding → L1 even if it looks like agent
> decides**. When the workflow requires the LLM to faithfully forward
> a user-given string (commit hash, ref range, PR URL, version tag, file
> path, regex, etc.) into a tool call, L3 ReAct is fragile: the LLM
> substitutes the user input with hallucinated "typical example" values
> from its training data. L1 (verbatim threading through
> `std::process::Command::new`-style code paths) is the correct tier.

### Empirical evidence for the refinement

**A7 as L3 skill (rejected)**: user input was `v0.2.0..HEAD` (a real
tag), `HEAD~10..HEAD` (a relative ref), and `v1.0.0-rc.1..HEAD` (a
nonexistent tag the user said in conversation). Across moonshot-v1-128k
and kimi-k2.6, the agent substituted **every one** with hallucinated
ranges like `v1.0.0..v1.1.0`, `v1.0.0..v2.0.0`, `v1.2.3..v1.3.0`.
Persona instructions to "use the user's exact string" did not stop
the substitution; tightening max_iterations to 3 did not stop it;
removing example refs from the persona did not stop it. Five
iterations failed; binary pivot worked first try.

**A7 as L1 binary (succeeded)**: the range string flows as a literal
through `initial_inputs` → `RunGitLogNode` → `Command::new("git")
.arg("log").arg(&range)`. No LLM involved in input handling. 399
commits processed in 117s; output usable.

**A2 as L3 skill (succeeded)**: user input was `Review commit 11b3707`.
The agent's persona explicitly says "use the user's exact reference,
do not substitute". The agent's job after that is **decisions**, not
pass-through: which shell command to run for this reference type;
which files in the diff to look at deeper; how to categorize each
issue's severity; how to balance Issues / Strengths sections. kimi-k2.6
honoured the anti-substitution instruction here (input `11b3707`
flowed through verbatim into `git show 11b3707`) because the rest of
the work was high-decision-density and the persona's first-class
instruction held.

### Operational implication

When designing a new agent application:

1. **List the user-provided values**. Are any of them strings that
   must reach a tool call verbatim?
2. **Count the genuine decisions** the agent will make. Multiple
   tool-choice / file-selection / severity-scoring / sequencing
   decisions per session = L3 sweet spot.
3. **Score each task**:
   - High pass-through + low decisions → **L1 binary**
   - Low pass-through + high decisions → **L3 skill**
   - High pass-through + high decisions → **L1 with embedded
     one-shot LLM calls** (A7's actual shape: shell out for the
     pass-through, then one-shot LLM for the LLM-worthy part)
   - Low pass-through + low decisions → reconsider — probably
     doesn't need an LLM at all

A2 and A1.5 fall in the L3 sweet spot. A1 and A7 fall in L1 (A1 is
all pipeline, A7 is pass-through + one-shot LLM).

## 3. Updated empirical data points

| Metric | A1 (L1) | A1.5 (L3) | A7 (L1) | A2 (L3) |
| --- | --- | --- | --- | --- |
| Wall clock | 19s | 37s | 117s | 200s |
| LLM calls | 1 script_gen + 12 TTS | 7 (ReAct) | 1 (one-shot) | 2-3 (shell + final) |
| LLM cost (estimate) | ~¥1 | ~¥0.02 | ~¥0.1 | ~¥0.05 |
| Per-LLM-call latency | TTS 150-300ms ea | Decisions 2-15s ea | One-shot 116s | Decisions 30-80s ea |
| Dominant cost | concurrent TTS network | LLM thinking (ReAct) | one big-context LLM | LLM thinking (ReAct) |
| Lines of project Rust | ~450 | 0 (skill manifest only) | ~280 (binary + 2 nodes) | 0 (skill manifest only) |
| Operator surface | binary CLI flags | natural-language skill prompt | binary CLI flags | natural-language skill prompt |

**Two new conclusions** from R2's data:

- **L1 wall clock is dominated by the work, not by overhead**. A1's
  12 TTS calls take most of its 19s; A7's one 354k-char LLM call takes
  most of its 117s. The agentflow `Flow` orchestration adds milliseconds.
- **L3 wall clock is dominated by LLM thinking per decision turn**.
  A1.5's 7 turns × ~5s each ≈ 35s. A2's 2-3 turns × 30-80s each ≈ 200s
  (longer per turn because each turn processes a 1166-line diff in
  context). The MCP / shell IPC roundtrips are sub-second; the cost
  is the LLM decision time.

## 4. L2 verdict — now CLOSED, not just DEFERRED

R1 deferred L2 with the reasoning "L1+L3 covered all observed
scenarios; L2 saves milliseconds out of seconds". R2 confirms with
4 applications instead of 2: **still zero scenarios surfaced** that
need L2 (agentflow `Tool` trait wrap for in-process LLM-driven tool
calling).

A2 was the most likely candidate — same-process, agent decides — but
even there L3 (subprocess shell or eventual MCP) is fine because:
- Tool calls are sub-second (shell `git show`, MCP `audio_*`)
- The dominant cost is LLM thinking, not IPC
- The decision surface fits naturally into ReAct, doesn't need
  same-process semantics

**Decision change**: mark L2 as **CLOSED — not in scope** instead of
DEFERRED. Promotion criteria written as a safety valve:

> Re-open L2 only if a future application emerges where ALL of:
> (a) IPC overhead is measurably > 10% of total wall clock,
> (b) the agent makes ≥5 tool-call decisions per session,
> (c) same-process state (large in-memory cache / handle that doesn't
>     fit through JSON-RPC) is genuinely required and can't be
>     re-architected as L3 + handle registry.

If none of (a)/(b)/(c) hold simultaneously, prefer L1 or L3.

## 5. All 40 findings — re-categorized

Combining R1's 20 + R2's 20.

### 5.1 agentflow code changes (now 7 items, 3 DONE since R1)

| ID | Source | Status | Summary | Pri |
| --- | --- | --- | --- | --- |
| F-AF-1 | R1 A1 | **DONE P9.1** | `skill validate` swallowed underlying error; switched main to `{:#}` Display | — |
| F-AF-2 | R1 A1.5 | TODO P9.4 | SKILL.md `model:` silently ignored | M |
| F-AF-3 | R1 A1 | **DONE P9.3** | Auto-load `~/.agentflow/.env` in CLI entry | — |
| F-AF-4 | R1 | TODO | Crisper Moonshot/Anthropic init error on fresh hosts | L |
| **F-A2-1** | R2 A2 | **DONE 2026-05-18** | Actual root cause was different: when `max_tokens` truncates LLM response mid-JSON, parser falls to Malformed and shows the raw `{"thought":..,"answer":..` envelope. Fix: best-effort `answer` field extraction in `react/parser.rs` + `warn!` log hinting at `max_tokens`. 6 new tests. | — |
| **F-A7-2** | R2 A7 | **DONE 2026-05-18** | Honesty-note path (not full factory add): permission report's shell branch now emits "not wired into the CLI workflow factory" note so authors don't see misleading "→ exec" classification and assume YAML will run. Full ShellNode factory add deferred — no real need surfaced. | — |
| **F-A7-3** | R2 A7 | **DONE 2026-05-18** | Deleted 6 dead `config/models/*.yml` files. Updated AGENTS.md ×2, IMPLEMENTATION_STATUS.md, GRANULAR_MODEL_TYPES.md docs that misdirected contributors to the wrong file. | — |

### 5.2 phonon code changes (3 items, all unchanged from R1)

| ID | Source | Status | Summary | Pri |
| --- | --- | --- | --- | --- |
| F-PH-1 | R1 A1 | TODO | `#[instrument(fields(...))]` truncate long values | M |
| F-PH-2 | R1 A1 | TODO | `PodcastPipeline::generate` return per-segment durations | M |
| F-PH-3 | R1 A1.5 | TODO | phonon-mcp `audio_info` surface `resampled_from` | L |

### 5.3 Documentation changes (now 6 items, 1 DONE since R1)

| ID | Source | Status | Summary | Pri |
| --- | --- | --- | --- | --- |
| F-DOC-1 | R1 A1.5 | **DONE P9.2** | docs/MCP_SKILLS.md "Spawning native binary MCP servers" section | — |
| F-DOC-2 | R1 A1 | TODO P9.5 | `FlowValue` field reference in docs/AGENT_SDK.md | L |
| F-DOC-3 | R1 A1 | TODO P9.8 | `target_segments` is a hint, not a cap — doc tightening | L |
| F-DOC-4 | R1 A1.5 | TODO | Bump phonon-mcp build instructions to prominent "Pre-flight" section | L |
| **F-A7-4** | R2 A7 | **NEW TODO** | `~/.agentflow/models.yml` silently overrides built-in; `doctor` should report active config source | M |
| **F-A7-5** | R2 A7 | **NEW TODO** | kimi-k2.6 requires `temperature: 1.0` — document in agentflow-llm provider docs | L |

### 5.4 Provider config changes (now 2 items, 0 DONE)

| ID | Source | Status | Summary | Pri |
| --- | --- | --- | --- | --- |
| **F-A7-6** | R2 A7 | partial: added kimi-k2.5/2.6 in this session | agentflow-llm registry lags Moonshot's `/v1/models`; consider `agentflow llm models --refresh-from-api` | L |
| **F-A7-8** | R2 A7 | **DONE 2026-05-18** | Bumped 94 text models in `templates/default_models.yml` from `max_tokens: 4096` → `32768` (multimodal 12 + tts 1 left at 4096 — vision outputs short, tts max_tokens semantics differ) | — |

### 5.5 Example / skill conventions (now 4 items, 0 DONE)

| ID | Source | Status | Summary | Pri |
| --- | --- | --- | --- | --- |
| F-EX-1 | R1 A1.5 | TODO P9.7 | A1.5 persona: add "re-measure LUFS before save" step | L |
| **F-A7-7** | R2 A7 | **NEW TODO** | `dotenvy::from_path("~/.agentflow/.env")` snippet duplicated across standalone application binaries; extract helper crate or document the canonical snippet | L |
| **F-A2-6** | R2 A2 | **NEW TODO** | `--trace` mixes human format + JSON on stdout; add `--output json` mode like harness | M |
| **F-A2-5** | R2 A2 | **NEW (no code fix)** | LLM-based code review is non-deterministic — 2 runs caught completely different issue sets. Document the practice: run multiple times, union the findings; or persona "systematic per-file walk" | M |

### 5.6 Intentional non-fixes / scope deferrals (now 5 items)

| ID | Source | Why no action |
| --- | --- | --- |
| F-NA-1 | R1 A1 | HK guest voice acceptable per user |
| F-NA-2 | R1 A1 | model name fixed in-app, no further action |
| F-NA-3 | R1 A1 | `ConsoleListener` unit struct OK |
| **F-A2-9** | R2 A2 | Harness Mode approval gate explicitly scoped to A2's next iteration |
| **F-A2-10** | R2 A2 | A2's "no real GitHub PR in agentflow repo to test against" — meta-observation, dogfooding on another repo is a future option |

### 5.7 Positive validations — recorded only (now 11 items, doubled from R1's 6)

R1 6: ConsoleListener events / MCP naming / Moonshot tool calling /
AssetRegistry pattern / `--trace` JSON / dotenvy pattern.

R2 5: 
- **F-A7-10**: one-shot LLM categorization on 399 commits exceeded
  expectations (graceful "added GitHub URLs beyond spec")
- **F-A2-2**: L3 works well for genuine agent decisions — empirical
  validation of the L1↔L3 rule via contrast with A7's L3 failure
- **F-A2-3**: kimi-k2.6 honoured anti-substitution persona (A7
  lesson successfully applied)
- **F-A2-4**: review output quality (severity calibration, balance,
  actionability) exceeds expectations on real code
- **F-A2-8**: P9.3 dotenvy auto-load works in `skill run` path
  beyond just `doctor` / `workflow run` — coverage confirmed
- **F-A7-9**: perf data point captured (354k char input → 117s on
  moonshot-v1-128k); long-context cost characterised

### 5.8 Performance data points (now 8 items)

Pure measurement; useful for "did we regress?" baselines later.

| Metric | Value | Source |
| --- | --- | --- |
| A1 (L1 fixed pipeline, 12 TTS) | 19s, ~¥1 | R1 |
| A1.5 (L3 ReAct, 7 turns, 6 tools) | 37s, ~¥0.02 | R1 |
| A7 (L1 one-shot LLM, 354k-char input) | 117s, ~¥0.1 | R2 |
| A2 (L3 ReAct, 1166-line diff, 2 shell calls) | 200s, ~¥0.05 | R2 |
| MCP IPC roundtrip (audio_save) | ~50ms | R1 A1.5 |
| Shell IPC roundtrip (git show) | ~40ms | R2 A7 (git_log node) |
| LLM decision turn latency (moonshot-v1-128k, long context) | 30-80s | R2 A2 |
| LLM decision turn latency (moonshot-v1-128k, normal) | 2-15s | R1 A1.5 |

## 6. Action queue (prioritized, updated)

R1's top-3 (P9.1, P9.2, P9.3) are all **DONE**. New top-3 from R2's
finding set:

| Pri | ID | Action | Owner |
| --- | --- | --- | --- |
| ~~H~~ DONE | F-A2-1 | ~~Populate `AgentRunResult.answer` from `final_answer` event~~ — actual fix was parser truncated-JSON best-effort recovery in `react/parser.rs` (root cause was different from the original framing). Landed 2026-05-18. | agentflow-agents |
| ~~M~~ DONE | F-A7-2 | Honesty-note path landed 2026-05-18. Permission report tells the truth about shell-not-in-factory. | agentflow-cli |
| ~~M~~ DONE | F-A7-8 | Bumped 94 text models 4096 → 32768 in templates/default_models.yml; vision (12) + tts (1) left at 4096. Landed 2026-05-18. | agentflow-llm |
| M | F-A7-4 | `agentflow doctor` reports active models.yml source | agentflow-cli |
| M | F-AF-2 (P9.4) | SKILL.md `model:` field — honour or warn | agentflow-skills |
| M | F-A2-6 | `agentflow skill run --output json` mode | agentflow-cli |
| M | F-A2-5 | Document "LLM review is non-deterministic; run multiple times" practice | examples conventions |
| ~~M~~ DONE | F-A7-3 | Deleted 6 dead `config/models/*.yml` files + updated 4 misdirecting docs. Landed 2026-05-18. | agentflow-llm |
| M | F-PH-1 | Truncate long `#[instrument(fields(...))]` values | phonon |
| M | F-PH-2 | `PodcastPipeline::generate` returns per-segment durations | phonon |
| L | F-DOC-2 (P9.5) | `FlowValue` field reference in docs/AGENT_SDK.md | agentflow docs |
| L | F-DOC-3 (P9.8) | `target_segments` doc tightening | phonon + examples |
| L | F-DOC-4 | Bump phonon-mcp pre-flight prominence | examples |
| L | F-EX-1 (P9.7) | A1.5 persona "verify LUFS before save" | examples |
| L | F-A7-5 | document kimi-k2.6 temp constraint | agentflow-llm |
| L | F-A7-6 | `agentflow llm models --refresh-from-api` | agentflow-llm |
| L | F-A7-7 | dotenvy helper crate or canonical snippet | examples / agentflow-cli |
| L | F-AF-4 | crisper Moonshot/Anthropic init error path | agentflow-llm |
| L | F-PH-3 | phonon-mcp `audio_info.resampled_from` | phonon |

19 open items. 1 High, 9 Medium, 9 Low. None require a core refactor;
all are surface / docs / config / convention scope.

## 7. Phonon scope reflection — still no change

R1 said the 6 essential agent-facing crates
(core / io / wav / ai / podcast / mcp) all get real use; the 7
non-essential crates (engine / cli / server / studio / video / wasm /
plugin) don't show up in any application.

R2 evidence: A7 and A2 didn't touch any new phonon crate beyond the
essential 6. **No new evidence to bump up the 4 maybe-archive crates'
priority** (studio / video / wasm / plugin). Decision unchanged:
optional cleanup, no urgency.

A new positive datapoint: adding `MiniMaxTts` to phonon-ai during R1's
prep took ~150 lines + 16 tests + zero core changes. **Phonon's
provider trait surface scales well to new TTS backends.** This was a
hypothesis-test of "phonon's audio-facing abstractions are well
designed for agent-driven extension" → confirmed.

## 8. Meta-pattern guidelines for future dogfooding

Lessons future applications (A3/A4/A5/A6, plus any new ones) should
apply by default:

### 8.1 Persona design

- **No concrete example values in persona body** — LLMs substitute
  them for user input. Use `<placeholder>` syntax or describe shape
  abstractly. (A7 F-A7-1; A2 F-A2-3 confirmed the fix works)
- **Anti-substitution instruction goes early and is bolded**:
  "**严格使用用户原话里的字符串，不要替换、不要"修正"**".
- **Step-by-step instructions with named tools work** (A1.5 phonon-mcp
  pattern, A2 git/gh pattern). Loose "do whatever you think is best"
  → too much variance.

### 8.2 Model selection

- **kimi-k2.6** for agent-style tasks needing strong instruction
  following (A2). Requires `temperature: 1.0`.
- **moonshot-v1-128k** for long-context one-shot LLM calls (A7's
  354k char input). Bump `max_tokens` for long outputs (F-A7-8).
- **Future iteration**: `kimi-k2.5` not yet tested separately;
  may be better/cheaper than k2.6 for some cases.

### 8.3 Application architecture

- Apply the L1↔L3 tier-selection rule (§2 above) BEFORE writing
  code. A7 wasted ~30 min on the wrong tier.
- **L1 binary template**: copy A1's `Cargo.toml` shape (empty
  `[workspace]`, path deps to agentflow-core + needed crates,
  `dotenvy::from_path("~/.agentflow/.env")` in main).
- **L3 skill template**: copy A1.5's skill.toml shape
  (persona + model + `[security] mcp_command_allowlist` if using
  native MCP binary).

### 8.4 Output capture

- **Save real outputs as fixtures** under
  `examples/applications/<app>/sample-*/` — they're the most
  honest dogfooding artifact. A2's `sample-reviews/` is the first
  instance; A1's `/tmp/episode-test.wav` should have been
  captured too.
- **Don't rely on `🤖 Agent:` line until F-A2-1 lands** — use
  `--trace` + JSON extraction. README of any L3 skill should
  include the extraction snippet.

### 8.5 Findings discipline

- Number findings F-<app>-N for easy cross-reference.
- Categorise: agentflow code / phonon code / docs / convention /
  positive validation / no-action / perf data.
- One reflection update per 2-3 new applications (instead of after
  every application — too noisy).

## 9. Next dogfooding steps

Order matters because dependencies cascade:

1. **Land F-A2-1** (high pri agentflow bug). One PR, ~1-2 hours.
   After this, every L3 skill becomes usable from `skill run` output
   directly without trace extraction. Top of queue.
2. **Land F-A7-8 + F-A7-2 + F-A7-3** (medium pri, all small).
   `max_tokens` bump in template, shell node fix-or-drop, dead
   config cleanup. ~2 hours bundle.
3. **A3 research-assistant** (next application). Validates
   `arxiv` node + RAG + memory layers + scheduled run. Touches
   areas A1/A1.5/A7/A2 didn't.
4. **A2 follow-up: Harness Mode approval gate**. Add write-side
   `add_review_comment` tool; route through `agentflow harness run`
   instead of `skill run`. Validates the third pillar of A2's
   original spec (F-A2-9).
5. **Then** another reflection round (R3) after A3 + A2-follow-up
   produce more data. Probably +20 findings to consolidate.

Parallel work (no dependency):

- Phonon-side action items (F-PH-1, F-PH-2, F-PH-3) batched as a
  phonon `v0.7.x` patch. Doesn't block agentflow work.
- F-EX-1 (A1.5 persona) — 5 minute fix, can land anytime.

Not on the queue (recap):

- L2 — closed, see §4.
- Phonon scope refactor — no urgency, see §7.
- v1.0.0-rc.1 tag — separate decision, gated on operator choice.

---

## Cross-references

- [R1 reflection](L1_L3_REFLECTION_2026-05-18.md) — first pass, A1 + A1.5
- [`EXAMPLES_TODOs.md`](../EXAMPLES_TODOs.md) — per-application Findings detail
- [`TODOs.md`](../TODOs.md) — P9 segment with the 8 reflection-derived tasks
- [`examples/applications/`](../examples/applications/) — the 4 validated apps

## Status of this doc

R2 is the **current authoritative reflection** for the action queue and
the L1↔L3 selection rule. Future reflections (R3, R4) supersede this
one with similar dated `_R3_2026-MM-DD.md` etc. naming. Older R-docs
stay as historical records of what evidence was available when each
decision was made.
