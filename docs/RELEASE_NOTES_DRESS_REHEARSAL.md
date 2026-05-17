# v1.0.0-rc.1 Release Dress Rehearsal (P7.4)

**Date**: 2026-05-17
**Profile under test**: production
**Host**: macOS (Apple Silicon)
**Operator**: AI-assisted (Claude Opus 4.7), supervised
**Linked TODO**: P7.4 in `TODOs.md`

This document captures findings from a dry run of the v1.0.0-rc.1
release flow described in `docs/RELEASE_CHECKLIST.md`. No git tag was
created — the rehearsal is a pre-flight pass surface for refilling
gaps as targeted tasks before the actual `v1.0.0-rc.1` cut.

---

## Summary

- **Overall status**: 🟢 ready for the `v1.0.0-rc.1` tag once the
  three cleanup-before-tag items in F2 / F3 / F4 are addressed (none
  block the runtime; all are operator-facing).
- **Release-blocking findings**: 1, **fixed in this rehearsal** — the
  Linux seccomp compilation gap (F1).
- **Cleanup-before-tag items**: 3 (pre-existing fmt / clippy drift +
  `doctor --profile production` advisory warnings on a developer
  machine).
- **Verified**:
  - Distributed worker stack (P2.8 / P5.5 / P5.6 / P5.7) — green, 23
    tests across the worker crate plus 9 admission + scheduler
    integration tests on the server.
  - `agentflow doctor --profile production --format json` runs and
    surfaces actionable warnings.
  - **Docker image build (post-fix)** — `cargo build --release`
    inside `rust:1-bookworm` finishes in ~83 s; final image exports
    cleanly via `docker buildx`.
  - **Docker stack boots end-to-end** — Postgres + agentflow-server
    reach `(healthy)`; `/health/live`, `/health/ready`, and
    `/ui` all return `200` with the expected payloads.
  - DB migrations apply automatically on first boot against a
    freshly-provisioned Postgres container.

---

## Findings

### F1 (RELEASE BLOCKER — FIXED IN REHEARSAL) — Linux seccomp filter compile errors

**Path**: `agentflow-tools/src/sandbox/linux.rs`

The Linux seccomp backend failed to compile under the docker
`rust:1-bookworm` builder image with two distinct rustc errors:

1. `error[E0308]: mismatched types` at `linux.rs:145` (and the parallel
   spot at line 140). The for-loops iterate `&'static [i64]` returned
   by `net_syscall_numbers()` / `fs_write_syscall_numbers()`, so the
   binding is `&i64`. `BTreeMap::<i64, _>::insert` expects an owned
   `i64`. Local macOS builds never noticed because the file is gated
   on `#[cfg(target_os = "linux")]`.
2. `error[E0271]: type mismatch resolving '<Vec<sock_filter> as TryFrom<SeccompFilter>>::Error == Error'`
   at `linux.rs:155`. `TryInto<BpfProgram>` returns `BackendError`,
   not the unified `seccompiler::Error`. The fix uses the upstream
   `From<BackendError> for Error` impl via
   `.map_err(seccompiler::Error::from)`.

**Action taken**: both fixes applied in this rehearsal. The docker
build now proceeds past `agentflow-tools` compilation. We should
backstop this regression in CI by adding `cargo check --target
x86_64-unknown-linux-gnu -p agentflow-tools` (or by extending the
existing Linux job to compile the Linux sandbox) — tracked as the
follow-up in section "Follow-up TODOs" below.

### F2 (CLEANUP BEFORE TAG) — `cargo fmt --all --check` failures pre-date this rehearsal

Running the release checklist command:

```
cargo fmt --all -- --check
```

surfaces diffs in benches across the workspace (e.g.
`agentflow-core/benches/scheduler.rs`,
`agentflow-tracing/benches/event_write.rs`). They are stylistic
re-flow changes from a newer `rustfmt` version, not behavior. None
of these files are touched by the worker / server changes that landed
during P2.8–P5.7.

**Action**: schedule a single `chore(fmt): apply rustfmt across
workspace` commit before tagging. Do **not** mix this with feature
work to keep the diff reviewable.

### F3 (CLEANUP BEFORE TAG) — `cargo clippy --workspace -- -D warnings` has 8 pre-existing warnings

All eight warnings are in `agentflow-server/src/scheduler/grpc.rs` and
flag `clippy::result_large_err` (the `Err` variant carries the full
176-byte `tonic::Status`). The fix is mechanical (box the status), but
mass-boxing touches every handler in the file and is unrelated to the
worker hardening this rehearsal validates.

**Action**: schedule a focused `refactor(server): box tonic::Status
for clippy::result_large_err` PR before tagging. Acceptable to defer
to v1.0.0-rc.2 if the rest of the rc.1 cut is otherwise clean.

### F4 (ADVISORY) — `agentflow doctor --profile production` exits with `status: warning`

On a developer machine (this host), the JSON output reports:

```json
"disk": {
  "trace_dir": { "exists": false, "error": "directory does not exist; will be created on first write" },
  "marketplace_cache": { "exists": false, "error": "directory does not exist; will be created on first write" }
},
"status": "warning"
```

Both are auto-created on first write, but the production profile flags
them because the operator hasn't pre-provisioned them. Same applies to
`auth token required: no` — the production profile expects
`AGENTFLOW_API_TOKEN` to be set in the environment.

