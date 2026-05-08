import React, { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';

type RunRecord = {
  id: string;
  workflow: string;
  status: string;
  tenant_id?: string;
  started_at?: string;
  finished_at?: string | null;
  error?: string | null;
};

type RunEnvelope = RunRecord & {
  run?: RunRecord;
};

type ListRunsEnvelope = {
  runs: RunRecord[];
};

type StreamedEvent = {
  run_id: string;
  seq: number;
  kind: string;
  payload: unknown;
  ts: string;
};

type VisualNode = {
  id: string;
  label?: string;
  status?: string;
};

type RunGraph = {
  graph: {
    nodes?: VisualNode[];
  };
  mermaid: string;
  active_node?: string | null;
};

type ConnectionState = 'idle' | 'loading' | 'streaming' | 'closed' | 'error';

const formatTime = (value?: string | null) => {
  if (!value) {
    return 'pending';
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(value));
};

const runFromEnvelope = (value: RunEnvelope): RunRecord => value.run ?? value;

const eventTone = (kind: string) => {
  const lower = kind.toLowerCase();
  if (lower.includes('fail') || lower.includes('error')) {
    return 'danger';
  }
  if (lower.includes('tool')) {
    return 'tool';
  }
  if (lower.includes('agent') || lower.includes('reflect') || lower.includes('plan')) {
    return 'agent';
  }
  if (lower.includes('complete') || lower.includes('succeed')) {
    return 'success';
  }
  return 'neutral';
};

const prettyJson = (value: unknown) => JSON.stringify(value, null, 2);

const knownSseEventKinds = [
  'run_started',
  'run_completed',
  'workflow.started',
  'workflow.completed',
  'workflow.failed',
  'node.started',
  'node.completed',
  'node.output.captured',
  'node.failed',
  'node.skipped',
  'checkpoint.saved',
  'checkpoint.restored',
  'retry.attempt',
  'resource.warning',
  'llm.prompt.sent',
  'llm.response.received',
  'StepStarted',
  'StepCompleted',
  'ToolCallStarted',
  'ToolCallCompleted',
  'ToolPolicyDecision',
  'ToolCapabilityDecision',
  'ReflectionAdded',
  'HandoffOccurred',
  'BlackboardWritten',
  'DebateRoundStarted',
  'DebateVerdictRendered',
  'RunStarted',
  'RunStopped',
];

const findLatest = <T,>(items: T[], predicate: (item: T) => boolean) => {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (predicate(items[index])) {
      return items[index];
    }
  }
  return undefined;
};

