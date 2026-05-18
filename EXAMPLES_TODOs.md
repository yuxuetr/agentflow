# AgentFlow Applications TODOs

Last updated: 2026-05-17

## 维护约定

- 这里追踪 **dogfooding 用的端到端 application 示例**（`examples/applications/<name>/`），
  区别于：
  - `examples/README.md` — **SDK feature 矩阵**（每个能力一个最小 demo，maintainer-facing）。
  - `examples/ecosystem/` — **生态形态样本**（skills / plugins / marketplace
    标准结构示范，contributor-facing）。
- 这里的 application 必须是「**真实业务场景**」，跨越多个 agentflow 子系统，
  目的是验证「agentflow 真能搭出可用产品」并发现实际使用中的缺口。
- 每个 application 一个目录 `examples/applications/<name>/`，必含：
  - `README.md` — 业务描述 + 架构 + 外部依赖 + 所需 API key
  - 至少一个 `workflow.yml` 或 `skill.toml`（如果用 skill 形态）
  - 自定义 Rust 节点（如需）放 `src/`
  - smoke 测试（若需要 live API 必须 self-skip）
- Dogfooding 过程中发现 agentflow 缺陷，写到本文件对应 application 的 `Findings`
  段；积够再回填 `TODOs.md` 的下一批 segment（例如「P8 Dogfooding-Driven
  Refinements」）。

## 状态约定

- `TODO`: 未开始或正在做
- `WIP`: 进行中（区别于 TODO，已有 commit）
- `DONE`: 已实现 + 跑通 + 文档完整
- `DEFERRED`: 显式推迟（说明原因）

## Active Queue

| # | Application | Status | 验证 agentflow 哪些面 | 外部依赖 |
| --- | --- | --- | --- | --- |
| A1 | [blog-to-podcast](examples/applications/blog-to-podcast/) | WIP — live smoke ✅ (1st run 2026-05-18) | custom Rust node, LLM, HTTP, file, trace, skill | phonon-podcast (path dep), Moonshot LLM + MiniMax TTS (default) / Edge TTS (free) |
| A1.5 | [podcast-mastering](examples/applications/podcast-mastering/) | WIP — live smoke ✅ (1st run 2026-05-18) | **L3 validation**: skill + `[[mcp_servers]]` + ReAct agent + native tool calling driving phonon-mcp subprocess | phonon-mcp binary (`cargo build --release -p phonon-mcp`), Moonshot LLM |
| A2 | [code-reviewer](examples/applications/code-reviewer/) | TODO | ReAct agent, MCP (github), skill packaging, tool admission | gh CLI / GitHub MCP server |
| A3 | [research-assistant](examples/applications/research-assistant/) | TODO | Arxiv node, RAG, memory, scheduled run | OpenAI/任一 LLM, 可选 Qdrant |
| A4 | [meeting-transcriber](examples/applications/meeting-transcriber/) | TODO | ASR node, LLM summarize, file output | Whisper (local 或 API) |
| A5 | [weekly-digest](examples/applications/weekly-digest/) | TODO | RAG, LLM, HTTP (SMTP/SendGrid), scheduled | SendGrid/Mailgun/SMTP |
| A6 | [doc-translator](examples/applications/doc-translator/) | TODO | template, batch / map (parallel), LLM, file | LLM API |
| A7 | [changelog-writer](examples/applications/changelog-writer/) | TODO | shell node, LLM, file（agentflow 给自己用） | 无（全本地） |

---

## A1 — blog-to-podcast

**业务**: 输入一篇 blog（URL 或本地 markdown），输出双人对话播客 `.wav` + `.srt`
字幕；可选 BGM / intro / outro / chapter 切分。

**为什么是第一个**: phonon-podcast 已经把 script_gen + TTS + 拼接 + BGM + 字幕
都做完了，agentflow 主要工作是把它包成可观测的 DAG，验证「agentflow + 外部
Rust 库」的集成路径是否顺滑。同时这是个 phonon 作者 + agentflow 作者都会自己
用的真实工具。

