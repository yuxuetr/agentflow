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
| A1 | [blog-to-podcast](examples/applications/blog-to-podcast/) | TODO | custom Rust node, LLM, HTTP, file, trace, skill | phonon-podcast (path dep), Moonshot LLM + MiniMax TTS (default) / Edge TTS (free) |
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
- [ ] 决定方案 A 还是 B 开始（建议先 A）
- [ ] 新建自定义节点（`src/podcast_node.rs` 或独立 `agentflow-podcast` crate）
- [ ] 写 `workflow.yml`
- [ ] 准备 3 篇真实 blog fixture（短/中/长各一）
- [ ] 跑通端到端，听一遍出来的音频
- [ ] 写 smoke 测试（self-skip if no API key）
- [ ] 决定是否包成 skill

**DONE 子项**:
- [x] 写 `README.md`（架构图 + 跑法 + 所需 key）— commit 2f4d4b0
- [x] **Prereq: phonon-ai 加 `MiniMaxTts` provider** — phonon repo
  commit 70daa58（2026-05-18）。`MINIMAX_API_KEY` 走 phonon-ai 的
  TtsProvider trait，phonon-podcast `PodcastPipeline::new(MiniMaxTts::new()?)`
  即可用。16 个单测，含 hex 解码、business-error mapping、language_boost
  映射、9 种 emotion 白名单。Streaming SSE 留 follow-up。

**Findings**: （dogfooding 中发现的 agentflow 缺陷写这里）

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
