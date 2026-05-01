# AgentFlow 当前执行计划

最后更新: 2026-05-01

维护约定:

- `RoadMap.md` 是可跟踪的中长期路线图。
- `TODOs.md` 是本地短期执行队列。
- 不再维护 `TODO.md`。
- 本文件按 `RoadMap.md` 当前未完成项整理；完成后同步回写 `RoadMap.md`。

## 当前基线

已完成短期闭环:

- Skills + MCP 基础打通。
- Agent Runtime MVP。
- DAG + Agent hybrid。
- trace 串联、checkpoint resume。
- 可运行教程和 Skill registry/index 示例。
- CI 已覆盖 fmt、clippy `-D warnings`、核心 crates test matrix、workspace examples 编译和无外部 API smoke tests。

最新评估结论:

- 已生成 `OVERALL_EVALUATION_REPORT.md`，结论是 DAG 核心成熟度高于 CLI/config-first 与 agent-native 产品化入口。
- `cargo check --workspace --all-targets` 已通过，当前主要短板不是编译健康度，而是 CLI 参数兑现、模型配置体验、Skill/Agent 配置化和端到端用户路径。
- 下一轮重点从“基础 runtime 闭环”切换到“CLI/config-first 产品化闭环”。

最近提交:

- `cbd83f3 feat(cli): support trace dir env default`
- `20701b8 feat(core): configure workflow run artifacts dir`
- `dffcb69 feat(agent): add tool replay recovery metadata`
- `df3957a docs: define agent tool recovery contract`
- `9684916 feat(cli): show concurrent plan hint`
- `e1a4fff test(cli): cover concurrent workflow fixtures`
- `aa8c150 test(core): cover concurrent checkpoint semantics`
- `d832b0d test(core): cover concurrent skip semantics`
- `1fba2d4 test(core): harden concurrent failure semantics`
- `e761e30 feat(cli): expose concurrent workflow execution`
- `b67629f feat(core): add concurrent DAG scheduler`
- `d2b29eb docs: clarify extensibility model`
- `ad31f64 feat(tools): add policy decisions and audit`
- `e0af0f3 feat(mcp): validate tool arguments with json schema`
- `9133e2a test(cli): cover workflow schema validation`
- `e704243 feat(cli): support strict workflow validation`
- `d9b2663 feat(cli): add json workflow validation output`
- `8c9ce3f feat(cli): retire standalone llm chat`
- `37bfb9e docs: add runnable AgentFlow tutorials`
- `7c0b448 ci: add examples quality gate`
- `6cd5d56 ci: enforce clippy warnings`

当前进行中:

- P1-1 DAG concurrent scheduler hardening: 已完成
  - 已新增 `FlowExecutionConfig` / `FlowExecutionMode`。
  - `Flow::run()` 保持默认串行，`execute_from_inputs_with_config(..., Concurrent)` 显式启用 ready-node 并发调度。
  - CLI `workflow run` 已新增 `--execution-mode serial|concurrent` 和 `--max-concurrency`。
  - 已覆盖独立分支并发耗时低于串行、依赖节点等待上游输出、CLI concurrent mode、非法并发数测试。
  - 已补强并发 failure 语义: fail-fast 会停止调度新节点但保留 in-flight 结果；非 fail-fast 会继续独立 ready 分支，失败分支下游不被误调度。
  - 已补强并发 skip / conditional 语义: `run_if=false` 会记录 `NodeSkipped`，独立分支继续执行，依赖 skipped 输出的 required input 会失败为 `DependencyNotMet`。
  - 已补强并发 checkpoint 语义: 成功并发分支写入最终 checkpoint，失败 run 标记 `Failed` 并保留 last completed node，resume 仍走兼容串行路径。
  - 已补强 CLI/config-first 并发路径: 新增无外部 API 多分支 fixture，覆盖 concurrent run 和 dry-run 不执行节点。
  - 已补强 trace / debug 并发展示: 当前 `WorkflowEvent` 已具备 started timestamp + completed duration；`workflow debug --plan` 会提示 concurrent mode 下同层 ready nodes 可并发。
  - P1-1 当前子任务已完成；下一步可进入 P1-2 Agent tool call 幂等与恢复策略预备设计。

- P1-2 Agent tool call 幂等与恢复策略预备设计: 已完成
  - 已盘点 `AgentStepKind::ToolCall`、`ToolCallTrace`、checkpoint `agent_resume` 的既有字段。
  - 已在 `docs/CHECKPOINT_RECOVERY.md` 定义 `call_id`、`idempotency_key`、`side_effect_class`、`replay_policy` 的最小恢复契约。
  - 已将恢复元数据落到 `AgentNodeResumeContract` 和 trace `ToolCallTrace`，保持旧 trace 字段兼容。
  - 默认策略已明确并测试: recorded result 复用，read-only 可 replay，mutating/external 走 manual required。

- P1-3 Runtime storage config hardening: 已完成
  - `FlowExecutionConfig` 已支持显式 `run_base_dir`，默认仍兼容 `~/.agentflow/runs`。
  - `agentflow workflow run` 已新增 `--run-dir`，并支持 `AGENTFLOW_RUN_DIR` 作为环境默认值。
  - 已覆盖 core 配置注入、CLI flag 和 env var 三条路径。