**验证 agentflow 哪些面**:
- 自定义 Rust 节点（`PodcastNode` 包 `phonon_podcast::PodcastPipeline`）
- LLM 节点做 blog → outline 提炼
- HTTP 节点拉 blog 原文
- File 节点写产物
- Trace 看每段 TTS 耗时 + LLM token
- 可选包成 skill（`podcast-producer`）让 `agentflow skill run` 触发

**外部依赖**:
- `phonon-podcast` 0.7（path dep 到 `/Users/hal/rustspace/phonon/phonon-podcast`）
- **Default 组合（零 OpenAI 依赖）**:
  - LLM: Moonshot via OpenAI-compat base URL (`MOONSHOT_API_KEY`)
  - TTS: MiniMax T2A v2 via phonon-ai 的 `MiniMaxTts`
    (`MINIMAX_API_KEY`，2026-05-18 在 phonon 70daa58 commit 落地)
- **Free-tier 备选**: Edge TTS（phonon-ai 已有，无 key）替代 MiniMax
- **Premium 备选**: ElevenLabs（高端英文）/ OpenAI TTS

**架构（薄壳方案 A）**:
```
HTTP fetch → LLM outline → PodcastNode (内部走完 phonon 全流程) → File write
```

**架构（拆开方案 B，dogfooding 后期）**:
```
HTTP fetch → LLM outline → PodcastScriptNode (phonon::OpenAiScriptGenerator)
  → PodcastTtsNode (并发 phonon TTS) → PodcastAssembleNode
  → SubtitleNode → File write
```

**TODO 子项**:
- [ ] 听 `/tmp/episode-test.wav` 主观评估：voice 区分度 / 对话自然度 /
      停顿合理性 / 是否需要 BGM 隔开
- [ ] 决定是否升级到 plan B（拆 DAG：fetch → outline → script_gen →
      tts → assemble → subtitle）。触发条件：dogfooding 中遇到
      「想中间编辑脚本再继续」「单段 TTS 失败想 retry」之类。
- [ ] 决定是否包成 skill（`SKILL.md` + persona + tool admission），
      让 `agentflow skill run podcast-producer` 直接触发
- [ ] 加 medium / long 两个 blog fixture（多场景测试）
- [ ] phonon-podcast 上游 PR：`OpenAiScriptGenerator::generate` 的
      `#[instrument]` 在 trace fields 里 dump 全文 `topic`，长 blog
      会把单行 trace 撑到几 KB。建议截断到 80-120 字符。

**DONE 子项**:
- [x] 写 `README.md`（架构图 + 跑法 + 所需 key）— commit 2f4d4b0
- [x] **Prereq: phonon-ai 加 `MiniMaxTts` provider** — phonon repo
  commit 70daa58（2026-05-18）。`MINIMAX_API_KEY` 走 phonon-ai 的
  TtsProvider trait，phonon-podcast `PodcastPipeline::new(MiniMaxTts::new()?)`
  即可用。16 个单测，含 hex 解码、business-error mapping、language_boost
  映射、9 种 emotion 白名单。Streaming SSE 留 follow-up。
- [x] **Plan A 薄壳实现** — standalone Cargo project at
  `examples/applications/blog-to-podcast/`：
  - `Cargo.toml`：empty `[workspace]` 跳出 agentflow workspace；path
    dep 到 agentflow-core + phonon-ai + phonon-podcast + phonon-io +
    phonon-core
  - `src/podcast_node.rs`：`PodcastNode` impl `AsyncNode`，内部串
    `OpenAiScriptGenerator`（Moonshot）+ `PodcastPipeline` +
    `phonon_io::write_audio` + `subtitle::write_srt_file`。
    新增 `estimate_subtitle_timing` helper（按字符数比例分配时长
    把 script segments 转成带时间的 SRT entries）。`TtsBackend` enum
    支持 MiniMax / Edge / OpenAi 切换。
  - `src/main.rs`：`tokio::main` + `tracing_subscriber` + 手写的
    `--blog --output --segments --tts` CLI 解析；`Flow::new` 构造
    2 节点 DAG（`read_blog → produce_podcast`），跑完打印 summary +
    输出路径
  - `fixtures/short_blog.md`：~500 字 zh-CN 关于 Rust 所有权和并发
  - `tests/smoke.rs`：3 个 hermetic CLI 测试（`--help` / 缺 `--blog` /
    未知 flag）+ 1 个 `#[ignore]` live end-to-end，gated 在
    `MOONSHOT_API_KEY` + (`MINIMAX_API_KEY` 或 `EDGE_TTS_OK=1`)
  - 8 unit tests in `podcast_node` 模块 + 3 hermetic integration
    tests + 1 ignored live test。`cargo check` / `cargo clippy
    --all-targets -- -D warnings` / `cargo fmt --check` 全绿。
  - 集成路径已验证：agentflow `Flow` orchestrate + 自定义 `AsyncNode`
    包外部 Rust 库（phonon-podcast）的 path-dep 跨 workspace 集成
    端到端编译 + 测试都通。

