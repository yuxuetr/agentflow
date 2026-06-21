# AgentFlow TODOs

Last updated: 2026-06-19

## 维护约定

- 旧执行计划按时间分批归档到 `docs/archive/`：
  - `TODOs-archive-2026-05-09-n1-n10.md` — N1–N10 路线图段（已闭环）。
  - `TODOs-archive-2026-05-10-p0-p4.md` — 早期 P-段执行计划（已闭环）。
  - `TODOs-archive-2026-05-19-recently-closed.md` — 5/19 从 Recently Closed
    扫出去的中段历史。
  - `TODOs-archive-2026-05-20-closed-segments.md` — 12 个全 closed 的 P-段
    （P0/P1/P2/P3/P4/P5/P6/P7/P-H/P9/P-LLM/M）整体外迁。
  - `TODOs-archive-2026-05-24-p10-optimization-backlog.md` — **本次 5/24 归档**：
    P10 优化 backlog（v1.0.0-rc.1 ops + 19 个 crate-level 子段），全部 DONE 项
    + 少量未拾起的 polish。其中 polish 项未自动迁移到 Q-段——只有当 Q-段处理
    某个 crate 时主动从档案重新挑选才会回到本文件。
- 本文件是短期执行队列，仅保留 **Q-段 (2026-05-24 深度审计驱动)** + 最近 closed 摘要。
- 最新评估：`docs/audit/README.md`（per-crate 16 份 + 总览，覆盖 26 CRITICAL /
  110 MAJOR / 184 MINOR 个 finding）。
- 上一份高层评估：`docs/archive/PROJECT_EVALUATION_2026-05-19.md`（A overall）。
  本次审计在更深的代码层面找到了那份评估未触及的关键问题，主要集中在
  RoadMap P1（安全）/ P2（多租户）/ P5（worker 上线）三段——它们在 P10 backlog
  里被标为"基本就绪"，但深审拆出了具体的 CRITICAL bug 仍未修。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态。
- `RoadMap.md` 保留中长期路线。
- `HARNESS_MODE_EVOLUTION.md` 是 Harness Agent Mode 的设计规范。
- 任务状态只使用：
  - `TODO`：未开始或正在执行。
  - `DONE`：已完成、已测试、已提交。
  - `DEFERRED`：显式推迟到 RoadMap Later Tracks 或 Non-Goals。

## Active Queue Overview

Current focus: **Q-段 — Audit Remediation (2026-05-24 deep audit)**。

| Segment | Theme | Status |
| --- | --- | --- |
| P0 → P9 / P-H / P-LLM / M / P10 | 历史段，全部 closed 或外迁 | ARCHIVED |
| **Q1** | **Wave 1: production-blocking security**（多租户 / sandbox / 凭据 / Worker 认证 / Harness 脱敏） | **NEW — active** |
| **Q2** | **Wave 2: correctness / data integrity**（SQLite 健壮性 / 事件丢失 / 确定性 / production panic） | **NEW — active** |
| **Q3** | **Wave 3: productization hygiene**（graceful shutdown / MCP 健壮性 / marketplace 完整性 / CLI 静默 bug） | **NEW — active** |
| **Q4** | **Wave 4: docs ↔ reality reconciliation**（CLAUDE.md / RoadMap.md 漂移） | **NEW — active** |
| **Q5** | **Cross-cutting sweeps**（unwrap/expect / redaction / 信号处理） | **DONE** |
| **H** | **Harness Mode follow-ups**（RFC loop-ownership + `harness chat` 收尾/打磨） | **NEW — backlog** |
| **P-A** | **契约内核 + 架构演进**（dynamic workflow 统一；见 `docs/RFC_CRATE_ARCHITECTURE.md`） | **NEW — backlog（Q 安全波次之后）** |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## H — Harness Mode follow-ups (post loop-ownership + chat)

> 来源：`docs/RFC_HARNESS_LOOP_OWNERSHIP.md`（已实现并合并，PR #2）+
> `agentflow harness chat`（已实现并合并，PR #3）。**核心已生产可用、全绿、进
> main**；以下都是收尾打磨或主动推迟项，**无任何生产阻断**。状态：`TODO` =
> 可做的收尾增强；`DEFERRED` = 需设计或属 RoadMap non-goal。

### H.1 — `step_started` 实时排序（RFC Phase 1 残留）

- TODO H.1.1 turn-driven 模式下实时发 `step_started`
  - 来源：`docs/RFC_HARNESS_LOOP_OWNERSHIP.md` §5.6 / Phase 1 诚实残留。
  - 现状：tool/approval 事件已在同一 seq 时钟实时交错；但 `step_started`
    仍由 `translate_inner_events` 事后批量发（信息性,不影响 tool/approval
    配对）。
  - 目标：harness 驱动 turn 时已知 turn 边界,可让 `ReActAgent` 实时发
    `AgentEvent::StepStarted`,bridge 实时映射,使事件流逻辑顺序完全一致。
  - 注意:agent 当前不发 `StepStarted`(只记 steps);需在 react loop 加发射点。

### H.2 — `harness chat`：REPL 集成审批（替换守卫）

