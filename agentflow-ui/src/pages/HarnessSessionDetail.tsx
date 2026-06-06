// /ui/harness/sessions/:id — P-H.5 slice 3 detail view. Wires the SSE
// timeline, polls session row + pending approvals, and exposes
// cancel / resume controls.

import { useEffect, useMemo, useState } from 'react';

import { apiFetch, parseSseChunk } from '../lib/api';
import {
  harnessStatusTone,
  isHarnessTerminal,
  type ApprovalOutcome,
  type ApprovalScope,
} from '../lib/harness';
import { eventTone, formatTime, prettyJson } from '../lib/helpers';
import {
  HarnessEventArraySchema,
  HarnessEventSchema,
  HarnessSessionSchema,
  PendingApprovalArraySchema,
  parseJsonResponse,
  type HarnessEvent,
  type HarnessSession,
  type PendingApproval,
} from '../schemas';

type ConnectionState = 'idle' | 'loading' | 'streaming' | 'reconnecting' | 'closed' | 'error';

/**
 * Pull the tenant out of `?tenant=<name>` on the current page URL. The
 * submit form encodes it on redirect so the detail view scopes its
 * `X-Agentflow-Tenant` header correctly under the gateway's strict
 * tenant boundary (Q1.4.2/3). When absent we fall back to `default` to
 * keep zero-config single-tenant deployments working.
 */
const readTenantFromUrl = (): string => {
  try {
    const params = new URLSearchParams(window.location.search);
    const value = params.get('tenant');
    if (value && value.trim()) {
      return value.trim();
    }
  } catch {
    // SSR / no-window — defensive only; the SPA always runs in browser.
  }
  return 'default';
};

