# Memory Layering

Status: design as of `P4.5`; backend implementations closed under
P4.7 (2026-05-24, `5098719`); Preference wiring (config, prompt
injection, write tool) closed under U2.2 (2026-07-31) — see the
"Wiring status" note under §Precedence for what remains unwired
(Entity facts).
Crate: `agentflow-memory`.
Implements: P4.7 (backend implementations) and P-H.4 (background task
context).

AgentFlow agents accumulate four kinds of state that are easy to
conflate but have very different lifetime, retrieval, and privacy
requirements. This document is the v1 boundary contract between those
four kinds so that:

1. Implementations of [`MemoryStore`](../agentflow-memory/src/store.rs)
   stay focused on one layer at a time.
2. Agents (`ReActAgent`, `PlanExecuteAgent`, supervisors) get a
   deterministic precedence when more than one layer can answer the
   same prompt-assembly question.
3. Retention rules can be enforced per layer instead of relying on a
   single global TTL.

## Layers

Four mutually exclusive layers. Each implementation declares which
layer it serves; an agent runtime may attach at most one store per
layer.

| Layer | Lifetime | Keyed by | Primary read API | Retention default |
| --- | --- | --- | --- | --- |
| **Session** | Single conversation | `session_id` | `get_history(session_id, limit)` | Token-windowed (`prune` on write) |
| **Semantic** | Cross-session, soft | embedding similarity | `search_semantic(query, k)` | Vector store TTL; defaults to keep |
| **Preference** | User-scoped, durable | `(tenant_id, user_id, key)` | `get_preference(key)` | Keep until explicit delete |
| **Entity facts** | Topic-scoped, durable | `(entity_id, fact_id)` | `get_facts(entity_id)` | Keep with provenance until invalidated |

### 1. Session memory

Holds the rolling conversation transcript: every `Message` (system /
user / assistant / tool) that the agent has emitted or received in
the active session. The defining property is *recency* — once a
message falls outside the token window it is evicted, and the agent
substitutes a summary produced by a `MemorySummaryBackend`.

Today's implementation:

- [`SessionMemory`](../agentflow-memory/src/session.rs) — in-process
  `HashMap<session_id, Vec<Message>>` with `prune` triggered on every
  write. `default_window()` ships an 8 000-token window;
  `large_window()` ships 128 000 for long-context models.
- [`SqliteMemory`](../agentflow-memory/src/sqlite.rs) — persistent
  Session memory backed by SQLite. Same shape as `SessionMemory`
  semantically; the difference is durability across process
  restarts. Used by skills that opt in with `[memory] type = "sqlite"`.

Retention: bounded by the token budget. Old messages are evicted
oldest-first; system messages are pinned and never evicted by
`prune`. There is no time-based TTL — a session that goes silent
keeps its full history until it is either explicitly cleared or the
process exits (for `SessionMemory`).

### 2. Semantic memory

Holds embeddings of (a) past messages worth recalling across sessions
and (b) optional "memory cards" the agent decides to commit. The
defining property is *similarity retrieval*: an agent runtime
queries the layer with an embedding and gets back the k nearest
items.

Today's implementation:

- [`SemanticMemory`](../agentflow-memory/src/semantic.rs) — wraps an
  `agentflow_rag::embeddings::EmbeddingProvider` and stores
  `Message + Vec<f32>` pairs. Search uses cosine similarity
  in-process (no Qdrant dependency for the in-memory backend).

#### Seam with RAG

Semantic memory and the RAG retriever (`agentflow-rag::retrieval`)
both perform "find nearest k by embedding", but they answer
different questions:

| Use case | Layer | Why |
| --- | --- | --- |
| "Did the user tell me their middle name in a prior conversation?" | Semantic memory | The data is conversational, ephemeral, user-scoped, and not part of any authored knowledge base. |
| "What does the company policy say about refunds?" | RAG | The data is an authored corpus, chunked at ingestion, shared across users / tenants, and updated by a deliberate `agentflow rag ops index` invocation. |
| "What's the last error message this agent saw for this customer?" | Semantic memory | Same as the first row — generated as a side-effect of the agent loop, not authored. |
| "Give me the API reference page for the `flow.resume` method." | RAG | Authored doc corpus. |

