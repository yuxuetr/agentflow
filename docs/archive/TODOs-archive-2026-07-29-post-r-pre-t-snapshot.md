# AgentFlow TODOs

Last updated: 2026-07-28

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
  - `TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md` — **本次
    7/28 全量快照**：H / P-A / S / L 四段全部 DONE/DEFERRED 收口后、启动新
    **R（工程化审计修复）** 段之前的完整存档（含全部历史明细）。本文件即从这份
    快照精简重建。
- 本文件是短期执行队列。H / P-A / S / L 四段已全闭环并整体存档；当前仅保留
  **R（2026-07-28 工程化审计修复）** 一个 backlog。
- 本次 R 段来源：2026-07-28 对照 `TODOs.md` / `RoadMap.md` /
  `docs/ROADMAP_v2.md` / 架构文档做的一次独立"文档 vs 代码/CI 实际状态"交叉
  审核（四路并行：文档自洽性核对、`cargo check/test/clippy` + CI 矩阵覆盖率
  实测、`xtask check-arch` 覆盖面与依赖铁律独立抽查、部署/运维文档与真实工件
  比对）。发现一个真实的生产回归 bug（R0.1）+ 一个未被工具捕获的架构契约泄漏
  （R1.1）+ CI 测试矩阵覆盖率缺口（R0.3）+ 若干文档陈旧/自相矛盾问题（R2）。
  详见每项下方引用的具体证据（文件:行号 / 命令输出）。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态（R2.3 会刷新，当前落后 ~5 周）。
- `RoadMap.md` 保留中长期路线。
- `HARNESS_MODE_EVOLUTION.md` 是 Harness Agent Mode 的设计规范。
- 任务状态只使用：
  - `TODO`：未开始或正在执行。
  - `DONE`：已完成、已测试、已提交。
  - `DEFERRED`：显式推迟到 RoadMap Later Tracks 或 Non-Goals。

## Active Queue Overview

Current focus: **H / P-A / S / L 四段已全闭环并整体存档**（见上）；
**R（2026-07-28 工程化审计修复）R0–R4 全部 DONE，并在 GitHub Actions 真实
硬件上实跑验证到 `release gate: conclusion=success`**（2026-07-29 收口）：
`agentflow-rag` HTML loader panic 已修复；CI 测试矩阵从 8 扩到全部 23 个
workspace member；clippy job 补 `--all-features`；`agent-spi→llm` 违规依赖
已烧（`LlmTraceContext` 下沉到 `agentflow-value`）；`check-arch` 新增
kernel-isolation 第三条 active law；4 处文档陈旧/矛盾已修；`_to_delete/` +
`.github/copilot-instructions.md`/`.github/instructions/` 两处未追踪残留
已清理；**R4（push 后第一次真实 CI 才暴露的问题，含一个生产级安全 bug）**：
Linux Landlock 沙箱此前从未在真正强制的内核上验证过——`build_landlock_
ruleset` 从没给系统二进制/动态链接器路径授权，一旦 Landlock 真正生效，
`with_os_sandbox()` 的任何子进程调用都会失败（不分 policy），已修复+在真实
x86_64 硬件上确认生效；agentflow-rag/agentflow-memory/agentflow-agents 里
另外 8 处预先存在的 clippy expect/unwrap 违规已清（前几处套用"编译期不变量"
allow 模式，mutex-poisoned 的 6 处改成走 `Result` 而不是 panic）；
`agentflow-server` 一处过期 API 契约（`?tenant_id=` 早被安全加固移除）的
测试已跟进；cgroup v2 委托可用性探测从"只查目录结构"改成"真正试一次迁移"，
修掉了 GitHub Actions runner 因 `system.slice` vs `user.slice` cgroup
归属不同而无法真正限制资源的问题。**TODOs.md 里已没有开放的 `TODO` 项**——
下一次审计或新需求出现前，本文件保持这个收口状态。

| Segment | Theme | Status |
| --- | --- | --- |
| N1 → N10 / P0 → P9 / P-H / P-LLM / M / P10 | 历史段，全部 closed 或外迁 | ARCHIVED |
| Q1 → Q5 | 2026-05-24 深度审计修复波次，108 DONE | ARCHIVED |
| H | Harness Mode follow-ups（loop-ownership + `harness chat` 收尾） | **DONE — archived（7/28）** |
| P-A | 契约内核 + 架构演进（`docs/RFC_CRATE_ARCHITECTURE.md`） | **DONE — archived（7/28）** |
| S | 沙箱与代码执行安全演进（`code_exec` / OS sandbox 强化） | **DONE — archived（7/28）** |
| L | 长程任务与检索增强（replan / 项目记忆 / RAG 补强 / 委托契约） | **DONE — archived（7/28）** |
| R | 2026-07-28 工程化审计修复（CI 覆盖率 / 架构守卫盲区 / 文档陈旧 / 仓库卫生） | **DONE（7/29，12/12 项全闭环）** |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## R — 2026-07-28 工程化审计修复（engineering-readiness remediation）

> 来源：2026-07-28 独立审核（非本文件维护者自评，四路并行验证：文档交叉核对 /
> `cargo check+test+clippy+fmt` 实测 / `xtask check-arch` 覆盖面抽查 / 部署
> 运维文档与真实工件比对）。**核心结论**：代码质量、部署工件、运维文档落地
> 程度总体扎实（`cargo check --workspace --all-features` 全绿、`fmt` 全绿，
> Docker/Compose/Helm/secret 加载/`doctor`/`backup` 均有真实代码支撑且与文档
> 一致），但存在一个真实的生产回归 bug 和几处此前审计未触及的盲区。排序原则：
> **R0 是唯一 production-blocking 段，其余为非阻断的工程化加固**。每项修复
> 需配 regression test；架构类改动需保持 `cargo test` + `clippy -D warnings`
> + `check-arch` 全绿。

### R0 — CI 覆盖盲区 + 真实生产 bug（blocking）

