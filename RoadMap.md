# AgentFlow 智能体框架 RoadMap

最后更新: 2026-05-02

## 目标定位

AgentFlow 的下一阶段目标是从工作流编排项目演进为一个同时支持固定 DAG 工作流和 agent-native 自主循环的智能体框架，并在此基础上推进到平台级 v1.0 候选。

核心方向:

- 支持确定性的 DAG 式工作流，适合生产流程、批处理、RAG pipeline、多步骤业务自动化。
- 支持 agent-native 执行模式，包含计划、观察、工具调用、反思、记忆、恢复和多轮决策。
- 将 Skills、Tools、MCP、Memory、Tracing、Runtime 统一为稳定的底层能力。
- 保持 Rust 项目继续演进，不另起新项目，优先复用当前 crates 的基础能力。
- README 已更新为 DAG workflow + agent-native runtime 的智能体框架定位。✅
- 推进平台化（server/db）、LLM 原生 tool calling、表达式与 checkpoint 一致性、多智能体协作四条 v1.0 候选主线。

## 架构原则

1. 工作流和智能体共享底座
   - DAG workflow 和 agent loop 都应复用同一套 ToolRegistry、SkillLoader、MCP Client、Memory Store、Trace Collector。
   - 差异只体现在执行策略，而不是工具、状态、观测、错误处理各自重做。

2. Skills 是能力包，不是单纯 prompt
   - Skill 应包含说明、工具声明、MCP server 配置、知识文件、脚本、沙箱策略和运行约束。
   - `SKILL.md` 作为人类可读标准入口。
   - `skill.toml` 暂时保留为结构化兼容格式，不建议短期移除。

3. Tools 是统一执行接口
   - 内置工具、脚本工具、MCP 工具、未来插件工具都适配到统一 Tool trait。
   - Tool metadata 需要保留 name、description、input schema、output schema、权限和来源信息。

4. Runtime 分层
   - Core Runtime: 状态、节点、DAG、错误、重试、超时、检查点。
   - Tool Runtime: 工具注册、调用、沙箱、MCP 适配。
   - Agent Runtime: 计划、观察、动作、反思、记忆、停止条件。
   - App/CLI Runtime: 配置加载、命令入口、交互式运行。

## 当前闭环结论

`TODOs.md` 中记录的短期主线已完成: Skills + MCP、Agent Runtime MVP、DAG + Agent hybrid、trace 串联、checkpoint resume、示例、文档和 CI 质量门禁均已有代码或文档落点。

旧版 v1.0 生产级计划已从 `TODO.md` 收敛进本路线图；短期执行只维护本文件和本地 `TODOs.md`。

2026-04-28 评估:

- `OVERALL_EVALUATION_REPORT.md` 完成，DAG code-first 已较成熟；agent-native SDK 具备主要骨架；CLI/config-first 产品化是当时最大短板。
- 由此引入 N6/N7，把已有 runtime 能力转化为用户可以稳定使用的 CLI 入口。

2026-05-01 复评:

- `PROJECT_EVALUATION_2026-05-01.md` 完成，HEAD `41ed3f8`，14+2 crate workspace。
- N1–N7 路线全部完成: agent runtime 生产化、observability/replay、security/tool governance、Skill CLI、CI 质量门禁、CLI 产品化、统一 trace/recovery/示例集。
- 综合评级 **B+**: 架构 A-、DAG 内核 A-、agent-native B+、CLI/config-first B、可观测性 B+、平台化 C-。
- DAG 已支持依赖就绪并发调度 (`FlowExecutionMode::Concurrent`)，YAML 支持 `agent`/`skill_agent` 节点，CLI 工作流参数全部兑现，trace/replay/redaction 链路完整。
- 下一阶段主要差距由"CLI 产品化"转向"平台化（server/db）+ LLM 原生 tool calling + 表达式/checkpoint 保真 + 多智能体协作 + 工具沙箱强化"。
- 因此路线图新增 Phase 7 / Phase 8，按 v0.3.0 → v0.4.0 → v1.0.0-rc 三阶段推进。

