# AgentFlow 项目深度评估报告 (2026-06-06)

- 评估日期：2026-06-06
- 评估范围：workspace 全部 **15 个 Rust crate + 1 个 xtask + 1 个 Web UI crate (`agentflow-ui`)**，`docs/`、`docs/audit/`、`RoadMap.md`、`TODOs.md`、CLI 执行路径、agent runtime、DAG 调度器、Harness Agent Mode、平台化（server/db/worker）、Web UI、插件/Skill/MCP/RAG/Tracing 全链路、9-provider live nightly CI、Q-段审计修复
- 与上一版报告 (`docs/archive/PROJECT_EVALUATION_2026-05-19.md`) 的关系：上一版评估在 18 天前定稿（HEAD `daaa912`），记录 16 个 Rust crate，整体评级 **A**，留下"v1.0.0-rc.1 tag cut 是人工 ops"作为唯一 gating。本版基于 `main` HEAD `76a8814`（2026-05-26）重新校核全部代码、测试与文档，覆盖 18 天内 **153 commits** 落地的所有变更
- **本周期的关键事件**：
  1. `v1.0.0-rc.1` 标签 **已切版**（`a8db47b 2026-05-21`，含 release notes 与 release workflow）
  2. `2026-05-24` 自动化 **16-crate 深度审计**（`docs/audit/`）暴露了上次评估未触及的 **26 CRITICAL / 110 MAJOR / 184 MINOR** finding，主要集中在多租户边界、沙箱默认值、worker gRPC 认证、harness 凭据脱敏、SQLite 健壮性、graceful shutdown
  3. 新的 **Q1–Q5 五波修复段**接管 TODOs.md，覆盖 33 个 Q-section / 108+ 子任务，**目前全部 DONE**（无 open Q-item）
  4. `agentflow-viz` 在 P10.13.1 被 **删除**（孤立未联动），workspace 从 16 crate 收敛到 15 crate
  5. LLM 层引入 **统一 `.thinking()` API**（Anthropic / OpenAI / Google + DeepSeek-R1 surfacing）跨 provider 推理控制
- 编译/测试基线（本评估实测重跑）：
  - `cargo test --workspace --lib`：**1,635** 个 lib 测试全过（5/19: 1,183，+38%）
  - `cargo test --workspace --tests --no-run`：所有集成测试二进制 compile clean
  - `cargo clippy --workspace --lib --no-deps`：clean（无 deny-level warning）
  - **注**：`cargo clippy --workspace --all-targets -- -D warnings` 在新 rustc 1.96 下因 3 个非 lib target 中的新 clippy lint（`manual_char_comparison` / `manual_default_impl` / `collapsible_if`）触发，需要一轮例行 sweep 才能在 CI 红线下过；属于 toolchain upgrade fallout 而非代码回归

---

## 0. TL;DR

18 天内（2026-05-19 → 2026-06-06）AgentFlow 完成了一次**深审驱动的全面硬化**：

1. **v1.0.0-rc.1 标签切版**（`a8db47b`），release.yml workflow 就绪，仅缺人工推 tag → GHCR / GitHub Release artefact 触发；531 commits 自 `v0.2.0` 累计的 RC 候选完整跨过
2. **16-crate 自动化深度审计**（2026-05-24，每 crate 独立 agent，独立 finding 表）首次系统暴露了 5/19 评估遗漏的 26 个 CRITICAL —— 包括 ShellTool `sh -c` 元字符旁路、Linux seccomp `openat(O_CREAT)` 不拦、macOS SBPL `/Library` 过宽、`SandboxPolicy.allowed_paths` 空集放行、`FileNode`/`HttpNode` 完全绕过 `agentflow-tools` 沙箱、`list_runs ?tenant_id=` 跨租户读写、Worker gRPC 无 TLS 无 auth 通道、Google API key 入 URL 串、Harness `params_summary` 不脱敏直接进 JSONL/SSE 等
3. **Q1–Q5 五波修复段全部 DONE**：Q1（生产阻断性安全）→ Q2（正确性/数据完整性）→ Q3（生产化卫生）→ Q4（文档↔现实对齐）→ Q5（unwrap/expect / redaction / 信号横切 sweep）
4. **`agentflow-viz` 退出 workspace**（P10.13.1, `d4a1b2d`）—— 与 trace 实时联动从未联通，独立 Mermaid/DOT 渲染 责任并入 `agentflow-ui` + 既有 CLI graph 输出
5. **跨 provider 统一 `.thinking()` API**（`419f991`）—— Anthropic `thinking={type:enabled,budget_tokens}` / OpenAI Reasoning / Google `generationConfig.thinkingConfig.thinkingBudget` / DeepSeek-R1 reasoning content 统一抽象到 `ThinkingConfig::{Auto, Low, Medium, High, Disabled}` 五档，response 侧 `LLMResponse::thinking` 字段携带 provider raw thinking trace
6. **Q5.1 production-clean unwrap/expect sweep + CI deny-lint**：`agentflow-llm` 6 个 `HeaderValue::from_str(api_key).expect(...)` panic 站点（`.env` 里 trailing newline 就 crash 整个进程）全部改为 `Result`；`agentflow-agents` batch dispatch / `Blackboard.write_internal` poison 路径全部清理；CI 落 `quality.yml::clippy-lib-deny` 把 `clippy::unwrap_used` / `clippy::expect_used` 钉死在生产库代码
7. **Q5.3 unified shutdown 助手**（`dc57dea`）—— CLI / server / worker 三处 SIGINT/SIGTERM 处理统一到 `agentflow-core::shutdown`，server 路径不再 `.expect()` panic
8. **Q5.2 redaction CI lint**（`abd6a6f`）—— `xtask redaction-lint` 扫描全 workspace `tracing::{debug,info,warn,error}!` 宏，禁止裸 prompt/response/content/body/params 插值未经 `agentflow_tracing::redaction::redact_text` / `prompt_fingerprint`

| 维度 | 上次评级 (5/19) | 本次评级 (6/6) | 一句话判断 |
| --- | --- | --- | --- |
| 架构清晰度 | A | **A** | L1/L2/L3/L4 四层心智模型在 viz 删除后更清晰；harness 仍在 L3，三轨入口稳定 |
| DAG 内核成熟度 | A | **A** | N8 闭环后 + Q2.4 确定性 sweep（topo sort BTreeMap）+ Q3.13 robustness module 清理 |
| Agent-native runtime 成熟度 | A- | **A-** | Q2.9 三个契约违反闭口；Q3.12 cancellation 文档 / Blackboard poison tolerance 加固 |
| LLM 抽象成熟度 | A | **A** | `.thinking()` 统一 API + Q1.8 Google key 脱敏 + Q2.5 streaming termination 修复 + Q3.6 余下 MAJOR |
| 工具/权限治理 | A- | **A** | Q1.1–Q1.3 六个沙箱 CRITICAL 全部修；ShellTool 默认 Argv 拒元字符；seccomp `openat` 按位 mask；SBPL 收紧 |
| Config-first / CLI 体验 | A- | **A-** | Q3.5 余下 MAJOR + `workflow logs --follow` reconnect + Ctrl-C 处理；CLI 484 个测试 (+143) |
| 生产可观测性 | A | **A** | Q2.2 drain panic 隔离 + W3C 合规 random ID + inbound traceparent；Q2.3 backpressure / file safety；Q3.10.4 Harness ExecutionTraceSink |
| 服务端平台化 | A- | **A** | Q1.4 多租户边界（list + submit + GET/SSE/action 端点全部 fixed）+ Q3.3 / Q3.4 server hardening + tonic-build 对齐 + H/2 channel 并发 |
| 分布式调度 | B | **B+** | Q1.6 worker gRPC PSK admission metadata 落 wire；Q3.3.2 Semaphore-spawn 真并发；signed-JWT 仍是 v1.x 项 |
| Web UI | B | **B+** | Q3.7 ErrorBoundary + zod 运行时校验 + SSE reconnect / 页面拆分；Token tab-scope（Q1.9）；仍是调试器形态 |
| Harness Agent Mode | A- | **A-** | Q1.7 seq 命名空间合并 + params_summary 脱敏；Q3.10 `step_index` 真值 + synthetic gate events；P-H 路径 Beta 稳定承诺重新可信 |
| Memory | B+ | **A-** | Q2.1 SQLite WAL/busy_timeout/FK + shared pool；Q2.10 row_to_message 错误传播；P10.7.2 age-based encryption-at-rest 完整落地 |
| **综合** | A | **A** | v1.0.0-rc.1 标签已切，全部生产阻断性 finding 已 closed，文档与代码实质对齐 |

**与主题契合度**：项目核心命题——"**DAG + Native-Agent 双底座 + LLM/Tools/RAG/MCP/Skill 能力层 + Rust SDK / CLI / WebUI 上层**"——在代码层面**100% 对齐**。深度审计虽然在多个 critical bug 上暴露了上一份评估的盲区，但**修复全部按 wave 优先级闭环**，没有触发任何架构性回退。

---

## 1. Workspace 全景（15 Rust crate + 1 xtask + 1 Web UI）

### 1.1 crate 规模 & 测试覆盖（本期实测）

