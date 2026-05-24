# Audit: agentflow-cli

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-cli/
**Crate version**: 0.2.0 (workspace edition 2024)
**Layer**: L3 (User Interface / Unified CLI binary)
**Stability tier**: beta — the `agentflow.cli/1` JSON envelope (`docs/CLI_JSON_OUTPUT.md`) is a stable wire contract; individual subcommands carry their own per-command stability tiers.

## Scope summary

`agentflow-cli` ships a single binary (`agentflow`) and a thin library facade re-exporting `commands`, `config`, `executor`, `json_envelope`, `redaction`, `server_client`. The CLI is the operator-facing entry point for **every** layer of the AgentFlow stack: workflow DAGs (`workflow run|list|cancel|logs|validate|resume-plan|debug`), agent loops (`harness run|resume|list|inspect|replay`, `agent replay`), eval datasets (`eval run`), LLM model discovery (`llm models`), MCP/Skill/Plugin/Marketplace package management, RAG search/index/eval, observability (`trace replay|tui`, `doctor`), audio (`audio asr|tts|clone`), image (`image generate|understand`), config (`config init|show|validate`), platform mode (`serve`, `cleanup`, `backup`, `memory prune`).

Total: 84 Rust source files (21,274 LoC), 27 integration test files (~304 tests), 10 example workflows. `src/main.rs` alone is 2,257 lines because every subcommand surface is defined inline via clap derive. The library exposes only ~12 public items (`CliJsonEnvelope`, `redact_cli_*`, `ServerClient`, env-var consts) plus the implicit `commands::*` modules invoked by `main.rs`.

I reviewed: `Cargo.toml`, `docs/CLI_JSON_OUTPUT.md`, the full `src/main.rs` clap surface, `src/lib.rs`, `src/json_envelope.rs`, `src/redaction.rs`, `src/server_client.rs`, the workflow / skill / audio / image / harness / doctor / backup / eval / executor command implementations, the `tests/` listing, and the `RoadMap.md` CLI roadmap entries.

## Findings

### CRITICAL
- None. The CLI has a sound architecture (clap derive, anyhow chain errors, redaction baked into all output paths, JSON envelope contract, hardened marketplace unpacker, dotenvy precedence rules) and no security or data-integrity issues at the level of "this will silently corrupt state".

### MAJOR

- [M1] `audio asr --prompt VALUE` writes the transcription to a file at path `VALUE` — `src/main.rs:1645-1651` + `src/commands/audio/asr.rs:5-11`
  **What**: The clap struct `AudioCommands::Asr` exposes a `prompt: Option<String>` flag (declared `main.rs:631`). The dispatch site at `main.rs:1651` calls `audio::asr::execute(file_path, model, format, language, prompt)`. The handler signature at `src/commands/audio/asr.rs:5-11` declares its 5th positional argument as `output: Option<String>`. The compiler is happy because both are `Option<String>` — but at runtime `--prompt "describe this audio"` is routed to the `output` parameter and `asr.rs:80-92` `fs::write(&output_path, ...)` happily writes the transcription to a file literally named "describe this audio".
  **Why it matters**: silent data loss / surprise file creation in `$CWD`. The `--prompt` flag has been dead since this dispatch site was wired up — there are zero integration tests under `tests/` exercising `audio asr` (or any audio subcommand) so the bug shipped uncaught.
  **Fix**: split the argument: either (a) thread `prompt` into `AsrRequest::prompt` (currently hard-coded to `None` at `asr.rs:56`) and add an explicit `--output` flag, or (b) rename `--prompt` to `--output` in the clap struct to match what the handler actually does. Add an `audio_cli_tests.rs` happy-path test that confirms `--prompt` does not produce a file. Also audit the sibling dispatchers (`audio tts` at `main.rs:1660-1668`, `audio clone` at `main.rs:1652-1659`) — by inspection their argument order matches, but no test gate catches a future regression.

