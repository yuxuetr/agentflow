// Q3.7.1: localStorage / sessionStorage helpers + UI-key constants
// shared by every page component. Lives in `lib/` so pages can import
// without a circular dep on `main.tsx`.

// ── Workflow runs console ────────────────────────────────────────────

/// Q1.9.1: the API token used to live in `localStorage`, which meant
/// (a) it survived browser restarts across multiple operator sessions,
/// (b) any XSS payload could exfiltrate it via `localStorage.getItem`,
/// and (c) the UI labels claimed "API token is never saved" while the
/// code was happily persisting it. The token now lives in
/// `sessionStorage` (cleared on tab close) plus the React component
/// tree's in-memory state, so it never outlives the active session and
/// no string with this constant name lands in the persistent store.
export const tokenKey = 'agentflow.ui.apiToken';
export const workflowKey = 'agentflow.ui.workflowDraft';
export const tenantKey = 'agentflow.ui.tenantId';

// P6.5: per-run event filter expression. Each run_id gets its own
// localStorage slot so navigating between runs doesn't bleed the
// previous filter into a fresh investigation. Long-term these also
// persist to /v1/preferences (P6.4) under a `ui.run.<id>.filter`
// key; the localStorage slot stays as a fast first-paint cache.
export const eventFilterKeyPrefix = 'agentflow.ui.run.eventFilter.';

// ── Run-create form (/ui/runs/new) ───────────────────────────────────

export const newFormWorkflowKey = 'agentflow.ui.newForm.workflow';
export const newFormTenantKey = 'agentflow.ui.newForm.tenant';
export const newFormProfileKey = 'agentflow.ui.newForm.profile';
export const newFormInputsKey = 'agentflow.ui.newForm.inputs';

// ── Storage primitives ───────────────────────────────────────────────
//
// All four are best-effort: a private-browsing tab with localStorage
// disabled (or a sandboxed iframe) returns `null`/throws; the UI still
// works without persistence.

export const readStorage = (key: string, fallback: string): string => {
  try {
    return window.localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};

export const writeStorage = (key: string, value: string): void => {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Storage is best-effort; the console still works without it.
  }
};

/// Q1.9.1: session-scoped storage for the API token. Cleared when
/// the tab closes — never persists across browser restarts.
export const readSessionStorage = (key: string, fallback: string): string => {
  try {
    return window.sessionStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};

export const writeSessionStorage = (key: string, value: string): void => {
  try {
    if (value) {
      window.sessionStorage.setItem(key, value);
    } else {
      window.sessionStorage.removeItem(key);
    }
  } catch {
    // Storage is best-effort; the console still works without it.
  }
};
