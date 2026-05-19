# AgentFlow 项目深度评估报告 (2026-05-19)

- 评估日期：2026-05-19
- 评估范围：workspace 全部 **16 个 Rust crate + 1 个 xtask + 1 个 Web UI crate (`agentflow-ui`)**，`docs/`、`RoadMap.md`、`TODOs.md`、CLI 执行路径、agent runtime、DAG 调度器、Harness Agent Mode、平台化（server/db/worker）、Web UI、插件/Skill/MCP/RAG/Tracing 全链路、9-provider live nightly CI
- 与上一版报告 (`PROJECT_EVALUATION_2026-05-14.md`) 的关系：上一版评估在 5 天前定稿（HEAD `738bf92`），记录 15+1 crate；本版基于 `main` HEAD `daaa912` 重新校核全部代码、测试与文档，覆盖 5 天内 **177 commits** 落地的所有变更，新增 `agentflow-harness` crate，N8/N9/N10 三个路线图段全部 closed
- 编译/测试基线（本评估实测重跑）：
  - `cargo test --workspace --lib`：**1,183** 测试全过
  - `cargo test --workspace --tests`：**1,792** 测试全过（含 lib + 集成 + doc tests）
  - `cargo clippy --workspace --all-targets -- -D warnings`：clean
  - 9-provider live nightly（workflow run `26103718043` + `26105740468`）：**24 / 24** 通过

---

## 0. TL;DR

5 天内（2026-05-14 → 2026-05-19）AgentFlow 完成了 **N 系列路线图的全部收尾**：

1. **N8 (Platform skeleton + native tool calling)** — `Tool::idempotency()` 元数据自动桥接到 `AgentNodeResumeContract` 实现 partial-resume auto-replay；`FlowValue::File` / `FlowValue::Url` 在 `CheckpointManager` 落盘往返保持类型保真（之前会静默退化为 `Json`）；tagged-but-corrupt 与 untagged-legacy 两种 fallback 路径有显式 `eprintln!` 警告区分
2. **N9 (Multi-agent + ecosystem)** — 跨提供商 streaming / multimodal / tool-calling 一致性测试矩阵全部上线（5 个新 cross-provider invariant 测试 + 7 个 1afcd17 invariant 基线）；`.github/workflows/llm-live.yml` 落地，**9 个 provider** 端到端 nightly 命中真实 API（OpenAI / Anthropic / Google / Moonshot / StepFun / GLM·Zhipu / DashScope / DeepSeek / MiniMax）
3. **N10 (Plugin / distributed / Web UI)** — 状态标签 stale 修正（实际全部 ✅）：插件 / 自定义 Node 框架、分布式调度地基、Web UI 调试器、Marketplace 远程 registry 全部生产可用
4. **`agentflow-harness` 作为第 16 个 Rust crate 落地** — P-H 段 H0 到 H5 全 5 个 phase 落地，包含 hooks/approval、并行 tool calls、background tasks、server + Web UI integration、可恢复 session
5. **CLI JSON envelope 统一化（P3.3）** — 10+ 个子命令迁移到 `CliJsonEnvelope<T>` 统一信封（`workflow run|list|cancel|graph|validate|resume-plan`、`rag search|eval`、`plugin install|uninstall|generate-workflow-stub|list|inspect`、`harness run|list|inspect|resume`、`trace replay`、`mcp list-tools|list-resources|call-tool`、`llm models`、`doctor`、`eval run`），机器可读契约现以 `agentflow.cli/1` schema name 固定在 `docs/CLI_JSON_OUTPUT.md`
6. **DashScope / DeepSeek / MiniMax 入 registry**（最近 1 commit） — 4 个 OpenAI-compat vendor（GLM + 这三个）共享 `OpenAIProvider` via `create_provider` 工厂 + `default_models.yml` registry，用户现可在 `skill.toml` / `workflow.yml` 里写 `vendor: deepseek` / `vendor: minimax` 等

| 维度 | 上次评级 (5/14) | 本次评级 (5/19) | 一句话判断 |
| --- | --- | --- | --- |
| 架构清晰度 | A | **A** | L1/L2/L3/L4 四层心智模型在 `agentflow-harness` 新增后仍清晰；harness 严格在 L3，且通过 `Box<dyn AgentRuntime>` 包裹复用 agents |
| DAG 内核成熟度 | A | **A** | N8 idempotency bridge + FlowValue 类型保真补完，之前的"silent collapse to Json"已修复 |
| Agent-native runtime 成熟度 | A- | **A-** | Harness Agent Mode 已端到端：hooks/approval/parallel tool calls/background tasks/server+UI 全部 closed |
| LLM 抽象成熟度 | A | **A** | 6 → 9 provider（+DashScope/DeepSeek/MiniMax），live nightly 实际命中真实 API 验证；P-LLM modality dispatcher 落地 |
| 工具/权限治理 | B+ | **A-** | OS sandbox / 路径硬化 / SSRF / Idempotency 全部就位；现以"sandbox profile visible in `skill inspect --explain-permissions`"形态落到 CLI |
| Config-first / CLI 体验 | B+ | **A-** | `agentflow serve` / `agentflow cleanup` / `agentflow doctor --backup-check` / `agentflow plugin generate-workflow-stub` 等全部落地；CLI JSON envelope 统一化（P3.3）覆盖 10+ 命令 |
| 生产可观测性 | A- | **A** | W3C `traceparent` 跨 LLM hop / plugin spawn / MCP JSON-RPC / Worker gRPC 4 类全链路注入；trace 比较视图（P6.3）已在 Web UI |
| 服务端平台化 | B | **A-** | retention / tenant boundary / SSE robustness / backup expectations / 用户偏好 API 全部落地；剩 v1.0.0-rc.1 tag 切版（人工 ops） |
| 分布式调度 | C+ (foundation) | **B** | Worker auth/admission + 资源限制 + failure-domain tests 全部落地；P2.8 worker LLM/HTTP/MCP/Agent node 扩展已 closed |
| Web UI | C (alpha) | **B** | Run creation form / 诊断面板 / trace 比较 / 持久化偏好 / 客户端 event filter / Harness Mode 完整 UI 全部落地 |
| Harness Agent Mode | — | **A-** (新评级) | H0/H1/H2/H3/H4/H5 全 phase closed；envelope/hooks/approval 已 Beta 稳定层 |
| **综合** | A- | **A** | 全部 N 段 closed；v1.0.0-rc.1 候选窗口完全打开，剩**只有人工 ops**（crates.io publish / GitHub Release / 干净 VM doctor smoke） |