## 下一阶段开发需求

### N1: Runtime 生产化

- 增加 agent run cancellation / shutdown boundary，保证长循环和长工具调用可控退出。✅
- 将 memory budget 从 prompt 裁剪推进到可插拔 summary backend，支持 LLM summary、规则 summary 和持久化 summary。✅
- 增加 Plan-and-Execute runtime 原型，复用现有 `AgentRuntime`、`ToolRegistry`、Memory hook 和 trace。✅
- 为 AgentNode 增加更细粒度的 resume contract，明确哪些 agent state 可恢复、哪些 tool call 必须幂等。✅
- 增加 AgentNode partial resume 执行能力，基于 `agent_resume` 复用已完成 observation 并从安全边界继续。✅
- 增强 partial resume 与 workflow checkpoint manager 的自动衔接，让失败的 AgentNode partial trace 能进入可恢复 checkpoint。✅
- 将 workflow run artifact directory 做成显式 runtime config，CLI 支持 `--run-dir` 和 `AGENTFLOW_RUN_DIR`。✅

### N2: Observability 和 Replay

- 增加 OpenTelemetry exporter，把 workflow、agent、tool、MCP trace 统一输出到 OTLP。✅
- 增加 trace persistence schema，用 SQLite/Postgres 保存 run、step、event、tool call、MCP call。✅
- 增加 `agentflow trace replay <run_id>` 或等价 API，用于复盘一次 workflow/agent 混合执行。✅
- 增加 trace redaction，默认隐藏 API key、env secret、tool sensitive params。✅
- trace replay/tui 支持 `AGENTFLOW_TRACE_DIR` 作为显式 storage 默认值，避免嵌入式和 CI 场景依赖 home 目录。✅

### N3: Security 和 Tool Governance

- 标准化 tool permission model，覆盖 builtin/script/mcp/workflow 四类来源。✅
- 为 script tools 增加 JSON Schema 参数校验和更严格的 sandbox policy。✅
- 增加 MCP server allowlist、command/env 审计和超时/并发默认限制。✅
- 在 CLI 和 trace 输出中统一敏感信息脱敏。✅

### N4: Skill 生态和 CLI

- 增加 `agentflow skill init`，生成标准 `SKILL.md`、示例、测试骨架。✅
- 增加 `agentflow skill test`，运行 skill manifest 校验、工具发现和最小调用回归。✅
- 设计本地 skill registry/index 格式，支持组织内共享和版本锁定。✅
- 补充可运行教程: 固定 DAG、agent-native、hybrid、Skill + MCP、WorkflowTool。✅
- 增加可验证的本地 skill registry/index 示例: `agentflow-skills/examples/skills.index.toml`。✅
- 增加 `agentflow skill install` 最小本地 registry 安装路径。✅
- 明确 registry/index schema、manifest lock 和后续远程分发边界。✅

### N5: 质量和发布门禁

- 清理 workspace clippy warning 债务，逐步把 CI 提升到 `cargo clippy --workspace --all-targets -- -D warnings`。✅
- 扩展核心 crates test matrix，增加 feature 组合、doc tests。✅
- 增加 examples compile/run CI gate，覆盖 workspace examples 编译和无外部 API smoke tests。✅
- 增加性能基准: 大 DAG 调度、ToolRegistry 调用、MCP tool latency、agent loop prompt assembly。
  - 大 DAG 调度 benchmark 已覆盖 100 / 1,000 / 10,000 synthetic DAG。✅
  - ToolRegistry benchmark 已覆盖 lookup、schema metadata、成功/错误执行路径。✅
  - MCP tool latency benchmark 已覆盖本地 stdio connect、tools/list、tools/call、reconnect。✅
  - Agent loop prompt assembly benchmark 已覆盖短上下文、长上下文和 summary 触发路径。✅
