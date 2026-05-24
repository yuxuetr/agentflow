# Audit: agentflow-rag

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-rag/
**Crate version**: 0.3.0-alpha
**Layer**: L2 (Capability Adapter)
**Stability tier**: alpha (per `version = "0.3.0-alpha"`)

## Scope summary

`agentflow-rag` is a self-contained RAG capability adapter: document loading
(text/md/csv/json + optional pdf/html), chunking (fixed/sentence/recursive/
semantic), embeddings (OpenAI HTTP + optional ONNX), Qdrant-backed vector
store, retrieval (vector + BM25 + RRF hybrid), reranking (NoOp / MMR /
Score), and an eval harness (Recall@K / MRR / nDCG@K + paired sign test).
~10k lines across ~30 source files. Notably, the crate has **zero
intra-workspace deps** — it does not pull `agentflow-core`, `agentflow-llm`,
or anything else from the workspace. That's a clean L2 boundary.

The eval module (`src/eval/`) is the strongest, best-documented part of the
crate. The chunker, embedding, and Qdrant layers are functional but show
prototype-grade rough edges (UTF-8 boundary risk, missing TLS/auth knobs,
batching bugs).

## Findings

### CRITICAL

- [C1] OpenAI embedding batch loop conflates token budget with batch count — `src/embeddings/openai.rs:235`
  **What**: `MAX_BATCH_SIZE` is defined as `2048` and described as "OpenAI
  limit" (the per-request *array length* limit). The flush condition is
  `current_tokens + tokens > MAX_BATCH_SIZE || current_batch.len() >= 2048`.
  The left side compares **token totals** to **2048**, which means the
  flush triggers when the combined estimated token count of a single
  request reaches 2048 — orders of magnitude below the real OpenAI
  per-request token cap (~300k for `text-embedding-3-small`). For typical
  English text (~4 chars/token), a flush happens every ~8 KB of input
  text instead of every ~1 MB.
  **Why it matters**: Under-batching by ~150x. Every `embed_batch` call
  hits the API far more often than necessary, multiplying latency, cost,
  and rate-limit pressure. The right-side `current_batch.len() >= 2048`
  check is dead code under realistic workloads because the left side
  always fires first.
  **Fix**: Separate the two limits: keep a `MAX_BATCH_LEN: usize = 2048`
  for array length and add a `MAX_BATCH_TOKENS: usize` matching the
  model's real per-request budget (e.g. 300k for the small model). Flush
  when *either* threshold is reached.

### MAJOR

- [M1] `FixedSizeChunker` panics on `overlap >= chunk_size` — `src/chunking/fixed_size.rs:47`
  **What**: `start_idx += self.chunk_size - self.overlap` underflows when
  `overlap >= chunk_size`. The constructor (`FixedSizeChunker::new`) does
  no validation, so a caller passing `(64, 64)` or `(64, 100)` triggers a
  debug-mode panic and wrong behaviour in release. The eval helper
  (`chunk_dataset` in `src/eval/chunking_eval.rs:87`) does validate this
  invariant before constructing the chunker, but the chunker itself
  doesn't — so any direct user of `FixedSizeChunker::new` is exposed.
  **Why it matters**: Trivial-to-trigger crash on bad config. Same hazard
  exists in `RecursiveChunker` (line 161 slicing `&current[overlap_start..]`
  with no UTF-8 boundary check) and `SemanticChunker` (multi-byte chars
  via `text.chars()...take(self.overlap)`).
  **Fix**: Return `Err(RAGError::configuration(...))` from `new()` when
  `overlap >= chunk_size`, or saturate `start_idx += chunk_size.saturating_sub(overlap).max(1)`.
  Document UTF-8 char-vs-byte boundary expectations explicitly.

