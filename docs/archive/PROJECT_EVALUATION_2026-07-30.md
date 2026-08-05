# AgentFlow 项目深度评估报告 (2026-07-30)

- 评估日期：2026-07-30
- 评估范围：workspace 全部 **25 个 Rust crate**（含新拆分的 `agentflow-tool`）+ 1 个 Web UI crate，五个维度并行审计：架构分层、模块完整度与职责单一性、Agent 框架/工程化理念、服务层/部署/数据管理、安全性
- 方法：五个独立 agent 各自读代码 + 跑测试 + 跑 `cargo xtask check-arch`/`helm lint`/相关 `cargo test`，互不共享结论；以 `docs/archive/PROJECT_EVALUATION_2026-07-29.md`（HEAD `86a1211`）逐条发现为基线，验证是否闭环、是否恶化，并独立挖掘新问题
- 与上一版关系：上一版定稿于 2026-07-29，综合评级 B+，给出 P0-P4 优先级清单。本版基于当前 `main` HEAD `81cda0f` 复核，覆盖 `86a1211..HEAD` 之间的 **14 个提交**（比最初预估的 5 个多得多——其中 `08c86d0`/`e09ec70`/`efe292b`/`8c4e057`/`2219276`/`6bd8cf9`/`7ab0e99`/`8181c4d`/`49fd76e` 均已直接命中昨天清单的具体条目，`b06ce03`/`dfc0e57`/`c1291da`/`9babf10`/`81cda0f` 是最初已知的 5 个）

---

## 0. TL;DR

| 维度 | 昨天 (86a1211) | 今天 (81cda0f) | 一句话判断 |
| --- | --- | --- | --- |
| 架构分层 | B+ | **A-** | 昨天唯一的 [Major]（`agentflow-tools` 契约/实现未拆分）已彻底、干净地解决，有编译验证+专门回归测试锁定；但复核发现一个更值得优先处理的同构债务：`agents`/`harness → agentflow-memory` 明明有现成的 `agentflow-store-spi` 契约 crate 可用，却没有顺手切过去 |
| 模块完整度 / 职责单一 | B+ | **A-** | 三个 [Major]（无成本熔断、node 级 timeout/retry 仅 mcp、workflow inputs 死功能）全部实证闭环，测试覆盖扎实；但成本熔断机制虽已落地，**没有任何 CLI/Server 入口能配置它**——机制存在但操作员摸不到 |
| Agent 框架生态 | B+ | **A-** | Agent eval CI 门、实时成本治理、动态工作流审批默认值三项昨天点名的缺口均已关闭；但记忆能力需要重新拆分评价——**存储层（Preference/EntityFacts）扎实完整，运行时集成为零**，产品可见行为上和"没做"一样；`harness run` 存在与 `workflow dynamic` 同构但未被覆盖的审批默认值口子 |
| 服务层 / 部署 / 数据 | B | **B+** | Worker↔Server gRPC TLS/mTLS 真正接线并通过端到端测试、`agentflow restore` 命令落地、Helm 资源限制+HPA 都是实打实的进展；但跨租户伪装（客户端自报 header，无 JWT/RLS）完全未动，且新发现 **Helm/docker-compose 默认落在 `SecurityProfile::Local`**——刚补上的 fail-closed 机制在标准部署路径里其实是休眠的 |
| 安全性 | B+ | **B+（不变）** | 昨天 5 条发现（市场签名、worker 准入、cgroup 泄漏、容器只读、Tera DoS）全部正确闭环，且 `agentflow-tool` 拆分逐文件核对无安全语义漂移；但同一窗口新增一个 **CRITICAL**：`agentflow restore` 在检查"不能删到文件系统根目录"的安全护栏**之前**就先执行了 `remove_dir_all`，护栏形同虚设 |
| **综合** | **B+** | **A-**（有条件） | 过去 24 小时是一次真正的"闭环冲刺"：昨天清单里 9/13 条已验证解决。但新引入的 restore 顺序 bug 和 Helm 默认安全档位问题说明同一种交付习惯仍在复发——**高速交付新功能时，安全护栏检查顺序/部署默认值这类"最后一公里"细节仍会被跳过**。这次不是"机制没接线"，而是"机制接线了，但接线本身有 bug 或默认值不对" |

