# AgentFlow 当前执行计划

最后更新: 2026-05-08（P1 #11 final follow-up: live-LLM nightly CI gate 落地，`provider_consistency_live.rs` + `.github/workflows/llm-live.yml`）

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

- `a78c3ff test(llm): add cross-provider multimodal consistency fixtures`
- `0848e0d test(llm): add cross-provider streaming consistency fixtures to consistency suite`
- `3267640 test(llm): add cross-provider tool-calling fixtures to consistency suite`
- `b1f361f test(llm): land P1 #11 cross-provider consistency suite (foundation)`
- `49b8b88 feat(rag): land P1 #10 RAG eval harness with Recall/MRR/nDCG and baseline compare`

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

状态: 已完成 (2026-05-07)

目标:

- 解决一次 hybrid run 的 OTel trace 在 LLM hop 断裂的问题。
- 统一 workflow / agent / tool / MCP / LLM 五层 span 的属性命名。

子任务:

- [x] `agentflow-llm/src/trace_context.rs` 新模块: `LlmTraceContext { trace_id, span_id, flags, tracestate }` 类型 + W3C `traceparent` 序列化 + tokio `task_local!` 保存 + `scope(ctx, fut).await` 安装 + `inject_into_headers(&mut headers)` 写出。`LlmTraceContext::random()`（基于 UUIDv4）/ `LlmTraceContext::new(trace_id, span_id)`（hex 校验）/ `LlmTraceContext::from_traceparent(s)` 解析。9 条单测覆盖格式校验、嵌套 scope、permissive 默认。
- [x] 6 个 provider (OpenAI / Anthropic / Google / Moonshot / StepFun / StepFunSpecializedClient) 的 `build_headers()` / `build_auth_headers()` 末尾调用 `crate::trace_context::inject_into_headers(&mut headers)`，自动读 task-local。Mock provider 不发 HTTP，无需改动。
- [x] `LLMClient` 新增 `trace_context: Option<LlmTraceContext>` 字段 + `LLMClientBuilder::trace_context(impl Into<Option<...>>)` builder；`execute()` / `execute_full()` / `execute_streaming()` 在 `Some` 时把 `provider.execute(&request)` 包进 `trace_context::scope(...)`。
- [x] `AgentContext` 新增 `trace_context: Option<LlmTraceContext>` 字段 + `with_trace_context(...)` builder；`ReActAgent::run_with_context` 与 `PlanExecuteAgent::call_planner` 在每次 LLM 调用前 `.trace_context(context.trace_context.clone())` 透传。
- [x] OTel 属性命名已统一（`agentflow-tracing/src/otel.rs::trace_to_spans` 既有实现已遵循 `agentflow.workflow.id` / `agentflow.node.id` / `agentflow.agent.session_id` / `agentflow.tool.name` 与 GenAI 标准 `gen_ai.system` / `gen_ai.request.model` / `gen_ai.usage.input_tokens` / `gen_ai.usage.output_tokens` / `gen_ai.usage.total_tokens` / `gen_ai.response.latency_ms`）。本次 P1 #9 沿用，未改动。
- [x] 单测覆盖 6 provider × `build_headers` 注入 traceparent 行为：openai / anthropic / google / moonshot / stepfun / stepfun specialized 共 7 条新单测，外加 `build_headers_omits_traceparent_when_no_scope_active` 等 2 条 negative。`agentflow-llm` 单测 88 条全绿。
- [x] 文档: `docs/TRACING_USAGE.md` 追加「W3C Trace Context 端到端连续」章节（工作原理 / 自动传播 / 显式调用 / 与 OTel 后端对接 / 不注入条件）；`docs/TRACING_DESIGN.md` 在 OpenTelemetry 段后追加 wiring 摘要，并交叉链接 USAGE。
- [x] 真实 HTTP 集成测试: `agentflow-llm/tests/trace_context_propagation.rs` 用 `OpenAIProvider::with_client(no_proxy_client, ...)` + 自建 tokio TCP listener 验证 (a) 有 active context 时 `traceparent` 字面等于 context (b) 无 context 时 header 缺席 (c) 嵌套 scope 内层胜出。这条测试是 reqwest 升级 0.12 的副产物（之前 reqwest 0.11 / hyper 0.14 与 mockito 1.7 / hyper 1.x 版本冲突，无法跑）。

显式不做:

- 「stdout exporter + 端到端 hybrid run + 断言无孤儿 span」集成测试。**理由**: span lineage 性质已被 `agentflow-tracing/src/otel.rs::tests::maps_workflow_agent_tool_and_mcp_to_spans` / `maps_llm_usage_to_gen_ai_attributes` 两条 unit 测试覆盖；`trace_to_spans` 是 `ExecutionTrace → Vec<OtelSpan>` 的确定性映射，端到端 hybrid run 只是把同样的 fixture 通过更多间接层走一遍，性价比一般。日后做 P0 #1 platform skeleton（`RunExecutor` 真跑 Flow）时这条更有意义，到时再补。