| 层 | Crate | 角色 | LOC | 测试数 | 版本 | edition | 成熟度 | Δ vs 5/19 |
| --- | --- | --- | ---: | ---: | --- | --- | :---: | --- |
| **L1 执行内核** | `agentflow-core` | DAG 引擎、AsyncNode、FlowValue、scheduler、checkpoint、retry、timeout、health、events、expression engine、plugin host、**新统一 shutdown 助手 (Q5.3)** | 13,447 | 242 | 0.2.0 | 2024 | ⭐⭐⭐ | LOC -4%（robustness.rs 清理）/ tests +6 |
| **L2 能力适配** | `agentflow-nodes` | 内置 16+ 节点；Q1.3 `FileNode/HttpNode/TextToImageNode` 安全旁路全部修；Q3.8 arxiv main-file 检测 + while.rs 空文件清理 + per-modality feature-gate shape 测试 | 4,860 | 54 | 0.2.0 | 2024 | ⭐⭐⭐ | LOC +7% / tests +9 |
| **L2 能力适配** | `agentflow-llm` | **9 provider**（含 dashscope/deepseek/minimax 共享 `OpenAIProvider`）+ 多模态 + streaming + provider-native tool_calls/tool_choice + OTel traceparent + **统一 `.thinking()` API** + DeepSeek-R1 reasoning content surfacing + Q1.8 Google key 脱敏 + Q2.5 streaming termination 修复 + Q5.4 sorted reports | 14,159 | 192 | 0.2.0 | 2024 | ⭐⭐⭐ | LOC +16% / tests -21（重构合并） |
| **L2 能力适配** | `agentflow-tools` | Tool/Registry/Policy/OS Sandbox (macOS sandbox-exec / Linux seccomp 含 clone/fork/execve 拦截 / no-op)/SSRF/ToolIdempotency；**Q1.1 ShellInterpretation::{Argv,Shell} 默认 Argv 拒元字符；seccomp `openat`/`open`/`creat`/`openat2` 按 flag bit MaskedEq 拦 O_CREAT；SBPL `/Library` + `/private/etc` blanket 收紧；Q1.2 `SandboxPolicy.allowed_paths` 空集翻转到 fail-closed + `HttpTool::new` 返回 Result** | 5,741 | 114 | 0.1.0 | 2024 | ⭐⭐⭐⭐ | LOC +23% / tests +26 |
| **L2 能力适配** | `agentflow-mcp` | client/server/stdio + retry/timeout/重连 + traceparent JSON-RPC meta；**Q2.6 stdio response demux by id + stderr 排空到 tracing + Drop kill_on_drop 避免死锁；Q3.2 Transport per-request demux 移除外层 Mutex + env sandbox + experimental→beta 提升** | 6,906 | 194 | 0.2.0 | 2024 | ⭐⭐⭐ | LOC +4% / tests +12 |
| **L2 能力适配** | `agentflow-rag` | chunk/embed/Qdrant/retrieval/rerank + eval harness (Recall@K, MRR, nDCG@K) + paired sign test + CI baseline + **Q2.8 OpenAI batch sizing（inputs vs tokens 分离）+ Q3.9 RecursiveChunker UTF-8 safe overlap + PDF/HTML loader size caps + Qdrant compat + BM25 IDF lazy + parallelize IndexingPipeline + chunker overlap 校验** + **P10.6.1 pluggable retriever trait + DenseEval / HybridEval (RRF) + `--chunk-size` 维度** | 10,873 | 195 | 0.3.0-alpha | 2024 | ⭐⭐⭐ | LOC +27% / tests +49 |
| **L2 能力适配** | `agentflow-memory` | MemoryStore + Session/SQLite/Semantic + 4 层 layering + **Q2.1 SQLite shared pool + WAL/busy_timeout/FK + 安全 sqlite:// URL builder；Q2.10 row_to_message 错误传播 + `add_message` 取 `&self`；P10.7.2 `AgeEncryptedPreferenceStore<S>` age-based encryption-at-rest + identity-file 帮助器 (chmod 0600)** | 3,764 | 60 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +36% / tests +23 |
| **L3 智能体/编排** | `agentflow-agents` | ReAct + PlanExecute + Handoff/Blackboard/Debate Supervisor + AgentNode + WorkflowTool + Reflection + MemorySummary + eval framework + cost tracking + **Q2.9 三个契约违反闭口；Q3.12 cancellation 文档钉契约 + Blackboard poison-tolerant version lock** | 14,377 | 193 | 0.2.0 | 2024 | ⭐⭐⭐ | LOC +4% / tests +6 |
| **L3 智能体/编排** | `agentflow-skills` | SKILL.md/skill.toml + SkillBuilder + Marketplace + MCP adapter + registry + Validator protocol + **Q1.10 真 Ed25519 marketplace 签名验证器 + bound marketplace fetches + 拒绝 knowledge-path escape；P10.4.1 per-tool `os_sandbox` override；P10.9.1 MCP discovery default-on + 24h 缓存** | 6,478 | 128 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +12% / tests +12 |
| **L3 智能体/编排** | `agentflow-harness` | Harness Agent Mode：HarnessRuntime + hooks/approval + parallel tool calls + background tasks + JSONL persistence + 4 default context providers；**Q1.7 seq counter 跨 runtime+hook 合并 + `redact_secrets` in `params_summary`；Q3.10 真 `step_index` + paired Requested for cached approvals + synthetic gate events on stop_after_deny + ExecutionTraceSink for `HarnessEvent → ExecutionTrace`** | 6,736 | 88 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +18% / tests +11 |
| **L3 智能体/编排** | `agentflow-cli` | workflow / skill / llm / image / audio / mcp / trace / rag / plugin / doctor / harness / serve / cleanup / eval / **marketplace** / **memory prune** / **agent replay --diff** / **harness replay** / **backup** + `CliJsonEnvelope<T>` 统一 JSON；**Q3.5 余下 MAJOR + `workflow logs --follow` reconnect on mid-stream drop + audio asr --prompt/--output 拆分 + plugin+rag 进 default features** | 21,510 | 484 | 0.2.0 | 2024 | ⭐⭐⭐ | LOC +32% / tests +143 |
| **L4 运维/产品化** | `agentflow-tracing` | EventListener + JSONL/SQLite/Postgres + replay + TUI + OTel OTLP + W3C traceparent + redaction + `context::scope` 助手 + **Q2.2 drain panic 隔离 + W3C random IDs + inbound traceparent；Q2.3 backpressure + file safety + redaction + retry rows；Q3.10.4 ExecutionTraceSink** | 5,800 | 67 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +32% / tests +29 |
| ~~`agentflow-viz`~~ | **已删除** (P10.13.1) — 与 trace 实时联动从未联通，独立 Mermaid/DOT 渲染并入 `agentflow-ui` + `workflow graph` 输出 | — | — | — | — | — | -1,801 LOC |
| **L4 运维/产品化** | `agentflow-server` | Axum gateway：Run/Cancel/SSE/Graph/Resume-plan/Bearer auth/profile/CORS+body limit/Web UI/分布式 control plane + Harness routes + tenant 边界 + retention/cleanup + diagnostics + user preferences；**Q1.4 多租户边界（list + submit + GET/SSE/action 全部 fixed）+ multi-row 测试覆盖；Q3.3 H/2 channel 共享 + Semaphore-spawn + worker.proto 对齐；Q3.4 余下 MAJOR + body cap + const-time PSK + cap concurrent live harness sessions；Prometheus /metrics 14 series + live-state size gauge + worker fleet gauges + harness session gauges + cleanup sweep metrics** | 13,353 | 296 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +45% / tests +126 |
| **L4 运维/产品化** | `agentflow-db` | Postgres schema (**9 表**, +`run_retention_overrides` migration `0005` + `mcp_sessions.tenant_id` migration `0006`) + sqlx migrations + 9 个 repos + tenant_id 列 + 完整 CRUD 测试；**Q1.5 SkillInstallRepo::list 租户过滤 + EventRepo / HarnessEventRepo tenant scope；Q3.11 O(1) max_seq for next_event_seq + pool test_before_acquire + max_lifetime + 大表 migration 0003 操作 playbook** | 1,629 | 23 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +33% / tests +8 |
| **L4 运维/产品化** | `agentflow-worker` | 分布式 worker (in-memory + gRPC transport)，claim/heartbeat/execute/report；6 类 node payload；admission/credential/PSK rotation + 资源限制 + 6 类 failure-domain 测试；**Q1.6 authenticate gRPC channel via PSK admission metadata；Q3.1.3/3.3.2 SIGINT/SIGTERM + recoverable Transport backoff + Semaphore-enforced free_slots + spawn-per-permit；P10.16.1 signed-JWT worker admission flavour + capability + locality hints** | 2,100 | 19 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +85% / tests -4（合并 + 删除冗余） |
| **L4 运维/产品化** | `agentflow-ui` | React 19 + Vite 7 + TypeScript 5.8 SPA + **zod 4.4.3** 运行时校验；零额外运行时依赖；编译期 `include_str!` 嵌入 server；**run 列表 + DAG 图 + 事件回放 + run creation form + 诊断面板 + trace 比较 + 偏好同步 + 客户端 event filter + Harness Mode 完整 UI（list/new/detail + SSE + approval cards + resume）+ Q3.7 顶层 ErrorBoundary + 页面组件抽取 + 共享 helpers + SSE reconnect uses live seq + polling guards；Q1.9 tab-scoped API token + fetch-based SSE that carries auth** | 20 TS 文件（含 5 个测试），dist 336 KB | UI 测试 16+ (vitest) | 0.1.0 | n/a | ⭐⭐⭐ | TS 文件 +30%；从单文件 SPA 上升到组件分层 |
| **工具链** | `xtask` | workspace 自动化：`verify-edition` / `examples-smoke` / `bench-gate` / `check-agent-sdk-doc` / **`redaction-lint`** / **`refresh-live-models`** / **`test-gate`** | 3,765 | 20 | 0.1.0 | 2024 | ⭐⭐⭐ | LOC +200%（3 个新子命令） |