- 将 release checklist 固化为 CI job 和人工发布模板。✅

### N6: CLI / config-first 产品化

目标:

- 让 CLI 成为 DAG、agent-native、Skill 三类能力的稳定统一入口。
- 让用户可以通过配置文件和命令行完成模型配置、模型切换、Skill 安装/运行、workflow dry-run/run/debug。
- 避免 CLI flags 与实际行为不一致，禁止 silent no-op。

核心任务:

- 补齐 `agentflow workflow run` 的 `--input`、`--dry-run`、`--output`、`--timeout`、`--max-retries` 行为。
- 明确或隐藏 `--watch`，在未实现前不能静默忽略。
- 增强模型配置 CLI:
  - `agentflow config init/show/validate` 能覆盖模型配置、env var 检查和脱敏输出。
  - `agentflow llm models --provider ... --detailed` 能展示 provider、model、capabilities。
  - `agentflow llm chat --model ...`、`agentflow workflow run --model ...`、`agentflow skill run/chat --model ...` 的覆盖语义一致。
- 丰富 Skill CLI:
  - `skill list` 默认扫描 `~/.agentflow/skills`。
  - `skill inspect` 展示 persona/model/tools/memory/knowledge/security。
  - `skill list-tools` 展示工具来源、权限和 schema 摘要。
  - `skill run/chat` 支持 `--model`、`--trace`、`--session-id`、memory backend 覆盖。
  - `skill test --dry-run` 在无 API key 环境完成 manifest、MCP discovery、prompt preview。
- 在 workflow YAML/factory 中暴露 `agent` 或 `skill_agent` 节点，补齐 config-first DAG + Agent hybrid。
- 清理旧 CLI runner 或明确 legacy 边界，保证文档、示例和当前执行路径一致。

验收标准:

- 用户可以只用 CLI 完成: 初始化模型配置 -> 查看模型 -> 切换模型 chat -> 安装 Skill -> inspect/list-tools -> run Skill -> dry-run workflow -> 运行含 skill-agent 的 workflow -> 查看 trace。
- 关键 CLI 示例能进入 CI smoke test，默认不依赖外部 API key。

### N7: 统一 Trace / Recovery / 端到端样例

目标:

- 把 workflow、agent、tool、MCP、LLM 的运行记录统一成可追踪、可回放、可恢复的用户体验。
- 用少量权威示例证明 DAG、agent-native、hybrid、Skill + MCP、RAG + Memory 五类典型路径。

核心任务:

- 定义统一 run id / trace id / parent span 关联模型。
- 让 `AgentNode` 继承 workflow run context，agent steps、tool calls、MCP calls 能挂到同一次 mixed run。
- 强化 `FlowValue::Json/File/Url` 的 checkpoint JSON roundtrip，避免文件/URL 输出在恢复时丢失类型。
- 增加 AgentNode completed/partial checkpoint 恢复测试，明确 unresolved tool call 的幂等边界。
- 建立权威 examples:
  - fixed DAG basic，无外部 API。
  - Skill agent hybrid，可 dry-run，可用 mock skill。
  - model switching 教程。
  - Skill + MCP 教程。
  - RAG + Memory 教程，明确外部依赖。

验收标准:

- `agentflow trace replay <run_id>` 能展示 DAG -> Agent -> Tool/MCP 的层级关系。
- checkpoint roundtrip 后 `FlowValue` 类型和值保持一致。
- 新用户可按文档完成模型配置、Skill 使用、workflow dry-run、hybrid run 和 trace 查看。

### N8: 平台骨架与 v0.3.0 候选

目标:

- 把 `agentflow-server` / `agentflow-db` 从骨架推进到可用 control plane。
- 把 LLM 原生 tool calling 引入 `agentflow-llm` 抽象层，作为 ReAct/Plan-Execute 的优先路径。
- 修复 FlowValue checkpoint round-trip 类型保真问题，并升级条件表达式引擎。

