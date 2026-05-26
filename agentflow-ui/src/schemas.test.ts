// Q3.7.2: tests pin the runtime-validation contract so a server-side
// schema change (or a misbehaving intermediary) doesn't silently
// corrupt UI state.
//
// Run with: `npm test -- --run` once a test runner is wired up.
// For now this file is `tsc`-checked via the existing `npm test`
// (`tsc --noEmit`) gate; the assertions are kept as standalone
// `if (!cond) throw` statements so they can be invoked from any
// test runner later without modification.

import {
  CreateRunEnvelopeSchema,
  DiagnosticsReportSchema,
  HarnessEventSchema,
  HarnessSessionSchema,
  ListRunsEnvelopeSchema,
  RunEnvelopeSchema,
  RunRecordSchema,
  SchemaValidationError,
  StreamedEventSchema,
  parseJson,
} from './schemas';

type TestCase = { name: string; run: () => void };

const cases: TestCase[] = [];
function test(name: string, run: () => void): void {
  cases.push({ name, run });
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`assert failed: ${message}`);
  }
}

// ── Happy paths ──────────────────────────────────────────────────────

test('RunRecord accepts the canonical server shape', () => {
  const parsed = RunRecordSchema.parse({
    id: 'r-1',
    workflow: 'wf',
    status: 'running',
    tenant_id: 't',
    started_at: '2026-01-01T00:00:00Z',
    finished_at: null,
    run_dir: null,
    error: null,
  });
  assert(parsed.id === 'r-1', 'id round-trips');
  assert(parsed.error === null, 'nullable null round-trips');
});

test('RunRecord allows missing optional fields', () => {
  const parsed = RunRecordSchema.parse({ id: 'r-2', workflow: 'w', status: 'queued' });
  assert(parsed.tenant_id === undefined, 'missing optional stays undefined');
});

test('RunRecord rejects missing required fields', () => {
  const result = RunRecordSchema.safeParse({ id: 'r-3', status: 'queued' });
  assert(!result.success, 'must reject when workflow is missing');
});

test('CreateRunEnvelope round-trips run_id + status', () => {
  const parsed = CreateRunEnvelopeSchema.parse({ run_id: 'rid', status: 'queued' });
  assert(parsed.run_id === 'rid', 'run_id preserved');
});

test('ListRunsEnvelope accepts an empty list', () => {
  const parsed = ListRunsEnvelopeSchema.parse({ runs: [] });
  assert(parsed.runs.length === 0, 'empty list round-trips');
});

test('StreamedEvent keeps payload as unknown', () => {
  const parsed = StreamedEventSchema.parse({
    run_id: 'r-1',
    seq: 5,
    kind: 'node_started',
    payload: { node_id: 'n', extra_field: 42 },
    ts: '2026-01-01T00:00:00Z',
  });
  assert(parsed.seq === 5, 'seq round-trips');
  // payload stays opaque; consumer narrows per kind.
  const payload = parsed.payload as Record<string, unknown>;
  assert(payload.node_id === 'n', 'payload contents preserved');
});

test('StreamedEvent rejects seq as a string', () => {
  const result = StreamedEventSchema.safeParse({
    run_id: 'r-1',
    seq: '5',
    kind: 'x',
    payload: null,
    ts: '2026-01-01T00:00:00Z',
  });
  assert(!result.success, 'seq must be a number');
});

test('HarnessSession round-trips the canonical server shape', () => {
  const parsed = HarnessSessionSchema.parse({
    id: 'sess-1',
    tenant_id: 'default',
    status: 'running',
    user_input: 'hello',
    workspace_root: '/tmp/ws',
    profile: 'local',
    runtime_kind: 'react',
    model: 'mock',
  });
  assert(parsed.skill_name === undefined, 'optional skill_name missing-fine');
});

test('HarnessEvent rejects missing session_id', () => {
  const result = HarnessEventSchema.safeParse({
    seq: 0,
    kind: 'session_started',
    payload: {},
    ts: '2026-01-01T00:00:00Z',
  });
  assert(!result.success, 'session_id required');
});