**Findings**:

- **2026-05-18 — 默认 LLM model 名称是猜的，不存在**。我默认填了
  `kimi-k2-0905-preview` 想用最新 preview model，第一次 live run 时
  Moonshot API 返回 `404 Not Found the model kimi-k2-0905-preview or
  Permission denied`。`curl https://api.moonshot.cn/v1/models` 显示
  我账号实际可用：`moonshot-v1-{8k,32k,128k}`, `moonshot-v1-auto`,
  `kimi-k2.5`, `kimi-k2.6`, 三个 vision-preview。改默认为
  `moonshot-v1-128k`（长上下文、命名稳定），加 `--model` CLI flag 让
  operator 自己挑。Lesson：**默认模型名要选 "永远不会下架" 的稳定
  名字，preview / dated 名字必须 operator 显式 opt-in**。
- **2026-05-18 — `target_segments` 是 hint 不是硬 cap**。我传
  `--segments 4`，Moonshot 自然展开成 12 段（按 blog 5 个章节自然
  分段）。这是 phonon `OpenAiScriptGenerator` 的 system prompt
  里写的 "Generate approximately N dialogue segments"。对长 blog 想
  限段数得 prompt 更严或加 post-process trim。**不是 bug**，但
  EXAMPLES_TODOs.md / README 应该改 `--segments` 描述为
  "approximate, not strict"。
- **2026-05-18 — `.env` 自动加载是 dogfooding 必需**。第一版只读
  process env vars，每次 `cargo run` 都得 `source ~/.agentflow/.env`，
  太烦。加了 `dotenvy::from_path("~/.agentflow/.env")` 在 main 开头
  silently no-op-when-missing。**这条经验值得提到所有 application
  examples 的 convention**：默认从 `~/.agentflow/.env` 加载，但允许
  process env vars 覆盖（dotenvy 默认行为）。
- **2026-05-18 — phonon trace 太冗长**。每次 `OpenAiScriptGenerator
  ::generate` / `MiniMaxTts::synthesize` 的 `#[instrument(fields(topic
  = %...))]` 都把整个 `topic`（即整篇 blog）dump 进 trace 一行。
  Terminal 输出基本不可读，需要 grep 才能看 trace 流程。phonon 侧的
  PR：截断长 instrument field（>= 80 字符就 `... (N chars)`）。
- **2026-05-18 — agentflow `ConsoleListener` events 干净好用**。
  `[workflow.started] / [node.started] / [node.output.captured] /
  [node.completed] / [workflow.completed]` 自动打，per-node 耗时
  立即可见（read_blog: 189µs vs produce_podcast: 18.78s）。这是
  agentflow 的明显 win。
- **2026-05-18 — `FlowValue::File { mime_type, .. }` 字段叫
  `mime_type` 不是 `media_type`**。第一版我猜错，编译报错才发现。
  Lesson: agentflow public types 的 field naming 可以更早在 SDK 文档
  里固化（已有 `docs/AGENT_SDK.md`，可加一节 "FlowValue field
  reference"）。
- **2026-05-18 — `ConsoleListener` 是 unit struct 没 `default()`**。
  小坑；trivial fix（直接 `ConsoleListener` 实例化）。不算缺陷。
- **2026-05-18 — 性能数据**：12 段 zh-CN 对话（2.5 分钟音频）端到端
  ~19s wall clock（read_blog 0.2ms + Moonshot script gen 几秒 +
  12 次 MiniMax T2A 并发 + 拼接 + 写 WAV），13MB WAV 文件。
  MiniMax 每段 ~150-300ms 响应（含 hex 解码 + Symphonia decode）。
  成本：估 MiniMax ~5000 字符 × 高清档定价 + Moonshot ~2000 token，
  整条 ~¥1 / $0.15。
