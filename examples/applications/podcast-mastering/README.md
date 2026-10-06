# A1.5 — podcast-mastering (L3 phonon-mcp validation)

**Status**: WIP — live end-to-end ✅ (2026-05-18 first run on
`/tmp/episode-test.wav`, A1's output).
**Tracking entry**: [`EXAMPLES_TODOs.md` § A1.5](../../../EXAMPLES_TODOs.md#a15--podcast-mastering)
**Sibling**: this app post-processes the output of
[A1 blog-to-podcast](../blog-to-podcast/).

## Business

Given a finished podcast `.wav`, run it through standard mastering
steps so it's ready for upload to a podcast platform: LUFS normalize
to a target loudness (-16 / -14), apply gentle fade in / fade out,
write the mastered result.

## Why this is in the dogfooding tree

This is the **L3 validation** in our three-tier integration
architecture (see [the 3-tier table in
EXAMPLES_TODOs.md](../../../EXAMPLES_TODOs.md)).
While [A1](../blog-to-podcast/) validates **L1** (yanshi Flow with
custom AsyncNode wrapping phonon as a Rust library), this app
validates **L3** (yanshi ReAct agent driving phonon-mcp as a
separate subprocess over stdio JSON-RPC).

Same `/tmp/episode-test.wav` audio buffer, completely different
integration path:

```
A1 (L1):  yanshi Flow → custom PodcastNode → phonon-podcast lib → audio
A1.5(L3): yanshi ReAct → mcp_phonon_audio_* tools → phonon-mcp subprocess → audio
```

## Architecture

```
┌─────────────────┐  spawns         ┌─────────────────────┐
│ yanshi CLI   │ ───stdio──────▶ │ phonon-mcp binary   │
│ (skill run)     │ ◀──JSON-RPC──── │ (Server::run_stdio) │
│                 │                 │                     │
│ ReActAgent loop │                 │ AssetRegistry       │
│  + ToolRegistry │                 │   handle → AudioBuf │
│  + Moonshot LLM │                 │ 14 audio_* tools    │
└─────────────────┘                 └─────────────────────┘
       │
       ▼
  user request →
  "master /tmp/x.wav to -16 LUFS"
       │
       ▼
  6-step ReAct loop:
  1. audio_load(path)              → handle_1
  2. audio_info(handle_1)          → {duration, sample_rate, channels}
  3. audio_loudness(handle_1)      → {lufs_integrated, rms, peak}
  4. audio_normalize_lufs(handle_1, -16) → handle_2
  5. audio_fade(handle_2, 0.5, 2.0)      → handle_3
  6. audio_save(handle_3, output_path)
  → final answer with before/after LUFS report
```

## External dependencies

| Dep | How to satisfy |
| --- | --- |
| `phonon-mcp` binary | `cd /Users/hal/rustspace/phonon && cargo build --release -p phonon-mcp`. Binary lands at `/Users/hal/.target/release/phonon-mcp`. Path is hardcoded in `skill.toml`'s `[[mcp_servers]].command`; update if you move it. |
| `MOONSHOT_API_KEY` env | Auto-loaded from `~/.yanshi/.env` if present, else `source` it manually. |
| `yanshi` CLI (release build recommended) | `cargo build --release -p yanshi-cli`. |

## Files

```
podcast-mastering/
├── README.md          # ← this file
└── skill.toml         # skill manifest with persona, model, mcp_servers,
                       # and the security.mcp_command_allowlist opt-in
                       # for phonon-mcp (compiled Rust binary, not in
                       # the default interpreter allowlist)
```

This app ships **only a skill manifest** — no Rust code. The whole
behaviour is the LLM following the persona's step-by-step instructions
and calling the right tools in order. That's the L3 thesis: agent
orchestration via natural-language persona + MCP tool schemas, zero
project-specific Rust code.

## Run

```bash
# Pre-flight (once per host):
cd /Users/hal/rustspace/phonon
cargo build --release -p phonon-mcp     # produces /Users/hal/.target/release/phonon-mcp

cd /Users/hal/arch/yanshi
cargo build --release -p yanshi-cli  # produces /Users/hal/.target/release/yanshi

# Run (assumes /tmp/episode-test.wav exists; produce it via A1 first):
/Users/hal/.target/release/yanshi skill run \
  examples/applications/podcast-mastering \
  --message "请把 /tmp/episode-test.wav 做后期：归一化到 -16 LUFS，加 0.5s 淡入 + 2s 淡出，输出到 /tmp/episode-mastered.wav。汇报前后 LUFS 对比。" \
  --trace
```

Add `--trace` to see the full ReAct loop (every plan / tool_call /
tool_result event). Without it you only get the final answer.

Validation only (no LLM / network calls):

```bash
/Users/hal/.target/release/yanshi skill validate \
  examples/applications/podcast-mastering
```

Output ends with `discovered MCP tools: 14` + `✅ Skill is valid!` —
confirms phonon-mcp spawns cleanly and exposes 14 tools.

## First-run observations (2026-05-18)

- **End-to-end wall clock: ~37s** for 7 ReAct steps (6 tool calls + final
  answer), including LLM thinking time. Per-tool round-trip via stdio
  was sub-second for every phonon-mcp call.
- **Moonshot tool-calling worked first-shot**. `moonshot-v1-128k`
  honoured the persona's step-by-step instructions, never deviated
  from the prescribed tool order, correctly passed each step's
  returned `handle` as the next step's input.
- **handle UUID chain stayed consistent across calls** — proves
  phonon-mcp's `AssetRegistry` works correctly under the
  multi-tool-call agent pattern.
- **Output**: `/tmp/episode-mastered.wav` byte-frame-identical
  duration to source (147.99s × 44.1kHz × 16-bit mono), but content
  is LUFS-normalized (-19.45 → -16 dB) + faded.

## What this validates in Yanshi

- `[[mcp_servers]]` skill manifest field correctly spawns a subprocess
  MCP server (compiled Rust binary, not just script interpreters).
- `security.mcp_command_allowlist` properly gates which binaries can
  run as MCP servers — phonon-mcp needs explicit allowlist entry
  (good security default).
- `McpClientPool` + `McpToolAdapter` automatically expose all 14
  phonon-mcp tools as `mcp_phonon_*` named agent tools.
- ReAct agent + Moonshot native tool calling drives a 6-tool linear
  workflow through a step-by-step persona without deviation.
- Cross-process JSON-RPC over stdio survives passing UUID handles
  between tool calls (AssetRegistry pattern works as designed).

## Findings during dogfooding

See [`EXAMPLES_TODOs.md` § A1.5 Findings](../../../EXAMPLES_TODOs.md#a15--podcast-mastering)
for the live list.