**本轮最值得记录的模式**：昨天报告总结的"接线在到达终点前一步停下"问题，这一轮被系统性地修了一大半——这是真实进展，不是文档粉饰。但修复这些问题的新代码本身又带来了两个新问题（restore 顺序 bug、Helm 默认档位），说明团队在"把已有机制接成默认路径"这件事上进步很快，但"新写的接线代码本身的安全审查"还没有跟上同样的节奏。建议下一轮把"新增的 fail-closed/破坏性操作代码路径，是否有独立安全审查"作为专项检查项，而不仅仅是复核旧清单。

---

## 1. 架构分层复核

### 昨天 [Major] 1：`agentflow-tools` 从未真正拆分成 contract-only crate → **已修复，验证扎实**

`b06ce03` 把 `Tool`/`ToolRegistry`/`ToolMetadata`/`Capability`/`ToolPolicy`/`SecurityProfile`/`SandboxBackend` 契约面整体搬到新的、真正 dependency-free 的 `agentflow-tool` crate（`agentflow-tool/Cargo.toml` 只有外部依赖）。`agentflow-tools` 现在 `pub use agentflow_tool::*` 完整 re-export 契约面 + 保留具体实现。`agentflow-agents`/`agentflow-harness`/`agentflow-agent-spi` 的生产代码 `agentflow_tools::` 引用清零（残留引用全部落在 `#[cfg(test)]`/doc 注释内），`agentflow-tools` 在这三个 crate 里降级为 dev-dependency。

验证证据：
- `cargo check -p agentflow-tool -p agentflow-tools -p agentflow-agents -p agentflow-harness -p agentflow-agent-spi --all-features` 全部通过。
- `xtask` 新增回归测试 `tool_contract_split_removed_the_tools_kernel_membership_and_latent_edges` 锁定这两条 latent edge 必须消失，`cargo test -p xtask` 通过。
- `cargo run -p xtask -- check-arch`：latent violation 从 13 条降到 **11 条**，精确对应 `agents->tools`/`harness->tools` 两行的移除，无静默增减。
- `docs/RFC_TOOL_CONTRACT_SPLIT.md` 声称的迁移范围与实际 diff 逐项吻合。

**这是一次教科书级别、闭环彻底的拆分。**

### 复核发现（比昨天报告更深入）：[Major] `agents`/`harness → agentflow-memory` 是与刚解决的 tools 问题同构、但修复门槛更低的未偿债务

`agentflow-store-spi` 早在 P-A1.2 就已经从 `agentflow-memory` 中拆出了 `MemoryStore`/`TaskSummaryStore`/`Message` 纯契约，`agentflow-agent-spi`（kernel crate）已经正确依赖它。但 `agentflow-agents`、`agentflow-harness` 两个运行时的 `[dependencies]` 仍然指向 impl-bundling 的 `agentflow-memory`，而不是已经就位的契约 crate。`b06ce03` 完全没碰这条边。

重新精确测量耦合面（修正昨天"14 处具体类型耦合、仅 1 处 trait"的表述，该表述与当前代码不符）：
- `agentflow-harness` 生产代码**只有 1 处引用，且是 trait**：`runtime.rs:846` `&dyn agentflow_memory::MemoryStore`。所有具体类型引用都在测试块内。
- `agentflow-agents` 生产代码主结构体字段均为 `Box<dyn MemoryStore>`/`Arc<dyn TaskSummaryStore>`/`Arc<dyn ProjectMemoryStore>`（trait object），仅 `dynamic.rs`（第 23/315 行）、`supervisor/mod.rs`（第 27/167 行）两处用具体类型 `SessionMemory::default_window()` 提供默认值——是合理的"给 trait object 一个默认具体实现"模式，不是大范围耦合。

**结论**：latent edge 真实存在，严重程度应上调不是因为耦合面大，而是因为修复它比昨天类比的 tools 问题成本更低——目标契约 crate 已经造好，只需要把两个运行时 crate 的依赖指向和少量具体类型引用换掉，量级远小于这次刚完成的 tools 拆分工程。