核心任务:

- 服务端最小路由: `POST /v1/runs`、`GET /v1/runs/{id}`、`GET /v1/runs/{id}/events` (SSE)、`POST /v1/skills/{name}:run`、`GET /v1/skills`。✅ (2026-05-03)
- DB schema + migration: run / step / event / artifact / skill_install / mcp_session 6 表，使用 sqlx-migrate 或 refinery；与 `agentflow-tracing` Postgres 后端共享 schema，避免双写。✅ (2026-05-03)
- 在 `agentflow-llm` 请求/响应类型中增加 `tool_calls` / `tool_choice`，并在 `ReActAgent` / `PlanExecuteAgent` 中替换 prompt 解析路径为 provider 原生（OpenAI tools / Anthropic tool_use / Google function declarations），不支持时降级到 prompt 解析。✅ (2026-05-03)
- 在 `Tool` metadata 增加幂等性枚举（`Idempotent` / `NonIdempotent` / `Unknown`），让 partial resume 在 Idempotent 时自动重放。✅
- 修复 `state_after_N.json` 对 `FlowValue::File`/`Url` 的 round-trip，并加 property test。✅
- 引入轻量表达式引擎 (`evalexpr` 或自研 PEG)，替换 `run_if`/`while.condition` 的字符串简单解析，支持 `&&`、`||`、`>`、`<`、`contains`、`len()`、`null` 检查。✅
- 统一 workspace edition: 把 `agentflow-server` / `agentflow-db` 与其他 crate 对齐到同一 Rust edition。✅

验收标准:

- 通过 `curl` 可以提交一次 workflow run、订阅 SSE 事件、查询 run 状态，全部走 server。
- DAG 中的 LLM 节点和 ReAct agent 默认走 provider 原生 tool calling，与 prompt 解析路径行为等价。
- 含 `FlowValue::File` 输出的 workflow 失败重启后能保留文件类型和路径，单测覆盖。
- `run_if: "len(nodes.search.outputs.items) > 0 && nodes.classify.outputs.score > 0.7"` 能编译通过并被求值。

### N9: 多智能体协作与生态完善（v0.4.0 候选）

目标:

- 在 `agentflow-agents/supervisor` 沉淀生产可用的多智能体协作范式。
- 把工具沙箱、OTel context 传播、RAG 评测、Skill 权限决策做到生产标准。

核心任务:

- 多智能体协作三种范式落实示例和测试: handoff（角色切换）、blackboard（共享白板）、debate（多 agent 投票/批判）。✅ (2026-05-03)
- `ShellTool`/`ScriptTool` 接入 macOS sandbox-exec / Linux seccomp + chroot 子集；`Tool` 增加 `requires_capabilities()` (`fs.read` / `fs.write` / `net` / `exec`) 和被三方裁剪后的 `effective_capabilities`。
- `agentflow-llm` 客户端在 HTTP 请求 headers 注入 `traceparent`，统一 `WorkflowRunId / AgentSessionId / ToolCallId` 属性命名，OTel 端到端连续。
- `agentflow-rag/eval/`: 标注集 + Recall@K / MRR / nDCG 指标 + baseline 对比；新增 CLI 子命令 `agentflow rag eval <dataset>`。
- `docs/SKILL_PERMISSIONS.md` 写明 SkillSecurity vs ToolPolicy vs CLI flag 三方决策合并算法；`agentflow skill inspect --explain-permissions` 展示一次实际运行的最终决策路径。

验收标准:

- 三种协作范式各有一个权威可运行示例（研究 + 写作 + 评审三 agent 协作示例进入 CI smoke）。
- `ShellTool` 在受限沙箱下运行，越界访问被强制阻断且产出 `ToolPolicyDecision` 拒绝事件。
- 一次 hybrid 运行的 OTel trace 在 LLM hop 不再断裂。
- `agentflow rag eval` 可输出指定数据集的评测报告。

