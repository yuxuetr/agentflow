# Audit: agentflow-llm

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-llm/
**Crate version**: 0.2.0
**Layer**: L2 (Capability Adapter)
**Stability tier**: production-ready per CLAUDE.md (6 providers, native tool_calls, multimodal, traceparent — claims confirmed; OpenAI-compat factory paths for GLM/DashScope/DeepSeek/MiniMax also confirmed at `providers/mod.rs:249-252`)

## Scope summary

Read every `src/**/*.rs` file (35 files, ~13k LOC), the four integration test files (4051 LOC), `Cargo.toml`, `config/`, `templates/`, `benches/`, and `docs/`. Cross-referenced the CLAUDE.md claims against the implementation. Examined provider parity (OpenAI / Anthropic / Google / Moonshot / StepFun / Mock + OpenAI-compat factory entries), error redaction, streaming, tool-calling, multimodal, trace propagation, timeouts, and dependency footprint.

## Findings

### CRITICAL

- [C1] Google API key leaks through `reqwest::Error` → `LLMError` conversion — `agentflow-llm/src/providers/google.rs:136-140`, `agentflow-llm/src/error.rs:79-112`
  **What**: `GoogleProvider::get_model_endpoint` builds the URL as `{base}/v1beta/models/{model}:{method}?key={api_key}` and passes it to `client.post(url).send()`. When `send()` or `json()` fails, the error flows through `impl From<reqwest::Error> for LLMError`, which calls `error.to_string()` (lines 87, 109). `reqwest::Error`'s `Display` includes the URL (with query string) for connect/decode/redirect failures; the API key is in that query string. The same key is also in `validate_config`'s URL at `google.rs:422`. Any error in those paths writes the API key into `LLMError::HttpError.message` / `LLMError::InternalError.message`, which the CLI prints to stderr and the tracing layer captures.
  **Why it matters**: Production deployments shipping logs to centralised infra (or users copy-pasting error messages into issues) will leak the Gemini API key on the very first transient network failure.
  **Fix**: Move the key out of the URL into an `x-goog-api-key` HTTP header (Google supports it), or sanitise the URL before formatting the error. At minimum, add a custom `From<reqwest::Error>` arm for Google that strips the query string, and add a redaction test fixture.

- [C2] No HTTP timeouts on any provider's default client — `agentflow-llm/src/providers/mod.rs:35-44`
  **What**: `default_http_client()` builds `reqwest::Client::builder()` with nothing but the optional `.no_proxy()` fallback. There is no `.timeout(...)`, `.connect_timeout(...)`, or `.pool_*(...)`. By contrast, `discovery::create_http_client` does set a 30 s timeout (`discovery/mod.rs:164`). The default `From<reqwest::Error>` impl in `error.rs:82-84` even hard-codes `timeout_ms: 30000` in the error message, falsely implying a 30 s budget is enforced.
  **Why it matters**: A hung provider socket blocks the executing agent step forever — no `LLMError::TimeoutError`, no retry-loop trigger, no `agentflow-core` timeout hand-off. Combined with the `unsafe impl Send + Sync` in streaming responses (M2), this is a clear path to unkillable tasks under packet loss / provider brownouts. The misleading `timeout_ms: 30000` in errors actively confuses operators debugging the symptom.
  **Fix**: Wire a default `.timeout(Duration::from_secs(60))` plus `.connect_timeout(Duration::from_secs(10))` into `default_http_client()`. Make both overridable via env (`AGENTFLOW_LLM_TIMEOUT_SECS`, `AGENTFLOW_LLM_CONNECT_TIMEOUT_SECS`) so production can tune without recompiling. Update the hard-coded `30000` to read the actual value.

### MAJOR

