# Tera 模板引擎集成分析

## 当前状态

- **Tera 依赖**: ✅ 已在 `agentflow-cli/Cargo.toml` 中添加 (v1.19)
- **使用情况**: ❌ 未使用，代码中没有任何 Tera 的引用
- **当前实现**: 简单的字符串替换（`template.rs`）

## 当前模板实现的限制

### TemplateNode 当前功能
```rust
// 仅支持简单的 {{ variable }} 替换
rendered = rendered.replace("{{ key }}", value);
```

### 限制清单

1. **❌ 无条件逻辑**
   ```
   当前不支持：
   {% if condition %}...{% endif %}
   ```

2. **❌ 无循环**
   ```
   当前不支持：
   {% for item in items %}...{% endfor %}
   ```

3. **❌ 无过滤器/函数**
   ```
   当前不支持：
   {{ name | upper }}
   {{ items | length }}
   {{ now() }}
   ```

4. **❌ 无数组/对象访问**
   ```
   当前不支持：
   {{ user.name }}
   {{ items[0] }}
   ```

5. **❌ 无数学运算**
   ```
   当前不支持：
   {{ count + 1 }}
   {{ price * 0.9 }}
   ```

6. **❌ 无模板继承**
   ```
   当前不支持：
   {% extends "base.html" %}
   {% block content %}...{% endblock %}
   ```

7. **❌ 无宏/可复用组件**
   ```
   当前不支持：
   {% macro greeting(name) %}Hello {{ name }}{% endmacro %}
   ```

## Tera 模板引擎的优势

### 1. 强大的控制流

```jinja2
{% if user.is_admin %}
  Admin Dashboard
{% elif user.is_member %}
  Member Area
{% else %}
  Guest View
{% endif %}
```

### 2. 循环和迭代

```jinja2
{% for item in items %}
  {{ loop.index }}: {{ item.name }}
{% endfor %}

{% for key, value in object %}
  {{ key }}: {{ value }}
{% endfor %}
```

### 3. 丰富的内置过滤器

```jinja2
{{ name | upper }}                    // 大写
{{ text | truncate(length=100) }}    // 截断
{{ items | length }}                  // 长度
{{ date | date(format="%Y-%m-%d") }} // 日期格式化
{{ list | join(sep=", ") }}          // 连接
{{ html | safe }}                     // 安全 HTML
{{ number | round }}                  // 四舍五入
```

### 4. 数学运算

```jinja2
{{ price * quantity }}
{{ (total - discount) * 1.1 }}
{{ count + 1 }}
```

### 5. 对象访问

```jinja2
{{ user.profile.name }}
{{ items[0].title }}
{{ data["key"] }}
```

### 6. 模板继承和包含

```jinja2
{% extends "base.html" %}
{% include "header.html" %}
```

### 7. 宏和可复用组件

```jinja2
{% macro render_user(user) %}
  <div>{{ user.name }} ({{ user.email }})</div>
{% endmacro %}

{{ render_user(user=current_user) }}
```

### 8. 测试和表达式

```jinja2
{% if name is defined %}...{% endif %}
{% if list is empty %}...{% endif %}
{{ value | default(value="N/A") }}
```

## 使用场景对比

### 场景 1: 条件输出

**当前实现（不支持）**:
```yaml
# 必须使用两个节点
- id: check
  type: llm
  ...

- id: conditional_output
  type: template
  run_if: "{{ nodes.check.outputs.result }}"
  parameters:
    template: "Condition is true"
```

**使用 Tera（优雅）**:
```yaml
- id: conditional_output
  type: template
  parameters:
    template: |
      {% if is_admin %}
        Admin Panel
      {% else %}
        User Panel
      {% endif %}
```

### 场景 2: 列表处理

**当前实现（需要 Map 节点）**:
```yaml
- id: process_list
  type: map
  parameters:
    input_list: [1, 2, 3]
    template:
      - id: format_item
        type: template
        parameters:
          template: "Item: {{ item }}"
```

**使用 Tera（直接）**:
```yaml
- id: format_list
  type: template
  parameters:
    template: |
      {% for item in items %}
        {{ loop.index }}. {{ item }}
      {% endfor %}
```

### 场景 3: 数据格式化

**当前实现（不支持）**:
```yaml
# 需要额外的节点来格式化日期、数字等
```

