# RFC: `agentflow-mcp` Protocol Modernization

- Status: **Proposed** — design only, no code changed yet. Written per
  `TODOs.md` W5.7's instruction (itself split out of W5.6 for being
  "large scope, promote to an independent section") and per the user's
  explicit direction this round: research + write an RFC before touching
  any code, given the scope discovered below is substantially larger than
  W5.7's original framing assumed.
- Parent: `TODOs.md` W5.6/W5.7 (`docs/PROJECT_EVALUATION_2026-08-09.md`
  §4.6 "生态短板" — ecosystem-compatibility gap). W5.6's contained scope
  (protocol version *verification*, capability gating) is closed; this RFC
  covers everything W5.6 explicitly deferred: Streamable HTTP transport
  and connection-level reconnect, **plus** a materially bigger finding
  surfaced while researching the transport work (below).
- Scope: research and a phased implementation plan. No `agentflow-mcp`
  code changes are proposed to land alongside this document.

## tl;dr

`agentflow-mcp` targets MCP protocol version `2024-11-05` exclusively —
the *first* released revision, using a transport (HTTP+SSE) that has been
**deprecated** since `2025-03-26`. The real spec has since shipped three
more revisions, and the **current stable version as of this writing is
`2026-07-28`**, which is not an incremental change: it **removes the
`initialize` handshake / capabilities-exchange model entirely** for the
modern flow, replacing it with a stateless, per-request design (every
request self-describes its version and identity; a mandatory
`server/discover` RPC replaces `initialize` for capability discovery; no
persistent sessions). `agentflow-mcp`'s entire client/server architecture
— `MCPClient::connect()`/`initialize()`, `ClientCapabilities`/
`ServerCapabilities`, the very `require_server_capability` gate W5.6 just
added — is what the spec now calls the **"Legacy" era** (`2025-11-25` and
earlier), and specifically the *oldest* legacy sub-era, since it doesn't
even implement the newer Legacy-era Streamable HTTP transport
(`2025-03-26`–`2025-11-25`, session-based, since itself superseded).

**Concrete, business-relevant consequence**: per the spec's own
compatibility matrix (reproduced below), a strictly-Modern MCP client
talking to `agentflow-mcp`'s server surface today **fails outright** — not
gracefully, not with a clean error the client can act on, just fails.
This will only get more common as the ecosystem moves onto `2026-07-28`.

This RFC is **not** a plan to rewrite the crate. It recommends a
**dual-era bridge**: keep every existing Legacy-era behavior byte-for-byte
(the server surface is Beta-stable per `docs/STABILITY.md` — breaking it
is not on the table), and add Modern-era support alongside it, so
`agentflow-mcp` can act as a Modern client to today's real-world servers
and — eventually — as a dual-era server.

## Background: what W5.6 already covers, and what this RFC covers instead

W5.6 (closed, commit `3a87922`) added strict `protocolVersion` **equality
verification** to `client/session.rs::initialize()` — reject a mismatch,
don't silently accept it — plus capability gating for the 8 client methods
that skipped checking `ServerCapabilities` before firing a request. Both
were scoped, at the time, as *hardening the existing Legacy-era model*,
explicitly **not** "genuine multi-version negotiation... only meaningful
once a second version's shape exists." This RFC is the follow-through on
that explicit deferral: a second version's shape (in fact, three) does
exist, and the deferred work — real negotiation, a Modern-era path,
Streamable HTTP, reconnect — needs a real design before implementation,
which is what follows.

## The spec, precisely (fetched live from modelcontextprotocol.io; not from training-data recollection — the crate's own audit doc got this wrong by relying on memory, see below)

### Version timeline

| Version | Era | Transport(s) | Status |
|---|---|---|---|
| `2024-11-05` | Legacy | HTTP+SSE (dual-endpoint: POST + separate GET SSE) | **Deprecated** since `2025-03-26`, eligible for removal (SEP-2596) |
| `2025-03-26` | Legacy | Streamable HTTP v1 (session-based: `Mcp-Session-Id`, GET SSE stream, resumable via `Last-Event-ID`, server can send requests on SSE) | Superseded |
| `2025-11-25` | Legacy | Streamable HTTP v1 (same shape as above) | Superseded, last Legacy revision |
| `2026-07-28` | **Modern** | Streamable HTTP v2 (stateless: no sessions, no GET stream, no resumability; POST-only, SSE scoped per-request) | **Current** (stable) |

`agentflow-mcp` implements **only** `2024-11-05` — the deprecated
transport, one revision before Streamable HTTP existed at all.

### The architectural break, precisely