- [M1] Native tool-call deltas are silently dropped during streaming — `agentflow-llm/src/providers/openai.rs:374-433`, `agentflow-llm/src/providers/anthropic.rs:386-440`
  **What**: `OpenAIStreamingResponse::parse_sse_chunk` only extracts `choice.delta.content`; it never inspects `delta.tool_calls`, which is how OpenAI emits incremental tool-call arguments during streaming. Anthropic's `AnthropicStreamingResponse::parse_sse_event` handles only `content_block_delta` with `text` and ignores `content_block_start`/`input_json_delta` (Anthropic's tool-use streaming events). `StreamChunk` itself has no tool-call field.
  **Why it matters**: Any agent that uses `execute_streaming()` and expects to react to tool calls early will see an empty stream when the model only emits tool calls. The non-streaming path (`execute()`) parses tool calls correctly, so callers can work around it by disabling streaming — but the silent drop is surprising and untested.
  **Fix**: Extend `StreamChunk` with `tool_call_deltas: Vec<ToolCallDelta>`, parse `delta.tool_calls` for OpenAI/Moonshot/StepFun (shared via `parse_openai_tool_calls` helper), parse `content_block_start{type=tool_use}` + `input_json_delta` for Anthropic. Add streaming-with-tools fixtures to `provider_consistency.rs`.

- [M2] Premature stream termination on Anthropic multi-block responses — `agentflow-llm/src/providers/anthropic.rs:425-433`
  **What**: `parse_sse_event` maps every `content_block_stop` event to `StreamChunk { is_final: true, ... }`. Anthropic emits `content_block_stop` after every content block (text and tool_use); the actual end-of-stream marker is `message_stop`. `next_chunk` reads the `is_final` flag and sets `self.finished = true` (line 463), returning `None` on the next poll.
  **Why it matters**: For any response that has more than one content block (text followed by tool_use, multi-paragraph responses split into blocks, etc.), the stream stops after the first block. Existing tests pass because the Anthropic mock fixture in `tests/provider_consistency.rs:674-680` only emits two `content_block_delta` events plus a `message_stop` — no `content_block_stop` is exercised.
  **Fix**: Drop the `content_block_stop` → `is_final` mapping. Only `message_stop` should terminate. Add a test fixture that emits `content_block_delta + content_block_stop + content_block_delta + message_stop` and assert the stream yields both text chunks.

- [M3] `expect("API key contains invalid characters")` panics in 5 providers' header builders — `agentflow-llm/src/providers/openai.rs:57`, `anthropic.rs:56`, `google.rs` (none — sends key in URL), `moonshot.rs:56`, `stepfun.rs:80,763`, `openai_asr.rs` (key in form)
  **What**: Every `build_headers` calls `HeaderValue::from_str(&format!("Bearer {}", self.api_key)).expect(...)`. The constructors only check `api_key.is_empty()`, not that the bytes form a valid HTTP header value. A key containing control bytes (e.g. an accidentally-appended `\n` from a `.env` file written by `printf`, or non-ASCII like a smart-quote pasted by a user) will panic mid-request rather than return `LLMError::AuthenticationError`. The comment "API key is validated in new(), so this should always succeed" is wrong.
  **Why it matters**: A bad `.env` brings down the executing thread. In `tokio` this often takes the runtime with it. Violates the global rule against `unwrap`/`expect` in non-test code.
  **Fix**: Move the `HeaderValue::from_str` check into each constructor (or into a shared helper) and return `LLMError::AuthenticationError { provider, message: "API key contains characters that cannot be sent as an HTTP header" }`. Then `build_headers` can use the pre-validated `HeaderValue` directly without `expect`.

- [M4] Unused `agentflow-core` dependency declared but never imported — `agentflow-llm/Cargo.toml:66`
  **What**: `Cargo.toml` declares `agentflow-core = { path = "../agentflow-core", version = "0.2" }`. `grep -r agentflow_core /Users/hal/arch/agentflow/agentflow-llm/src /Users/hal/arch/agentflow/agentflow-llm/tests` returns zero matches. CLAUDE.md describes L2 → L1 dependency as intentional, but the code does not actually use any L1 type.
  **Why it matters**: Pads the build graph and forces compiling `agentflow-core` whenever `agentflow-llm` is built standalone, slowing dev cycles and confusing the architectural picture. If this dep was supposed to be there (e.g., for shared error / event types), the integration is missing.
  **Fix**: Either delete the dep or introduce the planned integration. If the future-state is "LLM events flow into `agentflow-core::EventListener`", file an issue and add a `// TODO: P10.x — wire EventListener` comment; otherwise drop the line.