- **2026-05-18 — guest voice 选错（HK 口音）**。我默认配的
  `Chinese (Mandarin)_HK_Flight_Attendant` 名字里 "HK" 是港式国语
  口音，听上去跟纯普通话 host 不协调（用户反馈"一个普通话一个方言"
  的真因）。这不是 phonon 或 agentflow 的问题 —— 是 default config
  的 voice_id 挑错。1-line fix：换成 MiniMax 另一个纯 Mandarin
  `Chinese (Mandarin)_*` voice。Lesson：**default voice 选择前
  应该试听 MiniMax 提供的 sample audio**（MiniMax console 里有
  voice library + 试听）。
- **2026-05-18 — `segments_to_srt_file` API 实际签名不带 duration**。
  phonon 的 helper 期望 STT TranscriptSegment（带时间），不是 script
  Segment（只有 speaker + text）。自写了 `estimate_subtitle_timing`
  按字符数比例分配时长。**真改进方向**：phonon `PodcastPipeline` 应
  在 render 过程中记录每段实际时长，输出 `Vec<(Segment, f64 duration)>`
  让 SRT 用真实时长。已在 TODOs.md 末尾标了 phonon-podcast 上游 PR
  候选。

---

## A1.5 — podcast-mastering

**业务**: 输入一个录好的播客 `.wav`（典型场景是 A1 的输出），输出
mastered 版本：LUFS 归一化到目标响度、淡入淡出、上传平台可用。

**为什么是 A1.5 而非 A2**: 它是 A1 的 sibling，验证 **L3 集成路径**
（phonon-mcp via stdio JSON-RPC），跟 A1 用同一个 `/tmp/episode-test.wav`
源音频但跨完全不同的代码路径。两者放一起对比能直接看出 L1 vs L3
的工程取舍。

**为什么有价值**: 这是 3-tier 架构里 L3 的最小可验证用例，证明 agent
可以靠 ReAct + native tool calling 串起 6 个独立 MCP 工具完成端到端
工作流，零项目特定 Rust 代码。

**验证 agentflow 哪些面**:
- `[[mcp_servers]]` 启动 native binary 子进程
- `security.mcp_command_allowlist` 安全门把关
- `McpClientPool` + `McpToolAdapter` 自动暴露 14 个 `mcp_phonon_*` tool
- ReAct + Moonshot native tool calling 跨 6 步 linear workflow
- UUID handle 在多次 tool call 之间正确传递（AssetRegistry pattern）

**外部依赖**:
- `phonon-mcp` binary（path: `/Users/hal/.target/release/phonon-mcp`）—
  `cd /Users/hal/rustspace/phonon && cargo build --release -p phonon-mcp`
- Moonshot key（已有，via `~/.agentflow/.env`）
- agentflow CLI release build

**TODO 子项**:
- [ ] 听 `/tmp/episode-mastered.wav` 主观评估 master 质量
- [ ] 验证 `mcp_phonon_audio_loudness` 在 master 后输出的 LUFS
      跟 agent 报的 -16 对得上（agent 没有 re-measure，只是 trust
      tool 的 target_lufs 参数）
- [ ] 考虑加 trim silence 步骤（很多播客头尾有静音）
- [ ] 考虑包成更通用的 audio-mastering skill（接 mp3 / flac）
- [ ] 拼装一个 A1 + A1.5 复合 skill / workflow：blog → podcast →
      mastered，端到端一条命令

**DONE 子项**:
- [x] phonon-mcp binary build OK（`cargo build --release -p phonon-mcp`）
- [x] skill validate 通过，discovers 14 MCP tools
- [x] live end-to-end skill run 通过：7 步 ReAct loop（6 tool calls
      + final answer）；agent 严格按 persona 顺序调工具；handle 链
      正确传递；输出 `/tmp/episode-mastered.wav` 13MB 147.99s
      LUFS-normalized + faded
- [x] 总耗时 ~37s（含 LLM 决策 + 6 次跨进程 MCP 调用）
- [x] 沉 Finding 到本文件