test('DiagnosticsReport accepts minimal status-only shape', () => {
  const parsed = DiagnosticsReportSchema.parse({ status: 'ok' });
  assert(parsed.status === 'ok', 'status preserved');
});

test('DiagnosticsReport rejects unknown status value', () => {
  const result = DiagnosticsReportSchema.safeParse({ status: 'broken' });
  assert(!result.success, 'status enum is closed');
});

// ── Unknown-field passthrough ────────────────────────────────────────
// Server adding a new optional field must NOT break the UI.

test('RunRecord passthrough preserves unknown fields', () => {
  const parsed = RunRecordSchema.parse({
    id: 'r',
    workflow: 'w',
    status: 's',
    future_field: 'survives',
  } as Record<string, unknown>);
  assert(
    (parsed as Record<string, unknown>).future_field === 'survives',
    'unknown field preserved',
  );
});

test('HarnessSession passthrough preserves unknown fields', () => {
  const parsed = HarnessSessionSchema.parse({
    id: 's',
    tenant_id: 't',
    status: 's',
    user_input: 'u',
    workspace_root: 'w',
    profile: 'local',
    runtime_kind: 'react',
    model: 'm',
    new_capability: { nested: true },
  } as Record<string, unknown>);
  assert(
    typeof (parsed as Record<string, unknown>).new_capability === 'object',
    'unknown nested object preserved',
  );
});

// ── parseJson + SchemaValidationError helpers ───────────────────────

test('parseJson returns the validated value on success', () => {
  const value = parseJson(
    CreateRunEnvelopeSchema,
    { run_id: 'r', status: 'queued' },
    'POST /v1/runs',
  );
  assert(value.run_id === 'r', 'parseJson returns the validated value');
});

test('parseJson throws SchemaValidationError on shape mismatch', () => {
  let caught: unknown = null;
  try {
    parseJson(CreateRunEnvelopeSchema, { run_id: 1 }, 'POST /v1/runs');
  } catch (err) {
    caught = err;
  }
  assert(caught instanceof SchemaValidationError, 'must throw SchemaValidationError');
  const sve = caught as SchemaValidationError;
  assert(sve.contextLabel === 'POST /v1/runs', 'context label survives');
  assert(sve.message.includes('did not match expected shape'), 'message describes mismatch');
  assert(sve.zodError.issues.length >= 1, 'zodError carries issues');
});

test('SchemaValidationError truncates the issue summary to first 3', () => {
  // Build a payload with at least 4 issues so we can verify capping.
  let caught: unknown = null;
  try {
    parseJson(
      RunEnvelopeSchema,
      { id: 1, workflow: 2, status: 3, started_at: 4, finished_at: 5 },
      'GET /v1/runs/x',
    );
  } catch (err) {
    caught = err;
  }
  assert(caught instanceof SchemaValidationError, 'throws');
  const sve = caught as SchemaValidationError;
  // The truncated summary should mention at most three issues in the
  // header; the full list still lives on `zodError.issues`.
  assert(sve.zodError.issues.length >= 3, 'zodError has all issues');
  const head = sve.message.split('(')[1] ?? '';
  // Count semicolons in the parenthesised summary: 3 issues → 2 separators.
  const semiCount = (head.match(/;/g) ?? []).length;
  assert(semiCount <= 2, `summary capped at 3 issues, got ${semiCount + 1}`);
});

// ── Test runner ──────────────────────────────────────────────────────

let failed = 0;
for (const tc of cases) {
  try {
    tc.run();
    console.log(`ok ${tc.name}`);
  } catch (err) {
    failed += 1;
    console.error(`FAIL ${tc.name}: ${(err as Error).message}`);
  }
}

if (failed > 0) {
  console.error(`\n${failed}/${cases.length} test(s) failed`);
  // Use throw rather than process.exit so this still works under
  // `tsc --noEmit` (we just need the file to type-check) and from a
  // future Vitest/Jest harness alike.
  throw new Error(`schema tests: ${failed} failure(s)`);
} else {
  console.log(`\n${cases.length} test(s) passed`);
}
