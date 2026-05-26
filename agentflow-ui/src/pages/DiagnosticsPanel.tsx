// /ui/diagnostics — operator-facing doctor view. Calls `/v1/diagnostics`
// once on mount; never auto-polls (P6.2 spec — diagnostics changes are
// rare enough that operators want explicit refresh control).

import { useEffect, useMemo, useState } from 'react';

import { apiFetch } from '../lib/api';
import {
  DiagnosticsReportSchema,
  parseJsonResponse,
  type DiagnosticsDirCheck,
  type DiagnosticsReport,
  type DiagnosticsStatus,
} from '../schemas';

/**
 * Mask a tokenish value down to its last 4 characters. The doctor
 * report does not include API key *values* today (only env-var names
 * and a set/unset bool), but the UI still threads any displayed
 * token through this helper as a defense-in-depth measure for the
 * "Mask API keys to last 4 chars" requirement in P6.2.
 */
const maskToken = (raw: string): string => {
  const trimmed = raw.trim();
  if (!trimmed) return '';
  if (trimmed.length <= 4) return `••${trimmed}`;
  return `••••${trimmed.slice(-4)}`;
};

type DiagnosticsRow = {
  component: string;
  status: DiagnosticsStatus;
  detail: string;
};

const collectRows = (report: DiagnosticsReport): DiagnosticsRow[] => {
  const rows: DiagnosticsRow[] = [];
  const config = report.config;
  if (config) {
    const missing = config.missing_env_vars ?? [];
    const status: DiagnosticsStatus = config.error
      ? 'warning'
      : missing.length > 0
        ? 'warning'
        : 'ok';
    const detail = config.error
      ? config.error
      : missing.length > 0
        ? `Missing env vars: ${missing.join(', ')}`
        : `${config.models ?? 0} models / ${config.providers ?? 0} providers`;
    rows.push({ component: 'Models config', status, detail });
  }
  const security = report.security;
  if (security) {
    rows.push({
      component: 'Security profile',
      status: security.warning ? 'warning' : 'ok',
      detail: security.warning ?? `profile: ${security.profile ?? 'unknown'}`,
    });
  }
  const sandbox = report.sandbox;
  if (sandbox) {
    const status: DiagnosticsStatus = sandbox.enforcing
      ? 'ok'
      : (sandbox.warnings?.length ?? 0) > 0
        ? 'warning'
        : 'warning';
    const detail = `backend: ${sandbox.backend ?? 'unknown'} (${sandbox.enforcement ?? 'unknown'})`;
    rows.push({ component: 'OS sandbox', status, detail });
  }
  const dirs: Array<[string, DiagnosticsDirCheck | undefined]> = [
    ['Run dir', report.disk?.run_dir],
    ['Trace dir', report.disk?.trace_dir],
    ['Marketplace cache', report.disk?.marketplace_cache],
  ];
  for (const [label, dir] of dirs) {
    if (!dir) continue;
    const status: DiagnosticsStatus = !dir.exists
      ? 'warning'
      : !dir.writable
        ? 'fail'
        : 'ok';
    const detail = `${dir.path} (${dir.source}${dir.writable ? ', writable' : ''})`;
    rows.push({ component: label, status, detail });
  }
  const env = report.environment;
  if (env) {
    rows.push({
      component: 'AGENTFLOW_API_TOKEN',
      status: env.agentflow_api_token_set ? 'ok' : 'warning',
      detail: env.agentflow_api_token_set ? 'set (value masked)' : 'not set',
    });
  }
  return rows;
};

export function DiagnosticsPanel({
  apiToken,
  onTokenChange,
}: {
  apiToken: string;
  onTokenChange: (token: string) => void;
}) {
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchReport = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await apiFetch('/v1/diagnostics', apiToken);
      if (!response.ok) {
        setError(`HTTP ${response.status}`);
        setReport(null);
        return;
      }
      const json = await parseJsonResponse(
        DiagnosticsReportSchema,
        response,
        'GET /v1/doctor',
      );
      setReport(json);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setReport(null);
    } finally {
      setLoading(false);
    }
  };

  // No auto-poll: P6.2 explicitly asks the panel to refresh only on
  // the explicit user action. Fetch once on mount; never again
  // without a button click.
  useEffect(() => {
    void fetchReport();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const rows = useMemo(() => (report ? collectRows(report) : []), [report]);
  const statusBadge = (status: DiagnosticsStatus) => {
    const label = status === 'ok' ? 'PASS' : status === 'warning' ? 'WARN' : 'FAIL';
    return <span className={`diag-badge diag-badge-${status}`}>{label}</span>;
  };

  const overall = report?.status;
  const tokenDisplay = apiToken ? maskToken(apiToken) : '(none)';

  return (
    <div className="diagnostics-panel">
      <header className="diagnostics-header">
        <h1>Diagnostics</h1>
        <div className="diagnostics-controls">
          <label>
            API token (last 4 shown):{' '}
            <code className="diag-token">{tokenDisplay}</code>
          </label>
          <input
            type="password"
            placeholder="paste bearer token"
            value={apiToken}
            onChange={(event) => onTokenChange(event.target.value)}
          />
          <button onClick={() => void fetchReport()} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </header>
      {error && (
        <div className="diagnostics-error">
          <strong>Error:</strong> {error}
        </div>
      )}
      {overall && (
        <div className={`diagnostics-overall diagnostics-overall-${overall}`}>
          Overall: {statusBadge(overall)}
          {report?.version && (
            <span className="diag-version">version {report.version}</span>
          )}
          {report?.profile && (
            <span className="diag-profile">profile {report.profile}</span>
          )}
        </div>
      )}
      <table className="diagnostics-table">
        <thead>
          <tr>
            <th>Component</th>
            <th>Status</th>
            <th>Detail</th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && !error && (
            <tr>
              <td colSpan={3} className="diagnostics-empty">
                {loading ? 'Loading…' : 'No data yet'}
              </td>
            </tr>
          )}
          {rows.map((row) => (
            <tr key={row.component}>
              <td>{row.component}</td>
              <td>{statusBadge(row.status)}</td>
              <td>{row.detail}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
