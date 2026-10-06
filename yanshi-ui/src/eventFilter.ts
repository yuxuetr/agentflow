// P6.5 operator event filter — tiny expression language for the
// run-detail timeline.
//
// Grammar (case-insensitive keywords, whitespace flexible):
//
//   expr      := clause ( AND clause )*
//   clause    := kindClause | stepClause | kindLike
//   kindClause := 'kind' ('=' | '!=') VALUE
//   kindLike   := 'kind' '~' VALUE          (contains, case-insensitive)
//   stepClause := 'step' OP NUMBER
//   OP        := '>=' | '<=' | '!=' | '=' | '>' | '<'
//   VALUE     := non-space token
//   NUMBER    := signed integer
//
// Examples:
//   `kind=tool_call_completed`
//   `kind~node AND step>5`
//   `kind!=run_started`
//   `step>=10`
//   empty string  → match every event
//
// The parser is intentionally tolerant: unknown clauses surface as a
// structured `error` so the UI can render the message inline without
// crashing. Production-grade SQL is out of scope.

export interface FilterEvent {
  seq: number;
  kind: string;
  payload?: Record<string, unknown>;
  // Permit additional fields (e.g. `ts`) on the carrier struct so
  // consumers don't have to map their event types into a narrower
  // shape before filtering. The filter logic only reads
  // `seq` / `kind` / `payload`.
  [extra: string]: unknown;
}

export interface FilterResult {
  /** Compiled predicate; `null` when the expression failed to parse. */
  predicate: ((event: FilterEvent) => boolean) | null;
  /** Empty when `predicate` is non-null. */
  error: string | null;
}

interface Clause {
  test(event: FilterEvent): boolean;
}

class KindClause implements Clause {
  constructor(
    private readonly op: '=' | '!=' | '~',
    private readonly value: string,
  ) {}
  test(event: FilterEvent): boolean {
    const eventKind = event.kind.toLowerCase();
    const target = this.value.toLowerCase();
    switch (this.op) {
      case '=':
        return eventKind === target;
      case '!=':
        return eventKind !== target;
      case '~':
        return eventKind.includes(target);
    }
  }
}

type Op = '>' | '>=' | '<' | '<=' | '=' | '!=';

class StepClause implements Clause {
  constructor(private readonly op: Op, private readonly threshold: number) {}
  test(event: FilterEvent): boolean {
    const step = readStepIndex(event);
    if (step === null) {
      // Events without a step_index are ambiguous — exclude them from
      // every `step` clause so the operator's intent is preserved.
      return false;
    }
    switch (this.op) {
      case '>':
        return step > this.threshold;
      case '>=':
        return step >= this.threshold;
      case '<':
        return step < this.threshold;
      case '<=':
        return step <= this.threshold;
      case '=':
        return step === this.threshold;
      case '!=':
        return step !== this.threshold;
    }
  }
}

function readStepIndex(event: FilterEvent): number | null {
  const payload = event.payload ?? {};
  const candidates = ['step_index', 'step', 'seq'];
  for (const key of candidates) {
    const value = (payload as Record<string, unknown>)[key];
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }
  }
  // Fall back to the event's own seq so `step>5` is at least
  // meaningful for events that don't carry step_index in their payload.
  if (Number.isFinite(event.seq)) {
    return event.seq;
  }
  return null;
}

/** Parse an expression into a compiled predicate. Empty input matches everything. */
export function compileFilter(input: string): FilterResult {
  const trimmed = input.trim();
  if (trimmed.length === 0) {
    return { predicate: () => true, error: null };
  }
  // Split on the AND keyword (case-insensitive, surrounded by
  // whitespace). The simple regex is sufficient because our values
  // can't contain unescaped whitespace by definition.
  const rawClauses = trimmed.split(/\s+AND\s+/i);
  const clauses: Clause[] = [];
  for (const raw of rawClauses) {
    const piece = raw.trim();
    if (piece.length === 0) {
      return { predicate: null, error: 'empty clause between AND' };
    }
    const result = parseClause(piece);
    if (result.error) {
      return { predicate: null, error: result.error };
    }
    clauses.push(result.clause as Clause);
  }
  const predicate = (event: FilterEvent) => clauses.every((c) => c.test(event));
  return { predicate, error: null };
}

function parseClause(raw: string): { clause?: Clause; error?: string } {
  // kind ~ <value>
  const kindLike = /^kind\s*~\s*(\S+)$/i.exec(raw);
  if (kindLike) {
    return { clause: new KindClause('~', kindLike[1]) };
  }
  // kind = / kind != <value>
  const kindEq = /^kind\s*(!=|=)\s*(\S+)$/i.exec(raw);
  if (kindEq) {
    return { clause: new KindClause(kindEq[1] as '=' | '!=', kindEq[2]) };
  }
  // step <op> <number>
  const stepMatch = /^step\s*(>=|<=|!=|>|<|=)\s*(-?\d+)$/i.exec(raw);
  if (stepMatch) {
    const op = stepMatch[1] as Op;
    const threshold = Number.parseInt(stepMatch[2], 10);
    if (!Number.isFinite(threshold)) {
      return { error: `clause '${raw}': threshold is not a finite integer` };
    }
    return { clause: new StepClause(op, threshold) };
  }
  return { error: `clause '${raw}' did not match kind=…, kind!=…, kind~…, or step<op>N` };
}

/** Convenience: run a compiled predicate against a list of events. */
export function applyFilter<T extends FilterEvent>(events: T[], filter: FilterResult): T[] {
  if (!filter.predicate) {
    return events;
  }
  return events.filter((event) => filter.predicate!(event));
}
