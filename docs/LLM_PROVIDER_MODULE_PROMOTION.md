# LLM Provider Module Promotion Criteria

Status: **Decision document for P10.3.2**
Owner: AgentFlow core / `agentflow-llm`
Last updated: 2026-05-20
Closes: P10.3.2 (Medium — v1.x)

`agentflow-llm` ships dedicated provider modules for 5 vendors
(`OpenAI`, `Anthropic`, `Google`, `Moonshot`, `StepFun`) plus a
`Mock`. Four more OpenAI-compat vendors — **GLM (Zhipu)**,
**DashScope (Alibaba)**, **DeepSeek**, and **MiniMax** — share the
`OpenAIProvider` implementation via `create_provider` in
`agentflow-llm/src/providers/mod.rs:242-256`. P10.3.2 is the
placeholder for "would we ever peel one of those off into its own
module?"

The recommendation up front: **don't peel any of them off until
divergence is empirically observed.** The shared adapter passes
the cross-provider consistency suite for all four vendors today,
and a peel-off carries ~300-500 LoC of duplication per vendor.
This document pins what concrete divergence signal would tip the
scale, so a future contributor can answer "should I extract X?"
in minutes instead of re-deriving the analysis.

Same posture as P10.19.1 (WASM plugin runtime) and P10.10.1 (H6
items): decide-when-to-revisit, persist the analysis, let the
trigger drive the next P11.x.

---

## What's shared today

Wire-shape match against `OpenAIProvider`'s request / response /
streaming / tool-calling code:

| Feature | OpenAI | GLM | DashScope | DeepSeek | MiniMax |
| --- | :-: | :-: | :-: | :-: | :-: |
| `POST /v1/chat/completions` shape | ✅ | ✅ | ✅ (`/compatible-mode/v1`) | ✅ | ✅ |
| `tools` / `tool_choice` request fields | ✅ | ✅ | ✅ | ✅ | ✅ |
| `tool_calls[]` response field | ✅ | ✅ | ✅ | ✅ | ✅ |
| SSE streaming `delta.content` | ✅ | ✅ | ✅ | ✅ | ✅ |
| Multimodal `content: [{type, …}]` | ✅ | ✅ | ✅ | ⚠️ (text-only family today) | ✅ |
| Bearer-token auth header | ✅ | ✅ | ✅ | ✅ | ✅ |

Verification: `agentflow-llm/tests/provider_consistency_live.rs`
runs the four `cross_provider_*_paths_*_uniform_*` invariants
against all four shared-adapter vendors in the nightly
`llm-live.yml` GitHub Action. Tonight's run is the running proof
that the shared adapter still works.

The wire-shape match is the bargain: as long as it holds,
peeling a vendor off costs LoC and reduces test surface (every
vendor-specific module needs its own consistency coverage).

---

## When to peel a vendor off

A peel-off is justified when **any** of the following is true
for that specific vendor:

1. **Tool-call shape divergence.** The vendor changes the
   `tool_calls[]` response field to a non-OpenAI shape (e.g.
   nested `function.parameters` instead of `function.arguments`,
   or a typed `tool_use` block like Anthropic). The
   `cross_provider_tool_call_paths_produce_uniform_canonical_shape`
   invariant in `provider_consistency_live.rs` will fail —
   that's the empirical signal.

2. **Multimodal shape divergence.** The vendor introduces a
   modality input the `OpenAIProvider` content-block decoder
   can't represent (e.g. inline video, audio with non-OpenAI
   chunking, vendor-specific image preprocessing parameters).
   The `cross_provider_multimodal_paths_produce_uniform_response_shape`
   invariant will fail.

3. **Streaming protocol divergence.** The vendor moves from
   OpenAI's `data: <json>\n\n` SSE framing to a different chunk
   shape (e.g. NDJSON, gRPC streaming, or a wrapper envelope).
   The `cross_provider_streaming_paths_yield_uniform_hello_world_concatenation`
   invariant will fail.

4. **Auth / endpoint topology divergence.** The vendor moves
   off `Authorization: Bearer` toward HMAC-SHA1 signed requests
   (like AWS Bedrock) or per-request OAuth, OR splits
   `chat/completions` across resource-specific endpoints. The
   shared adapter's request builder can't absorb this without a
   per-vendor branch — extracting becomes cleaner than gating.

