# AgentFlow 项目深度评估报告

- 评估日期：2026-05-01
- 评估范围：workspace 全部 16 个 crate、`docs/` 设计文档、CLI 执行路径、agent runtime、DAG 调度器、产品化闭环
- 与上一版报告 (`OVERALL_EVALUATION_REPORT.md`，2026-04-28) 的关系：本报告基于 `main` 分支最新代码 (HEAD `41ed3f8`) 重新校核全部模块，纠正若干已经过时的结论（例如 DAG 并发调度、`agent`/`skill_agent` YAML 节点、`workflow run` flags），并将"机制设计 + 优化方案"作为重点重写
- 编译验证：依据上次官方记录 `cargo check --workspace --all-targets` 通过、479 测试 100% 通过；本报告未重跑全测

---

## 0. TL;DR

AgentFlow 已经从单一 DAG 引擎演进为 **"DAG 工作流 + agent-native runtime + 工具/Skill/MCP/Memory/RAG/Tracing 支撑层"** 的模块化 Rust 框架。两条主路径（确定性 DAG、自主智能体循环）都有可工作实现并能在同一 CLI、同一 ToolRegistry、同一 Trace 体系下混合编排（`AgentNode` × `WorkflowTool`），具备框架级骨架。

| 维度 | 评级 | 一句话判断 |
| --- | --- | --- |
| 架构清晰度 | A- | 分层、职责、扩展点都合理；少量模块边界仍重叠（MCP 工具适配、tool 权限执行）|
| DAG 内核成熟度 | A- | 拓扑、并发、map/while、checkpoint、事件、retry/timeout/health 全部齐备 |
| Agent-native SDK 成熟度 | B+ | ReAct + Plan-Execute + Reflection + Memory + 工具 + 取消/预算约束都已落实 |
| Config-first（YAML/CLI）成熟度 | B | `agent`/`skill_agent` 节点、`workflow run` flags、`skill init/run/test`、trace replay 都已落地，但 schema 校验和错误经验仍需收紧 |
| 生产可观测性 | B+ | OTel exporter、Trace 持久化、replay、TUI、redaction 链路完整 |
| 服务端平台化 | C- | `agentflow-server` 仅 130 行、`agentflow-db` 仅 48 行，仍是骨架 |
| 综合 | **B+** | 框架级骨架已具备；下一阶段重点是平台化（server/db）、表达式/调度精化、工具调用与原生 function-calling 收敛 |

**项目可以同时支持 DAG 传统智能体与 agent-native 自主智能体**，并已具备两者的混合编排，结论与项目目标一致。

---

## 1. 项目全景

### 1.1 Workspace 结构（16 crate，14 活跃）

| 层 | Crate | 角色 | 成熟度 | LOC | 测试数 |
| --- | --- | --- | --- | --- | --- |
| **执行内核** | `agentflow-core` | DAG 执行内核、节点抽象、FlowValue、checkpoint、retry、timeout、健康检查、事件 | ⭐⭐⭐ | ~9.3K | 58 |
| **能力适配** | `agentflow-nodes` | 内置节点库（LLM/HTTP/File/Template/Map/While/RAG/MCP/多模态等 16+ 类型）| ⭐⭐⭐ | ~5.1K | 15 |
| **能力适配** | `agentflow-llm` | 多供应商 LLM 客户端、流式、多模态、模型注册/发现 | ⭐⭐⭐ | ~8.5K | 39 |
| **能力适配** | `agentflow-tools` | 统一 Tool trait、注册表、sandbox、内置 file/http/shell 工具 | ⭐⭐ | ~1.7K | 6 |
| **能力适配** | `agentflow-mcp` | MCP client/server/transport/protocol，stdio 优先 | ⭐⭐⭐ | ~6.3K | 87 |
| **能力适配** | `agentflow-rag` | 文档解析、embedding、Qdrant、检索、rerank | ⭐⭐ | ~6.8K | 79 |
| **能力适配** | `agentflow-memory` | Session/SQLite/Semantic memory | ⭐⭐ | ~1.4K | 6 |
| **智能体/编排** | `agentflow-agents` | ReAct、Plan-Execute、Supervisor、AgentNode、WorkflowTool、Reflection | ⭐⭐⭐ | ~5.9K | 38 |
| **智能体/编排** | `agentflow-skills` | SKILL.md/skill.toml 解析、Marketplace、SkillBuilder、MCP adapter | ⭐⭐⭐ | ~3.4K | 39 |
| **智能体/编排** | `agentflow-cli` | 统一命令入口（workflow/skill/llm/image/audio/mcp/trace/rag/config）| ⭐⭐⭐ | ~6.3K | 5+ |
| **运维/产品化** | `agentflow-tracing` | Trace 采集、redaction、replay、TUI、OTel、SQLite/Postgres 持久化 | ⭐⭐⭐ | ~4.2K | 22 |
| **运维/产品化** | `agentflow-viz` | DAG → Mermaid/DOT/JSON 可视化 | ⭐⭐ | ~1.8K | 26 |
| **运维/产品化** | `agentflow-server` | Axum 网关，目前主要是 health/live/ready | ⭐ scaffold | 130 | 0 |
| **运维/产品化** | `agentflow-db` | PostgreSQL 连接 (sqlx) | ⭐ scaffold | 48 | 0 |

