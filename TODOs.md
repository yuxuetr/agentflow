# AgentFlow 当前执行计划

最后更新: 2026-05-02

维护约定:

- `RoadMap.md` 是可跟踪的中长期路线图。
- `TODOs.md` 是本地短期执行队列。
- 不再维护 `TODO.md`。
- 本文件按 `RoadMap.md` 当前未完成项整理；完成后同步回写 `RoadMap.md`。
- 任务条目模板: 状态 / 目标 / 关键路径 / 子任务 (checkbox) / 验收标准 / 验证命令 / 涉及文件。

## 当前基线

已完成短期闭环 (N1–N7):

- Skills + MCP 基础打通、Agent Runtime MVP、DAG + Agent hybrid。
- trace 串联、checkpoint resume。
- CLI 产品化闭环: workflow run flags、模型配置/切换、Skill 完整 CLI、agent/skill_agent YAML 节点、统一 trace replay。
- 可运行教程和 Skill registry/index 示例。
- CI 已覆盖 fmt、clippy `-D warnings`、核心 crates test matrix、workspace examples 编译和无外部 API smoke tests。

最新评估结论 (2026-05-01):

- `PROJECT_EVALUATION_2026-05-01.md` 完成，HEAD `41ed3f8`。
- 综合评级 B+: 架构 A-、DAG A-、agent-native B+、CLI B、可观测性 B+、平台化 C-。
- 主要短板由"CLI 产品化"转向"平台化（server/db）+ LLM 原生 tool calling + checkpoint/表达式保真 + 多智能体协作"。

下一阶段三档发布节奏 (RoadMap N8/N9/N10):

- v0.3.0: 平台骨架 + 原生 tool calling + checkpoint/表达式保真 (P0)
- v0.4.0: 多智能体协作 + 工具沙箱 + OTel 端到端 + RAG 评测 (P1)
- v1.0.0-rc: 插件体系 + 分布式 + Web UI + Agent SDK 文档 (P2)

最近提交:

- `9dc4e35 feat(agents): land P1 #7 multi-agent collaboration patterns`
- `b48f173 docs: close P0 #1 platform skeleton milestone`
- `c1d4a04 feat(server): bridge agentflow-core EventListener to DB + SSE`
- `210ac55 feat(server): GET /v1/skills + POST /v1/skills/{name}:run`
- `9777650 feat(server): SSE event stream with broker + DB-backed replay`

---

## P0: N8 平台骨架 + 原生 Tool Calling + 保真度修复 (v0.3.0 候选)

### 1. 平台服务端最小骨架

状态: 已完成 (2026-05-03)

目标:

- 把 `agentflow-server` / `agentflow-db` 从骨架推进到可独立部署的 control plane。
- 让"提交一次 workflow run、订阅事件、查询状态"全部走 HTTP，而不是命令行。

关键路径:

- 复用 `Flow::execute_*` 与 `agentflow-tracing` 已有能力，server 只承担 routing / persistence / streaming。
- DB schema 与 `agentflow-tracing` Postgres backend 共享（避免双写）。

子任务:

