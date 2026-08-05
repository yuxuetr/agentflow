# AgentFlow TODOs

Last updated: 2026-07-30

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
  - `TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md` — 7/29 全量快照：
    R 段（2026-07-28 工程化审计修复）收口后、启动 T（2026-07-29 架构评估
    修复）段之前的完整存档。
  - `TODOs-archive-2026-07-30-post-t-pre-u-snapshot.md` — **本次 7/30
    快照**：T 段（2026-07-29 架构评估修复，T0–T4 十五项，14 DONE /
    1 DEFERRED）全部决策完毕收口后、启动新 **U（2026-07-30 复核评估修复）**
    段之前的完整存档（含全部历史明细）。本文件即从这份快照重建。
- 本文件是短期执行队列。H / P-A / S / L / R / T 六段已全闭环并整体存档；
  当前仅保留 **U（2026-07-30 复核评估修复）** 一个 backlog。
- 本次 U 段来源：`docs/archive/PROJECT_EVALUATION_2026-07-30.md`——对 T 段
  修复效果的复核评估（五个独立维度并行审计，以 7/29 评估为基线逐条验证
  是否真正闭环，并独立挖掘新问题），综合评级 **A-（有条件）**（上一版
  7/29 为 B+）。核心结论：**过去 24 小时是一次真正的"闭环冲刺"**——7/29
  报告清单里 13 条发现中 9 条被验证真正解决，且验证方式扎实（编译通过、
  专门回归测试、端到端集成测试全部实测，而非只读 commit message）。但
  **同一窗口的新代码里又长出两个新的 P0 级安全问题**（`agentflow restore`
  命令删除目标目录的顺序 bug、Helm/docker-compose 默认部署档位问题）——
  说明"把已有机制接成默认路径"这件事进步很快，但"新写的接线代码本身的
  安全审查"还没跟上同样的节奏。U 段按 evaluation 报告 §6 的跨维度优先级表
  转化为可执行任务，编号延续该表的优先级分组（U0 对应 P0，以此类推）。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态。
- `RoadMap.md` 保留中长期路线。
- `docs/archive/PROJECT_EVALUATION_2026-07-30.md` 是本轮 U 段的评估依据，
  每个 U-item 下方引用其对应章节。
- 任务状态只使用：
  - `TODO`：未开始或正在执行。
  - `DONE`：已完成、已测试、已提交。
  - `DEFERRED`：显式推迟到 RoadMap Later Tracks 或 Non-Goals。

## Active Queue Overview

Current focus: **H / P-A / S / L / R / T 六段已全闭环并整体存档**（见上）；
新的 **U（2026-07-30 复核评估修复）** U0–U4 共 **17 项**（U2.1 复核时
拆出 U2.5，U2.5 完成时又拆出 U2.6——**16 DONE / 0 TODO / 1 DEFERRED，
U 段全部收口**），按 `docs/archive/PROJECT_EVALUATION_2026-07-30.md`
§6 的跨维度优先级排序：
**U0（阻断性安全，新引入的破坏性 bug）2 项，全部 DONE**、
**U1（生产健壮性/隔离护栏）3 项，全部 DONE**、
**U2（架构/生态债务）6 项，全部 DONE（U2.1 harness 半 / U2.2
preference 层 / U2.3 --approve 默认值 / U2.4 文档修正 / U2.5
ProjectFact 契约抽取 / U2.6 PreferenceStore 契约抽取 + `&self` 重设，
`agents -> agentflow-memory` 边本身因 `dynamic.rs` 的 `SessionMemory`
默认值仍无法完全闭合，已确认接受并写清楚理由）**、
**U3（完整度/卫生）4 项，全部 DONE（U3.1 第一阶段 / U3.2 Helm PDB /
U3.3 架构文档同步 / U3.4 gRPC worker TLS 警告）**、
**U4（长期 backlog，低优先级）2 项，全部 DONE/DEFERRED（U4.1 已文档化 /
U4.2 DEFERRED，延续 T4.2 的既有决定）**。**U0/U1/U2/U3/U4 五段均已
全部收口**——`agentflow
restore` 的顺序 bug（U0.1）与缺失的制品完整性校验（U0.2）均已修复；
跨租户身份绑定（U1.1，五轮评估里唯一从未被处理过的 P0/P1 级发现）、
Helm/docker-compose 默认部署档位（U1.2）、成本熔断 CLI/Server 配置入口
（U1.3）均已闭环。U2.1 动手时发现 evaluation 原文对 `agentflow-agents`
半的耦合面描述已过期（真实缺口是 `ProjectMemoryStore`/`ProjectFact`
缺少 `store-spi` 契约，不是"默认值构造"），拆成 harness 半（DONE）+
agents 半（新条目 U2.5）。U2.2 缩小范围为 preference 层（读+写全链路
+ `remember_preference` 工具），entity_facts 显式推迟；U2.4 作为
U2.2 的副产品一并解决；U2.3 修复 `harness run`/`chat` 的 `--approve`
默认值（核实后发现"resume"其实无需改动，真正的姊妹缺口在 `chat`）。
U4.1 讨论后决定不改 `workflow dynamic` 的默认 profile（避免打断依赖
裸命令行无监督迭代的现有脚本），仅醒目文档化。U2.5 动手时再次发现
验收标准（把 `agentflow-agents/Cargo.toml` 主依赖从 `agentflow-memory`
改为 `agentflow-store-spi`）已因 U2.2 引入的 preference 具体类型字段
而不可达成——只完成了 `ProjectFact` 契约抽取，新开 **U2.6** 跟踪
`PreferenceStore` 那一半。U2.6 深入核实两个 `PreferenceStore` 实现后
发现`&mut self` 约束其实并不成立，彻底重设为 `&self` 并完成契约抽取；
但过程中又发现第三个、性质不同的阻塞点——`dynamic.rs` 的
`DynamicWorkflowAgent` 生产代码直接构造具体的 `SessionMemory` 作为
默认 memory backend，这是设计上的具体实现需求而非契约缺口，用户确认
接受、不再新开条目。**U 段整体（17 项：16 DONE + 1 DEFERRED）已全部
收口**，`agents -> agentflow-memory` 边留存但已在 `xtask`/架构文档里
写清楚真实原因，是否要为 `DynamicWorkflowAgent` 的可注入 memory 开新
条目留给下一轮评估判断。

| Segment | Theme | Status |
| --- | --- | --- |
| N1 → N10 / P0 → P9 / P-H / P-LLM / M / P10 | 历史段，全部 closed 或外迁 | ARCHIVED |
| Q1 → Q5 | 2026-05-24 深度审计修复波次，108 DONE | ARCHIVED |
| H | Harness Mode follow-ups（loop-ownership + `harness chat` 收尾） | **DONE — archived（7/28）** |
| P-A | 契约内核 + 架构演进（`docs/RFC_CRATE_ARCHITECTURE.md`） | **DONE — archived（7/28）** |
| S | 沙箱与代码执行安全演进（`code_exec` / OS sandbox 强化） | **DONE — archived（7/28）** |
| L | 长程任务与检索增强（replan / 项目记忆 / RAG 补强 / 委托契约） | **DONE — archived（7/28）** |
| R | 2026-07-28 工程化审计修复（CI 覆盖率 / 架构守卫盲区 / 文档陈旧 / 仓库卫生） | **DONE — archived（7/29）** |
| T | 2026-07-29 架构评估修复（五维度独立审计：架构 / 完整度 / Agent 生态 / 服务层 / 安全） | **DONE — archived（7/30），15 项（14 DONE / 1 DEFERRED）** |
| U | 2026-07-30 复核评估修复（对 T 段修复效果的独立复核 + 新发现） | **DONE — 全部收口（2026-08-01），17 项，含 U2.1 拆出的 U2.5 与 U2.5 完成时又拆出的 U2.6（16 DONE / 1 DEFERRED）** |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## U — 2026-07-30 复核评估修复（re-audit remediation）

> 来源：`docs/archive/PROJECT_EVALUATION_2026-07-30.md`（五个独立 agent 并行
> 复核：架构分层、模块完整度与职责单一性、Agent 框架/工程化理念、服务层/
> 部署/数据管理、安全性；以 `docs/archive/PROJECT_EVALUATION_2026-07-29.md`
> 逐条发现为基线，验证是否闭环，并独立挖掘新问题，互不共享结论）。**核心
> 结论**：T 段 15 项里 T0/T1/T2/T3 全部经实证验证真正闭环（非仅读 commit
> message），但同一窗口新增的代码本身又带来两个新的 P0——**这不是历史债务
> 复发，是新功能上线时的安全审查缺口**。排序原则：**U0 是唯一涉及"新引入
> 的破坏性/安全 bug"的阻断性分组**，成本极低（U0.1 是一行代码顺序调整）
> 但影响极高，必须最先处理。U1 是标准生产健壮性/隔离缺口（部分是 7/29
> 报告就点名、T 段未覆盖的老问题——尤其跨租户隔离——部分是 U0 相关功能
> 的配套加固）。U2 是架构/生态维度里"修复路径已经清楚、成本较低"的债务。
> U3 是完整度/文档卫生类。U4 是长期 backlog。每项修复需配 regression
> test；涉及默认值变更的需要同步更新对应文档。

