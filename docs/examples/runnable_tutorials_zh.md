# AgentFlow 可运行教程

本教程覆盖当前五条核心路径:

- 固定 DAG: 确定性工作流，不使用 agent 或 LLM。
- agent-native: 直接运行 ReAct agent loop。
- hybrid: 父 DAG 中嵌入 `AgentNode`。
- Skill + MCP: Skill 暴露本地 MCP server 工具。
- WorkflowTool: agent 把子 DAG 当作普通工具调用。

下面的必跑命令都使用 mock model 或本地 fixture，不需要外部 LLM API key。命令默认从仓库根目录执行。

## 0. 准备

```bash
cargo --version
python3 --version
```

`Skill + MCP` 示例会启动 `agentflow-skills/examples/skills/mcp-basic/server.py`，因此需要本机有 `python3`。

如果你的 Cargo target 目录不在仓库内，或者当前环境不能写默认 `~/.agentflow`，用下面的前缀运行示例:

```bash
mkdir -p /tmp/agentflow-home
```

后续命令可以直接复制；它们显式使用 `/tmp/agentflow-target`，避免写入仓库外的默认 target 目录。

## 1. 固定 DAG 工作流

运行:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-core --example fixed_dag_workflow --target-dir /tmp/agentflow-target
```

这个示例位于 `agentflow-core/examples/fixed_dag_workflow.rs`。它构造四个确定性节点:

```text
validate_order
  -> calculate_subtotal
  -> calculate_shipping
  -> finalize_invoice
```

其中 `calculate_subtotal` 和 `calculate_shipping` 依赖同一个已校验订单，`finalize_invoice` 显式映射前面节点的输出。示例完成后会打印最终发票 JSON，重点检查这些字段:

```text
subtotal_cents
shipping_cents
tax_cents
total_cents
```

适用场景: 生产流程、批处理、RAG pipeline、订单/审批/报表等步骤稳定且需要可重放的任务。

## 2. agent-native ReAct

运行:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example agent_native_react --target-dir /tmp/agentflow-target
```

这个示例位于 `agentflow-agents/examples/agent_native_react.rs`。它不会创建 DAG，而是直接运行:

```text
AgentRuntime -> ReActAgent -> ToolRegistry -> echo tool
```

示例通过 `AGENTFLOW_MOCK_RESPONSES` 注入两轮 mock 模型输出:

1. 第一轮选择调用 `echo` 工具。
2. 第二轮根据工具结果给出最终答案。

成功时会看到:

```text
Answer:
final answer: echo: agent-native
```

同时会打印 `stop_reason` 和完整 `Runtime steps`，用于确认 observe、tool call、tool result、final answer 都进入了 agent runtime 结果。

适用场景: 工具选择、反思、记忆、多步推理等需要自主循环而不是固定节点顺序的任务。

## 3. Skill + MCP

先检查本地 Skill registry/index:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index validate agentflow-skills/examples/skills.index.toml
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index list agentflow-skills/examples/skills.index.toml
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index resolve agentflow-skills/examples/skills.index.toml mcp-demo
```

`agentflow-skills/examples/skills.index.toml` 是本地共享目录示例，`mcp-demo` alias 会解析到 `agentflow-skills/examples/skills/mcp-basic`。

也可以先安装到本地 skills 目录再验证:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill install agentflow-skills/examples/skills.index.toml mcp-demo --dir /tmp/agentflow-skills
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill validate /tmp/agentflow-skills/mcp-basic
```

`skill install` 会复制 index 中解析到的本地 Skill 目录；目标目录已存在时会拒绝覆盖，除非显式传入 `--force`。

先验证 Skill manifest 和 MCP 工具发现:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill validate agentflow-skills/examples/skills/mcp-basic
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill list-tools agentflow-skills/examples/skills/mcp-basic
```

预期工具包括:

```text
mcp_local_demo_echo
mcp_local_demo_status
```

然后运行无 LLM 的直接调用示例:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-skills --example skill_calls_mcp_tool --target-dir /tmp/agentflow-target
```

