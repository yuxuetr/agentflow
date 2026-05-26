// /ui/harness/sessions/new — P-H.5 slice 3 create form. Persists
// tenant / profile / runtime / model / skill / workspace / prompt to
// localStorage; the API token is intentionally never saved (Q1.9.1).

import React, { useEffect, useState } from 'react';

import { apiFetch } from '../lib/api';
import {
  HARNESS_PROFILES,
  HARNESS_RUNTIMES,
  harnessNewFormModelKey,
  harnessNewFormProfileKey,
  harnessNewFormPromptKey,
  harnessNewFormRuntimeKey,
  harnessNewFormSkillKey,
  harnessNewFormTenantKey,
  harnessNewFormWorkspaceKey,
  type HarnessProfileChoice,
  type HarnessRuntimeChoice,
} from '../lib/harness';
import { readStorage, writeStorage } from '../lib/storage';

const harnessFormStarterPrompt = '请用一句话总结当前工作区。';
const harnessFormStarterWorkspace = '/tmp';
const harnessFormStarterModel = 'moonshot-v1-auto';

export function HarnessSubmitForm({
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
      // Q3.7.2: validate the create-session response shape — the
      // session_id is used in a URL, so a missing/wrong field would
      // navigate to a 404.
      const raw = (await response.json()) as { session_id?: unknown };
      if (typeof raw?.session_id !== 'string' || raw.session_id.length === 0) {
        throw new Error(
          'POST /v1/harness/sessions: response missing session_id (got: ' +
            JSON.stringify(raw) +
            ')',
        );
      }
      window.location.assign(`/ui/harness/sessions/${encodeURIComponent(raw.session_id)}`);
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
            Inputs persist in localStorage (tenant, profile, runtime, model, skill, workspace,
            prompt). The API token is never saved.
          </small>
        </footer>
      </form>
    </main>
  );
}