### U0 — 阻断性：新引入的破坏性/安全 bug

- DONE U0.1 修复 `agentflow restore` 删除目标目录先于安全护栏检查的顺序
  bug（**CRITICAL**，evaluation §5"新发现"）：
  `agentflow-cli/src/commands/restore.rs::restore_dir()` 里
  `std::fs::remove_dir_all(&target)` 原先在
  `target.parent().is_none()`（"拒绝恢复到文件系统根路径"护栏）**之前**
  执行。`target` 来自 `resolve_include_dir` → `resolve_env_or_default`
  （`backup.rs:613-616`），对
  `AGENTFLOW_RUN_DIR`/`_TRACE_DIR`/`_SKILLS_DIR`/`_PLUGINS_DIR`/
  `_MARKETPLACE_CACHE` 等环境变量零校验直接 `PathBuf::from(env::var(...))`，
  配合 `--force` 会触发 `remove_dir_all("/")`。**Fix**：护栏逻辑抽成纯函数
  `reject_unsafe_restore_target(target: &Path) -> Option<String>`（无 I/O），
  在 `restore_dir()` 里作为**第一条语句**无条件执行，早于
  `target.exists()` / `remove_dir_all` / `create_dir_all` / `tar` 的任何
  分支。复核了 `restore.rs` 里唯一另一处破坏性操作
  `restore_db()`（`pg_restore --clean --if-exists`）——它的检查
  （artifact 存在、`pg_restore` 在 PATH 里）已经全部在执行前，不存在同类
  顺序问题，无需改动。**测试**（3 个新增，均通过，见
  `cargo test -p agentflow-cli --lib commands::restore::`）：
  `reject_unsafe_restore_target_rejects_filesystem_root` +
  `reject_unsafe_restore_target_allows_nested_path`（纯函数直接测试，
  `Path::new("/")` 只做字符串检查不触碰磁盘）；
  `restore_dir_fails_closed_before_deleting_an_unresolvable_root_target`
  端到端复现原始灾难场景（`AGENTFLOW_SKILLS_DIR=/` + `--force` +
  `dry_run=false`），断言 restore 在触碰 `target.exists()`/
  `remove_dir_all`/`tar` 之前就已 `status: "failed"` 退出——由于护栏是
  函数体第一条语句，该测试对真实 `/` 只读不写，不会真的删除任何文件。
  `cargo clippy -p agentflow-cli --all-features -- -D warnings` 通过。
  Commit: `fix(cli): guard restore target before destructive filesystem ops`。
- DONE U0.2 `agentflow restore` 增加制品完整性校验（**MAJOR**，evaluation
  §5"新发现"）：`BundleManifestArtifact` 原先只记录 `bytes`，从不记录
  哈希；`restore_db`/`restore_dir` 分别把 dump/tarball 直接喂给
  `pg_restore --clean --if-exists`/`tar`，无任何校验。**Fix**：
  1) `agentflow-cli/src/commands/backup.rs` 新增
  `pub(crate) fn sha256_file()`（流式读取，64KiB 分块，避免大文件整体
  载入内存），`run_pg_dump`/`run_tar_dir` 在 artifact 写入成功后立即计算
  并写入 `BackupStepReport.sha256` / `BundleManifestArtifact.sha256`
  （`Option<String>`，`#[serde(default)]` 保持对 U0.2 之前旧 manifest 的
  向后兼容——缺失哈希不是硬错误）；hashing 本身失败时该 step 直接判定
  `failed`（fail-closed，不静默生成一个没有哈希的"成功"条目）。
  2) `agentflow-cli/src/commands/restore.rs` 新增
  `verify_artifact_integrity()`：manifest 有记录哈希且不匹配 → 默认
  拒绝执行（`Err`，step 状态 `failed`，不触碰 `pg_restore`/`tar`）；
  显式传 `--skip-integrity-check` → 仍然执行但在 `reason` 字段打印醒目
  警告（`INTEGRITY CHECK FAILED ... but --skip-integrity-check was
  passed`）；manifest 没有记录哈希（旧 bundle）→ 视为"无法校验"，打印
  警告但不阻断。`restore_db`/`restore_dir` 分别在
  `which_in_path("pg_restore"/"tar")` 通过之后、真正调用命令之前插入该
  校验。`RestoreArgs` 新增 `skip_integrity_check: bool`，
  `agentflow-cli/src/main.rs` 新增 CLI flag `--skip-integrity-check`
  并透传。
  3) **测试**（均通过）：`agentflow-cli/src/commands/restore.rs` 单元测试
  新增 6 个（`verify_artifact_integrity_{accepts_a_matching_hash,
  rejects_a_tampered_artifact_by_default,
  allows_a_tampered_artifact_when_skip_flag_is_set,
  skips_verification_for_a_legacy_manifest_with_no_hash}` 纯函数级 +
  `restore_dir_{rejects_a_tampered_artifact_before_ever_invoking_tar,
  restores_normally_when_the_artifact_matches_its_recorded_hash}`
  端到端，后者用真实 `tar -czf` 构造 fixture 验证完整解包成功路径）；
  `agentflow-cli/tests/backup_restore_roundtrip_tests.rs` 新增 2 个 CLI
  级集成测试：`backup_manifest_records_a_sha256_for_each_artifact`（断言
  manifest.json 的 `sha256` 字段格式 `sha256:<64 hex>`）、
  `tampered_artifact_is_rejected_by_default_but_restorable_with_skip_integrity_check`
  （用两个独立 `agentflow backup` 产出的不同 tarball 互相替换模拟篡改，
  验证默认拒绝 + `--skip-integrity-check` 后确实恢复了"篡改后"的真实
  内容，证明 override 不是静默 no-op）；`restore_help_lists_documented_flags`
  同步断言新 flag 出现在 `--help`。`cargo test -p agentflow-cli --lib
  commands::restore / commands::backup` + `--test
  backup_restore_roundtrip_tests` 全部通过（36 项）；`cargo clippy -p
  agentflow-cli --all-features --all-targets -- -D warnings` 通过。
  4) `docs/SERVER_BACKUP_RESTORE.md` 新增"Integrity verification"一节 +
  `--skip-integrity-check` flag 说明 + manifest 输出布局注释更新。
  Commit: `feat(cli): verify backup artifact integrity before restore`。

### U1 — 生产健壮性 / 隔离护栏

