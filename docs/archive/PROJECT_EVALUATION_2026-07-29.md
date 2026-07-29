# AgentFlow 项目深度评估报告 (2026-07-29)

- 评估日期：2026-07-29
- 评估范围：workspace 全部 **24 个 Rust crate**（22 个业务 crate + `agentflow-worker-proto` + `xtask`）+ 1 个 Web UI crate (`agentflow-ui`)，五个独立维度并行审计：架构分层、模块完整度与职责单一性、Agent 框架生态、服务层/部署/数据管理、安全性
- 方法：五个独立 agent 各自阅读代码 + 文档 + 运行 `cargo xtask check-arch`，互不共享结论，产出后交叉核对去重；不复述已在 `docs/archive/PROJECT_EVALUATION_2026-06-06.md`（上一版，HEAD `76a8814`）中记录并已修复的问题，只验证其是否仍然成立，并挖掘新问题
- 与上一版关系：上一版定稿于 2026-06-06，综合评级 A，记录了 2026-05-24 深度审计的 26 CRITICAL 全部修复。本版基于当前 `main` HEAD `3c00b0f`（S-track 沙箱加固收尾：Landlock、cgroup v2、`code_exec` ContainerBackend、clippy 全量清理）重新校核，覆盖两个月内的 P-A（契约内核抽取）与 S（沙箱加固）两条主线

---

## 0. TL;DR

| 维度 | 评级 | 一句话判断 |
| --- | --- | --- |
| 架构分层 | **B+** | 方向正确（IR/executor 分离、nodes/nodes-ai 拆分都已验证干净），但 `check-arch` 只激活了 8 条依赖律中的 3 条，13 条latent violation 里至少 2 条（`agentflow-tools` 未拆分成 contract-only crate、`agents/harness → tools` 的具体 builtin 耦合）是真实架构债，不是可以直接归入"窄依赖"豁免的情况 |
| 模块完整度 / 职责单一 | **B+** | 无 `todo!()`/`unimplemented!()` 残留，TODOs.md 诚实维护（0 open）；但发现三处"接到一半"的功能：生产运行时零成本熔断、节点级 retry/timeout 只有 `mcp` 类型支持、workflow YAML `inputs:` schema 块解析后完全未使用 |
| Agent 框架生态 | **B+** | 工具调用并行分发、审批协议、扩展点 SDK 都是行业级设计；但长期记忆（Preference/Entity-facts）未实现、agent 质量 eval 没有接入 CI 回归、dynamic workflow 审批默认关闭（LLM 生成的计划默认无监督执行） |
| 服务层 / 部署 / 数据 | **B** | Helm chart、Docker、备份工具都是真实存在且比预期完整；但当前二进制**尚未真正支持"5 个 worker 打一个 server"**（server 缺 gRPC listener flag）、worker↔server 通道明文无 TLS、只有 backup 没有 restore 命令 |
| 安全性 | **B+** | S-track 加固（Landlock/cgroup v2/ContainerBackend）扎实，抽查的既往 CRITICAL 全部仍然成立（无回归）；但发现一个新 **CRITICAL**：市场签名验证的真实 Ed25519 实现存在但从未在 CLI 安装路径接线，默认路径只做 SHA-256 校验和自证 |
| **综合** | **B+** | 项目核心命题在代码层面依然对齐；本轮审计的共同模式是**"接线在到达终点前一步停下"**——功能/安全机制本身写得扎实，但没有被默认路径实际调用 |

**最值得警惕的共性模式**：五个独立维度分别独立发现了同一类问题——**核心机制已经正确实现，却没有被生产默认路径真正调用**：
- 市场 Ed25519 签名验证器已实现，CLI 默认用的是校验和（安全维度，CRITICAL）
- 生产 ReAct/PlanExecute 运行时的 `CostLimitExceeded` 全链路类型已打通，实际只有 eval harness 会真正掐断（完整度维度，Major）
- `agentflow-worker` 的 JWT admission、TLS flag 都存在，PSK/明文 gRPC 仍是事实默认（服务层+安全维度，各自独立发现）
- Agent 质量 eval harness 功能完整，但没有一个 CI job 调用它（生态维度，Weak）