### 其他复核

- `docs/ARCHITECTURE.md` 的一行改动准确（`tools` → `tool`），未引入新问题。
- **[Minor 新增]** `docs/ARCHITECTURE_DIAGRAM.md`（第 70/93/108 行）和被 CLAUDE.md/xtask 主动引用为权威参考的 `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`（第 56/84/85 行）拆分后均未同步，仍把 `agentflow-tools` 列为契约归属 crate、`agents/harness -> tools` 列为待办 latent edge。因为后者被 `check-arch` 输出主动引用为 "see docs/ARCHITECTURE_EVALUATION_2026-06-20.md §2"，滞后有实际误导性。
- `agentflow-config/src/executor/plugin.rs` 的改动纯粹是 import 路径重排，未引入新跨层依赖。

**架构分层评级：A-**（昨天 B+）

---

## 2. 模块完整度与职责单一性复核

### 三个 [Major] 全部实证闭环

**1. 生产运行时无成本熔断 → 已修复**（由未在简报中点名的 `8c4e057` 修复，早于本轮明确指名的两个提交）：`RuntimeLimits.cost_limit_usd: Option<f64>`（`agentflow-agent-spi/src/runtime.rs:42`）已接线；`ReActAgent`（`react/agent.rs:2008-2036`）、`PlanExecuteAgent`（`plan_execute.rs:327-340,560-575`）每轮/规划后真实检查累计成本，超限返回真正的 `AgentStopReason::CostLimitExceeded`，不再只是 eval harness 事后补记。5 个单测全过。

**2. node 级 timeout/max_retries 仅 mcp 支持 → 已修复**（`c1291da`）：没有给 `GraphNode` 加字段（commit message 解释了原因：~120 处裸结构体字面量调用点），而是在 `agentflow-config/src/executor/factory.rs:325-347` 新增 `TimeoutRetryNode` 装饰器，统一包住 llm/http/file/template/agent/skill_agent/batch/conditional/arxiv/markmap 等所有非 map/while 节点。`timeout_ms` 用 `tokio::time::timeout`；`max_retries` 复用 `RetryPolicy::default()` 的错误分类（只重试 network/timeout/rate-limit，非瞬时错误如 `ValidationError` 不重试，有专门测试验证），这个设计让它能安全地套在有副作用的节点（如 `HttpNode` POST）上。map/while 在 schema 校验和 factory 双重拒绝这两个字段。2 条端到端 CLI 集成测试全过。

**3. workflow inputs required/default 死功能 → 已修复**（`dfc0e57`）：`apply_declared_inputs`（`agentflow-config/src/executor/mod.rs:66-90`）逻辑完整——caller 提供优先、否则填 default、否则 required 报错命名字段。接线到 `workflow run`（dry-run 和真实执行前）和 server 的 `flow_execute`。3 条端到端 CLI 测试断言真实输出内容（不只是走到分支），全过。

### 修复带来的新问题

**[Minor 新增]** 成本熔断机制已在运行时层完整落地并测试完备，但**没有任何 CLI flag 或 Server API 字段可以设置 `cost_limit_usd`**——`agentflow harness run`/`chat`、`POST /v1/harness/sessions` 均无对应入口，只有直接嵌入 Rust API（`.with_cost_limit_usd(...)`）才能用上。对真实操作员入口，默认仍是零成本保护——相当于"安全带焊死在座位下面，够不着"。

### 其余发现

- [Minor] `MemorySummaryStrategy::Disabled` 仍是默认值，代码未动；`81cda0f` 是纯文档提交（修正 Preference/EntityFacts 的历史文档滞后），与此无关。综合来看该项风险有所缓解（成本熔断机制至少存在了），严重度从 Major 降为 Minor。
- [Nit] `agentflow-cli/src/main.rs` 分发/校验逻辑混杂**未修复，行数从 2525 涨到 2671**，是本轮唯一"原地踏步且规模仍在增长"的项。
- [Nit] `agentflow-tools`（L0 契约+L2 实现捆绑）→ 已随 `b06ce03` 解决。
- [Nit] worker payload 文档滞后 → 已由 `49fd76e` 修复。
- 全仓库 `todo!()`/`unimplemented!()`/`FIXME` 零命中（非 test/example）；`#[allow(dead_code)]` 新增的唯一一处（`v2.rs:22` `InputDefinitionV2.description`）是有意设计（纯文档字段），非半成品。

