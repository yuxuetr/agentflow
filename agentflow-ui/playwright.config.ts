// P10.17.4: Playwright config for the agentflow-ui e2e suite.
//
// The specs in `e2e/` run against an externally-managed
// `agentflow serve` instance (URL via $BASE_URL, default
// http://127.0.0.1:8080). CI spins the server up in a background
// job step; local devs typically run `cargo run -p agentflow-server`
// in another terminal. We deliberately do NOT use Playwright's
// `webServer` option to start the binary because:
//
//   1. agentflow-server needs a Postgres backend; bootstrapping it
//      from inside Playwright would couple test infrastructure to
//      a Rust toolchain.
//   2. Reusing an existing running server matches the "smoke" use
//      case — we're checking that a real deployed UI works, not
//      that the server can be spawned.
//
// Tests are Chromium-only by default for CI speed. Cross-browser
// is a separate concern; the SPA bundle is plain ES2022 + React 19
// so Firefox / WebKit divergence is unlikely to be the bug.

// @ts-ignore — @playwright/test is in devDependencies but
// resolution from ts-check passes only after `npm install`.
import { defineConfig, devices } from '@playwright/test';

// The bare `process` global needs node types which we deliberately
// don't pull in (keeps the typecheck surface narrow). The cast
// matches the pattern used in eventFilter.test.ts +
// preferences.test.ts; same shape, same justification.
const env = (globalThis as unknown as { process: { env: Record<string, string | undefined> } })
  .process.env;
const baseURL = env.BASE_URL ?? 'http://127.0.0.1:8080';
const ci = !!env.CI;

export default defineConfig({
  testDir: './e2e',
  // Each test gets a single retry in CI to absorb cold-start
  // flakiness (the first hit to the server can be slow if it just
  // booted). Local devs see immediate failures.
  retries: ci ? 1 : 0,
  // CI runs serially so a flaky test doesn't blast a database
  // mid-transaction in another worker; locally Playwright
  // auto-detects a sensible parallelism level.
  workers: ci ? 1 : undefined,
  // Per-test timeout. 30s is enough for the UI submit + redirect
  // round-trip against a freshly-booted server.
  timeout: 30_000,
  expect: {
    // Wait up to 10s for assertions to converge (the UI uses
    // useEffect-based state updates that take a tick).
    timeout: 10_000,
  },
  reporter: ci
    ? [
        ['list'],
        // The JUnit XML lets GitHub Actions parse test names on
        // failure; the HTML report is uploaded as an artifact.
        ['junit', { outputFile: 'playwright-results.xml' }],
        ['html', { outputFolder: 'playwright-report', open: 'never' }],
      ]
    : 'list',
  use: {
    baseURL,
    // Captured on first retry only — keeping the trace store
    // small for nightly runs. `on-first-retry` writes traces only
    // when a test is being retried (i.e. the initial run failed),
    // which is the only case we actually need to debug.
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
