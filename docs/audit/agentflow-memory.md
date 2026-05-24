# Audit: agentflow-memory

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: `agentflow-memory/`
**Crate version**: 0.1.0 (per `Cargo.toml`; package version intentionally pinned below the workspace 0.3.0-alpha line)
**Layer**: L2 (Capability Adapter)
**Stability tier**: Mixed — `MemoryStore` is documented as **Stable** in `lib.rs:15`; the four newer traits (`MemoryLayer`, `PreferenceStore`, `EntityFactStore`, `SemanticMemoryStore`) are explicitly **Experimental** at first land (`lib.rs:16-18`, `layer.rs:16`). `AgeEncryptedPreferenceStore` is **alpha** (no in-tree consumer yet).

## Scope summary

Source tree: 11 `*.rs` files in `src/`, ~3531 lines (3745 LOC including the single integration test). Three storage flavours co-exist:

- **In-process**: `SessionMemory` (HashMap + token-windowed sliding eviction).
- **SQLite-persistent**: `SqliteMemory` (messages), `SqlitePreferenceStore` (per-user K/V), `SqliteEntityFactStore` (provenance-tracked facts), `SemanticMemory` (messages + embedding BLOBs).
- **Encryption-at-rest wrapper**: `AgeEncryptedPreferenceStore<S: PreferenceStore>` (age/X25519 over any `PreferenceStore`).

Four trait surfaces: `MemoryStore` (store.rs), `PreferenceStore`, `EntityFactStore`, `SemanticMemoryStore` (all in `layer.rs`). One `TokenCounter` abstraction in `types.rs` (heuristic default; precise BPE bridged via `agentflow-llm::counter_for_model` per the comment in `types.rs:46-55`). Dependencies are minimal and stay within the L2 boundary: `agentflow-rag` (for the `EmbeddingProvider` trait only, `default-features = false`), `sqlx`, `age`, `chrono`, `uuid`, `serde_json`, `tracing`. **No** dependency on `agentflow-core` or `agentflow-llm` — the trait-only coupling to `EmbeddingProvider` is the right call.

Tests: 1 integration test (`tests/cross_layer_precedence.rs`) + extensive `#[cfg(test)]` per-module suites (token-window eviction, JSON round-trip, cosine math, encryption marker contract, identity-file mode 0600, scope isolation, prune semantics). No `examples/` directory.

Downstream callers in workspace: `agentflow-cli` (memory prune CLI, eval, harness), `agentflow-skills` (builder), `agentflow-harness`, `agentflow-worker`, `agentflow-agents` (ReAct / PlanExecute hold a `Box<dyn MemoryStore>`).

## Findings

### CRITICAL

- [C1] **No SQLite pragmas (WAL / busy_timeout / synchronous) — multiple-process and contention scenarios will deadlock or corrupt** — `src/sqlite.rs:27-35`, `src/preference.rs:54-62`, `src/entity_facts.rs:54-62`, `src/semantic.rs:60-68`
  **What**: All four SQLite backends open the DB through `SqliteConnectOptions::from_str(url).create_if_missing(true)` and **never** call `.journal_mode(Wal)`, `.busy_timeout(...)`, `.synchronous(...)`, or `.foreign_keys(true)`. The pool has `max_connections(5)` (file mode) which means up to 5 concurrent writers compete on the default rollback-journal locking. With the default `SQLITE_BUSY` behaviour (immediate failure, 0 ms timeout), any concurrent write attempt under load fails outright; with rollback-journal mode every read lock excludes every write lock.
  **Why it matters**: The CLI surface (`agentflow memory prune ...` in `agentflow-cli/src/commands/memory/prune.rs`) opens the same DB the running agent uses; the agent runtime (ReAct / PlanExecute via `Box<dyn MemoryStore>`) may issue concurrent `add_message` calls during parallel tool-call batching. Without WAL + `busy_timeout`, observed behaviour is "works in tests, errors under real load". This is the single largest production-readiness gap in the crate.
  **Fix**: For every `SqliteConnectOptions::from_str(...)` site, chain:
  ```rust
  .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
  .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
  .busy_timeout(std::time::Duration::from_secs(5))
  .foreign_keys(true)
  ```
  Add a regression test that spawns 10 concurrent writers against a file-backed `SqliteMemory` (use `tempfile::NamedTempFile`) and asserts no `SQLITE_BUSY` returns.