5. **Vendor-specific feature with no OpenAI equivalent.** The
   vendor ships a feature with no upstream OpenAI mapping
   (e.g. DeepSeek's reasoning-mode `reasoning_content` field
   that requires opt-in `enable_reasoning: true`, MiniMax's
   character role-play API, DashScope's `enable_thinking`
   flag). Patching the shared adapter to thread per-vendor
   knobs creates the kind of `if vendor == "x"` rot we extract
   to escape.

6. **Operator-side request.** A real downstream consumer of
   `agentflow-llm` files an issue saying "I need feature X from
   vendor Y and the shared adapter eats it." Empirical demand
   beats hypothetical purity.

**None of these has fired** as of 2026-05-20. The nightly live
suite passes for all four shared-adapter vendors.

---

## What a peel-off looks like

When the trigger fires for vendor `V`, the change is mechanical:

1. **New file** `agentflow-llm/src/providers/<v>.rs` (~300-500
   LoC per the TODO estimate). Implements `LLMProvider`. Starts
   as a copy-of-`OpenAIProvider` minus the bits the divergence
   moves to. *Do not* generalise prematurely — copy first, then
   refactor common helpers up if a second vendor extracts later.

2. **Dispatch update** in
   `agentflow-llm/src/providers/mod.rs::create_provider`: change
   `"v" => Ok(Box::new(OpenAIProvider::new(...)))` to
   `"v" => Ok(Box::new(VProvider::new(...)))`. Drop the
   `"<vendor> is OpenAI-compatible"` comment.

3. **Consistency tests** in
   `agentflow-llm/tests/provider_consistency_live.rs`: the
   vendor was already covered by the cross-provider suite. The
   peel-off shouldn't break that coverage; if it does, the
   peel-off was premature.

4. **Vendor-specific tests** for the divergence: at minimum a
   unit test of the new wire shape, a live integration test
   behind a feature flag.

5. **Docs**: update the wire-shape table at the top of this
   file (mark the divergence row with ⚠️ or ❌ for the vendor),
   and add a one-line entry to the vendor's row in
   `agentflow-llm/templates/default_models.yml` describing
   what changed.

The total per-vendor cost is roughly:
- 300-500 LoC of provider code,
- ~50 LoC of vendor-specific tests,
- ~20 minutes of doc updates.

That's not a huge investment, but the test surface (one more
combination in the consistency matrix) is permanent.

---

## What this document does **not** do

- It does not peel any vendor off.
- It does not commit to peeling any vendor off on any timeline.
- It does not write the per-vendor RFC in advance.

When a trigger fires for a specific vendor, open a fresh
`P11.x` TODO entry, link this document, and write a brief
per-vendor migration note in the commit message — no formal
RFC is needed for the peel-off itself; the criteria here are
the gate.

---

## Why a 1-pager instead of code

The TODO note for P10.3.2 is explicit:

> Current: 4 OpenAI-compat vendors (GLM + these 3) share
> `OpenAIProvider` via `create_provider`. Works for the wire
> shape match.
>
> Trigger to do this: vendor introduces wire-format divergence
> that `OpenAIProvider` can't cleanly handle.
>
> Until then: keep the shared-adapter approach.

The TODO is gated on a divergence signal that hasn't fired and
explicitly says "until then, don't." The work P10.3.2 actually
represents is *maintaining the gate* — the same discipline
P10.10.1 captures for H6. This document is the gate, made
empirically verifiable. The TODO can close because the gate is
documented; closing it didn't require peeling any vendor off,
only ensuring the next contributor has clear criteria.

---

## References

- `agentflow-llm/src/providers/mod.rs::create_provider` — the
  current dispatch table.
- `agentflow-llm/src/providers/openai.rs` — the shared adapter.
- `agentflow-llm/tests/provider_consistency_live.rs` — the
  cross-provider invariants that fire when divergence happens.
- `.github/workflows/llm-live.yml` — nightly cross-provider
  live suite. Per-provider tests self-skip when the
  corresponding `*_API_KEY` secret is absent, so flipping a
  single provider is safe.
- `agentflow-llm/templates/default_models.yml` — the
  authoritative model registry; vendor-specific divergence
  shows up here first.
- `docs/WASM_PLUGIN_EVALUATION.md` (P10.19.1) — the template
  this document follows.
- `docs/H6_PROMOTION_CRITERIA.md` (P10.10.1) — the other
  trigger-gated 1-pager in the v1.x backlog.
- `RoadMap.md::Later Tracks` — the "ecosystem expansion"
  context.
