use clap::{Args, Subcommand};

use super::{collections, eval, index, search};

#[derive(Args)]
pub struct RagArgs {
  #[command(subcommand)]
  command: RagCommands,
}

#[derive(Subcommand)]
enum RagCommands {
  /// Operate on a RAG vector store directly: search / index / collections.
  ///
  /// RAG's user-facing retrieval path is the `rag_search` tool a Skill exposes
  /// via `knowledge: backend = "rag"` (P-A4.1 / P-A4.2). These `ops`
  /// subcommands are for operators inspecting or populating the store
  /// out-of-band, not the day-to-day agent retrieval path.
  #[command(subcommand)]
  Ops(RagOpsCommands),
  /// Evaluate a retriever against a labeled dataset
  Eval {
    /// Dataset directory (must contain corpus.jsonl, queries.jsonl, qrels.jsonl)
    #[arg(short, long)]
    dataset: std::path::PathBuf,
    /// Retriever backend: bm25 (default, offline), dense (in-memory
    /// cosine over OpenAI embeddings — requires OPENAI_API_KEY), or
    /// hybrid (RRF fusion of BM25 + dense).
    #[arg(short = 'r', long, default_value = "bm25",
          value_parser = ["bm25", "dense", "hybrid"])]
    retriever: String,
    /// Embedding model used by the dense / hybrid retrievers.
    /// Ignored when --retriever bm25. Default text-embedding-3-small.
    #[arg(long, default_value = "text-embedding-3-small")]
    embedding_model: String,
    /// K cutoffs for Recall@K and nDCG@K (default: 1, 3, 5, 10)
    #[arg(short = 'k', long, value_delimiter = ',')]
    k_values: Vec<usize>,
    /// Compare baseline against a candidate retriever spec, e.g. "k1=1.5,b=0.6"
    #[arg(long)]
    compare_to: Option<String>,
    /// Compare the fresh run against a stored baseline snapshot
    /// (`agentflow-rag/eval_baselines/<dataset>/<retriever>.json`).
    /// Mutually exclusive with --compare-to.
    #[arg(long)]
    compare_baseline: Option<std::path::PathBuf>,
    /// Absolute drop in Recall@5 that, together with a significant
    /// paired sign-test p-value, trips the regression gate. Default 0.03.
    #[arg(long)]
    regression_recall_threshold: Option<f64>,
    /// One-tailed paired sign-test p-value below which the gate trips
    /// (when paired with a recall drop). Default 0.05.
    #[arg(long)]
    regression_p_value: Option<f64>,
    /// Optional JSON report output path (legacy bare body — preserved
    /// unchanged for back-compat with baseline comparison tooling
    /// that parses `eval_baselines/<dataset>/<retriever>.json`)
    #[arg(short, long)]
    output: Option<std::path::PathBuf>,
    /// Stdout output format: text (default — colored progress +
    /// tables) or json-envelope (canonical `CliJsonEnvelope` wraps
    /// the same payload `--output` writes). `--output` still writes
    /// the legacy bare body to disk regardless.
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
    /// Re-chunk the corpus with a fixed-size chunker (overlap=0)
    /// before building the retriever index, then remap retrieved
    /// chunk ids back to source doc ids before scoring (P10.6.3).
    /// Surfaces the chunking-strategy dimension of the eval — capture
    /// one baseline per chunk size to compare across them. The
    /// emitted report's `chunk_size` field persists the size used,
    /// so `--compare-baseline` can warn when the baseline + current
    /// run used different chunkings. When unset, behaviour is
    /// identical to pre-P10.6.3 (single un-chunked index).
    #[arg(long)]
    chunk_size: Option<usize>,
    /// Chunking strategy used when --chunk-size is set (L4.1). One of
    /// fixed_size (default), sentence, recursive, paragraph, heading, or
    /// code_ast (Rust source only; requires the CLI to be built with
    /// agentflow-rag's `code-chunking` feature). Lets an operator build a
    /// chunking-strategy comparison group instead of only fixed-size,
    /// e.g. run once per strategy and diff the reports.
    #[arg(long, default_value = "fixed_size",
          value_parser = ["fixed_size", "sentence", "recursive", "paragraph", "heading", "code_ast"])]
    chunk_strategy: String,
  },
}