> CLAUDE.md 里描述的"3 个核心 crate"早已不是现状；下次更新时建议改为 14+2 crate 的真实分层。

### 1.2 推荐的四层心智模型

```
+----------------------------------------------------+
| L4 运维/产品化 | tracing · viz · server · db        |
+----------------------------------------------------+
| L3 智能体/编排 | agents · skills · cli              |
+----------------------------------------------------+
| L2 能力适配    | nodes · llm · tools · mcp · rag · memory |
+----------------------------------------------------+
| L1 执行内核    | core (Flow / GraphNode / FlowValue) |
+----------------------------------------------------+
```

- **L1 是唯一执行核**：`Flow::execute_*` 拥有节点状态池、拓扑、并发、checkpoint、事件
- **L2 全部以 `AsyncNode` 或工具/客户端形态被 L3 使用**，L1 不直接依赖任何外部能力
- **L3 是双轨入口**：`agentflow-agents` 承载 agent-native，`agentflow-nodes` + `agentflow-cli` 承载 DAG，二者通过 `AgentNode`/`WorkflowTool` 互通
- **L4 是横切面**：`tracing` 通过 `EventListener` 非侵入接入 L1，`server`/`db` 暴露平台化 API（仍待补齐）

---

## 2. 机制设计深度评估

### 2.1 DAG 执行模型（成熟）

**核心抽象（`agentflow-core/src/flow.rs:534`、`scheduler.rs`）：**

```rust
pub enum NodeType {
  Standard(Arc<dyn AsyncNode>),
  Map { template: Box<GraphNode>, parallel: bool },
  While { condition: String, max_iterations: usize, template: Box<GraphNode> },
}

pub struct GraphNode {
  pub node_type: NodeType,
  pub dependencies: Vec<String>,
  pub input_mapping: Option<HashMap<String, String>>,   // {{ nodes.X.outputs.Y }}
  pub run_if: Option<String>,                            // 条件执行
  pub initial_inputs: HashMap<String, FlowValue>,
}

pub enum FlowExecutionMode { Serial, Concurrent }
pub struct FlowExecutionConfig { mode, max_concurrency, fail_fast, continue_on_skip, run_base_dir }
```

**亮点：**

1. **显式 I/O 模型**：`FlowValue { Json | File | Url }` 解决跨节点传递大对象的内存爆炸（`docs/ARCHITECTURE.md`）。配合 `input_mapping` 模板语法（`{{ nodes.X.outputs.Y }}`），状态池是 namespaced 的 `HashMap<NodeId, HashMap<OutputName, FlowValue>>`
2. **真正的依赖就绪并发调度**：`execute_concurrently`（`flow.rs:534-720`）使用 `FuturesUnordered` + `max_concurrency` 滑动窗口 + 拓扑预排序，对所有依赖均已就绪的节点滚动派发——这并非简单 topo 串行，而是基于"前置都 Ok 或 NodeSkipped 即可启动"的 ready-set 调度。**上一版评估报告"DAG 层没有基于依赖就绪的通用并发调度"已不成立**
3. **三类节点形态**：`Standard`（单步异步）、`Map`（fan-out，可串可并）、`While`（条件循环）。Map 并行模式通过 `tokio::spawn` 子 Flow，避免阻塞主循环
4. **持久化语义清晰**：每节点完成后写 `N_outputs.json` + `state_after_N.json`；checkpoint 启用时还会落 `CheckpointManager`；恢复路径通过 `skip_until` + `restored_state_pool` 复用先前完成的节点
5. **故障域可控**：retry / timeout / resource_manager / health / state_monitor 模块独立，可被节点装饰

