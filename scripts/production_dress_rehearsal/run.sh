#!/usr/bin/env bash
# Production deployment dress rehearsal — host-side driver (P10.0.1).
#
# Reuses the `agentflow-doctor-smoke` image built by
# `scripts/doctor_smoke/run.sh` (or builds it on the fly if missing),
# mounts the in-container rehearsal script, and captures the full
# transcript + JSON summary into this directory.
#
# Output:
#   last-run.log   — the step-by-step transcript with "✓ / ✗" marks
#   last-run.json  — structured per-step outcome summary
#
# Re-runs are idempotent: both files get overwritten. The checked-in
# fixtures alongside this script document the canonical "passing"
# outcome on a clean Apple-container Ubuntu 24.04 box.

set -euo pipefail

RUNTIME="${PROD_REHEARSAL_RUNTIME:-${DOCTOR_SMOKE_RUNTIME:-container}}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
IMAGE_TAG="agentflow-dress-rehearsal"
CONTAINERFILE="${SCRIPT_DIR}/Containerfile"
LOG_OUT="${SCRIPT_DIR}/last-run.log"
JSON_OUT="${SCRIPT_DIR}/last-run.json"
INSIDE_SCRIPT="${SCRIPT_DIR}/inside_container.sh"

if ! command -v "${RUNTIME}" >/dev/null 2>&1; then
  echo "error: '${RUNTIME}' CLI not on PATH. Set PROD_REHEARSAL_RUNTIME or DOCTOR_SMOKE_RUNTIME, or install."
  exit 2
fi

# Build the rehearsal image if it isn't cached. The image bundles both
# `agentflow` and `agentflow-server` (the latter needed by acceptance
# gate 2's `serve --check` flow).
if ! "${RUNTIME}" image list 2>/dev/null | grep -q "^${IMAGE_TAG}\\b"; then
  echo "[rehearsal] ${IMAGE_TAG} not cached; building via ${CONTAINERFILE} (~12-18 min first run; both binaries)"
  "${RUNTIME}" build \
    --tag "${IMAGE_TAG}" \
    --file "${CONTAINERFILE}" \
    "${REPO_ROOT}"
else
  echo "[rehearsal] using cached ${IMAGE_TAG}"
fi

echo "[rehearsal] launching dress-rehearsal container; capturing log to ${LOG_OUT}"

# Apple `container` doesn't support `--volume` for files (only dirs),
# so we pass the script body via stdin rather than mounting it.
# `bash -s` reads the script from stdin; the trailing arguments after
# `--` are forwarded to the script's `$@`. Pre-empties the log/json
# files so failed runs don't leave stale data around.
: > "${LOG_OUT}"
: > "${JSON_OUT}"

# Run the rehearsal. We capture stdout+stderr together since the
# in-container script `tee`s its own structured output into files
# inside the container; we just need the surface log here.
set +e
"${RUNTIME}" run --rm -i \
  --entrypoint bash \
  "${IMAGE_TAG}" \
  -s <"${INSIDE_SCRIPT}" >"${LOG_OUT}" 2>&1
REHEARSAL_EXIT=$?
set -e

echo "[rehearsal] in-container exit: ${REHEARSAL_EXIT}"
echo "[rehearsal] log written to ${LOG_OUT}"

# Extract the summary JSON from the log. The inside script writes a
# `{ ... }` block bracketed by the `Summary` heading — pull it out
# with awk so the JSON file is self-contained.
awk '
  /^\{$/    { in_json = 1 }
  in_json   { print }
  /^\}$/    { in_json = 0 }
' "${LOG_OUT}" > "${JSON_OUT}"

if command -v jq >/dev/null 2>&1; then
  echo "[rehearsal] step matrix:"
  jq -r '.steps[] | "  \(.outcome | ascii_upcase)\t\(.name)\t\(.detail)"' "${JSON_OUT}" \
    | column -t -s $'\t' || true
  echo "[rehearsal] doctor exit code: $(jq -r '.doctor_exit_code' "${JSON_OUT}")"
else
  echo "[rehearsal] install jq for a structured summary; raw JSON in ${JSON_OUT}"
fi

if [[ ${REHEARSAL_EXIT} -gt 1 ]]; then
  echo "[rehearsal] FATAL: in-container exit ${REHEARSAL_EXIT} (>1 — script crashed)" >&2
  exit ${REHEARSAL_EXIT}
fi

# `exit 1` from the in-container script means at least one step
# recorded "fail" — surface but don't double-report (the operator
# reads the matrix).
exit ${REHEARSAL_EXIT}