- P1-4 Trace storage config hardening: 已完成
  - `agentflow trace replay/tui` 已支持 `AGENTFLOW_TRACE_DIR` 作为 `--dir` 缺省值。
  - trace CLI help 和 tracing 文档已说明 env/default 查找顺序。
  - 已增加 CLI 测试覆盖 env trace dir。

当前任务清单:

- P1-1.1 并发 failure 语义测试:
  - [x] 在 `agentflow-core/src/flow.rs` tests 中新增并发 DAG: `root -> ok_branch` 与独立 `fail_branch`。
  - [x] 覆盖默认 `fail_fast=true`: 失败后 workflow 标记 failed，已完成节点保留输出，未就绪下游不执行。
  - [x] 覆盖 `fail_fast=false`: 独立 ready 分支继续执行，依赖失败节点的下游不被误调度。
  - [x] 断言 `WorkflowEvent::NodeFailed` 与 `WorkflowEvent::WorkflowFailed` 被触发。

- P1-1.2 并发 skip / conditional 语义测试:
  - [x] 构造 `run_if=false` 的分支，确认并发 scheduler 写入 `NodeSkipped`。
  - [x] 覆盖 `continue_on_skip=true`: 不依赖 skipped 节点的独立分支继续执行。
  - [x] 覆盖依赖 skipped 节点的 required input 行为，确认不会把缺失输出当成成功输入。
  - [x] 断言 skip 事件和 final state 中 `NodeSkipped` 一致。

- P1-1.3 并发 checkpoint 端到端测试:
  - [x] 使用临时 checkpoint 目录开启 `with_checkpointing`。
  - [x] 并发执行两个独立成功分支，确认 checkpoint state 包含两个成功节点输出。
  - [x] 构造一个成功分支 + 一个失败分支，确认 final checkpoint status 为 `Failed`，且 last completed node 非空。
  - [x] 验证 resume 仍走兼容串行恢复路径，不隐式启用 concurrent resume。

- P1-1.4 CLI/config-first 并发测试补强:
  - [x] 增加一个无外部 API 的多分支 workflow fixture，覆盖 `workflow run --execution-mode concurrent --max-concurrency 2`。
  - [x] 对 `--dry-run` 行为保持不执行节点，仅展示拓扑顺序，不受 execution mode 影响。
  - [x] 文档中注明 concurrent mode 当前适用于普通执行，checkpoint resume 仍按兼容路径恢复。

- P1-1.5 trace / debug 并发展示:
  - [x] 评估当前 `WorkflowEvent` 是否足够表达并发重叠: `NodeStarted.timestamp` / `NodeCompleted.duration`。
  - [x] 在 `trace replay` 或 `workflow debug --plan` 中增加简洁提示: concurrent mode 下同层 ready nodes 可并发。
  - [x] 若需要更强展示，再扩展 trace TUI 的 node row 输出 started/duration。

- P1-2 预备设计: Agent tool call 幂等与恢复策略:
  - [x] 盘点 `AgentStepKind::ToolCall`、`ToolCallTrace`、checkpoint `agent_resume` 中已有字段。
  - [x] 定义 `call_id`、`idempotency_key`、`side_effect_class`、`replay_policy` 的最小数据结构。
  - [x] 明确默认策略: read-only 可 replay，mutating/external 默认 manual 或 reuse recorded result。
  - [x] 先写设计小节到 `docs/CHECKPOINT_RECOVERY.md` 或新增 runtime recovery 文档，再改代码。
  - [x] 将最小数据结构落到 `AgentNodeResumeContract` 和 trace `ToolCallTrace`，保持旧 trace 兼容。
  - [x] 增加测试覆盖 recorded result、read-only replay、mutating/external manual 默认策略。

- P1-3 Runtime storage config hardening:
  - [x] 将 workflow run artifact base directory 从硬编码 home path 推进到 `FlowExecutionConfig`。
  - [x] 为 CLI `workflow run` 增加 `--run-dir`，并支持 `AGENTFLOW_RUN_DIR`。
  - [x] 增加 core 与 CLI 测试，确认 run artifacts 写入显式目录。
  - [x] 更新配置文档和 README，说明默认路径与覆盖方式。

- P1-4 Trace storage config hardening:
  - [x] 为 `agentflow trace replay/tui` 增加 `AGENTFLOW_TRACE_DIR` 缺省路径支持。
  - [x] 保持显式 `--dir` 优先于环境变量，环境变量优先于 `~/.agentflow/traces`。
  - [x] 增加 CLI 测试覆盖 env trace dir。
  - [x] 更新 trace CLI help 和文档。

近期已完成但需长期维护:

- P0-1 workflow YAML schema validation: 主体完成；后续随新增节点维护 schema 表和测试样例。
- P0-3 MCP tool JSON Schema validation: 主体完成；后续随 MCP schema 兼容性需求补测试。
- P0-4 Tool Policy Decision / audit: 主体完成；后续补更细粒度 runtime context 和 redaction hint。
- P0-5 extensibility model: 主体完成；后续若引入 remote catalog 或 plugin runtime，需要同步边界文档。

