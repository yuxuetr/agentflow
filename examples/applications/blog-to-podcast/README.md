# A1 — blog → two-speaker podcast

**Status**: TODO (scaffold only)
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

## Files (to be created during implementation)

```
blog-to-podcast/
├── README.md                # ← this file
├── Cargo.toml               # path deps: agentflow-* + phonon-podcast
├── src/
│   └── podcast_node.rs      # custom AgentFlow node wrapping phonon
├── workflow.yml             # the orchestrating DAG
├── fixtures/
│   ├── short_blog.md        # ~500 words
│   ├── medium_blog.md       # ~2000 words
│   └── long_blog.md         # ~5000 words
├── assets/
│   ├── intro.wav            # 5s intro (optional)
│   ├── outro.wav            # 5s outro (optional)
│   └── bgm.wav              # background music loop (optional)
└── tests/
    └── smoke.rs             # end-to-end smoke (self-skip if no API key)
```

## Run (planned, not yet implemented)

Default Moonshot + MiniMax combo:

```bash
cd examples/applications/blog-to-podcast
export MOONSHOT_API_KEY=sk-...     # for LLM (outline + script)
export MINIMAX_API_KEY=eyJ...      # for TTS
cargo run --release -- workflow run workflow.yml \
  --input blog_source=fixtures/medium_blog.md \
  --input output_path=/tmp/episode.wav
```

Free-tier fallback (Edge TTS, no MiniMax key):

```bash
export MOONSHOT_API_KEY=sk-...     # still need an LLM key
cargo run --release -- workflow run workflow.yml \
  --input blog_source=fixtures/medium_blog.md \
  --input output_path=/tmp/episode.wav \
  --input tts_provider=edge \
  --input host_voice=zh-CN-YunyangNeural \
  --input guest_voice=zh-CN-XiaoxiaoNeural
```

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