**Legacy** (what `agentflow-mcp` has today, matches `2025-11-25` and
earlier): client opens a connection, sends `initialize` with its
`ClientCapabilities`, server responds with `ServerCapabilities` +
`protocolVersion` it picked, client sends `notifications/initialized`,
*then* a session exists and subsequent requests are scoped to it.
`agentflow-mcp`'s entire `MCPClient` struct — `connected`,
`server_capabilities`, `server_info`, the whole `client/session.rs` — is
built around this lifecycle.

**Modern** (`2026-07-28`, current): no handshake, no session. Every
request carries `_meta.io.modelcontextprotocol/protocolVersion` +
`clientInfo` + `clientCapabilities` inline; the server "accepts or rejects
each request independently." A server that doesn't support the requested
version responds with `UnsupportedProtocolVersionError` (code `-32022`)
listing what it does support — the client retries with a mutually
supported version. `server/discover` (mandatory for servers to implement)
lets a client learn supported versions/capabilities/identity up front in
one call, but calling it is *optional* — a client can invoke any RPC
inline and handle the version error reactively.

On Streamable HTTP specifically (`2026-07-28`): every POST additionally
carries `MCP-Protocol-Version`, `Mcp-Method`, and (for
`tools/call`/`resources/read`/`prompts/get`) `Mcp-Name` HTTP headers
mirroring the JSON-RPC body, so intermediaries can route without parsing
the body; the server validates header/body agreement and rejects
mismatches with `HeaderMismatch` (`-32020`). Responses are either a single
`application/json` object or an SSE stream **scoped to that one request**
(carries `notifications/progress`/`notifications/message` for that
request, then the final response, then closes) — no more persistent GET
stream, no `Mcp-Session-Id`, no resumability. Server-to-client
interactions that used to be independent JSON-RPC requests on the SSE
stream (sampling, elicitation, roots) are now embedded as
`InputRequiredResult` inside the *response* to the original request — the
client answers by **retrying the same request** with `inputResponses`
attached (the "Multi Round-Trip Requests" / MRTR pattern, SEP-2322), not
by handling an inbound request.

### Compatibility matrix (from the spec, verbatim structure)

| Client | Server | Outcome |
|---|---|---|
| Modern | Modern | Works |
| Modern | Legacy | **Fails** — undefined/implementation-specific rejection |
| Dual-era | Modern | Works (probes, stays modern) |
| Dual-era | Legacy | Works (probes, falls back to `initialize`) |
| Legacy | Modern | **Fails** — no fall-forward mechanism for a legacy client |
| Legacy | Dual-era | Works |
| Legacy | Legacy | Works |

`agentflow-mcp`'s server is Legacy. Its client is Legacy. Both the
"Modern client → our Legacy server" row and the "our Legacy client →
Modern server" row are in the **Fails** set today.

### Era detection (for the dual-era bridge this RFC recommends)

- **stdio**: probe with `server/discover` first. A `DiscoverResult` (or a
  recognized modern error like `UnsupportedProtocolVersionError`) means
  Modern; anything else (unrecognized error, or none — `initialize` was
  never a thing on the modern side) means Legacy, fall back to
  `initialize`.
- **Streamable HTTP**: attempt a modern per-request POST first. Success or
  a *recognized* modern JSON-RPC error body (still `400`, but with a body
  shaped like `UnsupportedProtocolVersionError`/`HeaderMismatch`) means
  Modern. A `400`/`404`/`405` with an unrecognized or empty body means
  Legacy — fall back further to a plain `initialize` POST, then (if that
  also fails in the Legacy-server-signature way) to the deprecated
  HTTP+SSE transport's `GET`-returns-an-`endpoint`-event probe.
- Era is a property of the **server process** (stdio) or **origin**
  (HTTP), not of an individual request — cache the result for the
  connection's lifetime; re-probe only if a cached assumption later fails.

## Correction to this crate's own audit doc

`docs/audit/agentflow-mcp.md` (2026-05-24) doesn't mention any of this —
it was written when `2024-11-05` may genuinely have still been closer to
current, or simply didn't check. Its "M2" finding ("HTTP transport gap...
no MCP-related entry in RoadMap.md") is directionally right but
understates the problem: the gap isn't "missing an HTTP transport for the
version we target," it's "the version we target is deprecated and the
spec's shape has changed twice since." Worth a follow-up doc-audit pass
once this RFC's Phase 2/3 lands, separate from this RFC itself.

## Design: three options considered

### Option A — Dual-era bridge (recommended)

Keep every Legacy-era code path exactly as it is (honors the Beta
stability promise on `MCPServer`; zero behavior change for the 4 existing
downstream consumers). Add a parallel Modern-era path:

- **Client**: era-probe (per transport, as above) with a per-server cached
  result; Modern path sends per-request `_meta`, handles
  `UnsupportedProtocolVersionError` by retrying with a mutually supported
  version, handles `InputRequiredResult`/MRTR by re-issuing the request
  with `inputResponses`.