## P0: N6 CLI / config-first 产品化闭环

### 1. 补齐 `agentflow workflow run` 运行参数

状态: 已完成

目标:

- 让 CLI flags 与实际行为一致，避免用户认为已支持但运行时被忽略。
- 将 DAG workflow 的 config-first 使用体验提升到可作为主入口使用。

原问题（已修复）:

- `watch`、`output`、`input`、`dry_run`、`timeout`、`max_retries` 曾在 `workflow run` 执行路径中是占位参数。
- 输出曾包含 debug 打印，缺少稳定 JSON / YAML / text 输出 contract。

子任务:

- [x] 实现 `--input KEY VALUE` 注入到 `Flow::execute_from_inputs`，支持 string/number/bool/json 基础解析。
- [x] 实现 `--dry-run`，只解析 YAML、构建 graph、输出 execution order，不执行节点。
- [x] 实现 `--timeout`，对整次 workflow run 设置外层超时，并在错误中报告 workflow file、node context。
- [x] 实现 `--max-retries`，先对 workflow run 做外层 retry；后续再细化到 node retry policy。
- [x] 实现 `--output <path>`，保存最终 state pool，默认格式 JSON。
- [x] 定义 stdout 输出策略: 默认人类可读，`--output -` 输出机器可读 JSON。
- [x] 明确 `--watch` 的 MVP 行为；当前显式报错，不再静默忽略。
- [x] 增加 CLI tests 覆盖 input、dry-run、output、watch 未实现失败路径。

完成记录:

- `agentflow workflow run` 现在会解析 CLI inputs 并注入 V2 `Flow::execute_from_inputs`。
- `--dry-run` 会构建 graph 并输出 `Flow::execution_order()`，不会执行节点。
- `--timeout` 支持整数秒默认、`ms`、`s`、`m` 后缀，并包裹整次 workflow attempt。
- `--max-retries` 支持 workflow 级重试，失败时报告 attempt context。
- `--output <path>` 保存脱敏后的 final state JSON，`--output -` 打印 JSON。
- `--watch` 当前明确返回未实现错误，避免 silent no-op。
- 移除 workflow run 的临时 debug final state 打印。

验证:

```bash
cargo fmt --all -- --check
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

验收标准:

- `agentflow workflow run examples.yml --input topic rust --dry-run` 不执行节点但展示执行计划。
- `agentflow workflow run examples.yml --output /tmp/result.json` 能生成可解析 JSON。
- 未实现的 flag 不再静默忽略。

### 2. 强化模型配置和模型切换 CLI

状态: 已完成

目标:

- 让用户能通过 CLI 初始化、查看、验证、选择和临时覆盖模型。
- 支持 LLM、workflow node、Skill agent 三条路径共享同一套模型配置体验。

建议命令:

```bash
agentflow config init
agentflow config show models
agentflow config validate
agentflow llm models --provider openai --detailed
agentflow workflow run flow.yml --model gpt-4o-mini
agentflow skill run ./skills/code-reviewer --message "review this" --model gpt-4o-mini
agentflow skill chat ./skills/code-reviewer --model gpt-4o-mini
```

子任务:

- [x] 盘点 `agentflow-llm` 当前 `~/.agentflow/models.yml`、`.env`、builtin defaults 加载路径。
- [x] 增强 `agentflow config show models`，展示 provider、model、api_key_env，默认脱敏。
- [x] 增强 `agentflow config validate`，校验模型配置结构和必要 env var 是否存在。
- [x] 为 `workflow run`、`skill run/chat` 统一增加或规范 `--model` 覆盖语义；`llm chat` 已退休，`llm` 命名空间只保留模型发现/诊断。
- [x] 为 YAML workflow 支持 node-level `model`，并支持 CLI `--model` 覆盖 LLM 节点模型。
- [x] 增加模型别名机制设计，例如 `default_chat_model`、`default_vision_model`、`default_embedding_model`。
- [x] 增加文档: `docs/CONFIGURATION.md` 中补充模型切换和多 provider 示例。

完成记录:

- `agentflow llm models` 现在优先读取用户 `~/.agentflow/models.yml`，不会为了列模型而初始化 provider。
- `agentflow llm chat` 已从产品入口退休；裸模型聊天不再作为 AgentFlow 交互路径。
- `agentflow llm models` 保留为模型发现能力。
- `agentflow workflow run --model <model>` 会覆盖 workflow 中 `llm` 节点的模型输入。
- `agentflow skill run --model <model>` 和 `agentflow skill chat --model <model>` 会覆盖 Skill manifest 中声明的模型。
- `skill run` 保留既有 `-m/--message`，模型覆盖只使用长参数 `--model`，避免 clap 短参数冲突。
- `docs/CONFIGURATION.md` 已补充当前 CLI 模型配置、模型覆盖优先级和模型 alias 设计。

验证:

```bash
cargo fmt --all -- --check
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