这不是"某个模块没写好"，而是一个跨越五个维度、反复出现的**交付习惯**：写完机制、写完测试，但没有把它设为无需用户主动选择的默认路径。建议下一轮修复把"把已实现的安全/质量机制设为默认"当作一个专项，而不是分散到各个 crate 里零散处理。

---

## 1. 架构分层审计

**总体判断**：分层方向正确，但强制程度不足。IR（`agentflow-graph`）与执行器（`agentflow-core`）的分离是真实且干净的——`agentflow-agents` 对 `agentflow-core` 唯一的依赖只在 `[dev-dependencies]`（测试用的 runner），生产代码路径零耦合。`agentflow-nodes` / `agentflow-nodes-ai` 的拆分同样验证干净：`agentflow-nodes` 里没有任何 `agentflow_llm`/`agentflow_rag` 泄漏。`cargo xtask check-arch` 当前报告 **24 个 workspace member、82 条内部依赖边、3 条激活的依赖律**（runtime-isolation / surface-isolation / kernel-isolation——第三条 kernel-isolation 是 2026-07-28 才新增的，比 `docs/ARCHITECTURE_DIAGRAM.md` 和 `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` 两份文档里写的"2/8"要新，属于文档轻微滞后），0 stale allowlist，13 条 latent violation（对应尚未激活的 5 条依赖律）。

### 排序发现

1. **[Major] `agentflow-tools` 从未真正拆分成 contract-only crate。** RFC (`docs/RFC_CRATE_ARCHITECTURE.md` §4) 原本计划把 `Tool`/`ToolRegistry`/`ToolMetadata` 契约单独拆出到 `agentflow-tool`，把 `ShellTool`/`FileTool`/`HttpTool`/`SandboxPolicy` 等具体实现留在 `agentflow-tools-builtin`。这个拆分没有发生。`agentflow-agents/src/tools/agent_tool.rs` 和 `agentflow-harness`（`approval_providers.rs`/`tasks.rs`/`hooks_runtime.rs`）都直接引用具体 builtin（如 `builtin::ShellTool`），而不是只依赖 trait。这正是 "law 2/4 runtime→impl" 系列 latent violation 存在的根本原因——不是窄 SPI 需求的正常残留，而是一个真正未拆分的胖 crate。
2. **[Major] `agentflow-agents → agentflow-memory` 是宽泛的具体类型耦合，不是 trait-only。** grep 显示 `react/agent.rs`、`plan_execute.rs`、`supervisor/*.rs`、`dynamic.rs`、`delegation.rs` 里对具体类型 `SessionMemory`/`TaskSummaryStore`/`ProjectMemoryStore` 的引用有 14 处，对 `MemoryStore` trait 本身的引用只有 1 处。`harness/src/runtime.rs` 同样形态。建议：要么把默认后端的注入挪到 surface 层（cli/server），要么把这条边永久标记为"内置默认后端"例外并写进文档，而不是继续列在"待归零"的 latent violation 清单里。
3. **[Minor] `docs/ARCHITECTURE_DIAGRAM.md`、`docs/ARCHITECTURE_EVALUATION_2026-06-20.md` 的 "N/8 条律" 文案滞后于实际 gate**（见上，已激活 3 条而非 2 条）。建议以后每次新增一条 law 时顺手同步这两份文档。
4. **[Minor] `harness → llm`/`tracing` 确实窄（分别只在 `runtime.rs` 做 tokenizer、`tracing_bridge.rs`/`params_summary.rs` 做 redaction/存储），但 `harness → tools` 不窄**——3 个文件里做具体 registry/builtin 装配，量级上不该和 `llm`/`tracing` 归入同一条"折叠进 store-spi/agent-spi"补救计划，需要单独的补救路径（即上面第 1 条的 `agentflow-tool` 拆分）。
5. **[Minor] `agentflow-cli`（约 19.4K LOC）审查后不算 god-crate**——`main.rs` 只有约 2.5K 行做路由，`commands/*` 下每个子目录都是薄薄的单命令文件；`agentflow-cli/src/lib.rs` re-export `agentflow_config::{config, executor}`，`agentflow-server/src/runs.rs` 直接调用同一个 `build_flow_from_yaml`，两边没有分叉出各自的工作流装配逻辑。唯一值得关注的点：`agent/replay.rs`（~896 行）、`harness/chat.rs`/`server_ops.rs`（800+ 行）体量已经不小，如果 server 未来需要等价能力，这部分逻辑可能需要下沉到共享 crate。
6. **[Nit] `agentflow-worker → agentflow-server` 确认是 `[dev-dependencies]`-only**（仅用于 gRPC round-trip + `DistributedDagScheduler` 集成测试），是合理的长期形态，但两边独立演进时容易被不小心提升为正式依赖，值得留意。