- [M2] `RecursiveChunker` UTF-8 boundary panic on multi-byte overlap — `src/chunking/recursive.rs:160-161`
  **What**: `let overlap_start = current.len() - self.overlap;` uses byte
  length but `self.overlap` is documented as "Number of characters to
  overlap" — for any non-ASCII content the slice `&current[overlap_start..]`
  hits a non-char-boundary panic (Rust will panic on slicing UTF-8 at a
  non-boundary). Same drift in `SemanticChunker::apply_overlap`
  (`src/chunking/semantic.rs:340-348`) which mixes `.chars()` counts with
  later byte arithmetic.
  **Why it matters**: A Chinese/Japanese/emoji-bearing corpus that uses
  any chunker except `FixedSizeChunker` (which collects to `Vec<char>` first)
  can panic at runtime. The unit tests are ASCII-only and miss this.
  **Fix**: Convert overlap measurement to chars consistently — either
  index via `char_indices()` to find a real byte boundary, or change the
  API contract to "bytes" and rename the field. Add a Unicode test case
  (Chinese, emoji) to all four chunker test modules.

- [M3] `QdrantStoreBuilder` has no API-key / TLS / timeout knobs — `src/vectorstore/qdrant.rs:498-538`
  **What**: The builder accepts only a URL. There is no way to pass a
  Qdrant Cloud API key, configure TLS client certs, set a request
  timeout, or enable connection pooling. `Qdrant::from_url(&self.url).build()`
  uses whatever defaults the `qdrant-client` crate ships with.
  Production Qdrant deployments (Qdrant Cloud, self-hosted with auth) all
  require API key auth; this client cannot talk to them.
  **Why it matters**: Crate is unusable against any non-trivial Qdrant
  deployment. Also there is no log redaction story for credentials —
  `tracing::info!("Connecting to Qdrant at: {}", self.url)` will leak any
  basic-auth URL credentials into logs verbatim.
  **Fix**: Add `.api_key(...)`, `.timeout(...)`, `.tls_config(...)`
  builder methods. Redact userinfo from the logged URL. Surface
  `qdrant_client::QdrantBuilder` config knobs the upstream crate already
  exposes.

- [M4] `IndexingPipeline::index_documents` is purely sequential — `src/indexing/mod.rs:83-95`
  **What**: The batch indexer iterates docs with `for doc in docs { self.index_document(...).await? }`.
  Each document blocks the next on chunking + embedding API round-trip +
  Qdrant upsert. For a 1000-doc corpus with 200ms embedding latency, this
  is 200 seconds of serialised network I/O when concurrent dispatch would
  bring it under 10 seconds.
  **Why it matters**: Production indexing is order-of-magnitude slower
  than it needs to be. The embedding provider already supports batch
  embedding within a single doc's chunks — but the inter-doc concurrency
  is missing entirely.
  **Fix**: Use `futures::stream::iter(docs).buffer_unordered(N)` with a
  caller-configurable concurrency cap. Aggregate stats via reduction.
  Consider per-collection bulk upserts as a second optimisation.

- [M5] `BM25Retriever::add_document` triggers full IDF recompute — `src/retrieval/bm25.rs:161, 175`
  **What**: Every `add_document` call invokes `recompute_statistics()`,
  which iterates every document, every term, in O(N×T). Inserting N docs
  one-by-one is therefore O(N²×T). For the 10k-doc corpus in the
  benchmark, that's tens of millions of redundant hash lookups per
  index build. The benchmark `bench_bm25_index` `build_corpus(10_000)`
  is the canonical proof point.
  **Why it matters**: O(N²) indexing path silently scales the bench's
  10k-doc build into the multi-second range, and any production code
  using the indexer is hit by the same cost. The bench file even
  acknowledges the path: `bench_bm25_index` exists precisely because
  this is a bottleneck.
  **Fix**: Add a `bulk_add_documents(Vec<(id, content)>)` path that
  defers `recompute_statistics()` until the end. Optionally maintain
  incremental IDF state (track per-term doc-frequency counters) so
  individual `add_document` calls also remain near-O(1) amortised.

- [M6] PDF / HTML loaders read entire file into memory with no size cap — `src/sources/pdf.rs:56`, `src/sources/html.rs:159`
  **What**: `fs::read(path).await?` (PDF) and `fs::read_to_string(path).await?`
  (HTML) buffer the whole file. A malicious 4 GB PDF or a HTML "billion-laughs"
  expansion bomb takes the process OOM. `pdf-extract` is also known to
  have CVEs around malformed PDFs (no upper bound on processing time or
  memory either).
  **Why it matters**: Document parsing is the most adversarial surface
  in any RAG stack — public-facing servers that accept user-uploaded
  PDFs/HTML must defend against DoS. This crate offers no defence.
  **Fix**: Add a `max_bytes` config knob to both loaders, defaulting to
  e.g. 32 MiB. Wrap `extract_text_from_mem` in a `tokio::time::timeout`
  (PDFs commonly hang in pdf-extract on adversarial inputs). Document
  the threat model in the loader docs.

