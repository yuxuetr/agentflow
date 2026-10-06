// P10.17.2: React hook that wires the preferences helper module to
// the existing main.tsx apiFetch shape. Components that own
// syncable state call this hook with their (apiToken, tenant) and
// get back a `serverPrefs` snapshot + a debounced `syncToServer`
// function.

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  PreferenceWriteQueue,
  loadServerPreferences,
  saveServerPreferences,
  serverKeyForLocal,
  type ApiFetcher,
} from './preferences';

/// Debounce window before a queued preference write flushes to
/// the server. 500 ms balances "don't hammer the API on every
/// keystroke" against "operators see their change persisted
/// quickly enough to feel reactive".
const FLUSH_DEBOUNCE_MS = 500;

export interface PreferenceSyncHandle {
  /// Server snapshot. `null` until the first GET completes (or
  /// permanently `null` when token / tenant is missing or the GET
  /// errored — the hook treats server prefs as best-effort).
  serverPrefs: Record<string, unknown> | null;
  /// Queue a preference write. Drops silently when the local key
  /// isn't syncable (api token, workflow YAML drafts) or when the
  /// hook hasn't yet observed a token / tenant.
  syncToServer: (localKey: string, value: unknown) => void;
}

/// Build an [`ApiFetcher`] that attaches the bearer token.
/// Mirrors main.tsx's existing `apiFetch` helper but free-standing
/// so the preferences module can be unit-tested without React.
function makeFetcher(apiToken: string): ApiFetcher {
  return (path, init = {}) => {
    const headers = new Headers(init.headers);
    const trimmed = apiToken.trim();
    if (trimmed) {
      headers.set('Authorization', `Bearer ${trimmed}`);
    }
    return fetch(path, { ...init, headers });
  };
}

/// One-stop hook for the synced-preferences pattern. Components
/// that own a syncable field invoke it with their current
/// `(apiToken, tenant)`, then:
///
///   1. Read `serverPrefs?.[someServerKey]` in an effect to
///      overlay the server value onto local state when it
///      arrives.
///   2. Call `syncToServer(localKey, value)` from the same effect
///      that already calls `writeStorage(localKey, value)`.
///
/// The hook silently no-ops on missing token / tenant — both the
/// GET and the PUT need them, and a half-configured UI shouldn't
/// surface PUT errors to the operator.
export function usePreferenceSync(
  apiToken: string,
  tenant: string,
): PreferenceSyncHandle {
  const [serverPrefs, setServerPrefs] = useState<Record<string, unknown> | null>(null);

  // GET on (token, tenant) change. `cancelled` guards against
  // late responses overwriting state after the operator switched
  // tenants again.
  useEffect(() => {
    if (!apiToken.trim() || !tenant.trim()) {
      setServerPrefs(null);
      return;
    }
    let cancelled = false;
    loadServerPreferences(makeFetcher(apiToken), tenant)
      .then((prefs) => {
        if (!cancelled) setServerPrefs(prefs);
      })
      .catch((err) => {
        // Best-effort: a failed GET keeps `serverPrefs` null and
        // the UI continues with localStorage. console.warn so
        // operators with devtools open can see the diagnostic.
        // eslint-disable-next-line no-console
        console.warn('preferences GET failed:', err);
        if (!cancelled) setServerPrefs(null);
      });
    return () => {
      cancelled = true;
    };
  }, [apiToken, tenant]);

  // The queue persists across re-renders but the closure that
  // builds the flush callback must capture the latest token +
  // tenant — refs make that explicit without re-creating the
  // queue on every render.
  const tokenRef = useRef(apiToken);
  const tenantRef = useRef(tenant);
  tokenRef.current = apiToken;
  tenantRef.current = tenant;

  const queueRef = useRef<PreferenceWriteQueue | null>(null);
  if (queueRef.current === null) {
    queueRef.current = new PreferenceWriteQueue(FLUSH_DEBOUNCE_MS, (entries) => {
      const t = tokenRef.current.trim();
      const tn = tenantRef.current.trim();
      if (!t || !tn) return;
      saveServerPreferences(makeFetcher(t), tn, entries).catch((err) => {
        // eslint-disable-next-line no-console
        console.warn('preferences PUT failed:', err);
      });
    });
  }

  // Cleanup on unmount: flush whatever is pending so the operator
  // doesn't lose a last-second edit between navigation events.
  useEffect(() => {
    const queue = queueRef.current;
    return () => {
      queue?.flushNow();
    };
  }, []);

  const syncToServer = useCallback((localKey: string, value: unknown) => {
    const serverKey = serverKeyForLocal(localKey);
    if (!serverKey) return;
    queueRef.current?.enqueue(serverKey, value);
  }, []);

  return { serverPrefs, syncToServer };
}
