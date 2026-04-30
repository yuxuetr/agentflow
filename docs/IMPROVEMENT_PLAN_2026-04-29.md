# AgentFlow 改进方案计划

制定日期: 2026-04-29  
依据文档: `docs/PROJECT_EVALUATION_2026-04-29.md`  
目标周期: 8-12 周，分为 P0/P1/P2 三个阶段推进。

## 1. 改进目标

本计划不再优先扩展功能面，而是把现有能力收敛成稳定、可治理、可观测、可托管的产品 contract。

核心目标:

1. 让 config-first workflow/agent/skill 的 schema、错误、输出行为稳定。
2. 让 tool/MCP/skill 执行有可强制的安全策略和审计记录。
3. 让 DAG、agent、tool、MCP、LLM 的运行过程可度量、可追踪、可恢复。
4. 让 server 从 health gateway 逐步演进为 run/trace/skill 管理入口。
5. 明确 Skills、MCP、Tools、Plugins 的边界，避免生态概念混乱。

## 2. 总体里程碑

| 阶段 | 名称 | 周期 | 目标 |
| --- | --- | --- | --- |
| P0 | 生产一致性与用户体验 | 2-3 周 | 修复最影响用户预期和安全治理的短板。 |
| P1 | Runtime、观测与恢复深化 | 3-5 周 | 提升调度、恢复、指标、trace storage 和评测能力。 |
| P2 | 平台化与生态扩展 | 4-6 周 | 建立 server API、远程 marketplace、正式插件边界。 |

## 3. P0: 生产一致性与用户体验

### P0-1. 统一 workflow YAML schema 和节点参数校验

状态: 已完成

问题:

- `FlowDefinitionV2` 只定义了通用 YAML 结构，各节点参数仍由 factory 和节点实现分散解析。
- 错误信息可读性不一致，机器可读错误 contract 不稳定。
- 新节点扩展时容易漏文档、漏校验、漏 CLI 测试。

改进方案:

- 在 `agentflow-cli` 或 `agentflow-nodes` 增加 `NodeSchema` / `NodeParameterSchema` 模型。
- 为每个 config-first 节点定义:
  - required parameters
  - optional parameters
  - type
  - default
  - description
  - feature gate
  - sensitive fields
- `workflow validate` 和 `workflow run --dry-run` 先执行统一 schema validation。
- 错误输出包含 workflow file、node id、node type、parameter path、expected type、actual value summary。
- 增加 `--format json` 或等价机器可读输出，用于 CI 和 server 后续复用。

已完成:

- 新增 CLI workflow schema validation 模块，覆盖主要 factory 节点的 required/optional 参数和基础类型。
- `workflow run` 在构建 graph 前执行 schema validation，`--dry-run` 也会提前失败。
- 新增 `workflow validate <file>` 独立入口。
- `workflow validate <file> --format json` 输出机器可读 schema report，包含 workflow、valid、issues、warnings。
- unknown parameters 默认保持兼容性 warning，`workflow validate --strict` 可升级为 error 供 CI 使用。
- 新增 `docs/WORKFLOW_SCHEMA.md`，记录节点参数 contract、参数类型、input_mapping 规则和 nested node 校验规则。
- `workflow debug --validate` 复用 schema validation。
- 对未启用 feature 的 `mcp` / `rag` 节点输出明确 feature gate 提示。
- CLI/单元测试已覆盖缺失 required 参数、feature-gated MCP 节点、JSON report 输出、unknown parameter 严格模式，以及 10 个代表性 config-first 节点。

剩余:

- 继续随新增节点维护 schema 表和测试样例。

涉及模块:

- `agentflow-cli/src/config/v2.rs`
- `agentflow-cli/src/executor/factory.rs`
- `agentflow-cli/src/commands/workflow/validate.rs`
- `agentflow-nodes`

验收标准:

- 任意缺失 required 参数都能在执行前报错，不启动外部 API/MCP/RAG。
- 错误能定位到 `nodes.<id>.parameters.<key>`。
- `mcp`、`rag` feature-gated 节点在 feature 未启用时给出明确错误。
- 覆盖至少 10 个代表节点: `llm`、`template`、`file`、`http`、`skill_agent`、`mcp`、`rag`、`map`、`while`、`tts`。

建议验证:

```bash
cargo test -p agentflow-cli --test workflow_tests --target-dir /tmp/agentflow-target
cargo run -p agentflow-cli -- workflow debug agentflow-cli/examples/workflows/fixed_dag_basic.yml --validate
```

### P0-2. 移除独立 LLM chat 入口并清理 experimental 命令

