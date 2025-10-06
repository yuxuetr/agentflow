# AgentFlow 循环节点使用指南

AgentFlow 支持两种循环节点类型，用于处理迭代和批量操作：

## 1. Map 节点 - 批量处理

Map 节点用于对列表中的每个元素应用相同的处理逻辑。

### 基本语法

```yaml
- id: "process_items"
  type: "map"
  parameters:
    # 要处理的列表
    input_list: [1, 2, 3, 4, 5]

    # 是否并行处理（可选，默认 false）
    parallel: false

    # 子工作流模板 - 对每个元素执行
    template:
      - id: "process_item"
        type: "template"  # 或其他节点类型
        parameters:
          # 使用 {{ item }} 引用当前元素
          template: "Processing: {{ item }}"
```

### 参数说明

- **input_list**: 必需，要处理的数组
- **parallel**: 可选，布尔值
  - `false` (默认): 顺序处理，一个接一个
  - `true`: 并行处理，所有元素同时执行
- **template**: 必需，子工作流节点数组
  - 子工作流中使用 `{{ item }}` 访问当前处理的元素

### 输出

Map 节点输出一个名为 `results` 的数组，包含所有子工作流的执行结果。

```yaml
# 访问 map 节点的输出
input_mapping:
  all_results: "{{ nodes.process_items.outputs.results }}"
```

### 示例 1: 顺序处理数字

```yaml
name: "Sequential Number Processing"

nodes:
  - id: "double_numbers"
    type: "map"
    parameters:
      input_list: [10, 20, 30]
      parallel: false
      template:
        - id: "doubler"
          type: "template"
          parameters:
            template: "{{ item * 2 }}"
```

### 示例 2: 并行批量 LLM 调用

```yaml
name: "Parallel LLM Batch Processing"

nodes:
  - id: "batch_translate"
    type: "map"
    parameters:
      input_list: ["Hello", "Good morning", "Thank you"]
      parallel: true  # 并行执行以提高速度
      template:
        - id: "translate"
          type: "llm"
          parameters:
            model: "gpt-4o"
            prompt: "Translate to Chinese: {{ item }}"
```

### 最佳实践

1. **小列表用顺序，大列表用并行**
   - 小于 10 个元素: `parallel: false` (更易调试)
   - 大于 10 个元素: `parallel: true` (更快执行)

2. **注意 API 限流**
   - 并行调用 LLM API 时注意速率限制
   - 如遇到限流错误，改用顺序处理

3. **错误处理**
   - 目前任一元素失败会导致整个 Map 节点失败
   - 在子工作流中添加错误处理逻辑

---

## 2. While 节点 - 条件循环

While 节点用于重复执行工作流直到条件不满足。

### 基本语法

```yaml
- id: "loop_task"
  type: "while"
  parameters:
    # 循环条件 - 为 true 时继续循环
    condition: "{{ should_continue }}"

    # 最大迭代次数（防止无限循环）
    max_iterations: 10

    # 初始循环变量
    counter: 0
    should_continue: true

    # 循环体 - 每次迭代执行的子工作流
    do:
      - id: "step1"
        type: "template"
        parameters:
          template: "Count: {{ counter }}"

      - id: "update"
        type: "template"
        parameters:
          # 更新循环状态
          template: |
            {
              "counter": {{ counter + 1 }},
              "should_continue": {{ counter + 1 < 5 }}
            }
```

### 参数说明

- **condition**: 必需，循环继续的条件
  - 使用 `{{ variable }}` 引用循环变量
  - 当条件为 `true` 时继续，为 `false` 时停止

- **max_iterations**: 必需，最大迭代次数
  - 防止无限循环的安全措施
  - 达到最大次数后强制停止

- **初始变量**: 任意键值对
  - 这些变量在循环体中可访问
  - 每次迭代后会被子工作流的输出更新

- **do**: 必需，子工作流节点数组
  - 每次迭代执行的操作
  - 输出会合并回循环状态

### 循环机制

1. **初始化**: 使用提供的初始参数
2. **条件检查**: 评估 `condition`
3. **执行**: 如果条件为 true，执行 `do` 中的子工作流
4. **状态更新**: 子工作流的输出合并回循环变量
5. **重复**: 返回步骤 2，直到条件为 false 或达到 max_iterations

### 示例 1: 简单计数器

```yaml
name: "Simple Counter"

nodes:
  - id: "count_to_five"
    type: "while"
    parameters:
      condition: "{{ continue }}"
      max_iterations: 10
      count: 0
      continue: true
      do:
        - id: "increment"
          type: "template"
          parameters:
            template: |
              {
                "count": {{ count + 1 }},
                "continue": {{ count + 1 < 5 }}
              }
```