- [x] `agentflow-db`: 引入 `sqlx::migrate!()` 嵌入式迁移 + 6 张表 schema (`migrations/0001_initial_schema.sql`)：runs / steps / events / artifacts / skill_installs / mcp_sessions，附加索引 (runs_tenant_started_idx / runs_status_idx / events_run_ts_idx / artifacts_run_idx / mcp_sessions_server_idx)。`Database::connect_and_migrate(...)` 一步连接 + 应用迁移；`run_migrations()` 幂等。集成测试 (`tests/migrations.rs`) 通过 `AGENTFLOW_DATABASE_TEST_URL` env 触发，CI 默认跳过。
- [x] `agentflow-db`: Repository trait + Postgres 实现：`RunRepo` / `StepRepo` / `EventRepo` / `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo` (`src/repo.rs`)，`Repositories::from_pool` 一次性构造全套；模型层 `Run` / `Step` / `Event` / `Artifact` / `SkillInstall` / `McpSession` + `RunStatus` 枚举 (`src/models.rs`)。集成测试 `tests/repositories.rs` 覆盖 create/get/list/update_status 和 step/event round-trip（同样 `AGENTFLOW_DATABASE_TEST_URL` gate）。
- [x] `POST /v1/runs` + `GET /v1/runs/{id}`：JSON body `{workflow, workflow_id?, tenant_id?}` → queued run record；`RunExecutor` trait 抽象后台执行（默认 `StubExecutor` 写入 run_started/run_completed 事件并切到 succeeded，task #14 替换为真正的 Flow runner）；4 条 e2e 测试覆盖提交+落库+异步状态切换、缺 workflow → 400、未知 id → 404、查询持久化行。
- [x] `GET /v1/runs/{id}/events` SSE：`EventBroker` (per-run-id `tokio::sync::broadcast` 通道) + `publish_through` 同时落库与推送；handler 先回放 DB 历史 (`?after_seq=` 续传)，再桥接 live stream，每 15s keep-alive；`Lagged` 转换为 SSE 注释让客户端基于 `after_seq` 续连；3 条 broker 单测 + 1 条 DB-gated e2e (验证 stub 写入的 run_started/run_completed 抵达订阅者)。
- [x] `GET /v1/skills` + `POST /v1/skills/{name}:run`：`SkillCatalog` 包装 `agentflow_skills::SkillRegistryIndex`，`AGENTFLOW_SKILLS_INDEX` env 指向 `skills.index.toml` (空时仍然可用，listing 返回空数组)；run 路由解析 skill 后落到 `runs(workflow="@skill:<name>"[\n---\n<input>])` 并复用 `RunExecutor` 派发；3 条单测/集成测试覆盖空 catalog list、未知 skill 404、skill_routes 集成。
- [x] `agentflow-server`: 接 `agentflow-core::events::EventListener`，每个事件落库 + 推送给订阅者：`WorkflowEventListener` 把同步 `WorkflowEvent` 通过 unbounded mpsc 桥接到 async `EventSink::publish`，`workflow_event_payload` 把所有 14 个 variant 转成 JSON（drop `Instant`，`duration` → `duration_ms`），`WorkflowEventListener::from_state` 一行接入 `Repositories` + `EventBroker`；2 条单测覆盖完整 bridge + payload 序列化。真正的 Flow runner 替换 `StubExecutor` 留给 v0.4.0 集成（schema/路由/桥已就绪）。
- [x] `agentflow-server`: 错误响应统一为 `{ "error": { "code", "message", "details" } }`：`ApiError` 全面重写，每个 variant 映射稳定 `code` (not_found / bad_request / unauthorized / forbidden / database_error / internal_error / server_misconfigured)；3 条单测覆盖序列化。
- [x] 基础 AuthN: `Authorization: Bearer <token>` 中间件 (`src/auth.rs`)，常量时间比对，`AuthConfig::from_env()` 从 `AGENTFLOW_API_TOKEN` 读取，留 OAuth 扩展点；`/health*` 路由不需要 auth；5 条 e2e 测试覆盖缺失/错误/空格/正确 token + 401/403 envelope。
- [x] 端到端测试: `cargo test -p agentflow-server` 跑 14 个 tests（5 auth/error envelope + 4 run routes + 1 SSE + 2 skill routes + 2 listener bridge）。其中 7 条 DB-gated 通过 `AGENTFLOW_DATABASE_TEST_URL` 跳过保持 CI 干净，本地 `docker compose up -d postgres && export AGENTFLOW_DATABASE_TEST_URL=...` 即可全跑。
- [x] `docs/DEPLOYMENT.md` 增补 v0.3.0 N8 章节：bearer auth / submit run / SSE 订阅（含 `?after_seq=` 续传） / skills 路由 / unified error envelope / 本地 Postgres 测试入口。`docker-compose.yml` 注释化的 `AGENTFLOW_API_TOKEN` 与 `AGENTFLOW_SKILLS_INDEX` env，给出 skill 卷挂载示例，对外暴露 `5432` 方便本地 `cargo test` 直连。

验收标准:

- `curl -X POST http://localhost:8080/v1/runs -H "Authorization: Bearer dev" -d @workflow.json` 提交成功并返回 run_id。
- `curl -N http://localhost:8080/v1/runs/{id}/events` 能实时收到 NodeStarted / NodeCompleted 事件。
- `psql -c "SELECT * FROM runs ORDER BY started_at DESC LIMIT 5"` 能看到最近 5 次 run。

涉及文件:

- `agentflow-server/src/{lib,main,routes,db}.rs`
- `agentflow-db/src/{lib,schema,repo,migrations/}.rs`
- `docs/DEPLOYMENT.md`、`docker-compose.yml`

验证:

```bash
cargo check -p agentflow-server -p agentflow-db --target-dir /tmp/agentflow-target
cargo test -p agentflow-server -p agentflow-db --target-dir /tmp/agentflow-target
docker-compose up agentflow-server postgres
```

### 2. LLM 原生 Tool Calling 一等公民化

状态: 已完成 (2026-05-03)

目标:

- 让 `agentflow-llm` 抽象层把工具调用作为请求/响应的一等字段，而不是依赖 prompt 解析。
- ReAct / Plan-Execute 默认走 provider 原生 (`tool_calls` / `tool_use` / function declarations)，不支持的 provider 自动降级到 prompt。

关键路径:

- 不破坏当前 ReAct prompt 解析路径（保留为 fallback）。
- 在 LLM 抽象层引入 `LLMRequest::tools` 和 `LLMResponse::tool_calls`，由 provider 客户端转换。

子任务:

- [x] 在 `agentflow-llm` 请求/响应类型新增:
  - `ToolSpec` / `ToolChoice` / `ToolCallRequest` / `StopReason` / `LLMResponse` (`agentflow-llm/src/tool_calling.rs`)
  - `ProviderRequest::tools` + `tool_choice`、`ProviderResponse::tool_calls` + `stop_reason`
  - `LLMClient::execute_full() -> LLMResponse`、`LLMClientBuilder::tool_choice` / `tools_from_openai_json`
- [x] 增加 capability flag `ModelCapabilities::native_tool_calling: bool`，`ModelConfig::native_tool_calling`，`ConfigUpdater` 自动判定。
- [x] OpenAI provider: 把 `tools` 映射为 `tools` array、解析 `tool_calls` 字段（`build_request_body` 注入 `tools` / `tool_choice`，`execute` 解析 `OpenAIMessage::tool_calls` + `finish_reason`，5 条 fixture 单测）。
- [x] Anthropic provider: 把 `tools` 映射为 `tools` block (`name` / `description` / `input_schema`)，从 `content` 中提取 `tool_use` blocks，`stop_reason: tool_use` → `StopReason::ToolCalls`，4 条 fixture 单测。
- [x] Google provider: 映射 `tools[0].functionDeclarations` + `toolConfig.functionCallingConfig` (AUTO/ANY/NONE/specific via `allowedFunctionNames`)，从 `parts` 中抽 `functionCall`，合成 `call_<idx>` id；当 `finishReason: STOP` 但出现 functionCall 时改写为 `StopReason::ToolCalls`。4 条 fixture 单测。
- [x] StepFun / Moonshot / Mock:
  - StepFun / Moonshot 重用 OpenAI 编解码（`tool_spec_to_openai_value` / `tool_choice_to_openai_value` / `parse_openai_tool_calls`），透传 `tools` / `tool_choice`，解析 `tool_calls` + `finish_reason`。
  - Mock 新增 `with_tool_calls(...)` 注入入口；当队列非空时 `stop_reason: ToolCalls`，否则 `Stop`。
  - 4 条新单测覆盖 Moonshot / StepFun 直通 + Mock 注入。