---

## 2. 模块完整度与职责单一性审计

**总体判断**：无 `todo!()`/`unimplemented!()` 残留于非测试/示例代码，是一个干净的信号；`TODOs.md` 诚实维护、0 open item。但存在三处"接线在到达终点前一步停下"的半成品功能，且都与 CLAUDE.md 的"production-ready"表述存在张力。

### 排序发现

1. **[Major] 生产 ReAct/PlanExecute 运行时没有任何成本熔断。** `agentflow-agent-spi/src/runtime.rs:367-369` 的注释直接写明："仅 eval runner 今天会 emit 这个；agent runtime 本身还不强制执行成本预算"。`RuntimeLimits`（`runtime.rs:24-33`）根本没有 `cost_limit_usd` 字段——只有 `max_steps`/`max_tool_calls`/`timeout_ms`/`token_budget`。`CostLimitExceeded` 唯一的构造点是 `agentflow-agents/src/eval/runner.rs:779-782` 里硬编码的测试值，`runner.rs:337-362` 的"强制执行"其实是跑完之后的事后重新打标，不是运行中途的真实截断。一个反复调用昂贵模型的生产循环没有任何成本熔断器。
2. **[Major] 节点级 `timeout_ms`/`max_retries` 目前只有 `mcp` 节点类型支持。** `agentflow-config/src/executor/factory.rs:269-284` 是唯一调用 `with_timeout_ms`/`with_max_retries` 的地方，这两个方法只存在于 `MCPNode`（`agentflow-nodes-ai/src/nodes/mcp.rs:83,89`）；`LlmNode`/`HttpNode`/`FileNode`/`TemplateNode` 都没有，`GraphNode`（`agentflow-graph/src/flow.rs:43-50`）本身也没有通用的 timeout/retry 字段。schema 校验（`schema.rs:417-422`）与实现一致地把这两个参数限定在 `"mcp"`，不是"文档说有、实现没跟上"的不一致，而是从设计上就只覆盖了 MCP——但恰恰 `http`/`llm` 是最容易抖动的节点类型，YAML 层面反而没有声明式的逐节点 retry/timeout。
3. **[Major] Workflow YAML 顶层 `inputs:` schema 块解析后完全未被使用（死功能）。** `agentflow-config/src/config/v2.rs:7-21`：`FlowDefinitionV2.inputs: HashMap<String, InputDefinitionV2>` 及 `InputDefinitionV2` 的每个字段（`description`/`required`/`default`）都标了 `#[allow(dead_code)]`。全仓库 grep 没有找到除该文件外任何对 `InputDefinitionV2`/`.inputs` 的引用。用户声明 `required: true`/`default: ...` 会被静默接受、解析、然后丢弃——没有校验、没有默认值填充。这是一个真正半成品的功能，不是测试里的占位符。
4. **[Minor] `agentflow-worker` 的节点 payload �covid 已经超过文档描述，反方向滞后。** CLAUDE.md/`RoadMap.md:271-276` 仍然写"仅支持 template/file/mock，llm/http/mcp/agent 由 P2.8 跟踪"，但 `agentflow-worker/src/lib.rs:762-773` 已经在 dispatch 全部四种，且有对应测试（`dispatch_llm_and_agent.rs`、`dispatch_simple.rs`）。`agent` payload dispatcher（`lib.rs:867-893`）明确注释为"minimal"——针对空 `ToolRegistry` 运行，真正的工具分发推迟到 P5.5。风险低（代码领先于文档），但值得刷新文档。
5. **[Minor] `MemorySummaryStrategy::Disabled` 确认是默认值**（`agentflow-agents/src/react/agent.rs:274`）。结合第 1 条，一个长时间无人值守的 agent 会话既没有成本上限也没有默认的历史压缩——token 成本/上下文窗口爆炸是默认配置下最现实的失败模式。
6. **[Minor] `agentflow-cli/src/main.rs`（2525 行）把分发逻辑和真实的分支/校验逻辑混在一起**，而不是纯粹委托给 `commands::*`——例如 `main.rs:1625-1730` 内联校验 `input.len() % 2`、本地/server 模式分支、文件 I/O，然后才调用 `workflow::server_ops::run_via_server`。没有坏，但模糊了 CLI 解析和业务逻辑的边界。
7. **[Nit] `agentflow-tools`（约 10.2K src 行）把 L0 `Tool` 契约和完整的 L2 层实现（OS 沙箱后端、`code_exec` 容器编排、policy/capability/security-profile 模块）捆在一起。** CLAUDE.md 把它列在 L0 时只提"`Tool` 契约"，但这个 crate 实际做的是契约 + 注册表 + 具体沙箱 + 容器生命周期。大概率是 RFC 里刻意为之且受 `check-arch` 门控，但作为一个号称"窄腰"的 crate，职责宽度值得记一笔。
8. **[Nit] 零星的前向声明但未接线的桩**：`agentflow-llm/src/providers/stepfun.rs:21`（`VoiceClone` 变体未参与路由）、`agentflow-rag/src/embeddings/onnx.rs:237`（TODO：真正的批处理，仅性能优化）。严重度很低，非正确性缺口。