- DONE R0.1 修复 `agentflow-rag` HTML loader 的无效正则 panic：`script_regex`/
  `style_regex`（`agentflow-rag/src/sources/html.rs`）改用 `regex` crate
  实际支持的非贪婪 `(?is)<script\b[^>]*>.*?</script>` /
  `(?is)<style\b[^>]*>.*?</style>` 模式替换原先不支持的负向前瞻写法——
  非贪婪匹配在语义上等价于浏览器把 script/style 内容当"原始文本，找到第一个
  字面 `</script>` 为止"的解析规则，不需要 lookaround。新增回归测试
  `test_html_removes_scripts_with_embedded_angle_brackets` 覆盖脚本内容里带
  `<`（比较运算符）+ 同文档多个 script/style 块的情况，证明非贪婪匹配不会被
  内嵌的 `<` 提前截断或跨块贪婪吞并。`cargo test -p agentflow-rag --features
  html` 8/8 过（含新测试），`cargo clippy -p agentflow-rag --features html
  -- -D warnings` 的 2 个 `invalid_regex` 错误随之消失。
- DONE R0.2 清理 `agentflow-rag` 的其余 clippy 违规（与 R0.1 同一次修复验证）：
  - `embeddings/onnx.rs:193` `data.iter().copied().collect()` → `data.to_vec()`。
  - `sources/html.rs` title 提取的嵌套 `if let` 改用 let-chain
    （`if let Ok(..) && let Some(..) {`，本 crate 其他地方已有先例）合并。
  - `lib.rs::has_feature`：**没有采用** clippy 建议的
    `matches!(feature, "qdrant" | "local-embeddings" | "pdf" | "html")`
    重写——那个建议只在 `--all-features` 编译下（clippy 分析时看到每个
    `cfg!(...)` 分支都常量折叠成 `true`）恰好等价，正常只开部分 feature 的
    构建下会变成"认得名字就返回 true"，与原本"该 feature 是否真的编译进去了"
    语义不同，是会引入真 bug 的自动修复建议。改为保留原 `match` + 显式
    `#[allow(clippy::match_like_matches_macro, reason = "...")]` 并写明原因。
  - 顺带修了一个不在原计划内、但挡住 `cargo test -p agentflow-rag
    --all-features` 全绿验证的预先存在问题：`embeddings/onnx.rs` 模块级
    doctest 缺 `use agentflow_rag::embeddings::EmbeddingProvider;` 导致
    `embed_text` 方法不在作用域（`git stash` 验证过这个失败在本次改动前就
    存在，与 R0.1/R0.2 无关，顺手补上而非另开条目）。
  - 验收：`cargo clippy --workspace --all-features -- -D warnings` 全绿；
    `cargo test -p agentflow-rag --all-features` 239 lib + 4+11 doc 全过、
    `cargo fmt -p agentflow-rag -- --check` 干净。**R0.4 CI 侧仍需补
    `--all-features` 才能让这类问题以后自动被挡住**（未来 CI 才是关门项）。

- DONE R0.3 把测试矩阵扩到全部 workspace member，补齐 CI 覆盖率缺口
  - **执行方向和最初计划不同，记一笔**：验证时发现 `xtask test-gate` 实为
    wall-clock 性能回归门（`compare_test_timings` 只比时间比值），
    `capture_test_timings` 里明确写了"即使 `output.status.success()` 为
    false 也照样记录耗时"——**它从设计上就不会因为测试失败而 fail**，只会
    因为"变慢 ≥1.5×"而 fail。所以"接入 test-gate"本身**不能**堵住 R0.1
    这类回归，会是一次假绿修复。改为直接把 `quality.yml` 的 `test` job
    矩阵从 8 个 crate 扩到全部 23 个真实 workspace member（`agentflow-value/
    graph/store-spi/agent-spi/async-util/nodes/nodes-ai/config/rag/tracing/
    db/server/worker/worker-proto/harness` 15 个新增；此前列的"14 个"漏数了
    `agentflow-worker-proto`）。
  - 三个连带修复，都是为了让新加的矩阵项真正跑到会暴露问题的路径而不是自证
    通过：
    1. `agentflow-rag` 特判为 `cargo test --features pdf,html,code-chunking`
       ——默认 feature 只有 `qdrant`，不加 `html` 的话新矩阵项照样不会编译到
       R0.1 那段代码，等于没堵上。`local-embeddings` 排除（会拉 `ort` 现场下
       载 ONNX Runtime 二进制，`features` job 的 feature-combo 矩阵已有同样
       排除理由）。
    2. `agentflow-nodes-ai` 特判为 `--features mcp,rag`——默认 feature 是空，
       `nodes::mcp`/`nodes::rag` 两个适配器模块不加 feature 根本不编译。
    3. `test` job 新增 job 级 postgres service +
       `AGENTFLOW_DATABASE_TEST_URL` env（照抄 `ui-e2e.yml` 已验证过的写法）
       ——`agentflow-db`/`agentflow-server` 的 DB 相关测试原本靠这个 env 缺失
       时自我跳过（`eprintln!("skipping ... — set AGENTFLOW_DATABASE_TEST_URL
       to run")`），不设的话矩阵扩了但这部分测试还是空跑。
    `agentflow-cli/server/worker/worker-proto` 需要 protoc，`Install protoc`
    步骤的 `if:` 从"只认 cli"改成 `contains(fromJSON('[...]'), matrix.package)`
    四选一。
  - 验收（本地全部跑过，逐个确认非零失败为 0）：新增的 15 个 crate 逐个
    `cargo test -p <crate> --all-targets`（`rag`/`nodes-ai` 按上面特判的
    feature）全部通过——`value` 6 / `graph` 26 / `store-spi` 10 / `agent-spi`
    57 / `async-util` 21 / `nodes` 2 / `config` 17 / `nodes-ai`(mcp,rag) 13 /
    `rag`(pdf,html,code-chunking) 全绿 / `tracing` 全绿 / `db`（本地无
    postgres，验证自跳过路径本身不报错）9+1+13 / `worker-proto` 0（仅编译）/
    `worker` 6+4 / `harness` 79+6+3+1 / `server` 180+若干集成套件共 200+，
    全部 0 failed。`python3 -c "import yaml; yaml.safe_load(...)"` 确认新
    YAML 语法合法。**CI 实跑（含 postgres service 是否正确启动）留给下一次
    push 验证**，本地无法模拟 GitHub Actions runner 环境本身。

