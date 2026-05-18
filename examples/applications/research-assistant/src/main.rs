//! `research-assistant` — A3 Plan A (L1 binary).
//!
//! Fetches recent papers in an arxiv category, dedupes against the
//! local "seen papers" entity-fact store (so periodic runs only see
//! genuinely new arrivals), and asks one LLM call to produce a markdown
//! briefing. Writes the briefing to disk and marks the new papers as
//! seen so next run won't repeat them.
//!
//! ## Why L1 binary (not L3 skill)
//!
//! Per the L1+L3 R2 reflection rule:
//!
//! - Pass-through axis: HIGH. The arxiv category string, max-results
//!   count, state-db path, and output path all need to reach tool
//!   calls verbatim. Per A7's lesson, L3 skill agents substitute these
//!   with hallucinated examples.
//! - Decision density: LOW. The pipeline is fixed: fetch → dedupe →
//!   summarize → write. No genuine agent branching.
//!
//! Both axes point at L1. Wraps one shell-out-equivalent (HTTP fetch)
//! + one SQLite read/write + one LLM call.
//!
//! ## Usage
//!
//! ```bash
//! research-assistant \
//!   --category cs.AI \
//!   --max-results 30 \
//!   --output /tmp/arxiv-cs-AI.md
//!
//! # Subsequent runs only summarize NEW papers (dedup state at
//! # ~/.agentflow/state/research-assistant.db by default).
//! ```

mod arxiv_fetch;
mod briefing;
mod seen_store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::error::AgentFlowError;
use agentflow_core::events::ConsoleListener;
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::value::FlowValue;
use agentflow_llm::AgentFlow as LlmInit;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tracing::info;
use tracing_subscriber::EnvFilter;

use arxiv_fetch::Paper;
use seen_store::SeenStore;

fn load_agentflow_dotenv() {
  if let Some(home) = std::env::home_dir() {
    let _ = dotenvy::from_path(home.join(".agentflow").join(".env"));
  }
}

fn default_state_path() -> PathBuf {
  std::env::home_dir()
    .map(|h| h.join(".agentflow/state/research-assistant.db"))
    .unwrap_or_else(|| PathBuf::from("research-assistant.db"))
}

// ── Nodes ─────────────────────────────────────────────────────────────────

/// Stage 1 — fetch the arxiv category's recent papers via the public
/// Atom feed. Returns the full unfiltered list; dedup happens in stage 2.
struct FetchArxivNode {
  category: String,
  max_results: u32,
  // Shared bus for handing off the in-memory Vec<Paper> between nodes
  // without round-tripping through FlowValue::Json. The Flow's
  // input_mapping is by-output-key, not by reference type, so we'd
  // otherwise have to serialise + parse the paper list twice.
  bus: Arc<Mutex<Option<Vec<Paper>>>>,
}

#[async_trait]
impl AsyncNode for FetchArxivNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let papers = arxiv_fetch::fetch_recent(&self.category, self.max_results)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!(
          "fetch_recent({}, {}): {err:#}",
          self.category, self.max_results
        ),
      })?;
    let count = papers.len();
    {
      let mut bus = self.bus.lock().await;
      *bus = Some(papers);
    }
    let mut outputs = HashMap::new();
    outputs.insert("count".to_string(), FlowValue::Json(Value::from(count)));
    Ok(outputs)
  }
}

/// Stage 2 — diff against the SQLite-backed seen-papers store. Returns
/// the unseen subset on the bus AND records them as seen (so next run
/// won't repeat). If the LLM call later fails, the worst case is a
/// repeat of those paper ids in a re-run — better than silently losing
/// "new since" tracking by deferring the mark until after success.
struct DiffSeenNode {
  category: String,
  state_path: PathBuf,
  bus: Arc<Mutex<Option<Vec<Paper>>>>,
}

#[async_trait]
impl AsyncNode for DiffSeenNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let all_papers = {
      let mut bus = self.bus.lock().await;
      bus
        .take()
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "FetchArxivNode produced no paper list on the bus".to_string(),
        })?
    };

    let mut seen = SeenStore::open(self.state_path.clone())
      .await
      .map_err(|err| AgentFlowError::ConfigurationError {
        message: format!("open SeenStore({}): {err:#}", self.state_path.display()),
      })?;
    let unseen = seen
      .filter_unseen(&self.category, &all_papers)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("filter_unseen: {err:#}"),
      })?;
    seen
      .mark_seen_batch(&self.category, &unseen)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("mark_seen_batch: {err:#}"),
      })?;

    let unseen_count = unseen.len();
    let total = all_papers.len();
    info!(
      total_fetched = total,
      new_papers = unseen_count,
      "dedup complete"
    );
    {
      let mut bus = self.bus.lock().await;
      *bus = Some(unseen);
    }
    let mut outputs = HashMap::new();
    outputs.insert(
      "stats".to_string(),
      FlowValue::Json(json!({"total_fetched": total, "new_papers": unseen_count})),
    );
    Ok(outputs)
  }
}

/// Stage 3 — one-shot LLM call to render the briefing markdown for the
/// unseen subset. Writes to disk + returns a summary blob for the CLI
/// to print.
struct BriefingNode {
  category: String,
  model: String,
  output: PathBuf,
  bus: Arc<Mutex<Option<Vec<Paper>>>>,
}

