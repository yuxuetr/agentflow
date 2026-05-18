# A6 — doc-translator

**Status**: live ✅ as iteration 1 (2026-05-18, narrow validation of
`map parallel` + LLM fan-out; **no file I/O yet** — iter 2 expands).
**Tracking entry**: [`EXAMPLES_TODOs.md` § A6](../../../EXAMPLES_TODOs.md#a6--doc-translator)
**Why it's the next pillar**: per [R3 § 5](../../../docs/L1_L3_REFLECTION_R3_2026-05-18.md#5-recommended-next-pillar-a6-doc-translator),
`map` parallel was the biggest un-validated DAG primitive after the R2
sweep.

## Business (full A6 spec)

Input: a markdown folder + target language list (`["en", "ja", "zh"]`).
Output: per-language sibling folders preserving directory structure,
markdown formatting / code fences / heading hierarchy untouched, only
prose translated.

## Iteration 1 scope (intentionally narrow)

```
[map parallel, 4 items]
  │
  ├─ item 0 → template → llm  ─┐
  ├─ item 1 → template → llm  ─┤
  ├─ item 2 → template → llm  ─┼─→ [report fan-in]
  └─ item 3 → template → llm  ─┘
```

- **One hardcoded markdown blurb** instead of a `docs/` folder
- **4 target languages** (en, ja, fr, de) instead of arbitrary list
- **No file output** — sub-flows return strings, report node renders summary
- **No file discovery** — `input_list` hardcoded in YAML

The point of iter 1 is to exercise `map parallel + LLM` end-to-end
and capture findings on the primitive before scaling fan-out in iter 2.

## What this validates (already covered in iter 1)

- `map parallel: true` actually spawns sub-flows concurrently via
  `tokio::spawn` (verified end-to-end).
- Per-iteration `item` is exposed in sub-flow Tera context — template
  nodes can interpolate `{{ item.lang_name }}` cleanly.
- `input_mapping` correctly threads `nodes.build_prompt.outputs.output`
  into the downstream `llm` node's `prompt` parameter.
- `template → llm` sub-flow shape works as the canonical "render
  prompt, then call LLM" pattern (replaces the per-call inline Tera
  that doesn't exist in `LlmNode`).
- Fan-in via `nodes.translate.outputs.results` returns a JSON array
  of N sub-flow results, indexable by position.
- Per-sub-flow errors are preserved (don't crash the whole map) —
  the result array contains `Ok` and `Err` siblings, not a single
  Err that kills the run.

## Run

```bash
# Requires MOONSHOT_API_KEY in ~/.agentflow/.env (P9.3 auto-loads it)
agentflow workflow run examples/applications/doc-translator/workflow.yml

# Validate without execution
agentflow workflow validate examples/applications/doc-translator/workflow.yml
```

## Iteration 1 observations (2026-05-18)

- **Wall clock**: ~5s for N=4 parallel. First run (before F-A6-1
  fix) had all 4 fire simultaneously and the 4th hit a 429.
  Second run (after `max_concurrent: 3` shipped) is 4/4 OK.
- **Translation quality**: usable but uneven. Japanese / French /
  German came back correct. The English request still produces
  Chinese (separate issue — see F-A6-4).
- **Rate-limit collision (was a finding, now fixed)**: F-A6-1's
  unbounded `tokio::spawn` blew past Moonshot's 3-concurrent cap;
  the closing commit adds the `max_concurrent` knob this workflow
  now uses.

## Files

```
doc-translator/
├── README.md       # ← this file
└── workflow.yml    # the map + LLM workflow (iter 1)
```

## Findings (iteration 1)

These are sediment, captured in [`EXAMPLES_TODOs.md` § A6
Findings](../../../EXAMPLES_TODOs.md#a6--doc-translator) for the live
list.

- **F-A6-1 — `map parallel: true` has no concurrency cap**. ✅
  **CLOSED 2026-05-18**: added `max_concurrent: Option<usize>` to
  `NodeType::Map`, plumbed through YAML factory + `tokio::sync::
  Semaphore` in `execute_map_node_parallel`. Re-running this
  workflow with `max_concurrent: 3` on N=4 inputs now yields 4/4
  successes (was 3/4 before). Two new unit tests in
  `agentflow-core` assert the cap holds in practice and that
  `Some(0)` is rejected rather than deadlocking.

- **F-A6-2 — schema validator warns `input_list is not defined in
  the CLI schema for node type 'map'`**. ✅ **CLOSED 2026-05-18**:
  added `input_list` and `max_concurrent` to the map ParamSpec
  list in `agentflow-cli/src/config/schema.rs`. `agentflow workflow
  validate` now reports `✅ Schema validation passed` on this
  workflow.

- **F-A6-3 — per-sub-flow Err is buried inside the results array**.
  ✅ **CLOSED 2026-05-18**: map node now emits
  `results_summary: {total, ok, err, err_indexes}` alongside
  `results`. The doc-translator workflow's clean N=4 run now ships
  `total=4 ok=4 err=0 err_indexes=[]` as a sibling output.
  Downstream nodes can route on `results_summary.err > 0` via
  `run_if` without walking the nested `results` JSON. Partial
  failures also `eprintln!` to stderr so they're visible even
  when nothing downstream routes on the summary.

- **F-A6-3 — per-sub-flow Err is buried inside the results array**,
  not surfaced at the map node level. The top-level result is
  `Ok({results: [...]})`; failures live inside `results[i].
  translate_one.Err`. A workflow author who only checks the
  top-level Ok will silently miss partial failures. **Action**:
  consider emitting a `results_summary` output on map (`{
  total: 4, ok: 3, err: 1, err_indexes: [3] }`) so downstream nodes
  can route on failure without walking nested JSON. Or at minimum
  add a `tracing::warn!` for any sub-flow that returns an
  Err-containing state.

- **F-A6-4 — prompt ambiguity: "translate to {target_lang}" with
  English target produced Chinese output** on moonshot-v1-128k when
  the source was already English. The model interpreted "translate"
  literally and chose a different language. Workflow-author trap:
  for translation, validate that `source_lang != target_lang`
  before dispatching. Easy guard at the `build_prompt` template
  step (Tera `{% if item.lang != "en" %} ... {% endif %}` or
  workflow-level filter). Not an agentflow bug.

## What's NOT in iteration 1 (iter 2+ roadmap)

- **Real file I/O** — discover `*.md` in an input dir, write
  per-language outputs. Requires `file` node integration in the
  sub-flow.
- **Concurrency cap** — see F-A6-1. Iter 2 should add
  `max_concurrent: N` to map YAML so the workflow runs without
  per-provider tuning.
- **Code-fence preservation testing** — verify the LLM actually
  honours the "don't translate code blocks" rule across 5-10
  realistic markdown sources (not just hello-world).
- **Checkpoint resume** — kill mid-run, restart, skip already-
  translated (file, lang) pairs.
- **100+ file fanout stress test** — the headline A6 validation
  that needs all the above before it's even attemptable.
