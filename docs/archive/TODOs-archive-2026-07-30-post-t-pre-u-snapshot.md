# AgentFlow TODOs

Last updated: 2026-07-29

## 维护约定

- 旧执行计划按时间分批归档到 `docs/archive/`：
  - `TODOs-archive-2026-05-09-n1-n10.md` — N1–N10 路线图段（已闭环）。
  - `TODOs-archive-2026-05-10-p0-p4.md` — 早期 P-段执行计划（已闭环）。
  - `TODOs-archive-2026-05-19-recently-closed.md` — 5/19 从 Recently Closed
    扫出去的中段历史。
  - `TODOs-archive-2026-05-20-closed-segments.md` — 12 个全 closed 的 P-段
    （P0/P1/P2/P3/P4/P5/P6/P7/P-H/P9/P-LLM/M）整体外迁。
  - `TODOs-archive-2026-05-24-p10-optimization-backlog.md` — P10 优化 backlog
    （v1.0.0-rc.1 ops + 19 个 crate-level 子段），全部 DONE 项 + 少量未拾起的
    polish。
  - `TODOs-archive-2026-06-20-q1-q5-audit-remediation.md` — Q1–Q5 五段
    （2026-05-24 深度审计修复波次）全部闭环（108 DONE / 0 TODO）整体外迁，含
    Audit Assessment Summary。源审计仍在 `docs/audit/`。
  - `TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md` — 7/28
    全量快照：H / P-A / S / L 四段全部 DONE/DEFERRED 收口后、启动 R（工程化
    审计修复）段之前的完整存档（含全部历史明细）。
  - `TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md` — **本次 7/29 快照**：
    R 段（2026-07-28 工程化审计修复，R0–R4 十二项 + R4 追加 8 项）在
    GitHub Actions 真实硬件上验证 `release gate: conclusion=success` 后的
    完整存档，启动新 **T（2026-07-29 架构评估修复）** 段之前的收口状态。
    本文件即从这份快照重建。
- 本文件是短期执行队列。H / P-A / S / L / R 五段已全闭环并整体存档；当前仅
  保留 **T（2026-07-29 架构评估修复）** 一个 backlog。
- 本次 T 段来源：`docs/archive/PROJECT_EVALUATION_2026-07-29.md`——五个独立
  维度并行审计（架构分层 / 模块完整度与职责单一性 / Agent 框架生态 / 服务层
  部署数据管理 / 安全性）后的综合评估，综合评级 **B+**（上一版 6/6 为 A）。
  核心结论：**没有发现新的架构性倒退**，但反复出现同一种模式——"机制已经
  正确实现，却没有被生产默认路径真正调用"（marketplace 签名验证器、agent
  运行时成本熔断、worker TLS/JWT admission、agent-eval CI 门禁都是这个模式的
  实例）。T 段按 evaluation 报告 §6 的跨维度优先级表（P0–P4）转化为可执行
  任务，编号延续该表的优先级分组（T0 对应 P0，以此类推）。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态。
- `RoadMap.md` 保留中长期路线。
- `docs/archive/PROJECT_EVALUATION_2026-07-29.md` 是本轮 T 段的评估依据，
  每个 T-item 下方引用其对应章节。
- 任务状态只使用：
  - `TODO`：未开始或正在执行。
  - `DONE`：已完成、已测试、已提交。
  - `DEFERRED`：显式推迟到 RoadMap Later Tracks 或 Non-Goals。

## Active Queue Overview

Current focus: **H / P-A / S / L / R 五段已全闭环并整体存档**（见上）；新的
**T（2026-07-29 架构评估修复）** T0–T4 共 15 项待办
（**14 DONE / 1 DEFERRED**），按
`docs/archive/PROJECT_EVALUATION_2026-07-29.md` §6 的跨维度优先级排序：
**T0（阻断性安全/租户隔离）2 项，全部 DONE（T0.1 / T0.2）**、
**T1（生产健壮性护栏）3 项，全部 DONE（T1.1 / T1.2 / T1.3）**、
**T2（工程化 / CI 治理）4 项，全部 DONE（T2.1 / T2.2 / T2.3 / T2.4）**、
**T3（完整度缺口）3 项，全部 DONE（T3.1 / T3.2 / T3.3）**、
**T4（长期 backlog，低优先级）3 项，2 DONE（T4.1 / T4.3）/
1 DEFERRED（T4.2，Q2.3.3 既有决定的正式收纳）**。T 段所有可执行项已
全部闭环——T4.1 核查后发现 Preference/EntityFacts store 早在 2026-05-24
（P4.7）就已实现，evaluation 报告依据的是过期文档而非代码现状，本项
落地为文档修正而非新代码。

| Segment | Theme | Status |
| --- | --- | --- |
| N1 → N10 / P0 → P9 / P-H / P-LLM / M / P10 | 历史段，全部 closed 或外迁 | ARCHIVED |
| Q1 → Q5 | 2026-05-24 深度审计修复波次，108 DONE | ARCHIVED |
| H | Harness Mode follow-ups（loop-ownership + `harness chat` 收尾） | **DONE — archived（7/28）** |
| P-A | 契约内核 + 架构演进（`docs/RFC_CRATE_ARCHITECTURE.md`） | **DONE — archived（7/28）** |
| S | 沙箱与代码执行安全演进（`code_exec` / OS sandbox 强化） | **DONE — archived（7/28）** |
| L | 长程任务与检索增强（replan / 项目记忆 / RAG 补强 / 委托契约） | **DONE — archived（7/28）** |
| R | 2026-07-28 工程化审计修复（CI 覆盖率 / 架构守卫盲区 / 文档陈旧 / 仓库卫生） | **DONE — archived（7/29）** |
| T | 2026-07-29 架构评估修复（五维度独立审计：架构 / 完整度 / Agent 生态 / 服务层 / 安全） | **DONE — closed，15 项（14 DONE / 1 DEFERRED）** |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## T — 2026-07-29 架构评估修复（architecture-evaluation remediation）

> 来源：`docs/archive/PROJECT_EVALUATION_2026-07-29.md`（五个独立 agent 并行
> 审计：架构分层、模块完整度与职责单一性、Agent 框架生态、服务层/部署/数据
> 管理、安全性；互不共享结论，产出后交叉核对去重）。**核心结论**：项目核心
> 命题在代码层面依然对齐，S-track 沙箱加固和 P-A 架构抽取都是真实且高质量的
> 进展；本轮评级下调（A → B+）反映的是审计方法比历次评估更深入地追问"默认
> 路径是否真的调用了已实现的机制"，挖出了一批"实现完整但未接线为默认"的
> 问题。排序原则：**T0 是唯一涉及生产安全/隔离的阻断性分组**，其余为非阻断
> 的工程化/完整度加固。每项修复需配 regression test；涉及默认值变更的需要
> 同步更新对应文档（`docs/OPERATIONS_HANDBOOK.md`/`docs/SECURITY.md` 等）。

### T0 — 安全与租户隔离（blocking）

- DONE T0.1 Marketplace 安装路径默认启用真实 Ed25519 签名验证（**CRITICAL**，
  evaluation §5 finding 1）：`agentflow-cli/src/commands/marketplace.rs:275`
  → `agentflow-skills/src/remote_marketplace.rs:253-259` 的 `cache_from_dir`/
  `RemoteMarketplaceCache::new` 默认构造 `ChecksumSha256SignatureVerifier`
  （只是对制品重新哈希，不是对攻击者不可控内容的真实密码学签名验证）。真正
  的 `Ed25519SignatureVerifier`（`remote_marketplace.rs:458`）已完整实现且有
  单测覆盖，但没有任何 CLI 调用点通过 `with_client_and_verifier` 构造它。
  **验收标准**：非本地（`registry_kind` 非 `local`/`file://`）注册表的安装/
  验证路径默认使用 `Ed25555SignatureVerifier { require_signature: true }`；
  显式选择降级到 checksum-only 校验需要一个明确的 opt-out flag（如
  `--allow-unsigned`）并在 CLI 输出里打印醒目警告，不能再是静默默认行为；
  新增集成测试证明"未签名/签名不匹配的包默认被拒绝安装"；`agentflow skill
  marketplace install --dry-run` 或等价命令的输出里 `signature_checked` 字段
  语义要能反映"真正验证了密码学签名"还是"只校验了 checksum"，不能混用同一
  个 true/false。

  **证据**：
  - `agentflow-cli/src/commands/marketplace.rs`：新增 `is_remote_registry()`
    辅助函数（复用既有 http(s) 前缀判断）+ 重写 `cache_from_dir()`——非本地
    （http/https）注册表默认构造 `Ed25519SignatureVerifier::new(keys_dir)`
    （`require_signature: true` 是 `Ed25519SignatureVerifier::new` 自身默认值）；
    本地文件清单保持原 `ChecksumSha256SignatureVerifier` 默认不变。
    `install`/`verify` 子命令新增 `--allow-unsigned`（显式 opt-out，降级前打印
    三行 stderr 警告说明不做加密验证、不能用于生产）和 `--keys-dir`（覆盖
    `~/.agentflow/marketplace-keys` 默认路径）。`update` 子命令不涉及制品验证，
    保持原行为。
  - `agentflow-skills/src/remote_marketplace.rs`：新增
    `SignatureVerificationKind { Unsigned, ChecksumOnly, CryptographicSignature }`
    枚举；`MarketplaceSignatureVerifier::verify` 返回值从 `Result<(), _>` 改为
    `Result<SignatureVerificationKind, _>`；`CachedMarketplaceArtifact` 新增
    `signature_verification` 字段（`signature_checked` 字段语义保持不变，向后
    兼容，但现在有独立字段明确区分"真正验证了密码学签名"
    (`cryptographic_signature`) 与"只校验了 checksum"(`checksum_only`)，解决了
    验收标准里"不能混用同一个 true/false"的要求）。CLI `install`/`verify` 输出
    新增一行 `signature_verification: <kind>`。
  - 新增 5 个 CLI 集成测试（`agentflow-cli/tests/marketplace_cli_tests.rs`，起
    HTTP loopback server 验证真实 http(s) registry 路径）：
    `marketplace_verify_remote_registry_rejects_unsigned_by_default`、
    `marketplace_verify_remote_registry_rejects_checksum_only_signature_by_default`、
    `marketplace_verify_remote_registry_allow_unsigned_falls_back_to_checksum`、
    `marketplace_verify_remote_registry_accepts_valid_ed25519_signature_by_default`
    （生成真实 ed25519 密钥对，走完整签名验证成功路径）、
    `marketplace_install_remote_registry_rejects_unsigned_by_default`（证明
    `install` 而非仅 `verify` 在默认策略下拒绝未签名包并且不会 unpack）。
    `agentflow-cli/Cargo.toml` 新增 `ed25519-dalek` dev-dependency 支撑测试签名。
    既有 21 个 marketplace CLI 测试（全部用本地文件路径作为 registry）+
    `agentflow-skills` 的 18 个 remote_marketplace 单测 + 7 个
    `marketplace_signed.rs` 测试全部保持通过，未破坏本地/checksum-only 路径。
  - `docs/MARKETPLACE.md` 新增 "CLI Default Verifier Selection" 一节 + 更新
    "Signing Policy Boundary"/"Local signing"/"Current Boundaries"；
    `docs/STABILITY.md` Q1.10.1 一节更新为反映 CLI 已自动接线，不再要求调用方
    手动 opt-in。
  - 验收命令：`cargo test -p agentflow-skills --all-features`（158 passed）、
    `cargo test -p agentflow-cli --all-features`（全 suite green，含新增 5 项）、
    `cargo clippy --workspace --all-features -- -D warnings`（clean）、
    `cargo run -p xtask -- check-arch`（`check-arch: OK`，无新增依赖律违规）。
