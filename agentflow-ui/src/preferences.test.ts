// P10.17.2 — pure-function tests for the preferences sync helpers.

import { describe, it } from 'vitest';
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

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`assert failed: ${message}`);
  }
}

describe('serverKeyForLocal / localKeyForServer', () => {
  it('serverKeyForLocal maps every static key', () => {
    assert(
      Object.entries(STATIC_KEY_MAP).every(([local, server]) => serverKeyForLocal(local) === server),
      'every static key must round-trip',
    );
  });

  it('serverKeyForLocal returns null for the api token (security)', () => {
    assert(serverKeyForLocal('agentflow.ui.apiToken') === null, 'api token must not sync');
  });

  it('serverKeyForLocal returns null for workflow YAML draft', () => {
    assert(serverKeyForLocal('agentflow.ui.workflowDraft') === null, 'draft must not sync');
  });

  it('serverKeyForLocal returns null for harness user_input prompt', () => {
    assert(
      serverKeyForLocal('agentflow.ui.harness.newForm.user_input') === null,
      'prompt must not sync',
    );
  });

  it('serverKeyForLocal maps per-run event-filter keys with the run id', () => {
    assert(
      serverKeyForLocal('agentflow.ui.run.eventFilter.run-abc-123') ===
        'ui.event-filter.run-abc-123',
      'per-run event-filter key must map',
    );
  });

  it('serverKeyForLocal rejects empty-run-id event-filter (would create bogus server key)', () => {
    assert(
      serverKeyForLocal('agentflow.ui.run.eventFilter.') === null,
      'empty run id must not map',
    );
  });

  it('localKeyForServer round-trips every static key', () => {
    assert(
      Object.entries(STATIC_KEY_MAP).every(([local, server]) => localKeyForServer(server) === local),
      'every static key must round-trip in reverse',
    );
  });

  it('localKeyForServer round-trips dynamic event-filter keys', () => {
    assert(
      localKeyForServer('ui.event-filter.xyz') === 'agentflow.ui.run.eventFilter.xyz',
      'dynamic event-filter key must round-trip',
    );
  });

  it('localKeyForServer returns null for unknown server keys', () => {
    assert(localKeyForServer('not.a.known.key') === null, 'unknown key must be null');
  });

  it('localKeyForServer returns null for empty-run-id event-filter', () => {
    assert(localKeyForServer('ui.event-filter.') === null, 'empty run id must not map');
  });
});

describe('isSyncableLocalKey', () => {
  it('true for tenant', () => {
    assert(isSyncableLocalKey('agentflow.ui.tenantId'), 'tenant is syncable');
  });

  it('false for api token', () => {
    assert(!isSyncableLocalKey('agentflow.ui.apiToken'), 'api token is not syncable');
  });

  it('false for harness workspace_root (machine-specific path)', () => {
    assert(
      !isSyncableLocalKey('agentflow.ui.harness.newForm.workspace_root'),
      'workspace_root is not syncable',
    );
  });
});

describe('serverPreferencesToLocalEntries', () => {
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

  it('transcribes known string values directly', () => {
    assert(overlay['agentflow.ui.tenantId'] === 'team-alpha', 'string value transcribed');
  });

  it('JSON-stringifies non-string values', () => {
    assert(
      overlay['agentflow.ui.harness.newForm.runtime_kind'] === '42',
      'non-string value stringified',
    );
  });

  it('silently drops unknown server keys', () => {
    assert(
      !('agentflow.ui.future.unknown.key' in overlay) &&
        // Four KNOWN input keys → four output entries.
        // (run-console.tenant, new-form.profile, event-filter.run-7,
        // harness-new-form.runtime). The 5th input (future.unknown.key)
        // must NOT appear in the output.
        Object.keys(overlay).length === 4,
      'unknown key dropped, exactly 4 known entries survive',
    );
  });

  it('handles per-run-id event filters', () => {
    assert(
      overlay['agentflow.ui.run.eventFilter.run-7'] === 'kind:tool_call_completed',
      'per-run event filter preserved',
    );
  });
});

