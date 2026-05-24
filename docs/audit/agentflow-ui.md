# Audit: agentflow-ui

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-ui/ (TypeScript/React SPA, NOT Rust)
**Version**: 0.1.0 (from package.json)
**Layer**: L4 (Operations / Productization)
**Stability tier**: alpha (per CLAUDE.md)

## Scope summary

The agentflow-ui crate is a Vite-built React 19 SPA that the server
embeds at `/ui`. It is a thin client of the same `/v1/*` REST + SSE
contracts that the CLI uses. The implementation is a monolithic
`src/main.tsx` (2,624 LOC) covering 7 top-level routes
(`/ui`, `/ui/runs/new`, `/ui/runs/<id>/compare`, `/ui/diagnostics`,
`/ui/harness/sessions`, `/ui/harness/sessions/new`,
`/ui/harness/sessions/<id>`), plus three small companion modules
(`eventFilter.ts`, `preferences.ts`, `usePreferenceSync.ts`) and two
Playwright e2e specs. Dependencies are minimal: React 19, Vite 7,
TypeScript 5.8 + Playwright dev-only.

The product is functional for the alpha shell described in the
roadmap (run list, DAG status, event history, SSE updates, Harness
detail with approval cards), but several productization gaps remain
before it can serve as the headless-deployment console called out
in P6/P-H.5.

## Findings

### CRITICAL (XSS, auth bypass, data exposure)

- [C1] Bearer API token persisted to localStorage —
  `agentflow-ui/src/main.tsx:2581-2583` (write) and `:2573` (read),
  storage key `agentflow.ui.apiToken` defined at `:46`.
  **What**: The top-level `App` component writes `apiToken` to
  `window.localStorage` on every change and rehydrates it on mount.
  This contradicts the user-facing copy ("Bearer token (not
  persisted)" placeholders at `:361`, `:1197`, `:1498`, `:1733`,
  `:2069`). Only the per-form input is non-persisted; the global
  state DOES persist.
  **Why it matters**: localStorage is readable by any JavaScript on
  the same origin. The SPA's bundle is served by the same origin as
  the gateway; any future XSS sink, malicious third-party script
  (none today, but the bundle has no CSP), or a dev that copy-pastes
  a script into devtools can exfiltrate every operator's bearer
  token. The token has no rotation / scope info, so a leaked token
  is durable. The misleading "(not persisted)" UX makes the risk
  invisible to operators.
  **Fix**: Pick one of (a) drop the persistence — apiToken stays in
  React state only, operators paste it once per session; (b) move to
  httpOnly cookies set by `/v1/auth/login` server-side and remove
  bearer entirely from the SPA; (c) keep localStorage but update the
  copy honestly and add a "Forget token" button. The current
  half-state (persisted but labelled otherwise) is the worst option.

- [C2] Harness SSE stream fails under production auth profile —
  `agentflow-ui/src/main.tsx:1899` (`new EventSource(/v1/harness/sessions/${sessionId}/events)`).
  **What**: The native browser `EventSource` API cannot attach
  `Authorization: Bearer <token>` headers; only cookies travel. The
  gateway's `stream_harness_events` route
  (`agentflow-server/src/harness.rs:846`) extracts a `TenantId`
  through the same auth middleware as the rest of `/v1/*`, and
  `HarnessEventsQuery` has no `access_token` query-param fallback
  (`harness.rs:260`). The harness detail page will therefore 401 in
  every deployment that turns auth on.
  **Why it matters**: This is the headline feature of the Harness
  Mode Web UI (P-H.5 slice 3). It works in `local` profile (no
  auth) but silently degrades to the 5-second history-poll
  fallback (`main.tsx:1917-1921`) in production, meaning operators
  see approval prompts ~5s late instead of streamed live. The same
  approach is used inconsistently — the Workflow detail page wraps
  SSE in `fetch` with `Authorization` (`main.tsx:917-940`), proving
  the team knows the technique. The Harness path needs the same
  treatment.
  **Fix**: Replace the `EventSource` call with the same `apiFetch` +
  `ReadableStream` + `parseSseChunk` machinery used at
  `main.tsx:914-940`. Alternative: add an opt-in `?access_token=`
  query-param fallback on the server route (less ideal — token in
  request URLs leaks via access logs).