问题:

- `agentflow llm chat` 是早期“直接和某个模型对话”的入口，但 AgentFlow 当前定位是智能体框架，交互体验应围绕 Agent/Skill/Workflow，而不是裸模型聊天。
- `agentflow llm chat` 已进入退休路径；当前重点是清理文档、测试和兼容提示，避免它继续作为产品入口出现。
- voice cloning 命令显式未实现。
- 这些入口会制造“看起来支持但实际不可用”或“产品主线是模型聊天”的错误预期。

改进方案:

- 将 `agentflow llm chat` 从推荐路径中移除:
  - 从 help、README、examples、tutorials 中隐藏或删除。
  - 兼容期内执行该命令返回明确错误，提示使用 `agentflow skill chat`、`agentflow skill run` 或 workflow 中的 `skill_agent` 节点。
  - 不再实现 `--load` / `--save`，因为会把裸模型聊天继续产品化。
- `agentflow llm` 命名空间保留为模型配置与诊断入口:
  - `agentflow llm models` 用于查看可用模型和 capabilities。
  - 如需快速验证模型连通性，新增或保留非对话式 `agentflow llm probe --model ... --prompt ...`，只作为诊断命令，不保存会话，不承担 agent 交互体验。
- 将文档中的“切换模型 chat”类示例改为“切换模型运行 Skill/Agent”:
  - `agentflow skill chat ./skills/code-reviewer --model ...`
  - `agentflow skill run ./skills/code-reviewer --model ...`
  - `agentflow workflow run flow.yml --model ...`
- 对 voice cloning:
  - 短期建议隐藏命令或标记为 `experimental` 且默认不可见。
  - 如果保留，必须在 help 中明确 `not implemented`，并增加测试锁定错误行为。

涉及模块:

- `agentflow-cli/src/main.rs`
- `agentflow-cli/src/commands/llm/mod.rs`
- `agentflow-cli/src/commands/llm/models.rs`
- `agentflow-cli/src/commands/audio/clone.rs`
- `agentflow-cli/tests/config_cli_tests.rs` 或新增 CLI tests
- `README.md`
- `docs/examples/cli_config_first_tutorial.md`
- `docs/CONFIGURATION.md`

验收标准:

- `agentflow llm chat` 不再作为可用对话入口出现在 help、README 和教程中。
- 如果用户仍调用 `agentflow llm chat`，命令返回非零 exit code，并给出迁移建议: 使用 `skill chat/run` 或 `workflow run` 的 `skill_agent`。
- `llm chat --load/--save` 不再存在 warning 后继续忽略的 no-op 行为。
- 模型连通性验证若保留，必须是非会话式 probe/diagnostic，不形成第二套聊天体验。
- `--help` 不展示不可用能力，或明确显示 experimental/unavailable。
- 对不可用能力返回非零 exit code。

建议验证:

```bash
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
```

### P0-3. 补齐 MCP tool JSON Schema validation

状态: 已完成

问题:

- `agentflow-mcp/src/client/tools.rs` 当前只检查 arguments 是 object。
- schema 错误要等 server 返回，client 侧无法提前发现。

改进方案:

- 在 `agentflow-mcp` 增加 `jsonschema` 依赖，或复用 `agentflow-tools` 中已有 schema validation 能力。
- 对 MCP `Tool.inputSchema` 做 draft 兼容校验。
- 区分:
  - invalid schema: server/tool metadata 问题
  - invalid arguments: caller 参数问题
- CLI `mcp call-tool` 和 Skill/MCP adapter 输出参数错误上下文。

涉及模块:

- `agentflow-mcp/src/client/tools.rs`
- `agentflow-skills/src/mcp_tools.rs`
- `agentflow-cli/src/commands/mcp/call_tool.rs`

验收标准:

- 缺少 required field、类型不匹配、enum 不合法都在 client 侧失败。
- 错误包含 tool name、argument path、schema reason。
- MCP integration tests 增加成功/失败 schema case。

已完成:

- `agentflow-mcp` 复用 `jsonschema`，对 `Tool.inputSchema` 做 client-side 参数校验。
- `call_tool_validated`、CLI `mcp call-tool` 和 Skill MCP adapter 均在远程调用前执行 schema validation。
- 错误区分 invalid input schema 与 invalid arguments，并包含 tool name 和参数路径上下文。
- 单元测试覆盖 required、type、enum、invalid schema，以及 Skill adapter 调用前失败路径。
- 已通过 `cargo test -p agentflow-mcp -p agentflow-skills --target-dir /tmp/agentflow-target`。

