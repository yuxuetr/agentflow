import React, { useEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';
import { compileFilter, applyFilter, type FilterEvent } from './eventFilter';
import { usePreferenceSync } from './usePreferenceSync';
import { ErrorBoundary } from './components/ErrorBoundary';
import { DiagnosticsPanel } from './pages/DiagnosticsPanel';
import { HarnessSessionDetail } from './pages/HarnessSessionDetail';
import { HarnessSessionList } from './pages/HarnessSessionList';
import { HarnessSubmitForm } from './pages/HarnessSubmitForm';
import { RunCreateForm } from './pages/RunCreateForm';
import { harnessSessionIdFromPath } from './lib/harness';
import { apiFetch, parseSseChunk } from './lib/api';
import {
  eventFilterKeyPrefix,
  readSessionStorage,
  readStorage,
  tenantKey,
  tokenKey,
  workflowKey,
  writeSessionStorage,
  writeStorage,
} from './lib/storage';
import {
  eventNodeId,
  eventTone,
  findLatest,
  formatTime,
  isTerminalRun,
  prettyJson,
  reconnectDelayMs,
  runFromEnvelope,
} from './lib/helpers';
import {
  CancelRunEnvelopeSchema,
  CreateRunEnvelopeSchema,
  ListRunsEnvelopeSchema,
  RunEnvelopeSchema,
  StreamedEventArraySchema,
  StreamedEventSchema,
  parseJsonResponse,
  type RunEnvelope,
  type RunRecord,
  type StreamedEvent,
} from './schemas';

type ConnectionState = 'idle' | 'loading' | 'streaming' | 'reconnecting' | 'closed' | 'error';

// Storage keys, storage primitives, formatters, runFromEnvelope,
// apiFetch, parseSseChunk all moved to `./lib/{storage,helpers,api}.ts`
// in Q3.7.1 — import at the top of the file.

const starterWorkflow = `name: web-ui-console-smoke
nodes:
  - id: hello
    type: template
    parameters:
      template: "hello from the run console"`;

// `createFormStarterWorkflow` / `createFormStarterInputs` /
// `parseInputsBlock` / `lineCount` / `CreateProfile` / `CREATE_PROFILES`
// moved to `./pages/RunCreateForm.tsx` (Q3.7.1).

// ─── Existing run console ────────────────────────────────────────

// ── P6.3 Trace comparison view ────────────────────────────────────────────
//
// Mounted at `/ui/runs/:id/compare?against=<other_id>`. Loads the event
// history for both runs and renders a side-by-side timeline plus a
// per-step diff highlight + a hop-latency summary.

type CompareKey = string; // `${kind}#${step_index ?? seq}` used for cross-run pairing.

function compareKey(event: StreamedEvent): CompareKey {
  const payload = (event.payload as Record<string, unknown>) ?? {};
  const step = typeof payload.step_index === 'number' ? payload.step_index : event.seq;
  return `${event.kind}#${step}`;
}

function eventLatencyMs(events: StreamedEvent[], index: number): number | null {
  if (index === 0) return 0;
  const prev = Date.parse(events[index - 1].ts);
  const cur = Date.parse(events[index].ts);
  if (Number.isNaN(prev) || Number.isNaN(cur)) return null;
  return cur - prev;
}

async function fetchEventsHistory(
  runId: string,
  apiToken: string,
): Promise<StreamedEvent[]> {
  const response = await apiFetch(
    `/v1/runs/${encodeURIComponent(runId)}/events/history?limit=1000`,
    apiToken,
  );
  if (!response.ok) {
    throw new Error(`run ${runId}: ${response.status} ${response.statusText}`);
  }
  // Q3.7.2: validate the history payload before merging into compare
  // view. Endpoint wraps events in `{ events: [...] }`; build a
  // narrow inline schema so we don't need a one-off export.
  const raw = (await response.json()) as { events?: unknown };
  const events = StreamedEventArraySchema.parse(raw?.events ?? []);
  return events;
}

function RunCompare({
  primaryRunId,
  apiToken,
  onTokenChange,
}: {
  primaryRunId: string;
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
  const params = useMemo(() => new URLSearchParams(window.location.search), []);
  const against = params.get('against') ?? '';
  const [otherRunId, setOtherRunId] = useState(against);
  const [primaryEvents, setPrimaryEvents] = useState<StreamedEvent[]>([]);
  const [otherEvents, setOtherEvents] = useState<StreamedEvent[]>([]);
  const [state, setState] = useState<ConnectionState>('idle');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!primaryRunId || !otherRunId) return;
    let cancelled = false;
    setState('loading');
    setError(null);
    Promise.all([
      fetchEventsHistory(primaryRunId, apiToken),
      fetchEventsHistory(otherRunId, apiToken),
    ])
      .then(([primary, other]) => {
        if (cancelled) return;
        setPrimaryEvents(primary);
        setOtherEvents(other);
        setState('idle');
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
        setState('error');
      });
    return () => {
      cancelled = true;
    };
  }, [primaryRunId, otherRunId, apiToken]);

  // Compute the union of compare keys so both columns can highlight
  // events that lack a counterpart in the other run.
  const otherKeys = useMemo(
    () => new Set(otherEvents.map(compareKey)),
    [otherEvents],
  );
  const primaryKeys = useMemo(
    () => new Set(primaryEvents.map(compareKey)),
    [primaryEvents],
  );

  const primarySummary = useMemo(() => summarise(primaryEvents), [primaryEvents]);
  const otherSummary = useMemo(() => summarise(otherEvents), [otherEvents]);

  return (
    <main className="run-compare">
      <header className="run-compare-header">
        <div>
          <h1>Trace comparison</h1>
          <p className="run-compare-subtitle">
            Side-by-side event timelines plus per-step diff highlight + hop latency.
          </p>
        </div>
        <div className="run-compare-controls">
          <label>
            Primary run
            <input value={primaryRunId} readOnly />
          </label>
          <label>
            Compare against
            <input
              value={otherRunId}
              onChange={(ev) => setOtherRunId(ev.target.value.trim())}
              placeholder="run id"
            />
          </label>
          <label>
            API token (optional)
            <input
              type="password"
              value={apiToken}
              onChange={(ev) => onTokenChange(ev.target.value)}
              placeholder="bearer"
              autoComplete="off"
            />
          </label>
          <button
            type="button"
            onClick={() => {
              const next = new URLSearchParams({ against: otherRunId });
              window.location.assign(`/ui/runs/${encodeURIComponent(primaryRunId)}/compare?${next.toString()}`);
            }}
            disabled={!otherRunId || otherRunId === primaryRunId}
          >
            Compare
          </button>
        </div>
        {state === 'loading' && <p className="run-compare-status">Loading…</p>}
        {error && (
          <p className="run-compare-error" role="alert">
            {error}
          </p>
        )}
      </header>

      <section className="run-compare-summary" aria-label="Per-run summary">
        <SummaryCard title="Primary" runId={primaryRunId} summary={primarySummary} />
        <SummaryCard title="Compared" runId={otherRunId} summary={otherSummary} />
      </section>

      <section className="run-compare-grid" aria-label="Event timelines">
        <CompareColumn
          title={`Primary · ${primaryRunId}`}
          events={primaryEvents}
          otherKeys={otherKeys}
        />
        <CompareColumn
          title={`Compared · ${otherRunId || '—'}`}
          events={otherEvents}
          otherKeys={primaryKeys}
        />
      </section>
    </main>
  );
}