### MINOR

- [m1] StepFun embedding provider mentioned in CLAUDE.md but not implemented
  **What**: Project CLAUDE.md says agentflow-rag supports "embeddings
  (OpenAI/StepFun API or local ONNX)" but `src/embeddings/` ships only
  `openai.rs` and (gated) `onnx.rs`. Grepping for `stepfun`/`step-fun`
  returns nothing.
  **Why it matters**: Docs/reality drift. Either implement StepFun (the
  StepFun embeddings API is OpenAI-compatible so it would be a thin
  builder variant) or correct CLAUDE.md.
  **Fix**: Add a `.with_base_url(...)` and `.with_endpoint(...)` knob to
  `OpenAIEmbeddingBuilder` so it can target any OpenAI-compatible
  embeddings endpoint, then document StepFun as a config example.

- [m2] `RAGError` is missing a `From<qdrant_client::QdrantError>` impl — `src/error.rs`
  **What**: All Qdrant errors are stringified at call sites via
  `.map_err(|e| RAGError::vector_store(format!("...: {}", e)))`. There's
  no `#[from]` variant so the structured error context (status code,
  retryability) is lost. Same for `tokenizers::Error` and `ort::Error`.
  **Fix**: Add `#[from]` variants for these. Update `is_transient()` to
  inspect them.

- [m3] `embed_batch` returns one error for the whole batch on any failure — `src/embeddings/openai.rs:238, 250`
  **What**: If one sub-batch fails (e.g. transient 503), the entire
  `embed_batch` returns `Err` and all previously-successful embeddings
  are discarded. There is no partial-success path.
  **Fix**: Either return `Vec<Result<Vec<f32>>>` so callers can decide
  per-chunk, or add a configurable retry-with-skip mode.

- [m4] `parse_toml_manifest` is a hand-rolled toml parser — `src/eval/dataset.rs:234-256`
  **What**: The comment justifies avoiding a `toml` dep, but this
  parser silently accepts malformed input (e.g. `name="x\""` is parsed
  as `name="x\\""`). It also silently ignores `[sections]`. Manifest
  bugs become silent data quality bugs.
  **Why it matters**: Dataset manifests carry licence and source
  provenance — silent parse failures here mean a regression report could
  attribute a result to the wrong dataset version.
  **Fix**: Add the `toml` crate as an optional dep gated behind a default
  feature, or at minimum return errors on quoted-string parse failures
  instead of `trim_matches('"')` (which mangles legitimate quotes).

- [m5] `sentence::SentenceChunker::find_overlap_sentences` ignores `_chunk_end_idx` — `src/chunking/sentence.rs:60`
  **What**: The function signature accepts `_chunk_end_idx` but ignores
  it (leading underscore). It walks the full sentence list from the end
  every call, not from the chunk-end position. This is O(N) per chunk
  emit, making whole-document chunking O(N×C).
  **Fix**: Either use the passed-in end index to start the reverse walk,
  or remove the parameter.

- [m6] HTML loader script-removal walks the regex even when no scripts present — `src/sources/html.rs:96-101`
  **What**: The `for _ in scripts` loop runs the regex `replace_all` once
  *per script tag found*, but `replace_all` already removes all
  occurrences in one pass. So the regex runs O(N_scripts) times instead
  of once. Same pattern for styles.
  **Fix**: Drop the `for _ in scripts` loop entirely — call
  `script_regex().replace_all(&html, "")` exactly once.

- [m7] `cosine_similarity` in `chunking/semantic.rs` returns 0.0 on dim mismatch — `src/chunking/semantic.rs:461-463`
  **What**: Silent zero is a worst-case footgun for an embedding
  function: a dim-mismatch bug in the calling code produces identical
  zero-similarity for every comparison, which then looks like "no topic
  boundaries detected" or "every sentence is a boundary" depending on
  threshold direction.
  **Fix**: Return a `Result` or panic with a clear assertion in debug.
  At minimum log a `tracing::warn!`.