- TODO H.2.1 让 `--approve cli` 在 chat 里真正可用
  - 现状：chat REPL 独占 stdin,`CliApprovalProvider`(阻塞 std stdin)会和它
    抢字节,所以 `harness chat --approve cli` 被**启动守卫拒绝**(PR #3)。
  - 目标:实现一个从 REPL 共享行读取器取输入的自定义 `ApprovalProvider`,
    审批 prompt 走同一个 stdin 通道(channel),解除守卫,交互审批在 chat 可用。

### H.3 — `harness chat`：流式输出（可选 UX）

- TODO H.3.1 边出边打
  - 现状:答案整轮跑完一次性打印(仅有 TTY 的 "⏳ thinking…" 指示)。
  - 目标:利用 harness live 事件 / LLM streaming,逐 token 或逐步骤打印。
  - 注意:`--model` 路径无工具时一轮就是一次 LLM 调用,价值主要在带工具/多步场景。

### H.4 — `harness chat`：readline / 历史（可选 UX）

- TODO H.4.1 上下方向键历史 + 行内编辑
  - 现状:裸 stdin 行读(与 `skill chat` 一致,全仓库未用 rustyline)。
  - 目标:接入 readline(rustyline 或类似),提供历史 + 行编辑 + 补全。
  - 注意:属**全仓库一致性决策**——应同时给 `skill chat` 一起接,避免风格分裂;
    且需处理 rustyline(阻塞)与 tokio 运行时的集成。

### H.5 — `harness chat`：`/clear` 命令（可选）

- TODO H.5.1 清空当前 session 对话记忆(保留 id)
  - 现状:有 `/new`(开新 session)可达到"重开";`/clear` 是"原地清空"。
  - 注意:`--model` 路径清 run-dir memory_db 即可;`--skill` 路径记忆由 manifest
    决定,`/clear` 可能不影响其真实对话——需先解决后端定位才不产生困惑。

### H.6 — 服务端多节点共享 memory backend（DEFERRED）

- DEFERRED H.6.1 跨节点的 harness 对话记忆
  - 现状:`AGENTFLOW_HARNESS_MEMORY_DB` opt-in 用共享 SQLite 文件(单节点假设),
    已写进 `docs/DEPLOYMENT.md`。
  - 推迟原因:多节点部署需要 Postgres-backed 或外部 `MemoryStore`,属架构决策,
    待真实多节点需求出现再设计(对应 `docs/ROADMAP_v2.md` Theme B/C)。

### H.7 — skill 路径 resume 统一（DEFERRED）

- DEFERRED H.7.1 让 `--skill` 路径也默认/可选持久化
  - 现状:`--model` 路径默认持久化(SqliteMemory);`--skill` 路径记忆由 skill
    manifest 配置(`memory.type = sqlite` 才持久)——这是**刻意的**(不应覆盖
    skill 的记忆选择)。
  - 候选:加 `--persist-memory` 显式覆盖位,或文档引导 skill 作者配 sqlite。
    需 `ReActAgent::with_memory()`(已存在)注入。无需求前不做。

### H.8 — H6 高级兼容（DEFERRED / RoadMap non-goal）

- DEFERRED H.8.1 slash-command 生态扩展 / TUI 产品壳 / OpenHarness 配置导入 /
  第三方 agent 框架适配器。
  - 来源:`docs/H6_PROMOTION_CRITERIA.md` + `docs/ROADMAP_v2.md` §F。
  - 逐项按需 promote,各自需独立 RFC;TUI 壳与 provider 订阅桥是 `RoadMap.md`
    明确 non-goal。

## P-A — 契约内核 + 架构演进（contract kernel · post-Q backlog）

> 来源：`docs/RFC_CRATE_ARCHITECTURE.md`（2026-06-19）。把四范式（静态 DAG /
> 原生循环 / harness / **dynamic workflow**）收敛到一个窄腰契约内核 + 八条依赖
> 铁律，采用**绞杀式原地演进，不重写**。**排序：在 Q1/Q2 production-blocking
> 安全与正确性波次之后**——架构重构不得插队安全修复。每个 PR 只改一件事，旧路径
> `pub use` 兼容，`cargo test` + `clippy -D warnings` 全绿。
>
> 依赖：P-A0 → P-A1 →（P-A2 ∥ P-A3）→ P-A4。

### P-A0 — 立约 + 架构守卫

- TODO P-A0.1 落地 `docs/RFC_CRATE_ARCHITECTURE.md`，在 `RoadMap.md` /
  `docs/ARCHITECTURE.md` 加交叉引用。
- TODO P-A0.2 `xtask check-arch`：解析各 `Cargo.toml`，断言 RFC §7 八铁律；现有
  违例（`harness→agents` / `worker→server` / `server→cli` / reliability 两套）
  写入 allowlist 逐条烧空。
- TODO P-A0.3 `xtask check-arch` 接入 CI Quality workflow，违例即红。

### P-A1 — 契约内核抽取 + 垂直切片

- TODO P-A1.1 新建 `agentflow-agent-spi`（AgentRuntime / AgentEvent /
  HarnessEvent / EventSink / Approval* + 新写 `Capability` / `Lowered`），对象
  安全 + `…Ext` 拆分；旧路径 re-export。
- TODO P-A1.2 新建 `agentflow-store-spi`（KnowledgeBackend / MemoryStore）。
- TODO P-A1.3 从 `agentflow-core` 拆 `agentflow-graph`（IR），`core` 仅留
  FlowExecutor；re-export 兼容。
- TODO P-A1.4 抽 `agentflow-async-util`（retry / timeout / cancel 组合子）。
- TODO P-A1.5（可延后）抽 `agentflow-value`（FlowValue）叶子 crate。
- TODO P-A1.6 垂直切片 spike `examples/dynamic_workflow_spike.rs`：agent 产出
  Flow → core 执行，验证内核可承载 dynamic workflow。

### P-A2 — 运行时解耦（去横向依赖）

- TODO P-A2.1 `harness` 依赖 `agents` → `agent-spi`，运行时由前门注入。
- TODO P-A2.2 harness 经 EventSink 契约治理 `Flow` 执行。
- TODO P-A2.3 抽 `agentflow-worker-proto`，删 `worker→server`。
- TODO P-A2.4 抽共享 assembly，删 `server→cli`。

### P-A3 — 可靠性合并 + 类型加固

- TODO P-A3.1（前置）加厚 `agentflow-agents/src/react/agent.rs` 循环测试覆盖。
- TODO P-A3.2 timeout×cancellation `select!` 抽 `async-util::race_with_limits`，
  core/agents 共用（消两套实现 + 重复矩阵债）。
- TODO P-A3.3 `Session<Active|Finished>` type-state（SessionFinished 提前到编译期）。
- TODO P-A3.4 `Seq` newtype + `SeqAllocator::stamp`（消事件 seq-vs-write 乱序 race）。
- TODO P-A3.5 `ByteSafeStr` / 仅暴露 `.chars().take(n)`（消 UTF-8 切片 panic）。
- TODO P-A3.6 `Validated<ModelId>`（消 chat REPL 切换失败脏状态）。
- TODO P-A3.7 契约 enum 加 `#[non_exhaustive]` + sealed + thiserror 边界枚举。

### P-A4 — Dynamic workflow + RAG 归位（收口）

- TODO P-A4.1 `rag` impl `KnowledgeBackend` + 暴露 `rag_search` 工具；
  `rag search/index` CLI 降为运维子命令（保留 eval harness）。
- TODO P-A4.2 `skills` 分层知识解析；`SKILL.md` `knowledge:` 加 `backend:`（默认
  `files`，opt `rag`）。
- TODO P-A4.3 `Capability::lower` 在 `skills` 落地（Skill 降解为 工具+上下文）。
- TODO P-A4.4 `PlanExecuteAgent` 产出真正的 `Flow`（带依赖/并行/条件），交
  `core::FlowExecutor` 执行，节点可为 `AgentNode`。
- TODO P-A4.5 spike 产品化为一等 dynamic-workflow 路径（并行 verifier 节点 +
  收敛判定）。
- TODO P-A4.6 文档：`docs/HYBRID_WORKFLOW.md` 增补 dynamic workflow；
  `docs/ARCHITECTURE.md` 更新四范式 + 契约内核图。


## Audit Assessment Summary (2026-05-24)

### Drift between RoadMap promises and code reality

- **RoadMap P1 (Security)** 标 CLOSED；但深审在 `agentflow-tools` 找到 6 个
  CRITICAL 沙箱问题（shell 元字符旁路、seccomp 不实际拦 `openat(O_CREAT)`、
  macOS SBPL 过宽、`SandboxPolicy.allowed_paths` 空集变放行），在 `agentflow-nodes`
  找到 `FileNode` / `HttpNode` 完全旁路了 `agentflow-tools` 的 SandboxPolicy。
  → P1 的"硬化 HTTP/file/shell 执行"声明实际未达成。详见 Q1.1–Q1.3。
- **RoadMap P2 (Server)** 标 CLOSED；但深审在 `agentflow-server` 找到 3 个
  CRITICAL 多租户边界漏洞（`list_runs ?tenant_id=` 覆盖 header、
  `list_harness_sessions` 忽略租户上下文、所有 submit 端点从 body 拿
  tenant_id），叠加 `agentflow-db` 找到 `SkillInstallRepo::list()` 无租户
  过滤、`mcp_sessions` 无 `tenant_id` 列。"多租户"目前是软提示而非安全边界。
  → P2 的"tenant/session policy"声明实际未达成。详见 Q1.4–Q1.5。
- **RoadMap P5 (Worker)** 标 CLOSED；但深审找到 worker↔server gRPC 通道无 TLS、
  无 auth，PSK/JWT 策略只在 server 端配置但**从未在 wire 上强制执行**。当前
  distributed mode 仍为"单租户可信网络"专用。详见 Q1.6。
- **P-H.5 Harness 集成** 标 CLOSED；但深审找到 `HarnessRuntime` 内部 `seq` 与
  `HookConfig.seq` 是两个独立计数器——破坏 Beta 冻结契约里"monotonic, never
  gap"承诺；`ApprovalRequest.params_summary` / `ToolCallRequestedPayload.params_summary`
  原样 `params.clone()` 进入 JSONL/SSE，未调用 `agentflow-tracing::redaction`。
  详见 Q1.7。
- **README.md "Production-Ready Stability"** 章节宣称 retry / timeout / health /
  checkpoint 已交付；但深审找到 retry 把所有错误抹平成 `RetryExhausted`、
  `ExponentialBackoff` jitter < 4ms 会 panic、整个 workspace 无任何 graceful
  shutdown。详见 Q2.7, Q3.1。
- **CLAUDE.md docs 漂移**：db "8 张表" 实际 9 张；server "FlowRunExecutor 计划
  v0.4.0 上线" 已上线；nodes "per-modality feature gate" 不存在；mcp adapter
  实际位置在 `agentflow-skills`；rag "StepFun embedding" 未实现。详见 Q4。

### Per-crate finding counts (from `docs/audit/`)

| Crate | C | M | m | Hot spot |
|---|---|---|---|---|
| agentflow-tools | **6** | 9 | 12 | Sandbox 关键 bug |
| agentflow-server | 3 | 7 | 13 | 多租户边界 + 无 graceful shutdown |
| agentflow-nodes | 3 | 9 | 14 | SSRF / 路径遍历 / 静默假数据 |
| agentflow-llm | 2 | 7 | 12 | Google key 泄漏 / 无 HTTP timeout |
| agentflow-mcp | 2 | 6 | 11 | JSON-RPC id 不关联 / stderr 不排空 |
| agentflow-memory | 2 | 9 | 10 | 无 WAL/busy_timeout / sqlite URL bug |
| agentflow-harness | 2 | 7 | 12 | seq 命名空间分裂 / 无 redaction |
| agentflow-ui | 2 | 6 | 10 | Token 存 localStorage / EventSource 降级 |
| agentflow-rag | 1 | 6 | 12 | OpenAI 150× 欠批 |
| agentflow-tracing | 1 | 11 | 12 | Drain panic / OTLP exporter 未实现 |
| agentflow-db | 1 | 5 | 12 | `SkillInstallRepo::list` 无租户过滤 |
| agentflow-worker | 1 | 6 | 10 | gRPC 无 TLS / 无 auth / 无重连 |
| agentflow-core | 0 | 7 | 10 | 非确定性拓扑排序 / orphan robustness.rs |
| agentflow-agents | 0 | 5 | 12 | expect() 在 batch dispatch / plan-execute 忽略 token_budget |
| agentflow-skills | 0 | 4 | 10 | Marketplace 假签名 / 路径遍历 |
| agentflow-cli | 0 | 6 | 12 | `audio asr --prompt` 静默写文件 / 无 Ctrl-C |
| **TOTAL** | **26** | **110** | **184** | |

---

## Q1 — Wave 1: Production-Blocking Security

**Rationale**: 这些是 v1.0.0-rc.1 → v1.0 GA 切片之间必须先解决的 hard blocker。
所有 Q1 item 都涉及租户边界、身份验证、沙箱、或凭据/PII 泄漏。在 Q1 全部 DONE
之前 **不应该 cut v1.0.0 tag**。

### Q1.1 — agentflow-tools: ShellTool / seccomp / SBPL 沙箱旁路（CRITICAL × 4：C1/C3/C4/C5）

- DONE Q1.1.1 ShellTool `sh -c` 元字符旁路
  - 审计来源：`docs/audit/agentflow-tools.md` C1
  - 修复：引入 `ShellInterpretation::{Argv, Shell}` 枚举，默认 `Argv`：
    inline parser 解析 argv 同时拒绝任何引号外的 `|`/`;`/`&`/`$`/`` ` ``/
    `>`/`<`/`(`/`)`/`\n`；`with_shell_interpretation()` opt-in 才走 `sh -c`，
    且要求 `backend.is_enforcing() == true`（Noop 直接 fail-closed）。
    `awk 'BEGIN { ... }'` 这类带引号的合法用法仍然能 parse。
  - 测试：新增 6 个 regression test：`;`/`&&`/`$()`/`|`/`` ` `` 在 Argv 模式
    必拒；Shell 模式 + Noop backend 必拒；`awk` 单引号参数仍 round-trip 正常。
    sandbox_macos `>` 重定向用例切到 `with_shell_interpretation()`。
- DONE Q1.1.2 Linux seccomp 不实际拦 `openat(O_CREAT|O_WRONLY)`
  - 审计来源：`docs/audit/agentflow-tools.md` C4
  - 修复：新增 `install_write_open_rules`：当 `FsWrite` 能力缺失时，
    `openat`/`open` 按 `O_WRONLY`/`O_RDWR`/`O_CREAT`/`O_TRUNC` 每位单独
    `MaskedEq` 规则拦截；`openat2` 因 `struct open_how` 无法 deref 而无条件拦；
    `creat` 在 x86_64 上无条件拦（aarch64 上无此 syscall）。
  - 测试：`write_open_rules_emit_per_flag_under_no_fs_write` 单测确认 4 条
    规则按 flag bit 分别下发；新增 Linux 集成测试
    `linux_seccomp_blocks_openat_with_o_creat_when_fs_write_absent` 用
    `os.open(..., O_WRONLY|O_CREAT)` 验证 EPERM + 文件未创建（依赖
    Linux CI 跑）。
- DONE Q1.1.3 macOS SBPL profile 授权过宽
  - 审计来源：`docs/audit/agentflow-tools.md` C3（`sandbox/macos.rs:120-133`）
  - 修复：移除 `(subpath "/Library")` 和 `(subpath "/private/etc")` 的 blanket
    授权，替换为 `/Library/Frameworks` subpath + 三个 literal 文件（
    `/Library/Preferences/.GlobalPreferences.plist`、`/private/etc/localtime`）。
    DNS 解析所需的 `/private/etc/resolv.conf`、`/hosts`、`/services` 仅在
    `Capability::Net` 被授予时才暴露。`/private/etc/master.passwd` 显式不在
    任何路径下被授权。
  - 测试：单测 `profile_does_not_grant_blanket_library_or_private_etc` +
    `profile_only_exposes_resolver_files_when_net_capability_present` 验证
    profile 文本；集成测试 `macos_sandbox_denies_blanket_library_read` 跑
    真实 sandbox-exec 确认 `/Library/Preferences/SystemConfiguration/preferences.plist`
    读取被内核拒绝。
- DONE Q1.1.4 Linux seccomp 不拦 `clone` / `fork` / `execve`（子进程逃逸）
  - 审计来源：`docs/audit/agentflow-tools.md` C5（`sandbox/linux.rs:22-24`）
  - 修复：`compile_filter` 在 `!Exec` 时插入
    `clone` / `clone3` / `execve` / `execveat` 的无条件拦截，x86_64 上额外
    拦 `fork` / `vfork`（aarch64 上这两个 syscall 不存在，glibc 路由到 `clone`）。
    实践中这是 defense-in-depth——in-process 层早就拒掉没有 Exec 的工具——
    但 BPF 现在与文档承诺一致。
  - 测试：单测 `process_creation_syscalls_denied_when_exec_capability_absent`
    断言规则集中存在；集成测试
    `linux_seccomp_no_exec_filter_blocks_child_spawn` 让 no-Exec backend
    包裹 `/bin/true`，spawn 必须失败（execve 被拦）。Linux CI 跑。

### Q1.2 — agentflow-tools: SandboxPolicy 默认值反转 + HttpTool panic（CRITICAL × 2：C2/C6）

- DONE Q1.2.1 `SandboxPolicy.allowed_paths` 空集语义反转
  - 审计来源：`docs/audit/agentflow-tools.md` C6（`sandbox/policy.rs:104-112`）
  - 修复：新增 `allow_all_commands` / `allow_all_paths` 两个 explicit bool
    bypass 位。空 `allowed_paths` 现为"全拒"（denial reason 说明显式提示），
    与 `allowed_commands` 对称；`SandboxPolicy::permissive()` 把两个 bypass
    位显式置 `true`。`ScriptTool::with_default_policy` 在默认 policy 上把
    `scripts_dir` 加进 `allowed_paths`，否则 ScriptTool 找不到自己的脚本。
    `..SandboxPolicy::default()` 结构体更新形式调用方零改动（默认 false 即
    安全态）。`allowed_domains` 出于操作员负担考虑保留原语义。
  - 测试：`default_policy_denies_arbitrary_paths` 直接验证 `/etc/passwd` 被
    拒；`default_policy_only_allows_curated_command_set` 验证 `rm` 被拒；
    `permissive_policy_sets_explicit_allow_all_bits` 验证 permissive 行为；
    `explicit_allowed_paths_still_filter_after_flip` 用 TempDir 验证
    canonicalization 路径仍正确。
  - 文档：`docs/STABILITY.md` 新增"Migration Notes / Q1.2.1"小节。
- DONE Q1.2.2 HttpTool 在 `Client::build()` 失败时 panic
  - 审计来源：`docs/audit/agentflow-tools.md` C2（`builtin/http.rs:39-42`）
  - 修复：`HttpTool::new` 签名改为 `Result<Self, ToolError>`，
    `Client::build()` 错误映射为 `ToolError::ExecutionFailed`；
    `HttpTool::default_policy` 同步改返回 `Result`。
  - 测试：`new_returns_result_so_callers_can_handle_build_failures` 单测
    锁定签名（无法在单测中真实触发 build 失败，但类型即合约）。
  - 调用方 (`agentflow-skills::build_tool_registry`、
    `agentflow-tools::tool_policy_sandbox_demo`、`agentflow-agents::react_agent`、
    lib 顶层 doctest) 全部更新；`SkillError::ToolBuildError` 新增承接。
- DONE Q1.2.3 HttpTool 缺 `.no_proxy()` 风险 CI 抖动
  - 审计来源：`docs/audit/agentflow-tools.md` M1
  - 修复：新增 `HttpTool::with_client(client, policy)` 注入 API；in-source
    tests 改用 `test_client()` helper 调用
    `Client::builder().no_proxy().build()`，避免开发机/CI 上 system HTTP
    proxy 把 loopback 请求黑洞掉。生产 `HttpTool::new()` 仍走默认 client
    （生产命中真实公网，代理工作正常）。
  - 测试：6 个现有 HTTP 测试切换到 no-proxy client（loopback / SSRF /
    redirect / private-IP / cloud-metadata 拦截），全部通过。

### Q1.3 — agentflow-nodes: FileNode / HttpNode / TextToImageNode 安全旁路（CRITICAL × 3）

- DONE Q1.3.1 `FileNode` 旁路 `agentflow-tools::SandboxPolicy`
  - 审计来源：`docs/audit/agentflow-nodes.md` C3
  - 修复：`agentflow-nodes` 现在直接依赖 `agentflow-tools`；`FileNode` 改为
    带 `Arc<SandboxPolicy>` 字段，默认策略 `permissive()` 保持向后兼容，
    但 `..`/parent-dir 遍历无条件拒、symlink 读取无条件拒、hardlink 计数
    + max_file_read_bytes 由 policy 控。`with_policy(...)` builder 接收
    严格策略。所有 unit-struct caller (`FileNode` → `FileNode::default()`)
    更新：cli factory、worker, bench。
  - 测试：4 个新单测覆盖 round-trip / `..` 拒 / 策略外路径写拒 / symlink 拒。
- DONE Q1.3.2 `HttpNode` 无 timeout / 无 redirect cap / 无 SSRF 防护
  - 审计来源：`docs/audit/agentflow-nodes.md` C2
  - 修复：`HttpNode` 改为 `Arc<SandboxPolicy>` + 内嵌 `HttpTool` 委托。
    `HttpTool` 同步扩展支持 `PUT`/`DELETE`/`PATCH`/`HEAD`（RFC 7231 幂等性
    一并更新）+ `with_max_response_chars` builder（node 用 `usize::MAX`
    避免截断）。默认拿到 SSRF 防御 / 30s timeout / 10-redirect cap /
    cloud-metadata 拦截 / private-IP 拦截。
  - 测试：单测覆盖 cloud metadata SSRF 拒 / private IP 拒 / explicit
    policy allow loopback。
- DONE Q1.3.3 `TextToImageNode` 在 API 失败时静默返回 1×1 PNG
  - 审计来源：`docs/audit/agentflow-nodes.md` C1（`text_to_image.rs:397-441`）
  - 修复：删除 `execute_mock_image_generation` fallback；上游失败现在
    surface 为 `AgentFlowError::AsyncExecutionError` 携带 provider 错误
    信息。Mock 用法迁移到显式 `MockNode` / DAG 条件分支。
  - 测试：旧 `test_text_to_image_node_execution` 改为
    `execute_propagates_upstream_failure_instead_of_returning_mock`，用
    无效 model 验证上游失败 → Err 而非 Ok(placeholder)。

### Q1.4 — agentflow-server: 多租户边界（CRITICAL × 3）

- DONE Q1.4.1 `list_runs` 接受 `?tenant_id=` 覆盖 header 绑定
  - 审计来源：`docs/audit/agentflow-server.md` C1
  - 修复：`ListRunsQuery::tenant_id` 字段删除；`list_runs` 唯一从
    `Extension<TenantId>` 取值。文档同步更新（handler doc + Q1.4.1 注释）。
  - 测试：`list_runs_query_param_overrides_header` 改为
    `list_runs_ignores_unknown_tenant_id_query_param`——即使附带也只
    读 header；migrate offset/status/empty-page 测试用 header 而非
    query。
- DONE Q1.4.2 `list_harness_sessions` 完全忽略租户扩展
  - 审计来源：`docs/audit/agentflow-server.md` C2
  - 修复：handler 注入 `Extension<TenantId>`；`ListHarnessSessionsQuery::tenant_id`
    字段删除；repo 查询使用 header-bound tenant。
  - 测试：`list_sessions_returns_newest_first` migration 到 header
    binding；`submit_for_tenant` helper 同步带 header。
- DONE Q1.4.3 Submit 端点从 body 拿 tenant_id
  - 审计来源：`docs/audit/agentflow-server.md` C1 后段
  - 修复：`submit_run` / `submit_harness_session` / `run_skill` 都注入
    `Extension<TenantId>` 并以 header 为真值；body 中的 `tenant_id` 字段
    保留（向后兼容字段形状）但必须 match header 否则返回新
    `ApiError::TenantMismatch`（HTTP 403 + code `tenant_mismatch`）。
  - 测试：`submit_run_rejects_body_tenant_id_that_disagrees_with_header`
    断言 mismatch 403；`submit_run_accepts_body_tenant_id_that_matches_header`
    断言匹配仍 200。harness `submit_for_tenant` helper 同步 echo 字段。
  - error 模型：`ApiError::TenantMismatch(String)` 新增 variant，映射
    HTTP 403 + `tenant_mismatch` code（区别于通用 `forbidden`）。

### Q1.5 — agentflow-db: 租户过滤 + schema 缺列（CRITICAL × 1, MAJOR × 2）

- DONE Q1.5.1 `SkillInstallRepo::list()` 缺租户过滤
  - 审计来源：`docs/audit/agentflow-db.md` C1（`repo.rs:434-442`）
  - 修复：`list(&self, tenant_id: &str)` 签名 + SQL `WHERE tenant_id = $1`。
  - 测试：新增 `skill_install_repo_list_filters_by_tenant` —— 两个不同
    tenant 各插一行，列出 alpha 不含 beta，反之亦然。
- DONE Q1.5.2 `mcp_sessions` 表无 `tenant_id` 列
  - 审计来源：`docs/audit/agentflow-db.md` M1
  - 修复：migration `0006_mcp_sessions_tenant_id.sql` 加 `tenant_id TEXT
    NOT NULL DEFAULT 'default'` 列 + 复合 index `(tenant_id, started_at DESC)`；
    `McpSession` 模型新增 `tenant_id` 字段；`open(...)` 写入。
- DONE Q1.5.3 `events.list_after` / `harness_events.list_after` 忽略 tenant_id
  - 审计来源：`docs/audit/agentflow-db.md` M2
  - 修复：两个 trait 方法签名加 `tenant_id: &str`；`events.list_after` SQL
    加 `WHERE tenant_id = $1`（复合 index 命中）；`harness_events.list_after`
    用 JOIN harness_sessions 实现 tenant 过滤（harness_session_events 表无
    tenant 列，避免另一次 migration）。server `events_stream.rs` /
    `harness.rs` / `runs.rs` 所有 caller 同步更新；`next_event_seq` 加
    tenant 参数；`publish_cancellation_event` / `publish_cancel_event` 传递
    tenant_id。

### Q1.6 — agentflow-worker: gRPC 通道无认证（CRITICAL × 1）

- DONE Q1.6.1 Worker↔server gRPC 通道无 TLS、无 auth
  - 审计来源：`docs/audit/agentflow-worker.md` C1
  - 修复：(1) 新增 `AuthenticatedGrpcWorkerService<P>` —— 包装
    `AuthenticatedControlPlane<P>`，每个 `claim_task` / `report_result` /
    `heartbeat` 从 `authorization` gRPC metadata 提取 PSK 构造
    `WorkerCredential`，走 `AuthenticatedControlPlane` 已有的 PSK/JWT 校验。
    Admission 失败映射到 `Status::permission_denied`；(2) `GrpcWorkerProtocol`
    新增 `.with_admission_token(token)` builder，每次 RPC 注入
    `authorization: Bearer <token>` 元数据；(3) `agentflow-worker` CLI 加
    `--admission-token` (fallback `AGENTFLOW_ADMISSION_TOKEN`)；缺失时启动
    打印警告。TLS flags (`--server-ca` / `--client-cert` / `--client-key`)
    CLI 接受但 channel 尚未接入，留 Q3 follow-up。
  - 测试：`grpc_claim_without_admission_token_is_rejected` 端到端：spawn
    `AuthenticatedGrpcWorkerService` server + 无 token 的
    `GrpcWorkerProtocol` 客户端，`claim_task` 必须收到
    permission_denied；带正确 token 同一客户端能取到任务。
  - 用户决策：PSK + gRPC interceptor（非 JWT）；TLS 只加 CLI flag，不提供
    生成脚本，由 ops 自己准备证书。

### Q1.7 — agentflow-harness: 冻结契约破坏 + 凭据/PII 泄漏（CRITICAL × 2）

- DONE Q1.7.1 `seq` 命名空间分裂破坏 monotonic 契约
  - 审计来源：`docs/audit/agentflow-harness.md` C1
  - 修复：`HarnessRuntime` 用 `Arc<AtomicU64>` 替换本地 `mut seq`，新增
    `with_seq_counter(Arc<AtomicU64>)` builder + `seq_counter()` accessor。
    `with_initial_seq(n)` 改为 `seq_counter.store(n)`。`translate_inner_events`
    签名从 `&mut u64` 改为 `&AtomicU64`，所有 seq 通过 `fetch_add` 拿。
    `agentflow-server::harness_live` 现在创建一个共享
    `Arc<AtomicU64>::new(inputs.initial_seq)`，同时塞给 `HookConfig::with_seq_counter`
    和 `HarnessRuntime::with_seq_counter`——两条 emission 路径共享同一 atomic。
  - 测试：`shared_seq_counter_keeps_runtime_and_hook_emissions_monotonic`
    模拟 hook 在 runtime 首次 emit 前 fetch_add(0)，断言所有 seq 严格递增、
    无重复、`final_event_seq == max(all seqs)`。
- DONE Q1.7.2 ApprovalRequest / ToolCallRequested payload 无脱敏
  - 审计来源：`docs/audit/agentflow-harness.md` C2
  - 修复：`agentflow-harness` 新增 `agentflow-tracing` 依赖；
    `HookedTool::execute_with_event` 在构造 `ApprovalRequest` 前对
    `params_summary` 跑 `redaction::redact_value` (默认 RedactionConfig)；
    `runtime::tool_call_requested_from_step` 同样脱敏。
  - 测试：(1) `tool_call_requested_redacts_sensitive_params_before_emit`
    在 runtime path 验证 `Bearer sk-live-...` 和 `api_key` 不再进事件;
    (2) `approval_request_redacts_sensitive_params_before_emit` 在 hook
    path 验证 ApprovalRequested 事件中同样的 token 被替换为 `[REDACTED]`,
    非敏感字段（URL）仍保留。

### Q1.8 — agentflow-llm: Google API key 泄漏 + 无 HTTP timeout（CRITICAL × 2）

- DONE Q1.8.1 Google `?key=<API_KEY>` 通过 reqwest Error 泄漏
  - 审计来源：`docs/audit/agentflow-llm.md` C1
  - 修复：Google provider 把 API key 从 URL 移到 `x-goog-api-key` header；
    `build_headers` 返回 `Result<HeaderMap>`（非 ASCII key 报
    `ConfigurationError`，避免 panic）；`get_model_endpoint` /
    `validate_config` URL 不再拼 `?key=...`。
  - 测试：`google_model_endpoint_url_no_longer_carries_api_key` 直接断言
    URL 不含 `key=` 或密钥值；`build_headers_injects_traceparent_when_scope_active`
    扩展为同时验证 `x-goog-api-key` header 存在；旧
    `test_model_endpoint` flip 成断言 URL **不含** test-key。
- DONE Q1.8.2 6 个 provider 的 default_http_client 无 timeout
  - 审计来源：`docs/audit/agentflow-llm.md` C2
  - 修复：`providers::default_http_client` 现在显式
    `.connect_timeout(DEFAULT_HTTP_CONNECT_TIMEOUT_SECS=10s)` +
    `.timeout(DEFAULT_HTTP_REQUEST_TIMEOUT_SECS=600s)`。
    `LLMError::From<reqwest::Error>` 的 `timeout_ms` 字段引用同一常量，
    不再硬编码 30000（与实际不符）。所有 provider 共用 `default_http_client`
    所以一处 fix 覆盖 6 个 provider。

### Q1.9 — agentflow-ui: Token 与 EventSource 安全面（CRITICAL × 2）

- DONE Q1.9.1 Bearer token 持久化到 `localStorage`
  - 审计来源：`docs/audit/agentflow-ui.md` C1（`main.tsx:2581-2583`）
  - 修复：新增 `readSessionStorage` / `writeSessionStorage` helper；
    `App` 组件的 `apiToken` state 从 sessionStorage 读、写。token 不再
    跨 tab restart 持久化；XSS payload 也只能拿到 tab-scoped 值。
- DONE Q1.9.2 Harness Mode SSE `EventSource` 无法发 Authorization
  - 审计来源：`docs/audit/agentflow-ui.md` C2（`main.tsx:1899`）
  - 修复：harness session detail page 的 EventSource 用 `apiFetch` +
    `ReadableStream` 自实现 SSE：手动按行 / 双换行 parse，
    `Authorization: Bearer <token>` 头跟着请求一起发。AbortController
    管理生命周期；non-OK 响应或流断开后等 5s 重连，重连期间继续 5s 轮询
    history endpoint 作为 fallback。

### Q1.10 — agentflow-skills: marketplace 完整性（MAJOR × 2，但安全相关，提到 Q1）

- DONE Q1.10.1 `ChecksumSha256SignatureVerifier` 是自校验不是签名
  - 审计来源：`docs/audit/agentflow-skills.md` M2
  - 修复：新增 `Ed25519SignatureVerifier`，使用 `ed25519-dalek` 2.x
    pure-Rust crate（已加入 `agentflow-skills` 依赖）。Publisher key 静态
    加载——目录默认 `~/.agentflow/marketplace-keys/`，每个文件名为
    `<key_id>.pub`，内容是 base64-encoded 32 字节 raw Ed25519 公钥。
    Marketplace catalog 里 `[signature]` block 用 `algorithm = "ed25519"`
    + `key_id = "..."` + base64 detached signature。`require_signature`
    默认 true（production 拒未签名）；local fixture 可 opt out。`key_id`
    用作文件名所以拒 `..`/`/`/`\`。`ChecksumSha256SignatureVerifier`
    保留用于 fixture，但 `docs/STABILITY.md` 新增 Q1.10.1 章节明确
    production 必须显式切换到 Ed25519 verifier。
  - 测试：4 个新单测——签名通过/被篡改的 artifact 拒/缺
    `[signature]` 拒/路径遍历 `key_id` 拒。
  - 用户决策：静态配置文件方式（vs JWT / TOFU / remote registry）。
- DONE Q1.10.2 marketplace install 缺路径遍历守卫 + 缺大小上限
  - 审计来源：`docs/audit/agentflow-skills.md` M1, M3
  - 修复：(1) `RemoteMarketplaceClient` 新增 `max_manifest_bytes`
    (默认 1 MiB) + `max_artifact_bytes` (默认 256 MiB) 配置，
    每个 fetch 都校验 Content-Length 与实际 streamed bytes；
    新增 `fetch_artifact_bytes_with_etag` 接收 `If-None-Match`，
    server 返回 `304 Not Modified` 时 `ArtifactFetchOutcome::not_modified = true`，
    bytes 为空。(2) `loader::resolve_knowledge_path` 拒绝任何含 `..`
    component 的 pattern，并对 glob 展开结果做 canonical
    `starts_with(skill_root)` 校验——位于 skill_dir 之外的路径被静默 drop。
  - 测试：(a) `knowledge_path_with_parent_dir_traversal_is_rejected`
    断言 `../victim.md` 解析为空；(b)
    `knowledge_absolute_path_outside_skill_dir_is_rejected` 断言
    指向另一 TempDir 的绝对路径被 drop。Q1.10.1 Ed25519 签名仍待
    publisher key 策略决定，独立 commit 处理。

---

## Q2 — Wave 2: Correctness / Data Integrity

**Rationale**: 不直接是安全 bug，但会造成数据丢失、不可复现、生产环境随机崩溃。

### Q2.1 — agentflow-memory: SQLite 生产硬化（CRITICAL × 2）

- DONE Q2.1.1 4 个 SQLite 后端缺 WAL / busy_timeout / foreign_keys
  - 审计来源：`docs/audit/agentflow-memory.md` C1
  - 修复：新增 `agentflow-memory::sqlite_pool` 模块——`build_pool` /
    `build_in_memory_pool` 在每个 backend 之间共享 PRAGMA：`journal_mode=WAL`,
    `busy_timeout=5s`, `foreign_keys=ON`, `synchronous=NORMAL`。`SqliteMemory`,
    `SqliteEntityFactStore`, `SqlitePreferenceStore`, `SemanticMemory` 都
    切到这里。
  - 测试：`pool_applies_wal_busy_timeout_and_fk_pragmas` 直接 probe
    PRAGMA 值；`pool_handles_concurrent_writers_without_busy_errors` 跑
    5 个并发 task × 20 行 insert 验证不 SQLITE_BUSY。
- DONE Q2.1.2 `format!("sqlite://{}", path)` 构造非法 URL
  - 审计来源：`docs/audit/agentflow-memory.md` C2
  - 修复：所有 backend 改为 `SqliteConnectOptions::new().filename(path)`，
    raw `&Path` 直接给 sqlx，跳过 URI parser。含 `?`/`#`/空格/反斜杠
    的路径不再触发"URI parse 失败 → fallback 到相对路径"那条静默路径。
  - 测试：`pool_handles_path_with_special_characters` 用
    `db with spaces? and # signs.sqlite` 命名 TempDir 子文件，确认
    pool 构造 + 简单 SELECT 都跑得通。

### Q2.2 — agentflow-tracing: Drain task 生还 + W3C 合规（CRITICAL × 1, MAJOR × 2）

- DONE Q2.2.1 Drain task 在 `StorageErrorPolicy::FailWorkflow` 下 panic
  - 审计来源：`docs/audit/agentflow-tracing.md` C1（`collector.rs:419-431, 481`）
  - 修复：drain task 每次事件处理都用 `AssertUnwindSafe(...).catch_unwind()`
    包住；catch 到 panic 时 `tracing::error!` 输出 panic message，连续
    16 次失败后置 `drain_poisoned: AtomicBool`，`on_event` 检测 poison
    flag 后短路返回，杜绝 unbounded channel 无人 drain 仍被写入。新增
    `is_drain_poisoned()` 访问器方便测试观察。
  - 测试：`drain_task_survives_exporter_panic` 注入一个会 `panic!` 的
    exporter；先发一个会触发它的 workflow，紧跟一个 survivor workflow，
    断言后者依然能流到 storage + exporter。
- DONE Q2.2.2 OTel `trace_id` / `span_id` 用 FNV 哈希违反 W3C
  - 审计来源：`docs/audit/agentflow-tracing.md` M4
  - 修复：`trace_id` / `span_id` 函数体改为 `random_hex_id::<N>()`，用
    `rand::rngs::OsRng.fill_bytes` 生成密码学随机字节，循环跳过全零结果
    以满足 W3C "MUST NOT be all zeros"。callers 签名不变（trace_id 内仍
    保留 `_workflow_id`，仅作 doc，已注释解释为什么忽略）。span 间的
    parent/child 关系改由 explicit `parent_span_id` 维系。
  - 测试：4 个新单元测试 —— 长度/小写 hex/非全零、唯一性 (`same workflow_id` 两次
    调用必须产出不同 id) 各 1 个，覆盖 trace_id + span_id 两侧。
- DONE Q2.2.3 入站 traceparent 从未被 collector 消费
  - 审计来源：`docs/audit/agentflow-tracing.md` M5
  - 修复：`TraceMetadata` 新增 `external_trace_id` + `external_parent_span_id`
    两个 Option<String>。`on_event` 在 producer 同步上下文里调
    `current_traceparent()` 把 W3C header 提前捕获，连同 event 一起进
    channel（channel 类型改为 `(Option<String>, WorkflowEvent)`）；drain
    task 在执行 `process_event` 时用 `crate::context::scope(...)` 重新装上
    traceparent，`WorkflowStarted` 分支用新增的 `parse_traceparent()`
    解析 `00-<32>-<16>-<flags>` 并写入 metadata。OTel exporter
    (`trace_to_spans`) 现在优先使用 `external_trace_id` 作 root trace_id，
    并把 `external_parent_span_id` 作为 root span 的 `parent_span_id`。
  - 测试：`parse_traceparent_*` 4 个单元测试覆盖 canonical / 未知 version /
    全零 ID / 长度&非 hex 拒绝；`workflow_started_inherits_inbound_traceparent`
    集成测试在 `context::scope(...)` 内触发 WorkflowStarted+WorkflowCompleted，
    断言 stored trace 的 metadata + `trace_to_spans` 输出的 root span 都带
    上游 trace_id / parent_span_id。

### Q2.3 — agentflow-tracing: 其余 MAJOR

- DONE Q2.3.1 Event channel unbounded + 无 drop 计数
  - 审计来源：`docs/audit/agentflow-tracing.md` M1
  - 修复：`TraceConfig` 新增 `event_channel_capacity`（默认 8192）。drain
    channel 从 `unbounded_channel` 改为 `channel(cap)`；`on_event` 用
    `try_send`，full 时 drop event 并 `events_dropped: AtomicU64` 自增，
    每 power-of-two boundary 输出一次 `tracing::warn!`。新增
    `events_dropped()` 访问器；regression test `on_event_drops_when_channel_full`
    用 1-slot channel + 慢 exporter 模拟 backpressure。
- DONE Q2.3.2 Blocking exporter 调用与 drain task 共享 task
  - 审计来源：`docs/audit/agentflow-tracing.md` M2
  - 修复：`TraceConfig` 新增 `exporter_timeout`（默认 10s）；
    `export_trace_to_sinks` 每个 exporter call 用 `tokio::time::timeout`
    包住，timeout 走 `StorageErrorPolicy`。regression test
    `exporter_timeout_isolates_drain_task` 注入一个 sleep 5s 的 exporter
    + 100ms 超时，断言第二个 workflow 仍在 500ms 内 land 到 storage。
- DONE Q2.3.3 OTLP exporter 无 transport / TLS / auth（advertised 未实现）
  - 审计来源：`docs/audit/agentflow-tracing.md` M3
  - 修复：**deferred path**。`CLAUDE.md` 两处 advertised claim 改写为
    "OTel span model + `OtelSpanSink` trait；OTLP HTTP/gRPC transport
    deferred per Q2.3.3"，明确说明 operator 自带 `opentelemetry-otlp`
    实现。第一方 OTLP exporter 留给后续单独 RFC。
- DONE Q2.3.4 `FileTraceStorage` 无 fsync / 无 cleanup wiring / default umask
  - 审计来源：`docs/audit/agentflow-tracing.md` M6
  - 修复 (a) `FileTraceStorage::save_trace` 改为 write-temp + `sync_data` +
    rename 原子写入，unix 下 `OpenOptions::mode(0o600)` 让文件 owner-only。
    Tests `save_trace_uses_owner_only_permissions` +
    `save_trace_is_atomic_and_leaves_no_tmp_file`。
  - 修复 (b) `agentflow-server::cleanup::cleanup_expired` 新增
    `trace_dir_root: Option<&Path>` 参数；新增 `sweep_trace_dir` 函数走 root
    一层删除 `<workflow_id>.json` 中 mtime 超过 `runs_retention_days` 的文件；
    `CleanupReport.trace_files_deleted` 新字段。`spawn_cleanup_loop` 接入
    `config.trace_dir`，CLI 单次 `cleanup_expired` call 也带上 `trace_root`。
    Tests `sweep_trace_dir_deletes_old_json_only` +
    `sweep_trace_dir_returns_zero_when_root_missing`。
- DONE Q2.3.5 Redaction key 匹配为子串匹配，JWT / Cookie / AWS key 漏过
  - 审计来源：`docs/audit/agentflow-tracing.md` M8
  - 修复：`default_sensitive_key_patterns` 扩展为 25 项，新增 `jwt` /
    `cookie` / `set_cookie` / `refresh_token` / `client_secret` /
    `signature` / `webhook` / `ssh_key` / `pgp_key` /
    `aws_access_key_id` / `aws_secret_access_key` / `aws_session_token`。
    Test `default_patterns_cover_jwt_cookie_aws_refresh_webhook` round-trip
    覆盖所有新键。
- DONE Q2.3.6 内联文本 redaction 在 URL query string / CRLF header 上失败
  - 审计来源：`docs/audit/agentflow-tracing.md` M9
  - 修复：`redact_assignment_token` 改写为按 pair-boundary (`&` / `;` / `,`)
    切分 segment，对每个 segment 单独跑 `redact_single_pair`。这样
    `?api_key=secret&q=test` 只 redact `secret`，`q=test` 保留；
    `{"api_key":"secret","model":"gpt"}` 只 redact value 不吃掉闭括号；
    `session_token=opaque;path=/;httponly` 三个 pair 各自独立判定。三个
    regression tests 覆盖 URL / JSON / cookie。
- DONE Q2.3.7 retry/loop 节点 lookup 泄漏 phantom "running" 行
  - 审计来源：`docs/audit/agentflow-tracing.md` M11
  - 修复：所有 `iter_mut().rev().find(|n| n.node_id == node_id)` 加上
    `&& n.status == NodeStatus::Running` 过滤，确保只命中开放中的 attempt。
    `NodeStarted` 在 push 新行前主动把同 id 仍 Running 的旧行标记为
    `Failed("superseded by new attempt")`。Test
    `node_started_supersedes_stale_running_row` 模拟"第一次 attempt 终止
    事件丢失 → 第二次 attempt 正常完成"路径，断言两行都不在 Running 状态。

### Q2.4 — agentflow-core: 确定性 + 边角 panic

- DONE Q2.4.1 `topological_sort` 用 HashMap 迭代导致执行顺序非确定
  - 审计来源：`docs/audit/agentflow-core.md` M2
  - 修复：`in_degree` / `adj` 改为 `BTreeMap`；外层 node iteration 显式
    `sort()`；邻居入队前 sort。trace replay 跨 run 现在可复现。
  - 测试：`topological_sort_is_deterministic_across_runs` 跑 50 次相同 graph
    断言所有 order 一致。
- DONE Q2.4.2 `ExponentialBackoff` 在 `delay < 4ms` + `jitter = true` 下 panic
  - 审计来源：`docs/audit/agentflow-core.md` M4
  - 修复：`jitter_range = (delay / 4).max(1)`；`rand::random % 0` 不再发生。
  - 测试：`exponential_backoff_with_jitter_does_not_panic_on_tiny_delays`
    从 attempt=0 跑到 31 包含 0ms~1ms~2ms~3ms 边界。
- DONE Q2.4.3 `execute_with_retry` 抹除原始错误根因
  - 审计来源：`docs/audit/agentflow-core.md` M3
  - 修复：`AgentFlowError::RetryExhausted` 加 `last_error: Box<AgentFlowError>`
    字段；`thiserror` 错误信息现在 surface 根因；callers (`retry_executor` 两处)
    包装最后的 error。所有 tests/examples 的 pattern 改为
    `RetryExhausted { attempts, .. }` 或显式解构 `last_error`。
  - 测试：`test_retry_exhausted_error` 重写为验证 `last_error` 内容；
    `retry_policy_returns_error_after_max_attempts` 断言 `NodeExecutionFailed`
    "Always fails" 透传。
- DONE Q2.4.4 删除 orphan `robustness.rs`（1175 行死代码）
  - 审计来源：`docs/audit/agentflow-core.md` M1
  - 修复：`rm agentflow-core/src/robustness.rs`。文件根本不在任何 `mod` 中。
- DONE Q2.4.5 `with_timeout_context` 的 operation/node_id/workflow_id 被 `_` 吃掉
  - 审计来源：`docs/audit/agentflow-core.md` M5
  - 修复：参数去掉 `_` 前缀，timeout 路径下 `tracing::warn!` 把
    operation / node_id / workflow_id / duration_ms 一并打出。
    `#[cfg(not(feature = "observability"))]` 下保留 `let _ = (...)` 防 warn。
- DONE Q2.4.6 `ScopedPermit::Drop` 调 `tokio::spawn` 在非 runtime 上下文 panic
  - 审计来源：`docs/audit/agentflow-core.md` M6
  - 修复：`Drop` 内 `if tokio::runtime::Handle::try_current().is_ok()` 才 spawn；
    无 runtime 上下文（同步 Drop / runtime shutdown 后）跳过 stats 更新。
- DONE Q2.4.7 两个同名 `pub struct ErrorContext` 共存
  - 审计来源：`docs/audit/agentflow-core.md` M7
  - 修复：`error.rs::ErrorContext` → `InlineErrorContext`（保留），
    `error_context::ErrorContext`（被 lib re-export）继续叫 `ErrorContext`。
    `error.rs::ContextualError`、`with_context` 同步更新；
    `phase1_integration_tests.rs` 的 import 改名。

### Q2.5 — agentflow-llm: 流式与 panic site（MAJOR × 4）

- DONE Q2.5.1 Anthropic 流式在 `content_block_stop` 终止（应只在 `message_stop` 终止）
  - 审计来源：`docs/audit/agentflow-llm.md` M2
  - 修复：`parse_sse_event` 中删除 `content_block_stop` 终止分支；只在
    `message_stop` 标记 `is_final=true`。新增 3 个单元测试：
    `streaming_content_block_stop_does_not_finalize`、
    `streaming_message_stop_does_finalize`、`streaming_text_delta_remains_unaffected`。
- DONE Q2.5.2 流式 tool_call delta 被丢弃
  - 审计来源：`docs/audit/agentflow-llm.md` M1
  - 修复：`StreamChunk` 新增 `tool_call_deltas: Vec<ToolCallDelta>` 字段
    （`{ index, id?, name?, arguments_delta? }`）。OpenAI 解析
    `choices[].delta.tool_calls`，Anthropic 解析 `content_block_start`（捕获
    id/name）+ `input_json_delta`（累积 partial_json）。Consumers 按 index 合并
    arguments_delta 即可还原完整 JSON。新增 5 个单元测试覆盖两个 provider 的
    起始/续传/text 路径。
- DONE Q2.5.3 6 处 `HeaderValue::from_str(api_key).expect(...)` panic
  - 审计来源：`docs/audit/agentflow-llm.md` M3
  - 修复：Anthropic / OpenAI / Moonshot / StepFun 的 `build_headers` 改为
    返回 `Result<HeaderMap>`，对非法字符返回 `LLMError::ConfigurationError`；
    所有调用点改为 `?` 传递。新增 4 个回归测试（每个 provider 一个含 `\n`
    的非法 key 路径）。
- DONE Q2.5.4 `unsafe impl Sync` 在 streaming response 上无理由
  - 审计来源：`docs/audit/agentflow-llm.md` M5
  - 修复：`StreamingResponse` trait 的 bound 从 `Send + Sync` 改为 `Send`
    （streams 通过 `&mut self` 顺序消费，Sync 没有意义）。删除 5 个 provider
    上的 `unsafe impl Send/Sync`（OpenAI / Anthropic / Moonshot / StepFun /
    Google），换成解释性注释。

### Q2.6 — agentflow-mcp: 协议正确性（CRITICAL × 2）

- DONE Q2.6.1 JSON-RPC response 无 `id` 关联
  - 审计来源：`docs/audit/agentflow-mcp.md` C1（`transport/stdio.rs:280-290`）
  - 修复：`StdioTransport::send_message` 现在从 request 提取 `id`，循环读行
    直到拿到 `response.id == expected_id`；out-of-band 消息（典型的
    notification）暂存到新的 `pending_inbox: VecDeque<Value>`，下次
    `receive_message` 优先 drain。session 不再因 notification 永久 off-by-one。
  - 测试：`send_message_skips_notifications_until_matching_id` 用 shell
    fixture 在每行 echo 之前先吐一个 notification，断言 (1) response
    携带预期 id、(2) buffered notification 通过 `receive_message`
    可读出。
- DONE Q2.6.2 stderr 被 pipe 但从未排空
  - 审计来源：`docs/audit/agentflow-mcp.md` C2（`transport/stdio.rs:235`）
  - 修复：`connect` 现在抢走 `child.stderr` 并 spawn 后台 drain task
    （`spawn_stderr_drain`），每行 forward 到
    `tracing::warn!(target="agentflow_mcp::stdio::stderr")`。`disconnect`
    + `Drop` 都 abort 该 handle。Linux 64KB pipe 满后挂死的死锁场景消除。
  - 测试：`stderr_does_not_deadlock_when_server_floods_it` shell fixture
    在 echo 之前先往 stderr 写 128 KiB（128×1024 字节），仍然能 round-trip
    request/response。

### Q2.7 — agentflow-server / agentflow-cli: 关键正确性（MAJOR × 1 + 子项）

- DONE Q2.7.1 `audio asr --prompt VALUE` 静默把转录写到路径 VALUE
  - 审计来源：`docs/audit/agentflow-cli.md` M1
    （`main.rs:1651` vs `commands/audio/asr.rs:5-11`）
  - 修复：clap 中 `--output / -o` 现在是独立 named flag（之前根本不存在），
    `--prompt` 保留；call site 显式按 name 解构 + 传递。`asr::execute` 签名
    加 `prompt` 参数并塞进 `AsrRequest.prompt`（之前总是 `None`，操作员的
    hint 一直被吞）。两个槽位完全 disjoint。
  - 测试：`audio_asr_prompt_tests.rs`：
    (1) `asr_prompt_flag_does_not_write_transcript_to_prompt_value` 直接
        断言传入 `--prompt /tmp/x.txt` 后 `/tmp/x.txt` **不存在**；
    (2) `asr_output_flag_is_separate_from_prompt` 检查 `--help` 同时列
        `--prompt` 和 `--output`；
    (3) `asr_output_flag_is_not_pre_created_on_failure` 确认失败路径下
        `--output` 文件不会被预创建。

### Q2.8 — agentflow-rag: 批处理性能（CRITICAL × 1）

- DONE Q2.8.1 OpenAI embedding 批 size 150× 欠批
  - 审计来源：`docs/audit/agentflow-rag.md` C1（`embeddings/openai.rs:235`）
  - 修复：拆分常量 `MAX_INPUTS_PER_BATCH = 2048`（OpenAI input[] 长度）
    + `MAX_TOKENS_PER_BATCH = 300_000`（OpenAI 单 request token 上限）；
    flush 条件改为 OR 两个独立维度。pre-fix 13 条短文本一批的瓶颈解除，
    短文本现在可以一次塞满 2048 条直到长度上限触发。
  - 测试：(1) `batch_size_constants_are_disjoint` 检查两个常量
    disjoint 且 token > input；(2)
    `batch_flush_condition_uses_both_limits_independently` 模拟 short
    texts 的 packing 数学，断言 token budget 不再主导。

### Q2.9 — agentflow-agents: 契约违反（MAJOR × 3）

- DONE Q2.9.1 `react/agent.rs:2073` `expect("every prepared call must have an output...")`
  - 审计来源：`docs/audit/agentflow-agents.md` M1
  - 修复：将 `expect` 改为显式 `match`：缺失 output 时 emit `warn!` 并
    插入 synthetic `ToolOutput::error("internal invariant violation: ...")`。
    batch 的其余 tool calls 继续完成，operator 在 trace 里看到不一致。
- DONE Q2.9.2 `PlanExecuteAgent` 静默忽略 `RuntimeLimits.token_budget`
  - 审计来源：`docs/audit/agentflow-agents.md` M3
  - 修复：`PlanExecuteAgent::run` 读 `context.limits.token_budget`；planner
    回复持久化到 memory 后立即调 `self.memory.session_token_count`，超
    budget 时 `AgentStopReason::TokenBudgetExceeded { used, budget }`
    停掉。与 ReActAgent 行为对齐。
- DONE Q2.9.3 LLM 返回的 tool params 在 dispatch 前未做 JSON Schema 校验
  - 审计来源：`docs/audit/agentflow-agents.md` M5
  - 修复：`ToolError` 新增 `SchemaViolation { tool, message }` variant；
    `ToolRegistry::validate_params(name, &params)` 公开方法用 jsonschema
    compile + validate；`ToolRegistry::execute` 在 capability 检查之后、
    `tool.execute` 之前自动调一次 `validate_params`。任何 caller（agent
    / CLI / workflow node）派发畸形参数都会得到 SchemaViolation，
    LLM 端可以 self-correct。
  - 测试：`validate_params_rejects_schema_violations` —— missing required /
    wrong type / valid pass / unknown tool 四个分支全覆盖。

### Q2.10 — agentflow-memory: 数据完整性（MAJOR × 2）

- DONE Q2.10.1 `row_to_message` 在解析失败时静默伪造新 UUID + 时间戳
  - 审计来源：`docs/audit/agentflow-memory.md` M1
  - 修复：`SqliteMemory::row_to_message` 删除两个 `unwrap_or_else` fallback
    （`Uuid::parse_str(...).unwrap_or_else(|_| Uuid::new_v4())` 和
    `parse_from_rfc3339(...).unwrap_or_else(|_| Utc::now())`），改为
    `?` 上抛 `StorageError`。同一行多次读现在返回相同 id/timestamp，
    `AgentNodeResumeContract` 不再被破坏。
  - 测试：`row_to_message_returns_err_on_corrupt_uuid` 直接 INSERT 一个
    `id = "not-a-uuid"` 的行，断言 `get_all` 返回错误而不是静默成功。
- DONE Q2.10.2 `add_message` 接收 `&mut self` 阻塞 H3 并发写
  - 审计来源：`docs/audit/agentflow-memory.md` M6
  - 修复：`MemoryStore` trait 中 `add_message` 与 `clear_session` 改为
    `&self`。`SqliteMemory` / `SemanticMemory` 直接 drop `mut`（pool 已
    Send+Sync）；`SessionMemory` 把内部 `HashMap` 包进
    `tokio::sync::Mutex` 提供 interior mutability。callers 不再被借用
    检查卡住，可以并发 `add_message`。
  - 测试：`add_message_supports_concurrent_writers` spawn 32 个并发 task
    各跑 `add_message`，断言全部 32 条 message 落地。

---

## Q3 — Wave 3: Productization Hygiene

### Q3.1 — Workspace: graceful shutdown / 信号处理

- DONE Q3.1.1 `agentflow-server`：`axum::serve` 无 SIGTERM handler，spawn 的
  run/session task 全部被丢弃。
  - 审计来源：`docs/audit/agentflow-server.md` M2
  - 修复：`axum::serve(...).with_graceful_shutdown(shutdown_signal())`。
    `shutdown_signal()` 用 `tokio::select!` 同时监听 `tokio::signal::ctrl_c()`
    和 unix-only `SignalKind::terminate()`，触发后 axum 停止接受新连接但等
    in-flight 请求 drain。k8s rolling deploy 现在能在
    `terminationGracePeriodSeconds` 窗口内干净退出。
- DONE Q3.1.2 `agentflow-cli`：`workflow run` / `harness run` / `skill chat`
  无 Ctrl-C handler，退出无 trace flush。
  - 审计来源：`docs/audit/agentflow-cli.md` M2, m11
  - 修复：
    1. 新增 `agentflow-cli/src/shutdown.rs` shared helper：
       `shutdown_signal()` 同时监听 `tokio::signal::ctrl_c()`（全平台）
       和 SIGTERM（unix），匹配 server 的 Q3.1.1 模式；常量
       `SIGINT_EXIT_CODE = 130` (POSIX `128 + SIGINT`)。
    2. `workflow run`：注入 `FlowCancellationToken` 到
       `FlowExecutionConfig`，`tokio::pin!` + `tokio::select!`
       race against `shutdown_signal()`；信号触发 → `cancel()` +
       10s 等 run_future 收尾 + `TraceCollector::flush(5s)` 确保
       drain 落盘 + `process::exit(130)`。
    3. `harness run`：`HarnessRunOptions` 新增
       `with_cancellation_token(AgentCancellationToken)`；
       `HarnessRuntime::run` 把它 thread 到 `AgentContext`，
       inner ReAct/PlanExecute loop 收到 cancel 后 stop。CLI
       端同样 select! + 10s drain + exit(130)。
    4. `skill chat`：`agent.run()` 用 `tokio::select!` race
       against signal；信号触发 → exit(130)（REPL 不返回，因为
       in-flight tool call 没有安全的 unwind 路径）。
    5. `agentflow-tracing`：新增 `TraceCollector::flush(timeout)`，
       内部用 `submitted_count` / `processed_count` 原子计数器
       等 drain task 处理完所有已入队事件；drain 被
       poison 时立即返回 false。`process_event` 新增
       `WorkflowEvent::WorkflowCancelled` 处理（之前完全被
       忽略，导致 cancel 的 workflow 在 `trace tui` 里永远显示
       "Running"），落库为新的 `TraceStatus::Cancelled { reason }`。
       `format.rs` / `otel.rs` / `replay.rs` / `tui.rs` 的
       match 同步更新。
  - 测试：
    - 单元：`flush_waits_until_drain_catches_up` /
      `flush_returns_false_when_drain_is_poisoned` in
      `agentflow-tracing/src/collector.rs`。
    - 集成：`agentflow-cli/tests/workflow_ctrl_c_tests.rs` 两个
      用例验证 cancel + flush 后 `WorkflowCancelled` 真的落进
      `FileTraceStorage`（而不是停在 in-memory current_traces）+
      `SIGINT_EXIT_CODE` 常量契约。
    - Workspace `cargo build` + `agentflow-tracing` / `agentflow-cli` /
      `agentflow-harness` lib tests 全绿。
- DONE Q3.1.3 `agentflow-worker`：`run_forever` 在首次 transport 错误上 abort；
  `WorkerCancellationToken` 已存在但 `main.rs` 从未注册信号 hook。
  - 审计来源：`docs/audit/agentflow-worker.md` M1, M2
  - 修复：
    1. 新增 `ReconnectBackoff` 配置：`initial / max / multiplier_percent /
       max_attempts / jitter`；`Default` 是 100ms → 30s × 2 + ±25% jitter，
       `max_attempts = None` 即"重试到死"。`WorkerConfig` 加
       `reconnect_backoff: ReconnectBackoff` 字段 + `with_reconnect_backoff`
       builder。
    2. `WorkerRuntime::run_forever` 把 `SchedulerError::Transport` 重新
       归类为 recoverable：
       - 成功 → reset backoff，按 `poll_interval` 节奏继续。
       - Transport 错误 → log + jittered backoff sleep（用 `tokio::select!`
         race against cancellation，所以 SIGTERM 不必等满 30s 才能退出）。
       - 其他 `WorkerError`（`InvalidWorkerId` 等）→ 仍然 fatal。
       - `max_attempts` 触顶 → 把最后一次 `Transport` 错误返还给调用方。
    3. `agentflow-worker/src/main.rs` 新增 `shutdown_signal()`（match
       agentflow-cli/server 的 Q3.1.1 模式：`ctrl_c` + 可选 SIGTERM unix
       handler），`run_runtime` 用 `tokio::select!` 把 `run_forever` 跟
       signal future race；信号触发 → `cancellation_token.cancel()` +
       30s drain timeout + 干净退出。k8s rolling deploy 现在能在
       `terminationGracePeriodSeconds` 窗口内 drain。
  - 测试：`agentflow-worker/src/lib.rs` 新增 5 个用例：
    - `reconnect_backoff_doubles_until_cap` — 纯函数 backoff 曲线
      (100 → 200 → 400 → 800 → 1000 → 1000，deterministic 模式)。
    - `reconnect_backoff_jitter_stays_within_window` — jitter 50 次都
      落在 ±25% 范围内。
    - `run_forever_recovers_from_transport_blip` — `FlakyProtocol` 先
      抛 3 次 Transport 错误，runtime 必须 keep going 而不是首次 bail。
    - `run_forever_gives_up_after_max_attempts` — `max_attempts=3`
      时 runtime 必须 surface `Transport` 错误。
    - `run_forever_cancellation_unblocks_backoff_sleep` — 60s backoff
      window 期间触发 cancel，runtime 必须在 2s 内退出（验证
      backoff sleep 用了 `tokio::select!` 而不是 plain `sleep`）。
  - `cargo test -p agentflow-worker --lib` 13 / 13 通过。

### Q3.2 — agentflow-mcp: 健壮性（MAJOR × 2）

- DONE Q3.2.1 stdio 子进程继承父进程完整环境（含 secret）
  - 审计来源：`docs/audit/agentflow-mcp.md`
  - 修复：`StdioTransport` 默认 `env_clear()` + 显式 forward 一个
    `SAFE_INHERITED_ENV_VARS` 白名单（PATH/HOME/USER/LOGNAME/SHELL/LANG/
    TZ/TERM/TMPDIR/PWD + Windows 等价 + 所有 `LC_*`）；再叠加 `self.env`。
    OPENAI/ANTHROPIC/AWS/SSH 等敏感凭证不再泄漏给第三方 MCP server。
    新增 `with_inherit_parent_env(true)` 显式 opt-back-in。两个集成测试
    （unix-only）用 `/usr/bin/env` 作为 MCP server，验证默认 sandbox
    + 显式 opt-in 两条路径都 round-trip 正确。
  - 审计来源：`docs/audit/agentflow-mcp.md` M1
  - 现状：无 `env_clear` / 无 `current_dir` / 无 fd 收缩，第三方 stdio MCP server
    可读取所有 env 变量。
  - 验收：spawn 前 `env_clear`；显式 allowlist 注入；可选 `current_dir`
    pin 到 skill 工作目录；测试覆盖 env 不可见。
- DONE Q3.2.2 transport `Arc<Mutex<Box<dyn Transport>>>` 串行化所有 call
  - 审计来源：`docs/audit/agentflow-mcp.md` M6
  - 修复（大改）：
    1. `Transport` trait：`send_message` / `send_notification` /
       `receive_message` 改成 `&self`（interior mutability）。
       `connect` / `disconnect` 保留 `&mut self`（lifecycle 拥有
       I/O 资源）。
    2. `StdioTransport` 重写：
       - 字段拆成 `writer: Arc<AsyncMutex<Option<BufWriter<ChildStdin>>>>`
         （写 barrier 串行化保 JSON-RPC 行顺序，但 lookup +
         register 在 lock 外）。
       - `inflight: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>`
         按 JSON-RPC `id` key 注册每请求 oneshot。
       - `notifications_rx: Arc<AsyncMutex<mpsc::UnboundedReceiver<Value>>>`
         + `notifications_tx` 给 `receive_message` 排队
         server-initiated 消息。
       - `connect()` spawn 一个 `run_reader_task`：每行解析 →
         有 `id` 即 response 查 inflight 投递 oneshot；无 `id` 即
         notification 入 channel；malformed 即 `warn!` skip；EOF
         即 flip shared `Arc<AtomicBool> connected` + clear inflight
         （pending oneshot Sender drop → caller 收 RecvError →
         translate 成 connection error）。
       - `send_message`：register oneshot → write line → wait
         `timeout(rx)` → 干净 inflight 清理。Duplicate id surface
         error（防 silent clobber）。
       - `disconnect`：drop writer → abort reader_task → clear
         inflight → kill child。
    3. `MockTransport` 同步改 `&self`（本来就用 Arc<Mutex<>> 内部
       状态，trivial）。
    4. `MCPClient`：丢弃 `Arc<Mutex<Box<dyn Transport>>>`，改成
       `Box<dyn Transport>` 直接持有；`send_request` /
       `send_notification` 改 `&self`（其余 state 早已是
       interior mutable）。`connect` / `disconnect` 仍 `&mut self`。
       这让 `Arc<MCPClient>` 可在 agent parallel tool-call
       dispatcher 里被并发 fan-out 而不再撞 outer mutex。
    5. `MCPClient` 内部 send_request 的 retry closure 改成借用
       `&self.transport` 而不是 clone（Box 不 Clone；&self 借用
       足够，retry 循环按需 await 不跨借用边界）。
  - 测试 2 个新用例 + 重构 3 个旧的：
    - `stdio_transport_supports_concurrent_send_message`（核心
      regression）：sh echo loop 给 8 个并发 send_message 应
      id-by-id 正确路由，全部完成且无 timeout（pre-fix 必锁
      死 1 个 in-flight）。
    - `stdio_transport_rejects_duplicate_inflight_request_id`：两
      个并发同 id 第二个 surface "duplicate id" error 而不
      silent clobber。
    - 旧 `test_invalid_json_response`：行为变了 — malformed
      JSON 现在被 reader task 静默 drop（log warn），caller
      timeout（更好的失败模式，不再让坏服务连累后续 caller）；
      测试改成断言 timeout 路径。
    - 旧 `spawn_sandboxes_parent_env_*` 两个 env 隔离测试改用
      `sh -c "env > FILE"` + 读 FILE 回放（不再直戳
      `transport.stdout` 私有字段，因为 stdout 现在归 reader
      task）。
    - `test_disconnect_cleans_up_process` 改去查
      `transport.writer.lock().await.is_none()` 而不是消失的
      `stdin`/`stdout` 字段。
    - 旧 `test_check_process_health_*` 3 个删（helper 已不存在），
      换成 3 个 `is_connected()`-based 等价测试，验证 EOF 检测
      会真翻 false。
    `cargo test -p agentflow-mcp` 37 + 1 ignored，
    `cargo test -p agentflow-skills --lib mcp_tools` 5/5 通过，
    `cargo build --workspace` 整体绿。

### Q3.3 — agentflow-worker: 并发与 proto（MAJOR × 3）

- DONE Q3.3.1 `Mutex<Grpc<Channel>>` 让单 worker 进程并发硬卡 1
  - 审计来源：`docs/audit/agentflow-worker.md` M3
  - 修复：`GrpcWorkerProtocol` 字段从 `inner: Arc<Mutex<Grpc<Channel>>>`
    改成 `channel: Channel`，`unary()` 每次调用 `Grpc::new(self.channel.clone())`
    构造新 Grpc 实例（`Channel::clone` 是 `Arc::clone`，cost negligible；
    `Grpc::new` 是 thin wrapper over channel + codec）。`ready().await`
    保留 —— tonic 的 `Channel` 内部 `Buffer<...>` 服务要求 `poll_ready`
    先被观察过才能 `call`，pre-fix 的 mutex 把这个 contract 给藏起来了；
    `ready()` 本身只是 poll buffer 状态，cheap。结合 Q3.3.2 spawn-per-permit
    parallel dispatcher，一个 worker 进程现在能真正并发 heartbeat +
    claim + report 三条管路。
  - 测试：`scheduler::grpc::grpc_concurrency_tests::grpc_protocol_fires_concurrent_heartbeats`
    —— `SlowConcurrencyServer` 每个 heartbeat sleep 200ms 并 track
    in-flight high-water mark；8 个 cloned protocol 并发 fire heartbeat
    必须：(a) 全部 8 个完成；(b) peak in-flight >= 2（pre-fix
    必锁死 1）；(c) wall clock < 1.2s（serial 8 × 200ms = 1.6s 会
    fail，parallel 应该 ~200ms）。`cargo test -p agentflow-server
    --lib scheduler` 62/62 + `cargo test -p agentflow-worker
    --lib` 16/16 全部通过，无回归。
- DONE Q3.3.2 `free_slots` 广告但从未强制
  - 审计来源：`docs/audit/agentflow-worker.md` M4
  - 修复：`WorkerRuntime` 加 `dispatch_slots: Arc<tokio::sync::Semaphore>`
    initialised to `config.free_slots`（clamp >= 1 防止 0 死锁）。
    `run_forever` 重写为并行 dispatcher：
    1. 每次循环 `dispatch_slots.acquire_owned()` 拿 permit
       （race against cancellation，SIGTERM 不必等满 slot）。
    2. 持 permit 调 `dispatch_one_with_permit`：heartbeat → claim
       → `tokio::spawn` execute+report，permit 移入 spawned task。
    3. 任务结束 permit 自动 drop，slot 释放。
    4. Cancellation 路径 `acquire_many(total)` drain 所有 in-flight。
    `run_once` heartbeat 也改成报 `dispatch_slots.available_permits()`
    动态值，scheduler 看到的 free_slots 永远是实时的，不再撒谎。
    要求 `P: Clone + Send + 'static` —— InMemory / Grpc 协议都已
    Clone。`free_slots = 1` 行为完全等同 pre-Q3.3.2 串行 dispatch
    （semaphore 单 permit），向后兼容 0 风险。
  - 测试：
    - `free_slots_4_dispatches_concurrently` —— `SlowReportProtocol`
      每个 `report_result` sleep 400ms 并维护 in-flight high-water
      mark；4 任务并行必须在 < 1.2s 完成（串行需 ≥ 1.6s）+ peak
      in-flight ≥ 2。
    - `free_slots_1_keeps_serial_dispatch` —— `free_slots=1`
      保证 peak in-flight 恰好 1（serial baseline）。
    - `heartbeat_reports_dynamic_available_permits` —— 用
      `HeartbeatRecorder` 捕获每次 heartbeat 的 `free_slots`，
      assert 至少一次观察到 < `config.free_slots`（动态报告
      而非 static config），并且当所有 permit 都占用时报 0。
    `cargo test -p agentflow-worker --lib` 16/16 通过。
- DONE Q3.3.3 `worker.proto` 落后于 prost 手写结构
  - 审计来源：`docs/audit/agentflow-worker.md` M5
  - 现状：缺 `accepted_node_types` / `locality_run_id` / `node_type`，非 Rust
    语言绑定不兼容。
  - 修复（三层验收全部完成）：
    1. **`.proto` 与 prost 对齐**：`agentflow-server/proto/agentflow/scheduler/v1/worker.proto`
       补齐 3 个 P10.16.2-FU1 字段——`WorkerTask.node_type = 6`、
       `ClaimTaskRequest.{accepted_node_types = 2, locality_run_id = 3}`、
       `HeartbeatRequest.accepted_node_types = 5`，并标注新字段的
       backward-compat 语义（空值 = pre-FU1）。
    2. **`build.rs` 改为从 `.proto` 生成**：新增 `agentflow-server/build.rs`
       调用 `tonic_build::configure().build_client(false).build_server(false)
       .compile_protos(...)`，只生成 prost message 结构；手写的
       `WorkerControl` trait + Tower `Service` impl 保留（它们叠加了
       自定义 W3C traceparent scope + admission credential 提取，
       tonic 生成的 stub 没有这一层）。`src/scheduler/grpc.rs::pb`
       从手写 120 行降到 3 行的 `tonic::include_proto!`。
       `agentflow-server/Cargo.toml` 加 `tonic-build = "0.12"`
       到 `[build-dependencies]` 并显式 `build = "build.rs"`。
    3. **CI 加 `protoc` 校验**：`.github/workflows/quality.yml` 新增
       `proto-validate` job——`arduino/setup-protoc@v3` 装 protoc 27.x
       后跑 `protoc --proto_path=... --descriptor_set_out=/dev/null`
       校验 .proto 语法，并 grep `tonic::include_proto!` 防止有人
       再退回手写 pb。`proto-validate` 进 `release-gate` 的
       `needs:` 列表，与 fmt/clippy/test 一起作为发布门禁。
       clippy / doctest / examples / agentflow-cli test matrix /
       ui-e2e.yml 全部加 `Install protoc` 步骤，因为它们 transitively
       编译 agentflow-server。
  - 测试：新增 `proto_schema_drift_tests` 8 个用例覆盖：textual pin
    `worker.proto` 中的每个字段名+tag 三元组（防 .proto 被人改动
    后没人发现）+ prost encode→decode round-trip 验证 3 个新字段
    跨 wire 正确传输 + pre-FU1 空字节默认 decode。
    `cargo test -p agentflow-server --all-targets` 298 test 全绿；
    `cargo build -p agentflow-server --release` 通过；
    `cargo fmt -p agentflow-server -- --check` 在我触碰的代码上无 diff。

### Q3.4 — agentflow-server: 余下 MAJOR

- DONE Q3.4.1 `AGENTFLOW_MAX_REQUEST_BODY_BYTES` 读入但从未挂到 router
  - 审计来源：`docs/audit/agentflow-server.md` M1
  - 修复：`create_router` 顶层 stack 上加 `DefaultBodyLimit::max(global)`，
    其中 `global = state.security.request_limits.max_request_body_bytes`。
    Per-route 已有的 workflow/skill/harness limits 仍然 shadow 这个全局
    cap，但 `/v1/preferences` 等其余 POST endpoint 现在也有上限保护。
- DONE Q3.4.2 Worker PSK 比较非常量时间（`HashSet::contains`）
  - 审计来源：`docs/audit/agentflow-server.md` C3
  - 修复：PSK 比对从 `HashSet::contains` 改为 walk-all + `constant_time_eq`
    （新增 helper，xor-accumulate 字节差异，不 break-early），匹配
    `src/auth.rs::constant_time_eq` 的 bearer 路径。即使首字节就 mismatch
    也跑完所有 rotation 条目，total compute time 不再泄漏匹配位置。
- DONE Q3.4.3 `LiveHarnessExecutor` 每个并发 session 起一条 OS 线程
  - 审计来源：`docs/audit/agentflow-server.md` M6
  - 修复：临时方案——加并发上限 + 排队。`LiveHarnessExecutor` 新增
    `concurrency_limit: Arc<tokio::sync::Semaphore>`，默认 cap 32（由
    `default_max_concurrent_sessions()` 返回）；`execute()` 进入时先
    `acquire_owned().await` 拿 permit，session 结束 permit 自动释放。
    超过 cap 的 session 排队等待而不是无限制 spawn OS thread。可通过
    `with_max_concurrent_sessions(N)` builder 覆盖。根因（HarnessRuntime
    !Sync → 必须 spawn_blocking）留作后续上游修复（Q3.4.3-FU）。

### Q3.5 — agentflow-cli: 余下 MAJOR

- DONE Q3.5.1 `llm chat` 隐藏接收且丢弃 `--model/--system/--save/--load`
  - 审计来源：`docs/audit/agentflow-cli.md` M3
  - 修复：clap 层把这四个 args 完全删除——`LlmCommands::Chat {}` 改成
    unit-like variant。handler 仍返回原有的"retired，请用 skill chat"
    redirect message，但不再接受 structured 参数让用户误以为它们生效。
- DONE Q3.5.2 `plugin` / `rag` feature 不在 `default = []`，与 CLAUDE.md/RoadMap.md
  N10 "closed" 状态矛盾
  - 审计来源：`docs/audit/agentflow-cli.md` M4
  - 修复：`agentflow-cli/Cargo.toml` 改 `default = ["plugin", "rag"]`。
    新装 `agentflow` 现在 `rag --help` / `plugin --help` 直接出真子命令
    （rag search/index/collections/eval、plugin install/list/inspect 等），
    而不是 `FeatureUnavailableArgs` stub。`mcp` 保持 opt-in：`agentflow
    mcp *` 本身不需要 `agentflow-nodes/mcp`（CLI 自己已 dep
    `agentflow-mcp`），feature gate 只影响 workflow YAML 中的 `type:
    mcp` 节点。
  - 测试：`cargo build -p agentflow-cli` 默认通过；手动 smoke
    `rag --help` / `plugin --help` 真子命令出现。
- DONE Q3.5.3 `workflow logs --follow` SSE 无重连 backoff
  - 审计来源：`docs/audit/agentflow-cli.md` M5
  - 修复：在 `commands/workflow/server_ops.rs` 加
    `stream_events_with_reconnect` 包装：
    - 每次 SSE close / transport error 后 1s → 2s → 4s ... 30s
      exponential sleep（无 jitter，单 CLI 不像 worker fleet
      有 thundering herd），然后用最新 `high_water_mark` 作为
      `after_seq` 重新打开 stream。
    - 用 last-seen seq 做去重（server `?after_seq=N` 是
      strictly `seq > N`，直接 forward 已观察到的最大 seq）。
    - 新 helper `is_terminal_run_event_kind` 识别
      `run_finished` / `run_failed` / `run_cancelled` 等终态
      事件，做"clean close after terminal = exit"判定，
      否则 close 视为 mid-stream blip 触发重连。
    - 每次重连前在 stderr 打印 `⚠ workflow logs stream ...
      reconnecting in <duration>`，避免污染 stdout JSONL pipeline。
  - 测试：`cli_workflow_logs_follow_reconnects_after_mid_stream_drop`
    用 `Arc<AtomicU32>` 追踪 attempt 数，mock SSE handler 第一次
    返 seq 0+1 then close（模拟 blip），第二次根据 after_seq
    返 seq 2+3 then close；CLI 必须 surface 4 个事件无重复、
    stderr 有 reconnect 提示、server 恰好收到 2 次 attempt。
    `cargo test -p agentflow-cli --test workflow_logs_tests` 6/6
    通过。

### Q3.6 — agentflow-llm: 余下 MAJOR

- DONE Q3.6.1 `agentflow-core` 在 Cargo.toml 声明但从未 import（死依赖）
  - 审计来源：`docs/audit/agentflow-llm.md` M4
  - 修复：`grep -r agentflow_core agentflow-llm/src/` 0 命中，确认死依赖；
    `agentflow-llm/Cargo.toml` 删除该行，留注释解释为什么之前在那里。
    cargo build 通过，无 import 需要清理。
- DONE Q3.6.2 完整 prompt/response 在 DEBUG 级日志（PII 风险）
  - 审计来源：`docs/audit/agentflow-llm.md` M6
  - 修复：`LLMClient::log_request_start` / `log_request_complete` 在 DEBUG
    级只打 `len=N sha_prefix=...`（FNV-1a 64-bit 指纹，stable cross-run），
    全文 prompt/response 降级到 TRACE 级。新增 `prompt_fingerprint(text)`
    helper，专门 deduplicate 关联追踪而无需写出 PII。FNV 而非 sha256 是
    deliberate—不做认证，避免引入新 crypto dep 给日志用。

### Q3.7 — agentflow-ui: 结构性 MAJOR

- DONE Q3.7.1 `main.tsx` 2624 行单体 + 无 ErrorBoundary (commits 9ba4f00 + 24537ce)
  - 审计来源：`docs/audit/agentflow-ui.md` M1, M2
  - 验收：按 page / hook / component 拆分；顶层加 ErrorBoundary 防白屏。
  - **Stage A (9ba4f00)**：新增 `src/components/ErrorBoundary.tsx` —
    class-based React ErrorBoundary 包住 `<App />`。捕获未处理 throw
    时回退到诊断面板：error.message + componentDidCatch 的 component
    stack（默认展开）+ Reset / Reload page 按钮 + 指引读 console。
    rose-accent 样式与 UI 调色板一致。5 个单测固定契约
    (`getDerivedStateFromError` / 默认 children passthrough / fallback
    DOM role+message+stack / reset 清错 / label prop)。Bundle +1.7 KiB
    gzip。
  - **Stage B (24537ce)**：main.tsx 从 2705 行降到 1052 行（-61%）：
    - `src/lib/api.ts` — apiFetch + parseSseChunk (泛型, workflow/harness
      共用)
    - `src/lib/storage.ts` — localStorage / sessionStorage helpers +
      tokenKey / workflowKey / tenantKey / newForm* / eventFilterKeyPrefix
    - `src/lib/helpers.ts` — formatTime / prettyJson / runFromEnvelope /
      eventTone / isTerminalRun / findLatest / eventNodeId
    - `src/lib/harness.ts` — harness-specific keys + status helpers +
      ApprovalOutcome / ApprovalScope + HARNESS_PROFILES /
      HARNESS_RUNTIMES + harnessSessionIdFromPath
    - `src/pages/RunCreateForm.tsx` (/ui/runs/new)
    - `src/pages/DiagnosticsPanel.tsx` (/ui/diagnostics)
    - `src/pages/HarnessSessionList.tsx` (/ui/harness/sessions)
    - `src/pages/HarnessSubmitForm.tsx` (/ui/harness/sessions/new)
    - `src/pages/HarnessSessionDetail.tsx` (/ui/harness/sessions/:id，
       含 ApprovalCard 内联)
    - main.tsx 留下 App router + RunConsole + RunCompare（这两个互相
      引用 state/effects 多，拆分价值低；以后文件再涨再拆）
  - 测试：`tsc --noEmit` exit 0；`npm run build` (vite) 干净 (app.js
    312 KiB)；`schemas.test.ts` 16/16；`ErrorBoundary.test.tsx` 5/5；
    `cargo test -p agentflow-server --lib ui::` 4/4；
    Playwright MCP 各 route (`/ui` / `/ui/runs/new` / `/ui/diagnostics` /
    `/ui/harness/sessions` / `/ui/harness/sessions/new`) DOM 结构
    与重构前 baseline 完全一致。
- DONE Q3.7.2 23 处未检查的 `as Type` JSON 响应断言 (commit 4e7423d)
  - 审计来源：`docs/audit/agentflow-ui.md` M3
  - 验收：引入 zod / valibot / typia 之一做 runtime 校验；最少先包裹 SSE 与
    重要 GET。
  - 修复：选 zod 4 (~10 KiB gzip 增量)。新建 `agentflow-ui/src/schemas.ts`
    (235 行) 给 11 种 JSON 响应形状各定 schema：RunRecord / RunEnvelope /
    ListRunsEnvelope / CreateRunEnvelope / CancelRunEnvelope /
    StreamedEvent / HarnessSession / HarnessEvent / PendingApproval /
    DiagnosticsReport（含 DirCheck / Status）+ 它们的 `*ArraySchema`
    伴生。每个 object schema 用 `.loose()` 让 server 加新字段不破 UI，
    只拒绝缺 required 字段或类型错。helper：`parseJsonResponse(schema,
    response, label)` / `parseJson(schema, raw, label)` /
    `SchemaValidationError`（issue 摘要截断到前 3 个）。
  - main.tsx 11 个 cast 点全部接入 schema：POST /v1/runs（new + resubmit）/
    GET /v1/runs（list + by id + events/history）/ POST :cancel / SSE
    chunk（重写 `parseSseChunk` 为泛型，workflow 走 StreamedEventSchema、
    harness 走 HarnessEventSchema 共用一份 parser 不再 unsafe cast）/
    POST + GET /v1/harness/sessions / by id / events/history /
    approvals / GET /v1/doctor。
  - 删除 main.tsx 里 6 个重复 local type 定义（HarnessSession /
    HarnessEvent / PendingApproval / DiagnosticsReport / DiagnosticsDirCheck
    / DiagnosticsStatus）— 全部从 schemas.ts import，运行时 + 编译时
    锁步。
  - 删除 `mergeEvent(ev as unknown as HarnessEvent)` 这一处 unsafe
    double-cast（参数化后的 parseSseChunk 已经返 HarnessEvent[]）。
  - 测试：新增 `src/schemas.test.ts` 16 个用例覆盖 happy path /
    缺字段拒绝 / 类型错拒绝 / passthrough / parseJson 错误路径 /
    SchemaValidationError 摘要截断；`npm test` (tsc --noEmit) exit 0；
    `npm run build` (vite) 干净（app.js 311 KiB）；
    `npx tsx src/schemas.test.ts` 16/16 通过；
    `cargo test -p agentflow-server --lib ui::` 4/4 通过（embed SPA 没破）。
- DONE Q3.7.3 SSE 断连重连用陈旧 snapshot 从 `seq=-1` 回放；run list 不自动
  刷新；轮询 interval 叠加 (commit 76a8814)
  - 审计来源：`docs/audit/agentflow-ui.md` M4, M5, M6
  - 修复：
    1. **M4 (SSE reconnect stale snapshot)**：
       - 用 `useRef<number>(-1)` 跟踪最新看到的 `seq`，绕开 effect
         deps 漏掉 `events` 导致 closure 抓的是 `setEvents([])` 后
         的空数组的 bug。`appendEvent` 每次 bump ref，initial
         history fetch 把 ref 种到 high-watermark。
       - 抽 `reconnectDelayMs(attempt)` 到 `lib/helpers.ts`（pure
         function 方便单测）。schedule: 250ms → 500ms → 1s → 2s →
         4s → 8s → 16s → cap 30s。
       - `scheduleReconnect()` 现在无限循环，每次 (re)connect 成功
         attempts 归零；transient blip 自愈，不再一次失败就停在
         `error` 永远不重试。
    2. **M5 (polling stacking)** — 给 4 个 setInterval 全加
       `inFlight` 标志：
       - RunConsole 的新 4s loadRuns (M6 新增)
       - HarnessSessionList 4s refresh
       - HarnessSessionDetail 2s session+approvals poll
       - HarnessSessionDetail 5s SSE fallback poll
       前一个 request 没回时下一 tick skip，节奏不变但不再叠加。
    3. **M6 (run list 不刷新)** — RunConsole `loadRuns` 从一次性
       effect 改成 4s interval（镜像 HarnessSessionList），同时
       带 M5 inFlight 守护。新 run 在另一 tab 提交后 4s 内可见。
  - 测试：新增 `src/lib/helpers.test.ts` 10 个用例
    （reconnectDelayMs schedule × 4 + isTerminalRun × 3 + eventTone
    × 3）；`npx tsx` 跑 schemas (16/16) / ErrorBoundary (5/5) /
    helpers (10/10) 全绿；`tsc --noEmit` exit 0；`npm run build`
    干净（app.js 313 KiB，+0.7 KiB vs Q3.7.1 stage B）；
    Playwright MCP 各 route DOM 与之前完全一致；
    `cargo test -p agentflow-server --lib ui::` 4/4 通过（embed
    SPA 没破）；现场观察 4s 周期 `GET /v1/runs?…&limit=20`
    404 在 cadence 上重发，验证 interval 活着。

### Q3.8 — agentflow-nodes: MAJOR

- DONE Q3.8.1 `MarkMapConfig::default()` 硬编码 personal Cloudflare Worker URL
  - 审计来源：`docs/audit/agentflow-nodes.md` M5
  - 修复：`MarkMapConfig::default().api_url` 从 hardcoded
    `markmap-api.jinpeng-ti.workers.dev` 改为读 env
    `AGENTFLOW_MARKMAP_API_URL`；未设置时 `None`。execute 阶段无 url
    fail-fast 抛 `ConfigurationError`，错误消息指明环境变量名 + 安全
    背景。原 live-network test 改成断言 fail-fast 路径，不再依赖第三方
    endpoint 的可用性。
- DONE Q3.8.2 `template.rs` 全局 `Mutex<Tera>` 串行化所有渲染且 `.lock().unwrap()`
  poison panic
  - 审计来源：`docs/audit/agentflow-nodes.md` M3
  - 修复（poison panic 部分）：`tera.lock().unwrap()` 改为
    `.lock().unwrap_or_else(|poisoned| { eprintln! warn; poisoned.into_inner() })`。
    一次 panic-mid-render 不再下次直接撞死整个 workflow；Tera 的 internal
    template cache 是 best-effort，stale entry 可接受。串行化本身（全局
    Mutex）保留——拆 per-thread/per-instance 涉及 Tera filter/function
    注册逻辑搬迁，留作未来优化（template render 通常不是热点路径）。
- DONE Q3.8.3 `NodeFactory` trait 声明且公开但全 workspace 零实现（CLI 用平行
  API）
  - 审计来源：`docs/audit/agentflow-nodes.md` M4
  - 修复：删除 `agentflow-nodes/src/factory_traits.rs` + 移除
    `lib.rs` 的 `pub mod factory_traits` 及 4 个 re-export
    (`NodeConfig` / `NodeFactory` / `NodeRegistry` /
    `ResolvedNodeConfig`)。CLI 用的是 `executor::build_flow_from_definition`
    返回 `GraphNode`（嵌 `Arc<dyn AsyncNode>`），数据模型与该 trait
    返的 `Box<dyn AsyncNode>` 不同，迁移成本远大于价值。lib.rs
    顶部留 deletion note 解释为何移除以及替代方案。
    `cargo build --workspace` 通过。
- DONE Q3.8.4 `src/nodes/while.rs` 0 字节空文件且不在 `mod.rs` 中
  - 审计来源：`docs/audit/agentflow-nodes.md` M6
  - 修复：`rm agentflow-nodes/src/nodes/while.rs`。文件本就是 0 字节
    orphan、未在 `mod.rs` 声明，删除不影响任何调用方。
    `cargo build -p agentflow-nodes` 通过。
- DONE Q3.8.5 documented per-modality feature gates（`asr` / `tts` /
  `text_to_image` 等）实际不存在
  - 审计来源：`docs/audit/agentflow-nodes.md` M7
  - 验收路径选 B：文档已在 Q4.5 修正（CLAUDE.md 改成 "NOT
    individually gated today"），本任务加保护性回归测试 + 文档化
    Cargo.toml [features] 段以杜绝再次漂移。`实装 gate` 路径被
    放弃，理由：agentflow-llm 仍是硬依赖，per-modality feature
    带来的 binary size 收益边际；维护 8 个 feature + CI matrix 行
    成本远大于收益。
  - 修复：
    1. `agentflow-nodes/Cargo.toml` `[features]` 段顶上加 27 行
       注释块，逐项解释 `llm/http/file/template` 各自实际 gate 了
       什么（哪些只 gate `pub use`、哪些 gate transitive dep），
       opt-in `mcp/rag/batch/conditional`，以及为什么
       `asr/tts/text_to_image/image_*/arxiv/markmap`故意不 gate。
       新 contributor 看到 feature 矩阵就能知道 "改这块需要同步
       更新 CLAUDE.md + pin test"。
    2. 新增 `agentflow-nodes/tests/feature_gate_pin_tests.rs` 2 个
       用例：
       - `per_modality_nodes_are_unconditional_under_default_features`：
         在 default features 下 use 8 个 modality / 内容节点
         类型（`ASRNode/TTSNode/TextToImageNode/ImageToImage/
         ImageEdit/ImageUnderstand/ArxivNode/MarkMapNode`），任何
         一个被 `#[cfg(feature = "…")]` 包裹就会编译失败。
       - `cargo_toml_feature_matrix_matches_audit_pinned_shape`：
         `include_str!` 解析 Cargo.toml，断言 9 个必有 feature
         行（包括 `default = [...]`）存在，并断言 8 个 forbidden
         per-modality feature 行（`asr = [`、`tts = [` 等）
         不存在。任何漂移 → 测试失败 + 错误信息明确指向
         CLAUDE.md L2 段 + 本测试要一起改。
  - 测试：`cargo test -p agentflow-nodes --test feature_gate_pin_tests`
    2/2 通过；`cargo test -p agentflow-nodes --all-targets` 既有 33
    个 lib test 仍全绿；`cargo fmt -p agentflow-nodes` 我触碰的
    文件无 diff。Pre-existing `cargo check -p agentflow-nodes
    --no-default-features` 失败（`arxiv.rs:202` 直接用 `reqwest::get`
    但 reqwest 是 http feature-gated 的 dep）在 stash 验证下确认
    与本任务无关，留给未来 follow-up。
- DONE Q3.8.6 `arxiv.rs:252` `r"\\begin{document}"` 匹配字面两个反斜杠，永远
  不命中
  - 审计来源：`docs/audit/agentflow-nodes.md` m7
  - 修复：审计建议的 `r"\\begin\{document\}"` 仍是两个反斜杠（同样错），
    正确字面量是 `r"\begin{document}"`（单反斜杠，匹配真实 LaTeX 源）。
    抽出 `contains_document_marker(&str) -> bool` 自由函数让单测能
    脱离 tar 解压上下文跑。pre-fix 时 `main_content` 永远命中不到，
    每次都 fallback 到 concat 后的 `all_tex_files`，导致下游
    `simplify_latex_content` 处理的内容比预期大得多 / 包含 supporting
    files 噪声。
  - 测试：`detects_real_latex_begin_document_marker` /
    `ignores_supporting_tex_files_without_marker` 单测覆盖正反样本，
    `cargo test -p agentflow-nodes --lib nodes::arxiv` 2/2 通过。

### Q3.9 — agentflow-rag: MAJOR

- DONE Q3.9.1 Chunker `overlap >= chunk_size` panic（无 constructor 校验）
  - 审计来源：`docs/audit/agentflow-rag.md` M
  - 修复：3 个 chunker（`FixedSizeChunker` / `SentenceChunker` /
    `RecursiveChunker`）：
    1. `new()` 保留 infallible 签名（不破 callers），但 clamp
       `chunk_size.max(1)` 和 `overlap.min(chunk_size - 1)`，确保
       `chunk_size - overlap` / `current.len() - self.overlap`
       等 subtraction 都不会下溢，循环总有 ≥1 的 forward stride。
    2. 新增 `try_new(chunk_size, overlap) -> Result<Self>` strict
       validator——`chunk_size == 0` 或 `overlap >= chunk_size` 返回
       `RAGError::chunking(...)`。
    3. `chunking::create_chunker` factory 切到 `try_new?`，
       YAML driven config 中的 typo 现在 surface 成 error 而不
       silently 被 clamp（operator 想知道自己写错了）。
  - 测试：`chunking/mod.rs::tests` 新增 6 个用例（reject 重叠/0、
    accept 正常、clamp 不会无限循环、create_chunker 拒绝越界）。
    `cargo test -p agentflow-rag --lib chunking` 32/32 通过。
- DONE Q3.9.2 `RecursiveChunker` / `SemanticChunker` UTF-8 边界风险（CJK/emoji）
  - 审计来源：`docs/audit/agentflow-rag.md` M
  - 现状：审计点名 RecursiveChunker + SemanticChunker，但深审后
    SemanticChunker.apply_overlap 已经走 `chars().rev().take()`
    char-safe 路径（不 panic）；`split_large_chunk` 用
    `chars().collect().chunks(N)` 也是 char-safe。只有
    `RecursiveChunker::merge_and_overlap` 的 `&current[overlap_start..]`
    用 byte offset 切，CJK / emoji 必 panic。
  - 修复：新增 stable-Rust `floor_char_boundary(text, target)`
    helper（向下找最近 UTF-8 codepoint boundary，复杂度 O(1)，
    最多走 4 次因为最长 codepoint 4 bytes），把
    `&current[current.len() - self.overlap..]` 改成
    `&current[floor_char_boundary(&current, target)..]`。
  - 测试：
    - `recursive_chunker_handles_cjk_overlap_without_panic`：
      108 字 / 324 byte 中文输入，chunk_size=30 / overlap=10
      (10 % 3 != 0 故意让 cursor 落入多字节中)，pre-fix panic，
      post-fix 多 chunk + 每个 chunk 含至少一个 CJK 字符。
    - `recursive_chunker_handles_emoji_overlap_without_panic`：
      4-byte emoji 同样验证。
    - `floor_char_boundary_returns_valid_offset`：纯函数
      invariants（mid-codepoint 向下 snap、ASCII fast path、
      target > len 时 clamp 到 len）。
    `cargo test -p agentflow-rag --lib chunking::recursive` 11/11 通过。
- DONE Q3.9.3 Qdrant client 无 API key / TLS / timeout 配置面（Qdrant Cloud
  不可用）
  - 审计来源：`docs/audit/agentflow-rag.md` M
  - 修复：`QdrantStoreBuilder` 加 4 个 production-critical 配置：
    - `.api_key(impl Into<String>)` —— Qdrant Cloud 必填，
      自托管 + `service.api_key` 也必填；客户端走 gRPC `api-key`
      metadata header。
    - `.with_api_key_from_env()` —— 读 `QDRANT_API_KEY` env
      (`pub const QDRANT_API_KEY_ENV`)，空值 / 未设都 no-op
      避免误锁空 credential。
    - `.timeout(Duration)` / `.connect_timeout(Duration)` ——
      hung 服务不再让 caller 阻塞到永远（推荐 indexing 10-30s、
      search 1-5s）。
    - `.skip_compatibility_check(bool)` —— 跑新版 Qdrant 时
      bypass 客户端/服务端版本检查。
    TLS 走 Qdrant URL scheme（`https://...`）+ tonic 默认 rustls，
    无需额外配置面；纯 plaintext `http://` 部署不受影响。
  - 测试：`qdrant_builder_threads_q393_knobs` 验 4 个 setter
    都正确落到字段；`qdrant_builder_with_api_key_from_env_respects_unset_and_empty`
    覆盖 env 未设/空/有值 3 个分支。
    `cargo test -p agentflow-rag --lib --features qdrant
    vectorstore::qdrant` 8/8 通过。
- DONE Q3.9.4 `IndexingPipeline::index_documents` 纯串行
  - 审计来源：`docs/audit/agentflow-rag.md` M
  - 修复：把原 `for doc in docs { ... await }` 串行循环改成
    `futures::stream::iter(docs).map(...).buffer_unordered(N).collect()`
    fan-out。`IndexingPipeline` 新增 `max_concurrency: usize` 字段
    + `pub const DEFAULT_INDEX_CONCURRENCY: usize = 4`（保守默认值——
    既不触发 OpenAI embedding API 的 RPM/TPM 限速，也不超载共享
    Qdrant 部署的并发 upsert 容量）+ builder `with_max_concurrency`
    + 一次性 override `index_documents_with_concurrency(collection,
    docs, concurrency)`。两个入口都 `concurrency.max(1)` clamp，
    避免 YAML 默认值 0 让 `buffer_unordered` 永久 stall。错误
    隔离保留：单 doc 失败 → `stats.errors += 1`，其他 doc 继续。
  - 测试：5 个新用例（`indexing::tests`）：
    - `index_documents_runs_at_least_two_documents_concurrently` ——
      peak in-flight embedder calls ≥ 2（pre-fix 永远 1），用
      `DelayEmbedder` + `AtomicUsize` CAS 测量真实并发。
    - `index_documents_completes_faster_than_serial_bound` ——
      8 docs × 40ms / concurrency=4 必须 < 半 serial bound。
    - `with_max_concurrency_clamps_zero_to_one` —— 0 → 1。
    - `index_documents_with_concurrency_overrides_pipeline_default`
      —— pipeline default 1，但 one-shot override=6 实测 peak ≥ 2。
    - `index_documents_isolates_per_document_errors` —— 1 失败 + 3
      成功 → `errors=1, documents_processed=3`。
    `cargo test -p agentflow-rag --lib indexing` 5/5 通过；
    `cargo test -p agentflow-rag --all-targets` 既有 164 测试不动；
    `cargo fmt -p agentflow-rag` indexing/mod.rs clean；
    `cargo clippy -p agentflow-rag --all-targets` indexing 无新 warning。
- DONE Q3.9.5 BM25 O(N²) indexing：`add_document` 每次触发完整 IDF 重算
  - 审计来源：`docs/audit/agentflow-rag.md` M
  - 修复：把 `idf: HashMap<String, f32>` + `avg_doc_length: f32`
    迁移到 `derived: Mutex<DerivedStats { idf, avg_doc_length, dirty }>`
    内部可变状态。变更：
    - `add_document` / `add_document_with_metadata` /
      `remove_document` 都改成 `mark_dirty()`（O(1) flag flip），
      跳过 recompute。批量插入 N 个 document 从 O(N²) 退化成 O(N)。
    - 新增 `add_documents(iter)` 批量便利方法，仅 mark dirty 一次。
    - 新增 `finalize()` 显式预热 IDF（serving 前调用，避免首个
      用户请求承担 recompute）。
    - `search(&self)` 内部 lock + 必要时 `refresh_derived_if_dirty`
      —— 接口签名不变，与 `HybridRetriever::search(&self)` 兼容。
    - `calculate_score` 改成接 `&DerivedStats`，调用方 hold mutex
      一次，避免 per-document 重锁。
    Mutex 选 std `Mutex` 而不是 `&mut self` 是为了不破 hybrid 的 API；
    搜索时的 lock 是单 owner 单调用，几乎零争用。
  - 测试 4 个新用例：
    - `add_document_defers_idf_recompute` —— 验证 3 次
      `add_document` 后 derived 仍 dirty + idf 空 + avg_doc_length=0；
      首个 search 后 dirty=false + idf populated。
    - `add_documents_batch_marks_dirty_once` —— 批量插入 3 doc
      → 单次 dirty。
    - `finalize_prewarms_idf` —— `finalize()` 后 idf/avg/dirty
      契约都正确。
    - `remove_document_defers_recompute` —— remove 也走 lazy 路径。
    `test_idf_calculation` 既有用例同步改成 `finalize() + derived.lock()`
    读法。`cargo test -p agentflow-rag --lib retrieval` 28/28 通过
    （含 BM25 17 + Hybrid 11）。
- DONE Q3.9.6 PDF / HTML loader 无 size cap / 无 timeout（DoS 面）
  - 审计来源：`docs/audit/agentflow-rag.md` M
  - 修复：两个 loader 都加 `max_bytes: Option<u64>` 字段 + 默认值
    + `with_max_bytes(Option<u64>)` builder：
    - `PdfLoader` 默认 50 MiB（pub const `DEFAULT_PDF_MAX_BYTES`）
      —— `pdf_extract::extract_text_from_mem` 把全文 byte buffer
      eager 加载，多 GiB 上传可直接 OOM。
    - `HtmlLoader` 默认 10 MiB（pub const `DEFAULT_HTML_MAX_BYTES`）
      —— `scraper::Html::parse_document` 大约 O(n) memory，
      恶意 / runaway crawl 输入也会 OOM。
    - 两个 loader 都先 `fs::metadata().len()` 做 fail-fast 检查，
      然后 read 后再二次检查（防止 producer-still-writing race），
      错误信息明确指向 `with_max_bytes(None)` override 路径。
    Filesystem timeout 不适用于本地 `tokio::fs::*`；如未来加
    远程 URL fetcher，那条路径上再单独引入 HTTP timeout。
  - 测试：`pdf_loader_rejects_files_above_max_bytes` /
    `pdf_loader_default_has_50_mib_cap` /
    `pdf_loader_with_max_bytes_none_disables_cap` /
    `html_loader_rejects_files_above_max_bytes` /
    `html_loader_default_has_10_mib_cap`。
    顺便修了 `test_load_simple_html` / `test_html_title_extraction`
    pre-existing 编译错误（`MetadataValue` 无 `Display`）。
    `cargo test -p agentflow-rag --lib --features pdf,html sources::{pdf,html::tests::html_loader_*}` 8/8 通过。
    （`test_html_removes_scripts` 仍因 SCRIPT_REGEX 用 look-around
    panic，pre-existing 问题非本任务范围。）

### Q3.10 — agentflow-harness: 余下 MAJOR

- DONE Q3.10.1 `HookedTool::build_pending` 硬编码 `step_index: 0`
  - 审计来源：`docs/audit/agentflow-harness.md` M1
  - 修复：`HookConfig` 加 `step_index_counter: Arc<AtomicU64>`
    （和 `seq_counter` 同样的 inject 模式，配套
    `with_step_index_counter` builder），`SharedHookConfig` 复制
    一份。`build_pending` `fetch_add(1, SeqCst)` 取唯一序号
    （`usize::try_from` + `unwrap_or(usize::MAX)` 安全降级 32-bit
    target）。从此每个 ApprovalRequested / ApprovalDecided /
    ToolCallRequested 都带真实 step ordinal，operator audit log
    grep "step_index: 0" 不再撞到所有调用。
  - 测试：`pending_step_index_increments_per_call`（监 hook 收到
    0/1/2 三次单调递增）+ `shared_step_index_counter_is_honored`
    （inject counter seed=42 → 观察 42/43 + counter end=44）。
    `cargo test -p agentflow-harness --lib hooks_runtime` 15/15 通过。
- DONE Q3.10.2 `stop_after_deny` 拒绝后续 call 但不发任何 approval event（静默 gate）
  - 审计来源：`docs/audit/agentflow-harness.md` M2
  - 修复：`hooks_runtime::resolve_proceed_decision` 命中
    `cache.stop_after_deny` 的 short-circuit 路径上现在 emit 一对
    synthetic ApprovalRequested + ApprovalDecided（`request_id`
    namespace 为 `stop-after-deny-<tool>-<uuid>`，
    `decided_by = "stop-after-deny-gate"`），让 operator tail
    JSONL/SSE 能看到 gate 触发原因，而不是工具调用神秘消失。
    `params_summary` 同样走 redaction，避免合成事件意外
    leak sensitive header value。
  - 测试：
    - 既有 `deny_and_stop_blocks_subsequent_calls_without_reprompt`
      改 assert 2 个 Requested + 2 个 Decided（1 real + 1 synthetic）。
    - 新增 `stop_after_deny_gate_emits_namespaced_events_with_redacted_params`
      验证 request_id 前缀 + decided_by + redaction 全部正确。
    - `cargo test -p agentflow-harness --lib hooks_runtime` 13/13 通过。
- DONE Q3.10.3 Cached decision 用合成 `request_id: "cached-<tool>"`，无前置
  `ApprovalRequested`
  - 审计来源：`docs/audit/agentflow-harness.md` M3
  - 修复：`emit_cached_decision` 现在 emit 两个事件：先发一个合成
    `ApprovalRequested`（id `cached-<tool>-<uuid>`，每次唯一；
    带 redacted params_summary、risk、reason、step_index），
    然后发匹配 id 的 `ApprovalDecided`。同一对 id 让 UI
    pending-approval widget + audit replay 的 request_id
    correlation 都能正常工作。原先的 `cached-<tool>` 不带 uuid
    会让连续多次缓存命中都共享同一 id，replay 工具无法区分。
  - 测试：`session_scope_decisions_are_cached_across_calls`
    更新断言为 3 Requested + 3 Decided（1 real + 2 cached），
    并新增 request_id correlation 反断言（每个 Decided 都能在
    Requested 流里找到匹配 id）。`cargo test -p agentflow-harness
    --lib hooks_runtime` 15/15 通过。
- DONE Q3.10.4 `tracing_bridge` 只返回 JsonlEventSink，没真正接
  `agentflow_tracing::ExecutionTrace`，但 CLAUDE.md 把 P-H.5 标 closed
  - 审计来源：`docs/audit/agentflow-harness.md` M4
  - 修复：实装 `ExecutionTraceSink`（per-session 累计 ExecutionTrace
    + on Stopped persist 到任意 `TraceStorage`）+ `open_execution_trace_sink(storage)`
    factory。Translation rules：
    - `SessionStarted` → new `ExecutionTrace`（workflow_id =
      session_id, name = `harness:<session_id>`, status Running）。
    - `StepStarted` → `NodeTrace { node_id: step:<i>, type:
      harness_step, status: Running }`。
    - `ToolCallRequested` → `NodeTrace { node_id: tool:<name>,
      type: tool_call, status: Running }`（多 in-flight 工具调用
      映射为多个独立 node 行，replay 可视化并行）。
    - `ToolCallCompleted` → LIFO 匹配最近的 same-tool Running
      行，填 duration_ms + 翻 Completed/Failed。
    - `Stopped` → 关掉所有还在 Running 的 node 行（防 phantom
      Running 行）+ 翻终态：Completed → TraceStatus::Completed，
      Cancelled → TraceStatus::Cancelled { reason }，
      Failed/LimitReached/ApprovalDenied → TraceStatus::Failed
      { error }；`storage.save_trace(&trace).await`。
    - 其余 variant（Approval / MemorySummary / BackgroundTask）
      由并行 JSONL sink 保真审计，不映到 ExecutionTrace 这个
      operator-facing summary surface。
    可以把两个 sink 都挂进 `SinkChain`：JSONL 走 human replay，
    ExecutionTrace 走 UI / dashboard。CLAUDE.md 同步把 P-H.5 段
    的 "with caveats" 缓和 / Q3.10.4 caveat 删除，只保留 OTLP
    transport (Q2.3.3) 这一个 open item。
  - 测试：5 个新用例：
    - `execution_trace_sink_persists_completed_session` —— 完整
      lifecycle (SessionStarted → 2 tool calls → Stopped Completed)
      落库 3 NodeTrace + TraceStatus::Completed。
    - `execution_trace_sink_maps_cancellation_and_closes_open_rows`
      —— Cancelled 翻 TraceStatus::Cancelled，未完成的 tool 行
      被 close-out 成 Failed。
    - `execution_trace_sink_maps_failed_variants` —— 3 个 Failed
      族 reason 都正确映到 TraceStatus::Failed + 带 error text。
    - `execution_trace_sink_has_distinct_name` —— `name() ==
      "execution_trace"`（区分 SinkChain 中的 JSONL sink）。
    - `execution_trace_sink_drops_orphan_events` —— 没 SessionStarted
      前来的 events 静默丢弃，无 panic 无 fake save。
    `cargo test -p agentflow-harness --lib tracing_bridge` 8/8 通过。

### Q3.11 — agentflow-db: 余下 MAJOR

- DONE Q3.11.1 `next_event_seq` server-side 用 `list_after(..., 10_000)` cap，
  超大 run 会碰撞 PK
  - 审计来源：`docs/audit/agentflow-db.md` M3
  - 修复：
    1. `agentflow-db::EventRepo` trait 加 `max_seq(tenant_id, run_id)
       -> Result<Option<i64>>`；`PgEventRepo` 实现 `SELECT MAX(seq)
       FROM events WHERE tenant_id = $1 AND run_id = $2`（命中
       `events_tenant_run_idx` 覆盖索引，O(1) read）。
    2. `agentflow-server::runs::next_event_seq` 切到 `events.max_seq +
       1` 而不是 paging `list_after(..., 10_000)`；超 10k events 的
       run 不再 silent roll-back seq 导致 PK 碰撞。
    3. `harness.rs::next_event_seq` 同样切到现成的
       `harness_events.max_seq + 1`（之前也走 10k cap）。
  - 测试：`cargo build --workspace` 通过。Pg-touching repo 测试需要
    `AGENTFLOW_DATABASE_TEST_URL`，本机未设置，自动 skip；CI
    覆盖既有 list_after 路径不动。
- DONE Q3.11.2 Connection pool 缺 `test_before_acquire` / `max_lifetime`（云 LB
  reap 问题）
  - 审计来源：`docs/audit/agentflow-db.md` M4
  - 修复：抽出 `apply_pool_defaults(PgPoolOptions)` 一处 statement
    of pool 默认值，primary 和 replica 都走它。设置：
    - `test_before_acquire(true)` —— 处理 RDS / Cloud SQL / Aiven /
      Neon 等云 LB 静默 reap idle 连接的常见 footgun（pool 把死连接
      丢给 caller → "connection closed" 错误位置离根因很远）。
    - `max_lifetime(30 min)` —— 强制定期重建连接，pick up
      server-side 配置改动 + 释放 per-backend memory。
    - `idle_timeout(10 min)` —— burst-then-quiet 工作负载不会持续
      占用全部 max_connections。
    - `acquire_timeout(3s)` 保持不变。
  - 测试：`pool_defaults_enable_test_before_acquire_and_max_lifetime`
    pin 配置（不需要真实 Postgres，用 PgPoolOptions getter assert）。
    `cargo test -p agentflow-db --lib database` 5/5 通过。
- DONE Q3.11.3 Migration `0003` 的 unbatched `UPDATE...FROM` backfill 在大表
  上锁写
  - 审计来源：`docs/audit/agentflow-db.md` M5
  - 修复：选 **文档化 operator playbook** 而不是改已发布的 migration
    （`sqlx::migrate!` 编译期 checksum 每个 .sql 文件，改动会让
    已应用过的部署拒绝启动）。新增 `docs/MIGRATIONS.md`，覆盖：
    1. 普通 upgrade 步骤（小数据量 fresh install / under 10M rows）。
    2. **Q3.11.3 大数据集 0003 outline**：先 `ALTER TABLE ADD
       COLUMN ... DEFAULT 'default'`（Postgres 11+ metadata-only
       fast path），然后用 `DO $$ ... LOOP ... LIMIT 10000 FOR
       UPDATE OF events SKIP LOCKED ... pg_sleep(0.05) ... END
       LOOP $$` 批量 backfill，最后 `CREATE INDEX CONCURRENTLY`，
       全部跑完才让 auto-migrator 进来（此时 UPDATE 0 行 = no-op，
       CONCURRENTLY 索引已存在 = no-op）。
    3. Validation SQL（assert 0 mismatched rows）。
    4. 未来 migration 写法 + 显式 forward-only 回滚说明。
    Migration 文件本身保持不动，部署兼容性 0 风险。

### Q3.12 — agentflow-agents: 余下 MAJOR

- DONE Q3.12.1 Cancellation 通过 `select!` drop in-flight tool future 但不真正
  abort detached work
  - 审计来源：`docs/audit/agentflow-agents.md` M2
  - 决策：选 Path A（"文档化 cooperative 模型"）而不是 Path B（"给
    `Tool::execute` 加 per-call cancellation token"）。Path B 是
    workspace 级 breaking change（agentflow-tools / agentflow-mcp /
    所有 plugin 的 Tool impl 都要改），且对 4 个内建 tool
    (File/Http/Shell/Script) 几乎不增收益——它们的 future 已经
    cooperatively 可 drop（reqwest / tokio::process / tokio::time）。
    Path A 把"实际行为"与"用户期望"对齐，让 tool 作者写 detached
    work 时知道自己需要 wire 自己的 cancellation signal。
  - 修复：
    1. `agentflow-tools/src/tool.rs` `Tool::execute` rustdoc 加 27 行
       cancellation contract 段：明确"runtime 用 tokio::select! drop
       future，无 Tool::cancel hook"；列举 cooperatively-cancellable
       await（reqwest / tokio::time::sleep / tokio::process::Child
       with kill_on_drop / channel recv）vs 不会被 abort 的 detached
       work（tokio::spawn 不 join / spawn_blocking / FFI /
       std::process::Child without kill_on_drop）；指向回归测试 +
       Q3.12.1。
    2. `agentflow-agents/src/runtime.rs` `AgentCancellationToken`
       rustdoc 加 28 行 propagation model 段：把 ReActAgent /
       PlanExecuteAgent / supervisors 的 cancellation check 行为
       条理化；列举 cooperative-OK 和 detached-leak 两类；解释
       为什么这是 intentional trade-off（避免 breaking trait API）。
    3. `AgentRuntime` trait rustdoc 加 16 行同源段，让 custom runtime
       作者实现 trait 时知道契约。
    4. 新增 2 个 regression test：
       - `tool_future_drop_runs_when_token_is_cancelled` — positive
         path：DropFlag guard 在 tool future 中持有，cancellation
         触发后断言 Drop 跑过（cooperative 模型按预期工作）。
       - `detached_spawn_survives_cancellation_for_documentation` —
         limitation path：tool 用 tokio::spawn 起 detached work 后
         立即 await sleep；cancellation 后断言 detached work 仍然
         完成（pin 当前 cooperative 模型的边界，未来若加 per-call
         token 这个 assertion 翻转就强制 author 同步更新 rustdoc）。
  - 测试：`cargo test -p agentflow-agents --lib runtime::tests`
    19/19 通过；`cargo test -p agentflow-agents --lib` 全部 175 个
    lib test 仍全绿；`cargo test -p agentflow-tools --lib` 85/85
    通过；`cargo fmt -p agentflow-agents -p agentflow-tools` 无 diff。
- DONE Q3.12.2 `Blackboard.write_internal` 用 `.expect("blackboard version
  poisoned")`，相邻 lock site 都用 `if let Ok`
  - 审计来源：`docs/audit/agentflow-agents.md` M4
  - 修复：`next_version.lock()` 的 `expect` 改为 `match ... Err(poisoned)
    => poisoned.into_inner()`，跟相邻 `entries.write()` / `ops.lock()`
    的 `if let Ok` 风格对齐。version counter 是 monotonic u64，
    poison 状态下回收锁后内部值仍可信，无 torn-write 风险。
  - 测试：`write_survives_prior_version_lock_panic` — 在另一个线程
    panic-while-holding 该锁触发 poison（用 `is_poisoned()`
    pre-condition assert），然后调用 `write_internal` 必须不 panic
    且 entry 落库 + version=1。`cargo test -p agentflow-agents --lib
    supervisor::blackboard` 13/13 通过。

### Q3.13 — agentflow-core: 余下 MINOR-ish MAJOR

（其余 minor 项在 `docs/audit/agentflow-core.md` 中，承担方在 Q5 sweep 中处理。）

---

## Q4 — Wave 4: Docs ↔ Reality Reconciliation

**Rationale**: 改文档比改代码便宜，但漂移会让贡献者/用户走入与代码不符的
心智模型，长期成本极高。

- DONE Q4.1 `CLAUDE.md` 把 db "Eight-table schema" 改成 9 张表（含
  `user_preferences`）；`agentflow-db/src/lib.rs` / `models.rs` / `repo.rs` 中
  "six tables" 注释一并更新。
  - 审计来源：`docs/audit/agentflow-db.md` 文档漂移
  - 修复：CLAUDE.md 改 "Nine-table schema (... + user_preferences)" +
    把 `UserPreferenceRepo` 加进 Repository layer 列表；`repo.rs`
    顶部 "//! Repository abstractions over the gateway's nine tables"
    + 列出全部 9 个 Pg* repo；`models.rs` "nine-table schema" +
    explicit table list。
- DONE Q4.2 `CLAUDE.md` 关于 `agentflow-server` 的 "Real Flow runner replacing
  StubExecutor lands in v0.4.0" 已落地（`FlowRunExecutor` 已是默认）；更新陈述。
  - 审计来源：`docs/audit/agentflow-server.md` 文档漂移
  - 修复：改成 "`FlowRunExecutor` is the production default and runs
    config-first workflows in-process; `StubExecutor` remains as the
    test-only stand-in for route-plumbing tests that don't need
    real execution."
- DONE Q4.3 `CLAUDE.md` 关于 `agentflow-mcp` 的 "adapter into
  agentflow-tools::ToolRegistry" 描述错位——adapter 实际在
  `agentflow-skills/src/mcp_tools.rs`。
  - 审计来源：`docs/audit/agentflow-mcp.md` 文档漂移
  - 修复：明确"MCP→agentflow-tools::Tool 适配器（`McpToolAdapter` +
    `McpClientPool`）lives in `agentflow-skills/src/mcp_tools.rs`，
    不在本 crate" + 解释为什么 (`agentflow-skills` owns it because
    the skill builder is the entry point that knows which MCP
    servers a manifest declares)。