/// Operational RAG subcommands (`agentflow rag ops <cmd>`) — see
/// [`RagCommands::Ops`]. These manage a vector store directly; the agent
/// retrieval path is the `rag_search` tool surfaced by a Skill.
#[derive(Subcommand)]
enum RagOpsCommands {
  /// Search documents in a RAG collection
  Search {
    /// Qdrant URL
    #[arg(long, default_value = "http://localhost:6334")]
    qdrant_url: String,
    /// Collection name
    #[arg(short, long)]
    collection: String,
    /// Search query
    #[arg(short, long)]
    query: String,
    /// Number of results to return
    #[arg(short = 'k', long, default_value_t = 5)]
    top_k: usize,
    /// Search type: semantic, hybrid, or keyword
    #[arg(short = 't', long, default_value = "semantic")]
    search_type: String,
    /// Alpha for hybrid search (0.0=keyword, 1.0=semantic)
    #[arg(short, long, default_value_t = 0.5)]
    alpha: f32,
    /// Enable MMR re-ranking for diversity
    #[arg(long)]
    rerank: bool,
    /// Lambda for MMR (0.0=diversity, 1.0=relevance)
    #[arg(short, long, default_value_t = 0.5)]
    lambda: f32,
    /// OpenAI embedding model
    #[arg(short = 'm', long, default_value = "text-embedding-3-small")]
    embedding_model: String,
    /// Output file path to save results as JSON (legacy bare body —
    /// preserved unchanged for back-compat with RAG eval baseline
    /// tooling that parses these files)
    #[arg(short, long)]
    output: Option<String>,
    /// Stdout output format: text (default — colored table) or
    /// json-envelope (canonical `CliJsonEnvelope` — wraps the same
    /// payload the legacy `--output` file mode emits). `--output`
    /// still writes the legacy bare body to disk regardless.
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// Index documents into a RAG collection
  Index {
    /// Qdrant URL
    #[arg(long, default_value = "http://localhost:6334")]
    qdrant_url: String,
    /// Collection name
    #[arg(short, long)]
    collection: String,
    /// Documents as JSON array: [{"content": "...", "metadata": {...}}]
    #[arg(short, long)]
    documents: String,
    /// OpenAI embedding model
    #[arg(short = 'm', long, default_value = "text-embedding-3-small")]
    embedding_model: String,
  },
  /// Manage RAG collections (create, delete, list, stats)
  Collections {
    /// Qdrant URL
    #[arg(long, default_value = "http://localhost:6334")]
    qdrant_url: String,
    /// Operation: create, delete, list, stats
    #[arg(short, long)]
    operation: String,
    /// Collection name (required for create, delete, stats)
    #[arg(short, long)]
    collection: Option<String>,
    /// Vector size (for create operation)
    #[arg(short = 's', long)]
    vector_size: Option<usize>,
    /// Distance metric: cosine, euclidean, dot (for create operation)
    #[arg(short, long)]
    distance: Option<String>,
  },
}

pub async fn dispatch(args: RagArgs) -> anyhow::Result<()> {
  match args.command {
    RagCommands::Ops(ops) => match ops {
      RagOpsCommands::Search {
        qdrant_url,
        collection,
        query,
        top_k,
        search_type,
        alpha,
        rerank,
        lambda,
        embedding_model,
        output,
        format,
      } => {
        search::execute(
          qdrant_url,
          collection,
          query,
          top_k,
          search_type,
          alpha,
          rerank,
          lambda,
          embedding_model,
          output,
          format,
        )
        .await
      }
      RagOpsCommands::Index {
        qdrant_url,
        collection,
        documents,
        embedding_model,
      } => index::execute(qdrant_url, collection, documents, embedding_model).await,
      RagOpsCommands::Collections {
        qdrant_url,
        operation,
        collection,
        vector_size,
        distance,
      } => collections::execute(qdrant_url, operation, collection, vector_size, distance).await,
    },
    RagCommands::Eval {
      dataset,
      retriever,
      embedding_model,
      k_values,
      compare_to,
      compare_baseline,
      regression_recall_threshold,
      regression_p_value,
      output,
      format,
      chunk_size,
      chunk_strategy,
    } => {
      eval::execute(
        dataset,
        retriever,
        embedding_model,
        k_values,
        compare_to,
        compare_baseline,
        regression_recall_threshold,
        regression_p_value,
        output,
        format,
        chunk_size,
        chunk_strategy,
      )
      .await
    }
  }
}