- [M5] `unsafe impl Send + Sync` on streaming responses is unjustified — `agentflow-llm/src/providers/openai.rs:371-372`, `anthropic.rs:366-367`, plus likely Google/Moonshot/StepFun mirroring the pattern
  **What**: Both `OpenAIStreamingResponse` and `AnthropicStreamingResponse` are `Pin<Box<dyn Stream<Item=Result<String>> + Send>>` plus `Option<String>` and `bool`. Inner stream is only `Send`, not `Sync`. The manual `unsafe impl Sync for ... {}` claims a contract the inner type does not provide. `bytes_stream()` returns a `Send` stream; nothing in the implementation requires `Sync`. There is no safety comment explaining the invariant.
  **Why it matters**: `Sync` means "&T can be sent across threads". Nothing in this code base requires that (the trait `StreamingResponse: Send + Sync` is over-tight). If a future refactor lets two threads simultaneously poll the same `&OpenAIStreamingResponse`, the `String` buffer becomes a data race. The unsafe assertion lies to the borrow checker, and the lie has no benefit.
  **Fix**: Drop `Sync` from the `StreamingResponse` trait bound (only `Send` is needed for `Box<dyn StreamingResponse>` to cross `.await`), then delete the `unsafe impl Sync` blocks. If `Sync` is genuinely required by some downstream constraint, document the safety invariant in a `// SAFETY:` comment.

- [M6] Full prompt + response logged at DEBUG by default — `agentflow-llm/src/client/llm_client.rs:144,174`
  **What**: When `enable_logging` is true (it defaults to `true` in `LLMClient::new`), `log_request_start` emits `debug!("Full prompt: {}", self.prompt)` and `log_request_complete` emits `debug!("Response content: {}", response)`. No redaction, no length cap. Anyone running with `RUST_LOG=agentflow_llm=debug` or with a tracing collector at DEBUG will capture every user prompt and LLM response — including secrets the user pastes in.
  **Why it matters**: PII / credentials leak into shared infra. The CLAUDE.md security section explicitly calls for "Mask sensitive data in logs and error messages", which this directly violates.
  **Fix**: Cap to a fixed character budget (e.g. 1024 chars truncated with `...` marker), or gate full-payload logging behind an explicit `AGENTFLOW_LLM_LOG_PAYLOADS=1` env var that is off by default. Same treatment for `Response content:`.

- [M7] `default_http_client()` panic-catching fallback is a fragile workaround — `agentflow-llm/src/providers/mod.rs:26-44`
  **What**: `build_http_client` calls `std::panic::catch_unwind(AssertUnwindSafe(...))` around `reqwest::ClientBuilder::build()`, and on panic retries with `.no_proxy()`. This is presumably hedging against a panic inside `system-configuration` (macOS proxy detection). But `catch_unwind` does not catch `abort` panics, only the default `unwind` panic strategy; release builds in the workspace currently inherit `unwind`, but if anyone ever sets `panic = "abort"` in `[profile.release]`, this hedge silently stops working and the process aborts on every fresh-host startup. The fallback also masks the root cause — no log, no `warn!`, just silent strategy switch.
  **Why it matters**: Brittle, undocumented, untested. The supposed real fix (use `.no_proxy()` always in tests, document that production may need it on macOS) is already what the test suite does.
  **Fix**: Replace with a single `reqwest::Client::builder().timeout(...).build()`. If `system-configuration` actually panics on some configurations, log a `warn!` and retry with `.no_proxy()` once, but make the fall-through visible to operators.

### MINOR

- [m1] `Default` impls in `discovery/` use `.expect()` to call constructors — `agentflow-llm/src/discovery/model_fetcher.rs:181`, `model_validator.rs:233`, `config_updater.rs:422`
  **What**: All three are `impl Default for X { fn default() -> Self { Self::new().expect("Failed to create X") } }`. `Self::new()` builds a reqwest client, which can fail. `Default::default()` is conventionally infallible; if anyone calls `X::default()` on a system with broken TLS, the process panics.
  **Why**: Violates the global no-`expect` rule for non-test code. Better to either drop the `Default` impl or have `new()` return `Self` and push the fallible work into a separate `with_client` constructor.

- [m2] `unwrap()` on `as_object()` for fresh `json!({})` values — `agentflow-llm/src/providers/google.rs:111`, `anthropic.rs:111`
  **What**: `if !generation_config.as_object().unwrap().is_empty()` and `if !body.as_object().unwrap().contains_key("max_tokens")`. Both are safe (the value was constructed locally with `json!({...})`), but they violate the global no-`unwrap` rule and would silently break if the construction site is ever changed to use a non-object literal.
  **Fix**: Use `if let Value::Object(obj) = &generation_config { if !obj.is_empty() {...} }`.