Rule of thumb: if the data was produced *by the agent itself* during
runtime, it lives in semantic memory. If it was produced by a human
or upstream pipeline and indexed offline, it lives in RAG. The two
stores never alias the same document.

Implementations may share an embedding model and a vector backend
(both Qdrant and the in-process implementation are valid for either
layer); the boundary is data ownership, not infrastructure.

### 3. Preference memory

Holds durable, user-scoped key / value pairs the agent learns over
time: preferred language, default verbosity, opt-outs, project-
specific aliases. The defining property is *exactness* — the agent
asks "what is the value of key `tone`?" and expects either a single
value back or `None`.

Today's implementation (T4.1, closed 2026-05-24 under P4.7 —
`5098719`):

- [`SqlitePreferenceStore`](../agentflow-memory/src/preference.rs) —
  SQLite-backed, schema `(tenant_id, user_id, key, value JSON,
  updated_at, version)`, UPSERT with monotonic per-key `version`.
- [`AgeEncryptedPreferenceStore`](../agentflow-memory/src/preference_encrypted.rs) —
  `age`-encrypted-at-rest wrapper (P10.7.2) for deployments that need
  it; the plaintext `SqlitePreferenceStore` remains the local-profile
  default.

Retention: keep indefinitely. Operators can prune via
`agentflow memory prune --layer preference --older-than 1y`
(`agentflow-cli/src/commands/memory/prune.rs`).

### 4. Entity facts memory

Holds extracted, structured facts about entities the agent
encounters — people, projects, codebases, files, accounts. The
defining property is *provenance*: each fact has the source
`message_id` (or `tool_call_id`) it was extracted from, a confidence
score, and an extraction timestamp. Conflicting facts about the same
`(entity, attribute)` pair are kept as separate rows, not merged, so
the agent runtime can render a per-fact citation when challenged.

Today's implementation (T4.1, closed 2026-05-24 under P4.7 —
`5098719`):

- [`SqliteEntityFactStore`](../agentflow-memory/src/entity_facts.rs) —
  SQLite-backed, schema `(entity_id, fact_id, attribute, value JSON,
  source_message_id, confidence, extracted_at, invalidated_at
  NULLABLE, invalidation_reason)`.

Retention: keep until explicitly invalidated. Invalidation sets
`invalidated_at`; the row is preserved for audit. A separate
`agentflow memory prune --layer entity_facts --older-than 2y`
(`EntityFactStore::prune_invalidated`) removes invalidated rows past a
grace window — active facts are never touched, even at a zero cutoff.

## Layer trait surface

The existing [`MemoryStore`](../agentflow-memory/src/store.rs)
trait covers the Session layer well today. P4.7 extends the trait
surface conservatively: rather than adding methods to `MemoryStore`
(which would force every backend to stub the methods it doesn't
support), each layer gets a dedicated trait that *extends*
`MemoryStore` only where it makes sense.