- [C2] **`SqliteMemory::open` accepts the path verbatim as URL — backslashes / spaces / `?` / `#` in paths silently break** — `src/sqlite.rs:19-30`, mirrored in `src/preference.rs:46-57`, `src/entity_facts.rs:46-57`, `src/semantic.rs:48-62`
  **What**: `format!("sqlite://{}", path.to_str()...)` builds a URL by string-concatenation. A user-provided path containing `?`, `#`, `%`, spaces, or (on Windows) backslashes ends up as a malformed URL or — worse — silently parses with the trailing query interpreted as URI parameters (sqlx interprets `?mode=memory` as the in-memory connector, for example). `agentflow doctor` and the CLI accept arbitrary `--db-path`.
  **Why it matters**: Path traversal isn't the concern (`std::path` already rejected NUL); the concern is silent misbehaviour where a path like `/tmp/foo bar/mem.db` constructs an invalid URL and the user sees a cryptic `sqlx` error, or a path like `/tmp/x?mode=memory` becomes a transient in-memory DB and **silently discards writes on restart**.
  **Fix**: Switch to `SqliteConnectOptions::new().filename(path).create_if_missing(true)` — `filename` accepts any `AsRef<Path>` and does not require URL encoding. Eliminates 4 copies of fragile `format!("sqlite://...")` code.

### MAJOR

- [M1] **`row_to_message` silently fabricates a fresh UUID / timestamp on parse error instead of erroring** — `src/sqlite.rs:106-110`, `src/semantic.rs:566-570`
  **What**: `uuid::Uuid::parse_str(&id_str).unwrap_or_else(|_| uuid::Uuid::new_v4())` and `chrono::DateTime::parse_from_rfc3339(&ts_str)...unwrap_or_else(|_| chrono::Utc::now())`. A corrupted row therefore "succeeds" with a brand-new id and a clock-now timestamp.
  **Why it matters**: The primary key invariant breaks. Two reads of the same corrupted row return two different `Message::id` values, breaking checkpoint resume in `AgentNodeResumeContract` (which keys on message id). The timestamp fallback also makes `ORDER BY timestamp DESC` non-deterministic for corrupted rows — they appear as the newest.
  **Fix**: Return `MemoryError::StorageError(format!("invalid stored uuid: {id_str}"))` and equivalently for the timestamp. The "tolerate corrupt rows" policy, if intended, should be a separate `read_lenient` mode, not the default.

- [M2] **`session_token_count` swallows the `try_get("total")` error** — `src/sqlite.rs:215`, `src/semantic.rs:458`
  **What**: `let total: i64 = row.try_get("total").unwrap_or(0);` After `SELECT COALESCE(SUM(token_count), 0)` it will never legitimately fail, but the silent zero return masks a schema-evolution mistake. Compare with the explicit `map_err` on every other `try_get` call in the same files.
  **Why it matters**: Budget enforcement in `ReActAgent::apply_memory_prompt_budget` (downstream in agentflow-agents) drives prompt truncation off this number. A silent `0` causes the agent to think the session is empty and load the entire history into prompt, blowing past the model context window.
  **Fix**: Same pattern as the other reads — `.map_err(|e| MemoryError::StorageError(e.to_string()))?` and `as u32` separately.

- [M3] **`prune` in `SemanticMemory` is O(window-size × token-budget-overshoot) and issues 3 SQL round-trips per eviction** — `src/semantic.rs:257-303`
  **What**: For each evicted message the code runs: (1) `SUM(token_count)`, (2) `SELECT id ... ORDER BY timestamp ASC LIMIT 1`, (3) `DELETE FROM embeddings`, (4) `DELETE FROM messages`. With a 128k-token window and a sudden 64k-token burst, this is ~16k × 4 = 64k SQL calls inside `add_message` — each request waits on the connection pool.
  **Why it matters**: A single oversize message (e.g. a tool result with a long file dump) trips a multi-second tail latency on what should be a fast write. The session lock blocks the entire ReAct loop. Token-window eviction in `SessionMemory::prune` (`src/session.rs:36-54`) has the same shape but stays in-process so the SQL cost doesn't apply.
  **Fix**: Compute the eviction batch in a single SQL pass: `SELECT id, token_count FROM messages WHERE session_id = ? AND role != 'system' ORDER BY timestamp ASC` (load into memory once), accumulate until the budget is met, then `DELETE FROM messages WHERE id IN (?, ?, ?...)` and `DELETE FROM embeddings WHERE message_id IN (...)` in a single transaction. Wrap the whole prune in `tx = pool.begin().await?` so a failure mid-prune doesn't leave orphaned embeddings.

