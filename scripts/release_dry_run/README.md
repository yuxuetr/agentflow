# Release pipeline dry-run (P10.0.4)

Local rehearsal of the two build legs in
[`.github/workflows/release.yml`](../../.github/workflows/release.yml):
release-mode CLI binary build + `docker buildx` of the
`agentflow-server` image (without pushing). Catches Dockerfile /
feature-flag / dep-graph regressions before the real `v*` tag cut
fires the workflow.

## Usage

```sh
scripts/release_dry_run/run.sh
```

Set `DOCTOR_SMOKE_RUNTIME=docker` (or `RELEASE_DRY_RUN_RUNTIME=docker`)
to invoke the full `docker buildx` multi-arch leg locally. Apple's
`container` CLI doesn't ship a `buildx` equivalent, so the default
fall-through path checks the single-arch (host) build only.

## What's actually validated

| Leg | What it catches | Local cost |
|-----|-----------------|------------|
| `cargo build --release -p agentflow-cli --bin agentflow` (host triple) | feature-flag regressions, dep-graph breakage, default-feature compile errors | ~3-5 min cold, ~30 s warm |
| `docker buildx build linux/amd64,linux/arm64 --output cacheonly` (Docker runtime) | Dockerfile drift, apt-get base bitrot, multi-arch QEMU compat | ~10-15 min cold, ~2 min warm |
| Apple `container build` single-arch | smoke check that the Dockerfile still builds at all; multi-arch validation defers to CI | ~5-8 min cold, ~30 s warm |

## What this script does NOT do

- Push the image anywhere. No GHCR auth needed.
- Create a GitHub Release. No PAT / contents:write needed.
- Build the per-target tarballs in the CI matrix
  (`x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, etc.). Cross-
  compilation from macOS to Linux ARM64 is non-trivial without a
  cross-compile toolchain; CI handles it via the right per-target
  runner.

The full multi-arch matrix is only meaningfully exercised on CI.
This dry-run is the host-side "the basic shapes still work" check;
the operator pairs it with a `gh workflow run release.yml -f
dry_run=true -f ref=<branch>` to validate the full matrix without
publishing.

## When to run

- Before tagging a release (paired with `scripts/doctor_smoke/`
  and `scripts/production_dress_rehearsal/`).
- After any PR that touches:
  - `Dockerfile` at repo root.
  - `agentflow-cli/Cargo.toml` (default features).
  - The release workflow itself.
- After a Rust toolchain bump.

This script is not wired into `quality.yml` — same rationale as the
two other release-eng scripts: manual pre-release step, the build
cost is wrong fit for per-PR gating.
