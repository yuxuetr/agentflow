# L1 + L3 Integration Validation — Reflection

**Date**: 2026-05-18
**Trigger**: A1 (blog-to-podcast, L1 Rust dep) + A1.5 (podcast-mastering,
L3 phonon-mcp) both ran end-to-end with live Moonshot + MiniMax APIs.
**Goal**: consolidate findings from both validation runs, decide what
gets fixed where, settle the L2 question, and emit a prioritized
action queue.

---

## 1. What we set out to validate

The 3-tier integration architecture proposed when we asked "how should
agentflow consume phonon?" (see commit history for the original
discussion):

| Tier | What | Validation vehicle |
| --- | --- | --- |
| **L1** | agentflow consumes phonon as a Rust library (path dep), uses it inside custom `AsyncNode` impls | A1 blog-to-podcast |
| **L2** | agentflow wraps phonon functions as `Tool` trait impls so an in-process agent can drive them via native tool calling | (intentionally deferred — see § 4) |
| **L3** | agentflow drives phonon-mcp as a separate subprocess via stdio JSON-RPC, agent uses the auto-exposed `mcp_phonon_*` tools | A1.5 podcast-mastering |

L1 and L3 both reached "agent successfully produces correct audio
output end-to-end" — see EXAMPLES_TODOs.md A1 / A1.5 entries for
the per-run details.

## 2. Empirical data points worth keeping

| Metric | A1 (L1) | A1.5 (L3) | Note |
| --- | --- | --- | --- |
| Wall clock | 19s | 37s | A1 = 2-node DAG with 12 sequential TTS calls; A1.5 = 7-step ReAct with 6 MCP calls |
| Source path | blog text → final podcast | finished podcast → mastered podcast | Different scope, not apples-to-apples for raw "phonon speed" comparison |
| Per-tool/operation cost | TTS calls 150-300ms each | MCP calls all sub-second (audio_save = 50ms) | IPC overhead is not the bottleneck in either tier |
| Dominant cost | Sequential TTS (12 × HTTP roundtrip) | LLM thinking time (per-step 2-15s decision latency) | L3's "slow" is LLM-in-loop, not subprocess IPC |
| LLM cost (estimate) | ~¥0.01 Moonshot + ~¥1 MiniMax (high-def voice) | ~¥0.02 Moonshot (7 chat turns with tool defs) | L3's LLM cost is non-trivial relative to L1's pipeline cost |
| Lines of project-specific Rust | ~450 (PodcastNode + main.rs + smoke) | 0 (skill.toml + persona only) | L3 trades Rust code for prompt engineering |
| User-facing surface | binary CLI flags | natural language skill prompt | L3 lets non-Rust operators rebuild the workflow |

**Single most important takeaway**: L1 vs L3 is not "library vs
subprocess overhead" — both are fast on the IPC axis. The real trade
is **"is the workflow fixed or does an agent need to decide step by
step"**.

- **Fixed pipeline** → L1 (no LLM-in-loop tax)
- **Agent picks tools / branches** → L3 (LLM-in-loop tax is unavoidable, IPC overhead negligible)

## 3. Findings — categorized

20 findings total (10 from A1, 10 from A1.5). Categorized by who fixes:

### 3.1 agentflow code changes (4 items)