### N10: Plugin / 分布式 / Web UI（v1.0.0-rc 候选）

目标:

- 让第三方节点和 Skill 不修改主仓库即可发布。
- 让大型 DAG 可以分布式执行。
- 让混合执行（DAG × Agent × Tool × MCP）有统一的 Web/TUI 调试器。

核心任务:

- Plugin / Custom Node 体系: 评估 dlopen + abi_stable 与 WASM (wasmtime/wasmer) 两条路径，给出 ABI/lifecycle/permissions/signatures 设计文档；提交最小可用实现。
- 分布式调度: 抽象 worker，通过 gRPC / NATS / Redis Streams 之一让 DAG 可分布式；明确 `agentflow-server` 是否进化为 control plane。
- Web UI: React/Svelte SPA + `agentflow-server` SSE，展示 DAG 实时状态、Agent steps、Tool decisions；TUI 保留作为 headless 替代。
- Agent SDK 文档化: `docs/AGENT_SDK.md`，覆盖 `AgentRuntime` trait 实现、自定义 `ReflectionStrategy`/`MemorySummaryBackend`、自定义 `AgentStepKind`，"五分钟入门"教程化。

验收标准:

- 至少一个独立仓库可以分发一个 plugin 节点，被 AgentFlow 加载并运行；权限/版本/签名校验有据可查。
- 分布式 DAG 在 2 worker 集群上能执行 100+ 节点 workflow，trace 仍能跨 worker 拼接。
- Web UI 能实时展示一次 hybrid run，并接管 trace replay 的可视化职责。
- 三方插件示例进入 CI 编译/烟测。

## Phase 1: Skills + MCP 真正打通

状态: 已完成短期闭环，后续进入 Skill 生态化。

目标:

- Skill manifest 能声明 MCP servers。
- Skill 构建时能启动或连接 MCP server，发现 MCP tools，并注册到 ToolRegistry。
- Agent/Skill 运行时能像调用普通工具一样调用 MCP 工具。
- 完成最小端到端测试和 CLI 入口。

关键成果:

- `agentflow-skills` 支持 MCP server 配置加载和校验。
- `agentflow-mcp` stdio transport 支持环境变量。
- MCP tools 被适配为 `agentflow-tools::Tool`。
- 已增加本地 mock MCP server fixture，覆盖 `SKILL.md/skill.toml -> mcp_servers -> ToolRegistry -> call_tool`。
- MCP tool `description` 和 `inputSchema` 已透传到 Tool metadata。
- Tool metadata 已增加来源字段，覆盖 `builtin`、`script`、`mcp`、`workflow`。
- MCP tool metadata 已保留原始 MCP server name 和 tool name。
- `SKILL.md` 已确认为推荐标准入口，`skill.toml` 保留为兼容/覆盖 manifest；同目录同时存在时 `skill.toml` 生效。
- 已增加 `examples/skills/mcp-basic` 标准示例和本地最小 MCP server。
- CLI 已支持 `agentflow skill validate <path>` 真实校验 MCP server，并支持 `agentflow skill list-tools <path>` 展示 skill 工具。
- CLI `agentflow skill run/chat` 已通过 mock LLM 集成测试覆盖 MCP 工具调用链路。
- MCP tool adapter 已支持 `timeout_secs` 调用超时配置，并覆盖 env 参数传递测试。
- MCP `CallToolResult` 已转换为兼容字符串输出和 typed output parts，覆盖 text、image、resource 内容。
- MCP server connect、tool discovery、tool call success/failure/timeout 已输出结构化 tracing 事件。
- CLI MCP 失败信息已补充 server name、tool naming rule 和失败原因；MCP tool error result 已补充 server/tool 前缀。
- 修复格式化、示例和 doctest，保证后续开发基线干净。

下一阶段重点:

- 进入 registry/index 分发体验设计。

## Phase 2: 统一 Agent Runtime

