// Q3.7.2: runtime JSON validation for the most security-critical
// server responses. Replaces `(await response.json()) as XYZ` casts
// that previously trusted whatever the server / a misbehaving
// intermediary returned.
//
// Scope rules:
// - SSE event payloads + workflow & harness JSON responses + the
//   diagnostics report are wrapped — they drive UI state and a wrong
//   shape silently corrupts the run/session detail view.
// - HTMl `<option>` values are NOT wrapped — those are constrained at
//   the markup level and a misbehaving value falls through to a sane
//   default already.
// - `payload: unknown` inside StreamedEvent / HarnessEvent stays
//   intentionally `unknown` — event payloads are an open enum keyed by
//   `kind`, so per-kind narrowing happens at the consumer site.
//
// All schemas use `.loose()` so a server that adds *new* optional
// fields doesn't break the UI; only missing required fields or wrong
// types are rejected. Use `parseJsonResponse(schema, response, label)`
// as the canonical entry-point so the error surface stays uniform.

import { z } from 'zod';

// ── Workflow runs ────────────────────────────────────────────────────

export const RunRecordSchema = z
  .object({
    id: z.string(),
    workflow: z.string(),
    status: z.string(),
    tenant_id: z.string().optional(),
    started_at: z.string().optional(),
    finished_at: z.string().nullable().optional(),
    run_dir: z.string().nullable().optional(),
    error: z.string().nullable().optional(),
  })
  .loose();
export type RunRecord = z.infer<typeof RunRecordSchema>;

export const RunEnvelopeSchema = RunRecordSchema.extend({
  run: RunRecordSchema.optional(),
});
export type RunEnvelope = z.infer<typeof RunEnvelopeSchema>;

export const ListRunsEnvelopeSchema = z
  .object({
    runs: z.array(RunRecordSchema),
  })
  .loose();
export type ListRunsEnvelope = z.infer<typeof ListRunsEnvelopeSchema>;

export const CreateRunEnvelopeSchema = z
  .object({
    run_id: z.string(),
    status: z.string(),
  })
  .loose();
export type CreateRunEnvelope = z.infer<typeof CreateRunEnvelopeSchema>;

export const CancelRunEnvelopeSchema = z
  .object({
    run: RunRecordSchema,
    cancelled: z.boolean(),
  })
  .loose();
export type CancelRunEnvelope = z.infer<typeof CancelRunEnvelopeSchema>;

export const StreamedEventSchema = z
  .object({
    run_id: z.string(),
    seq: z.number(),
    kind: z.string(),
    payload: z.unknown(),
    ts: z.string(),
  })
  .loose();
export type StreamedEvent = z.infer<typeof StreamedEventSchema>;

export const StreamedEventArraySchema = z.array(StreamedEventSchema);

// ── Harness sessions ─────────────────────────────────────────────────

export const HarnessSessionSchema = z
  .object({
    id: z.string(),
    tenant_id: z.string(),
    status: z.string(),
    user_input: z.string(),
    workspace_root: z.string(),
    profile: z.string(),
    runtime_kind: z.string(),
    model: z.string(),
    skill_name: z.string().nullable().optional(),
    started_at: z.string().optional(),
    finished_at: z.string().nullable().optional(),
    final_answer: z.string().nullable().optional(),
    error: z.string().nullable().optional(),
  })
  .loose();
export type HarnessSession = z.infer<typeof HarnessSessionSchema>;

export const HarnessEventSchema = z
  .object({
    session_id: z.string(),
    seq: z.number(),
    kind: z.string(),
    payload: z.unknown(),
    ts: z.string(),
  })
  .loose();
export type HarnessEvent = z.infer<typeof HarnessEventSchema>;

export const HarnessEventArraySchema = z.array(HarnessEventSchema);
export const HarnessSessionArraySchema = z.array(HarnessSessionSchema);

export const PendingApprovalSchema = z
  .object({
    id: z.string(),
    session_id: z.string(),
    step_index: z.number(),
    tool: z.string(),
    source: z.string().nullable().optional(),
    permissions: z.array(z.string()).optional(),
    idempotency: z.string().optional(),
    params_summary: z.unknown().optional(),
    risk: z.string(),
    reason: z.string(),
    requested_at: z.string(),
    expires_at: z.string().nullable().optional(),
  })
  .loose();
export type PendingApproval = z.infer<typeof PendingApprovalSchema>;
export const PendingApprovalArraySchema = z.array(PendingApprovalSchema);