验收标准:

- 用户可以不改 YAML，通过 `--model` 临时切换一次运行的模型。
- 配置校验不会打印 API key，但能明确报告缺失的 env var 名称。
- Skill、workflow LLM node 对模型选择的优先级一致；交互式使用统一走 Skill/Agent/Workflow。

### 3. 提升 Skill CLI 的实际使用体验

状态: 已完成核心闭环

目标:

- 把 Skill 从“可验证/可安装”推进到“可发现、可运行、可调试、可复用”。
- 让 Skill 成为 agent-native 应用的 config-first 主入口。

子任务:

- [x] 完善 `agentflow skill list`，默认扫描 `~/.agentflow/skills`，显示 name、version、description、installed path。
- [x] 增强 `agentflow skill run/chat` 参数: `--model`、`--trace`、`--session-id`。
- [x] 增强 `agentflow skill run/chat` 参数: `--memory sqlite|session|none`。
- [x] 增强 `agentflow skill list-tools`，展示工具来源 builtin/script/mcp/workflow、权限、参数 schema 摘要。
- [x] 增强 `agentflow skill test`，支持无真实 LLM 的 dry-run: manifest 校验、MCP tool discovery。
- [x] 增加 `agentflow skill inspect <path|name>`，输出 persona/model/tools/memory/knowledge/security 汇总。
- [x] 将 marketplace resolve 输出和 install 命令串联，减少用户手动复制路径。
- [x] 增加可运行教程: 安装 skill、列工具、切模型运行、查看 trace。

完成记录:

- 新增 `agentflow skill inspect <skill_dir>`，汇总 identity、persona、model、memory、tools、MCP、knowledge、security 和 validation status。
- `agentflow skill test --dry-run` 现在只做 manifest validation 和 tool discovery，不执行 script regressions 或 smoke scripts。
- `skill run/chat` 支持 `--model`，并为 `--session` 提供 `--session-id` alias。
- `skill run` 打印实际使用模型，便于确认运行时覆盖是否生效。
- `skill run/chat` 支持 `--memory session|sqlite|none`，运行前覆盖 Skill manifest memory backend。
- 新增 `agentflow skill marketplace install <marketplace> <skill>`，直接 resolve marketplace 并复用本地 install 流程。
- 新增 `docs/examples/cli_config_first_tutorial.md`，覆盖 mock 模型配置、固定 DAG、Skill inspect/list-tools/test、Skill run、skill-agent workflow、marketplace install 和 trace 查看。

验证:

```bash
cargo fmt --all -- --check
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

验收标准:

- 用户能完成 `install -> list -> inspect -> list-tools -> run --model ... --trace` 的完整链路。
- 无 API key 环境也能运行 `skill test --dry-run`。

### 4. 在 workflow YAML 中暴露 agent / skill-agent 节点

状态: 已完成

目标:

- 补齐 Config-first 的 DAG + Agent hybrid 能力。
- 让用户不写 Rust 也能在 DAG 中嵌入智能体节点。

建议 YAML 形态:

```yaml
nodes:
  - id: review
    type: skill_agent
    parameters:
      skill: ./skills/code-reviewer
      model: gpt-4o-mini
      message: "Review {{ nodes.prepare.outputs.output }}"
```

子任务:

- [x] 在 `agentflow-cli/src/executor/factory.rs` 增加 `agent` / `skill_agent` node type。
- [x] 复用 `SkillLoader` + `SkillBuilder` 构建 ReActAgent。
- [x] 支持 `message` 来自 parameters 或 input_mapping。
- [x] 支持 node-level `model` 覆盖 Skill manifest model，并支持 `workflow run --model` 覆盖。
- [x] 输出 `response`、`session_id`、`stop_reason`、`agent_result`、`agent_resume`。
- [x] 增加 YAML CLI workflow 测试。

完成记录:

- 新增 CLI workflow 专用 `skill_agent` / `agent` 节点，执行时加载 Skill、构建 agent 并运行 ReAct loop。
- `skill_agent` 支持 `skill`、`message`、`model` 输入，能从 `input_mapping` 接收上游节点输出。
- 运行输出与 `AgentNode` 对齐，包含 `agent_result` 和 `agent_resume`，便于后续 checkpoint/recovery 继续增强。
- `workflow run --model` 同时覆盖 `llm`、`skill_agent` 和 `agent` 节点。

验证:

```bash
cargo fmt --all -- --check
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

验收标准:

- 一个 YAML workflow 可以先用 template/file 节点准备输入，再调用 skill-agent 节点，最后将 agent response 传给后续节点。
- checkpoint 后已完成的 agent 节点不会重复执行工具调用。

### 5. 清理旧 CLI runner 和文档错位

状态: 已完成

目标:

- 降低维护者误判执行路径的概率。
- 让 README/docs/CLI help 与当前实现一致。

子任务:

- [x] 删除或隔离不再使用的旧 runner 代码，或明确标记为 legacy。
- [x] 检查 README、docs、examples 中的 workflow YAML 是否符合 V2 parser。
- [x] 更新 `agentflow workflow debug` 文档，明确 validate/plan/analyze/visualize 能力边界。
- [x] 为尚未实现能力加 TODO 注释或 CLI 明确报错，避免 silent no-op。