状态: 已完成 MVP，后续进入生产化 runtime 能力。
Runtime 与现有 DAG `Flow` 的职责边界已记录在 `docs/AGENT_RUNTIME.md`。

目标:

- 在现有 DAG runtime 旁边增加 agent-native runtime。
- 定义统一的 agent 执行循环: observe -> plan -> act -> reflect -> update memory。
- 支持 ReAct、Plan-and-Execute、Reflective Agent 三类基础模式。

核心任务:

- 定义 `AgentRuntime`、`AgentContext`、`AgentStep`、`AgentEvent`。✅
- 现有 ReAct loop 已接入 `AgentRuntime` trait，并返回 structured steps/events。✅
- 抽象模型调用接口，复用 `agentflow-llm`。
- Tool 调用全部通过 ToolRegistry。✅
- Runtime guard 已覆盖 max steps、max tool calls、timeout 和 stop condition。✅
- Memory 已接入短期会话记忆；长期语义记忆后续深化。✅
- Agent runtime 已暴露 memory query 接口，使用 `SemanticMemory` 时可走语义检索。✅
- Agent runtime 已提供 memory read/search/write hook。✅
- ReAct runtime 已支持 prompt memory budget 和确定性摘要策略。✅
- 已增加 `PlanExecuteAgent` 原型，支持结构化计划 JSON、顺序工具执行、memory hook、timeout/cancellation 和统一 runtime trace。✅
- AgentNode 已输出 `agent_resume` 合约，标记完成态、partial resume 支持状态、tool call 重放策略和幂等要求。✅
- AgentNode 已支持消费既有 `agent_result` 输入执行 partial resume，恢复已记录 observation，并拒绝未完成 tool call 的隐式重放。✅
- Agent runtime 多步测试已覆盖 memory 在工具调用前后的写入和再读取。✅
- Reflection 作为可插拔策略，而不是写死在 agent loop 中。✅
- ReAct runtime 已把 reflection 输出写入 `Reflect` step 和 `ReflectionAdded` event。✅
- ReAct runtime 已支持在配置层关闭 reflection。✅
- Agent runtime 已增加 mock LLM 单元测试覆盖 action、tool、answer、steps/events/reflection。✅
- Runtime 与现有 `Flow` 的边界已明确。✅
- `agentflow skill run --trace` 已暴露 Skill/MCP tool 调用的 AgentRuntime steps/events。✅

验收标准:

- 可以运行一个最小 ReAct agent。
- 可以加载一个 Skill，并在 agent loop 中调用 Skill/MCP 工具。✅
- 每一步 agent 决策、工具调用和反思都可追踪。✅

## Phase 3: DAG + Agent 混合编排

目标:

- DAG 节点可以调用 agent。
- Agent 可以调用 workflow 作为工具。
- 支持固定流程和自主推理混合执行。

典型场景:

- DAG 中某个节点是 `AgentNode`，负责非确定性任务。
- Agent 将一个稳定业务流程作为 `WorkflowTool` 调用。
- Map/Parallel 节点批量执行多个 agent task。
- 失败时可通过 checkpoint 恢复 DAG 状态和 agent 状态。

核心任务:

- 标准化 `AgentNode`。✅
- 标准化 `WorkflowTool`。✅
- 统一状态序列化和恢复。✅
- 增强 trace，能跨 workflow/agent/tool 串联一次完整执行。✅
- 恢复后继续执行 DAG，并跳过已完成 AgentNode，避免重复工具调用。✅
- `WorkflowTool` 支持配置调用超时。✅

## Phase 4: Memory、Reflection、Planning 深化

目标:

- 支持生产可用的记忆和反思机制。
- 让 agent 不只是会调工具，而是能沉淀经验、修正计划、控制循环。

核心任务:

- 短期 memory: 当前任务上下文、tool observations、intermediate reasoning 摘要。
- 长期 memory: 用户偏好、历史任务、可检索知识、失败案例。
- Reflection 策略: step reflection、failure reflection、final reflection。
- Planning 策略: static plan、dynamic replan、DAG plan emission。
- 增加循环预算、token 预算、工具调用预算和停止条件。

## Phase 5: 标准化 Skills 生态

目标:

- 形成稳定的 Skill 包格式和发布/安装机制。
- 支持本地 skill、仓库 skill、组织内部 skill registry。
- 让 Skill 成为 agent-native 应用的 config-first 主入口，支持 CLI 安装、发现、检查、运行、调试和模型覆盖。

建议标准:

- `SKILL.md`: 必需，作为主入口和人类可读说明。
- frontmatter: 推荐，用于 name、description、version、allowed_tools、mcp_servers、permissions。
- `skill.toml`: 可选，作为结构化 manifest；短期保留兼容，长期根据实践决定是否降级为生成物或移除。
- `references/`: 可选，知识文件。
- `scripts/`: 可选，脚本工具。
- `examples/`: 可选，示例输入输出。
- `tests/`: 可选，skill 级回归测试。

核心任务:

- `agentflow skill init`
- `agentflow skill validate`
- `agentflow skill list-tools`
- `agentflow skill test`
- `agentflow skill install` 本地 registry 安装路径。✅
- `agentflow skill list` 默认扫描本地 skill home。
- `agentflow skill inspect` 汇总 persona、model、tools、memory、knowledge、security。
- `agentflow skill run/chat --model --trace --session-id`，支持运行时覆盖模型和追踪输出。
- `agentflow skill test --dry-run`，支持无 API key 环境验证 manifest、tool discovery 和 prompt preview。
- Skill 权限模型和 sandbox 策略。
- Skill registry/index 元数据。✅

## Phase 5.5: CLI / config-first 使用体验

目标:

- 把已实现的 DAG、agent-native、Skill、MCP、LLM 能力通过 CLI 串成完整用户路径。
- 优先解决“命令存在但参数未生效”和“SDK 有能力但 YAML/CLI 无入口”的问题。

核心任务:

- `workflow run` flags 全部兑现: input、dry-run、output、timeout、retry；未实现 watch 前显式报错或隐藏。
- 模型配置体验完整: init/show/validate/models/chat/run 共享统一模型选择优先级。
- workflow YAML 支持 `skill_agent` 或等价节点。
- CLI 输出 contract 稳定，支持人类可读输出和机器可读 JSON 输出。
- docs/examples/CI smoke tests 覆盖关键 CLI 使用路径。

## Phase 6: 生产化和生态工具

目标:

- 将框架推进到可部署、可监控、可调试、可扩展。

核心任务:

- OpenTelemetry exporter。
- 数据库存储 trace。
- Web UI 或 TUI 调试器。✅ 最小终端 timeline 已支持 `agentflow trace tui`。
- 运行记录 replay。
- 配置加密和 secret 管理。✅ 已定义 secret 边界，`config show/validate` 默认不打印密钥值。
- Docker/Helm 部署。✅ 已提供 server Dockerfile、docker-compose 和 Helm chart 初版。
- Plugin/Skill marketplace 雏形。✅ 已提供 marketplace manifest、CLI 浏览/解析和本地示例。

## 里程碑

### M1: Skills + MCP 可用

- Skill 能声明 MCP server。✅
- MCP tools 自动注册到 ToolRegistry。✅
- CLI 可以查看 skill tools。✅
- CLI 可以通过 run/chat 调用 skill tools。✅
- 有端到端测试覆盖。✅

### M2: Agent Runtime 可用

- 最小 ReAct agent 可运行。✅
- 支持 ToolRegistry、Memory、Tracing。✅
- 支持 reflection 策略。✅
- 支持 memory query、memory hook、prompt memory budget 和摘要策略。✅
- Agent runtime trace 有 golden fixture 覆盖。✅
- Release 前检查清单已建立，用于人工门禁和后续 CI 对齐。✅