验收标准:

- ✅ 一次 LLM HTTP 出站请求在有 active context 时携带 `traceparent: 00-{trace_id}-{span_id}-{flags}`，无 active context 时不注入（向后兼容）。`build_headers_injects_traceparent_when_scope_active` / `build_headers_omits_traceparent_when_no_scope_active` 单测覆盖。
- ✅ token 用量在 LLM span 上可见（`gen_ai.usage.total_tokens` 由 OTel exporter 写出，已存在；P1 #9 未改动）。

涉及文件:

- `agentflow-llm/src/trace_context.rs`（新）、`agentflow-llm/src/lib.rs`（pub use）
- `agentflow-llm/src/client/llm_client.rs`（新字段 + builder + execute scope wrap）
- `agentflow-llm/src/providers/{openai,anthropic,google,moonshot,stepfun}.rs`（build_headers 注入 + 单测）
- `agentflow-agents/src/runtime.rs`（AgentContext 新字段 + builder）
- `agentflow-agents/src/react/agent.rs`、`agentflow-agents/src/plan_execute.rs`（透传到 LLMClient）
- `docs/TRACING_USAGE.md`、`docs/TRACING_DESIGN.md`

### 10. RAG 评测 Harness

状态: 已完成 (2026-05-07)

目标:

- 给 `agentflow-rag` 加上端到端评测能力，让知识库迭代可以被量化。

子任务:

- [x] 在 `agentflow-rag/src/eval/` 新增模块: `Dataset` / `Judgment` / `Retriever` trait + `dataset` / `metrics` / `runner` / `compare` / `retrievers`。`Dataset::load_from_dir` 校验所有 judgment 引用的 query_id / doc_id 都已知，缺失立即报错而不是静默打 0 分。
- [x] 支持指标: `Recall@K`, `MRR`, `nDCG@K`（标准 `(2^rel - 1) / log2(i+1)` 公式 + IDCG 归一化, 支持 graded relevance），`LatencyAggregate` mean/p50/p95；macro-average 仅基于"至少有一条 relevant doc"的 query，`queries_with_relevant` 暴露分母。
- [x] 支持 baseline 对比: `eval::compare::compare(&baseline, &candidate)` 同 dataset paired sign-test，60% 阈值给 `CandidateWins` / `BaselineWins` / `Inconclusive` / `NotComparable { reason }` 四态判决；含 metric delta（abs + rel）和 paired wins/losses/ties。
- [x] 标注集格式: 三 JSONL 文件（`corpus.jsonl` / `queries.jsonl` / `qrels.jsonl`） + 可选 `dataset.toml`（name/version/source/license/description 五个 flat key）。
- [x] 新增 CLI: `agentflow rag eval --dataset <dir> [--retriever bm25] [-k 1,3,5,10] [--compare-to "k1=1.5,b=0.6"] [-o report.json]`。报告文本表 + 可选 JSON（含 dataset manifest / baseline / candidate / comparison）。
- [x] 自带一个开源数据集: `agentflow-rag/examples/datasets/agentflow_mini/`（16 docs / 12 queries / graded relevance 0-2，MIT，synthetic hand-authored）。
- [x] 文档: `docs/RAG_EVAL.md` 覆盖 dataset 格式 / metric 定义 / verdict 阈值 / JSON shape / 自定义 retriever。

📊 测试增量: agentflow-rag +25 单测 (metrics 9 + dataset 4 + runner 4 + retrievers 3 + compare 5) + 4 集成测试 (`tests/eval_harness.rs` BM25 在 demo 数据集上 Recall@5 ≥ 0.7、MRR ≥ 0.5、self-compare 必为 ties only inconclusive、tuned vs default compare 结构完整)。`cargo test -p agentflow-rag --all-targets` 113 单测 + 4 集成测试全绿，`cargo clippy --workspace --all-targets -- -D warnings` 干净。

验收标准:

- ✅ `agentflow rag eval --dataset agentflow-rag/examples/datasets/agentflow_mini --retriever bm25 -k 1,3,5,10` 输出 Recall/nDCG/MRR/Latency 表，BM25 在 demo dataset 上 Recall@5 = 0.96, MRR = 0.96。
- ✅ baseline 对比有清晰的 winner 标注（`candidate_wins` / `baseline_wins` / `inconclusive` / `not_comparable`），sign-test wins/losses/ties 明示，60% 阈值低于不下结论。

涉及文件:

- `agentflow-rag/src/eval/{mod,dataset,metrics,runner,retrievers,compare}.rs`（新模块）
- `agentflow-rag/src/lib.rs`（pub mod eval）
- `agentflow-rag/tests/eval_harness.rs`（4 集成测试）
- `agentflow-rag/examples/datasets/agentflow_mini/{dataset.toml,corpus.jsonl,queries.jsonl,qrels.jsonl}`
- `agentflow-cli/src/commands/rag/{mod,eval}.rs`、`agentflow-cli/src/main.rs`（`RagCommands::Eval` 变体 + 路由）
- `docs/RAG_EVAL.md`