#[async_trait]
impl AsyncNode for BriefingNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let papers = {
      let mut bus = self.bus.lock().await;
      bus
        .take()
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "DiffSeenNode produced no unseen-paper list on the bus".to_string(),
        })?
    };

    if papers.is_empty() {
      let msg = format!(
        "# Arxiv Briefing — `{}` (no new papers)\n\nAll most-recent papers were already seen.\n",
        self.category
      );
      tokio::fs::write(&self.output, &msg).await.map_err(|e| {
        AgentFlowError::AsyncExecutionError {
          message: format!("write {}: {e}", self.output.display()),
        }
      })?;
      let mut outputs = HashMap::new();
      outputs.insert(
        "summary".to_string(),
        FlowValue::Json(json!({
          "new_papers": 0,
          "output_path": self.output.to_string_lossy().to_string(),
          "llm_chars": 0,
        })),
      );
      return Ok(outputs);
    }

    let markdown = briefing::render_briefing(&self.category, &papers, &self.model, None)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("render_briefing: {err:#}"),
      })?;

    tokio::fs::write(&self.output, &markdown)
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError {
        message: format!("write {}: {e}", self.output.display()),
      })?;

    let mut outputs = HashMap::new();
    outputs.insert(
      "summary".to_string(),
      FlowValue::Json(json!({
        "new_papers": papers.len(),
        "output_path": self.output.to_string_lossy().to_string(),
        "llm_chars": markdown.chars().count(),
      })),
    );
    Ok(outputs)
  }
}

// ── CLI ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Args {
  category: String,
  max_results: u32,
  output: PathBuf,
  state_path: PathBuf,
  model: String,
}

fn parse_args() -> Result<Args> {
  let mut category: Option<String> = None;
  let mut max_results: u32 = 30;
  let mut output: Option<PathBuf> = None;
  let mut state_path: Option<PathBuf> = None;
  let mut model: String = "moonshot-v1-128k".to_string();

  let mut it = std::env::args().skip(1);
  while let Some(flag) = it.next() {
    match flag.as_str() {
      "--category" => category = Some(it.next().context("--category expects a value")?),
      "--max-results" => {
        max_results = it
          .next()
          .context("--max-results expects a number")?
          .parse()
          .context("--max-results must be an integer")?;
      }
      "--output" => output = Some(it.next().context("--output expects a path")?.into()),
      "--state" => state_path = Some(it.next().context("--state expects a path")?.into()),
      "--model" => model = it.next().context("--model expects a model name")?,
      "-h" | "--help" => {
        print_help();
        std::process::exit(0);
      }
      other => anyhow::bail!("unknown flag `{other}` — pass --help for usage"),
    }
  }

  Ok(Args {
    category: category.context("--category is required (e.g. cs.AI, cs.CL, math.ST)")?,
    max_results,
    output: output.unwrap_or_else(|| PathBuf::from("/tmp/arxiv-briefing.md")),
    state_path: state_path.unwrap_or_else(default_state_path),
    model,
  })
}

fn print_help() {
  println!(
    "research-assistant — A3 AgentFlow app (arxiv briefing via dedup + LLM summary)\n\
     \n\
     USAGE:\n  \
       research-assistant --category <cat> [--max-results N] [--output <path>]\n  \
                          [--state <path>] [--model <name>]\n\
     \n\
     FLAGS:\n  \
       --category <cat>     Arxiv category (cs.AI / cs.CL / math.ST / …) [required]\n  \
       --max-results <N>    How many recent papers to fetch (default: 30; arxiv max: 2000)\n  \
       --output <path>      Where to write the markdown briefing (default: /tmp/arxiv-briefing.md)\n  \
       --state <path>       SQLite file for seen-papers dedup (default: ~/.agentflow/state/research-assistant.db)\n  \
       --model <name>       LLM model for the briefing call (default: moonshot-v1-128k)\n  \
       -h, --help           Show this help\n\
     \n\
     ENV:\n  \
       MOONSHOT_API_KEY     Required by the default model. Auto-loaded from\n  \
                            ~/.agentflow/.env if present.\n"
  );
}

#[tokio::main]
async fn main() -> Result<()> {
  load_agentflow_dotenv();
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();

  let args = parse_args()?;
  info!(?args, "starting research-assistant");

  LlmInit::init()
    .await
    .context("failed to initialise agentflow-llm (model registry / provider config)")?;

  let bus: Arc<Mutex<Option<Vec<Paper>>>> = Arc::new(Mutex::new(None));

  let flow = Flow::new(vec![
    GraphNode {
      id: "fetch_arxiv".to_string(),
      node_type: NodeType::Standard(Arc::new(FetchArxivNode {
        category: args.category.clone(),
        max_results: args.max_results,
        bus: Arc::clone(&bus),
      })),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "diff_seen".to_string(),
      node_type: NodeType::Standard(Arc::new(DiffSeenNode {
        category: args.category.clone(),
        state_path: args.state_path.clone(),
        bus: Arc::clone(&bus),
      })),
      dependencies: vec!["fetch_arxiv".to_string()],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "briefing".to_string(),
      node_type: NodeType::Standard(Arc::new(BriefingNode {
        category: args.category.clone(),
        model: args.model.clone(),
        output: args.output.clone(),
        bus: Arc::clone(&bus),
      })),
      dependencies: vec!["diff_seen".to_string()],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ])
  .with_event_listener(Arc::new(ConsoleListener));

  let state = flow.run().await?;
  let summary = state
    .get("briefing")
    .and_then(|res| res.as_ref().ok())
    .and_then(|outs| outs.get("summary"))
    .ok_or_else(|| anyhow::anyhow!("briefing node did not produce a summary"))?;

  match summary {
    FlowValue::Json(value) => {
      println!("\n=== Research briefing generated ===");
      println!("{}", serde_json::to_string_pretty(value)?);
      println!("Output: {}", args.output.display());
      println!("State:  {}", args.state_path.display());
    }
    other => println!("Unexpected summary shape: {other:?}"),
  }

  Ok(())
}
