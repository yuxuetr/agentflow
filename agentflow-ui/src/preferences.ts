// P10.17.2: durable UI preferences via the server's `/v1/preferences`
// API (P6.4). The localStorage path remains as a fast first-paint
// cache; server values overlay on top when they arrive, so the UI
// stays usable offline / before the API responds but eventually
// reflects the user's cross-browser preferences.
//
// Scope decision: only small, non-sensitive, cross-browser-useful
// values sync. The exclusion list is documented in
// `docs/WEB_UI.md` § Product positioning.
//
// Synced (4 categories):
//   - Tenant ids (run console + new-run form + harness new session)
//   - Profile selections (new-run form + harness new session)
//   - Harness runtime kind
//   - Per-run event-filter expressions (keyed by run id)
//
// NOT synced (security / size / machine-specific):
//   - API token (security — explicit comment forbids in main.tsx)
//   - Workflow YAML drafts (large; can contain example tokens
//     that would trip the server's token-shape rejection)
//   - Harness user_input prompts (may contain personal info)
//   - Harness workspace_root paths (machine-specific
//     filesystem paths)

/// Server-side preference keys must match the regex
/// `^[a-zA-Z0-9_.\-:]{1,128}$` enforced by
/// `agentflow_server::preferences::is_valid_preference_key`.
/// Static synced keys go through this table; the dynamic
/// per-run-id event-filter key is handled by a dedicated mapper
/// below.
export const STATIC_KEY_MAP: Record<string, string> = {
  'agentflow.ui.tenantId': 'ui.run-console.tenant',
  'agentflow.ui.newForm.tenant': 'ui.new-form.tenant',
  'agentflow.ui.newForm.profile': 'ui.new-form.profile',
  'agentflow.ui.harness.newForm.tenant_id': 'ui.harness-new-form.tenant',
  'agentflow.ui.harness.newForm.profile': 'ui.harness-new-form.profile',
  'agentflow.ui.harness.newForm.runtime_kind': 'ui.harness-new-form.runtime',
};

/// Per-run event-filter localStorage keys use a dynamic prefix
/// because each run id gets its own slot. Mirror it on the server
/// side via `ui.event-filter.<run_id>`.
const EVENT_FILTER_LOCAL_PREFIX = 'agentflow.ui.run.eventFilter.';
const EVENT_FILTER_SERVER_PREFIX = 'ui.event-filter.';

/// Returns the server-side preference key for a syncable
/// localStorage key, or `null` when the key is intentionally
/// local-only (security / size / machine-specific reasons).
export function serverKeyForLocal(localKey: string): string | null {
  const direct = STATIC_KEY_MAP[localKey];
  if (direct) return direct;
  if (localKey.startsWith(EVENT_FILTER_LOCAL_PREFIX)) {
    const runId = localKey.slice(EVENT_FILTER_LOCAL_PREFIX.length);
    // Empty run-id would map to the trailing-prefix slot; refuse
    // it so an accidental empty key doesn't reach the server.
    if (!runId) return null;
    return `${EVENT_FILTER_SERVER_PREFIX}${runId}`;
  }
  return null;
}

/// Reverse of [`serverKeyForLocal`]. Used on the read path: when
/// the server returns `{ "ui.run-console.tenant": "x" }`, the
/// caller needs to know which localStorage slot + React setter
/// should receive `"x"`.
export function localKeyForServer(serverKey: string): string | null {
  for (const [local, server] of Object.entries(STATIC_KEY_MAP)) {
    if (server === serverKey) return local;
  }
  if (serverKey.startsWith(EVENT_FILTER_SERVER_PREFIX)) {
    const runId = serverKey.slice(EVENT_FILTER_SERVER_PREFIX.length);
    if (!runId) return null;
    return `${EVENT_FILTER_LOCAL_PREFIX}${runId}`;
  }
  return null;
}

/// Predicate — true when this localStorage key syncs to the
/// server. Wraps [`serverKeyForLocal`] for ergonomic call sites
/// that only need the yes/no answer.
export function isSyncableLocalKey(localKey: string): boolean {
  return serverKeyForLocal(localKey) !== null;
}