- DONE Q4.4 `CLAUDE.md` 关于 `agentflow-rag` 提到 "StepFun embedding"，但只
  实装 OpenAI + ONNX。删除 StepFun 声明或单独 TODO 实现。
  - 审计来源：`docs/audit/agentflow-rag.md` 文档漂移
  - 修复：CLAUDE.md L2 — agentflow-rag 段从 "embeddings (OpenAI/StepFun
    API or local ONNX)" 改成 "embeddings (OpenAI API or local ONNX)"，
    末尾加显式注释 "StepFun embedding provider mentioned in earlier
    drafts is not implemented; only OpenAI + local ONNX exist
    today."；顺便补 Q3.9.6 的 PDF/HTML size cap default 提示。
- DONE Q4.5 `CLAUDE.md` 关于 `agentflow-nodes` 的 per-modality feature gates
  与代码不一致（参考 Q3.8.5）。
  - 修复：明确列出实际 default features `["llm", "http", "file",
    "template"]`、opt-in features `mcp` / `rag` / `batch` /
    `conditional`，并显式说明 asr / tts / text_to_image / image_*
    "are NOT individually gated today — those nodes ship in the base
    crate regardless of features"。
- DONE Q4.6 `CLAUDE.md` / `docs/HARNESS_MODE.md` / `docs/STABILITY.md` 关于
  P-H.5 "closed" 的陈述——其中 tracing_bridge 与 OTLP exporter 实际未完成
  （参考 Q2.3.3, Q3.10.4）；要么标 partial、要么落地实现后再标 closed。
  - 修复：P-H.5 段从 "Stability tier beta as of P-H.5 closure" 改成
    "Stability tier **beta** as of P-H.5 closure (with caveats)"，
    显式列出两个 still-open item: (a) `tracing_bridge` 只返
    `JsonlEventSink` 没接 ExecutionTrace 适配 (Q3.10.4); (b) OTLP
    exporter transport deferred (Q2.3.3)。明确区分 "wire shape
    stable" vs "实现 feature-complete"。