建议验证:

```bash
cargo test -p agentflow-mcp --target-dir /tmp/agentflow-target
cargo test -p agentflow-skills --target-dir /tmp/agentflow-target
```

### P0-4. 建立 Tool Policy Decision 和审计记录

状态: 已完成

问题:

- Tool permission/source metadata 已存在，但还不是统一强制策略。
- 缺少每次 tool call 的 allow/deny 决策记录。

改进方案:

- 新增 `ToolPolicy` / `PolicyEngine` 抽象。
- 输入:
  - tool name
  - source
  - permissions
  - skill security config
  - runtime context
  - requested params summary
- 输出:
  - allowed/denied
  - matched rule
  - deny reason
  - sensitive fields redaction hint
- `ToolRegistry::execute` 或 agent runtime 调用前必须经过 policy decision。
- 将 decision 写入 `AgentEvent` 和 `ToolCallTrace`。

涉及模块:

- `agentflow-tools`
- `agentflow-agents/src/react/agent.rs`
- `agentflow-skills/src/builder.rs`
- `agentflow-tracing`

验收标准:

- 未授权 tool call 被拒绝，并返回结构化错误。
- trace replay 能展示 tool policy allowed/denied。
- Skill security allowlist 能实际影响 tool execution，不只是 metadata。

已完成:

- `agentflow-tools` 新增 `ToolPolicy` / `ToolPolicyDecision`，`ToolRegistry::execute` 在调用前统一评估并记录 audit log。
- policy decision 包含 allow/deny、matched rule、deny reason、source、permissions 和参数类型摘要。
- 未授权 tool call 返回结构化 `ToolError::PolicyDenied`。
- `agentflow-skills` 新增 `security.tool_permission_allowlist`，可把 Skill 安全配置映射为执行期 permission allowlist。
- ReAct / PlanExecute agent runtime 在每次 tool call 前写入 `tool_policy_decision` AgentEvent。
- `agentflow-tracing` 的 `ToolCallTrace`、replay 和 TUI 已展示 policy allow/deny 与规则信息。
- 已覆盖 registry allow/deny audit、Skill security allowlist、agent golden trace 和 tracing 展示测试。
- 已通过 `cargo test -p agentflow-tools -p agentflow-skills -p agentflow-agents -p agentflow-tracing --target-dir /tmp/agentflow-target`。

建议验证:

```bash
cargo test -p agentflow-tools --target-dir /tmp/agentflow-target
cargo test -p agentflow-agents --target-dir /tmp/agentflow-target
cargo test -p agentflow-tracing --target-dir /tmp/agentflow-target
```

### P0-5. 明确 plugin / skill / MCP / tool 边界

状态: 已完成

问题:

- 当前项目具备 Skills、MCP、Tools、Marketplace，但还没有正式 plugin runtime。
- 文档如果混用 plugin/skill/marketplace，会误导生态定位。

改进方案:

- 新增 `docs/EXTENSIBILITY_MODEL.md`。
- 明确:
  - Tool: runtime callable function abstraction
  - MCP: external tool transport/protocol
  - Skill: persona/tools/knowledge/memory/security 能力包
  - Marketplace: skill catalog/index aggregator
  - Plugin: future extension boundary, not currently implemented
- 更新 README / SKILLS / SKILL_REGISTRY 中相关措辞。

验收标准:

- 用户能根据文档判断应该写 Rust node、Tool、MCP server 还是 Skill。
- 不再把 marketplace 描述为完整 plugin marketplace。

已完成:

- 新增 `docs/EXTENSIBILITY_MODEL.md`，明确 Rust node、Tool、MCP、Skill、Skill registry/marketplace catalog、未来 Plugin 的边界。
- README 增加扩展模型入口，并说明 Skill registry / marketplace 是 local-first Skill catalog，不是通用 plugin runtime。
- `docs/SKILLS.md` 和 `docs/SKILL_REGISTRY.md` 已将 marketplace 相关措辞收敛为 Skill catalog，并明确 Skills 不是动态加载插件。

## 4. P1: Runtime、观测与恢复深化

### P1-1. 引入通用 DAG 并发 scheduler

状态: 进行中

目标:

- 从拓扑顺序串行执行升级到“依赖就绪即可调度”。
- 支持全局并发、节点级资源、失败策略和事件一致性。

改进方案:

- 在 `agentflow-core` 新增 scheduler 模块。
- 保留当前串行执行作为默认或兼容模式。
- 新增 `FlowExecutionConfig`:
  - mode: serial/concurrent
  - max_concurrency
  - fail_fast
  - continue_on_skip
  - run_dir/checkpoint config