- DONE T0.2 Worker gRPC 准入补 `SecurityProfile` 驱动的 fail-closed 检查
  （evaluation §5 finding 2 + §4"Worker/分布式"）：
  `agentflow-server/src/scheduler/admission.rs:134-167` 的
  `WorkerAdmissionPolicy::default()`/`::open()` 允许任何无凭据 worker 接入，
  没有类似 `auth.rs:60` 的 `require_api_token` 那种在 `SecurityProfile::
  Production` 下强制校验的机制。**验收标准**：新增一个profile-aware 构造
  路径/校验函数，在 `SecurityProfile::Production` 下如果
  `allowed_workers`/`pre_shared_keys`/`jwt` 全部未设置则启动失败（镜像
  `resolve_auth_config` 的 fail-closed 行为）；新增单测覆盖"production
  profile + 空 admission 配置 → 启动错误"和"production profile + 至少一种
  凭据配置 → 正常启动"两个分支；`agentflow doctor --profile production`
  的诊断输出里体现这项检查的结果。**与 T1.2（gRPC listener 接线）解耦**：
  本项只处理 `AuthenticatedControlPlane` 类型本身的默认值安全性，不要求
  同时完成 server 二进制的 gRPC listener flag（那是 T1.2 的范围）。

  **证据**：
  - `agentflow-tools/src/security_profile.rs`：新增 `WorkerAdmissionDefaults
    { require_credential_config: bool }`，接入 `SecurityProfileDefaults`
    （`dev`/`local` = `false`，`production` = `true`，镜像
    `AuthDefaults::require_api_token` 的形状），从 crate 根重新导出。
  - `agentflow-server/src/scheduler/admission.rs`：新增
    `WorkerAdmissionPolicy::for_profile(self, profile: SecurityProfile) ->
    Result<Self, AdmissionConfigError>`（私有 `has_credential_config()`
    辅助判断 `allowed_workers`（非空集合）/`pre_shared_keys`（非空）/`jwt`
    （`Some`）三者是否至少配置一项）+ 新增 `AdmissionConfigError` 错误类型
    （与逐次调用的 `AdmissionError` 区分，这是启动期一次性校验）。
    `agentflow-server/src/scheduler/mod.rs` + `lib.rs` 导出新符号。
  - 新增 6 个单测（`agentflow-server/src/scheduler/admission.rs#tests`）：
    `production_profile_rejects_empty_admission_policy`、
    `production_profile_accepts_allowed_workers_only`、
    `production_profile_accepts_pre_shared_keys_only`、
    `production_profile_accepts_jwt_policy_only`、
    `production_profile_rejects_empty_allowed_workers_set`（`Some(空集合)`
    虽然实际语义上"谁都不放行"，但仍不构成真实凭据机制，同样判定失败）、
    `dev_and_local_profiles_accept_empty_admission_policy`。
  - `agentflow-tools/src/security_profile.rs` 新增
    `dev_defaults_do_not_require_worker_admission_credentials` 单测 +
    更新 `local_is_backward_compatible_default`/
    `production_defaults_fail_closed_for_exposed_runtime` 断言覆盖新字段。
  - `agentflow-config/src/diagnostics.rs`：`agentflow doctor` 文本报告新增
    一行 `worker admission credentials required: <yes/no>`；JSON 报告的
    `security.defaults.worker_admission.require_credential_config` 因
    `SecurityProfileDefaults` 整体序列化而自动获得，无需额外接线。
  - `docs/DISTRIBUTED.md` 新增 "Fail-closed construction (T0.2)" 小节 +
    测试引用列表补充；`docs/STABILITY.md` 分布式控制面条目更新说明新增的
    `AdmissionConfigError` 类型。
  - 验收命令：`cargo test -p agentflow-tools --all-features`（109+ passed）、
    `cargo test -p agentflow-server --all-features`（186+ passed，含新增
    6 项）、`cargo test -p agentflow-config --all-features`（29 passed）、
    `cargo clippy --workspace --all-features -- -D warnings`（clean）、
    `cargo run -p xtask -- check-arch`（`check-arch: OK`）。

### T1 — 生产运行时护栏

- DONE T1.1 生产 agent 运行时补齐成本熔断（evaluation §2 finding 1 + §3
  ecosystem gap 3）：`agentflow-agent-spi/src/runtime.rs:367-369` 明确写明
  `CostLimitExceeded` "仅 eval runner 今天会 emit"；`RuntimeLimits`
  （`runtime.rs:24-33`）没有 `cost_limit_usd` 字段，只有
  `max_steps`/`max_tool_calls`/`timeout_ms`/`token_budget`。**验收标准**：
  `RuntimeLimits` 新增可选 `cost_limit_usd: Option<f64>` 字段；`ReActAgent`/
  `PlanExecuteAgent` 的主循环在每次 LLM 调用后累加实际花费（复用
  `agentflow-agents/src/eval/pricing.rs` 的 `ModelPricing` 计价逻辑，不要
  重新发明一套定价表），超出预算时以真实的 `AgentStopReason::
  CostLimitExceeded { used_usd, budget_usd }` 中途停止循环（而不是事后
  在 eval runner 里补记）；新增测试证明"预算内正常运行"和"预算耗尽后在
  下一步工具调用前停止，累计花费不超过预算太多"两个场景；文档
  （`docs/AGENT_RUNTIME.md`/`docs/OPERATIONS_HANDBOOK.md` §5.1 的
  `AgentStopReason` 表）同步更新"仅 eval harness 生效"这条已过期的表述。

  **证据**：
  - `agentflow-agent-spi/src/runtime.rs`：`RuntimeLimits` 新增
    `cost_limit_usd: Option<f64>`（`Eq` 派生因此移除——`f64` 不满足 `Eq`，
    保留 `PartialEq`，加注释说明）；`react_defaults()` 补上新字段（`None`）。
  - `agentflow-agents/src/react/agent.rs`：`ReActConfig` 新增
    `cost_limit_usd: Option<f64>` + `pricing_table:
    crate::eval::PricingTable`（默认全零价格表，复用
    `agentflow-agents::eval::pricing`，不是新定价表）+ 对应 `with_*` 构造器；
    `LoopState` 新增 `cost_limit_usd`/`cumulative_cost_usd` 字段；
    `check_turn_limits`（每轮顶部检查，紧跟 `TokenBudgetExceeded` 检查之后）
    新增成本超限分支 → `AgentStopReason::CostLimitExceeded`；
    `run_turn_llm_call` 在每次 LLM 调用后用 `pricing_table.lookup(model)
    .cost_for_call(prompt_tokens, completion_tokens)` 累加 `cumulative_cost_usd`。
    turn-driven（`ReActLoopSession`/`ReActTurnDriver`）与批量 `run_with_context`
    共享同一个 `run_one_turn`，故两条路径都自动获得熔断。
  - `agentflow-agents/src/plan_execute.rs`：`PlanExecuteConfig` 同样新增
    `cost_limit_usd`/`pricing_table` + `with_*`；`run_as_flow`/
    `run_with_context` 两条路径在唯一一次 planner 调用后（token budget 检查
    之后、解析 plan 之前）新增成本检查（单次调用即整个运行的总成本，无需
    跨轮累加）；新增 `cost_for_response` 辅助方法。
  - `agentflow-agents/src/eval/runner.rs::limits_from_case`：同步补上
    `cost_limit_usd` 字段（从 `EvalCase` 透传），eval harness 的事后
    `cost_exceeded` 重新核算逻辑保留作为独立的报告层安全网，不依赖/不替代
    这次的运行时内熔断。
  - 修复因新增字段导致的全部 `RuntimeLimits { .. }` 字面量编译错误（未用
    `..Default::default()` 的 4 处：`agentflow-cli/src/commands/harness/
    {run,chat}.rs`、`examples/applications/code-reviewer-write/src/main.rs`、
    `agentflow-agents/src/eval/runner.rs`）。
  - 新增 4 个回归测试（`flat_dollar_per_call_pricing` 辅助：mock provider
    固定 `prompt_tokens=50`，配 `input_per_1k` 让每次调用花费恒定 $1，规避
    响应文本 word-count 带来的不确定性）：
    `react::agent::tests::cost_limit_stops_run_before_next_llm_call_once_exceeded`
    （2 次调用共 $2.00 超过 $1.5 预算，第 3 轮顶部停止，验证累计花费"不超
    预算太多"）、
    `react::agent::tests::cost_limit_does_not_interrupt_a_run_that_stays_within_budget`、
    `plan_execute::tests::run_with_context_stops_with_cost_limit_exceeded_before_executing_plan`
    （预算内单次 planner 调用即超限，验证计划完全不执行）、
    `plan_execute::tests::run_with_context_cost_limit_does_not_interrupt_a_run_within_budget`。
  - `docs/AGENT_RUNTIME.md`（Core Types / ReAct Runtime / Plan-and-Execute
    Runtime 三处）+ `docs/AGENT_SDK.md`（自定义 runtime 契约新增可选第五项
    bound 的说明）+ `docs/OPERATIONS_HANDBOOK.md`（§2.2 排查提示 + §5.1
    `AgentStopReason` 表）更新"仅 eval harness 生效"的过期表述。
  - 验收命令：`cargo test -p agentflow-agent-spi --all-features`（57
    passed）、`cargo test -p agentflow-agents --all-features`（235+7+1+4+1
    passed，含新增 4 项）、`cargo test -p agentflow-cli --all-features`
    （全 suite green）、`cargo clippy --workspace --all-features --all-targets
    -- -D warnings`（clean）、`cargo run -p xtask -- check-arch`
    （`check-arch: OK`）。