- [M4] **`SemanticMemory::search` loads the **entire** session's embedding set into memory and scores in a Rust loop** — `src/semantic.rs:381-431` (and the duplicated `search_semantic` path at `481-538`)
  **What**: For each search, all embeddings for the session are pulled into a `Vec<(String, f32)>`, scored serially with `cosine_similarity` (no SIMD, no batching), sorted, truncated. Then one additional `SELECT ... WHERE id = ?` per result row.
  **Why it matters**: For a long-running session with 10k embedded turns at 1536 dimensions, that's a 60 MB pull on every search call. The crate documents itself as a "production-ready" memory backend (per CLAUDE.md). Once the design doc's recommended migration to a vector DB (`docs/MEMORY_LAYERING.md`) lands, this falls away — but until then a `WHERE timestamp > NOW() - INTERVAL` filter and an embedded ANN index (e.g. `usearch`, `instant-distance`) would buy two orders of magnitude.
  **Fix (incremental)**: (1) At minimum, batch the per-row `SELECT messages WHERE id = ?` into a single `WHERE id IN (...)` query, eliminating `k` round-trips. (2) De-duplicate `MemoryStore::search` and `SemanticMemoryStore::search_semantic` — the two methods are 90 % identical, divergence will rot. (3) Add an explicit `RecentN` truncation so callers cap the candidate set.

- [M5] **`SessionMemory::prune` recomputes the full sum on every iteration** — `src/session.rs:36-54`
  **What**: The outer `while total > self.max_tokens` loop subtracts the evicted message's `token_count` via `saturating_sub`, but the initial `total` is a fresh `msgs.iter().map(|m| m.token_count).sum()`. Adding a new message doesn't reuse cached sum state, so the per-add cost is O(n) even when no eviction happens.
  **Why it matters**: The CLAUDE.md "Rust Performance Optimization Guidelines" call this exact pattern out — recomputing aggregates inside a hot loop. For long sessions (`large_window` = 128k tokens, ~30k+ short messages) the eviction-free common case still walks the entire vector on every `add_message`.
  **Fix**: Track `running_total: u32` on the struct (or per-session in a sibling HashMap), update on add/evict/clear, and skip the recomputation. Add a `cfg(test)` audit that asserts the running total matches a recomputed `iter().sum()`.

- [M6] **`MemoryStore::add_message` takes `&mut self` — forces a single-writer lock for every backend that's actually behind a connection pool** — `src/store.rs:9, 26`
  **What**: `add_message(&mut self, ...)` and `clear_session(&mut self, ...)` are `&mut self`. Both `SqliteMemory` and `SemanticMemory` hold a `SqlitePool` internally (already concurrent-safe), so the `&mut self` requirement is gratuitous — it forces every caller to wrap the store in `Mutex<Box<dyn MemoryStore>>` or `RwLock`.
  **Why it matters**: `agentflow-agents::ReActAgent` holds `memory: Box<dyn MemoryStore>` (`agentflow-agents/src/react/agent.rs:260`). The `&mut` requirement is the only thing standing between today's serial memory writes and concurrent writes from H3 parallel tool calls. The pool already supports it.
  **Fix**: Change trait signature to `add_message(&self, ...)` and `clear_session(&self, ...)`. `SessionMemory` will need `sessions: parking_lot::Mutex<HashMap<...>>` (or `RwLock`) internally — that's a one-file change and isolated to the in-process backend that legitimately needs the lock. This is a breaking change but the crate is 0.1.0; do it before downstream usage pins more places.

- [M7] **`LIKE` query with user-supplied `query` is unescaped — `%` and `_` in user input change semantics** — `src/sqlite.rs:176`, `src/semantic.rs:236`
  **What**: `let like = format!("%{}%", query);` then `.bind(&like)`. SQL injection isn't the worry (the binding is parameterized) — but `query = "100%_off"` will match any string starting with "100" because `%` and `_` are LIKE wildcards.
  **Why it matters**: Search results are non-deterministic w.r.t. their query. Caller-side escaping isn't documented. For a memory backend used inside agent loops, a user message containing the literal `%` triggers surprising recall behaviour.
  **Fix**: Escape `%`, `_`, and `\` in `query` before formatting, and add `ESCAPE '\'` to the `LIKE` clause: `WHERE content LIKE ?2 ESCAPE '\\'`. Add a test asserting `query = "50%"` finds the literal "50%" and not "5001".

