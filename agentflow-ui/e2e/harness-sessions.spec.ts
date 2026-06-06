/**
 * Playwright E2E spec for Harness Mode UI (`P-H.5 slice 3`).
 *
 * Drives the three new deep links against a running `agentflow serve`:
 *
 *   1. `/ui/harness/sessions/new` — submit form, redirect to detail.
 *   2. `/ui/harness/sessions/{id}` — detail page, event timeline, cancel.
 *   3. `/ui/harness/sessions` — list page, click row → detail.
 *
 * Just like the `/ui/runs/new` spec (P6.1), this is intentionally
 * lightweight and opt-in: install `@playwright/test` + `chromium`
 * locally, stand up a real `agentflow serve` against Postgres, then
 * run:
 *
 *   npx playwright test e2e/harness-sessions.spec.ts
 *
 * `BASE_URL` defaults to `http://127.0.0.1:8080`; set
 * `AGENTFLOW_API_TOKEN` if the gateway has production-profile auth
 * enabled. The submit test uses the `local` profile so any LLM
 * provider with a Mock key is enough — Moonshot is the convenient
 * default but not required.
 *
 * The spec stays import-fail-loud rather than silently disabling so
 * CI logs surface a missing `@playwright/test` clearly instead of a
 * green-but-empty run.
 */

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore — @playwright/test is intentionally an optional dev dep.
import { expect, test } from '@playwright/test';

const baseURL = process.env.BASE_URL ?? 'http://127.0.0.1:8080';
const apiToken = process.env.AGENTFLOW_API_TOKEN ?? '';

test.describe('P-H.5 slice 3 — /ui/harness/sessions', () => {
  test('submits a session and redirects to the detail page', async ({ page }) => {
    await page.goto(`${baseURL}/ui/harness/sessions/new`);

    if (apiToken) {
      await page.getByTestId('harness-new-token').fill(apiToken);
    }
    await page.getByTestId('harness-new-tenant').fill(`e2e-${Date.now()}`);
    await page.getByTestId('harness-new-profile').selectOption('local');
    await page.getByTestId('harness-new-runtime').selectOption('react');
    await page.getByTestId('harness-new-model').fill('moonshot-v1-auto');
    await page.getByTestId('harness-new-workspace').fill('/tmp');
    await page.getByTestId('harness-new-prompt').fill('Reply with a one-word answer.');

    await page.getByTestId('harness-new-submit').click();

    // Submit redirects to /ui/harness/sessions/<uuid>. We wait for
    // the URL to change and assert the path matches.
    // The detail-page URL may carry a `?tenant=<name>` query param
    // (added when the submit form propagates a non-`default` tenant
    // to the detail view so its scoped GETs / SSE pick up the same
    // `X-Agentflow-Tenant` header under Q1.4.3). Allow it optionally
    // so the regex still matches single-tenant deployments too.
    await page.waitForURL(/\/ui\/harness\/sessions\/[0-9a-f-]+(\?.*)?$/i);
    // Detail page renders the summary block. We don't wait for
    // terminal status here because that depends on the LLM provider —
    // the contract under test is "form → detail page renders".
    await expect(page.getByText('Status')).toBeVisible();
    await expect(page.getByTestId('harness-approvals-section')).toBeVisible();
  });

  test('list page links back to detail rows', async ({ page }) => {
    // Submit a quick session first so the list isn't empty.
    await page.goto(`${baseURL}/ui/harness/sessions/new`);
    if (apiToken) {
      await page.getByTestId('harness-new-token').fill(apiToken);
    }
    const tenant = `e2e-list-${Date.now()}`;
    await page.getByTestId('harness-new-tenant').fill(tenant);
    await page.getByTestId('harness-new-prompt').fill('list test');
    await page.getByTestId('harness-new-submit').click();
    // The detail-page URL may carry a `?tenant=<name>` query param
    // (added when the submit form propagates a non-`default` tenant
    // to the detail view so its scoped GETs / SSE pick up the same
    // `X-Agentflow-Tenant` header under Q1.4.3). Allow it optionally
    // so the regex still matches single-tenant deployments too.
    await page.waitForURL(/\/ui\/harness\/sessions\/[0-9a-f-]+(\?.*)?$/i);

    // Now visit the list scoped to the same tenant and click the row.
    await page.goto(`${baseURL}/ui/harness/sessions`);
    await page.getByTestId('harness-list-tenant').fill(tenant);
    // Wait for the row to appear (refresh runs on tenant change).
    await expect(page.getByTestId('harness-list-row').first()).toBeVisible({ timeout: 8000 });
    await page.getByTestId('harness-list-row').first().click();
    // The detail-page URL may carry a `?tenant=<name>` query param
    // (added when the submit form propagates a non-`default` tenant
    // to the detail view so its scoped GETs / SSE pick up the same
    // `X-Agentflow-Tenant` header under Q1.4.3). Allow it optionally
    // so the regex still matches single-tenant deployments too.
    await page.waitForURL(/\/ui\/harness\/sessions\/[0-9a-f-]+(\?.*)?$/i);
  });

  test('persists form inputs except the API token across reloads', async ({ page }) => {
    await page.goto(`${baseURL}/ui/harness/sessions/new`);
    await page.getByTestId('harness-new-tenant').fill('persist-tenant');
    await page.getByTestId('harness-new-profile').selectOption('production');
    await page.getByTestId('harness-new-prompt').fill('persistent prompt');
    await page.getByTestId('harness-new-token').fill('SECRET_DO_NOT_PERSIST');

    await page.reload();
    await expect(page.getByTestId('harness-new-tenant')).toHaveValue('persist-tenant');
    await expect(page.getByTestId('harness-new-profile')).toHaveValue('production');
    await expect(page.getByTestId('harness-new-prompt')).toHaveValue(/persistent prompt/);

    // The new-form-namespaced storage slots must not include the
    // token — same invariant as P6.1's spec.
    const newFormKeys = await page.evaluate(() => {
      try {
        return Object.keys(window.localStorage).filter((key) =>
          key.startsWith('agentflow.ui.harness.newForm.'),
        );
      } catch {
        return [];
      }
    });
    expect(newFormKeys).not.toContain('agentflow.ui.harness.newForm.apiToken');
    expect(newFormKeys).not.toContain('agentflow.ui.harness.newForm.token');
  });
});
