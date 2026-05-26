// Q3.7.1: HTTP / SSE helpers shared by every page component. Lives in
// `lib/` so individual pages don't have to re-import from `main.tsx`
// (which would create a circular dep once pages live in `pages/`).

import { parseJson } from '../schemas';

/**
 * Attach the operator-supplied bearer token to a fetch request, then
 * call `fetch`. Trims whitespace and skips the header entirely when
 * the token is empty so the gateway treats it as "no auth provided"
 * rather than "literal empty token" (the gateway distinguishes those).
 */
export const apiFetch = (path: string, token: string, init: RequestInit = {}): Promise<Response> => {
  const headers = new Headers(init.headers);
  const trimmed = token.trim();
  if (trimmed) {
    headers.set('Authorization', `Bearer ${trimmed}`);
  }
  return fetch(path, {
    ...init,
    headers,
  });
};

/**
 * Generic SSE chunk parser used by both the workflow runs detail view
 * (`StreamedEventSchema` shape) and the harness session detail view
 * (`HarnessEventSchema` shape). Caller picks the schema so the same
 * chunk parser can validate either wire shape without sharing the
 * `run_id` / `session_id` field name (Q3.7.2).
 *
 * A malformed event payload that fails validation now surfaces as a
 * `SchemaValidationError` the caller can recover from, instead of
 * leaking into downstream rendering with `kind=undefined` / `seq=NaN`.
 */
export const parseSseChunk = <T,>(
  buffer: string,
  schema: import('zod').ZodType<T>,
  contextLabel: string,
): { events: T[]; rest: string } => {
  const events: T[] = [];
  let cursor = buffer;
  let boundary = cursor.indexOf('\n\n');
  while (boundary >= 0) {
    const raw = cursor.slice(0, boundary);
    cursor = cursor.slice(boundary + 2);
    const data = raw
      .split('\n')
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice(5).trimStart())
      .join('\n');
    if (data) {
      events.push(parseJson(schema, JSON.parse(data), contextLabel));
    }
    boundary = cursor.indexOf('\n\n');
  }
  return { events, rest: cursor };
};