- [m3] `unwrap()` on `voice_label.as_mut()` in StepFun TTS builder — `agentflow-llm/src/providers/stepfun.rs:1163,1175,1187`
  **What**: Three call sites all guard with `if self.request.voice_label.is_none() { ... = Some(VoiceLabel { ... }) }` immediately above, so the `unwrap()` is logically safe. Same global-rule violation as [m2].
  **Fix**: Use `get_or_insert_with(VoiceLabel::default).language = Some(...)`. One line, no `unwrap`, no duplicated struct literal.

- [m4] `traceparent` propagation is only tested for OpenAI — `agentflow-llm/tests/trace_context_propagation.rs:137-214`
  **What**: All three tests in this file (`openai_emits_traceparent_when_context_active`, `openai_omits_traceparent_when_no_context_active`, `nested_scope_uses_inner_context_for_outbound_call`) exercise `OpenAIProvider`. There is no equivalent assertion for `AnthropicProvider`, `GoogleProvider`, `MoonshotProvider`, or `StepFunProvider`, even though each provider's `build_headers` calls `inject_into_headers` independently.
  **Fix**: Replicate the same three tests parameterised over `[OpenAIProvider, AnthropicProvider, GoogleProvider, MoonshotProvider, StepFunProvider]`. The infrastructure (`spawn_capturing_server`, `no_proxy_client`) is already shared.

- [m5] `From<reqwest::Error>` hard-codes `"unknown"` provider name — `agentflow-llm/src/error.rs:90-101`
  **What**: When `reqwest::Error` carries a 401 / 429 / 503 status, the impl maps to `AuthenticationError { provider: "unknown" }` etc. The actual provider is always known at the call site but cannot be plumbed through `From`. Downstream code that switches on `provider` (e.g., per-provider Retry-After parsing) cannot do so.
  **Fix**: Stop relying on `From<reqwest::Error>` in providers — each provider already explicitly constructs `LLMError::HttpError` with the response status in its `execute` arm (e.g. `openai.rs:173-180`), so the `From` impl is only hit on send-time/network errors. For those, introduce a `with_provider(provider: &str)` wrapper, or have each provider `.map_err(|e| ProviderError::from(e).with_provider("openai"))`.

- [m6] No `Retry-After` parsing on 429 responses — searched `src/`, no matches
  **What**: When a provider returns 429, the response body becomes `LLMError::HttpError.message` (or `RateLimitExceeded` via the `From` impl), but the `Retry-After` HTTP header is dropped. `agentflow-core::retry_executor` cannot honour the server's back-off advice.
  **Fix**: Extract `Retry-After` from the response and attach it to a typed `LLMError::RateLimitExceeded { provider, message, retry_after_secs: Option<u64> }` (requires variant shape extension; current callers `match` on it, so add `retry_after_secs` with `#[serde(default)]`).

- [m7] `Vec<u8>` payload cloning in `OpenAIAsrProvider::build_form` — `agentflow-llm/src/providers/openai_asr.rs:83,87`
  **What**: `request.audio_data.clone()` is called twice — once for the primary part, once for the fallback if MIME detection fails. For large audio uploads (multi-megabyte) this doubles the allocation on the unhappy path.
  **Fix**: Detect the MIME first, then clone exactly once. Trivial refactor.

- [m8] `LLMError::TimeoutError` hard-codes 30 s even when no timeout is enforced — `agentflow-llm/src/error.rs:81-84`
  **What**: As noted in [C2], the default client has no timeout, but the `From<reqwest::Error>` impl claims the error came at `timeout_ms: 30000`. Operators debugging a hang will trust the message and look in the wrong place.
  **Fix**: Once [C2] is fixed, plumb the actual configured value through (e.g., via a thread-local or `OnceCell` set when the client is built). Or, at minimum, change the message to `"Request timed out (reqwest default)"`.

- [m9] OpenAI `parse_openai_tool_calls` falls back to `Value::String` on malformed JSON without warning — `agentflow-llm/src/providers/openai.rs:135`
  **What**: `serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))`. Documented as intentional ("the call can still surface in traces") but no `warn!` or telemetry is emitted, so operators have no visibility into malformed-arguments rates.
  **Fix**: Add `tracing::warn!(provider = "openai", "tool_call arguments are not valid JSON, falling back to raw string")` so this shows up in dashboards.