describe('tenantHeaders', () => {
  it('sets X-Agentflow-Tenant', () => {
    const headers = tenantHeaders('team-alpha') as Record<string, string>;
    assert(headers['X-Agentflow-Tenant'] === 'team-alpha', 'tenant header set');
  });
});

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

describe('loadServerPreferences', () => {
  it('GETs /v1/preferences and forwards the tenant header', async () => {
    await withFakeFetch(
      {
        ok: true,
        body: { preferences: { 'ui.run-console.tenant': 'team-alpha' } },
      },
      async (fetcher, calls) => {
        const prefs = await loadServerPreferences(fetcher, 'tenant-x');
        assert(
          calls.length === 1 && calls[0].path === '/v1/preferences',
          'GETs /v1/preferences',
        );
        const headerStore = (calls[0].init?.headers ?? {}) as Record<string, string>;
        assert(
          headerStore['X-Agentflow-Tenant'] === 'tenant-x',
          'forwards X-Agentflow-Tenant header',
        );
        assert(
          prefs['ui.run-console.tenant'] === 'team-alpha',
          'returns the preferences object',
        );
      },
    );
  });

  it('throws on non-2xx with the status in the message', async () => {
    await withFakeFetch({ ok: false, status: 500, statusText: 'oops' }, async (fetcher) => {
      let threw = false;
      try {
        await loadServerPreferences(fetcher, 'tenant-x');
      } catch (err) {
        threw = true;
        assert(
          String(err).includes('500') && String(err).includes('oops'),
          'error message names the status',
        );
      }
      assert(threw, 'non-2xx must propagate; caller decides whether to swallow');
    });
  });
});

describe('saveServerPreferences', () => {
  it('PUTs /v1/preferences wrapped in the PreferencesEnvelope shape', async () => {
    await withFakeFetch({ ok: true, body: {} }, async (fetcher, calls) => {
      await saveServerPreferences(fetcher, 'tenant-y', {
        'ui.run-console.tenant': 'team-beta',
      });
      assert(
        calls.length === 1 &&
          calls[0].path === '/v1/preferences' &&
          calls[0].init?.method === 'PUT',
        'PUTs /v1/preferences',
      );
      const body = JSON.parse(String(calls[0].init?.body));
      assert(
        body.preferences?.['ui.run-console.tenant'] === 'team-beta',
        'wraps in the PreferencesEnvelope shape',
      );
    });
  });
});

describe('PreferenceWriteQueue', () => {
  it('collapses rapid writes into one flush, last write wins per key', async () => {
    const flushes: Record<string, unknown>[] = [];
    const q = new PreferenceWriteQueue(50, (entries) => flushes.push(entries));

    q.enqueue('a', 1);
    q.enqueue('a', 2); // overwrites pending 'a'
    q.enqueue('b', 3);

    await new Promise((r) => setTimeout(r, 80));

    assert(flushes.length === 1, `expected exactly one flush, got ${JSON.stringify(flushes)}`);
    assert(flushes[0]?.a === 2, 'last write wins per key');
    assert(flushes[0]?.a === 2 && flushes[0]?.b === 3, 'includes every distinct key');
  });

  it('cancel() aborts the pending flush', async () => {
    const flushes: Record<string, unknown>[] = [];
    const q = new PreferenceWriteQueue(50, (entries) => flushes.push(entries));
    q.enqueue('x', 'val');
    q.cancel();
    await new Promise((r) => setTimeout(r, 80));
    assert(flushes.length === 0, 'cancel() aborts the pending flush');
  });

  it('flushNow() fires synchronously', () => {
    const flushes: Record<string, unknown>[] = [];
    const q = new PreferenceWriteQueue(10_000, (entries) => flushes.push(entries));
    q.enqueue('k', 'v');
    q.flushNow();
    assert(flushes.length === 1 && flushes[0].k === 'v', 'flushNow() fires synchronously');
  });
});