**关键观察**：

- **总 Rust LOC ≈ 131.7K**（5/19: ~127.7K, +3%），增量主要来自 `agentflow-cli`（16,281 → 21,510, +32%）、`agentflow-server`（9,185 → 13,353, +45%）、`agentflow-rag`（8,580 → 10,873, +27%）、`agentflow-tracing`（4,401 → 5,800, +32%）；`agentflow-viz` 删除净降 -1,801 LOC
- **总 Rust 测试 ≈ 1,635 lib + 集成**（5/19: 1,183 lib，+38%），主要增量集中在 `agentflow-cli`（341 → 484, +143）、`agentflow-server`（170 → 296, +126）、`agentflow-rag`（146 → 195, +49）、`agentflow-tracing`（38 → 67, +29）
- **crate 数下降**：16 → **15 publishable + 1 xtask + 1 UI**（`agentflow-viz` 退出）
- **DB 表数**：8 → **9**（+`run_retention_overrides`），migration 总数 4 → 6（+`0005_run_retention_overrides.sql` + `0006_mcp_sessions_tenant_id.sql`）
- **agentflow-tools 成熟度升级**：原 ⭐⭐⭐ → ⭐⭐⭐⭐（在审计揭出 6 个 CRITICAL 沙箱旁路后，本期全部修复并补 regression test + 真 sandbox-exec / seccomp 集成测试）
- **CLI 测试数**：341 → **484**（+143）—— 反映 Q3.5 / P10 / P-H 阶段的命令族扩张（marketplace search / memory prune / agent replay / harness replay / backup / `workflow logs`）

### 1.2 四层心智模型（仍然成立，且更清晰）

```
+----------------------------------------------------------------+
| L4 运维/产品化 | tracing · server · db · worker · ui            |
+----------------------------------------------------------------+
| L3 智能体/编排 | agents · skills · harness · cli                |
+----------------------------------------------------------------+
| L2 能力适配    | nodes · llm · tools · mcp · rag · memory       |
+----------------------------------------------------------------+
| L1 执行内核    | core (Flow / GraphNode / FlowValue / Expr /     |
|                |       Plugin / Shutdown)                       |
+----------------------------------------------------------------+
```

- L1 唯一执行核；Q5.3 把 SIGINT/SIGTERM 助手收敛到 `agentflow-core::shutdown`，所有 binary（CLI / server / worker）共享同一退出码 / 信号语义 / `ShutdownReason` 枚举
- L2 全部以 `AsyncNode` / `Tool` / `EmbedClient` / `MemoryStore` / `LLMProvider` 等抽象被 L3 使用；本期 L2 没有新增 crate，但每个 L2 都过了一遍审计 + 修复
- L3 **三轨入口**保持不变：`agentflow-agents`（agent-native）、`agentflow-nodes + agentflow-cli`（DAG）、`agentflow-harness`（长期会话 + workspace-aware + governable agent）。三轨通过 `AgentNode` × `WorkflowTool` × `HarnessRuntime::wrap` 互通
- L4 横切面：删除 `agentflow-viz` 后只剩 5 个 crate，每个都是生产路径上的强依赖（tracing / server / db / worker / ui）；交付物边界更利落

---

## 2. 自 2026-05-19 以来的主要架构变化（153 commits）

### 2.1 commit 类型分布

| 类型 | 数量 | 占比 | 主要主题 |
| --- | ---: | ---: | --- |
| fix | 64 | 42% | Q1（生产阻断性安全）+ Q2（正确性）+ Q3（productization hygiene）全部以 `fix` 落地 |
| feat | 41 | 27% | `.thinking()` 统一 API / P10 系列（Prometheus 指标 / age 加密 / 跨 retriever / read-replica routing / JWT worker admission / harness session gauges 等） |
| docs | 17 | 11% | CLAUDE.md / README / db 文档漂移修正（Q4）+ release notes / audit / RFC |
| chore | 8 | 5% | fmt sweep / 删除孤儿模块 / 维护 |
| test | 7 | 5% | tenant boundary / serve_check / harness route 加固 |
| refactor | 7 | 5% | viz 删除 / mcp client_old 删除 / unwrap sweep / page 组件抽取 / Tera poison recovery |
| ops | 4 | 3% | release dress rehearsal / Apple container smoke / release.yml |
| ci | 3 | 2% | bench cache / macos-13 drop / step-if secrets |
| perf | 1 | 1% | `IndexingPipeline::index_documents` 并行化 |
| build | 1 | 1% | tonic-build 对齐 worker.proto |

**主题观察**：

- `fix` 占 42%（5/19 是 5%）**主轴从"功能补齐"切换到"深审驱动的硬化"** —— 这是 RC tag cut 后正常的 stabilisation 周期
- 0 个 `revert`，0 个 `BREAKING CHANGE` —— 所有修复都是行为对齐到契约/文档，没有切公开 API
- `feat` 仍占 27%（41 commits）—— P10 优化 backlog 在 5/24 归档前仍在持续落地，含 `.thinking()` 统一 API 这种用户可见的横切特性

### 2.2 五大主题（按时间顺序）

#### 主题 A — v1.0.0-rc.1 release engineering（5/20–5/21，~12 commits）

- `P10.0.1` Apple container 中的生产部署演练（fresh VM doctor smoke）
- `P10.0.2` `[workspace.package]` 集中包 metadata（消除 `cargo publish --dry-run` 警告）
- `P10.0.3` release notes + bracket CHANGELOG
- `P10.0.4` `release.yml` workflow + 多架构 GHCR push
- `P10.0.5` 可复现 fresh-VM `agentflow doctor` smoke
- **tag `v1.0.0-rc.1`** 切于 `a8db47b`（2026-05-21）

#### 主题 B — P10 优化 backlog 收尾（5/20–5/21，~30 commits）

19 个 P10 子段，本期持续推进：

- **P10.14.2** Prometheus `/metrics`（slice 1 → FU6）：6 个 follow-up 上线 14 个 live series（live-state size gauge / worker fleet gauges / harness session gauges / cleanup sweep metrics / scrape-time process inspectors / per-component coverage）
- **P10.3** LLM 模块层：`P10.3.3` TokenCounter trait + tiktoken-rs；`P10.3.4` `cargo xtask refresh-live-models` 验证 `/models` 端点
- **P10.6** RAG eval：`P10.6.1` pluggable retriever trait + Bm25 / Dense / Hybrid (RRF)；`P10.6.3` `--chunk-size` 维度
- **P10.7** Memory layering：`P10.7.2` age-based encryption-at-rest + identity helpers；`P10.7.1` `memory prune` CLI
- **P10.16** Worker：`P10.16.1` signed-JWT admission；`P10.16.2` capability + locality hints；FU1 plumb across gRPC
- **P10.17** Web UI：`P10.17.1` debugger-only 定位 RFC；`P10.17.2` `/v1/preferences` sync；`P10.17.3` server-side `?filter=`；`P10.17.4` Playwright e2e nightly
- **P10.8.1** ReAct trace replay diff
- **P10.10.2** harness session replay pacing
- **P10.4.1** per-tool `os_sandbox` override
- **P10.13.1** **删除 `agentflow-viz` crate** 及依赖面
- **P10.19.1** WASM plugin runtime 1-pager + v2 deferral
- **P10.19.3** `docs/ROADMAP_v2.md` consolidating post-v1.0 direction

#### 主题 C — 2026-05-24 自动化深度审计（1 commit）

`78f9424 docs(audit): land 2026-05-24 per-crate deep audit + Q-segment backlog`

- 16 个并行 agent，每 crate 独立 `docs/audit/<crate>.md`
- 总计 **26 CRITICAL / 110 MAJOR / 184 MINOR** finding
- 横切主题：多租户边界、沙箱默认值、secret/PII 泄漏路径、`expect()/unwrap()` in production、graceful shutdown 缺失、连接健壮性、productization claims 与实现差距、文档↔现实漂移、并发反模式、潜伏 silent bug
- TODOs.md 重写成 Q1–Q5 五波修复段，每个 Q-item 引用 `docs/audit/<crate>.md` 中的具体 finding id + file:line

#### 主题 D — Q1–Q5 修复段全部 DONE（5/24–5/26，~75 commits）