- DONE U1.1 跨租户身份改为真正绑定，而非客户端自报 header（evaluation §4
  "服务层"，**本轮最严重的标准开放风险，7/29 报告已点名，T 段未覆盖**）：
  `agentflow-server/src/tenant.rs::extract_tenant_id` 原先只读客户端可控的
  `X-Agentflow-Tenant` header（找不到则回退 `"default"`），没有签名/token
  绑定；配合服务器全局单一 bearer token，任何认证过的调用方都能自称任意
  租户读写数据。**方向确认**（动手前按 Execution Notes 要求先确认）：
  落地"token→tenant 绑定"这个最小闭环（而非完整 JWT/OIDC，后者留给未来
  U 段），因为现有 DB schema 没有 users/tokens 表（`agentflow-db` 里
  `tenant_id` 只是裸 `TEXT` 列，无 FK），是 greenfield 加法而非改造，
  改动量可控。**Fix**：
  1) `agentflow-server/src/auth.rs`：`AuthConfig` 新增 `tenant_tokens:
  Vec<TenantToken>`（`TenantToken { token, tenant_id }`），`expected_token`
  保留为"遗留无绑定 token"（向后兼容，行为不变：信任客户端 header，默认
  `"default"`）。新增环境变量 `AGENTFLOW_API_TOKEN_TENANTS`（逗号分隔
  `token:tenant_id` 列表），`resolve_auth_config`/
  `resolve_auth_config_from_env` 解析并校验（格式错误/同一 token 出现两次/
  token 同时出现在 `AGENTFLOW_API_TOKEN` 和 `AGENTFLOW_API_TOKEN_TENANTS`
  三种情况全部在启动期 fail-closed 拒绝，错误信息不回显 token 明文）；
  `AGENTFLOW_API_TOKEN_TENANTS` 单独也能满足 Production profile 的
  `require_api_token` 门槛（不强制要求同时设置遗留 token）。
  `require_bearer_token` 匹配到 bound token 时把
  `AuthenticatedTenant(Some(tenant_id))` 写入 request extension（匹配到
  遗留 token 则写 `None`）。
  2) `agentflow-server/src/tenant.rs::extract_tenant_id`：签名改为
  `Result<Response, ApiError>`，优先读取 `AuthenticatedTenant` extension；
  有绑定 tenant 时该值是权威来源——客户端 header 声明了不同的 tenant 时
  直接 `Err(ApiError::TenantMismatch(...))` 拒绝（复用 Q1.4.3 已有的
  "显式声明与权威值冲突→拒绝而非静默覆盖"约定和同一个错误 code
  `tenant_mismatch`），未声明或声明一致则放行；无绑定（未配置 auth，或
  匹配到遗留 token）时保持 pre-U1.1 行为，信任 header，默认 `"default"`。
  3) `agentflow-server/src/lib.rs::create_router`：中间件挂载顺序修正
  （原顺序是 bug 的根因之一）——`.layer()` 后挂载的是外层、先于内层执行；
  原代码先挂 `require_bearer_token` 再挂 `extract_tenant_id`，导致
  tenant 中间件实际上先于 auth 中间件跑，读不到任何 auth 产出的
  extension。改为先挂 `extract_tenant_id`（内层）再挂 `require_bearer_token`
  （外层，仅在配置了 auth 时才挂），使 auth 先跑并把
  `AuthenticatedTenant` 写进 request，tenant 中间件才能读到。
  4) `agentflow-server/src/serve.rs`：`build_startup_report`/`AuthReport`
  同步识别 `AGENTFLOW_API_TOKEN_TENANTS`（新增 `tenant_tokens_present`
  字段），否则纯 per-tenant-token 部署会被 `agentflow doctor`/`serve
  --check` 误报"缺 token"。
  **测试**（均通过）：`auth.rs` 单元测试 6 个（解析格式错误/重复/歧义、
  Production profile 仅靠 tenant_tokens 也能满足门槛）；
  `tests/auth_and_errors.rs` 新增 `auth_and_tenant_router` 帮助函数
  （按 U1.1 顺序同时挂载两个中间件）+ 6 个集成测试，核心回归
  `tenant_bound_token_cannot_self_report_a_different_tenant_via_header`
  断言 403 `tenant_mismatch`；`tests/e2e_runs.rs` 新增 2 个走完整
  `create_router` + DB 的端到端测试（`tenant_bound_token_cannot_submit_a_
  run_as_a_different_tenant` / `..._submits_a_run_as_its_own_tenant`，
  本机无 `AGENTFLOW_DATABASE_TEST_URL` 时按现有约定自动跳过）；
  `serve.rs` 新增 `run_check_production_satisfied_by_tenant_tokens_alone_u1_1`
  ——过程中发现两个新测试与既有测试并发时对全局 env var
  `AGENTFLOW_API_TOKEN_TENANTS` 有 race（该 var 不像 `auth_token_env`
  一样可按测试自定义名称），补了一个 `tokio::sync::Mutex` 序列化锁
  （复刻 `agentflow-harness/tests/runtime_react_smoke.rs` 现有的
  `env_lock()` 模式）修复，重复跑 5 次确认不再 flaky。
  `cargo test -p agentflow-server`（全部 195+56 项）、
  `cargo test -p agentflow-cli --test serve_cli_tests`、`cargo clippy
  -p agentflow-server -p agentflow-cli --all-features --all-targets --
  -D warnings`、`cargo xtask check-arch` 全部通过。
  `docs/DEPLOYMENT.md` 新增"Multi-tenant deployments: bind tokens to
  tenants (U1.1)"一节；`docs/SECURITY_PROFILES.md` 同步更新 token 门槛
  说明。Commit: `feat(server): bind bearer tokens to tenants`。
- DONE U1.2 Helm chart / docker-compose 默认部署档位问题（**MAJOR 新发现**，
  evaluation §4"新发现"）：`charts/agentflow/values.yaml` 和
  `docs/DEPLOYMENT.md` 原先全文都没有引用 `AGENTFLOW_SECURITY_PROFILE`，
  严格照着 `docs/DEPLOYMENT.md` 部署的 Helm 安装跑在 `Local` 模式——没有
  强制 API token。**Fix**：
  1) `charts/agentflow/values.yaml` 新增 `securityProfile: local`
  字段（默认保持 `local` 不破坏现有安装），字段上方是一段醒目注释，
  逐条列出"生产部署必须显式设置为 `production`"的具体后果（token 不
  fail-closed / CORS 宽松 / worker gRPC 准入不 fail-closed）；澄清了一个
  容易误解的细节——`local` 下设置了 `AGENTFLOW_API_TOKEN` 仍然会被强制
  校验，profile 只决定"是否必须设置"而非"设置了是否生效"。
  2) `charts/agentflow/templates/deployment.yaml` 新增
  `AGENTFLOW_SECURITY_PROFILE` 容器 env（置于 `.Values.env` range 之前，
  值来自 `.Values.securityProfile`，`helm template`/`helm lint` 验证
  默认值 `local` 和 `--set securityProfile=production` 两种场景均正确
  渲染，且与 `autoscaling.enabled=true` 组合正常）。
  3) `docs/DEPLOYMENT.md` 的 `## Helm` 段新增"### Security profile
  (U1.2)"小节，说明默认值的风险 + 生产部署命令示例 + 一并指出这个 chart
  目前没有把 `AGENTFLOW_API_TOKEN` 接成专门的 K8s Secret（只有
  `DATABASE_URL` 有）；`## Docker Compose` 段补充一行指向该小节。
  4) `docker-compose.yml` 里 `AGENTFLOW_API_TOKEN` 注释旁新增
  `AGENTFLOW_SECURITY_PROFILE` 说明，同样澄清"设置了 token 在 local 下
  依然生效，profile 只影响是否强制要求"这一点，避免运维误读。
  5) `docs/SECURITY_PROFILES.md` "Compatibility Notes" 补充一句指向
  Helm chart 和 docker-compose 的默认值 + 新文档小节的链接。
  **未采纳的进阶选项**：验收标准里"agentflow doctor/启动日志在
  securityProfile 未显式设置时打印醒目提示"——评估后发现无法在
  `agentflow-server::serve::run()` 里可靠区分"用户显式传了
  `--security-profile local`"和"完全没设置、隐式落到默认值"（CLI flag
  与 env var 两条输入路径汇合后，`ServeConfig.security_profile` 已经是
  resolved 后的枚举值，只能在 Helm/compose 这种纯 env var 驱动的部署里
  可靠判断，在 CLI 直接调用场景下会产生误报），为避免引入一个可能误导
  的启发式，本轮不实现这一项；核心验收标准（values.yaml 字段 + 文档）
  已完整覆盖。
  **验证**：`helm lint charts/agentflow`（0 失败）；`helm template`
  默认值 + `--set securityProfile=production` 两种场景 diff 确认
  `AGENTFLOW_SECURITY_PROFILE` 正确渲染；`python3 -c "import yaml;
  yaml.safe_load(open('docker-compose.yml'))"` 确认改动后仍是合法
  YAML（本机沙箱无 `docker` 命令，无法跑 `docker compose config`）。
  Commit: `feat(helm): make AGENTFLOW_SECURITY_PROFILE explicit and documented`。
- DONE U1.3 生产 agent 运行时成本熔断补齐 CLI/Server 配置入口
  （evaluation §2/§3，T1.1 的"接线在最后一公里停下"残留）：T1.1 已经把
  `cost_limit_usd` 完整接线进 `RuntimeLimits`/`ReActAgent`/
  `PlanExecuteAgent` 的运行时逻辑并测试完备，但没有任何 CLI flag 或
  Server API 字段可以设置它。**Fix**：
  1) `agentflow-cli/src/main.rs`：`HarnessCommands::Run`/`Chat` 新增
  `--cost-limit-usd <f64>` flag，透传给 `agentflow-cli/src/commands/
  harness/{run,chat}.rs::execute()`，在两处的 `RuntimeLimits { ... }`
  字面量里补上 `cost_limit_usd`（此前是 `..Default::default()` 隐式
  留空）。`workflow dynamic` **不在范围内**——核查后确认它走
  `DynamicWorkflowAgent`/`compile_plan_to_flow` 编译成 `Flow` DAG，是
  完全不同的执行模型，压根没有 `RuntimeLimits`/`ReActConfig`/
  `PlanExecuteConfig` 可接。
  2) `agentflow-server/src/harness.rs`：`CreateHarnessSessionRequest`
  新增可选 `cost_limit_usd: Option<f64>`（`#[serde(default)]`），
  `HarnessSessionContext` 同步新增字段并在 `submit_harness_session` 里
  透传。`:resume` 路径**不透传**——`cost_limit_usd` 不是
  `harness_sessions` 表的持久化列（不像 `profile`/`runtime_kind`/
  `model`/`skill_name`），resume 重建 context 时没有原始值可恢复，显式
  设为 `None`（代码注释说明了这一已知限制，不是遗漏）。
  3) `agentflow-server/src/harness_live.rs`（真正执行 agent 的
  `LiveHarnessExecutor`）：`RunInputs`/`clone_run_inputs` 新增字段，
  `run_harness_inner` 给 `HarnessRunOptions` 补上此前完全没有的
  `.with_limits(RuntimeLimits { cost_limit_usd, ..Default::default() })`
  调用——这是本项里唯一"新增而非复制既有模式"的接线点，因为 Server
  API 此前没有任何 `RuntimeLimits` 形状的字段可以照抄。
  **测试**（均通过）：`agentflow-harness/tests/runtime_react_smoke.rs`
  新增 `harness_runtime_stops_react_agent_when_cost_limit_usd_is_exceeded`
  ——这是本项里分量最重的回归测试，用 mock LLM provider（无需真实
  API key）驱动真实 `ReActAgent` 走完整路径
  `HarnessRunOptions::with_limits(...)` → `HarnessRuntime::run(...)` →
  `AgentContext.limits` → `ReActAgent::run_with_context`，断言 harness
  事件流的终止 `Stopped` 事件带 `cost_limit_usd exceeded` 错误——T1.1
  当年的测试只验证了 `agent.run_with_context()` 直调，从未验证 CLI/
  Server 现在接入的这条完整链路，这条测试补上了这个真实空白。另外
  `agentflow-cli/tests/harness_cli_tests.rs` 新增 2 个 `--help` flag
  存在性测试；`agentflow-server/tests/harness_routes.rs` 新增
  `submit_accepts_optional_cost_limit_usd_field` 验证字段解析不破坏
  session 创建（`StubHarnessExecutor` 不跑真实 agent，无法在这一层
  验证熔断本身，已在测试注释里说明并指向上面那条 harness 层测试）。
  `cargo test -p agentflow-harness -p agentflow-cli --test
  harness_cli_tests -p agentflow-server`、`cargo clippy -p
  agentflow-cli -p agentflow-server -p agentflow-harness
  --all-features --all-targets -- -D warnings` 全部通过。
  `docs/OPERATIONS_HANDBOOK.md` §2.2 的 `CostLimitExceeded` 条目补充
  `--cost-limit-usd`/`cost_limit_usd` 入口说明；`docs/HARNESS_MODE.md`
  在"CLI surface"节后追加一段"Runtime-limit flags"（该文档的 Phase H1
  flags 代码块本身已经和实际实现脱节，不在本项范围内一并修正，新内容
  以独立小节形式追加而非改写旧块）。
  Commit: `feat(harness): expose cost_limit_usd via CLI flag and API field`。