- **Transport**: implement Streamable HTTP (`2026-07-28` shape — POST
  only, response is single JSON or per-request-scoped SSE). See "Transport
  trait fit" below — this looks implementable *without* redesigning the
  existing `Transport` trait, contrary to what was assumed when W5.7 was
  first filed.
- **Server**: implement `server/discover` + `MCP-Protocol-Version`/
  `Mcp-Method`/`Mcp-Name` header validation on a *new* endpoint, while the
  existing `initialize`-based stdio path keeps serving Legacy clients
  unchanged — a genuine dual-era server per the spec's own model ("A
  dual-era server MAY serve both eras concurrently on the same endpoint or
  process").

### Option B — Modern-only rewrite (rejected)

Drop the handshake model, target `2026-07-28` only. Reaches "spec current"
fastest, but breaks `docs/STABILITY.md`'s Beta promise on `MCPServer`
outright, and breaks the connect()/initialize()-shaped assumptions baked
into all 4 real downstream consumers (`agentflow-skills::McpClientPool`,
3 `agentflow-cli mcp` subcommands, 2 independent node implementations).
Rejected — no compatibility story for existing embedders, and the blast
radius is unjustified when Option A gets the same end state without it.

### Option C — Narrow stepping stone: bump to `2025-11-25` only

Stay entirely within the Legacy/handshake model (near-zero architectural
change — `2025-11-25` still uses `initialize`); add its Streamable HTTP
shape (session-based, `Mcp-Session-Id`, GET SSE, resumable) as an
additional transport alongside stdio. Smaller than Option A, and a
legitimate *first phase* if the team wants to de-risk incrementally — but
it does not fix the "Modern client can't reach our server" problem, and a
second modernization pass would still be needed once `2025-11-25`-era
servers themselves age out (which, per the timeline above, they already
are). Not recommended as an end state; **could** be folded into Option A's
Phase 2 as a smaller first milestone if the phased plan below is felt to
be too much to take in one pass — flagged as a sequencing choice, not
re-litigating the overall direction.

## Transport trait fit (a specific, load-bearing finding)

`docs/audit/agentflow-mcp.md` and the original W5.7 TODO entry both
assumed the current `Transport` trait (`send_message(&self, req) ->
MCPResult<Value>` + separate `receive_message(&self) ->
MCPResult<Option<Value>>` for out-of-band messages) would need
redesigning for a "write-once-read-many" SSE flow. Re-reading
`transport/traits.rs` and the Modern spec together suggests otherwise:

- `send_message`'s contract ("send a request and wait for the matching
  response") maps directly onto a POST whose response is either a single
  JSON object (trivial) **or** an SSE stream scoped to exactly that
  request (read the stream, forward any `notifications/*` events to the
  same internal queue `StdioTransport::run_reader_task` already feeds for
  `receive_message`, and resolve `send_message`'s return value when the
  final JSON-RPC response event arrives, then the stream closes on its
  own by spec — no lingering read loop to manage past that point).
