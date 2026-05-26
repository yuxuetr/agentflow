// /ui/harness/sessions — P-H.5 slice 3 list view. Polls
// `/v1/harness/sessions?tenant_id=…` every 4s; clicking a row
// navigates to the session detail page.

import { useEffect, useState } from 'react';

import { apiFetch } from '../lib/api';
import { harnessNewFormTenantKey, harnessStatusTone } from '../lib/harness';
import { formatTime } from '../lib/helpers';
import { readStorage, writeStorage } from '../lib/storage';
import { HarnessSessionArraySchema, type HarnessSession } from '../schemas';

export function HarnessSessionList({
  apiToken,
  onTokenChange,
}: {
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
  const [tenantId, setTenantId] = useState(() =>
    readStorage(harnessNewFormTenantKey, 'default'),
  );
  const [sessions, setSessions] = useState<HarnessSession[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    writeStorage(harnessNewFormTenantKey, tenantId);
  }, [tenantId]);

  const refresh = async () => {
    setBusy(true);
    setError(null);
    try {
      const params = new URLSearchParams({ tenant_id: tenantId.trim() || 'default' });
      const response = await apiFetch(`/v1/harness/sessions?${params.toString()}`, apiToken);
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`list failed: HTTP ${response.status} ${text}`);
      }
      // Q3.7.2: validate session list. Endpoint returns `{ sessions: [...] }`;
      // unwrap then run the dedicated array schema.
      const raw = (await response.json()) as { sessions?: unknown };
      const next = HarnessSessionArraySchema.parse(raw?.sessions ?? []);
      setSessions(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    // Q3.7.3 M5: in-flight guard prevents stacked refresh requests
    // when the gateway is slow. The pre-fix code fired a fresh
    // `refresh()` every 4s regardless of whether the previous one
    // had returned, so a 6-second response stamped over a 4-second
    // response with stale data (last-response-wins-not-most-recent-
    // request). The guard turns the interval into a "fire only if
    // idle" loop without losing the cadence.
    let cancelled = false;
    let inFlight = false;
    const tick = async () => {
      if (cancelled || inFlight) {
        return;
      }
      inFlight = true;
      try {
        await refresh();
      } finally {
        inFlight = false;
      }
    };
    void tick();
    const handle = window.setInterval(() => {
      void tick();
    }, 4000);
    return () => {
      cancelled = true;
      window.clearInterval(handle);
    };
    // We intentionally exclude `refresh` from deps: it closes over
    // tenantId+apiToken which we want to re-resolve via the explicit
    // dependency below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tenantId, apiToken]);

  return (
    <main className="shell harness-list-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AgentFlow / Harness</p>
          <h1>Sessions</h1>
        </div>
        <nav className="harness-nav">
          <a className="topbar-link" href="/ui">
            ← Run console
          </a>
          <a
            data-testid="harness-new-link"
            className="topbar-link topbar-cta"
            href="/ui/harness/sessions/new"
          >
            + New session
          </a>
        </nav>
      </header>

      <section className="harness-controls">
        <label>
          <span>Tenant</span>
          <input
            data-testid="harness-list-tenant"
            value={tenantId}
            onChange={(event) => setTenantId(event.target.value)}
            placeholder="default"
          />
        </label>
        <label>
          <span>API token</span>
          <input
            data-testid="harness-list-token"
            type="password"
            autoComplete="off"
            value={apiToken}
            onChange={(event) => onTokenChange(event.target.value)}
            placeholder="Bearer token (not persisted)"
          />
        </label>
        <button type="button" onClick={() => void refresh()} disabled={busy}>
          {busy ? 'Loading…' : 'Refresh'}
        </button>
      </section>

      {error ? <p className="error-line">{error}</p> : null}

      <section className="harness-table">
        <table data-testid="harness-list-table">
          <thead>
            <tr>
              <th>Started</th>
              <th>Status</th>
              <th>Profile</th>
              <th>Runtime</th>
              <th>Model</th>
              <th>Prompt</th>
              <th>ID</th>
            </tr>
          </thead>
          <tbody>
            {sessions.length === 0 ? (
              <tr>
                <td colSpan={7} className="harness-table-empty">
                  No sessions yet for tenant "{tenantId || 'default'}". Use{' '}
                  <a href="/ui/harness/sessions/new">+ New session</a> to create one.
                </td>
              </tr>
            ) : (
              sessions.map((session) => (
                <tr
                  key={session.id}
                  data-testid="harness-list-row"
                  onClick={() => window.location.assign(`/ui/harness/sessions/${session.id}`)}
                  className="harness-row"
                >
                  <td>{formatTime(session.started_at)}</td>
                  <td>
                    <span className={`status-pill status-${harnessStatusTone(session.status)}`}>
                      {session.status}
                    </span>
                  </td>
                  <td>{session.profile}</td>
                  <td>{session.runtime_kind}</td>
                  <td className="harness-cell-mono">{session.model}</td>
                  <td className="harness-cell-prompt">{session.user_input}</td>
                  <td className="harness-cell-mono">{session.id.slice(0, 8)}…</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </section>
    </main>
  );
}