### 11. 多 LLM provider 一致性回归套件

状态: 已完成 (2026-05-08)。基础 + streaming / tool-calling / multimodal / live-LLM nightly CI 全部落地；剩余 follow-up 仅为成本仪表板（与 P1 #11 解耦）

目标:

- 给 5 个真实 provider (OpenAI / Anthropic / Google / StepFun / Moonshot) 建立"行为一致性矩阵"，避免 silent regression。

子任务:

- [x] 给 4 个 provider (Anthropic / Google / Moonshot / StepFun) 加 `with_client(client, api_key, base_url)` 构造函数，与 OpenAI 看齐；这是把 `.no_proxy()` 测试 client 注入 provider 的前置条件，也支持生产侧的 HTTPS pinning / 共享连接池。
- [x] 设计共用 fixture: 单轮 prompt 已落地（每个 provider 的原生 wire format 各一个 success-fixture）。多模态 prompt / tool calling / streaming 留作 follow-up（per-provider 单测已覆盖，跨 provider 一致性未覆盖）。
- [x] 单测改用 hand-rolled `tokio::net::TcpListener` + `.no_proxy()` reqwest client（与 `trace_context_propagation.rs` 同模式），避免 `mockito` / `wiremock` 版本churn 与 macOS 系统代理把 loopback 黑洞的坑。`agentflow-llm/tests/provider_consistency.rs` 一个文件覆盖 5 provider × 2 路径 = 10 测试。
- [x] 在每个 provider 下断言: 文本回答字段位置 (`ContentType::Text` 含 `"ok"`)、token 用量字段 (prompt/completion/total tokens 三者皆 populated)、`StopReason::Stop` 收尾、错误码到 `LLMError` 的映射 (5 个 provider 各 1 条 error case 覆盖 401/429/500/503，统一 `LLMError::HttpError { status_code, .. }`)。
- [x] 文档: `docs/LLM_PROVIDERS_MATRIX.md` capability 矩阵 + error mapping 契约 + 验证策略 + 加新 provider 的 checklist + follow-up 列表。
- [x] **Follow-up (2026-05-08)**: streaming consistency tests 跨 provider 落地。`agentflow-llm/tests/provider_consistency.rs` +5 集成测试 (`{openai,anthropic,google,moonshot,stepfun}_streaming_path`) 共用 `assert_stream_yields_hello_world(...)` helper，每个 provider 用自己 native streaming wire format (OpenAI/Moonshot/StepFun SSE `data: {chunk}` + `[DONE]`、Anthropic SSE `event: …`/`data: …` + `message_stop`、Google newline-delimited JSON + `finishReason`) 注入 "Hello"/" world" 增量。新增 `spawn_streaming_mock_server(events)` 用 `Transfer-Encoding: chunked` 把每个 event 作为单独 HTTP 帧交付，避免单 Content-Length 把 events 合并到一次 `bytes_stream` 派发后续 events 丢失（per-provider stream parser 是 one-chunk-per-call）。共用契约：≥2 个 content-bearing chunk 拼接成 `"Hello world"`、至少一个 chunk 有 `is_final = true`、终止后 `next_chunk()` 返回 `Ok(None)`、`content_type == Some("text")`。
- [x] **Follow-up (2026-05-08)**: multimodal consistency tests (image + text inputs 跨 provider)。`agentflow-llm/tests/provider_consistency.rs` +5 集成测试 (`{openai,anthropic,google,moonshot,stepfun}_multimodal_path`) 共用 `run_multimodal(...)` helper：每个 provider 用自己 native multimodal wire format（OpenAI/Moonshot/StepFun 的 `image_url` part；Anthropic 的 `image` content block；Google 经 adapter 重写为 `inline_data`）注入同一份 marker base64 payload `"AAAA"`。共用契约: (a) 捕获到的请求体必须保留 marker payload; (b) provider-specific part-type 标识在请求体中存在 (`image_url` / `image` / `inline_data`); (c) 响应解析回退到与文本路径同一份 `(text, Stop, populated usage)` 契约。同时给 Google adapter 加了 `openai_content_to_gemini_parts()` —— 把 OpenAI-style `content: [{type:text}, {type:image_url, image_url:{url:"data:..."}}]` 翻译为 Gemini `parts[].inline_data` (data-URL) 或 `parts[].file_data` (remote URL)，配 5 条新 unit test。这之前 Google adapter 直接把整个 array 包进 `{"text": <array>}`，发到 Gemini 必然 400。
- [x] **Follow-up (2026-05-08)**: tool-calling fixtures 在 cross-provider consistency 层。`agentflow-llm/tests/provider_consistency.rs` +5 集成测试 (`{openai,anthropic,google,moonshot,stepfun}_tool_call_path`) 共用 `assert_tool_call(...)` helper，每个 provider 用自己的 native wire format (`tool_calls` array / `tool_use` content block / `functionCall` parts / OpenAI-compatible passthrough) 注入 `get_weather(city="Tokyo")` 响应，断言：解析出唯一一条 `ToolCallRequest`，`name == "get_weather"`、`arguments.city == "Tokyo"`、`id` 非空（Google 合成 `call_<idx>`）、`stop_reason == StopReason::ToolCalls`（含 Google `finishReason: STOP` → `ToolCalls` 的归一化）。`provider_request_with_tools(...)` 同时把 `tools`/`tool_choice` 编入请求确保请求侧编码路径不 panic。`cargo test -p agentflow-llm --test provider_consistency` 现 15 测试全绿；`cargo clippy -p agentflow-llm --all-targets -- -D warnings` 干净。
- [x] **Follow-up (2026-05-08)**: 集成测试 gated by `AGENTFLOW_LIVE_LLM_TESTS=1`。`agentflow-llm/tests/provider_consistency_live.rs` +6 测试 (5 provider × 单轮文本契约 + 1 条 gate-default-off 自检)，全部基于 `provider.execute(...)` 直接打公网 endpoint；当 gate 未开时 `eprintln!("[live] {provider}: skipped ({GATE_ENV} not set)")` 并立即返回，不发任何 HTTP 请求；当 gate 开但单个 provider 的 key 缺失时也只跳过该 provider，其他继续；每条测试 30s 内置超时。模型默认 `gpt-4o-mini` / `claude-3-5-haiku-20241022` / `gemini-1.5-flash` / `moonshot-v1-8k` / `step-1-8k`，可通过 `AGENTFLOW_LIVE_<PROVIDER>_MODEL` 覆盖。`.github/workflows/llm-live.yml`（cron `30 9 * * *` UTC + `workflow_dispatch` 接受可选 `providers` 子集 filter）独立于 `quality.yml::release-gate`，因此 PR 永远不会被 live 测试 gate；`max_tokens=16` + `temperature=0.0` 把每次 nightly 跑成本压到最低。`docs/LLM_PROVIDERS_MATRIX.md` 把 "Live LLM nightly CI job" 从 follow-ups 移到 closed follow-ups。