### U2 — 架构 / 生态债务（U2.1/U2.2/U2.3/U2.4 均已闭环；仅剩 U2.5，
U2.1 复核后拆出的新条目、工程量较大）

- DONE（部分，拆分为 U2.1 harness 半 + U2.5 新增 agents 半后续）U2.1
  `agentflow-agents`/`agentflow-harness` 改为依赖 `agentflow-store-spi`
  而非 `agentflow-memory`（evaluation §1"复核发现"）：动手前重新核查
  发现 evaluation 原文的耦合面描述**已经过期**——`agentflow-agents` 生产
  代码并不像原文说的那样"仅两处用 `SessionMemory::default_window()`
  提供默认值"，实际耦合面大得多且性质不同：`react/agent.rs` 生产逻辑
  依赖 `ProjectMemoryStore`/`ProjectFact`（project-memory 特性，晚于
  evaluation 基线上线），这两个类型**从未被拆进 `agentflow-store-spi`**
  （不像 `MemoryStore`/`TaskSummaryStore`/`Message` 那样已有契约）。这不是
  "默认值构造可以挪走"的例外，是一个真实的、更大的缺口。**决策**（按
  Execution Notes 允许拆分里程碑的条款）：拆成两半分别处理——
  1) **`agentflow-harness` 半，DONE**：复核确认这半的耦合面描述准确
  （生产代码只有 `runtime.rs:846`/`866` 两处，都是 `MemoryStore`/
  `Message`，均为纯 `store-spi` 转出口）。`agentflow-harness/Cargo.toml`
  的 `[dependencies]` 从 `agentflow-memory` 改为 `agentflow-store-spi`；
  `agentflow-memory` 保留在 `[dev-dependencies]`（本来就在，测试用
  `SessionMemory`）；`runtime.rs` 两处引用改为
  `agentflow_store_spi::{MemoryStore, Message}`。`xtask/src/main.rs` 的
  `ARCH_LATENT_EDGES` 移除 `harness -> agentflow-memory` 条目（改判为
  "PAID DOWN"，加注释说明）；`cargo xtask check-arch` 从"11 latent"
  降到"10 latent"，`agentflow-harness -> agentflow-memory` 不再出现。
  `docs/RFC_CRATE_ARCHITECTURE.md` R6 后追加 Status 段、
  `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` TL;DR 后追加 Update
  段，两处都写清楚 harness 半闭环、agents 半为什么没有闭环（区分"过期
  的评估结论"与"新决策"，不覆盖历史表格原文）。测试：
  `cargo test -p agentflow-harness`（90 项）+ `cargo test -p xtask`
  （80 项）+ `cargo check --workspace --lib` + `cargo clippy -p
  agentflow-harness -p xtask --all-features --all-targets -- -D
  warnings` 全部通过。Commit: `refactor(harness): depend on
  agentflow-store-spi instead of agentflow-memory`。
  2) **`agentflow-agents` 半，未关闭，改判为新条目 U2.5**（见下方）——
  不是"放着不决策"，是判断这半的真实工作量（契约抽取，不是依赖切换）
  超出 U2.1 原定范围，值得单独排期评估。
- DONE（缩小范围后，见下方"范围边界"）U2.2 把 Preference/EntityFactStore
  接线进 SkillBuilder + agent 运行时（evaluation §3"缺口 2 重新表述"，
  T4.1 核查后确认的真实剩余缺口）：`SqlitePreferenceStore`/
  `SqliteEntityFactStore` 存储层完整，但 `agentflow-skills`/
  `agentflow-agents`/`agentflow-harness` 原先零处引用这两个 trait，
  产品可见行为上和"没做"没有区别。**范围边界**（动手前与用户确认，
  按 Execution Notes 允许的"先从最小子集起步"）：只做 **preference 层**
  的读+写全链路，**entity_facts 层显式推迟**（`EntityFactStore::get_facts`
  按单个 `entity_id` 查询，没有"列出某用户全部 facts"的方法，要做到
  "自动读取"需要先解决"当前轮次里哪些实体在范围内"这个 NLU 性质的
  设计问题，比 preference 的"列出某用户全部 preference"复杂一个量级，
  值得单独排期）；用户身份**不建多用户体系**，固定用
  `PreferenceScope::local("default")`（与代码库其它地方"default"
  tenant/session 的既有默认值写法一致）；只接 `ReActAgent`，不接
  `PlanExecuteAgent`（该 agent 本来就没有 task_summary_store/
  project_memory_store 可以类比的既有先例，是独立、无关的缺口，不是
  U2.2 新引入的）。**Fix**：
  1) `agentflow-memory` 新增 `agentflow-tools` 依赖 + 新模块
  `preference_tool.rs`：`RememberPreferenceTool`（`Tool` impl），把
  `SqlitePreferenceStore` 包一层 `Arc<tokio::sync::Mutex<_>>`
  （`PreferenceStore::put_preference` 签名是 `&mut self`——单一 owner
  设计，不像 `TaskSummaryStore` 那样天生 `&self`/可 `Arc` 共享——
  Mutex 让同一个 store 实例能同时被"写"的 tool 和"读"的 agent 共享，
  不用改 `agentflow-memory` 里已有的 trait 签名），暴露给 LLM 调用，
  写入即生效（下一轮就能读到）。
  2) `agentflow-agents/src/react/agent.rs`：`ReActAgent` 新增
  `preference_store`/`preference_scope` 字段 + `with_preference_store`
  builder setter + `format_preference_for_prompt`，注入位置在
  `build_llm_messages` 里系统 prompt 之后、project facts/task summary
  之前（比它们都更"基础"——preference 是关于用户本身，不是某个
  project/session 范围内的）。
  3) `agentflow-skills`：`manifest.rs` 的 `MemoryConfig` 新增
  `preference: Option<PreferenceMemoryConfig>` 子表（`enabled` +
  `db_path`，最小配置）；`loader.rs` 新增校验，**顺带修了一个真 bug**
  ——`KNOWN_MEMORY_TYPES` 原来是 `["session","sqlite","none"]`，缺
  `"semantic"`，导致合法的 `type = "semantic"` manifest 在 loader
  校验阶段就被拒绝，`builder.rs::build_memory` 里的 semantic 分支其实
  永远到不了（这不只是 U2.4 说的"文档过期"，是校验列表本身的 bug，
  U2.4 剩下的工作缩小为纯文本修正）；`builder.rs` 新增
  `build_preference_store`（镜像 `build_memory` 的结构），
  `build_with_extra_tools`/`build_with_admission` 都在构建 agent 前
  注册 `remember_preference` 工具（`build_with_admission` 路径下同样
  受 `admit()` 过滤器约束）并把 store attach 到 agent。**已知遗留缺口**
  （记录不隐藏）：裸调用 `SkillBuilder::build_registry()`（`agentflow
  skill list-tools` 等场景）看不到 `remember_preference`，因为把它塞进
  `build_registry` 会改变该方法的公开返回签名（需要额外把 store 传给
  调用方），当前只在 `build_with_extra_tools`/`build_with_admission`
  两条构建完整 agent 的路径里注册。
  **测试**（均通过，见下方验证命令）：`agentflow-memory` 新增 4 个
  `RememberPreferenceTool` 单测；`agentflow-agents` 新增 3 个（直接
  seed 验证注入 / 未配置时 no-op / **核心回归**
  `remember_preference_tool_write_is_visible_to_a_second_agent_instance`
  ——mock LLM 真实驱动一次 tool call 写入，第二个全新 agent 实例读到，
  镜像既有的 L3.1 project-memory 回归测试模式）；`agentflow-skills`
  新增 3 个（工具注册与否 + **核心端到端**
  `preference_written_in_one_session_is_read_in_the_next`——用
  `SkillBuilder` 构建的 agent 直接调用它自己注册的 `remember_preference`
  工具模拟一次真实对话轮次，不需要 mock LLM，第二个由同一份
  `skill.toml` 构建出的全新 agent 读到，直接对应验收标准的
  "配置了 preference 的 Skill 在一次会话中写入的偏好，在下一次会话中
  被读取"表述）。`docs/MEMORY_LAYERING.md` 的 "Wiring status" 更新为
  反映 preference 已闭环、entity_facts 仍未接线；"Migration path" 的
  skill.toml 示例更新为真实字段形状（`enabled`/`db_path`，不是最初
  设想草稿里的 `type`/`path`）；顺带修正了 `KNOWN_MEMORY_TYPES`
  bug 对应的文档表述（U2.4 剩余工作范围因此缩小）。
  **验证**：`cargo test -p agentflow-memory -p agentflow-agents -p
  agentflow-skills -p agentflow-cli --lib`（全部通过，无回归）；
  `cargo clippy -p agentflow-memory -p agentflow-agents -p
  agentflow-skills -p agentflow-cli --all-features --all-targets --
  -D warnings` 通过；`cargo xtask check-arch` 通过（新增
  `agentflow-memory -> agentflow-tools` 边，镜像
  `agentflow-rag -> agentflow-tools` 的既有模式，未触发任何一条
  已启用的依赖铁律）。Commit: `feat(memory): wire preference layer into
  SkillBuilder and ReActAgent`。