**机制级不足：**

| 问题 | 现状 | 影响 |
| --- | --- | --- |
| 表达式引擎弱 | `run_if` / `while.condition` 是字符串路径或简单比较 | 复杂分支只能借助 LLM/Template 节点，工程化分支决策不便 |
| FlowValue 序列化损耗 | `state_after_N.json` 对 `FlowValue::File`/`Url` 在 checkpoint 恢复时 round-trip 不完整（路线图 N7 已记入) | 失败重启后类型可能退化为 Json 字符串 |
| `run_dir` 默认依赖 home 目录 | 已通过 `--run-dir` / `AGENTFLOW_RUN_DIR` 缓解 | 多租户服务端嵌入仍需要程序化 API |
| 子 Flow 失败语义 | `execute_concurrently` 对单节点失败默认 fail-fast；Map 子 Flow 失败可选继续 | 缺少"失败重试到 N 次再放弃"的细粒度策略组合 |
| 节点依赖隐式自动推导 | 必须显式声明 `dependencies` | 表达力可控但模板写起来啰嗦；可考虑"输入引用即依赖"自动推导 |

### 2.2 Agent-native Runtime 机制（接近 production-ready）

**核心抽象（`agentflow-agents/src/runtime.rs`）：**

```rust
pub struct AgentContext {
  pub session_id: String,
  pub input: String,
  pub model: String,
  pub persona: Option<String>,
  pub skill_name: Option<String>,
  pub limits: RuntimeLimits,           // max_steps, max_tool_calls, timeout_ms, token_budget
  pub cancellation_token: Option<AgentCancellationToken>,
}

pub enum AgentStepKind { Observe, Plan, ToolCall, ToolResult, Reflect, FinalAnswer }
pub enum AgentEvent { RunStarted, StepStarted, StepCompleted, ToolCallStarted, ToolPolicyDecision,
                      ToolCallCompleted, ReflectionAdded, RunStopped }
pub enum AgentStopReason { FinalAnswer, StopCondition, MaxSteps, MaxToolCalls,
                           Timeout, Cancelled, TokenBudgetExceeded, Error }

#[async_trait]
pub trait AgentRuntime {
  async fn run(&mut self, ctx: AgentContext) -> Result<AgentRunResult, AgentRuntimeError>;
  fn runtime_name(&self) -> &'static str;
}
```

**亮点：**

1. **Step/Event 双轨记录**：`AgentStep` 是结构化推理痕迹，`AgentEvent` 是观测时间线；`AgentRunResult` 同时返回二者，对 trace replay 和回放训练数据非常友好
2. **停止原因显式枚举**：8 种 `AgentStopReason` 让"为什么停了"在工程上可观测、可测试，远比"最大轮数 / 错误"二元结构信息量大
3. **Runtime 限制**：`max_steps`、`max_tool_calls`、`timeout_ms`、`token_budget` 同时存在，`react_defaults()` 提供合理默认（15 步、50K token）
4. **可插拔反思策略**：`ReflectionStrategy` trait（`FailureReflection` / `FinalReflection` / `NoOpReflection`）从 ReAct 主循环中外提，可扩展也可关闭
5. **可插拔 memory summary**：`MemorySummaryBackend` trait（`RecentOnlyMemorySummary` / `CompactMemorySummary`）让上下文裁剪策略和 LLM 摘要可替换
6. **取消令牌**：`AgentCancellationToken` 基于 `AtomicBool + Notify`，长循环和长工具调用都可中止
7. **三种主流模式已就位**：`ReActAgent`（1972 行）、`PlanExecuteAgent`（797 行）、`Supervisor`（多智能体协作雏形）

**机制级不足：**

