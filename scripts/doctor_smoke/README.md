# Fresh-VM `agentflow doctor` smoke (P10.0.5)

End-to-end reproduction of the
[`docs/RELEASE_NOTES_v1.0.0-rc.1.md`](../../docs/RELEASE_NOTES_v1.0.0-rc.1.md)
fresh-VM checklist step: provision a clean Ubuntu 24.04 environment
with zero `~/.agentflow/` state, build the `agentflow` binary, run
`agentflow doctor --profile production --backup-check --format json`,
record the exit code + JSON output.

Drives Apple's `container` CLI by default; pass
`DOCTOR_SMOKE_RUNTIME=docker` for Docker.

## Usage

```sh
# One command — builds the multi-stage image + runs the smoke + writes
# the JSON capture to last-run.json. ~10–15 min on first run; cache hits
# bring re-runs down to ~30 s.
scripts/doctor_smoke/run.sh
```

The script honours `DOCTOR_SMOKE_RUNTIME` (default `container`).

## Files

| Path                    | Purpose |
|-------------------------|---------|
| `Containerfile`         | Multi-stage: `rust:1-slim-bookworm` builder → `ubuntu:24.04` smoke image. Default CMD is the canonical doctor invocation. |
| `run.sh`                | Driver: build + run + capture exit code + summarise verdict via `jq`. |
| `last-run.json`         | **Checked in** — the canonical JSON the smoke produces against zero state. Reference for "what should fresh users see?" + regression check. |
| `README.md`             | This file. |

## Expected outcomes

The doctor's `status` field maps to an exit code (see
`agentflow-cli/src/commands/doctor.rs::DoctorStatus`):

| Doctor status | Exit | Fresh-VM expected? | Notes |
|---------------|-----:|--------------------|-------|
| `ok`          | 0    | No                 | Implies pre-seeded `~/.agentflow/` (operator already configured the machine). |
| `warning`     | 1    | Yes — on `--profile {dev,local}` | Missing dirs / optional env vars promote to warning, not fail, on non-production profiles. |
| `fail`        | 2    | **Yes — on `--profile production`** | This is the canonical first-run-on-Ubuntu signal: every default `~/.agentflow/*` dir is missing, and production-profile treats missing dirs as fail. |

`run.sh` only surfaces exit codes `>2` as its own non-zero — those
represent the binary itself crashing, not the doctor's profile
semantics. Exit 2 on a fresh VM means everything works as designed.

## What the checked-in `last-run.json` shows

Snapshot captured on `apple-aarch64` running Apple `container 0.12.3`
+ image `ubuntu:24.04.4` (Noble Numbat):

* `version`: `0.2.0` (the CLI's own version).
* `profile`: `production`.
* `config.models_config_source_kind`: `built_in_default` (no
  `~/.agentflow/default_models.yml` to override the bundled defaults).
* `config.missing_env_vars`: `[]` (none of the bundled default-models
  providers gates the env check — the registry tolerates missing keys
  per P10.3.1).
* `disk.{run_dir, trace_dir, marketplace_cache}`: all `exists: false`,
  `error: "directory does not exist; will be created on first write"`.
* `backup_check.{...,skills_dir,plugins_dir}`: same shape, expanded
  to cover the two extra dirs only `--backup-check` walks.
* `status`: `fail` (exit code 2).

If a future change drops a dir from the production-profile fail
list, or relaxes the "must exist" check, the checked-in fixture
will diff visibly and the operator updates the README's "Expected
outcomes" table.

## Re-capturing the fixture

After a deliberate behavioural change to `doctor` or its dependencies:

```sh
scripts/doctor_smoke/run.sh
# `last-run.json` is overwritten; eyeball the diff, update this
# README's "What the checked-in last-run.json shows" section if the
# shape changed, then `git add` it.
```

## When to run

* Before cutting a release (the
  `docs/RELEASE_NOTES_v1.0.0-rc.1.md` checklist names this as
  step 5).
* After any PR that touches `agentflow-cli/src/commands/doctor.rs`.
* After bumping the workspace's Rust toolchain pin (catches a
  cross-compile regression).

This script is **not** wired into `quality.yml` — the 10–15 min
build cost makes it the wrong fit for per-PR gating. It's a manual,
pre-release operator step.

## Why a checked-in fixture instead of asserting against it?

The fixture is documentation + a diff target, not a strict test
oracle. The `disk.run_dir.error` string includes a fixed message
today, but if we ever localise that string or include a timestamp
the test would false-positive without warning. Leaving it as a
fixture + README + a `jq` summary in the driver keeps the workflow
honest without locking in incidental formatting details.