- DONE U2.3 `harness run`/`resume` 的 `--approve` 默认值补齐 profile-aware
  逻辑（evaluation §3"缺口 4 复核"，T1.3 的未覆盖姊妹口子）：T1.3 已经
  给 `agentflow workflow dynamic` 的 `--approve` 接上了
  `resolve_approve_default(profile)` 逻辑，但 `agentflow harness run`
  的 `--approve` 依然硬编码默认值 `"none"`，与 `--profile` 完全脱钩。
  **范围核实**（动手前发现原文措辞不准）：`agentflow harness resume`
  CLI 子命令实际上是**纯只读 replay**（读取已持久化的 JSONL session
  log 重新打印，不调用 LLM、不跑工具、没有 approval 概念）——
  `agentflow-cli/src/commands/harness/resume.rs` 的 `execute()` 根本没有
  `--approve` 参数，所以"resume"没有默认值可修。真正和 `run` 共享同一个
  硬编码 bug 的是 `harness chat`（同样 `default_value = "none"`），
  评估原文遗漏了它——本项范围因此调整为 **`run` + `chat`**，`resume`
  确认无需改动（记录理由，不是遗留）。**Fix**：
  1) `resolve_approve_default` 从 `commands::workflow::dynamic.rs` 挪到
  共享位置 `commands::harness::mod.rs`（`pub(crate)`，与已经在那里的
  `parse_profile`/`resolve_run_dir` 同级），`dynamic.rs` 改为导入复用
  （`dynamic.rs` 本来就已经从 `commands::harness` 导入
  `parse_profile`，这是同一个既有的跨模块复用惯例，不是新模式）。
  2) `agentflow-cli/src/main.rs`：`HarnessCommands::Run`/`Chat` 的
  `approve` 字段从 `String`（`default_value = "none"`）改为
  `Option<String>`（无 default，靠 `resolve_approve_default` 补上
  profile-aware 默认值——`default_value` 会让"用户没传"和"用户显式传
  none"在解析后变得无法区分，必须去掉才能做真正的 profile-aware 默认）。
  3) `harness/run.rs`/`harness/chat.rs` 的 `execute()` 签名同步改为
  `approve: Option<String>`，在 `parse_profile` 之后、
  `ApproveMode::parse` 之前插入 `resolve_approve_default(approve,
  profile)`。
  **测试**（均通过）：`resolve_approve_default` 原有 4 个单测随函数一起
  迁移到 `commands::harness::tests`；`agentflow-cli/tests/
  harness_cli_tests.rs` 新增 2 个 CLI 集成测试
  （`harness_chat_defaults_to_cli_approval_under_local_profile_without_approve_flag`
  / `harness_chat_stays_unsupervised_under_dev_profile_without_approve_flag`，
  通过 REPL 启动 banner 里的 `approve: <value>` 字样验证——banner 打印
  的是**解析后**的值，不是原始 flag，能真正证明 CLI 接线生效）。`run`
  没有加对应的活体 LLM 集成测试——`harness_cli_tests.rs` 文件顶部注释
  本来就明确说明"`run` 的活体调用路径在
  `agentflow-harness/tests/runtime_react_smoke.rs` 里测，这里只测不需要
  LLM 的子命令"，`run.rs`/`chat.rs` 调用的是同一个已被详尽单测覆盖的
  `resolve_approve_default` 函数，且接线方式（两行代码）完全一致，
  `chat` 的集成测试已经足够对这条共享逻辑的接线做端到端验证。
  `cargo test -p agentflow-cli --lib` + `--test harness_cli_tests` +
  `--test workflow_dynamic_tests` 全部通过；`cargo clippy -p
  agentflow-cli --all-features --all-targets -- -D warnings` 通过。
  **已确认并记录的残留细节**（不在本项范围内）：`agentflow workflow
  dynamic` 命令自身 `--profile` 默认值仍是 `"dev"`（而非 `harness
  run`/`chat` 用的 `"local"`），裸命令行调用（不传任何 flag）今天仍然
  解析为 `dev → none`（无监督）——这是 U4.1 的范围（讨论默认 profile
  是否应更保守），本项不处理，避免下一轮复核把"已修复"和"裸调用仍是
  dev 默认"混为一谈。
  Commit: `feat(cli): make harness run/chat --approve profile-aware by default`。
- DONE（U2.2 同期一并解决，文档部分）U2.4 修正 `docs/MEMORY_LAYERING.md`
  里 T4.1 自己引入的新文档错误（evaluation §3"副作用"，成本极低）：
  T4.1（commit `81cda0f`）声称 `agentflow-skills` 的 `[memory]` 解析
  "只接受 `session`/`sqlite`/`none`"，但 `builder.rs::build_memory`
  实际接受四种（含 `semantic`）。核实时发现这不只是文档滞后——
  `loader.rs::KNOWN_MEMORY_TYPES` 校验列表本身确实缺 `"semantic"`，是
  个真代码 bug（合法的 `type = "semantic"` manifest 会在校验阶段被拒绝，
  `build_memory` 的 semantic 分支实际不可达），已随 U2.2 一并修复（见
  U2.2 的 fix 第 3 点）。**本项范围**（文档表述）：
  `docs/MEMORY_LAYERING.md` "Migration path" 一节改为准确列出全部四种
  已支持类型，并说明校验列表缺失是刚修的 bug 而非未实现——已随 U2.2 的
  commit `feat(memory): wire preference layer into SkillBuilder and
  ReActAgent` 一并完成（同一次文档更新，commit message 里已分别说明
  这是两个独立问题：preference 接线 vs. 类型列举校验 bug）。