完成记录:

- 删除未编译引用的 legacy `agentflow-cli/src/executor/runner.rs`，`executor/mod.rs` 只保留当前 V2 factory。
- `agentflow-cli/README.md` 已更新 workflow run/debug、llm chat/models、config show/validate 和 skill inspect/test/run 使用说明。
- README 明确当前 `workflow run` 走 `FlowDefinitionV2 -> GraphNode -> agentflow_core::Flow` 路径。
- `--watch` 已在 P0-1 中改为显式未实现错误，不再 silent no-op。

验证:

```bash
cargo fmt --all -- --check
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

验收标准:

- 新贡献者能从 CLI command 直接追到当前 V2 Flow 执行路径。
- 文档中 CLI 示例可在无外部 API 的 smoke test 中至少完成 dry-run/validate。

## P1: N7 统一可观测、恢复和端到端样例

### 6. 统一 workflow / agent / tool / MCP trace 关联

状态: 已完成

目标:

- 一次 mixed run 能用同一个 run id / trace id 串起 workflow node、agent step、tool call、MCP call、LLM call。

子任务:

- [x] 盘点当前 `WorkflowEvent`、`AgentEvent`、Tool/MCP tracing 字段。
- [x] 定义统一 `run_id`、`span_id`、`parent_span_id` 或等价关联模型。
- [x] 让 `AgentNode` 执行时继承 workflow run context。
- [x] 让 `SkillBuilder`/ToolRegistry 调用可以记录 tool source、permission、duration、error。
- [x] 增加 hybrid trace replay fixture。

完成记录:

- 在 `agentflow-tracing` 持久化模型中新增 `TraceContext`，包含 `run_id`、`trace_id`、`span_id`、`parent_span_id`。
- `TraceCollector` 现在为 workflow、node、agent、tool/MCP call 写入层级 context: workflow -> node -> agent -> tool。
- 新增 tracing 单元测试覆盖 agent/tool context 链接。
- `docs/TRACING_USAGE.md` 已记录 context 字段和层级规则。
- `ToolRegistry` 暴露 tool metadata 查询，ReAct 和 Plan-and-Execute runtime 的 tool events 现在记录 source、permissions、duration、error。
- trace collector 会把 tool source、permissions、duration、error 汇总到 `ToolCallTrace`，replay/TUI 会展示 source 和权限。
- 新增 `agentflow-tracing/tests/fixtures/hybrid_trace_replay.json` 和 replay fixture 测试，覆盖 DAG -> Agent -> Tool/MCP 的展示链路。

验收标准:

- `agentflow trace replay <run_id>` 能展示 DAG -> Agent -> Tool/MCP 的层级关系。

### 7. 强化 checkpoint / resume 的 FlowValue 和 AgentNode 语义

状态: 已完成

目标:

- 避免复杂输出、文件引用、agent partial output 在恢复时信息丢失。

子任务:

- [x] 为 `FlowValue::File`、`FlowValue::Url` 设计稳定 checkpoint JSON 格式。
- [x] 修复 checkpoint state 转换中非 JSON 输出可能退化为 `null` 的风险。
- [x] 增加 checkpoint roundtrip tests 覆盖 Json/File/Url。
- [x] 增加 AgentNode partial output checkpoint/recovery 测试。
- [x] 文档化 unresolved tool call 的恢复边界和幂等要求。

完成记录:

- `Flow::state_pool_to_checkpoint_state` 现在复用稳定的 `FlowValue` JSON 表示，不再把 `File` / `Url` 输出退化为 `null`。
- 新增 checkpoint roundtrip 单元测试，覆盖 `FlowValue::Json`、`FlowValue::File`、`FlowValue::Url` 的原始 checkpoint JSON 和恢复后类型和值。
- 现有 checkpoint recovery 测试已覆盖 AgentNode 完成态恢复不重复执行工具、partial trace checkpoint/recovery、重复 partial resume 保持 last completed node。
- `docs/CHECKPOINT_RECOVERY.md` 已补充 FlowValue checkpoint JSON 形态，并已记录 unresolved tool call 需要显式幂等重试策略。

验证:

```bash
cargo fmt --all -- --check
cargo test -p agentflow-core --target-dir /tmp/agentflow-target
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
```

验收标准:

- checkpoint roundtrip 后 FlowValue 类型和值保持一致。
- AgentNode 完成态恢复不会重复调用工具；partial 态恢复行为有明确错误或继续策略。

### 8. 建立权威端到端示例集

状态: 已完成

目标:

- 用少量高质量示例覆盖 DAG、agent-native、hybrid、Skill + MCP、RAG + Memory。

子任务:

- [x] 新增 `examples/workflows/fixed_dag_basic.yml`，无外部 API。
- [x] 新增 `examples/workflows/skill_agent_hybrid.yml`，可 dry-run，可用 mock skill。
- [x] 新增 `examples/skills/model-switching` 或教程，展示 `--model` 覆盖。
- [x] 新增 RAG + Skill 示例，区分需要 Qdrant/API key 的步骤。
- [x] 将关键示例纳入 CI smoke test: validate/dry-run，不调用外部 API。

完成记录:

- 新增 `agentflow-cli/examples/workflows/rag_skill_assistant.yml`，覆盖 RAG search -> template -> skill_agent 的 config-first hybrid 路径。
- 示例注释和文档明确 dry-run 不需要外部服务，完整运行需要 Qdrant、embedding credentials 和 chat model。
- `.github/workflows/quality.yml` 的 examples smoke 新增固定 DAG dry-run、Skill-agent dry-run、RAG + Skill dry-run。
- CI feature matrix 新增 `agentflow-cli --features rag` check。
- `docs/examples/cli_config_first_tutorial.md`、`docs/examples/README.md`、`agentflow-cli/examples/workflows/RAG_EXAMPLES.md`、`docs/RELEASE_CHECKLIST.md` 已同步。

验收标准:

- 新用户按教程可以完成模型配置、Skill 安装、Skill 运行、workflow dry-run、trace 查看。

## P0: N5 质量和发布门禁

### 1. 扩展 test matrix: feature 组合和 doc tests

状态: 已完成

目标:

- CI 不只跑核心包单元测试，也覆盖文档示例和关键 feature 组合。
- release checklist 中的人工检查与 CI job 对齐。

子任务:

- [x] 盘点 workspace 中实际存在的 feature flags。
- [x] 增加 doc tests CI job，例如 `cargo test --workspace --doc`。
- [x] 增加至少一组 feature 组合检查，避免拉满所有 feature 导致外部服务依赖。
- [x] 更新 `docs/RELEASE_CHECKLIST.md` 的对应命令。
- [x] 更新 `RoadMap.md` / 本文件状态。

完成记录:

- 新增 `.github/workflows/quality.yml` doctest job: `cargo test --workspace --doc`。
- 新增 feature matrix job，覆盖 `agentflow-core/observability`、`agentflow-mcp/client,server,stdio`、`agentflow-cli/mcp`。
- `docs/RELEASE_CHECKLIST.md` 已同步 feature inventory、doc test 和 CI-covered feature commands。

建议验证:

```bash
cargo test --workspace --doc --target-dir /tmp/agentflow-target
cargo check --workspace --target-dir /tmp/agentflow-target
```

验收标准:

- CI 有明确 doc test job。
- feature 组合命令能在无外部 API key 环境运行。
- `git diff --check` 通过。

### 2. 固化 release checklist 为 CI job / 发布模板

状态: 已完成

目标:

- 把 `docs/RELEASE_CHECKLIST.md` 中已经自动化的部分放进 CI。
- 保留需要人工判断的发布项，避免假装全自动。

子任务:

- [x] 新增或扩展 GitHub Actions release gate job。
- [x] 自动执行 fmt、clippy、核心 tests、examples compile/smoke、doc tests。
- [x] 在 release checklist 中标注哪些已由 CI 覆盖，哪些仍需人工确认。
- [x] 考虑是否增加 PR checklist / release issue template。

完成记录:

- `.github/workflows/quality.yml` 新增 `workflow_dispatch`、tag 触发和聚合 `release gate` job。
- `release gate` 依赖 fmt、clippy、核心 test matrix、doctest、feature matrix、examples compile/smoke。
- 新增 `.github/ISSUE_TEMPLATE/release.md` 记录仍需人工判断的发布项。
- `docs/RELEASE_CHECKLIST.md` 已标注 CI covered 与 manual sections。

建议验证:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --target-dir /tmp/agentflow-target -- -D warnings
cargo test --workspace --examples --target-dir /tmp/agentflow-target
```

