# UI E2E Suite (Playwright)

The specs in this directory exercise `agentflow-ui` SPA flows
against a real, running `agentflow-server` instance. They cover
form submission, redirect routing, and localStorage persistence —
behaviours that don't have unit-test coverage in either the React
source or the server's Rust integration tests.

## Status (P10.17.4)

- **Not PR-gated.** The pattern matches `llm-live.yml` — manual
  `workflow_dispatch` + nightly schedule. See
  `.github/workflows/ui-e2e.yml` for the CI surface.
- **Local-runnable.** `npm run e2e` against a dev `agentflow serve`.

## Local run

### One-time setup

```bash
cd agentflow-ui
npm install                      # picks up @playwright/test
npm run e2e:install              # installs Chromium + system deps
```

### Each run

1. Start `agentflow-server` in another terminal:

   ```bash
   # From the workspace root:
   DATABASE_URL=postgres://agentflow:agentflow@localhost:5432/agentflow \
   AGENTFLOW_SECURITY_PROFILE=local \
     cargo run -p agentflow-server
   ```

   The Postgres host can be local (Docker / Homebrew) or remote
   — anything reachable from `DATABASE_URL`. The server's first
   start runs `sqlx::migrate!()` automatically.

2. (Optional) rebuild the UI bundle if you've touched
   `agentflow-ui/src/`. The server embeds `dist/assets/` via
   `include_str!`, so an outdated bundle gives you a stale UI:

   ```bash
   cd agentflow-ui && npm run build
   ```

   …then restart `agentflow-server` to pick up the new bundle.

3. Run the suite:

   ```bash
   cd agentflow-ui
   npm run e2e                   # all specs
   npm run e2e -- runs-new       # filter to one spec
   npm run e2e -- -g "persists"  # filter by test name
   ```

### Environment knobs

| Var | Default | Purpose |
| --- | --- | --- |
| `BASE_URL` | `http://127.0.0.1:8080` | Gateway URL the specs hit. |
| `AGENTFLOW_API_TOKEN` | `(unset)` | Filled into the token field when the deployment uses `production` profile. The `local` default doesn't need it. |
| `CI` | `(unset)` | When set, Playwright uses serial workers + retry-on-flake + writes a JUnit XML + HTML report. |

## CI run

The `.github/workflows/ui-e2e.yml` job:

- Runs on `workflow_dispatch` (manual) and nightly `schedule` at
  10:30 UTC.
- **Not** in `quality.yml::release-gate.needs` — failures don't
  block PRs. Catches regressions between releases, not on every
  commit.
- Provisions a Postgres 16 service container, builds
  `agentflow-server` (release), boots it in the background,
  installs Playwright + Chromium, runs the specs, and uploads
  the `playwright-report/` HTML report as an artifact on
  failure.
- Manual `workflow_dispatch` accepts an optional `spec_filter`
  input that maps to `npx playwright test -g <pattern>`.

### Reading the artifact

When a nightly run fails, the report is attached to the run
under "Artifacts → `playwright-report-<run_id>`". Download +
unzip, open `playwright-report/index.html`. Traces + screenshots
are embedded for failed tests; the first-retry trace shows
exactly where the assertion broke against the real DOM.

## Adding a spec

1. Drop a `*.spec.ts` file in this directory.
2. Use `data-testid` selectors (the SPA wires them on every form
   field that matters; grep `agentflow-ui/src/main.tsx` for
   `data-testid=` to find the inventory).
3. Don't hard-code timing — use Playwright's
   `expect(locator).toBeVisible()` / `page.waitForURL()` so the
   spec converges on the right state instead of guessing
   milliseconds.
4. If the spec needs a fresh tenant scope, generate one inline
   (`const tenant = \`e2e-${crypto.randomUUID()}\``) — every
   spec runs against the same database, so cross-spec
   contamination is on you if you reuse names.

## Why not PR-gated

(Captured here so the question doesn't keep coming back.)

- **Build cost.** Playwright + browser install + server boot
  adds ~3-5 min to every PR. The current two specs don't catch
  enough to justify that on every commit.
- **Flakiness exposure.** E2E timing / animation issues can
  block legitimate PRs on infra problems. Manual + nightly gives
  the same regression signal without the false-positive tax.
- **Coverage tier.** The specs are sanity-level smoke
  (form-submission round-trip, localStorage persistence). Deeper
  coverage lives in unit tests; PRs already gate on those.

If a PR-gated layer becomes worth the cost (e.g., a real
regression slips through nightly to a release), the workflow can
be promoted to `quality.yml::release-gate.needs` with a single
edit; no code change required.
