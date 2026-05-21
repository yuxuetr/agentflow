# RAG Evaluation Harness

> Status: shipped in v0.4.0 (P1 #10).
> Crate: `agentflow-rag::eval`. CLI: `agentflow rag eval`.

The eval harness turns retrieval quality into a number. Given a labeled dataset
(`corpus + queries + judgments`) and a retriever, it produces a structured
report covering Recall@K, MRR, nDCG@K, and latency, plus an optional paired
comparison against a baseline run.

The harness is deliberately **retriever-agnostic** — the runner needs nothing
more than `search(query, k) -> Vec<doc_id>`. BM25 is the default offline
backend (no external services); vector / hybrid retrievers can be plugged in
directly via the `Retriever` trait.

## When to use it

- Tuning a retrieval config (chunk size, BM25 k1/b, embedding model).
- Catching regressions when changing the indexing pipeline.
- Comparing two retrievers (e.g. BM25 vs vector vs hybrid) on the same gold set.
- Smoke-testing in CI: the bundled `agentflow_mini` dataset takes ~10 ms to run.

## Dataset format

A dataset lives in a directory with the following layout:

```
<dataset>/
├── dataset.toml      # optional manifest (name / version / source / license)
├── corpus.jsonl      # one CorpusDoc per line
├── queries.jsonl     # one Query per line
└── qrels.jsonl       # one Judgment per line (query → relevance map)
```

### corpus.jsonl

```json
{"id": "doc_dag", "title": "DAG execution engine", "text": "AgentFlow Flow orchestrator runs ..."}
```

- `id` — stable identifier referenced from judgments.
- `text` — document body.
- `title` — optional; concatenated as `title\ntext` when present.

### queries.jsonl

```json
{"id": "q_dag", "text": "How does the DAG engine schedule async nodes?"}
```

- `id` — referenced from `qrels.jsonl::query_id`.
- `notes` — optional free-form annotation rationale.

### qrels.jsonl

```json
{"query_id": "q_dag", "relevances": {"doc_dag": 2, "doc_react": 1}}
```

- `relevances` — `doc_id → score`. Score is a `u8`. Use `0` for **explicitly
  judged non-relevant** (informational only — only positive scores count for
  metric averages).
- Binary datasets use `0`/`1`; graded datasets typically use `0..=3`.

### dataset.toml (optional)

Flat key-value provenance metadata. Recognized keys: `name`, `version`,
`source`, `license`, `description`. Anything else is ignored.

```toml
name = "agentflow_mini"
version = "0.1.0"
source = "synthetic, hand-authored"
license = "MIT"
description = "Tiny offline RAG demo dataset built from AgentFlow facts."
```

Loading via `Dataset::load_from_dir(path)` validates that every judgment
references a known query id and a known doc id; missing references abort with
a clear error rather than silently scoring 0.

## Metrics

All averages are macro-averaged (per-query metric → mean over queries with at
least one relevant doc).

| Metric | Definition | When it matters |
| --- | --- | --- |
| **Recall@K** | fraction of relevant docs that appear in top-K | "Did we find them at all?" |
| **MRR** | `1 / rank_of_first_relevant` (0 if missing) | "How early does the first hit show up?" |
| **nDCG@K** | normalized DCG; `(2^rel - 1) / log2(i + 1)` | Graded relevance + rank position |
| **Latency** | per-query retrieval time (mean / p50 / p95 ms) | Performance regressions |

Notes:

- nDCG uses the standard exponential gain `2^rel - 1`. Binary relevance
  collapses to 0/1; graded relevance rewards higher scores super-linearly,
  matching IR conventions.
- IDCG is computed against the ideal ordering of the judgment set, capped at
  the same K as DCG.
- Recall@K is undefined for queries with zero relevant docs and is excluded
  from the macro-average (`queries_with_relevant` in the report shows the
  effective denominator).

## CLI usage

```bash
# Quick smoke test on the bundled demo dataset (BM25 — no API key)
agentflow rag eval \
  --dataset agentflow-rag/examples/datasets/agentflow_mini \
  --retriever bm25 \
  -k 1,3,5,10
```

### Retriever backends (P10.6.1)

The CLI supports three backends via `--retriever`:

- `bm25` (default): offline lexical retrieval. No external services or
  API keys; deterministic across runs.
- `dense`: in-memory cosine similarity over OpenAI embeddings. Requires
  `OPENAI_API_KEY` at run time. Pick the model with
  `--embedding-model <name>` (default `text-embedding-3-small`). The
  CLI embeds the corpus + queries once, then scores in RAM — no vector
  store needed for eval-scale corpora (<100k docs).
- `hybrid`: Reciprocal Rank Fusion (RRF) combining BM25 + dense. Also
  requires `OPENAI_API_KEY`. The default RRF smoothing constant
  `k = 60` matches Cormack-Clarke-Buettcher 2009; the inner-k
  multiplier defaults to `3× --k_values.max()` so mid-ranked docs
  from either backend still have a chance to win on the fusion score.

```bash
# Dense embedding-based retrieval (needs OPENAI_API_KEY)
agentflow rag eval \
  --dataset path/to/dataset \
  --retriever dense \
  --embedding-model text-embedding-3-small \
  -k 1,3,5,10

# Hybrid BM25 + dense via RRF
agentflow rag eval \
  --dataset path/to/dataset \
  --retriever hybrid \
  --embedding-model text-embedding-3-small \
  -k 1,3,5,10
```

Output:

```
Loaded dataset: agentflow-rag/examples/datasets/agentflow_mini
  manifest: name=agentflow_mini
  corpus=16 queries=12 judgments=12

Retriever: bm25
Label:     baseline
Queries:   12 (12 with relevant)

K          Recall       nDCG
------ ---------- ----------
1          0.7500     0.9167
3          0.9167     0.9403
5          0.9583     0.9502
10         1.0000     0.9583

MRR:       0.9583
Latency:   mean=0.07ms p50=0.08ms p95=0.09ms
```

### Per-chunk-size latency profile (P10.6.3)

By default the eval indexes the corpus one-doc-one-id. Pass
`--chunk-size <N>` to re-chunk every corpus doc with a fixed-size
chunker (overlap=0) before building the retriever index. The runner
remaps retrieved chunk ids back to source doc ids before scoring,
so `Recall@K` / `MRR` / `nDCG@K` stay comparable across chunk
sizes (qrels still reference source doc ids). The latency block,
however, reflects the chunked index — operators capture one baseline
per chunk strategy to spot chunking-side regressions:

```bash
# Capture three baselines, one per chunk size:
agentflow rag eval --dataset path/to/dataset --chunk-size 256 \
  --output baselines/chunk-256.json
agentflow rag eval --dataset path/to/dataset --chunk-size 512 \
  --output baselines/chunk-512.json
agentflow rag eval --dataset path/to/dataset --chunk-size 1024 \
  --output baselines/chunk-1024.json
```

The CLI's text output includes a `Chunk size:` line directly under
the latency block when `--chunk-size` is set:

```
Latency:   mean=0.18ms p50=0.20ms p95=0.32ms
Chunk size: 256 (fixed-size, overlap=0)
```

The JSON `--output` file persists `baseline.chunk_size` (omitted
when un-chunked, matching the pre-P10.6.3 schema for back-compat).
When `--compare-baseline` is supplied and the stored baseline's
`chunk_size` differs from the current run's, the CLI prints a
stderr warning so cross-chunk comparisons aren't silently
misinterpreted.

### Baseline comparison

```bash
agentflow rag eval \
  --dataset path/to/dataset \
  --retriever bm25 \
  --compare-to "k1=1.8,b=0.6" \
  --output report.json
```

The `--compare-to` flag re-runs the same retriever with custom BM25 parameters
and prints a paired comparison table:

```
Metric           Baseline  Candidate      Δ abs      Δ rel
-------------- ---------- ---------- ---------- ----------
Recall@5           0.9583     0.9750    +0.0167     +1.74%
nDCG@5             0.9502     0.9650    +0.0148     +1.56%
MRR                0.9583     0.9750    +0.0167     +1.74%
Latency (mean ms)  0.0718     0.0717    -0.0001     -0.14%

Paired sign test (per-query reciprocal rank):
  wins=2  losses=0  ties=10
Verdict:   inconclusive — win-rate 2/2 below 60% threshold
```

#### Verdict thresholds

- `candidate_wins`: candidate strictly beats baseline on ≥60% of decisive
  (non-tied) queries.
- `baseline_wins`: symmetric.
- `inconclusive`: below threshold or all queries tied.
- `not_comparable`: per-query rows could not be paired (different query ids,
  different lengths). Almost always means the two reports came from different
  datasets.

The sign test is a coarse signal — if you need real statistical claims, run a
larger dataset and compute paired t-tests externally on the per-query rows in
the JSON report.

#### Checked-in regression baselines (P10.6.2)

The repo ships three regression-gate baselines for the bundled
`ci_offline` dataset under `agentflow-rag/eval_baselines/ci_offline/`:

| Baseline file | Retriever | API key needed at run time? | CI gating |
| --- | --- | --- | --- |
| `bm25.json` | `bm25` (lexical, offline) | No | Always (every PR) |
| `dense.json` | `dense` (OpenAI `text-embedding-3-small`, in-memory cosine) | Yes — `OPENAI_API_KEY` | When `OPENAI_API_KEY` secret is set on the runner |
| `hybrid.json` | `hybrid` (RRF over BM25 + dense) | Yes — `OPENAI_API_KEY` | When `OPENAI_API_KEY` secret is set on the runner |

The CI workflow (`.github/workflows/quality.yml::rag-eval-smoke`)
runs `--compare-baseline` against all three; forks without the
`OPENAI_API_KEY` secret stay green because the dense + hybrid steps
self-skip via `if: ${{ secrets.OPENAI_API_KEY != '' }}`.

Both the bare `EvalReport` shape (the `bm25.json` convention) and
the `{ dataset, baseline, candidate, ... }` envelope shape (what
`--output <path>` writes) are accepted by `--compare-baseline`, so
operators can feed their own `--output` files back without any
manual extraction.

Regenerating after upstream changes (new corpus docs, new queries,
embedding-model upgrade):

```bash
agentflow rag eval \
  --dataset agentflow-rag/eval_datasets/ci_offline \
  --retriever dense \
  --embedding-model text-embedding-3-small \
  --output agentflow-rag/eval_baselines/ci_offline/dense.json

agentflow rag eval \
  --dataset agentflow-rag/eval_datasets/ci_offline \
  --retriever hybrid \
  --embedding-model text-embedding-3-small \
  --output agentflow-rag/eval_baselines/ci_offline/hybrid.json
```

## JSON report shape

When `--output report.json` is set, the harness writes a single JSON document
suitable for downstream tooling:

```json
{
  "dataset": {
    "path": "...",
    "manifest": {"name": "agentflow_mini", "version": "0.1.0", ...},
    "corpus_size": 16,
    "queries": 12,
    "judgments": 12
  },
  "baseline": {
    "retriever": "bm25",
    "label": "baseline",
    "per_k": [{"k": 5, "recall": 0.96, "ndcg": 0.95}, ...],
    "mrr": 0.96,
    "latency": {"mean_ms": 0.07, "p50_ms": 0.08, "p95_ms": 0.09},
    "num_queries": 12,
    "queries_with_relevant": 12,
    "per_query": [{"query_id": "q_dag", "reciprocal_rank": 1.0, ...}, ...]
  },
  "candidate": { ... },
  "comparison": {
    "deltas": [{"metric": "MRR", "abs_delta": 0.02, ...}, ...],
    "verdict": "candidate_wins",
    "verdict_reason": "candidate wins on 8/10 decisive queries (≥60% threshold)"
  }
}
```

`per_query` is the unit of paired analysis — each row carries the query id,
text, top-K Recall / nDCG, reciprocal rank, and retrieval latency.

## Plugging in custom retrievers

The CLI ships only with BM25; for vector / hybrid / external retrievers,
implement the `Retriever` trait directly:

```rust
use agentflow_rag::eval::{Dataset, EvalConfig, Retriever, evaluate};

struct MyVectorRetriever { /* ... */ }

impl Retriever for MyVectorRetriever {
  fn name(&self) -> &str { "vector:openai" }
  fn search(&self, query: &str, k: usize) -> agentflow_rag::Result<Vec<String>> {
    // ... call your vector store, return ranked doc ids
    Ok(vec![])
  }
}

let dataset = Dataset::load_from_dir("path/to/dataset")?;
let retriever = MyVectorRetriever { /* ... */ };
let config = EvalConfig {
  k_values: vec![1, 3, 5, 10],
  label: "openai-large".into(),
};
let report = evaluate(&retriever, &dataset, &config)?;
println!("{}", report.render_table());
```

The `Retriever` trait is sync because eval runs are batched / offline; if your
backend is async, run the search inside `tokio::runtime::Handle::block_on(...)`
or stage embeddings/results in a buffer.

## Bundled datasets

| Path | Size | Source | License |
| --- | --- | --- | --- |
| `agentflow-rag/examples/datasets/agentflow_mini` | 16 docs / 12 queries | synthetic, hand-authored from AgentFlow architecture facts | MIT |

The mini dataset is intended as a CI smoke test, not a benchmark. For real
quality numbers, point the harness at a public IR dataset
(BEIR/SciFact/MS-MARCO subsets) converted to the JSONL format above. A
conversion utility for BEIR is on the roadmap.

## Related

- [`docs/PHASE3_CHANGELOG.md`](../agentflow-rag/PHASE3_CHANGELOG.md) — RAG
  pipeline history.
- `agentflow rag search|index|collections` — operational CLI on top of the
  same retrieval stack.