- DONE（契约抽取部分；Cargo.toml 依赖切换确认无法达成，见下）U2.5
  （新增，U2.1 复核时拆出）为 `ProjectMemoryStore`/`ProjectFact` 抽取
  `store-spi` 契约：`agentflow-agents/src/react/agent.rs` 生产代码依赖
  `agentflow_memory::{ProjectMemoryStore, ProjectFact}`（project-memory
  特性），这两个类型原本只存在于 `agentflow-memory`（`src/project.rs`），
  从未被拆进 `agentflow-store-spi`——不像同一份代码里用到的
  `MemoryStore`/`TaskSummaryStore`/`Message`/`TokenCounter`/
  `MemoryError`，那些早就是纯契约。**方向确认**（动手前用
  AskUserQuestion 询问用户，因为发现验收标准本身已不可达成——见下）：
  核实代码时发现 U2.2（本轮 U2.5 之前才做的）已经给 `ReActAgent` 加了
  第二个、独立的具体类型生产字段
  `Option<Arc<tokio::sync::Mutex<agentflow_memory::SqlitePreferenceStore>>>`
  + `agentflow_memory::PreferenceScope`——`PreferenceStore` 的写方法是
  `&mut self`（U2.2 自己的范围决定明确没把它挪进 `store-spi`，因为这个
  形状跟本 crate `Arc<dyn Trait>` 契约不兼容，需要先重新设计成
  `&self` + 内部锁）。所以就算把 `ProjectMemoryStore`/`ProjectFact`
  按原验收标准挪进 `store-spi`，`agentflow-agents/Cargo.toml` 的
  `[dependencies]` **也依然无法改成纯 `agentflow-store-spi`**——
  preference 部分仍需要真实的 `agentflow-memory` 依赖。用户确认：只做
  `ProjectFact` 契约抽取（仍是真实进展，消掉一处契约泄漏），不动
  Cargo.toml，也不额外抽取 `PreferenceStore`（那是更大工程量，需要
  重新推翻 U2.2 的 `&mut self` 设计决定）。**fix**：参照 T3.3
  `agentflow-tool`/`agentflow-tools` 拆分的模式（同时也是
  `TaskSummaryStore` 在 `agentflow-store-spi` 里已有的先例），新增
  `agentflow-store-spi/src/project.rs`（`ProjectMemoryStore` trait +
  `ProjectFact` struct + `project_key_for_path` fn，含 2 个单测），
  `lib.rs` 新增 `pub mod project;` + 对应 re-export；
  `agentflow-store-spi/Cargo.toml` 新增 `sha2` 依赖 +
  `[dev-dependencies]` 的 `tempfile`。`agentflow-memory/src/project.rs`
  改写为只保留 `InMemoryProjectMemoryStore`/`SqliteProjectMemoryStore`
  两个具体实现，`pub use agentflow_store_spi::{ProjectFact,
  ProjectMemoryStore, project_key_for_path};` 向后兼容 re-export（现有
  `use agentflow_memory::{ProjectMemoryStore, ProjectFact}` 调用点不用
  改，`agentflow-memory/src/lib.rs` 本身的 re-export 语句也不用改）。
  `xtask/src/main.rs` 的 `ARCH_LATENT_EDGES`：`agents -> agentflow-memory`
  条目**保留**（边没有真正闭合），但注释改写清楚真正的剩余阻塞点是
  `PreferenceStore`，不是 `ProjectMemoryStore`/`ProjectFact`；`harness ->
  memory` 那条 U2.1 注释里对 `agents -> memory` 的过时描述也一并更新，
  改为指向这条新注释。`docs/ARCHITECTURE_EVALUATION_2026-06-20.md`/
  `docs/RFC_CRATE_ARCHITECTURE.md` 各新增一段"Update/Status (U2.5,
  2026-07-31)"，说明 `ProjectFact` 契约已拆完但 `agents -> memory`
  边仍未闭合、真正原因是 preference 层（不重写历史快照表格，延续
  U2.1/U3.3 的既有约定）。`cargo xtask check-arch` 确认：latent edge
  数量维持 10（`agents -> memory` 本来就没被移除，只是注释更新，符合
  预期，不同于 U2.1 那次真正闭合边导致数量下降）。`cargo test -p
  agentflow-store-spi -p agentflow-memory -p agentflow-agents -p
  xtask` 全部通过（245+ 条 lib 测试 + 集成测试）；`cargo fmt` + `cargo
  clippy --all-features --all-targets -- -D warnings`（4 个 crate）+
  `cargo check --workspace --all-targets` 全部通过。Commit:
  `refactor(memory): extract ProjectMemoryStore/ProjectFact contract to
  store-spi`。**明确记录的残留缺口**：`agentflow-agents` 仍然真实依赖
  `agentflow-memory`（`SqlitePreferenceStore`/`PreferenceScope`），要
  完全闭合这条边需要先把 `PreferenceStore` 契约抽出去并解决
  `&mut self` 设计问题——本项不做；已作为新条目 U2.6 跟踪（见下）。
- DONE（PreferenceStore 契约抽取部分；Cargo.toml 依赖切换确认仍无法
  完全达成，见下）U2.6（新增，U2.5 完成时发现）为 `PreferenceStore`
  抽取 `store-spi` 契约，评估能否把 `agentflow-agents/Cargo.toml` 的
  `[dependencies]` 从 `agentflow-memory` 真正改为
  `agentflow-store-spi`：`agentflow-agents::react::agent::ReActAgent`
  生产代码原本有
  `Option<Arc<tokio::sync::Mutex<agentflow_memory::SqlitePreferenceStore>>>`
  + `agentflow_memory::PreferenceScope` 两个具体类型字段（U2.2 引入）。
  **方向确认**（动手前用 AskUserQuestion 询问用户两次，因为过程中连续
  发现两层新情况）：第一次确认——深入看两个 `PreferenceStore` 实现后
  发现 U2.2/U2.5 写下的"`&mut self` 不兼容 `Arc<dyn Trait>`"结论其实
  不完全成立（`Arc<Mutex<dyn Trait>>` 本来就能包 `&mut self` 方法，
  只是不能用裸 `Arc<dyn Trait>`），所以有"最小改动：包
  `Arc<Mutex<dyn Trait>>`"和"彻底重设：`&mut self` → `&self`，去掉
  Mutex"两条路；用户选择彻底重设（更彻底，跟 `ProjectMemoryStore`/
  `TaskSummaryStore` 完全一致）。第二次确认——开始改造后发现第三个
  阻塞点：`agentflow-agents/src/dynamic.rs`（`DynamicWorkflowAgent`
  编译 LLM 授权计划里的 `agent` 步骤时）生产代码直接
  `Box::new(SessionMemory::default_window())` 构造一个具体的
  `SessionMemory` 作为默认 memory backend——`SessionMemory` 没有
  store-spi 契约（故意不抽，它是具体实现不是契约缺失），所以就算把
  `PreferenceStore` 改完，`agentflow-agents/Cargo.toml` 的
  `[dependencies]` **仍然无法完全去掉 `agentflow-memory`**；用户确认
  继续做 `PreferenceStore` 改造，接受 Cargo.toml 无法完全切换、不为
  `SessionMemory` 这个点新开跟踪条目。**fix**：核实两个实现——
  `SqlitePreferenceStore::put_preference` 等写方法只用到
  `&self.pool`（`sqlx::SqlitePool` 内部是 Arc 包装的连接池，本身线程
  安全）；`AgeEncryptedPreferenceStore` 只是先加密/解密再转发给
  inner——两者都不真需要 `&mut self`。新增
  `agentflow-store-spi/src/preference.rs`（`PreferenceStore` trait 三个
  写方法全部改成 `&self` + `PreferenceScope`/`PreferenceValue` +
  1 个单测），`lib.rs` 新增 `pub mod preference;` + re-export。
  `agentflow-memory/src/layer.rs` 删除原地定义，改为
  `pub use agentflow_store_spi::{PreferenceScope, PreferenceStore,
  PreferenceValue};`（向后兼容，调用点不用改），移除重复的
  `preference_scope_local_uses_default_tenant` 测试（已在 store-spi
  侧覆盖）。`agentflow-memory/src/preference.rs`
  （`SqlitePreferenceStore`）+ `preference_encrypted.rs`
  （`AgeEncryptedPreferenceStore`）两个 impl 的写方法全部
  `&mut self` → `&self`；两文件 + `agentflow-cli/src/commands/
  memory/prune.rs` + 对应测试文件里所有不再需要的 `let mut store`
  绑定同步去掉（否则 `-D warnings` 下 `unused_mut` 会挂）。
  `agentflow-memory/src/preference_tool.rs`
  （`RememberPreferenceTool`）：字段从
  `Arc<Mutex<SqlitePreferenceStore>>` 改成 `Arc<dyn PreferenceStore>`，
  `execute()` 里去掉 `.lock().await`，doc comment 改写（不再提
  "`&mut self` 所以要包 Mutex"）。`agentflow-agents/src/react/agent.rs`：
  `preference_store` 字段改成 `Option<Arc<dyn
  agentflow_memory::PreferenceStore>>`（跟 `project_memory_store`/
  `task_summary_store` 同形状），`with_preference_store` builder
  签名同步；`apply_context` 里读 preference 的地方去掉
  `.lock().await`；移除了不再需要的顶层 `PreferenceStore` trait
  import（dyn trait object 调用自身 trait 方法不需要 `use`）；2 个
  测试的 `Arc<Mutex<...>>` 构造改成 `Arc<dyn PreferenceStore>`。
  `agentflow-skills/src/builder.rs`：`build_preference_store` 返回类型
  从 `Arc<Mutex<SqlitePreferenceStore>>` 改成 `Arc<dyn
  PreferenceStore>`，两个调用点（`build_with_extra_tools`/
  `build_with_admission`）不用改（`store.clone()` 对 `Arc<dyn Trait>`
  一样工作）。**残留缺口**（写进
  `xtask/src/main.rs`/两份架构文档，未新开跟踪条目）：`agents ->
  agentflow-memory` 边依然真实存在，原因从"缺 `PreferenceStore` 契约"
  变成"`dynamic.rs` 的 `SessionMemory` 默认值是设计上的具体实现需求，
  不是契约缺口"；要真正闭合这条边需要让 `DynamicWorkflowAgent` 的
  默认 memory 可注入，规模更大，本项不做。`cargo xtask check-arch`
  确认：latent edge 数量维持 10（跟 U2.5 一样，边没被移除，只是注释
  更新）。`cargo test -p agentflow-store-spi -p agentflow-memory -p
  agentflow-agents -p agentflow-skills -p agentflow-cli`：51 个测试
  二进制全部通过（含 245 条 agentflow-cli lib 测试、184 条
  agentflow-agents lib 测试），零失败；`cargo fmt` + `cargo clippy
  --all-features --all-targets -- -D warnings`（6 个 crate）+ `cargo
  check --workspace --all-targets` 全部通过。`docs/MEMORY_LAYERING.md`
  里过期的 `PreferenceStore` trait 签名片段（还写着 `&mut self`）一并
  修正。Commit: `refactor(memory): redesign PreferenceStore to &self
  and extract contract to store-spi`。