---

## 3. Agent 框架生态审计

按业界成熟 agent 框架（LangChain/AutoGen/CrewAI/Claude Agent SDK 级别）的标准逐项评估：

| 能力项 | 评级 | 要点 |
| --- | --- | --- |
| 1. 核心循环模式（ReAct/Plan-Execute/Reflection） | **Adequate** | `ReActAgent` 成熟；`PlanExecuteAgent` 官方标注"first prototype"、无中途重规划（对比 `dynamic.rs::run_with_replan` 确实支持失败后重规划）；Reflection 是纯观察型，真正的纠错闭环由 `VerificationStrategy` 承担——这是合理的设计取舍，但 CLAUDE.md 的措辞低估了它（并非没有闭环机制，只是不叫"reflection"） |
| 2. 验证 / 护栏 | **Adequate** | `Rejected { feedback }` 会写回记忆并触发下一轮，`max_verification_attempts`（默认 2）耗尽后优雅降级；但 `AlwaysApprove` 是唯一内置实现，没有任何开箱即用的"真"验证器（LLM-judge/schema-checker/test-runner），每个团队都得自己写 |
| 3. 工具调用 | **Strong** | ≥2 个原生 tool_calls 自动批量分发（幂等并发/非幂等串行），policy/capability 决策步骤严格按 LLM 返回顺序落盘以保证 trace 确定性；4 个 `tool_choice_*` 一致性 + 9-provider 夜间 live CI 背书，是全框架里工程质量最扎实的一角 |
| 4. 记忆 | **Weak/Adequate** | 四层设计（Session/Semantic/Preference/EntityFacts）里只有前两层真正实现；**Preference 和 Entity-facts 显式标注"尚未实现，P4.7 待办"**；摘要策略是规则式的（`RecentOnly`/`Compact`），默认不接 LLM 摘要。这是相对成熟框架明显落后的一块——有 trait 设计，没有可持久化的长期记忆存储 |
| 5. 多智能体编排 | **Adequate/Strong** | Handoff/Blackboard/Debate 是真正可用的实现，非脚手架；`DelegationSpec`（`agentflow-agent-spi/src/delegation.rs`）支持基于 `ToolRegistry::narrowed` 的层级委派。缺口：没有 AutoGen 式的任意 agent-to-agent 图/GroupChat 拓扑，只有三个固定模式 + 委派模式 |
| 6. 动态工作流生成 | **Adequate** | `compile_plan_to_flow` 校验充分（重复 id、悬空依赖、必填字段），`run_with_replan` 能对失败步骤精准重规划；**但 CLI 的 `--approve` 默认值是 `"none"`——一个 LLM 生成、天然对抗性的计划默认无监督执行**，仅靠 `--allow-path`/`--allow-domain` 的沙箱兜底 |
| 7. Skill 打包与发现 | **Weak/Adequate** | `SKILL.md`/`skill.toml` 设计良好但完全自成一派，没有到 MCP manifest 或任何跨厂商标准的互操作桥接；Marketplace 有真实的 SHA-256 + 可插拔签名验证机制，但明确停在"验证后缓存"这一步——"不自动解包"是文档化的非目标，安装不是一条命令能完成的 |
| 8. Human-in-the-loop / 审批 | **Adequate** | 协议设计是生产级的（typed risk/scope、fail-closed 超时、生产 profile 自动升级非幂等调用），但 `agentflow-harness` crate 本身只自带 `AutoAllow`/`AutoDeny`/阻塞式 `Cli` 三个 provider——真正的异步/HTTP 远程审批 provider（`ServerApprovalProvider`）活在 `agentflow-server` 而非 harness crate 里，不算纯粹的"自己实现"，但 harness crate 本身对 CLI 之外场景不够开箱即用 |
| 9. Evals | **Weak** | `agentflow-agents/src/eval/` 是真实的评测框架（六种断言类型、USD 成本估算），但全仓库 `.github/workflows/*.yml` 里**没有一个 job 调用 agent-eval 数据集**——只有 RAG eval 接了 CI 回归门（`quality.yml::rag-eval-smoke`）。Agent 质量的评测存在，但不在回归安全网内 |
| 10. 扩展性 / SDK | **Strong** | `docs/AGENT_SDK.md` 六个 trait 都有契约/坑点/可运行示例，closed-enum 策略明确（`AgentStepKind`/`AgentStopReason`/`ReflectionTrigger` 封闭，扩展靠包装而非加变体），`cargo doc --no-deps` 零警告是硬性 CI gate。对 Rust agent 框架而言这个纪律程度不常见，第三方今天就能真正基于它构建 |