**Action**: the operator runbook for `v1.0.0-rc.1` deployment should
spell out the pre-provisioning checklist for a fresh production host:

- `mkdir -p $AGENTFLOW_RUN_DIR $AGENTFLOW_TRACE_DIR $AGENTFLOW_MARKETPLACE_CACHE`
- Set `AGENTFLOW_API_TOKEN` via secret manager / env file.
- Set `AGENTFLOW_SECURITY_PROFILE=production`.

This is *not* a code change; it's a runbook gap to close in the
release notes for rc.1.

### F5 (VERIFIED) — Distributed worker stack closure

All four Phase C tasks (P2.8, P5.5, P5.6, P5.7) closed during this
rehearsal cycle:

- `agentflow-worker` test count: 23 (8 in-lib + 6 failure-domain + 4
  resource-limit + 3 dispatch-simple + 2 dispatch-llm-and-agent).
- `agentflow-server` admission test count: 6 policy units + 3
  integration scenarios.
- Docs: `docs/DISTRIBUTED.md` carries the supported-node-type table,
  Worker Admission knob table, Worker Resource Limits table, and the
  Failure Domains matrix.
- Stability: distributed worker control-plane row in
  `docs/STABILITY.md` is **Experimental** with the
  pin-the-worker-minor-version warning.

---

## Docker image rehearsal

### Build

```bash
PATH="/Applications/Docker.app/Contents/Resources/bin:$PATH" \
  docker buildx build --progress=plain \
  --build-arg PACKAGE=agentflow-server \
  --build-arg BIN=agentflow-server \
  -t agentflow:rehearsal .
```

**Status (after F1 fix applied)**: ✅ PASS. `cargo build --release`
inside `rust:1-bookworm` completes in ~83 s on this host (`#14 DONE
83.2s` for the builder stage), and the final multi-arch image manifest
(`docker.io/library/agentflow:rehearsal`) is exported cleanly via
`docker buildx`. Compose pulls it under the name
`agentflow-agentflow-server:latest`.

### Compose stack

The repo ships a `docker-compose.yml` that fronts a Postgres + the
agentflow-server image. Operator should:

```bash
docker compose up -d
docker compose ps
docker compose logs agentflow-server | head -50
curl -fsS http://localhost:3000/health/live
open http://localhost:3000/ui
```

### Findings from boot pass

- `docker compose ps`: both `postgres` and `agentflow-server`
  containers reach `(healthy)` (compose healthcheck on
  `GET /health/live` succeeds within the configured 10s × 12-retry
  envelope).
- `GET /health/live` → `200 {"status":"ok","service":"agentflow-server"}`
- `GET /health/ready` → `200 {"status":"ok","service":"agentflow-server"}`
- `GET /ui` → `200 text/html`. SPA shell ships
  `<script type="module" crossorigin src="/ui/assets/app.js">` and
  the matching stylesheet link.
- `HEAD /ui/assets/app.js` → `200 application/javascript`, with the
  expected `cache-control: public, max-age=3600` from `ui.rs`.
- Server startup logs show: DB connect ok → migrations applied (10 from
  `agentflow-db/migrations/`) → `Using 'local' security profile` →
  listening on `0.0.0.0:3000`. The single `WARN` is the expected
  "`AGENTFLOW_API_TOKEN` is not set" advisory for rehearsals where
  the token isn't wired up.
- No crash / restart loops observed across the boot window.

---

## Follow-up TODOs (refile in `TODOs.md` before tagging)

- **`cargo check --target x86_64-unknown-linux-gnu -p agentflow-tools`
  must be part of the Quality CI matrix.** Today the Linux seccomp
  backend (`agentflow-tools/src/sandbox/linux.rs`) is only compiled on
  Linux CI, and the only Linux build path that *fails fast* on it is
  the docker image. Two regressions slipped past the macOS-only
  developer build (F1 above). Add a `linux-sandbox-check` job to
  `.github/workflows/quality.yml` that runs `cargo check --target
  x86_64-unknown-linux-gnu -p agentflow-tools` (using
  `dtolnay/rust-toolchain@stable` with `targets:
  x86_64-unknown-linux-gnu`).
- **Single `chore(fmt): workspace rustfmt sweep` commit** before
  tagging — see F2.
- **`refactor(server): box tonic::Status for clippy::result_large_err`**
  — see F3. Can land in rc.2 if rc.1 is otherwise green.
- **Runbook section in release notes** spelling out the
  pre-provisioning steps for the production deployment — see F4.

---

## What this rehearsal did NOT cover (deferred to a real rc.1 cut)

- Tagging `v1.0.0-rc.1` from a release branch. We're rehearsing
  against `main` — the tag itself is a deliberate one-way decision
  the human operator should make after F1–F4 are resolved.
- Publishing to crates.io. The rehearsal does not run `cargo publish
  --dry-run` for each publishable crate; that's part of the actual
  cut.
- GitHub Release artifacts (docker image push, source tarball signing).
  Out of scope for the pre-flight rehearsal.
- A *fresh* machine `doctor` smoke. The rehearsal ran on a developer
  host with pre-existing `~/.agentflow/` content. A clean macOS
  / Linux VM pass is recommended before the actual rc.1 tag.