### MAJOR

- [M1] Single monolithic `main.tsx` of 2,624 lines — entire SPA in
  one file. Six top-level components (`RunCreateForm`,
  `RunConsole`, `RunCompare`, `HarnessSessionList`,
  `HarnessSubmitForm`, `HarnessSessionDetail`, `DiagnosticsPanel`)
  plus an `App` router live in the same module. Type definitions
  (`RunRecord`, `HarnessSession`, `HarnessEvent`,
  `PendingApproval`, `DiagnosticsReport`, ...) are mixed in line.
  **Impact**: Hard to navigate, hostile to code review, every git
  blame collides, multiplies merge conflicts as new pages land
  (P6.6+ has more planned). Encourages the copy-paste pattern
  observed in the form components.
  **Fix**: Split into `src/pages/` (one file per route), `src/api.ts`
  (the `apiFetch` + envelope types), `src/types.ts` (shared wire
  types), `src/components/` (reusable bits like `ApprovalCard`,
  `SummaryCard`). The two existing companion modules
  (`eventFilter.ts`, `preferences.ts`) already demonstrate this
  pattern works.

- [M2] No error boundary anywhere in the React tree —
  `agentflow-ui/src/main.tsx` (no `componentDidCatch` /
  `getDerivedStateFromError` / `ErrorBoundary` definitions found).
  **Impact**: Any uncaught render-time exception (e.g. a server
  returning a malformed payload that breaks one of the 23 unchecked
  `as X` casts — see [M3]) blanks the whole SPA. Operators lose
  whatever in-flight state they had (pasted token, draft workflow
  YAML, partially-typed approval scope).
  **Fix**: Wrap each route component in a single top-level
  `ErrorBoundary` that shows the error, a reload button, and a
  "Copy diagnostics" affordance. React 19 has the
  `errorElement` pattern via React Router, or a hand-rolled class
  component works.

- [M3] 23 unchecked `as Type` casts on server payloads — sample:
  `main.tsx:170` (`JSON.parse(data) as StreamedEvent`),
  `main.tsx:297` (`await response.json() as CreateRunEnvelope`),
  `main.tsx:474` (`as { events: StreamedEvent[] }`),
  `main.tsx:1438` (`as { sessions: HarnessSession[] }`),
  `main.tsx:1839` (`as HarnessSession`), `main.tsx:1860`
  (`as HarnessEvent[]`), `main.tsx:1877`
  (`as { approvals: PendingApproval[] }`), `main.tsx:2477`
  (`as DiagnosticsReport`).
  **Impact**: Type system gives false confidence — there is zero
  runtime validation of inbound JSON. A backend that adds a
  required field, returns `null` for a non-optional, or changes a
  status enum value (e.g. server adds `running` while the UI assumes
  `succeeded/failed/cancelled` per `isTerminalRun` at `:112`) will
  produce silent UI bugs rather than typed errors. The
  `harness.status` enum is hard-coded in two places
  (`isHarnessTerminal` `:1392`, `harnessStatusTone` `:1377`) with
  no shared schema.
  **Fix**: Either (a) add `zod` / `valibot` schemas at every
  response boundary (one-time cost, runs in tests too); or (b)
  generate types + parsers from the gateway's OpenAPI spec
  (`agentflow-server` would need to emit one); or (c) at minimum,
  centralize the parsing in `apiFetch` so a typo lives in one
  place.