**Findings**:

- **2026-05-18 — SKILL.md 不支持 `model:` 字段**。SKILL.md 的
  frontmatter `model` 始终被 `Default::default()` 覆盖（默认 `gpt-4o`）。
  Bug or feature？SKILL.md 跨工具 portable 不带 agentflow 特定字段
  是合理 design，但 doc / error message 没说明白；用户配了 model
  但被静默忽略很迷惑。**改进方向**：要么 SKILL.md 加 model 支持，
  要么 validate 时 warn "ignoring frontmatter.model in SKILL.md;
  use skill.toml for model config"。
- **2026-05-18 — `agentflow skill validate` 错误信息不够具体**。
  报 `Error: Validation failed` 没说哪一条 validation 失败。
  实际是 `[[mcp_servers]] command '/.../phonon-mcp' executable
  name 'phonon-mcp' is not in security.mcp_command_allowlist`，
  但要 grep loader.rs 才知道。**改进方向**：validate command
  应该把 underlying `SkillError::ValidationError.message` 直接打
  出来，而不是吞掉。可能 anyhow `with_context` 链丢了底层 message。
- **2026-05-18 — Default `mcp_command_allowlist` 设计合理但要文档化**。
  默认只有 `["python", "python3", "node", "npx", "uvx"]` —— 解释器
  脚本可以跑，compiled Rust binary 默认拒绝。这是好的安全 posture
  （强迫 operator 显式审批每个 native binary），但 docs/AGENT_SDK.md
  或 SKILL_FORMAT.md 里要明写「想跑 compiled binary MCP 一定要加
  `security.mcp_command_allowlist`」。
- **2026-05-18 — `mcp_phonon_*` 命名 convention 干净**。agentflow
  自动用 `mcp_<server_name>_<tool_name>` 命名，14 个 phonon tool
  全自动暴露 `mcp_phonon_audio_load` 等，agent 看到的名字一致、
  好预测。
- **2026-05-18 — Moonshot tool calling first-shot 工作正常**。`moonshot-v1-128k`
  对 native tool calling 支持稳定，按 persona 写的步骤严格走 6 步，
  不乱用 tool、不跳步、handle 串接正确。这是 agentflow ↔ Moonshot
  集成的额外验证点。
- **2026-05-18 — phonon-mcp `AssetRegistry` 在 multi-tool-call
  pattern 下完全正常**。每个 `normalize_lufs` / `fade` 返回新 UUID
  handle，agent 自动用新 handle 喂下一步，最终 save 用最新 handle，
  没有用错老 handle 或 leak。
- **2026-05-18 — phonon-mcp 内部 sample rate resample 没说明**。
  源 wav 是 32kHz，audio_load 后 audio_info 报 44100Hz。phonon-mcp
  内部某处做了 resample（可能 audio_load 默认转 44.1k？也可能 LUFS
  pipeline 内部）。不影响功能但行为不可见。**改进方向**：phonon-mcp
  的 audio_load tool description 或 audio_info 输出加一个
  `resampled_from` 字段说清楚是不是被改采样率了。
- **2026-05-18 — agent 没有 verify mastering 后的实际 LUFS**。
  persona 让 agent 在 save 后汇报，但 agent 只 trust normalize_lufs
  的 target 参数，没真的 re-call audio_loudness 量一下结果。这是
  persona prompt 的弱点（没强制要求 post-measure 步骤）。**改进方向**：
  persona 加 "Step 5.5: re-measure with audio_loudness before save,
  report actual achieved LUFS"。
- **2026-05-18 — skill run 总耗时 37s 主要花在 LLM 思考上**。tool
  calls 本身全都 sub-second（最慢的 audio_save 也只 50ms）；agent
  在每步 tool_call 之后要等 LLM 决定下一步，单次决策 2-15s。这是
  L3 跨进程 + LLM-in-loop 架构的固有特性，不是 phonon-mcp 慢。
  对比 A1 用 L1 同进程直接调，PodcastPipeline 走完 12 段 TTS 才 19s。