| 问题 | 现状 | 影响 |
| --- | --- | --- |
| ReAct 主路径未统一通过 `AgentRuntime` trait | `ReActAgent::run(prompt)` 是公共 API；`AgentRuntime::run(ctx)` 主要用于 `AgentNode`、跨语言/跨 runtime 互操作能力受限 | trait 生态难以扩展第三方 runtime |
| Tool 调用未走 LLM 原生 function-calling | 仍以 prompt 注入工具描述，由 ReAct 解析器抽取 `Action:` / `ActionInput:` | 部分 model（GPT-4o tools / Claude tool_use）可达性差，token 成本高、稳健性弱 |
| Partial resume 仍限制未完成 tool call | `AgentNodeResumeContract` 拒绝隐式重放 | 严格但当前代价较高；对 idempotent tool 没有自动 fast-path |
| 多智能体协作 | `supervisor/` 为雏形，代码量较少 | Agent-as-Tool / Workflow-as-Tool 已可手工组合，但 swarm/handoff/共享 blackboard 模型尚未沉淀 |
| Token 计数估算 | 现实现是粗粒度估算（按 4 字符≈1 token） | 长上下文准确度不够，跨 provider 不一致 |

### 2.3 工具与权限模型（方向正确，强制力中等）

**抽象（`agentflow-tools/src/tool.rs`、`policy.rs`、`sandbox.rs`）：**

- 统一 `Tool` trait + JSON Schema 参数 + `ToolOutput { ToolOutputPart::{ Text | Image | Resource } }`
- `ToolMetadata` 携带 `source: ToolSource::{ Builtin | Script | Mcp | Workflow }`、permissions、原始 server/tool 名
- `SandboxPolicy` 声明 allowed paths/domains/timeout
- `ToolPolicy` 输出 allow/deny 决策并产出 `ToolPolicyDecision` 事件

**亮点：**

- 四类来源统一，trace 里能看到"这次工具调用从哪个 MCP server 解析出来、命中了哪条 policy 规则"
- `ToolOutputPart` 已支持图像、资源类型化输出，对多模态 agent 是重要支撑

**不足：**

- **权限是声明式过滤为主**，强 enforcement（路径白名单、网络白名单、子进程沙箱）依赖具体工具实现，没有进程级 jail（如 Linux seccomp、macOS sandbox-exec）
- 内置 `ShellTool` 已默认注释关闭——安全立场正确，但缺少"显式 opt-in + 受限 shell 子集"的官方推荐路径
- 工具调用审计与 trace 已串联，但**幂等性元数据**（`Idempotent | NonIdempotent | Unknown`）尚未进入 metadata，partial resume 只能保守拒绝

### 2.4 LLM 抽象（成熟，工具调用待统一）

`AgentFlow::model("gpt-4o").prompt(...).execute()` 流式 Builder 已稳定：

- 6 个 provider：OpenAI / Anthropic / Google / StepFun / Moonshot / Mock
- 多模态 `MultimodalMessage`（文本 + image url/base64）
- 流式 `StreamingResponse`
- 模型注册和能力描述（`ModelCapabilities`、`ModelType`）
- StepFun 专用图像/音频 API 完整覆盖

**待办：** `tool_calls` 字段在抽象层尚未一等公民化——目前 ReAct/Plan-Execute 仍在 prompt 层做工具调用协议，没有用 OpenAI tools array 或 Anthropic tool_use blocks 让模型原生输出。`agentflow-tools::ToolRegistry` 已暴露 `as_openai_tools()` 便利函数，差最后一步：把 LLM 抽象层的请求/响应类型扩展成 tool calls 一等公民，并在 ReAct 中替换字符串解析路径。

### 2.5 Skills 体系（架构正确，权限统一仍可加强）

- `SKILL.md` + frontmatter 是推荐入口，`skill.toml` 兼容（同目录共存时以 toml 优先）
- `SkillBuilder` 把 manifest（persona / model / tools / knowledge / memory / mcp_servers / security）一次性装配为可运行 ReAct agent
- Marketplace 雏形 + 本地 `skills.index.toml` 已经能跑，CLI 有 `skill install/list/inspect/list-tools/run/chat/test/validate`
- 与 `agent`/`skill_agent` YAML 节点打通，使得 DAG 中可以直接声明一个由某个 Skill 驱动的 agent 节点

**机制级建议：** Skill 的 `security` 部分目前主要影响 sandbox 与允许的工具集合，但**与 `agentflow-tools::ToolPolicy` 的合并优先级**（Skill 声明 vs CLI flag vs 全局 policy）需要在 docs 中固化为一份决策表，避免运行时"哪条规则赢了"不直观。

### 2.6 Tracing/Recovery（链路完整）