📊 测试增量: agentflow-llm `tests/provider_consistency.rs` +25 集成测试（5 success + 5 tool-call + 5 error mapping + 5 streaming + 5 multimodal）；`tests/provider_consistency_live.rs` +6 测试（5 provider live + 1 gate-default-off 自检，gate 关闭时全部毫秒级 ok）；`agentflow-llm` lib +6 单测 (Google adapter 多模态 helper)；`cargo test -p agentflow-llm` 94 lib + 25 provider_consistency + 3 trace_context + 6 provider_consistency_live = 128 tests 全绿；`cargo clippy -p agentflow-llm --all-targets -- -D warnings` 干净。

验收标准:

- ✅ 5 个 provider 各自有一组共用的 fixture 测试，回归时能定位到具体 provider（10 测试都按 provider 名命名）。
- ✅ Error-mapping 契约统一: 所有 HTTP provider 把非 2xx 映射成 `LLMError::HttpError { status_code, .. }`，下游消费者可信赖此契约不会被无声修改。
- ✅ nightly CI live LLM smoke job — `.github/workflows/llm-live.yml` cron `30 9 * * *` UTC + `workflow_dispatch`；offline 默认运行毫秒级 skip，PR 永远不被 live 测试 gate。

涉及文件:

- `agentflow-llm/src/providers/{anthropic,google,moonshot,stepfun}.rs`（新增 `with_client` 构造函数）
- `agentflow-llm/src/providers/google.rs`（`openai_content_to_gemini_parts` / `openai_content_to_text` / `parse_data_url` + `build_request_body` 多模态分支 + 5 新 unit test）
- `agentflow-llm/tests/provider_consistency.rs`（25 集成测试：5 success + 5 tool-call + 5 error mapping + 5 streaming + 5 multimodal）
- `agentflow-llm/tests/provider_consistency_live.rs`（6 测试，gate 关闭时干净跳过）
- `.github/workflows/llm-live.yml`（nightly + workflow_dispatch；独立于 `release-gate`）
- `docs/LLM_PROVIDERS_MATRIX.md`（live-LLM CI 章节从 "not yet implemented" 改写为 closed follow-up）

---

## P2: N10 Plugin / 分布式 / Web UI / Agent SDK (v1.0.0-rc 候选)

### 12. Plugin / Custom Node 体系

状态: PoC + workflow YAML 接入完成 (2026-05-08)；CLI 子命令 / 沙箱 / 签名留作后续

目标:

- 让第三方节点和 Skill 不修改主仓库即可发布、加载、运行。

子任务:

- [x] 评估文档 `docs/PLUGIN_DESIGN.md`: 对比 dlopen + abi_stable vs WASM (wasmtime/wasmer) vs subprocess + JSON-RPC 三条路径。结论: 选 subprocess + JSON-RPC 作为 v1.0.0-rc 主路径（OS 级崩溃隔离、复用 MCP/sandbox 设施、polyglot、零新主机重 dep），WASM 作为 v1.1+ 的 in-process tier 候选，dlopen 在开放生态中拒绝。
- [x] 选定方案后实现最小 PoC: `agentflow-core/src/plugin/`（manifest + protocol + host + node + registry），后端为 newline-delimited JSON-RPC 2.0 over stdio。`agentflow-core` 的 `[[bin]] agentflow-echo-plugin` 提供独立 entrypoint 作为参考插件，`examples/plugin_host_demo.rs` 可端到端运行；`tests/plugin_poc.rs` 4 个测试覆盖 load/handshake/execute/shutdown/未知节点类型/protocol 校验。
- [x] Plugin manifest 格式: `plugin.toml` (`[plugin] name/version/runtime/entrypoint/protocol` + `[[plugin.nodes]]` + `[plugin.capabilities]`)。runtime = `subprocess` 已实现，`wasm` 留 v1.1+。`[plugin.signature]` 字段位置在设计文档中保留。
- [x] 生命周期: `PluginHost::load → spawn child + handshake → register → execute (× N) → shutdown`。崩溃隔离来自 OS 进程边界；`shutdown` 幂等，`execute_node` after shutdown 返回结构化错误而不是 panic / hang；`Drop` 兜底 kill child。
- [x] 权限模型: `[plugin.capabilities]` → `agentflow-tools::sandbox` 接线已落地。`agentflow-core::plugin` 引入 `CommandPreparer` trait + `NoopCommandPreparer` + `PluginHostBuilder::with_command_preparer(...)`，让外部层在不引入平台 sandbox crate 的前提下 hook spawn-time 命令。`Capabilities` 新增 `filesystem_entries` / `requires_fs_read|fs_write|net|exec|env`，`FilesystemEntry::parse` 接受 `read:<p>` / `write:<p>` / 裸路径。`agentflow-cli/src/executor/plugin.rs` 新 `OsSandboxPluginPreparer` 适配器把 manifest 转译为 `Vec<Capability>` + `SandboxScope`（manifest 目录始终 read-allowed；空 filesystem 块回退到 `/tmp`；relative 路径相对 manifest dir 解析；`write` 同时算 read），随后调 `SandboxBackend::wrap_command`。CLI 通过 `AGENTFLOW_PLUGIN_SANDBOX=1` 环境变量 opt-in；缺省保持 v0.3 PoC 行为不变。`agentflow-core/tests/plugin_poc.rs` +2 集成测试 (`builder_invokes_command_preparer_before_spawn` 用 `CountingPreparer` 断言 hook 调用次数；`builder_surfaces_preparer_rejection` 验证拒绝路径返回 `PluginError::PreparerRejected` 且不 spawn 子进程)。`agentflow-core` lib +7 单测覆盖 `FilesystemEntry::parse` 边界条件与 `Capabilities::requires_*`。`agentflow-cli` `executor::plugin` 模块 +6 单测覆盖 capability 集合 / scope 解析 / 错误传播 / env-var 默认。`docs/PLUGIN_DESIGN.md` §6.5 重写为完整翻译表 + opt-in 文档；`docs/TOOL_PERMISSIONS.md` 末尾追加 "Plugin runtime: same backend, different bridge" 章节交叉链接。`cargo clippy -p agentflow-core -p agentflow-cli --features plugin --all-targets -- -D warnings` 干净，default-features 构建未受影响。
- [x] CLI: `agentflow plugin install/list/inspect/uninstall` (`agentflow-cli/src/commands/plugin/{mod,install,list,inspect,uninstall}.rs` + `Commands::Plugin` 路由)。所有命令 gated by `feature = "plugin"`，默认 plugins 目录为 `~/.agentflow/plugins/`，每个动词支持 `--dir` override。`install` 在拷贝前 validate manifest、防止递归拷贝、`--force` 覆盖、Unix 下保留可执行位、entrypoint 缺失只 warn 不 fail；`list` 扫描子目录列出 name/version/runtime/entrypoint 状态/declared nodes/capability 概要；`inspect` 接受 plugin 目录或 `plugin.toml` 路径，打印完整 manifest + 解析后的 entrypoint exists/executable 状态，**不** spawn 子进程；`uninstall` 删除 `<dir>/<name>/`，要求目标含 `plugin.toml` 才删（防误删），`--force` 让缺失场景幂等。`tests/plugin_cli_tests.rs` 6 条 e2e (round-trip / 无 manifest 拒绝 / `--force` 覆盖 / 未知插件失败 / 缺 manifest 拒删 / 空目录友好提示) 全绿；workspace `cargo clippy -p agentflow-cli --features plugin --all-targets -- -D warnings` 干净。docs/PLUGIN_DESIGN.md §10 给四个命令的 CLI Reference + 完整 build→install→list→inspect→uninstall 示例。
- [x] CLI workflow 执行器接入: 新增 `agentflow-cli` `plugin` cargo feature；`executor/factory.rs::create_graph_node` 路由 `type: plugin`，`executor/plugin.rs::PluginWorkflowNode` 在每个 `workflow run` 进程内通过 `Mutex<HashMap<PathBuf, Arc<PluginHost>>>` 缓存按 manifest 路径复用 host；`config/schema.rs` 接受 `plugin` 节点 + 缺 `manifest`/`node_type` 时给出 schema 错误；CI `features` matrix 增加 `cli-plugin` (`cargo check --no-default-features --features plugin`) 和 `core-plugin` (`cargo check --features plugin --all-targets`) 两条新行。`tests/workflow_tests.rs::plugin_node_tests` 新增 2 条端到端 CLI 测试 (`cli_workflow_run_supports_plugin_node` / `cli_workflow_run_rejects_plugin_node_missing_manifest`)。`docs/PLUGIN_DESIGN.md` §9 给出 YAML schema、resolution 规则、生命周期、build/run 命令与失败模式。