- The Modern spec's per-request-scoped SSE is actually *simpler* than what
  the trait's `receive_message`/notifications-queue machinery was built
  to support (`StdioTransport`'s indefinite background reader demuxing an
  unbounded stream) — there's no persistent GET stream to manage in the
  current spec at all (removed in `2026-07-28`; even `2025-03-26`–
  `2025-11-25`'s version is gone now).
- `StdioTransport`'s existing architecture — background task + per-request
  `oneshot` correlation by JSON-RPC id, notifications routed to a separate
  mpsc channel — is structurally the *same shape* a Streamable HTTP
  transport needs per-POST, just swapping the I/O source. This is a real
  precedent to reuse, not a redesign target.

**Recommendation**: do not redesign `Transport` speculatively. Attempt the
Streamable HTTP implementation directly against the current trait in
Phase 2 below; if a genuine shape mismatch turns up during that
implementation spike (not found by this research pass), redesign then,
informed by a concrete blocker rather than a hypothetical one.

## Phased plan

Each phase is independently shippable and independently verifiable
(matches this session's established discipline: full test suite + clippy
+ fmt + check-arch + full workspace build before each commit).

### Phase 1 (this document) — done
Research + design. No code.

### Phase 2 — Modern-era client support (highest value; recommended next)
- Era-probe + cache (stdio: `server/discover`; note this crate's own
  server will need to implement `server/discover` before the *client*
  can meaningfully self-test against it — Phase 2 can still ship against
  real external Modern servers without waiting for Phase 3).
- Per-request `_meta` construction, `UnsupportedProtocolVersionError`
  retry-with-supported-version handling, MRTR/`InputRequiredResult`
  re-issue handling.
- Streamable HTTP transport (client side): `reqwest` (already a workspace
  dependency, rustls-pinned since W5.5 — zero new version to introduce),
  hand-rolled minimal SSE frame parsing for the per-request-scoped stream
  (no SSE-parsing crate exists in this workspace today; the wire format
  needed is narrow enough — `event:`/`data:` lines terminated by a blank
  line, plus `:`-prefixed keep-alive comments to ignore — that hand-rolling
  avoids a new dependency; revisit if real-world server responses turn out
  messier than the spec's happy path).
- New `TransportType` variant (the existing `Http`/`HttpWithSSE` names
  reflect the *old*, now-fully-superseded HTTP+SSE and session-based
  Streamable-HTTP shapes respectively — neither name fits the `2026-07-28`
  stateless shape; naming TBD at implementation time, e.g.
  `StreamableHttp`).
- Legacy path (stdio, `initialize`) stays completely untouched.

### Phase 3 — Modern-era server support (dual-era)
- Implement `server/discover` (mandatory-for-servers RPC).
- New Streamable HTTP server endpoint: `MCP-Protocol-Version`/`Mcp-Method`/
  `Mcp-Name` header validation, `HeaderMismatch` (`-32020`) /
  `UnsupportedProtocolVersionError` (`-32022`) error responses.
- Existing `initialize`-based stdio server path (the Beta-pinned 4-method
  surface) stays byte-for-byte unchanged — this phase is additive only.
- Once stable, add a `docs/STABILITY.md` entry for the new surface
  (currently `client`/`transport` carry no compatibility promise at all,
  per W5.6's research — this phase is where that changes and a promise
  should be written down).
- Extend the `tests/server_contracts.rs` fixture-pinning pattern to the
  new wire shapes.

### Phase 4 — connection-level reconnect (small, independent, can be done anytime — including before Phases 2/3)
Reframed from the original W5.7 filing after reading
`agentflow-skills::McpClientPool` closely: **the lazy-reconnect machinery
already exists** (`ensure_client`/`ensure_client_for_tool` rebuild a fresh
client whenever the cached slot is `None`). The actual gap is narrower
than "build reconnect from scratch" — it's that only the `tokio::time::
timeout` path in `McpClientPool::call_tool` clears the cached slot on
failure; a real (non-timeout) connection error from `client.call_tool(...)`
(e.g. the child process died and the next write/read errors immediately,
not via timeout) falls through `result.map_err(...)` **without** clearing
the slot, leaving a known-dead client cached for every subsequent call
until something explicitly calls `disconnect()`. `list_tools` has the same
gap (no error-path slot-clearing at all). Fix: after any `MCPError` classified
transient/connection-level (`error.rs::MCPError::is_transient()` already
exists for exactly this classification, reused by `client/retry.rs`),
clear the pool slot the same way the timeout path does. Small, contained,
no new dependencies — can land independently of Phases 2/3 if there's
appetite to close it sooner.

## Consumer impact (blast radius, unchanged from W5.6's research)

Changes to `MCPClient`/`ClientBuilder`/`Transport` ripple to 4 real
consumers: `agentflow-skills::McpClientPool` (the canonical adapter),
`agentflow-cli`'s 3 `mcp` subcommands, `agentflow-nodes-ai::mcp` node, and
`agentflow-agents::mcp_tool_node` (marked `Experimental`, and — as an
aside noticed during this research, not this RFC's main concern — an
entirely separate, non-shared client-construction path from the
`nodes-ai` node; worth considering whether to unify them as a small
adjacent cleanup whenever Phase 2 lands, since Phase 2's era-probing
logic would otherwise need duplicating into a second call site).

## Explicitly out of scope

- Extension negotiation (`capabilities.extensions` — e.g. the MCP Apps or
  Tasks extensions mentioned in the spec). No current requirement
  identified for this workspace.
- WebSocket/gRPC transports (present in the old, stale
  `docs/MCP_PRODUCTION_DESIGN.md`'s "Future Enhancements" appendix from
  2025-10-27; never requested since, not revisited here).
- Rewriting/removing the deprecated `2024-11-05` HTTP+SSE transport shape
  — not applicable, since this crate never implemented it as a *transport*
  in the first place (only stdio exists today); nothing to remove.

## Verification plan (once implementation phases begin)

Same discipline as every prior W-track item this session: full
`cargo test -p agentflow-mcp --lib --tests`, `clippy -D warnings`,
`fmt --check`, `cargo xtask check-arch`, full
`cargo build --workspace --all-targets` before each phase's commit.
Phase 2/3 additionally need either a real external Modern MCP server to
test the client against (or a hand-built `MockTransport`-style fixture
speaking the Modern wire shape — extend `transport/mock.rs`'s pattern) and
new fixture files under `tests/fixtures/` mirroring
`server_contracts.rs`'s existing pinning approach for whatever new
request/response shapes Phase 3 introduces.
