// P6.5 filter-language self-test. Runs via `node --import tsx
// src/eventFilter.test.ts`. We intentionally avoid pulling in a full
// test runner (vitest/jest) — the parser surface is small enough that
// straightforward assertions are easier to audit than test-framework
// scaffolding. CI invokes this through the agentflow-ui `npm test`
// script via `tsc --noEmit` for type safety; the runtime checks below
// run manually as a sanity pass.

import { applyFilter, compileFilter, type FilterEvent } from './eventFilter';

const events: FilterEvent[] = [
  { seq: 0, kind: 'run_started', payload: { step_index: 0 } },
  { seq: 1, kind: 'node.started', payload: { step_index: 1, node_id: 'alpha' } },
  { seq: 2, kind: 'node.completed', payload: { step_index: 1, node_id: 'alpha' } },
  { seq: 3, kind: 'tool_call_started', payload: { step_index: 2, tool: 'shell' } },
  { seq: 4, kind: 'tool_call_completed', payload: { step_index: 2, tool: 'shell' } },
  { seq: 5, kind: 'run_completed', payload: { step_index: 3 } },
];

let failures = 0;

function check(label: string, condition: boolean, detail?: string): void {
  if (!condition) {
    failures += 1;
    console.error(`FAIL ${label}${detail ? `: ${detail}` : ''}`);
  } else {
    console.log(`PASS ${label}`);
  }
}

// Empty expression matches everything.
{
  const filter = compileFilter('');
  check('empty filter parses ok', filter.error === null);
  check('empty filter matches all', applyFilter(events, filter).length === events.length);
}

// kind= exact
{
  const filter = compileFilter('kind=tool_call_started');
  check('kind= no error', filter.error === null);
  const matched = applyFilter(events, filter);
  check('kind= isolates one event', matched.length === 1 && matched[0].seq === 3);
}

// kind!= exact
{
  const filter = compileFilter('kind!=run_completed');
  const matched = applyFilter(events, filter);
  check('kind!= excludes the named kind', matched.length === 5);
  check('kind!= preserves the other 5', matched.every((e) => e.kind !== 'run_completed'));
}

// kind~ substring (case-insensitive)
{
  const filter = compileFilter('kind~TOOL_CALL');
  const matched = applyFilter(events, filter);
  check('kind~ substring case-insensitive', matched.length === 2);
  check('kind~ keeps tool_call_*', matched.every((e) => e.kind.includes('tool_call')));
}

// step>N reads payload.step_index
{
  const filter = compileFilter('step>1');
  const matched = applyFilter(events, filter);
  check('step>1 drops the early step', matched.length === 3, `got ${matched.length}`);
}

// step>=N
{
  const filter = compileFilter('step>=2');
  check('step>=2 finds the right count', applyFilter(events, filter).length === 3);
}

// AND between clauses
{
  const filter = compileFilter('kind~tool_call AND step=2');
  const matched = applyFilter(events, filter);
  check('AND narrows to overlap', matched.length === 2);
  check('AND requires both clauses', matched.every((e) => e.kind.includes('tool_call')));
}

// AND with kind!=
{
  const filter = compileFilter('kind!=run_started AND kind!=run_completed');
  const matched = applyFilter(events, filter);
  check('chained kind!= excludes both', matched.length === 4);
}

// Malformed clauses surface as errors without throwing.
{
  const filter = compileFilter('nonsense');
  check('malformed clause has error', filter.error !== null);
  check('malformed clause has null predicate', filter.predicate === null);
}
{
  const filter = compileFilter('step>banana');
  check('step with non-number errors', filter.error !== null);
}
{
  const filter = compileFilter('kind=foo AND');
  check('trailing AND errors', filter.error !== null);
}

// Whitespace tolerance.
{
  const filter = compileFilter('  kind  =   node.started   AND   step  >=  1  ');
  const matched = applyFilter(events, filter);
  check('whitespace tolerated', matched.length === 1 && matched[0].seq === 1);
}

if (failures > 0) {
  console.error(`\n${failures} failure(s)`);
  // `process` is a Node/Bun runtime global; the type isn't part of the
  // browser-leaning tsconfig used by the UI build. Tolerate that with
  // a small declaration here so `tsc --noEmit` stays clean even though
  // this file only runs under Bun / Node.
  (globalThis as unknown as { process: { exit(code: number): never } }).process.exit(1);
} else {
  console.log('\nAll filter expression tests passed.');
}
