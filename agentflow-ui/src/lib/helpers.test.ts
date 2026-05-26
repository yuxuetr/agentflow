// Q3.7.3: pin the SSE reconnect backoff schedule. The schedule lives
// in `helpers.ts` so unit tests can run without a React renderer; the
// real consumers are `main.tsx::RunConsole` (workflow SSE) and
// `HarnessSessionDetail` (harness SSE) — both call
// `reconnectDelayMs(attempt)` from a long-lived `connect()` closure.
//
// Run: `npx tsx src/lib/helpers.test.ts`

import { eventTone, isTerminalRun, reconnectDelayMs } from './helpers';

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

// ── reconnectDelayMs ──────────────────────────────────────────────────

test('reconnectDelayMs starts at 250ms on attempt 0', () => {
  assert(reconnectDelayMs(0) === 250, `expected 250, got ${reconnectDelayMs(0)}`);
});

test('reconnectDelayMs doubles each step', () => {
  const schedule = [0, 1, 2, 3, 4, 5, 6].map(reconnectDelayMs);
  assert(
    JSON.stringify(schedule) === JSON.stringify([250, 500, 1000, 2000, 4000, 8000, 16_000]),
    `expected doubling schedule, got ${schedule.join(', ')}`,
  );
});

test('reconnectDelayMs caps at 30_000ms', () => {
  // 250 * 2^7 = 32_000 → cap kicks in. Subsequent attempts stay at 30_000.
  assert(
    reconnectDelayMs(7) === 30_000,
    `attempt 7 must cap at 30_000, got ${reconnectDelayMs(7)}`,
  );
  assert(
    reconnectDelayMs(20) === 30_000,
    `attempt 20 must cap at 30_000, got ${reconnectDelayMs(20)}`,
  );
});

test('reconnectDelayMs clamps negative + fractional attempts safely', () => {
  // Defensive: a future caller fat-finger might pass -1 or 0.7.
  // The Math.max(0, floor(...)) guard keeps the schedule pinned to
  // the smallest sensible value rather than producing a fractional
  // millisecond or a giant fraction-driven base.
  assert(reconnectDelayMs(-3) === 250, `negative must clamp to 250, got ${reconnectDelayMs(-3)}`);
  assert(reconnectDelayMs(0.7) === 250, `fractional must floor, got ${reconnectDelayMs(0.7)}`);
  assert(reconnectDelayMs(1.5) === 500, `fractional must floor, got ${reconnectDelayMs(1.5)}`);
});

// ── isTerminalRun + eventTone (smoke regression — these moved into
//   lib/helpers.ts under Q3.7.1; pin them so the page-component split
//   doesn't accidentally drop a branch.) ──────────────────────────────

test('isTerminalRun: null run is terminal (no in-flight work)', () => {
  assert(isTerminalRun(null) === true, 'null must be terminal');
});

test('isTerminalRun: succeeded / failed / cancelled are terminal', () => {
  for (const status of ['succeeded', 'failed', 'cancelled']) {
    assert(
      isTerminalRun({ id: 'r', workflow: 'w', status }) === true,
      `${status} must be terminal`,
    );
  }
});

test('isTerminalRun: running / queued / pending are NOT terminal', () => {
  for (const status of ['running', 'queued', 'pending']) {
    assert(
      isTerminalRun({ id: 'r', workflow: 'w', status }) === false,
      `${status} must not be terminal`,
    );
  }
});

test('eventTone classifies the danger family by substring', () => {
  assert(eventTone('node_failed') === 'danger', 'failed → danger');
  assert(eventTone('PolicyDenied') === 'danger', 'denied → danger');
  assert(eventTone('error_emitted') === 'danger', 'error → danger');
});

test('eventTone classifies tool / agent / success families', () => {
  assert(eventTone('tool_call_started') === 'tool', 'tool → tool');
  assert(eventTone('agent_plan_emitted') === 'agent', 'agent → agent');
  assert(eventTone('run_succeeded') === 'success', 'succeed → success');
});

test('eventTone falls through to neutral for unknown kinds', () => {
  assert(eventTone('whatever_else') === 'neutral', 'unknown → neutral');
  assert(eventTone('') === 'neutral', 'empty → neutral');
});

// ── runner ───────────────────────────────────────────────────────────

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
  throw new Error(`helpers tests: ${failed} failure(s)`);
} else {
  console.log(`\n${cases.length} test(s) passed`);
}
