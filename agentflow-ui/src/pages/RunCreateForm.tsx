// /ui/runs/new — P6.1 run creation form. Persists tenant / profile /
// workflow / inputs to localStorage; the API token is intentionally
// never saved (Q1.9.1).

import React, { useEffect, useMemo, useState } from 'react';

import { apiFetch } from '../lib/api';
import {
  newFormInputsKey,
  newFormProfileKey,
  newFormTenantKey,
  newFormWorkflowKey,
  readStorage,
  tenantKey,
  workflowKey,
  writeStorage,
} from '../lib/storage';
import { CreateRunEnvelopeSchema, parseJsonResponse } from '../schemas';

const createFormStarterWorkflow = `name: my-new-run
nodes:
  - id: greet
    type: template
    parameters:
      template: "hello {{ name }}"`;

const createFormStarterInputs = `{
  "name": "world"
}`;

type CreateProfile = 'dev' | 'local' | 'production';

const CREATE_PROFILES: CreateProfile[] = ['dev', 'local', 'production'];

const parseInputsBlock = (
  raw: string,
): { ok: true; value: unknown } | { ok: false; error: string } => {
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

export function RunCreateForm({
  apiToken,
  onTokenChange,
}: {
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
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

  const handleFilePick = async (
    event: React.ChangeEvent<HTMLInputElement>,
    target: 'workflow' | 'inputs',
  ) => {
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
      const payload = await parseJsonResponse(
        CreateRunEnvelopeSchema,
        response,
        'POST /v1/runs',
      );
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
            <span
              className={`validation ${
                yamlValidation.ok ? 'validation-ok' : 'validation-err'
              }`}
            >
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
            <span
              className={`validation ${
                inputsValidation.ok ? 'validation-ok' : 'validation-err'
              }`}
            >
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
            Inputs persist in localStorage (tenant, profile, workflow, inputs). The API
            token is never saved.
          </small>
        </footer>
      </form>
    </main>
  );
}