- [x] `ReActAgent` 优先消费 `LLMResponse::tool_calls`：通过 `execute_full()` 拿到结构化响应，命中时合成 `AgentResponse::Action`；为空时回退原有 JSON prompt 解析。同时 `tools(self.collect_tool_specs())` 把已注册工具透传给 LLM。
- [x] `PlanExecuteAgent` 同样替换 plan→tool 过渡阶段：`call_planner` 返回 `LLMResponse`，命中时把每个 tool_call 映射成 `PlanExecuteStep`，否则走 `parse_plan` JSON。
- [x] 单测覆盖: 每个 provider 的 tool calling 请求/响应 fixture（OpenAI 5 + Anthropic 4 + Google 4 + Moonshot/StepFun/Mock 4 = 17 条）；ReAct 与 Plan-Execute 各一条 native-path 端到端 golden trace（通过新 `AGENTFLOW_MOCK_TOOL_CALLS` env-var 注入）；现有 fallback 路径测试维持原状未受影响。

验收标准:

- 在配置中切换到 GPT-4o / Claude Sonnet / Gemini Pro 后，ReAct 默认走原生 tool calling，trace 中 ToolCall step 来自 provider 而不是 prompt 解析。
- Mock provider 仍能驱动 fallback 路径，离线 CI 不依赖外部 API。
- token 使用对比基准下降（实际数值待测）。

涉及文件:

- `agentflow-llm/src/{client,request,response,providers/*}.rs`
- `agentflow-agents/src/react/{agent,parser}.rs`
- `agentflow-agents/src/plan_execute.rs`

验证:

```bash
cargo test -p agentflow-llm --target-dir /tmp/agentflow-target
cargo test -p agentflow-agents --target-dir /tmp/agentflow-target
cargo run -p agentflow-agents --example react_agent
```

### 3. Tool 幂等性与 Partial Resume 自动重放

状态: 已完成

目标:

- 在 `Tool` metadata 上增加幂等性标识，让 `AgentNode` partial resume 在 Idempotent 工具上自动重放。
- 减少 partial resume 在保守拒绝场景下的人工介入。

子任务:

- [x] 在 `agentflow-tools::ToolMetadata` 增加 `idempotency: ToolIdempotency::{Idempotent, NonIdempotent, Unknown}`。
- [x] 内置工具默认值: `FileTool::read/list = Idempotent`, `FileTool::write = NonIdempotent`, `HttpTool::GET = Idempotent`, `HttpTool::POST = NonIdempotent`, `ShellTool = NonIdempotent`。
- [x] MCP tool adapter 通过约定 hint（如 `description` 中 `[idempotent]` 标签或 inputSchema 自定义字段）传递。
- [x] `AgentNodeResumeContract`: Idempotent 工具自动重放，NonIdempotent 拒绝并给出明确报错，Unknown 拒绝并提示用户显式标注。
- [x] 新增测试: partial resume 跨 Idempotent/NonIdempotent/Unknown 三种 tool 路径。

验收标准:

- AgentNode partial resume 在 GET HTTP / read file 两条路径上能自动恢复并跳过重新调用。
- POST HTTP / write file 路径仍要求显式人工干预，错误信息清晰。

涉及文件:

- `agentflow-tools/src/{tool,builtin/*}.rs`
- `agentflow-agents/src/nodes/agent_node.rs`
- `agentflow-mcp/src/tools.rs`

### 4. FlowValue Checkpoint 类型保真

状态: 已完成

目标:

- 修复 `state_after_N.json` 对 `FlowValue::File`/`Url` 的 round-trip 问题，保证失败重启后输出类型一致。

子任务:

- [x] 在 `agentflow-core/src/value.rs` 给 `FlowValue` 实现 `serde::Serialize/Deserialize` 的稳定 schema:
  - Json: `{"type": "json", "value": ...}`
  - File: `{"type": "file", "path": "...", "mime_type": "..."}`
  - Url: `{"type": "url", "url": "...", "mime_type": "..."}`
- [x] `Flow::state_pool_to_checkpoint_state` 与 `state_pool_from_checkpoint` 使用上述 schema。
- [x] 增加 property test: `for value in arbitrary FlowValue: assert_eq!(value, from_json(to_json(value)))`。
- [x] 增加端到端测试: workflow 输出 `FlowValue::File`，checkpoint 写盘后重启，下游节点仍读到 `FlowValue::File`。
- [x] 更新 `docs/CHECKPOINT_RECOVERY.md` 描述 schema 与兼容策略（已有数据如何迁移，必要时给 0.2.x → 0.3.0 转换工具）。

验收标准:

- 在 `agentflow-core` 中 `cargo test --test checkpoint_*` 全绿，包括新的 property test。
- 含 `text_to_image` 节点的 workflow，失败重启后下游 `image_understand` 仍能拿到 `FlowValue::File`。

