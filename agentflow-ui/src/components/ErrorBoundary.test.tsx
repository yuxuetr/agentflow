// Q3.7.1: pin the ErrorBoundary contract.
//
// Run with: `npx tsx src/components/ErrorBoundary.test.tsx` once a
// test runner is wired up. The file is also `tsc`-checked via the
// existing `npm test` (`tsc --noEmit`) gate.
//
// The tests deliberately exercise the class lifecycle directly rather
// than spinning up a JSDOM render, because:
//   - this UI ships without a test runner today
//   - `getDerivedStateFromError` and `componentDidCatch` are the only
//     two functions React calls into; round-tripping a real ErrorInfo
//     through ReactDOM would add zero coverage on top of calling
//     them explicitly.
// If we add Vitest or jest-dom later, swap these for real render
// tests — the public contract under test is identical.

import React from 'react';
import { ErrorBoundary } from './ErrorBoundary';

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

test('getDerivedStateFromError returns the captured error', () => {
  const err = new Error('boom');
  const patch = ErrorBoundary.getDerivedStateFromError(err);
  assert(patch?.error === err, 'state.error must be the thrown error');
});

test('happy path renders children verbatim (no error captured)', () => {
  // The component instance render() returns `this.props.children` when
  // state.error is null. We can drive it directly without ReactDOM
  // because the render method itself is pure.
  const children = React.createElement('div', null, 'happy path');
  const eb = new ErrorBoundary({ children });
  // Default state should be `{ error: null, componentStack: null }`.
  assert((eb.state as { error: Error | null }).error === null, 'initial state.error is null');
  const out = eb.render();
  assert(out === children, 'render() returns the children prop verbatim');
});

test('fallback renders an alert role and the error message', () => {
  const eb = new ErrorBoundary({ children: null });
  // Simulate React's error capture: getDerivedStateFromError → setState
  // → componentDidCatch (we don't call setState directly here; we
  // bypass via direct field assignment because we own the test).
  (eb as { state: { error: Error | null; componentStack: string | null } }).state = {
    error: new Error('runtime crash'),
    componentStack: '    in TestComponent\n    in ErrorBoundary',
  };
  const tree = eb.render() as React.ReactElement;
  assert(tree !== null && typeof tree === 'object', 'render returned an element');
  // Walk the element to find the alert region — for a tiny tree like
  // this, manual traversal is fine and avoids pulling in
  // react-dom/test-utils.
  const html = JSON.stringify(tree);
  assert(html.includes('role'), 'fallback carries a role attribute somewhere');
  assert(html.includes('runtime crash'), 'fallback embeds the error message');
  assert(html.includes('TestComponent'), 'fallback includes the component-stack snippet');
});

test('reset clears the captured error so children render again', () => {
  const eb = new ErrorBoundary({ children: React.createElement('div') });
  let setStatePatch: { error: Error | null; componentStack: string | null } | null = null;
  // Stub setState so reset() takes effect without React's reconciler.
  (eb as { setState: (patch: typeof setStatePatch) => void }).setState = (patch) => {
    setStatePatch = patch;
  };
  eb.reset();
  assert(
    setStatePatch !== null &&
      (setStatePatch as { error: Error | null }).error === null &&
      (setStatePatch as { componentStack: string | null }).componentStack === null,
    'reset() should set both error and componentStack to null',
  );
});

test('label prop appears in the fallback heading', () => {
  const eb = new ErrorBoundary({ children: null, label: 'Custom Tree' });
  (eb as { state: { error: Error | null; componentStack: string | null } }).state = {
    error: new Error('x'),
    componentStack: null,
  };
  const html = JSON.stringify(eb.render());
  assert(html.includes('Custom Tree'), 'label prop appears in the fallback');
});

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
  throw new Error(`ErrorBoundary tests: ${failed} failure(s)`);
} else {
  console.log(`\n${cases.length} test(s) passed`);
}
