// V2.2: pin `extractTokenDelta`'s narrowing rules against a `token_delta`
// event's `unknown` payload — the schema layer deliberately leaves
// `payload` untyped (see schemas.ts), so this is the only guard against a
// malformed/missing `delta` field reaching the live-typing accumulator.

import { describe, it } from 'vitest';
import { extractTokenDelta } from './harness';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`assert failed: ${message}`);
  }
}

describe('extractTokenDelta', () => {
  it('returns the delta string from a well-formed payload', () => {
    const delta = extractTokenDelta({ step_index: 2, delta: 'hello' });
    assert(delta === 'hello', `expected "hello", got ${JSON.stringify(delta)}`);
  });

  it('returns null when delta is missing', () => {
    assert(extractTokenDelta({ step_index: 2 }) === null, 'missing delta must be null');
  });

  it('returns null when delta is not a string', () => {
    assert(extractTokenDelta({ delta: 42 }) === null, 'non-string delta must be null');
  });

  it('returns null for non-object payloads', () => {
    assert(extractTokenDelta(null) === null, 'null payload must be null');
    assert(extractTokenDelta(undefined) === null, 'undefined payload must be null');
    assert(extractTokenDelta('not an object') === null, 'string payload must be null');
  });
});
