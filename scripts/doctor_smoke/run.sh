#!/usr/bin/env bash
# Fresh-VM `agentflow doctor` smoke (P10.0.5).
#
# Builds the multi-stage image declared in `Containerfile` (rust builder
# → fresh ubuntu:24.04 with the binary) and runs the canonical doctor
# invocation. Captures exit code + JSON report to
# `scripts/doctor_smoke/last-run.json` and prints the verdict.
#
# The script is intentionally idempotent: re-running picks up
# incremental build cache, and re-overwrites `last-run.json`. The
# checked-in fixture under the same directory is the canonical
# "expected output on a fresh Ubuntu 24.04 with no AgentFlow state"
# operators consult during release prep — see `README.md`.
#
# Usage:
#   scripts/doctor_smoke/run.sh
#
# Container runtime: defaults to Apple's `container` CLI (macOS); set
# `DOCTOR_SMOKE_RUNTIME=docker` to use Docker instead (the CLIs are
# argument-compatible for the build + run subset this script needs).

set -euo pipefail

RUNTIME="${DOCTOR_SMOKE_RUNTIME:-container}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
IMAGE_TAG="agentflow-doctor-smoke"
OUTPUT_PATH="${SCRIPT_DIR}/last-run.json"

if ! command -v "${RUNTIME}" >/dev/null 2>&1; then
  echo "error: '${RUNTIME}' CLI not on PATH. Set DOCTOR_SMOKE_RUNTIME or install."
  exit 2
fi

echo "[smoke] building ${IMAGE_TAG} via ${RUNTIME} (this may take 10-15 min on first run; subsequent runs hit cache)"
"${RUNTIME}" build \
  --tag "${IMAGE_TAG}" \
  --file "${SCRIPT_DIR}/Containerfile" \
  "${REPO_ROOT}"

echo "[smoke] running fresh-VM doctor; capturing JSON to ${OUTPUT_PATH}"
# Capture stdout to file, preserve exit code via `set +e` window.
set +e
"${RUNTIME}" run --rm "${IMAGE_TAG}" > "${OUTPUT_PATH}"
EXIT_CODE=$?
set -e

echo "[smoke] doctor exited with code ${EXIT_CODE}"
echo "[smoke] verdict:"
if command -v jq >/dev/null 2>&1; then
  jq -r '
    "  status:  \(.status // "<missing>")",
    "  profile: \(.profile // "<missing>")",
    "  warnings: \(.warnings // [] | length)",
    "  errors:   \(.errors // [] | length)"
  ' "${OUTPUT_PATH}"
else
  echo "  (install jq for a structured summary; raw JSON in ${OUTPUT_PATH})"
fi

# Exit-code reading guide:
#   0 — doctor reports `ok`. Either everything's configured, or the
#       fresh-VM defaults are tolerated by the active profile.
#   1 — `warning`. Recoverable issues (missing optional dirs in
#       `local`/`dev` profile, advisory env vars, etc.).
#   2 — `fail`. On `--profile production` against a fresh VM with no
#       `~/.agentflow/*` dirs, this is the **documented expected**
#       outcome: every directory check (`runs`, `traces`,
#       `marketplace/cache`, `skills`, `plugins`) reports
#       `exists: false`, which production-profile promotes to fail.
#       See `README.md::Expected outcomes` for the full table.
#   other — the binary itself failed to *run* (crashed, panicked, OOM).
#           That's a regression in the binary itself, not in the
#           doctor's profile semantics. Surface it.
#
# This script reports the verdict but does NOT translate exit 1/2 into
# its own non-zero — operators interpret the JSON. Only exits non-zero
# when the binary crashed (exit > 2 or < 0).
if [[ ${EXIT_CODE} -gt 2 ]]; then
  echo "[smoke] WARNING: doctor exited ${EXIT_CODE} (>2 — binary likely crashed; see ${OUTPUT_PATH})" >&2
  exit ${EXIT_CODE}
fi
exit 0
