# AgentFlow 架构设计

**最后更新**: 2025-11-22  
**版本**: v0.2.0  
**状态**: Phase 0 & Phase 1.5 完成

## 🎯 设计原则

1. **高内聚，低耦合** - 每个 crate 职责清晰，依赖最小
2. **保持核心纯粹** - core 只包含工作流编排核心，不包含工具
3. **简单实用优先** - 优先简单方案（如 .env），避免过度设计

---

## 📦 Crate 职责划分

### **agentflow-core** - 工作流编排核心 ⭐

**只包含工作流执行引擎的核心功能**

```
✅ 核心抽象
- node.rs, async_node.rs  // 节点抽象
- flow.rs, value.rs        // 工作流定义

✅ 执行引擎  
- concurrency.rs, retry.rs // 并发、重试
- timeout.rs               // 超时控制

✅ 可靠性
- checkpoint.rs            // 检查点恢复
- resource_manager.rs      // 资源管理

✅ 可观测性
- metrics.rs, logging.rs   // 指标、日志
- health.rs                // 健康检查

✅ 错误处理
- error.rs                 // 错误类型
```

**不包含** (保持纯粹):
- ❌ 具体节点实现 → `agentflow-nodes`
- ❌ LLM 集成 → `agentflow-llm`
- ❌ 凭证管理 → `agentflow-llm`
- ❌ 通用工具 → 未来的 `agentflow-utils`

---

### **agentflow-llm** - LLM 提供商集成

```
✅ 提供商 API
- providers/ (openai, anthropic, google, moonshot, stepfun)

✅ 配置管理
- config/model_config.rs   // 模型配置
- registry/                // 模型注册表

✅ 凭证管理
- 使用 ~/.agentflow/.env 文件（简单实用）
```

---

### **agentflow-nodes** - 具体节点实现

```
✅ 16+ 内置节点
- llm.rs, http.rs, file.rs, template.rs
- mcp.rs, rag.rs, ...
```

---

### **其他 Crates**

- **agentflow-mcp**: MCP 协议客户端 ✅ (生产就绪)
- **agentflow-rag**: RAG 系统 ✅ (98% 完成)
- **agentflow-cli**: 命令行工具
- **agentflow-agents**: 预构建智能体

---

## 🔑 API Key 管理（当前方案）

### 使用 `~/.agentflow/.env` 文件

**适用场景**: CLI 工具、本地开发

```bash
# 1. 初始化
agentflow config init

# 2. 编辑配置
vim ~/.agentflow/.env
```

```bash
# ~/.agentflow/.env
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
GOOGLE_API_KEY=AIza...
```

```bash
# 3. 直接使用
agentflow prompt "Hello" --model gpt-4o
```

**安全建议**:
```bash
# .gitignore 必须包含
.env
.agentflow/

# 设置文件权限
chmod 600 ~/.agentflow/.env
```

---

## 🧪 测试覆盖（100% 通过）

- **总计**: 479 测试
  - agentflow-core: 107 单元 + 48 集成 ✅
  - agentflow-llm: 49 单元 ✅
  - agentflow-mcp: 117 单元 + 45 集成 ✅
  - agentflow-rag: 83 单元 ✅
  - agentflow-nodes: 25 单元 ✅  
  - agentflow-cli: 5 集成 ✅

---

## 🚀 性能指标（全部达标）

| 指标 | 目标 | 实际 |
|------|------|------|
| 单节点执行 | < 100ms | ✅ 达标 |
| 超时控制开销 | < 1μs | ✅ 244ns |
| 健康检查 | < 10ms | ✅ <4μs |

---

## 📖 相关文档

- `CLAUDE.md` - 项目配置和开发指南
- `TODO.md` - 开发路线图
- `docs/phase0/` - 错误处理审计报告
- `docs/HEALTH_CHECKS.md` - 健康检查指南
- `docs/CHECKPOINT_RECOVERY.md` - 检查点恢复

---

**维护者**: AgentFlow Core Team
