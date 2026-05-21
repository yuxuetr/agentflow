#!/usr/bin/env bash
# Local rehearsal of the release pipeline (P10.0.4).
#
# Walks the same two build legs that `.github/workflows/release.yml`
# runs in CI:
#   1. Build the `agentflow` CLI binary in release mode for the
#      *host* triple. (CI builds 4 targets; we only do the host here
#      — the goal is to catch feature-flag / dep-graph breakage, not
#      cross-compilation issues.)
#   2. `docker buildx build` the multi-arch `agentflow-server` image
#      WITHOUT pushing. Confirms the root `Dockerfile` still works on
#      both `linux/amd64` and `linux/arm64`.
#
# The script intentionally does NOT touch the GHCR registry or
# attempt to create a GitHub Release — those steps require auth and
# a real tag, and belong to the CI workflow firing on tag push.
#
# Usage:
#   scripts/release_dry_run/run.sh
#
# Honors `DOCTOR_SMOKE_RUNTIME` (defaults to Apple `container`, fall
# through to Docker) for the docker-buildx step; pass `docker` if
# Apple's `container` doesn't support multi-arch (it doesn't ship a
# `buildx` equivalent — the dry-run on macOS falls back to checking
# the single-arch build).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
RUNTIME="${DOCTOR_SMOKE_RUNTIME:-${RELEASE_DRY_RUN_RUNTIME:-container}}"

echo "[dry-run] repo: ${REPO_ROOT}"
echo "[dry-run] container runtime: ${RUNTIME}"

# ── 1. CLI release binary on the host platform ──────────────────────
echo
echo "[dry-run] step 1: cargo build --release -p agentflow-cli --bin agentflow"
cd "${REPO_ROOT}"
cargo build --release -p agentflow-cli --bin agentflow
# Use `cargo metadata` to discover the real `target_directory` —
# `~/.cargo/config.toml` may redirect target out of the workspace
# (the AgentFlow dev convention does, per `CLAUDE.md`'s Cargo
# Configuration section).
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' \
  2>/dev/null || echo "${REPO_ROOT}/target")"
BIN_PATH="${TARGET_DIR}/release/agentflow"
if [[ ! -x "${BIN_PATH}" ]]; then
  echo "[dry-run] FAIL: expected ${BIN_PATH} after build" >&2
  exit 1
fi
SIZE="$(du -h "${BIN_PATH}" | awk '{print $1}')"
echo "[dry-run] ✓ ${BIN_PATH} (${SIZE})"

# Smoke the binary with a no-side-effect command. `agentflow --version`
# matches the canonical fresh-user invocation.
"${BIN_PATH}" --version
echo "[dry-run] ✓ --version exit ok"

# ── 2. Docker buildx of the server image ────────────────────────────
#
# `container build` doesn't have a multi-arch buildx equivalent, so
# the path depends on the runtime:
#   - `container`: single-arch local build (host arch only).
#   - `docker`:    full multi-arch buildx invocation matching the
#                  CI workflow, without `--push`.
echo
case "${RUNTIME}" in
  docker)
    if ! docker buildx version >/dev/null 2>&1; then
      echo "[dry-run] WARN: docker found but `buildx` not installed; skipping multi-arch step." >&2
      echo "[dry-run] WARN: install with: docker buildx install" >&2
      exit 0
    fi
    echo "[dry-run] step 2: docker buildx (multi-arch, no push)"
    docker buildx build \
      --platform linux/amd64,linux/arm64 \
      --file "${REPO_ROOT}/Dockerfile" \
      --tag agentflow-server:dry-run \
      --output type=cacheonly \
      "${REPO_ROOT}"
    echo "[dry-run] ✓ multi-arch image build (cached only, no push, no load)"
    ;;
  container | *)
    echo "[dry-run] step 2: ${RUNTIME} build (single-arch local — multi-arch needs docker buildx)"
    "${RUNTIME}" build \
      --tag agentflow-server:dry-run \
      --file "${REPO_ROOT}/Dockerfile" \
      "${REPO_ROOT}"
    echo "[dry-run] ✓ single-arch image build (${RUNTIME})"
    echo "[dry-run] NOTE: Apple \`container\` doesn't expose \`buildx\` — multi-arch validation only runs in CI."
    echo "[dry-run] NOTE: set DOCTOR_SMOKE_RUNTIME=docker to invoke the full multi-arch buildx leg locally."
    ;;
esac

echo
echo "[dry-run] release pipeline dry-run completed cleanly."
echo "[dry-run] Real release: push a v*.*.* tag and the .github/workflows/release.yml workflow takes over."
