//! `blog-to-podcast` binary — A1 podcast app (Plan A thin wrapper).
//!
//! Reads a blog (markdown or plain text) from disk and produces a
//! two-speaker podcast `.wav` + `.srt` by delegating the heavy lifting
//! to `phonon-podcast` (script generation + TTS + assembly).
//!
//! The interesting bit for AgentFlow validation is the 2-node DAG —
//! the trace event listener emits one event per node transition, and
//! the `Flow` orchestration / checkpoint surface remains usable for
//! future Plan B (per-segment retry) splits.
//!
//! ## Usage
//!
//! ```bash
//! export MOONSHOT_API_KEY=sk-...        # required, for script_gen
//! export MINIMAX_API_KEY=eyJ...         # if PODCAST_TTS=minimax (default)
//! # PODCAST_TTS=edge                    # free fallback, no MINIMAX key needed
//!
//! cargo run --release -- \
//!   --blog   examples/applications/blog-to-podcast/fixtures/short_blog.md \
//!   --output /tmp/episode.wav
//! ```

mod podcast_node;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::error::AgentFlowError;
use agentflow_core::events::ConsoleListener;
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::value::FlowValue;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use tracing::info;
use tracing_subscriber::EnvFilter;

use podcast_node::{PodcastNode, PodcastNodeConfig, TtsBackend};

/// Reads a text file (UTF-8) and surfaces the contents as
/// `outputs["text"] = FlowValue::Json(String)`. Kept inline here rather
/// than reaching into `agentflow-nodes::FileNode` because the FileNode
/// API has more knobs than this app needs.
struct ReadBlogNode {
  path: PathBuf,
}

#[async_trait]
impl AsyncNode for ReadBlogNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let text =
      std::fs::read_to_string(&self.path).map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("read blog at {}: {err}", self.path.display()),
      })?;
    let mut outputs = HashMap::new();
    outputs.insert("text".to_string(), FlowValue::Json(Value::String(text)));
    Ok(outputs)
  }
}

#[derive(Debug)]
struct Args {
  blog: PathBuf,
  output: PathBuf,
  target_segments: usize,
  tts: TtsBackend,
}

fn parse_args() -> Result<Args> {
  let mut blog: Option<PathBuf> = None;
  let mut output: Option<PathBuf> = None;
  let mut target_segments: usize = 10;
  let mut tts: Option<TtsBackend> = None;

  let mut args = std::env::args().skip(1);
  while let Some(flag) = args.next() {
    match flag.as_str() {
      "--blog" => blog = Some(args.next().context("--blog expects a path")?.into()),
      "--output" => output = Some(args.next().context("--output expects a path")?.into()),
      "--segments" => {
        target_segments = args
          .next()
          .context("--segments expects a number")?
          .parse()
          .context("--segments must be an integer")?;
      }
      "--tts" => {
        let v = args.next().context("--tts expects minimax|edge|openai")?;
        tts = Some(match v.as_str() {
          "minimax" => TtsBackend::MiniMax,
          "edge" => TtsBackend::Edge,
          "openai" => TtsBackend::OpenAi,
          other => anyhow::bail!("--tts: unknown value `{other}`"),
        });
      }
      "-h" | "--help" => {
        print_help();
        std::process::exit(0);
      }
      other => anyhow::bail!("unknown flag `{other}` — pass --help for usage"),
    }
  }

  Ok(Args {
    blog: blog.context("--blog is required")?,
    output: output.unwrap_or_else(|| PathBuf::from("/tmp/episode.wav")),
    target_segments,
    tts: tts.unwrap_or_else(TtsBackend::from_env),
  })
}

fn print_help() {
  println!(
    "blog-to-podcast — AgentFlow A1 app (Plan A thin wrapper)\n\
     \n\
     USAGE:\n  \
       blog-to-podcast --blog <path> [--output <path>] [--segments N] [--tts minimax|edge|openai]\n\
     \n\
     FLAGS:\n  \
       --blog <path>       Source blog (markdown / text) [required]\n  \
       --output <path>     Output audio path (default: /tmp/episode.wav). SRT is written alongside.\n  \
       --segments <N>      Approx number of dialogue segments to generate (default: 10).\n  \
       --tts <backend>     TTS provider: minimax (default) / edge (free) / openai. Also via PODCAST_TTS env.\n  \
       -h, --help          Show this help.\n\
     \n\
     ENV:\n  \
       MOONSHOT_API_KEY    Required for script generation (Moonshot via OpenAI-compat base URL).\n  \
       MINIMAX_API_KEY     Required if --tts minimax (the default).\n  \
       OPENAI_API_KEY      Required if --tts openai.\n  \
       PODCAST_TTS         Backend override if --tts not passed.\n"
  );
}

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();

  let args = parse_args()?;
  info!(?args, "starting blog-to-podcast");

  let mut podcast_cfg = PodcastNodeConfig::default_moonshot_minimax();
  podcast_cfg.tts_backend = args.tts;
  podcast_cfg.target_segments = args.target_segments;
  if matches!(args.tts, TtsBackend::Edge) {
    // When using Edge TTS, swap voice IDs to Microsoft's zh-CN namespace.
    podcast_cfg = podcast_cfg.with_edge_tts();
  }

  let read_node = ReadBlogNode {
    path: args.blog.clone(),
  };
  let podcast_node = PodcastNode::new(podcast_cfg);

  let flow = Flow::new(vec![
    GraphNode {
      id: "read_blog".to_string(),
      node_type: NodeType::Standard(Arc::new(read_node)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "produce_podcast".to_string(),
      node_type: NodeType::Standard(Arc::new(podcast_node)),
      dependencies: vec!["read_blog".to_string()],
      input_mapping: Some(HashMap::from([(
        // PodcastNode input key  ←  source node id   .   output key
        "source_text".to_string(),
        ("read_blog".to_string(), "text".to_string()),
      )])),
      run_if: None,
      initial_inputs: HashMap::from([(
        "output_audio_path".to_string(),
        FlowValue::Json(Value::String(args.output.to_string_lossy().to_string())),
      )]),
    },
  ])
  .with_event_listener(Arc::new(ConsoleListener));

  let state = flow.run().await?;
  let podcast_summary = state
    .get("produce_podcast")
    .and_then(|res| res.as_ref().ok())
    .and_then(|outs| outs.get("summary"))
    .ok_or_else(|| anyhow::anyhow!("podcast node did not produce a summary"))?;

  match podcast_summary {
    FlowValue::Json(value) => {
      println!("\n=== Podcast generated ===");
      println!("{}", serde_json::to_string_pretty(value)?);
      println!("Audio: {}", args.output.display());
      println!("SRT:   {}", args.output.with_extension("srt").display());
    }
    other => println!("Unexpected summary shape: {other:?}"),
  }

  // Touch json! to keep the dep used; emit a trailing trace line so
  // operators can grep run logs for completion.
  info!(summary = %json!({"status": "ok"}), "blog-to-podcast complete");
  Ok(())
}