**模块完整度评级：A-**（昨天 B+）

---

## 3. Agent 框架 / 工程化理念复核

对标 LangChain/AutoGen/CrewAI/Claude Agent SDK 标准，昨天 Top 5 生态缺口逐条复核：

| 缺口 | 昨天 | 今天 | 证据 |
| --- | --- | --- | --- |
| 1. Agent eval 无 CI 门 | Weak | **已关闭（Strong）** | `2219276` 新增 `agent-eval-smoke` job（`.github/workflows/quality.yml:420-467`），完全镜像 `rag-eval-smoke`；新增 `EvalBaseline`/容差回归比对；接入 `release-gate` 硬门（`quality.yml:528-529,563`），失败即拦 PR |
| 2. 长期记忆不完整 | Weak/Adequate（笼统打分） | **需拆分评价** | 存储层（`SqlitePreferenceStore`/`SqliteEntityFactStore`）：schema/加密/prune/CLI 全部完整，7-8 个单测覆盖 CRUD、scope 隔离、prune 边界 → **Strong**。运行时集成：`agentflow-agents`/`agentflow-harness`/`agentflow-skills` **零处**引用这两个 trait，`skill.toml` 也没有 `[memory.preference]`/`[memory.entity_facts]` 的解析入口 → **Weak（未变）**。产品可见行为上和"没做"没有区别 |
| 3. 无实时成本治理 | Weak | **已关闭（Strong）** | 见第 2 节，`8c4e057` |
| 4. 动态工作流 `--approve` 默认 none | Weak (P1) | **部分修复（Adequate）** | `6bd8cf9` 让 `resolve_approve_default` 按 profile 决定（`Local`/`Production` → `"cli"`），但 `--profile` 自身仍默认 `"dev"`——**裸命令行调用**（不传任何 flag）的实际默认行为未变，仍是 `dev→none`；且 `harness run`/`resume` 存在同构但**未被覆盖**的姊妹口子：`--approve` 硬编码默认 `"none"`，与 `--profile` 完全脱钩 |
| 5. Skill 无 MCP 互操作桥接 | Weak | **无变化** | 未被任何新提交触及 |

### 副作用：`81cda0f` 自己引入了一处新的文档不准确

`81cda0f` 在修正"Preference/EntityFacts 未实现"这个旧滞后的同时，新写的 `docs/MEMORY_LAYERING.md` 文本声称 `agentflow-skills` 的 `[memory]` 解析"只接受 `session`/`sqlite`/`none`"——但 `agentflow-skills/src/builder.rs::build_memory`（第 789-847 行）实际接受 **四种**：`session`/`sqlite`/`semantic`/`none`，`semantic` 分支自 2026-04-25（`01fc8e0a`）就已工作。文档修正提交本身也需要代码交叉验证，不能假定其改写的新文本必然准确。

### 其他复核

- 核心循环模式：`PlanExecuteAgent` 仍标注"first prototype"，无中途重规划；`dynamic.rs::run_with_replan` 确认支持失败驱动重规划（`max_replans` 上限+已完成步骤复用），判断维持昨天的 Adequate。
- 验证/护栏：除 `AlwaysApprove` 外仍无第二个内置 `VerificationStrategy` 实现，Adequate 不变。
- `agentflow-tool` 拆分对 `agentflow-agents`/`agentflow-harness` 生产代码逐处核对，均为纯 import 路径替换，无行为语义变化。

**Agent 框架/工程化理念评级：A-**（昨天 B+，五项缺口里三项关闭，一项需拆分重新表述后仍部分开放，一项无变化）

---

## 4. 服务层 / 部署 / 数据管理复核

昨天 Top 5 生产风险复核：