**使用 Tera**:
```yaml
- id: format_data
  type: template
  parameters:
    template: |
      Name: {{ name | upper }}
      Price: ${{ price | round(precision=2) }}
      Date: {{ timestamp | date(format="%Y-%m-%d") }}
      Items: {{ items | length }}
```

### 场景 4: 复杂报告生成

**当前实现（非常困难）**:
```yaml
# 需要多个节点和复杂的依赖链
```

**使用 Tera**:
```yaml
- id: generate_report
  type: template
  parameters:
    template: |
      # {{ project_name }} Report

      ## Summary
      Total Tasks: {{ tasks | length }}
      Completed: {{ tasks | filter(attribute="status", value="done") | length }}

      ## Task List
      {% for task in tasks %}
      - [{% if task.status == "done" %}x{% else %} {% endif %}] {{ task.title }}
        Priority: {{ task.priority | default(value="Normal") }}
        {% if task.due_date %}Due: {{ task.due_date | date(format="%Y-%m-%d") }}{% endif %}
      {% endfor %}

      ---
      Generated on {{ now() | date(format="%Y-%m-%d %H:%M:%S") }}
```

## 性能对比

### 当前实现
- ✅ 极快（简单字符串替换）
- ✅ 零开销（无解析）
- ❌ 功能有限

### Tera
- ⚠️ 稍慢（需要解析和编译）
- ✅ 可缓存编译后的模板
- ✅ 功能强大

**性能测试示例**:
```
简单替换 (当前):     ~0.5µs
Tera 首次渲染:       ~50µs  (编译 + 渲染)
Tera 缓存渲染:       ~5µs   (仅渲染)
```

## 集成方案设计

### 方案 1: 完全替换（推荐）

**优点**:
- 统一的模板系统
- 充分利用 Tera 的所有功能
- 代码更简洁

**缺点**:
- 轻微性能开销
- 需要修改现有模板语法（如果有）

**实现**:
```rust
use tera::Tera;
use std::sync::Arc;

pub struct TemplateNode {
    pub name: String,
    pub template: String,
    pub tera: Arc<Tera>,  // 共享 Tera 实例
}

impl AsyncNode for TemplateNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let mut context = tera::Context::new();

        // 将所有输入添加到上下文
        for (key, value) in inputs {
            context.insert(key, &flow_value_to_tera_value(value));
        }

        // 渲染模板
        let rendered = self.tera
            .render_str(&self.template, &context)
            .map_err(|e| AgentFlowError::NodeExecutionError {
                message: format!("Template error: {}", e)
            })?;

        // ... 返回结果
    }
}
```

### 方案 2: 双模式支持

**优点**:
- 向后兼容
- 用户可选择

**缺点**:
- 代码复杂
- 维护负担

**实现**:
```rust
pub enum TemplateEngine {
    Simple,  // 当前的简单替换
    Tera,    // Tera 引擎
}

pub struct TemplateNode {
    pub name: String,
    pub template: String,
    pub engine: TemplateEngine,
}
```

### 方案 3: 分离节点类型

**优点**:
- 清晰分离
- 无兼容性问题

**缺点**:
- 两种节点类型
- 用户需要选择

**实现**:
```yaml
# 简单模板
- id: simple
  type: template
  parameters:
    template: "Hello {{ name }}"

# Tera 模板
- id: advanced
  type: tera_template
  parameters:
    template: "{% if name %}Hello {{ name }}{% endif %}"
```

## 推荐实现步骤

### 第 1 步: 添加 Tera 支持到 agentflow-nodes

```toml
# agentflow-nodes/Cargo.toml
[dependencies]
tera = "1.19"
```

### 第 2 步: 创建 Tera 辅助函数

```rust
// agentflow-nodes/src/common/tera_helpers.rs

use agentflow_core::value::FlowValue;
use serde_json::Value as JsonValue;

pub fn flow_value_to_tera_value(value: &FlowValue) -> tera::Value {
    match value {
        FlowValue::Json(json) => json.clone().into(),
        FlowValue::File { path, .. } => path.to_string_lossy().to_string().into(),
        FlowValue::Url { url, .. } => url.clone().into(),
    }
}

pub fn register_custom_filters(tera: &mut tera::Tera) {
    // 添加自定义过滤器
    tera.register_filter("flow_path", |value, _| {
        // 自定义过滤器实现
    });
}
```

### 第 3 步: 修改 TemplateNode