| 段 | 主题 | 关键 commits | 完成情况 |
| --- | --- | --- | --- |
| **Q1.1** | ShellTool / seccomp / SBPL 沙箱旁路（CRITICAL × 4） | `b580d3f` (Argv default + 拒元字符) + `7875c14` (seccomp openat) + `4abd3a1` (macOS SBPL) + `da7cda0` (seccomp blocks process creation) | ✅ |
| **Q1.2** | SandboxPolicy 默认值反转 + HttpTool panic（CRITICAL × 2） | `c1b4cb6` (allowed_paths empty-set) + `cfe3693` (HttpTool::new fallible + .no_proxy() tests) | ✅ |
| **Q1.3** | FileNode/HttpNode/TextToImageNode 安全旁路（CRITICAL × 3） | `9862d26` (close bypass + drop silent mock) | ✅ |
| **Q1.4** | Server 多租户边界（CRITICAL × 3） | `7fafb2e` (list + submit) + `ddc497c` (per-id GET/SSE/action) + `55a5fa9` (per-id pin tests) | ✅ |
| **Q1.5** | DB 租户过滤 + schema 缺列（CRITICAL × 1, MAJOR × 2） | `b67bd6b` (SkillInstallRepo + EventRepo + HarnessEventRepo) + migration `0006` | ✅ |
| **Q1.6** | Worker gRPC 通道认证（CRITICAL × 1） | `c688a02` (PSK admission metadata 落 wire) | ✅ |
| **Q1.7** | Harness 冻结契约 + 凭据/PII 泄漏（CRITICAL × 2） | `2ac0a84` (unify seq counter) + `9f819e2` (redact secrets in params_summary) | ✅ |
| **Q1.8** | LLM Google key + 无 HTTP timeout（CRITICAL × 2） | `2be9424` (Google API key 从 URL 移到 header + pin HTTP timeouts) | ✅ |
| **Q1.9** | UI Token + EventSource 安全（CRITICAL × 2） | `6748a43` (tab-scoped API token + fetch-based SSE that carries auth) | ✅ |
| **Q1.10** | Skills marketplace 完整性 | `d79dced` (Ed25519 verifier) + `b89d1d0` (bound fetches + knowledge-path escape) | ✅ |
| **Q2.1** | Memory SQLite 生产硬化（CRITICAL × 2） | `81c844d` (shared pool + WAL + busy_timeout + FK + 安全 path) | ✅ |
| **Q2.2** | Tracing drain task 生还 + W3C 合规 | `49714f7` (panic isolation + W3C random IDs + inbound traceparent) | ✅ |
| **Q2.3** | Tracing 余下 MAJOR | `06306a7` (backpressure + file safety + redaction + retry rows) | ✅ |
| **Q2.4** | Core 确定性 + 边角 panic | `289f44d` (7 hygiene + determinism fixes) | ✅ |
| **Q2.5** | LLM 流式 + panic site（MAJOR × 4） | `f6b275c` (streaming termination + tool_call delta + header panic + unsafe Sync) | ✅ |
| **Q2.6** | MCP 协议正确性（CRITICAL × 2） | `d76325d` (stdio response demux by id + drain stderr) | ✅ |
| **Q2.7** | Server / CLI 关键正确性 | `83cc43a` (JSON body deserialization unify) + `2d9f5cd` (413 preserve) + `dc539a6` (audio asr 拆分) | ✅ |
| **Q2.8** | RAG 批处理性能（CRITICAL × 1） | `6db14f1` (split MAX_BATCH_SIZE into inputs vs tokens) | ✅ |
| **Q2.9** | Agents 契约违反（MAJOR × 3） | `6016b4e` (close 3 contract violations) | ✅ |
| **Q2.10** | Memory 数据完整性 | `d8c9036` (add_message 取 &self + corrupt row 报错) | ✅ |
| **Q3.1** | Graceful shutdown 信号横切 | `2b5de85` (server + CLI) + `e7060d2` (CLI + tracing WorkflowCancelled) + `3e53dfa` (worker SIGINT/SIGTERM) | ✅ |
| **Q3.2** | MCP 健壮性 | `cb1f85b` (Drop deadlock 修) + `4a7c869` (Transport per-request demux + 移除外层 Mutex + env sandbox + experimental→beta) | ✅ |
| **Q3.3** | Worker 并发与 proto | `0a15abc` (Semaphore-spawn) + `da49381` (worker.proto 对齐) + `700a8ed` (drop Mutex<Grpc<Channel>>) | ✅ |
| **Q3.4** | Server 余下 MAJOR | `ff992dd` (env sandbox + 常数时间 PSK + 全局 body cap) + `25455b5` (cap 并发 live harness sessions) | ✅ |
| **Q3.5** | CLI 余下 MAJOR | `2b5de85` (drop ghost flags) + `8dca1b0` (plugin + rag 进 default) + `e51b1cc` (workflow logs --follow reconnect) | ✅ |
| **Q3.6** | LLM 余下 MAJOR | `a33a427` (PII-safe debug + 删除个人 CF 默认) + `da1c10f` (drop 死 dep) | ✅ |
| **Q3.7** | UI 结构性 MAJOR | `9ba4f00` (顶层 ErrorBoundary) + `4e7423d` (zod 运行时校验) + `24537ce` (页面抽取) + `76a8814` (SSE reconnect + polling guards) | ✅ |
| **Q3.8** | Nodes MAJOR | `a33a427` (drop 个人 CF) + `1d27bbe` (drop dead NodeFactory trait) + `0492716` (arxiv main-file + drop while.rs) + `b5e0f9a` (per-modality feature-gate shape test) | ✅ |
| **Q3.9** | RAG MAJOR | `470775f` (overlap < chunk_size 校验) + `a5ab4a0` (UTF-8 safe overlap) + `14c06da` (Qdrant compat) + `3adba45` (并行 IndexingPipeline) + `be73776` (BM25 IDF lazy) + `4174cb5` (PDF/HTML size caps) | ✅ |
| **Q3.10** | Harness 余下 MAJOR | `9c0ce21` (synthetic gate events) + `180144d` (真 step_index + paired Requested) + `030bbda` (ExecutionTraceSink) | ✅ |
| **Q3.11** | DB 余下 MAJOR | `f700b98` (O(1) max_seq) + `8131c8d` (pool test_before_acquire + max_lifetime) + `0d04cf8` (大表 migration playbook) | ✅ |
| **Q3.12** | Agents 余下 MAJOR | `6e23f7b` (cancellation 契约钉文档) + `adbf3c1` (Blackboard poison-tolerant) | ✅ |
| **Q4.1–Q4.7** | 文档 ↔ 现实对齐 | `be6fc28` (CLAUDE.md / README / db comment doc-drift sweep) | ✅ |
| **Q5.1** | Production unwrap/expect sweep + CI deny-lint | `90d5f72` (wave 1) + `bdeb30a` (wave 2 + CI deny) | ✅ |
| **Q5.2** | Redaction workspace audit + xtask grep lint | `abd6a6f` | ✅ |
| **Q5.3** | 统一 SIGINT/SIGTERM shutdown 助手 | `dc57dea` | ✅ |
| **Q5.4** | Tools/LLM 决定性 sweep（BTreeMap + sorted reports） | `6c042c1` | ✅ |

#### 主题 E — 统一 `.thinking()` API（5/25, 1 commit）

`419f991 feat(llm): unified .thinking() API across Anthropic/OpenAI/Google + DeepSeek-R1 surfacing`

- 新 `agentflow_llm::thinking::{ThinkingConfig, ThinkingKind}`
  - `ThinkingConfig::{Auto, Low, Medium, High, Disabled}` 五档 + `Custom { kind, budget_tokens }`
  - Cross-provider mapping：
    - Anthropic: `thinking: { type: enabled, budget_tokens: N }` 请求体（Low=1024, Med=4096, High=16384, Auto=None→provider default）
    - Google: `generationConfig.thinkingConfig.thinkingBudget`（同档 budget）
    - OpenAI: Reasoning（`reasoning_effort: low|medium|high`）+ provider-specific 字段
    - DeepSeek: `reasoning_content` 字段 surfacing 到 `LLMResponse::thinking`
- 响应侧新增 `ProviderResponse::thinking: Option<String>` / `LLMResponse::thinking` —— 调用端可拿到 raw thinking trace
- Fluent API：`AgentFlow::model(...).thinking(ThinkingConfig::High).prompt(...).execute()`
- 注：`provider_consistency` 旧 bench / 部分集成测试因新增 `thinking` 字段需要 round 2 sweep；本期 lib 测试已 green，bench 编译需要补字段

---

## 3. 每 crate 细评（基于本期变更）

### 3.1 `agentflow-core` ⭐⭐⭐ 成熟度：A

- ✅ Flow / scheduler / FlowValue / expression / plugin host 全部 production-ready
- ✅ **Q2.4 7 hygiene/determinism fixes**：`topological_sort` 用 BTreeMap 保证 deterministic 节点顺序；`openai_tools_array` 同样；`ScopedPermit::Drop` 不再 `tokio::spawn`（drop outside runtime 不再 panic）
- ✅ **Q5.3 统一 shutdown** 助手收敛到 `src/shutdown.rs`，三个 binary 共享 `ShutdownReason` 枚举 + `SIGINT_EXIT_CODE` / `SIGTERM_EXIT_CODE` 常量
- ✅ **Q3.13 robustness.rs** orphan module 清理
- ✅ **P10.1.1** FlowValue + checkpoint hot-path criterion benches 进 bench-gate
- 不足：未发现

### 3.2 `agentflow-nodes` ⭐⭐⭐ 成熟度：A

- ✅ 16+ 内置节点全 production-ready
- ✅ **Q1.3 三个 CRITICAL 安全旁路修复**：`FileNode` 现走 `FileTool` 沙箱（路径遍历 guard）；`HttpNode` 走 `HttpTool` 沙箱（SSRF 防护、超时）；`TextToImageNode` 不再 silent fake-data mock
- ✅ **Q3.8** arxiv main-file detection 修复（`\\begin{document}` 双反斜杠 bug）；`while.rs` 0-byte orphan 删除；dead `NodeFactory` trait 删除；per-modality feature-gate shape test pin
- ✅ **P10.2.1** per-node latency criterion benches 进 bench-gate
- 不足：未发现

### 3.3 `agentflow-llm` ⭐⭐⭐ 成熟度：A