- 节点完成后唤醒下游 ready nodes。
- 与 checkpoint/event listener 保持一致。

验收标准:

- 独立分支 DAG 并发执行耗时低于串行执行。
- failure/skip/checkpoint 行为有测试覆盖。
- 事件顺序可解释，trace 能显示并发节点。

已完成:

- `agentflow-core` 新增 `FlowExecutionConfig` / `FlowExecutionMode`，保留 `Flow::run()` 默认串行兼容路径。
- 新增 `execute_from_inputs_with_config`，显式 `Concurrent` 模式会按依赖 ready 状态并发调度节点。
- concurrent scheduler 支持 `max_concurrency`、`fail_fast`、`continue_on_skip`，并复用现有 node started/completed/failed/skipped/output events。
- concurrent scheduler 会在节点完成后唤醒下游 ready nodes，并保留 checkpoint 保存路径。
- CLI `workflow run` 新增 `--execution-mode serial|concurrent` 和 `--max-concurrency`，默认仍为 serial。
- 单元测试覆盖独立分支并发耗时低于串行、依赖节点等待上游输出。
- CLI 测试覆盖 concurrent 执行模式和非法 `--max-concurrency 0`。
- 已通过 `cargo test -p agentflow-core --target-dir /tmp/agentflow-target` 和 workspace `cargo check`。

剩余:

- 扩展 failure/skip/checkpoint 的并发端到端测试。
- 让 trace UI 更明确展示并发节点重叠关系。

### P1-2. Agent tool call 幂等与恢复策略

目标:

- 让 AgentNode partial resume 的边界更生产化。
- 对有副作用工具提供明确策略，而不是只依赖“不隐式重放”。

改进方案:

- 为 tool call 增加:
  - call_id
  - idempotency_key
  - side_effect_class: read_only/idempotent/mutating/external
  - replay_policy: never/reuse_result/retry_with_key/manual
  - compensation_hint
- AgentNode checkpoint 保存 unresolved tool calls 和策略。
- Resume 时根据策略决定继续、拒绝、复用或要求人工处理。

验收标准:

- read-only tool 可安全 replay。
- mutating tool 默认不能自动 replay。
- checkpoint 文档和测试覆盖 completed/partial/unresolved 三类。

### P1-3. 建立 metrics 与结构化日志 contract

目标:

- 在 trace 之外补齐指标和日志，服务生产排障和告警。

改进方案:

- 定义 metrics names:
  - `agentflow_workflow_duration_ms`
  - `agentflow_node_duration_ms`
  - `agentflow_tool_call_duration_ms`
  - `agentflow_mcp_call_duration_ms`
  - `agentflow_llm_request_duration_ms`
  - `agentflow_retry_total`
  - `agentflow_checkpoint_recovery_total`
- 定义 structured log fields:
  - run_id
  - trace_id
  - span_id
  - node_id
  - tool_name
  - skill_name
  - model
  - error_kind
- CLI/server/runtime 都使用一致字段。

验收标准:

- workflow、agent、tool、MCP 至少覆盖 duration/error/retry 指标。
- server 和 CLI 可开启 JSON log。
- 日志默认脱敏。

### P1-4. Trace storage 生产化

目标:

- 让 file/Postgres trace storage 有明确 schema version、migration、retention。

改进方案:

- 增加 trace storage schema version。
- Postgres storage 增加 migration SQL 或 migration runner。
- 增加 retention policy:
  - max age
  - max runs
  - failed run retention override
- CLI 增加 trace prune/list/query 子命令。

验收标准:

- trace storage 可升级。
- 可按 run_id/status/date 查询。
- 可清理历史 trace。

### P1-5. RAG/Memory/Agent 评测集

目标:

- 从“功能存在”推进到“质量可度量”。

改进方案:

- 建立小型固定语料 fixture。
- 定义 eval cases:
  - retrieval recall@k
  - hybrid search quality
  - rerank correctness
  - memory search relevance
  - agent answer uses retrieved context
- 输出 JSON eval report。

验收标准:

- `cargo test` 或独立 eval command 可在无外部 API 情况下跑 mock/local case。
- 外部 API case 独立 feature/env gate。

## 5. P2: 平台化与生态扩展

### P2-1. 扩展 server run management API

目标:

- `agentflow-server` 从 health gateway 变成最小可用 run service。

API 建议:

- `POST /api/runs/workflows`
- `GET /api/runs/:run_id`
- `POST /api/runs/:run_id/cancel`
- `GET /api/runs/:run_id/trace`
- `GET /api/skills`
- `POST /api/skills/install`

