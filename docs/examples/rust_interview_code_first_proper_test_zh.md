# Rust Interview Code-First Workflow Example (Chinese)

## 概述

`rust_interview_code_first_proper_test_zh.rs` 是一个完整的示例，展示如何正确使用 AgentFlow 的代码优先方法来构建工作流程。这个例子专门针对 Rust 后端面试问题的生成和评估，使用中文系统提示。

## 核心架构

### LlmNode - LLM 集成节点

这个自定义节点展示了如何正确地将 `agentflow-core` 的工作流程引擎与 `agentflow-llm` 的模型接口集成：

```rust
pub struct LlmNode {
    name: String,
    model: String,
    prompt_template: String,
    system_template: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}
```

#### 关键特性：
- **模板解析**: 使用 `SharedState::resolve_template_advanced()` 进行动态模板替换
- **流畅式构建器**: 支持链式方法配置节点参数
- **状态管理**: 通过 SharedState 在节点间共享数据
- **错误处理**: 包含优雅的降级机制和模拟响应

### InterviewWorkflow - 工作流程编排器

展示了如何构建完整的工作流程：

```rust
pub struct InterviewWorkflow {
    shared_state: SharedState,
    question_generator: LlmNode,
    question_evaluator: LlmNode,
}
```

## 工作流程执行流程

### 1. 初始化阶段
```rust
let shared_state = SharedState::new();
shared_state.insert("model", Value::String("step-2-mini"));
shared_state.insert("experience_level", Value::String("3-5 years"));
```

### 2. 节点配置
- **问题生成节点**: 创建中文 Rust 面试题
- **问题评估节点**: 评估题目质量和难度适配性

### 3. 执行编排
工作流程展示了正确的节点依赖关系：
1. 问题生成节点独立执行
2. 评估节点依赖于生成节点的输出（通过模板 `{{ question_generator_output }}`）

## 数据流详解 - 节点间的数据传递机制

### 核心机制：SharedState + 模板解析

数据从 `question_generator` 节点传递到 `question_evaluator` 节点的过程分为以下几个关键步骤：

#### 第一步：问题生成节点存储结果 (`post_async`)

```rust
async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
) -> Result<Option<String>, agentflow_core::AgentFlowError> {
    // 生成唯一的输出键名：节点名 + "_output"
    let output_key = format!("{}_output", self.name);  // "question_generator_output"
    
    // 将 LLM 的执行结果存储到 SharedState 中
    shared.insert(output_key.clone(), exec_result.clone());
    
    println!("💾 Stored result in SharedState as: {}", output_key);
    Ok(None)
}
```

**关键点**:
- `run_async` 方法会自动调用 `post_async`
- 结果以 `{node_name}_output` 格式存储在 SharedState 中
- 对于 `question_generator` 节点，键名为 `"question_generator_output"`

#### 第二步：问题评估节点配置模板依赖

```rust
let question_evaluator = LlmNode::new("question_evaluator", "step-2-mini")
    .with_prompt("{{ question_generator_output }}")  // 声明模板依赖
    .with_system("你是一名资深的 Rust 后端面试官，请帮我评估以下面试题是否符合 {{ experience_level }} 级别的 Rust 后端开发标准");
```

**关键点**:
- 模板 `{{ question_generator_output }}` 引用第一个节点的输出
- 模板 `{{ experience_level }}` 引用 SharedState 中的初始配置

#### 第三步：模板解析和数据注入 (`prep_async`)

```rust
async fn prep_async(&self, shared: &SharedState) -> Result<Value, agentflow_core::AgentFlowError> {
    // SharedState 自动解析模板中的占位符
    let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
    let resolved_system = self
        .system_template
        .as_ref()
        .map(|s| shared.resolve_template_advanced(s));
    
    // resolved_prompt 现在包含第一个节点生成的实际面试题
    // resolved_system 包含解析后的系统提示
}
```

**关键点**:
- `resolve_template_advanced()` 查找 SharedState 中对应的值
- `{{ question_generator_output }}` 被替换为实际的面试题内容
- `{{ experience_level }}` 被替换为 "3-5 years"

### 完整的数据流时序图

```
1. workflow.execute() 调用
   ↓
2. question_generator.run_async()
   ├── prep_async()  - 解析模板（无依赖）
   ├── exec_async()  - 调用 LLM 生成面试题
   └── post_async()  - 存储结果到 SharedState["question_generator_output"]
   ↓
3. question_evaluator.run_async()
   ├── prep_async()  - 解析模板：{{ question_generator_output }} → 实际面试题
   ├── exec_async()  - 调用 LLM 评估面试题
   └── post_async()  - 存储结果到 SharedState["question_evaluator_output"]
   ↓
4. 从 SharedState 提取最终结果
```