- DONE Q4.7 `README.md` "Production-Ready Stability" 章节自信声明 retry /
  timeout / health / checkpoint，但 Q2.4 中 3 处缺陷影响真实可用性；待 Q2.4
  完成后回填 release note。
  - 修复：README "Production-Ready Stability" 段开头加一段提示，
    指 audit / Q2 / Q3.1 已 close 了 ExponentialBackoff jitter
    panic (Q2.4.2) / retry 错误根因丢失 (Q2.4.3) / Ctrl-C signal
    handling (Q3.1.x) 等关键加固，让读者知道示例反映的是 post-audit
    形态，open item 见 `TODOs.md`。

---

## Q5 — Cross-Cutting Sweeps

**Rationale**: 横切问题适合一次 sweep 而不是按 crate 分散处理。

- DONE Q5.1 `unwrap()` / `expect()` production code sweep（违反 user 全局规则）
  - 范围：
    - `agentflow-llm` × 6（Q2.5.3 已含 build_headers，其余顺手）
    - `agentflow-agents` × 2（Q2.9.1 + Blackboard，Q3.12.2 已含）
    - `agentflow-cli` × 2（`commands/eval.rs:240, 252`）
    - `agentflow-nodes` × ~16（含 template.rs，Q3.8.2 已含）
    - `agentflow-core` × 1（ScopedPermit::Drop，Q2.4.6 已含）
    - `agentflow-rag` × ~12（大部分为 compile-time regex，需逐一判定是否真无害）
  - 验收：单一 PR 或 1 PR/crate；新增 clippy lint `-D clippy::unwrap_used
    -D clippy::expect_used` 在 CI workspace 级别（test/example 用 `#[allow]`
    标注）。
  - **WAVE 1 (commit 90d5f72)**：21 个文件 / 298 insertions / 130
    deletions / 21 个 fmt-clean、全部相关 crate `cargo test --lib` 全绿。
    覆盖的命中点：
    - `agentflow-cli/src/commands/eval.rs` × 2（Mutex poison recovery）
    - `agentflow-llm`：6 个 production 站点全部修掉
      （discovery 三个 Default 删除 + model_validator/anthropic/google
      逻辑不变量重写 + openai_asr 链式 fallback + stepfun 三个
      `get_or_insert_with` + VoiceLabel 加 Default）
    - `agentflow-nodes`：6 个文件 14 处 production 命中
      （batch / rag / template / tera_helpers / text_to_image 重构 +
      arxiv 6 个 regex 提到 LazyLock 并 `#[allow(clippy::expect_used,
      reason = "...")]` 注解）
    - `agentflow-rag`：6 个文件 12 处 production 命中
      （onnx file_stem fallback + chunking/sentence last() let-else +
      bm25 Mutex poison recovery × 2 + eval/runner ok_or_else +
      eval/compare let-else + sources/html.rs & preprocessing.rs
      OnceLock 加 allow+reason 注解）
  - **WAVE 2 (commit bdeb30a)**：17 个文件 / 228 insertions / 52
    deletions / 910 lib test 全绿、`cargo clippy --workspace --lib
    --no-deps -- -D clippy::unwrap_used -D clippy::expect_used` 0 errors.
    覆盖的命中点：
    - `agentflow-server` × 15（4 个 Mutex poison hot spot →
      私有 `lock_inner()` helper：harness_approval / events_stream /
      harness / runs；3 个 build-time invariant 加 per-site
      `#[allow(.., reason = "...")]`：serve/events_filter/scheduler）
    - `agentflow-server/build.rs` → file-level `#![allow]`
      （build script 按约定豁免）
    - `agentflow-mcp/src/transport/mock.rs` × 17 → file-level
      `#![allow]`（public test fixture）
    - `agentflow-tracing/src/types.rs` × 2 → `NodeTrace::complete()` /
      `fail()` 捕获 `now` 复用，消除 `completed_at.unwrap()`
    - `agentflow-harness/src/persistence.rs:142` → `ok_or_else`
      返回 HarnessError envelope（替代 `expect("file opened…")`）
    - `agentflow-skills/src/remote_marketplace.rs:64` reqwest
      infallible expect → per-site allow
    - `agentflow-agents/src/common/pdf_parser.rs` × 2 →
      `path.file_name()` 链 `unwrap_or_else("upload.pdf")`
    - `agentflow-agents/src/common/batch_processor.rs` × 2 →
      Semaphore acquire per-site allow（semaphore 私有不会 close）
    - `agentflow-cli/src/commands/skill/mcp_discovery_cache.rs:194`
      → String-only struct `to_vec` per-site allow
  - **CI 落地（wave 2 同一 commit）**：`.github/workflows/quality.yml`
    新增 `clippy-lib-deny` 作业 + 进入 `release-gate.needs`。
    跑：`cargo clippy --workspace --lib --no-deps -- -A warnings -D
    clippy::unwrap_used -D clippy::expect_used`。
    **没有**在 `[workspace.lints.clippy]` 加 workspace-level lint —
    现有 `clippy` job 用 `-D warnings`，workspace-level `warn` 会
    被 `-D warnings` 升格为 deny，破坏 ~438 个未注解的 test 站点；
    专门 CI step 给我们生产库代码的 deny 强制，又不动 test/example/
    bench/build script。