- DONE T1.2 Worker↔Server 分布式部署形态补全：gRPC TLS 接线 + server
  listener flag（evaluation §4"Worker/分布式"，`docs/DISTRIBUTED.md` 自述
  的缺口）：`agentflow-worker/src/main.rs:57-63` 已经接受
  `--server-ca`/`--client-cert`/`--client-key` 参数但未使用（明文 gRPC）；
  `agentflow-server` 二进制目前没有 `--worker-grpc` 监听 flag，导致"多个
  worker 打一个 server"这个分布式部署形态今天实际跑不起来。**验收标准**：
  worker 侧的证书/密钥 flag 真正接入 tonic 的 TLS 配置（`ClientTlsConfig`/
  `Channel::tls_config`）；server 侧新增 CLI flag 启动 gRPC 控制面监听
  （可复用 worker 侧已有的 `WorkerProtocol` server 实现）；新增一个端到端
  集成测试或至少手工验证记录：一个真实 worker 进程通过 TLS 连接到 server 的
  gRPC listener 并完成一次 claim/heartbeat/execute/report 循环；
  `docs/DISTRIBUTED.md` 更新为反映这个形态已经可以端到端跑通，去掉"仍需要
  一步接线"的免责声明。

  **证据**：
  - `agentflow-worker-proto/Cargo.toml` + `agentflow-server/Cargo.toml`：
    tonic 的 `tls`/`tls-roots` feature 从"靠 qdrant-client 特性统一意外带入"
    改为显式声明（原先若裁剪 agentflow-rag 依赖链会静默破坏 TLS 编译）。
  - `agentflow-worker-proto/src/grpc.rs`：新增 `GrpcWorkerProtocol::
    connect_tls(endpoint, ClientTlsConfig)`；重新导出 `Certificate`/
    `ClientTlsConfig`/`Identity`（一路透传到 `agentflow_server::scheduler::*`），
    使 `agentflow-worker` 不需要对 tonic 加正式依赖就能构造 TLS 配置。
  - `agentflow-worker/src/main.rs`：`grpc_endpoint()` 新增 `tls_enabled`
    参数，TLS 开启时 `grpc://` 映射到 `https://` 而非 `http://`（tonic 对
    `http://` + `tls_config` 组合会报错）；新增 `build_tls_config()` 从
    PEM 文件路径读取 CA/客户端证书/私钥构造 `ClientTlsConfig`，
    `--client-cert`/`--client-key` 必须成对出现或都不出现；`memory://local`
    下设置 TLS flag 只警告不报错（对齐既有 `--admission-token` 行为）；
    去掉"not yet wired"警告 + 更新 help 文本。
  - `agentflow-server/src/worker_grpc.rs`（新模块）：`WorkerGrpcServeConfig`
    / `WorkerGrpcTlsConfig` / `build_worker_control_plane`（把
    `allowed_worker_ids` + 单个共享 `shared_psk` 组装成
    `WorkerAdmissionPolicy`，经 T0.2 的 `for_profile` fail-closed 校验）/
    `serve_worker_grpc`（读 TLS 文件 → `ServerTlsConfig`（可选
    `client_ca_root` 做 mTLS）→ fail-fast bind probe → `WorkerControlServer`
    + `AuthenticatedGrpcWorkerService` 走 `serve_with_shutdown`）。刻意与
    `serve::run` 的 Postgres/`AppState` 依赖解耦，可独立测试/复用。
  - `agentflow-server/src/serve.rs`：`ServeConfig` 新增
    `worker_grpc: Option<WorkerGrpcServeConfig>`；`run()` 里 admission
    校验失败同步冒泡为 `ServeError`（等同 `--database-url` 缺失的严重性），
    bind/TLS 文件错误则作为后台任务日志（对齐既有 `spawn_cleanup_loop`
    "尽力而为"先例，不因此拒绝整个网关启动）。
  - `agentflow-server/src/main.rs` 新增 `AGENTFLOW_WORKER_GRPC_BIND` /
    `_TLS_CERT` / `_TLS_KEY` / `_CLIENT_CA` / `AGENTFLOW_WORKER_IDS` /
    `AGENTFLOW_WORKER_PSK` 环境变量解析；`agentflow-cli` 的 `ServeArgs` +
    `commands/serve.rs` 新增对应 `--worker-grpc*`/`--worker-ids`/
    `--worker-psk` flag，转发为上述环境变量（复用现有 flag→env 透传模式）。
  - 新增 4 个测试：`agentflow-server/src/worker_grpc.rs#tests`
    （明文 claim/heartbeat/report 循环 + production profile 无凭据时拒绝）；
    `agentflow-worker/tests/grpc_tls_e2e.rs`（**真实编译出的
    `agentflow-worker` 二进制**——不是 in-process mock——通过 `rcgen` 现场生成
    的自签 CA + server/client 叶子证书，走完整 mTLS 握手完成
    claim→execute→report；配套的反向测试证明不带客户端证书的连接在 mTLS
    握手阶段就被拒绝，走不到 admission/PSK 校验）。
  - `docs/DISTRIBUTED.md`：新增"Transport Security (T1.2)"章节（完整
    flag/env 参考表）；重写"Two-Worker Deployment Shape"为真实可跑通的
    `agentflow serve`/`agentflow-worker` 命令（含 TLS flag）；顺带修正两处
    过期表述（admission-token metadata propagation 早已落地，不再是
    "deferred follow-up"）。
  - 验收命令：`cargo test -p agentflow-worker-proto -p agentflow-server
    -p agentflow-worker -p agentflow-cli --all-features`（全部 green，
    含新增 4 项：`agentflow-server` 188 passed、`agentflow-worker` 33
    passed）、`cargo clippy --workspace --all-features --all-targets --
    -D warnings`（clean）、`cargo run -p xtask -- check-arch`
    （`check-arch: OK`，无新增依赖律违规）。
- DONE T1.3 `agentflow workflow dynamic --approve` 在非 dev profile 下默认
  要求审批（evaluation §3 ecosystem gap 4）：`agentflow-cli/src/commands/
  workflow/dynamic.rs` 里 `--approve` 当前默认值是 `"none"`——一个 LLM 生成、
  天然对抗性的计划默认无监督执行，仅靠 `--allow-path`/`--allow-domain`
  沙箱兜底。**验收标准**：`local`/`production` `SecurityProfile` 下
  `--approve` 的默认值改为至少 `"cli"`（或要求显式传 `--approve none` 才能
  关闭审批，而不是相反）；`dev` profile 可以保留当前"默认无审批"的宽松行为
  以不打断本地快速迭代；新增测试覆盖"未显式指定 --approve 时，非 dev
  profile 下动态生成的计划触发审批请求"；`--dry-run` 路径不受影响（dry-run
  本身不执行，不需要审批）。

  **证据**：
  - `agentflow-cli/src/main.rs`：`Dynamic` 子命令的 `approve` 字段从
    `String`（clap `default_value = "none"`）改为 `Option<String>`（无
    clap 默认值），使"未传 --approve"与"显式传 --approve none"在
    `execute()` 里可区分——这是验收标准的前提，此前二者信息一致，无法区分。
  - `agentflow-cli/src/commands/workflow/dynamic.rs`：新增
    `resolve_approve_default(approve: Option<String>, profile: HarnessProfile)
    -> String`（纯函数，直接单测）——`None` 时 `dev` → `"none"`，
    `local`/`production` → `"cli"`；显式传值（含 `--approve none`）在任何
    profile 下都优先生效。
  - **实现过程中发现并修复了一个更根本的既有 gap**：`HookConfig` 自身的
    profile-based escalation 只在 `HarnessProfile::Production` 下才会把
    `NonIdempotent` 调用自动升级为 `RequireApproval`；`Local` 下即使挂了
    审批 provider 也从不会被真正询问（每次调用照常 auto-allow）。这意味着
    修复前"`--approve cli --profile local`"这个显式组合本身就是无效的
    (与本模块文档字符串"routes every call through the approval pipeline"
    的既有声明不符)，不只是新默认值的问题。新增
    `RequireApprovalForEveryCall`（`PreToolHook`）无条件对每次调用返回
    `RequireApproval`，作为 pre-hook 挂载在配置了 provider 的场景下，使
    "配置了 provider"和"每次调用真正被拦截"在任何 profile 下语义一致。
  - 新增 4 个纯函数单测（`resolve_approve_default`）+ 2 个 CLI 端到端集成
    测试（`agentflow-cli/tests/workflow_dynamic_tests.rs`）：
    `local_profile_without_approve_flag_defaults_to_requiring_cli_approval`
    （不传 `--approve`、`--profile local`，关闭 stdin 使审批读取立即 EOF
    → 确定性 deny，断言看到"Harness approval request"提示且文件未写入）、
    `production_profile_with_explicit_auto_allow_still_executes_unattended`
    （证明显式 `--approve auto-allow` 在 production 下依然优先生效、无需
    交互输入即可执行）。既有 5 个测试（均未传 `--profile`，因此仍走
    `dev` 默认值）保持通过，无监督行为不变。
  - 验收命令：`cargo test -p agentflow-cli --all-features`（全 suite
    green，164 lib + 7 workflow_dynamic_tests，较修复前净增 6 项）、
    `cargo test -p agentflow-harness --all-features`（79+ passed，未受
    影响）、`cargo clippy --workspace --all-features --all-targets --
    -D warnings`（clean）、`cargo run -p xtask -- check-arch`
    （`check-arch: OK`）。

### T2 — 工程化 / CI 治理