### Top 5 生态缺口（按影响排序）

1. **Agent eval 没有 CI 门**——ReAct/verification/多智能体质量的回归可以静默发生，而 RAG 有真实的回归门。建议照抄 `rag-eval-smoke` 的 baseline-compare 模式加一个 `agent-eval-smoke` job。
2. **长期记忆不完整**——Preference / Entity-facts 层设计完成但未实现（P4.7）。建议在宣称"记忆能力对标"之前先补一个最小可用的 SQLite `PreferenceStore`/`EntityFactStore`。
3. **没有实时成本治理**——`pricing.rs` 只给 eval 跑分定价；`RuntimeLimits.token_budget` 限制 token 但没有任何机制在生产环境对 Debate/Handoff 扇出的真实 USD 花费设上限。建议把 `ModelPricing` 接入 `AgentContext`/`RuntimeLimits`，做成可强制执行的运行时护栏而非离线 eval 指标。
4. **动态工作流审批默认关闭**——计划是 LLM 生成、天然对抗性的，但 `--approve` 默认 `"none"`。建议非 `dev` profile 下默认要求审批（或至少默认 `--dry-run`），与 Harness 生产 profile 的自动升级逻辑保持一致。
5. **Skill 格式没有外部互操作性**——自成一派的 `SKILL.md`/`skill.toml`，没有到任何标准的映射路径。建议定义一条导出路径（例如 Skill → MCP server manifest），让 Skill 里的工具能在 AgentFlow 之外被消费。

---

## 4. 服务层 / 部署 / 数据管理审计