- `Collector` + `EventListener` 非侵入采集 workflow/agent/tool/MCP 事件
- 持久化：JSONL（默认）或 SQLite/Postgres（feature gate）
- `agentflow trace replay <run_id>` + TUI timeline
- OpenTelemetry exporter 把同一 trace 输出到 OTLP
- Redaction 默认遮蔽 API key / env secret / 工具参数中的敏感字段
- `AGENTFLOW_TRACE_DIR` / `AGENTFLOW_RUN_DIR` 让嵌入式与 CI 不再依赖 home 目录

**待办：** 跨 DAG / Agent / Tool 的 `run_id` / `trace_id` / `parent_span_id` 传播链路已覆盖主要路径，但 LLM provider 调用层（`agentflow-llm`）尚未把当前 span 上下文统一注入到 HTTP headers 或日志，OTel 端到端串联在 LLM 这一跳容易断。

---

## 3. 双轨能力——DAG vs Agent-native vs Hybrid

### 3.1 传统 DAG 智能体（已经满足）

| 需求 | 当前状态 |
| --- | --- |
| 显式节点依赖 + 拓扑排序 | ✅ |
| 标准节点 / Map / While | ✅ |
| 条件执行 (`run_if`) | ✅（表达式弱）|
| 显式输入映射 + namespaced state pool | ✅ |
| 依赖就绪并发调度 | ✅ `FlowExecutionMode::Concurrent` |
| Checkpoint + Resume | ✅（FlowValue::File/Url roundtrip 待修）|
| 事件 / Trace / Replay | ✅ |
| YAML 配置 + dry-run + timeout + retry | ✅（N6 闭环）|
| DAG 可视化 | ✅ Mermaid/DOT/JSON |
| 内置节点库 | ✅ 16+ 类型 |

**结论：** 对**确定性流程、批处理、RAG pipeline、工具链编排、多步骤业务自动化**等场景，DAG 模式已经达到生产可用。短板主要在表达式语言和 FlowValue 序列化一致性。

### 3.2 Agent-native 智能体（已经满足主路径）

| 需求 | 当前状态 |
| --- | --- |
| 自主循环：Observe → Plan → Act → Reflect | ✅ ReAct |
| 计划-执行模式 | ✅ PlanExecuteAgent |
| 工具调用（统一注册） | ✅ ToolRegistry |
| 可插拔记忆（短期/长期/语义） | ✅ Session/Sqlite/Semantic |
| 反思策略 | ✅ FailureReflection / FinalReflection |
| 取消 / 超时 / 步数 / token 预算 | ✅ RuntimeLimits + CancellationToken |
| 结构化 trace + 8 种停止原因 | ✅ |
| 多智能体协作 | 🟡 Supervisor 雏形 |
| LLM 原生 function calling | 🟡 仍走 prompt 协议 |
| Config-first agent (YAML) | ✅ `skill_agent` / `agent` 节点 |
| Skill manifest → agent | ✅ SkillBuilder |
| 与 DAG 互通 (AgentNode / WorkflowTool) | ✅ |

**结论：** 对**研究助理、代码助手、调研型任务、半自主操作**等 agent-native 场景，SDK-first 已经可用，并通过 Skills 提供了 Config-first 入口。最大三个改进点是：(a) LLM 原生 tool calling 一等公民化，(b) 多智能体协作（handoff、blackboard、role-play、协作合约）沉淀范式，(c) Token 预算的精确化。

### 3.3 混合编排（已经满足）

```
DAG --[node]--> AgentNode --[loop]--> Tool --[invoke]--> WorkflowTool --[child DAG]--> ...
```

`AgentNode`（`agentflow-agents/src/nodes/agent_node.rs`）让 ReAct 嵌入 DAG 一节点；`WorkflowTool`（`agentflow-agents/src/tools/workflow_tool.rs`）让 DAG 暴露给 agent 调用，受 timeout 约束并接入 trace。`AgentNodeResumeContract` 给出 partial resume 合约。

**唯一仍需持续打磨：** AgentNode partial 失败后的 checkpoint 与 idempotent tool 自动重放策略（已记入 N7 路线图）。

---

## 4. 模块逐项评估

### 4.1 agentflow-core ⭐⭐⭐

- **职责**：DAG 执行内核、AsyncNode 抽象、FlowValue、scheduler、checkpoint、retry、timeout、resource、health、events
- **完备性**：高
- **不足**：表达式引擎过弱；FlowValue::File/Url checkpoint roundtrip；run_dir 默认依赖 home

### 4.2 agentflow-nodes ⭐⭐⭐