### M3: DAG + Agent 混合

- DAG 可调用 agent。✅
- Agent 可调用 workflow。✅
- checkpoint 能覆盖 AgentNode 状态和 agent step history。✅
- trace 能覆盖混合执行。✅
- DAG + Agent hybrid 可运行示例已覆盖。✅
- checkpoint resume 可从下一个 DAG 节点继续，并复用已完成 agent 状态。✅
- 固定 DAG、agent-native ReAct、Skill 调 MCP 工具示例已覆盖。✅
- CI 已增加格式化、clippy 和核心 crates test matrix 质量门禁。✅

### M4: Skill 标准稳定

- `SKILL.md` 标准确定。✅
- `skill.toml` 兼容策略确定。✅
- CLI 支持 init/validate/test/install。✅
- CLI 支持 list/inspect/list-tools/run/chat 的完整本地 Skill 使用链路。
- Skill run/chat 支持 `--model`、`--trace`、`--session-id` 等运行时覆盖。

### M4.5: CLI / config-first 可用

- `workflow run` 的 input、dry-run、output、timeout、retry 行为可用。
- `config show/validate` 能覆盖模型配置并默认脱敏。
- `llm chat`、`workflow run`、`skill run/chat` 的模型覆盖语义一致。
- workflow YAML 支持 `skill_agent` 或等价 agent 节点。
- CLI help、README、docs/examples 与实际实现一致，不存在 silent no-op 的公开参数。

### M5: 生产部署候选

- tracing、security、deployment、docs 完整。
- 核心测试和集成测试稳定。
- 示例覆盖 DAG、agent-native、hybrid 三种模式。✅

### M6: 混合智能体应用体验稳定

- 一个端到端教程覆盖模型配置、Skill 安装、Skill 运行、DAG dry-run、skill-agent workflow、trace replay。
- trace 能串联 workflow、agent、tool、MCP、LLM 调用。
- checkpoint/recovery 覆盖 Json/File/Url 和 AgentNode completed/partial 状态。
- 关键示例进入 CI smoke test，默认不依赖外部 API。

### M7: 平台骨架 + 原生 Tool Calling (v0.3.0)

- `agentflow-server` 暴露 `runs`、`skills`、`events` 路由，可独立部署。
- `agentflow-db` 落实 run/step/event/artifact/skill_install/mcp_session schema 和 migration。
- `agentflow-llm` 支持 OpenAI tools / Anthropic tool_use / Google function declarations 一等公民。
- `Tool` metadata 增加幂等性枚举，partial resume 在 Idempotent 时自动重放。
- `FlowValue::File` / `Url` checkpoint round-trip 类型保真。
- 表达式引擎升级，`run_if` / `while.condition` 支持复合表达式。✅
- workspace edition 统一。✅

### M8: 多智能体协作 + 生态完善 (v0.4.0)

- handoff / blackboard / debate 三种协作范式各有权威示例并进入 CI smoke。
- `ShellTool` / `ScriptTool` 在受限沙箱下运行，capability 决策可观察可强制。
- OTel context 在 LLM hop 不再断裂，跨 DAG/Agent/Tool/MCP/LLM 的 trace 完整连续。
- `agentflow rag eval <dataset>` 输出 Recall@K / MRR / nDCG 报告。
- Skill 权限合并算法形成正式文档，`skill inspect --explain-permissions` 展示决策路径。

### M9: Plugin / 分布式 / Web UI (v1.0.0-rc)

- 至少一个独立仓库分发的插件节点能被 AgentFlow 加载并运行。
- DAG 可在 2+ worker 集群上分布式执行；trace 跨 worker 完整拼接。
- Web UI 能实时展示 hybrid run 的 DAG / Agent / Tool 状态。
- Agent SDK 文档完整，`docs/AGENT_SDK.md` 提供五分钟入门和扩展点参考。