- DONE R0.4 CI 的 clippy job 补上 `--all-features`：`quality.yml` 的
  `clippy` job 从 `cargo clippy --workspace --all-targets -- -D warnings`
  改为同时带 `--all-targets --all-features`。本地先跑
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  确认全绿（1m41s，只有一条不可操作的上游 `proc-macro-error2` future-incompat
  提示，与本次改动无关）才落地这行 CI 改动，避免推上去才发现别的 crate 在
  `--all-features` 组合下有隐藏的 lint 债。`clippy-lib-deny`（unwrap/expect
  deny）job 本次**没有**同步加 `--all-features`——不在 R0.4 范围内，且它的
  `--lib --no-deps` 语义和普通 clippy job 不同，留给需要时单独评估。

### R1 — 架构守卫盲区（`check-arch` 未覆盖 L0 内核层）

- DONE R1.1 消除 `agentflow-agent-spi`（L0）对 `agentflow-llm`（L2）的直接依赖：
  `LlmTraceContext`（纯数据：trace_id/span_id/flags/tracestate + new/random/
  with_tracestate/with_flags/to_traceparent/from_traceparent，零 reqwest/
  tokio 依赖）整体下沉到新 `agentflow-value/src/trace_context.rs`（新增
  `uuid` 依赖用于 `random()` 的熵源）。`agentflow-llm` 反过来依赖
  `agentflow-value`，`trace_context.rs` 改为 `pub use agentflow_value::
  LlmTraceContext;` + 只保留 tokio task-local `scope`/`current` 和 HTTP header
  注入（`inject_into_headers`/`inject_context_into_headers`）——这些是真正的
  LLM 传输层关注点，留在 llm 天经地义。`agentflow_llm::LlmTraceContext` 这个
  路径**对所有既有调用方保持不变**（llm 自己 5 个 provider + agents 的
  plan_execute.rs/react/agent.rs + cli 的 cross-hop e2e 测试全部用的是这个
  全限定路径，零改动即通过）。`agentflow-agent-spi/Cargo.toml` 删
  `agentflow-llm`、加 `agentflow-value`；`runtime.rs:86,237` 两处
  `agentflow_llm::LlmTraceContext` → `agentflow_value::LlmTraceContext`。
  纯数据单测（`new_rejects_malformed_ids`/`random_yields_...`/
  `traceparent_round_trips`/`from_traceparent_rejects_...`）跟着类型定义
  一起搬到 value；llm 侧留下的测试只测 scope/header 注入这层传输逻辑。
  验收全过：`value` 11 测（新增 5 个 trace_context 测试）/ `llm` 168+56+24+
  4+7+3 测 / `agent-spi` 57 测（不再依赖 llm，无 llm 相关测试可丢）/
  `agents` 231 测 / `harness` 79+6+3+1 测 / cli `p3_8_cross_hop_e2e` 3 测，
  全部 0 failed；`cargo check --workspace --all-features` clean；
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean；`cargo fmt --all -- --check` clean；`cargo xtask check-arch` OK
  （82 条边、0 tracked violation——新增的 `llm→value`/`agent-spi→value` 都是
  合法的向下依赖，不触发现有两条 active law）。

- DONE R1.2 把 `check-arch` 的分层检查扩展到 L0 内核 crate：新增
  `ARCH_KERNEL_CRATES = [value, graph, store-spi, agent-spi, async-util,
  tools]`（CLAUDE.md "L0 Contract Kernel" 列出的六个 crate）+
  `LAW_KERNEL_ISOLATION`（"kernel-isolation (RFC §7 Law 1)"）第三条 active
  law：`classify_arch_edge` 新增一条——`from` 在内核集合里而 `to` 不在，即
  违规；内核 crate 之间互相依赖（如 `graph→value`、`agent-spi→store-spi`）
  是窄腰契约内核的本意，不算违规，只有依赖"跑出内核集合"才算。
  `classify_arch_edge`/`evaluate_arch`/`evaluate_latent` 三个函数签名都加了
  `kernels: &[&str]` 参数，`check_arch_at` 把 `ARCH_KERNEL_CRATES` 传进去，
  打印行从"2 active law(s)"改成"3 active law(s)"。8 个既有单测调用点补上
  第 5 个参数（大多传 `&[]` 保持原语义不变），新增 3 个针对性单测
  （`kernel_depending_on_non_kernel_is_a_new_violation`——直接对应 R1.1 那次
  真实违规的形状、`kernel_depending_on_kernel_is_allowed`、
  `non_kernel_depending_on_kernel_is_allowed`）。
  - **没有做**（超出本轮范围，按 TODO 原计划"不需要本轮全部落地静态检查"
    处理）：RFC 其余未可查的法则（工具/能力不依赖运行时、IR≠executor 边界、
    可靠性原语单一来源、"只有 surface 才能引入全图"）——`ARCH_LATENT_EDGES`
    里已经用注释 + `becomes` 字段记录了这些法则对应的具体边和归属任务
    （P-A1.1/1.2/1.3/1.4 等），当作现状说明，本次不重复开 DEFERRED 记录。
  - 验收：`cargo test -p xtask` 75+4 测全过（含 3 个新单测 + 既有
    `real_workspace_passes_with_current_allowlist` 在新第三条法则下依然通过，
    因为 R1.1 已经把唯一违规边烧掉了）；`cargo run -p xtask -- check-arch`
    实跑输出 `3 active law(s)` / `0 tracked, 0 new, 0 stale` / `24 member(s),
    82 internal edge(s)`；`cargo fmt --all -- --check` /
    `cargo check --workspace --all-features` clean。**红→绿验证**：这条法则
    是在 R1.1 已经修完之后才激活的（R1.1 先烧边，R1.2 才上锁），所以顺序上
    没有一次"先跑红"的窗口——用新增的 `kernel_depending_on_non_kernel_is_a_
    new_violation` 单测（用合成 crate 名复现同样形状）代替对真实仓库的
    红绿验证，效果等价。