```rust
// agentflow-nodes/src/nodes/template.rs

use tera::Tera;
use once_cell::sync::Lazy;

static TERA: Lazy<Tera> = Lazy::new(|| {
    let mut tera = Tera::default();
    crate::common::tera_helpers::register_custom_filters(&mut tera);
    tera
});

#[async_trait]
impl AsyncNode for TemplateNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let mut context = tera::Context::new();

        // 添加预定义变量
        for (key, value) in &self.variables {
            context.insert(key, value);
        }

        // 添加输入
        for (key, value) in inputs {
            context.insert(key, &flow_value_to_tera_value(value));
        }

        // 渲染
        let rendered = TERA.render_str(&self.template, &context)
            .map_err(|e| AgentFlowError::NodeExecutionError {
                message: format!("Template rendering failed: {}", e)
            })?;

        // 返回结果...
    }
}
```

### 第 4 步: 更新测试

```rust
#[tokio::test]
async fn test_tera_conditional() {
    let node = TemplateNode::new("test", "{% if show %}Hello{% endif %}");
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert("show".to_string(), FlowValue::Json(json!(true)));

    let result = node.execute(&inputs).await.unwrap();
    assert_eq!(result.get("output"), &FlowValue::Json(json!("Hello")));
}

#[tokio::test]
async fn test_tera_loop() {
    let node = TemplateNode::new("test",
        "{% for i in items %}{{ i }}{% endfor %}");
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert("items".to_string(), FlowValue::Json(json!([1, 2, 3])));

    let result = node.execute(&inputs).await.unwrap();
    assert_eq!(result.get("output"), &FlowValue::Json(json!("123")));
}
```

### 第 5 步: 更新文档和示例

```yaml
# examples/tera-template-example.yml
name: "Tera Template Example"

nodes:
  - id: "advanced_format"
    type: "template"
    parameters:
      template: |
        # {{ project_name | upper }}

        {% if tasks %}
        ## Tasks ({{ tasks | length }})
        {% for task in tasks %}
        - {{ task.name }} ({{ task.status | default(value="pending") }})
        {% endfor %}
        {% else %}
        No tasks found.
        {% endif %}
```

## 迁移指南

### 兼容性

**好消息**: 现有的简单模板仍然有效！

```yaml
# 这些仍然可以工作
template: "Hello {{ name }}"
template: "Count: {{ count }}"
```

**新功能**: 现在可以使用高级功能

```yaml
# 新的高级模板
template: |
  {% if name %}
    Hello {{ name | upper }}!
  {% else %}
    Hello stranger!
  {% endif %}
```

### 潜在问题

1. **冲突的语法**: 如果现有模板中有 `{%` 或 `{#`，可能被误认为 Tera 控制语句
   - **解决**: 使用 `{{ "{% " }}` 或 `{% raw %}...{% endraw %}`

2. **过滤器冲突**: 如果变量名包含 `|`
   - **影响**: 很少见
   - **解决**: 重命名变量

## 成本收益分析

### 成本
- ⏱️ 实现时间: ~4-6 小时
- 📦 依赖大小: +250KB (已在依赖中)
- 🐛 潜在 Bug: 低（Tera 成熟稳定）
- 📚 文档更新: 中等

### 收益
- ✨ 大幅提升模板功能
- 🎯 减少所需节点数量
- 📝 更清晰的工作流
- 🚀 更好的用户体验
- 🔧 与行业标准对齐（Jinja2 风格）

## 决策建议

### 强烈推荐实现 Tera 集成 ✅

**理由**:

1. **已有依赖** - Tera 已经在 Cargo.toml 中，没有额外成本
2. **功能缺口大** - 当前实现缺少关键功能
3. **行业标准** - Tera 基于 Jinja2，用户熟悉
4. **低风险** - 向后兼容，成熟的库
5. **高价值** - 显著提升工作流表达能力

### 不实现的风险

- 用户需要编写更多节点完成简单任务
- 与其他工作流工具相比功能较弱
- 复杂的模板逻辑需要使用 LLM 节点（成本高）

## 替代方案

如果不使用 Tera，可以考虑：

1. **Handlebars-rs** - 类似功能，但社区较小
2. **MiniJinja** - 更轻量，但功能较少
3. **自行实现** - 成本极高，不推荐

## 结论

**建议: 立即实现 Tera 集成**

- 实现简单（方案 1）
- 价值巨大
- 零额外依赖成本
- 向后兼容
- 符合用户期望

下一步：等待确认后开始实现。