涉及文件:

- `agentflow-core/src/{value,checkpoint,flow}.rs`
- `agentflow-core/tests/checkpoint_*`
- `docs/CHECKPOINT_RECOVERY.md`

### 5. 表达式引擎升级

状态: 已完成 (2026-05-02)

目标:

- 把 `run_if` / `while.condition` 从字符串简单解析升级为可读、可测的表达式语言。
- 不引入完整脚本语言，避免成本和安全风险。

子任务:

- [x] 评估 `evalexpr` (现成 crate) vs 自研 PEG: 对比表达力、依赖大小、错误信息、安全模型。
- [x] 选择方案后，在 `agentflow-core/src/expr.rs` 实现统一 `evaluate(expr: &str, state: &StatePool) -> Result<FlowValue>`。
- [x] 支持运算符: `&&`, `||`, `!`, `==`, `!=`, `>`, `<`, `>=`, `<=`, `+`, `-`, `*`, `/`。
- [x] 支持函数: `len(x)`, `contains(s, sub)`, `is_null(x)`, `is_empty(x)`, `to_number(x)`, `to_string(x)`。
- [x] 路径访问: `nodes.X.outputs.Y`, `inputs.Z`, `nodes.X.outputs.Y.0`（数组索引）。
- [x] 替换 `flow.rs::evaluate_condition` 调用点，保留旧行为兼容（`{{ value }}` 与简单布尔路径继续可用）。
- [x] 文档: `docs/EXPRESSION_LANGUAGE.md` 给完整参考 + 示例。
- [x] CLI `workflow validate --strict` 校验所有 `run_if` / `while.condition` 表达式是否能编译。

验收标准:

- [x] `run_if: "len(nodes.search.outputs.items) > 0 && nodes.classify.outputs.score > 0.7"` 能正确求值。
- [x] 错误表达式给出列号与上下文提示，类似 `Error at col 1: unknown function 'lenn', did you mean 'len'?`。
- [x] 已有 workflow 在不修改 YAML 的前提下继续工作。

涉及文件:

- `agentflow-core/src/{expr,flow}.rs`
- `agentflow-core/Cargo.toml` (新依赖)
- `docs/EXPRESSION_LANGUAGE.md`、`docs/WORKFLOW_SCHEMA.md`

### 6. Workspace Edition 统一

状态: 已完成 (2026-05-02)

目标:

- 解决 `agentflow-server` / `agentflow-db` 使用 Rust 2024 edition、其他 workspace crate 使用 2021 edition 的不一致。

子任务:

- [x] 评估两条路径: 全 workspace 升到 2024，或将 server/db 暂时回退到 2021。
- [x] 推荐路径: 全部升到 2024，一次性吸收 edition 差异。
- [x] 修复 edition 升级带来的 lint / warning。
- [x] 更新 `CLAUDE.md` 与 `docs/ARCHITECTURE.md` 中 Rust edition 说明。

验收标准:

- [x] `cargo metadata` 中所有 workspace member 的 `edition` 字段一致。
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 通过。

---

## P1: N9 多智能体协作 + 工具沙箱 + 端到端 OTel + RAG 评测 (v0.4.0 候选)

### 7. 多智能体协作三种范式

状态: 已完成 (2026-05-03)

目标:

- 在 `agentflow-agents/supervisor` 沉淀 handoff / blackboard / debate 三种生产可用的协作范式，每种范式有权威示例与单测。

子任务:

- [x] 扩展 `AgentStepKind` / `AgentEvent`: `Handoff` / `BlackboardOp` / `DebateProposal` / `DebateVerdict` 步骤变体 + `HandoffOccurred` / `BlackboardWritten` / `DebateRoundStarted` / `DebateVerdictRendered` 事件变体；10 条 serde round-trip 单测。
- [x] `HandoffSupervisor` (`agentflow-agents/src/supervisor/handoff.rs`): 共享 `HandoffSignal` + `HandoffTool`，支持 `max_handoffs` 上限和外部 `use_signal()` 注入；14 条单测覆盖 builder 校验、tool 行为、端到端 mock LLM 链路、cancellation；example `multi_agent_handoff.rs`。
- [x] `BlackboardSupervisor` (`blackboard.rs`): `Blackboard { Arc<RwLock<HashMap>> }`、`bb_read`/`bb_write` 工具、`Sequential|Parallel` 调度、`AllAgentsCompleted|KeySet` 停止条件、`answer_from` 出口；12 条单测；example `multi_agent_blackboard.rs`。
- [x] `DebateSupervisor` (`debate.rs`): N participants 并发提案 → 多轮可选修正 → judge 终裁；8 条单测；example `multi_agent_debate.rs`。
- [x] CLI/YAML 暴露: `multi_agent` 节点 (`agentflow-cli/src/executor/multi_agent.rs`) 支持 `mode: handoff|blackboard|debate`、按 mode 解析 `agents`/`participants`/`judge`、`schedule` / `stop_when` / `answer_from` / `rounds` / `judge_prompt`；`SkillBuilder::build_with_extra_tools(...)` 让 supervisor 把协调工具注入 skill agents；`schema.rs` 接受 `multi_agent` 节点；`workflow run --model` 也覆盖 `multi_agent`；5 条 config 解析单测 + 1 条端到端 CLI smoke (`cli_workflow_run_supports_multi_agent_handoff_node`)。
- [x] Trace replay 渲染: `agentflow-tracing/src/replay.rs` 增加 `handoff` / `blackboard_op` / `debate_proposal` / `debate_verdict` 4 个 step kind 的结构化输出分支，TUI 路径(`tui.rs::compact_json`)无需改动天然兼容；1 条新单测 (`replay_renders_multi_agent_step_kinds`) 覆盖 4 个变体的格式化。
- [x] 文档: `docs/MULTI_AGENT.md` 三范式决策表、API 用法、YAML 参考、trace 形状。