### U3 — 完整度 / 卫生

- DONE（第一阶段，见范围说明）U3.1 `agentflow-cli/src/main.rs`
  分发/校验逻辑拆分重构（evaluation §2，本轮唯一"原地踏步且规模仍在
  增长"的发现）：`main.rs` 从 T 段开始前的 2525 行涨到评估时的
  2671 行（本次动手前已经是 2699 行，说明期间还在继续长）。**范围**
  （按验收标准"允许分阶段，可先给不超过 3-5 个子命令"）：本阶段处理了
  3 个子命令分支，`main.rs` **2699 → 2670 行**（真实下降，不是平移）：
  1) `Commands::Backup`/`Commands::Restore`：两处几乎逐字重复的
  `--include` 解析（`map` + `ok_or_else` + `collect`）合并成
  `agentflow-cli/src/commands/backup.rs::parse_includes()` 一个共享
  函数（顺带修了一个 DRY 违规，不只是"挪地方"）；两个 match 分支都
  简化成 `match backup_cmd::parse_includes(&args.includes) { ... }`。
  2) `SkillCommands::Run`：原来是一个内联 IIFE 闭包，混合调用既有的
  `skill::server_ops::reject_local_only_flags(...)` 加一段独立的
  `--output json` 内联 `bail!`——把这条 `--output` 校验并进
  `reject_local_only_flags` 本体（新增一个 `output: &str` 参数），
  闭包整个消失，`main.rs` 里只剩一次函数调用 + `match`，和
  `workflow::server_ops::reject_local_only_flags` 已有的既定模式
  （同一个文件里 `workflow run --server` 分支已经是这个形状）对齐。
  **测试**（均通过）：`backup.rs` 新增 2 个 `parse_includes` 单测；
  `skill/server_ops.rs` 新增 1 个 `--output json` 拒绝单测，其余
  4 个既有测试补上新增的第 5 个参数；`cargo test -p agentflow-cli --lib`
  （184 项）+ `--test backup_restore_roundtrip_tests` +
  `--test skill_run_server_tests` 全部通过，无回归；`cargo clippy -p
  agentflow-cli --all-features --all-targets -- -D warnings` 通过。
  **未完成部分**（如实记录，不是"已完全解决"）：`main.rs` 仍有
  2670 行，本项只处理了评估报告点名问题里体量最大的一小部分（3 个
  分支），还有相当数量的 match 分支未拆分；后续如需继续下沉，可以
  从 `Commands::Doctor`（tuple-match 校验）或更大的 `HarnessCommands`/
  `WorkflowCommands` 分支入手，按同样的模式（提取成
  `commands::<mod>::` 里的纯函数 + `main.rs` 只留 `match` 调用）
  逐步推进——这不在本次范围内，如果后续复核认为还需要更多阶段，应该
  开一个新的 U-item 而不是重开本项。
  Commit: `refactor(cli): dedupe --include parsing and delegate skill run
  --server validation`。
- DONE U3.2 Helm chart 补 PodDisruptionBudget（evaluation §4，T4.3 验收
  标准明确排除、遗留至今的 gap）：`charts/agentflow/templates/` 原先
  没有 `pdb.yaml`。**Fix**：`charts/agentflow/templates/pdb.yaml`
  新增可选模板（比照 `hpa.yaml` 的 `{{- if .Values.X.enabled }}` 开关
  形状）；`values.yaml` 新增 `podDisruptionBudget: {enabled: false,
  minAvailable: 1, maxUnavailable: ""}`——**默认关闭**（与斟酌结论
  一致：`replicaCount`/HPA `minReplicas` 默认是 1，`minAvailable: 1`
  的 PDB 在单副本部署下会让自愿驱逐永远无法满足，阻塞节点维护/
  升级），只有 `maxUnavailable` 非空时模板才用它而不是 `minAvailable`
  （两者只应设置一个，`minAvailable` 优先）。`docs/DEPLOYMENT.md`
  autoscaling 小节后新增"PodDisruptionBudget (U3.2)"一节，明确写
  "只在多副本部署时启用"的理由 + 一个 `autoscaling` + PDB 组合的
  安装示例命令。**验证**：`helm lint charts/agentflow`（0 失败）；
  `helm template` 三种场景全部渲染正确——默认值（PDB 完全不出现在
  输出里）、显式启用 + `minAvailable`、显式启用 + `maxUnavailable`
  （确认互斥逻辑生效）；`docs/DEPLOYMENT.md` 里给出的安装示例命令
  实际跑过 `helm template` 确认能正常渲染。纯 Helm chart + 文档改动，
  无 Rust 代码变更。Commit: `feat(helm): add optional
  PodDisruptionBudget template`。
- DONE U3.3 同步 `docs/ARCHITECTURE_DIAGRAM.md` 和
  `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` 反映 `agentflow-tool` 的
  存在（evaluation §1，T3.3 拆分产生的新文档滞后）。**Fix**：
  1) `docs/ARCHITECTURE_DIAGRAM.md`：L0 契约内核 ASCII 框图的脚注从
  "-tools 的 Tool 契约亦属 L0 契约面，物理上随 L2 工具 crate 提供"改为
  准确描述"T3.3 起 Tool 契约拆为独立、零依赖的 L0 crate -tool"；L2
  框图里 `-tools` 盒子的标签从"Tool契约"改为"内置工具"（同宽度替换，
  不破坏 ASCII 对齐）；L0 表格新增 `agentflow-tool` 一行（对齐
  CLAUDE.md 的精确描述：`Tool` trait/`ToolRegistry`/`ToolIdempotency`/
  `Capability`/`SecurityProfile`/`SandboxBackend` 等，零依赖）；L2 表格
  `agentflow-tools` 一行改为准确描述"依赖并完整 re-export `agentflow-tool`
  + 内置实现 + OS 沙箱后端 + `code_exec`"，不再写"见 L0 契约面"这个
  过期指代。
  2) `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`：沿用 T2.4 与 U2.1 已经
  用过的模式——历史快照表格（§1/§2，2026-06-20 基线，早于 T3.3 五周）
  不整体重写，在已有的两段 Update 之后追加第三段"Update (U3.3,
  2026-07-31)"，说明 T3.3（commit `b06ce03`，2026-07-30）之后
  `agentflow-tool`/`agentflow-tools` 的准确划分，并指向
  `cargo xtask check-arch` 的实时输出和更新后的 `ARCHITECTURE_DIAGRAM.md`
  作为当前状态的权威来源。
  纯文档改动，无代码/测试变更。Commit: `docs(architecture): reflect the
  T3.3 agentflow-tool/agentflow-tools split`。
- DONE U3.4 生产 gRPC worker 监听器补齐 TLS 强制要求（evaluation §5"新
  发现"，T0.2/T1.2 的配套加固）：`build_worker_control_plane` 的
  fail-closed 检查（T0.2）只要求准入凭据在 `SecurityProfile::Production`
  下非空，不要求同时配置 `WorkerGrpcTlsConfig`。**方向确认**（动手前
  按 Execution Notes 要求先决策）：选 **warn-only，不 fail-closed**——
  `docs/DISTRIBUTED.md` 已经把"丢弃 TLS 三个 flag、明文 gRPC"文档化为
  一个有意、合法的部署形态（"仅限可信网络/同主机"），而 T0.2 的凭据
  检查不同：缺凭据没有任何合法部署场景（意味着任何匿名连接都能被
  准入），两者不对称，fail-closed 会破坏一个已经写进文档的合法配置。
  **Fix**：`agentflow-server/src/worker_grpc.rs` 新增纯函数
  `production_worker_grpc_lacks_tls(config, profile) -> bool`（`profile
  == Production && config.tls.is_none()`，拆成纯函数是为了不依赖捕获
  `tracing` 输出就能单测这个判断条件本身），`build_worker_control_plane`
  在构建 `WorkerAdmissionPolicy`（T0.2 fail-closed 检查所在处）之前
  调用它，为真时 `tracing::warn!` 打印一条说明风险（PSK/JWT 凭据明文
  过网）+ 可信网络前提的警告，**不影响函数返回 `Ok`**。
  `docs/DISTRIBUTED.md` "Transport Security (T1.2)"节里的 fail-closed
  凭据检查段落后新增一节"`production` without TLS: warns, does not
  fail startup (U3.4)"，完整写清楚不对称的理由（凭据缺失=无合法场景
  vs. 可信网络明文=已文档化的合法场景）；文末"Dropping the three...
  flags"段落加一个指向新小节的交叉引用。**测试**（均通过）：
  `production_worker_grpc_lacks_tls` 的 3 个纯函数单测（无 TLS + 
  Production→true、有 TLS→false、Dev/Local→false）+ 1 个集成级回归测试
  `build_worker_control_plane_succeeds_without_tls_under_production_when_credentials_present`
  ——直接证明"警告不阻断启动"这个行为本身（凭据齐全、TLS 缺失、
  Production profile 下 `build_worker_control_plane` 仍返回 `Ok`）。
  `cargo test -p agentflow-server`（全部 199+ 项）+ `cargo clippy -p
  agentflow-server --all-features --all-targets -- -D warnings` +
  `cargo check --workspace --lib` 全部通过。Commit: `feat(server): warn
  (not fail-closed) when production worker gRPC lacks TLS`。