验收标准:

- CI 配置和 release checklist 不再重复或互相矛盾。
- 人工 release checklist 更短、更明确。

## P1: Phase 5 / M4 标准化 Skills 生态

### 3. `agentflow skill install` 最小本地 registry 安装路径

状态: 已完成

目标:

- 从 `skills.index.toml` resolve 一个 skill。
- 将本地 skill 安装到用户指定目录或默认 skill home。
- 为后续 Git/repo/marketplace 安装预留接口，但先不做远程下载。

建议最小行为:

```bash
agentflow skill install agentflow-skills/examples/skills.index.toml mcp-demo --dir /tmp/agentflow-skills
agentflow skill validate /tmp/agentflow-skills/mcp-basic
```

子任务:

- [x] 设计 CLI 参数: index file、skill name/alias、target dir、overwrite 策略。
- [x] 复用 `SkillRegistryIndex::resolve_skill`。
- [x] 复制 skill 目录，保留相对文件结构。
- [x] 防止覆盖已有目录，除非显式 `--force`。
- [x] 增加 CLI 测试，使用 `agentflow-skills/examples/skills.index.toml` 或临时 fixture。
- [x] 更新 `docs/SKILLS.md` 和可运行教程。

完成记录:

- 新增 `agentflow skill install <index_file> <skill> [--dir <target>] [--force]`。
- 默认安装目录为 `~/.agentflow/skills`，显式 `--dir` 时安装到目标目录下的 canonical skill name 子目录。
- 安装前 resolve 并校验本地 skill，安装时递归复制目录，保留相对结构。
- 目标目录存在时默认拒绝覆盖；`--force` 会先移除旧安装目录再复制。
- CLI 测试覆盖安装、validate、重复安装拒绝和 `--force`。