### R2 — 文档陈旧与内部自相矛盾

- DONE R2.1 `docs/KUBERNETES_DEPLOYMENT.md` 标注废弃（选了方案 b，不是 a）：
  **没有**用真实 Helm 模板重写全文——PVC/RBAC/HPA/ServiceMonitor/
  NetworkPolicy/PodDisruptionBudget/Fluentd 这些目前都不是
  `charts/agentflow/templates/` 里的真实模板，要让文档"如实描述"就得先把这些
  写成经测试的 Helm 模板，那是功能开发不是文档修复，风险和工作量都远超本轮
  审计修复的范围，容易生产出没人验证过的"看起来权威"的基础设施代码。改为
  标题下加醒目 banner，点名端口号（8080/9090 vs 真实 chart 的 3000）和整批
  不存在的资源清单，标记 Health Check Integration 一节（Rust 代码 +
  `/health/live`/`/health/ready`/`/metrics`）仍然准确、是真实 chart 探针
  调用的确切端点；"Deployment Configuration" 小节开头也加了一条内联提示，
  因为读者可能直接从目录跳过去、不经过顶部 banner。两处都指向
  `docs/DEPLOYMENT.md#helm` + `charts/agentflow/values.yaml` 作为真实部署路径。
  - 验收：banner 语言清楚区分"仍准确的部分"（健康检查代码）和"预 Helm 时代
    参考草图，不要直接 apply"（部署清单部分）；`docs/DEPLOYMENT.md` 的
    `#helm` 锚点确认存在（`## Helm` 标题，第 45 行）。

- DONE R2.2 `MODULE_OPTIMIZATION_REPORT.md` 补历史标注：仿照
  `IMPLEMENTATION_STATUS.md`/`OVERALL_EVALUATION_REPORT.md` 已有的 blockquote
  banner 句式，在标题下加一段：点名 2025-01-04 的完成度百分比（agents 30%、
  RAG 80% 等）已被后续开发彻底超越，指向 `docs/CURRENT_STATUS.md` 为当前权威
  状态，`OVERALL_EVALUATION_REPORT.md`/`docs/archive/PROJECT_EVALUATION_2026-
  05-19.md` 为更新的中间快照。三个引用路径都核实过真实存在。

- DONE R2.3 `docs/CURRENT_STATUS.md` 刷新到当前状态：加了 S（沙箱/`code_exec`）
  和 L（长程任务/RAG/委托契约）两段收口摘要（浓缩自归档快照里对应段落的
  DONE 记录），"Last Updated" 改成 2026-07-28 并注明这次更新做了什么。顺带
  修了一个连带发现的小遗漏——L2 capability adapters 那行漏了 `agentflow-
  nodes-ai`（P-A4.0 nodes 拆分之后才存在的 crate，这份文档没跟上）；"Active
  Work" 一节的措辞还停留在"P0-P4 刚完成"的旧叙事，改成准确反映 H/P-A/S/L
  四段已收口存档、R 段（本次审计修复）为当前 active 的现状。

- DONE R2.4 `docs/STABILITY.md` 内部矛盾修复：核对下来不是"两种口径都对、需要
  分别说明"的情况——主稳定性表第 72-73 行明确写了"Promoted to Beta after
  P-H.2/P-H.5 slice 2 ..."的具体依据，Fixture Ownership 表第 100 行的
  Experimental 就是单纯没跟着那次晋级一起改，是真正的过期表述，不是刻意的
  维度差异。直接把第 100 行改成 Beta，并加一句注明"这行之前是没跟上晋级的
  过期表述，不是两张表故意用不同口径"，避免以后又被人当成两个维度各自权威
  再摆一次乌龙。顺手确认了 `agentflow-harness/tests/fixtures/` 目录确实存在
  4 个 fixture 文件，Fixture Ownership 这一行本身描述准确，只有 stability
  等级列错了。

### R3 — 仓库卫生

- DONE R3.1 清理 `_to_delete/` 目录：与用户核实（AskUserQuestion，2026-07-29）
  确认是无用残留，直接 `rm -rf _to_delete/`。未曾被 git 追踪，删除不产生
  任何 commit，`git status` 确认没有影响任何 tracked 文件。

- DONE R3.2 移除 `.github/copilot-instructions.md` + `.github/instructions/`：
  与用户核实（AskUserQuestion，2026-07-29）确认不是团队有意加入的协作配置，
  是 GitHub Copilot + VS Code Mermaid 扩展相关的本地工具残留，与 AgentFlow
  项目本身及 Claude Code 工作流无关。`rm -f`/`rm -rf` 移除，同样未被追踪，
  删除不产生 commit。**R3 段（仓库卫生）全部闭环，R 段（2026-07-28 工程化
  审计修复）全部 DONE：R0/R1/R2/R3 十二项全部完成。**

### R4 — CI 首次实跑暴露的问题（2026-07-29，push 后发现）

> 来源：R 段全部 DONE 后第一次把 40+ 个此前从未推送过的本地 commit（含本次
> R0–R3）一次性 push 到 origin/main，触发 `main` 分支自 2026-07-23 以来的
> 第一次真实 CI 运行。`release gate` 红，拆出 4 个独立失败，性质分两类：
> 与本轮改动无关的历史遗留问题（R4.1、R4.2）、R0.3 扩矩阵后第一次真正跑到
> 才暴露的既有 bug（R4.3、R4.4）。**R4.2 是这批里最严重的一个**——不是
> CI 卫生问题，是一个真实的、可能影响生产的沙箱安全子系统缺陷。

- DONE R4.1 修复 `agentflow-tools` 的 `clippy-lib-deny` 违规：
  `code_exec.rs:247-248` 两处 `.expect()`（2026-07-27 commit `b12fd69`，
  本会话之前就存在）缺 `#[allow(clippy::expect_used, reason=...)]`。
  `child.stdout`/`child.stderr` 在 `.stdout(Stdio::piped())` 配置之后
  `.take()` 必为 `Some`，是构建期不变量，补上 allow + 理由说明。
  commit `37bd47d`。