| ID | Finding | Action | Priority |
| --- | --- | --- | --- |
| F-AF-1 | `agentflow skill validate` swallows `SkillError::ValidationError.message`, user sees only "Validation failed" | Surface the underlying message in `agentflow-cli/src/commands/skill/validate.rs`. Check the `with_context` chain didn't override an `anyhow::Error`'s root cause display. | **High** — blocks new skill authors |
| F-AF-2 | SKILL.md frontmatter `model:` field is silently ignored (`SkillMd::into_manifest` always sets `model: Default::default()`) | Either (a) parse `model.name` from frontmatter, or (b) warn at parse time when a `model` field is present in SKILL.md frontmatter | Medium — surprising, but skill.toml is the workaround |
| F-AF-3 | `.env` auto-load from `~/.agentflow/.env` should be a built-in convention for `agentflow skill run` / `workflow run` / etc., not just per-example boilerplate | Add `dotenvy::from_path` call early in `agentflow-cli/src/main.rs` (silent no-op when missing). Respect existing env vars (dotenvy default). | Medium — reduces friction for every CLI invocation |
| F-AF-4 | `agentflow-llm::AgentFlow::init()` requires either workspace config or `~/.agentflow/models.yml` to discover Moonshot, and the error path is not always crisp | Already mostly fine; add an explicit error message hint pointing at the docs section when init fails | Low — only hit on fresh hosts |

### 3.2 phonon code changes (3 items)

| ID | Finding | Action | Priority |
| --- | --- | --- | --- |
| F-PH-1 | `phonon-podcast::OpenAiScriptGenerator::generate` and `phonon-ai::MiniMaxTts::synthesize` `#[instrument(fields(topic = %request.topic))]` / `fields(text)` dump the entire input string into a single trace line. For long blogs that's KB-per-line. | Truncate via `tracing::field::display(&truncate(s, 120))` in the `fields` macro, or use a `Display` helper that caps length | Medium — terminal output unreadable for long inputs |
| F-PH-2 | `phonon-podcast::PodcastPipeline::generate` returns `AudioBuffer` but loses per-segment timing. SRT consumers (like our `estimate_subtitle_timing` helper in A1) have to estimate. | Either return `(AudioBuffer, Vec<SegmentDuration>)` from `generate`, or add a `generate_with_segment_times` variant. | Medium — A1 wrote a workaround; pattern will recur |
| F-PH-3 | `phonon-mcp::audio_load` / `audio_info` don't surface `resampled_from` — we noticed source was 32kHz, audio_info reported 44.1kHz, no indication that internal resampling happened. | Add `original_sample_rate` to `audio_info` output and a `resampled` boolean if the loader changed the rate. | Low — doesn't break correctness, but breaks introspection |

### 3.3 Documentation changes (4 items)

| ID | Finding | Action | Priority |
| --- | --- | --- | --- |
| F-DOC-1 | Default `security.mcp_command_allowlist = [python, python3, node, npx, uvx]` is a good security posture, but it's not documented anywhere user-facing | Add a section to `docs/SKILL_FORMAT.md` (or wherever skill format is documented) titled "Spawning native binary MCP servers" with the explicit allowlist opt-in pattern | **High** — silently breaks every compiled binary MCP integration |
| F-DOC-2 | `FlowValue::File { mime_type, .. }` field is `mime_type` not `media_type`. Easy to guess wrong (I did). | Add a "FlowValue field reference" subsection to `docs/AGENT_SDK.md` enumerating exact field names | Low — discoverable via cargo errors |
| F-DOC-3 | `target_segments` (phonon `ScriptRequest`) is documented as "approximate number of segments to generate" but caller experience is "it's a hint, not a cap" — Moonshot returned 12 when asked for 4 | Tighten `ScriptRequest.target_segments` docstring + A1 README `--segments` description | Low — operator self-corrects in seconds |
| F-DOC-4 | A1 README documents how to add `phonon-mcp` to skill manifest, but the cross-link to "build phonon-mcp first" is buried at the top | Bump the build-phonon-mcp instruction to a numbered "Pre-flight" section in the application's README | Low — present, just not prominent |

### 3.4 Example / skill conventions (1 item)

| ID | Finding | Action | Priority |
| --- | --- | --- | --- |
| F-EX-1 | A1.5 podcast-mastering persona has steps 1-6 but the agent never re-measures LUFS after normalize. It trusts the `target_lufs` parameter and reports the target value as if measured. | Add Step 5.5 to the persona: "re-measure with `audio_loudness` before save; report actual achieved LUFS". This is a persona-only fix, no code change. | Medium — slight integrity issue in final answer |

