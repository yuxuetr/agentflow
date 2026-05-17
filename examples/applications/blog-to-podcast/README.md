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
| OpenAI API key | LLM for outline + TTS via phonon's `OpenAiTts` | `OPENAI_API_KEY` env |
| *(or)* Edge TTS | Free TTS alternative (phonon's `EdgeTts`) | No key — uses Microsoft's free anonymous endpoint |
| *(or)* ElevenLabs | Premium voice TTS | `ELEVENLABS_API_KEY` env |

LLM provider for outline can be any agentflow-llm supported provider —
pick via the workflow's `llm` node `model:` field.

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

```bash
cd examples/applications/blog-to-podcast
export OPENAI_API_KEY=sk-...
cargo run --release -- workflow run workflow.yml \
  --input blog_source=fixtures/medium_blog.md \
  --input output_path=/tmp/episode.wav
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
