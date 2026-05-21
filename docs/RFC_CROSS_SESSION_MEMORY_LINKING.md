# RFC: Cross-session memory linking strategy

**Status**: Design RFC (P10.7.3). Not a binding spec — frames the v2
design conversation per [`docs/ROADMAP_v2.md`](./ROADMAP_v2.md) Theme B
("Memory / RAG"). Promotion to a real P11.x line item gates on the
concrete demand signals in the [Promotion criteria](#promotion-criteria)
section.

**Owner**: P10.7.3 (closed as design-doc only).
**Companion documents**: [`MEMORY_LAYERING.md`](./MEMORY_LAYERING.md)
(the four-layer model this RFC builds on),
[`H6_PROMOTION_CRITERIA.md`](./H6_PROMOTION_CRITERIA.md) and
[`WASM_PLUGIN_EVALUATION.md`](./WASM_PLUGIN_EVALUATION.md) (peer
"decide-when-to-revisit" design notes).

---

## Problem statement

The current four-layer memory model (Session / Semantic / Preference /
Entity facts) handles within-layer recall well but leaves **explicit
cross-session linkage** under-specified. When an agent has a
conversation about Project X in Session A and a follow-up about
Project X in Session B (same user, different session), it can recover
*some* of the prior context — but the recovery path is opportunistic,
not designed.

Concretely:

* **What works today** (no design work needed): preferences persist
  across sessions because they're user-scoped, not session-scoped.
  Entity facts about Project X stay live across sessions for the same
  reason. Semantic memory can surface relevant transcript fragments
  via vector search.
* **What doesn't work today**:
  1. *Session inventory per entity.* "Which past sessions discussed
     Project X?" requires a full-corpus scan; there's no index from
     entities → sessions.
  2. *Context-aware session resumption.* When the agent encounters
     a known entity in a new session, it can't surface a curated
     "you previously discussed X in Sessions Y on Z dates" header.
  3. *Implicit knowledge graph queries.* "Who else has Daisy talked
     to about AgentFlow?" requires walking across entity facts +
     session participants, which the current schema doesn't model.
  4. *Conflict resolution across sessions.* Two sessions producing
     contradictory facts about the same `(entity, attribute)` are
     stored as separate rows (per the existing entity-facts
     provenance design), but there's no explicit "these conflict"
     signal beyond the schema's permissiveness.

The TODO that gave rise to this RFC frames this honestly: *"a 'memory
graph' linking entities across sessions is a v2 design
conversation."* The point of the RFC is to **frame the conversation**,
not pre-decide the implementation.

## Status quo

The four memory layers already provide a substantial baseline. Before
designing anything new, document explicitly what cross-session
linkage *already exists*:

| Layer | Cross-session today? | Mechanism |
|-------|----------------------|-----------|
| Session | No (by design) | Each session has its own `session_id`; no implicit join. |
| Semantic | Yes, opportunistic | Embedding-based vector search over the user's full corpus; sessions are an internal partition that the retriever can choose to ignore. |
| Preference | Yes, durable | User-scoped, not session-scoped. `(tenant_id, user_id, key)` primary key. |
| Entity facts | Yes, durable | User-scoped + entity-scoped. `(entity_id, attribute, source_message_id)` rows survive sessions. The `source_message_id` carries provenance back to a specific session if needed. |

The honest framing: **three of four layers already cross sessions
automatically.** What's missing is a *navigable index* — given an
entity, find its session footprint; given a session, find its entity
neighbours.

## Use cases

Concrete scenarios driving the RFC. Each one is checked against
"can the current four-layer model do this?" — that's the test for
whether linkage actually adds value.

### UC1 — Session inventory per entity

> *"List the sessions where Daisy discussed AgentFlow's harness
> module."*

Today: requires a full scan of `entity_facts` filtered by entity_id =
"harness_module" + then dereferencing each fact's `source_message_id`
to its session. O(N) in facts about the entity. Could be made
O(1)-ish with a covering index but the join is still per-row.

Linked design: a direct `session_participants(entity_id, session_id,
first_mentioned_at, last_mentioned_at, mention_count)` index. O(1)
lookup, but writes cost more (every entity mention now touches two
tables).

### UC2 — Session resumption with entity context

> *"At the start of Session B (same user), the agent recognises
> Project X is mentioned and proactively surfaces a 'previously
> discussed in Sessions [A, …]' header."*

Today: requires UC1's scan at session startup, which is feasible but
not curated (raw session list, no ranking).

Linked design: the same session-participants index, with an additional
`relevance_score` or `last_mentioned_at` for ordering. Top-K
truncation at session start = curated header.

### UC3 — Implicit knowledge graph queries

> *"Who has Daisy collaborated with on AgentFlow this quarter?"*

This needs proper graph semantics: walk from User(Daisy) → entities
they've discussed → other users who've discussed those same
entities. The current entity facts schema has *one* implicit edge
(user-discussed-entity); the second hop isn't queryable.

Linked design: an explicit `discusses(user_id, entity_id, ...)` edge
table. With it, the query becomes a two-hop SQL join.

### UC4 — Cross-session fact conflict surfacing

> *"In Session A, Daisy said Project X uses Rust. In Session B
> (same user), Daisy said Project X uses Go. Surface the conflict."*

Today: both facts coexist in `entity_facts` with different
`source_message_id`s. Retrieval at prompt-assembly time returns both
but doesn't flag the conflict — the agent sees two contradictory
context lines and must reconcile in-prompt.

Linked design: explicit `conflicts_with` edges between fact rows,
populated by a background reconciliation pass. Conflict detection
becomes a query, not an inference.

### UC5 — Time-bounded session linkage

> *"What did Daisy discuss in the 3 sessions immediately before
> this one?"*

Today: orderable via Session metadata (`created_at`), but no
*direct* "previous N sessions for this user" query without a full
sessions table scan.

Linked design: small. Just an index on `sessions(user_id,
created_at DESC)`. Doesn't require a graph at all.

## Design space

Three possible implementation directions, in order of increasing
ambition:

### Option A — Index-only (additive SQLite)

**Scope**: add two SQLite tables to the existing memory database:

```sql
CREATE TABLE session_entities (
  session_id          TEXT NOT NULL,
  entity_id           TEXT NOT NULL,
  first_mentioned_at  TEXT NOT NULL,
  last_mentioned_at   TEXT NOT NULL,
  mention_count       INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (session_id, entity_id)
);

CREATE INDEX session_entities_by_entity
  ON session_entities(entity_id, last_mentioned_at DESC);

CREATE TABLE entity_fact_conflicts (
  fact_a_id    TEXT NOT NULL,
  fact_b_id    TEXT NOT NULL,
  detected_at  TEXT NOT NULL,
  PRIMARY KEY (fact_a_id, fact_b_id)
);
```

Maintenance: a write-path hook on `EntityFactStore::insert_fact` (or
on the prompt-assembly entity-extraction step) updates
`session_entities`. Conflict detection runs as a background job
(daily? on-demand?).

**Coverage**: UC1, UC2, UC5. Partial UC4 (the table exists, the
detection logic is operator-defined). No UC3 directly — would need
a second join through `session_entities` (entity X → entity Y via
sessions where both were mentioned), which is awkward in SQL.

**Trade-offs**:
- ✅ Zero new dependencies. Same SQLite schema migrations as the
  existing memory layers.
- ✅ All existing trait surface stays. New methods on
  `EntityFactStore` (or a new `SessionEntityIndex` trait).
- ❌ Two-hop queries (UC3) require manual joins; doesn't feel like
  a graph natively.
- ❌ Maintenance burden on every fact insert. Background reconcile
  job is operator-defined complexity.

### Option B — Embedded graph store

**Scope**: introduce a lightweight embedded graph layer alongside
SQLite. Candidates: [`oxigraph`](https://crates.io/crates/oxigraph)
(RDF triple store, has SQLite backend), [`indradb`](https://crates.io/crates/indradb)
(property graph, RocksDB backend), [`cozo`](https://crates.io/crates/cozo)
(Datalog + relational hybrid).

The memory layer's trait surface stays; a new
`MemoryGraphStore` trait adds:

```rust
trait MemoryGraphStore {
  async fn add_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind, ...);
  async fn walk(&self, from: NodeId, kind: EdgeKind, depth: usize) -> Vec<NodeId>;
  async fn shortest_path(&self, from: NodeId, to: NodeId) -> Option<Path>;
}
```

**Coverage**: All five UCs natively. UC3 in particular becomes a
two-edge walk.

**Trade-offs**:
- ✅ Native graph queries — multi-hop traversals are first-class.
- ✅ Each graph backend has its own query language (Cypher, SPARQL,
  Datalog) which is more ergonomic than ad-hoc SQL joins.
- ❌ A new dependency, with its own version / stability story.
- ❌ Two storage backends to keep in sync (SQLite for layers, graph
  store for edges).
- ❌ Operator-facing complexity: another DB to back up, another
  one to monitor, another one to migrate.

### Option C — Vector-graph hybrid

**Scope**: extend the existing Semantic-memory layer (which already
uses embeddings + Qdrant) to model *typed* nodes + edges. Each entity
gets an embedding; relationships are derived from co-occurrence in
sessions + cosine similarity over entity-context embeddings.

**Coverage**: UC1, UC2, UC3 (fuzzily), partial UC4. UC5 still wants
SQLite time-bounded queries.

**Trade-offs**:
- ✅ Builds on existing infrastructure (no new DB).
- ✅ Fuzzy graph queries (`find entities semantically near
  Project X`) work natively via vector search.
- ❌ Edges are derived, not explicit — conflict detection (UC4) and
  exact session inventory (UC1) need a separate index anyway.
- ❌ Vector-based "linking" is statistical, not symbolic — surprising
  results when entity names alias each other.

## Recommendation

**None yet** — this RFC deliberately doesn't pre-decide. The
honest read of the use cases:

* **UC1 + UC2 + UC5** are the highest-value, lowest-cost wins. All
  three are achievable with **Option A** alone.
* **UC3** is the headline "knowledge graph" feature but is also the
  one users rarely ask for in practice (per the dogfooding traces
  from N9 / P9 closure). Probably premature to optimise for.
* **UC4** is genuinely useful but doesn't require a graph store —
  a background reconcile job over Option A's `entity_fact_conflicts`
  table covers it.

Therefore the **pragmatic default**, when this RFC is promoted to a
P11.x line item, is **Option A**. Options B + C stay parked as v2
exploratory tracks; revisit when concrete UC3-shaped demand surfaces.

## Non-goals

To keep the RFC scope honest:

- **Full knowledge-graph backend** (Neo4j, Memgraph, etc.). Out of
  scope for the local profile — operator complexity tax is too
  high. If a hosted/team deployment wants a graph store, that's a
  separate v2 SaaS-track conversation.
- **Cross-tenant linkage.** Sessions for different `tenant_id`s
  remain isolated. The `(tenant_id, user_id)` scope of the existing
  trait surface is preserved.
- **Real-time entity extraction**. Existing extraction happens at
  message-time (P4.5 design); the linkage tables don't change that.
  Asynchronous batch extraction is a different RFC.
- **Memory-graph for agent runtime decisions** (e.g. ReAct picks
  which session to recall). That's an agent-side design, not a
  memory-layer design — separate Theme C RFC if it surfaces.

## Open questions

For the v1.x → v2 transition, these need real-world signal before a
P11 line item commits to one direction:

1. **Maintenance write-path overhead.** How much of a hit does
   updating `session_entities` on every entity mention impose on
   the agent loop? Need a profiling pass on a realistic workload.
2. **Conflict detection cadence.** Background job vs. on-demand at
   query time vs. write-time check? Each has different latency /
   complexity trade-offs.
3. **Entity ID stability.** The existing entity facts model uses
   raw `entity_id: String`. Cross-session linkage assumes these
   IDs are stable across sessions; in practice the extractor may
   produce `"project_x"` vs. `"Project X"` vs. `"the X project"`.
   A canonicalisation pass is implicit prerequisite.
4. **Sessions that span days.** A long-running harness session has
   different "last-mentioned-at" semantics than a 5-message chat
   session. Need to decide: is the index per-message or
   per-session-boundary?
5. **Retention interaction.** When a session is pruned (per
   retention policy), what happens to the `session_entities` rows
   pointing at it? Cascade delete? Tombstone? Per-row TTL?

## Promotion criteria

This RFC graduates to a real P11.x line item when **any one** of the
following signals fires:

1. **Concrete UC1/UC2 operator request.** A real user (not internal
   dogfooding) reports "I want to know which past sessions discussed
   X" as a workflow blocker.
2. **Three or more skill manifests** in the wild start hand-rolling
   session-inventory queries against the existing `entity_facts`
   table (i.e. operators are already paying the O(N) scan cost).
3. **A Harness Mode session running 4+ weeks** surfaces a real
   conflict-resolution incident that Option A's
   `entity_fact_conflicts` would have caught.
4. **External demand** — a forked deployment or commercial
   integration explicitly asks for the graph traversal surface.

Until then, the four-layer model already covers the durable-memory
baseline (preferences + entity facts + semantic), and the
incremental value of explicit linkage doesn't pencil out against
the maintenance + complexity cost.

## Migration considerations (when promoted)

Whichever option lands eventually:

- **Schema changes are additive** to the existing memory DB. No
  breaking changes to the `MemoryStore` / `PreferenceStore` /
  `EntityFactStore` trait surfaces.
- **Backfill is opt-in.** Existing sessions don't auto-populate the
  link tables; users who want them backfilled run an
  `agentflow memory rebuild-index` command (to be designed).
- **Stability tier** starts at Experimental. Graduates to Beta after
  one minor-release cycle of operator dogfooding.

## Cross-references

- [`MEMORY_LAYERING.md`](./MEMORY_LAYERING.md) — the four-layer
  baseline.
- [`ROADMAP_v2.md`](./ROADMAP_v2.md) Theme B (Memory / RAG) — the
  parent track this RFC sits under.
- [`H6_PROMOTION_CRITERIA.md`](./H6_PROMOTION_CRITERIA.md) — peer
  "decide-when-to-revisit" doc with the same shape.
- [`WASM_PLUGIN_EVALUATION.md`](./WASM_PLUGIN_EVALUATION.md) — peer
  v2 evaluation that closed with "deferred, here are the triggers."
- `TODOs.md` P10.7.3 — closes this entry as the design deliverable.

---

_End of RFC. Updates land as commits on this file referencing
P11.x sub-items once promoted._