- [m10] `MockProvider`'s streaming response emits a single chunk, not a multi-chunk stream — `agentflow-llm/src/providers/mock.rs:144-169`
  **What**: `MockStreamingResponse::next_chunk` returns the whole response in one `is_final: true` chunk. Real providers emit dozens of chunks. Tests that depend on the Mock to validate multi-chunk handling (buffering, partial-SSE, etc.) get a free pass.
  **Fix**: Add `MockProvider::with_chunked_response(chunks: Vec<String>)` so tests can opt into realistic chunking.

- [m11] `_` parameter naming dishonesty — `agentflow-llm/src/providers/mock.rs:41`
  **What**: `pub fn new(_api_key: &str, _base_url: Option<String>) -> Result<Self>`. The leading underscores hint "unused", but both are part of the `LLMProvider` factory contract (`providers/mod.rs:243-256`). Misleading on inspection.
  **Fix**: Drop the underscores — the parameters are deliberately accepted to match the factory shape; add a `// API-shape parity with other providers` comment.

- [m12] Lint-by-omission: no `#![deny(clippy::unwrap_used, clippy::expect_used)]` — `agentflow-llm/src/lib.rs`
  **What**: 12 non-test `unwrap`/`expect` calls slipped past review (counted above). A crate-level deny would catch new ones at CI time.
  **Fix**: Add `#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]` after fixing the existing offenders.

### POSITIVE OBSERVATIONS

- Test discipline around `.no_proxy()` is exemplary. Every test that hits a localhost mock uses `no_proxy_client()`, including the three integration test files (`provider_consistency.rs`, `provider_consistency_live.rs`, `trace_context_propagation.rs`) — over 60 call sites. The user's global rule is followed perfectly.
- `with_client(client, ...)` constructor pattern is consistently provided for all six providers, enabling tests / production to inject a custom `reqwest::Client`. This is the right primitive and is what makes the `.no_proxy()` discipline possible.
- `provider_consistency.rs` is a 2129-line suite that pins per-provider invariants (success / tool-call / streaming / multimodal / status-code mapping) across all 5 production providers — strong defense against silent regressions when a wire-format detail drifts.
- `trace_context.rs` is a clean, well-documented W3C implementation with rejection of malformed inputs (`is_lower_hex`, all-zero ids), task-local propagation, and round-trip tests. The integration with provider `build_headers` is minimal and consistent.
- `LLMError::MissingApiKey` renders a per-provider env-var hint (`error.rs:115-134`), `agentflow config init` suggestion, and `~/.agentflow/.env` pointer. The "fresh-host operator can act in one read" goal is met. Tests cover unknown-provider fallback.
- `OpenAIAsrProvider::Debug` impl explicitly redacts the API key (`openai_asr.rs:42-51`) — the only provider that bothers to do this. Should be the template for the others.
- `ModelRegistry` distinguishes "provider unsupported" from "provider supported but key missing" via `missing_key_providers` (`registry/model_registry.rs:117-131`). Excellent error-UX detail.
- `Arc<dyn LLMProvider>` cached in `ModelRegistry::providers` means reqwest's internal connection pool is shared across calls per-provider — the "shared `reqwest::Client`" question in the audit prompt is answered correctly.
- The OpenAI-compat factory entries (GLM/DashScope/DeepSeek/MiniMax mapping to `OpenAIProvider`, `providers/mod.rs:249-252`) match the CLAUDE.md claim and avoid 4 redundant provider modules.
- `stepfun.rs` (1569 LOC, by far the largest provider) is comprehensively unit-tested in its `#[cfg(test)]` block, including the specialized client's TTS / ASR / image paths.
- TODO/FIXME footprint is tiny: 2 markers total (`tokenizer.rs:29`, `client/llm_client.rs:494`), both narrowly scoped and acknowledged.

## Metrics