### `run_async` 方法的作用

`run_async` 是 AgentFlow 核心的编排方法，它：

1. **自动调用生命周期方法**: `prep_async` → `exec_async` → `post_async`
2. **处理模板解析**: 在 `prep_async` 中自动解析所有模板占位符
3. **管理状态传递**: 确保每个阶段的结果正确传递到下一阶段
4. **提供错误处理**: 统一的错误处理和日志记录

### 数据流的优势

#### 1. 声明式依赖关系
```rust
// 无需手动传递数据，只需声明模板依赖
.with_prompt("{{ question_generator_output }}")
```

#### 2. 类型安全的状态访问
```rust
// 运行时验证模板键是否存在
let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
```

#### 3. 解耦的节点设计
- 节点不需要直接引用其他节点
- 通过 SharedState 进行松耦合的数据交换
- 便于测试和重构

#### 4. 可扩展的依赖图
```rust
// 可以轻松添加更多依赖节点
.with_prompt("基于问题: {{ question_generator_output }} 和评估: {{ question_evaluator_output }}")
```

这种设计模式使得 AgentFlow 能够处理复杂的有向无环图（DAG）工作流程，同时保持代码的简洁性和可维护性。

## 关键技术特性

### 模板依赖管理
```rust
.with_prompt("{{ question_generator_output }}")  // 模板依赖!
```
展示了如何在工作流程中建立节点间的数据依赖关系。

### 鲁棒性功能
```rust
// 超时保护
self.question_generator
    .run_async_with_timeout(&self.shared_state, timeout_duration)
    .await?;

// 重试机制
self.question_evaluator
    .run_async_with_retries(&self.shared_state, 3, retry_wait)
    .await?;
```

### 可观测性
```rust
println!("📊 Workflow State After Execution:");
for (key, value) in self.shared_state.iter() {
    // 显示工作流程状态
}
```

## 使用说明

### 运行基本工作流程
```bash
cargo run --example rust_interview_code_first_proper_test_zh
```

### 预期输出
1. AgentFlow 系统初始化日志
2. 问题生成节点执行状态
3. 模板解析和依赖处理信息
4. 问题评估节点执行状态
5. 最终的面试题和质量评估结果

## 架构优势

### 1. 正确的关注点分离
- `agentflow-core`: 工作流程编排和状态管理
- `agentflow-llm`: LLM 提供商抽象和 API 调用
- 自定义节点: 业务逻辑和集成桥梁

### 2. 模板驱动的依赖管理
- 声明式的节点依赖关系
- 自动的状态解析和注入
- 类型安全的状态访问

### 3. 生产就绪的特性
- 错误处理和优雅降级
- 超时和重试机制
- 详细的日志和可观测性
- 模拟响应支持开发和测试

## 最佳实践示例

### 1. 节点设计模式
```rust
impl LlmNode {
    pub fn new(name: &str, model: &str) -> Self
    pub fn with_prompt(mut self, template: &str) -> Self
    pub fn with_system(mut self, template: &str) -> Self
    // 流畅式构建器模式
}
```

### 2. 异步节点实现
```rust
#[async_trait]
impl AsyncNode for LlmNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError>
    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError>
    async fn post_async(&self, shared: &SharedState, ...) -> Result<Option<String>, AgentFlowError>
}
```

### 3. 状态管理策略
- 在 `prep_async` 中解析模板
- 在 `exec_async` 中执行业务逻辑
- 在 `post_async` 中存储结果到共享状态

## 中文本地化特性

### 系统提示中文化
```rust
.with_system("你是一个资深Rust后端开发工程师")
.with_prompt("请帮我创建5道Rust后端面试题")
```

### 评估标准本地化
```rust
.with_system("你是一名资深的 Rust 后端面试官，请帮我评估以下面试题是否符合 {{ experience_level }} 级别的 Rust 后端开发标准")
```

## 技术要求

- Rust 2021 edition
- Tokio 异步运行时
- AgentFlow 0.1.0+
- 有效的 LLM API 密钥（用于实际 API 调用）

## 扩展建议

1. **添加更多节点类型**: 代码评审、技术栈评估等
2. **实现工作流程链**: 多轮面试问题生成
3. **集成持久化**: 将结果保存到数据库
4. **添加 Web 接口**: 提供 REST API 服务
5. **支持批处理**: 并行处理多个候选人

这个示例展示了 AgentFlow 的完整能力，是学习如何构建生产级别工作流程应用的最佳起点。