这个示例位于 `agentflow-skills/examples/skill_calls_mcp_tool.rs`，执行链路是:

```text
SKILL.md
  -> SkillLoader
  -> SkillBuilder::build_registry
  -> local stdio MCP server
  -> ToolRegistry
  -> mcp_local_demo_echo
```

成功时会看到类似输出:

```text
Called tool: mcp_local_demo_echo
Output: mcp-basic: from skill example
Is error: false
```

如果要通过 CLI 跑完整 agent loop，可以在配置好模型后执行:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill run agentflow-skills/examples/skills/mcp-basic \
  --message "echo hello through MCP" \
  --trace
```

这条可选命令会初始化 LLM provider，并把 MCP 工具暴露给 ReAct agent。`--trace` 会打印结构化 `AgentRunResult`。

适用场景: 把外部工具服务器、企业系统适配器或本地脚本能力包装成可复用 Skill。

## 4. hybrid: DAG 中嵌入 AgentNode

运行:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example hybrid_workflow_agent --target-dir /tmp/agentflow-target
```

这个示例位于 `agentflow-agents/examples/hybrid_workflow_agent.rs`。父工作流只有一个 `AgentNode`，但 agent 内部会调用一个 `WorkflowTool`:

```text
Parent Flow
  -> AgentNode
       -> ReActAgent
          -> ToolRegistry
             -> format_summary_workflow
                  -> child Flow
                     -> format_summary
```

成功时会看到:

```text
Agent response:
"Hybrid answer: workflow summary for hybrid DAG + agent runtime"
```

输出中的 `Agent runtime result` 会保留 agent step history，父 DAG 可以把它作为普通节点输出保存、追踪或 checkpoint。

适用场景: 大部分流程稳定，但其中一个节点需要 agent 决策、调用工具或处理自由文本。

## 5. WorkflowTool

`WorkflowTool` 在上面的 hybrid 示例中已经被实际调用。核心代码形态是:

```rust
let workflow_tool = WorkflowTool::new(
  "format_summary_workflow",
  "Run a deterministic child workflow that formats a summary.",
  child_workflow(),
);

let mut registry = ToolRegistry::new();
registry.register(Arc::new(workflow_tool));
```

agent 看到的是普通工具 `format_summary_workflow`，但工具执行时会运行 `child_workflow()` 这个确定性 DAG。工具参数会转换成子工作流初始输入，子工作流结果会作为 JSON 工具输出返回给 agent。

需要限制子工作流耗时时，可以配置:

```rust
let workflow_tool = WorkflowTool::new(name, description, flow)
  .with_timeout_ms(10_000);
```

适用场景: 让 agent 决定何时调用稳定业务流程，同时把可测试、可恢复的确定性逻辑留在 DAG 中。

## 6. 一次性验证

从仓库根目录运行:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-core --example fixed_dag_workflow --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example agent_native_react --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index validate agentflow-skills/examples/skills.index.toml
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index list agentflow-skills/examples/skills.index.toml
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index resolve agentflow-skills/examples/skills.index.toml mcp-demo
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill install agentflow-skills/examples/skills.index.toml mcp-demo --dir /tmp/agentflow-skills --force
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill validate /tmp/agentflow-skills/mcp-basic
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill validate agentflow-skills/examples/skills/mcp-basic
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill list-tools agentflow-skills/examples/skills/mcp-basic
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-skills --example skill_calls_mcp_tool --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example hybrid_workflow_agent --target-dir /tmp/agentflow-target
```

如果只想快速验证编译:

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo check --target-dir /tmp/agentflow-target -p agentflow-core -p agentflow-agents -p agentflow-skills -p agentflow-cli
```

## 7. 模式选择

| 需求 | 推荐模式 |
| --- | --- |
| 步骤固定、需要重放和审计 | 固定 DAG |
| 需要自主工具选择和多轮观察 | agent-native |
| 稳定流程中有一个非确定性决策点 | hybrid / `AgentNode` |
| 工具来自外部 MCP server | Skill + MCP |
| agent 应调用一个稳定子流程 | `WorkflowTool` |