- Source files: 35
- Lines of code (src/): ~12,958 (workspace counts: 17,088 incl. tests/benches)
- Providers implemented: 6 distinct modules (OpenAI, Anthropic, Google, Moonshot, StepFun, Mock) + 1 modality-specific (OpenAIAsrProvider) + 4 OpenAI-compat factory passthroughs (GLM/Zhipu, DashScope, DeepSeek, MiniMax)
- Test files: 4 integration suites (`provider_consistency.rs`, `provider_consistency_live.rs`, `provider_matrix_doc.rs`, `trace_context_propagation.rs`) totaling 4051 LOC + per-module `#[cfg(test)]` blocks in nearly every source file (~13 `mod tests`)
- `unwrap()/expect()` in non-test code: 12 calls
  - `discovery/model_fetcher.rs:181` — `expect` in `Default` impl
  - `discovery/model_validator.rs:88` — `unwrap` after `is_none` guard (safe but lint-bait)
  - `discovery/model_validator.rs:233` — `expect` in `Default` impl
  - `discovery/config_updater.rs:422` — `expect` in `Default` impl
  - `providers/openai.rs:57` — `HeaderValue::from_str(api_key).expect(...)`
  - `providers/anthropic.rs:56`, `anthropic.rs:111` — same `expect` + `as_object().unwrap()`
  - `providers/google.rs:111` — `as_object().unwrap()` on local `json!({})`
  - `providers/moonshot.rs:56` — `HeaderValue::from_str(api_key).expect(...)`
  - `providers/stepfun.rs:80`, `stepfun.rs:763` — `HeaderValue::from_str(api_key).expect(...)`
  - `providers/stepfun.rs:1163,1175,1187` — `voice_label.as_mut().unwrap()` after `is_none` guard
  - `providers/openai_asr.rs:90` — `Part::mime_str("application/octet-stream").expect(...)` (static fallback; safe but lint-bait)
- Tests NOT using `.no_proxy()`: 0 — every reqwest client construction in `tests/` uses `Client::builder().no_proxy()...`. Exemplary.
- TODO/FIXME in code: 2 (`src/tokenizer.rs:29`, `src/client/llm_client.rs:494`)
- Public items missing rustdoc: ~32 of ~116 (estimated 28%). Most are obvious builder methods (`temperature`, `max_tokens`, etc. in `llm_client.rs`), simple field accessors, and one-line factory functions. The non-trivial APIs (`AgentFlow::*`, `LLMProvider` trait, `LlmTraceContext`, `MultimodalMessage`, `ToolSpec`/`ToolChoice`/`StopReason`) are well-documented.

## Recommendations (prioritized)

1. **Stop the Google API key leak (C1).** This is a confidentiality bug in a production-tier crate; one transient network failure leaks a key. Move to `x-goog-api-key` header. ~2 h.
2. **Wire default timeouts on all providers (C2)** and stop hard-coding `30000` in the timeout error. ~1 h. Pair with reading two new env vars so operators can tune in-flight.
3. **Replace the 6 `HeaderValue::from_str(api_key).expect(...)` panics (M3)** with validation at construction time returning `LLMError::AuthenticationError`. ~30 min — applies to all 5 production providers plus their `StepFunSpecializedClient`.
4. **Fix the Anthropic premature-termination bug (M2)** — drop the `content_block_stop → is_final` mapping, add a multi-block streaming test fixture. ~1 h. Tool-call streaming (M1) can ride the same PR by extending `StreamChunk` and the Anthropic + OpenAI parsers.
5. **Audit and trim the logging surface (M6)** — cap prompt/response logs at a fixed budget or gate behind opt-in env var. Update `OpenAIAsrProvider`'s redacted-Debug pattern across all providers. ~1 h.
6. **Remove the unused `agentflow-core` dependency (M4)** or document the planned integration. ~5 min.
7. **Drop the unjustified `unsafe impl Sync` (M5)** by loosening the `StreamingResponse` trait bound to `Send` only. Run the test suite to confirm no regressions. ~30 min.
8. **Cleanup pass on the 12 non-test `unwrap`/`expect` calls (m1, m2, m3, m12)** and turn on the crate-level lint to prevent regressions. ~1 h total.
9. **Extend `traceparent` propagation tests (m4)** to cover all 5 providers — the same `spawn_capturing_server` infrastructure works for each. ~1 h.
10. **Surface `Retry-After` (m6)** so `agentflow-core::retry_executor` can do server-advised back-off. Requires variant-shape extension. ~2 h.

End of report.