- [M4] Reconnect loop captures stale `events` snapshot, replays
  from `seq=-1` — `main.tsx:1011`
  (`const lastSeq = events.at(-1)?.seq ?? -1;`) inside the catch
  branch of the SSE effect whose deps are `[runId, state, apiToken]`
  (`:1039`) with `events` deliberately excluded
  (`:1037 eslint-disable-next-line react-hooks/exhaustive-deps`).
  **Impact**: On the first SSE failure after a fresh `connect()`,
  `events` in the closure is still the empty array assigned at
  `:954` (`setEvents([])`). The retry therefore re-fetches the
  entire history rather than resuming from the last seen seq. The
  `appendEvent` dedup at `:906-907` rescues correctness but bills
  the bandwidth twice. Also, reconnect happens ONCE — if it fails
  again, the UI flips to `error` with no backoff or further retry.
  **Fix**: Keep `events` in a ref (`useRef<StreamedEvent[]>`) and
  read it inside the catch; or move the retry into a separate
  effect keyed on a `reconnectGeneration` counter. Add exponential
  backoff (250ms → 1s → 5s → cap) instead of a single fixed
  1.2s delay.

- [M5] Stacked polling intervals risk request pile-up —
  `main.tsx:1929` (sessionHandle every 2s),
  `:1918` (fallbackHandle every 5s when SSE errors),
  `:1449` (HarnessSessionList every 4s). None gate on
  "previous request still pending". Each interval calls
  `void fetchSession()` / `void refresh()` that race with each
  other. Under a slow API or large response, the browser queues
  multiple concurrent fetches; if any race-orders out of order, the
  UI shows stale data (last response wins, not most-recent
  request).
  **Impact**: At best, wasted bandwidth + battery; at worst, the
  approval card list flickers between "present" and "absent" if a
  late 2s-tick response overwrites a fresh 0s tick that already
  cleared the approval.
  **Fix**: Use the `AbortController` pattern already in use at
  `:752` to cancel an in-flight fetch before starting a new one,
  or guard with a `let inFlight = false` flag.

- [M6] No live updates on the Workflow run list view — `loadRuns`
  at `main.tsx:875` runs once on `(apiToken, tenantId)` change
  (effect at `:888`) and never refreshes. The CLAUDE.md
  "live SSE updates" claim applies only to the per-run detail view;
  the run *list* is stale until the operator changes tenant or
  reloads. The Harness session list got this right
  (`:1449` interval), but the runs list did not.
  **Impact**: Operators on the run console don't see new runs
  appear until they trigger a refresh. Minor UX gap; trivial fix.
  **Fix**: Mirror the 4-second `setInterval` already in
  `HarnessSessionList:1449`.

### MINOR

- [m1] Token-persistence regression test absent — the two e2e
  specs assert the *form-scoped* tokens never persist
  (`runs-new.spec.ts:96-98`, `harness-sessions.spec.ts:95-107`),
  but neither catches the actual leak at `main.tsx:2582`
  (`tokenKey` at the App level). Add a spec that types into the
  `create-token` field, reloads, and asserts
  `window.localStorage.getItem('agentflow.ui.apiToken')` is empty.

- [m2] Native unit tests not in CI — `eventFilter.test.ts` and
  `preferences.test.ts` are hand-rolled assertion scripts
  (`node --import tsx <file>`) but `package.json:10` runs only
  `tsc --noEmit` as `npm test`. The tests are never executed in
  CI. Add an `npm run test:unit` script and wire it into the
  workflow.

- [m3] Inconsistent URI encoding — `runId` is interpolated raw at
  `:944`, `:957`, `:968`, `:999`, `:1088`; `sessionId` at `:1834`,
  `:1853`, `:1870`, `:1980`, `:2011`, `:1899`. Other sites do
  `encodeURIComponent` (e.g. `:468`, `:570`, `:1662`, `:1954`).
  Practical risk is low (IDs are server-issued UUIDs today), but
  inconsistency invites future bugs if any field grows special
  chars. Pick one (always encode).

- [m4] Anchor-based navigation forces full reload — `<a href="/ui/...">`
  at `:319`, `:1129`, `:1467`, `:1678`, `:2051`, `:2054` triggers
  browser navigation, bypassing the in-app router's `App` state
  (only `popstate` is listened for, `:2576-2579`). Each click
  re-runs the full bundle init. With a 240KB bundle this is fine,
  but state like the pasted bearer token survives only because of
  the localStorage leak documented in [C1] — fixing [C1] would
  surface this UX regression. Add `onClick={ev => {
  ev.preventDefault(); window.history.pushState(...);
  setPathname(...); }}` to internal anchors, or adopt a tiny
  router.

