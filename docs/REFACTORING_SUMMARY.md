# AgentFlow Core 重构总结

**日期**: 2025-11-22
**版本**: v0.2.0 → v0.3.0
**状态**: ✅ 完成

## 🎯 重构目标

**保持 agentflow-core 纯粹**，只包含工作流编排的核心功能。

---

## ✅ 完成的工作

### 1. 移除了什么（1,678 行代码）

**删除的模块**：
- ❌ `logging.rs` (440 行) - 日志配置
- ❌ `metrics.rs` (348 行) - Prometheus 指标
- ❌ `observability.rs` (481 行) - 事件收集器
- ❌ `health.rs` (409 行) - 健康检查

**删除的依赖**：
- ❌ `tracing` (optional)
- ❌ `tracing-subscriber` (optional)
- ❌ `prometheus` (optional)
- ❌ `lazy_static` (optional)

**删除的 feature flags**：
- ❌ `observability`
- ❌ `metrics`

---

### 2. 新增了什么（300 行代码）

**新增模块**：
- ✅ `events.rs` (300 行) - 轻量级事件系统

**核心特性**：
```rust
// 事件定义（零依赖）
pub enum WorkflowEvent {
    WorkflowStarted { ... },
    NodeCompleted { ... },
    // ...
}

// 监听器 trait（用户实现）
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &WorkflowEvent);
}

// 内置监听器
pub struct NoOpListener;      // 零开销（默认）
pub struct ConsoleListener;   // 打印到控制台
pub struct MultiListener;     // 组合多个监听器
```

---

### 3. 保留了什么（核心功能）

```
agentflow-core/
├── 核心抽象
│   ├── node.rs
│   ├── async_node.rs
│   ├── flow.rs
│   └── value.rs
│
├── 执行引擎
│   ├── concurrency.rs
│   ├── retry.rs
│   └── timeout.rs
│
├── 可靠性
│   ├── checkpoint.rs
│   ├── resource_manager.rs
│   └── state_monitor.rs
│
├── 可观测性（轻量级）
│   └── events.rs  ✨ 新增
│
└── 错误处理
    ├── error.rs
    └── error_context.rs
```

---

## 📊 对比

| 指标 | 重构前 | 重构后 | 变化 |
|------|--------|--------|------|
| 模块数量 | 22 个 | 18 个 | -4 |
| 可观测性代码 | 1,678 行 | 300 行 | -82% |
| 依赖数量 | 25+ | 20 | -5 |
| Feature flags | 3 个 | 0 个 | -100% |
| 测试数量 | 107 个 | 93 个 | -14 |
| **核心纯度** | ⚠️ 混杂 | ✅ 纯粹 | 💯 |

---

## 🔄 迁移指南

### 场景 1: 你之前使用了日志

**之前**（不再可用）：
```rust
use agentflow_core::logging;

logging::init(); // ❌ 已移除
```

**现在**（两种选择）：

**选项 A: 使用事件监听器**
```rust
use agentflow_core::events::{WorkflowEvent, EventListener, ConsoleListener};

struct MyLogger;

impl EventListener for MyLogger {
    fn on_event(&self, event: &WorkflowEvent) {
        // 使用任何你喜欢的日志库
        log::info!("{}", event);          // 或者
        tracing::info!("{}", event);      // 或者
        println!("{}", event);            // 或者
        // 发送到 Sentry, DataDog, etc.
    }
}

// 在 Flow 中使用
let flow = Flow::new()
    .with_listener(Box::new(MyLogger));
```

**选项 B: 自己配置日志（推荐）**
```rust
// 在你的 main.rs
fn main() {
    // 使用标准的 env_logger
    env_logger::init();
    
    // 或使用 tracing
    tracing_subscriber::fmt::init();
    
    // 然后在你的节点中正常打日志
}
```

---

### 场景 2: 你之前使用了 Prometheus 指标

**之前**（不再可用）：
```rust
use agentflow_core::metrics;

metrics::increment_counter("workflow_completed"); // ❌ 已移除
```

**现在**（使用事件监听器）：
```rust
use agentflow_core::events::{WorkflowEvent, EventListener};
use prometheus::{Counter, Registry};

struct PrometheusListener {
    workflow_completed: Counter,
}

impl EventListener for PrometheusListener {
    fn on_event(&self, event: &WorkflowEvent) {
        match event {
            WorkflowEvent::WorkflowCompleted { .. } => {
                self.workflow_completed.inc();
            }
            _ => {}
        }
    }
}
```

---

### 场景 3: 你之前使用了健康检查

**之前**（不再可用）：
```rust
use agentflow_core::health::HealthChecker; // ❌ 已移除
```

**现在**（等待 agentflow-telemetry）：
- 健康检查功能将在未来的 `agentflow-telemetry` crate 中提供
- 如果你需要立即使用，可以自己实现简单的健康检查端点

---

## 💡 设计哲学

### 为什么移除这些功能？

1. **保持核心纯粹**
   - Core 应该只有工作流编排逻辑
   - 日志、指标、追踪是"附加功能"，不是核心

2. **用户自由选择**
   - 用户可以选择任何日志库（log, tracing, slog, etc.）
   - 用户可以选择任何指标库（Prometheus, StatsD, etc.）
   - Core 不强制任何特定实现

3. **零依赖开销**
   - 如果你不需要可观测性，零开销
   - 不需要可选的 feature flags

4. **简化架构**
   - 减少 core 的复杂度
   - 更容易理解和维护

---

## 🔮 未来计划

### agentflow-telemetry（计划中）

未来会创建独立的 `agentflow-telemetry` crate：

```
agentflow-telemetry/
├── listeners/
│   ├── logging.rs       // 预配置的日志监听器
│   ├── prometheus.rs    // Prometheus 指标监听器
│   └── opentelemetry.rs // OpenTelemetry 追踪
├── health.rs            // 健康检查
└── prelude.rs           // 开箱即用的组合
```

**使用示例**（未来）：
```toml
[dependencies]
agentflow-core = "0.3.0"
agentflow-telemetry = "0.3.0"  # 可选
```

```rust
use agentflow_telemetry::prelude::*;

let flow = Flow::new()
    .with_listener(Box::new(TelemetryListener::new()
        .with_logging()
        .with_prometheus()
        .with_tracing()
    ));
```

---

## ✅ 验证

**编译通过**：
```bash
✓ cargo build -p agentflow-core
```

**测试通过**：
```bash
✓ 93/93 tests passing (100%)
```

**代码减少**：
```bash
✓ -1,378 行代码 (-82%)
✓ -5 个依赖
✓ -2 个 feature flags
```

---

## 📖 相关文档

- `ARCHITECTURE.md` - 更新的架构文档
- `agentflow-core/src/events.rs` - 事件系统 API 文档
- `CLAUDE.md` - 项目配置

---

**维护者**: AgentFlow Core Team
**问题反馈**: GitHub Issues