验收标准:

- 一个"研究 + 写作 + 评审"三 agent 协作的 example 进入 CI smoke (mock LLM)。
- 三种范式的单测覆盖关键路径，各自至少 5 个测试。

涉及文件:

- `agentflow-agents/src/runtime.rs` (新 step/event 变体), `agentflow-agents/src/react/agent.rs` (穷举 match 适配), `agentflow-agents/src/lib.rs` (re-export `BlackboardOpKind`)
- `agentflow-agents/src/supervisor/{mod,handoff,blackboard,debate}.rs`
- `agentflow-agents/examples/multi_agent_{handoff,blackboard,debate}.rs`
- `agentflow-skills/src/builder.rs` (新 `build_with_extra_tools`)
- `agentflow-cli/src/executor/{mod,multi_agent,factory}.rs`、`agentflow-cli/src/config/schema.rs`、`agentflow-cli/src/commands/workflow/run.rs`、`agentflow-cli/tests/workflow_tests.rs`
- `agentflow-tracing/src/replay.rs`
- `docs/MULTI_AGENT.md`

### 8. 工具进程级沙箱

状态: 已完成 (PR-A 2026-05-04 / PR-B 2026-05-07)

目标:

- 把 `ShellTool` / `ScriptTool` 从声明式权限提升到进程级强 enforcement。
- 让权限模型从"过滤"升级到"裁剪"。

子任务:

- [x] 在 `agentflow-tools/src/sandbox/` 引入平台抽象 (PR-B):
  - 模块拆分: `sandbox/{mod,policy,backend,macos,linux,noop}.rs`。`SandboxBackend` trait + `SandboxScope` + `SandboxError` + `default_backend()` 工厂。
  - macOS: `MacosSandboxExecBackend` 生成 SBPL profile（`(deny default)` 基线 + `process-info*` / `ipc-posix-shm` / `(allow file-read* (literal "/"))` 等启动期必需规则；按 capability 注入 `file-read*` / `file-write*` / `network*` / `process-exec`），并把命令重写为 `/usr/bin/sandbox-exec -f <profile.sb> <cmd>`。
  - Linux: `LinuxSeccompBackend` 通过 `seccompiler` 编译 default-allow + per-cap deny 的 BPF filter，在 `Command::pre_exec` 内 `apply_filter`；缺 `Net` 拒 socket/connect/bind/...，缺 `FsWrite` 拒 unlinkat/renameat/mkdirat/...；x86_64 + aarch64。
  - 其他平台: `NoopSandboxBackend` 返回 `is_enforcing()=false`，调用方可据此拒绝调度。
- [x] 在 `Tool` trait 增加 `requires_capabilities() -> Vec<Capability>`，枚举 `Capability::{FsRead, FsWrite, Net, Exec, Env}`。`agentflow-tools/src/capability.rs` 新模块；`Capability::from_permission(s)` 提供 `ToolPermission → Vec<Capability>` 默认映射 (`FilesystemRead → FsRead` / `FilesystemWrite → FsWrite` / `ProcessExec → Exec` / `Network → Net` / `Mcp → {Net, Exec}` / `Workflow → []`)；`Tool` trait 默认 impl 由声明的 permissions 派生。
- [x] 实现三方权限合并算法: SkillSecurity → ToolPolicy → CLI flag → effective capabilities，每一步可观察。`EffectiveCapabilities::resolve(tool, required, skill, policy, cli)` 做四层 (`tool_required` + 三层) 交集；`ToolRegistry::with_skill_capabilities` / `with_cli_capabilities` 安装层；`ToolPolicy::allowed_capabilities()` 把已有 permission allowlist 投影到 capability；`evaluate_capabilities(name)` 输出含 `trace: [CapabilityDecisionEntry]` 的 `EffectiveCapabilities`；`ToolRegistry::execute` 在策略层之后串入第二道 capability 裁剪 (返回 `PolicyDenied` + 写入 `capability_audit_log`)。`agentflow-skills::SkillBuilder` 改走 `with_skill_capabilities(Capability::from_permissions(...))` 并保留 `ToolPolicy` 兼容旧审计。
- [x] 在 trace 中固化 `ToolCapabilityDecision` 事件: 显式记录每个 capability 是否被允许、由哪条规则裁剪。`AgentEvent::ToolCapabilityDecision { tool, allowed, required, effective, denied, deny_reason, trace, .. }` 新 variant；`ReActAgent` 与 `PlanExecuteAgent` 在 `ToolPolicyDecision` 之后立即发射；`react/agent.rs` + `supervisor/{handoff,blackboard,debate}.rs` 的 step_index merge match 全部覆盖；`agent_runtime_react_trace.json` golden fixture 同步。
- [x] 文档: `docs/SKILL_PERMISSIONS.md` 写明三方决策合并算法与示例。新文件 178 行：capability 与 permission 的关系、四层交集语义、layer 顺序、permissive vs restrictive 的语义、worked example、向后兼容性说明。
- [x] CLI: `agentflow skill inspect --explain-permissions <skill>` 展示一次实际运行的最终决策路径。`Inspect` arg 新增 `--explain-permissions` 布尔；`commands/skill/inspect.rs` 对每个 built-in tool 打印 `required` / `effective` / `denied` / 每层 (`tool_required` / `skill_security` / `tool_policy` / `cli_flag`) 的 `allowed` / `running` / `dropped`；`skill_inspect_explain_permissions_prints_capability_decision` 是端到端 CLI 烟雾测试 (走 `examples/skills/rust_expert`)。
- 📊 PR-A 测试增量: agentflow-tools +12 单测 (capability 模块 9 + registry capability 集成 3)，agentflow-agents +1 (`tool_capability_decision_event_round_trips_through_serde`)，agentflow-cli +1 CLI smoke。`cargo test -p agentflow-tools -p agentflow-agents -p agentflow-skills -p agentflow-cli` 全绿；workspace `cargo clippy -- -D warnings` 干净。