改进方案:

- DB 增加 run、run_events、trace_refs、skill_installations schema。
- server 复用 CLI/config parser，不复制解析逻辑。
- 长任务先用 in-process tokio task，后续再考虑 worker/queue。

验收标准:

- 可以通过 HTTP 提交 dry-run workflow。
- 可以查询 run status 和 trace。
- health/readiness 能检查 DB。

### P2-2. Skill marketplace remote 和供应链安全

目标:

- 从 local catalog 走向可控远程分发。

改进方案:

- remote index download 到 cache。
- skill bundle checksum。
- manifest、scripts、knowledge、server config 分别 checksum。
- optional signature metadata。
- install policy:
  - trusted index
  - allowed source
  - checksum required
  - script review required

验收标准:

- remote marketplace validate 不直接执行任何 skill 内容。
- install 前能展示安全摘要。
- checksum mismatch 阻止安装。

### P2-3. 正式 Plugin System 设计

目标:

- 在 Skills/MCP/Tools 稳定后，再定义真正 plugin。

设计范围:

- plugin manifest
- lifecycle: install/validate/enable/disable/remove
- extension points:
  - node
  - tool
  - skill template
  - trace exporter
  - memory backend
- runtime model:
  - static Rust crate
  - external process
  - WASM sandbox
- permission model
- version compatibility

验收标准:

- 先产出设计文档，不急于实现动态加载。
- 明确 plugin 不替代 Skills/MCP，而是更底层扩展点。

### P2-4. Multi-agent config-first 与 trace-first

目标:

- 让 Supervisor/multi-agent 不只存在于 SDK 示例。

改进方案:

- 增加 YAML agent graph:
  - agents
  - roles
  - handoff policy
  - shared memory
  - tool scopes
- trace 展示 multi-agent handoff。
- CLI 增加 dry-run/validate。

验收标准:

- 无外部 API mock 示例进入 CI smoke。
- trace replay 能展示 agent handoff。

## 6. 建议执行顺序

建议按下面顺序执行，避免先做平台化导致基础 contract 不稳:

1. P0-2 移除独立 `llm chat` 入口，并清理 CLI no-op 和 experimental 命令。
2. P0-1 统一 YAML schema 和参数校验。
3. P0-3 MCP JSON Schema validation。
4. P0-4 Tool Policy Decision 和审计。
5. P0-5 扩展模型文档澄清。
6. P1-2 Agent tool call 幂等与恢复。
7. P1-3 metrics 和结构化日志。
8. P1-4 Trace storage 生产化。
9. P1-1 DAG 并发 scheduler。
10. P1-5 RAG/Memory/Agent 评测集。
11. P2-1 server run management API。
12. P2-2 remote marketplace。
13. P2-3 plugin system 设计。
14. P2-4 multi-agent config-first。

## 7. 每阶段质量门禁

每个阶段完成前至少运行:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --target-dir /tmp/agentflow-target -- -D warnings
cargo test -p agentflow-core -p agentflow-tools -p agentflow-mcp -p agentflow-skills -p agentflow-agents -p agentflow-cli --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

涉及 examples 或 CLI 行为时额外运行:

```bash
cargo test --workspace --examples --target-dir /tmp/agentflow-target
cargo run -p agentflow-cli -- workflow run agentflow-cli/examples/workflows/fixed_dag_basic.yml --dry-run
cargo run -p agentflow-cli -- workflow run agentflow-cli/examples/workflows/skill_agent_hybrid.yml --dry-run
```

## 8. 计划成功标准

完成 P0 后:

- 用户不会再遇到主要 CLI silent no-op。
- 裸模型聊天不再作为 AgentFlow 的产品入口；交互体验统一收敛到 Skill/Agent/Workflow。
- workflow YAML 错误能稳定、提前、精确报告。
- MCP 参数错误能在 client 侧发现。
- tool call 有 policy decision 和基础审计记录。
- 文档清楚区分 Skill、MCP、Tool、Plugin。

完成 P1 后:

- runtime 支持更强恢复语义和并发调度。
- workflow/agent/tool/MCP/LLM 有统一指标和结构化日志。
- trace storage 具备 schema version、query、retention。
- RAG/Memory/Agent 质量可以被固定评测集衡量。

完成 P2 后:

- server 能管理最小 workflow run 和 trace 查询。
- marketplace 具备远程索引和供应链校验基础。
- plugin system 有清晰设计边界。
- multi-agent 进入 config-first 和 trace-first 路径。