验收标准:

- [x] 内置参考 plugin (`agentflow-echo-plugin`) 能被 host 加载并执行；输入 `text:"hello plugin"` 返回 `text:"HELLO PLUGIN"`。
- [x] plugin 崩溃 / shutdown 后再次 execute 不会让 host 进程 panic 或 hang，统一返回 `AgentFlowError::AsyncExecutionError` envelope。
- [x] OS 沙箱接线已就绪（macOS sandbox-exec / Linux seccomp）—— `AGENTFLOW_PLUGIN_SANDBOX=1` opt-in，缺省保持 v0.3 PoC 行为不变。
- [x] 一个独立仓库的 plugin（不在 workspace 内）端到端跑通——`type: plugin` workflow 节点 + `agentflow plugin install <source-dir>` CLI 已就绪。out-of-tree 流程：开发者在自己仓库 `cargo build` 生成 entrypoint → `agentflow plugin install ./my-plugin` 拷贝到 `~/.agentflow/plugins/` → workflow YAML `manifest:` 指向已安装路径即可。远程仓库拉取属于 P2 #16 marketplace 范畴。
- [ ] 签名 / 版本校验有记录——manifest 字段已就绪，校验逻辑由 P2 #16 marketplace 任务承接。

验证:

```bash
cargo test -p agentflow-core --features plugin --test plugin_poc
cargo run  -p agentflow-core --features plugin --example plugin_host_demo
cargo test -p agentflow-cli --features plugin --test workflow_tests plugin_node_tests
cargo test -p agentflow-cli --features plugin --test plugin_cli_tests
cargo clippy -p agentflow-core -p agentflow-cli --features plugin --all-targets -- -D warnings
```

涉及文件:

- `docs/PLUGIN_DESIGN.md` (新)
- `agentflow-core/src/plugin/{mod,manifest,protocol,host,node,registry}.rs` (新)
- `agentflow-core/src/bin/echo_plugin.rs` (新参考插件)
- `agentflow-core/examples/plugin_host_demo.rs` (新)
- `agentflow-core/tests/plugin_poc.rs` (新)
- `agentflow-core/Cargo.toml` (新增 `plugin` feature + 可选 `toml` dep)
- `agentflow-cli/Cargo.toml` (新 `plugin` feature)
- `agentflow-cli/src/executor/{mod,factory,plugin}.rs` (新 `plugin` 模块 + factory 路由)
- `agentflow-cli/src/config/schema.rs` (`plugin` 节点 schema + feature_hint)
- `agentflow-cli/tests/workflow_tests.rs` (`plugin_node_tests` 模块 2 条端到端测试)
- `agentflow-cli/src/commands/plugin/{mod,install,list,inspect,uninstall}.rs` (新 CLI 子命令 module，feature-gated)
- `agentflow-cli/src/commands/mod.rs` (`pub mod plugin` feature-gated)
- `agentflow-cli/src/main.rs` (`Commands::Plugin(PluginArgs)` + `PluginCommands` enum + dispatch)
- `agentflow-cli/tests/plugin_cli_tests.rs` (6 条端到端测试)
- `.github/workflows/quality.yml` (`cli-plugin` / `core-plugin` matrix)
- `agentflow-core/src/plugin/manifest.rs` (`FsAccess` / `FilesystemEntry::parse` / `Capabilities::requires_*` + `ManifestError::InvalidCapability`)
- `agentflow-core/src/plugin/host.rs` (`CommandPreparer` trait + `NoopCommandPreparer` + `PluginHostBuilder` + `Connection::spawn` 接受 preparer + `PluginError::PreparerRejected`)
- `agentflow-core/src/plugin/mod.rs` (新 re-exports: `CommandPreparer` / `NoopCommandPreparer` / `PluginHostBuilder` / `FilesystemEntry` / `FsAccess`)
- `agentflow-core/tests/plugin_poc.rs` (+2 集成测试覆盖 builder + preparer hook)
- `agentflow-cli/src/executor/plugin.rs` (新 `OsSandboxPluginPreparer` 适配器 + `preparer_from_env` + 6 单测)
- `docs/PLUGIN_DESIGN.md` §6.5 (重写为完整翻译表 + opt-in 文档)
- `docs/TOOL_PERMISSIONS.md` (追加 "Plugin runtime: same backend, different bridge" 交叉链接章节)