- [M2] `agentflow workflow run` has no signal handler; Ctrl-C tears the process down mid-workflow with no state flush — `src/commands/workflow/run.rs:241-286`
  **What**: `run_with_retries` awaits `flow.execute_from_inputs_with_id_and_config` inside `tokio::time::timeout` but never selects against `tokio::signal::ctrl_c()`. The whole src tree has zero matches for `ctrl_c` / `signal::` (only a `println!` in `skill/chat.rs:65` advising users to press Ctrl-C). For a workflow that has emitted partial JSONL trace events to `~/.agentflow/traces/<workflow_id>/`, Ctrl-C kills the runtime instantly — pending event-drain tasks (`agentflow-tracing` event listener) may not flush their queues, and the file the operator was told to inspect via `agentflow trace tui <workflow_id>` could be missing the last N events.
  **Why it matters**: when CLAUDE.md promises "the in-process drain task processes events in arrival order so terminal node state cannot race the `WorkflowCompleted` save", that holds **only** for graceful completion. Operator Ctrl-C on a long-running workflow is a real-world case and currently produces inconsistent trace files.
  **Fix**: wrap the `flow.execute_from_inputs_with_id_and_config` future in a `tokio::select!` against `tokio::signal::ctrl_c()`. On signal: (a) call into the existing `agentflow-core` cancellation token (or wire one if not exposed), (b) await the trace collector's flush, (c) print a `🛑 Cancelled` line, exit with code 130 (POSIX `128 + SIGINT`). Mirror the same pattern in `harness run` (`src/commands/harness/run.rs`) and `skill chat` (interactive).

- [M3] `agentflow llm chat` is documented as "Deprecated compatibility stub" but its hidden flags `--model/--system/--save/--load` are silently accepted then ignored — `src/main.rs:736-748` + `main.rs:1720-1727`
  **What**: The clap variant exposes four flags whose values are immediately discarded (`model: _, system: _, save: _, load: _` at `main.rs:1721-1724`). The handler always returns `Err(anyhow!(...))` regardless of input. Users who pass `--save my-conversation.json` get the deprecation error message with no hint that their requested action did nothing.
  **Why it matters**: documentation-vs-reality drift. The visible-help shows `[command(hide = true)]` so it doesn't appear in `agentflow llm --help`, but the flags are still parseable and operators who copy old commands from blog posts get no signal that `--save` is a no-op.
  **Fix**: drop the flags entirely (so unknown-arg validation rejects them) or keep them but emit `eprintln!("⚠  --save / --load / --model / --system are ignored; `agentflow llm chat` is retired")` before returning the error. Lower-risk alternative: bare `Chat {}` with no args — users get a clean clap error explaining the migration.

- [M4] CLAUDE.md advertises `agentflow plugin` / `agentflow rag` as L3 commands but they are feature-gated and not built by default — `Cargo.toml:18-24`, `src/main.rs:58-69, 429-449, 2127-2243`
  **What**: `[features] default = []`. The `plugin` and `rag` subcommands exist as enum variants under cfg guards. When the binary is built without the feature, the user gets a `Commands::Plugin(FeatureUnavailableArgs)` stub whose `after_help` says "Rebuild with the matching Cargo feature". CLAUDE.md's "L3 — agentflow-cli" bullet point lists `rag search|index|collections` and the broader doc claims plugin commands are "production-ready" under N10. New users following the docs will hit the feature-disabled stub.
  **Why it matters**: doc-vs-build drift. `agentflow rag --help` shows nothing useful in the default cargo install. CLAUDE.md "Pre-Commit Requirements" doesn't mandate that the published binary include these features.
  **Fix**: either (a) flip `default = ["plugin", "rag"]` in `Cargo.toml` so the binary distribution matches the docs (this is the user-friendly path; both features are documented as closed/production), or (b) update CLAUDE.md and `RoadMap.md` to mark the commands as "opt-in feature builds" so operators know to pass `--features plugin,rag`. Recommend (a) — N10 is closed per CLAUDE.md and gating production features by default contradicts the "v1.0.0-rc candidate" framing.