### U4 — 长期 backlog（低优先级）

- DONE U4.1 讨论并文档化 `workflow dynamic` 裸命令行调用的默认 profile
  是否应更保守（evaluation §3/§5，T1.3 的已知残留细节）：T1.3 修复后，
  `agentflow workflow dynamic` 未显式传 `--profile` 时仍解析为 `"dev"`
  → `--approve` 默认 `"none"`（无监督执行），这与 `harness run`/`chat`
  默认 `--profile local`（更保守，U2.3）不一致。**方向确认**（动手前
  用 AskUserQuestion 询问用户）：不改默认值，只文档化——保留
  `--profile` 默认 `dev`，不打断依赖"裸命令行=无监督快速迭代"行为的
  现有用户/脚本；在 `--help` 文本和 `docs/HYBRID_WORKFLOW.md` 里醒目
  标注生产/CI 场景需显式传 `--profile local`/`--profile production`。
  **fix**：`agentflow-cli/src/main.rs` `WorkflowCommands::Dynamic` 的
  `profile` 字段 doc comment 改写，明确点出"裸命令行不会提示审批，
  与 `harness run`/`chat` 的 `local` 默认不同，CI/生产请显式传
  `--profile`"，clap 生成的 `--help` 输出已核实包含这段文字。
  `docs/HYBRID_WORKFLOW.md` Governance 一节新增一段醒目 blockquote
  说明这个不对称是有意保留（predates U2.3），并给出替代建议。纯文档/
  doc-comment 改动，不涉及行为变更，因此不新增测试；`cargo fmt -p
  agentflow-cli` + `cargo check -p agentflow-cli --bin agentflow` +
  `cargo clippy -p agentflow-cli --all-features --all-targets -- -D
  warnings` 全部通过，并手动跑 `workflow dynamic --help` 核对渲染
  正常。Commit: `docs(cli): document workflow dynamic's unsupervised
  bare-invocation default`。
- DEFERRED U4.2（延续 T4.2）首方 OTLP exporter（HTTP/gRPC + TLS + 认证）
  （evaluation §4"Tracing/可观测性"，即 Q2.3.3，历次评估中反复记录为
  deferred，本轮复核确认现状无变化）：继续保持 deferred 状态，operator
  自带 `OtelSpanSink` 实现仍是当前推荐路径。本项只是延续 T4.2 的既有
  决定，不是新待办，标成 `TODO` 会误导为"这轮要做"。如果未来决定启动
  实现，届时改回 `TODO` 并按常规流程执行。

## Recently Closed

- **2026-07-30 — T 段整体存档 + 启动 U 段**：T 段（2026-07-29 架构评估
  修复，T0–T4 共 15 项，14 DONE / 1 DEFERRED）全部决策完毕，收口前的
  完整历史存档到
  [`docs/archive/TODOs-archive-2026-07-30-post-t-pre-u-snapshot.md`](docs/archive/TODOs-archive-2026-07-30-post-t-pre-u-snapshot.md)。
  同日基于对 T 段修复效果的五维度独立复核评估
  （`docs/archive/PROJECT_EVALUATION_2026-07-30.md`，综合评级
  A-（有条件），上一版 B+）规划新的 U0–U4 共 15 项待办队列（14 TODO /
  1 DEFERRED）。

> 7/30 之前的 Recently Closed 全部归档在上述历史快照 + 更早的
> [`docs/archive/TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md`](docs/archive/TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md)、
> [`docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md`](docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md)
> 和
> [`docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md`](docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md)。

---

## Deferred / Explicit Non-Goals

（沿用 5/20 版本，无变化；U4 段的一项属于"暂不排期的 backlog"，与下面的
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

- **U0/U1 已全部闭环**（U0.1 顺序 bug + U0.2 完整性校验 + U1.1 跨租户
  绑定 + U1.2 Helm 部署档位 + U1.3 成本熔断入口，均 DONE）——
  `agentflow restore` 现在可视为生产可用的恢复路径；U1.1 是连续三轮
  评估（7/28、7/29、7/30）都点名、始终未被处理的最高严重度标准风险，
  本轮已处理。U2/U3 不阻断，是当前优先级最高的剩余分组。U4 是低优先级
  backlog。
- 每个 U-item 完成后引用 `docs/archive/PROJECT_EVALUATION_2026-07-30.md`
  对应章节 + 本文件里的验收标准逐条对照，不要只写"已修复"三个字。
- 一次只挑一个 U-item；不要在同一 PR 里混不同 crate 的修复。
- 每个 fix 必须配至少一个 regression test 证明问题不会复现；涉及默认值
  变更的（U1.2/U2.3）额外要求一个"验证旧默认值行为不再发生"的测试。
- Commit message 引用 task ID：`Refs U0.1`。
- 涉及设计取舍的项（U1.1/U2.1/U2.2/U3.4/U4.1）在动手实现前先确认方向，
  允许拆分为更小的里程碑或改判为 `DEFERRED`，但必须写明理由，不要放着
  不决策。
- U-item 完成后将状态从 `TODO` 改成 `DONE` 并简述 fix + 测试（参照本文件
  历史归档中 T 段 DONE 项的写法：证据段落 + 验收命令输出）。
- **交付习惯提醒（本轮评估的核心教训）**：T 段修复过程中新增的代码
  （`agentflow restore`）本身又带来了新的 CRITICAL/MAJOR 发现。处理
  U0/U1 时，任何涉及破坏性操作（删除、覆盖、清空）或安全默认值的新代码，
  完成实现后应该额外过一遍"如果这个函数的输入被恶意/误配置，最坏情况是
  什么"这个问题，而不只是验证 happy path 的验收标准。

---

## Quality Gates

每个 task：

- 先读相关代码 + `docs/archive/PROJECT_EVALUATION_2026-07-30.md` 里该项
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

- `docs/archive/PROJECT_EVALUATION_2026-07-30.md` — **本次 U 段的评估
  依据**：对 T 段修复效果的五维度独立复核（架构分层 / 模块完整度 /
  Agent 生态 / 服务层 / 安全性）+ 跨维度优先级表（§6）。
- `docs/archive/PROJECT_EVALUATION_2026-07-29.md` — T 段的评估依据（U 段
  复核的基线）。
- `docs/archive/TODOs-archive-2026-07-30-post-t-pre-u-snapshot.md` — 7/30
  全量快照：T 段（15 项）收口前的完整历史明细。
- `docs/archive/TODOs-archive-2026-07-29-post-r-pre-t-snapshot.md` — 7/29
  全量快照：R 段（12+8 项）收口前的完整历史明细。
- `docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md` —
  7/28 全量快照：H/P-A/S/L 四段收口前的完整历史明细。
- `docs/CURRENT_STATUS.md` — 当前已实现状态。
- `RoadMap.md` / `docs/ROADMAP_v2.md` — 中长期方向。
- `docs/STABILITY.md` / `docs/API_COMPATIBILITY.md` — 稳定面契约。
- `docs/RFC_CRATE_ARCHITECTURE.md` / `docs/RFC_TOOL_CONTRACT_SPLIT.md` /
  `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` — 依赖铁律定义 + T3.3 拆分
  RFC + 架构评估（U2.1/U3.3 涉及）。
- `docs/DISTRIBUTED.md` — 分布式部署形态说明（U3.4 涉及的现状描述）。
- `docs/MEMORY_LAYERING.md` — 四层记忆设计（U2.2/U2.4 涉及）。
- `docs/SERVER_BACKUP_RESTORE.md` — 备份/恢复流程（U0.1/U0.2 涉及）。
- `docs/DEPLOYMENT.md` — 部署指南（U1.2/U3.2 涉及）。
- `HARNESS_MODE_EVOLUTION.md` — Harness Mode 设计规范。
- `docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md` — 更早
  的深度审计修复波次（108 DONE）。
- `docs/archive/TODOs-archive-2026-05-24-p10-optimization-backlog.md` —
  P10 优化 backlog 全部 DONE 项 + 少量 polish 未拾起。
- `docs/archive/TODOs-archive-2026-05-20-closed-segments.md` — 12 个全 closed
  P-段（P0–P9 + P-H + P-LLM + M）。
- `docs/archive/TODOs-archive-2026-05-19-recently-closed.md` —
  5/19 扫出的中段历史。
- `docs/archive/TODOs-archive-2026-05-09-n1-n10.md` + `...05-10-p0-p4.md` —
  N 系列 + 早期 P 系列执行计划历史。
