# LLM Providers Support Matrix

> Status: foundation shipped in v0.4.0 (P1 #11). Streaming, multimodal, and
> live-LLM nightly CI all closed (see Closed follow-ups).
> Crates: `agentflow-llm`. Test entries:
> `agentflow-llm/tests/provider_consistency.rs` (offline, mocked) and
> `agentflow-llm/tests/provider_consistency_live.rs` (opt-in, real APIs).

AgentFlow's LLM abstraction targets seven providers/profiles. This document is the
authoritative reference for what works on each, what doesn't, and how the
behavior is verified.

## ProviderRequest contract

Every `LLMProvider` impl receives a `ProviderRequest` and returns a
`ProviderResponse`. The wire fields are intentionally narrow so adapter
behavior can stay consistent. **The field names below are exercised by
`agentflow-llm/tests/provider_matrix_doc.rs` — adding or renaming a
field on `ProviderRequest` fails CI until this section is updated.**

| Field | Type | Required | Description |
| --- | --- | :-: | --- |
| `model` | `String` | ✅ | Provider-resolved model identifier (e.g. `gpt-4o-mini`, `claude-3-5-sonnet-20241022`). Adapters translate to the wire shape each provider expects. |
| `messages` | `Vec<Value>` | ✅ | OpenAI-style message array. Multimodal content is encoded as `image_url` blocks; adapters translate to provider-native shapes (Anthropic `image`, Google `inline_data`). |
| `stream` | `bool` | ✅ | When `true`, the provider returns a chunked / SSE response. `ModelCapabilities::requires_streaming` rejects `stream = false` for streaming-only models. |
| `parameters` | `HashMap<String, Value>` | ✅ | Free-form provider passthrough (temperature, top_p, max_tokens, custom flags). Adapters whitelist and rename as needed; unknown keys are ignored. |
| `tools` | `Option<Vec<ToolSpec>>` | – | Native tool / function-calling specification. `None` skips tool wiring entirely. |
| `tool_choice` | `Option<ToolChoice>` | – | Selection strategy used together with `tools`. See the [`ToolChoice` table](#toolchoice-modes). |
| `thinking` | `Option<ThinkingConfig>` | – | Extended-reasoning ("thinking") configuration. Travels as a typed field so Anthropic/Google whitelists don't drop it. Adapters map to native shapes: Anthropic `thinking: { budget_tokens }`, OpenAI `reasoning_effort`, Google `generationConfig.thinkingConfig.thinkingBudget`. `None` disables. |

## ToolChoice modes

The `ToolChoice` enum is serialised in snake_case so the wire shape
matches OpenAI's `tool_choice` field. Anthropic / Google adapters
translate to their respective vocabularies.

| Mode | Provider behavior |
| --- | --- |
| `auto` (default) | Model decides whether to call a tool. Adapters omit `tool_choice` from the outbound request when this is the default. |
| `none` | Model MUST NOT call a tool; reply text-only. |
| `required` | Model MUST call at least one tool. Providers that lack a literal "required" mode (some Google revisions) raise `LLMError::ProviderUnsupportedMode` at request time. |
| `tool` (with `{ name }`) | Model MUST call exactly the named tool. Useful for deterministic agent steps and benchmarks. |

## ModelCapabilities flags

`ModelCapabilities` (in `agentflow-llm::model_types`) is the per-model
description loaded from the YAML registry. The flags below drive
provider-side validation and ReAct fallback behavior:

| Flag | Type | Purpose |
| --- | --- | --- |
| `model_type` | `ModelType` | Stable model-type classification (`text`, `image_generate`, `image_understand`, `audio_tts`, `audio_asr`, `video_generate`, `video_understand`, `doc_understand`, `code_gen`, `function_calling`, `embedding`). Determines admissible inputs / outputs. |
| `supports_streaming` | `bool` | Model exposes a streaming variant. |
| `requires_streaming` | `bool` | Model has no non-streaming mode; callers MUST set `ProviderRequest::stream = true`. |
| `supports_tools` | `bool` | Tool calling is supported on any path (native or prompt-based). |
| `native_tool_calling` | `bool` | Provider-native tool calling (OpenAI `tool_calls`, Anthropic `tool_use`, Google `functionCall`). When `false`, ReAct falls back to prompt-based protocols. |
| `max_context_tokens` | `Option<u32>` | Hard ceiling on the model's input context window. Used by prompt assembly. |
| `max_output_tokens` | `Option<u32>` | Hard ceiling on a single response. |
| `supports_system_messages` | `bool` | When `false`, the adapter folds system content into the first user message. |
| `custom_capabilities` | `HashMap<String, Value>` | Provider-specific opt-ins (vision detail mode, JSON-mode strictness, etc). Stable surface but opaque to the registry. |

## Configuration source

Provider definitions and model aliases are loaded with the shared AgentFlow
configuration resolver:

1. `AGENTFLOW_MODELS_CONFIG`
2. `~/.agentflow/models.yml`
3. `~/.agentflow/models.yaml`
4. bundled `default_models.yml`

`~/.agentflow/.env` remains the default local API-key file. CLI diagnostics
show the selected config path/source and redact credential values.

## Capability matrix

| Capability | OpenAI | Anthropic | Google | Moonshot | StepFun | GLM | Mock |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| Text completion | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Token usage in response | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |
| Streaming | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Native tool calling (`tool_calls` / `tool_use` / `functionCall`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ (injection) |
| Multimodal text + image (URL or base64) | ✅ | ✅ | ✅ | partial | ✅ | ✅ | n/a |
| Audio TTS / ASR | – | – | – | – | ✅ | – | n/a |
| Image generation | – | – | – | – | ✅ | – | n/a |
| W3C `traceparent` injection | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |
| `with_client(...)` for custom `reqwest::Client` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |

Key:

- ✅ — supported, verified in unit + integration tests.
- partial — works for text but not all multimodal corner cases.
- – — not implemented. Most LLM providers don't ship the modality.
- n/a — Mock provider doesn't make HTTP calls; capability is irrelevant.

### StepFun live-test status

Status vocabulary: `supported`, `live_tested`, `mock_only`, `unsupported`,
`flaky`.

| StepFun capability | Status | Verification |
| --- | --- | --- |
| Text generation | `live_tested` | `stepfun_live_text_path` |
| Streaming | `live_tested` | `stepfun_live_streaming_path` |
| Native tool calling / compatible fallback | `live_tested` | `stepfun_live_tool_calling_or_fallback_path`; adapter normalizes non-empty `tool_calls` to `StopReason::ToolCalls` even when StepFun returns `finish_reason: "stop"` |
| Vision understanding | `live_tested` | `stepfun_live_vision_path` with a tiny base64 PNG |
| Image generation | `live_tested` | `stepfun_live_image_generation_path` via `/images/generations` |
| TTS | `live_tested` | `stepfun_live_tts_path` via `/audio/speech` |
| ASR | `live_tested` | `stepfun_live_asr_path`; generates a tiny TTS fixture, then transcribes it |
| Video generation | `unsupported` | No StepFun video API is implemented in `agentflow-llm` |

StepFun live tests use deterministic model selection. Environment overrides
still win (`AGENTFLOW_LIVE_STEPFUN_TEXT_MODEL`,
`AGENTFLOW_LIVE_STEPFUN_TOOLS_MODEL`, `AGENTFLOW_LIVE_STEPFUN_VISION_MODEL`,
`AGENTFLOW_LIVE_STEPFUN_IMAGE_MODEL`, `AGENTFLOW_LIVE_STEPFUN_TTS_MODEL`,
`AGENTFLOW_LIVE_STEPFUN_ASR_MODEL`); otherwise the harness picks from the
loaded AgentFlow model config using a low-cost preference order before falling
back to built-in defaults.

### GLM live-test status

BigModel GLM is wired as an OpenAI-compatible profile over the official
`https://open.bigmodel.cn/api/paas/v4` endpoint documented in
[BigModel 使用概述](https://docs.bigmodel.cn/cn/api/introduction). Chat
completion uses `/chat/completions`, which BigModel documents with Bearer auth,
SSE streaming, function tools, and multimodal input in
[对话补全](https://docs.bigmodel.cn/api-reference/%E6%A8%A1%E5%9E%8B-api/%E5%AF%B9%E8%AF%9D%E8%A1%A5%E5%85%A8).

Status vocabulary: `supported`, `live_tested`, `mock_only`, `unsupported`,
`flaky`.

| GLM capability | Status | Verification |
| --- | --- | --- |
| Text generation | `live_tested` | `glm_live_text_path` via `OpenAIProvider` |
| Streaming | `live_tested` | `glm_live_streaming_path`; GLM live tests are serialized to avoid account-level 429s |
| OpenAI-compatible chat path | `live_tested` | `glm_live_openai_compatible_chat_path` |
| Native tool calling / compatible fallback | `live_tested` | `glm_live_tool_calling_or_fallback_path`; OpenAI-compatible adapter normalizes non-empty `tool_calls` to `StopReason::ToolCalls` |
| Vision understanding | `live_tested` | `glm_live_vision_path` using `glm-4.5v` and an HTTPS JPEG image URL |
| Image generation | `unsupported` | BigModel exposes `/images/generations`, but AgentFlow has no GLM image-generation client/profile yet |
| ASR | `unsupported` | BigModel exposes `/audio/transcriptions`, but AgentFlow has no GLM ASR client/profile yet |
| TTS | `unsupported` | BigModel exposes `/audio/speech`, but AgentFlow has no GLM TTS client/profile yet |
| Video generation | `unsupported` | BigModel exposes async `/videos/generations`, but AgentFlow has no GLM video client/profile yet |

Environment overrides win for model selection:
`AGENTFLOW_LIVE_GLM_TEXT_MODEL`, `AGENTFLOW_LIVE_GLM_TOOLS_MODEL`, and
`AGENTFLOW_LIVE_GLM_VISION_MODEL`. Without overrides, the harness first checks
the loaded AgentFlow model config and then falls back to low-cost defaults:
`glm-4.5-flash` for text/tools and `glm-4.5v` for vision.

## Model families & context windows

The numbers below are the **public** context windows documented by
each vendor; the runtime ceiling is whatever the model's YAML registry
entry sets in `max_context_tokens`. When the registry value is `None`,
prompt assembly assumes a conservative 4 K token cap so adapters
never silently truncate.

| Provider | Model family | Public context | Verification |
| --- | --- | --- | --- |
| OpenAI | `gpt-4o`, `gpt-4o-mini`, `gpt-4-turbo`, `o1` | 128K–200K | `tested` (live + offline) |
| OpenAI | `gpt-3.5-turbo` | 16K | `tested` (offline only) |
| Anthropic | `claude-3-5-sonnet-20241022`, `claude-3-5-haiku-20241022` | 200K | `tested` (live + offline) |
| Anthropic | `claude-3-opus-20240229` | 200K | `best_effort` (offline only) |
| Google | `gemini-1.5-flash`, `gemini-1.5-pro` | 1M | `tested` (offline; live nightly) |
| Google | `gemini-2.0-flash`, `gemini-2.0-flash-exp` | 1M | `best_effort` (offline only) |
| Moonshot | `moonshot-v1-8k`, `moonshot-v1-32k`, `moonshot-v1-128k` | 8K / 32K / 128K | `tested` (live + offline) |
| StepFun | `step-1-8k`, `step-1-128k`, `step-1v-8k`, `step-2-16k` | 8K – 128K | `tested` (live + offline) |
| GLM | `glm-4.5`, `glm-4.5-flash`, `glm-4.5v` | 128K | `tested` (live + offline, vision opt-in) |
| Mock | `mock-runtime-*`, `mock-*` | configurable per test | n/a |

Status vocabulary:

- `tested` — verified in unit + integration tests (and / or nightly live CI).
- `best_effort` — provider supports the model and adapters wire it,
  but no AgentFlow test asserts the exact wire shape.
- `n/a` — Mock provider; not a real backend.

## Rate-limit handling

AgentFlow does **not** auto-retry rate-limited requests by default.
Provider adapters preserve the upstream `Retry-After` header (when
present) in `LLMError::HttpError::message` so downstream consumers
(workflow retry middleware, the `agentflow-core` `retry_executor`)
can react with exponential backoff or queue back-pressure.

| Layer | Behavior |
| --- | --- |
| Provider adapter (per `LLMProvider`) | Maps `HTTP 429` to `LLMError::HttpError { status_code: 429, message }`. Adapter never sleeps. |
| `LLMClient` (high-level helper) | Reads `RetryPolicy` from the registry's model config. When set, drives backoff via `agentflow-core::retry_executor`. Default is no retry. |
| Workflow / agent retry executor | Applies `retry::ErrorPattern::Status(429)` if the workflow config declares retries. ReActAgent has its own `max_iterations` ceiling unrelated to rate-limit retry. |
| Operator-visible behavior | A 429 response surfaces as a `ToolError::PolicyDenied`-style `LLMError` in the agent step trace; the operator sees the upstream message verbatim. |

If a deployment needs aggressive 429 handling, set
`max_retries` + an exponential `RetryPolicy` in the model YAML and the
high-level helper will respect it.

## Error mapping (verified contract)

All concrete HTTP-based providers and OpenAI-compatible profiles map non-2xx responses to a single
`LLMError` variant — this is the consistency contract that downstream code
(retry middleware, error reporting) depends on:

```rust
match err {
  LLMError::HttpError { status_code, message } => {
    // status_code is the actual HTTP status from the provider
    // (401, 429, 500, 503, ...) — NOT remapped to a higher-level variant.
  }
  _ => unreachable!(),
}
```

The `From<reqwest::Error>` impl in `agentflow-llm::error` does map 401 / 429 /
503 to `AuthenticationError` / `RateLimitExceeded` / `ServiceUnavailable`, but
this only fires when the **transport itself** signals a status (e.g.
`response.error_for_status()?`). All providers inspect `response.status()`
manually and emit `HttpError`, which is what the consistency test asserts.

If a future provider needs richer error classification, the recommended path
is a new `LLMError::ProviderError { provider, kind: ProviderErrorKind, … }`
variant rather than reinterpreting status codes inconsistently across
providers.

## Verification

### Unit tests (per-provider, JSON fixtures)

Each provider has its own unit test suite that pins down request body
construction and response parsing against representative JSON fixtures:

```bash
cargo test -p agentflow-llm --lib providers::openai     # 10 tests
cargo test -p agentflow-llm --lib providers::anthropic  # 7 tests
cargo test -p agentflow-llm --lib providers::google     # 8 tests
cargo test -p agentflow-llm --lib providers::moonshot   # 5 tests
cargo test -p agentflow-llm --lib providers::stepfun    # 8 tests
```

These tests don't make network calls — they construct `ProviderRequest` /
`ProviderResponse` instances directly and assert wire-format invariants.

### Cross-provider integration (`provider_consistency.rs`)

The integration suite drives the concrete HTTP providers through a hand-rolled
tokio TCP listener and asserts a uniform contract:

```bash
cargo test -p agentflow-llm --test provider_consistency  # 15 tests
```

Coverage:

- **Success path** (5 tests): each provider, given a well-formed response
  in its native shape, returns `ContentType::Text`, `StopReason::Stop`,
  and populated `TokenUsage` (prompt / completion / total).
- **Tool-calling success path** (5 tests): each provider, given a response
  carrying a model-issued `get_weather(city="Tokyo")` tool call in its
  native shape (OpenAI `tool_calls`, Anthropic `tool_use` content blocks,
  Google `functionCall` parts, Moonshot/StepFun OpenAI-compatible
  passthrough), parses to a single `ToolCallRequest { name, arguments,
  id }` with non-empty id (synthesised when the provider doesn't supply
  one) and reports `StopReason::ToolCalls`. Google's adapter is
  responsible for normalising `finishReason: STOP` to `ToolCalls` when
  functionCall parts are present.
- **Error mapping** (5 tests): each provider, given a 4xx / 5xx response,
  returns `LLMError::HttpError` with the exact status code preserved.

The mock server is hand-rolled (not `mockito` / `wiremock`) for the same
reason as `trace_context_propagation.rs`: it lets us inspect raw request
bytes and stays independent of HTTP-mock crate version churn. The reqwest
client is built with `.no_proxy()` to avoid being routed through a system
HTTP proxy on dev machines (a common source of `IncompleteMessage` / hang
in localhost tests).

### Trace context propagation (`trace_context_propagation.rs`)

Verifies that an active `LlmTraceContext` injects a W3C `traceparent` header
into outbound HTTP calls, and that no header is injected when no scope is
active. See [`docs/TRACING_USAGE.md`](TRACING_USAGE.md) for usage.

## Live-API tests (opt-in)

By default, every provider test in this crate is offline / mocked. Live API
calls are deliberately not part of the default `cargo test` run — they require
real keys, cost money, and are non-deterministic (provider behavior shifts
between model versions).

Live tests are gated by:

```bash
AGENTFLOW_LIVE_LLM_TESTS=1 \
OPENAI_API_KEY=sk-… \
ANTHROPIC_API_KEY=sk-ant-… \
GEMINI_API_KEY=… \
MOONSHOT_API_KEY=… \
STEPFUN_API_KEY=… \
GLM_API_KEY=… \
cargo test -p agentflow-llm --test provider_consistency_live
```

Additional live-test gates are reserved for modality-specific suites:

```bash
AGENTFLOW_LIVE_MULTIMODAL_TESTS=1
AGENTFLOW_LIVE_IMAGE_TESTS=1
AGENTFLOW_LIVE_AUDIO_TESTS=1
AGENTFLOW_LIVE_VIDEO_TESTS=1
```

StepFun modality smoke tests are enabled by those gates:

```bash
AGENTFLOW_LIVE_LLM_TESTS=1 \
AGENTFLOW_LIVE_MULTIMODAL_TESTS=1 \
AGENTFLOW_LIVE_IMAGE_TESTS=1 \
AGENTFLOW_LIVE_AUDIO_TESTS=1 \
cargo test -p agentflow-llm --test provider_consistency_live stepfun_live
```

GLM OpenAI-compatible smoke tests are enabled with:

```bash
AGENTFLOW_LIVE_LLM_TESTS=1 \
cargo test -p agentflow-llm --test provider_consistency_live glm_live

AGENTFLOW_LIVE_MULTIMODAL_TESTS=1 \
cargo test -p agentflow-llm --test provider_consistency_live glm_live_vision_path
```

**Status**: live-test harness landed 2026-05-08. The default `cargo test`
run is unaffected — without `AGENTFLOW_LIVE_LLM_TESTS` set, every test in
`provider_consistency_live.rs` short-circuits before issuing a request and
reports `ok` in milliseconds. With the gate set but a provider's API key
env var missing, that single provider self-skips with a log line; the rest
still run.

Behavior of the harness:

1. Skips cleanly when `AGENTFLOW_LIVE_LLM_TESTS` is unset (test passes,
   prints `[live] <provider>: skipped`).
2. Calls `AgentFlow::init()` after the live gate is enabled, so
   `~/.agentflow/.env`, `AGENTFLOW_MODELS_CONFIG`, `~/.agentflow/models.yml`,
   and legacy `~/.agentflow/models.yaml` are loaded through the same resolver
   used by CLI and server code.
3. Uses minimum-cost defaults per provider (`gpt-4o-mini`,
   `claude-3-5-haiku-20241022`, `gemini-1.5-flash`, `moonshot-v1-8k`,
   `step-1-8k`); each is overridable via
   `AGENTFLOW_LIVE_<PROVIDER>_TEXT_MODEL`. The older
   `AGENTFLOW_LIVE_<PROVIDER>_MODEL` form remains accepted for compatibility.
   Examples: `AGENTFLOW_LIVE_STEPFUN_TEXT_MODEL`,
   `AGENTFLOW_LIVE_GLM_TEXT_MODEL`.
4. One single-turn text request per provider with `max_tokens = 16` and
   `temperature = 0.0` for deterministic-as-possible cost.
5. Runs nightly via `.github/workflows/llm-live.yml` (cron `30 9 * * *` UTC,
   plus `workflow_dispatch` with an optional `providers` filter); not part
   of the PR-blocking `release-gate` aggregate in `quality.yml`.

The harness logs provider names, selected env-var names, selected model names,
and skip reasons, but never prints API key values. Keep live runs small:
prefer one short prompt, low `max_tokens`, and explicit provider model
overrides. Do not put live tests in ordinary PR CI; use local manual runs or
nightly CI with budget/rate-limit controls.

## Adding a new provider

1. Implement `LLMProvider` in `agentflow-llm/src/providers/<name>.rs`.
2. Provide both `new(...)` and `with_client(...)` constructors. The
   `with_client` constructor is mandatory for the consistency suite to be
   able to wire in a no-proxy test client.
3. In `build_headers` / `build_auth_headers`, call
   `crate::trace_context::inject_into_headers(&mut headers)` last so
   `traceparent` propagation works automatically.
4. Map non-success HTTP responses to `LLMError::HttpError { status_code,
   message }`. Don't promote to `AuthenticationError` /
   `RateLimitExceeded` directly — that's a downstream consumer concern.
5. Add per-provider unit tests for request body shape + response parsing.
6. Add a row to `provider_consistency.rs` covering the success path and
   one error code.
7. Update this document with the capability matrix entries.

## Follow-ups (non-blocking)

- **Quota / cost dashboards**: currently each provider exposes raw
  `TokenUsage` on responses; a higher-level cost roll-up keyed by model is
  not implemented.

## Closed follow-ups

- **Streaming consistency tests** — landed 2026-05-08. Cross-provider
  streaming wire formats covered in `provider_consistency.rs` via a
  chunked-encoding mock server.
- **Multimodal consistency tests** — landed 2026-05-08. Each provider
  receives a text + image user message in its native format
  (OpenAI/Moonshot/StepFun: `image_url`; Anthropic: `image` content block;
  Google: OpenAI-style → translated to `inline_data` by the adapter).
  Tests assert the captured request body preserves the base64 payload and
  that the response parses to the same `(text, Stop, populated usage)`
  contract as the text-only path.
- **Live LLM nightly CI job** — landed 2026-05-08.
  `agentflow-llm/tests/provider_consistency_live.rs` plus
  `.github/workflows/llm-live.yml` (cron + `workflow_dispatch`). Tests
  default to a clean skip; nightly CI sets `AGENTFLOW_LIVE_LLM_TESTS=1`
  along with per-provider API keys from secrets. Each provider asserts the
  same contract as the offline suite: non-empty text, populated
  `TokenUsage`, `StopReason::Stop`.