```rust
// agentflow-memory/src/store.rs (existing — covers Session today)
#[async_trait]
pub trait MemoryStore: Send + Sync {
  async fn add_message(&mut self, message: Message) -> Result<(), MemoryError>;
  async fn get_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>, MemoryError>;
  async fn get_all(&self, session_id: &str) -> Result<Vec<Message>, MemoryError>;
  async fn search(&self, session_id: &str, query: &str, limit: usize) -> Result<Vec<Message>, MemoryError>;
  async fn clear_session(&mut self, session_id: &str) -> Result<(), MemoryError>;
  async fn session_token_count(&self, session_id: &str) -> Result<u32, MemoryError>;
  async fn to_prompt(&self, session_id: &str) -> Result<String, MemoryError>;
}

// agentflow-memory/src/layer.rs (landed under P4.7, T4.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryLayer { Session, Semantic, Preference, EntityFacts }

#[async_trait]
pub trait SemanticMemoryStore: MemoryStore {
  async fn search_semantic(
    &self,
    session_id: Option<&str>,
    query: &str,
    k: usize,
  ) -> Result<Vec<(Message, f32)>, MemoryError>;
}

// U2.6: moved to `agentflow-store-spi::preference` with `&self` write
// methods (originally `&mut self`; re-auditing found that constraint
// wasn't load-bearing — `SqlitePreferenceStore` only touches
// `&self.pool`, an `Arc`-backed `sqlx::SqlitePool`). `agentflow-memory`
// re-exports it under this same `PreferenceStore` path.
#[async_trait]
pub trait PreferenceStore: Send + Sync {
  async fn get_preference(
    &self,
    scope: &PreferenceScope,
    key: &str,
  ) -> Result<Option<PreferenceValue>, MemoryError>;
  async fn put_preference(
    &self,
    scope: &PreferenceScope,
    key: &str,
    value: serde_json::Value,
  ) -> Result<(), MemoryError>;
  async fn delete_preference(
    &self,
    scope: &PreferenceScope,
    key: &str,
  ) -> Result<(), MemoryError>;
  async fn list_preferences(
    &self,
    scope: &PreferenceScope,
  ) -> Result<Vec<(String, PreferenceValue)>, MemoryError>;
  async fn prune_older_than(&self, older_than: Duration) -> Result<u64, MemoryError>;
}

#[async_trait]
pub trait EntityFactStore: Send + Sync {
  async fn record_fact(&mut self, fact: EntityFact) -> Result<(), MemoryError>;
  async fn get_facts(
    &self,
    entity_id: &str,
    include_invalidated: bool,
  ) -> Result<Vec<EntityFact>, MemoryError>;
  async fn invalidate_fact(
    &mut self,
    entity_id: &str,
    fact_id: &str,
    reason: &str,
  ) -> Result<(), MemoryError>;
  async fn prune_invalidated(&mut self, older_than: Duration) -> Result<u64, MemoryError>;
}
```

Implementations: [`SqlitePreferenceStore`](../agentflow-memory/src/preference.rs)
/ [`SqliteEntityFactStore`](../agentflow-memory/src/entity_facts.rs). `PreferenceValue`
wraps the stored `value` with its `updated_at`/`version` metadata on
read; writes take a bare `serde_json::Value` since the store computes
those fields itself.

Rationale: `SemanticMemoryStore` extends `MemoryStore` because every
semantic backend is also a valid session backend (you can read
recent messages directly without going through similarity).
`PreferenceStore` and `EntityFactStore` do *not* extend `MemoryStore`
because their data shapes are not messages — keeping the trait
hierarchies separate prevents accidental dispatch through the wrong
read API.

## Retention / forgetting policy

| Layer | Default | Operator override | Per-message override |
| --- | --- | --- | --- |
| Session | Token-windowed (8 000 / 128 000) | `[memory] window_tokens = N` in `skill.toml` | n/a |
| Semantic | Keep until explicit delete | `agentflow memory prune --layer semantic --older-than DUR` | n/a |
| Preference | Keep indefinitely | `agentflow memory prune --layer preference --older-than DUR` (purges last-updated > DUR ago) | `--user-id` filter |
| Entity facts | Keep until invalidated; invalidated rows kept 2 y | `agentflow memory prune --layer entity_facts --hard-delete --older-than DUR` | `--entity-id` filter |

The CLI subcommand lands alongside P4.7 — schema design now, command
later.

## Precedence at prompt-assembly time

When an agent runtime builds the prompt, it asks each layer for
context in this fixed order:

1. **Session** — full history under the token budget (the working
   set the LLM already remembers).
2. **Preference** — exact-match facts about the active user (tone,
   language, opt-outs). Always small, always inserted into the
   persona.
3. **Entity facts** — structured facts about entities named in the
   current turn. Inserted with citations.
4. **Semantic** — top-k similar messages from past sessions, gated
   by a relevance threshold so unrelated chatter doesn't leak in.

Why this order: high-trust data first (session is verbatim,
preference is exact, entity facts have provenance). Semantic is last
because it's the noisiest layer and most likely to retrieve
something that *looks* relevant but isn't.

A `MemorySummaryBackend` (already in `agentflow-agents/src/reflection.rs`)
operates **before** this list: when session history overflows the
token budget, the summary backend compacts the oldest messages into
a single synthetic message that takes their slot.