- [x] `ShellTool` / `ScriptTool` wire-through (PR-B): 新增 `with_os_sandbox()` builder + 可注入的 `with_backend(...)`；执行前调用 `backend.wrap_command(&mut cmd, &caps, &scope)`；`build_scope_from_policy` 把 `SandboxPolicy.allowed_paths` 投影成 `SandboxScope`（permissive 默认回落到 `/tmp` + cwd），`build_script_scope` 始终允许 `scripts/` 目录读；默认 backend 仍是 `NoopSandboxBackend` 以保持向后兼容。
- [x] SkillBuilder 接线 (PR-B): `SecurityConfig::os_sandbox: bool`（默认 false）；`build_tool_registry` 在 `os_sandbox = true` 时对 `shell` / `script` 工具调用 `with_os_sandbox()`，文件 / HTTP 工具不变（无子进程）。
- [x] 集成测试 (PR-B): `agentflow-tools/tests/sandbox_macos.rs` 覆盖 baseline echo 成功 + 范围外写入被 sandbox-exec 拒绝；`agentflow-tools/tests/sandbox_linux.rs` 覆盖 baseline echo + Net 缺失时 `python3 socket.socket()` 触发 EPERM。两个文件分别用 `#[cfg(target_os = ...)]` 门控。
- 📊 PR-B 测试增量: agentflow-tools +6 (3 个 unit on shell scope + 3 个 macOS profile/linux filter unit)、+2 macOS 集成、+2 Linux 集成；workspace `cargo clippy -p agentflow-tools -p agentflow-skills --all-targets -- -D warnings` 干净；`cargo test -p agentflow-tools -p agentflow-skills` 全绿（含 sandbox_macos）。

验收标准:

- 在受限沙箱下运行的 `ShellTool` 越界访问被强制阻断 (sandbox 拒绝而非 policy 拒绝)。 ✅ (PR-B `macos_sandbox_blocks_write_outside_scope`)
- 一次 skill run 产出的 trace 包含可读的 capability 决策链路。 ✅ (PR-A)
- macOS / Linux 两条路径各自有集成测试。 ✅ (PR-B `tests/sandbox_macos.rs` + `tests/sandbox_linux.rs`)

涉及文件:

PR-A (已完成):

- `agentflow-tools/src/{capability,policy,registry,tool,lib}.rs`
- `agentflow-agents/src/{runtime,react/agent,plan_execute}.rs`、`agentflow-agents/src/supervisor/{handoff,blackboard,debate}.rs`、`agentflow-agents/tests/fixtures/agent_runtime_react_trace.json`
- `agentflow-skills/src/builder.rs`
- `agentflow-cli/src/main.rs`、`agentflow-cli/src/commands/skill/inspect.rs`、`agentflow-cli/tests/skill_cli_tests.rs`
- `docs/SKILL_PERMISSIONS.md`

PR-B (已完成):

- `agentflow-tools/src/sandbox/{mod,policy,backend,macos,linux,noop}.rs`（新模块拆分）
- `agentflow-tools/src/builtin/{shell,script}.rs`（接线 backend + scope）
- `agentflow-tools/Cargo.toml`（新增 `seccompiler` / `libc` 在 `cfg(target_os="linux")` 下的 dep）
- `agentflow-tools/tests/sandbox_macos.rs`、`agentflow-tools/tests/sandbox_linux.rs`
- `agentflow-skills/src/manifest.rs`（新 `os_sandbox: bool`）、`agentflow-skills/src/builder.rs`（接线）
- `docs/TOOL_PERMISSIONS.md`（追加 OS sandbox 章节，链接 SKILL_PERMISSIONS）

### 9. OpenTelemetry 端到端连续

状态: 待开始

目标:

- 解决一次 hybrid run 的 OTel trace 在 LLM hop 断裂的问题。
- 统一 workflow / agent / tool / MCP / LLM 五层 span 的属性命名。

子任务:

- [ ] `agentflow-llm` HTTP 客户端注入 `traceparent` header (W3C Trace Context 格式)。
- [ ] 统一 OTel 属性命名:
  - `agentflow.run_id`, `agentflow.workflow_id`, `agentflow.agent_session_id`, `agentflow.tool_call_id`, `agentflow.mcp_server`
  - `gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` (复用 OTel GenAI semantic conventions)
- [ ] 把 LLM 客户端封装成 OTel-aware: 入口创建 span，结束时记录 token 用量、HTTP 状态码、错误码。
- [ ] 集成测试: 用 stdout exporter 采一次 hybrid run 的 trace，断言 span 链路完整 (no orphan spans)。
- [ ] 文档: 更新 `docs/TRACING_DESIGN.md` 与 `docs/TRACING_USAGE.md`。

验收标准:

- 一次 `agentflow workflow run` 产出的 OTel trace 在 Jaeger/Tempo 中显示为连续树状结构，从 workflow → agent → tool → LLM HTTP call 的根到叶都不断。
- token 用量在 LLM span 上可见。

涉及文件:

- `agentflow-llm/src/{client,providers/*}.rs`
- `agentflow-tracing/src/otel.rs`
- `docs/TRACING_*.md`

### 10. RAG 评测 Harness

状态: 待开始

目标:

- 给 `agentflow-rag` 加上端到端评测能力，让知识库迭代可以被量化。

子任务:

- [ ] 在 `agentflow-rag/eval/` 新增模块: `Dataset` / `Judgment` / `Metric`。
- [ ] 支持指标: Recall@K (1, 3, 5, 10), MRR, nDCG@K, 平均延迟。
- [ ] 支持 baseline 对比: 同一数据集跑两份配置（如 BM25 vs vector，或不同 embedding 模型）。
- [ ] 标注集格式: TOML 或 JSONL `{ query, expected_doc_ids: [...], notes }`。
- [ ] 新增 CLI: `agentflow rag eval <dataset> [--config <yaml>]`，输出表格 + JSON 报告。
- [ ] 自带一个开源数据集（如 BEIR/SciFact 子集）作为 demo + CI smoke。
- [ ] 文档: `docs/RAG_EVAL.md`。

验收标准:

- `agentflow rag eval examples/datasets/scifact_mini.toml` 能输出 Recall@K / MRR / nDCG。
- baseline 对比有清晰的 winner 标注与显著性提示。

涉及文件:

- `agentflow-rag/src/eval/{mod,dataset,metrics}.rs`
- `agentflow-cli/src/commands/rag/eval.rs`
- `docs/RAG_EVAL.md`

### 11. 多 LLM provider 一致性回归套件

状态: 待开始

目标:

- 给 5 个真实 provider (OpenAI / Anthropic / Google / StepFun / Moonshot) 建立"行为一致性矩阵"，避免 silent regression。

子任务:

- [ ] 设计共用 fixture: 单轮 prompt、多模态 prompt、tool calling、streaming。
- [ ] 单测使用 `wiremock` 或 `httpmock` 模拟 provider HTTP 响应（避免依赖真实 API）。
- [ ] 集成测试 (gated by `AGENTFLOW_LIVE_LLM_TESTS=1`): 真实 API 调用，只在 nightly CI 跑。
- [ ] 在每个 provider 下断言: 文本回答字段位置、token 用量字段、错误码到 `LLMError` 的映射。
- [ ] 文档: `docs/LLM_PROVIDERS_MATRIX.md` 列出每个 provider 的支持矩阵。

验收标准:

- 5 个 provider 各自有一组共用的 fixture 测试，回归时能定位到具体 provider。
- nightly CI 有 live LLM smoke job（可选 enable）。

涉及文件:

- `agentflow-llm/tests/provider_consistency.rs`
- `docs/LLM_PROVIDERS_MATRIX.md`

---

## P2: N10 Plugin / 分布式 / Web UI / Agent SDK (v1.0.0-rc 候选)

### 12. Plugin / Custom Node 体系

状态: 待开始

目标:

- 让第三方节点和 Skill 不修改主仓库即可发布、加载、运行。

子任务:

- [ ] 评估文档 `docs/PLUGIN_DESIGN.md`: 对比 dlopen + abi_stable vs WASM (wasmtime/wasmer) vs subprocess + IPC 三条路径。决策依据: ABI 稳定性、跨平台、安全沙箱、调用开销、生态。
- [ ] 选定方案后实现最小 PoC: 一个独立 cargo 项目编译产物能被 AgentFlow 加载，注册一个新的 `AsyncNode` 类型，并能在 workflow 中使用。
- [ ] Plugin manifest 格式: 名称、版本、入口、声明的节点/工具、要求的 capabilities、签名（可选）。
- [ ] 生命周期: load → register → execute → unload；崩溃隔离策略。
- [ ] 权限模型: plugin 默认无 capabilities，必须显式声明并被用户批准。
- [ ] CLI: `agentflow plugin install/list/inspect/uninstall`。

验收标准:

- 一个独立仓库的 plugin 节点能被 AgentFlow 加载并运行；签名/版本/权限校验有记录。
- plugin 崩溃不影响主进程，错误信息可观察。

涉及文件:

- `agentflow-core/src/plugin/` (新)
- `agentflow-cli/src/commands/plugin/` (新)
- `docs/PLUGIN_DESIGN.md`

### 13. 分布式调度

状态: 待开始

目标:

- 让大型 DAG 可以分布式执行，跨多个 worker 节点。

子任务:

- [ ] 抽象 `WorkerProtocol` trait: 提交任务、领取任务、上报结果、心跳。
- [ ] 选定一种传输: gRPC (tonic) / NATS / Redis Streams 之一，其他保留扩展点。
- [ ] `agentflow-server` 进化为 control plane: 调度任务到 worker、聚合结果、维护 run state。
- [ ] worker 二进制: `agentflow-worker`，启动时连接 control plane。
- [ ] 跨 worker trace 拼接: worker 把本地 trace 通过协议回传，control plane 拼成完整 OTel trace。
- [ ] 文档: `docs/DISTRIBUTED.md`，给 2-worker 集群部署示例。

验收标准:

- 100+ 节点 workflow 能在 2 worker 集群上正确执行，trace 跨 worker 完整连续。
- 单 worker 故障时任务能被重派或标记失败，control plane 不挂。

