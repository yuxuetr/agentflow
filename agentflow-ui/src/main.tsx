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
  return <RunConsole apiToken={apiToken} onTokenChange={setApiToken} />;
}

const container = document.getElementById('agentflow-debugger');
if (container) {
  createRoot(container).render(<App />);
}