- [m8] `ONNXEmbedding::embed_batch` is documented as sequential with a TODO — `src/embeddings/onnx.rs:236`
  **What**: `// TODO: Implement true batch processing for better performance`.
  Single-text-at-a-time inference loses 10-20x throughput vs. batched
  ONNX inference. Combined with `Mutex<Session>` for the session this
  also fully serialises across threads.
  **Fix**: Pad inputs to a common length, batch tensor along the batch
  axis, run a single `session.run`. Or at minimum drop the global Mutex
  and use a session pool.

- [m9] Tests use `OpenAIEmbedding::new` with `unwrap()` and bare reqwest — no `.no_proxy()`
  **What**: `tests/embeddings_integration.rs` and the inline ignored
  `tokio::test` blocks in `src/embeddings/openai.rs` build `reqwest`
  clients via `Client::builder()` with no `.no_proxy()`. All tests are
  `#[ignore]`'d so CI doesn't run them, but a dev who removes `--ignored`
  on a machine with a system proxy (per user's global Rust HTTP
  guideline) will see `IncompleteMessage` errors with no obvious cause.
  **Fix**: Add `.no_proxy()` to the `Client::builder()` in `OpenAIEmbeddingBuilder::build`
  (production code can keep the default; only test clients need it) OR
  document the dev requirement in `tests/embeddings_integration.rs`.

- [m10] `EmbeddingProvider::estimate_tokens` uses `text.len() / 4` — Unicode-blind — `src/embeddings/mod.rs:38-40`
  **What**: Byte length / 4 dramatically over-estimates for CJK
  (3 bytes/char) and under-estimates for emoji. Token cap check
  (`is_within_limit`) is therefore wrong on non-English text.
  **Fix**: Use char count, or wire in `tiktoken-rs` behind an optional
  feature. At minimum, comment the limitation.

- [m11] `Document::new` always allocates a fresh UUID — `src/types.rs:27`
  **What**: Calling `Document::new` 10k times runs the UUID v4 RNG 10k
  times. Not a hotspot but wasteful when the loader assigns IDs anyway.
  **Fix**: Defer ID generation to first access, or expose
  `Document::with_lazy_id`.

- [m12] No `agentflow-rag` integration with the OpenAI-compatible providers in `agentflow-llm`
  **What**: The workspace's `agentflow-llm` crate has 9 providers with a
  battle-tested HTTP client, retry, `traceparent` propagation, and
  redaction. `agentflow-rag::embeddings::openai` reimplements its own
  reqwest+retry+rate-limit stack. This is the only place in the workspace
  where an HTTP API client is duplicated.
  **Why it matters**: Bugs and feature gaps (W3C trace context, key
  redaction, OpenAI-compatible endpoints) have to be fixed twice.
  **Fix**: Define an `EmbeddingProvider` adapter inside `agentflow-llm`
  (it already speaks to OpenAI/StepFun/etc.) and depend on it from
  `agentflow-rag`. Keep the trait in `agentflow-rag` so the layering
  remains clean.

### POSITIVE OBSERVATIONS

- Eval harness (`src/eval/`) is high quality: clean retriever-agnostic
  trait, log-space binomial CDF for the paired sign test
  (`paired_sign_lower_tail_p_value`), explicit chunk-id remap so chunked
  vs un-chunked baselines stay comparable, schema-stable serde with
  `chunk_size` defaulting to `None` (P10.6.3 forward-compat is explicitly
  tested at `src/eval/runner.rs:479-515`). Module-level doc comments are
  excellent.
- Feature flags are correctly applied: `qdrant`, `local-embeddings`, `pdf`,
  `html` are all optional and gate the heavyweight deps (`qdrant-client`,
  `ort`, `pdf-extract`, `scraper`). Default is `qdrant` only.
- `error.rs` has both a structured enum and constructor helpers, plus
  `is_transient()` for retry decisions. Better than most crates in the
  workspace.
- The bundled `agentflow_mini` dataset + `tests/eval_harness.rs` are a
  proper end-to-end test of dataset loading, BM25 eval, and baseline
  comparison.
