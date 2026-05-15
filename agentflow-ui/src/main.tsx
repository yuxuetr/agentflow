import React, { useEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';

type RunRecord = {
  id: string;
  workflow: string;
  status: string;
  tenant_id?: string;
  started_at?: string;
  finished_at?: string | null;
  run_dir?: string | null;
  error?: string | null;
};

type RunEnvelope = RunRecord & {
  run?: RunRecord;
};

type ListRunsEnvelope = {
  runs: RunRecord[];
};

type CreateRunEnvelope = {
  run_id: string;
  status: string;
};

type CancelRunEnvelope = {
  run: RunRecord;
  cancelled: boolean;
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

type ConnectionState = 'idle' | 'loading' | 'streaming' | 'reconnecting' | 'closed' | 'error';

const tokenKey = 'agentflow.ui.apiToken';
const workflowKey = 'agentflow.ui.workflowDraft';
const tenantKey = 'agentflow.ui.tenantId';
// P6.1 — last-used inputs for the create form. NEVER persists the API token.
const newFormWorkflowKey = 'agentflow.ui.newForm.workflow';
const newFormTenantKey = 'agentflow.ui.newForm.tenant';
const newFormProfileKey = 'agentflow.ui.newForm.profile';
const newFormInputsKey = 'agentflow.ui.newForm.inputs';

const starterWorkflow = `name: web-ui-console-smoke
version: "1.0"
nodes:
  hello:
    type: template
    template: "hello from the run console"
outputs:
  message: "{{ hello.output }}"`;

const createFormStarterWorkflow = `name: my-new-run
version: "1.0"
nodes:
  greet:
    type: template
    template: "hello {{ name }}"
outputs:
  message: "{{ greet.output }}"`;

const createFormStarterInputs = `{
  "name": "world"
}`;

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
  if (lower.includes('fail') || lower.includes('error') || lower.includes('denied')) {
    return 'danger';
  }
  if (lower.includes('tool') || lower.includes('policy') || lower.includes('capability')) {
    return 'tool';
  }
  if (lower.includes('agent') || lower.includes('reflect') || lower.includes('plan') || lower.includes('step')) {
    return 'agent';
  }
  if (lower.includes('complete') || lower.includes('succeed')) {
    return 'success';
  }
  return 'neutral';
};

const prettyJson = (value: unknown) => JSON.stringify(value, null, 2);

const isTerminalRun = (run: RunRecord | null) =>
  run ? ['succeeded', 'failed', 'cancelled'].includes(run.status) : true;

const readStorage = (key: string, fallback: string) => {
  try {
    return window.localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};

const writeStorage = (key: string, value: string) => {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Storage is best-effort; the console still works without it.
  }
};

const findLatest = <T,>(items: T[], predicate: (item: T) => boolean) => {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (predicate(items[index])) {
      return items[index];
    }
  }
  return undefined;
};

const eventNodeId = (event: StreamedEvent) => {
  const payload = event.payload as Record<string, unknown>;
  return String(payload.node_id ?? payload.node_name ?? payload.node ?? payload.step ?? '').trim();
};

const apiFetch = (path: string, token: string, init: RequestInit = {}) => {
  const headers = new Headers(init.headers);
  const trimmed = token.trim();
  if (trimmed) {
    headers.set('Authorization', `Bearer ${trimmed}`);
  }
  return fetch(path, {
    ...init,
    headers,
  });
};

const parseSseChunk = (buffer: string) => {
  const events: StreamedEvent[] = [];
  let cursor = buffer;
  let boundary = cursor.indexOf('\n\n');
  while (boundary >= 0) {
    const raw = cursor.slice(0, boundary);
    cursor = cursor.slice(boundary + 2);
    const data = raw
      .split('\n')
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice(5).trimStart())
      .join('\n');
    if (data) {
      events.push(JSON.parse(data) as StreamedEvent);
    }
    boundary = cursor.indexOf('\n\n');
  }
  return { events, rest: cursor };
};

// ─── P6.1 Run creation form ───────────────────────────────────────

type CreateProfile = 'dev' | 'local' | 'production';

const CREATE_PROFILES: CreateProfile[] = ['dev', 'local', 'production'];

