// Q3.7.1: shared utilities, storage keys, and small types for the
// Harness Mode page components (HarnessSessionList /
// HarnessSubmitForm / HarnessSessionDetail / ApprovalCard).

import type { HarnessSession } from '../schemas';

// ── Storage keys (HarnessSubmitForm) ─────────────────────────────────

export const harnessNewFormPromptKey = 'agentflow.ui.harness.newForm.user_input';
export const harnessNewFormWorkspaceKey = 'agentflow.ui.harness.newForm.workspace_root';
export const harnessNewFormProfileKey = 'agentflow.ui.harness.newForm.profile';
export const harnessNewFormRuntimeKey = 'agentflow.ui.harness.newForm.runtime_kind';
export const harnessNewFormModelKey = 'agentflow.ui.harness.newForm.model';
export const harnessNewFormSkillKey = 'agentflow.ui.harness.newForm.skill_name';
export const harnessNewFormTenantKey = 'agentflow.ui.harness.newForm.tenant_id';

// ── Status helpers ───────────────────────────────────────────────────

export const harnessStatusTone = (status: string): 'pending' | 'success' | 'danger' | 'neutral' => {
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

export const isHarnessTerminal = (session: HarnessSession | null): boolean =>
  session ? ['completed', 'failed', 'cancelled'].includes(session.status) : true;

/**
 * Parse `/ui/harness/sessions/<uuid>` URL into the session id segment.
 * Returns `null` for the list view, the create-form view, or any path
 * that doesn't match the deep-link shape. Used by the App router.
 */
export const harnessSessionIdFromPath = (): string | null => {
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

// ── Approval flow types (HarnessSessionDetail / ApprovalCard) ────────

export type ApprovalOutcome = 'allow' | 'deny' | 'deny_and_stop';
export type ApprovalScope = 'once' | 'session' | 'run';

// ── Submit-form selection enums ──────────────────────────────────────

export type HarnessProfileChoice = 'dev' | 'local' | 'production';
export const HARNESS_PROFILES: HarnessProfileChoice[] = ['dev', 'local', 'production'];

export type HarnessRuntimeChoice = 'react' | 'plan_execute';
export const HARNESS_RUNTIMES: HarnessRuntimeChoice[] = ['react', 'plan_execute'];