| 子领域 | 成熟度 | 关键发现 |
| --- | --- | --- |
| Server/API | **Beta** | 租户边界在 Q1.4 之后确实持续生效（跨租户探测返回 404，`?tenant_id=` query 覆盖已移除）；但租户身份本身只是客户端可控的 `X-Agentflow-Tenant` header（`tenant.rs:47-61`），配合服务器全局单一 bearer token——任何持有该 token 的调用方都可以自称任意租户。代码注释里自己承认"等 JWT/OIDC 落地后…"。数据库层没有任何 Postgres RLS，隔离完全是应用层 `WHERE tenant_id` 过滤，未来任何遗漏该子句的新路由都会静默泄露跨租户数据 |
| Database | **Beta** | 6 个正向迁移全部幂等（`IF NOT EXISTS`）；连接池针对云负载均衡场景有周到的默认值（3s acquire timeout + max-lifetime 回收，专门针对 Neon/RDS 这类 LB 后面出现死连接的场景）；`cleanup_expired` 真实存在、有测试、支持 per-run 覆盖和 dry-run，但只通过 CLI/后台循环触发，需要确认 `agentflow serve` 是否默认启动该循环 |
| Worker / 分布式 | **Beta，部署形态尚未完整** | 节点 payload 覆盖已经超出 CLAUDE.md 里"仅 template/file"的滞后描述（llm/http/mcp/agent 均已 dispatch，P2.8 已闭环）；worker 准入同时支持 PSK（默认）与 JWT（P10.16.1，同样是 opt-in），但**没有任何机制强制走 JWT**；**gRPC 通道没有接线 TLS/mTLS**——`agentflow-worker/src/main.rs:57-63` 接受 `--server-ca`/`--client-cert`/`--client-key` 参数但明确没有使用；**`agentflow-server` 二进制目前没有 `--worker-grpc` 监听 flag**——`docs/DISTRIBUTED.md` 自己承认"server 二进制仍需要一步 CLI flag/listener 接线才能成为完整的最终用户命令"。换句话说，"5 个 worker 打一个 server"这个部署形态今天还不能直接跑起来 |
| Tracing / 可观测性 | **Beta** | JSONL 默认 + Postgres/SQLite 功能门控都工作正常，OTel span 模型能拼接分布式 worker trace；首方 OTLP exporter 的缺失（Q2.3.3）是真实的采用门槛——今天任何想直接接 Jaeger/Tempo/Honeycomb 的人都得自己手写一个 `OtelSpanSink`，没有开箱即用的 `opentelemetry-otlp` 配置项 |
| UI | **Alpha**（与 CLAUDE.md 自我定位一致） | 纯 `/v1/*` 薄客户端：没有 RBAC，没有租户登录态——租户切换只是客户端偏好项，发送的是服务端对任何人都信任的同一个 header；没有告警面 |
| 部署 / 运维 | **Beta，好于预期** | 真实存在 `Dockerfile`（多阶段、非 root 用户）+ `docker-compose.yml` + 真正的 Helm chart（`charts/agentflow/`，含 deployment/service/serviceaccount/secret 模板、非 root securityContext、`readOnlyRootFilesystem`、liveness/readiness probe），已经取代 `docs/KUBERNETES_DEPLOYMENT.md` 里过时的手写参考 YAML（该文档顶部已标注为 pre-Helm 参考）。缺口：`values.yaml` 的 `resources: {}` 留空（无默认 CPU/内存 request/limit）、**没有 HPA 模板**、`replicaCount: 1` 且无 PodDisruptionBudget、没有 Postgres 本身的多可用区/故障转移 runbook（全程单实例假设） |
| 数据生命周期 | **备份是真的，恢复不是** | `agentflow backup`（884 行）确实调用 `pg_dump --format=custom` 并打包 run_dir/trace_dir/marketplace cache 成一个带 manifest 的归档，支持 dry-run 和 doctor 集成；**但完全没有 `agentflow restore` 命令**——`docs/SERVER_BACKUP_RESTORE.md` 明确写"这个 manifest 是未来 restore 命令要消费的契约"，今天让运维手动跑 `pg_restore`/`tar -xzf`。没有任何自动化恢复测试（只有 `doctor --backup-check` 测目录可写性） |

### 如果明天要把这套部署到生产，Top 5 风险

1. **跨租户伪装**：单一共享 bearer token + 客户端自报的租户 header，任何认证过的调用方都能读写任意租户数据；没有 RLS 兜底。
2. **没有恢复工具**：备份存在，但恢复是手动、未经端到端测试的多步 runbook——真正出事故时才是第一次真正跑一遍。
3. **分布式 worker 走明文 gRPC**：TLS/mTLS 的 flag 存在但未接线，PSK/JWT 凭据和任务数据都是明文过网。
4. **两节点部署形态目前跑不起来**：server 二进制还缺 `docs/DISTRIBUTED.md` 自己点名的 gRPC listener flag。
5. **Helm chart 默认空资源限制、无 HPA**：默认 `helm install` 得到的是无资源上限、不能自动扩缩的 pod，在共享集群上容易成为吵闹邻居。

---

## 5. 安全性审计