interface RunSummary {
  eventCount: number;
  toolCallCount: number;
  finalAnswer: string | null;
  totalLatencyMs: number;
  meanLatencyMs: number;
}

function summarise(events: StreamedEvent[]): RunSummary {
  let total = 0;
  let count = 0;
  for (let i = 1; i < events.length; i += 1) {
    const dt = eventLatencyMs(events, i);
    if (dt !== null) {
      total += dt;
      count += 1;
    }
  }
  const toolCallCount = events.filter((e) => e.kind.toLowerCase().includes('tool_call')).length;
  const finalAnswer = (() => {
    const last = [...events].reverse().find((e) => {
      const k = e.kind.toLowerCase();
      return k === 'run_completed' || k === 'final_answer' || k === 'stopped';
    });
    if (!last) return null;
    const payload = (last.payload as Record<string, unknown>) ?? {};
    const candidate = payload.answer ?? payload.final_answer ?? payload.message;
    return typeof candidate === 'string' ? candidate : null;
  })();
  return {
    eventCount: events.length,
    toolCallCount,
    finalAnswer,
    totalLatencyMs: total,
    meanLatencyMs: count > 0 ? Math.round(total / count) : 0,
  };
}

function SummaryCard({
  title,
  runId,
  summary,
}: {
  title: string;
  runId: string;
  summary: RunSummary;
}) {
  return (
    <article className="run-compare-card">
      <h2>{title}</h2>
      <p className="run-compare-card-id">{runId || '—'}</p>
      <dl>
        <div>
          <dt>Events</dt>
          <dd>{summary.eventCount}</dd>
        </div>
        <div>
          <dt>Tool calls</dt>
          <dd>{summary.toolCallCount}</dd>
        </div>
        <div>
          <dt>Total wall-clock</dt>
          <dd>{summary.totalLatencyMs} ms</dd>
        </div>
        <div>
          <dt>Mean hop</dt>
          <dd>{summary.meanLatencyMs} ms</dd>
        </div>
      </dl>
      {summary.finalAnswer && (
        <p className="run-compare-final" title="final answer">
          <strong>Final:</strong> {summary.finalAnswer}
        </p>
      )}
    </article>
  );
}

