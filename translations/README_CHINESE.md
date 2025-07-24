# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **一个现代化的、异步优先的 Rust 框架，用于构建具有企业级稳健性和可观测性的智能代理工作流。**

AgentFlow 是一个受 PocketFlow 概念启发的新 Rust 框架，提供生产就绪的工作流编排，具备异步并发、可观测性和可靠性模式。

## 🚀 核心特性

### ⚡ **异步优先架构**

- 基于 Tokio 运行时构建，提供高性能异步执行
- 原生支持并行和批处理
- 利用 Rust 所有权模型的零成本抽象
- Send + Sync 兼容，确保安全并发

### 🛡️ **企业级稳健性**

- **熔断器**: 自动故障检测和恢复
- **速率限制**: 滑动窗口算法进行流量控制
- **重试策略**: 带抖动的指数退避
- **超时管理**: 负载下的优雅降级
- **资源池**: RAII 守卫确保安全的资源管理
- **负载削减**: 自适应容量管理

### 📊 **全面可观测性**

- 流级别和节点级别的实时指标收集
- 带时间戳和持续时间的结构化事件日志
- 性能分析和瓶颈检测
- 可配置的报警系统
- 分布式追踪支持
- 与监控平台集成就绪

### 🔄 **灵活的执行模型**

- **顺序流**: 传统的节点到节点执行
- **并行执行**: 使用 `futures::join_all` 的并发节点处理
- **批处理**: 可配置批次大小的并发批次执行
- **嵌套流**: 分层工作流组合
- **条件路由**: 基于运行时状态的动态流控制

## 📦 安装

在您的 `Cargo.toml` 中添加 AgentFlow：

```toml
[dependencies]
agentflow-core = "0.2.0"
tokio = { version = "1.0", features = ["full"] }
```

## 🎯 快速开始

### 基本顺序流

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;

// 定义自定义节点
struct GreetingNode {
    name: String,
}

#[async_trait]
impl AsyncNode for GreetingNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        Ok(Value::String(format!("正在为 {} 准备问候", self.name)))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        Ok(Value::String(format!("你好，{}！", self.name)))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("greeting".to_string(), exec);
        Ok(None) // 结束流
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let node = Box::new(GreetingNode {
        name: "AgentFlow".to_string()
    });

    let flow = AsyncFlow::new(node);
    let shared = SharedState::new();

    let result = flow.run_async(&shared).await?;
    println!("流程完成: {:?}", result);

    Ok(())
}
```

### 带可观测性的并行执行

```rust
use agentflow_core::{AsyncFlow, MetricsCollector};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // 为并行执行创建节点
    let nodes = vec![
        Box::new(ProcessingNode { id: "worker_1".to_string() }),
        Box::new(ProcessingNode { id: "worker_2".to_string() }),
        Box::new(ProcessingNode { id: "worker_3".to_string() }),
    ];

    // 设置可观测性
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("parallel_processing".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await?;

    // 检查指标
    let execution_count = metrics.get_metric("parallel_processing.execution_count");
    println!("执行次数: {:?}", execution_count);

    Ok(())
}
```

### 带熔断器的稳健流

```rust
use agentflow_core::{CircuitBreaker, TimeoutManager};
use std::time::Duration;

async fn robust_workflow() -> Result<()> {
    // 设置稳健性模式
    let circuit_breaker = CircuitBreaker::new(
        "api_calls".to_string(),
        3, // 故障阈值
        Duration::from_secs(30) // 恢复超时
    );

    let timeout_manager = TimeoutManager::new(
        "operations".to_string(),
        Duration::from_secs(10) // 默认超时
    );

    // 在工作流逻辑中使用
    let result = circuit_breaker.call(async {
        timeout_manager.execute_with_timeout("api_call", async {
            // 您的业务逻辑在这里
            Ok("成功")
        }).await
    }).await?;

    Ok(())
}
```

## 🏗️ 架构

AgentFlow 建立在四个核心支柱之上：

1. **执行模型**: 具有 prep/exec/post 生命周期的 AsyncNode 特征
2. **并发控制**: 并行、批处理和嵌套执行模式
3. **稳健性保证**: 熔断器、重试、超时和资源管理
4. **可观测性**: 指标、事件、报警和分布式追踪

有关详细的架构信息，请参阅 [docs/design.md](docs/design.md)。

## 📚 文档

- **[设计文档](docs/design.md)** - 系统架构和组件图
- **[功能规范](docs/functional-spec.md)** - 功能需求和 API 规范
- **[用例](docs/use-cases.md)** - 实际应用场景
- **[API 参考](docs/api/)** - 完整的 API 文档
- **[迁移指南](docs/migration.md)** - 从 PocketFlow 升级

## 🧪 测试

AgentFlow 通过综合测试套件保持 100% 测试覆盖率：

```bash
# 运行所有测试
cargo test

# 带输出运行
cargo test -- --nocapture

# 运行特定模块测试
cargo test async_flow
cargo test robustness
cargo test observability
```

**当前状态**: 67/67 测试通过 ✅

## 🚢 生产就绪

AgentFlow 专为生产环境设计，具有：

- **内存安全**: Rust 的所有权模型防止数据竞争和内存泄漏
- **性能**: 零成本抽象和高效的异步运行时
- **可靠性**: 全面的错误处理和优雅降级
- **可扩展性**: 内置水平扩展模式支持
- **监控**: 完整的可观测性堆栈提供生产洞察

## 🛣️ 路线图

- **v0.3.0**: MCP (模型上下文协议) 集成
- **v0.4.0**: 分布式执行引擎
- **v0.5.0**: WebAssembly 插件系统
- **v1.0.0**: 生产稳定性保证

## 🤝 贡献

我们欢迎贡献！请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解指导方针。

## 📄 许可证

本项目采用 MIT 许可证 - 详情请参阅 [LICENSE](LICENSE) 文件。

## 🙏 致谢

- 建立在原始 PocketFlow 概念的基础之上
- 受现代分布式系统模式启发
- 由 Rust 生态系统和 Tokio 运行时支持

---

**AgentFlow**: 智能工作流与企业可靠性的交汇之处。🦀✨