- [M8] **`std::fs::read_to_string` / `std::fs::write` called from an async-looking API surface** — `src/preference_encrypted.rs:137, 146, 155, 165`
  **What**: `generate_identity_file` and `load_identity_file` are sync functions (`fn`, not `async fn`). They're called from `AgeEncryptedPreferenceStore::open_sqlite` which is `async`. The sync fs calls inside an async context block the Tokio runtime worker.
  **Why it matters**: The identity file is small (~100 bytes) so the blocking is bounded, but the pattern is wrong and the crate's API documentation doesn't warn callers that they should run these inside `tokio::task::spawn_blocking`. If the identity path is on a slow / network-backed filesystem (NFS, FUSE), the blocking is unbounded.
  **Fix**: Either (a) mark the helpers `async` and use `tokio::fs`, or (b) add `// SAFETY-NOTE: small file, sync IO acceptable` and call them from `tokio::task::spawn_blocking` inside `open_sqlite`.

- [M9] **`AgeEncryptedPreferenceStore` is exported but has no in-tree consumer or CLI wiring** — `src/lib.rs:38-40`, no matches in `agentflow-cli/src/commands/memory/` for `AgeEncrypted*`
  **What**: The encryption wrapper is fully implemented + tested but nothing else in the workspace constructs one. `agentflow-cli/src/commands/memory/prune.rs` and the rest of the harness wire `SqlitePreferenceStore` directly.
  **Why it matters**: Productization gap — the security claim "encryption-at-rest for the preference layer" (Cargo.toml:24) is only true if a caller opts in, and no caller does. Users won't get encrypted storage unless they read the source. P10.7.2's threat model promises this; the wiring doesn't deliver.
  **Fix**: Add a `--encrypted` / `--identity-file` flag pair to `agentflow memory init` (and / or wire it through `agentflow doctor --json` reporting) so the encryption posture is observable. Without a caller this code is dead weight.

### MINOR

- [m1] **`MemoryError::StorageError(String)` loses upstream error types** — `src/error.rs:6`, `src/error.rs:20-24`
  Every `sqlx::Error` collapses into a `String`. Callers can't `matches!` on `sqlx::Error::PoolClosed` vs `sqlx::Error::Database(db_err)` to distinguish retryable from permanent failures. Consider preserving the source: `StorageError { msg: String, #[source] source: Box<dyn std::error::Error + Send + Sync> }` or split into `Sqlx(#[from] sqlx::Error)` + `Storage(String)`.

- [m2] **`Role::from(&str)` silently coerces unknown strings to `Role::User`** — `src/types.rs:33-43`
  Reading a corrupted row with `role = "supervisor"` returns `Role::User` rather than erroring. Combined with [M1] this means a wholesale row corruption restores as a sequence of fabricated `User` messages with fresh ids. Switch to `TryFrom<&str>` and surface as `MemoryError::StorageError` from `row_to_message`.

- [m3] **`session_token_count` returns `u32` but SQL `SUM` is `i64` — silent truncation past 4 GiB tokens** — `src/sqlite.rs:216`, `src/semantic.rs:459`
  Token counts are tiny in practice, but the `as u32` cast wraps silently. Use `u32::try_from(total).unwrap_or(u32::MAX)` or widen the return type to `u64`. (Same with `token_count as u32` at `src/sqlite.rs:119`, `src/semantic.rs:579`.)

- [m4] **`HeuristicCounter::count_tokens` uses `text.len() / 4` — byte length, not char length** — `src/types.rs:73`
  CJK text under UTF-8 is 3 bytes per char, so the heuristic over-counts by ~50% (one char ≈ one token, not 0.25 token). The doc comment acknowledges this (`types.rs:62-67`); a `char_indices().count()` would be marginally more honest, but the real fix is what the codebase already does — route through `agentflow-llm::counter_for_model`. Make the doc comment of `HeuristicCounter` the canonical citation for when *not* to use it.