- DONE T2.1 新增 agent 质量 eval 的 CI 回归门（evaluation §3 ecosystem gap 1）
  ：`agentflow-agents/src/eval/`（dataset/assertion/runner/pricing）是完整
  的评测框架，但 `.github/workflows/*.yml` 里没有任何 job 调用它——只有 RAG
  eval 有 `quality.yml::rag-eval-smoke` 这样的回归门。**验收标准**：照抄
  `rag-eval-smoke` 的 baseline-compare 模式，新增 `agent-eval-smoke` job：
  跑一个小型、便宜的（Mock provider 或固定 fixture，不依赖真实付费 API）
  agent eval 数据集，和一份 checked-in baseline 比较关键指标（成功率/
  verification pass rate/平均步数），偏离超过阈值时 CI 失败；新数据集/
  baseline 文件放在 `agentflow-agents/eval-data/` 或与 RAG eval 对称的位置。

  **证据**：
  - 发现 `agentflow-agents/eval_datasets/ci_offline/`（dataset.toml +
    cases.jsonl，2 个 hermetic mock-provider 用例）和对应的
    `agentflow-cli/tests/eval_cli_tests.rs` 早已存在（`1c4b23d` P4.4 slice
    3）——评测框架本身完整，缺的确实只是 CI 回归门 + baseline-compare 机制，
    与 evaluation 报告描述的 gap 完全一致。
  - `agentflow-agents/src/eval/baseline.rs`（新模块）：`EvalBaseline`
    （`success_rate`/`avg_step_count`/`avg_tool_call_count`，均只统计
    非-skipped 用例）+ `BaselineTolerance`（默认：success_rate 零容忍，
    step/tool_call count 各容忍 +1.0，吸收无关紧要的漂移但不掩盖真实
    回归）+ `compare_against_baseline`——容忍度比较而非 RAG eval 的配对
    显著性检验（smoke 级数据集用例太少，没有 RAG per-query 分布的统计
    功效基础，直接容忍度比较更合适也更简单）。7 个单测。
  - `agentflow-cli/src/commands/eval.rs` + `main.rs`：`agentflow eval run`
    新增 `--compare-baseline <path>`（比较失败时不论 `--fail-on-status`
    取值都以 exit 1 收尾——baseline 回归对 CI 而言始终是硬信号）和
    `--dump-baseline <path>`（从当前跑的报告生成/刷新 baseline 文件，
    两者互斥）。`docs/AGENT_EVAL_FORMAT.md` 早先已经"预写"了
    `--compare-baseline` 的示例命令（未实现时的前瞻性文档），本次补上
    `--dump-baseline` + baseline JSON schema 说明，把文档落到实处。
  - `agentflow-agents/eval_baselines/ci_offline/baseline.json`（新增，
    对称于 `agentflow-rag/eval_baselines/ci_offline/`）：用
    `--dump-baseline` 对 `ci_offline` 跑出的真实 baseline
    （`success_rate=1.0, avg_step_count=3.0, avg_tool_call_count=0.0`），
    不是手写数字。
  - 新增 4 个 CLI 集成测试（`eval_cli_tests.rs`）覆盖
    `--dump-baseline`/`--compare-baseline` 通过/回归失败/互斥校验。
  - `.github/workflows/quality.yml` 新增 `agent-eval-smoke` job（跑
    `eval_cli_tests.rs` + `eval::baseline` 单测 + 对 `ci_offline` 跑
    `agentflow eval run --compare-baseline`，全程 Mock provider，无需
    API key/网络/DB，fork 上也能跑绿），并加入 `release-gate` 的
    `needs`/校验列表（呼应 `rag-eval-smoke` 已有的接线方式）。
  - 验收命令：`cargo test -p agentflow-agents --lib eval`（49 passed，
    含新增 7 项）、`cargo test -p agentflow-cli --test eval_cli_tests`
    （18 passed，含新增 4 项）、`cargo test -p agentflow-cli
    --all-features`（全 suite green）、`cargo clippy --workspace
    --all-features --all-targets -- -D warnings`（clean）、`cargo run -p
    xtask -- check-arch`（`check-arch: OK`）、`python3 -c "import
    yaml; yaml.safe_load(open('.github/workflows/quality.yml'))"`
    （YAML 语法校验通过）。
- DONE T2.2 新增 `agentflow restore` 命令与 `agentflow backup` 配对
  （evaluation §4"数据生命周期"）：`agentflow backup`
  （`agentflow-cli/src/commands/backup.rs`）真实可用，但完全没有对应的
  restore 命令；`docs/SERVER_BACKUP_RESTORE.md` 目前把 backup manifest
  描述为"未来 restore 命令要消费的契约"，今天要求运维手动
  `pg_restore`/`tar -xzf`。**验收标准**：新增 `agentflow restore <backup-dir>
  --dry-run` 命令，消费 backup 产出的 manifest，恢复 Postgres（`pg_restore`）
  + run_dir/trace_dir/marketplace cache 三类有状态数据；`--dry-run` 模式
  打印将要执行的恢复步骤而不实际操作（对齐 `backup`/`cleanup` 已有的
  dry-run 惯例）；至少一个集成测试覆盖"backup 然后 restore 到一个全新
  Postgres 实例，数据一致"的端到端场景；`docs/SERVER_BACKUP_RESTORE.md`
  更新为描述真实可用的 restore 命令而非"未来契约"。

  **证据**：
  - `agentflow-cli/src/commands/backup.rs`：把 `resolve_include_dir` /
    `which_in_path` / `redact_url` / `exit_label` 四个私有 helper 提升为
    `pub(crate)`，供 restore.rs 复用，而不是重复实现同一份路径解析 /
    PATH 探测逻辑。
  - `agentflow-cli/src/commands/restore.rs`（新增，~660 行）：
    `agentflow restore <input>` 读取 bundle 的 `manifest.json`，按固定顺序
    （`Db → MarketplaceCache → SkillsDir → PluginsDir → TraceDir →
    RunDir`，与 `docs/SERVER_BACKUP_RESTORE.md` 记录的恢复顺序一致，
    与 `--include`/manifest 自身顺序无关）逐个还原：`pg_restore --clean
    --if-exists --no-owner --no-acl` 处理 `db.dump`，`tar -xzf` 处理四类
    目录 tarball。支持 `--database-url`、`--dry-run`、`--force`、
    `--include`（重复）、`--format text|json|json-envelope`，报告结构
    （`RestoreReport`/`RestoreStepReport`）对称于 `backup.rs` 的
    `BackupReport`。manifest 里没有的 include 记为 `skipped` 而非
    `failed`；`manifest_version` 不匹配当前二进制直接拒绝执行。8 个单测
    覆盖恢复顺序完整性/DB 优先 RunDir 最后/manifest 缺失报错/
    manifest_version 不匹配拒绝/dry-run 不写盘/manifest 缺项 skip 而非
    fail/磁盘缺文件 fail/`--include` 过滤生效。
  - 手动 smoke 测试中发现并修复一处关键 bug：`agentflow backup` 的
    `run_tar_dir` 用 `tar -C <parent> -czf <artifact> <basename>` 把归档
    顶层条目锚定为**源目录自身的 basename**；`restore_dir` 最初实现用
    `tar -C <target 的 parent> -xzf`，会把内容原样解到"源目录同名"路径
    下，而不是恢复目标目录本身——当恢复目标改名（换主机、env override
    改名）时内容根本不会出现在预期路径，`--force` 的存在性检查也因为
    真正的目标目录从未被创建而永远不触发。修复：`restore_dir` 先自己
    `create_dir_all(&target)`，再用 `tar -C <target> --strip-components=1
    -xzf <artifact>` 剥离归档里的顶层路径前缀，使解包内容始终直接落在
    `<target>` 下，与备份时的原始目录名无关。新增回归测试
    `fs_only_backup_then_restore_lands_content_at_differently_named_target`
    复现并锁定这个场景。
  - `agentflow-cli/src/commands/mod.rs` 新增 `pub mod restore;`；
    `agentflow-cli/src/main.rs` 新增 `Commands::Restore` 变体 +
    `RestoreArgs`（positional `input`、`--database-url`、`--dry-run`、
    `--force`、`--include`/`-i`、`--format`）+ 分发逻辑。
  - `agentflow-cli/Cargo.toml` 新增 dev-dependency `sqlx`（runtime-tokio +
    postgres features）：round-trip 集成测试需要在共享的
    `AGENTFLOW_DATABASE_TEST_URL` server 上新建/删除一个独立命名的
    数据库，让 `pg_restore --clean` 这种破坏性操作永远不会碰到其他并行
    测试写入共享测试库的行（沿用 `agentflow-db/tests/migrations.rs` 的
    env-gate 自跳过惯例）。
  - `agentflow-cli/tests/backup_restore_roundtrip_tests.rs`（新增，5 个
    测试）：4 个始终运行（不依赖外部服务）—— 差异化目标目录名下的
    fs-only 端到端恢复（回归 tar 重锚定 bug，同时覆盖 `--force` 的
    有/无两条路径）、`--dry-run` 不写目标目录、`--help` 列出四个文档化
    flag、非 bundle 目录报清晰错误；1 个由 `AGENTFLOW_DATABASE_TEST_URL`
    门控（未设置时 eprintln 自跳过，本地沙箱环境即如此）——backup 后
    销毁 Postgres 行 + run_dir，再 restore，断言两者都正确还原，随后
    drop 掉临时数据库。
  - `docs/SERVER_BACKUP_RESTORE.md`：删除"restore 尚未实现，未来才有"的
    表述，新增 `## agentflow restore (T2.2)` 一节（flags、失败处理语义、
    目标目录"先创建再解包"避免归档路径前缀泄漏的说明），"Restore
    sequencing" 一节改为"该顺序已经内建在 `agentflow restore` 里，这里
    是给手动操作单个 surface 的运维参考"，"Related" 一节补上 restore
    命令条目。
  - 验收命令：`cargo fmt --all`（无残留 diff）、`cargo clippy -p
    agentflow-cli --all-features --all-targets -- -D warnings`（clean）、
    `cargo clippy --workspace --all-features --all-targets -- -D
    warnings`（clean）、`cargo test -p agentflow-cli --all-features`
    （全 suite green，含新增 8 个 restore 单测 + 5 个 round-trip 集成
    测试，其中 Postgres 场景本地自跳过、留给 CI 的
    `postgres:16-alpine` service 真正执行）、`cargo run -p xtask --
    check-arch`（`check-arch: OK`）。