const parseInputsBlock = (raw: string): { ok: true; value: unknown } | { ok: false; error: string } => {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { ok: true, value: null };
  }
  try {
    return { ok: true, value: JSON.parse(trimmed) };
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) };
  }
};

const lineCount = (text: string) => Math.max(1, text.split('\n').length);

function RunCreateForm({ apiToken, onTokenChange }: { apiToken: string; onTokenChange: (token: string) => void }) {
  const [tenantId, setTenantId] = useState(() => readStorage(newFormTenantKey, 'default'));
  const [profile, setProfile] = useState<CreateProfile>(() => {
    const value = readStorage(newFormProfileKey, 'local');
    return (CREATE_PROFILES as string[]).includes(value) ? (value as CreateProfile) : 'local';
  });
  const [workflowYaml, setWorkflowYaml] = useState(() =>
    readStorage(newFormWorkflowKey, createFormStarterWorkflow),
  );
  const [inputsJson, setInputsJson] = useState(() =>
    readStorage(newFormInputsKey, createFormStarterInputs),
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [info, setInfo] = useState<string | null>(null);

  // Persist last-used inputs (everything except the API token).
  useEffect(() => {
    writeStorage(newFormTenantKey, tenantId);
  }, [tenantId]);
  useEffect(() => {
    writeStorage(newFormProfileKey, profile);
  }, [profile]);
  useEffect(() => {
    writeStorage(newFormWorkflowKey, workflowYaml);
  }, [workflowYaml]);
  useEffect(() => {
    writeStorage(newFormInputsKey, inputsJson);
  }, [inputsJson]);

  const yamlValidation = useMemo(() => {
    const trimmed = workflowYaml.trim();
    if (!trimmed) {
      return { ok: false, message: 'workflow YAML is required' };
    }
    // Minimal pre-flight: structural checks the server also enforces,
    // surfaced client-side for snappier feedback.
    if (!/^\s*name\s*:/m.test(trimmed)) {
      return { ok: false, message: "workflow must define a top-level 'name:' field" };
    }
    if (!/^\s*nodes\s*:/m.test(trimmed)) {
      return { ok: false, message: "workflow must define a top-level 'nodes:' map" };
    }
    return { ok: true as const, message: 'looks like a workflow YAML (basic checks)' };
  }, [workflowYaml]);

  const inputsValidation = useMemo(() => parseInputsBlock(inputsJson), [inputsJson]);

  const handleFilePick = async (event: React.ChangeEvent<HTMLInputElement>, target: 'workflow' | 'inputs') => {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }
    try {
      const text = await file.text();
      if (target === 'workflow') {
        setWorkflowYaml(text);
      } else {
        setInputsJson(text);
      }
      setInfo(`loaded ${file.name} into ${target}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      // Reset so the same filename can be re-picked.
      event.target.value = '';
    }
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    setInfo(null);
    if (!yamlValidation.ok) {
      setError(yamlValidation.message);
      return;
    }
    if (!inputsValidation.ok) {
      setError(`inputs JSON is invalid: ${inputsValidation.error}`);
      return;
    }
    setBusy(true);
    try {
      const body: Record<string, unknown> = {
        tenant_id: tenantId.trim() || 'default',
        workflow: workflowYaml,
        profile,
      };
      if (inputsValidation.value && typeof inputsValidation.value === 'object') {
        body.inputs = inputsValidation.value;
      }
      const response = await apiFetch('/v1/runs', apiToken, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`run submission failed with HTTP ${response.status}: ${text}`);
      }
      const payload = (await response.json()) as CreateRunEnvelope;
      // Stash the freshly-submitted workflow into the console's draft
      // slot so the run detail view can reuse it.
      writeStorage(workflowKey, workflowYaml);
      writeStorage(tenantKey, tenantId);
      // Redirect to the run detail view (`?run=<id>` parses on mount).
      window.location.assign(`/ui?run=${encodeURIComponent(payload.run_id)}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="shell create-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AgentFlow</p>
          <h1>Submit a run</h1>
        </div>
        <nav>
          <a className="topbar-link" href="/ui">
            ← Run console
          </a>
        </nav>
      </header>

      {error ? <p className="error-line">{error}</p> : null}
      {info ? <p className="info-line">{info}</p> : null}

      <form className="create-form" onSubmit={submit}>
        <section className="create-row">
          <label>
            <span>Tenant</span>
            <input
              data-testid="create-tenant"
              value={tenantId}
              onChange={(event) => setTenantId(event.target.value)}
              placeholder="default"
            />
          </label>
          <label>
            <span>Profile</span>
            <select
              data-testid="create-profile"
              value={profile}
              onChange={(event) => setProfile(event.target.value as CreateProfile)}
            >
              {CREATE_PROFILES.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>API token</span>
            <input
              data-testid="create-token"
              autoComplete="off"
              type="password"
              value={apiToken}
              onChange={(event) => onTokenChange(event.target.value)}
              placeholder="Bearer token (not persisted)"
            />
          </label>
        </section>

        <section className="create-editor" aria-label="Workflow YAML editor">
          <div className="pane-heading">
            <span>Workflow YAML</span>
            <span className={`validation ${yamlValidation.ok ? 'validation-ok' : 'validation-err'}`}>
              {yamlValidation.message}
            </span>
          </div>
          <div className="editor-actions">
            <label className="file-pick">
              Load from file…
              <input
                type="file"
                accept=".yaml,.yml,.txt,text/yaml,application/yaml"
                onChange={(event) => handleFilePick(event, 'workflow')}
              />
            </label>
            <span className="line-meter">lines: {lineCount(workflowYaml)}</span>
          </div>
          <textarea
            data-testid="create-workflow"
            className="code-editor code-editor-yaml"
            spellCheck={false}
            value={workflowYaml}
            onChange={(event) => setWorkflowYaml(event.target.value)}
            rows={18}
          />
        </section>

        <section className="create-editor" aria-label="Inputs JSON editor">
          <div className="pane-heading">
            <span>Inputs (JSON, optional)</span>
            <span className={`validation ${inputsValidation.ok ? 'validation-ok' : 'validation-err'}`}>
              {inputsValidation.ok ? 'valid JSON or empty' : inputsValidation.error}
            </span>
          </div>
          <div className="editor-actions">
            <label className="file-pick">
              Load from file…
              <input
                type="file"
                accept=".json,application/json"
                onChange={(event) => handleFilePick(event, 'inputs')}
              />
            </label>
            <span className="line-meter">lines: {lineCount(inputsJson)}</span>
          </div>
          <textarea
            data-testid="create-inputs"
            className="code-editor code-editor-json"
            spellCheck={false}
            value={inputsJson}
            onChange={(event) => setInputsJson(event.target.value)}
            rows={8}
          />
        </section>

        <footer className="create-actions">
          <button
            data-testid="create-submit"
            disabled={busy || !yamlValidation.ok || !inputsValidation.ok}
            type="submit"
          >
            {busy ? 'Submitting…' : 'Submit run'}
          </button>
          <small>
            Inputs persist in localStorage (tenant, profile, workflow, inputs). The API token is never saved.
          </small>
        </footer>
      </form>
    </main>
  );
}

// ─── Existing run console ────────────────────────────────────────

function RunConsole({ apiToken, onTokenChange }: { apiToken: string; onTokenChange: (token: string) => void }) {
  const [runId, setRunId] = useState('');
  const [tenantId, setTenantId] = useState(() => readStorage(tenantKey, 'default'));
  const [workflowDraft, setWorkflowDraft] = useState(() => readStorage(workflowKey, starterWorkflow));
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const [activeRun, setActiveRun] = useState<RunRecord | null>(null);
  const [runGraph, setRunGraph] = useState<RunGraph | null>(null);
  const [events, setEvents] = useState<StreamedEvent[]>([]);
  const [selectedSeq, setSelectedSeq] = useState<number | null>(null);
  const [state, setState] = useState<ConnectionState>('idle');
  const [error, setError] = useState<string | null>(null);
  const [submitState, setSubmitState] = useState<'idle' | 'submitting' | 'cancelling'>('idle');
  const [lastReconnect, setLastReconnect] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const selectedEvent = useMemo(
    () => events.find((event) => event.seq === selectedSeq) ?? events.at(-1) ?? null,
    [events, selectedSeq],
  );

  const nodeSummaries = useMemo(() => {
    if (runGraph?.graph.nodes?.length) {
      return runGraph.graph.nodes.map((node) => ({
        name: node.id,
        label: node.label ?? node.id,
        status: node.status ?? 'pending',
        tone: eventTone(node.status ?? 'pending'),
      }));
    }
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
  }, [events, runGraph]);

  const selectedNode = useMemo(() => {
    if (!selectedEvent) {
      return null;
    }
    const nodeId = eventNodeId(selectedEvent);
    if (!nodeId) {
      return null;
    }
    const node = runGraph?.graph.nodes?.find((item) => item.id === nodeId);
    return {
      id: nodeId,
      label: node?.label ?? nodeId,
      status: node?.status ?? selectedEvent.kind,
      event: selectedEvent,
    };
  }, [runGraph, selectedEvent]);

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
  }, [tenantId]);

  useEffect(() => {
    writeStorage(workflowKey, workflowDraft);
  }, [workflowDraft]);

  const loadRuns = async (nextTenant = tenantId) => {
    const response = await apiFetch(`/v1/runs?tenant_id=${encodeURIComponent(nextTenant)}&limit=20`, apiToken);
    if (!response.ok) {
      throw new Error(`run list failed with HTTP ${response.status}`);
    }
    const payload = (await response.json()) as ListRunsEnvelope;
    setRuns(payload.runs);
    if (!runId && payload.runs[0]) {
      setRunId(payload.runs[0].id);
      setState('loading');
    }
  };

  useEffect(() => {
    void loadRuns().catch(() => {
      // Explicit run connection still works when the list is unavailable.
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apiToken, tenantId]);

  useEffect(() => {
    if (!runId || state !== 'loading') {
      return undefined;
    }

    let cancelled = false;
    let reconnectTimer: number | undefined;
    abortRef.current?.abort();

    const appendEvent = (event: StreamedEvent) => {
      setEvents((current) => {
        if (current.some((item) => item.seq === event.seq)) {
          return current;
        }
        return [...current, event].sort((left, right) => left.seq - right.seq);
      });
      setSelectedSeq((current) => current ?? event.seq);
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
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      let buffer = '';
      while (!cancelled) {
        const { done, value } = await reader.read();
        if (done) {
          break;
        }
        buffer += value;
        const parsed = parseSseChunk(buffer);
        buffer = parsed.rest;
        for (const event of parsed.events) {
          appendEvent(event);
        }
      }
    };

    const connect = async () => {
      try {
        const response = await apiFetch(`/v1/runs/${runId}`, apiToken);
        if (!response.ok) {
          throw new Error(`run lookup failed with HTTP ${response.status}`);
        }
        const payload = (await response.json()) as RunEnvelope;
        if (cancelled) {
          return;
        }
        const nextRun = runFromEnvelope(payload);
        setActiveRun(nextRun);
        setRunGraph(null);
        setEvents([]);
        setSelectedSeq(null);
        setError(null);
        window.history.replaceState(null, '', `/ui?run=${encodeURIComponent(runId)}`);

        const historyResponse = await apiFetch(`/v1/runs/${runId}/events/history`, apiToken);
        let afterSeq = -1;
        if (historyResponse.ok) {
          const history = (await historyResponse.json()) as StreamedEvent[];
          setEvents(history);
          setSelectedSeq(history.at(-1)?.seq ?? null);
          afterSeq = history.at(-1)?.seq ?? -1;
        }

        const graphResponse = await apiFetch(`/v1/runs/${runId}/graph`, apiToken);
        if (graphResponse.ok) {
          setRunGraph((await graphResponse.json()) as RunGraph);
        }

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
        const lastSeq = events.at(-1)?.seq ?? -1;
        setLastReconnect(formatTime(new Date().toISOString()));
        setState('reconnecting');
        reconnectTimer = window.setTimeout(() => {
          if (!cancelled) {
            void connectStream(lastSeq).catch((streamErr) => {
              if (!cancelled) {
                setError(streamErr instanceof Error ? streamErr.message : String(streamErr));
                setState('error');
              }
            });
          }
        }, 1200);
        setError(err instanceof Error ? err.message : String(err));
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
      const payload = (await response.json()) as CreateRunEnvelope;
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
      const payload = (await response.json()) as CancelRunEnvelope;
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

// ─── P-H.5 slice 3 — Harness Mode Web UI ─────────────────────────

type HarnessSession = {
  id: string;
  tenant_id: string;
  status: string;
  user_input: string;
  workspace_root: string;
  profile: string;
  runtime_kind: string;
  model: string;
  skill_name?: string | null;
  started_at?: string;
  finished_at?: string | null;
  final_answer?: string | null;
  error?: string | null;
};

type HarnessEvent = {
  session_id: string;
  seq: number;
  kind: string;
  payload: unknown;
  ts: string;
};

type PendingApproval = {
  id: string;
  session_id: string;
  step_index: number;
  tool: string;
  source?: string | null;
  permissions?: string[];
  idempotency?: string;
  params_summary?: unknown;
  risk: string;
  reason: string;
  requested_at: string;
  expires_at?: string | null;
};

type ApprovalOutcome = 'allow' | 'deny' | 'deny_and_stop';
type ApprovalScope = 'once' | 'session' | 'run';

const harnessNewFormPromptKey = 'agentflow.ui.harness.newForm.user_input';
const harnessNewFormWorkspaceKey = 'agentflow.ui.harness.newForm.workspace_root';
const harnessNewFormProfileKey = 'agentflow.ui.harness.newForm.profile';
const harnessNewFormRuntimeKey = 'agentflow.ui.harness.newForm.runtime_kind';
const harnessNewFormModelKey = 'agentflow.ui.harness.newForm.model';
const harnessNewFormSkillKey = 'agentflow.ui.harness.newForm.skill_name';
const harnessNewFormTenantKey = 'agentflow.ui.harness.newForm.tenant_id';

const harnessStatusTone = (status: string): 'pending' | 'success' | 'danger' | 'neutral' => {
  switch (status) {
    case 'running':
      return 'pending';
    case 'completed':
      return 'success';
    case 'failed':
      return 'danger';
    case 'cancelled':
      return 'danger';
    default:
      return 'neutral';
  }
};

const isHarnessTerminal = (session: HarnessSession | null) =>
  session ? ['completed', 'failed', 'cancelled'].includes(session.status) : true;

const harnessSessionIdFromPath = () => {
  // `/ui/harness/sessions/<uuid>`. We extract the trailing segment
  // without depending on a router library — keeps the SPA dep tree
  // small and lets the server own the deep-link list.
  const match = window.location.pathname.match(/^\/ui\/harness\/sessions\/([^/]+)$/);
  if (!match) {
    return null;
  }
  const candidate = match[1];
  if (candidate === 'new' || candidate.length === 0) {
    return null;
  }
  return candidate;
};

function HarnessSessionList({
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
      const body = (await response.json()) as { sessions: HarnessSession[] };
      setSessions(body.sessions);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void refresh();
    const handle = window.setInterval(() => {
      void refresh();
    }, 4000);
    return () => window.clearInterval(handle);
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

const harnessFormStarterPrompt = '请用一句话总结当前工作区。';
const harnessFormStarterWorkspace = '/tmp';
const harnessFormStarterModel = 'moonshot-v1-auto';

type HarnessProfileChoice = 'dev' | 'local' | 'production';
const HARNESS_PROFILES: HarnessProfileChoice[] = ['dev', 'local', 'production'];

type HarnessRuntimeChoice = 'react' | 'plan_execute';
const HARNESS_RUNTIMES: HarnessRuntimeChoice[] = ['react', 'plan_execute'];

function HarnessSubmitForm({
  apiToken,
  onTokenChange,
}: {
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
  const [tenantId, setTenantId] = useState(() =>
    readStorage(harnessNewFormTenantKey, 'default'),
  );
  const [userInput, setUserInput] = useState(() =>
    readStorage(harnessNewFormPromptKey, harnessFormStarterPrompt),
  );
  const [workspaceRoot, setWorkspaceRoot] = useState(() =>
    readStorage(harnessNewFormWorkspaceKey, harnessFormStarterWorkspace),
  );
  const [profile, setProfile] = useState<HarnessProfileChoice>(() => {
    const value = readStorage(harnessNewFormProfileKey, 'local');
    return (HARNESS_PROFILES as string[]).includes(value)
      ? (value as HarnessProfileChoice)
      : 'local';
  });
  const [runtimeKind, setRuntimeKind] = useState<HarnessRuntimeChoice>(() => {
    const value = readStorage(harnessNewFormRuntimeKey, 'react');
    return (HARNESS_RUNTIMES as string[]).includes(value)
      ? (value as HarnessRuntimeChoice)
      : 'react';
  });
  const [model, setModel] = useState(() =>
    readStorage(harnessNewFormModelKey, harnessFormStarterModel),
  );
  const [skillName, setSkillName] = useState(() => readStorage(harnessNewFormSkillKey, ''));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    writeStorage(harnessNewFormTenantKey, tenantId);
  }, [tenantId]);
  useEffect(() => {
    writeStorage(harnessNewFormPromptKey, userInput);
  }, [userInput]);
  useEffect(() => {
    writeStorage(harnessNewFormWorkspaceKey, workspaceRoot);
  }, [workspaceRoot]);
  useEffect(() => {
    writeStorage(harnessNewFormProfileKey, profile);
  }, [profile]);
  useEffect(() => {
    writeStorage(harnessNewFormRuntimeKey, runtimeKind);
  }, [runtimeKind]);
  useEffect(() => {
    writeStorage(harnessNewFormModelKey, model);
  }, [model]);
  useEffect(() => {
    writeStorage(harnessNewFormSkillKey, skillName);
  }, [skillName]);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    const promptTrimmed = userInput.trim();
    if (!promptTrimmed) {
      setError('User prompt is required');
      return;
    }
    const workspaceTrimmed = workspaceRoot.trim();
    if (!workspaceTrimmed) {
      setError('Workspace root is required');
      return;
    }
    setBusy(true);
    try {
      const body: Record<string, unknown> = {
        user_input: promptTrimmed,
        workspace_root: workspaceTrimmed,
        tenant_id: tenantId.trim() || 'default',
        profile,
        runtime_kind: runtimeKind,
        model: model.trim() || harnessFormStarterModel,
      };
      const skillTrimmed = skillName.trim();
      if (skillTrimmed) {
        body.skill_name = skillTrimmed;
      }
      const response = await apiFetch('/v1/harness/sessions', apiToken, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`HTTP ${response.status}: ${text}`);
      }
      const payload = (await response.json()) as { session_id: string };
      window.location.assign(`/ui/harness/sessions/${encodeURIComponent(payload.session_id)}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="shell create-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AgentFlow / Harness</p>
          <h1>New session</h1>
        </div>
        <nav className="harness-nav">
          <a className="topbar-link" href="/ui/harness/sessions">
            ← Sessions
          </a>
        </nav>
      </header>

      {error ? <p className="error-line">{error}</p> : null}

      <form className="create-form" onSubmit={submit}>
        <section className="create-row">
          <label>
            <span>Tenant</span>
            <input
              data-testid="harness-new-tenant"
              value={tenantId}
              onChange={(event) => setTenantId(event.target.value)}
              placeholder="default"
            />
          </label>
          <label>
            <span>Profile</span>
            <select
              data-testid="harness-new-profile"
              value={profile}
              onChange={(event) => setProfile(event.target.value as HarnessProfileChoice)}
            >
              {HARNESS_PROFILES.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Runtime</span>
            <select
              data-testid="harness-new-runtime"
              value={runtimeKind}
              onChange={(event) => setRuntimeKind(event.target.value as HarnessRuntimeChoice)}
            >
              {HARNESS_RUNTIMES.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>API token</span>
            <input
              data-testid="harness-new-token"
              type="password"
              autoComplete="off"
              value={apiToken}
              onChange={(event) => onTokenChange(event.target.value)}
              placeholder="Bearer token (not persisted)"
            />
          </label>
        </section>

        <section className="create-row">
          <label className="harness-grow">
            <span>Model</span>
            <input
              data-testid="harness-new-model"
              value={model}
              onChange={(event) => setModel(event.target.value)}
              placeholder="moonshot-v1-auto"
            />
          </label>
          <label className="harness-grow">
            <span>Skill (optional)</span>
            <input
              data-testid="harness-new-skill"
              value={skillName}
              onChange={(event) => setSkillName(event.target.value)}
              placeholder="leave blank for no skill"
            />
          </label>
          <label className="harness-grow">
            <span>Workspace root</span>
            <input
              data-testid="harness-new-workspace"
              value={workspaceRoot}
              onChange={(event) => setWorkspaceRoot(event.target.value)}
              placeholder="/path/to/workspace"
            />
          </label>
        </section>

        <section className="create-editor" aria-label="User prompt editor">
          <div className="pane-heading">
            <span>Prompt</span>
          </div>
          <textarea
            data-testid="harness-new-prompt"
            className="code-editor"
            spellCheck={true}
            value={userInput}
            onChange={(event) => setUserInput(event.target.value)}
            rows={10}
          />
        </section>

        <footer className="create-actions">
          <button data-testid="harness-new-submit" disabled={busy} type="submit">
            {busy ? 'Submitting…' : 'Start session'}
          </button>
          <small>
            Inputs persist in localStorage (tenant, profile, runtime, model, skill, workspace, prompt).
            The API token is never saved.
          </small>
        </footer>
      </form>
    </main>
  );
}

function HarnessSessionDetail({
  sessionId,
  apiToken,
  onTokenChange,
}: {
  sessionId: string;
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
  const [session, setSession] = useState<HarnessSession | null>(null);
  const [events, setEvents] = useState<HarnessEvent[]>([]);
  const [approvals, setApprovals] = useState<PendingApproval[]>([]);
  const [selectedSeq, setSelectedSeq] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);

  const fetchSession = async () => {
    try {
      const response = await apiFetch(`/v1/harness/sessions/${sessionId}`, apiToken);
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`session fetch failed: HTTP ${response.status} ${text}`);
      }
      const body = (await response.json()) as HarnessSession;
      setSession(body);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const fetchEvents = async () => {
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}/events/history`,
        apiToken,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`events fetch failed: HTTP ${response.status} ${text}`);
      }
      const body = (await response.json()) as HarnessEvent[];
      setEvents(body);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const fetchApprovals = async () => {
    try {
      const response = await apiFetch(
        `/v1/harness/sessions/${sessionId}/approvals`,
        apiToken,
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`approvals fetch failed: HTTP ${response.status} ${text}`);
      }
      const body = (await response.json()) as { approvals: PendingApproval[] };
      setApprovals(body.approvals);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => {
    void fetchSession();
    void fetchEvents();
    void fetchApprovals();
    // Poll every 2s while running. Once terminal, slow to 10s so
    // operators can still refresh without hammering the gateway.
    const handle = window.setInterval(() => {
      void fetchSession();
      void fetchEvents();
      void fetchApprovals();
    }, 2000);
    return () => window.clearInterval(handle);
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
      );
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`decide failed: HTTP ${response.status} ${text}`);
      }
      setInfo(`Approval ${requestId} → ${decision}/${scope}`);
      // Refresh immediately so the approval clears without waiting
      // for the next poll tick.
      void fetchApprovals();
      void fetchEvents();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const cancel = async () => {
    setError(null);
    setInfo(null);
    try {
      const response = await apiFetch(`/v1/harness/sessions/${sessionId}:cancel`, apiToken, {
        method: 'POST',
      });
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`cancel failed: HTTP ${response.status} ${text}`);
      }
      setInfo('Cancel requested');
      void fetchSession();
      void fetchEvents();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
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
        <button
          data-testid="harness-detail-cancel"
          type="button"
          onClick={() => void cancel()}
          disabled={terminal}
        >
          {terminal ? 'Terminal' : 'Cancel session'}
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

// ─── Top-level router ────────────────────────────────────────────

function App() {
  const [pathname, setPathname] = useState(() => window.location.pathname);
  const [apiToken, setApiToken] = useState(() => readStorage(tokenKey, ''));

  useEffect(() => {
    const handler = () => setPathname(window.location.pathname);
    window.addEventListener('popstate', handler);
    return () => window.removeEventListener('popstate', handler);
  }, []);

  useEffect(() => {
    writeStorage(tokenKey, apiToken);
  }, [apiToken]);

  if (pathname === '/ui/runs/new') {
    return <RunCreateForm apiToken={apiToken} onTokenChange={setApiToken} />;
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
  return <RunConsole apiToken={apiToken} onTokenChange={setApiToken} />;
}

const container = document.getElementById('agentflow-debugger');
if (container) {
  createRoot(container).render(<App />);
}
