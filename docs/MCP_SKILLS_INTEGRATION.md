# MCP 与 Skills 集成及 Schema 校验强化架构设计

## 1. 背景与目标
在将 AgentFlow 推进为生产级后端网关的服务化进程中，底层的 `agentflow-skills` 模块需要能够处理更加复杂和健壮的外部工具集成。目前的系统：
1. 只能执行受限的本地内置工具（`file`, `shell`, `http`, `script`）。
2. `script` 工具执行缺乏大模型输入参数的 Schema 强校验，极易因为模型幻觉导致脚本崩溃。

**目标：**
通过引入 `agentflow-mcp` 客户端，使技能清单（`skill.toml` 和 `SKILL.md`）支持声明式挂载外部 MCP Server；同时为本地脚本注入 `parameters` Schema，实现网关侧的早期参数拦截和校验。

## 2. 数据结构设计 (Manifest Schema)

### 2.1 MCP Server 配置
在 `agentflow-skills/src/manifest.rs` 中增加：
```rust path=null start=null
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}
```
并在 `SkillManifest` 顶层增加 `#[serde(default)] pub mcp_servers: Vec<McpServerConfig>`。

### 2.2 SKILL.md 支持
对于标准的 `SKILL.md`，利用其现有的 `metadata` 字典，通过约定的 JSON 字符串形式（例如 `metadata.mcp_servers = "[{\"name\": \"github\", \"command\": \"npx\", ...}]"`）解析注入 MCP 服务。

### 2.3 工具参数校验 (Tool Schema)
修改 `manifest.rs` 中的 `ToolConfig`，为其增加：
```rust path=null start=null
#[serde(default)]
pub parameters: Option<serde_json::Value>, // 期望是一个 JSON Schema
```

## 3. Builder 实例化流程 (SkillBuilder)

在 `agentflow-skills/src/builder.rs` 的 `build()` 函数中：
1. 依然初始化原有的 `FileTool`, `ShellTool`, `HttpTool`, `ScriptTool`。
2. 遍历清单中的 `mcp_servers` 数组。
3. 对每个 Server，使用 `StdioTransport` 拉起其子进程，并实例化 `McpClient`。
4. 调用客户端的 `list_tools()` 获取远程工具列表。
5. 通过 `ToolRegistry` 注册远程工具的适配器包装类。

*(注：鉴于异步和所有权生命周期问题，可能需要将 `McpClient` 包装为 `Arc` 或在 Agent 执行上下文中保持。为简化第一版实现，我们在此仅添加解析模型并在本地 Tool 增加 Schema 校验)*

## 4. 脚本工具改造 (ScriptTool)

在 `agentflow-tools/src/builtin/script.rs` 中：
1. `ScriptTool::new` 将接收一个可选的 `parameters_schema` (即 `serde_json::Value`)。
2. 引入 `jsonschema` 库（如果不需要额外依赖，可先使用 `serde_json` 检查必需字段）。
3. 在 `Tool::execute` 执行之前：
   ```rust path=null start=null
   if let Some(schema) = &self.parameters_schema {
       // Validate JSON args against schema
       // If failed -> Return ToolOutput with validation error message immediately
   }
   ```

## 5. 错误处理策略 (No Unwrap Policy)
- 在解析、验证阶段遇到错误应返回 `SkillError` 或 `ToolError`。
- 不使用 `unwrap` 或 `expect`，保证系统遇到配置异常时能平滑失败，不会产生 Panic。