### 13. 分布式调度

状态: 进行中 (2026-05-08: `WorkerProtocol` 抽象 + gRPC 选型 + `docs/DISTRIBUTED.md` 初版)

目标:

- 让大型 DAG 可以分布式执行，跨多个 worker 节点。

子任务:

- [x] 抽象 `WorkerProtocol` trait: 提交任务、领取任务、上报结果、心跳。`agentflow-server/src/scheduler/mod.rs` 新增 `WorkerTask` / `WorkerTaskResult` / `WorkerHeartbeat` / `WorkerTraceEvent` / `SchedulerError`，并提供 `InMemoryWorkerProtocol` 锁定 FIFO claim、claiming-worker result 校验和 heartbeat 语义。
- [x] 选定一种传输: gRPC (tonic) / NATS / Redis Streams 之一，其他保留扩展点。当前选择 gRPC + tonic 作为 v1.0-rc 主路径，NATS / Redis Streams 作为后续 adapter 扩展点；`WorkerTransport` 记录选择，`docs/DISTRIBUTED.md` 写明 rationale。
- [ ] `agentflow-server` 进化为 control plane: 调度任务到 worker、聚合结果、维护 run state。
- [ ] worker 二进制: `agentflow-worker`，启动时连接 control plane。
- [ ] 跨 worker trace 拼接: worker 把本地 trace 通过协议回传，control plane 拼成完整 OTel trace。
- [x] 文档: `docs/DISTRIBUTED.md`，给 2-worker 集群部署示例。当前为设计与目标部署形态；真实 `agentflow-worker` CLI 待后续子任务落地。

验收标准:

- 100+ 节点 workflow 能在 2 worker 集群上正确执行，trace 跨 worker 完整连续。
- 单 worker 故障时任务能被重派或标记失败，control plane 不挂。

涉及文件:

- `agentflow-server/src/scheduler/` (新，已新增 protocol 抽象)
- `agentflow-worker/` (新 crate)
- `docs/DISTRIBUTED.md` (已新增)

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

状态: 已完成 (2026-05-07)

目标:

- 让第三方开发者能在 30 分钟内理解并扩展 AgentFlow agent runtime。

子任务:

- [x] `docs/AGENT_SDK.md`: 五分钟入门 + 完整扩展点参考。
  - [x] 实现自定义 `AgentRuntime`（`agentflow-agents/examples/custom_runtime.rs`，无 LLM 的最小骨架）。
  - [x] 实现自定义 `ReflectionStrategy`（`agentflow-agents/examples/custom_reflection.rs`，mock provider + ReActAgent）。
  - [x] 实现自定义 `MemorySummaryBackend`（`agentflow-agents/examples/custom_memory_summary.rs`，直接调用 trait + 集成到 `ReActAgent`）。
  - [N/A] 实现自定义 `AgentStepKind` (extension variant)：N8 期间未把 `AgentStepKind` 改为开放枚举，故文档中以 "Closed enums and stability" 一节明确该边界（继续 reuse 既有 variants，必要时开 issue 讨论）。
  - [x] 实现自定义 `Tool` 与 `MemoryStore`：以扩展点表 + 内置实现指引覆盖（`agent_native_react.rs::EchoTool` / `SessionMemory` / `SqliteMemory` 作为参考实现）。
- [x] 配套示例: 在 `agentflow-agents/examples/` 增加 `custom_runtime.rs`、`custom_reflection.rs`、`custom_memory_summary.rs`，全部使用 mock provider 或纯结构化骨架，无需 API key。
- [x] 把核心扩展 trait 的 rustdoc 补齐：`AgentRuntime` / `AgentRuntimeError` / `RuntimeLimits` / `AgentContext` / `AgentStep(Kind)` / `AgentStopReason` / `AgentRunResult` / `AgentCancellationToken` / `AgentMemoryHook` / `MemoryHookKind` / `ReflectionStrategy` / `ReflectionContext` / `ReflectionTrigger` / `Reflection` / `ReflectionError` / `MemorySummaryBackend` / `MemorySummaryContext`；并修掉了 `agentflow-agents` / `agentflow-tools` / `agentflow-memory` / `agentflow-cli` 的 9 条 broken intra-doc / redundant link 警告（`registry.rs` / `sandbox/mod.rs` / `sandbox/backend.rs` / `react/agent.rs` / `supervisor/handoff.rs` / `supervisor/mod.rs` / `tools/agent_tool.rs` / `tools/workflow_tool.rs` / `executor/multi_agent.rs`）。