- **职责**：内置节点库（LLM/HTTP/File/Template/Map/While/Conditional/Batch/RAG/MCP/Arxiv/MarkMap/ASR/TTS/Text-to-image/Image-to-image/Image-edit/Image-understand）
- **完备性**：覆盖面广，feature gate 管理可选能力
- **不足**：节点参数 schema 不统一；缺少节点级 mock 框架，离线测试隔离弱；错误码标准化未完成

### 4.3 agentflow-llm ⭐⭐⭐

- **职责**：多 provider LLM 抽象 + 多模态 + 流式 + 模型注册/发现/校验
- **完备性**：高
- **不足**：tool calling 未一等公民化；配置默认依赖用户目录，多租户服务端注入仍粗放；token 计数粗粒度

### 4.4 agentflow-agents ⭐⭐⭐

- **职责**：agent-native runtime（ReAct/Plan-Execute/Reflection/Supervisor）+ AgentNode + WorkflowTool + 公共工具（PDF/批处理）
- **完备性**：高
- **不足**：`AgentRuntime` trait 与具体 agent 公共 API 双轨；多智能体协作仅雏形；resume 严格但保守

### 4.5 agentflow-tools ⭐⭐

- **职责**：Tool trait + Registry + Sandbox + Policy + 内置工具
- **完备性**：方向正确
- **不足**：权限以声明/过滤为主，缺进程级强 enforcement；缺工具幂等性元数据；缺统一审计 schema 与 tracing 强绑定

### 4.6 agentflow-mcp ⭐⭐⭐

- **职责**：MCP client / server / transport / protocol，stdio 优先
- **完备性**：client 完整可用，retry/timeout/重连测试齐
- **不足**：`client_old` 历史包袱仍在；server 标 experimental；与 Skills/Tools/Nodes 的权限继承尚未跨模块统一

### 4.7 agentflow-skills ⭐⭐⭐

- **职责**：SKILL.md/skill.toml 解析、SkillLoader、SkillBuilder、Marketplace、MCP tool adapter、本地 registry
- **完备性**：高
- **不足**：Skill `security` 与 `agentflow-tools::ToolPolicy`、CLI flag 三方决策优先级缺权威表

### 4.8 agentflow-memory ⭐⭐

- **职责**：MemoryStore 抽象 + Session/Sqlite/Semantic 三个实现
- **完备性**：基础够用
- **不足**：长期记忆 schema、隐私/清理策略、检索质量评估、跨 session 关联策略均较初级

### 4.9 agentflow-rag ⭐⭐

- **职责**：document → chunk → embed → vectorstore → retrieval → rerank
- **完备性**：模块全
- **不足**：版本仍 0.3.0-alpha；缺端到端召回/精排评测 harness；index 配置模板过于轻量；与 MemoryStore.semantic 的边界稍模糊

### 4.10 agentflow-cli ⭐⭐⭐

- **职责**：所有功能的统一入口，覆盖 workflow / config / llm / image / audio / mcp / skill / trace / rag
- **完备性**：N6 闭环完成后，`workflow run` flags、`agent`/`skill_agent` 节点、trace replay 已串通
- **不足**：YAML 节点参数错误对人类读者仍偏技术化；机器可读 JSON 与人类可读输出共存的 contract 文档未集中化

### 4.11 agentflow-tracing ⭐⭐⭐

- **职责**：采集 / 持久化 / replay / TUI / OTel / redaction / schema
- **完备性**：高
- **不足**：LLM provider 调用层尚未承担 OTel context 注入，跨 LLM hop 易断；TUI 仍是基础 timeline，复杂 hybrid 视图未做

### 4.12 agentflow-viz ⭐⭐

- **职责**：YAML → VisualGraph → Mermaid/DOT/JSON
- **完备性**：静态可视化够用
- **不足**：未与 trace 实时状态、checkpoint 进度联动，无法在调试 UI 中看到"当前 DAG 跑到哪个节点"

### 4.13 agentflow-server ⭐ scaffold

- **职责**：Axum 网关
- **现状**：130 行，仅 health/live/ready；无 run / agent / skill / trace 管理 API
- **缺口**：业务路由、AuthN/AuthZ、租户隔离、API 版本化、SSE/WebSocket 流式、与 agentflow-db 的仓储协作

### 4.14 agentflow-db ⭐ scaffold

