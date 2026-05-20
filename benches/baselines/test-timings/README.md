# Test-timing baselines (P10.19.2)

Per-host snapshots of `cargo test -p <crate> --all-targets` total
wall-clock for every workspace member (except `xtask` itself). These
are consumed by `cargo xtask test-gate`, the test-suite-bloat sibling
to the existing `bench-gate` (criterion microbench gate).

The baselines deliberately live in a parallel directory to
`benches/baselines/<host>.json` (criterion data) — same `<host>.json`
naming convention, different schema. Don't merge the two files; the
gate code routes off file path, not file content.

## Why a test-timing gate?

Compile + execution wall-clock for `cargo test` is a leading indicator
of test-suite bloat:

- A 10 ms unit test feels free; sixty of them in one crate adds half
  a second every CI run.
- An "I'll just sleep 500 ms to dodge a race" patch shows up in the
  gate before it lands in a 30-minute pipeline.
- A new heavyweight integration test (real Postgres / real LLM) that
  bypasses the documented `self-skips without …_TEST_URL` pattern is
  the kind of regression `cargo test` itself doesn't surface — it
  passes, just slower.

The gate's threshold is **1.5×** by default (vs. bench-gate's 1.25×).
Test wall-clock is meaningfully noisier than criterion microbenches
because it captures both incremental compile and test execution, and
a single sample is what the gate gets (no warm-up / measurement-time
flags exist for `cargo test`).

## Naming

`<host-id>.json`, matching the criterion baselines convention. Today's
known ids:

- `apple-aarch64.json` — Apple Silicon laptops (M-series, macOS arm64).
- `linux-x86_64.json` — generic Linux x86_64 (CI runners + most
  workstations).

When a new host comes online, run `--update` on it to capture a fresh
file; don't overwrite another host's file with numbers from yours.

## Schema

```json
{
  "host": {
    "id": "apple-aarch64",
    "machine": "MacBookPro18,2",
    "arch": "aarch64",
    "os": "macos",
    "rustc": "rustc 1.85.0 (...)",
    "captured_at": "2026-05-21"
  },
  "notes": ["..."],
  "timings": {
    "agentflow-core": { "wall_clock_ns": 12345678900, "test_count": 139 },
    "agentflow-tools": { "wall_clock_ns": 3000000000, "test_count": 87 }
  }
}
```

`wall_clock_ns` is the total nanoseconds for one
`cargo test -p <crate> --all-targets --quiet` invocation. `test_count`
is best-effort, parsed from the `test result: ok. N passed; …`
summary lines (summed across lib + integration test binaries). `None`
when no summary line was present (compile error, harness disabled).

## Capture flow

```sh
# First-time capture (or refresh after a major dep bump):
cargo xtask test-gate --update

# Filter to a subset while iterating:
cargo xtask test-gate --update --include agentflow-core --include agentflow-tools

# Compare against the checked-in baseline (default mode):
cargo xtask test-gate

# Tighten / loosen the threshold:
cargo xtask test-gate --threshold 1.25

# Compare a pre-captured timing file (CI two-stage flow):
cargo xtask test-gate --baseline benches/baselines/test-timings/ci-ubuntu-latest.json \
                     --input /tmp/ci-current.json
```

`--update` writes to the host-specific baseline path that
`default_test_timing_baseline_path` resolves. Pass `--baseline <path>`
to override.

## Determinism notes

- `cargo test` wall-clock includes incremental compile. To minimise
  variance, run `cargo build --workspace --tests` once before
  `test-gate --update` so the build artifacts cache is hot. The
  comparison run should also be done from a hot cache.
- The gate is not wired into CI today. Land the baseline file first,
  give the workspace experience time to confirm the heuristic, then
  graduate it to `quality.yml` (see P10.19.2-FU1 if it's opened).
- Per-run variance of 1.2–1.4× is normal on noisy hosts (laptops on
  battery, CI runners under contention). The 1.5× threshold absorbs
  that without flagging false positives.

## Initial baseline files

This directory ships **without** initial timing baselines — running
`cargo test` across the full workspace takes 5–15 minutes on a hot
cache and the numbers vary per host. The first operator to land a
baseline should:

1. Run `cargo build --workspace --tests` to warm the cache.
2. Run `cargo xtask test-gate --update --baseline benches/baselines/test-timings/<host>.json`.
3. Commit the JSON, add a one-line note to this README under
   "Naming" above pinning the host id.

Without a baseline, plain `cargo xtask test-gate` (no `--update`)
fails fast with an actionable error pointing at `--update`.