function CompareColumn({
  title,
  events,
  otherKeys,
}: {
  title: string;
  events: StreamedEvent[];
  otherKeys: Set<CompareKey>;
}) {
  return (
    <div className="run-compare-column" aria-label={title}>
      <h3>{title}</h3>
      {events.length === 0 ? (
        <p className="run-compare-empty">No events.</p>
      ) : (
        <ol className="run-compare-events">
          {events.map((event, idx) => {
            const key = compareKey(event);
            const matched = otherKeys.has(key);
            const latency = eventLatencyMs(events, idx);
            return (
              <li
                key={event.seq}
                className={`run-compare-event ${matched ? 'matched' : 'unmatched'}`}
              >
                <div className="run-compare-event-row">
                  <span className={`dot dot-${eventTone(event.kind)}`} />
                  <span className="run-compare-event-kind">{event.kind}</span>
                  {latency !== null && (
                    <span className="run-compare-event-latency">+{latency} ms</span>
                  )}
                </div>
                {!matched && (
                  <span className="run-compare-event-tag" title="present only in this run">
                    only here
                  </span>
                )}
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}

function RunConsole({ apiToken, onTokenChange }: { apiToken: string; onTokenChange: (token: string) => void }) {
  const [runId, setRunId] = useState('');
  const [tenantId, setTenantId] = useState(() => readStorage(tenantKey, 'default'));
  const [workflowDraft, setWorkflowDraft] = useState(() => readStorage(workflowKey, starterWorkflow));
  // P10.17.2: durable preferences via /v1/preferences. localStorage
  // stays the fast first-paint cache (set above); when the server
  // returns prefs for this tenant, the overlay effect below
  // updates `tenantId` if the server value differs. The PUT side
  // is wired in the existing writeStorage effect.
  const prefSync = usePreferenceSync(apiToken, tenantId);
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const [activeRun, setActiveRun] = useState<RunRecord | null>(null);
  const [events, setEvents] = useState<StreamedEvent[]>([]);
  const [selectedSeq, setSelectedSeq] = useState<number | null>(null);
  // P6.5: event-filter expression (matches the syntax in
  // `eventFilter.ts`). Empty string = match everything. Persisted per
  // run id under the eventFilterKeyPrefix slot.
  const [eventFilterExpr, setEventFilterExpr] = useState('');
  const [state, setState] = useState<ConnectionState>('idle');
  const [error, setError] = useState<string | null>(null);
  const [submitState, setSubmitState] = useState<'idle' | 'submitting' | 'cancelling'>('idle');
  const [lastReconnect, setLastReconnect] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  // Q3.7.3: SSE reconnect needs the latest seen `seq` to resume the
  // stream with `?after_seq=N` instead of re-replaying the whole
  // history. We can't use the `events` state directly because the
  // effect's deps list excludes it (intentionally — see comment at
  // the end of the effect). A ref keeps the latest value visible
  // inside the long-lived `connect()` closure across reconnect
  // attempts.
  const latestSeqRef = useRef<number>(-1);
  // Q3.7.3: continuous reconnect with exponential backoff. The pre-fix
  // code retried once on a 1.2s timeout and then surrendered to the
  // `error` state; operators saw a permanent "error" pill the moment
  // the SSE channel had a transient blip. We now retry indefinitely
  // with capped backoff so a 5-second network glitch self-recovers
  // without operator intervention.
  const reconnectAttemptRef = useRef<number>(0);

  const selectedEvent = useMemo(
    () => events.find((event) => event.seq === selectedSeq) ?? events.at(-1) ?? null,
    [events, selectedSeq],
  );

  // P6.5: load any per-run filter expression as the operator switches
  // between runs, and persist new edits so a reload picks up where the
  // operator left off.
  useEffect(() => {
    if (!runId) {
      setEventFilterExpr('');
      return;
    }
    setEventFilterExpr(readStorage(`${eventFilterKeyPrefix}${runId}`, ''));
  }, [runId]);
  useEffect(() => {
    if (!runId) return;
    writeStorage(`${eventFilterKeyPrefix}${runId}`, eventFilterExpr);
  }, [runId, eventFilterExpr]);

  // Compile the expression once per change, then apply to the event
  // list. Parse errors don't crash the UI — we surface the message
  // under the input.
  const eventFilter = useMemo(() => compileFilter(eventFilterExpr), [eventFilterExpr]);
  const filteredEvents = useMemo(
    () =>
      eventFilter.predicate
        ? applyFilter(events as unknown as FilterEvent[], eventFilter)
        : (events as unknown as FilterEvent[]),
    [events, eventFilter],
  );

  // P10.13.1: viz-derived graph data is gone. The button grid now
  // derives entirely from observed events — group per unique
  // `node_id`/kind, surface the most recent status, cap at 12 to
  // keep the lane compact for long runs.
  const nodeSummaries = useMemo(() => {
    const seen = new Map<string, { name: string; label: string; status: string; tone: string }>();
    for (const event of events) {
      const name = eventNodeId(event) || event.kind;
      seen.set(name, {
        name,
        label: name,
        status: event.kind,
        tone: eventTone(event.kind),
      });
    }
    return Array.from(seen.values()).slice(-12);
  }, [events]);

  const selectedNode = useMemo(() => {
    if (!selectedEvent) {
      return null;
    }
    const nodeId = eventNodeId(selectedEvent);
    if (!nodeId) {
      return null;
    }
    return {
      id: nodeId,
      label: nodeId,
      status: selectedEvent.kind,
      event: selectedEvent,
    };
  }, [selectedEvent]);

  const agentToolEvents = useMemo(
    () =>
      events.filter((event) => {
        const tone = eventTone(event.kind);
        return tone === 'agent' || tone === 'tool';
      }),
    [events],
  );

  const providerEvents = useMemo(
    () =>
      events.filter((event) => {
        const payload = event.payload as Record<string, unknown>;
        return payload.provider || payload.model || event.kind.toLowerCase().includes('llm');
      }),
    [events],
  );

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const id = params.get('run');
    if (id) {
      setRunId(id);
      setState('loading');
    }
  }, []);

  useEffect(() => {
    writeStorage(tenantKey, tenantId);
    // P10.17.2: best-effort PUT to /v1/preferences. Queue
    // debounces so rapid typing in the tenant input collapses to
    // one PUT. Workflow YAML drafts stay local-only — they're
    // large and can contain example tokens that would trip the
    // server's token-shape rejection.
    prefSync.syncToServer(tenantKey, tenantId);
  }, [tenantId, prefSync]);

  // P10.17.2: when the server snapshot arrives for this tenant,
  // overlay it onto the local state. Skip when the value matches
  // current state (avoids a render cycle) or when the server has
  // no entry for this key (first-time tenant, no sync yet).
  useEffect(() => {
    const fromServer = prefSync.serverPrefs?.[
      'ui.run-console.tenant'
    ];
    if (typeof fromServer === 'string' && fromServer !== tenantId) {
      setTenantId(fromServer);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefSync.serverPrefs]);

  useEffect(() => {
    writeStorage(workflowKey, workflowDraft);
  }, [workflowDraft]);

  const loadRuns = async (nextTenant = tenantId) => {
    const response = await apiFetch(`/v1/runs?tenant_id=${encodeURIComponent(nextTenant)}&limit=20`, apiToken);
    if (!response.ok) {
      throw new Error(`run list failed with HTTP ${response.status}`);
    }
    const payload = await parseJsonResponse(
      ListRunsEnvelopeSchema,
      response,
      'GET /v1/runs',
    );
    setRuns(payload.runs);
    if (!runId && payload.runs[0]) {
      setRunId(payload.runs[0].id);
      setState('loading');
    }
  };

  useEffect(() => {
    // Q3.7.3 M6: auto-refresh the run list every 4 seconds (mirrors
    // the HarnessSessionList pattern). Without this, the runs sidebar
    // is stale until the operator changes tenant or reloads — new
    // runs submitted from a different tab never show up.
    //
    // Q3.7.3 M5: in-flight guard prevents stacked requests when the
    // gateway is slow / under load. `inFlight` flips while a request
    // is outstanding; the next tick is skipped (next sample will
    // still arrive on the following interval).
    let inFlight = false;
    let cancelled = false;

    const tick = async () => {
      if (cancelled || inFlight) {
        return;
      }
      inFlight = true;
      try {
        await loadRuns();
      } catch {
        // Explicit run connection still works when the list is unavailable.
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apiToken, tenantId]);

  useEffect(() => {
    if (!runId || state !== 'loading') {
      return undefined;
    }

    let cancelled = false;
    let reconnectTimer: number | undefined;
    abortRef.current?.abort();
    // Q3.7.3: reset per-connection state. The seq ref starts at -1
    // (replay-from-beginning); each appended event bumps it. The
    // attempt counter drives the exponential-backoff delay.
    latestSeqRef.current = -1;
    reconnectAttemptRef.current = 0;

    const appendEvent = (event: StreamedEvent) => {
      setEvents((current) => {
        if (current.some((item) => item.seq === event.seq)) {
          return current;
        }
        return [...current, event].sort((left, right) => left.seq - right.seq);
      });
      setSelectedSeq((current) => current ?? event.seq);
      // Q3.7.3: keep the ref in lock-step with the visible events so a
      // mid-stream reconnect resumes from the last seen seq rather
      // than re-replaying from -1.
      if (event.seq > latestSeqRef.current) {
        latestSeqRef.current = event.seq;
      }
    };

    const connectStream = async (afterSeq: number) => {
      const controller = new AbortController();
      abortRef.current = controller;
      const response = await apiFetch(
        `/v1/runs/${runId}/events?after_seq=${encodeURIComponent(String(afterSeq))}`,
        apiToken,
        { signal: controller.signal },
      );
      if (!response.ok || !response.body) {
        throw new Error(`event stream failed with HTTP ${response.status}`);
      }
      setState('streaming');
      // Q3.7.3: successful (re)connect resets the backoff counter so
      // subsequent disconnects start from the 250ms floor again.
      reconnectAttemptRef.current = 0;
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = '';
      while (!cancelled) {
        const { done, value } = await reader.read();
        if (done) {
          break;
        }
        buffer += value;
        const parsed = parseSseChunk(buffer, StreamedEventSchema, 'workflow SSE event');
        buffer = parsed.rest;
        for (const event of parsed.events) {
          appendEvent(event);
        }
      }
    };

    // Q3.7.3: exponential backoff schedule for SSE reconnects lives in
    // `lib/helpers.ts` so unit tests can pin the cadence without
    // spinning up a React renderer.

    const scheduleReconnect = () => {
      if (cancelled) {
        return;
      }
      const attempt = reconnectAttemptRef.current;
      reconnectAttemptRef.current = attempt + 1;
      const delay = reconnectDelayMs(attempt);
      setLastReconnect(formatTime(new Date().toISOString()));
      setState('reconnecting');
      reconnectTimer = window.setTimeout(() => {
        if (cancelled) {
          return;
        }
        // Resume from the latest seq we've actually seen — the ref
        // sidesteps the M4 stale-closure bug where `events` was
        // captured at `[]` after the initial `setEvents([])` call.
        void connectStream(latestSeqRef.current).catch((streamErr) => {
          if (cancelled || (streamErr instanceof DOMException && streamErr.name === 'AbortError')) {
            return;
          }
          setError(streamErr instanceof Error ? streamErr.message : String(streamErr));
          scheduleReconnect();
        });
      }, delay);
    };

    const connect = async () => {
      try {
        const response = await apiFetch(`/v1/runs/${runId}`, apiToken);
        if (!response.ok) {
          throw new Error(`run lookup failed with HTTP ${response.status}`);
        }
        const payload = await parseJsonResponse(
          RunEnvelopeSchema,
          response,
          `GET /v1/runs/${runId}`,
        );
        if (cancelled) {
          return;
        }
        const nextRun = runFromEnvelope(payload);
        setActiveRun(nextRun);
        setEvents([]);
        setSelectedSeq(null);
        setError(null);
        window.history.replaceState(null, '', `/ui?run=${encodeURIComponent(runId)}`);

        // P10.17.3: when the operator already has a filter expression
        // (loaded from localStorage on the run-id effect above), pass
        // it through to the server so very long runs don't ship every
        // event just to be filtered client-side. The client-side
        // filter still runs on the returned events as a defensive
        // (live SSE events arrive after the initial fetch and aren't
        // server-pre-filtered). On a 400 from a malformed expression
        // the UI silently retries without the filter — the inline
        // parse error from `compileFilter` is already visible.
        let historyUrl = `/v1/runs/${runId}/events/history`;
        const initialFilter = readStorage(`${eventFilterKeyPrefix}${runId}`, '');
        if (initialFilter.trim()) {
          historyUrl += `?filter=${encodeURIComponent(initialFilter)}`;
        }
        let historyResponse = await apiFetch(historyUrl, apiToken);
        if (historyResponse.status === 400 && initialFilter.trim()) {
          // Malformed filter — retry without it so the timeline
          // still loads. The inline filter input will show the
          // parse error from compileFilter.
          historyResponse = await apiFetch(
            `/v1/runs/${runId}/events/history`,
            apiToken,
          );
        }
        let afterSeq = -1;
        if (historyResponse.ok) {
          const history = await parseJsonResponse(
            StreamedEventArraySchema,
            historyResponse,
            `GET /v1/runs/${runId}/events/history`,
          );
          setEvents(history);
          setSelectedSeq(history.at(-1)?.seq ?? null);
          afterSeq = history.at(-1)?.seq ?? -1;
          // Q3.7.3: seed the seq ref with the initial-history high-watermark
          // so a reconnect that fires before the first live event still
          // resumes from the right point.
          latestSeqRef.current = afterSeq;
        }

        // P10.13.1: the `/v1/runs/{id}/graph` fetch + the
        // mermaid-preview block it fed were removed alongside the
        // `agentflow-viz` crate deletion. The node grid below now
        // derives entirely from observed events.

        await connectStream(afterSeq);
        if (!cancelled) {
          setState('closed');
          void apiFetch(`/v1/runs/${runId}`, apiToken)
            .then((latest) => (latest.ok ? latest.json() : null))
            .then((latest: RunEnvelope | null) => {
              if (latest) {
                setActiveRun(runFromEnvelope(latest));
              }
            });
        }
      } catch (err) {
        if (cancelled || (err instanceof DOMException && err.name === 'AbortError')) {
          return;
        }
        setError(err instanceof Error ? err.message : String(err));
        // Q3.7.3: kick off the indefinite-with-backoff reconnect loop.
        // The previous code did a single 1200ms retry and then surrendered
        // to the `error` state; with this change a transient network
        // blip self-recovers without operator intervention.
        scheduleReconnect();
      }
    };

    void connect();

    return () => {
      cancelled = true;
      abortRef.current?.abort();
      if (reconnectTimer) {
        window.clearTimeout(reconnectTimer);
      }
    };
    // events is intentionally not a dependency; reconnect uses the snapshot from this connection attempt.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runId, state, apiToken]);

  const connectExisting = (event: React.FormEvent) => {
    event.preventDefault();
    if (!runId.trim()) {
      return;
    }
    setError(null);
    setState('loading');
  };

  const submitRun = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!workflowDraft.trim()) {
      setError('workflow YAML is required');
      return;
    }
    setSubmitState('submitting');
    setError(null);
    try {
      const response = await apiFetch('/v1/runs', apiToken, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tenant_id: tenantId.trim() || 'default',
          workflow: workflowDraft,
        }),
      });
      if (!response.ok) {
        throw new Error(`run submission failed with HTTP ${response.status}`);
      }
      const payload = await parseJsonResponse(
        CreateRunEnvelopeSchema,
        response,
        'POST /v1/runs (resubmit)',
      );
      setRunId(payload.run_id);
      setState('loading');
      await loadRuns(tenantId.trim() || 'default');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitState('idle');
    }
  };

  const cancelActiveRun = async () => {
    if (!activeRun || isTerminalRun(activeRun)) {
      return;
    }
    setSubmitState('cancelling');
    setError(null);
    try {
      const response = await apiFetch(`/v1/runs/${activeRun.id}:cancel`, apiToken, { method: 'POST' });
      if (!response.ok) {
        throw new Error(`run cancellation failed with HTTP ${response.status}`);
      }
      const payload = await parseJsonResponse(
        CancelRunEnvelopeSchema,
        response,
        `POST /v1/runs/${activeRun.id}:cancel`,
      );
      setActiveRun(payload.run);
      setState('closed');
      abortRef.current?.abort();
      await loadRuns(payload.run.tenant_id ?? tenantId);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitState('idle');
    }
  };

  const refreshActiveRun = () => {
    if (!runId.trim()) {
      return;
    }
    setState('loading');
  };

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AgentFlow</p>
          <h1>Run Console</h1>
        </div>
        <form className="run-form" onSubmit={connectExisting}>
          <input
            aria-label="Run ID"
            value={runId}
            onChange={(event) => setRunId(event.target.value)}
            placeholder="Run ID"
          />
          <button type="submit">Connect</button>
          <button disabled={!activeRun || isTerminalRun(activeRun) || submitState === 'cancelling'} type="button" onClick={cancelActiveRun}>
            Cancel
          </button>
          <a className="topbar-link" href="/ui/runs/new">
            New run →
          </a>
        </form>
      </header>

      <section className="status-strip" aria-label="Run status">
        <div>
          <span>Stream</span>
          <strong>{state}</strong>
        </div>
        <div>
          <span>Status</span>
          <strong>{activeRun?.status ?? 'none'}</strong>
        </div>
        <div>
          <span>Tenant</span>
          <strong>{activeRun?.tenant_id ?? tenantId}</strong>
        </div>
        <div>
          <span>Events</span>
          <strong>{events.length}</strong>
        </div>
        <div>
          <span>Reconnect</span>
          <strong>{lastReconnect ?? 'none'}</strong>
        </div>
      </section>

      {error ? <p className="error-line">{error}</p> : null}

      <section className="workspace">
        <aside className="run-pane">
          <div className="pane-heading">
            <span>Runs</span>
            <button type="button" onClick={refreshActiveRun}>Refresh</button>
          </div>
          <ol className="run-list">
            {runs.map((run) => (
              <li key={run.id}>
                <button
                  className={run.id === runId ? 'selected' : ''}
                  type="button"
                  onClick={() => {
                    setRunId(run.id);
                    setState('loading');
                  }}
                >
                  <span>{run.workflow.split('\n')[0] || run.id}</span>
                  <small>
                    {run.status} · {formatTime(run.started_at)}
                  </small>
                </button>
              </li>
            ))}
          </ol>
          <form className="submission-form" onSubmit={submitRun}>
            <label>
              <span>Tenant</span>
              <input value={tenantId} onChange={(event) => setTenantId(event.target.value)} />
            </label>
            <label>
              <span>API token</span>
              <input
                autoComplete="off"
                type="password"
                value={apiToken}
                onChange={(event) => onTokenChange(event.target.value)}
                placeholder="Bearer token"
              />
            </label>
            <label className="workflow-field">
              <span>Workflow YAML</span>
              <textarea value={workflowDraft} onChange={(event) => setWorkflowDraft(event.target.value)} />
            </label>
            <button disabled={submitState === 'submitting'} type="submit">
              {submitState === 'submitting' ? 'Submitting' : 'Submit run'}
            </button>
          </form>
        </aside>

        <section className="graph-pane" aria-label="DAG status">
          <div className="pane-heading">
            <span>DAG</span>
            <strong>{nodeSummaries.length} nodes</strong>
          </div>
          <div className="node-grid">
            {nodeSummaries.length === 0 ? (
              <div className="empty-node">Waiting for events</div>
            ) : (
              nodeSummaries.map((node) => (
                <button
                  className={`node node-${node.tone}`}
                  key={node.name}
                  type="button"
                  onClick={() => {
                    const match = findLatest(events, (event) => eventNodeId(event) === node.name);
                    setSelectedSeq(match?.seq ?? null);
                  }}
                >
                  <span>{node.label}</span>
                  <small>{node.status}</small>
                </button>
              ))
            )}
          </div>
        </section>

        <aside className="timeline-pane" aria-label="Agent timeline">
          <div className="pane-heading">
            <span>Timeline</span>
            <strong>
              {selectedEvent ? `#${selectedEvent.seq}` : '-'}
              {eventFilterExpr.trim() && eventFilter.predicate ? ` (${filteredEvents.length}/${events.length})` : ''}
            </strong>
          </div>
          <div className="event-filter">
            <label htmlFor="event-filter-input">Filter</label>
            <input
              id="event-filter-input"
              type="text"
              placeholder="kind=tool_call_completed AND step>5"
              value={eventFilterExpr}
              onChange={(ev) => setEventFilterExpr(ev.target.value)}
              spellCheck={false}
              autoComplete="off"
            />
            {eventFilter.error && (
              <span className="event-filter-error" role="alert">
                {eventFilter.error}
              </span>
            )}
          </div>
          <ol className="timeline">
            {filteredEvents.map((event) => (
              <li key={event.seq}>
                <button
                  className={selectedSeq === event.seq ? 'selected' : ''}
                  type="button"
                  onClick={() => setSelectedSeq(event.seq)}
                >
                  <span className={`dot dot-${eventTone(event.kind)}`} />
                  <span>{event.kind}</span>
                  <time>{formatTime(event.ts as string | null | undefined)}</time>
                </button>
              </li>
            ))}
            {eventFilter.predicate && filteredEvents.length === 0 && events.length > 0 && (
              <li className="timeline-empty">No events match this filter.</li>
            )}
          </ol>
        </aside>
      </section>

      <section className="details-grid" aria-label="Run details">
        <section className="details-pane">
          <div className="pane-heading">
            <span>Provider / config</span>
            <strong>{providerEvents.length ? 'from events' : apiToken ? 'token set' : 'open / unset'}</strong>
          </div>
          <pre>{prettyJson({
            tenant_id: activeRun?.tenant_id ?? tenantId,
            run_dir: activeRun?.run_dir ?? null,
            auth: apiToken ? 'bearer token configured in browser' : 'no browser token configured',
            latest_provider_event: providerEvents.at(-1)?.payload ?? null,
          })}</pre>
        </section>

        <section className="details-pane">
          <div className="pane-heading">
            <span>DAG node</span>
            <strong>{selectedNode?.id ?? 'none'}</strong>
          </div>
          <pre>{selectedNode ? prettyJson(selectedNode) : 'Select a node event.'}</pre>
        </section>

        <section className="details-pane">
          <div className="pane-heading">
            <span>Agent / tool policy</span>
            <strong>{agentToolEvents.at(-1)?.kind ?? 'none'}</strong>
          </div>
          <pre>{agentToolEvents.at(-1) ? prettyJson(agentToolEvents.at(-1)) : 'No agent or tool events.'}</pre>
        </section>

        <section className="details-pane">
          <div className="pane-heading">
            <span>Event payload</span>
            <strong>{selectedEvent?.kind ?? 'none'}</strong>
          </div>
          <pre>{selectedEvent ? prettyJson(selectedEvent.payload) : 'Select an event.'}</pre>
        </section>
      </section>
    </main>
  );
}


// `DiagnosticsPanel` moved to `./pages/DiagnosticsPanel.tsx` (Q3.7.1).

// ─── Top-level router ────────────────────────────────────────────

function App() {
  const [pathname, setPathname] = useState(() => window.location.pathname);
  // Q1.9.1: token comes from sessionStorage (tab-scoped) instead of
  // localStorage. Operators who close the tab re-enter it; an XSS
  // payload that fires after the operator left no longer finds it.
  const [apiToken, setApiToken] = useState(() => readSessionStorage(tokenKey, ''));

  useEffect(() => {
    const handler = () => setPathname(window.location.pathname);
    window.addEventListener('popstate', handler);
    return () => window.removeEventListener('popstate', handler);
  }, []);

  useEffect(() => {
    writeSessionStorage(tokenKey, apiToken);
  }, [apiToken]);

  if (pathname === '/ui/runs/new') {
    return <RunCreateForm apiToken={apiToken} onTokenChange={setApiToken} />;
  }
  if (pathname === '/ui/diagnostics' || pathname === '/ui/diagnostics/') {
    return <DiagnosticsPanel apiToken={apiToken} onTokenChange={setApiToken} />;
  }
  if (pathname === '/ui/harness/sessions' || pathname === '/ui/harness/sessions/') {
    return <HarnessSessionList apiToken={apiToken} onTokenChange={setApiToken} />;
  }
  if (pathname === '/ui/harness/sessions/new') {
    return <HarnessSubmitForm apiToken={apiToken} onTokenChange={setApiToken} />;
  }
  const harnessId = harnessSessionIdFromPath();
  if (harnessId) {
    return (
      <HarnessSessionDetail
        sessionId={harnessId}
        apiToken={apiToken}
        onTokenChange={setApiToken}
      />
    );
  }
  // P6.3: /ui/runs/<id>/compare?against=<other>
  const compareMatch = pathname.match(/^\/ui\/runs\/([^/]+)\/compare\/?$/);
  if (compareMatch) {
    return (
      <RunCompare
        primaryRunId={decodeURIComponent(compareMatch[1])}
        apiToken={apiToken}
        onTokenChange={setApiToken}
      />
    );
  }
  return <RunConsole apiToken={apiToken} onTokenChange={setApiToken} />;
}

const container = document.getElementById('agentflow-debugger');
if (container) {
  // Q3.7.1: ErrorBoundary catches unhandled throws inside `<App />` —
  // a malformed payload that slips past zod (Q3.7.2), a stale property
  // access after a schema migration, or any other runtime crash — and
  // shows a diagnostic panel with the error message + reset / reload
  // controls. Without the wrap, any such throw white-screens the page
  // with no URL change to indicate what happened.
  createRoot(container).render(
    <ErrorBoundary>
      <App />
    </ErrorBoundary>,
  );
}