- [m5] **No retention-policy `RetentionPolicy::default_for(MemoryLayer::Session)` enforcement path** — `src/layer.rs:74-94`
  The type carries a `keep_invalidated_for` value but only `SqliteEntityFactStore::prune_invalidated` actually consults it. `SessionMemory` and `SqliteMemory` have no `prune_older_than` equivalent; only `SqlitePreferenceStore::prune_older_than` is wired. Either deprecate the session-layer retention default or add a session prune path.

- [m6] **Connection pool sized at `max_connections(5)` for file-backed stores, `1` for in-memory** — `src/sqlite.rs:32, 45`, mirrored 3 more times
  Magic numbers. Make them const + documented (`MEMORY_DB_POOL_SIZE: u32 = 5`, with a note on the WAL contention trade-off from [C1]). The `in_memory` use of `max_connections(1)` is correct (the `:memory:` URL otherwise creates separate DBs per connection) — call that out in a doc comment so a future refactor doesn't innocently widen it.

- [m7] **Hash-table key churn: `session_id.clone()` allocated twice in `SessionMemory::add_message`** — `src/session.rs:60-67`
  Cheap fix: `let session_id = message.session_id.clone(); self.sessions.entry(session_id.clone()).or_default()...; self.prune(&session_id);` allocates two `String`s per message. Could borrow if `prune` took `&str` (it already does) and the `entry` API path used `entry(message.session_id.clone())`.

- [m8] **`SemanticMemory::search` discards messages whose `id` column doesn't parse** — `src/semantic.rs:393-403`
  `filter_map(|row| { let msg_id: String = row.try_get("message_id").ok()?; ... })` silently drops rows on `try_get` failure. Same critique as [M1] / [m2]: corrupt rows should surface as errors, not vanish from search results.

- [m9] **Test `decrypt_with_wrong_identity_fails` reconstructs the wrapper by destructuring private fields** — `src/preference_encrypted.rs:466`
  `let AgeEncryptedPreferenceStore { inner, .. } = writer;` works only because the test lives in `mod tests` inside the same file. Public API offers no way to "swap identity on an existing store", which is correct. The test is fine as-is, but worth a comment to head off a future refactor that adds a public re-key API casually.

- [m10] **`AgeEncryptedPreferenceStore` re-encrypts the whole value on every write but the `version` and `updated_at` stay in plaintext** — `src/preference_encrypted.rs:244-255`
  Documented in the module header (`preference_encrypted.rs:38-47`). Acceptable for the local profile, but the "Threat model" comment should explicitly call out the metadata-leak surface: an attacker with the SQLite file but not the identity can still see *when* each preference was last written and *how often* it has changed (via `version`). Add a sentence to the module docs.

### POSITIVE OBSERVATIONS

- **Zero `unwrap()` / `expect()` in production code paths.** All 13 `unwrap_or*` patterns in `src/` are intentional fallbacks (one of which is fragile — see [M1]). Test code (137 instances) is fine. The crate adheres to CLAUDE.md's no-panic rule for the L2 boundary.
- **Zero TODO/FIXME/XXX/HACK markers.** Notably clean.
- **All SQL is parameterized.** No `format!` into SQL string anywhere — every dynamic value uses `bind(...)`. No SQL-injection surface.
- **Schema migrations use `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS`.** Idempotent boot for every backend.
- **`age` encryption wrapper has the right contract tests.** `ciphertext_is_not_recognizable_as_the_plaintext` (line 350), `decrypt_with_wrong_identity_fails` (line 442), `get_rejects_plaintext_row_missing_marker` (line 480), `generated_identity_file_has_mode_0600` (line 555). These are the right four assertions to pin for an at-rest encryption store.
- **Cross-layer integration test exists and is well-scoped** — `tests/cross_layer_precedence.rs` exercises all four layers through their trait objects and asserts the "no aliasing" contract. This is the strongest test in the crate.
- **`MemoryStore` trait has a default `to_prompt` method** — `src/store.rs:32-41`. Smart: every backend gets prompt rendering for free, and overrides remain possible.
- **`MemoryError: From<sqlx::Error>`** — `src/error.rs:20-24` saves the `?`-operator ergonomics across every backend.
- **In-memory test isolation via `SqliteMemory::in_memory` / `SqlitePreferenceStore::in_memory` / `SqliteEntityFactStore::in_memory` / `SemanticMemory::in_memory`** — every test uses isolated per-test connections (`max_connections(1)`, `:memory:` URL); no shared SQLite file races between parallel test binaries.
- **Layer separation is principled.** The decision to split `PreferenceStore` / `EntityFactStore` / `SemanticMemoryStore` out of `MemoryStore` (instead of bolting methods on) is documented in `layer.rs:7-14` and is the right architectural call.
- **`SemanticMemory` degrades to keyword search on embedding failure** — `src/semantic.rs:319-341, 377-431`. The fallback is tested (`search_falls_back_to_keyword_when_embedding_fails`, line 808) and the `tracing::warn` on the degrade path means operators can detect it.