export function HarnessSessionDetail({
  sessionId,
  apiToken,
  onTokenChange,
}: {
  sessionId: string;
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
  // Tenant comes from the URL query param so a fresh tab / direct link
  // works without relying on localStorage residue. The form encodes it
  // on redirect (`/ui/harness/sessions/{id}?tenant=...`).
  const [tenant] = useState<string>(() => readTenantFromUrl());
  const [session, setSession] = useState<HarnessSession | null>(null);
  const [events, setEvents] = useState<HarnessEvent[]>([]);
  const [approvals, setApprovals] = useState<PendingApproval[]>([]);
  const [selectedSeq, setSelectedSeq] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [streamState, setStreamState] = useState<ConnectionState>('idle');
  const [resumeBusy, setResumeBusy] = useState(false);
  const [resumePrompt, setResumePrompt] = useState('');
  const [resumeMode, setResumeMode] = useState<'rerun' | 'append'>('rerun');

  const mergeEvent = (incoming: HarnessEvent) => {
    setEvents((prior) => {
      // Idempotent merge: if we already have this seq, keep the
      // earlier copy (DB rows are immutable). SSE backfill + DB
      // backfill happily overlap during the EventSource warm-up.
      if (prior.some((existing) => existing.seq === incoming.seq)) {
        return prior;
      }
      return [...prior, incoming].sort((a, b) => a.seq - b.seq);
    });
  };

  const replaceEvents = (incoming: HarnessEvent[]) => {
    setEvents([...incoming].sort((a, b) => a.seq - b.seq));
  };

  const fetchSession = async () => {
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}`,
        apiToken,
        {},
        tenant,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`session fetch failed: HTTP ${response.status} ${text}`);
      }
      const body = await parseJsonResponse(
        HarnessSessionSchema,
        response,
        `GET /v1/harness/sessions/${sessionId}`,
      );
      setSession(body);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  // Polling fallback used when SSE fails. Hits the JSON history route
  // and replaces the local list — works whether or not the server has
  // a broker channel for this session.
  const fetchEventsFallback = async () => {
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}/events/history`,
        apiToken,
        {},
        tenant,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`events fetch failed: HTTP ${response.status} ${text}`);
      }
      const body = await parseJsonResponse(
        HarnessEventArraySchema,
        response,
        `GET /v1/harness/sessions/${sessionId}/events/history`,
      );
      replaceEvents(body);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const fetchApprovals = async () => {
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}/approvals`,
        apiToken,
        {},
        tenant,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`approvals fetch failed: HTTP ${response.status} ${text}`);
      }
      // Q3.7.2: validate pending-approval list. Endpoint wraps in
      // `{ approvals: [...] }`; unwrap then run the array schema.
      const raw = (await response.json()) as { approvals?: unknown };
      const next = PendingApprovalArraySchema.parse(raw?.approvals ?? []);
      setApprovals(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  // SSE wires the event timeline to the gateway's broker. The session
  // row + pending approvals still poll on a slower cadence since
  // EventSource only covers the event stream — approvals are a
  // separate REST surface.
  useEffect(() => {
    void fetchSession();
    void fetchApprovals();
    // Seed once via the history route so the timeline doesn't appear
    // empty before the first SSE frame arrives.
    void fetchEventsFallback();

    setStreamState('loading');
    // Q1.9.2: native `EventSource` cannot send custom headers, so
    // there was no way to attach the `Authorization: Bearer <token>`
    // a production-profile gateway requires. The old code happily
    // built `new EventSource(...)` and silently fell back to 5-second
    // polling when the server returned 401 — operators saw "live"
    // updates but they were actually arriving from the history
    // endpoint with seconds of lag. We now use `fetch` +
    // ReadableStream so the auth header travels with the SSE
    // request; auto-reconnect is implemented by re-entering the
    // same loop on stream end.
    const abortController = new AbortController();
    let fallbackHandle: number | null = null;
    let reconnectHandle: number | null = null;
    let closed = false;
    // Q3.7.3 M5: in-flight guards prevent stacked requests when the
    // gateway is slow. Each poll-loop call inside `setInterval`
    // skips its tick if the previous request hasn't returned yet —
    // turns the interval into a "fire only if idle" loop without
    // losing the cadence.
    let sessionInFlight = false;
    let fallbackInFlight = false;

    const startStream = async () => {
      while (!closed && !abortController.signal.aborted) {
        try {
          const response = await apiFetch(
            `/v1/harness/sessions/${sessionId}/events`,
            apiToken,
            { signal: abortController.signal, headers: { Accept: 'text/event-stream' } },
            tenant,
          );
          if (!response.ok || response.body === null) {
            setStreamState('error');
            if (fallbackHandle === null) {
              fallbackHandle = window.setInterval(() => {
                if (closed || fallbackInFlight) {
                  return;
                }
                fallbackInFlight = true;
                void fetchEventsFallback().finally(() => {
                  fallbackInFlight = false;
                });
              }, 5000);
            }
            // Wait before retrying to avoid hot-looping on a 401.
            await new Promise<void>((resolve) => {
              reconnectHandle = window.setTimeout(() => resolve(), 5000);
            });
            continue;
          }
          setStreamState('streaming');
          const reader = response.body.getReader();
          const decoder = new TextDecoder();
          let buffer = '';
          while (!closed && !abortController.signal.aborted) {
            const { value, done } = await reader.read();
            if (done) {
              break;
            }
            buffer += decoder.decode(value, { stream: true });
            const { events: parsed, rest } = parseSseChunk(
              buffer,
              HarnessEventSchema,
              'harness SSE event',
            );
            buffer = rest;
            for (const ev of parsed) {
              mergeEvent(ev);
            }
          }
        } catch (err) {
          if (abortController.signal.aborted) {
            break;
          }
          setStreamState('error');
          if (fallbackHandle === null) {
            fallbackHandle = window.setInterval(() => {
              if (closed || fallbackInFlight) {
                return;
              }
              fallbackInFlight = true;
              void fetchEventsFallback().finally(() => {
                fallbackInFlight = false;
              });
            }, 5000);
          }
          await new Promise<void>((resolve) => {
            reconnectHandle = window.setTimeout(() => resolve(), 5000);
          });
        }
      }
    };

    try {
      void startStream();
    } catch (err) {
      setStreamState('error');
      setError(err instanceof Error ? err.message : String(err));
    }

    // Approval poll + session row poll, every 2 s while not terminal.
    // Q3.7.3 M5: in-flight guard skips ticks while a previous poll is
    // still outstanding; the next sample arrives on the next interval
    // without queuing concurrent requests against the gateway.
    const sessionHandle = window.setInterval(() => {
      if (closed || sessionInFlight) {
        return;
      }
      sessionInFlight = true;
      void Promise.all([fetchSession(), fetchApprovals()]).finally(() => {
        sessionInFlight = false;
      });
    }, 2000);

    return () => {
      closed = true;
      abortController.abort();
      window.clearInterval(sessionHandle);
      if (fallbackHandle !== null) {
        window.clearInterval(fallbackHandle);
      }
      if (reconnectHandle !== null) {
        window.clearTimeout(reconnectHandle);
      }
      setStreamState('closed');
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, apiToken]);

  const decide = async (
    requestId: string,
    decision: ApprovalOutcome,
    scope: ApprovalScope,
  ) => {
    setError(null);
    setInfo(null);
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}/approvals/${encodeURIComponent(requestId)}`,
        apiToken,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ decision, scope, decided_by: 'ui' }),
        },
        tenant,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`decide failed: HTTP ${response.status} ${text}`);
      }
      setInfo(`Approval ${requestId} → ${decision}/${scope}`);
      // Refresh immediately so the approval clears without waiting
      // for the next poll tick. The SSE stream picks up the
      // `approval_decided` envelope on its own.
      void fetchApprovals();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const cancel = async () => {
    setError(null);
    setInfo(null);
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}:cancel`,
        apiToken,
        { method: 'POST' },
        tenant,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`cancel failed: HTTP ${response.status} ${text}`);
      }
      setInfo('Cancel requested');
      void fetchSession();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  // P-H.5: resume restarts (rerun) or extends (append) the session.
  // Server enforces the terminal precondition; the button is only
  // enabled when `terminal` is true. After the POST the SSE stream
  // takes over again — for rerun we drop the local timeline so stale
  // events don't show while the executor reproduces them; for append
  // we keep prior events on screen since the new seqs will arrive on
  // top of them as a single continuous timeline.
  const resume = async () => {
    setError(null);
    setInfo(null);
    setResumeBusy(true);
    try {
      const body: Record<string, unknown> = { mode: resumeMode };
      const trimmed = resumePrompt.trim();
      if (trimmed) {
        body.user_input = trimmed;
      }
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}:resume`,
        apiToken,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        },
        tenant,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`resume failed: HTTP ${response.status} ${text}`);
      }
      if (resumeMode === 'rerun') {
        setInfo('Resume (rerun) — events reset, executor restarted.');
        replaceEvents([]);
        setSelectedSeq(null);
      } else {
        setInfo('Resume (append) — prior events preserved, seq continues.');
      }
      setResumePrompt('');
      void fetchSession();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setResumeBusy(false);
    }
  };

  const selectedEvent = useMemo(
    () => events.find((event) => event.seq === selectedSeq) ?? null,
    [events, selectedSeq],
  );

  const terminal = isHarnessTerminal(session);

  return (
    <main className="shell harness-detail-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AgentFlow / Harness</p>
          <h1>Session {sessionId.slice(0, 8)}…</h1>
        </div>
        <nav className="harness-nav">
          <a className="topbar-link" href="/ui/harness/sessions">
            ← Sessions
          </a>
          <a className="topbar-link" href="/ui/harness/sessions/new">
            + New session
          </a>
        </nav>
      </header>

      <section className="harness-controls">
        <label>
          <span>API token</span>
          <input
            data-testid="harness-detail-token"
            type="password"
            autoComplete="off"
            value={apiToken}
            onChange={(event) => onTokenChange(event.target.value)}
            placeholder="Bearer token (not persisted)"
          />
        </label>
        <span
          data-testid="harness-detail-stream-state"
          className={`stream-pill stream-${streamState}`}
        >
          stream: {streamState}
        </span>
        <button
          data-testid="harness-detail-cancel"
          type="button"
          onClick={() => void cancel()}
          disabled={terminal}
        >
          {terminal ? 'Terminal' : 'Cancel session'}
        </button>
      </section>

      <section className="harness-controls harness-resume-controls">
        <label className="harness-grow">
          <span>Resume prompt (optional — empty replays original)</span>
          <input
            data-testid="harness-detail-resume-prompt"
            value={resumePrompt}
            onChange={(event) => setResumePrompt(event.target.value)}
            placeholder="Leave blank to rerun with the original prompt"
            disabled={!terminal || resumeBusy}
          />
        </label>
        <label>
          <span>Mode</span>
          <select
            data-testid="harness-detail-resume-mode"
            value={resumeMode}
            onChange={(event) => setResumeMode(event.target.value as 'rerun' | 'append')}
            disabled={!terminal || resumeBusy}
          >
            <option value="rerun">rerun (reset events)</option>
            <option value="append">append (continue seq)</option>
          </select>
        </label>
        <button
          data-testid="harness-detail-resume"
          type="button"
          onClick={() => void resume()}
          disabled={!terminal || resumeBusy}
        >
          {resumeBusy ? 'Resuming…' : `Resume (${resumeMode})`}
        </button>
      </section>

      {error ? <p className="error-line">{error}</p> : null}
      {info ? <p className="info-line">{info}</p> : null}

      <section className="harness-detail-grid">
        <section className="harness-summary">
          <h2>Summary</h2>
          {session ? (
            <dl>
              <div>
                <dt>Status</dt>
                <dd>
                  <span className={`status-pill status-${harnessStatusTone(session.status)}`}>
                    {session.status}
                  </span>
                </dd>
              </div>
              <div>
                <dt>Tenant</dt>
                <dd>{session.tenant_id}</dd>
              </div>
              <div>
                <dt>Profile</dt>
                <dd>{session.profile}</dd>
              </div>
              <div>
                <dt>Runtime</dt>
                <dd>{session.runtime_kind}</dd>
              </div>
              <div>
                <dt>Model</dt>
                <dd className="harness-cell-mono">{session.model}</dd>
              </div>
              {session.skill_name ? (
                <div>
                  <dt>Skill</dt>
                  <dd>{session.skill_name}</dd>
                </div>
              ) : null}
              <div>
                <dt>Workspace</dt>
                <dd className="harness-cell-mono">{session.workspace_root}</dd>
              </div>
              <div>
                <dt>Started</dt>
                <dd>{formatTime(session.started_at)}</dd>
              </div>
              <div>
                <dt>Finished</dt>
                <dd>{formatTime(session.finished_at)}</dd>
              </div>
              {session.error ? (
                <div>
                  <dt>Error</dt>
                  <dd className="harness-cell-error">{session.error}</dd>
                </div>
              ) : null}
              {session.final_answer ? (
                <div className="harness-summary-answer">
                  <dt>Final answer</dt>
                  <dd>
                    <pre>{session.final_answer}</pre>
                  </dd>
                </div>
              ) : null}
              <div className="harness-summary-prompt">
                <dt>Prompt</dt>
                <dd>
                  <pre>{session.user_input}</pre>
                </dd>
              </div>
            </dl>
          ) : (
            <p>Loading…</p>
          )}
        </section>

        <section
          className="harness-approvals"
          aria-label="Pending approvals"
          data-testid="harness-approvals-section"
        >
          <h2>Pending approvals ({approvals.length})</h2>
          {approvals.length === 0 ? (
            <p className="harness-approvals-empty">
              No approvals waiting for this session.
            </p>
          ) : (
            <ul className="harness-approvals-list">
              {approvals.map((approval) => (
                <ApprovalCard
                  key={`${approval.session_id}-${approval.id}`}
                  approval={approval}
                  onDecide={(decision, scope) => void decide(approval.id, decision, scope)}
                />
              ))}
            </ul>
          )}
        </section>

        <section className="harness-timeline" aria-label="Event timeline">
          <h2>Timeline ({events.length})</h2>
          {events.length === 0 ? (
            <p className="harness-timeline-empty">No events yet.</p>
          ) : (
            <ol className="harness-event-list" data-testid="harness-event-list">
              {events.map((event) => (
                <li
                  key={event.seq}
                  className={`harness-event harness-event-${eventTone(event.kind)} ${
                    selectedSeq === event.seq ? 'harness-event-selected' : ''
                  }`}
                  onClick={() => setSelectedSeq(event.seq)}
                >
                  <span className="harness-event-seq">#{event.seq}</span>
                  <span className="harness-event-kind">{event.kind}</span>
                  <span className="harness-event-time">{formatTime(event.ts)}</span>
                </li>
              ))}
            </ol>
          )}
        </section>

        <section className="harness-event-payload">
          <h2>Event payload</h2>
          {selectedEvent ? (
            <pre>{prettyJson(selectedEvent.payload)}</pre>
          ) : (
            <p>Select an event from the timeline.</p>
          )}
        </section>
      </section>
    </main>
  );
}

