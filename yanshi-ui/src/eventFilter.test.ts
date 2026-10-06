// P6.5 filter-language self-test.

import { describe, it } from 'vitest';
import { applyFilter, compileFilter, type FilterEvent } from './eventFilter';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(`assert failed: ${message}`);
  }
}

const events: FilterEvent[] = [
  { seq: 0, kind: 'run_started', payload: { step_index: 0 } },
  { seq: 1, kind: 'node.started', payload: { step_index: 1, node_id: 'alpha' } },
  { seq: 2, kind: 'node.completed', payload: { step_index: 1, node_id: 'alpha' } },
  { seq: 3, kind: 'tool_call_started', payload: { step_index: 2, tool: 'shell' } },
  { seq: 4, kind: 'tool_call_completed', payload: { step_index: 2, tool: 'shell' } },
  { seq: 5, kind: 'run_completed', payload: { step_index: 3 } },
];

describe('compileFilter / applyFilter', () => {
  it('empty expression matches everything', () => {
    const filter = compileFilter('');
    assert(filter.error === null, 'empty filter parses ok');
    assert(applyFilter(events, filter).length === events.length, 'empty filter matches all');
  });

  it('kind= exact', () => {
    const filter = compileFilter('kind=tool_call_started');
    assert(filter.error === null, 'kind= no error');
    const matched = applyFilter(events, filter);
    assert(matched.length === 1 && matched[0].seq === 3, 'kind= isolates one event');
  });

  it('kind!= exact', () => {
    const filter = compileFilter('kind!=run_completed');
    const matched = applyFilter(events, filter);
    assert(matched.length === 5, 'kind!= excludes the named kind');
    assert(
      matched.every((e) => e.kind !== 'run_completed'),
      'kind!= preserves the other 5',
    );
  });

  it('kind~ substring (case-insensitive)', () => {
    const filter = compileFilter('kind~TOOL_CALL');
    const matched = applyFilter(events, filter);
    assert(matched.length === 2, 'kind~ substring case-insensitive');
    assert(
      matched.every((e) => e.kind.includes('tool_call')),
      'kind~ keeps tool_call_*',
    );
  });

  it('step>N reads payload.step_index', () => {
    const filter = compileFilter('step>1');
    const matched = applyFilter(events, filter);
    assert(matched.length === 3, `step>1 drops the early step, got ${matched.length}`);
  });

  it('step>=N', () => {
    const filter = compileFilter('step>=2');
    assert(applyFilter(events, filter).length === 3, 'step>=2 finds the right count');
  });

  it('AND between clauses', () => {
    const filter = compileFilter('kind~tool_call AND step=2');
    const matched = applyFilter(events, filter);
    assert(matched.length === 2, 'AND narrows to overlap');
    assert(
      matched.every((e) => e.kind.includes('tool_call')),
      'AND requires both clauses',
    );
  });

  it('AND with kind!=', () => {
    const filter = compileFilter('kind!=run_started AND kind!=run_completed');
    const matched = applyFilter(events, filter);
    assert(matched.length === 4, 'chained kind!= excludes both');
  });

  it('malformed clauses surface as errors without throwing', () => {
    const nonsense = compileFilter('nonsense');
    assert(nonsense.error !== null, 'malformed clause has error');
    assert(nonsense.predicate === null, 'malformed clause has null predicate');

    const badNumber = compileFilter('step>banana');
    assert(badNumber.error !== null, 'step with non-number errors');

    const trailingAnd = compileFilter('kind=foo AND');
    assert(trailingAnd.error !== null, 'trailing AND errors');
  });

  it('whitespace tolerance', () => {
    const filter = compileFilter('  kind  =   node.started   AND   step  >=  1  ');
    const matched = applyFilter(events, filter);
    assert(matched.length === 1 && matched[0].seq === 1, 'whitespace tolerated');
  });
});