**总体判断**：S-track 加固周期（Landlock、cgroup v2、`code_exec` 的 ContainerBackend、os_sandbox 默认开启）是扎实的工程——Linux seccomp+Landlock 分层、`code_exec` 强制隔离层、SSRF 感知的 `HttpTool`（真正 DNS 解析后检查地址类别，不只是检查 hostname 字符串）、常量时间的鉴权比较，背后都有针对棘手细节的真实实证验证（root 的 `RLIMIT_NPROC` 豁免、Apple `container` CLI 未文档化的 `--network none` 行为、client 端与容器端的 kill 语义差异）。

### 排序发现

1. **[CRITICAL] Marketplace 签名验证默认没有接到真实签名上。** CLI 安装路径（`agentflow-cli/src/commands/marketplace.rs:275` → `agentflow-skills/src/remote_marketplace.rs:253-259`）默认构造的是 `ChecksumSha256SignatureVerifier`——只是对制品重新哈希，和一个本身只是校验和（不是对攻击者不可控内容的真实密码学签名）的"signature"字段比对。真正的 `Ed25519SignatureVerifier`（`remote_marketplace.rs:458`，针对密钥目录的真实分离签名验证）完整实现且存在，但**只在测试里被引用**——没有任何 CLI 调用点通过 `with_client_and_verifier` 构造它。一个被攻破/中间人的注册表可以提供任意载荷、自证一个自洽的 SHA-256"签名"，CLI 会打印 `signature_checked: true` 给出错误的安全感，同时运行了未经验证的第三方代码。
2. **[MAJOR] Worker gRPC 准入没有 fail-closed 默认，和 HTTP 鉴权不对称。** `agentflow-server/src/scheduler/admission.rs:134-167` 的 `WorkerAdmissionPolicy::default()`/`::open()` 允许任何无凭据 worker 接入——这本身是合理的 dev 默认值，但没有类似 `require_api_token`（`auth.rs:60`）那种由 `SecurityProfile` 驱动的检查，在 `SecurityProfile::Production` 下如果 `allowed_workers`/`pre_shared_keys`/`jwt` 全部未设置就让启动失败。当前发行的 `agentflow-server` 二进制根本没有接线 gRPC 控制面（见 §4），所以眼下不可利用，但对任何直接实例化 `AuthenticatedControlPlane` 的操作者或未来集成而言，这个类型的 `Default` 是静默开放的活雷区。
3. **[MINOR] 每次 spawn 的叶子 cgroup 从不清理。** `agentflow-tools/src/sandbox/linux.rs:474-517`，文档化为可接受的资源记账成本，不是即时威胁——但在长期运行、执行大量沙箱调用的主机上会造成无界的目录增长（数周尺度的 inode/cgroup 计数压力），从"成本"变成缓慢的可用性问题。
4. **[MINOR] ContainerBackend 没有加固容器自身的根文件系统。** `agentflow-tools/src/sandbox/container.rs:237-296`，两种引擎都没有传 `--read-only` 根文件系统 flag，LLM 生成的代码可以在 `/workspace` 之外的容器内部任意写入。由于 `--rm` + 零网络 + 除工作目录外无 bind mount，实际影响有限，但既然这个工具的输入天然是对抗性的，这层加固成本很低、值得补上。
5. **[NIT] 动态工作流计划里 LLM 可控的是 Tera 模板源本身，而不只是渲染上下文。** `agentflow-nodes/src/nodes/template.rs`，`register_custom_functions`（`tera_helpers.rs:85`）只暴露 `now`/`uuid`，没有文件/网络/exec 原语，所以这是 DoS 级别（病态循环/递归 include 烧 CPU）而非 RCE。如果节点自身的 timeout 没有覆盖这个场景，值得加一个渲染时间上限。

### 既往 CRITICAL 抽查结果（全部仍然成立，无回归）

