// V2.2: pin `extractTokenDelta`'s narrowing rules against a `token_delta`
// event's `unknown` payload — the schema layer deliberately leaves
// `payload` untyped (see schemas.ts), so this is the only guard against a
// malformed/missing `delta` field reaching the live-typing accumulator.
//
// Run: `npx tsx src/lib/harness.test.ts`

import { extractTokenDelta } from './harness';

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

test('extractTokenDelta returns the delta string from a well-formed payload', () => {
  const delta = extractTokenDelta({ step_index: 2, delta: 'hello' });
  assert(delta === 'hello', `expected "hello", got ${JSON.stringify(delta)}`);
});

test('extractTokenDelta returns null when delta is missing', () => {
  assert(extractTokenDelta({ step_index: 2 }) === null, 'missing delta must be null');
});

test('extractTokenDelta returns null when delta is not a string', () => {
  assert(extractTokenDelta({ delta: 42 }) === null, 'non-string delta must be null');
});

test('extractTokenDelta returns null for non-object payloads', () => {
  assert(extractTokenDelta(null) === null, 'null payload must be null');
  assert(extractTokenDelta(undefined) === null, 'undefined payload must be null');
  assert(extractTokenDelta('not an object') === null, 'string payload must be null');
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
  throw new Error(`harness tests: ${failed} failure(s)`);
} else {
  console.log(`\n${cases.length} test(s) passed`);
}