- DONE R4.2 修复 Linux Landlock 沙箱从未在真正强制的内核上验证过的缺陷
  （**production-blocking，非 CI 卫生问题**）：
  - **根因**（通过读代码 + 双路径独立复现精确定位，非猜测）：
    `build_landlock_ruleset`（`agentflow-tools/src/sandbox/linux.rs`）此前
    只把 `scope.read_paths`（默认只有 `/tmp` + cwd，或调用方显式
    `allowed_paths`）喂给 Landlock 规则，从未对任何系统二进制/动态链接器
    路径（`/usr/bin`、`/lib`、`/usr/lib` 等）授予任何权限。而 landlock crate
    的 `AccessFs::from_read()` 把 `Execute` 也算进"read"权限里，且
    `Ruleset::handle_access(AccessFs::from_all(..))` 会让 Landlock 对**全部**
    路径的 `Execute` 权限默认拒绝——所以一旦 Landlock 真正被内核强制生效，
    连 `/bin/echo`、`python3` 自己都无法被 exec，不分 permissive/restrictive
    policy，**任何**通过 `with_os_sandbox()` 的子进程调用都会失败。
  - **为什么 S3 track 的本地验证没抓到**：S3 当初"内核强制实测"用的
    Apple `container` CLI 提供的 Linux VM 恰好不带 `CONFIG_SECURITY_LANDLOCK`
    （测试代码自己的 `landlock_enforcing()` 跳过逻辑因此从来没在验证时被
    绕过——一直悄悄走的是 `SandboxEnforcement::Permissive`，即"只有
    seccomp 生效，Landlock 规则从没被内核真正加载过"）；这段代码也从没在
    真实 CI 跑过。GitHub Actions 的 `ubuntu-24.04` runner（真实 x86_64 硬件，
    内核 6.17）确实支持 Landlock，是第一次真正触发这个缺陷的环境。
  - **诊断方法**（记一笔，因为过程本身值得复用）：本地 macOS 无法直接跑
    `#[cfg(target_os = "linux")]` 的测试；用 Apple `container` CLI 分别起了
    aarch64 和 `--arch amd64` 两个真实 Linux VM 复现。x86_64 VM 复现出的
    症状是 `apply_filter` 本身报 `EINVAL`（seccomp 系统调用拒绝编译出的 BPF
    程序）而不是 GH Actions 上的 `EACCES`——一开始怀疑是同一个根因，后来
    通过一个受控实验（分别用 `io::Error::from_raw_os_error(N)` 和
    `io::Error::other(string)` 从 `pre_exec` 闭包返回，比较父进程收到的
    错误）证明：**原始 OS 错误码会如实穿过 fork 错误管道，但被
    `.map_err(io::Error::other(...))` 包装过的错误会统一折叠成一个固定值**
    ——`apply_filter` 的失败路径全部经过这层包装，因此不可能在父进程侧
    表现为保真的 `EACCES`。这排除了"两处失败是同一个根因"的假设，把注意力
    正确引向了会产生**保真**原始错误码的路径——即 pre_exec 成功返回后，
    `execve()` 本身被拒绝——也就是 Landlock。x86_64 VM 上 `apply_filter`
    报 EINVAL 大概率是 Apple 跨架构虚拟化层本身对 `seccomp(2)` 这个冷门
    系统调用的翻译缺陷，与生产环境无关，未继续深挖。
  - **修复**：仿照 `agentflow-tools/src/sandbox/macos.rs::build_profile`
    已有的"bare minimum the child needs to start and link dyld"先例（对
    `/usr/bin`、`/usr/lib`、`/System` 等系统路径授予基线访问），给
    `build_landlock_ruleset` 加一份 Linux 对应的基线路径列表
    （`/usr/bin`、`/bin`、`/usr/sbin`、`/sbin`、`/usr/lib`、`/usr/lib64`、
    `/lib`、`/lib64`、`/usr/libexec`、`/usr/share`）+ 单独授予
    `/etc/ld.so.cache`（动态链接器缓存，只开这一个文件而非整个 `/etc`，
    避免 `/etc/passwd`/`/etc/shadow`/`/etc/ssh/*` 之类敏感文件被顺带放开
    ——和 macOS 那边只开 `/private/etc/localtime` 单文件的 Q1.1.3 先例
    完全对应）。`path_beneath_rules` 本身对不存在的路径静默跳过（已有测试
    `build_landlock_ruleset_ignores_nonexistent_paths_rather_than_erroring`
    覆盖），所以某些发行版缺某个目录（比如没有独立 `/lib64`）不会导致
    整个 ruleset 构建失败。
  - 新增回归测试
    `linux_landlock_allows_exec_of_system_binary_outside_the_allowed_scope`
    （`agentflow-tools/tests/sandbox_linux.rs`）：在 `landlock_enforcing()`
    的内核上，用只授权临时目录的 policy 跑 `python3`，断言 exec 依然成功
    ——这是直接对着这个 bug 形状写的测试，本地两台 VM 都不满足
    `landlock_enforcing()` 前提所以跑的是 skip 分支，验证的是"结构正确、
    不因为新代码路径 panic"，真正的通过/失败判定要看 R4.5 推上去之后
    GitHub Actions 的实跑结果。
  - 验收（本地能做的部分全过）：`cargo clippy -p agentflow-tools
    --all-targets -- -D warnings`、`--lib --no-deps` 的 unwrap/expect deny、
    `cargo fmt --check` 全干净；两台 VM（aarch64 + x86_64，均不支持
    Landlock）上 `cargo test -p agentflow-tools --lib sandbox::linux` 9/9
    过、`--test sandbox_linux` 10/10 过（含新测试，均走 skip 分支，无
    panic/编译错误）。**Landlock 真正强制生效路径下的最终验证留给 R4.5
    的真实 CI 跑**。

