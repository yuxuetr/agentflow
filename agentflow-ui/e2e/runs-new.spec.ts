/**
 * Playwright E2E test for `/ui/runs/new` (`P6.1`).
 *
 * The test is intentionally lightweight: it spins up a Playwright
 * browser against an already-running `agentflow serve` instance,
 * fills in the form, and asserts the redirect lands on the run
 * console with the new run id.
 *
 * Running this suite requires three pieces of local infrastructure
 * that we do not pull in as workspace dependencies:
 *
 *   1. `@playwright/test` installed:
 *        cd agentflow-ui && npm install --save-dev @playwright/test
 *        npx playwright install chromium
 *
 *   2. A reachable `agentflow serve` with a real Postgres backend.
 *      Set `BASE_URL` to the gateway's public URL
 *      (defaults to http://127.0.0.1:8080) and `AGENTFLOW_API_TOKEN`
 *      when production-profile auth is enabled.
 *
 *   3. The `agentflow-ui/dist/` bundle the server embeds at build
 *      time must reflect the latest `src/main.tsx` — rebuild with
 *      `npm run build` after any UI change.
 *
 * Once installed, run with:
 *
 *   npx playwright test e2e/runs-new.spec.ts
 *
 * The test is structured so the import-failure-on-missing-dep
 * surfaces clearly rather than silently disabling the suite, so the
 * intent stays obvious in CI logs.
 */

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore — @playwright/test is intentionally an optional dev dep.
import { expect, test } from '@playwright/test';

const baseURL = process.env.BASE_URL ?? 'http://127.0.0.1:8080';
const apiToken = process.env.AGENTFLOW_API_TOKEN ?? '';

test.describe('P6.1 — /ui/runs/new', () => {
  test('submits a workflow and redirects to the run console', async ({ page }) => {
    await page.goto(`${baseURL}/ui/runs/new`);

    // Form fields are wired by data-testid so the assertions don't
    // rely on label copy churn.
    await page.getByTestId('create-tenant').fill('e2e');
    await page.getByTestId('create-profile').selectOption('local');
    if (apiToken) {
      await page.getByTestId('create-token').fill(apiToken);
    }
    await page.getByTestId('create-workflow').fill(
      [
        'name: e2e-run',
        'version: "1.0"',
        'nodes:',
        '  greet:',
        '    type: template',
        '    template: "hi from playwright"',
      ].join('\n'),
    );
    await page.getByTestId('create-inputs').fill('{}');

    await page.getByTestId('create-submit').click();

    // The form redirects to /ui?run=<uuid>. We wait for the URL to
    // change and assert the run query param is present.
    await page.waitForURL(/\/ui\?run=/);
    expect(page.url()).toMatch(/\/ui\?run=[0-9a-f-]+$/i);
  });

  test('persists workflow and profile between page loads', async ({ page }) => {
    await page.goto(`${baseURL}/ui/runs/new`);
    await page.getByTestId('create-profile').selectOption('production');
    await page.getByTestId('create-workflow').fill('name: persist-test\nnodes:\n  noop:\n    type: template\n    template: "x"');

    // Reload and confirm the values are still there.
    await page.reload();
    await expect(page.getByTestId('create-profile')).toHaveValue('production');
    await expect(page.getByTestId('create-workflow')).toHaveValue(/persist-test/);
  });

  test('does NOT persist the API token across reloads', async ({ page }) => {
    await page.goto(`${baseURL}/ui/runs/new`);
    await page.getByTestId('create-token').fill('SECRET_DO_NOT_PERSIST');
    await page.reload();
    // The token field falls back to whatever the global console-side
    // localStorage holds; what matters is the new-form never wrote
    // SECRET_DO_NOT_PERSIST into a new-form-specific slot.
    const local = await page.evaluate(() => {
      try {
        return Object.keys(window.localStorage).filter((key) => key.startsWith('agentflow.ui.newForm.'));
      } catch {
        return [];
      }
    });
    expect(local).not.toContain('agentflow.ui.newForm.apiToken');
  });
});
