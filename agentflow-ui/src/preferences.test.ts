// P10.17.2 — pure-function tests for the preferences sync helpers.
// Same `node --import tsx` pattern as `eventFilter.test.ts`: no test
// framework, just assertions audited at a glance. CI runs via
// `npm test` which currently only typechecks, but the file is
// importable and runnable on demand.

import {
  PreferenceWriteQueue,
  STATIC_KEY_MAP,
  isSyncableLocalKey,
  localKeyForServer,
  loadServerPreferences,
  saveServerPreferences,
  serverKeyForLocal,
  serverPreferencesToLocalEntries,
  tenantHeaders,
  type ApiFetcher,
} from './preferences';

let failures = 0;

function check(label: string, condition: boolean, detail?: string): void {
  if (!condition) {
    failures += 1;
    console.error(`FAIL ${label}${detail ? `: ${detail}` : ''}`);
  } else {
    console.log(`PASS ${label}`);
  }
}

// ── serverKeyForLocal / localKeyForServer ────────────────────────────

check(
  'serverKeyForLocal maps every static key',
  Object.entries(STATIC_KEY_MAP).every(
    ([local, server]) => serverKeyForLocal(local) === server,
  ),
);

check(
  'serverKeyForLocal returns null for the api token (security)',
  serverKeyForLocal('agentflow.ui.apiToken') === null,
);

check(
  'serverKeyForLocal returns null for workflow YAML draft',
  serverKeyForLocal('agentflow.ui.workflowDraft') === null,
);

check(
  'serverKeyForLocal returns null for harness user_input prompt',
  serverKeyForLocal('agentflow.ui.harness.newForm.user_input') === null,
);

check(
  'serverKeyForLocal maps per-run event-filter keys with the run id',
  serverKeyForLocal('agentflow.ui.run.eventFilter.run-abc-123') ===
    'ui.event-filter.run-abc-123',
);

check(
  'serverKeyForLocal rejects empty-run-id event-filter (would create bogus server key)',
  serverKeyForLocal('agentflow.ui.run.eventFilter.') === null,
);

check(
  'localKeyForServer round-trips every static key',
  Object.entries(STATIC_KEY_MAP).every(
    ([local, server]) => localKeyForServer(server) === local,
  ),
);

check(
  'localKeyForServer round-trips dynamic event-filter keys',
  localKeyForServer('ui.event-filter.xyz') ===
    'agentflow.ui.run.eventFilter.xyz',
);

check(
  'localKeyForServer returns null for unknown server keys',
  localKeyForServer('not.a.known.key') === null,
);

check(
  'localKeyForServer returns null for empty-run-id event-filter',
  localKeyForServer('ui.event-filter.') === null,
);

// ── isSyncableLocalKey ───────────────────────────────────────────────

check(
  'isSyncableLocalKey true for tenant',
  isSyncableLocalKey('agentflow.ui.tenantId'),
);

check(
  'isSyncableLocalKey false for api token',
  !isSyncableLocalKey('agentflow.ui.apiToken'),
);

check(
  'isSyncableLocalKey false for harness workspace_root (machine-specific path)',
  !isSyncableLocalKey('agentflow.ui.harness.newForm.workspace_root'),
);

// ── serverPreferencesToLocalEntries ───────────────────────────────────

const overlay = serverPreferencesToLocalEntries({
  'ui.run-console.tenant': 'team-alpha',
  'ui.new-form.profile': 'production',
  'ui.event-filter.run-7': 'kind:tool_call_completed',
  // Unknown keys must be silently ignored — we don't want a
  // future server schema version to drop old client UIs.
  'ui.future.unknown.key': 'whatever',
  // Numeric / boolean values get JSON.stringify'd so localStorage
  // (string-only) round-trips them.
  'ui.harness-new-form.runtime': 42,
});

check(
  'serverPreferencesToLocalEntries transcribes known string values directly',
  overlay['agentflow.ui.tenantId'] === 'team-alpha',
);

check(
  'serverPreferencesToLocalEntries JSON-stringifies non-string values',
  overlay['agentflow.ui.harness.newForm.runtime_kind'] === '42',
);

check(
  'serverPreferencesToLocalEntries silently drops unknown server keys',
  !('agentflow.ui.future.unknown.key' in overlay) &&
    // Four KNOWN input keys → four output entries.
    // (run-console.tenant, new-form.profile, event-filter.run-7,
    // harness-new-form.runtime). The 5th input (future.unknown.key)
    // must NOT appear in the output.
    Object.keys(overlay).length === 4,
);

check(
  'serverPreferencesToLocalEntries handles per-run-id event filters',
  overlay['agentflow.ui.run.eventFilter.run-7'] ===
    'kind:tool_call_completed',
);

// ── tenantHeaders ─────────────────────────────────────────────────────

const headers = tenantHeaders('team-alpha') as Record<string, string>;
check(
  'tenantHeaders sets X-Agentflow-Tenant',
  headers['X-Agentflow-Tenant'] === 'team-alpha',
);

// ── loadServerPreferences ─────────────────────────────────────────────