**Wiring status (U2.2, 2026-07-31)**: **Preference is wired end to end.**
`ReActAgent` reads `PreferenceStore::list_preferences` fresh every turn
and injects it into the persona (`agentflow-agents/src/react/agent.rs`),
and a Skill can both configure it (`[memory.preference]`, see below) and
*write* to it mid-conversation via the new `remember_preference` tool
(`agentflow_memory::RememberPreferenceTool`, registered automatically
by `SkillBuilder` whenever `[memory.preference]` is enabled) — the store
was previously usable only "standalone" (a direct Rust API call, or
`agentflow memory prune` for retention), so nothing in a real
conversation could ever populate it; that gap is now closed.
`PlanExecuteAgent` does **not** get this — the precedent it would mirror
(`task_summary_store`/`project_memory_store`) was never added there
either, so this is a pre-existing, unrelated gap, not new to U2.2.

**Entity facts remains unwired** — `EntityFactStore::get_facts` is keyed
by a single `entity_id`, so consulting it automatically requires first
deciding *which* entities are "named in the current turn" (real NLU-ish
work, not just a store read); the store itself is unaffected and still
only reachable standalone / via `agentflow memory prune`. No
`agentflow-skills`/`agentflow-agents`/`agentflow-harness` code
references `EntityFactStore` yet. Wiring it is unscheduled future work
(tracked in `TODOs.md`).

## Migration path

Today (`v0.3.0`):

- `SessionMemory` and `SqliteMemory` already implement the Session
  layer. No change needed.
- `SemanticMemory` already implements similarity search but is
  surfaced through `MemoryStore::search`. Under P4.7 it gains a
  `SemanticMemoryStore` impl that exposes the typed `search_semantic`
  API. The existing `search(session_id, query, k)` route stays for
  one stability tier (Beta) before being deprecated.
- Preference and Entity facts stores landed as new code (T4.1, P4.7,
  `5098719`) — no existing rows to back-fill.
- Preference *wiring* (config parsing, prompt injection, the
  `remember_preference` write tool) landed U2.2 (2026-07-31).

Skill manifest impact — `agentflow-skills`' `[memory]` parsing accepts
`type = "session" | "sqlite" | "semantic" | "none"` (all four; a prior
version of this document incorrectly said only three — `semantic` has
been handled by `SkillBuilder::build_memory` since 2026-04-25, the gap
was a stale validator allowlist, fixed alongside U2.2). `[memory.preference]`
is real as of U2.2; `[memory.entity_facts]` remains proposed, not
implemented (see the Wiring status note above for why):

```toml
# skill.toml (existing — no change required)
[memory]
type = "session"  # or "sqlite" | "semantic" | "none"
window_tokens = 12000

# skill.toml (U2.2 — implemented)
[memory.preference]
enabled = true                                  # optional, defaults to true
db_path = "~/.agentflow/memory/my-skill.preference.db"  # optional override;
                                                  # defaults to
                                                  # ~/.agentflow/memory/<skill_name>.preference.db

# skill.toml (proposed, not implemented — no SkillBuilder support yet)
[memory.entity_facts]
type = "sqlite"
path = "~/.agentflow/memory/{skill}.facts.db"  # optional override
```

The existing `[memory]` table is unchanged. `[memory.preference]` is
independent of the primary `type` above — it can be combined with any
of `session`/`sqlite`/`semantic`/`none`. `[memory.entity_facts]` is
still the proposed additive shape for when that layer's wiring lands;
until then no skill.toml can attach a facts store.

## Stability

- `MemoryStore` trait: **stable**. Already at v1.
- `MemoryLayer` enum, `SemanticMemoryStore`, `PreferenceStore`,
  `EntityFactStore`, `PreferenceScope`, `EntityFact`: **experimental**
  at first land under P4.7, promote to Beta in the next release after
  one stable skill ships a preference or facts integration.

See `docs/STABILITY.md` for the stability tier definitions.

## Related

- `docs/AGENT_RUNTIME.md` — how the agent loop consumes memory.
- `docs/RAG_EVAL.md` — the eval harness for the authored corpus.
- `docs/AGENT_SDK.md` — `MemorySummaryBackend` extension trait.
- [`agentflow-memory/src/store.rs`](../agentflow-memory/src/store.rs) —
  the trait this document extends.
- [`agentflow-memory/src/semantic.rs`](../agentflow-memory/src/semantic.rs) —
  the current `SemanticMemory` implementation.
- P-H.4 `tasks` module — background task agents reuse the same
  layer contract when they spawn an inner runtime.