- [M5] `agentflow workflow logs --follow` SSE consumer has no exponential backoff or reconnect logic — `src/server_client.rs:276-334`
  **What**: `stream_events_sse` builds a no-timeout `reqwest::Client`, opens the SSE stream, and loops on `byte_stream.next().await`. When the underlying TCP connection drops (network blip, server restart), the call returns `Err` via the `?` propagation at line 318. The CLI `workflow logs` command surfaces this as a hard error and exits — operators using `--follow` for a long-running run lose their tail and must reissue the command manually with `--after-seq <last_seen>`. The CLAUDE.md `workflow logs` doc string promises "keeps streaming until the server closes or the user cancels" which the code under-delivers when the network hiccups.
  **Why it matters**: production usability. The server has a 15s SSE keep-alive but real-world long runs (hours) almost certainly see at least one transient blip. The `--after-seq` plumbing is in place so a single retry loop with capped exponential backoff would be ~30 lines.
  **Fix**: wrap the existing `stream_events_sse` call in a retry loop inside `workflow::server_ops::logs` (`src/commands/workflow/server_ops.rs`): track `last_seq` as events arrive, on transport error sleep with exponential backoff (1s → 2s → 4s → … cap 30s), reconnect with `after_seq = last_seq + 1`. Emit a `⚠  reconnected after N events` line to stderr so the operator knows. Add an integration test that simulates a mid-stream EOF.

- [M6] `commands/eval.rs` uses `Mutex::lock().unwrap()` in async code, panicking the worker if the cache mutex is poisoned — `src/commands/eval.rs:240, 252`
  **What**: `ReActAgentFactory::resolve_validator` reaches for `self.validators.lock().unwrap()` twice. CLAUDE.md "Rust" guidance explicitly forbids `unwrap()` outside test code. The mutex is poisoned only after a previous thread panicked while holding it, but eval runs can spawn multiple cases concurrently and a panic in one validator construction would poison the cache for all subsequent cases.
  **Why it matters**: violates the project rule and produces a non-actionable `thread panicked at 'PoisonError'` line instead of a useful eval error.
  **Fix**: use `.lock().unwrap_or_else(|e| e.into_inner())` (poison-tolerant — the cache value is just a `HashMap`, no torn-write risk) or migrate to `tokio::sync::Mutex` since this is inside an async runtime anyway. Lower-effort: a `parking_lot::Mutex` (no poisoning at all).

### MINOR

- [m1] `audio asr` / `image generate` / `image understand` have **zero** assert_cmd integration tests
  Search across `tests/*.rs` returns no matches for `"audio"` or `"image"` commands. Combined with M1 above, this is the proximate cause of the silent prompt-as-output bug. Add at least: `audio asr --help` (help text loads), `audio tts <missing-file>` (error path), `image generate --help` (help loads). The bare `--help` smoke tests would have caught a `tokio::main` regression too.

- [m2] `redaction.rs` public functions lack `///` rustdoc comments — `src/redaction.rs:7, 11, 15`
  Three public re-export helpers (`redact_cli_text`, `redact_cli_value`, `to_redacted_json_value`) have no doc comments. They're trivial wrappers over `agentflow-tracing` but since they're the canonical CLI entry point for redacting operator-facing output, a one-line `///` each describing precedence (`RedactionConfig::default()`) and use-case (call before `println!`) would help reviewers spot redaction bypasses.

- [m3] `src/main.rs` is 2,257 lines and pure clap glue; one giant `enum Commands` plus 14 sibling Args/Subcommand structs
  `clippy::large_enum_variant` had to be muted on `HarnessCommands` (`main.rs:298`). The `match cli.command { ... }` block runs from line 1445 to 2244. Refactoring proposal: split each top-level command's clap structs into their own `commands/<name>/cli.rs` (next to the existing `commands/<name>/mod.rs`) and have main.rs just dispatch — would cut `main.rs` to ~300 lines and let new command authors stay inside one directory. Not a correctness issue but a maintenance one.