- ✅ **9 provider 端到端**（含 nightly live 验证）：OpenAI / Anthropic / Google / Moonshot / StepFun / GLM·Zhipu / DashScope / DeepSeek / MiniMax
- ✅ **统一 `.thinking()` API**（横切特性）：5 档 + Custom，Anthropic / OpenAI / Google native + DeepSeek-R1 reasoning content surfacing；response 携带 raw thinking trace
- ✅ **Q1.8 Google API key 脱敏**：从 `?key=<KEY>` URL 参数移到 `x-goog-api-key` header，避免在 `LLMError` / 日志中泄漏；HTTP timeout pin 到所有 provider
- ✅ **Q2.5 streaming termination + tool_call delta + header panic + unsafe Sync**：4 个 streaming MAJOR 闭口
- ✅ **Q3.6 PII-safe debug logging + 删除个人 CF 默认**：full prompt/response 在 DEBUG 默认走 `redact_text`
- ✅ **Q5.4 sorted reports**：`validation_report_summary` 在 provider 顺序上是 deterministic
- ✅ **P10.3.3** TokenCounter trait + tiktoken-rs；agent message 构造经精确分词器
- ✅ **P10.3.4** `cargo xtask refresh-live-models` 校验每 provider `/models` 端点
- ✅ **fe69594** `LLMConfig::validate` 在缺 key 时 lenient（warn-only），strict 变体 opt-in
- 不足：新 `thinking` 字段导致 `benches/provider_hop.rs` 需要补 init（非 production path）

### 3.4 `agentflow-tools` ⭐⭐⭐⭐ 成熟度：A（升级）

- ✅ **Q1.1 ShellTool / seccomp / SBPL 沙箱旁路 6 CRITICAL 全部修**：
  - **C1 修复**：引入 `ShellInterpretation::{Argv, Shell}` 枚举，默认 `Argv`，inline parser 拒元字符（`;` / `|` / `&` / `$` / `` ` `` / `>` / `<` / `(` / `)` / `\n`）；Shell 模式要求 `backend.is_enforcing() == true`
  - **C3 修复**：macOS SBPL 移除 `(subpath "/Library")` / `(subpath "/private/etc")` blanket，只暴露 `/Library/Frameworks` 与三个 literal 文件；resolver 文件只在 `Capability::Net` 授予时暴露
  - **C4 修复**：Linux seccomp `install_write_open_rules` 把 `openat` / `open` / `openat2` 按 `O_WRONLY` / `O_RDWR` / `O_CREAT` / `O_TRUNC` 每位单独 MaskedEq；`openat2` 因 struct deref 无法 mask 改无条件拦；`creat`/`fork`/`vfork`/`execve` 在 `!Exec` 时拦截
  - **C5 修复**：`clone` / `clone3` / `execve` / `execveat` 在 `!Exec` 时无条件拦
- ✅ **Q1.2 SandboxPolicy 默认值翻转 + HttpTool panic**：`allowed_paths` 空集语义从"放行"翻转到"fail-closed"；`HttpTool::new` 现返回 `Result`，构造失败不 panic；测试用 `.no_proxy()`
- ✅ **Q5.4 决定性**：`openai_tools_array` 用 BTreeMap 保证 JSON 序列化 deterministic
- ✅ 安全核心类型（`SandboxPolicy` / `SandboxBackend` / `EffectiveCapabilities` / `ToolPermission` / `ToolMetadata` / `SecurityProfile`）现在与 wire/trace 契约对齐
- 不足：未发现

### 3.5 `agentflow-mcp` ⭐⭐⭐ 成熟度：A-

- ✅ client / server / stdio + retry / timeout / 重连
- ✅ **Q2.6 协议正确性 2 CRITICAL**：stdio response demux by id（避免响应错配）+ stderr 排空到 tracing（避免 pipe deadlock）
- ✅ **Q3.2 健壮性**：`cb1f85b` StdioTransport 用 `kill_on_drop` 避免 Drop deadlock；`4a7c869` Transport per-request demux + 移除外层 Mutex；`ff992dd` 子进程 `env_clear` 后白名单注入 + 不再继承全 parent env
- ✅ **server 从 experimental 提升到 beta**（`3d1d8fd`）+ fixture compat tests
- ✅ `b12fb42` 删除历史包袱 `client_old` + 重命名 `transport_new` → `transport`
- ✅ P3.8 W3C traceparent 注入到 MCP JSON-RPC `_meta` envelope（5/19 已落地）
- 不足：未发现

### 3.6 `agentflow-rag` ⭐⭐⭐ 成熟度：A

- ✅ 全链路 + eval harness + CI baseline + paired regression gate
- ✅ **Q2.8 OpenAI 批处理性能**：`MAX_BATCH_SIZE` 拆分成 inputs vs tokens 两个独立维度（之前 150× 欠批）
- ✅ **Q3.9 6 个 MAJOR 修复**：`470775f` overlap < chunk_size 校验；`a5ab4a0` RecursiveChunker UTF-8 safe overlap；`14c06da` Qdrant client api_key / timeout / compat；`3adba45` `IndexingPipeline::index_documents` 并行化；`be73776` BM25 IDF 推迟到首次 search；`4174cb5` PDF/HTML loader 50 MiB / 10 MiB size cap
- ✅ **P10.6.1 pluggable retriever trait**：`Bm25Eval` / `DenseEval` / `HybridEval (RRF)` + CLI `--retriever <bm25|dense|hybrid>`
- ✅ **P10.6.3 `--chunk-size <N>` 维度**：chunk id → source doc id 重映射，跨 chunked / un-chunked 报告 metrics 仍可对比
- ✅ Dense + Hybrid eval baselines ship + dual-shape reader（`0adcf5d`）
- 不足：未发现

### 3.7 `agentflow-memory` ⭐⭐⭐ 成熟度：A-（升级）

- ✅ 4 层 trait 表面（5/19 基线）
- ✅ **Q2.1 SQLite 生产硬化**：shared `sqlite_pool` with WAL + `busy_timeout` + `foreign_keys` + 安全 `sqlite://` URL builder；4 个后端统一
- ✅ **Q2.10 数据完整性**：`add_message` 取 `&self`（解锁 H3 并行 tool-call memory writes）；`row_to_message` 在解析失败时 propagate 错误而非 silently 假造 UUID
- ✅ **P10.7.2 age-based encryption-at-rest**：`AgeEncryptedPreferenceStore<S: PreferenceStore>` 透明 age-encrypt 每个 `value`；keys 留 plaintext；on-disk shape `"age:v1:<base64>"` 拒绝 plaintext bleed-through；`generate_identity_file` / `load_identity_file` 拒绝覆盖 + chmod 0600；12 hermetic 测试
- ✅ **P10.7.1** `agentflow memory prune` CLI（preference + entity_facts layer）
- 不足：cloud KMS / envelope re-keying / multi-user 推迟到 v2（`docs/ROADMAP_v2.md` Theme B）

### 3.8 `agentflow-agents` ⭐⭐⭐ 成熟度：A-

- ✅ ReAct + PlanExecute + 三种 Supervisor + AgentNode + WorkflowTool + Reflection + MemorySummary + cancellation + RuntimeLimits + eval framework + cost tracking
- ✅ **Q2.9 三个契约违反闭口**：`expect("every prepared call must have an output...")` 在 batch dispatch 改为 propagate；`Blackboard.write_internal` 的 `.expect("blackboard version poisoned")` 改为 graceful；PlanExecute `token_budget` 现在被消费
- ✅ **Q3.12.1 cooperative cancellation 契约**钉到 rustdoc（之前是隐含约定）
- ✅ **Q3.12.2 Blackboard poison-tolerant version lock**：write 路径在 poisoned mutex 上不再 panic 整个 supervisor
- 不足：未发现

### 3.9 `agentflow-skills` ⭐⭐⭐ 成熟度：A-

- ✅ SKILL.md / skill.toml / SkillBuilder / Marketplace / MCP adapter / registry / Validator protocol
- ✅ **Q1.10 marketplace 完整性**：`d79dced` 真 Ed25519 marketplace signature verifier（之前是 self-checksum 不是签名）；`b89d1d0` bound marketplace fetches + 拒绝 `knowledge-path` escape；`2300647` validator stdin broken-pipe 在 child early-exit 时容错
- ✅ **P10.4.1 per-tool `os_sandbox` override** on `[[tools]]`：`Option<bool>`，`None` 继承 manifest-level，`Some(true)/Some(false)` 单独 opt-in/out；只对 `shell` / `script` 生效；`agentflow skill inspect --explain-permissions` 打表
- ✅ **P10.9.1 MCP capability discovery default-on + 24h cache + spinner**（5/19 是 opt-in，现在默认开）
- ✅ **P10.9.2 `agentflow marketplace search --format text|json|json-envelope`**
- 不足：未发现

### 3.10 `agentflow-harness` ⭐⭐⭐ 成熟度：A-

- ✅ H0/H1/H2/H3/H4/H5 全 5 phase 完成（5/19 基线）
- ✅ **Q1.7 冻结契约 + 凭据/PII 泄漏 2 CRITICAL**：`2ac0a84` `HarnessRuntime` 内部 `seq` 与 `HookConfig.seq` 合并到单一 source of truth（恢复 Beta 冻结契约里"monotonic, never gap"承诺）；`9f819e2` `ApprovalRequest.params_summary` / `ToolCallRequestedPayload.params_summary` 调用 `agentflow-tracing::redaction::redact_secrets` 后才 emit 到 JSONL / SSE
- ✅ **Q3.10 余下 MAJOR**：`9c0ce21` synthetic gate events on `stop_after_deny`（恢复 trace 完整性）；`180144d` 真 `step_index`（不是 0 占位）+ paired Requested for cached approvals；`030bbda` `ExecutionTraceSink` 把 `HarnessEvent` 流翻译成 `agentflow_tracing::ExecutionTrace`，持久化任意 `TraceStorage` backend（关闭 `tracing_bridge` 只能写 JSONL 的缺口）
- ✅ `25455b5` server 端 cap concurrent live harness sessions
- 不足：H6 advanced compatibility 仍 DEFERRED 到 Later Tracks（`docs/H6_PROMOTION_CRITERIA.md`）