- DONE Q5.2 Workspace-wide redaction audit (commit abd6a6f)
  - 范围：找出所有把 user-supplied data 写入 log / trace / event payload 的
    位置，确认 `agentflow-tracing::redaction` 一致调用。
  - 已知 missing：harness `params_summary`（Q1.7.2）、llm DEBUG log（Q3.6.2）、
    mcp env inheritance（Q3.2.1）、tracing redaction key 子串匹配（Q2.3.5）+
    内联 redaction URL-aware（Q2.3.6）。
  - 验收：新增 grep-based CI lint 拒绝 `tracing::debug!("...{params}..."` 之
    类直接插值用户数据的 pattern；redaction 测试 dataset 扩展到 JWT/Cookie/AWS
    key/Authorization header/CRLF。
  - 修复：
    1. **Production fixes** — 3 处 DEBUG/INFO 级 log 泄漏：
       - `agentflow-agents/src/react/agent.rs:773` `debug!(response = %raw)`
         → `debug!(response_len, response_sha = %fingerprint)`，全文 → TRACE
       - `agentflow-agents/src/react/agent.rs:1222` `info!(thought = %x)`
         → fingerprint + len at INFO，全文 → TRACE
       - `agentflow-agents/src/plan_execute.rs:212` planner response → 同 pattern
    2. **Helper 共享**：`agentflow_llm::prompt_fingerprint` 从 private
       提升为 `pub`，agentflow-agents 直接复用，避免重复 FNV-1a 实现。
    3. **xtask `redaction-lint`**（new）：
       - 扫所有 `agentflow-*/src/**/*.rs` 找
         `(debug|info|warn|error)!(... <danger> = %text, ...)` /
         `... "{<danger>}"` format-string interpolation
       - 11 个 danger tokens：prompt / response / content / body /
         raw_response / planner_text / user_input / message_body /
         params / request_body / response_body
       - `trace!` 故意豁免（生产 exclude）
       - per-site 逃生口：`// allow-redaction-lint: <reason>`
       - 7 个单测 + 1 个 end-to-end temp workspace 测试
       - workspace 当前：`redaction-lint: OK (16 crate dirs scanned)`
    4. **CI gate**：`.github/workflows/quality.yml` 新增 `redaction-lint`
       job + 进 `release-gate.needs`。
    5. **Redaction dataset 扩展**（`agentflow-tracing/src/redaction.rs`）：
       7 个新测试 — JWT (Bearer header / URL query)、AWS credentials
       (3 个 key 一行 env dump)、CRLF 边界、LF-only 边界、Cookie 紧凑
       form、Set-Cookie inline-text known-limitation pin、
       Set-Cookie 结构化 path 完整 redaction。新增的最后两个互相补足：
       一个是 redact_text 的 tokenizer 已知 gap (whitespace 后 colon
       的 header 值会泄漏)，另一个证明生产 redact_value path（agent
       tool call 走的）没这个 gap。
  - 测试：`cargo test -p agentflow-tracing --lib redaction` 16/16 通过；
    `cargo test -p xtask redaction_lint` 7/7 通过；`cargo xtask
    redaction-lint` 现场 0 hit；`cargo test -p agentflow-llm
    -p agentflow-agents -p agentflow-tracing --lib` 413 个 lib test
    全绿；`cargo clippy --workspace --lib --no-deps -- -A warnings
    -D clippy::unwrap_used -D clippy::expect_used`（Q5.1 wave 2 lint）
    仍 clean。
