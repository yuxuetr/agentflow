# LLM Providers Support Matrix

> Status: foundation shipped in v0.4.0 (P1 #11). Streaming, multimodal, and
> live-LLM nightly CI all closed (see Closed follow-ups).
> Crates: `agentflow-llm`. Test entries:
> `agentflow-llm/tests/provider_consistency.rs` (offline, mocked) and
> `agentflow-llm/tests/provider_consistency_live.rs` (opt-in, real APIs).

AgentFlow's LLM abstraction targets six providers. This document is the
authoritative reference for what works on each, what doesn't, and how the
behavior is verified.

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

| Capability | OpenAI | Anthropic | Google | Moonshot | StepFun | Mock |
| --- | :-: | :-: | :-: | :-: | :-: | :-: |
| Text completion | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Token usage in response | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |
| Streaming | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Native tool calling (`tool_calls` / `tool_use` / `functionCall`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ (injection) |
| Multimodal text + image (URL or base64) | ✅ | ✅ | ✅ | partial | ✅ | n/a |
| Audio TTS / ASR | – | – | – | – | ✅ | n/a |
| Image generation | – | – | – | – | ✅ | n/a |
| W3C `traceparent` injection | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |
| `with_client(...)` for custom `reqwest::Client` | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |

Key:

- ✅ — supported, verified in unit + integration tests.
- partial — works for text but not all multimodal corner cases.
- – — not implemented. Most LLM providers don't ship the modality.
- n/a — Mock provider doesn't make HTTP calls; capability is irrelevant.

## Error mapping (verified contract)

All five HTTP-based providers map non-2xx responses to a single
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

The integration suite drives all five HTTP providers through a hand-rolled
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
cargo test -p agentflow-llm --test provider_consistency_live
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
2. Uses minimum-cost defaults per provider (`gpt-4o-mini`,
   `claude-3-5-haiku-20241022`, `gemini-1.5-flash`, `moonshot-v1-8k`,
   `step-1-8k`); each is overridable via
   `AGENTFLOW_LIVE_<PROVIDER>_MODEL`.
3. One single-turn text request per provider with `max_tokens = 16` and
   `temperature = 0.0` for deterministic-as-possible cost.
4. Runs nightly via `.github/workflows/llm-live.yml` (cron `30 9 * * *` UTC,
   plus `workflow_dispatch` with an optional `providers` filter); not part
   of the PR-blocking `release-gate` aggregate in `quality.yml`.

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