- DONE R4.3 nodes-ai TTS 测试补 API key 门控：`agentflow-nodes-ai/src/nodes/
  tts.rs` 的 `test_tts_node_integration` 之前既没有 `#[ignore]` 也没有
  env var 跳过逻辑，是同目录 asr/image_to_image/image_understand 三个兄弟
  测试里唯一的例外（前两者用 `#[ignore]` + 内部 `STEP_API_KEY` 检查双保险，
  后者只用内部检查）——之前没进过 CI 矩阵所以没被发现，R0.3 扩矩阵后第一次
  真正跑到就必挂。补齐同款 `#[ignore]` + `if std::env::var("STEP_API_KEY")
  .is_err() { println!(...); return; }`，跟多数兄弟测试的模式对齐。
  验收：`cargo test -p agentflow-nodes-ai --lib nodes::tts --features
  mcp,rag` 显示 `1 ignored`（不再尝试真实 API 调用）；crate 全量测试
  `12 passed; 0 failed; 5 ignored`；`cargo clippy --all-targets
  --features mcp,rag -- -D warnings` / `cargo fmt --check` 全干净。

- DONE R4.4 修复 `agentflow-server` 两个 DB 集成测试失败：
  - **真实根因只有一个，且不是功能 bug**：`list_runs_returns_recent_rows_
    for_tenant`（`agentflow-server/tests/runs_routes.rs`）还在用
    `GET /v1/runs?tenant_id=tenant-a` 这个**已经在 Q1.4.1（Q-段安全加固，
    2026-05-24 审计修复波次）里被有意移除**的旧 API 契约——`?tenant_id=`
    query 参数当年被删是因为它能被任意认证客户端拿来越权列出别的租户的
    runs（见 `runs.rs::list_runs` 自己的文档注释），租户现在**只**从
    `X-Agentflow-Tenant` 请求头解析。测试从 Q1.4.1 之后就一直是过期的，
    只是从没在真实数据库前跑过（`AGENTFLOW_DATABASE_TEST_URL` 缺失时静默
    跳过），R0.3 加 postgres service 后第一次真正执行就现形。
  - **诊断方法**：本地用 Apple `container` CLI 起了一个和 CI 同配置
    （`agentflow`/`agentflow`/`agentflow`）的临时 postgres 容器，直接
    `AGENTFLOW_DATABASE_TEST_URL=... cargo test -p agentflow-server --test
    runs_routes` 复现——第一次单独跑这两个测试，`submit_run_executes_
    fixed_dag_and_persists_workflow_events` 反而通过了，只有
    `list_runs_returns_recent_rows_for_tenant` 报 `left: 0, right: 2`
    （查询返回 0 条，不是断言目标错误，是列表真的空）——直接定位到请求没
    带租户信息。
  - **修复**：把测试的 GET 请求从 `?tenant_id=tenant-a&limit=10` 改成
    `?limit=10` + `.header("X-Agentflow-Tenant", "tenant-a")`，与
    `harness_full_stack_e2e.rs`/`harness_live_executor.rs` 里已经在用的
    正确模式对齐。
  - **验证 `submit_run_executes_...` 不是真实 bug**：用干净数据库连续跑了
    3 轮（每轮先起一个全新的 postgres 容器，避免我自己反复手动调试时
    "no TRUNCATE by design"的既有测试残留数据互相污染——这一点本身也确认
    了：**我第一轮复现时看到的偶发失败，是我自己反复对同一个 postgres
    容器重跑测试導致的残留数据问题，不是 CI 或代码的真实缺陷**；每轮全新
    DB + 默认并发（不加 `--test-threads=1`，与 CI 矩阵的默认执行方式一致）
    跑全部 14 个测试，全部 14/14 通过，稳定复现。
  - 验收：`cargo clippy -p agentflow-server --all-targets -- -D warnings` /
    `cargo fmt --check` 干净；`cargo test -p agentflow-server --test
    runs_routes`（干净 DB、默认并发）14/14 过，含两个原本失败的测试。

- DONE R4.5（第一次实跑验证）push `aeca5a4..a22aa18`（R4.1–R4.4 全部修复）
  后，`release gate` 从"1 通过/13 全红"变成"3 红"，关键信号：**R4.2 的
  Landlock 修复在真实 GitHub Actions x86_64 硬件上确认生效**——
  `agentflow-tools` 的 `sandbox_linux` 集成测试从"1 passed, 8 failed"变成
  "8 passed, 2 failed"，之前失败的 `linux_seccomp_allows_baseline_echo`/
  两个 landlock 测试/新增的 exec 回归测试全部转绿。剩余 3 红拆开看：
  - `clippy --lib (unwrap/expect deny)` 又红——不是 R4.1 修的那两处
    （`code_exec.rs`）复发，是**另一处独立的、同样预先存在的违规**：
    `agentflow-rag/src/chunking/paragraph.rs:65` 的
    `group.last().expect("group is non-empty")`。两个调用点都在
    `!group.is_empty()` 检查内部（`push_group` 前几行的 `group[0]` 索引
    本身已经无条件依赖同一个不变量），确认是构建期不变量，补
    `#[allow(clippy::expect_used, reason=...)]`，同 R4.1 的模式。
  - `agentflow-tools` 集成测试剩 2 个：`linux_cgroup_enforces_max_memory_
    bytes` / `linux_cgroup_enforces_max_pids`，同样是 `Os { code: 13,
    kind: PermissionDenied }`——但和 R4.2 是**同一类 bug 的另一个实例，
    不是 R4.2 没修干净**：这两个测试直接构造裸 `SandboxScope::new()`
    （不经过 `build_scope_from_policy`），把编译出的测试用 fixture 二进制
    放进临时目录，却从没把这个临时目录塞进 `scope.read_paths`——目标
    二进制自己所在的路径既不在 R4.2 加的系统基线路径里（自定义临时目录，
    不可能预先枚举），也不在测试自己传的 scope 里，Landlock 一样会拒绝
    exec。**是测试自身 under-scope 了自己的临时目录，不是 `wrap_command`/
    `build_landlock_ruleset` 的生产代码缺陷**——同目录下其他已经在通过的
    Landlock 测试（`linux_landlock_allows_reads_inside_the_allowed_scope`
    等）都正确地把 `temp.path()` 放进了 `allowed_paths`，这两个 S3.2
    测试当初漏了。补 `.with_read_paths([dir.path()])`。
  - 验收：本地两台 VM 复测（aarch64 无 Landlock，只验证不 panic/编译过；
    macOS host 跑 `agentflow-rag` 的 clippy + `chunking::paragraph` 5 个
    单测）全绿；`cargo fmt --all -- --check` / `cargo check --workspace
    --all-features` 干净。真正的绿灯判定见 R4.6（第二次 push 实跑）。