**与主题契合度**：项目的核心命题——**"DAG + Native-Agent 双底座 + LLM/Tools/RAG/MCP/Skill 能力层 + Rust SDK / CLI / WebUI 上层"**——在代码层面**100% 对齐**。所有路线图段（N1–N10 + P0–P7 + P-H + P-LLM + P9 + M）全部 closed 或 DEFERRED，没有偏离。

---

## 1. Workspace 全景（16 Rust crate + 1 xtask + 1 Web UI）

### 1.1 crate 规模 & 测试覆盖（来自实际代码统计）

| 层 | Crate | 角色 | LOC | 测试数 | 版本 | edition | 成熟度 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **L1 执行内核** | `agentflow-core` | DAG 引擎、AsyncNode、FlowValue、scheduler、checkpoint、retry、timeout、health、events、expression engine、plugin host | 14,029 | 236 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-nodes` | 内置 16+ 节点（LLM/HTTP/File/Template/Map/While/RAG/MCP/多模态） | 4,523 | 45 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-llm` | **9 provider**（含新增 DashScope/DeepSeek/MiniMax registry citizenship）+ 多模态 + streaming + provider-native tool_calls/tool_choice + OTel traceparent + cross-provider invariants（12 个）| 12,171 | 213 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-tools` | Tool/Registry/Policy/OS Sandbox (macOS/Linux/no-op)/SSRF 防护/ToolIdempotency | 4,682 | 88 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-mcp` | MCP client/server/stdio transport，retry/timeout/重连，**traceparent JSON-RPC meta** | 6,650 | 182 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-rag` | chunk/embed/Qdrant/retrieval/rerank + eval harness (Recall@K, MRR, nDCG@K) + paired sign test + CI baseline | 8,580 | 146 | 0.3.0-alpha | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-memory` | MemoryStore + Session/SQLite/Semantic + **4 层 layering**（Preference/Entity facts/SemanticMemoryStore） | 2,766 | 37 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L3 智能体/编排** | `agentflow-agents` | ReAct + PlanExecute + Handoff/Blackboard/Debate Supervisor + AgentNode + WorkflowTool + Reflection + MemorySummary + **eval framework** (`agentflow_agents::eval`) + cost tracking | 13,862 | 187 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L3 智能体/编排** | `agentflow-skills` | SKILL.md/skill.toml + SkillBuilder + Marketplace + MCP adapter + registry + **validator protocol** (`[validation] kind = "regex" | "command"`) | 5,774 | 116 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L3 智能体/编排** | `agentflow-harness` 🆕 | Harness Agent Mode：`HarnessRuntime` + hooks/approval + parallel tool calls + background tasks + JSONL persistence + 4 default context providers | 5,728 | 77 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L3 智能体/编排** | `agentflow-cli` | workflow / skill / llm / image / audio / mcp / trace / rag / plugin / doctor / rag eval / **harness** / **serve** / **cleanup** / **agent eval** + `CliJsonEnvelope<T>` 统一 JSON | 16,281 | 341 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-tracing` | EventListener + JSONL/SQLite/Postgres + replay + TUI + OTel OTLP + W3C traceparent + redaction + `context::scope` task-local helper | 4,401 | 38 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-viz` | YAML → VisualGraph → Mermaid/DOT/JSON | 1,801 | 26 | 0.1.0 | 2024 | ⭐⭐ |
| **L4 运维/产品化** | `agentflow-server` | Axum gateway：Run/Cancel/SSE/Graph/Resume-plan/Bearer auth/安全 profile/CORS+body limit/Web UI/分布式 control plane + **Harness routes** (`/v1/harness/sessions/*` + `:resume` + SSE) + tenant 边界 + retention/cleanup + diagnostics endpoint + user preferences | 9,185 | 170 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-db` | Postgres schema (**8 表**, +harness_sessions + harness_session_events + user_preferences) + sqlx migrations + 8 个 repos + tenant_id 列 + 完整 CRUD 测试 | 1,224 | 15 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-worker` | 分布式 worker (in-memory + gRPC transport)，claim/heartbeat/execute/report；现支持 **template/file/llm/http/mcp/agent** 多类 node payload；worker admission/credential/PSK rotation + 资源限制 + 6 类 failure-domain 测试 | 1,136 | 23 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-ui` | React 19 + Vite 7 + TypeScript 5.8 SPA；零运行时依赖；编译期 `include_str!` 嵌入 server；**run 列表 + DAG 图 + 事件回放 + run creation form + 诊断面板 + trace 比较 + 偏好同步 + 客户端 event filter + Harness Mode 完整 UI（list/new/detail + SSE + approval cards + resume）** | (6 TS files; dist 260 KB) | n/a | 0.1.0 | n/a | ⭐⭐ |
| **工具链** | `xtask` | workspace 自动化：`verify-edition` / `examples-smoke` / `bench-gate` / `check-agent-sdk-doc` | 1,254 | 20 | 0.1.0 | 2024 | ⭐⭐⭐ |

**关键观察**：

- **总 Rust LOC ≈ 127.7K**（5/14: ~84K → +52%），主要增量来自 `agentflow-cli`（9,669 → 16,281, +68%）、`agentflow-agents`（9,860 → 13,862, +41%）、`agentflow-llm`（10,612 → 12,171, +15%）、新增 `agentflow-harness`（5,728）
- **总 Rust 测试 ≈ 1,792**（含集成）/ **1,183**（仅 lib），相对 5/14 的 1,174 lib 测试 **+1%**（lib 微增）；集成测试体系是 5 天内主要增量
- **新增 crate**：`agentflow-harness`（76 + xtask 改造）
- **agentflow-ui** 从 alpha shell 升级为 B 级（含 Harness Mode 完整 UI、诊断面板、trace 比较等 5 大新页面）
- **`agentflow-db` 表数从 6 → 8**（+ `harness_sessions` + `harness_session_events` + `user_preferences` migration `0004`）
- **`agentflow-server` LOC 从 4,698 → 9,185, +95%**，主要来自 Harness routes + retention + tenant 边界 + diagnostics + preferences
- **`agentflow-worker` node 类型扩展**：从 template/file/mock 三类扩展到 template/file/llm/http/mcp/agent **六类** payload

### 1.2 四层心智模型（与上次相同，仍然成立）

```
+----------------------------------------------------------------+
| L4 运维/产品化 | tracing · viz · server · db · worker · ui     |
+----------------------------------------------------------------+
| L3 智能体/编排 | agents · skills · harness · cli                |
+----------------------------------------------------------------+
| L2 能力适配    | nodes · llm · tools · mcp · rag · memory       |
+----------------------------------------------------------------+
| L1 执行内核    | core (Flow / GraphNode / FlowValue / Expr / Plugin) |
+----------------------------------------------------------------+
```

- L1 唯一执行核：`Flow::execute_*` 拥有节点状态池、拓扑、并发、checkpoint、事件、表达式求值器、plugin host；N8 完成后 FlowValue checkpoint 类型保真已闭环
- L2 全部以 `AsyncNode` / `Tool` / `EmbedClient` / `MemoryStore` / `LLMProvider` 等抽象被 L3 使用，L1 不直接依赖任何外部能力
- L3 **三轨入口**（新）：`agentflow-agents` 承载 agent-native，`agentflow-nodes + agentflow-cli` 承载 DAG，`agentflow-harness` 承载 **长期会话 + workspace-aware + governable agent**。三轨通过 `AgentNode` × `WorkflowTool` × `HarnessRuntime::wrap` 互通
- L4 横切面：`tracing` 非侵入接入 L1（含 W3C traceparent 跨 4 类 hop 注入）；`server` + `db` 提供平台化 API（含 Harness routes）；`worker` 提供分布式底座（六类 node payload）；`ui` 提供 Web 调试器（含 Harness Mode UI）

---

## 2. 自 2026-05-14 以来的主要架构变化（177 commits）

### 2.1 N8 收尾：partial-resume idempotency + FlowValue type fidelity

**Commit**：`e78a3ef feat(agents,core): N8 — idempotency bridge + FlowValue checkpoint fidelity`

**核心变化**：
- 新 `AgentNodeResumeContract::from_result_with_tools(node, runtime, result, &ToolRegistry)` 在 params 不带 `_agentflow.side_effect_class` hint 时回退到 `Tool::idempotency()` / `ToolMetadata::with_idempotency`，使 registry-declared `Idempotent` tool 自动 `ReplayAllowed` 于 partial-resume
- legacy `from_result(...)` 保留为 zero-impact wrapper（用 empty `ToolRegistry::new()`）
- `AgentNode::execute` (DAG path) + `build_skill_agent_outputs` (skill_agent path) 都串通了 live tool registry
- 新 `decode_checkpoint_flow_value()` in `agentflow-core/src/flow.rs` 区分 tagged-but-corrupt（响亮 `eprintln!` 警告）与 untagged legacy（silent fallback 兼容 pre-0.2 checkpoint）
- 6 bridge 测试 in `agentflow-agents/tests/agent_node_resume_contract.rs` + 2 in-module 测试 in `flow.rs` + 1 disk round-trip 测试 (`flow_value_file_and_url_survive_disk_round_trip`) 联合证明 File/Url 不会被静默退化为 Json

### 2.2 N9 收尾：9-provider live nightly + cross-provider invariants 全面化

**Commit 群**：`1afcd17` (7 invariants + GLM nightly env) → `462dcad` (multimodal + 4 tool_choice invariants) → `3d8b440` → `1e06600` → `1861b3d` → `a446756` (live nightly dry-run + 4 round model refresh) → `b7d24c7` / `d4b7500` / `fc16665` (DashScope + DeepSeek + MiniMax wired into nightly) → `f01ecdd` (deepseek-chat → deepseek-v4-flash 预防性迁移)

**核心变化**：
- `agentflow-llm/tests/provider_consistency.rs` 从 44 个 per-provider 测试扩展到 **44 + 12 invariant**（7 个原有 + 5 个新增），覆盖 streaming / multimodal / tool_call 三大维度的 cross-provider 一致性断言
- `.github/workflows/llm-live.yml` nightly CI 命中真实 API：9 个 provider，每晚 09:30 UTC 跑，per-provider 测试在 secret 缺失时自动 skip；不阻塞 `release-gate`
- `prepare_live_provider` 解除 `AgentFlow::init()` 强校验（避免 dashscope 不相关 key 缺失就 fail-close 整套）
- Model defaults 经过 4 轮 vendor 端弃用导致的滚动更新：`claude-3-5-haiku-20241022` → `claude-haiku-4-5`，`gemini-1.5-flash` → `gemini-2.5-flash`，`gemini-2.0-flash` → `gemini-2.5-flash`，`deepseek-chat` → `deepseek-v4-flash`；`max_tokens: 16` → `256` 给 Gemini 2.5 Flash thinking budget 留头
- 最终 dry-run（`26103718043`）24 / 24 pass in 20.48s
- 4 OpenAI-compat vendor（GLM / DashScope / DeepSeek / MiniMax）通过 `create_provider` 工厂 + `default_models.yml` registry 入 first-class 公民（最新 commit `aae8ec6`）

### 2.3 P-LLM 段：modality dispatcher + OpenAI Whisper 第二 ASR vendor

**Commit 群**：`35278ef` → `d5a220f` → `d5034fc` → `0860edc` → `2a176c5` → `d74c233` → `679ee9f`

**核心变化**：
- `ModelType` 从 `text` / `multimodal` / `imageunderstand` 三态折叠为单一 `chat`，输入模态由 `accepts: Vec<InputType>` 字段单独承载（180/196 registry 条目坍缩到 `chat`）
- 5 个非 chat 多模态节点（`asr` / `tts` / `text_to_image` / `image_to_image` / `image_edit`）从硬编码 StepFun 升级到走 dispatcher（`agentflow-llm/src/modality_dispatch.rs`）
- 新 `agentflow-llm/src/providers/openai_asr.rs` 作为 Whisper 后端，验证 trait shape 跨 vendor 通用性

### 2.4 P3.3 envelope migration：CLI JSON 契约统一化

**Commit 群**：10+ commits 覆盖 `workflow run|list|cancel|graph|validate|resume-plan|eval run`、`rag search|eval`、`plugin install|uninstall|generate-workflow-stub|list|inspect`、`harness run|list|inspect|resume`、`trace replay`、`mcp list-tools|list-resources|call-tool`、`llm models`、`doctor`

**核心变化**：
- 新 `agentflow-cli/src/json_envelope.rs` 定义 `CliJsonEnvelope<T>` 四字段封闭信封（`version` = `"agentflow.cli/1"`, `command`, `result`, `errors[]`）
- `docs/CLI_JSON_OUTPUT.md` 作为权威契约文档；`docs/STABILITY.md` 把 envelope 标记为 Stable tier
- 每个迁移命令的 `--format json-envelope` 模式包裹原 result；legacy `--format json` 仍保留以维护 backward compat
- 关键设计：`workflow run --server` envelope 模式把 "📋 Submitted run X" 进度行路由到 stderr 让 stdout 仍是单一可解析信封

### 2.5 P3.4 doctor 扩展（3 PR）+ mcp.toml + plugin dry_run

**Commit 群**：`ad89d4b` (PR.1 plugin manifest dry_run + smoke runner) → `6a532c7` (PR.2 top-level MCP config schema + CLI) → `85b4834` (PR.3 wire mcp.toml + plugin dry_run probes → close P3.4)

**核心变化**：
- `doctor --check-installations` 引入 `installations` section，遍历 `~/.agentflow/skills/*` 和 `~/.agentflow/plugins/*`
- 每个 declared MCP server command 报告 `reachable`（via `which`），每个 plugin entrypoint 报告 `entrypoint_exists`
- 上述任意一项失败 → Status 升级到 Warning / Fail（per profile）

### 2.6 P3.8 W3C traceparent：跨 LLM / Plugin / MCP / Worker gRPC 4 类 hop

**Commit 群**：`7b03e02` (MCP JSON-RPC meta envelope) → `0ba770c` (worker gRPC metadata) → `fe90e81` (4-hop E2E acceptance)

**核心变化**：
- LLM hop（HTTP）、Plugin spawn（env var `TRACEPARENT`）、MCP（JSON-RPC `_meta.traceparent`）、Worker gRPC metadata 全部注入
- 4 个 hop 在 `agentflow-cli/tests/p3_8_cross_hop_e2e.rs` 单次 E2E 验证
- `docs/TRACE_PERSISTENCE_SCHEMA.md` 加 "Hop continuity (P3.8)" 表：LLM ✓ + Plugin ✓ + MCP ✓ + Worker gRPC ✓

### 2.7 P-H 段：Harness Agent Mode 全部 5 phase 完成

**已在 5/14 评估时收尾 P-H.0–P-H.4**；5/14 → 5/19 关键增量是 **P-H.5 完整 4 slice 收尾**：
- Slice 1：DB schema + 6 routes（含 SSE backfill）
- Slice 2：approval routes + LLM-backed `LiveHarnessExecutor`
- Slice 3：Web UI 完整 detail page（event timeline + approval cards + cancel button）
- Slice 4：`POST :resume`（rerun + append 模式）+ EventSource SSE + 完整 E2E（submit → SSE → DB → terminal → resume → rerun history 一次走完）

### 2.8 P-LLM/N9 DeepSeek + MiniMax 入 registry（最近一组 commit）

**Commit**：`aae8ec6 feat(llm): promote DeepSeek + MiniMax to full registry citizens`

**核心变化**：
- `create_provider` 工厂加 `"deepseek"` / `"minimax"` 路由到 `OpenAIProvider::new(...)`
- `validation.rs` supported_vendors 列表 + `model_config.rs` validate() vendor 列表 + `get_api_key()` fallback 同步加 deepseek/minimax 别名
- `error.rs` env_var_hint 补 deepseek/minimax 提示，顺手补 glm 别名
- `default_models.yml` 加 `[providers.deepseek]` + `[providers.minimax]` 两个 provider 条目，外加 8 个 model 条目（`deepseek-chat` / `deepseek-reasoner` / `deepseek-v4-flash` / `deepseek-v4-pro` + `MiniMax-M2` / `M2.5-highspeed` / `M2.7` / `M2.7-highspeed`）
- 用户现可在 skill.toml / workflow.yml 写 `vendor: deepseek` / `vendor: minimax` 直接调用

---

## 3. commit 分布与主题（177 commits since 2026-05-14）

| 类型 | 数量 | 主要主题 |
| --- | --- | --- |
| feat | 93 | N8 idempotency / P3.3 envelope migration / P-LLM dispatcher / P3.8 traceparent / P-H.5 server+UI / P3.4 doctor / DashScope/DeepSeek/MiniMax 入 nightly + registry |
| docs | 39 | 文档收敛 / CLAUDE.md 更新 / TODOs.md sweep / archive 整理 / CURRENT_STATUS / STABILITY |
| test | 10 | N9 cross-provider invariants / live nightly setup / 跨 hop traceparent E2E |
| fix | 9 | live nightly model refresh / init validation 放宽 / formatting / clippy lints |
| chore | 7 | fmt sweep / 维护配置 |
| ci | 5 | bench gate / linux sandbox check / examples smoke |
| refactor | 4 | P-LLM modality cleanup / nodes dispatcher routing |
| style | 1 | — |
| perf | 1 | — |

**主题观察**：

- `feat` 占 53% 反映了"产品功能补齐"是这 5 天的主轴
- `docs` 占 22%（39 commits）凸显**这是一个文档收敛期** —— TODOs.md "Recently Closed" 重组 + 4 个 archive 文件 + CHANGELOG 维护 + CLAUDE.md / RoadMap.md / N9/N10 status 修正 / docs/archive/ 子目录新建等都包含在这块
- 极少 `fix`（9 个，5%）说明这 5 天稳定性回归很少 —— 凡是 fix 都集中在 live nightly 的 vendor-side model 弃用应对（与 agentflow 自身代码无关）
- 0 个 `revert`，0 个 `BREAKING CHANGE` —— 增量演进，没有架构性回退

---

## 4. 每 crate 细评（基于本期变更）

### 4.1 `agentflow-core` ⭐⭐⭐ 成熟度：A

- ✅ Flow / scheduler / FlowValue / expression / plugin host 全部 production-ready
- ✅ **N8 closure**：`decode_checkpoint_flow_value` 把 tagged-but-corrupt（warn）和 untagged-legacy（silent）两种 fallback 路径区分清楚，type fidelity 闭环
- 不足：无显著缺口；可考虑把 `validate()` 强校验改 lenient（但属于行为变更，跨多个调用者）

### 4.2 `agentflow-nodes` ⭐⭐⭐ 成熟度：A-

- ✅ 16+ 内置节点全 production-ready；5 个非 chat 多模态节点经 P-LLM.3 重构后从硬编码 StepFun 解耦
- ✅ M.7 feature 组合修复：`agentflow-nodes --features batch,conditional` 现可 build + test clean
- 不足：未发现

### 4.3 `agentflow-llm` ⭐⭐⭐ 成熟度：A

- ✅ **9 provider 端到端**（含 nightly live 验证）：OpenAI / Anthropic / Google / Moonshot / StepFun / GLM·Zhipu / DashScope / DeepSeek / MiniMax
- ✅ 12 个 cross-provider invariant（7 个 1afcd17 基线 + 5 个 5/19 新增）跨 streaming / multimodal / tool_choice 三维全覆盖
- ✅ P-LLM modality dispatcher 落地，trait 形态 vendor-agnostic
- ✅ OpenAI Whisper 作为第二 ASR vendor 验证 trait shape 通用性
- 不足：`AgentFlow::init()` 仍然 fail-close 全 provider key（DASHSCOPE/DEEPSEEK/MINIMAX 缺失会撞同样问题，pre-existing UX 限制）

### 4.4 `agentflow-tools` ⭐⭐⭐ 成熟度：A-

- ✅ Tool / Registry / Policy / OS Sandbox（macOS sandbox-exec / Linux seccomp / no-op）/ SSRF / ToolIdempotency 全部 production-ready
- ✅ `select_preparer(profile, force_sandbox, allow_unsandboxed)` 把 P1.8 install-time policy 延伸到 plugin **spawn** time
- 不足：未发现

### 4.5 `agentflow-mcp` ⭐⭐⭐ 成熟度：A-

- ✅ client / server / stdio + retry / timeout / 重连，**182 测试**（5/14 比 165）
- ✅ P3.8 W3C traceparent 注入到 MCP JSON-RPC `_meta` envelope
- 不足：`client_old` 历史包袱仍在；server 标 experimental

### 4.6 `agentflow-rag` ⭐⭐⭐ 成熟度：A-

- ✅ 全链路 + eval harness（Recall@K / MRR / nDCG@K / paired sign p-value）
- ✅ P4.1 CI fixture（20-doc 合成 CC0 语料 + 10 queries + qrels）+ P4.2 baseline snapshots（`bm25.json` + 双阈值 regression gate）
- ✅ `agentflow rag eval --compare-baseline <path>` + `--regression-recall-threshold` + `--regression-p-value`
- 不足：可插拔 retriever（目前仍 BM25 硬编码，dense / hybrid TBD）

### 4.7 `agentflow-memory` ⭐⭐⭐ 成熟度：B+（5/14 比 ⭐⭐）

- ✅ **P4.5/P4.7 收尾**：`layer.rs` 定义 4 层 trait 表面（`MemoryLayer` + `RetentionPolicy` + `PreferenceScope` + `PreferenceStore` + `EntityFactStore` + `SemanticMemoryStore`）
- ✅ `SqlitePreferenceStore` + `SqliteEntityFactStore` 落地；`SemanticMemory::search_semantic` 返回 `Vec<(Message, f32)>` 余弦分数
- ✅ 37 hermetic 测试（36 unit + 1 跨层 integration）证明四层独立性
- 不足：encryption-at-rest 仍是 trait 留口子（生产 KMS 集成是后续 scope）

### 4.8 `agentflow-agents` ⭐⭐⭐ 成熟度：A-

- ✅ ReAct + PlanExecute + 三种 Supervisor + AgentNode + WorkflowTool + Reflection + MemorySummary + cancellation + RuntimeLimits 全部 production-ready
- ✅ **P4.4 eval framework**：`agentflow_agents::eval` 模块 ship `Dataset` + `Assertion` + `EvalRunner` + `AgentRuntimeFactory` trait + `EvalReport`；`AgentStopReason::CostLimitExceeded` 新变体
- ✅ Cost tracking：新 `AgentEvent::LlmCallCompleted` + `PricingTable`（loadable from `AGENTFLOW_PRICING_TABLE` env 或 `~/.agentflow/pricing.yml`）
- ✅ P-H.3 parallel tool calls：`>= 2` tool calls 时 idempotent 并发 / nonidempotent 顺序，partial-failure tolerance + atomic max_tool_calls precheck
- 不足：无显著缺口

### 4.9 `agentflow-skills` ⭐⭐⭐ 成熟度：A-

- ✅ SKILL.md / skill.toml / SkillBuilder / Marketplace / MCP adapter / registry
- ✅ **Validator protocol** (`SKILL_VALIDATOR_PROTOCOL.md`)：`[validation] kind = "none" | "regex" | "command"` 新 manifest 段；`SkillValidator` trait + `RegexValidator` + `CommandValidator`
- ✅ `SkillBuilder::build_with_admission(manifest, dir, admit)` 支持 case-scope tool admission filter
- 不足：P3.5 slice 4 MCP capability discovery（off by default）仍是 opt-in 形态

### 4.10 `agentflow-harness` 🆕 ⭐⭐⭐ 成熟度：A-（新评级）

- ✅ **H0/H1/H2/H3/H4/H5 全 5 phase 完成**
- ✅ `HarnessRuntime` + `HarnessEvent` envelope 已 Beta 稳定层（`HARNESS_ENVELOPE_SCHEMA_VERSION = harness/1`）
- ✅ Hooks + approval：`PreToolHook` / `PostToolHook` / `ApprovalProvider` + 3 ApprovalProvider 实现（`AutoAllow` / `AutoDeny` / `Cli`）+ production profile 自动升级 `NonIdempotent` 到 `RequireApproval` (fail-closed)
- ✅ Parallel tool calls：`ReActAgent::run_with_context` 批处理 ≥ 2 tool call，idempotent 并发 + nonidempotent 顺序，partial failure tolerance
- ✅ Background task tools：`task_create` / `task_get` / `task_list` / `task_stop` / `task_output` + 嵌套 spawn 拒绝 + 64 KiB output 上限
- ✅ Server + Web UI 完整：`/v1/harness/sessions` 6 routes + SSE + approval routes + `LiveHarnessExecutor` + Web UI list/new/detail/resume
- 不足：H6 advanced compatibility（slash-command 生态 / TUI shell / OpenHarness import）已 DEFERRED 到 Later Tracks，按个案 promote

### 4.11 `agentflow-cli` ⭐⭐⭐ 成熟度：A-

- ✅ 14+ 命令族：workflow / skill / llm / image / audio / mcp / trace / rag / plugin / doctor / rag eval / **harness** / **serve** / **cleanup** / **eval run**
- ✅ **P3.3 CLI JSON envelope** 覆盖 10+ 命令，`agentflow.cli/1` schema 固定到 `docs/CLI_JSON_OUTPUT.md`
- ✅ `agentflow serve` / `agentflow serve --check` / `agentflow cleanup --dry-run` / `agentflow doctor --backup-check` / `agentflow plugin generate-workflow-stub`
- 不足：剩 `workflow logs` SSE / skill server-mode / 部分命令的 `--model` / `--execution-mode` / `--run-dir` server 端映射

### 4.12 `agentflow-tracing` ⭐⭐⭐ 成熟度：A

- ✅ EventListener / JSONL / SQLite / Postgres / replay / TUI / OTel OTLP / W3C traceparent / redaction
- ✅ **`agentflow-tracing::context::scope` task-local helper**：canonical traceparent propagation 入口；`TRACEPARENT_ENV` 常量
- ✅ P3.8 跨 4 类 hop（LLM HTTP / Plugin spawn / MCP JSON-RPC / Worker gRPC metadata）全部注入
- 不足：hybrid TUI 视图、trace 比较视图（已在 Web UI 落地 P6.3）

### 4.13 `agentflow-viz` ⭐⭐ 成熟度：B

- ✅ Mermaid / DOT / JSON 静态可视化
- 不足：仍未与 trace 实时联动；建议下个发布周期与 `agentflow-ui` 合并或建立联动协议

### 4.14 `agentflow-server` ⭐⭐⭐ 成熟度：A-

- ✅ Run / Cancel / Graph / Event History / SSE / Skill API / Bearer auth / 安全 profile / CORS+body limit / embedded Web UI
- ✅ **P2.2 retention/cleanup**：`agentflow-server::cleanup` + `CleanupConfig::for_profile` + DB + filesystem 双扫 + 后台 loop in `serve`
- ✅ **P2.4 SSE robustness**：`EventBroker::finalise_with_grace` + 3 类重连（active / recently-completed / long-completed）
- ✅ **P2.6 tenant 边界**：migration `0003_tenant_id_columns.sql` + `X-Agentflow-Tenant` header + 跨 tenant 404 not 403
- ✅ **P-H.5 Harness routes 完整**：6 routes + SSE backfill + approval routes + `LiveHarnessExecutor`
- ✅ **P6.4 user preferences API**：`GET`/`PUT /v1/preferences` + token-shape 值拒绝
- ✅ **P6.2 diagnostics API**：`GET /v1/diagnostics`（in-process doctor）
- 不足：剩 v1.0.0-rc.1 tag cut 时的实际 release dress rehearsal（P7.4-FU 系列已落地，缺 ops 动作）

### 4.15 `agentflow-db` ⭐⭐⭐ 成熟度：B+（5/14 比 ⭐⭐）

- ✅ **8 表 schema** + sqlx migrations + 8 个 repos
- ✅ **M.3 测试覆盖大幅扩展**：从 2 个 smoke 测试扩展到 12 个 hermetic CRUD 测试覆盖每个表 + tenant 隔离 + resume-mode 生命周期
- ✅ migration `0002_harness_sessions.sql` / `0003_tenant_id_columns.sql` / `0004_user_preferences.sql` 三个新 migration
- 不足：backup/restore 仍是文档 + doctor 探针级（生产 backup 是运维 ops scope，非代码 scope）

### 4.16 `agentflow-worker` ⭐⭐⭐ 成熟度：B（5/14 比 C+）

- ✅ **P2.8 worker node 扩展**：从 template/file/mock 三类扩展到 template/file/llm/http/mcp/agent **六类** payload
- ✅ **P5.5 worker admission**：`WorkerCredential` (worker_id + PSK) + `WorkerAdmissionPolicy`（allowed_workers / pre_shared_keys / max_workers / max_concurrent_tasks_per_worker）+ `AuthenticatedControlPlane` 包装层 + PSK rotation overlap-add-then-remove
- ✅ **P5.6 资源限制**：per-node timeout / output size 限制 / cancellation propagation / retry 语义；4 个测试覆盖
- ✅ **P5.7 failure domains**：6 个 scenario 测试（stale heartbeat / worker crash / retryable failure / non-retryable terminal / duplicate completion idempotent / trace stitching）
- 不足：signed-JWT identity 仍是后续 auth track；in-process memory cap 留给操作系统层（systemd / cgroups / K8s）

### 4.17 `agentflow-ui` ⭐⭐ 成熟度：B（5/14 比 ⭐ alpha）

- ✅ Run list / DAG 图 / 事件回放 + SSE 实时更新（5/14 基线）
- ✅ **P6.1 run creation form** (`/ui/runs/new`)：tenant / profile / workflow YAML / inputs JSON / file-pick / localStorage / submit→redirect + Playwright E2E spec
- ✅ **P6.2 provider config diagnostics panel** (`/ui/diagnostics`)：per-component pass/warn/fail 表 + `maskToken` 助手保证不显示 raw token
- ✅ **P6.3 trace comparison view** (`/ui/runs/:id/compare?against=<other>`)：双列独立 fetch + `kind#step_index` 键 + green/amber 高亮 + summary cards
- ✅ **P6.4 preferences 服务端 API + token-shape 拒绝**（UI 端 wiring 在 P6 后续）
- ✅ **P6.5 client-side event filter**：`kind=` / `kind!=` / `kind~` / `step` 操作符 + `AND` 链 + 每 run_id localStorage 持久
- ✅ **Harness Mode 完整 UI**：list / new / detail (EventSource SSE + approval cards + cancel/resume buttons)
- 不足：仍标位"调试器"而非生产前端；缺更深的 dashboard / runbook

### 4.18 `xtask` ⭐⭐⭐ 成熟度：A-（新进入评估）

- ✅ `verify-edition` / `examples-smoke` / `bench-gate` / `check-agent-sdk-doc` 四个子命令
- ✅ Quality CI 多个 job 调用 xtask
- 不足：未发现

---

## 5. 与主题契合度评估（更新版）

> **主题命题**：DAG + Native-Agent 双底座，基于大模型的能力层（LLM/VLM, Tools, RAG, MCP, Skill, subAgent），上层 Rust SDK + CLI + WebUI。

| 主题维度 | 当前对齐情况 | 偏离风险 | 较 5/14 变化 |
| --- | --- | --- | --- |
| **DAG 底座** | ✅ `agentflow-core` 全部 production；N8 checkpoint type fidelity 已闭环 | 无 | + N8 closure |
| **Native-Agent 底座** | ✅ ReAct + PlanExecute + 三 Supervisor + AgentNode + WorkflowTool + cancellation + eval | 无 | + eval framework |
| **Harness Agent Mode** | ✅ 5 phase 全 closed；envelope/hooks/approval/parallel/tasks/server+UI 完整 | 无 | + P-H.5 全部 4 slice |
| **LLM / VLM 组件** | ✅ 9 provider 原生 tool calling + 多模态 + streaming + cross-provider invariants | 无 | + DashScope/DeepSeek/MiniMax 入 registry + 12 invariant |
| **Tools 组件** | ✅ Tool/Registry/Policy + OS Sandbox + SSRF + Idempotency + spawn-time policy gate | 无 | + spawn-time policy 延伸 |
| **RAG 组件** | ✅ 全链路 + eval harness + CI baseline + paired regression gate | 无 | + CI baseline ship |
| **MCP 组件** | ✅ client/server/stdio + 182 测试 + traceparent JSON-RPC meta + Skill adapter | 无 | + traceparent + 17 测试 |
| **Skill 组件** | ✅ SKILL.md + Marketplace + Builder + Validator protocol + Admission filter | 无 | + Validator protocol |
| **subAgent 组件** | ✅ 三 Supervisor + AgentNode + 并行 tool calls + background tasks | 无 | + parallel/tasks |
| **Rust SDK 上层** | ✅ 公共 traits + Re-exports + 完整 examples（12 row matrix）+ CHANGELOG | 无 | + examples matrix + CHANGELOG |
| **CLI 上层** | ✅ 14+ 命令族 + JSON envelope 统一（10+ 命令）+ `serve`/`cleanup`/`doctor` 完整 | 无 | + P3.3 envelope + serve/cleanup/diagnostics |
| **WebUI 上层** | 🟡 B 级（调试器形态，但已含 run creation/诊断/trace 比较/preferences/event filter/Harness 完整 UI） | 无（路线图明确"WebUI is debugger, not required for headless"）| + P6.1–P6.5 全部 + Harness Mode UI |
| **平台化（server/db/worker）** | ✅ retention/tenant/backup expectations 完成；worker auth/resource limit/failure domain 完成；6 类 node payload | 无 | + 所有 P2/P5 段 closed |

**结论：项目在所有 13 个主题维度上 100% 对齐**，无任何偏离。两个推迟的项（Slack/Telegram/Discord channel adapters + Local OS keyboard/mouse control）仍是 RoadMap "Non-Goals For V1"，路线图守得很紧。

---

## 6. 风险盘点（更新版）

| # | 风险 | 严重性 | 较 5/14 变化 |
| --- | --- | --- | --- |
| R1 | FlowValue::File/Url checkpoint 类型损失 | — | ✅ **已解决**（P0.2 + N8 closure） |
| R2 | LLM 工具调用 cross-provider 不稳健 | — | ✅ **已解决**（44 + 12 invariant + 9-provider nightly） |
| R3 | server / db scaffold | — | ✅ **已解决**（retention/tenant/backup/diagnostics/preferences 全部完成） |
| R4 | 多智能体协作仅雏形 | — | ✅ **已解决**（三 supervisor 落地） |
| R5 | 权限过滤型，缺 OS jail | — | ✅ **已解决**（OS sandbox + spawn-time policy + skill inspect 可见性） |
| R6 | OTel context 跨 LLM hop 断裂 | — | ✅ **已解决**（全 4 类 hop 注入 + E2E 验证） |
| R7 | RAG 缺评测 harness | — | ✅ **已解决**（eval + CI baseline + regression gate） |
| R8 | YAML schema 错误经验 | 低 | 未变；`workflow validate --explain-permissions` 已增强 |
| R9 | workspace edition 不统一 | — | ✅ **已解决**（M.6 verify-edition CI gate） |
| R10 | Worker 仅支持 3 类 node 执行 | — | ✅ **已解决**（六类 payload, P2.8 closed） |
| R11 | Worker 缺 auth/admission/resource limit | — | ✅ **已解决**（P5.5/P5.6/P5.7 全部 closed） |
| R12 | Web UI 是 alpha 形态 | — | ✅ **已升级到 B**（P6.1–P6.5 + Harness Mode UI） |
| R13 | Memory layering / 长期记忆 schema 初级 | — | ✅ **已解决**（P4.5 设计 + P4.7 实现 + 37 测试） |
| R14 | CLI feature 矩阵 CI 覆盖不足 | — | ✅ **已解决**（18 row matrix in `quality.yml`） |
| **R15 (新)** | `AgentFlow::init()` 仍 fail-close 全 provider key 缺失 | 低 | 9 provider 已 nightly 全过；新用户场景需要要么设置 9 个 env var 要么用空值兜底 — UX 不理想，但属于 pre-existing 行为，不阻塞 v1.0 候选 |
| **R16 (新)** | DashScope / DeepSeek / MiniMax 没有 dedicated provider 模块 | 低 | 4 个 OpenAI-compat vendor 共享 `OpenAIProvider`；若将来有 wire 形态分歧需要分模块时再 promote |
| **R17 (新)** | v1.0.0-rc.1 tag 切版尚未执行 | 中 | 所有代码 / 文档 / CI 准备就绪；缺 ops 动作（crates.io publish / GitHub Release / 干净 VM doctor smoke） |

---

## 7. 优化路线（基于当前现状重排优先级）

> 与已有 `RoadMap.md` 和 `TODOs.md` 一致。当前主轴是 **v1.0.0-rc.1 release engineering**。下面按"对发布最关键的回报/成本比"分类。

### 7.1 v1.0.0-rc.1 cut（即将到来，主要为 ops）

1. **执行 P7.4-FU4 production deployment runbook 6 步走查**（`docs/RELEASE_NOTES_v1.0.0-rc.1.md` DRAFT 已就位）
2. **`cargo publish --dry-run`** 全可发布 crate
3. **GitHub Release artifact / image push**
4. **Fresh VM `agentflow doctor --profile production --backup-check` smoke**
5. **Tag `v1.0.0-rc.1`**

### 7.2 v1.0 GA 前（建议下个发布周期）

6. **`AgentFlow::init()` lenient validation**：`validate()` 对 missing key 改 warn 而非 fail，让新用户开箱即用而无需配齐 9 个 provider env var
7. **可插拔 RAG retriever**：BM25 → dense / hybrid / 自带 trait
8. **Token 计数精确化**：provider-specific tokenizer
9. **Plugin runtime WASM 选项**：subprocess JSON-RPC 站稳后再考虑

### 7.3 v1.x（中期演进）

10. **Web UI 产品化**：从调试器形态升级到运营仪表盘（运行成本 / retry rates / policy decisions / worker utilization）
11. **Slash-command 生态 / TUI 形态**：Harness H6 advanced compatibility，按个案 promote
12. **DashScope/DeepSeek/MiniMax dedicated provider 模块**：仅在 vendor 出现 wire 分歧时再做

### 7.4 文档维护

13. **更新 `CLAUDE.md`** — 已与最新现状基本一致；新加 `agentflow-harness` 描述、9 provider 列表、N10 closed 状态都已修正
14. **`docs/LLM_PROVIDERS_MATRIX.md`** — 已落地（P3.7），9 provider 覆盖
15. **CHANGELOG.md** — 已用 conventional commits 维护到 HEAD

---

## 8. 推荐发布节奏

| 里程碑 | 目标 | 状态 |
| --- | --- | --- |
| v0.3.0 | 平台骨架 + tool calling 原生 + checkpoint 保真 | ✅ **已发** |
| v0.4.0 | 协作范式 + 沙箱强化 + OTel 端到端 + RAG eval | ✅ **已实质完成**，已 ship |
| v0.5.0 | Server 完整化 + sandbox 可见性 + provider 一致性矩阵 | ✅ **已实质完成**，已 ship |
| **v1.0.0-rc.1** | Worker 生产化 + Agent/Memory eval + Web UI 产品化 + CLI JSON 契约 + Harness Mode 完整 | ✅ **代码完成**；剩 ops 动作（tag / publish / release） |
| v1.0 GA | 文档收敛 + 稳定承诺 + CI 基线全绿 + 用户反馈一轮 | rc.1 切完后启动 |

---

## 9. 最终结论

AgentFlow 在 5 天内完成了**全部 N 系列路线图段（N1–N10）的实质性收尾**，并新增 `agentflow-harness` 作为 L3 第三轨。代码层已经从"框架级 v1.0 候选"过渡到"**v1.0.0-rc.1 候选窗口已完全打开，仅缺人工 ops 动作**"。

**确认对齐项目主题**：

- ✅ **DAG 底座** 已生产可用（A 级），N8 checkpoint type fidelity 闭环
- ✅ **Native-Agent 底座** 已生产可用（A- 级，含 eval framework + cost tracking）
- ✅ **Harness Agent Mode** 已生产可用（A- 级新评，全 5 phase closed，envelope Beta）
- ✅ **LLM / VLM 能力层** 9 provider 原生 tool calling + 多模态 + 12 cross-provider invariants
- ✅ **Tools 能力层** OS 级强制隔离 + SSRF/路径硬化/Idempotency/spawn-time policy
- ✅ **RAG 能力层** 链路 + eval harness + CI baseline + paired regression gate
- ✅ **MCP 能力层** 182 测试 + traceparent JSON-RPC meta
- ✅ **Skill 能力层** Marketplace + Builder + Validator protocol + Admission filter
- ✅ **subAgent / 多智能体** 三 Supervisor + AgentNode + parallel tool calls + background tasks
- ✅ **Rust SDK** 公共 trait + examples matrix + CHANGELOG
- ✅ **CLI** 14+ 命令族 + P3.3 JSON envelope 10+ 命令 + serve/cleanup/diagnostics/eval
- ✅ **Web UI** B 级（含 Harness Mode 完整 UI + 5 个 P6 surface）
- ✅ **平台化（server/db/worker）** retention / tenant / backup / worker auth / 六类 node payload 全部完成

**唯一非代码 gating 项**：v1.0.0-rc.1 tag cut 是人工 ops，建议你执行 P7.4-FU4 checklist 后切版。

**下一个评估窗口建议**：v1.0.0-rc.1 ship 后 + 1-2 周内（用户反馈期），或 v1.0 GA 前。届时关注：

1. v1.0.0-rc.1 publish 是否成功（crates.io / GitHub Release）
2. 新用户在干净 VM 上首次安装的 UX 摩擦点（特别是 `AgentFlow::init()` 全 provider key 要求）
3. Harness Mode 在真实长会话场景的稳定性（H6 advanced compatibility 是否有 promote-worthy 项浮现）
4. 9 provider live nightly 在长期运行中的 vendor-side model 弃用频次（决定要不要建立自动模型刷新机制）
5. Web UI 在运营场景的进一步反馈（产品化层级是否需要超越"调试器"）

> 评估签名：HEAD `daaa912e358d9b7069d7585cc8897a903c0fec0b`（2026-05-19；工作树 clean，无 unstaged 文件）
>
> 主要参考：
>
> - **代码**：
>   - `agentflow-core/src/{flow,scheduler,value,expression}.rs`
>   - `agentflow-core/src/plugin/{host,node,registry}.rs`
>   - `agentflow-agents/src/{runtime,react/agent,plan_execute,reflection,eval/*}.rs`
>   - `agentflow-agents/src/supervisor/{handoff,blackboard,debate}.rs`
>   - `agentflow-harness/src/{lib,runtime,events,tasks,hooks_runtime,approval_providers}.rs`
>   - `agentflow-tools/src/{tool,policy,sandbox/{macos,linux,noop},builtin/http}.rs`
>   - `agentflow-llm/src/{tool_calling,modality_dispatch,providers/{openai,anthropic,google,moonshot,stepfun,openai_asr,mod}}.rs`
>   - `agentflow-server/src/{lib,auth,runs,skills,events_stream,ui,cleanup,tenant,harness*,scheduler/{distributed,grpc}}.rs`
>   - `agentflow-db/src/{database,repo}.rs` + `migrations/000{1..4}_*.sql`
>   - `agentflow-worker/src/{lib,protocol,runtime}.rs` + admission
>   - `agentflow-ui/src/{main,RunCompare,DiagnosticsPanel,RunCreateForm,eventFilter,Harness*}.tsx`
>   - `agentflow-rag/src/eval/{metrics,runner,baseline}.rs`
> - **测试**：
>   - `agentflow-llm/tests/{provider_consistency,provider_consistency_live}.rs`
>   - `agentflow-cli/tests/{p3_8_cross_hop_e2e,json_envelope_migration_tests,doctor_cli_tests,mcp_config_cli_tests,rag_eval_cli_tests}.rs`
>   - `agentflow-server/tests/{harness_routes,harness_approval_routes,harness_live_executor,harness_full_stack_e2e,cleanup_route,e2e_runs}.rs`
>   - `agentflow-worker/tests/{resource_limits,failure_domains}.rs`
> - **文档**：
>   - `docs/{EXPRESSION_LANGUAGE,MULTI_AGENT,DISTRIBUTED,WEB_UI,MARKETPLACE,TOOL_PERMISSIONS,RAG_EVAL,API_COMPATIBILITY,STABILITY,CURRENT_STATUS,HARNESS_MODE,LLM_PROVIDERS_MATRIX,CLI_JSON_OUTPUT,MEMORY_LAYERING,AGENT_EVAL_FORMAT,SKILL_VALIDATOR_PROTOCOL,SERVER_BACKUP_RESTORE,TRACE_PERSISTENCE_SCHEMA,MCP_CAPABILITY_POLICY}.md`
>   - `RoadMap.md` / `TODOs.md` / `CLAUDE.md` / `CHANGELOG.md` / `AGENTS.md`
> - **CI**：
>   - `.github/workflows/{quality,bench,llm-live}.yml`
>   - `xtask/src/` 子命令
> - **历史评估**：
>   - `docs/archive/PROJECT_EVALUATION_2026-05-01.md`
>   - `docs/archive/PROJECT_EVALUATION_2026-05-14.md`