function App() {
  const [runId, setRunId] = useState('');
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const [activeRun, setActiveRun] = useState<RunRecord | null>(null);
  const [runGraph, setRunGraph] = useState<RunGraph | null>(null);
  const [events, setEvents] = useState<StreamedEvent[]>([]);
  const [selectedSeq, setSelectedSeq] = useState<number | null>(null);
  const [state, setState] = useState<ConnectionState>('idle');
  const [error, setError] = useState<string | null>(null);

  const selectedEvent = useMemo(
    () => events.find((event) => event.seq === selectedSeq) ?? events.at(-1) ?? null,
    [events, selectedSeq],
  );

  const nodeSummaries = useMemo(() => {
    if (runGraph?.graph.nodes?.length) {
      return runGraph.graph.nodes.map((node) => ({
        name: node.id,
        status: node.status ?? 'pending',
        tone: eventTone(node.status ?? 'pending'),
      }));
    }
    const seen = new Map<string, { name: string; status: string; tone: string }>();
    for (const event of events) {
      const payload = event.payload as Record<string, unknown>;
      const name =
        String(payload.node_id ?? payload.node_name ?? payload.node ?? payload.step ?? event.kind).trim() ||
        event.kind;
      seen.set(name, {
        name,
        status: event.kind,
        tone: eventTone(event.kind),
      });
    }
    return Array.from(seen.values()).slice(-8);
  }, [events, runGraph]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const id = params.get('run');
    if (id) {
      setRunId(id);
    }
  }, []);

  useEffect(() => {
    const loadRuns = async () => {
      try {
        const response = await fetch('/v1/runs?limit=20');
        if (!response.ok) {
          return;
        }
        const payload = (await response.json()) as ListRunsEnvelope;
        setRuns(payload.runs);
        if (!runId && payload.runs[0]) {
          setRunId(payload.runs[0].id);
          setState('loading');
        }
      } catch {
        // The UI can still connect by explicit run id when the list route is
        // unavailable or auth denies the request.
      }
    };
    void loadRuns();
  }, [runId]);

  useEffect(() => {
    if (!runId || state !== 'loading') {
      return undefined;
    }

    let cancelled = false;
    let source: EventSource | null = null;

    const connect = async () => {
      try {
        const response = await fetch(`/v1/runs/${runId}`);
        if (!response.ok) {
          throw new Error(`run lookup failed with HTTP ${response.status}`);
        }
        const payload = (await response.json()) as RunEnvelope;
        if (cancelled) {
          return;
        }
        setActiveRun(runFromEnvelope(payload));
        setRunGraph(null);
        setEvents([]);
        setSelectedSeq(null);
        setState('streaming');
        window.history.replaceState(null, '', `/ui?run=${encodeURIComponent(runId)}`);

        const historyResponse = await fetch(`/v1/runs/${runId}/events/history`);
        if (historyResponse.ok) {
          const history = (await historyResponse.json()) as StreamedEvent[];
          setEvents(history);
          setSelectedSeq(history.at(-1)?.seq ?? null);
        }

        const graphResponse = await fetch(`/v1/runs/${runId}/graph`);
        if (graphResponse.ok) {
          setRunGraph((await graphResponse.json()) as RunGraph);
        }

        source = new EventSource(`/v1/runs/${runId}/events`);
        const handleMessage = (message: MessageEvent<string>) => {
          const event = JSON.parse(message.data) as StreamedEvent;
          setEvents((current) => {
            if (current.some((item) => item.seq === event.seq)) {
              return current;
            }
            return [...current, event].sort((left, right) => left.seq - right.seq);
          });
          setSelectedSeq((current) => current ?? event.seq);
        };
        source.onmessage = handleMessage;
        for (const kind of knownSseEventKinds) {
          source.addEventListener(kind, handleMessage);
        }
        source.onerror = () => {
          source?.close();
          setState((current) => (current === 'streaming' ? 'closed' : current));
        };
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setState('error');
        }
      }
    };

    void connect();

    return () => {
      cancelled = true;
      source?.close();
    };
  }, [runId, state]);

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!runId.trim()) {
      return;
    }
    setError(null);
    setState('loading');
  };

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AgentFlow</p>
          <h1>Hybrid Run Debugger</h1>
        </div>
        <form className="run-form" onSubmit={submit}>
          <input
            aria-label="Run ID"
            value={runId}
            onChange={(event) => setRunId(event.target.value)}
            placeholder="Run ID"
          />
          <button type="submit">Connect</button>
        </form>
      </header>

      <section className="status-strip" aria-label="Run status">
        <div>
          <span>State</span>
          <strong>{state}</strong>
        </div>
        <div>
          <span>Status</span>
          <strong>{activeRun?.status ?? 'none'}</strong>
        </div>
        <div>
          <span>Tenant</span>
          <strong>{activeRun?.tenant_id ?? 'default'}</strong>
        </div>
        <div>
          <span>Events</span>
          <strong>{events.length}</strong>
        </div>
      </section>

      {error ? <p className="error-line">{error}</p> : null}

      <section className="workspace">
        <aside className="run-pane">
          <div className="pane-heading">
            <span>Runs</span>
            <strong>{activeRun ? formatTime(activeRun.started_at) : '-'}</strong>
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
          <pre className="workflow-preview">{activeRun?.workflow ?? 'No run loaded.'}</pre>
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
                    const match = findLatest(events, (event) => {
                      const payload = event.payload as Record<string, unknown>;
                      return (
                        String(payload.node_id ?? payload.node_name ?? payload.node ?? payload.step ?? event.kind) ===
                        node.name
                      );
                    });
                    setSelectedSeq(match?.seq ?? null);
                  }}
                >
                  <span>{node.name}</span>
                  <small>{node.status}</small>
                </button>
              ))
            )}
          </div>
          {runGraph ? <pre className="mermaid-preview">{runGraph.mermaid}</pre> : null}
        </section>

        <aside className="timeline-pane" aria-label="Agent timeline">
          <div className="pane-heading">
            <span>Timeline</span>
            <strong>{selectedEvent ? `#${selectedEvent.seq}` : '-'}</strong>
          </div>
          <ol className="timeline">
            {events.map((event) => (
              <li key={event.seq}>
                <button
                  className={selectedSeq === event.seq ? 'selected' : ''}
                  type="button"
                  onClick={() => setSelectedSeq(event.seq)}
                >
                  <span className={`dot dot-${eventTone(event.kind)}`} />
                  <span>{event.kind}</span>
                  <time>{formatTime(event.ts)}</time>
                </button>
              </li>
            ))}
          </ol>
        </aside>
      </section>

      <section className="details-pane" aria-label="Tool call details">
        <div className="pane-heading">
          <span>Details</span>
          <strong>{selectedEvent?.kind ?? 'none'}</strong>
        </div>
        <pre>{selectedEvent ? prettyJson(selectedEvent.payload) : 'Select an event.'}</pre>
      </section>
    </main>
  );
}

createRoot(document.getElementById('agentflow-debugger') as HTMLElement).render(<App />);