- `SandboxPolicy` 路径/命令空集合=拒绝所有（`sandbox/policy.rs:112-137`，Q1.2.1）：确认完好；domains 空集合=允许所有的不对称设计仍然是有意为之且文档化的，不是回归。
- `ShellTool` argv-vs-shell 默认（`builtin/shell.rs`）：确认 `ShellInterpretation::Argv` 仍是默认，`parse_argv_safe` 仍拒绝 `;`/`&&`/反引号/`$()`/管道；shell 元字符解释需要显式 opt-in。
- Google API key 拼进 URL（Q1.8.1）：确认已修复，且 grep 确认其余 provider（OpenAI/Anthropic/Moonshot/StepFun）都没有类似问题，一律走 `Authorization: Bearer`。
- Bearer token / worker-PSK 常量时间比较（Q3.4.2）：`server/src/auth.rs:119` 与 `scheduler/admission.rs:49` 都实现了相同的先比长度再异或的常量时间比较器，PSK 循环刻意不在命中时提前退出——是真正做对了，不只是"存在"。
- cgroup v2 delegation-verification 修复（3c00b0f，HEAD）：这是真正的修复，不是给 CI flakiness 打补丁。`cgroup_v2_delegation_available()` 现在 fork 一个一次性子进程真实尝试 `cgroup.procs` 迁移并报告真实的内核权限结果（同时处理了"共同祖先目录必须可写"这个在 GH Actions runner 上才会暴露的细节），而不只是检查 delegation 目录是否存在。

### Top 3 优先行动

1. 把 `Ed25519SignatureVerifier`（`require_signature=true`）接为 CLI 安装/验证路径的默认签名验证器——这是唯一的 CRITICAL，且是一处外科手术式的小改动。
2. 在生产 gRPC worker 控制面真正接线之前，给 `WorkerAdmissionPolicy` 加一个 `SecurityProfile` 门控的 fail-closed 检查（镜像 `require_api_token` 的行为）。
3. 给 Linux 沙箱后端加一个 cgroup 叶子回收器（定时任务或 drop 时尽力 `rmdir`），堵上缓慢的资源耗尽缺口。

---

## 6. 跨维度综合优先级清单

按"影响面 × 修复成本"排序，供下一轮 remediation wave 参考：

| 优先级 | 发现 | 来源维度 | 修复成本 |
| --- | --- | --- | --- |
| P0 | Marketplace 签名验证默认未启用真实 Ed25519 校验 | 安全 | 低（接线已存在的实现） |
| P0 | 跨租户伪装（共享 token + 客户端自报 header，无 RLS 兜底） | 服务层 | 中（需要真实身份认证，非一次性补丁） |
| P1 | 生产 agent 运行时无成本熔断（`RuntimeLimits` 缺 `cost_limit_usd`） | 完整度 + 生态 | 中 |
| P1 | Worker↔Server gRPC 明文无 TLS，且当前二进制未接线监听 flag | 服务层 + 安全 | 中 |
| P1 | 动态工作流 `--approve` 默认 `none`（LLM 计划默认无监督执行） | 生态 | 低（改默认值 + 文档） |
| P2 | `agentflow-tools` 未按 RFC 拆分为 contract-only crate | 架构 | 高（真实的 crate 拆分工程） |
| P2 | Agent 质量 eval 未接入 CI 回归门 | 生态 | 低（照抄 rag-eval-smoke 模式） |
| P2 | 只有 `backup` 没有 `restore` 命令 | 服务层 | 中 |
| P3 | 节点级 retry/timeout 只有 `mcp` 类型支持 | 完整度 | 中 |
| P3 | Workflow YAML `inputs:` schema 块解析后未使用（死功能） | 完整度 | 低（要么实现校验/默认值，要么删除字段） |
| P3 | 长期记忆 Preference/Entity-facts 层未实现（P4.7） | 生态 | 高 |
| P4 | 无首方 OTLP exporter | 服务层 | 中（可延续现状，由运维自带 sink） |
| P4 | Helm chart 空资源限制、无 HPA | 服务层 | 低 |

**综合评级：B+**（上一版 2026-06-06 为 A）。评级下调不代表项目退步——期间的 S-track 加固和 P-A 架构抽取都是真实且高质量的进展；下调反映的是本轮五维度独立审计比历次评估更深入地追问"默认路径是否真的调用了已实现的机制"，从而挖出了一批此前评估口径下不会浮现的"实现完整但未接线为默认"问题。这类问题的共同修复模式很简单：**审查每一个"高质量实现存在但只在测试/非默认路径可达"的机制，把它扶正为默认行为**，而不是继续新增功能。
