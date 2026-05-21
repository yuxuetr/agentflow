# Bench baselines (P7.1)

Per-host snapshots of Criterion median wall-clock for the bench suites
the regression gate watches:

| Crate              | Bench               | Run                                              |
| ------------------ | ------------------- | ------------------------------------------------ |
| `agentflow-core`   | `scheduler`         | `cargo bench -p agentflow-core --bench scheduler`        |
| `agentflow-core`   | `hot_paths`         | `cargo bench -p agentflow-core --bench hot_paths` (P10.1.1) |
| `agentflow-llm`    | `provider_hop`      | `cargo bench -p agentflow-llm --bench provider_hop`      |
| `agentflow-rag`    | `retrieval`         | `cargo bench -p agentflow-rag --bench retrieval`         |
| `agentflow-tracing`| `event_write`       | `cargo bench -p agentflow-tracing --bench event_write`   |
| `agentflow-nodes`  | `node_latency`      | `cargo bench -p agentflow-nodes --bench node_latency --features conditional` (P10.2.1) |

## Naming

`<host-id>.json`. The host id should be stable enough to recognise
across re-captures (e.g. `apple-m2-max`, `linux-ci-x86_64`,
`linux-zen3-32c`). Add a new file rather than overwriting another
host's baseline — host differences are expected, and the CI gate
(P7.2) compares against the runner's own baseline.

## Schema

```json
{
  "host": { "id": "...", "machine": "...", "arch": "...", "os": "...", "rustc": "...", "captured_at": "YYYY-MM-DD" },
  "notes": ["..."],
  "benchmarks": {
    "<crate>/<bench>": {
      "<criterion-id>": { "median_ns": <int>, "throughput_elem_per_s": <int> }
    }
  }
}
```

`median_ns` is the criterion median in nanoseconds. `throughput_elem_per_s`
mirrors the `thrpt` line criterion prints — it is computed from
`Throughput::Elements(...)` and varies with the bench shape, so don't
divide one by the other.

## Capture flow

The numbers checked in alongside this README were captured with:

```sh
cargo bench -p <crate> --bench <name> -- \
  --warm-up-time 1 --measurement-time 3 --sample-size 20
```

For a full release re-capture, drop those flags and let Criterion run
the defaults — but expect each crate to take 1–2 minutes per shape.

## Caveats

- `bm25_index/build_corpus/10000` is dominated by the O(n²)
  `recompute_statistics` pass that `BM25Retriever::add_document` does
  on every insert. The bench surfaces this so any indexing speed-up
  shows up immediately; the search-side numbers are not affected.
- `agentflow-core::flow` currently prints a `▶️  Executing node 'xxx'`
  line per node from `flow.rs`. The bench numbers include the cost of
  those prints — a future PR that silences debug prints behind a flag
  should be expected to improve every `flow_*` baseline.