- DONE Q5.3 Workspace-wide signal handler sweep（Q3.1 的 meta）
  - 验收：建立 `agentflow-core::shutdown` 帮助函数，统一 cli + server + worker
    使用；新增 SIGTERM 集成测试模板。
  - 修复：
    1. 新建 `agentflow-core/src/shutdown.rs`：
       - `pub const SIGINT_EXIT_CODE: i32 = 130` / `pub const
         SIGTERM_EXIT_CODE: i32 = 143`（POSIX 128 + signal number）。
       - `pub enum ShutdownReason { Interrupt, Terminate }` +
         `impl ShutdownReason::exit_code()` 让 caller 一行映射到
         正确 exit code，不再在多处 hardcode 130。
       - `pub async fn shutdown_signal_with_reason() -> ShutdownReason`
         —— 安装 SIGINT + SIGTERM (unix-only) handlers，返回最先
         resolve 的 reason。Install failure 走 `eprintln!` +
         `pending::<()>()` 优雅降级（不 panic），保留 Q3.1.x
         pre-existing CLI / worker 的 fail-safe 语义。
       - `pub async fn shutdown_signal()` —— 兼容性 wrapper，丢
         reason 返回 `()`，与 axum `with_graceful_shutdown` /
         `tokio::select!` 完美兼容。
       - Doc-comment 内嵌完整 usage template + tokio::select!
         example，作为"SIGTERM 集成测试模板"的代码化形态——
         doctest 校验类型正确性，但 `# ignore` 不实际触发 signal
         （避免污染其他并发测试进程）。
    2. 迁移三个调用方：
       - `agentflow-cli::shutdown` 改成薄 re-export：`pub use
         agentflow_core::shutdown::{...}`，保留 CLI-only
         `DEFAULT_TRACE_FLUSH_TIMEOUT`。既有 `workflow_ctrl_c_tests`
         的 `SIGINT_EXIT_CODE` pin test 因 re-export 自动通过。
       - `agentflow-server::serve::shutdown_signal` 删除 38 行
         in-line 实现，调 `shutdown_signal_with_reason`
         + match ShutdownReason 维持现有 info-level 日志区分。
         **关键修复**：消除两处 `.expect("install … signal
         handler")` panic 站点（Q1 / Q5.1 关心的 panic 隐患）。
       - `agentflow-worker/src/main.rs` 删 33 行 inline 实现，
         改 `use agentflow_core::shutdown::shutdown_signal`。
         agentflow-core 已是它的硬依赖，无新增 dep。
  - 测试：
    - `agentflow_core::shutdown::tests` 5 个用例：常数 = 128 + signal
      number；`ShutdownReason.exit_code()` round-trip；`PartialEq` /
      `Eq` 派生；签名类型 pin（`shutdown_signal_with_reason() ->
      ShutdownReason`、`shutdown_signal() -> ()`）。
    - 既有 `agentflow_cli` 的 `workflow_ctrl_c_tests::ctrl_c_constants_match_posix`
      和 `cancel_and_flush_writes_workflow_cancelled_to_trace_file`
      均自动通过 re-export 继续验证 CLI 接入 ok。
    - `cargo test -p agentflow-core -p agentflow-server -p agentflow-worker
      --lib --tests` 全部通过；`cargo build --workspace` clean；
      `cargo fmt` 我触碰的文件无 diff。
    - 预先存在 `cargo test -p agentflow-cli` 的
      `llm_chat_is_retired_with_agent_first_guidance` 单元测试失败
      （Q3.5.1 删除 `--model` 等参数后的 stale test）与本任务无关。