- [m4] `agentflow doctor` calls `std::process::exit(report.status.exit_code())` from `commands/doctor.rs:400`, bypassing main's `Result` return path
  Same pattern in `commands/cleanup.rs:48`, `commands/serve.rs:98`, `commands/backup.rs:214`, `commands/rag/eval.rs:244`, `commands/eval.rs:100`. These work, but inconsistency means any future cleanup (Drop guards, async destructors) added between the command body and `main()`'s exit point gets skipped for these commands. Prefer returning a non-zero `Result::Err` and letting `main()`'s single `process::exit(1)` at line 2255 handle it. Doctor is special because it has more than two exit codes (0=ok, 1=warning, 2=fail) — make `main()` aware of a `CliExitCode(u8)` error type rather than hard-coding `1` everywhere.

- [m5] `parse_input_value` at `commands/workflow/run.rs:188-190` silently falls back to a string if the JSON parse fails, with no diagnostic
  `serde_json::from_str(raw_value).unwrap_or_else(|_| Value::String(raw_value.to_string()))` — a user who types `--input answer '{ "broken": json' ` gets the malformed text as a literal string instead of an error. CLI usability would be better with `--input answer @-` for JSON-from-stdin or at least a `--input-json` variant that bails on parse failure.

- [m6] `load_agentflow_dotenv()` runs unconditionally before `Cli::parse()` (`main.rs:1442`), so `agentflow --help` triggers a file-stat
  Microscopic perf hit (~50 µs) but it means `agentflow --help` is not hermetic — a malformed `~/.agentflow/.env` (e.g. with a `=` in a value) prints a dotenvy warning to stderr before help even renders. Consider gating dotenv loading on `cli.command` actually needing env vars (i.e. skip for help/completion/version paths), or at minimum silence dotenvy warnings unless the operator passes `-v`.

- [m7] `#[tokio::main]` with `features = ["full"]` spins up the multi-thread runtime even for synchronous commands like `mcp config path / validate / show / list`
  `mcp config` handlers at `main.rs:1768-1771` are non-async (`run_path`, `run_validate`, `run_list`, `run_show`) yet pay for the full Tokio runtime. Negligible at human-interactive timescale but contributes to a slower cold-start than necessary for shell-completion scripts. Switch to `tokio = { version = "1.35", features = ["rt-multi-thread", "macros", "fs", "net", "signal", "process", "time", "io-util", "sync"] }` and audit which features actually leak through.

- [m8] `agentflow llm chat` accepts and silently drops `--model/--system/--save/--load` (see M3 for the bigger framing) — file-level note: `main.rs:1720-1727`

- [m9] Output stability for several commands listed as "planned" in `docs/CLI_JSON_OUTPUT.md` coverage matrix
  `agentflow plugin list/install/inspect`, `agentflow trace list/replay/show`, `agentflow workflow run/list/cancel/graph/logs` row says envelope migration is "planned". Until then JSON-consuming tooling has to special-case those commands. Track each as its own follow-up commit per the doc's plan, or at least add a `// TODO(envelope): wrap in CliJsonEnvelope` marker at the print site so the gap is greppable.

- [m10] `executor/factory.rs:317` returns `Err(anyhow!("Unknown node type: {}", node_def.node_type))` with no hint about feature gating
  When a workflow declares `type: rag` but the binary was built without `--features rag`, the user sees "Unknown node type: rag" instead of "node type 'rag' requires `cargo build -p agentflow-cli --features rag`". Add a short feature-mapping lookup in the `_ =>` arm.

- [m11] `commands/skill/chat.rs:65` prints `"Ctrl-C to exit."` but the interactive chat loop has no `tokio::signal::ctrl_c()` handler either
  Same class as M2 — Ctrl-C kills the process; pending memory writes to SqliteMemory may not flush cleanly. Lower severity than M2 because chat history loss is less surprising than workflow-trace truncation.

- [m12] `redact_cli_text` at `src/redaction.rs:7` is called on user message (`commands/skill/run.rs:87, 106, 127`) but the same skill's eventual answer redaction relies on the agent runtime returning already-redacted content
  If the agent's final answer contains a literal API key (e.g. echoed back from a tool result), `redact_cli_text(&answer)` does run, but the JSON `trace` field at `commands/skill/run.rs:132` goes through `to_redacted_json_value` which uses the structural-key redactor. Confirm with a test that a tool result like `{"output": "sk-proj-XXX"}` actually gets redacted by the structural pass — the current logic redacts by **key name** (`api_key`, `secret`, `token`, …) so a payload that happens to *contain* a key string under an innocuous key like `"output"` is not redacted. The text-fragment redactor (`redact_text`) does catch `sk-…` patterns but is only called on `&str` not on JSON values.