### 示例 2: 迭代式故事生成

```yaml
name: "Iterative Story Builder"

nodes:
  - id: "build_story"
    type: "while"
    parameters:
      condition: "{{ not_finished }}"
      max_iterations: 5
      story: "Once upon a time..."
      chapter: 0
      not_finished: true
      do:
        # 生成下一章节
        - id: "write_chapter"
          type: "llm"
          parameters:
            model: "gpt-4o"
            prompt: |
              Current story:
              {{ story }}

              Write the next chapter (chapter {{ chapter + 1 }}).

        # 更新故事状态
        - id: "update_story"
          type: "template"
          dependencies: ["write_chapter"]
          input_mapping:
            new_chapter: "{{ nodes.write_chapter.outputs.output }}"
          parameters:
            template: |
              {
                "story": "{{ story }}\n\nChapter {{ chapter + 1 }}:\n{{ new_chapter }}",
                "chapter": {{ chapter + 1 }},
                "not_finished": {{ chapter + 1 < 3 }}
              }
```

### 示例 3: 数值收敛

```yaml
name: "Numerical Convergence"

nodes:
  - id: "converge"
    type: "while"
    parameters:
      condition: "{{ not_converged }}"
      max_iterations: 20
      value: 100.0
      not_converged: true
      do:
        - id: "iterate"
          type: "template"
          parameters:
            template: |
              {
                "value": {{ value * 0.9 }},
                "not_converged": {{ value * 0.9 > 1.0 }}
              }
```

### 最佳实践

1. **始终设置 max_iterations**
   - 防止意外的无限循环
   - 建议值: 10-100 之间

2. **明确退出条件**
   - 确保循环能够终止
   - 在子工作流中更新条件变量

3. **使用有意义的变量名**
   ```yaml
   # 好
   should_continue: true
   is_complete: false

   # 不好
   flag: true
   x: false
   ```

4. **调试技巧**
   - 在子工作流中添加日志节点
   - 使用较小的 max_iterations 测试
   - 检查每次迭代的状态输出

5. **性能考虑**
   - While 循环是顺序执行的（不能并行）
   - 如需处理大量独立数据，使用 Map 节点代替
   - 每次迭代都会创建新的子工作流实例

---

## 嵌套循环

可以在 Map 或 While 节点内嵌套另一个循环节点：

```yaml
name: "Nested Loops Example"

nodes:
  # 外层: Map 遍历多个主题
  - id: "process_topics"
    type: "map"
    parameters:
      input_list: ["AI", "Blockchain", "Quantum Computing"]
      parallel: false
      template:
        # 内层: While 迭代式深入研究每个主题
        - id: "research_topic"
          type: "while"
          parameters:
            condition: "{{ depth < 3 }}"
            max_iterations: 3
            topic: "{{ item }}"
            depth: 0
            do:
              - id: "research_step"
                type: "llm"
                parameters:
                  model: "gpt-4o"
                  prompt: "Research {{ topic }} at depth {{ depth }}"

              - id: "update_depth"
                type: "template"
                parameters:
                  template: '{"depth": {{ depth + 1 }}}'
```

---

## 常见问题

### Q: Map 节点如何访问外部变量？

A: 在子工作流中，除了 `{{ item }}` 外，还可以通过 `input_mapping` 传递额外参数。但当前实现主要关注 `item`。

### Q: While 循环可以提前退出吗？

A: 可以，通过在子工作流中将条件变量设为 false。

### Q: 循环节点的结果如何传递给后续节点？

A:
- **Map**: 通过 `{{ nodes.map_id.outputs.results }}` 访问结果数组
- **While**: 循环结束后，所有循环变量作为输出可用

### Q: 如何处理循环中的错误？

A: 当前版本中，任何错误都会终止循环。建议在子工作流中添加错误处理逻辑。

---

## 示例文件位置

- **Map Sequential**: `templates/map-example.yml`
- **Map Parallel**: `templates/map-parallel-example.yml`
- **While Basic**: `templates/while-example.yml`
- **While Advanced**: `templates/while-advanced-example.yml`

## 运行示例

```bash
# 运行 Map 示例
cargo run -- workflow run templates/map-example.yml

# 运行 While 示例
cargo run -- workflow run templates/while-example.yml
```

## 集成测试

查看完整的集成测试示例：
- `agentflow-cli/tests/workflow_tests.rs`
  - `test_parallel_map_workflow`
  - `test_stateful_while_loop_workflow`