- DONE T2.3 Linux 沙箱 cgroup 叶子回收 + ContainerBackend 只读根文件系统
  （evaluation §5 findings 3-4，合并为一项因为都是 S-track 沙箱的小幅加固）：
  - cgroup 叶子清理：`agentflow-tools/src/sandbox/linux.rs:474-517` 每次
    spawn 创建的叶子 cgroup 从不删除，长期运行主机上会造成无界目录/inode
    增长。验收：`wrap_command`/对应 spawn 路径在子进程 `wait()` 之后尽力
    （best-effort，失败不 panic）`rmdir` 自己创建的叶子 cgroup 目录；新增
    测试证明"多次 spawn 后叶子 cgroup 数量不随调用次数无界增长"。
  - ContainerBackend 只读根文件系统：`agentflow-tools/src/sandbox/
    container.rs:237-296` 没有给容器传 `--read-only` 根文件系统 flag，
    `code_exec` 执行的 LLM 生成代码可以在 `/workspace` 之外任意写入（虽然
    因为 `--rm` + 零网络 + 无 bind mount 而实际影响有限）。验收：两种引擎
    （Apple `container` CLI / rootless Podman）的调用都加上只读根文件系统
    参数，只保留 `/workspace`（或等价的 ephemeral workdir）可写；新增/更新
    `code_exec_linux.rs`/`code_exec_macos.rs` 测试证明"尝试写 `/workspace`
    之外的路径会失败"。

  **证据**：
  - 本机（macOS/Darwin）无法编译 `agentflow-tools/src/sandbox/linux.rs`
    （`#[cfg(target_os = "linux")]` 整模块 gate），交叉编译又卡在
    `openssl-sys` 找不到 sysroot；改用本机已装的 Apple `container` CLI
    拉起真实 `rust:1.90` Linux 容器，把仓库挂载进去跑
    `cargo check`/`cargo clippy`/`cargo test`，拿到跟原生 Linux 一致的
    真实编译期 + 运行期验证，而不是"改完盲提交，指望 CI 兜底"。
  - `agentflow-tools/src/sandbox/linux.rs`：`LinuxSeccompBackend` 新增
    `tracked_leaf_cgroups: Mutex<Vec<PathBuf>>` 字段；`setup_cgroup_for_
    spawn` 拆成模块级自由函数（改为同时返回叶子目录路径 `PathBuf` +
    `cgroup.procs` 的 `CString`）和一个新的同名 `&self` 方法——方法每次
    创建新叶子前，先对已跟踪的旧叶子跑一遍 `sweep_removable_leaf_cgroups`
    （`retain` + 逐个尝试 `rmdir`，成功即从跟踪列表里剔除），再把新叶子
    push 进去。`wrap_command` 里的调用点从模块级函数换成 `self.` 方法。
    由于 `wrap_command` 没有子进程 `wait()` 之后的钩子（cgroup v2 的
    `rmdir` 只有在叶子已清空——子进程退出、被自动迁出 `cgroup.procs`——
    才会成功），选择"每次新 spawn 顺带扫一遍上次的"而不是引入新的
    trait 方法/生命周期钩子，跟踪列表的大小因此只随"上次退出之后、
    这次 spawn 之前"这一个时间窗口扩张，不会无界增长。
  - `sweep_removable_leaf_cgroups` 是纯文件系统操作（只调用
    `std::fs::remove_dir`），因此可以脱离真实 cgroup 基础设施单测：新增
    `tracked_leaf_cgroups_do_not_accumulate_once_each_becomes_removable`
    （模拟 5 次"上一个叶子已清空"的 spawn，断言跟踪列表长度始终为 1，
    不随调用次数增长）、`tracked_leaf_cgroups_keep_a_still_populated_
    leaf_until_it_empties`（用一个内部有文件的目录模拟"仍有成员进程"
    的叶子，断言 sweep 不会误删；文件消失后下次 sweep 才会真正回收并
    从磁盘删除该目录）。真实 cgroup v2 内核语义仍由 `tests/
    sandbox_linux.rs` 里既有的、门控在真实硬件上的集成测试覆盖，这两个
    新单测只验证 retain/eviction 记账逻辑本身。
  - `agentflow-tools/src/sandbox/container.rs`：`wrap_command` 给两种
    引擎都加上 `--read-only`（Apple `container` CLI 与 Podman 用的是
    同一个 docker 兼容 flag 名，不需要按引擎分支）。真机验证
    （`container run --read-only -v ... --network none --uid 1000
    python:alpine ...`）：普通 `python:alpine` 计算脚本在只读根文件系统
    下无需任何改动即可正常运行（stdlib 字节码已在镜像里预编译），写
    `/etc` 下的文件报 `OSError`，写 `/workspace` 下的文件仍然成功。
  - `agentflow-tools/tests/code_exec_macos.rs` 新增
    `code_exec_root_filesystem_is_read_only_outside_workspace`（真实驱动
    `CodeExecTool` 端到端，断言写 `/etc` 失败、写 `/workspace` 成功）；
    `agentflow-tools/tests/code_exec_linux.rs` 新增同名镜像测试（针对
    Podman 路径，本机无 Podman 时按既有 `cgroup_delegation_available()`
    惯例自跳过）。
  - 验收命令（均在真实 Linux 容器 + 本机 macOS 双重跑通）：`cargo check
    -p agentflow-tools --all-features --lib`（Linux 容器内，clean）、
    `cargo clippy -p agentflow-tools --all-features --all-targets --
    -D warnings`（Linux 容器 + macOS 本机，均 clean）、`cargo fmt -p
    agentflow-tools`（无残留 diff）、`cargo test -p agentflow-tools
    --all-features`（macOS 本机，全 suite green，含新增 2 个 cgroup
    单测 + 2 个 read-only rootfs 集成测试，`code_exec_macos.rs` 11
    个测试全部针对真实 Apple `container` CLI 跑通，非 mock）、`cargo
    clippy --workspace --all-features --all-targets -- -D warnings`
    （clean）、`cargo run -p xtask -- check-arch`（`check-arch: OK`）。
- DONE T2.4 文档滞后清理（evaluation §1 finding 3 + §2 finding 4，合并为一
  项文档 sweep）：
  - `docs/ARCHITECTURE_DIAGRAM.md`、`docs/ARCHITECTURE_EVALUATION_2026-06-20.md`
    的 "2/8 条依赖律" 文案落后于实际（`xtask check-arch` 已激活第三条
    kernel-isolation law，见 R1.2），更新为 "3/8"。
  - `RoadMap.md:271-276`、CLAUDE.md 里"agentflow-worker 仅支持
    template/file 节点 payload，llm/http/mcp/agent 由 P2.8 跟踪"的表述已经
    落后于代码——`agentflow-worker/src/lib.rs:762-773` 和
    `dispatch_llm_and_agent.rs`/`dispatch_simple.rs` 测试证明四种 payload
    均已 dispatch（`agent` payload 明确注释为 "minimal"，真正的工具分发
    推迟到 P5.5，这个限定要保留）。更新为准确反映当前覆盖范围 + 保留
    P5.5 这一具体的剩余限制。

  **证据**：
  - `cargo run -p xtask -- check-arch` 的真实输出确认 `3 active law(s)`
    （非 2），逐条核对 `xtask/src/main.rs` 里
    `LAW_RUNTIME_ISOLATION`/`LAW_SURFACE_ISOLATION`/`LAW_KERNEL_ISOLATION`
    三个常量，`kernel-isolation` 确认是 R1.2（2026-07-28，L0 契约内核
    落地时）新增——文档claim 与代码现状核对一致后才动笔，而不是照抄
    evaluation 报告里的数字。
  - `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`：在原 "2 of the 8 laws"
    段落后追加一段 "Update (T2.4, post-R1.2)" 说明第三条 law 何时新增、
    指向哪，明确本文件其余部分维持 2026-06-20 当时的时间点快照不做
    整体重写（历史评估文档，不是活文档）。
  - `docs/ARCHITECTURE_DIAGRAM.md`：`xtask` 一行从"8 条依赖律的子集
    （runtime-isolation / surface-isolation）"改为"8 条依赖律中的 3 条
    （runtime-isolation / surface-isolation / kernel-isolation，后者
    R1.2 随 L0 契约内核落地新增）"。
  - worker payload 覆盖：读 `agentflow-worker/src/lib.rs`
    的 `execute_supported_node_payload` 确认实际 dispatch
    `template`/`file`/`mock`/`llm`/`http`/`mcp`/`agent` 共七类（不是文档
    里说的仅 template/file 两类）；`execute_agent_payload` 的真实实现
    确认其工具注册表是空的 `ToolRegistry::new()`——文档要保留的
    "agent payload 工具分发仍是 minimal" 这一限定确有其事，代码里的
    doc comment 原样标注"tracked under P5.5 worker admission"（P5.5 编号
    虽然在别处已经用于"worker 认证/admission" 且已 closed，但这是源代码
    注释自身的既有措辞，不在本次 docs sweep 范围内修正，只是原样保留
    以免引入新的不一致）。
  - `RoadMap.md`（Later Tracks → Distributed Execution）：把"扩展
    worker 可执行 node 类型…tracked under P2.8"的待办语气改写为"已完成
    （P2.8 closed）"陈述句，列出实际的七类 payload + 指向
    `execute_supported_node_payload` 的代码位置；"worker 认证/资源限制/
    failure-domain" 一项同样从"待做"改写为"已完成（P5.5–P5.7 closed）"
    ——`docs/archive/PROJECT_EVALUATION_2026-05-19.md` 已记录这两组任务
    在 2026-05 就已 closed，只是 RoadMap.md 本身从未回来同步。
  - `CLAUDE.md` 的 `agentflow-worker` 小节（L4 crate 说明）+ "Distributed
    worker foundation" 状态条目：同步改写为"七类 payload 均已 dispatch
    （P2.8 closed），agent payload 的分布式工具调用仍是唯一剩余缺口"。
  - 纯文档改动，无代码变更；验收命令：`cargo run -p xtask -- check-arch`
    （`check-arch: OK`，`3 active law(s)`，与文档新文案一致）、
    `grep -rn "template/file.*mock\|限定为 template/file"
    docs/*.md CLAUDE.md RoadMap.md`（排除 archive 后确认无残留旧文案）。

### T3 — 完整度缺口