- **职责**：Postgres 连接管理
- **现状**：48 行，仅 pool 初始化；无 schema / migration / repository 层
- **缺口**：核心实体（run / step / event / artifact / skill_install / mcp_session）schema、migration（refinery/sqlx-migrate）、Repository trait

### 4.15 跨 workspace 一致性

- `agentflow-server`、`agentflow-db` 使用 Rust **2024 edition**，其他 crate 多为 **2021**——风格未统一
- `agentflow-rag` 处于 0.3.0-alpha，`agentflow-mcp` 处于 0.1.0-alpha，应在下一次发布周期里把版本/稳定性策略写入 README

---

## 5. 风险盘点

| # | 风险 | 严重性 | 触发场景 |
| --- | --- | --- | --- |
| R1 | FlowValue::File/Url checkpoint roundtrip 损失类型 | 中 | 含多模态/文件输出的工作流失败重启 |
| R2 | LLM 工具调用走 prompt 解析，跨 provider 稳健性差 | 中 | 切换到 Claude/GPT-4o 原生 tool_use 时输出格式不齐 |
| R3 | `agentflow-server` / `agentflow-db` 仍是骨架 | 高（产品化）| 想以平台/SaaS 形态部署 |
| R4 | 多智能体协作仅雏形 | 中 | 需要 swarm / handoff / 协作合约 |
| R5 | 权限是过滤型，缺进程级 jail | 中 | shell/script tool 在不可信环境运行 |
| R6 | OTel context 跨 LLM hop 断裂 | 低 | 端到端 distributed tracing |
| R7 | RAG 缺评测 harness | 中 | 知识库迭代/调参缺反馈回路 |
| R8 | YAML schema 错误经验 | 低 | 用户编写复杂 workflow 时 |
| R9 | workspace edition 不统一 | 低 | 升级 Rust 工具链时 |

---

## 6. 优化路线（基于现状）

> 与项目已有 `RoadMap.md` (N1–N7) 的关系：以下建议在 N1–N7 已完成的基础上，把视角换到"框架级 v1.0 候选"该补的能力。

### 6.1 P0（建议下一次发布前必做）

1. **平台化最小骨架**——把 `agentflow-server` 推到可用：
   - 提供 `POST /v1/runs`、`GET /v1/runs/{id}`、`GET /v1/runs/{id}/events`（SSE）、`POST /v1/skills/{name}:run`
   - `agentflow-db` 落实 run/step/event/artifact 4 张表的 schema + migration（refinery 或 sqlx::migrate）
   - 与 `agentflow-tracing` 的 Postgres 后端复用同一 schema，避免双写

2. **LLM 原生 tool calling 一等公民化**：
   - 在 `agentflow-llm` 抽象层把 `tool_calls` / `tool_choice` 加入请求/响应类型
   - 在 `ReActAgent` 与 `PlanExecuteAgent` 中替换 prompt 解析路径为：
     - 优先走 provider 原生（OpenAI tools、Anthropic tool_use、Google function declarations）
     - 不支持的 provider 自动降级到 prompt 解析路径
   - 暴露 `Tool` 幂等性元数据 (`Idempotent | NonIdempotent | Unknown`)，让 partial resume 在 Idempotent 时自动重放

3. **FlowValue checkpoint 类型保真**：
   - 修复 `state_after_N.json` 对 `FlowValue::File`/`Url` 的 round-trip
   - 增加 property test 强约束：`from_json(to_json(v)) == v`

4. **表达式引擎升级**：
   - 引入轻量表达式（如 `evalexpr` 或自研 PEG），支持 `nodes.X.outputs.Y > 0 && len(nodes.Z.outputs.items) > 5`、字符串包含、null 检查
   - 取代 `run_if` / `while.condition` 当前的简单解析

### 6.2 P1（v1.0 候选阶段）

5. **多智能体协作范式**：
   - 在 `agentflow-agents/supervisor` 沉淀三种范式：handoff（角色切换）、blackboard（共享白板）、debate（多 agent 投票/批判）
   - 给出权威示例：研究 + 写作 + 评审三 agent 协作

6. **工具沙箱强化**：
   - 在 `ShellTool`/`ScriptTool` 上接入 macOS sandbox-exec / Linux seccomp + chroot 子集
   - `Tool` 增加 `requires_capabilities()`（fs.read / fs.write / net / exec）和 `effective_capabilities`（被 SkillSecurity / ToolPolicy / CLI flag 三方裁剪后的最终权限）
   - 在 trace 中固化 capability 决策事件

