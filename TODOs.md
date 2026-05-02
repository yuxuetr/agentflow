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

- `41ed3f8 docs: refresh active documentation`
- `54f782e docs: sync runtime config task hashes`
- `cbd83f3 feat(cli): support trace dir env default`
- `20701b8 feat(core): configure workflow run artifacts dir`
- `18695e0 docs: sync p1 task status`

---

## P0: N8 平台骨架 + 原生 Tool Calling + 保真度修复 (v0.3.0 候选)

### 1. 平台服务端最小骨架

状态: 待开始

目标:

- 把 `agentflow-server` / `agentflow-db` 从骨架推进到可独立部署的 control plane。
- 让"提交一次 workflow run、订阅事件、查询状态"全部走 HTTP，而不是命令行。

关键路径:

- 复用 `Flow::execute_*` 与 `agentflow-tracing` 已有能力，server 只承担 routing / persistence / streaming。
- DB schema 与 `agentflow-tracing` Postgres backend 共享（避免双写）。

子任务:

- [ ] `agentflow-db`: 引入 sqlx-migrate 或 refinery，落实 6 张表的 schema:
  - `runs(id, workflow, status, started_at, finished_at, run_dir, tenant_id)`
  - `steps(run_id, node_id, kind, status, started_at, duration_ms, payload)`
  - `events(run_id, seq, kind, payload, ts)`
  - `artifacts(run_id, node_id, name, path_or_url, mime_type)`
  - `skill_installs(name, version, source, installed_at, checksum)`
  - `mcp_sessions(id, server, started_at, ended_at, tool_calls)`
- [ ] `agentflow-db`: 建立 Repository trait（`RunRepo`/`StepRepo`/`EventRepo`/`ArtifactRepo`），给 `agentflow-server` 复用。
- [ ] `agentflow-server`: 实现路由
  - `POST /v1/runs` 提交 workflow（接受 YAML body 或 workflow_id 引用）
  - `GET /v1/runs/{id}` 返回当前状态 + 最后一步
  - `GET /v1/runs/{id}/events` 通过 SSE 流式推送 trace events
  - `POST /v1/skills/{name}:run` 触发 skill agent 单次运行
  - `GET /v1/skills` 列出本地 skill registry
- [ ] `agentflow-server`: 接 `agentflow-tracing` 的 `EventListener`，每个事件落库 + 推送给订阅者。
- [ ] `agentflow-server`: 错误响应统一为 `{ "error": { "code", "message", "details" } }`。
- [ ] 基础 AuthN: `Authorization: Bearer <token>` 简单匹配 env var；为后续 OAuth 留扩展点。
- [ ] 增加端到端测试: `cargo test -p agentflow-server`，覆盖 run 提交、状态查询、SSE 订阅、skill run。
- [ ] 更新 `docs/DEPLOYMENT.md`，给出最小 server + db docker-compose 示例。

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

状态: 待开始

目标:

- 在 `agentflow-agents/supervisor` 沉淀 handoff / blackboard / debate 三种生产可用的协作范式，每种范式有权威示例与单测。

子任务:

- [ ] 设计 `Supervisor` 的三种 trait 实现:
  - `HandoffSupervisor`: 角色切换式，由当前 agent 决定 handoff 给哪个 next agent；记录 handoff 链。
  - `BlackboardSupervisor`: 共享状态板（同一 `MemoryStore` 切片），多 agent 顺序或并发读写。
  - `DebateSupervisor`: 多 agent 各自给出方案 → 评审 agent 投票/合并。
- [ ] 每种范式: 一个权威 example (`agentflow-agents/examples/`) + 一组 mock LLM 单测。
- [ ] CLI/YAML 暴露: 增加 `multi_agent` 节点类型，`mode: handoff|blackboard|debate`，引用多个 skill。
- [ ] 文档: `docs/MULTI_AGENT.md` 给三种范式的决策图与示例。

验收标准:

- 一个"研究 + 写作 + 评审"三 agent 协作的 example 进入 CI smoke (mock LLM)。
- 三种范式的单测覆盖关键路径，各自至少 5 个测试。

涉及文件:

- `agentflow-agents/src/supervisor/{mod,handoff,blackboard,debate}.rs`
- `agentflow-agents/examples/`
- `agentflow-cli/src/config/schema.rs`、`agentflow-cli/src/executor/factory.rs`
- `docs/MULTI_AGENT.md`

### 8. 工具进程级沙箱

状态: 待开始

目标:

- 把 `ShellTool` / `ScriptTool` 从声明式权限提升到进程级强 enforcement。
- 让权限模型从"过滤"升级到"裁剪"。

子任务:

- [ ] 在 `agentflow-tools/src/sandbox.rs` 引入平台抽象:
  - macOS: `sandbox-exec` profile 模板。
  - Linux: `seccomp-bpf` syscall whitelist + chroot/mount namespace 子集。
  - 其他平台: 显式不支持，工具调用拒绝并给出可操作建议。
- [ ] 在 `Tool` trait 增加 `requires_capabilities() -> Vec<Capability>`，枚举 `Capability::{FsRead, FsWrite, Net, Exec, Env}`。
- [ ] 实现三方权限合并算法: SkillSecurity → ToolPolicy → CLI flag → effective capabilities，每一步可观察。
- [ ] 在 trace 中固化 `ToolCapabilityDecision` 事件: 显式记录每个 capability 是否被允许、由哪条规则裁剪。
- [ ] 文档: `docs/SKILL_PERMISSIONS.md` 写明三方决策合并算法与示例。
- [ ] CLI: `agentflow skill inspect --explain-permissions <skill>` 展示一次实际运行的最终决策路径。

验收标准:

- 在受限沙箱下运行的 `ShellTool` 越界访问被强制阻断 (sandbox 拒绝而非 policy 拒绝)。
- 一次 skill run 产出的 trace 包含可读的 capability 决策链路。
- macOS / Linux 两条路径各自有集成测试。

涉及文件:

- `agentflow-tools/src/{sandbox,policy,tool,builtin/shell,builtin/script}.rs`
- `agentflow-skills/src/manifest.rs` (security 字段对接)
- `docs/SKILL_PERMISSIONS.md`、`docs/TOOL_PERMISSIONS.md`

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