- DONE Q5.4 Workspace-wide HashMap → BTreeMap / sorted-iter sweep（影响决定性）
  - 范围：`agentflow-core::topological_sort`（Q2.4.1 之前已 DONE）、
    `agentflow-tools::ToolRegistry`（openai_tools_array / list /
    prompt_tools_description 三个外部可见接口）、`agentflow-llm`
    `ValidationReport.summary()`（CLI 用户面）。
  - 修复：
    1. **`agentflow-tools::ToolRegistry`**：内部 `tools` 字段从
       `HashMap<String, Arc<dyn Tool>>` 改为 `BTreeMap<...>`。
       BTreeMap iteration 按 key 字母序，所有 `tools.values()` /
       `tools.get(name)` 调用 API 不变，但 wire-shape 一致性
       天然落库——同样的工具集，无论注册顺序如何，
       `openai_tools_array()` 字节级别相同。Doc-comment 顶部
       新增 Q5.4 注释解释为什么必须 deterministic（feeds LLM
       request body）。
    2. **`agentflow-llm::validate_all_providers`**：返回前对
       `valid_providers` 和 `invalid_providers` 按 name 排序。
       底层 `HashMap` 保留（迁移成本不划算），但 reporter
       输出 deterministic。
  - 测试：2 个新决定性回归用例：
    - `agentflow_tools::registry::tests::registry_iteration_is_deterministic_across_registration_orders`：
      用 5 个工具（zebra/alpha/mango/delta/echo）以 forward /
      reverse / shuffled 三种顺序注册，断言 `openai_tools_array()`
      JSON 序列化、`list()` name 序列、`prompt_tools_description()`
      字节级相同；额外断言 `list()` 输出按字母序（alpha < delta
      < echo < mango < zebra）pin 住 contract。
    - `agentflow_llm::registry::model_registry::tests::validation_report_summary_is_deterministic_in_provider_order`：
      构造混乱顺序的 valid/invalid provider 列表 + 调用
      summary()，用 string position 断言 anthropic < google <
      openai 顺序、moonshot < zhipu 顺序。
    `cargo test -p agentflow-tools --all-targets` 85 + 既有测试
    全绿；`cargo test -p agentflow-llm --lib --tests` 全部通过；
    `cargo fmt -p agentflow-tools -p agentflow-llm` 无 diff。
    workspace-level pre-existing failure（`agentflow-cli` 的
    `llm_chat_is_retired_with_agent_first_guidance` Q3.5.1 fallout
    + `agentflow-llm/benches/provider_hop.rs` missing `thinking`
    字段）已经在 main 上存在，确认与 Q5.4 无关。