| 风险 | 昨天 | 今天 |
| --- | --- | --- |
| 跨租户伪装（客户端自报 header + 单一共享 token，无 RLS） | 开放 | **完全未动，仍是最严重的开放风险**——`agentflow-server/src/tenant.rs` 最后改动是 2026-05-17，早于任何本轮提交；`extract_tenant_id` 仍只读 `X-Agentflow-Tenant` header 无签名绑定；DB 层仍无 Postgres RLS |
| 无恢复工具 | 开放 | **已修复**——`7ab0e99` 新增 `agentflow restore`（727 行），5/5 端到端 round-trip 测试通过（含 tar `--strip-components=1` 重锚定 bug 的回归测试）。但见 §5 的新 CRITICAL |
| Worker↔Server gRPC 明文无 TLS | 开放 | **已修复**——`efe292b` 真正接线 `ClientTlsConfig`/`ServerTlsConfig`（含 mTLS），2/2 端到端测试（`grpc_tls_e2e`）跑真实编译的 worker 二进制 + rcgen 证书通过 |
| server 二进制缺 gRPC listener flag | 开放 | **已修复**——`agentflow-server/src/worker_grpc.rs`（新增 271 行）+ `AGENTFLOW_WORKER_GRPC_BIND` 等环境变量族，"5 worker 打 1 server" 现在真的能跑起来 |
| Helm 默认空资源限制、无 HPA | 开放 | **已修复**——`9babf10`，`helm lint`/`helm template`（默认值 + `autoscaling.enabled=true` 两种场景）均验证通过，resources 默认值合理（100m/128Mi request，500m/512Mi limit） |

### 新发现 [MAJOR]：Helm chart / docker-compose 默认落在 `SecurityProfile::Local`，文档从不提及

`charts/agentflow/values.yaml` 和 `docs/DEPLOYMENT.md` 全文都没有引用 `AGENTFLOW_SECURITY_PROFILE`。`SecurityProfile::default()` 是 `Local`（`agentflow-tool/src/security_profile.rs`），`Local` 档位下 `require_api_token: false`、CORS 宽松，`agentflow-server/src/auth.rs` 的 `resolve_auth_config` 只在 `Production` 档位下才 fail-closed。净效果：**严格照着 `docs/DEPLOYMENT.md` 部署的 Helm 安装跑在 Local 模式**——没有强制 API token，如果运维之后开了 `--worker-grpc` 却没意识到要设置 profile，worker 准入策略同样是开放的（`for_profile` 的 fail-closed 检查只在 `Production` 下触发）。本节和 §5 刚验证完成的所有 fail-closed 机制，在这一个默认部署路径上其实是休眠的。

### 仍未解决

- PodDisruptionBudget 依然缺失。
- 首方 OTLP exporter 依然缺失（Q2.3.3，可接受现状）。
- `b06ce03`/`dfc0e57`/`c1291da` 对 server/worker 零影响，`cargo check -p agentflow-server -p agentflow-worker` 编译通过，确认无意外回归。

**服务层/部署/数据评级：B+**（昨天 B，进展扎实但跨租户风险完全未动，且新增一个部署默认档位问题，未给更高分）

---

## 5. 安全性复核

昨天 5 条发现逐条复核：

| # | 昨天 | 今天 |
| --- | --- | --- |
| 1 CRITICAL 市场签名验证未接线 | 未接线，默认仅 SHA-256 校验和 | **已修复**（`08c86d0`）：非本地 registry 默认切到 `Ed25519SignatureVerifier{require_signature:true}`；新增 `SignatureVerificationKind` 枚举防止未来再混淆"校验和"与"签名"；`--allow-unsigned` 需显式选择退出并打印警告；`key_id` 路径穿越校验；5 条针对 loopback HTTP registry 的集成测试 |
| 2 MAJOR worker 准入非 fail-closed | 类型存在但未强制 | **已修复且真正生效**（`e09ec70`+`efe292b`）：`WorkerAdmissionPolicy::for_profile(Production)` 在准入策略为空时启动即失败；`efe292b` 新增的真实 gRPC 监听器在启动时调用它，配置错误同步失败而非静默放行 |
| 3 MINOR cgroup 叶子泄漏 | 从不清理 | **已修复**（`8181c4d`）：`LinuxSeccompBackend` 用 `Mutex<Vec<PathBuf>>` 跟踪叶子并在每次新 spawn 前尝试回收，2 个新单测覆盖回收与"仍在使用不误删"两种场景 |
| 4 MINOR 容器根文件系统可写 | 无 `--read-only` | **已修复**（同 `8181c4d`）：两种容器引擎均已加 `--read-only`，`/workspace` 是唯一可写路径 |
| 5 NIT Tera 模板 DoS | 开放 | **无变化**，仍是低严重度开放项 |

