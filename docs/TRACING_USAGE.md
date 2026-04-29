# AgentFlow 工作流追踪系统使用指南

**版本**: v1.0
**日期**: 2025-11-23
**状态**: ✅ Production Ready

---

## 🎯 概述

AgentFlow 追踪系统提供详细的工作流执行追踪，包括：
- ✅ 每个节点的输入/输出
- ✅ LLM 提示词和响应
- ✅ Token 使用和成本
- ✅ 执行时间和性能指标
- ✅ 完整的调试日志

### 核心特性

1. **不污染核心** - 通过 EventListener 集成，零侵入
2. **零开销** - 不启用时无性能影响
3. **灵活存储** - 文件存储（开发）、数据库（生产）
4. **异步处理** - 不阻塞工作流执行
5. **可查询** - 支持过滤、分页、时间范围

### 关联上下文

持久化 trace 在每个层级都包含显式 `context` 字段，用来把一次
mixed run 串成同一棵树:

- workflow: `context.run_id` / `context.trace_id` 等于 workflow run id，`span_id = "workflow"`
- node: `span_id = "node:<node_id>"`，父 span 是 `workflow`
- agent: `span_id = "agent:<session_id>"`，父 span 是所在 workflow node
- tool/MCP call: `span_id = "tool:<index>:<tool>"`，父 span 是 agent

这些字段会直接写入 JSON trace，并与 OpenTelemetry exporter 的 span
层级保持一致。

---

## 📦 快速开始

### 1. 添加依赖

```toml
[dependencies]
agentflow-core = "0.3.0"
agentflow-tracing = "0.1.0"
```

### 2. 基本使用

```rust
use agentflow_core::events::{EventListener, WorkflowEvent};
use agentflow_tracing::{
    TraceCollector,
    TraceConfig,
    storage::file::FileTraceStorage,
    TraceStorage,
    format_trace_human_readable,
};
use std::sync::Arc;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建存储
    let storage = Arc::new(FileTraceStorage::new(
        PathBuf::from("./traces")
    )?);

    // 2. 创建追踪收集器
    let collector = Arc::new(TraceCollector::new(
        storage.clone(),
        TraceConfig::development()  // 或 TraceConfig::production()
    ));

    // 3. 使用收集器（实现 EventListener trait）
    // collector.on_event(&event);

    // 4. 查询追踪
    let trace = storage.get_trace("workflow-id").await?;
    if let Some(t) = trace {
        println!("{}", format_trace_human_readable(&t));
    }

    Ok(())
}
```

---

## ⚙️ 配置选项

### TraceConfig

```rust
use agentflow_tracing::{TraceConfig, StorageErrorPolicy};

// 开发环境配置（完整追踪）
let dev_config = TraceConfig::development();

// 生产环境配置（脱敏）
let prod_config = TraceConfig::production();

// 自定义配置
let custom_config = TraceConfig {
    capture_io: true,              // 是否捕获输入/输出
    capture_prompts: true,          // 是否捕获 LLM 提示词
    max_io_size_bytes: 1024 * 1024, // 最大数据大小（1MB）
    async_storage: true,            // 异步存储（推荐）
    on_storage_error: StorageErrorPolicy::LogError,
};
```

### 配置对比

| 配置项 | 开发环境 | 生产环境 | 说明 |
|--------|----------|----------|------|
| `capture_io` | ✅ true | ❌ false | 生产环境可能包含敏感数据 |
| `capture_prompts` | ✅ true | ❌ false | LLM 提示词可能包含敏感信息 |
| `max_io_size_bytes` | 10MB | 0 | 限制日志大小 |
| `async_storage` | ✅ true | ✅ true | 异步存储不阻塞工作流 |

---

## 💾 存储后端

### 1. 文件存储（开发环境）

```rust
use agentflow_tracing::storage::file::FileTraceStorage;
use std::path::PathBuf;

let storage = FileTraceStorage::new(
    PathBuf::from("./traces")
)?;

// 每个工作流一个 JSON 文件
// ./traces/workflow-001.json
// ./traces/workflow-002.json
```

**优点**:
- 简单易用
- 无需数据库
- 适合本地开发

**缺点**:
- 查询性能差
- 不适合大规模生产

### 2. PostgreSQL 存储（生产环境，未来支持）

```rust
// 未来版本
use agentflow_tracing::storage::postgres::PostgresTraceStorage;

let storage = PostgresTraceStorage::new(
    "postgresql://user:pass@localhost/agentflow"
).await?;
```

**优点**:
- 高性能查询
- 支持大规模数据
- 事务支持

---

## 🔍 查询追踪

### 获取单个追踪

