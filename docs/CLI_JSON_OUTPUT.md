# CLI JSON Output Envelope

Status: stable contract as of `P3.3`.
Crate: `agentflow-cli`.
Wire schema: `agentflow.cli/1`.
Reference implementation: [`agentflow_cli::json_envelope::CliJsonEnvelope`](../agentflow-cli/src/json_envelope.rs).

`agentflow` CLI commands that support automation expose a `--format
json-envelope` (or `--output json-envelope`) mode that emits the
canonical envelope below. The envelope is the v1 contract every new
JSON output mode follows; existing raw-JSON modes (`--format json`)
remain available for backward compatibility until v1.0.

## Envelope shape

```json
{
  "version": "agentflow.cli/1",
  "command": "doctor",
  "result": { /* per-command payload */ },
  "errors": []
}
```

The four fields are **closed** — adding a fifth top-level field is a
breaking change that bumps `version` to `agentflow.cli/2`.

| Field | Type | Notes |
| --- | --- | --- |
| `version` | string | Wire schema discriminator. Always `agentflow.cli/N`. Producers and consumers MUST treat unknown versions as opaque. |
| `command` | string | Subcommand path the operator typed, space-separated (`"doctor"`, `"workflow validate"`, `"marketplace install"`). Lets multiplexed log captures be parsed without inspecting the payload. |
| `result` | object \| array \| null | Per-command payload. Each command documents its own `result` schema and tracks it under its own stability tier. May be `null` for metadata-only outputs. |
| `errors` | array of strings | Zero-or-more single-line user-actionable error messages. **Never `null`** — successful runs emit `[]`. Structured error codes belong inside `result` when the per-command schema needs them. |

## Producer contract

```rust
use agentflow_cli::json_envelope::CliJsonEnvelope;

// Success path: any Serialize payload, no errors.
let envelope = CliJsonEnvelope::ok("doctor", &report);
println!("{}", serde_json::to_string_pretty(&envelope)?);

// Partial success with operator-visible warnings/errors:
let envelope = CliJsonEnvelope::with_errors(
  "workflow validate",
  &report,
  vec!["missing parameter 'foo'".to_string()],
);
```

A command that surfaces only errors (no usable `result`) still emits
the full envelope — `result` is set to the per-command "empty" shape
(e.g. an empty list, or a sentinel `{"failed": true}` block).
Returning `null` is allowed for commands where the absence of data
is meaningful.

## Consumer contract

- **Parse against `version` first.** Reject any envelope whose
  `version` you don't know.
- **`errors` is always present.** Treat an empty `errors` array as a
  successful run; the process exit code carries the same signal but
  consumers parsing piped JSON shouldn't have to track exit codes
  separately.
- **Tolerate additive `result` fields.** Per-command payloads follow
  the P0.3 additive-field contract: every consumer must ignore
  unknown keys inside `result` so future additions don't break old
  parsers.
- **Don't rely on key order.** Serialization order is not part of the
  contract — sort keys before comparing.

## Coverage matrix

| Command | Bare JSON | Envelope | Notes |
| --- | --- | --- | --- |
| `agentflow doctor` | `--format json` | `--format json-envelope` | First migration; envelope wraps `DoctorReport`. The bare-JSON form is preserved for the in-process `/v1/diagnostics` handler. |
| `agentflow workflow validate` | `--format json` | n/a (planned) | Per-node permission report under `result`; envelope migration tracked as a P3.3 follow-up. |
| `agentflow workflow resume-plan` | `--format json` | n/a (planned) | `ResumePlan` payload. |
| `agentflow eval run` | `--format json` | n/a (planned) | `EvalReport` payload. |
| `agentflow harness run|list|inspect` | `--output json` / `stream-json` | n/a (planned) | Stream-JSON keeps emitting raw `HarnessEvent` lines; the envelope mode would wrap the trailing summary. |
| `agentflow llm models` | (text only today) | n/a (planned) | Add `--output json-envelope` alongside text. |
| `agentflow mcp list-tools \| list-resources \| call-tool` | partial | n/a (planned) | The `call-tool` result currently prints raw JSON; needs the envelope plus a `tool_call_id` field in `result`. |
| `agentflow plugin list \| install \| inspect` | text only | n/a (planned) | Auto-completion-friendly output needed. |
| `agentflow rag search \| eval` | partial | n/a (planned) | `rag eval` already emits a structured `EvalReport`; envelope migration adds the wrapping. |
| `agentflow trace list \| replay \| show` | text only | n/a (planned) | Auto-tooling consumers want JSON. |
| `agentflow workflow run \| list \| cancel \| graph \| logs` | text only | n/a (planned) | Server-backed; depends on `P2.5` `--server` plumbing. |

Each "planned" row lands as its own commit per the P3.3 follow-up
checklist in `TODOs.md`. The envelope itself is stable today.

## Versioning policy

- `agentflow.cli/1` is the current wire version.
- A version bump is required for: removing or renaming a top-level
  envelope field; changing the shape of `errors`; reinterpreting
  `command` semantics.
- A version bump is **not** required for: adding optional fields
  inside per-command `result`; adding a new command; adding a new
  `version` value to the consumer matrix.

## Related

- [`agentflow-cli/src/json_envelope.rs`](../agentflow-cli/src/json_envelope.rs) — reference implementation + round-trip tests.
- [`docs/STABILITY.md`](STABILITY.md) — stability tier registry; `CliJsonEnvelope` is listed in the Server / CLI table.
- [`docs/API_COMPATIBILITY.md`](API_COMPATIBILITY.md) — additive-field contract that per-command `result` schemas inherit.