`agentflow-tool` 拆分（`b06ce03`）独立安全等价性核对：`capability.rs`/`error.rs`/`plugin_policy.rs`/`policy.rs`/`security_profile.rs`/`tool.rs` 逐文件比对为 **Git 确认的完全相同 blob**，纯改名；`sandbox/backend.rs → sandbox.rs` 内容一致，diff 只是 doc 注释路径重写；`registry.rs` 的 47 行 diff 是两个单测被迁到独立测试文件，断言未变。`Production` 档位下 `require_api_token`/`require_signature_verification`/`require_os_sandbox`/`require_credential_config` 全部确认仍为 `true`，搬迁过程无任何默认值被悄悄放宽。**结论：这次架构重构对安全性零影响，是干净的搬迁。**

### 新发现 [CRITICAL]：`agentflow restore` 在安全护栏检查**之前**执行了破坏性删除

`agentflow-cli/src/commands/restore.rs::restore_dir()`：`std::fs::remove_dir_all(&target)`（第 399 行）在 `target.parent().is_none()`（"拒绝恢复到文件系统根路径"护栏，第 413 行）**之前**执行。`target` 来自 `resolve_include_dir` → `resolve_env_or_default`（`backup.rs:613-616`），对 `AGENTFLOW_RUN_DIR`/`_TRACE_DIR`/`_SKILLS_DIR`/`_PLUGINS_DIR`/`_MARKETPLACE_CACHE` 等环境变量**零校验**直接 `PathBuf::from(env::var(...))`。如果这些环境变量被误配置为 `/`（拼写错误、容器模板错误、`docker run` 参数复制粘贴事故），配合 `--force`（`/` 目录"存在"所以需要它）会触发 `remove_dir_all("/")`——递归删除文件系统根目录，而护栏本应阻止这个场景却因为顺序错误从未生效。**修复是一行代码顺序调整**，但当前状态下这是一个真实可触发的灾难性 bug。

### 新发现 [MAJOR]：`agentflow restore` 不校验制品完整性

`BundleManifestArtifact` 只记录 `bytes`（大小），从不记录哈希。`restore_db` 把数据库转储直接喂给 `pg_restore --clean --if-exists`（会先删除现有对象）无任何校验和检查；`restore_dir` 的 tar 解包无归档条目检查，路径穿越/符号链接安全性完全依赖宿主机未锁定版本的 `tar` 二进制。损坏或被篡改的备份会在无警告的情况下破坏现网数据库。

### 其他新发现

- [MINOR] `max_retries`/`timeout_ms`（`c1291da`）无显式上限校验，但 `TimeoutRetryNode` 构建的 `RetryPolicy` 保留了 `RetryPolicy::default()` 的 `max_duration: Some(300s)`，`should_retry_time()` 会在墙钟超过 5 分钟后停止重试——**自限性的**，不是无界放大，严重度从最初判断的 Major 下调为 Minor，但显式校验仍是更好的卫生实践。
- [MINOR] 生产 gRPC 监听器的 fail-closed 检查只要求准入凭据（allowed IDs/PSK/JWT），不强制要求 TLS——运维可以在生产环境只配 PSK 不配 TLS，PSK 明文过网。`docs/DISTRIBUTED.md` 已文档化为"仅限可信网络"，是有意的范围限定而非静默回归，但值得补一个类似 `for_profile` 的 TLS 强制检查。
- [MINOR] `workflow dynamic` 的审批默认值修复未覆盖裸命令行调用场景（见 §3）。