- DONE T3.1 节点级 `timeout_ms`/`max_retries` 从 `mcp`-only 扩展到通用节点
  （evaluation §2 finding 2）：`agentflow-config/src/executor/factory.rs:
  269-284`、`schema.rs:417-422` 目前只有 `MCPNode`
  （`agentflow-nodes-ai/src/nodes/mcp.rs:83,89`）支持 YAML 声明式的逐节点
  timeout/retry；`LlmNode`/`HttpNode`/`FileNode`/`TemplateNode` 没有，而
  `http`/`llm` 恰恰是最容易抖动的节点类型。**验收标准**：先决定设计方向
  ——是把 `timeout_ms`/`max_retries` 提升为 `GraphNode`
  （`agentflow-graph/src/flow.rs:43-50`）层面的通用字段（让所有节点类型
  自动获得），还是逐节点类型单独加（像 MCP 那样）；提升到 `GraphNode` 通用
  字段是推荐方向，因为避免未来每加一个节点类型都要重复这个模式。至少让
  `http`/`llm` 节点类型支持声明式 timeout/retry；更新 schema 校验和文档
  （`docs/CONFIGURATION.md` 或等价文档）反映新的覆盖范围；如果经过设计
  讨论后决定保持现状（例如认为节点级重试应该统一用 `while` 节点包一层而不是
  每个节点类型各自实现），将本项改为 `DEFERRED` 并在
  `docs/OPERATIONS_HANDBOOK.md` §2.1 补充这个设计取舍的说明，而不是让
  文档继续暗示这是一个"待补齐"的缺口。

  **证据**：
  - **设计决策**：没有直接把 `timeout_ms`/`max_retries` 加成
    `agentflow-graph::GraphNode` 的新字段——`GraphNode` 全字段 `pub`、
    全靠 struct literal 直接构造，工作区里有 120 处直接字面量构造点
    （`agentflow-core/src/flow.rs` 自己的测试模块就占 52 处），加字段会
    强制逐一改掉全部 120 处，对一个"节点级 timeout/retry"功能而言
    blast radius 完全不成比例。改用装饰器模式：新增
    `executor::timeout_retry::TimeoutRetryNode`（实现 `AsyncNode`，包一层
    `tokio::time::timeout` + 复用既有但此前从未被调度器实际调用过的
    `agentflow_core::{RetryPolicy, execute_with_retry}`），只在
    `agentflow-config::executor::factory::create_graph_node` 里对
    `NodeType::Standard` 分支按需包装——`GraphNode`/`agentflow-core` 调度器
    零改动，现有 120 个构造点全部不受影响，同时仍然满足"通用、不用每个
    节点类型各自重复实现"的推荐方向：`create_graph_node` 只需在函数尾部
    加一段包装逻辑，不用碰每个 match 分支。
  - **重试安全性**：`RetryPolicy::builder()` 单独调用时
    `retryable_errors` 默认是空 vec，而 `RetryPolicy::is_retryable` 把
    "空列表"解释成"重试一切错误"——第一版实现因此在单测里踩坑（一个
    永远返回 `ValidationError` 的 mock 节点被重试了 6 次而不是 1 次）。
    修复为 `RetryPolicy { max_attempts, ..RetryPolicy::default() }`，
    保留 `RetryPolicy::default()` 自带的"仅网络/超时/限流类错误可重试"
    分类——这也是把 `max_retries` 判定为"可以安全通用应用到所有节点类型
    （包括 `HttpNode` 这种有副作用的 POST/PUT/DELETE 节点）"的关键前提：
    一个明确失败的请求（如校验错误、4xx）不会被盲目重放，只有"看起来可能
    根本没到达对端"的错误类别（超时本身、网络错误、限流）才会重试。
  - `agentflow-config/src/config/v2.rs`：`NodeDefinitionV2` 新增顶层字段
    `timeout_ms: Option<u64>`、`max_retries: Option<u32>`（`run_if` 的
    同级字段，不在 `parameters:` 内——避免与 `mcp` 节点自己的
    `parameters.timeout_ms`/`parameters.max_retries`（只控制 MCP client
    连接，语义完全不同）发生字段名混淆）。
  - `agentflow-config/src/config/schema.rs`：新增校验——`map`/`while`
    节点声明 `timeout_ms`/`max_retries` 时报 issue（这两种节点类型执行的
    是嵌套子流程而不是单个节点，包装语义不明确，拒绝而非静默忽略）。
  - `agentflow-config/src/executor/factory.rs`：`create_graph_node` 计算出
    `node_type` 后，对 `NodeType::Standard` 按需调用
    `timeout_retry::wrap_if_configured`；对 `Map`/`While` + 这两个字段
    非空的情况直接报错（即使调用方跳过 `validate_flow_definition` 直接
    调 `build_flow_from_yaml`，也有同样的兜底拒绝）。
  - `agentflow-config/src/executor/timeout_retry.rs`（新增）：
    `TimeoutRetryNode` + `wrap_if_configured`，5 个单测（zero-config 不
    包装/超时正确触发并报告配置的时长/瞬时错误重试后成功/重试耗尽后报
    `RetryExhausted`/非瞬时错误不重试且只调用一次）。
  - `agentflow-server/src/scheduler/distributed.rs`：测试用的
    `mock_node` 直接构造 `NodeDefinitionV2 { ... }`，补上新增两个字段
    （`None`）——这是全工作区里除 `v2.rs` 结构体定义本身外唯一的
    `NodeDefinitionV2` 字面量构造点，验证了"不碰 `GraphNode`、只碰
    `NodeDefinitionV2`"这个决策确实把 blast radius 控制在了个位数。
  - `agentflow-cli/tests/workflow_tests.rs` 新增 2 个集成测试：
    `cli_workflow_run_dry_run_accepts_generic_timeout_ms_and_max_retries_on_http_and_llm_nodes`
    （`http`/`llm` 节点声明这两个字段，`--dry-run` 构建 Flow 成功，验证
    满足"至少 http/llm 支持"这条硬性验收标准）、
    `cli_workflow_validate_rejects_generic_timeout_ms_on_while_node`
    （`while` 节点 + `timeout_ms` 被 `workflow validate` 拒绝）。
  - `docs/CONFIGURATION.md`：Node fields 表新增 `timeout_ms`/
    `max_retries` 行 + 一段 http/llm 用法示例。
    `docs/WORKFLOW_SCHEMA.md`：新增一段说明这是通用字段、与 mcp 节点自己
    的 `parameters.timeout_ms`/`max_retries` 是两套独立机制，`mcp` 那一行
    也同步改写标注"MCP-client-only"。
  - 验收命令：`cargo fmt --all`（无残留 diff）、`cargo clippy --workspace
    --all-features --all-targets -- -D warnings`（clean）、`cargo test -p
    agentflow-config --all-features`（34 passed，含新增 5 个
    timeout_retry 单测）、`cargo test -p agentflow-server --all-features
    --lib`（188 passed，`distributed.rs` 改动未破坏任何测试）、`cargo
    test -p agentflow-cli --all-features`（全 suite green，含新增 2 个
    集成测试）、`cargo check --workspace --all-features --all-targets`
    （clean，确认 120 处 `GraphNode` 字面量构造点确实零改动零破坏）、
    `cargo run -p xtask -- check-arch`（`check-arch: OK`）。
- DONE T3.2 Workflow YAML `inputs:` schema 块要么实现、要么移除
  （evaluation §2 finding 3）：`agentflow-config/src/config/v2.rs:7-21`
  的 `FlowDefinitionV2.inputs: HashMap<String, InputDefinitionV2>`
  （`description`/`required`/`default` 字段全部标 `#[allow(dead_code)]`）
  解析后完全未被使用——用户声明 `required: true`/`default: ...` 会被静默
  接受、解析、然后丢弃，没有校验也没有默认值填充。**验收标准**：二选一，
  不要保持现状：(a) 实现真正的校验——`agentflow workflow validate`/
  `build_flow_from_yaml` 在缺少 `required` input 时报错、在缺省时应用
  `default` 值填充到初始 `FlowValue` 池；或 (b) 如果决定这个功能不值得
  实现，直接删除 `inputs:` 字段解析和 schema 声明，避免用户依赖一个实际
  不生效的声明。两个方向都需要更新 `docs/CONFIGURATION.md`（如果该文档
  提到过 `inputs:`）并新增/更新对应测试。

  **证据**：
  - **选择方向 (a)（实现）**：`initial_inputs` 的真正来源是运行时的
    `--input`（CLI）或（server 侧目前压根没有 ad-hoc input 机制），跟
    `agentflow workflow validate`（纯 schema 静态检查，不接受
    `--input`）根本不是同一个阶段——所以"必填校验"落地在实际拿到
    `initial_inputs` 的地方（`workflow run`、server 的 `flow_execute`），
    而不是 `validate`。`agentflow workflow debug` 不执行节点、也不接受
    `--input`，不需要改。
  - `agentflow-config/src/config/v2.rs`：`InputDefinitionV2.required`
    补上 `#[serde(default)]`（原来没有任何 default，YAML 里省略
    `required` 会直接解析报错"missing field `required`"——用真实反序列化
    实验验证过；`description`/`default` 因为是 `Option<T>` 本来就已经
    在缺省时解析为 `None`，不受影响），使"纯文档、不填 required"的
    用法可用。字段文档补充说明各自行为，`required`/`default` 去掉
    `#[allow(dead_code)]`（现在真的被消费了）；`description` 保留
    `#[allow(dead_code)]`，因为它确实仍然只是文档用途、不参与任何校验。
  - `agentflow-config/src/executor/mod.rs`（新增
    `pub fn apply_declared_inputs(flow_def, external_inputs)`）：对每个
    声明的 input——已经在 `external_inputs` 里的跳过（调用方显式提供的值
    永远优先于 `default`）；否则若有 `default` 就填进去；否则若
    `required` 就报错并点名缺失的 input 名字；否则（可选、无 default）
    什么都不做。5 个单测覆盖：default 填充/调用方值不被覆盖/缺失
    required 报错/缺失可选字段保持不存在/`required` 省略时默认为
    `false`。
  - `agentflow-cli/src/commands/workflow/run.rs`：把 `parse_inputs`
    的调用挪到 `dry_run` 分支判断之前，紧接着调用
    `apply_declared_inputs`——这样 `--dry-run` 和真正执行都会在节点真正
    跑之前，把"缺 required input"这个错误暴露出来，而不是让它在下游
    某个节点 `input_mapping` 解析失败时报一个不知所云的错误。
  - `agentflow-server/src/runs.rs`：`flow_execute` 从
    `build_flow_from_yaml`（内部悄悄丢弃了 parse 出的 `FlowDefinitionV2`）
    改成 `parse_workflow_definition` + `build_flow_from_definition` 两步，
    这样才能同时拿到 `flow_def` 用于 `apply_declared_inputs`；执行时
    传入的 `HashMap::new()` 换成经过 `apply_declared_inputs` 处理过的
    map——server 目前完全没有 ad-hoc input 机制（`--input` 未接入
    `POST /v1/runs`），所以 `default` 填充是 server 侧提交的 run 唯一
    能获得声明输入值的途径；`required` 且无 `default` 的输入会让 server
    提交的 run 直接失败在这里，而不是更晚、更难定位的
    input_mapping 解析失败。
  - `agentflow-cli/tests/workflow_tests.rs` 新增 3 个集成测试：
    `cli_workflow_run_fails_clearly_when_required_input_is_missing`
    （`--dry-run` 下必填 input 缺失即报错并给出清晰信息）、
    `cli_workflow_run_fills_declared_default_when_not_supplied`
    （不传 `--input`，declared default 生效并体现在渲染输出里）、
    `cli_workflow_run_cli_input_overrides_declared_default`（`--input`
    显式提供的值覆盖 declared default）。
  - `docs/CONFIGURATION.md`：`inputs` 字段的说明从"仅描述性"改写为
    准确描述新的强制语义（必填缺省报错、default 自动填充、`--input`
    优先于 default、`required` 省略时默认为 `false`）。
  - 验收命令：`cargo fmt --all`（无残留 diff）、`cargo clippy
    --workspace --all-features --all-targets -- -D warnings`（clean）、
    `cargo test -p agentflow-config --all-features`（39 passed，含
    新增 5 个 `apply_declared_inputs` 单测）、`cargo test -p
    agentflow-cli --all-features`（全 suite green，172 passed 跨全部
    测试二进制，含新增 3 个集成测试）、`cargo test -p agentflow-server
    --all-features --lib`（188 passed，`runs.rs` 改动未破坏任何测试）、
    `cargo run -p xtask -- check-arch`（`check-arch: OK`）。
