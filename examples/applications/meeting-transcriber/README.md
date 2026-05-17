# A4 — meeting-transcriber

**Status**: TODO (scaffold only)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A4](../../../EXAMPLES_TODOs.md#a4--meeting-transcriber)

## Business

Input: a meeting recording (`.wav` / `.mp3` / `.m4a`).
Output:
- `transcript.md` — full transcript, ideally speaker-segmented.
- `summary.md` — meeting summary (decisions, topics covered).
- `action_items.md` — extracted action items, format `"<owner> will
  <action> by <deadline>"`.

## Architecture (planned)

```
file load (audio) →
  asr_node (Whisper or StepFun ASR) →
  llm summarize (per-section if long) →
  llm extract_action_items →
  file write × 3
```

For long meetings (> ASR context window), the audio gets chunked first;
ASR output is concatenated with speaker-turn detection (if the provider
supports it) before summary.

## External dependencies

| Dep | Why |
| --- | --- |
| ASR provider | One of: StepFun ASR / OpenAI Whisper API / local Whisper |
| LLM provider | Summary + action item extraction |

Provider choice is a cost / quality / privacy tradeoff documented in
this README at implementation time.

## What this validates in AgentFlow

- `asr` node (existing)
- LLM multi-call pipeline with structured output (action items JSON)
- Long-input chunking strategy
- Multi-file output convention

## Findings during dogfooding

_Pending implementation._