```rust
use agentflow_tracing::TraceStorage;

// 通过 workflow_id 查询
let trace = storage.get_trace("workflow-001").await?;

if let Some(trace) = trace {
    println!("Status: {:?}", trace.status);
    println!("Duration: {}ms", trace.duration_ms().unwrap_or(0));
    println!("Nodes: {}", trace.nodes.len());
}
```

### 查询多个追踪

```rust
use agentflow_tracing::{TraceQuery, TraceStatus};
use chrono::{Utc, Duration};

// 构建查询
let query = TraceQuery {
    status: Some(TraceStatus::Completed),
    user_id: Some("user-123".to_string()),
    time_range: Some(TimeRange {
        start: Utc::now() - Duration::days(7),
        end: Utc::now(),
    }),
    limit: Some(50),
    offset: Some(0),
    ..Default::default()
};

// 执行查询
let traces = storage.query_traces(query).await?;

for trace in traces {
    println!("Workflow: {}", trace.workflow_id);
}
```

---

## 📊 格式化输出

### 人类可读格式

```rust
use agentflow_tracing::format_trace_human_readable;

let trace = storage.get_trace("workflow-001").await?.unwrap();
let output = format_trace_human_readable(&trace);
println!("{}", output);
```

输出示例：
```
═══════════════════════════════════════════════════════════
Workflow: AI Research Assistant
ID: workflow-001
═══════════════════════════════════════════════════════════

Status: Completed
Started: 2025-11-23 10:30:00 UTC
Completed: 2025-11-23 10:30:45 UTC
Duration: 45000ms
Environment: production
User: user-123
Tags: research, arxiv

───────────────────────────────────────────────────────────
Nodes Executed: 3
───────────────────────────────────────────────────────────

[1] search_arxiv (HttpNode)
    Status: Completed
    Duration: 5000ms
    Input: {"query": "large language models"}
    Output: {"papers": [...]}

[2] summarize (LLMNode)
    Status: Completed
    Duration: 30000ms
    Model: gpt-4o (openai)
    System Prompt: You are a research assistant...
    User Prompt: Summarize the following papers...
    Response: Here is a summary of the papers...
    Tokens: 1500 (prompt) + 500 (completion) = 2000
    Cost: $0.06
    Latency: 30000ms

[3] generate_report (TemplateNode)
    Status: Completed
    Duration: 100ms

═══════════════════════════════════════════════════════════
```

### JSON 格式

```rust
use agentflow_tracing::export_trace_json;

let json = export_trace_json(&trace)?;
println!("{}", json);

// 或保存到文件
std::fs::write("trace-001.json", json)?;
```

### 简洁摘要

```rust
use agentflow_tracing::format_trace_summary;

let summary = format_trace_summary(&trace);
println!("{}", summary);
// 输出: ✅ workflow-001 | completed | 3 nodes | 45000ms
```

### 终端调试视图

`agentflow trace tui` 提供最小的静态 TUI timeline，用于在不重新执行 workflow、tool、MCP server 或 LLM 的情况下聚焦查看已持久化 trace。

```bash
agentflow trace tui workflow-001 --dir ./traces
agentflow trace tui workflow-001 --dir ./traces --filter mcp --details
agentflow trace tui workflow-001 --dir ./traces --filter agent --max-field-chars 240
```

可用 filter:

- `all`: workflow、node、agent、tool/MCP 调用。
- `workflow`: workflow 和 node 层级概览。
- `agent`: agent session、step 数量、tool 数量。
- `tool`: 所有 tool 调用。
- `mcp`: 只显示 MCP tool 调用。

---

## 📝 事件类型

### 工作流事件

```rust
use agentflow_core::events::WorkflowEvent;

// 工作流开始
WorkflowEvent::WorkflowStarted {
    workflow_id: String,
    timestamp: Instant,
}

// 工作流完成
WorkflowEvent::WorkflowCompleted {
    workflow_id: String,
    duration: Duration,
    timestamp: Instant,
}

// 工作流失败
WorkflowEvent::WorkflowFailed {
    workflow_id: String,
    error: String,
    duration: Duration,
    timestamp: Instant,
}
```

### 节点事件

```rust
// 节点开始
WorkflowEvent::NodeStarted {
    workflow_id: String,
    node_id: String,
    timestamp: Instant,
}

// 节点完成
WorkflowEvent::NodeCompleted {
    workflow_id: String,
    node_id: String,
    duration: Duration,
    timestamp: Instant,
}

// 节点失败
WorkflowEvent::NodeFailed {
    workflow_id: String,
    node_id: String,
    error: String,
    duration: Duration,
    timestamp: Instant,
}
```

### LLM 专用事件

```rust
// LLM 提示词发送
WorkflowEvent::LLMPromptSent {
    workflow_id: String,
    node_id: String,
    model: String,              // "gpt-4o"
    provider: String,            // "openai"
    system_prompt: Option<String>,
    user_prompt: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    timestamp: Instant,
}

// LLM 响应接收
WorkflowEvent::LLMResponseReceived {
    workflow_id: String,
    node_id: String,
    model: String,
    response: String,
    usage: Option<TokenUsage>,  // Token 统计
    duration: Duration,
    timestamp: Instant,
}
```

