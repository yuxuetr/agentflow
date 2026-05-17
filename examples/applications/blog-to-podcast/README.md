# A1 — blog → two-speaker podcast

**Status**: WIP (Plan A skeleton runnable; live end-to-end smoke pending API keys)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A1](../../../EXAMPLES_TODOs.md#a1--blog-to-podcast)

## Business

Input: a blog post (URL or local markdown file).
Output: a two-speaker dialogue podcast (`.wav` or `.mp3`) plus `.srt`
subtitles, with optional BGM / intro / outro / chapter markers.

## Architecture (Plan A — thin wrapper)

```
┌────────────────┐    ┌──────────────────┐    ┌──────────────────────────┐    ┌────────┐
│ HTTP / file    │ ──▶│ LLM outline      │ ──▶│ PodcastNode              │ ──▶│ file   │
│ (fetch blog)   │    │ (blog → topic +  │    │ (wraps                   │    │ write  │
│                │    │  key points)     │    │  phonon_podcast::        │    │ .wav   │
│                │    │                  │    │  PodcastPipeline         │    │ + .srt │
│                │    │                  │    │  end-to-end)             │    │        │
└────────────────┘    └──────────────────┘    └──────────────────────────┘    └────────┘
```

The `PodcastNode` is a custom AgentFlow node defined in this directory's
`src/` (created during implementation) that internally:

1. Calls `phonon_podcast::OpenAiScriptGenerator` with the outline as topic.
2. Runs `phonon_podcast::PodcastPipeline::generate` (concurrent TTS +
   crossfade + BGM + intro/outro + chapter detection + SRT).
3. Writes `.wav` + `.srt` to the configured output path.

## Architecture (Plan B — split DAG, deferred)

When dogfooding Plan A surfaces "I want to edit the script before TTS"
or "I want to retry a single segment's TTS", we'll split into:

```
fetch → outline → script_gen → tts (parallel per segment) →
  assemble → subtitle → file write
```

…where each step is its own AgentFlow node and the script is checkpointed
between `script_gen` and `tts`. Tracked in
[`EXAMPLES_TODOs.md` A1 Findings](../../../EXAMPLES_TODOs.md#a1--blog-to-podcast).

## External dependencies

| Dep | Why | How to get it |
| --- | --- | --- |
| `phonon-podcast` 0.7 | TTS + assembly + BGM + chapter + SRT pipeline | Path dep to `/Users/hal/rustspace/phonon/phonon-podcast` |
| `phonon-ai` 0.7 | Underlying TTS providers (MiniMax / OpenAI / Edge / ElevenLabs) | Re-exported from `phonon-podcast` |

### Provider matrix

| Step | Default (recommended) | Alternatives |
| --- | --- | --- |
| LLM (blog → outline → script) | **Moonshot** `kimi-k2-0905-preview` — long context, strong Chinese, OpenAI-compatible base URL (`https://api.moonshot.cn/v1`). Set `MOONSHOT_API_KEY`. | Any `agentflow-llm` provider (OpenAI / Anthropic / StepFun / DeepSeek / Mock). Pick via the workflow's `llm` node `model:` field. |
| TTS (per-segment voice) | **MiniMax T2A v2** `speech-2.8-hd` via phonon-ai's `MiniMaxTts` — has documented `Cantonese_podacast_host_*` voices, 9 emotion levels (calm/whisper/happy/...), 32 kHz native. Set `MINIMAX_API_KEY`. | `EdgeTts` (free, no key; Microsoft anonymous endpoint), `OpenAiTts` (paid, English-leaning), `ElevenLabsTts` (premium, best English). |

**Zero-OpenAI-key configuration**: Moonshot for LLM + MiniMax for TTS
covers the full pipeline using only mainland-Chinese providers, which
matters for billing / network access in PRC and gives MiniMax's
emotion control for a more podcast-feel result.

**Free-tier configuration**: any `agentflow-llm` provider for LLM
(mock works for the script structure; a real model is needed for
actual content) + `EdgeTts` for TTS (free, no key).

## Files (as implemented)

```
blog-to-podcast/
├── README.md                # ← this file
├── Cargo.toml               # standalone Cargo project; path deps to
│                            # agentflow-core + phonon-{ai,podcast,io,core}
├── src/
│   ├── main.rs              # binary entry; builds Flow(read_blog → produce_podcast)
│   └── podcast_node.rs      # custom AsyncNode wrapping phonon's pipeline
├── fixtures/
│   └── short_blog.md        # ~500 chars (zh-CN, Rust topic); medium / long
│                            # added when dogfooding demands them
└── tests/
    └── smoke.rs             # CLI smoke (--help / missing-flag / unknown-flag)
                             # + #[ignore]-d live end-to-end
```

**Not yet shipped** (deferred until dogfooding shows we need them):
- `workflow.yml` — the app is a standalone Rust binary, not a YAML
  workflow. Plan B (split into multiple AgentFlow nodes via YAML)
  would introduce one.
- `assets/` — intro / outro / BGM. phonon-podcast handles them when
  passed; this app's first run is bare voice-only.
- `medium_blog.md` / `long_blog.md` — added as the short fixture
  reveals limitations.

## Run

The binary owns its own CLI; no workflow YAML is involved (Plan A).

Default Moonshot + MiniMax combo:

```bash
cd examples/applications/blog-to-podcast
export MOONSHOT_API_KEY=sk-...     # for script generation
export MINIMAX_API_KEY=eyJ...      # for TTS

cargo run --release -- \
  --blog   fixtures/short_blog.md \
  --output /tmp/episode.wav
```

Free-tier fallback (Edge TTS, no MiniMax key):

```bash
export MOONSHOT_API_KEY=sk-...     # still need an LLM key

cargo run --release -- \
  --blog   fixtures/short_blog.md \
  --output /tmp/episode.wav \
  --tts    edge
```

CLI flags (also see `--help`):

| Flag | Default | Notes |
| --- | --- | --- |
| `--blog <path>` | (required) | UTF-8 markdown or plain text |
| `--output <path>` | `/tmp/episode.wav` | SRT written alongside with `.srt` extension |
| `--segments <N>` | `10` | Approximate dialogue segment count |
| `--tts <backend>` | `minimax` (or `$PODCAST_TTS`) | `minimax` / `edge` / `openai` |

Trace output goes to stderr via `tracing`; `RUST_LOG=blog_to_podcast=debug,phonon_podcast=debug,phonon_ai=debug`
turns on detailed per-step logs.

## Live smoke test

Hermetic CLI smoke tests run by default (`cargo test`). The live
end-to-end test is marked `#[ignore]` and runs when you opt in:

```bash
export MOONSHOT_API_KEY=sk-...
export MINIMAX_API_KEY=eyJ...   # or: export EDGE_TTS_OK=1 (and skip MINIMAX_API_KEY)
cargo test --test smoke --release -- --ignored --nocapture
```

It points at `fixtures/short_blog.md`, requests 4 segments to keep
cost / time low, and asserts the produced `.wav` is non-trivial and
the `.srt` exists.

## What this validates in AgentFlow

- Custom Rust node integration with an external workspace via path dep
- LLM node for content transformation (blog → outline)
- HTTP / file node for source fetch
- Trace replay for per-segment TTS latency + LLM token usage
- File output convention
- Optional: skill packaging (`skill.toml`) so this becomes
  `agentflow skill run podcast-producer`

## Findings during dogfooding

_Will be filled in as we use this application. Push insights up into
the main [`TODOs.md`](../../../TODOs.md) queue periodically._