- [m5] Hard-coded LLM default in form starter —
  `harnessFormStarterModel = 'moonshot-v1-auto'` at `:1560`.
  Deployments without Moonshot configured will see the prompt
  fail. Either source the default from the server's
  `/v1/diagnostics` (already fetched at `:2471`) or document this
  in `e2e/README.md`.

- [m6] Two `eslint-disable no-console` in
  `usePreferenceSync.ts:83,108` log to the browser console.
  `console.warn` is fine for diagnostics, but the messages include
  raw `err` which may contain server internals — e.g. a stack
  trace from the server's JSON error envelope. Consider passing
  only `err.message` or scrubbing.

- [m7] No `<meta name="viewport">` for mobile + no `<meta http-equiv="Content-Security-Policy">` —
  `index.html:5` has the standard `width=device-width` viewport
  but no CSP. The bundle does no inline scripts, so a strict
  `script-src 'self'` would be cheap to add and would substantially
  reduce the impact of any future XSS sink. Could be set
  server-side via response headers instead of meta.

- [m8] `<table>` row click navigation in `HarnessSessionList:1534`
  (`onClick={() => window.location.assign(...)}` on `<tr>`) is
  not accessible — no `role="button"`, no keyboard focusability,
  cmd+click cannot open in a new tab. Wrap the row content in an
  `<a>` instead.

- [m9] No virtualization on the event timeline — both
  `RunConsole` (`:1262`) and `HarnessSessionDetail` (`:2225`)
  render the full `events` list. A 10k-event run renders 10k
  `<li>` nodes. At the alpha scope this is fine; tracked for
  consideration if traces grow.

- [m10] `parseSseChunk` at `:157-175` calls `JSON.parse(data)`
  without try/catch. A malformed SSE frame (line that starts with
  `data:` but has invalid JSON) crashes the read loop, falls into
  the catch at `:1007`, and triggers the broken-reconnect path
  [M4]. Wrap in try/catch and skip the bad frame.

### POSITIVE OBSERVATIONS

- TypeScript strict mode IS enabled (`tsconfig.json:10 "strict": true`)
  and no `any` keyword appears anywhere in `src/`. The two
  `@ts-ignore`s are in e2e specs only, justifying optional dev-dep
  imports — the SPA proper is 100% typed.
- 2-space indentation observed throughout (per CLAUDE.md style rule);
  no tab characters.
- No `dangerouslySetInnerHTML`, no `eval`, no `new Function`, no
  `document.write`, no `innerHTML` assignment. All user/server data
  flows through React's text-escaping render path.
- All list renders include stable `key` props (verified at
  `:347`, `:706`, `:1168`, `:1222`, `:1264`, `:1532`, `:1705`,
  `:1719`, `:2211`, `:2228`, `:2557`).
- `eventFilter.ts` is genuinely well-designed: clean grammar
  comment block, fail-soft parsing (errors surface in `FilterResult.error`
  rather than throwing), 13 self-checking assertions in the
  companion test file.
- `preferences.ts` correctly documents the security exclusion list
  (api token, workflow drafts, harness prompts, machine-specific
  paths) and the `serverKeyForLocal` / `localKeyForServer` pair is
  symmetric + property-tested.
- Storage helpers (`readStorage`/`writeStorage` at `:115-129`) wrap
  `localStorage` in try/catch — Safari private-mode safe.
- SSE backfill + live merge in both `appendEvent` (`:904-912`) and
  `mergeEvent` (`:1816-1826`) are idempotent on `seq` — a known
  hazard handled correctly.
- AbortController cleanup in the run-detail effect (`:1030-1036`)
  prevents the documented "EventSource leak on unmount" failure
  mode. Harness detail's cleanup (`:1934-1941`) closes the
  EventSource similarly.