### 3.5 Intentional non-fixes (3 items)

| ID | Finding | Why we're not fixing |
| --- | --- | --- |
| F-NA-1 | A1 default guest voice `Chinese (Mandarin)_HK_Flight_Attendant` produces HK-accented Mandarin | User reviewed result and accepted: "港式英语这个可以不用再做纠正". Acceptable variation. |
| F-NA-2 | A1 default LLM model name was `kimi-k2-0905-preview` (didn't exist) | Already fixed in-app (changed to `moonshot-v1-128k`); no further action |
| F-NA-3 | `ConsoleListener` is a unit struct (no `default()`) | Trivial; tripped me up once; correct API is `ConsoleListener` not `::default()`. Not worth changing. |

### 3.6 Positive validation (no action — record only)

| ID | Finding |
| --- | --- |
| F-OK-1 | agentflow `ConsoleListener` emits per-node trace events with timing visible immediately (`read_blog: 189µs`, `produce_podcast: 18.78s`) |
| F-OK-2 | `mcp_<server_name>_<tool_name>` auto-naming convention is clean and predictable |
| F-OK-3 | Moonshot native tool calling worked first-shot through `moonshot-v1-128k`; honoured persona's step-by-step instructions across 6 tools |
| F-OK-4 | phonon-mcp's `AssetRegistry` (UUID handle) pattern survived multi-step agent workflow without leaks or wrong-handle bugs |
| F-OK-5 | `agentflow skill run --trace` JSON output (plan / tool_call / tool_result / final_answer per index + timestamp) is genuinely useful for dogfooding/debug |
| F-OK-6 | `dotenvy::from_path` pattern for `~/.agentflow/.env` works cleanly in standalone application binaries (precedent for F-AF-3 promotion) |

## 4. L2 verdict

**L2 (agentflow `Tool` trait wrapping phonon functions for in-process,
LLM-driven tool calling) is deferred indefinitely.**

Evidence:

- L1 already handles the **fixed-workflow + same-process** case (A1).
  Adding LLM-in-loop tax here only makes sense if the workflow needs
  branching, and at that point IPC overhead is negligible compared to
  LLM latency.
- L3 already handles the **agent-driven + cross-process** case (A1.5).
  The dominant cost (LLM thinking) is identical whether the tool runs
  in-process or out-of-process. The IPC overhead saving from L2 over
  L3 is **milliseconds out of seconds**.
- The only meaningful L2 niche is "agent decides tools AND IPC
  overhead matters AND we want same-process". This combination
  doesn't appear in any of the seeded application examples (A1-A7),
  and we have no concrete use case asking for it.

**Decision**:
- L2 is marked DEFERRED in EXAMPLES_TODOs.md
- If a future application surfaces a scenario where L2 genuinely
  beats both L1 and L3, revisit. Otherwise L1+L3 is the supported
  surface.

This is **not** "we forgot to do L2". This is "L2 was a hypothetical
that didn't survive contact with real apps."

## 5. Phonon scope reflection (revisited)

Re-reading the earlier scope-creep conversation, the dogfooding
gave new data:

- **Core 6 crates (core / io / wav / ai / podcast / mcp) all
  actively used by A1 + A1.5**. Either L1 or L3 touches every one.
  That validates "these are the agent-facing essential set."
- **engine / cli / server / studio / video / wasm / plugin —
  used by neither A1 nor A1.5**. Confirms the earlier hypothesis
  that these 7 crates are out of the agent-facing thesis.
- **MiniMax provider was added during this validation cycle**.
  Adding a new provider to phonon-ai took ~150 lines, hex decode
  + business-error mapping + emotion whitelist; the trait surface
  held up.

**No urgent need to refactor phonon's core crates.** The 4
maybe-archive crates (studio / video / wasm / plugin) decision
stays where it was: optional cleanup, no urgency, can decide
later. If any of them sees a new release in the next 3 months
they're not actually neglected; if none of them see updates
they can be archived without disruption.

## 6. Action queue (prioritized)

| Pri | ID | Action | Owner | Tracking |
| --- | --- | --- | --- | --- |
| 1 | F-AF-1 | Surface `SkillError::ValidationError.message` in `skill validate` | agentflow | Add as `P9.1` in TODOs.md (or pick another segment) |
| 2 | F-DOC-1 | Document `security.mcp_command_allowlist` default + how to opt-in compiled binaries | agentflow docs | Add as `P9.2` |
| 3 | F-AF-3 | `dotenvy::from_path("~/.agentflow/.env")` in agentflow CLI entrypoint | agentflow | `P9.3` |
| 4 | F-PH-1 | Truncate long values in phonon `#[instrument(fields(...))]` | phonon | phonon Todos.md |
| 5 | F-PH-2 | `PodcastPipeline::generate` returns per-segment durations | phonon | phonon Todos.md |
| 6 | F-AF-2 | SKILL.md `model:` either honour or warn | agentflow | `P9.4` |
| 7 | F-EX-1 | A1.5 persona: add "re-measure LUFS before save" step | agentflow examples | EXAMPLES_TODOs.md A1.5 |
| 8 | F-PH-3 | phonon-mcp `audio_info` surfaces `resampled_from` | phonon | phonon Todos.md |
| 9 | F-DOC-2 | `FlowValue` field reference in `docs/AGENT_SDK.md` | agentflow docs | `P9.5` |
| 10 | F-DOC-3 | Tighten `target_segments` docstring + A1 README | phonon + agentflow examples | small |
| 11 | F-DOC-4 | Bump phonon-mcp build instruction prominence in A1.5 README | agentflow examples | small |
| - | F-AF-4 | Crisper Moonshot/Anthropic init error path on fresh hosts | agentflow | low pri |

**No high-priority refactor of either core (phonon-core / phonon-podcast / agentflow-core / agentflow-agents).** Every action is documentation, error-message, or peripheral-fix scope.

## 7. Next dogfooding steps

In rough order of value:

1. **Land the top-3 priority actions** (F-AF-1, F-DOC-1, F-AF-3) —
   these directly affect every future skill author. ~2-4 hours total.
2. **A7 changelog-writer** — zero-external-dep app exercising shell
   node + OS sandbox + LLM (agentflow eats its own dogfood). Should
   surface another batch of findings about shell admission + sandbox
   actually working as documented.
3. **A2 code-reviewer** — real ReAct agent + MCP integration (GitHub
   MCP server). Closest analogue to A1.5 but with write-side tools
   (`add_review_comment`) which exercises Harness Mode approval gate
   for the first time outside synthetic tests.
4. **Phonon-side action items** (F-PH-1, F-PH-2, F-PH-3) — batch
   into a single phonon `v0.7.x` patch release. None block agentflow
   work.
5. **Continue dogfooding without a fixed schedule.** Add findings to
   EXAMPLES_TODOs.md as they accumulate. Open another reflection
   doc when enough new evidence builds up (probably after A2 + A7 +
   one more app, ~2 months from now).

---

## Cross-references

- `EXAMPLES_TODOs.md` — application tracking; A1 + A1.5 entries hold
  the per-finding detail
- `examples/applications/blog-to-podcast/README.md` — A1 (L1)
- `examples/applications/podcast-mastering/README.md` — A1.5 (L3)
- `phonon/RoadMap.md` — confirms phonon's "audio engine + MCP" thesis
- `phonon/Todos.md` — for the F-PH-* phonon-side actions

## Status

This reflection itself does not change any code or schedule. The
action items above become tasks in `TODOs.md` (agentflow side) or
`/Users/hal/rustspace/phonon/Todos.md` (phonon side) in the next
commit cycle.