## Metrics

- Source files: 11
- Lines of code: 3531 src + 214 integration test = 3745 total
- Backends: 5 (SessionMemory in-process, SqliteMemory, SqlitePreferenceStore, SqliteEntityFactStore, SemanticMemory) + 1 wrapper (AgeEncryptedPreferenceStore)
- Test files: ~50 unit tests across 7 modules + 1 integration test (`tests/cross_layer_precedence.rs`)
- `unwrap()/expect()` in non-test code: **0** (per-module `#[cfg(test)]` blocks contain 141 — all test-only). 13 `unwrap_or*` patterns in src; problematic ones flagged in [M1], [M2], [m2], [m8]
- TODO/FIXME/XXX/HACK: **0**
- Public items missing rustdoc: estimated **~15** — concentrated in `types.rs` (`Role` variants and `as_str`, `Message::system/user/assistant/tool_result`, `to_prompt_line`); `Role::From<&str>`, `Message::new`'s mutable-field doc; constructors `SessionMemory::new`. The `layer.rs` / `preference.rs` / `preference_encrypted.rs` / `semantic.rs` / `entity_facts.rs` surfaces are well-documented.
- SQL injection: 0 surface (all parameterized)
- SQLite pragmas set: **0** (no WAL, no busy_timeout, no synchronous, no foreign_keys) — see [C1]

## Recommendations (prioritized)

1. **[C1] Enable SQLite WAL + busy_timeout + foreign_keys on every backend** (4-line change × 4 files). Add a multi-writer regression test. Highest-leverage production fix in the crate.
2. **[C2] Replace `format!("sqlite://{}", ...)` with `SqliteConnectOptions::new().filename(...)`** across 4 backends. Eliminates a class of silent path-handling bugs.
3. **[M1, m2, m8] Stop fabricating data on row-parse failure.** Convert `unwrap_or_else(|_| ...)` in `row_to_message` and `Role::from` to explicit errors. Either tolerate corruption *visibly* (`tracing::error` + skip the row) or fail loudly — never silently invent ids/timestamps.
4. **[M6] Change `MemoryStore::add_message` / `clear_session` to `&self`** before downstream pins more callers. Move the in-process lock into `SessionMemory` where it belongs. Unblocks H3 parallel-tool-call concurrent writes without `Mutex<Box<dyn MemoryStore>>` wrappers.
5. **[M3, M5] Make eviction a single-pass operation.** SQL version: load → accumulate → batch DELETE in a transaction. In-process: track a running `total` instead of recomputing.
6. **[M4] De-duplicate `MemoryStore::search` and `SemanticMemoryStore::search_semantic`**, batch the per-result `SELECT messages WHERE id = ?` into one `IN (...)` query, and add an explicit candidate-set cap. Plan the migration to an embedded ANN index (`usearch` or `instant-distance`) before the workspace promotes `SemanticMemoryStore` past experimental.
7. **[M9] Wire `AgeEncryptedPreferenceStore` into the CLI surface** (`agentflow memory init --encrypted --identity ~/.agentflow/identity.age`). Without a caller this is dead weight and the encryption-at-rest claim is aspirational.
8. **[M7] Escape `%` / `_` in `LIKE` queries.** Tiny fix, deterministic search results.
9. **[M2, m3] Stop silent integer narrowing and silent `try_get("total")` failure.** Either widen to `u64` or `try_from`.
10. **[m1] Preserve `sqlx::Error` source in `MemoryError`.** Callers can then distinguish transient pool exhaustion from permanent schema corruption.
11. **[m4, m5, m6, m10] Documentation tidies.** Heuristic-counter accuracy caveat, retention-policy enforcement gaps, magic numbers, encrypted-store metadata leakage — all small, all worth fixing before the four newer trait surfaces graduate from Experimental to Beta.

End of report.
