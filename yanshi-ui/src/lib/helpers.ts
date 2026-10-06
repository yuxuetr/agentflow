// Q3.7.1: pure utility helpers shared across page components.
// Non-React, no DOM side-effects (except `formatTime` which calls into
// Intl.DateTimeFormat in browser timezones).

import type { RunEnvelope, RunRecord, StreamedEvent } from '../schemas';

/**
 * Format an ISO timestamp into a wall-clock HH:MM:SS string in the
 * operator's local timezone. Returns "pending" when the input is null
 * / undefined (the server emits `null` for the finish time of a run
 * that hasn't completed yet).
 */
export const formatTime = (value?: string | null): string => {
  if (!value) {
    return 'pending';
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(value));
};

/**
 * The `/v1/runs` and `/v1/runs/{id}` endpoints share a wire shape but
 * the single-run response is wrapped in `{ run: ... }` while the list
 * surface returns the record verbatim. Normalise to the record so the
 * rest of the UI doesn't have to branch.
 */
export const runFromEnvelope = (value: RunEnvelope): RunRecord => value.run ?? value;

/**
 * Render-side classification of an event kind into a CSS tone class.
 * The mapping is intentionally loose — we look for substring matches
 * rather than exact event-name equality so future event kinds with
 * the same semantic category pick up the right colour automatically.
 */
export const eventTone = (kind: string): 'danger' | 'tool' | 'agent' | 'success' | 'neutral' => {
  const lower = kind.toLowerCase();
  if (lower.includes('fail') || lower.includes('error') || lower.includes('denied')) {
    return 'danger';
  }
  if (lower.includes('tool') || lower.includes('policy') || lower.includes('capability')) {
    return 'tool';
  }
  if (
    lower.includes('agent') ||
    lower.includes('reflect') ||
    lower.includes('plan') ||
    lower.includes('step')
  ) {
    return 'agent';
  }
  if (lower.includes('complete') || lower.includes('succeed')) {
    return 'success';
  }
  return 'neutral';
};

export const prettyJson = (value: unknown): string => JSON.stringify(value, null, 2);

/** A run is terminal iff its status is one of succeeded/failed/cancelled. */
export const isTerminalRun = (run: RunRecord | null): boolean =>
  run ? ['succeeded', 'failed', 'cancelled'].includes(run.status) : true;

/**
 * Generic "find the last item satisfying `predicate`" — equivalent to
 * `items.findLast(...)` but supports the older ES targets and is
 * explicit about reverse iteration so callers know it's O(n) worst case.
 */
export const findLatest = <T,>(items: T[], predicate: (item: T) => boolean): T | undefined => {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (predicate(items[index])) {
      return items[index];
    }
  }
  return undefined;
};

/**
 * Best-effort extraction of the DAG node id from a workflow SSE event's
 * payload. Different event kinds spell the field differently
 * (`node_id`, `node_name`, `node`, `step`) — try each in priority order.
 */
export const eventNodeId = (event: StreamedEvent): string => {
  const payload = event.payload as Record<string, unknown>;
  return String(payload.node_id ?? payload.node_name ?? payload.node ?? payload.step ?? '').trim();
};

/**
 * Q3.7.3: exponential-backoff schedule for SSE reconnects.
 *
 * Returns the delay in milliseconds before the *next* reconnect attempt
 * given the current attempt counter (zero-indexed). The schedule
 * doubles each time and caps at 30 seconds:
 *
 *   attempt 0 →   250 ms
 *   attempt 1 →   500 ms
 *   attempt 2 → 1,000 ms
 *   attempt 3 → 2,000 ms
 *   attempt 4 → 4,000 ms
 *   attempt 5 → 8,000 ms
 *   attempt 6 → 16,000 ms
 *   attempt 7+ → 30,000 ms (cap)
 *
 * Extracted as a pure function so the schedule can be unit-tested
 * without spinning up a React render tree.
 */
export const reconnectDelayMs = (attempt: number): number => {
  const safe = Math.max(0, Math.floor(attempt));
  const base = 250 * Math.pow(2, safe);
  return Math.min(base, 30_000);
};