// ── Diagnostics report ───────────────────────────────────────────────

const DiagnosticsStatusSchema = z.enum(['ok', 'warning', 'fail']);
export type DiagnosticsStatus = z.infer<typeof DiagnosticsStatusSchema>;

const DiagnosticsDirCheckSchema = z
  .object({
    path: z.string(),
    source: z.string(),
    exists: z.boolean(),
    writable: z.boolean(),
    error: z.string().nullable().optional(),
  })
  .loose();
export type DiagnosticsDirCheck = z.infer<typeof DiagnosticsDirCheckSchema>;

export const DiagnosticsReportSchema = z
  .object({
    version: z.string().optional(),
    profile: z.string().optional(),
    status: DiagnosticsStatusSchema,
    features: z
      .object({
        rag: z.boolean().optional(),
        plugin: z.boolean().optional(),
        mcp_workflow_nodes: z.boolean().optional(),
      })
      .loose()
      .optional(),
    config: z
      .object({
        models_config_source: z.string().optional(),
        models_config_path: z.string().optional(),
        models_config_exists: z.boolean().optional(),
        models_config_loadable: z.boolean().optional(),
        models: z.number().optional(),
        providers: z.number().optional(),
        missing_env_vars: z.array(z.string()).optional(),
        warnings: z.array(z.string()).optional(),
        error: z.string().nullable().optional(),
      })
      .loose()
      .optional(),
    security: z
      .object({
        env_var: z.string().optional(),
        profile: z.string().optional(),
        warning: z.string().nullable().optional(),
      })
      .loose()
      .optional(),
    sandbox: z
      .object({
        backend: z.string().optional(),
        enforcement: z.string().optional(),
        enforcing: z.boolean().optional(),
        capabilities: z.array(z.string()).optional(),
        warnings: z.array(z.string()).optional(),
      })
      .loose()
      .optional(),
    environment: z
      .object({
        agentflow_run_dir: z.string().nullable().optional(),
        agentflow_trace_dir: z.string().nullable().optional(),
        agentflow_api_token_set: z.boolean().optional(),
        agentflow_skills_index: z.string().nullable().optional(),
      })
      .loose()
      .optional(),
    disk: z
      .object({
        run_dir: DiagnosticsDirCheckSchema.optional(),
        trace_dir: DiagnosticsDirCheckSchema.optional(),
        marketplace_cache: DiagnosticsDirCheckSchema.optional(),
      })
      .loose()
      .optional(),
  })
  .loose();
export type DiagnosticsReport = z.infer<typeof DiagnosticsReportSchema>;

// ── Helpers ──────────────────────────────────────────────────────────

/**
 * Parse and validate a `fetch` response body against `schema`. Throws
 * a `SchemaValidationError` carrying both the raw payload and the zod
 * issue list when validation fails — the caller can surface this in
 * the existing error UI without rolling its own JSON handling.
 *
 * Use this instead of `(await response.json()) as T` so a malformed
 * server response surfaces as a clear, attributable error instead of
 * silently corrupting downstream UI state.
 */
export async function parseJsonResponse<T>(
  schema: z.ZodType<T>,
  response: Response,
  contextLabel: string,
): Promise<T> {
  const raw = await response.json();
  return parseJson(schema, raw, contextLabel);
}

/**
 * Validate an already-decoded JSON value (typed `unknown`) against
 * `schema`. The SSE handler uses this because `EventSource` delivers
 * pre-parsed `data` strings that we `JSON.parse` ourselves.
 */
export function parseJson<T>(
  schema: z.ZodType<T>,
  raw: unknown,
  contextLabel: string,
): T {
  const result = schema.safeParse(raw);
  if (!result.success) {
    throw new SchemaValidationError(contextLabel, raw, result.error);
  }
  return result.data;
}

export class SchemaValidationError extends Error {
  readonly contextLabel: string;
  readonly raw: unknown;
  readonly zodError: z.ZodError;

  constructor(contextLabel: string, raw: unknown, zodError: z.ZodError) {
    const issueSummary = zodError.issues
      .slice(0, 3)
      .map((i) => `${i.path.join('.') || '<root>'}: ${i.message}`)
      .join('; ');
    super(
      `${contextLabel}: response did not match expected shape (${zodError.issues.length} issue${
        zodError.issues.length === 1 ? '' : 's'
      }: ${issueSummary})`,
    );
    this.name = 'SchemaValidationError';
    this.contextLabel = contextLabel;
    this.raw = raw;
    this.zodError = zodError;
  }
}