---

## Recently Closed

（5/24 重写后清空。下一次提交后回填实际 closed 项。）

> 5/24 之前的 Recently Closed 全部归档到
> [`docs/archive/TODOs-archive-2026-05-24-p10-optimization-backlog.md`](docs/archive/TODOs-archive-2026-05-24-p10-optimization-backlog.md)。

---

## Deferred / Explicit Non-Goals

（沿用 5/20 版本，无变化。）

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

---

## Execution Notes

- **Wave 优先级硬性**：Q1 全部 DONE 之前不应该 cut v1.0.0 tag。Q2 完成可以发
  v1.0.0；Q3/Q4/Q5 可以滚动到 v1.0.1+。
- 每个 Q-item 都引用了 `docs/audit/<crate>.md` 中的具体 finding ID + file:line；
  开始动手前先重读那段 finding 的 "Why it matters" + "Fix" 字段。
- 一次只挑一个 Q-item；不要在同一 PR 里混不同 crate 的修复（除非是 Q5 sweep）。
- 每个 fix 必须配至少一个 regression test 证明 finding 不会复现。
- Commit message 引用 task ID：`Refs Q1.4.1`。
- Q-item 完成后将状态从 `TODO` 改成 `DONE` 并简述 fix + 测试（如本文件中其他
  DONE 项的写法）。

---

## Quality Gates

每个 task：

- 先读相关代码与 `docs/audit/<crate>.md` finding 详情。
- 实现最小可行修复。
- 跑聚焦的 regression test + crate 全测。
- Conventional commit 提交：`fix(scope): ...` / `refactor(scope): ...`。
- 提交成功后再把 TODO 改成 DONE。

Pre-commit workspace 命令仍是：

```bash
cargo fmt --all
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace
```

---

## Cross-References

- `docs/audit/README.md` — **本次 5/24 深度审计总览**（per-crate 16 份）。
- `RoadMap.md` — 中长期方向；本 Q-段在精神上落实 P1/P2/P5 段未完成的硬化项。
- `docs/CURRENT_STATUS.md` — 当前已实现状态（待与 Q4 一并更新）。
- `docs/STABILITY.md` / `docs/API_COMPATIBILITY.md` — 稳定面契约（Q1.7.1 与
  Q2.2.x 的修复需同步更新）。
- `HARNESS_MODE_EVOLUTION.md` — Harness Mode 设计规范。
- `docs/archive/PROJECT_EVALUATION_2026-05-19.md` — 上一份高层评估（A overall）。
  本次审计在更深层级找到了那份评估未触及的 critical 项。
- `docs/archive/TODOs-archive-2026-05-24-p10-optimization-backlog.md` —
  **最近归档**：P10 优化 backlog 全部 DONE 项 + 少量 polish 未拾起。
- `docs/archive/TODOs-archive-2026-05-20-closed-segments.md` — 12 个全 closed
  P-段（P0–P9 + P-H + P-LLM + M）。
- `docs/archive/TODOs-archive-2026-05-19-recently-closed.md` —
  5/19 扫出的中段历史。
- `docs/archive/TODOs-archive-2026-05-09-n1-n10.md` + `...05-10-p0-p4.md` —
  N 系列 + 早期 P 系列执行计划历史。