- `maskToken` at `:2380` correctly hides all but the last 4 chars
  of a displayed token (the diagnostics panel uses it).
- Playwright config (`playwright.config.ts:36-71`) is thoughtful:
  retry-on-flake in CI only, JUnit + HTML reports, traces only on
  retry, 30s test timeout, single Chromium project (justified
  comment).
- `e2e/README.md` is unusually clear documentation including a
  "Why not PR-gated" rationale block.

## Metrics

- Source files (.ts/.tsx, excluding e2e + dist): 5
  (`main.tsx`, `eventFilter.ts`, `eventFilter.test.ts`,
  `preferences.ts`, `preferences.test.ts`, `usePreferenceSync.ts`)
- E2E spec files: 2 (`runs-new.spec.ts`, `harness-sessions.spec.ts`)
- Lines of code:
  - `src/main.tsx` 2,624 (monolith — see [M1])
  - `src/eventFilter.ts` 168
  - `src/preferences.ts` 223
  - `src/usePreferenceSync.ts` 130
  - `src/styles.css` 1,503
  - `src/*.test.ts` 424 total
  - `e2e/*.spec.ts` 207 total
- Routes / pages: 7
  - `/ui` (`RunConsole`)
  - `/ui/runs/new` (`RunCreateForm`)
  - `/ui/runs/<id>/compare?against=<id>` (`RunCompare`)
  - `/ui/diagnostics` (`DiagnosticsPanel`)
  - `/ui/harness/sessions` (`HarnessSessionList`)
  - `/ui/harness/sessions/new` (`HarnessSubmitForm`)
  - `/ui/harness/sessions/<id>` (`HarnessSessionDetail`)
- Test files: 4 (2 unit-script, 2 Playwright e2e)
- `any` usages in `src/`: 0
- `@ts-ignore` usages: 2 (both in e2e for optional dev-dep import)
- `@ts-expect-error` / `@ts-nocheck` usages: 0
- `as X` unchecked structural casts on server payloads in main.tsx:
  ~23 (see [M3] for top 8)
- TODO/FIXME/XXX/HACK: 0 (zero — uncommonly clean)
- `eslint-disable` lines: 8 (6 `react-hooks/exhaustive-deps`,
  2 `no-console`) — all with justifying inline comments
- Error boundaries: 0 (see [M2])
- Bundle (`dist/assets/`): 240KB JS + 20KB CSS (very small)
- Components missing prop type annotations: 0 (every component has
  explicit `{ apiToken, onTokenChange, ... }: { ... }` props)

## Recommendations (prioritized)

1. **Fix [C1]** — pick a token-storage strategy and align UI copy
   with reality. This is the single biggest user-facing security
   regression and is a trivial code change (delete the persistence
   effect at `main.tsx:2581-2583`) once a product decision is made.
2. **Fix [C2]** — replace the harness `EventSource` with the
   `apiFetch` + ReadableStream + `parseSseChunk` pattern already in
   use at `main.tsx:914-940`. Without this, Harness Mode in any
   auth-enabled deployment silently falls back to 5-second polling.
3. **Address [M3]** — introduce runtime validation (zod /
   valibot) at every `response.json()` site or generate types from
   an OpenAPI spec. Cheapest meaningful improvement to long-term
   reliability.
4. **Split [M1]** — pull each top-level component into its own
   file under `src/pages/` and lift shared types into `src/types.ts`.
   This is foundational for any further UI work (P6 productization).
5. **Wrap [M2]** — one top-level `ErrorBoundary` plus a per-route
   recovery affordance. Cheap to write, prevents whitescreens.
6. **Tighten [M4] + [M5]** — fix the stale-snapshot reconnect, add
   exponential backoff, gate pollers on in-flight requests.
7. **CI-gate the existing unit tests [m2]** and add the regression
   test for [m1].
8. **Add CSP [m7]** server-side (response header) — `script-src 'self'`
   is sufficient given the bundle has no inline scripts.

End of report.