### 3.11 `agentflow-cli` ⭐⭐⭐ 成熟度：A-

- ✅ 14+ 命令族 + `CliJsonEnvelope<T>` 统一信封（5/19 基线）
- ✅ **本期新增 4 个子命令**：`marketplace search` / `memory prune` / `agent replay --diff` / `harness replay` / `backup`（`pg_dump` + tar 编排）
- ✅ **Q3.5 余下 MAJOR**：`2b5de85` drop ghost flags（discarded `llm chat` flags）；`8dca1b0` plugin + rag 进 default features（修文档漂移）；`e51b1cc` `workflow logs --follow` 在 mid-stream drop 时重连
- ✅ **Q2.7 audio asr 拆分 `--prompt` 与 `--output`**（之前 positional arg mismatch silently 写文件）
- ✅ **Q3.1.2 Ctrl-C handler + WorkflowCancelled trace persistence**
- ✅ **484 个测试**（5/19 比 341，+143）
- 不足：未发现

### 3.12 `agentflow-tracing` ⭐⭐⭐ 成熟度：A

- ✅ EventListener / JSONL / SQLite / Postgres / replay / TUI / OTel OTLP / W3C traceparent / redaction / `context::scope` 助手
- ✅ **Q2.2 drain panic 隔离 + W3C 合规 random IDs + inbound traceparent**：drain task panic 后不再 silent event loss；OTel `trace_id` / `span_id` 用真 random 而不是 FNV hash（W3C 合规）
- ✅ **Q2.3 backpressure + file safety + redaction + retry rows**：drain channel 加 bound；事件文件路径 traversal-safe；retry row 不再 phantom "running"
- ✅ **Q3.10.4 ExecutionTraceSink**：Harness `HarnessEvent` 流可翻译并持久化到任意 `TraceStorage` backend，恢复"tracing_bridge 一致性"的 CLAUDE.md 承诺
- 不足：第一方 OTLP transport（HTTP/gRPC + TLS + auth）仍 deferred；操作员仍需 BYO `OtelSpanSink` 实现

### 3.13 `agentflow-server` ⭐⭐⭐ 成熟度：A

- ✅ Run / Cancel / Graph / Event History / SSE / Skill API / Bearer auth / 安全 profile / CORS+body limit / embedded Web UI / Harness routes / tenant / retention / diagnostics / preferences（5/19 基线）
- ✅ **Q1.4 多租户边界 3 CRITICAL 全部修**：`7fafb2e` list + submit；`ddc497c` 每个 `:id`-bound GET / SSE / action 端点；`60b3987` test 不再用 global TRUNCATE race；`55a5fa9` 每个 endpoint pin tenant boundary
- ✅ **Q3.3 worker control plane**：`700a8ed` drop `Mutex<Grpc<Channel>>` 共享 H/2 channel；`0a15abc` Semaphore + spawn-per-permit 真并发；`da49381` 用 tonic-build 对齐 worker.proto
- ✅ **Q3.4 余下 MAJOR**：`ff992dd` env sandbox + 常数时间 PSK 比较 + 全局 body cap；`25455b5` cap 并发 live harness sessions；`83cc43a` JSON body deserialization 走 error envelope；`2d9f5cd` 413 Payload Too Large 透传 JsonReq extractor；`03da1de` emit terminal stopped event when `LiveHarnessExecutor` fails
- ✅ **Prometheus `/metrics`** 14 live series（live-state size gauge / worker fleet / harness session gauges / cleanup sweep / scrape-time process inspectors）
- ✅ **P10.14.1 per-run retention override** on POST /v1/runs
- ✅ **P10.15.2 optional read-replica routing** for `get_*` / `list_*`
- ✅ **P10.16.1 signed-JWT worker admission flavour** + capability + locality hints
- 不足：未发现

### 3.14 `agentflow-db` ⭐⭐⭐ 成熟度：A-

- ✅ **9 表 schema**（+`run_retention_overrides`）+ migration 0001-0006
- ✅ **Q1.5 租户过滤 1 CRITICAL + 2 MAJOR**：`b67bd6b` `SkillInstallRepo::list` + `EventRepo` + `HarnessEventRepo` 全部 tenant-scope；migration `0006_mcp_sessions_tenant_id.sql` 补缺列
- ✅ **Q3.11 余下 MAJOR**：`f700b98` O(1) `max_seq` for `next_event_seq`（之前是 phantom "running" row 风险）；`8131c8d` pool `test_before_acquire` + `max_lifetime` defaults（cloud LB reaping）；`0d04cf8` 大表 migration `0003_tenant_id_columns.sql` 操作 playbook
- ✅ `432bfb0` `PgRunRepo::list_filtered` 现在 SELECT retention 列
- 不足：backup/restore 仍是文档 + CLI 编排（`agentflow backup` 在 CLI 端，`pg_dump` + tar）

### 3.15 `agentflow-worker` ⭐⭐⭐ 成熟度：B+

- ✅ 6 类 node payload（5/19 基线）+ admission / credential / PSK rotation + resource limits + 6 failure-domain tests
- ✅ **Q1.6 gRPC 通道认证 CRITICAL**：`c688a02` worker 客户端把 PSK 注入 gRPC metadata；服务端 `AuthenticatedControlPlane` 在 wire 上强制（之前是配了但从未在 wire 上 enforce）
- ✅ **Q3.1.3 SIGINT/SIGTERM handler + recoverable Transport backoff**：`run_forever` 第一次 transport error 不再 abort
- ✅ **Q3.3.2 Semaphore-enforced free_slots + spawn-per-permit**：真实并发，不再 `Mutex<Grpc<Channel>>` 串行
- ✅ **P10.16.1 signed-JWT worker admission flavour**（HS256 / RS256）
- ✅ **P10.16.2-FU1 capability + locality hints across gRPC**
- 不足：TLS 仍是 operator 责任（reverse proxy / mTLS sidecar），第一方 TLS 内置仍是 v1.x

### 3.16 `agentflow-ui` ⭐⭐⭐ 成熟度：B+

- ✅ Run list / DAG 图 / 事件回放 + SSE 实时更新 + Run creation form + 诊断面板 + trace 比较 + 偏好同步 + 客户端 event filter + Harness Mode 完整 UI（5/19 基线）
- ✅ **Q1.9 Token 与 EventSource 安全**：`6748a43` tab-scoped API token + fetch-based SSE that carries auth（之前是 `localStorage` + 裸 EventSource 静默降级 polling）
- ✅ **Q3.7 结构性 MAJOR**：`9ba4f00` 顶层 React ErrorBoundary fallback；`4e7423d` zod 4.4.3 runtime JSON validation；`24537ce` 页面组件抽取 + 共享 helpers（`pages/` + `components/` + `lib/`）；`76a8814` SSE reconnect uses live seq + polling guards + run list refresh
- ✅ **P10.17.4 Playwright e2e workflow** + 本地 `npm run e2e`
- ✅ **P10.17.3 server-side `?filter=`** pre-filter
- ✅ **P10.17.2 `/v1/preferences` sync** for run-console tenant
- ✅ **P10.17.1 debugger-only 产品定位** pin 到 RFC
- ✅ **UI 测试 16 个**（vitest）：`eventFilter` / `preferences` / `schemas` / `ErrorBoundary` / `helpers`
- 不足：仍标位"调试器"而非生产前端；缺更深的 dashboard / runbook

### 3.17 `xtask` ⭐⭐⭐ 成熟度：A-

- ✅ `verify-edition` / `examples-smoke` / `bench-gate` / `check-agent-sdk-doc`（5/19 基线）
- ✅ **本期新增 3 个子命令**：`redaction-lint`（Q5.2 CI）；`refresh-live-models`（P10.3.4，校验每 provider `/models` 端点）；`test-gate`（P10.19.2，workspace test-timing regression gate）
- ✅ `ff77b66` `bench-gate --allow-missing` 容忍 absent baseline；`d82ff6e` 支持 edition inheritance + allowlist FlowValue variants + bench-gate test injection
- 不足：未发现

---

## 4. 与主题契合度评估（更新版）

> **主题命题**：DAG + Native-Agent 双底座，基于大模型的能力层（LLM/VLM, Tools, RAG, MCP, Skill, subAgent），上层 Rust SDK + CLI + WebUI。