**安全性评级：B+（不变）**——旧发现全部正确闭环，`agentflow-tool` 拆分零安全回归，但同一窗口新增的 CRITICAL 抵消了这些进展。

---

## 6. 跨维度综合优先级清单

| 优先级 | 发现 | 来源维度 | 修复成本 |
| --- | --- | --- | --- |
| **P0** | `agentflow restore` 删除目标目录先于安全护栏检查，可被误配置的环境变量+`--force`触发 `rm -rf /` | 安全 | 极低（一行顺序调整） |
| **P0** | `agentflow restore` 无制品完整性校验（无 checksum），`pg_restore --clean`/tar 解包对损坏或篡改的备份没有防护 | 安全 | 中 |
| P1 | 跨租户伪装：客户端自报 `X-Agentflow-Tenant` header + 单一共享 bearer token，无 JWT/OIDC 绑定，DB 层无 RLS 兜底 | 服务层 | 中高（需要真实身份认证机制） |
| P1 | Helm chart/docker-compose 默认 `SecurityProfile::Local`，刚补上的 fail-closed 机制（API token/worker 准入）在标准部署路径中处于休眠状态，文档从未提及 `AGENTFLOW_SECURITY_PROFILE` | 服务层 | 低（文档 + chart 默认值/校验） |
| P1 | 成本熔断机制已在运行时落地，但无 CLI flag / Server API 字段可配置，操作员摸不到 | 模块完整度 + 生态 | 低（补 `--cost-limit-usd` flag + 请求体字段） |
| P2 | `agents`/`harness → agentflow-memory` 应改为依赖已存在的 `agentflow-store-spi` 契约 crate（与刚完成的 tools 拆分同构，但目标 crate 已就位，成本更低） | 架构 | 低-中 |
| P2 | Preference/EntityFactStore 存储层完整但零运行时集成（`SkillBuilder`/agent runtime 均未消费，`skill.toml` 无解析入口） | 生态 | 中高 |
| P2 | `harness run`/`resume` 的 `--approve` 默认值硬编码 `"none"`，与 `--profile` 脱钩，是 `workflow dynamic` 刚修复问题的未覆盖姊妹口子 | 生态 | 低 |
| P2 | `81cda0f` 新引入的文档错误：声称 skill `[memory]` 只接受 3 种类型，实际 `semantic` 自 4 月起就受支持 | 生态/文档 | 极低 |
| P3 | `agentflow-cli/src/main.rs` 分发/校验逻辑混杂，且规模持续增长（2525→2671 行） | 模块完整度 | 中 |
| P3 | Helm chart 无 PodDisruptionBudget | 服务层 | 低 |
| P3 | `docs/ARCHITECTURE_DIAGRAM.md`、`docs/ARCHITECTURE_EVALUATION_2026-06-20.md` 拆分后未同步 `agentflow-tool` 的存在 | 架构/文档 | 低 |
| P3 | 生产 gRPC 监听器 fail-closed 检查只覆盖凭据，不强制 TLS | 安全 | 低-中 |
| P4 | `workflow dynamic` 裸命令行调用仍解析为 `dev` profile → `none` 审批 | 生态 | 低（改默认值或加提示） |
| P4 | Tera 模板渲染无超时上限（DoS 级别，非 RCE） | 安全 | 低 |
| P4 | 无首方 OTLP exporter | 服务层 | 中（可维持现状） |

**综合评级：A-（有条件）**（上一版 2026-07-29 为 B+）。上调理由是过去 24 小时内昨天清单里 9/13 条被验证真正解决，且验证方式扎实（编译通过、专门回归测试、端到端集成测试全部实测，而非只读 commit message）。未给到更高等级、且标注"有条件"的原因：**P0 的两条新安全发现（尤其 restore 删除顺序 bug）严重程度不低于昨天已修复的市场签名 CRITICAL，且是本轮新引入的代码里的问题，不是历史债务**——在这两条被修复之前，任何"综合评级已经很好"的结论都应该被这个新发现打折扣。建议下一轮验证优先确认这两条 P0 已修复，再评估是否能给出不带条件的 A-。