/// Wire shape of `GET /v1/preferences` and the request body for
/// `PUT /v1/preferences`. Mirrors
/// `agentflow_server::preferences::PreferencesEnvelope`.
export interface PreferencesEnvelope {
  preferences: Record<string, unknown>;
}

/// `apiFetch`-shaped callable. The UI's existing `apiFetch`
/// helper attaches the bearer token; this signature lets us pass
/// it in without coupling the preferences module to the
/// `main.tsx` implementation.
export type ApiFetcher = (
  path: string,
  init?: RequestInit,
) => Promise<Response>;

/// Build the tenant header all preference routes require.
/// Centralised so callers can't accidentally send the bare query
/// param (which the runs routes accept but the preference routes
/// do not).
export function tenantHeaders(tenant: string): HeadersInit {
  return { 'X-Agentflow-Tenant': tenant };
}

/// `GET /v1/preferences` — returns the full per-tenant
/// preference set. Throws when the response is non-2xx so the
/// caller can decide whether to swallow or surface (the React
/// hook swallows, treating server prefs as best-effort).
export async function loadServerPreferences(
  fetcher: ApiFetcher,
  tenant: string,
): Promise<Record<string, unknown>> {
  const response = await fetcher('/v1/preferences', {
    headers: tenantHeaders(tenant),
  });
  if (!response.ok) {
    throw new Error(
      `GET /v1/preferences failed: ${response.status} ${response.statusText}`,
    );
  }
  const body = (await response.json()) as PreferencesEnvelope;
  return body.preferences ?? {};
}

/// `PUT /v1/preferences` — batched upsert. The server runs the
/// entire batch in a single transaction and rejects with 400 on
/// the first invalid key or value, so this caller treats any
/// non-2xx as a hard failure (the queue will retry the next
/// change cycle).
export async function saveServerPreferences(
  fetcher: ApiFetcher,
  tenant: string,
  entries: Record<string, unknown>,
): Promise<void> {
  const body: PreferencesEnvelope = { preferences: entries };
  const response = await fetcher('/v1/preferences', {
    method: 'PUT',
    headers: {
      ...tenantHeaders(tenant),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(
      `PUT /v1/preferences failed: ${response.status} ${response.statusText}`,
    );
  }
}

/// Convert a server preferences map into the localStorage shape
/// the existing UI code already reads. Each entry that has a
/// matching local key gets transcribed; entries the UI doesn't
/// know about (older schema versions, future keys) are skipped
/// silently — we don't want the UI to drop unknown server data.
export function serverPreferencesToLocalEntries(
  serverPrefs: Record<string, unknown>,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [serverKey, value] of Object.entries(serverPrefs)) {
    const localKey = localKeyForServer(serverKey);
    if (!localKey) continue;
    // The localStorage layer stores strings; coerce JSON values
    // via a stable transform. Numbers / booleans / objects get
    // JSON.stringify; strings stay as-is (no double-quoting).
    out[localKey] = typeof value === 'string' ? value : JSON.stringify(value);
  }
  return out;
}

/// Minimal debouncer that flushes batched preference writes once
/// the operator stops changing a field for `waitMs`. A single
/// pending batch is held; rapid changes within the window collapse
/// into one PUT. No coalescing across keys — each key is upserted
/// with whichever value the queue saw last.
///
/// Behaviour notes pinned by tests:
///   - Multiple `enqueue` calls within `waitMs` produce ONE flush.
///   - The flush receives the LAST value per key (later writes win).
///   - `cancel()` aborts the pending flush without firing it.
///   - `flushNow()` fires immediately + clears the pending state.
export class PreferenceWriteQueue {
  private pending: Record<string, unknown> = {};
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly waitMs: number,
    private readonly onFlush: (entries: Record<string, unknown>) => void,
  ) {}

  enqueue(key: string, value: unknown): void {
    this.pending[key] = value;
    if (this.timer !== null) {
      clearTimeout(this.timer);
    }
    this.timer = setTimeout(() => this.flushNow(), this.waitMs);
  }

  flushNow(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    if (Object.keys(this.pending).length === 0) {
      return;
    }
    const entries = this.pending;
    this.pending = {};
    this.onFlush(entries);
  }

  cancel(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    this.pending = {};
  }
}