| 主题维度 | 当前对齐情况 | 偏离风险 | 较 5/19 变化 |
| --- | --- | --- | --- |
| **DAG 底座** | ✅ `agentflow-core` 全部 production；Q2.4 determinism + Q5.3 unified shutdown | 无 | + 7 hygiene fixes + 统一 shutdown |
| **Native-Agent 底座** | ✅ ReAct + PlanExecute + 三 Supervisor + AgentNode + WorkflowTool + cancellation + eval | 无 | + Q2.9 / Q3.12 contract closure |
| **Harness Agent Mode** | ✅ 5 phase 全 closed；envelope/hooks/approval/parallel/tasks/server+UI 完整；Q1.7 seq + redaction 修复后 Beta 冻结承诺重新可信 | 无 | + Q1.7 + Q3.10 全部 |
| **LLM / VLM 组件** | ✅ 9 provider 原生 tool calling + 多模态 + streaming + cross-provider invariants + 统一 `.thinking()` API | 无 | + thinking + Q1.8 + Q2.5 + Q3.6 |
| **Tools 组件** | ✅ Tool/Registry/Policy + OS Sandbox（fixed × 6）+ SSRF + Idempotency + spawn-time policy | 无 | + Q1.1 + Q1.2 全部修 6 CRITICAL |
| **RAG 组件** | ✅ 全链路 + eval harness + CI baseline + Dense / Hybrid retriever + chunk-size 维度 | 无 | + Q3.9 6 个 MAJOR + P10.6 retriever 矩阵 |
| **MCP 组件** | ✅ client/server/stdio + 194 测试 + traceparent JSON-RPC meta + Skill adapter；server 从 experimental → beta | 无 | + Q2.6 / Q3.2 + server beta |
| **Skill 组件** | ✅ SKILL.md + Marketplace（Ed25519 真签名）+ Builder + Validator protocol + Admission filter + per-tool sandbox override | 无 | + Q1.10 + P10.4.1 + P10.9.1 default-on |
| **subAgent 组件** | ✅ 三 Supervisor + AgentNode + 并行 tool calls + background tasks | 无 | + Q3.12 cancellation + Blackboard poison tolerance |
| **Memory 组件** | ✅ 4 层 layering + SQLite 生产硬化 + age-based encryption-at-rest | 无 | + Q2.1 + Q2.10 + P10.7.2 加密 |
| **Rust SDK 上层** | ✅ 公共 traits + Re-exports + 完整 examples + CHANGELOG | 无 | + Q5.1 production-clean unwrap/expect + CI deny |
| **CLI 上层** | ✅ 14+ 命令族 + JSON envelope + serve/cleanup/diagnostics/eval + marketplace/memory/agent replay/harness replay/backup | 无 | + Q3.5 + 新增 5 个子命令 |
| **WebUI 上层** | 🟡 B+ 级（调试器形态，但已含 ErrorBoundary + zod 校验 + 组件分层 + Playwright e2e） | 无（debugger-only RFC 已 pin） | + Q1.9 + Q3.7 全部 |
| **平台化（server/db/worker）** | ✅ retention / tenant / backup / worker auth / metrics / 6 类 node payload；多租户边界现在是 hard boundary | 无 | + Q1.4 / Q1.5 / Q1.6 + Prometheus 14 series |

**结论：项目在所有 14 个主题维度上 100% 对齐**。两个 DEFERRED 项（channel adapters + Local OS control）仍是 RoadMap "Non-Goals For V1"；新增 DEFERRED：原生动态库 plugin（subprocess JSON-RPC 是唯一 v1 runtime）+ Harness H6 advanced compatibility。路线图守得很紧。

---

## 5. 风险盘点（更新版）

| # | 风险 | 严重性 | 较 5/19 变化 |
| --- | --- | --- | --- |
| R1 | FlowValue::File/Url checkpoint 类型损失 | — | ✅ 已解决（N8） |
| R2 | LLM 工具调用 cross-provider 不稳健 | — | ✅ 已解决（9-provider 矩阵） |
| R3 | server / db scaffold | — | ✅ 已解决 |
| R4 | 多智能体协作仅雏形 | — | ✅ 已解决 |
| R5 | 权限过滤型，缺 OS jail | — | ✅ 已解决 + **Q1.1/Q1.2 关闭 6 个深审 CRITICAL** |
| R6 | OTel context 跨 LLM hop 断裂 | — | ✅ 已解决 |
| R7 | RAG 缺评测 harness | — | ✅ 已解决 + Dense / Hybrid retriever |
| R8 | YAML schema 错误经验 | 低 | 未变 |
| R9 | workspace edition 不统一 | — | ✅ 已解决 |
| R10 | Worker 仅支持 3 类 node 执行 | — | ✅ 已解决（6 类） |
| R11 | Worker 缺 auth/admission/resource limit | — | ✅ 已解决 + **Q1.6 wire-level enforcement** |
| R12 | Web UI 是 alpha 形态 | — | ✅ 升级到 B+（Q3.7 ErrorBoundary + 组件分层） |
| R13 | Memory layering / 长期记忆 schema 初级 | — | ✅ 已解决 + **P10.7.2 age-based encryption** |
| R14 | CLI feature 矩阵 CI 覆盖不足 | — | ✅ 已解决 |
| R15 | `AgentFlow::init()` fail-close 全 provider key | — | ✅ 已解决（`fe69594` lenient + strict opt-in） |
| R16 | DashScope / DeepSeek / MiniMax 共享 OpenAIProvider | 低 | 未变（按个案 promote 策略保持） |
| R17 | v1.0.0-rc.1 tag 切版未执行 | — | ✅ **已切**（`a8db47b 2026-05-21`，含 release.yml） |
| **R18 (新)** | 多租户边界曾是软提示而非安全边界 | — | ✅ **已修**（Q1.4 + Q1.5 + 测试每端点 pin） |
| **R19 (新)** | Worker gRPC 通道无 wire-level auth | — | ✅ **已修**（Q1.6 PSK admission metadata 落 wire） |
| **R20 (新)** | Harness `params_summary` 凭据/PII 泄漏 | — | ✅ **已修**（Q1.7 redaction + seq 命名空间合并） |
| **R21 (新)** | ShellTool / seccomp / SBPL 6 CRITICAL 沙箱旁路 | — | ✅ **已修**（Q1.1 / Q1.2） |
| **R22 (新)** | SQLite 无 WAL / busy_timeout / FK | — | ✅ **已修**（Q2.1） |
| **R23 (新)** | tracing drain panic 静默 event loss | — | ✅ **已修**（Q2.2） |
| **R24 (新)** | Google API key 入 URL 串泄漏 | — | ✅ **已修**（Q1.8） |
| **R25 (新)** | 全 workspace 无 graceful shutdown | — | ✅ **已修**（Q3.1 + Q5.3 统一助手） |
| **R26 (新)** | Production unwrap/expect 违反全局规则 | — | ✅ **已修**（Q5.1 wave 1 + 2 + CI deny-lint） |
| **R27 (新)** | rustc 1.96 升级带来 3 个新 clippy lint 在 lib 上触发 | — | ✅ **已修**（2026-06-06 housekeeping commit `97e4b8c`）：批量修复 11 个 clippy 1.96 新 lint 类型（`manual_pattern_char_comparison` / `derivable_impls` / `collapsible_if` / `doc_lazy_continuation` / `needless_borrow` / `unnecessary_cast` / `unnecessary_sort_by` / `assertions_on_constants` / `field_reassign_with_default` / `manual_contains` / `unused_mut`）跨 11 个 crate；`cargo clippy --workspace --all-targets -- -D warnings` 全 clean |
| **R28 (新)** | `agentflow-viz` 删除后，旧用户文档中"VisualGraph → Mermaid/DOT/JSON" 引用可能产生死链 | 低 | CLAUDE.md 已修；外部用户引用需要在 RC release notes 显式提醒 |
| **R29 (新)** | `provider_consistency` bench/integration 测试新 `thinking` 字段未覆盖 | — | ✅ **已修**（2026-06-06 housekeeping commit `97e4b8c`）：`benches/provider_hop.rs` 的 `ProviderRequest` 构造点补 `thinking: None` |
| **R30 (新)** | UI E2E nightly 长期红（2026-05-23 起 14 连续夜失败） | — | ✅ **已修**（2026-06-06 commits `e7997e8` + `e2b67b6`）：根因是 Q1.4.2/3 多租户硬边界落地后 UI `apiFetch` 没传 `X-Agentflow-Tenant` header；body tenant ≠ auth header tenant → 403。修复：`apiFetch` 新增可选 4th 参数 `tenant`；`HarnessSubmitForm` / `HarnessSessionList` / `HarnessSessionDetail` / `RunCreateForm` 全部传 tenant；detail 页通过 URL `?tenant=` 串联 tenant；harness e2e regex 拓宽允许 `?tenant=` 后缀。CI cargo cache 隐藏了之前 14 天的真实失败 —— 直到 dist asset 变更触发 cache invalidation 才暴露 |

---

## 6. 优化路线（基于当前现状重排优先级）

> 当前主轴：**v1.0.0-rc.1 → v1.0 GA 的 stabilisation 期**。tag 已切，但 GHCR push / GitHub Release artefact / dress rehearsal 反馈轮 仍需运维。

### 6.1 v1.0 GA 前（建议短期 polish）

1. **Toolchain housekeeping**：3 个新 rustc 1.96 lint 在 `agentflow-tracing` / `agentflow-tools` / `agentflow-rag` 触发；批量修复 `manual_char_comparison` / `derive Default` / `collapsible_if` 让 `clippy --all-targets` 再次 clean
2. **`provider_consistency` bench/integration `thinking` 字段 init sweep**：让所有 `ProviderRequest` 构造点显式 `thinking: None`（或 `Default::default()`）
3. **`agentflow doctor` fresh-VM smoke 修补**：dress rehearsal 暴露的 F4（fresh host warning）已在 release notes 写 runbook；可考虑把 runbook 步骤变成 `doctor --bootstrap` 的可执行自检
4. **OTLP first-party transport**（R28 / Q2.3.3 deferred）：HTTP/gRPC + TLS + auth 仍待，操作员仍需 BYO；若 v1.0 GA 前能 ship 会显著降低 observability 接入门槛
5. **Web UI 产品化进一步规划**：debugger-only 已 pin，但 P10.17 阶段揭示 UI 是新用户 onboarding 关键面；可基于 RC 反馈决定是否启动"运营仪表盘"路线（运行成本 / retry rates / policy decisions / worker utilization）

### 6.2 v1.x 中期演进