function ApprovalCard({
  approval,
  onDecide,
}: {
  approval: PendingApproval;
  onDecide: (decision: ApprovalOutcome, scope: ApprovalScope) => void;
}) {
  const [scope, setScope] = useState<ApprovalScope>('once');
  return (
    <li className="harness-approval-card" data-testid="harness-approval-card">
      <header>
        <strong>{approval.tool}</strong>
        <span className={`risk-pill risk-${approval.risk}`}>{approval.risk}</span>
      </header>
      <p className="harness-approval-reason">{approval.reason}</p>
      <p className="harness-approval-meta">
        step #{approval.step_index} · {approval.idempotency ?? 'unknown'} · raised{' '}
        {formatTime(approval.requested_at)}
      </p>
      {approval.params_summary !== undefined && approval.params_summary !== null ? (
        <pre className="harness-approval-params">{prettyJson(approval.params_summary)}</pre>
      ) : null}
      <div className="harness-approval-controls">
        <label>
          <span>Scope</span>
          <select
            data-testid="harness-approval-scope"
            value={scope}
            onChange={(event) => setScope(event.target.value as ApprovalScope)}
          >
            <option value="once">once</option>
            <option value="session">session</option>
            <option value="run">run</option>
          </select>
        </label>
        <button
          data-testid="harness-approval-allow"
          type="button"
          className="harness-btn harness-btn-allow"
          onClick={() => onDecide('allow', scope)}
        >
          Allow
        </button>
        <button
          data-testid="harness-approval-deny"
          type="button"
          className="harness-btn harness-btn-deny"
          onClick={() => onDecide('deny', scope)}
        >
          Deny
        </button>
        <button
          data-testid="harness-approval-deny-stop"
          type="button"
          className="harness-btn harness-btn-deny"
          onClick={() => onDecide('deny_and_stop', scope)}
        >
          Deny &amp; Stop
        </button>
      </div>
    </li>
  );
}