- `DenseEval` precomputes L2 norms once per corpus vector
  (`src/eval/retrievers.rs:93`) — a small but real perf win in the
  inner loop, with the comment explaining why.
- Filter conversion in `vectorstore/qdrant.rs` properly distinguishes
  Float vs Integer match (rejects Floats with a clear error per Qdrant
  semantics) and supports `must` / `should` / `must_not` composition.
- `Bm25Eval::from_dataset` uses the BEIR title+body concatenation
  convention — documented at `src/eval/retrievers.rs:38-44` — which keeps
  the eval-harness numbers comparable across published benchmarks.

## Metrics

- Source files: 30 (incl. `lib.rs`, mod files, parser/chunker variants)
- Lines of code: ~10,300 total (incl. inline tests; production code
  roughly 6,500)
- Parsers: 5 — Text/Markdown, CSV, JSON (shares CSV loader), PDF
  (feature-gated), HTML (feature-gated)
- Vectorstores: 1 — Qdrant (feature-gated, only backend); trait is
  generic to allow more
- Test files: 26 inline `#[cfg(test)]` modules + 2 integration test
  files (`tests/embeddings_integration.rs`, `tests/eval_harness.rs`)
- `unwrap()/expect()` in non-test code: 12 — all are either
  compile-time-constant regex `expect()` patterns (6 in `sources/html.rs`
  + `sources/preprocessing.rs`) or post-precondition asserts that are
  technically safe. Top 5 noteworthy:
  1. `src/embeddings/onnx.rs:333` — `path.file_stem().unwrap()` (panics
     on path without filename — adversarial but real)
  2. `src/eval/runner.rs:165` — `config.k_values.iter().max().unwrap()`
     (safe — `is_empty()` checked one line above)
  3. `src/chunking/sentence.rs:103, 135` — `current_chunk_sentences.last().unwrap()`
     (safe — explicit `is_empty()` guard above)
  4. `src/eval/compare.rs:129-130` — `per_k.iter().find().unwrap()`
     (safe — `intersection` precheck)
  5. `src/eval/metrics.rs:300` — `partial_cmp().unwrap()` (panics on
     NaN — replace with `unwrap_or(Ordering::Equal)` like the rest of
     the file)
- Tests NOT using `.no_proxy()`: 6 integration tests + 3 inline
  `tokio::test` tests, all in `embeddings_integration.rs` / `openai.rs`.
  All are `#[ignore]`'d so CI is fine, but dev override path is hazardous
  (per user global Rust HTTP guideline).
- TODO/FIXME: 1 (`src/embeddings/onnx.rs:236` — batch processing)
- Public items missing rustdoc: estimated ~20. Most public types/fns
  carry `///` comments; the gaps are mainly in `types.rs` enum variants
  (e.g. `MetadataValue::String`, `DistanceMetric::Cosine`) and a few
  trait method docs in `vectorstore/mod.rs`.

## Recommendations (prioritized)

1. **Fix the OpenAI batch-size bug** (C1, `src/embeddings/openai.rs:235`).
   Single highest-leverage change — ~150x reduction in API requests for
   typical workloads.
2. **Add Qdrant auth + TLS knobs** (M3). Without these the crate can't
   talk to any production Qdrant deployment.
3. **Harden chunking against UTF-8 and overlap edge cases** (M1, M2).
   Add Unicode test fixtures (CJK, emoji) to every chunker. Validate
   `overlap < chunk_size` in constructors.
4. **Add document-loader size caps + timeouts** (M6). Required before
   PDF/HTML loaders can be exposed via any public surface.
5. **Concurrent batch indexing** (M4) — easy `buffer_unordered` win for
   any non-trivial corpus.
6. **De-duplicate the HTTP/embedding client with `agentflow-llm`** (m12).
   Largest long-term maintainability win; would also pull in
   `traceparent` propagation and API-key redaction for free.
7. **Incremental BM25 IDF maintenance** (M5). Necessary before BM25 can
   index corpora larger than a few thousand docs.
8. **Document the StepFun gap** (m1) — either implement it (trivial via
   OpenAI-compatible endpoint) or update CLAUDE.md.

End of report.
