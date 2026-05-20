# Checkpoint Schema

AgentFlow persists DAG run state to disk so a partially-completed
run can resume after a crash, an operator-triggered cancel, or a
distributed-worker handoff. This document describes the on-disk
shape, the read/write rules, and the legacy-compatibility behaviour
that the runtime guarantees.

The Rust implementation lives in `agentflow-core/src/checkpoint.rs`
(write path) and `agentflow-core/src/flow.rs::decode_checkpoint_flow_value`
(read path). The companion fixture suite is
`agentflow-core/tests/fixtures/checkpoints/`.

## Stability tier

- **Persisted `FlowValue` payloads**: Stable (see `docs/STABILITY.md`).
- **Surrounding checkpoint metadata** (file layout, surrounding
  envelope): Beta. New fields are additive; existing required
  fields don't change shape.

## Per-node output value encoding

Every checkpoint stores each node's outputs as a JSON object keyed
by output name. The value half is a [`FlowValue`] serialized with
the **tagged** schema:

```json
{ "type": "json", "value": { "answer": 42 } }
{ "type": "file", "path": "/tmp/out.png", "mime_type": "image/png" }
{ "type": "url", "url": "https://example.test/a.png", "mime_type": "image/png" }
```

Tag values are closed: `"json"` / `"file"` / `"url"` are the
three variants of `FlowValue`. Adding a new variant is a
schema-versioned migration.

### Reader contract: `decode_checkpoint_flow_value`

The reader handles three categorical inputs. The distinction
between "tagged-but-corrupt" and "genuinely untagged legacy"
matters because the two need different operator-visible feedback.

| Input shape | Behaviour | Operator-visible signal |
| --- | --- | --- |
| Object with a recognised `"type"` tag (`json`/`file`/`url`) that **deserialises cleanly** | Returns the matching `FlowValue` variant. | None — happy path. |
| Object with a recognised `"type"` tag that **fails to deserialise** (corrupt payload, missing `path` for `file`, etc.) | Falls back to `FlowValue::Json(original)` so resume can proceed, **and warns loudly via `eprintln!`** naming the node id, output key, the attempted tag, and the deserialiser error. | `⚠️  Warning: …` line on stderr. |
| Object **without** a recognised tag, or **non-object** value (number / string / array / null / object whose `type` is unfamiliar) | Treated as a legacy raw-JSON checkpoint; wrapped silently as `FlowValue::Json(original)`. | None — silent. |

### Why the asymmetry

A tagged-but-corrupt value is a **regression signal**: a writer
emitted the tagged form but the bytes on disk don't round-trip,
either because the writer was buggy, the file was truncated, or
the schema changed without a migration. The operator needs to
know — silently downgrading to `Json` would let downstream
consumers that pattern-match on `File` / `Url` quietly produce
wrong outputs. The warning surfaces the partial loss so the
regression is debuggable.

A genuinely-untagged value is the **pre-0.2 legacy shape**:
checkpoints written before the tagged schema landed were stored
as raw JSON, and the reader has always wrapped them in
`FlowValue::Json` without complaint. Warning here would spam
operators every time they resume a long-lived workflow whose
on-disk format hasn't been rewritten. That's why this branch
stays silent.

The asymmetry is pinned by tests:

- `agentflow-core/tests/flow_value_checkpoint_compat.rs::legacy_raw_json_checkpoint_values_read_as_json_flow_values`
  proves the silent fallback for legacy data.
- `agentflow-core/src/flow.rs::tests::malformed_tagged_checkpoint_value_falls_back_to_json`
  proves the loud-warning fallback for corrupt-tagged data.
- `agentflow-core/src/flow.rs::tests::legacy_untagged_checkpoint_values_decode_as_json`
  pins the silent path from inside the same module.

If you're auditing a regression where a node's `File` / `Url`
output came through as `Json` downstream, grep your stderr capture
for `tagged ... but failed to deserialize` first — that's the
diagnostic surface this design committed to.

## Writer contract

Writers MUST emit the tagged schema for every `FlowValue` they
persist. The unit tests in `agentflow-core/tests/fixtures/checkpoints/`
include golden snapshots that pin the wire shape; running them
against a writer change catches accidental untagged regressions.

Per the Stability table in `docs/STABILITY.md`, this contract is
Stable — bumping the encoding form is a v2-level migration.

## Related

- `docs/STABILITY.md` § Workflow and Checkpoint Schemas
- `agentflow-core/src/checkpoint.rs` — `CheckpointManager` (write path)
- `agentflow-core/src/flow.rs::decode_checkpoint_flow_value` (read path)
- `agentflow-core/tests/fixtures/checkpoints/` — golden fixtures
- `agentflow-core/tests/flow_value_checkpoint_compat.rs` — legacy
  read compat tests