- DONE T3.3 `agentflow-tools` 按 RFC 拆分为 contract-only + builtin-impl
  两个 crate（evaluation §1 finding 1，架构维度里工程量最大的一项）：
  `docs/RFC_CRATE_ARCHITECTURE.md` §4 原计划把 `Tool`/`ToolRegistry`/
  `ToolMetadata` 拆到独立的 `agentflow-tool` 契约 crate，`ShellTool`/
  `FileTool`/`HttpTool`/`SandboxPolicy` 等具体实现留在
  `agentflow-tools`（builtin）。这个拆分从未发生，是 "law 2/4
  runtime→impl" 系列 latent violation 存在的根本原因（`agentflow-agents`/
  `agentflow-harness` 直接依赖具体 builtin 而非只依赖 trait）。**这是一个
  真实的 crate 拆分工程，不是小补丁**——处理前建议先写一份简短的迁移计划
  （哪些类型搬家、re-export 兼容策略、`cargo xtask check-arch` 的
  `ARCH_KERNEL_CRATES` 列表如何调整），参考 R1.1（`LlmTraceContext` 下沉到
  `agentflow-value`）的拆分手法（保留旧路径的 `pub use` 做零改动兼容）。
  验收标准：拆分后 `agentflow-agents`/`agentflow-harness` 只依赖新的
  `agentflow-tool` 契约 crate 而不再直接依赖 `agentflow-tools`（builtin）；
  `cargo xtask check-arch` 里 "agentflow-agents/harness → tools" 系列
  latent violation 消失或明确改判为"合法窄依赖"；全部既有测试保持通过。
  **如果评估后认为这个工程量在当前阶段不划算，允许改判为 `DEFERRED` 并
  写明理由**，但不要放着不决策。

  **证据**：
  - **迁移计划先行**：动手前先写 `docs/RFC_TOOL_CONTRACT_SPLIT.md`，逐文件
    核查 `agentflow-agents`/`agentflow-harness`/`agentflow-agent-spi` 的
    生产代码（排除 examples/`#[cfg(test)]`）实际 import 了哪些
    `agentflow_tools::` 符号——三个 crate 的生产代码无一例外只用到契约层
    （`Tool`/`ToolRegistry`/`ToolError`/`ToolMetadata`/`ToolIdempotency`/
    `ToolOutput(Part)`/`Capability`/`CapabilityDecisionEntry`/
    `EffectiveCapabilities`/`SandboxStatus`/`SandboxEnforcement`/
    `ToolPermission(Set)`/`ToolPolicy`/`ToolSource`），唯一的具体实现引用
    （`agentflow-harness`'s `hooks_runtime.rs` 用真实 `CodeExecTool` 验证
    生产环境 approval 升级路径）在它自己的 `#[cfg(test)]` 模块里——这个
    发现把"拆分"的实际风险从"可能要解开耦合的业务逻辑"降到了"纯粹的
    文件搬家 + `Cargo.toml`/`use` 路径重指向"。
  - **新 crate `agentflow-tool`**（contract-only，`agentflow-tool/`）：
    `git mv` 过去 `tool.rs`/`capability.rs`/`registry.rs`/`error.rs`/
    `policy.rs`/`plugin_policy.rs`/`security_profile.rs` 共 7 个文件
    （~3450 行）；新增 `sandbox.rs`（`SandboxBackend` trait +
    `SandboxScope`/`SandboxStatus`/`SandboxEnforcement`/`SandboxError`，
    从原 `sandbox/backend.rs` 里"契约"那一半搬过来）。零工作区内部依赖
    （真正的 L0 kernel crate）。
  - **`agentflow-tools`（builtin，不改名）**：`lib.rs` 改为
    `pub use agentflow_tool::{...}` 全量 re-export 加自己的
    `pub mod builtin; pub mod sandbox;`——这就是为什么依赖具体实现的
    8 个下游 crate（`cli`/`server`/`worker`/`skills`/`config`/`nodes`/
    `rag`；`agent-spi`/`agents`/`harness` 除外）**零改动**：`use
    agentflow_tools::{Tool, ToolRegistry, ...}` 原样编译通过。
    `sandbox/backend.rs` 只保留 `default_backend()`（具体平台分发逻辑，
    引用 macos/linux/noop 具体后端，本质是 impl 不是 contract）+
    对应平台专属测试；`sandbox/policy.rs`（`SandboxPolicy`，
    `ShellTool`/`FileTool`/`HttpTool` 自己用的进程内 allow-list）保持
    不动——生产代码里没有任何 runtime/kernel crate 引用它，只有
    `agentflow-agents` 的 examples 用到过。
  - **`agentflow-agents`/`agentflow-harness`/`agentflow-agent-spi` 三个
    `Cargo.toml`**：`[dependencies]` 从 `agentflow-tools` 改成
    `agentflow-tool`；examples-only（`agentflow-agents`）/ 自身测试
    需要真实具体实现（`agentflow-harness` 的 `CodeExecTool` 集成测试）的
    地方新增 `agentflow-tools` 作为 `[dev-dependencies]`。三个 crate
    src 内 `agentflow_tools::` → `agentflow_tool::` 批量替换，逐一手工
    修回少数几处例外（doc comment 示例代码里用到的
    `ShellTool`/`SandboxPolicy` 具体实现引用、`agentflow-agents`
    delegation.rs 测试模块里的真实 `FileTool`/`HttpTool`/`SandboxPolicy`）。
  - **一处真实踩坑**：`agentflow-tool` 最初也想给自己的
    `ToolRegistry` 单测加 `agentflow-tools` 作为 dev-dependency（拿真实
    `ShellTool` 验证注册表/权限收窄逻辑），但这是一条**循环** dev
    依赖（`agentflow-tools` 反过来正式依赖 `agentflow-tool`）——Cargo
    因此在图里产生了两份不同的 `agentflow_tool` 编译单元，导致
    `ShellTool: Tool` 无法满足契约 crate 自己那份 `Tool` trait，报
    "multiple different versions of crate `agentflow_tool` in the
    dependency graph"。修复：移除该 dev-dependency，把这两个测试原样
    搬到 `agentflow-tools/tests/registry_against_real_shell_tool.rs`
    ——那里 `agentflow-tools` 已经正式依赖 `agentflow-tool` 且没有反向
    边，不存在这个问题。
  - `xtask/src/main.rs`：`ARCH_KERNEL_CRATES` 里 `"agentflow-tools"` 换成
    `"agentflow-tool"`；`ARCH_LATENT_EDGES` 删掉
    `agentflow-agents/harness -> agentflow-tools` 两条（已经真正解决，
    不是重新归类）；新增回归测试
    `tool_contract_split_removed_the_tools_kernel_membership_and_latent_edges`
    锁定"kernel 集合含 tool 不含 tools"+"这两条 latent edge 确实消失"。
  - `CLAUDE.md`：workspace crate 数从 22 改为 23；L0 Contract Kernel
    列表、L2 Capability Adapters 列表、`agentflow-tools`/`agentflow-mcp`
    小节同步更新，新增 `#### L0 — agentflow-tool` 小节。
    `docs/ARCHITECTURE.md` 的层次图 `tools (Tool contract)` 改为
    `tool (Tool contract)`。
  - 验收命令（真实跑通，非仅静态检查）：`cargo fmt --all`（无残留 diff）、
    `cargo clippy --workspace --all-features --all-targets -- -D
    warnings`（clean）、`cargo test --workspace --all-features`（155 个
    测试二进制全 green，含新增的
    `agentflow-tools/tests/registry_against_real_shell_tool.rs` 2 项 +
    xtask 新增 1 项回归测试；过程中真实发现并修复了 2 个 doctest 编译
    失败——`agentflow-agents` 的 `react/agent.rs`/`agent_tool.rs` 文档
    示例代码里误把具体实现引用批量替换成了契约 crate 路径）、`cargo run
    -p xtask -- check-arch`（`check-arch: OK`，25 个成员、11 条 latent
    edge，较拆分前少 2 条）。

### T4 — 长期 backlog（低优先级，暂不安排具体执行顺序）

- DONE T4.1 实现长期记忆 Preference / Entity-facts store（evaluation §3
  ecosystem gap 2，对应 `docs/MEMORY_LAYERING.md` 里标注"P4.7 待办"的
  两层）：四层记忆设计（Session/Semantic/Preference/EntityFacts）里只有
  前两层真正实现。工程量较大，暂列入 backlog，不在本轮 T 段强制排期；
  如果决定启动，至少先实现一个最小可用的 SQLite `PreferenceStore`，对齐
  `agentflow-memory` 现有的 `SqliteMemory` 实现风格。

  **证据**：
  - **核查结论：evaluation §3 finding 2 是过期结论，不是真实 gap**——
    `git log --follow` 证实 `SqlitePreferenceStore`
    （`agentflow-memory/src/preference.rs`）和 `SqliteEntityFactStore`
    （`agentflow-memory/src/entity_facts.rs`）早在 `5098719`
    "feat(memory): close P4.7 memory backend implementations"
    （2026-05-24，本轮 2026-07-29 评估之前两个多月）就已经落地并提交，
    验收标准要求的"至少一个最小可用的 SQLite PreferenceStore"不仅已经
    满足，还超额交付：`EntityFactStore` 一并实现、`AgeEncryptedPreference
    Store`（`preference_encrypted.rs`，P10.7.2 静态加密变体）、
    `agentflow memory prune --layer preference|entity_facts --older-than
    <dur>` CLI 命令（`agentflow-cli/src/commands/memory/prune.rs`）均已
    存在且有测试覆盖。评估报告显然是读了
    `docs/MEMORY_LAYERING.md` 里"not yet implemented"的过期表述，未核对
    实际代码——本项因此不需要新增任何 Rust 实现，真正的行动项是修正
    这份被 evaluation 引用为证据的文档本身。
  - `docs/MEMORY_LAYERING.md`：§3/§4 "Today's implementation" 从
    "not yet implemented, land under P4.7" 改写为指向真实实现文件 +
    落地 commit（`5098719`）；trait 代码示例补上此前遗漏的
    `list_preferences`/`prune_older_than`/`prune_invalidated` 方法，
    修正 `put_preference` 的入参类型（真实签名是 `serde_json::Value`
    而非文档写的 `PreferenceValue`）；retention 命令示例改为真实 CLI
    flag（移除文档臆造的、代码里根本不存在的 `--hard-delete` flag）。
  - **同时发现并修正了文档另一侧的过度声明**：`docs/MEMORY_LAYERING.md`
    "Migration path" 一节的 `skill.toml` 示例（`[memory.preference]`/
    `[memory.entity_facts]` 子表）写得像是已经随 P4.7 落地——但核查
    `agentflow-skills/src/loader.rs` 的 `KNOWN_MEMORY_TYPES` 常量确认
    `[memory].type` 只接受 `session`/`sqlite`/`none` 三种，且
    `agentflow-agents` 生产代码里没有任何地方引用
    `PreferenceStore`/`EntityFactStore`（`grep` 验证为空）——`store` 类型
    本身可用，但 SkillBuilder 声明式接线 + agent 运行时 prompt-assembly
    自动读取这两层都尚未实现。改写为明确标注"proposed，尚未实现"，
    避免文档反向误导为"已完成"。同时在文档头部 Status 行 + "Precedence
    at prompt-assembly time" 一节后补充"Wiring status"说明，把
    "store 已存在可独立使用"和"agent 运行时尚未自动接线"两件事分开
    表述清楚。
  - 纯文档改动，无代码/测试变更（既有实现本身已有完整单测覆盖，见
    `preference.rs`/`entity_facts.rs` 内 `#[cfg(test)]` 模块）。
  - 验收命令：`grep -rn "PreferenceStore\|EntityFactStore"
    agentflow-agents/src`（空，确认"尚未接线"的判断）；
    `cargo doc -p agentflow-memory --no-deps`（文档引用的源文件路径均
    存在，无死链接）。