涉及文件:

- `agentflow-server/src/scheduler/` (新)
- `agentflow-worker/` (新 crate)
- `docs/DISTRIBUTED.md`

### 14. Web UI 调试器

状态: 待开始

目标:

- 让混合执行 (DAG × Agent × Tool × MCP) 有可视化调试界面。
- TUI 保留作为 headless 替代。

子任务:

- [ ] 选型: React/Svelte SPA + Vite，TypeScript。
- [ ] 后端: 复用 `agentflow-server` 的 SSE 路由 + REST。
- [ ] 视图:
  - DAG 实时状态 (复用 `agentflow-viz` 的 Mermaid/DOT 输出 + 高亮当前节点)。
  - Agent step timeline (Observe → Plan → ToolCall → ToolResult → Reflect → FinalAnswer)。
  - Tool call 详情 (params / output / capability decision)。
  - Trace replay 视图 (替换或补强 TUI)。
- [ ] 部署: 静态资源打包到 `agentflow-server` 二进制（embed_files / rust-embed），单进程交付。
- [ ] 文档: `docs/WEB_UI.md`。

验收标准:

- 启动 `agentflow-server` 后访问 `http://localhost:8080/ui` 能看到 run 列表与实时 hybrid run 展开。
- TUI 仍然可用，作为 SSH/CI 场景替代。

涉及文件:

- `agentflow-ui/` (新前端目录)
- `agentflow-server/src/ui.rs` (静态资源 mount)
- `docs/WEB_UI.md`

### 15. Agent SDK 文档化

状态: 待开始

目标:

- 让第三方开发者能在 30 分钟内理解并扩展 AgentFlow agent runtime。

子任务:

- [ ] `docs/AGENT_SDK.md`: 五分钟入门 + 完整扩展点参考。
  - 实现自定义 `AgentRuntime`
  - 实现自定义 `ReflectionStrategy`
  - 实现自定义 `MemorySummaryBackend`
  - 实现自定义 `AgentStepKind` (extension variant) — 取决于是否在 N8 把 step 定义改为开放枚举
  - 实现自定义 `Tool` 与 `MemoryStore`
- [ ] 配套示例: 在 `agentflow-agents/examples/` 增加 `custom_runtime.rs`、`custom_reflection.rs`、`custom_memory_summary.rs`。
- [ ] 把所有公开 trait 的 rustdoc 补齐 + doc tests。

验收标准:

- 一个外部开发者按 `docs/AGENT_SDK.md` 能在 30 分钟内跑通"自定义 reflection strategy" 示例。
- `cargo doc --workspace --no-deps` 警告数 = 0（针对已选定的核心 trait）。

涉及文件:

- `docs/AGENT_SDK.md`
- `agentflow-agents/examples/custom_*.rs`
- 所有 `pub trait` 的 doc 注释

### 16. Plugin marketplace 远程化

状态: 待开始（依赖 12）

目标:

- 把当前的本地 Skill marketplace 与未来的 Plugin marketplace 统一为远程可分发的目录。

子任务:

- [ ] 设计 manifest schema: name / version / type (skill | plugin) / source (registry url + checksum) / signature。
- [ ] 远程 registry HTTP 接口 (read-only)。
- [ ] 本地缓存与签名校验。
- [ ] CLI: `agentflow marketplace search/install/update/verify`。
- [ ] 文档: `docs/MARKETPLACE.md`。

验收标准:

- 从远程 registry 安装一个 Skill / Plugin 并验证签名通过。
- 离线模式下仍能使用已缓存的 Skill / Plugin。

---

## 维护任务（持续）

### M1. 文档与 CLAUDE.md 同步

- [ ] 每完成一个 P0/P1 任务后立刻更新 `CLAUDE.md` "Recent Updates" + `RoadMap.md` 状态。
- [ ] 当 docs/ 文件描述的特性落地或变更时同步该文档。
- [ ] 每月一次 `docs/` 全量复查，移除过期描述。

### M2. CI 健康度

- [ ] 保持 `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` 全绿。
- [ ] 每个 P0 任务交付时同步增加最少一个集成测试。
- [ ] 测试数当前 479，目标 v0.3.0 ≥ 600，v0.4.0 ≥ 750。

### M3. 性能基准回归

- [ ] 大 DAG 调度 / ToolRegistry / MCP latency / agent loop prompt assembly 已有 benchmark；每次 release 前对比上一次基准，回归 > 10% 必须给出原因。

---

## 已完成执行顺序

1. P0-1 (N6-1): 已补齐 `agentflow workflow run` 的 input、dry-run、output、timeout、retry 行为。
2. P0-2 (N6-2): 已强化模型配置和模型切换 CLI，统一 `llm chat`、`workflow run`、`skill run/chat` 的 `--model` 语义。
3. P0-3 (N6-3): 已提升 Skill CLI 使用链路，完成 `install -> list -> inspect -> list-tools -> run --model --trace`。
4. P0-4 (N6-4): 已在 workflow YAML 中暴露 `agent` / `skill_agent` 节点，补齐 config-first hybrid。
5. P0-5 (N6-5): 已清理旧 CLI runner 和文档错位，避免 silent no-op。
6. P1-6 to P1-8 (N7): 已统一 trace/recovery，并建立权威端到端示例集。
7. 评估 (2026-04-28): `OVERALL_EVALUATION_REPORT.md` 完成。
8. 评估 (2026-05-01): `PROJECT_EVALUATION_2026-05-01.md` 完成；规划 N8/N9/N10 三档发布。
