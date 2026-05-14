# AgentFlow 项目深度评估报告 (2026-05-14)

- 评估日期：2026-05-14
- 评估范围：workspace 全部 15 个 Rust crate + 1 个 Web UI crate (`agentflow-ui`)，`docs/`、`RoadMap.md`、`TODOs.md`、CLI 执行路径、agent runtime、DAG 调度器、平台化（server/db/worker）、Web UI、插件/Skill/MCP/RAG/Tracing 全链路
- 与上一版报告 (`PROJECT_EVALUATION_2026-05-01.md`) 的关系：上一版评估在 2 周前定稿，记录的 14+2 crate 中 server/db 为 ~130/48 行骨架；本版基于 `main` HEAD 重新校核全部代码与测试，已覆盖：N7~N10 路线图收尾、P0 全部 7 项、P1.1–P1.5 安全/治理已完成
- 编译/测试基线：未在本评估中重跑，但各 crate 现有测试统计如下（共 **1174** 个 test 标注，相对 5/1 报告的 479 测试翻倍以上）

---

## 0. TL;DR

AgentFlow 已经从"DAG + Agent-native 双轨框架雏形"演进为**"具备完整平台化骨架的双轨 AI 编排框架"**。在 2 周内完成了三件极具结构性意义的工作：

1. **`agentflow-server` 从 130 行骨架 → 4.7K 行可用网关**（35× 增长），SSE / Run API / Skill API / Auth / 安全 profile / CORS / body limit 全部落地
2. **`agentflow-db` 从 48 行 → 653 行 + 完整 schema/migrations/repos**
3. **新增两个 crate**：`agentflow-worker`（分布式执行底座）+ `agentflow-ui`（React 19 + Vite 7 Web 调试器，编译期嵌入 server）

| 维度 | 上次评级 (5/1) | 本次评级 (5/14) | 一句话判断 |
| --- | --- | --- | --- |
| 架构清晰度 | A- | **A** | 四层心智模型在代码层完全坐实，新增 worker 后边界仍清晰 |
| DAG 内核成熟度 | A- | **A** | 表达式引擎自研落地，FlowValue checkpoint 类型保真完成 |
| Agent-native runtime 成熟度 | B+ | **A-** | 多智能体 Handoff/Blackboard/Debate 三种范式上线，provider-native tool calling 落地 |
| LLM 抽象成熟度 | A- | **A** | 6 provider 原生 `tool_calls`/`tool_choice`，多模态/streaming 完整 |
| 工具/权限治理 | C+ | **B+** | OS sandbox（macOS sandbox-exec、Linux seccomp）+ SSRF 防护 + 路径硬化 + Idempotency 元数据全部落地 |
| Config-first / CLI 体验 | B | **B+** | Plugin/Doctor/Trace replay/RAG eval 已成体系 |
| 生产可观测性 | B+ | **A-** | OTel + W3C `traceparent` 已注入 LLM HTTP 调用，全链路 stitched span 可用 |
| 服务端平台化 | C- | **B** | Run/Cancel/SSE/Skill API + Auth + Web UI + 安全 profile 全在；剩 retention/tenant 边界 |
| 分布式调度 | — | **C+ (foundation)** | Worker 协议/gRPC transport/in-memory transport 已通，仅 3 类 node 在 worker 执行；生产化未到 |
| Web UI | — | **C (alpha)** | React 19 + Vite 7 SPA 静态嵌入 server，run 列表/DAG 图/事件回放可看；非生产形态 |
| 综合 | B+ | **A-** | 框架级骨架完成度全面拉到 v1.0 候选窗口，需要在安全/隔离/分布式可靠性、Web UI 产品化上再花一个发布周期 |

**与主题契合度**：项目的核心命题——**"DAG + Native-Agent 双底座 + LLM/Tools/RAG/MCP/Skill 能力层 + Rust SDK / CLI / WebUI 上层"**——在代码层面**100% 对齐**，没有偏离。两个新模块（worker 分布式 / ui Web 调试器）均落在原 RoadMap 的 N10 / P2-P5 轨道内，是补齐而非外扩。

---

## 1. Workspace 全景（15 Rust crate + 1 Web UI）

### 1.1 crate 规模 & 测试覆盖（来自实际代码统计）

