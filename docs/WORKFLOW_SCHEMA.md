# Workflow Schema

本文档描述 `agentflow workflow validate` 当前执行的 CLI workflow schema 校验规则。

## 校验入口

```bash
agentflow workflow validate path/to/workflow.yml
agentflow workflow validate path/to/workflow.yml --format json
agentflow workflow validate path/to/workflow.yml --strict
```

- 默认模式下，未知参数作为 warning 输出，用于兼容已有 YAML。
- `--strict` 会把未知参数升级为 error，适合 CI 或发布前检查。
- `--format json` 输出 `workflow`、`valid`、`issues`、`warnings`，供脚本和 server 复用。
- `workflow run` 和 `workflow run --dry-run` 会在构建 graph 前执行同一套 schema validation。
- `workflow debug --validate` 会复用同一套 schema validation，并叠加依赖和结构分析。
- `run_if` 与 `while.parameters.condition` 使用统一表达式语言；参考
  `docs/EXPRESSION_LANGUAGE.md`。`--strict` 会编译这些表达式并报告列号。

## 通用规则

- `nodes` 至少包含一个节点。
- 每个 node `id` 必须非空，并且在 workflow 内唯一。
- `dependencies` 必须引用已存在的 node id。
- `input_mapping` 支持 `{{ nodes.<id>.outputs.<field> }}` 形式，并校验 `<id>` 是否存在。
- 标记为 input-compatible 的 required 参数可以通过 `parameters` 或 `input_mapping` 满足。
- `mcp` 和 `rag` 节点需要对应 crate feature；未启用时会输出明确 feature gate 错误。

## 节点参数

| Node type | Required | Input-compatible required | Optional |
| --- | --- | --- | --- |
| `llm` | - | `prompt` | `model`, `system`, `temperature`, `max_tokens` |
| `skill_agent`, `agent` | - | `skill`, `message` | `model` |
| `http` | - | `url` | `method`, `headers`, `body` |
| `file` | - | `operation`, `path` | `content` |
| `template` | `template` | - | `output_key`, `output_format` |
| `arxiv` | `url` | - | `fetch_source`, `simplify_latex` |
| `asr` | `model` | `audio_source` | - |
| `image_edit` | `model` | `prompt`, `image_source` | - |
| `image_to_image` | `model` | `prompt`, `source_image` | - |
| `image_understand` | `model` | `text_prompt`, `image_source` | - |
| `markmap` | - | - | `markdown`, `save_to_file` |
| `text_to_image` | `model` | `prompt` | - |
| `tts` | `model`, `voice` | `input_template` | - |
| `map` | `template` | - | `parallel` |
| `while` | `condition`, `max_iterations`, `do` | - | - |
| `mcp` | `server_command`, `tool_name` | - | `tool_params`, `timeout_ms`, `max_retries` |
| `rag` | `operation`, `collection` | - | `qdrant_url`, `embedding_model`, `query`, `documents`, `top_k`, `search_type`, `alpha`, `rerank`, `lambda`, `vector_size`, `distance` |

## 参数类型

- `String`: YAML string。
- `Number`: integer 或 float。
- `Integer`: YAML integer。
- `Bool`: YAML boolean。
- `Object`: YAML map。
- `Sequence`: YAML list。
- `SequenceOfStrings`: 所有元素均为 string 的 YAML list。
- `Any`: 不限制类型。

## 嵌套节点

- `map.parameters.template` 必须是 workflow node definition 列表。
- `while.parameters.do` 必须是 workflow node definition 列表。
- 嵌套节点复用普通节点的 required 参数、类型和 unknown parameter 校验规则。

## 条件表达式

`run_if` 和 `while.parameters.condition` 支持布尔、比较、算术和函数调用:

```yaml
run_if: "len(nodes.search.outputs.items) > 0 && nodes.classify.outputs.score > 0.7"

parameters:
  condition: "{{ count < 3 }}"
```

支持的路径包括 `nodes.X.outputs.Y`、`inputs.Z`、数组索引
`nodes.X.outputs.items.0`，以及 while loop 输入的简写形式 `count`。