6. **Plugin runtime WASM 选项**：subprocess JSON-RPC 已是稳定 v1，WASM 作为 v2 候选；`docs/ROADMAP_v2.md` 已留 Theme
7. **Worker 第一方 TLS / mTLS**：当前依赖 reverse proxy + sidecar；若客户有强需求可内置
8. **DashScope/DeepSeek/MiniMax dedicated provider 模块**：仅在 vendor 出现 wire 分歧时再做
9. **`agentflow-rag` cloud KMS + envelope re-keying + multi-user encryption**：v2 Theme B 已留
10. **Slash-command 生态 / TUI 形态**（Harness H6）：按个案 promote

### 6.3 文档维护

11. **`CLAUDE.md` 已与最新现状基本一致**（Q4.1–Q4.7 sweep 已修：db 9 张表 / FlowRunExecutor 已上线 / mcp adapter 位置 / per-modality feature gate 状态 / StepFun embedding 移除）
12. **`docs/audit/`**：16 个深度审计报告是新增的高价值参考资产；建议在每个 release window 重跑一次（成本可控，每 crate 一个并行 agent）
13. **`docs/ROADMAP_v2.md`**：post-v1.0 direction 已 consolidate；v1.0 GA 后 promote 到顶级
14. **CHANGELOG.md** 已用 conventional commits 维护到 `[v1.0.0-rc.1]` block，`[Unreleased]` 已准备好下个 tag

---

## 7. 推荐发布节奏

| 里程碑 | 目标 | 状态 |
| --- | --- | --- |
| v0.3.0 | 平台骨架 + tool calling 原生 + checkpoint 保真 | ✅ 已发 |
| v0.4.0 | 协作范式 + 沙箱强化 + OTel 端到端 + RAG eval | ✅ 已发 |
| v0.5.0 | Server 完整化 + sandbox 可见性 + provider 一致性矩阵 | ✅ 已发 |
| **v1.0.0-rc.1** | Worker 生产化 + Agent/Memory eval + Web UI 产品化 + CLI JSON 契约 + Harness Mode 完整 + 16-crate 深度审计修复 | ✅ **已切版**（`a8db47b 2026-05-21`） |
| **v1.0.0-rc.2** | RC 反馈轮 + toolchain housekeeping + OTLP first-party transport（可选） | 候选窗口已打开；本期 Q1-Q5 收尾后即可启动 |
| v1.0 GA | 文档收敛 + 稳定承诺 + CI 基线全绿 + RC 反馈一轮 | rc.2 切完后 |

---

## 8. 最终结论

AgentFlow 在 18 天内完成了一次**深审驱动的全面硬化**，并切下 **v1.0.0-rc.1 标签**。代码层从"v1.0.0-rc.1 候选窗口完全打开"过渡到"**v1.0.0-rc.1 已签，全部生产阻断性 finding 全部 closed，进入 RC 反馈轮**"。

**确认对齐项目主题**：

- ✅ **DAG 底座**（A 级）+ Q2.4 7 hygiene/determinism fixes + Q5.3 统一 shutdown
- ✅ **Native-Agent 底座**（A- 级）+ Q2.9 / Q3.12 三条契约违反闭口
- ✅ **Harness Agent Mode**（A- 级）+ Q1.7 seq + redaction 修复后 Beta 冻结承诺重新可信
- ✅ **LLM / VLM 能力层** 9 provider + 统一 `.thinking()` API + Q1.8/Q2.5/Q3.6
- ✅ **Tools 能力层** ⭐⭐⭐⭐ 升级 + Q1.1/Q1.2 关闭 6 个深审 CRITICAL（ShellTool 元字符 / seccomp openat / SBPL / SandboxPolicy / HttpTool panic）
- ✅ **RAG 能力层** + Q3.9 6 个 MAJOR + P10.6 Dense/Hybrid retriever + `--chunk-size` 维度
- ✅ **MCP 能力层** + Q2.6/Q3.2 + server 从 experimental → beta
- ✅ **Skill 能力层** + Q1.10 真 Ed25519 + P10.4.1 per-tool sandbox + P10.9.1 MCP discovery default-on
- ✅ **subAgent / 多智能体** + Q3.12 cancellation 契约 + Blackboard poison tolerance
- ✅ **Memory** A- 升级 + Q2.1 SQLite WAL + Q2.10 数据完整性 + P10.7.2 age-based encryption-at-rest
- ✅ **Rust SDK** + Q5.1 production-clean unwrap/expect + CI deny-lint
- ✅ **CLI** + Q3.5 + 5 个新子命令（marketplace / memory prune / agent replay / harness replay / backup）
- ✅ **Web UI** B+ 升级（ErrorBoundary + zod + 组件分层 + Playwright e2e）
- ✅ **平台化（server/db/worker）** + Q1.4/Q1.5/Q1.6 + Prometheus 14 series + read-replica routing + JWT admission

**唯一非代码 gating 项**：v1.0.0-rc.1 的 GHCR push / GitHub Release artefact 在切 tag 后还需推到远端触发 `release.yml`，是人工 ops 一步操作。

**下一个评估窗口建议**：v1.0.0-rc.1 远程推送 + RC 反馈 2-3 周后，或 v1.0 GA 前。届时关注：

1. RC release 反馈：fresh-host onboarding 摩擦（`AGENTFLOW_API_TOKEN` + provider keys + Postgres 初次部署）
2. 9 provider nightly 长期运行：vendor-side model 弃用频次（决定 `xtask refresh-live-models` 是否要进 CI 触发自动 PR）
3. Harness Mode 长会话稳定性（H6 advanced compatibility 是否有 promote-worthy 项浮现）
4. Web UI 在运营场景的进一步反馈（debugger-only RFC 是否需要升级 product 定位）
5. Toolchain housekeeping（rustc 1.96 新 lint sweep）是否进 CI 红线
6. 深审 cadence：建议在每个 release window 重跑一次 16-crate audit，本期已证明其高价值

> 评估签名：HEAD `76a88140c9432b842236f0e24dbeff0cda55063b`（2026-05-26；本地 `main` 工作树轻微 dirty：仅 `Cargo.lock` 与 `.playwright-mcp/` 临时目录，无源代码 unstaged）
>
> 评估日期：2026-06-06
>
> 主要参考：
>
> - **代码**：
>   - `agentflow-core/src/{flow,scheduler,value,expression,shutdown,robustness,plugin/*}.rs`
>   - `agentflow-agents/src/{runtime,react/agent,plan_execute,reflection,supervisor/{handoff,blackboard,debate},eval/*}.rs`
>   - `agentflow-harness/src/{lib,runtime,events,tasks,hooks_runtime,approval_providers,tracing_bridge,execution_trace_sink}.rs`
>   - `agentflow-tools/src/{tool,policy,sandbox/{macos,linux,noop},builtin/{shell,http,file}}.rs`
>   - `agentflow-llm/src/{tool_calling,thinking,modality_dispatch,providers/{openai,anthropic,google,moonshot,stepfun,openai_asr,mod}}.rs`
>   - `agentflow-server/src/{lib,auth,runs,skills,events_stream,ui,cleanup,tenant,harness*,scheduler/{distributed,grpc},metrics}.rs`
>   - `agentflow-db/src/{database,repo}.rs` + `migrations/000{1..6}_*.sql`
>   - `agentflow-worker/src/{lib,protocol,runtime,admission}.rs`
>   - `agentflow-ui/src/{main,pages/*,lib/*,components/*,schemas,eventFilter,preferences,usePreferenceSync}.{ts,tsx}`
>   - `agentflow-rag/src/{eval/{metrics,runner,baseline,dense,hybrid},chunking/recursive,loaders/{pdf,html}}.rs`
>   - `agentflow-memory/src/{layer,age_encrypted_preference_store,sqlite/*}.rs`
> - **测试**：
>   - `agentflow-llm/tests/{provider_consistency,provider_consistency_live,thinking_*}.rs`
>   - `agentflow-server/tests/{harness_routes,harness_approval_routes,harness_live_executor,harness_full_stack_e2e,tenant_boundary_*,e2e_runs}.rs`
>   - `agentflow-tools/tests/{sandbox_macos,sandbox_linux,shell_interpretation}.rs`
>   - `agentflow-worker/tests/{resource_limits,failure_domains,gRPC_auth}.rs`
> - **文档**：
>   - `docs/audit/{README,agentflow-*}.md`（16 个深度审计报告）
>   - `docs/{HARNESS_MODE,LLM_PROVIDERS_MATRIX,CLI_JSON_OUTPUT,MEMORY_LAYERING,STABILITY,CURRENT_STATUS,API_COMPATIBILITY,RAG_EVAL,TOOL_PERMISSIONS,RELEASE_NOTES_v1.0.0-rc.1,ROADMAP_v2,H6_PROMOTION_CRITERIA,LLM_PROVIDER_MODULE_PROMOTION}.md`
>   - `RoadMap.md` / `TODOs.md` / `CLAUDE.md` / `CHANGELOG.md` / `AGENTS.md`
> - **CI**：
>   - `.github/workflows/{quality,bench,llm-live,release}.yml`
>   - `xtask/src/main.rs` 8 个子命令
> - **历史评估**：
>   - `docs/archive/PROJECT_EVALUATION_2026-05-01.md`
>   - `docs/archive/PROJECT_EVALUATION_2026-05-14.md`
>   - `docs/archive/PROJECT_EVALUATION_2026-05-19.md`
> - **归档 TODO**：
>   - `docs/archive/TODOs-archive-2026-05-09-n1-n10.md`
>   - `docs/archive/TODOs-archive-2026-05-10-p0-p4.md`
>   - `docs/archive/TODOs-archive-2026-05-19-recently-closed.md`
>   - `docs/archive/TODOs-archive-2026-05-20-closed-segments.md`
>   - `docs/archive/TODOs-archive-2026-05-24-p10-optimization-backlog.md`