- **2026-05-18 — `--trace` 输出 `RuntimeTrace` JSON 极其清晰**。
  每个 `plan` / `tool_call` / `tool_result` / `final_answer` 都
  带 timestamp + index，是 dogfooding / debug 利器。这是 agentflow
  的明显 win。
- **2026-05-18 — L1 vs L3 的真实工程取舍数据点**：A1 (L1) 19s 出
  2.5 分钟播客；A1.5 (L3) 37s 对同一文件 mastering。L3 慢的是
  「LLM 决策开销」，不是 IPC 序列化。意味着：**如果工作流形态
  固定，用 L1；如果需要 agent 在中间做决策 / 工具组合，用 L3**。

---

## A2 — code-reviewer

**业务**: 输入 PR URL（GitHub）或本地 diff 文件，输出结构化评审评论（按文件
分组、按严重度排序），可选直接推到 GitHub。

**为什么有价值**: 验证 agentflow 的 ReAct agent + MCP（GitHub server）组合是否
真能搭出可用 reviewer；skill 包装让团队能 `agentflow skill install
code-reviewer` 后立即用。

**验证 agentflow 哪些面**:
- `ReActAgent` 主循环（读 diff → 思考 → 调 tool → 回答）
- MCP 集成（GitHub MCP server 提供 `get_pr_diff` / `add_comment`）
- Skill 包装 + persona + 工具白名单（admission）
- 工具沙箱（shell node 调 git）

**外部依赖**:
- `gh` CLI 或 GitHub MCP server（`@modelcontextprotocol/server-github`）
- `GITHUB_TOKEN`
- LLM API key（建议 Anthropic / OpenAI 强模型，评审质量敏感）

**TODO 子项**:
- [ ] 写 `README.md`
- [ ] 设计 persona（评审风格、关注点）
- [ ] 选 MCP server vs shell 调 gh CLI
- [ ] 写 `skill.toml` + persona prompt
- [ ] 准备 3 个真实 PR fixture
- [ ] 测「敏感建议先走人工审批」（Harness Mode approval flow）

**DONE 子项**: （待填）

**Findings**: （待填）

---

## A3 — research-assistant

**业务**: 配置感兴趣的 arxiv 主题（如 "LLM agents", "Rust async"）+ 关键词，
agent 周期性抓新论文、读摘要、与已索引文献做对比、产出周报 markdown。

**为什么有价值**: 验证 agentflow 的 Arxiv node + RAG + memory（"已读过哪些论文"
持久化）+ scheduled run（CronCreate / `/schedule`）的组合。

**验证 agentflow 哪些面**:
- `arxiv` node 抓论文 metadata + 摘要
- RAG ingest 持久索引 + retrieve 做新旧对比
- `MemoryStore`（`SqlitePreferenceStore` 存「关注的主题列表」+
  `SqliteEntityFactStore` 存「这篇我读过」）
- 调度（手动 cron 或 agentflow 自己的 schedule 机制）
- LLM 写周报

**外部依赖**:
- Arxiv API（免费，无 key）
- LLM provider
- 可选 Qdrant（如果用 vector RAG）；不用也能跑 BM25

**TODO 子项**:
- [ ] 写 `README.md`
- [ ] 设计 memory schema（关注主题 + 已读论文）
- [ ] 写 `workflow.yml` 或 skill
- [ ] 跑一周看产出质量

**DONE 子项**: （待填）

**Findings**: （待填）

---

## A4 — meeting-transcriber

**业务**: 输入会议录音（.wav / .mp3 / .m4a），输出转录 markdown +
按 speaker 切分 + 行动项列表（"X 负责 Y，DDL Z"）+ 可选会议纪要。

**为什么有价值**: 验证 agentflow 的 ASR node（StepFun 或 OpenAI Whisper）+
LLM 后处理的组合；典型「音频 → 结构化输出」流程。

**验证 agentflow 哪些面**:
- `asr` node（现有）
- LLM 多轮做摘要 + action item 提取
- 长输入分块（如果会议超过 ASR 上下文限制）
- File 输出多文件（transcript + summary + action items）

**外部依赖**:
- ASR provider（StepFun ASR / OpenAI Whisper API / 本地 Whisper）
- LLM provider