### POSITIVE OBSERVATIONS

- **Canonical JSON envelope is well-engineered** — `src/json_envelope.rs` is 162 lines including 5 round-trip tests, has a clear contract doc (`docs/CLI_JSON_OUTPUT.md`), and is consistently applied to new JSON outputs (doctor, backup, eval, memory prune, workflow validate, marketplace, mcp config list, plugin list/install/inspect, llm models, rag search/eval, harness commands). The `envelope_field_set_is_closed_to_four_keys` test locks in the contract.
- **Redaction is plumbed in at every print/serialize seam** — `workflow run` final state (`run.rs:139`), `config show` YAML (`show.rs:41`), `skill run` text + JSON + trace paths (`skill/run.rs:87,106,127,132`), `skill validate` MCP server args (`validate.rs:133`). Few CLIs treat redaction as a first-class concern.
- **No `unwrap()` / `expect()` in production code paths** — every match outside `#[cfg(test)]` is either justified (`server_client.rs:307` uses `.unwrap_or_default()`; `skill/mcp_discovery_cache.rs:194` notes "canonical hash input must serialise" for a `Vec<simple struct>`) or trivially infallible. The two true exceptions are M6 (eval mutex) and a single `unwrap_or_else` fallback in `parse_input_value`.
- **`reqwest::Client::builder().no_proxy()` is consistently used in `server_client.rs`** (lines 110, 292) — matches the project's CLAUDE.md HTTP guidelines exactly and prevents the macOS Clash-proxy regression class.
- **Marketplace unpacker is hardened** — `commands/marketplace.rs:395-505` enforces entry-count cap, path-traversal rejection, duplicate detection, per-file size cap, cumulative decompression-bomb cap, and absolute-path / `..` / non-UTF-8 rejection. The dedicated `marketplace_unpack_hardening_tests.rs` file proves the rules.
- **Shell node refuses empty `allowed_commands`** — `executor/shell.rs:64-80` bails at YAML parse time if `allowed_commands` is missing or empty, rather than running with a wide-open shell allowlist. Same pattern in `factory.rs` for plugin manifest/node_type required params.
- **Server-mode local-only flag validation** — `commands/workflow/server_ops.rs::reject_local_only_flags` and `commands/skill/server_ops.rs::reject_local_only_flags` reject misused flags up front with actionable messages, closing the silent-drop class of bug (P10.11.4).
- **Anyhow error chain printed with `{:#}` formatter** — `main.rs:2254` prints the full cause chain rather than only the outermost message; the comment at lines 2247-2253 documents the rationale tied to a real past bug (P9.1 / F-AF-1).
- **27 integration test files / ~304 tests / assert_cmd-based** — broad coverage of workflow, config, doctor, eval, harness (run + replay), marketplace (incl. unpack hardening), mcp config, plugin, rag eval, skill (lots), trace, workflow logs SSE, cross-hop E2E with traceparent. Test density is high.
- **Subcommand layout matches CLAUDE.md "L3 — agentflow-cli" docs exactly** — `workflow run|validate|debug`, `config init|show|validate`, `llm models`, `skill *`, `mcp list-tools|call-tool|list-resources`, `trace replay|tui`, `audio asr|tts`, `image generate|understand`, `rag search|index|collections` are all present. Required flags for `workflow run` (`--input/--dry-run/--output/--timeout/--max-retries/--model/--run-dir/--max-concurrency`) are all present and documented in clap help.
- **CLAUDE.md L3 boundary respected** — command modules do not re-implement domain logic. `workflow/run.rs` delegates to `agentflow_core::Flow`; `harness/run.rs` wires `agentflow_agents::ReActAgent` into `agentflow_harness::HarnessRuntime`; `skill/run.rs` defers to `SkillBuilder::build`. The CLI is a thin orchestration layer per the L3 contract.