7. **OTel 端到端连续性**：
   - `agentflow-llm` 客户端在 HTTP 请求里注入 `traceparent` header（即使 LLM 提供商不解析，本地 hop 也能被 OTel 拼起来）
   - 统一 `WorkflowRunId / AgentSessionId / ToolCallId` 的属性命名

8. **RAG 评测 harness**：
   - `agentflow-rag/eval/`：标注集 + Recall@K / MRR / nDCG 指标 + 对比 baseline
   - CLI 子命令 `agentflow rag eval <dataset>`

9. **Skill 权限决策表**：
   - `docs/SKILL_PERMISSIONS.md` 把 SkillSecurity vs ToolPolicy vs CLI flag 的合并算法写成正式表
   - `agentflow skill inspect --explain-permissions` 展示一次实际运行的最终决策路径

### 6.3 P2（中期演进）

10. **Plugin / Custom Node 体系**：
    - 现在 `agentflow-nodes` 是固定集合；引入 `dyn AsyncNode` 的动态加载（dlopen/abi_stable）或 WASM（wasmtime/wasmer），让第三方节点不修改主仓库即可分发
    - Skill marketplace 可承担一部分分发能力

11. **分布式调度**：
    - `Flow::execute_concurrently` 是单进程并发；引入 worker 抽象（gRPC/NATS/Redis Streams 任选其一），让大型 DAG 可分布式执行
    - 关键决策：是否把 `agentflow-server` 进化为 control plane

12. **Web UI / 调试器**：
    - 在 `agentflow-viz` 之上叠加 React/Svelte SPA，连接 `agentflow-server` SSE，看 DAG 实时跑
    - TUI 已经够用，但混合 (DAG × Agent × Tool) 视图不易在终端表达

13. **Agent SDK 文档化**：
    - 写 `docs/AGENT_SDK.md`，把 `AgentRuntime` trait、扩展自定义 Step/Event、自定义 ReflectionStrategy/MemorySummaryBackend 都做成"五分钟入门"教程
    - 现在示例已多但散在 `examples/`

### 6.4 文档与一致性维护

14. **更新 CLAUDE.md**：把"3 crate"改为"14+2 crate"四层结构；把 N6/N7 已完成项整理进"已完成"，避免老旧描述误导
15. **统一 workspace edition**：把 `agentflow-server` / `agentflow-db` 回退或把其他 crate 升到 2024
16. **README 与 docs/README 单一来源**：当前 docs/ 32+ 文件信息密度高，需要一个 doc map 或 SUMMARY

---

## 7. 推荐发布节奏

| 里程碑 | 目标 | 主要构成 |
| --- | --- | --- |
| v0.3.0 | 平台骨架 + tool calling 一等公民 + checkpoint 保真 | P0 全部 4 项 |
| v0.4.0 | 协作范式 + 沙箱强化 + OTel 端到端 + RAG eval | P1 5 项 |
| v1.0.0-rc | 插件体系（动态/WASM）+ Web UI + 完整文档体系 | P2 全部 |

---

## 8. 最终结论

AgentFlow 的核心命题——**同时支持 DAG 工作流和 agent-native 自主智能体，并允许两者在同一执行上下文里混合编排**——已经在代码层面成立，并具备产品化雏形：

- DAG 内核成熟到可生产
- Agent-native runtime 在 ReAct/Plan-Execute/Reflection/Memory/Tools/MCP 这些核心能力上完整
- Skills + CLI + Trace 让"非 Rust 用户用 YAML 配置即可跑混合智能体"这条路径已经可走

下一阶段的关键升级方向是 **(a) 平台化（server/db）真正落地、(b) LLM 原生 tool calling 一等公民、(c) 表达式与 checkpoint 一致性、(d) 多智能体协作范式与工具沙箱强化**——做完这四件事，AgentFlow 就能从"框架级骨架"过渡到"框架级 v1.0 候选"。

> 评估签名：HEAD `41ed3f8` (2026-05-01)
> 主要参考：`agentflow-core/src/{flow,scheduler,value}.rs`、`agentflow-agents/src/{runtime,react/agent,plan_execute,reflection}.rs`、`agentflow-cli/src/{config/schema,executor/factory}.rs`、`docs/`、`RoadMap.md`、`CLAUDE.md`