---

## 🔧 高级用法

### 自定义 EventListener

```rust
use agentflow_core::events::{EventListener, WorkflowEvent};

struct MyCustomListener {
    // 你的字段
}

impl EventListener for MyCustomListener {
    fn on_event(&self, event: &WorkflowEvent) {
        match event {
            WorkflowEvent::LLMPromptSent { model, user_prompt, .. } => {
                // 发送到 Sentry
                // sentry::capture_message(&format!("LLM call to {}", model));
            }
            WorkflowEvent::WorkflowFailed { error, .. } => {
                // 发送告警
                // alert::send_notification(error);
            }
            _ => {}
        }
    }
}
```

### 组合多个监听器

```rust
use agentflow_core::events::MultiListener;

let multi_listener = MultiListener::new(vec![
    Box::new(TraceCollector::new(storage, config)),
    Box::new(MyMetricsListener),
    Box::new(MyAlertsListener),
]);

// 所有监听器都会收到事件
```

---

## 📈 性能考虑

### 1. 异步存储

```rust
let config = TraceConfig {
    async_storage: true,  // 推荐！
    ..Default::default()
};
```

**影响**:
- ✅ 不阻塞工作流执行
- ✅ 存储失败不影响工作流
- ⚠️ 需要等待异步任务完成才能查询

### 2. 数据大小限制

```rust
let config = TraceConfig {
    max_io_size_bytes: 1024 * 1024,  // 1MB
    ..Default::default()
};
```

**防止**:
- 日志文件过大
- 内存占用过高
- 查询性能下降

### 3. 数据脱敏（生产环境）

```rust
let config = TraceConfig::production(); // 自动脱敏

// 或手动配置
let config = TraceConfig {
    capture_io: false,       // 不捕获输入/输出
    capture_prompts: false,  // 不捕获提示词
    ..Default::default()
};
```

---

## 🐛 调试和故障排查

### 检查追踪是否保存

```rust
// 等待异步存储完成
tokio::time::sleep(Duration::from_millis(200)).await;

// 检查追踪
let trace = storage.get_trace("workflow-id").await?;
if trace.is_none() {
    println!("❌ Trace not found! Check storage configuration.");
}
```

### 查看错误策略

```rust
let config = TraceConfig {
    on_storage_error: StorageErrorPolicy::LogError,  // 记录错误
    // or
    on_storage_error: StorageErrorPolicy::Ignore,    // 忽略错误
    // or
    on_storage_error: StorageErrorPolicy::FailWorkflow, // 失败时中止
    ..Default::default()
};
```

### 查看正在运行的工作流

```rust
let running = collector.list_running().await;
println!("Running workflows: {}", running.len());

for trace in running {
    println!("  - {}: {} nodes", trace.workflow_id, trace.nodes.len());
}
```

---

## 📚 示例

### 完整示例

参见 `agentflow-tracing/examples/simple_tracing.rs`

运行：
```bash
cargo run --example simple_tracing -p agentflow-tracing
```

### 输出示例

```
╔══════════════════════════════════════════════════════╗
║   AgentFlow Tracing System - Simple Example         ║
╚══════════════════════════════════════════════════════╝

🚀 Simulating workflow execution: demo-workflow-001

   ✓ Workflow started
   ✓ Node 'fetch_papers' started
   ✓ Node 'fetch_papers' completed (200ms)
   ✓ Node 'summarize' started
   ✓ LLM prompt sent to gpt-4o
   ✓ LLM response received (300ms, 2000 tokens)
   ✓ Node 'summarize' completed (400ms)
   ✓ Workflow completed (750ms total)

📊 Retrieving execution trace...

[完整追踪输出...]

📈 Statistics:
   - Total nodes executed: 2
   - Duration: 813ms
   - Total tokens used: 2000
   - LLM latency: 300ms

✅ Example completed successfully!
```

---

## 🚀 下一步

1. **查看示例**: `cargo run --example simple_tracing`
2. **阅读设计文档**: `docs/TRACING_DESIGN.md`
3. **集成到你的工作流**: 添加 TraceCollector as EventListener
4. **配置存储**: 选择合适的存储后端

---

## 📖 相关文档

- [追踪系统设计](./TRACING_DESIGN.md) - 详细架构设计
- [架构重构总结](./REFACTORING_SUMMARY.md) - Core 纯粹性设计
- [API 文档](../agentflow-tracing/src/lib.rs) - 完整 API 参考

---

**维护者**: AgentFlow Team
**问题反馈**: GitHub Issues
**最后更新**: 2025-11-23