验收标准:

- [x] 一个外部开发者按 `docs/AGENT_SDK.md` 能在 30 分钟内跑通"自定义 reflection strategy" 示例：`cargo run -p agentflow-agents --example custom_reflection` 在 mock provider 下端到端走通 ReAct loop 并打印 `Reflect` step。
- [x] `cargo doc --workspace --no-deps` 警告数 = 0（针对已选定的核心 trait）：`cargo doc -p agentflow-agents -p agentflow-tools -p agentflow-memory --no-deps` 0 warning（其余 crate 仍有 14 条与扩展点无关的 broken-link 警告，超出 #15 范围）。

涉及文件:

- `docs/AGENT_SDK.md`、`docs/AGENT_RUNTIME.md`（追加 SDK 链接）。
- `agentflow-agents/examples/custom_runtime.rs`、`custom_reflection.rs`、`custom_memory_summary.rs`。
- `agentflow-agents/src/runtime.rs`、`reflection.rs`、`react/agent.rs`、`supervisor/{mod,handoff}.rs`、`tools/{agent_tool,workflow_tool}.rs`。
- `agentflow-tools/src/{registry,sandbox/mod,sandbox/backend}.rs`、`agentflow-cli/src/executor/multi_agent.rs`。

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
9. P2-15 (2026-05-07): `docs/AGENT_SDK.md` + `custom_runtime` / `custom_reflection` / `custom_memory_summary` 三个 mock-only 示例落地，核心扩展 trait rustdoc 补齐，相关 crate `cargo doc` 警告归零。
10. P1-11 follow-up (2026-05-08): 跨 provider tool-calling 一致性 fixture 落地。`provider_consistency.rs` 15 集成测试 (5 success + 5 tool-call + 5 error mapping)，5 个 provider 各用自己 native wire format (`tool_calls` / `tool_use` / `functionCall`) 跑同一条 `get_weather(city="Tokyo")` 契约，覆盖 id 合成、stop_reason 归一化、arguments 结构化解析。
11. P1-11 follow-up (2026-05-08): 跨 provider streaming 一致性 fixture 落地。`provider_consistency.rs` 现 20 集成测试 (+5 streaming)，每个 provider 用自己 native streaming wire format（OpenAI/Moonshot/StepFun SSE `data:` + `[DONE]`、Anthropic SSE event/data + `message_stop`、Google newline-delimited JSON + `finishReason`）跑同一条 `"Hello world"` 拼接契约。新增 `spawn_streaming_mock_server` 用 `Transfer-Encoding: chunked` 还原真实 LLM 流式语义，确保每个 event 走独立 HTTP 帧。
12. P1-11 follow-up (2026-05-08): live-LLM nightly CI gate 落地。`agentflow-llm/tests/provider_consistency_live.rs` (5 provider × 单轮文本契约 + 1 gate-default-off 自检) + `.github/workflows/llm-live.yml`（cron `30 9 * * *` UTC + `workflow_dispatch` + 可选 `providers` 子集 filter）；`AGENTFLOW_LIVE_LLM_TESTS` 关闭时 6 条测试全部毫秒级跳过、不发任何 HTTP 请求；开启时单条 provider 缺 key 也只 skip 该 provider，其他继续。模型默认压在 `gpt-4o-mini` / `claude-3-5-haiku` / `gemini-1.5-flash` / `moonshot-v1-8k` / `step-1-8k`，可由 `AGENTFLOW_LIVE_<PROVIDER>_MODEL` 覆盖；`max_tokens=16` + `temperature=0` 把每次 nightly 跑成本压到最低。`docs/LLM_PROVIDERS_MATRIX.md` 把这条从 follow-up 移到 closed follow-up，header 状态行同步更新。P1 #11 整体闭环。
12. P1-11 follow-up (2026-05-08): 跨 provider multimodal 一致性 fixture 落地。`provider_consistency.rs` 现 25 集成测试 (+5 multimodal)，每个 provider 用 native multimodal 格式（OpenAI/Moonshot/StepFun 的 `image_url`、Anthropic 的 `image` content block、Google 经 adapter 重写为 `inline_data`）注入 marker base64 payload 并断言请求体保留 + 响应解析回退到统一文本契约。同步给 Google adapter 实装 OpenAI-style multimodal content 翻译（`openai_content_to_gemini_parts`），把 data-URL 译成 `inline_data`、远程 URL 译成 `file_data`。修复了之前 Google adapter 把整个 content array 直接塞进 `{"text": <array>}` 的错误编码。
