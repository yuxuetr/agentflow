# AgentFlow 智能体框架 RoadMap

最后更新: 2026-04-26

## 目标定位

AgentFlow 的下一阶段目标是从工作流编排项目演进为一个同时支持固定 DAG 工作流和 agent-native 自主循环的智能体框架。

核心方向:

- 支持确定性的 DAG 式工作流，适合生产流程、批处理、RAG pipeline、多步骤业务自动化。
- 支持 agent-native 执行模式，包含计划、观察、工具调用、反思、记忆、恢复和多轮决策。
- 将 Skills、Tools、MCP、Memory、Tracing、Runtime 统一为稳定的底层能力。
- 保持 Rust 项目继续演进，不另起新项目，优先复用当前 crates 的基础能力。

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

## Phase 1: Skills + MCP 真正打通

状态: 已启动，基础接入、端到端测试和 Skill 格式策略已完成。

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

- 进入 Phase 2 Agent Runtime 类型设计。

## Phase 2: 统一 Agent Runtime

状态: 已启动，基础 runtime 类型边界已加入 `agentflow-agents`。
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
- Memory 接入短期会话记忆和长期语义记忆。
- Reflection 作为可插拔策略，而不是写死在 agent loop 中。✅
- ReAct runtime 已把 reflection 输出写入 `Reflect` step 和 `ReflectionAdded` event。✅
- Agent runtime 已增加 mock LLM 单元测试覆盖 action、tool、answer、steps/events/reflection。✅
- Runtime 与现有 `Flow` 的边界已明确。✅

验收标准:

- 可以运行一个最小 ReAct agent。
- 可以加载一个 Skill，并在 agent loop 中调用 Skill/MCP 工具。
- 每一步 agent 决策、工具调用和反思都可追踪。

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
- 标准化 `WorkflowTool`。
- 统一状态序列化和恢复。
- 增强 trace，能跨 workflow/agent/tool 串联一次完整执行。

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
- Skill 权限模型和 sandbox 策略。
- Skill registry/index 元数据。

## Phase 6: 生产化和生态工具

目标:

- 将框架推进到可部署、可监控、可调试、可扩展。

核心任务:

- OpenTelemetry exporter。
- 数据库存储 trace。
- Web UI 或 TUI 调试器。
- 运行记录 replay。
- 配置加密和 secret 管理。
- Docker/Helm 部署。
- Plugin/Skill marketplace 雏形。

## 里程碑

### M1: Skills + MCP 可用

- Skill 能声明 MCP server。✅
- MCP tools 自动注册到 ToolRegistry。✅
- CLI 可以查看 skill tools。✅
- CLI 可以通过 run/chat 调用 skill tools。✅
- 有端到端测试覆盖。✅

### M2: Agent Runtime 可用

- 最小 ReAct agent 可运行。
- 支持 ToolRegistry、Memory、Tracing。
- 支持 reflection 策略。

### M3: DAG + Agent 混合

- DAG 可调用 agent。✅
- Agent 可调用 workflow。
- checkpoint/trace 能覆盖混合执行。

### M4: Skill 标准稳定

- `SKILL.md` 标准确定。
- `skill.toml` 兼容策略确定。
- CLI 支持 init/validate/test/install。

### M5: 生产部署候选

- tracing、security、deployment、docs 完整。
- 核心测试和集成测试稳定。
- 示例覆盖 DAG、agent-native、hybrid 三种模式。