**TODO 子项**:
- [ ] 写 `README.md`
- [ ] 决定 ASR provider（成本 / 质量 / 隐私三角）
- [ ] 准备会议录音 fixture（自己录一段或用公开样本）
- [ ] 写 `workflow.yml`

**DONE 子项**: （待填）

**Findings**: （待填）

---

## A5 — weekly-digest

**业务**: 每周固定时间，从 RAG 索引（前面 A3 / 自己博客 / 收藏夹）查最近一周
新增内容，LLM 生成 digest，通过 SMTP/SendGrid 发邮件给指定收件人。

**为什么有价值**: 验证 agentflow 的 scheduled run + RAG + LLM + HTTP（往外
发请求）完整业务回路；测「无人值守」可靠性。

**验证 agentflow 哪些面**:
- 长期 scheduled run（不是 dev 期间手动触发）
- RAG `search` query
- LLM 写长文
- HTTP node 调邮件 API
- 失败重试 / 死信处理（如果某周邮件没发出去怎么办）

**外部依赖**:
- SendGrid / Mailgun / SMTP server
- 持久 RAG 索引（A3 的产物可以复用）

**TODO 子项**:
- [ ] 写 `README.md`
- [ ] 选邮件 provider
- [ ] 设计失败兜底（重试 + 上次成功时间戳）
- [ ] 跑 4 周观察

**DONE 子项**: （待填）

**Findings**: （待填）

---

## A6 — doc-translator

**业务**: 输入一个 markdown 文件夹（如 `docs/`），目标语言列表（如 `["en",
"ja", "zh"]`），输出按语言分目录的翻译版本，保留 markdown 结构 + code fence
不翻译。

**为什么有价值**: 验证 agentflow 的 `batch` / `map`（并行）+ template +
LLM 大量调用 + file batch write；典型「输入扇出、输出扇入」场景。

**验证 agentflow 哪些面**:
- `map` 节点并行（每个文件 × 每个语言）
- Template 节点做 system prompt 渲染
- 并发上限 / rate limit 应对
- 失败单个文件不阻塞整体（partial failure tolerance）
- Checkpoint 恢复（中途挂了重启不重译）

**外部依赖**:
- LLM provider（Anthropic 中长文翻译质量较好）

**TODO 子项**:
- [ ] 写 `README.md`
- [ ] 设计 prompt（保留 code fence / 链接 / 标题层级）
- [ ] 写 `workflow.yml` 用 `map` parallel
- [ ] 验证 checkpoint 中途重启
- [ ] 测 100+ 文件 fanout 时的稳定性

**DONE 子项**: （待填）

**Findings**: （待填）

---

## A7 — changelog-writer

**业务**: 输入 git tag 范围（如 `v1.0.0..HEAD`），shell 节点跑 `git log`
拿提交，LLM 按 conventional-commits 分类（feat/fix/docs/chore），生成
markdown changelog 段。

**为什么有价值**: **agentflow 给自己用** —— 每次 release 时跑一遍，验证基础
工具链；纯本地无外部依赖，最适合频繁 dogfood。

**验证 agentflow 哪些面**:
- `shell` node 跑 `git log --pretty=...`（验证 shell admission + sandbox）
- LLM 分类 + 改写
- File 节点写 / append CHANGELOG.md
- 测「OS sandbox 把 shell 限制在 git 命令内」是否真有效

**外部依赖**:
- 无（git 在 PATH 上即可）；LLM 用 mock provider 也能 dry-run

**TODO 子项**:
- [ ] 写 `README.md`
- [ ] 设计 prompt（conventional commits 分类规则）
- [ ] 写 `workflow.yml`
- [ ] 跑一次给 agentflow 自己生成 CHANGELOG
- [ ] 把它包成 `agentflow changelog` CLI 子命令？（可能升级成 P3.x）

**DONE 子项**: （待填）

**Findings**: （待填）

---

## Cross-References

- `TODOs.md` — agentflow 主任务队列；从 dogfooding 涌出的缺陷回填到这里
- `examples/README.md` — SDK feature 矩阵（性质不同，互补）
- `examples/ecosystem/` — 生态形态样本（性质不同，互补）
- `docs/RELEASE_NOTES_v1.0.0-rc.1.md` — release notes draft，dogfooding 阶段
  保持 DRAFT 不动