验收标准:

- 能从本地 registry index 安装 mcp-basic skill。
- 安装后 `skill validate` / `skill list-tools` 可运行。
- 错误信息包含 index、skill name、目标目录。

### 4. registry/index 分发体验设计

状态: 已完成

目标:

- 明确组织内共享 skill index 的版本锁定、校验、安装、升级体验。

子任务:

- [x] 记录 index schema 字段和兼容策略。
- [x] 明确 `manifest_sha256` 的使用场景。
- [x] 设计后续远程 registry / Git repo install 的边界。
- [x] 将设计沉淀到 `docs/SKILLS.md` 或独立 `docs/SKILL_REGISTRY.md`。

完成记录:

- 新增 `docs/SKILL_REGISTRY.md`，覆盖 schema v1 字段、兼容策略、本地 validate/resolve/install workflow。
- 明确 `manifest_sha256` 只锁 manifest 文件，不替代脚本/MCP/knowledge 文件审查。
- 记录显式升级流程和未来 Git/remote registry 边界: 先下载到 cache，再复用本地 resolve/validate/copy 模型。
- `docs/SKILLS.md` 已链接 registry 设计文档。

验收标准:

- 文档能指导用户创建、验证、resolve、install 一个本地 index。
- 后续实现远程安装时不需要推翻本地模型。

## P1: N5 性能基准

### 5. 大 DAG 调度 benchmark

状态: 已完成

目标:

- 衡量不同规模 DAG 的构建和调度开销。
- 对 RoadMap 中生产化性能目标提供基线。

子任务:

- [x] 确认 benchmark 工具选择，优先复用现有 test/benchmark 结构，必要时引入 criterion。
- [x] 覆盖 100 / 1,000 / 10,000 节点 synthetic DAG。
- [x] 记录本地基线和 CI 是否运行的策略。

完成记录:

- 复用现有 benchmark-style test 结构，不引入 criterion。
- 新增 `agentflow-core/tests/large_dag_benchmarks.rs`，测 synthetic DAG 构建和 `Flow::execution_order()` 调度规划。
- 覆盖 100 / 1,000 / 10,000 节点，无外部 API 依赖。
- 本地命令: `cargo test -p agentflow-core --test large_dag_benchmarks --target-dir /tmp/agentflow-target -- --nocapture`。
- 当前策略: 本地和按需 CI 可运行，不加入默认 quality gate，避免每个 PR 增加性能噪声。

验收标准:

- 有可重复运行的 benchmark 命令。
- benchmark 不依赖外部 API。

### 6. ToolRegistry 调用 benchmark

状态: 已完成

目标:

- 衡量 tool lookup、schema metadata、执行 wrapper 的基础开销。

子任务:

- [x] 构造内置 mock tool。
- [x] 覆盖单工具、多工具、大 registry lookup。
- [x] 覆盖成功和错误输出路径。

完成记录:

- 新增 `agentflow-tools/tests/tool_registry_benchmarks.rs`。
- 覆盖 1 / 100 / 10,000 工具 registry lookup。
- 覆盖 OpenAI schema metadata 生成、成功 execute wrapper、错误 execute wrapper。
- 本地命令: `cargo test -p agentflow-tools --test tool_registry_benchmarks --target-dir /tmp/agentflow-target -- --nocapture`。

验收标准:

- 输出能区分 registry lookup 和 tool execute 的开销。

### 7. MCP tool latency benchmark

状态: 已完成

目标:

- 衡量本地 stdio MCP server 的 connect、tools/list、tools/call latency。

子任务:

- [x] 复用 `agentflow-skills/examples/skills/mcp-basic`。
- [x] 区分首次连接、复用连接、shutdown/reconnect。
- [x] 记录超时配置对失败路径的影响。

完成记录:

- 新增 `agentflow-mcp/tests/mcp_latency_benchmarks.rs`，直接使用本地 stdio MCP server。
- 覆盖 first connect、first tools/list、复用连接 tools/list、复用连接 tools/call、shutdown/reconnect/list。
- 输出 p50/p95/avg；client timeout 固定 5s、max_retries=0，用于后续失败路径对比。
- 本地命令: `cargo test -p agentflow-mcp --test mcp_latency_benchmarks --target-dir /tmp/agentflow-target -- --nocapture`。

验收标准:

- benchmark 无外部网络依赖。
- 输出包含 p50/p95 或等价统计。

### 8. agent loop prompt assembly benchmark

状态: 已完成

目标:

- 衡量 ReAct prompt 组装、memory budget、summary backend 对延迟的影响。

子任务:

- [x] 构造 mock memory 和 mock tools。
- [x] 覆盖短上下文、长上下文、触发 summary 的上下文。
- [x] 记录与 token/message 数量的关系。