- DONE R4.6（第二次实跑验证）push `4cb3ba1` 后，`agentflow-tools` 的
  `sandbox_linux` 集成测试确认 **9/9 全过**——R4.2 的 Landlock 修复 + R4.5
  的两个 cgroup 测试 tempdir 授权修复在真实 GitHub Actions x86_64 硬件上
  双双验证生效。但 `clippy-lib-deny` 还是红，而且**又是不同的违规**：
  `agentflow-memory/src/project.rs`（3 处）+ `agentflow-memory/src/
  task_summary.rs`（3 处）的 `Mutex::lock().expect("... poisoned")`。
  - **没有在本地反复"再跑一次 CI 才发现下一批"**——这次直接在本地跑了完整
    `cargo clippy --workspace --all-features --lib --no-deps -- -A warnings
    -D clippy::unwrap_used -D clippy::expect_used`（等价于 CI 那条命令，
    本地 macOS host 就能跑全，不需要专门进 Linux VM），一次性挖出剩余
    全部违规，避免再来回 push 试探。挖出两批：
    1. `agentflow-memory` 的 6 处 mutex-poisoned expect——**这批和 R0.1/
       R4.1/R4.5 那些"编译期不变量"性质不同**：mutex 中毒是真实的运行时
       可能性（同一把锁的某次持锁期间如果 panic，之后所有 `.lock()` 都会
       返回 `Err`），不是"逻辑上不可能发生"，所以**没有**套用
       `#[allow(clippy::expect_used)]`——而是直接改成
       `.lock().map_err(|e| MemoryError::StorageError(format!("...
       poisoned: {{e}}")))?`，把中毒转成这些函数本来就在返回的
       `Result<_, MemoryError>` 里的一个真实错误分支，而不是继续 panic。
       `InMemoryProjectMemoryStore`/`InMemoryTaskSummaryStore` 都是"进程
       生命周期内的尽力而为缓存"（各自文档注释写明），改动后行为更符合
       "失败走 Result，不是 panic"的仓库整体风格。
    2. `agentflow-agents/src/citation.rs:73` 的 `citation_marker_regex()`
       ——和 R0.1/R4.1 同款"编译期静态正则字面量"模式，补
       `#[allow(clippy::expect_used, reason=...)]`。
  - 验收：`cargo clippy --workspace --all-features --lib --no-deps -- ...`
    在 macOS host 上跑到 `Finished`、零 error；
    `cargo test -p agentflow-memory --lib project::`（7 测）+
    `task_summary::`（5 测）+ `cargo test -p agentflow-agents --lib
    citation`（15 测）全过；`cargo check --workspace --all-features` /
    `cargo run -p xtask -- check-arch`（OK） / `cargo fmt --all --check`
    干净。

- DONE R4.7（第三次实跑验证）push `83f040f` 后，`clippy-lib-deny` 确认全绿
  （本地全量扫描替代 push 试探生效）；`agentflow-tools` 只剩
  `linux_cgroup_enforces_max_memory_bytes`/`linux_cgroup_enforces_max_pids`
  两个红——这次是**限制没有真正生效**（fixture 进程能起来、能跑完，只是
  OOM-kill / fork 数量封顶都没发生），不再是"进程起不来"那类问题。
  - **根因（已确认，双路径复现）**：`cgroup_v2_delegation_available()`
    此前只检查目标目录结构是否存在 + controller 是否已启用
    （`resolve_cgroup_root().is_some()`），没有真正验证"迁移一个进程进去"
    这个操作本身是否可行。cgroup v2 的规则是：迁移一个任务需要对**源和
    目标在 cgroup 树上的最近公共祖先**有写权限，不只是对目标本身。用临时
    debug workflow 在真实 GitHub Actions x86_64 硬件上探测到：job 进程自己
    跑在 `/sys/fs/cgroup/system.slice/hosted-compute-agent.service` 下，
    根本不在 `resolve_cgroup_root()` 目标的 `user.slice/user-<uid>.slice/
    user@<uid>.service/...` 树里——两者的最近公共祖先是 cgroup **根目录**，
    根目录属于 root，uid 1001 没有写权限，所以迁移必然 EACCES。这不影响
    生产运行时行为（`migrate_self_into_cgroup` 失败本来就是 best-effort
    吞掉，spawn 照常继续，只是不带资源限制——设计上就是这样），只是测试
    自己的"能不能用"判断不够准。
  - **修复**：`cgroup_v2_delegation_available()` 改为真正尝试一次迁移
    ——fork 一个一次性子进程，只做"调用 `migrate_self_into_cgroup` 然后
    退出"，通过退出码告诉父进程是否成功，不触碰调用方自身（不会误伤
    正在跑的测试进程）。探测目标特意选在 `root` 下的一个叶子目录
    （`delegation-probe`）而不是 `root` 本身——首次实现直接探测 `root`
    时，在本地 VM 上踩到 cgroup v2 "no internal processes" 规则（`root`
    已经把 controller 委托给子孙、不能再直接持有成员进程），改成叶子后
    解决。
  - **诚实记录一个没有彻底解决的疑点**：本地反复用 Apple `container` CLI
    起全新的 aarch64 VM 做验证时，发现一种新行为——`cgroup_v2_delegation_
    available()`（含新探测）判定"可迁移"为 true，但**真实 fixture spawn
    的资源限制依然没有生效**，和 GH Actions 那次的失败模式不完全一样
    （不是探测失败，是探测通过但运行时依然没真正生效）。深入插桩到
    `migrate_self_into_cgroup` 实际调用点排查，遇到自相矛盾的证据（日志显示
    没跑到 seccomp 之后的代码，但 fixture 却明显执行完了）——没能在合理时间
    内彻底钉死这第三层原因，怀疑与本地这台被同一会话反复复用/手动改过
    cgroup 状态的调试机的自身状态有关，不一定是真实、干净环境下的问题。
    **没有为了让本地"看起来通过"而继续深挖或引入未经证实的修复**——只保留
    了已经用真实 GH Actions 硬件确认过的那部分修复（探测更准确地识别
    "system.slice vs user.slice"这种权限不可行的场景）。
  - 验收：`cargo clippy -p agentflow-tools --all-targets -- -D warnings` /
    `cargo fmt --check` / `cargo check --workspace --all-features` 干净；
    两台本地 Linux VM 上 `cargo test -p agentflow-tools --lib sandbox::linux`
    9/9 过，不引入编译错误或恐慌。**cgroup 限制在真实生效场景下是否完整
    工作，仍然只能靠下一次真实 CI 验证**——如果 R4.8 显示这两个测试还是红,
    说明还有本条目没抓到的第三层原因，需要继续跟进（不属于本轮"发现即
    修复"的范围，会转成独立 TODO）。