- DEFERRED T4.2 首方 OTLP exporter（HTTP/gRPC + TLS + 认证）（evaluation §4
  "Tracing/可观测性"，即 Q2.3.3，此前已多次评估中记录为 deferred）：继续
  保持 deferred 状态，operator 自带 `OtelSpanSink`
  实现仍是当前推荐路径；本项只是把它正式纳入 T 段 backlog 视野，不改变
  优先级。

  **理由**：本项的验收标准本身就是"继续保持 deferred"——不存在一个
  "TODO 未开始"的待实现工作项，只是把既有的 Q2.3.3 deferred 决定正式
  收纳进 T 段 backlog 视野以便追踪，标成 `TODO` 会误导为"这轮要做"。
  `docs/CURRENT_STATUS.md`/`CLAUDE.md`/`docs/audit/agentflow-tracing.md`
  M3 已经记录这是 deferred 状态，不需要额外代码或文档改动；本次只是
  TODOs.md 自身的状态标注更正（从 `TODO` 改为 `DEFERRED`），无可提交的
  代码/文档 diff。如果未来决定启动实现，届时改回 `TODO` 并按常规流程
  执行。
- DONE T4.3 Helm chart 补默认资源 request/limit + HPA 模板（evaluation §4
  "部署/运维"）：`charts/agentflow/values.yaml` 当前 `resources: {}` 留空，
  且没有 HPA/PodDisruptionBudget 模板，`replicaCount: 1` 是唯一副本数假设。
  验收标准：`values.yaml` 提供合理的默认 CPU/内存 request/limit（可覆盖）；
  新增可选的 HPA 模板（`autoscaling.enabled` 开关，默认关闭以保持向后
  兼容）；`docs/DEPLOYMENT.md#helm` 补充这些新字段的说明。

  **证据**：
  - `charts/agentflow/values.yaml`：`resources: {}` 替换为具体默认值
    （`requests`: 100m CPU / 128Mi memory；`limits`: 500m CPU / 512Mi
    memory），可通过 `--set resources.requests.cpu=...` 等覆盖；新增
    `autoscaling` 块（`enabled: false` 默认关闭 + `minReplicas`/
    `maxReplicas`/`targetCPUUtilizationPercentage`，`
    targetMemoryUtilizationPercentage` 默认不设置，留给用户按需
    `--set` 追加）。未新增 PodDisruptionBudget 模板——验收标准只要求
    HPA，PDB 只在 finding 的问题描述里提及，不在验收标准内，避免范围
    蔓延。
  - `charts/agentflow/templates/hpa.yaml`（新增）：`autoscaling.v2`
    `HorizontalPodAutoscaler`，`{{- if .Values.autoscaling.enabled }}`
    包裹整个资源（关闭时模板不产出任何 manifest）；CPU/memory 两个
    utilization 指标各自独立 `if`，未设置 memory 阈值时只产出 CPU 指标。
  - `charts/agentflow/templates/deployment.yaml`：`spec.replicas` 包一层
    `{{- if not .Values.autoscaling.enabled }}`——HPA 启用时 Deployment
    不再声明副本数，避免 `helm upgrade` 每次把 HPA 调整过的副本数摁回
    `replicaCount`（标准 Helm HPA 接线惯例）。
  - 验收命令：`helm lint charts/agentflow`（0 chart(s) failed，仅一条
    "icon is recommended" 提示，跟本次改动无关）；
    `helm template test charts/agentflow --set existingSecret=agentflow-db`
    渲染出的 Deployment 含新默认 `resources.requests`/`resources.limits`
    且带 `replicas: 1`；`helm template test charts/agentflow --set
    existingSecret=agentflow-db --set autoscaling.enabled=true --set
    autoscaling.targetMemoryUtilizationPercentage=70` 渲染出
    `HorizontalPodAutoscaler`（CPU 80% + memory 70% 两个指标）且
    Deployment 里不再出现 `replicas:` 字段——两条命令均本机真实跑通
    （`helm v4.2.3`），非仅静态检查模板语法。
  - `docs/DEPLOYMENT.md`：`## Helm` 一节新增 "Resource requests/limits and
    autoscaling (T4.3)" 小节，说明默认值、启用 HPA 的命令示例、
    `replicas` 字段在 HPA 启用时被省略的原因、memory 指标默认不设置
    需显式 `--set` 追加。

## Recently Closed

- **2026-07-30 — T 段全部闭环（14 DONE / 1 DEFERRED，15/15 已决策）**：
  T4.3（Helm chart `values.yaml` 默认 CPU/内存 request/limit + 可选 HPA
  模板，`commit 9babf10`）、T4.2（首方 OTLP exporter 正式改判
  `DEFERRED`，验收标准本身就是"继续保持 deferred"，无代码/文档改动）、
  T4.1（核查后发现 evaluation §3 finding 2 是过期结论——
  `SqlitePreferenceStore`/`SqliteEntityFactStore` 早在 2026-05-24
  `5098719` 就已随 P4.7 落地并测试覆盖，真正的行动项是修正
  `docs/MEMORY_LAYERING.md` 里"not yet implemented"的过期表述，
  `commit 81cda0f`）依次收口。至此
  T0–T4 共 15 项全部决策完毕，暂无新的执行队列；下一个 backlog 待新一轮
  评估或用户指派后开启。
- **2026-07-29 — R 段整体存档 + 启动 T 段**：R 段（2026-07-28 工程化审计
  修复，R0–R4 共 12 项 + R4 追加发现的 8 项）已在 GitHub Actions 真实硬件上
  验证 `release gate: conclusion=success`，收口前的完整历史存档到
  [`docs/archive/TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md`](docs/archive/TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md)。
  同日基于五维度独立架构评估（`docs/archive/PROJECT_EVALUATION_2026-07-29.md`）
  规划新的 T0–T4 共 15 项待办队列。

> 7/29 之前的 Recently Closed 全部归档在上述历史快照 + 更早的
> [`docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md`](docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md)
> 和
> [`docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md`](docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md)。

---

## Deferred / Explicit Non-Goals

（沿用 5/20 版本，无变化；T4 段的三项属于"暂不排期的 backlog"，与下面的
显式 non-goal 性质不同，不重复记录在此处。）

- DEFERRED Channel adapters: Slack, Telegram, Discord, email, webhook routers,
  desktop tray, and multi-channel message normalization.
- DEFERRED Local OS control tools: screenshot, keyboard, mouse, clipboard,
  window-management.
- DEFERRED Full SaaS productization: organization management, billing, hosted
  multi-user UI, OAuth/JWT, background Skill updates, channel routing.
- DEFERRED Native dynamic library plugins: subprocess JSON-RPC 仍是唯一 v1
  plugin runtime。
- DEFERRED P-H.H6 Harness advanced compatibility: promoted to RoadMap Later
  Tracks。
- DEFERRED（H.6/H.7/H.8 归档明细）服务端多节点共享 memory backend / `--skill`
  路径持久化统一 / slash-command 生态·TUI 产品壳·OpenHarness 配置导入·第三方
  agent 框架适配器——理由与出处见
  `docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md`
  H.6–H.8 段。

---

## Execution Notes

- **T0 优先级硬性**：T0 全部 DONE 之前，不应该认为多租户/marketplace
  安装路径已经达到生产安全基线。T1/T2/T3 不阻断，但 T1（生产健壮性护栏）
  应尽快跟进——尤其 T1.2 是"分布式部署形态能否端到端跑通"的前提。T4 是
  低优先级 backlog，不需要在近期强制排期。
- 每个 T-item 完成后引用 `docs/archive/PROJECT_EVALUATION_2026-07-29.md`
  对应章节 + 本文件里的验收标准逐条对照，不要只写"已修复"三个字。
- 一次只挑一个 T-item；不要在同一 PR 里混不同 crate 的修复。
- 每个 fix 必须配至少一个 regression test 证明问题不会复现；涉及默认值
  变更的（T0.1/T1.1/T1.3）额外要求一个"验证旧默认值行为不再发生"的测试。
- Commit message 引用 task ID：`Refs T0.1`。
- 涉及设计取舍的项（T3.1/T3.3）在动手实现前先确认方向，允许改判为
  `DEFERRED` 但必须写明理由，不要放着不决策。
- T-item 完成后将状态从 `TODO` 改成 `DONE` 并简述 fix + 测试（参照本文件
  历史归档中 R 段 DONE 项的写法：证据段落 + 验收命令输出）。

---

## Quality Gates

每个 task：

- 先读相关代码 + `docs/archive/PROJECT_EVALUATION_2026-07-29.md` 里该项
  引用的证据（文件:行号）。
- 实现最小可行修复。
- 跑聚焦的 regression test + crate 全测。
- Conventional commit 提交：`fix(scope): ...` / `feat(scope): ...` /
  `refactor(scope): ...` / `docs(scope): ...`。
- 提交成功后再把 TODO 改成 DONE。

Pre-commit workspace 命令：

```bash
cargo fmt --all
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace
```

---

## Cross-References

- `docs/archive/PROJECT_EVALUATION_2026-07-29.md` — **本次 T 段的评估依据**：
  五维度独立审计（架构分层 / 模块完整度 / Agent 生态 / 服务层 / 安全性）+
  跨维度优先级表（§6）。
- `docs/archive/TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md` — 7/29
  全量快照：R 段（12+8 项）收口前的完整历史明细。
- `docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md` —
  7/28 全量快照：H/P-A/S/L 四段收口前的完整历史明细。
- `docs/CURRENT_STATUS.md` — 当前已实现状态。
- `RoadMap.md` / `docs/ROADMAP_v2.md` — 中长期方向。
- `docs/STABILITY.md` / `docs/API_COMPATIBILITY.md` — 稳定面契约。
- `docs/RFC_CRATE_ARCHITECTURE.md` / `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`
  — 八条依赖铁律定义 + 上一轮架构评估（T3.3 涉及的铁律来源）。
- `docs/DISTRIBUTED.md` — 分布式部署形态说明（T1.2 涉及的现状描述）。
- `docs/MEMORY_LAYERING.md` — 四层记忆设计（T4.1 涉及的 P4.7 待办来源）。
- `HARNESS_MODE_EVOLUTION.md` — Harness Mode 设计规范。
- `docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md` — 上一轮
  深度审计修复波次（108 DONE）。
- `docs/archive/TODOs-archive-2026-05-24-p10-optimization-backlog.md` —
  P10 优化 backlog 全部 DONE 项 + 少量 polish 未拾起。
- `docs/archive/TODOs-archive-2026-05-20-closed-segments.md` — 12 个全 closed
  P-段（P0–P9 + P-H + P-LLM + M）。
- `docs/archive/TODOs-archive-2026-05-19-recently-closed.md` —
  5/19 扫出的中段历史。
- `docs/archive/TODOs-archive-2026-05-09-n1-n10.md` + `...05-10-p0-p4.md` —
  N 系列 + 早期 P 系列执行计划历史。