完成记录:

- 新增 `ReActAgent::preview_llm_messages()`，用于不调用模型的 prompt preview/benchmark。
- 新增 `agentflow-agents/tests/prompt_assembly_benchmarks.rs`。
- 覆盖 20 条短上下文、1,000 条长上下文、1,000 条且触发 compact summary 的上下文。
- 使用 `SessionMemory` 和 mock `ToolRegistry`，不调用真实 LLM。
- 本地命令: `cargo test -p agentflow-agents --test prompt_assembly_benchmarks --target-dir /tmp/agentflow-target -- --nocapture`。

验收标准:

- benchmark 不调用真实 LLM。
- 能帮助判断 memory budget 策略是否退化。

## P2: Phase 6 / M5 生产化和生态工具

### 9. Web UI 或 TUI 调试器

状态: 已完成

目标:

- 提供 workflow / agent / tool / MCP trace 的交互式查看入口。

建议先做 TUI 或 CLI 增强，避免引入 Web 前端复杂度。

子任务:

- [x] 评估现有 `agentflow trace replay` 输出。
- [x] 设计最小 trace timeline 交互。
- [x] 决定 TUI crate 或纯 CLI 分页输出。

完成记录:

- 新增 `agentflow trace tui <run_id>`，提供静态终端 timeline 入口。
- 支持 `--filter all|workflow|agent|tool|mcp` 和 `--details` 聚焦查看 workflow、agent、tool、MCP trace。
- 暂不引入 ratatui/crossterm；先用纯 CLI 输出降低依赖和维护成本。

### 10. 配置加密和 secret 管理

状态: 已完成

目标:

- 明确 API key、env secret、tool sensitive params 的存储和展示策略。

子任务:

- [x] 盘点当前配置加载路径和 `.env` 使用。
- [x] 设计本地加密存储或外部 secret manager 集成边界。
- [x] 确保 trace / CLI 输出默认脱敏。

完成记录:

- 新增 `docs/SECRET_MANAGEMENT.md`，明确 `~/.agentflow/models.yml` 只保存 env var 名称，密钥值来自环境变量或 `~/.agentflow/.env`。
- 明确后续加密/外部 secret manager 应走 `env:`、`file:`、`keychain:`、`vault:` 等 resolver 边界。
- 实现 `agentflow config show` 的默认脱敏输出，并保留 `api_key_env` 这类环境变量名可见。
- 实现 `agentflow config validate`，只报告缺失 env var 名称，不打印密钥值。

### 11. Docker / Helm 部署

状态: 已完成

目标:

- 提供可部署的服务镜像和 Kubernetes 安装入口。

子任务:

- [x] 明确需要容器化的二进制: CLI、server、gateway 或 worker。
- [x] 编写 Dockerfile。
- [x] 编写 docker-compose 或 Helm chart 初版。
- [x] 文档化 health checks、env、volume、secret。

完成记录:

- 主部署目标确定为 `agentflow-server` 长运行网关；`agentflow` CLI 可通过 Dockerfile build args 构建。
- 新增多阶段 `Dockerfile` 和 `.dockerignore`。
- 新增 `docker-compose.yml`，包含 PostgreSQL 和 `agentflow-server`。
- 新增 `charts/agentflow` Helm chart 初版，支持 probes、env、existingSecret / chart-managed secret。
- 新增 `docs/DEPLOYMENT.md`，记录 health checks、env、volume、secret 策略。

### 12. Plugin / Skill marketplace 雏形

状态: 已完成

目标:

- 在 Skill registry/index 基础上形成可浏览、可安装的 marketplace 雏形。

子任务:

- [x] 定义 marketplace metadata。
- [x] 明确本地 index、组织 index、远程 marketplace 的关系。
- [x] 与 `agentflow skill install` 对齐。

完成记录:

- 新增 `SkillMarketplace` schema，支持 `local`、`organization` 和预留 `remote` index kind。
- 新增 `agentflow skill marketplace validate|list|resolve`，聚合本地/组织 registry index 并输出现有 `skill install` 命令。
- 新增 `agentflow-skills/examples/marketplace.toml` 作为本地 marketplace 示例。
- 更新 `docs/SKILL_REGISTRY.md` 和 `docs/SKILLS.md`，明确 marketplace 只是 catalog，安装仍走 registry index。

## 已完成执行顺序

1. P0-1: 已补齐 `agentflow workflow run` 的 input、dry-run、output、timeout、retry 行为。
2. P0-2: 已强化模型配置和模型切换 CLI，统一 `llm chat`、`workflow run`、`skill run/chat` 的 `--model` 语义。
3. P0-3: 已提升 Skill CLI 使用链路，完成 `install -> list -> inspect -> list-tools -> run --model --trace`。
4. P0-4: 已在 workflow YAML 中暴露 `agent` / `skill_agent` 节点，补齐 config-first hybrid。
5. P0-5: 已清理旧 CLI runner 和文档错位，避免 silent no-op。
6. P1-6 到 P1-8: 已统一 trace/recovery，并建立权威端到端示例集。