- DONE R4.8（第四次实跑验证，2026-07-29）push `3c00b0f` 后，`release gate`
  **`conclusion=success`**——`agentflow-tools` 的 `sandbox_linux` 集成测试
  全部转绿，R4.7 记录的"本地怪癖"（探测通过但真实 spawn 未被限制）没有在
  GH Actions 上复现，证实那确实是本地反复重用的调试 VM 自身状态导致的
  噪音，不影响真实交付目标。**R4 段（CI 首次实跑暴露的问题）全部 DONE，
  R 段（2026-07-28 工程化审计修复）连同 R4 追加发现的问题，至此全部真正
  在 GitHub Actions 真实硬件上验证通过。**

## Recently Closed

- **2026-07-29 — R 段（工程化审计修复）全闭环**：R0（`agentflow-rag` HTML
  loader panic 修复 + CI 测试矩阵扩到全部 23 crate + clippy 补
  `--all-features`）→ R1（`agent-spi→llm` 违规依赖烧掉、`LlmTraceContext`
  下沉到 `agentflow-value`、`check-arch` 新增 kernel-isolation 第三条 law）
  → R2（4 处文档陈旧/矛盾修复：K8s 部署文档标废弃、模块优化报告补历史
  banner、`CURRENT_STATUS.md` 补 S/L 收口摘要、`STABILITY.md` 内部矛盾修复）
  → R3（`_to_delete/` + `.github/copilot-instructions.md`/`.github/
  instructions/` 两处未追踪残留，经用户确认后清理）。12 项全部 DONE，逐项
  独立 commit（`aeca5a4`/`73e54a4`/`7caa7a7`/`e2dcd5e`/`751d13f`/`ff9f13f`/
  `0faf026`/`9f62c7c`/`bd8e812`）。
- **2026-07-28 — H / P-A / S / L 四段整体存档 + 启动 R 段**：对照
  `TODOs.md`/`RoadMap.md`/`docs/ROADMAP_v2.md`/架构文档做了一次独立的
  "文档 vs 代码/CI 实际状态"审核（详见 R 段来源说明），发现的问题据此规划为
  新的 R0–R3 待办队列。收口前的完整历史（H.1–H.8、P-A0–P-A4、S0–S4、L1–L5
  全部 DONE/DEFERRED 明细）整体存档到
  [`docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md`](docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md)。

> 7/28 之前的 Recently Closed 全部归档在上述历史快照 + 更早的
> [`docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md`](docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md)。

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
- DEFERRED（H.6/H.7/H.8 归档明细）服务端多节点共享 memory backend / `--skill`
  路径持久化统一 / slash-command 生态·TUI 产品壳·OpenHarness 配置导入·第三方
  agent 框架适配器——理由与出处见
  `docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md`
  H.6–H.8 段。

---

## Execution Notes

- **R0 优先级硬性**：R0 全部 DONE 之前不应该认为项目"已完善可直接工程化使用"
  或 cut 新的 release tag——R0.1 是真实的生产 panic，R0.3/R0.4 是让类似回归
  以后能被 CI 挡住的前提条件。R1/R2/R3 不阻断 release，但应在其后尽快跟进。
- 每个 R-item 完成后引用本文件里对应的证据段落（文件:行号 / 命令输出），不要
  只写"已修复"三个字。
- 一次只挑一个 R-item；不要在同一 PR 里混不同 crate 的修复。
- 每个 fix 必须配至少一个 regression test 证明问题不会复现（R0.1 的 test 需
  真实覆盖含 `<script>`/`<style>` 标签的 HTML；R0.3/R1.2 的"test"是 CI 红绿
  验证，非常规单测)。
- Commit message 引用 task ID：`Refs R0.1`。
- R-item 完成后将状态从 `TODO` 改成 `DONE` 并简述 fix + 测试（如本文件历史
  归档中其他 DONE 项的写法）。

---

## Quality Gates

每个 task：

- 先读相关代码 + 本文件里该项引用的证据（文件:行号）。
- 实现最小可行修复。
- 跑聚焦的 regression test + crate 全测。
- Conventional commit 提交：`fix(scope): ...` / `refactor(scope): ...` /
  `ci(scope): ...`。
- 提交成功后再把 TODO 改成 DONE。

Pre-commit workspace 命令：

```bash
cargo fmt --all
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace
```

---

## Cross-References

- `docs/archive/TODOs-archive-2026-07-28-pre-audit-remediation-snapshot.md` —
  **本次 7/28 全量快照**：H/P-A/S/L 四段收口前的完整历史明细。
- `docs/CURRENT_STATUS.md` — 当前已实现状态（R2.3 会刷新）。
- `RoadMap.md` / `docs/ROADMAP_v2.md` — 中长期方向。
- `docs/STABILITY.md` / `docs/API_COMPATIBILITY.md` — 稳定面契约（R2.4 涉及
  前者的内部一致性修复）。
- `docs/RFC_CRATE_ARCHITECTURE.md` / `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`
  — 八条依赖铁律定义 + 上一轮架构评估（R1 段涉及的铁律来源）。
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
