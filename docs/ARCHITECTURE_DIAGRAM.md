# AgentFlow 架构图与模块功能说明

> 本文档由字符绘图（ASCII art）描绘 AgentFlow 的分层架构与模块（crate）划分，并逐一说明每个模块的详细功能。
>
> 配套文档：四种执行范式与三轴心智模型见 `docs/ARCHITECTURE.md` § Four Execution Paradigms；契约内核抽取的 RFC 见 `docs/RFC_CRATE_ARCHITECTURE.md`；依赖律验证见 `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`。

## 架构总览

AgentFlow 是一个 Rust workspace，采用 **"窄腰"（narrow-waist）契约内核 + 运行时隔离** 的五层架构。核心原则：**运行时之间互不依赖，只依赖共享契约（L0）**，由 `cargo xtask check-arch` 用 8 条依赖律强制约束。

共 24 个 Rust crate（workspace member）+ `agentflow-ui`（Vite/React SPA，由 server 内嵌）+ `xtask`（内部工具）。

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│                          L4 · 运维 / 产品化 (Operations)                                  │
│                                                                                          │
│  ┌────────────┐  ┌────────────┐  ┌──────────┐  ┌────────────┐  ┌─────────────────────┐ │
│  │  -server   │  │    -db     │  │ -worker  │  │ -tracing   │  │     -ui (SPA)        │ │
│  │ Axum 网关   │  │ Postgres   │  │ 分布式    │  │ 可观测性    │  │ React+Vite, 内嵌/ui  │ │
│  │ /v1/* SSE  │  │ 9 表+repo  │  │ 执行节点  │  │ OTel/JSONL │  │ run列表/DAG/审批     │ │
│  └─────┬──────┘  └─────┬──────┘  └────┬─────┘  └──────┬─────┘  └──────────┬──────────┘ │
│        │               │     ┌────────┴─────────┐     │                   │            │
│        │               │     │ -worker-proto    │     │                   │            │
│        │               │     │ gRPC 协议(proto) │     │                   │            │
│        │               │     └──────────────────┘     │                   │            │
└────────┼───────────────┼────────────────────────────┼───────────────────┼────────────┘
         │               │                              │                   │
┌────────┼───────────────┼──────────────────────────────────────────────────────────────┐
│        ▼               ▼   L3 · 智能体 / 编排 (Agent & Orchestration)                    │
│                                                                                          │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌────────────┐  ┌──────────┐  ┌──────────┐ │
│  │  -cli    │  │ -config  │  │ -harness  │  │  -agents   │  │ -skills  │  │ (dynamic)│ │
│  │ 统一CLI   │  │ 配置装配  │  │ Harness   │  │ Agent运行时│  │ Skill包  │  │  module  │ │
│  │ workflow │  │ YAML→Flow│  │ Mode/审批 │  │ ReAct/Plan │  │ SKILL.md │  │ in agents│ │
│  │ skill... │  │ doctor   │  │ 后台任务  │  │ 多智能体    │  │ Capab.   │  │          │ │
│  └────┬─────┘  └────┬─────┘  └─────┬─────┘  └─────┬──────┘  └────┬─────┘  └──────────┘ │
└───────┼─────────────┼──────────────┼──────────────┼─────────────┼─────────────────────┘
        │             │              │              │             │
┌───────┼─────────────┼──────────────┼──────────────┼─────────────┼─────────────────────┐
│       ▼             ▼              ▼              ▼             ▼  L2 · 能力适配器          │
│                                                                                          │
│  ┌────────────┐ ┌──────────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐ ┌────────┐ ┌──────┐│
│  │  -nodes    │ │ -nodes-ai    │ │  -llm   │ │ -tools  │ │ -mcp   │ │ -rag   │ │-memory││
│  │ 工具层节点  │ │ 能力层节点    │ │ 6家LLM  │ │ Tool契约│ │ MCP    │ │ 检索增强│ │ 会话  ││
│  │ http/file  │ │ llm/asr/tts  │ │ 流式/   │ │ 注册表/ │ │ 协议    │ │ 向量/  │ │ 记忆  ││
│  │ template   │ │ image/mcp/rag│ │ 工具调用│ │ 沙箱    │ │ 客户端  │ │ BM25   │ │ SQLite││
│  └─────┬──────┘ └──────┬───────┘ └────┬────┘ └────┬────┘ └───┬────┘ └───┬────┘ └──┬───┘│
└────────┼───────────────┼──────────────┼───────────┼──────────┼──────────┼─────────┼────┘
         │               │              │           │          │          │         │
┌────────┼───────────────┼──────────────┼───────────┼──────────┼──────────┼─────────┼────┐
│        ▼               ▼              ▼  L1 · 执行核心 (Execution Core)                   │
│                          ┌─────────────────────────────────────┐                        │
│                          │            -core                     │                        │
│                          │  DAG 执行引擎: 调度器(Serial/Concurrent)│                        │
│                          │  checkpoint / retry / resource /     │                        │
│                          │  health / events · FlowExt::run()    │                        │
│                          └──────────────────┬──────────────────┘                        │
└─────────────────────────────────────────────┼──────────────────────────────────────────┘
                                               │ 运行 L0 的 Flow IR
┌──────────────────────────────────────────────────────────────────────────────────────┐
│            ▼          L0 · 契约内核 (Contract Kernel / 窄腰)                              │
│                  ── 所有运行时只依赖这里，彼此不互相依赖 ──                                  │
│                                                                                          │
│  ┌────────────┐ ┌────────────┐ ┌──────────────┐ ┌──────────────┐ ┌─────────────────┐  │
│  │   -value   │ │   -graph   │ │ -store-spi   │ │ -agent-spi   │ │  -async-util    │  │
│  │ FlowValue  │ │ Flow IR /  │ │ MemoryStore +│ │ AgentRuntime │ │ retry/timeout/  │  │
│  │ 类型化状态  │ │ AsyncNode/ │ │ Knowledge-   │ │ /Capability  │ │ race_with_limits│  │
│  │            │ │ expr/Error │ │ Backend SPI  │ │ 契约         │ │                 │  │
│  └────────────┘ └────────────┘ └──────────────┘ └──────────────┘ └─────────────────┘  │
│         (-tools 的 Tool 契约亦属 L0 契约面，物理上随 L2 工具 crate 提供)                    │
└──────────────────────────────────────────────────────────────────────────────────────┘

工具:  xtask — 内部任务运行器 (check-arch 依赖律守卫, 非发布 crate)
```

**依赖方向**：自上而下（L4 → L3 → L2 → L1 → L0）。同层运行时之间**禁止**互相依赖；跨层只能向下，且尽量经由 L0 契约面（SPI / trait 注入）解耦。

---

## 模块详细功能说明

### L0 — 契约内核（窄腰）

> 定义稳定的类型与 trait 契约。所有上层运行时只与这里发生依赖，从而保证运行时之间彼此隔离、可独立替换。

| Crate | 职责 |
|---|---|
| **agentflow-value** | 提供 `FlowValue`（`Json` / `File` / `Url` 三种变体），是工作流状态池的类型化数据单元。让状态在节点间显式、带命名空间地流动，并支持 checkpoint 序列化时保持变体标签的类型保真。 |
| **agentflow-graph** | **Flow IR（中间表示）**：`Flow`、`AsyncNode` trait、`GraphNode`（dependencies / `input_mapping` / `run_if` / `initial_inputs`）、`NodeType::{Standard, Map, While}`、表达式引擎 `expr`、统一错误类型 `AgentFlowError`。注意 **IR ≠ 执行器**——它只描述图，不负责跑图。 |
| **agentflow-store-spi** | 存储服务提供者接口：`MemoryStore`（会话记忆抽象）+ `KnowledgeBackend`（知识检索抽象）。让 memory / rag 的具体实现可被注入而非硬编码。 |
| **agentflow-agent-spi** | 智能体契约：`AgentRuntime` trait、turn-driven（逐回合驱动）façade、`Capability` 降解（lowering）。这是 harness 等编排层依赖 agent 的窄接口，避免直接依赖 `agentflow-agents` 实现。 |
| **agentflow-async-util** | 异步原语：retry、timeout、`race_with_limits`（带并发上限的竞速）。被各层复用的可靠性工具箱。 |
| **agentflow-tools**（契约面） | `Tool` trait + `ToolRegistry` + `SandboxPolicy` / `ToolPolicy` + `ToolMetadata`（`source: Builtin/Script/Mcp/Workflow`、权限、幂等性）。内置 `FileTool` / `HttpTool` / `ShellTool`（shell 默认禁用），`ToolOutputPart::{Text,Image,Resource}` 多模态输出，以及 macOS sandbox-exec / Linux seccomp 的 OS 级沙箱后端。物理上是 L2 crate，但其 `Tool` 契约充当 L0 契约面。 |

### L1 — 执行核心

| Crate | 职责 |
|---|---|
| **agentflow-core** | **DAG 执行引擎**——真正"跑" L0 Flow IR 的执行器。拓扑排序 + `FlowExecutionMode::{Serial, Concurrent}`（基于 `FuturesUnordered` + `max_concurrency` 的依赖就绪调度）。生产级原语：retry / retry_executor、timeout、checkpoint（断点恢复）、resource_manager / resource_limits、health（K8s 健康检查）、state_monitor、events。通过 `FlowExt` trait 暴露 `flow.run()`。L0 类型在此 re-export 为 `agentflow_core::*` 以兼容。 |

### L2 — 能力适配器

| Crate | 职责 |
|---|---|
| **agentflow-nodes** | **工具层** `AsyncNode`：`template`(Tera) / `file` / `http` / `batch` / `conditional` / `arxiv` / `markmap`。只依赖 IR + `agentflow-tools`，**不携带任何能力依赖**。Feature：默认 `["http","file","template"]`，`batch` / `conditional` 可选。 |
| **agentflow-nodes-ai** | **能力层**节点适配器：`llm` / `asr` / `tts` / `text_to_image` / `image_to_image` / `image_understand` / `image_edit` / `mcp` / `rag`。依赖 `agentflow-nodes`（共享 common/error）+ 能力 crate（llm 必选；mcp/rag 经 feature 门控）。AI 模态节点不再有逐模态门控。 |
| **agentflow-llm** | LLM provider 抽象。流式 fluent API `AgentFlow::model(...).prompt(...).execute()`。6 家 provider：OpenAI / Anthropic / Google / StepFun / Moonshot / Mock（另 4 家 OpenAI 兼容厂商 GLM / DashScope / DeepSeek / MiniMax 复用 `OpenAIProvider`）。多模态（text + image url/base64）、流式、模型注册/发现、原生 `tool_calls` / `tool_choice`、W3C `traceparent` 透传。 |
| **agentflow-tools** | （见 L0 契约面）统一工具抽象与沙箱实现。 |
| **agentflow-mcp** | Model Context Protocol 集成：client + server + transport（stdio 优先）、JSON-RPC 2.0、retry/timeout/重连、延迟基准。注意 MCP→`Tool` 的适配器（`McpToolAdapter` + `McpClientPool`）住在 `agentflow-skills` 而非这里——因为 skill builder 才是知道某个 skill 声明了哪些 MCP server 的入口。 |
| **agentflow-rag** | 检索增强：文档分块、embedding（OpenAI API 或本地 ONNX）、Qdrant 向量库、检索、重排。来源 PDF/HTML/CSV/text（PDF/HTML 加载器默认 50 MiB / 10 MiB 上限）。实现 L0 `KnowledgeBackend`：`Bm25KnowledgeBackend`（内存关键词索引）+ `VectorStoreKnowledgeBackend`（向量层），并暴露 `RagSearchTool`（可注册的只读幂等 `rag_search` 工具）。Eval 框架：JSONL 数据集，Recall@K / MRR / nDCG@K + 配对符号检验，CLI `agentflow rag eval`。 |
| **agentflow-memory** | 智能体对话记忆：`MemoryStore` 实现 `SessionMemory`（token 窗口内存）+ `SqliteMemory`（持久化）。`SemanticMemory` 相似度检索（与 rag 联动）。 |

### L3 — 智能体 / 编排

| Crate | 职责 |
|---|---|
| **agentflow-agents** | Agent-native 运行时与模式：`AgentRuntime`（含 `AgentContext` / `RuntimeLimits` / `AgentCancellationToken`）；`ReActAgent`（含并行工具调用批量分发）、`PlanExecuteAgent`；`ReflectionStrategy`（`FailureReflection` / `FinalReflection` / `NoOpReflection`，观察型、不改变控制流）、`VerificationStrategy`（`AlwaysApprove` 内置；在候选最终答案*停止前*把关——`Rejected { feedback }` 把反馈写回记忆并让循环再走一轮，`max_verification_attempts` 耗尽则优雅降级为强制接受）、`MemorySummaryBackend`；混合组合 `AgentNode`（agent 嵌入 DAG）+ `WorkflowTool`（DAG 当工具）+ `AgentNodeResumeContract`（部分恢复）；多智能体 `HandoffSupervisor` / `BlackboardSupervisor` / `DebateSupervisor`。`dynamic` 模块：`compile_plan_to_flow` + `DynamicWorkflowAgent`（LLM 生成 `WorkflowPlan` 再编译执行）。 |
| **agentflow-skills** | 声明式能力包：`SKILL.md`（推荐）+ `skill.toml`（兼容）解析。`SkillBuilder` 把 persona/model/tools/knowledge/memory/mcp_servers/security 装配成可运行 agent。分层知识：每个 `[[knowledge]]` 的 `backend` 为 `files`（默认，内联进 persona）或 `rag`（建 BM25 索引 + 暴露共享 `rag_search`）。`SkillCapability` 实现 L0 `Capability` 契约（`lower()` 产出工具注册表 + persona 上下文项）。本地注册表 `skills.index.toml` + 市场目录。CLI：`init` / `install` / `list` / `inspect` / `list-tools` / `run` / `chat` / `test` / `validate` / `index` / `marketplace`。 |
| **agentflow-harness** | **Harness Agent Mode**：冻结的 `HarnessEvent`（行分隔 JSON 信封）契约 + 交互式审批协议（`ApprovalRequest` / `Decision` / `Risk` / `Scope`）+ 异步 hook trait（Pre/PostToolHook、ApprovalProvider、ContextProvider）。`HarnessRuntime` 包装任意 `AgentRuntime`；4 个默认上下文 provider（AgentsMd / TodosMd / RoadmapMd / WorkspaceLayout）+ 预算裁剪；多种事件 sink；hooks+审批管线（`wrap_registry`，pre-hook fail-closed）；并行工具调用（H3）；后台任务（H4，`TaskRuntime` + 5 个内置工具）；Flow 治理 `for_flow()` / `run_flow()`。稳定级别 **beta**。 |
| **agentflow-config** | 共享的"配置优先"工作流装配（从 CLI 抽出供 server 复用）：`config`（YAML schema `FlowDefinitionV2` / `NodeDefinitionV2`）、`executor`（`build_flow_from_yaml` + 节点工厂，feature `plugin` / `rag` / `mcp` 门控）、`diagnostics`（`agentflow doctor` 报告构建器）。CLI 与 server 共用，是 YAML `type:`→节点分发的唯一出口。 |
| **agentflow-cli** | 统一用户界面：`workflow run\|validate\|debug\|dynamic`、`config`、`llm models`、`skill *`、`mcp`、`trace replay\|tui`、`audio asr\|tts`、`image`、`rag ops\|eval`、`harness run\|run-flow\|resume\|list\|inspect`。re-export `agentflow_cli::{config, executor}` 以兼容。 |

### L4 — 运维 / 产品化

| Crate | 职责 |
|---|---|
| **agentflow-tracing** | 可观测性：`EventListener`（非侵入事件采集，按到达顺序排空避免竞态）；持久化 JSONL（默认）/ SQLite / Postgres；`trace replay` + TUI 时间线；OpenTelemetry span 模型（`OtelSpan` / `OtelSpanSink`）+ W3C trace context 传播；密钥/敏感参数脱敏；`AGENTFLOW_TRACE_DIR` / `AGENTFLOW_RUN_DIR` 存储根。首方 OTLP 传输 **暂缓**（Q2.3.3），由运维自带 `OtelSpanSink`。 |
| **agentflow-server** | 平台模式的 Axum 网关。工作流面：`/v1/runs`(POST/GET)、`/v1/runs/{id}/events`(SSE+backfill)、`/v1/skills`。Harness 面（P-H.5 已闭环）：`/v1/harness/sessions` 全套（创建/查询/取消/恢复/事件 SSE/历史/审批），生产用 `LiveHarnessExecutor`、测试用 `StubHarnessExecutor`。Bearer 认证、统一错误信封、`WorkflowEventListener`→DB 桥接。生产默认 `FlowRunExecutor` 内进程跑配置工作流。 |
| **agentflow-db** | 网关的 PostgreSQL 持久化。9 表 schema（runs / steps / events / artifacts / skill_installs / mcp_sessions + harness_sessions / harness_session_events + user_preferences），`sqlx::migrate!()`。Repository 层：`RunRepo` / `StepRepo` / `EventRepo` / `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo` / `HarnessSessionRepo` / `HarnessEventRepo` / `UserPreferenceRepo`。 |
| **agentflow-worker** | 分布式 DAG 执行的独立 worker 进程。经 gRPC 用 `WorkerProtocol` 与 server 控制面通信，拉取任务、本地执行节点 payload、回流事件并保持 `traceparent` 连续性以拼接 OTel trace。当前支持 payload 有限（template/file），扩展到 LLM/HTTP/MCP/agent 由 P2.8 跟踪。 |
| **agentflow-worker-proto** | worker 与 server 之间 gRPC 协议定义（protobuf 生成的消息类型）。被 `agentflow-worker` 和 `agentflow-server` 共同消费，作为分布式控制面的共享 wire 契约。 |
| **agentflow-ui** | React + Vite + TypeScript SPA，由 server 内嵌于 `/ui`。已实现：run 列表、DAG 状态面板、事件历史回放、实时 SSE。Harness 面：会话列表/新建/详情（`EventSource` 事件时间线、审批卡片 allow/deny/deny_and_stop × scope、取消、resume 的 rerun/append）。它是 `/v1/*` 与 SSE 契约的客户端，**绝不绕过 server API**。 |

### 工具链

| 模块 | 职责 |
|---|---|
| **xtask** | 内部任务运行器（非发布 crate）。核心命令 `check-arch`：对工作区依赖图断言 8 条 crate 依赖律中的 3 条（runtime-isolation / surface-isolation / kernel-isolation，后者 R1.2 随 L0 契约内核落地新增），任何新增越层边或过期 allowlist 条目都会让 gate 失败——这是架构隔离的自动化守卫。 |

---

## 两种执行风格如何协作

```
   ┌─────────────────────┐         AgentNode          ┌──────────────────────┐
   │   DAG 工作流          │  ◄── agent 嵌入 DAG ────    │   Agent-native 循环   │
   │ core::Flow           │                            │ agents::AgentRuntime │
   │ 拓扑/并发/checkpoint  │  ──── DAG 当工具 ──► ◄──    │ ReAct/Plan/Reflection│
   │ retry/timeout/条件   │      WorkflowTool          │ 工具调用/记忆/取消     │
   └─────────────────────┘                            └──────────────────────┘
            ▲                                                     ▲
            └────────── 配置优先 YAML (agent / skill_agent 节点) ───┘
                         由 agentflow-config 统一装配
```

- **DAG 工作流**：经 `agentflow-core::Flow`（顺序或 `FlowExecutionMode::Concurrent` 依赖就绪调度），具备显式 I/O、checkpoint、retry、timeout、条件执行。
- **Agent-native 循环**：经 `agentflow-agents::AgentRuntime`（ReAct、Plan-Execute、Reflection、Supervisor），具备结构化 `AgentStep` / `AgentEvent` / `AgentStopReason`、工具调用、记忆、取消。
- **二者组合**：`AgentNode`（agent 嵌入 DAG）与 `WorkflowTool`（DAG 暴露为 agent 工具）。配置优先 YAML 支持 `agent` / `skill_agent` 节点类型。

四种执行范式（静态 DAG / native loop / harness / dynamic workflow）及其三轴心智模型详见 `docs/ARCHITECTURE.md` § Four Execution Paradigms。