| 层 | Crate | 角色 | LOC | 测试数 | 版本 | edition | 成熟度 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **L1 执行内核** | `agentflow-core` | DAG 引擎、AsyncNode、FlowValue、scheduler、checkpoint、retry、timeout、health、events、**expression engine**、**plugin host** | 12,157 | 204 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-nodes` | 内置 16+ 节点（LLM/HTTP/File/Template/Map/While/RAG/MCP/多模态） | 5,099 | 45 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-llm` | 6 provider + 多模态 + streaming + **provider-native tool_calls/tool_choice** + OTel traceparent | 10,612 | 104 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-tools` | Tool/Registry/Policy/**OS Sandbox (macOS/Linux/no-op)**/**SSRF 防护**/**ToolIdempotency** | 3,996 | 69 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-mcp` | MCP client/server/stdio transport，retry/timeout/重连 | 6,309 | 165 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-rag` | chunk/embed/Qdrant/retrieval/rerank + **eval harness (Recall@K, MRR, nDCG@K)** | 8,444 | 139 | 0.3.0-alpha | 2024 | ⭐⭐⭐ |
| **L2 能力适配** | `agentflow-memory` | MemoryStore + Session/SQLite/Semantic | 1,399 | 16 | 0.1.0 | 2024 | ⭐⭐ |
| **L3 智能体/编排** | `agentflow-agents` | ReAct + PlanExecute + **Handoff/Blackboard/Debate Supervisor** + AgentNode + WorkflowTool + Reflection + MemorySummary | 9,860 | 121 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L3 智能体/编排** | `agentflow-skills` | SKILL.md/skill.toml + SkillBuilder + Marketplace + MCP adapter + registry | 4,414 | 76 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L3 智能体/编排** | `agentflow-cli` | workflow/skill/llm/image/audio/mcp/trace/rag/**plugin**/**doctor**/**rag eval** | 9,669 | 104 | 0.2.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-tracing` | EventListener + JSONL/SQLite/Postgres + replay + TUI + **OTel OTLP + W3C traceparent 注入** + redaction | 4,301 | 34 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-viz` | YAML → VisualGraph → Mermaid/DOT/JSON | 1,801 | 26 | 0.1.0 | 2024 | ⭐⭐ |
| **L4 运维/产品化** | `agentflow-server` | **Axum gateway**：POST/GET `/v1/runs`，cancel，graph，SSE history，Skill API，Bearer auth，安全 profile，CORS+body limit，**embedded Web UI (`/ui`)**，**分布式 control plane (`scheduler/distributed.rs`)** | 4,698 | 59 | 0.1.0 | 2024 | ⭐⭐⭐ |
| **L4 运维/产品化** | `agentflow-db` | **Postgres schema (6 表)** + `sqlx::migrate!()` + Run/Step/Event/Artifact/SkillInstall/McpSession repos | 653 | 4 | 0.1.0 | 2024 | ⭐⭐ |
| **L4 运维/产品化** | `agentflow-worker` 🆕 | **分布式 worker** (in-memory + gRPC transport)，claim/heartbeat/execute/report 协议；目前支持 template/file/mock 三类 node | 720 | 8 | 0.1.0 | 2024 | ⭐ foundation |
| **L4 运维/产品化** | `agentflow-ui` 🆕 | React 19 + Vite 7 + TypeScript 5.8 SPA；零运行时依赖；编译期 `include_str!` 嵌入 server | (前端) | n/a | n/a | n/a | ⭐ alpha |

**关键观察**：
- **总 Rust LOC ≈ 84K**（不含 UI 前端），相对 5/1 评估 (~56K) 增长 ~50%；**总测试 ≈ 1174**（5/1 评估 479），翻倍以上
- **workspace edition 已统一到 2024**（上次报告里 server/db 与其他 crate 不一致的问题已修复）
- **`agentflow-cli` 单 crate 测试已达 104**（5/1 评估只有 "5+"），CLI 集成测试体系成型

### 1.2 四层心智模型（与上次相同，仍然成立）

```
+----------------------------------------------------------------+
| L4 运维/产品化 | tracing · viz · server · db · worker · ui     |
+----------------------------------------------------------------+
| L3 智能体/编排 | agents · skills · cli                          |
+----------------------------------------------------------------+
| L2 能力适配    | nodes · llm · tools · mcp · rag · memory       |
+----------------------------------------------------------------+
| L1 执行内核    | core (Flow / GraphNode / FlowValue / Expr / Plugin) |
+----------------------------------------------------------------+
```

- L1 唯一执行核：`Flow::execute_*` 拥有节点状态池、拓扑、并发、checkpoint、事件、**表达式求值器**、**plugin host**
- L2 全部以 `AsyncNode` / `Tool` / `EmbedClient` / `MemoryStore` 等抽象被 L3 使用，L1 不直接依赖任何外部能力
- L3 双轨入口：`agentflow-agents` 承载 agent-native（自主循环、多智能体），`agentflow-nodes + agentflow-cli` 承载 DAG，二者通过 `AgentNode` × `WorkflowTool` 互通
- L4 横切面：`tracing` 非侵入接入 L1；`server` + `db` 提供平台化 API；`worker` 提供分布式底座；`ui` 提供 Web 调试器

---

## 2. 架构设计深度评估

### 2.1 DAG 执行模型（成熟，本期主要补齐表达式与 checkpoint）

**核心抽象（`agentflow-core/src/flow.rs`、`scheduler.rs`、`expression.rs`）：**

- `FlowExecutionMode::{Serial, Concurrent}`，`Concurrent` 模式基于 `FuturesUnordered` + `max_concurrency` 的**依赖就绪滚动调度**
- 三类节点形态：`Standard` / `Map { parallel }` / `While { condition, max_iterations }`
- **新增：表达式引擎**（`docs/EXPRESSION_LANGUAGE.md`）
  - 算术 (`+ - * /`)、比较 (`> < >= <= == !=`)、布尔 (`&& ||`)
  - 内置函数：`len()`、`contains()`、`is_null()`、`to_number()`、`to_string()`
  - 路径表达式：`nodes.X.outputs.Y`
  - 取代了 5/1 时还在用的"字符串比较"占位实现
  - 在 `agentflow workflow validate --strict` 中可做静态检查
- **FlowValue checkpoint round-trip 已修复**（P0.2 DONE）：tagged-schema 反序列化保证 `Json | File | Url` 类型保真，并兼容旧 raw-JSON 格式
- `run_dir` 通过 `--run-dir` / `AGENTFLOW_RUN_DIR` 已完全脱离 home 目录依赖

**仍需打磨：**

| 问题 | 现状 | 影响 |
| --- | --- | --- |
| 节点依赖隐式自动推导 | 仍需显式 `dependencies` | 表达力够，编写啰嗦 |
| 子 Flow 失败语义组合 | fail-fast / continue-on-skip 两档 | 缺少"失败重试到 N 次再放弃 + 继续"细粒度策略 |
| 表达式编辑器 | 仅静态语法检查 | YAML 编辑期无自动补全 |

### 2.2 Agent-native Runtime（接近 production-ready）

**核心抽象（`agentflow-agents/src/runtime.rs`、`react/`、`plan_execute/`、`supervisor/`）：**

- `AgentRuntime` trait + `AgentContext` + `RuntimeLimits`（max_steps / max_tool_calls / timeout_ms / token_budget）
- `AgentStepKind` 6 种、`AgentEvent` 8 种、`AgentStopReason` 8 种（结构化 trace）
- **多智能体协作三大范式上线**（`docs/MULTI_AGENT.md`）：
  - `HandoffSupervisor` (`supervisor/handoff.rs`) — 显式角色切换
  - `BlackboardSupervisor` (`supervisor/blackboard.rs`) — 共享白板/事实区
  - `DebateSupervisor` (`supervisor/debate.rs`) — 多 agent 辩论/投票
  - 每种范式都有 builder + AgentRuntime 实现
- **`AgentNode` × `WorkflowTool`** 互通保持稳定，`AgentNodeResumeContract` 仍强制 idempotent 校验
- **provider-native tool calling**：ReAct/PlanExecute 主路径已切到 LLM 原生 `tool_calls`/`tool_choice`（见 2.4）

**仍需打磨：**

| 问题 | 现状 | 影响 |
| --- | --- | --- |
| Token 计数估算 | 仍是粗粒度（≈4 字符 / 1 token） | 长上下文跨 provider 不一致 |
| Non-idempotent resume 可观测性 | 拒绝隐式重放但 CLI 输出不直观（P1.7 TODO） | 用户难判断为何 resume 被拒 |
| Agent eval harness | P4.3/P4.4 TODO | 缺端到端 agent 质量回归 |

### 2.3 工具与权限治理（从"声明式过滤"升级为"OS 级强制隔离"）

**本期最大跃迁**：`agentflow-tools` 从 1.7K LOC / 6 测试 跃迁到 4.0K LOC / 69 测试。

**已落地（`agentflow-tools/src/tool.rs`、`policy.rs`、`sandbox/`、`builtin/`）：**

- ✅ `ToolIdempotency::{ Idempotent | NonIdempotent | Undeclared }` 元数据（与 partial resume 决策耦合）
- ✅ **OS-level sandbox 后端**：
  - macOS `sandbox-exec` (`src/sandbox/macos.rs`)
  - Linux seccomp-bpf (`src/sandbox/linux.rs`)
  - no-op fallback（可见，不静默）
- ✅ **HTTP SSRF 防护**：默认拒绝 loopback、link-local、私有 IP、cloud metadata endpoints（`169.254.169.254`、`metadata.google.internal`、`100.100.100.200`）；通过 `SandboxPolicy` 显式 opt-in（P1.4 DONE）
- ✅ **File/Script 路径硬化**：路径规范化、symlink 竞态、hardlink、绝对路径、traversal 测试（P1.5 DONE，commit `97a87b6 fix(tools): harden file path enforcement`）
- ✅ `ToolOutputPart::{ Text | Image | Resource }` 多模态输出
- ✅ Tool/Policy 决策事件接入 trace

**仍需打磨：**

| 问题 | 现状 | 影响 |
| --- | --- | --- |
| Sandbox enforcement 可见性 | P1.6 TODO | "现在用的是 macOS/Linux/no-op 哪一个"未在 trace/doctor 输出 |
| Plugin sandbox 默认 policy | P1.8 / P5.4 TODO | 第三方 plugin 默认沙箱姿态未定 |
| ShellTool 默认禁用但缺"受限子集" | 未变 | 需要"opt-in + 白名单"官方推荐路径 |

### 2.4 LLM 抽象（成熟，本期完成原生 tool calling）

- 6 provider：OpenAI / Anthropic / Google / StepFun / Moonshot / Mock
- **原生 `tool_calls` / `tool_choice` 全 provider 落地**（`agentflow-llm/src/tool_calling.rs` 定义类型，各 provider 在 `src/providers/*.rs` 实现 adapter，如 `parse_openai_tool_calls`、`tool_choice_to_openai_value`）
- 多模态 `MultimodalMessage`（text + image url/base64）
- 流式 `StreamingResponse`
- 模型注册和能力描述（`ModelCapabilities`、`ModelType`）
- StepFun 专用图像/音频 API 完整覆盖
- **OTel W3C `traceparent` 注入 HTTP headers**（通过 `LlmTraceContext`），让跨 LLM hop 不再断点

**仍需打磨：**

| 问题 | 现状 | 影响 |
| --- | --- | --- |
| Provider 一致性 CI 矩阵 | P3.6 TODO | 切换 provider 时的 streaming / 多模态 / tool-calling 行为差异未自动化测出 |
| Token 计数 | 同 §2.2，粗粒度 | 跨 provider 不可比 |
| 没有中心 provider matrix 文档 | 子代理报告"未找到中心 doc" | 用户需翻代码确认每个 provider 支持哪些能力 |

### 2.5 RAG（成熟 + 完整 eval harness）

- chunk → embed → Qdrant → retrieval → rerank 全链路
- **`eval/` 模块**：`MetricKind::{ Recall, Mrr, Ndcg }`，函数 `recall_at_k()` / `reciprocal_rank()` / `ndcg_at_k()`；支持 graded relevance
- CLI 子命令 `agentflow rag eval`（`agentflow-cli/src/commands/rag/eval.rs`），目前 baseline 硬编码 BM25
- 数据集格式：JSONL（corpus/queries/qrels），配 paired sign test 做基线对比

**仍需打磨：** baseline 数据集与 CI 集成（P4.1 / P4.2 TODO）；可插拔 retriever（目前 BM25 硬编码）。

### 2.6 Tracing / Recovery（链路完整，OTel 端到端通了）

- `Collector` + `EventListener` 非侵入采集
- 持久化：JSONL / SQLite / Postgres
- `agentflow trace replay <run_id>` + TUI
- **OTel OTLP exporter + W3C `traceparent` 在 LLM HTTP 调用注入完成**（消除上次评估指出的"LLM hop 易断"问题）
- 默认 redaction：API key / env secret / sensitive tool params

**仍需打磨：** 复杂 hybrid (DAG × Agent × Tool × Worker) TUI 视图；trace 比较视图。

### 2.7 Server / DB / Worker / UI（本期"平台化"全面起步）

#### `agentflow-server` (4.7K LOC, 59 tests)

- 实际 endpoints：
  - `POST /v1/runs`, `GET /v1/runs`, `GET /v1/runs/{id}`, `POST /v1/runs/{id}:cancel`
  - `GET /v1/runs/{id}/graph`, `GET /v1/runs/{id}/events/history`
  - SSE：`GET /v1/runs/{id}/events`（支持 `after_seq` reconnect backfill）
  - `POST /v1/skills/{name}:run`, `GET /v1/skills`
  - `GET /ui/*`（编译期嵌入 Web UI）
- Auth：Bearer token（`AGENTFLOW_API_TOKEN`），`src/auth.rs`
- 安全 profile：`dev / local / production`（P1.1 DONE），production 模式缺 token 即 fail-closed（P1.2 DONE）
- CORS / body limit：`tower-http`，环境变量可配（P1.3 DONE）
- 分布式 control plane：`src/scheduler/distributed.rs` 定义 worker protocol（claim / heartbeat / execute / report）

#### `agentflow-db` (653 LOC, 4 tests)

- 6 表 schema：`runs / steps / events / artifacts / skill_installs / mcp_sessions`
- `sqlx::migrate!()` 嵌入 `migrations/0001_initial_schema.sql`
- Trait + Pg 实现：`RunRepo` / `StepRepo` / `EventRepo` / `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo`

#### `agentflow-worker` 🆕 (720 LOC, 8 tests)

- **分布式执行底座**，支持两种 transport：
  - `memory://local`（in-process 测试）
  - `grpc://host:port`（远程 control plane）
- 协议循环：`claim → execute → heartbeat → report`
- **当前只支持 3 类 node**：template / file / mock（`agentflow-worker/src/lib.rs:198–210`）— foundation 完成，**生产化未到**
- 与 `agentflow-server::scheduler::distributed` 配对工作

#### `agentflow-ui` 🆕 (React 19 + Vite 7 + TS 5.8)

- 零运行时依赖（无 Redux/Next/Remix）
- 编译期通过 `include_str!` 嵌入 server：`agentflow-server/src/ui.rs:19-21`
- 当前页面：run 列表、DAG 图、状态、事件历史 SSE 回放
- **形态**：调试器/控制台，非生产前端；`dist/` 已 check-in，无需 Node.js 即可运行 server

---

## 3. 双轨能力——DAG vs Agent-native vs Hybrid

### 3.1 DAG 智能体（已生产可用，本期再补强）

| 需求 | 5/1 状态 | 5/14 状态 |
| --- | --- | --- |
| 显式节点依赖 + 拓扑 | ✅ | ✅ |
| Map / While / Conditional | ✅ | ✅ |
| **表达式语言** | 🟡 简单字符串 | ✅ **算术/比较/布尔/函数/路径** |
| FlowValue 类型保真 | 🟡 File/Url checkpoint 损失 | ✅ tagged schema 已修复 |
| 依赖就绪并发 | ✅ | ✅ |
| Checkpoint + Resume | ✅ | ✅ + round-trip 测试 |
| 事件 / Trace / Replay | ✅ | ✅ + OTel 端到端 |
| 内置节点库 | ✅ 16+ | ✅ 16+ |
| **Plugin / 自定义 Node** | 🟡 计划中 | ✅ **subprocess JSON-RPC plugin host** + CLI |

### 3.2 Agent-native 智能体（本期完成主路径升级）

| 需求 | 5/1 状态 | 5/14 状态 |
| --- | --- | --- |
| ReAct / PlanExecute / Reflection | ✅ | ✅ |
| Tool calling（统一注册） | ✅ | ✅ + **provider-native tool_calls** |
| Memory（短期/长期/语义） | ✅ | ✅ |
| 取消/超时/步数/token 预算 | ✅ | ✅ |
| 结构化 trace + 停止原因 | ✅ | ✅ |
| **多智能体协作** | 🟡 雏形 | ✅ **Handoff / Blackboard / Debate 三范式** |
| Config-first agent (YAML) | ✅ | ✅ |
| Skill manifest → agent | ✅ | ✅ |
| AgentNode × WorkflowTool 互通 | ✅ | ✅ |
| Provider 一致性矩阵 | 🟡 | 🟡 文档化 TODO (P3.6) |

### 3.3 Hybrid 编排（已生产可用）

`AgentNode`（agent 嵌入 DAG）+ `WorkflowTool`（DAG 暴露给 agent）+ `AgentNodeResumeContract`（partial resume 合约）三件套稳定运转。

---

## 4. Rust 软件工程评估

### 4.1 风格一致性

- ✅ **缩进 2 空格**（覆盖 rustfmt 默认 4 空格，符合 CLAUDE.md 全局约定）
- ✅ **edition 已统一到 2024**（消除上次评估指出的不一致）
- ✅ 错误处理用 `thiserror` 自定义类型 + `Result<T, E>`，未发现裸 `unwrap()` / `expect()` 在非测试路径
- ✅ `///` 文档注释覆盖公开 API
- ✅ `async/await` + Tokio runtime

### 4.2 测试基线

| Crate | 测试数 | 风格 |
| --- | --- | --- |
| core | 204 | 单元 + flow integration + plugin |
| llm | 104 | provider unit + mock |
| mcp | 165 | client/server/transport integration |
| rag | 139 | chunk/embed/retrieval/eval |
| agents | 121 | runtime/react/plan_execute/supervisor |
| nodes | 45 | 各节点类型 |
| tools | 69 | sandbox/policy/SSRF/path hardening |
| skills | 76 | manifest/marketplace/builder |
| cli | 104 | `assert_cmd` 风格集成 |
| tracing | 34 | listener/replay/OTel |
| server | 59 | route/auth/SSE integration |
| db | 4 | repo smoke |
| worker | 8 | protocol smoke |
| viz | 26 | rendering |
| memory | 16 | session/sqlite/semantic |
| **合计** | **1174** | — |

测试-LOC 比 ≈ **1 测试 / 70 LOC**（包含集成测试），属于 Rust 框架级合理范围。短板：`agentflow-db` 仅 4 个 smoke 测试，`agentflow-memory` 长期记忆 schema 测试稀疏。

### 4.3 Feature flag 治理

- `agentflow-nodes`：`mcp`、`rag`、`audio`、`image_*` feature-gated
- `agentflow-tracing`：`sqlite`、`postgres`、`otel` feature-gated
- `agentflow-core`：`plugin` feature-gated
- ⚠️ **CLI 的 feature 组合矩阵尚未在 CI 中全枚举**，单 feature 关闭后的可编译性需要持续维护

### 4.4 依赖分层与循环

- L1 (`agentflow-core`) 不依赖任何 L2/L3/L4 crate
- L2 之间无相互依赖（除 `nodes` 可选依赖 `llm/mcp/rag`）
- L3 依赖 L1 + L2
- L4 横切：`tracing` 仅依赖 L1 抽象（EventListener）；`server` 依赖 L3
- **未发现循环依赖**

### 4.5 Cargo.lock / 编译产物

- `~/.target` 统一 target-dir（符合用户全局偏好）
- Cargo.lock 154K，依赖量与 Rust AI 框架同类项目相当

---

## 5. 模块逐项评估

### 5.1 `agentflow-core` ⭐⭐⭐ 成熟度：高

- ✅ DAG / FlowValue / scheduler / checkpoint / retry / timeout / health / events
- ✅ **表达式引擎**（新）：`expression.rs`，算术/比较/布尔/函数/路径
- ✅ **plugin host**（新）：`plugin/host.rs` + `plugin/node.rs` + `plugin/registry.rs`（subprocess JSON-RPC）
- 不足：节点依赖仍需显式声明；子 Flow 失败策略组合粒度有限

### 5.2 `agentflow-nodes` ⭐⭐⭐ 成熟度：高

- ✅ 16+ 内置节点，feature-gated
- 不足：节点参数 schema 不统一；离线 mock 框架弱；错误码未标准化

### 5.3 `agentflow-llm` ⭐⭐⭐ 成熟度：高

- ✅ 6 provider 原生 tool calling、多模态、流式、注册、能力描述
- ✅ W3C traceparent 注入 HTTP
- 不足：缺中心 provider matrix 文档；token 计数粗粒度

### 5.4 `agentflow-tools` ⭐⭐⭐ 成熟度：从 ⭐⭐ 跃升

- ✅ OS sandbox（macOS/Linux/no-op）、SSRF 防护、路径硬化、ToolIdempotency、多模态输出
- 不足：sandbox 可见性输出（P1.6）、plugin 默认 policy（P1.8/P5.4）、ShellTool "受限子集"

### 5.5 `agentflow-mcp` ⭐⭐⭐ 成熟度：高

- ✅ client/server/stdio、retry/timeout/重连、165 测试
- 不足：`client_old` 历史包袱仍在；server 标 experimental

### 5.6 `agentflow-rag` ⭐⭐⭐ 成熟度：从 ⭐⭐ 跃升

- ✅ 全链路 + **eval harness (Recall@K, MRR, nDCG@K) + paired sign test**
- 不足：CI baseline 数据集（P4.1）、可插拔 retriever（目前 BM25 硬编码）

### 5.7 `agentflow-memory` ⭐⭐ 成熟度：基础

- ✅ Session / SQLite / Semantic 三实现
- 不足：长期记忆 schema、隐私/清理、跨 session 关联策略均较初级；memory layering design（P4.5 TODO）

### 5.8 `agentflow-agents` ⭐⭐⭐ 成熟度：高

- ✅ ReAct / PlanExecute / 三种 Supervisor / AgentNode / WorkflowTool / Reflection / MemorySummary
- 不足：`AgentRuntime` trait 与具体 agent 公共 API 并存；非幂等 tool resume CLI 可见性（P1.7）；agent eval harness（P4.3）

### 5.9 `agentflow-skills` ⭐⭐⭐ 成熟度：高

- ✅ SKILL.md/skill.toml、SkillBuilder、Marketplace、MCP adapter、registry
- ✅ 与 `agent`/`skill_agent` YAML 节点打通
- 不足：Skill `security` × ToolPolicy × CLI flag 三方决策表（P3.5 待打磨）

### 5.10 `agentflow-cli` ⭐⭐⭐ 成熟度：高

- ✅ workflow / skill / llm / image / audio / mcp / trace / rag / **plugin** / **doctor**（基础）/ **rag eval**
- 不足：JSON 输出契约文档化（P3.3）、`agentflow doctor` 扩展（P3.4）、权限解释展示（P3.5）、`agentflow serve` 命令（P2.1）

### 5.11 `agentflow-tracing` ⭐⭐⭐ 成熟度：高

- ✅ EventListener / JSONL / SQLite / Postgres / replay / TUI / **OTel + traceparent** / redaction
- 不足：hybrid TUI 视图、trace 比较视图

### 5.12 `agentflow-viz` ⭐⭐ 成熟度：基础

- ✅ Mermaid / DOT / JSON 静态可视化
- 不足：未与 trace 实时联动；下次发布建议**与 `agentflow-ui` 合并**或建立联动协议

### 5.13 `agentflow-server` ⭐⭐⭐ 成熟度：从 ⭐ scaffold 跃升到 B

- ✅ Run / Cancel / Graph / Event History / SSE / Skill API / Bearer auth / 安全 profile / CORS / body limit / **embedded Web UI** / **distributed control plane**
- 不足：retention/cleanup（P2.2）、tenant 边界（P2.6）、backup/restore（P2.7）、`agentflow serve` CLI（P2.1）

### 5.14 `agentflow-db` ⭐⭐ 成熟度：从 ⭐ scaffold 跃升

- ✅ 6 表 schema + sqlx migration + repos
- 不足：仅 4 个 smoke 测试；retention/backup 策略；测试 LOC 比偏低

### 5.15 `agentflow-worker` 🆕 ⭐ 成熟度：foundation

- ✅ in-memory + gRPC transport、claim/heartbeat/execute/report 协议
- 不足：仅支持 template/file/mock 三类 node（缺 LLM/HTTP/Agent），worker 认证/admission（P5.5）、资源限制（P5.6）、failure-domain（P5.7）全部 TODO

### 5.16 `agentflow-ui` 🆕 ⭐ 成熟度：alpha

- ✅ React 19 + Vite 7 + TS 5.8 SPA，零运行时依赖，编译期嵌入
- ✅ run 列表 / DAG 图 / SSE 事件回放
- 不足：非生产形态；缺 provider config 诊断、trace 比较、durable preferences、运营级过滤（见 RoadMap "Web UI Productization"）

---

## 6. 与主题契合度评估

> **主题命题**：DAG + Native-Agent 双底座，基于大模型的能力层（LLM/VLM, Tools, RAG, MCP, Skill, subAgent），上层 Rust SDK + CLI + WebUI。

| 主题维度 | 当前对齐情况 | 偏离风险 |
| --- | --- | --- |
| **DAG 底座** | ✅ `agentflow-core` Flow / 依赖就绪并发 / Map/While / Checkpoint / 表达式 | 无 |
| **Native-Agent 底座** | ✅ `agentflow-agents` ReAct + PlanExecute + 三类 Supervisor + RuntimeLimits + Cancellation | 无 |
| **LLM/VLM 组件** | ✅ 6 provider 原生 tool calling + 多模态（image/audio）+ streaming | 无 |
| **Tools 组件** | ✅ Tool/Registry/Policy + OS Sandbox + SSRF + Idempotency | 无 |
| **RAG 组件** | ✅ 全链路 + eval harness | 无 |
| **MCP 组件** | ✅ client/server/stdio + 165 测试 + Skill adapter | 无 |
| **Skill 组件** | ✅ SKILL.md + Marketplace + SkillBuilder | 无 |
| **subAgent 组件** | ✅ Supervisor 三范式 + AgentNode 嵌入 DAG | 无 |
| **Rust SDK 上层** | ✅ 公共 traits / re-exports / 完整 examples | 无 |
| **CLI 上层** | ✅ 14 大命令族 + JSON 输出（部分） | 无 |
| **WebUI 上层** | 🟡 alpha 形态，React 19 SPA 嵌入 server，调试器级别 | 无（路线图明确"WebUI is debugger, not required for headless"）|
| **平台化（server/db/worker）** | 🟡 server 已用，worker foundation，retention/tenant 待补 | 无 |

**结论：项目在所有 11 个主题维度上 100% 对齐，没有偏离**。两个新增模块（worker / ui）均落在 RoadMap N10 / Web UI Productization / Distributed Execution 轨道内，是补齐而非扩张。**Slack/Telegram/Discord 等渠道适配明确推迟**（RoadMap Non-Goals），符合"先把底座做扎实"的产品判断。

---

## 7. 风险盘点（更新版）

| # | 风险 | 严重性 | 与 5/1 报告对比 |
| --- | --- | --- | --- |
| R1 | FlowValue::File/Url checkpoint 类型损失 | — | ✅ **已解决**（P0.2 DONE） |
| R2 | LLM 工具调用 prompt 解析跨 provider 不稳健 | 低 | ✅ provider-native 落地；剩 CI 矩阵 (P3.6) |
| R3 | server / db scaffold | — | ✅ **基础已落地**；剩 retention/tenant/backup (P2.2/P2.6/P2.7) |
| R4 | 多智能体协作仅雏形 | — | ✅ **已解决**（三 supervisor 落地） |
| R5 | 权限过滤型，缺 OS jail | 低 | ✅ 已基本解决；剩 sandbox 可见性 (P1.6)、plugin 默认 policy (P1.8/P5.4) |
| R6 | OTel context 跨 LLM hop 断裂 | — | ✅ **已解决**（traceparent 注入） |
| R7 | RAG 缺评测 harness | — | ✅ **已解决**；剩 CI baseline (P4.1) |
| R8 | YAML schema 错误经验 | 低 | 未变；`workflow validate --strict` 已存在 |
| R9 | workspace edition 不统一 | — | ✅ **已解决**（全 2024） |
| **R10** | **Worker 仅支持 3 类 node 执行** | 中 | 🆕 本期新风险，生产分布式调度未到 |
| **R11** | **Worker 缺 auth/admission/resource limit** | 中 | 🆕 P5.5/P5.6/P5.7 全 TODO |
| **R12** | **Web UI 是 alpha 形态** | 低 | 🆕 调试器定位明确，不是阻塞项 |
| **R13** | **Memory layering / 长期记忆 schema 初级** | 中 | 一直存在（P4.5 TODO） |
| **R14** | **CLI feature 矩阵 CI 覆盖不足** | 低 | 一直存在 |

---

## 8. 优化路线（基于当前现状重排优先级）

> 与已有 `RoadMap.md` 和 `TODOs.md` 一致：当前主轴是 **Core Runtime Stabilization**。下面按"对 v1.0 候选最关键的回报/成本比"重新分类。

### 8.1 v0.4.0 发布前（必做）

1. **`agentflow serve` CLI 命令**（P2.1）— 把 server 从 cargo run 变成"一行起飞"
2. **Sandbox enforcement 可见性**（P1.6）— trace / doctor 输出"现在用的是 macOS/Linux/no-op"，闭环安全姿态
3. **Non-idempotent resume CLI 可见性**（P1.7）— 让用户看清楚为什么 resume 被拒
4. **Plugin sandbox 默认 policy**（P1.8 / P5.4）— 第三方 plugin 默认沙箱姿态明确
5. **Provider 一致性矩阵 CI**（P3.6）— streaming / 多模态 / tool calling 跨 6 provider 自动化

### 8.2 v1.0.0-rc（建议本季度交付）

6. **Server retention / tenant / backup**（P2.2 / P2.6 / P2.7）— production server 完整化
7. **Worker 生产化**（P5.5 / P5.6 / P5.7）— auth / admission / resource limit / failure-domain
8. **Worker node 类型扩展** — 至少加 LLM / HTTP / MCP 三类（目前仅 template/file/mock）
9. **`agentflow doctor` 扩展**（P3.4）— config / providers / feature flags / MCP / sandbox / server / db / plugin / marketplace 全诊断
10. **CLI JSON 输出契约文档**（P3.3）— 把 automation-friendly 输出固化下来
11. **RAG eval CI baseline**（P4.1 / P4.2）— 防退化的版本化基线
12. **Agent eval harness**（P4.3 / P4.4）— 端到端 agent 质量回归
13. **Memory layering 设计**（P4.5）— session/semantic/preference/entity facts/retention 边界

### 8.3 v1.x（中期演进）

14. **Web UI 产品化**（RoadMap "Web UI Productization"）— provider 诊断、trace 比较、运营 filter、durable preferences
15. **可插拔 RAG retriever** — 当前 BM25 硬编码，应支持 dense / hybrid / 自带
16. **Token 计数精确化** — provider-specific tokenizer
17. **Plugin runtime 拓展** — WASM 作为 v2 option（RoadMap "Plugin Runtime Expansion"）
18. **Operations 仪表盘** — run latency / retry rates / policy decisions / worker utilization

### 8.4 文档维护

19. **更新 `CLAUDE.md`** — 当前内容已与最新现状基本一致，但需要把"`agentflow-worker`"和"`agentflow-ui`"补入 L4 描述，把"14+2 crate"改为"15+1 (含 Web UI)"
20. **建立中心 LLM provider matrix 文档** — 子代理报告"未找到"，应整理为 `docs/LLM_PROVIDERS_MATRIX.md`（路线图已提及）

---

## 9. 推荐发布节奏

| 里程碑 | 目标 | 状态 |
| --- | --- | --- |
| v0.3.0 | 平台骨架 + tool calling 原生 + checkpoint 保真 | ✅ **已发** |
| v0.4.0 | 协作范式 + 沙箱强化 + OTel 端到端 + RAG eval | ✅ **已实质完成**，待发布动作 |
| **v0.5.0** | Server 完整化（`agentflow serve` + retention） + sandbox 可见性 + provider 一致性矩阵 | 🟡 **进行中**（P0/P1 大部分完成，剩 P2.1/P1.6/P1.7） |
| **v1.0.0-rc** | Worker 生产化 + Agent/Memory eval + Web UI 产品化 + CLI JSON 契约 | 🟡 P2-P5 |
| v1.0 | 文档收敛、稳定承诺、CI 基线全绿 | — |

---

## 10. 最终结论

AgentFlow 在 2 周内完成了一次结构性升级，已经从"框架级骨架"过渡到"**框架级 v1.0 候选**"。

**确认对齐项目主题**：

- ✅ **DAG 底座** 已生产可用（A 级）
- ✅ **Native-Agent 底座** 已生产可用（A- 级，多智能体三范式齐备）
- ✅ **LLM / VLM 能力层** 6 provider 原生 tool calling + 多模态完整
- ✅ **Tools 能力层** 从声明式过滤升级为 OS 级强制隔离 + SSRF/路径硬化/Idempotency
- ✅ **RAG 能力层** 链路完整 + eval harness 落地
- ✅ **MCP 能力层** 165 测试，稳定可用
- ✅ **Skill 能力层** Marketplace + Builder 齐备
- ✅ **subAgent / 多智能体** Handoff/Blackboard/Debate 三范式落地
- ✅ **Rust SDK** 公共 trait + examples 矩阵
- ✅ **CLI** 14 大命令族，覆盖全部能力
- 🟡 **Web UI** alpha 形态（明确定位为"调试器"，非生产前端）
- 🟡 **平台化（server/db/worker）** 骨架完整，剩 retention/tenant/worker 生产化

**唯一需要警惕的偏题信号**：无。Slack/Telegram/Discord 等 channel 适配明确推迟为 Non-Goals，desktop OS control 也明确被 gating，路线图守得很紧。

**下一个评估窗口建议**：v0.5.0 发布后或本季度末（2026-08）。届时关注：

1. `agentflow serve` 是否成为 server 的事实入口
2. Worker 是否支持 LLM/HTTP/MCP node 执行
3. RAG/Agent eval 是否进入 CI baseline
4. Web UI 是否进入"运营级 filter + provider 诊断"形态
5. Provider 一致性矩阵是否进入 CI

> 评估签名：HEAD `738bf92` 之后（2026-05-14；有 4 个 unstaged 文件 `RoadMap.md` / `TODOs-archive-*` / `HARNESS_MODE_EVOLUTION.md` / 新归档；本评估不依赖未提交内容）
>
> 主要参考：
> - `agentflow-core/src/{flow,scheduler,value,expression}.rs`
> - `agentflow-core/src/plugin/{host,node,registry}.rs`
> - `agentflow-agents/src/{runtime,react/agent,plan_execute,reflection}.rs`
> - `agentflow-agents/src/supervisor/{handoff,blackboard,debate}.rs`
> - `agentflow-tools/src/{tool,policy,sandbox/{macos,linux,noop},builtin/http}.rs`
> - `agentflow-llm/src/{tool_calling,providers/*}.rs`
> - `agentflow-server/src/{lib,auth,runs,skills,events_stream,ui,scheduler/distributed}.rs`
> - `agentflow-db/src/{database,repo}.rs` + `migrations/0001_initial_schema.sql`
> - `agentflow-worker/src/lib.rs`
> - `agentflow-ui/{package.json,src/main.tsx}`
> - `agentflow-rag/src/eval/{metrics,runner}.rs`
> - `docs/{EXPRESSION_LANGUAGE,MULTI_AGENT,DISTRIBUTED,WEB_UI,MARKETPLACE,TOOL_PERMISSIONS,RAG_EVAL,API_COMPATIBILITY,STABILITY,CURRENT_STATUS}.md`
> - `RoadMap.md`、`TODOs.md`