interface FakeResponseSpec {
  ok?: boolean;
  status?: number;
  statusText?: string;
  body?: unknown;
}

async function withFakeFetch(
  fakeResponse: FakeResponseSpec,
  fn: (
    fetcher: ApiFetcher,
    calls: { path: string; init?: RequestInit }[],
  ) => Promise<void>,
) {
  const calls: { path: string; init?: RequestInit }[] = [];
  const fetcher: ApiFetcher = async (path, init) => {
    calls.push({ path, init });
    // The helpers only touch `ok` / `status` / `statusText` / `json`,
    // so a minimal shape cast through `unknown` is the cheapest way
    // to satisfy `tsc --noEmit` without pulling in a fetch mock lib.
    return {
      ok: fakeResponse.ok ?? true,
      status: fakeResponse.status ?? 200,
      statusText: fakeResponse.statusText ?? 'OK',
      async json() {
        return fakeResponse.body ?? {};
      },
    } as unknown as Response;
  };
  await fn(fetcher, calls);
}

// Top-level await keeps the bun test runner from marking the file
// "done" before the async assertions complete. Previous attempts
// used IIFEs but bun returned to the outer test discovery before
// the IIFE timers fired, leaving the queue tests silently unrun.

await withFakeFetch(
  {
    ok: true,
    body: { preferences: { 'ui.run-console.tenant': 'team-alpha' } },
  },
  async (fetcher, calls) => {
    const prefs = await loadServerPreferences(fetcher, 'tenant-x');
    check(
      'loadServerPreferences GETs /v1/preferences',
      calls.length === 1 && calls[0].path === '/v1/preferences',
    );
    const headerStore = (calls[0].init?.headers ?? {}) as Record<string, string>;
    check(
      'loadServerPreferences forwards X-Agentflow-Tenant header',
      headerStore['X-Agentflow-Tenant'] === 'tenant-x',
    );
    check(
      'loadServerPreferences returns the preferences object',
      prefs['ui.run-console.tenant'] === 'team-alpha',
    );
  },
);

await withFakeFetch({ ok: false, status: 500, statusText: 'oops' }, async (fetcher) => {
  let threw = false;
  try {
    await loadServerPreferences(fetcher, 'tenant-x');
  } catch (err) {
    threw = true;
    check(
      'loadServerPreferences error message names the status',
      String(err).includes('500') && String(err).includes('oops'),
    );
  }
  check(
    'loadServerPreferences throws on non-2xx',
    threw,
    'non-2xx must propagate; caller decides whether to swallow',
  );
});

// saveServerPreferences body shape.
await withFakeFetch({ ok: true, body: {} }, async (fetcher, calls) => {
  await saveServerPreferences(fetcher, 'tenant-y', {
    'ui.run-console.tenant': 'team-beta',
  });
  check(
    'saveServerPreferences PUTs /v1/preferences',
    calls.length === 1 &&
      calls[0].path === '/v1/preferences' &&
      calls[0].init?.method === 'PUT',
  );
  const body = JSON.parse(String(calls[0].init?.body));
  check(
    'saveServerPreferences wraps in the PreferencesEnvelope shape',
    body.preferences?.['ui.run-console.tenant'] === 'team-beta',
  );
});

// ── PreferenceWriteQueue ──────────────────────────────────────────────

const flushes: Record<string, unknown>[] = [];
const q = new PreferenceWriteQueue(50, (entries) => flushes.push(entries));

q.enqueue('a', 1);
q.enqueue('a', 2); // overwrites pending 'a'
q.enqueue('b', 3);

await new Promise((r) => setTimeout(r, 80));

check(
  'PreferenceWriteQueue collapses rapid writes into one flush',
  flushes.length === 1,
  `flushes=${JSON.stringify(flushes)}`,
);
check('PreferenceWriteQueue last write wins per key', flushes[0]?.a === 2);
check(
  'PreferenceWriteQueue includes every distinct key',
  flushes[0]?.a === 2 && flushes[0]?.b === 3,
);

// cancel()
const flushes2: Record<string, unknown>[] = [];
const q2 = new PreferenceWriteQueue(50, (entries) => flushes2.push(entries));
q2.enqueue('x', 'val');
q2.cancel();
await new Promise((r) => setTimeout(r, 80));
check(
  'PreferenceWriteQueue.cancel() aborts the pending flush',
  flushes2.length === 0,
);

// flushNow()
const flushes3: Record<string, unknown>[] = [];
const q3 = new PreferenceWriteQueue(10_000, (entries) => flushes3.push(entries));
q3.enqueue('k', 'v');
q3.flushNow();
check(
  'PreferenceWriteQueue.flushNow() fires synchronously',
  flushes3.length === 1 && flushes3[0].k === 'v',
);

// Tail summary — exit code 1 if anything failed so CI signals.
// The `globalThis` cast lets strict-mode typecheck succeed without
// pulling in `@types/node` for one process.exit call.
if (failures > 0) {
  console.error(`\n${failures} test(s) failed`);
  (globalThis as unknown as { process: { exit(code: number): never } }).process.exit(1);
} else {
  console.log('\nAll preference helper tests passed.');
}