## Metrics

- Source files: **84** (`.rs` files under `src/`)
- Lines of code: **21,274** (incl. tests; `main.rs` 2,257; `commands/workflow/server_ops.rs` 700; `commands/doctor.rs` ~1,330; `config/schema.rs` 719)
- Top-level subcommands: **18** — `workflow`, `audio`, `config`, `image`, `llm`, `mcp`, `memory`, `skill`, `marketplace`, `trace`, `doctor`, `harness`, `agent`, `serve`, `cleanup`, `backup`, `eval`, `plugin` (feature-gated), `rag` (feature-gated)
- Test files: **0 inline `#[cfg(test)] mod tests` counted separately** + **27 integration files in `tests/`** (~304 assert_cmd-based functions). Integration coverage gaps: `audio asr|tts|clone` (zero tests), `image generate|understand` (zero), `llm models --refresh-from-api` (no live-API test in this crate).
- `unwrap()` / `expect()` in non-test, non-main code: **2 true cases** (both `commands/eval.rs:240, 252` `Mutex::lock().unwrap()`). Every other unwrap/expect occurrence falls inside `#[cfg(test)]` modules or is a deliberately justified infallible (`commands/skill/mcp_discovery_cache.rs:194` for `serde_json::to_vec` of a `Vec<&str-only struct>`).
- TODO / FIXME / XXX / HACK: **5 occurrences**, all benign — 1 in a comment about TODOs.md migration plan (`json_envelope.rs:30`), 2 in clap help text mentioning `TODOs.md`, 1 in a deprecation message, 1 in a `/// the TODO:` doc-comment (`commands/agent/replay.rs:7`).
- Public items missing rustdoc: **~3** (the three free fns in `src/redaction.rs`). The other public items (envelope, server client, env-var consts) have rustdoc.

## Recommendations (prioritized)

1. **Fix M1 immediately** — silent data-loss bug in `audio asr --prompt`. Two-line patch (route `prompt` into `AsrRequest::prompt`, add a separate `--output` flag) plus a happy-path assert_cmd test. Backport into the next patch release.
2. **Add Ctrl-C handling to `workflow run` and `harness run`** (M2). Wire `tokio::select!(_ = workflow_future, _ = signal::ctrl_c())` with a cancellation token and trace-flush on signal. Exit code 130.
3. **Flip `default = ["plugin", "rag"]` in `Cargo.toml`** (M4) so the published binary matches CLAUDE.md / RoadMap.md. Document the trade-off (extra dep weight, Qdrant client only loaded when used) in a Cargo.toml comment.
4. **Replace `Mutex::lock().unwrap()` in `commands/eval.rs:240, 252`** with poison-tolerant `unwrap_or_else(|e| e.into_inner())` or migrate to `parking_lot::Mutex`. Trivial patch, satisfies the project rule.
5. **Plug the audio + image integration-test gap** (m1). At minimum: `agentflow audio asr --help`, `agentflow audio tts --help`, `agentflow image generate --help`, plus one error-path test per command (missing file, missing required arg). Prevents M1-class regressions.
6. **Add SSE reconnect-with-backoff in `workflow logs --follow`** (M5). The `--after-seq` infrastructure is already in place; this is a ~30-line retry wrapper around `stream_events_sse`.
7. **Standardize exit-code handling** (m4) — introduce a `CliExitCode(u8)` error variant so commands return `Err(CliExitCode(2))` rather than scattering `std::process::exit(code)` calls across 6 files. `main()` then has one exit point.
8. **Refactor `src/main.rs` clap definitions out into per-command `cli.rs` modules** (m3). Pure cosmetic but unblocks future contributors. Could land as a single tree-wide commit since no behaviour changes.
9. **Wrap deprecated `llm chat` flags** (M3 / m8) — either drop them or emit a per-flag warning before erroring out.
10. **Audit redaction structural-vs-text coverage for trace payloads in `skill run --trace`** (m12). Add a test that a tool-result string containing `sk-…` is redacted by the text fragment pass even when nested inside a JSON value under an innocuous key.

End of report.
