# AgentFlow TODOs

Last updated: 2026-07-23

## 维护约定

- 旧执行计划按时间分批归档到 `docs/archive/`：
  - `TODOs-archive-2026-05-09-n1-n10.md` — N1–N10 路线图段（已闭环）。
  - `TODOs-archive-2026-05-10-p0-p4.md` — 早期 P-段执行计划（已闭环）。
  - `TODOs-archive-2026-05-19-recently-closed.md` — 5/19 从 Recently Closed
    扫出去的中段历史。
  - `TODOs-archive-2026-05-20-closed-segments.md` — 12 个全 closed 的 P-段
    （P0/P1/P2/P3/P4/P5/P6/P7/P-H/P9/P-LLM/M）整体外迁。
  - `TODOs-archive-2026-05-24-p10-optimization-backlog.md` — **5/24 归档**：
    P10 优化 backlog（v1.0.0-rc.1 ops + 19 个 crate-level 子段），全部 DONE 项
    + 少量未拾起的 polish。其中 polish 项未自动迁移到 Q-段——只有当 Q-段处理
    某个 crate 时主动从档案重新挑选才会回到本文件。
  - `TODOs-archive-2026-06-20-q1-q5-audit-remediation.md` — **本次 6/20 归档**：
    Q1–Q5 五段（2026-05-24 深度审计修复波次）全部闭环（108 DONE / 0 TODO）整体
    外迁，含 Audit Assessment Summary。源审计仍在 `docs/audit/`。
- 本文件是短期执行队列。Q-段（2026-05-24 审计修复）已全闭环并外迁；当前仅保留
  **H（Harness 收尾打磨）+ P-A（契约内核演进）** 两个 backlog + 最近 closed 摘要。
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

Current focus: **Q-段（2026-05-24 审计修复）已全闭环并外迁**；**P-A 契约内核轨道
已收口**（P-A4 ✅）；**S0 + S1 + S2 + S3 + S4 均已闭环**（S0.1–S0.3、
S1.1–S1.3、S2.1–S2.3 全 DONE，2026-07-24；S2.2b 网络安装+确认 UX 显式推迟；
S3.1–S3.4 全 DONE，2026-07-25～2026-07-27，借助 Apple `container` CLI 提供的
真实 Linux 6.12 VM 完成编译+内核强制实测，`security.os_sandbox` 默认值已
翻转为 `true`；S4.1 RFC 已采纳 + S4.2 `ContainerBackend`/`code_exec` 工具
已实现，2026-07-27，宏观复用 S3 的容器化验证基础设施，macOS/Linux 两端真实
隔离实测）；**L1–L5 全 DONE**；**H.1–H.5 全 DONE**（含本次补写的 H.2.1
完整记录）；剩余全是 DEFERRED / RoadMap non-goal——**TODOs.md 里已没有开放的
`TODO` 项**。

| Segment | Theme | Status |
| --- | --- | --- |
| P0 → P9 / P-H / P-LLM / M / P10 | 历史段，全部 closed 或外迁 | ARCHIVED |
| Q1 → Q5 | 2026-05-24 深度审计修复波次（安全 / 正确性 / 产品化 / 文档 / 横切），108 DONE | **DONE — archived（6/20 外迁）** |
| **H** | **Harness Mode follow-ups**（RFC loop-ownership + `harness chat` 收尾/打磨） | **active — backlog（可选）** |
| **P-A** | **契约内核 + 架构演进**（dynamic workflow 统一；见 `docs/RFC_CRATE_ARCHITECTURE.md`） | **active — backlog（next）** |
| **S** | **沙箱与代码执行安全演进**（2026-07-23 sandbox 复审：file+script 组合链 / skill 脚本完整性 / 依赖环境 / OS 后端强化 / code-exec） | **active — backlog（new）** |
| **L** | **长程任务与检索增强**（2026-07-23 课程大纲对照评审：replan 闭环 / 任务摘要恢复 / 项目记忆 / RAG 补强 / 委托契约） | **active — backlog（new）** |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## H — Harness Mode follow-ups (post loop-ownership + chat)

> 来源：`docs/RFC_HARNESS_LOOP_OWNERSHIP.md`（已实现并合并，PR #2）+
> `agentflow harness chat`（已实现并合并，PR #3）。**核心已生产可用、全绿、进
> main**；以下都是收尾打磨或主动推迟项，**无任何生产阻断**。状态：`TODO` =
> 可做的收尾增强；`DEFERRED` = 需设计或属 RoadMap non-goal。

### H.1 — `step_started` 实时排序（RFC Phase 1 残留）

- DONE H.1.1 turn-driven 模式下实时发 `step_started`（提交 `d905eb8`）：原 ReAct 循环从不发
  `AgentEvent::StepStarted`，harness 事后从 `result.steps` 重建所有 `step_started`，故事件流里
  step 边界整批堆在末尾、与 step 内的 tool/approval 事件不实时同序（RFC §5.6 诚实残留）。改为每记
  一个 step 就实时发：`AgentStepKind::kind_name()`（agent-spi 共享，live 与 post-hoc 永不 step_type
  漂移；harness `step_kind_name` 委托它）；`push_step!`（agents）记 step + 经同一 sink 实时发
  StepStarted，应用于全部产出 step 点（plan/tool_call/tool_result/reflect/final_answer 单+批）
  + init_run 的首个 observe step；无 live sink 时与旧裸 push 字节一致。bridge 直通映射 StepStarted
  并**单独计数**——某 runtime 可能实时发 tool 事件却不发 StepStarted，故仅当 bridge 确实发了才让
  `translate_inner_events` 跳过 post-hoc 重建；不发的 runtime（含测试 double）保持 post-hoc 不变。
  tool step 内 live step_started 落在该 step 首个事件之后而非严格之前,共享 step_index 无歧义配对,
  收益是实时交错而非末尾批量。golden agent trace 重生成（含 7 个 live step_started，steps/answer/
  stop 不变）+ 新 harness 测证明 live runtime 不被重复计数（每个 step_started 恰一次）。agents 179 +
  harness 77 + 集成绿，workspace --all-targets / clippy(-D) / check-arch 绿。**H 段全部 active 项闭环。**

### H.2 — `harness chat`：REPL 集成审批（替换守卫）

- DONE H.2.1 让 `--approve cli` 在 chat 里真正可用（提交 `68d6bb2` + 补测
  `afdd69e`，2026-06-22/23）：原状态 chat REPL 独占 stdin，`CliApprovalProvider`
  （阻塞 `std::io::stdin`）会和它抢字节，故 `harness chat --approve cli` 被
  启动守卫拒绝（PR #3）。落地：新增 channel-based `ChatApprovalProvider`——
  每次审批请求把 `(request, oneshot)` 转发给 REPL 循环，在 `select!` 里与
  run future / shutdown 信号一起服务，打印 prompt（`[y]es/[s]ession/[r]un/
  [n]o/[q]uit`）并从**同一个**共享 stdin reader 读决定；agent turn 阻塞等待
  审批期间主循环不会另外读 stdin，故无 reader 竞争；provider/channel 出错
  （REPL 已退出、EOF）按 fail-closed 处理为 deny。移除启动守卫，
  `build_chat_runtime` 接受 `--approve cli`。回归：`harness_chat_rejects_
  approve_cli` 改名 `harness_chat_accepts_approve_cli`，断言组合被接受、
  REPL banner 显示 `approve: cli`、`exit` 正常退出；`harness_cli_tests` 17
  测全绿。

### H.3 — `harness chat`：流式输出（可选 UX）

- DONE H.3.1 边出边打（逐步骤，提交 `c8a0e87`）：新增 `ChatProgressSink`（`HarnessEventSink`），
  与 JSONL sink 并挂进 chat runtime，随 harness bridge 实时发事件逐行打 stderr：工具请求
  `🔧 <tool>…`、完成 `✓/✗ <tool> (<ms> ms)`、记忆压缩 `📝 context compacted (<layer>)`；每行
  先 CR+erase-to-EOL 清掉 spinner/上一行，progress 干净堆叠在答案上方。**TTY 门控**：stderr 被
  管道（测试 / `| cat`）时静默，stdout 仍恰是答案、捕获输出不变。event→line 抽成纯函数
  `progress_line` + 单测；既有 harness_cli_tests(17) 仍绿（无 stdout 泄漏）。逐 token streaming
  需把 LLM streaming 贯穿 harness 层，属更大改动，本步做逐步骤级。注：`step_started` 暂不 stream
  （当前 post-hoc 发）——H.1.1 让它实时后 sink 会自动拾起。

### H.4 — `harness chat`：readline / 历史（可选 UX）

- DONE H.4.1 上下方向键历史 + 行内编辑（提交 `a71b3fa`）：新增共享
  `commands::repl::LineReader`（新依赖 `rustyline`），`harness chat` + `skill chat` 都用它
  （**全仓库一致性**：审批 prompt 也走同一 reader）。**双路径不动测试**：TTY → `rustyline`
  DefaultEditor（行编辑 + 上下历史；阻塞且占终端 raw mode，故跑在 `spawn_blocking`，每次读把
  editor move 进/出阻塞线程以保历史）；非 TTY（管道 / 集成测试）→ 裸 async 行读，与 H.4.1 前
  字节一致。`read_line` 返回 `ReadLine::{Line,Interrupted,Eof}`：Ctrl-C 弃当前行重新 prompt，
  Ctrl-D / `exit` / `/exit` 离开（标准 REPL 语义）；审批 prompt 的 Interrupted/Eof → fail-closed
  deny。harness_cli_tests(17) + skill_chat MCP 测 + cli lib(159) 经 fallback 全绿，clippy(-D)/
  check-arch 绿。tab 补全留后续（rustyline 默认无 completer）。

### H.5 — `harness chat`：`/clear` 命令（可选）

- DONE H.5.1 清空当前 session 对话记忆(保留 id)
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

> 来源：`docs/RFC_CRATE_ARCHITECTURE.md`（2026-06-19）+ 架构透镜评估
> `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`（验证 RFC 方向成立，给出 R1–R6
> 修订）。把四范式（静态 DAG / 原生循环 / harness / **dynamic workflow**）收敛
> 到一个窄腰契约内核 + 八条依赖铁律，采用**绞杀式原地演进，不重写**。**排序：在
> Q1/Q2 production-blocking 安全与正确性波次之后**——架构重构不得插队安全修复。
> 每个 PR 只改一件事，旧路径 `pub use` 兼容，`cargo test` + `clippy -D warnings`
> 全绿。
>
> 依赖：P-A0 → P-A1 →（P-A2 ∥ P-A3）→ P-A4。
>
> 评估关键结论（指导执行顺序）：
> - `agents→core` 全是 IR 符号（零 executor 符号）→ `graph` 拆分零残留耦合（R2）。
> - `value`（`FlowValue`）是 `graph` 的前置依赖且被最广泛引用 → **先抽 `value`**，
>   不再"可延后"（R1）。
> - `llm→core` 已在 Q3.6.1 删除 → RFC §1/§4 该处过时（R4）。
> - check-arch 当前只追踪 4 条 runtime/surface 边；还有 7 条 latent 违例（能力/
>   工具层）需随内核 crate 落地时一并 repoint（R5，见 P-A0.4）。
> - `harness→llm`(仅 tokenizer) / `mcp→tracing`(仅 traceparent) / `memory→rag`
>   (仅 EmbeddingProvider) 是"薄理由胖依赖"，折叠进 `value`/`agent-spi`/`store-spi`，
>   不另起微 crate（R6）。
> - `agentflow-nodes` 是横跨 tool/capability/runtime 三层的胖 crate → 需显式拆分
>   决策（R3，见 P-A0.5）。

### P-A0 — 立约 + 架构守卫

- DONE P-A0.1 落地 `docs/RFC_CRATE_ARCHITECTURE.md`，在 `RoadMap.md` /
  `docs/ARCHITECTURE.md` 加交叉引用。（RFC 已合并 `8567d37`；RoadMap 已加
  Contract-Kernel 段 + 交叉引用 2026-06-20。）
- DONE P-A0.2 `xtask check-arch`：解析各 `Cargo.toml`，断言 RFC §7 八铁律的可查
  子集（runtime-isolation + surface-isolation）；现有违例（`agents→core` /
  `harness→agents` / `worker→server` / `server→cli`）写入 `ARCH_ALLOWLIST` 逐条
  烧空。（已合并 `2d9c6e9`。）
- DONE P-A0.3 `xtask check-arch` 接入 CI Quality workflow，违例即红。（`quality.yml`
  加 `check-arch` job + 进 `release-gate` needs/summarize；本地 green 验证后提交
  `30ab107`。）
- DONE P-A0.4（R5）完整目标态边图已**代码化**进 `check-arch`：新增
  `ARCH_LATENT_EDGES`（16 条 latent 能力/工具边 = 评估 §2 展开），每次运行打印，
  并自维护——边被还清（dep 消失）→ FAIL 要求 prune；边升级为 active 违例 → FAIL
  要求移入 `ARCH_ALLOWLIST`，故清单只会变真或缩小。含 `evaluate_latent` 纯函数 +
  present/resolved/misfiled 单测 + 唯一性/与 allowlist 不相交 guard。提交 `3195ee3`。
  （`harness` 5 条 impl 边中 4 条 latent + 1 条 allowlist 均已显式追踪。）
- DONE P-A0.5（R3）`agentflow-nodes` 拆分决策已定，见
  `docs/RFC_NODES_DECOMPOSITION.md`（提交 `8a366e5`）：按实测 per-file 能力导入分两
  crate——`agentflow-nodes`（tool 层 7 个：template/file/http/batch/conditional/
  arxiv/markmap，仅 `graph`+`tool`）+ 新 `agentflow-nodes-ai`（能力适配 10 个：
  llm/asr/tts/image*/rag/mcp）。否决 feature-gate（optional dep 仍是 check-arch 边）
  与分散到各能力 crate（碎片化 node factory）。**落地在 P-A4**（依赖 P-A1.3 graph
  拆分）；之后 `worker→nodes` 零能力负担，解锁 P2.8。

### P-A1 — 契约内核抽取 + 垂直切片

> 执行顺序（评估修订）：**P-A1.5（value）→ P-A1.3（graph）→ P-A1.1/1.2（spi）→
> P-A1.4（async-util）→ P-A1.6（spike）**。`value` 是 `graph` 的前置依赖，必须先抽。

- DONE P-A1.5（R1，首抽）`agentflow-value` 叶子 crate 已抽出（提交 `e315849`）：
  `FlowValue` + serde 转换从 `agentflow-core/src/value.rs` 移入零内部依赖的新 crate；
  `core` 依赖并经 `pub use agentflow_value as value` re-export，`agentflow_core::value::
  FlowValue` / `agentflow_core::FlowValue` 全部不变即编译通过（忠实绞杀，暂不 repoint
  consumers——在 graph 拆分前 repoint 不消任何边）。验证：value 6 测试 + core 190+ 测试
  通过、`cargo check --workspace --all-targets` clean、check-arch green（17 members）、
  fmt/clippy clean。`nodes`/`agents` 的 repoint 留到 P-A1.3 graph 拆分时一并做。
- DONE P-A1.3 从 `agentflow-core` 拆 `agentflow-graph`（IR），`core` 留执行引擎；
  re-export 兼容。**IR ≠ executor 拆分已闭环**（提交 `0252972` / `caf4b04` /
  `4cc5067` / `c8a5323` / `be75e5c` / `56246e9` / `91c6604`）。决策（用户选定）：
  **FlowExt 扩展 trait 保留 `flow.run()`**，引擎在 core 的 `FlowExecutor<'a>(&Flow)`
  内联实现（orphan-rule 干净），`Flow`/`GraphNode`/`NodeType` 在 graph。
  `agentflow_core::{Flow,GraphNode,NodeType,FlowExt}` re-export，调用方唯一变化是
  `use agentflow_core::FlowExt`。验证：workspace build/clippy/fmt clean、core 184 +
  graph 178 + agents + workspace doc 测试全过、check-arch green。**`agents→core` 已烧**
  （2d-ii/2d-iv 随 P-A2.1 FlowRunner 契约线落地，2026-06-23 审核确认空 allowlist）。
  分两步执行明细见下：
  - DONE **step 1/2**（提交 `0252972`）：纯 IR 叶子 `error`/`async_node`/`node`/`expr`
    移入 `agentflow-graph`（仅依赖 `value`）；`core` 依赖 graph 并按原路径 re-export，
    全仓库不变即编译。graph 30+ / core 190+ 测试通过，check-arch green（18 members）。
  - step 2 拆成 2a–2d 四个绿色子步：
    - DONE **2a/2b**（提交 `caf4b04`）：`events`（EventListener/WorkflowEvent/listeners，
      零 crate 依赖）+ `state_size`（StateSizeObserver）整体移入 graph，core re-export。
    - DONE **2c**（提交 `4cc5067`）：`CheckpointConfig`（数据）移入 `graph::checkpoint`；
      `CheckpointManager`/`Checkpoint`/`WorkflowStatus`（IO 逻辑）留 core 经 re-export 引用。
      **至此 `Flow` 的全部字段类型都已在 graph**，2d 解锁。
    - **2d**（最大、触热路径）分 i–iv：
      - DONE **2d-i**（提交 `c8a5323`）：`Flow.checkpoint_manager: Option<Arc<CheckpointManager>>`
        字段改为 `checkpoint_config: Option<CheckpointConfig>`；私有 `checkpoint_manager()`
        helper 按需重建无状态 manager（resume/load_resume_plan + 两个执行循环共 6 处）；
        `with_checkpointing` 仍 eager 校验后存 config。**至此 Flow struct 全字段皆 graph 数据**。
        checkpoint/resume/recovery 测试全过。
      - DONE **2d-iii**（提交 `be75e5c`，**先于 2d-ii 做**）：Flow 加 5 个 pub accessor
        （`nodes`/`is_checkpoint_enabled`/`checkpoint_config`/`event_listener`/
        `state_size_observer`），执行引擎全部 field **读**改走 accessor；builder 的写仍直存
        （随 struct 迁 graph）。13 套测试通过。**至此执行引擎对 Flow 的耦合只剩 5 个 accessor
        + 私有 `checkpoint_manager()` helper。**
      - DONE **2d-ii**（core 内，独立可绿）：`struct FlowExecutor<'a>{ flow:&'a Flow }` 已落地
        （`agentflow-core/src/flow.rs:35`）——引擎（私有 helper + execute_* 深层逻辑）从 `impl Flow`
        迁入 `impl<'f> FlowExecutor<'f>`，Flow 的公开方法保留为薄委托。全程 core 内，无跨 crate。
        （随 P-A2.1 的 `FlowRunner` 契约线一并落地；2026-06-23 审核确认 core 99 单测 + 全集成测过。）
      - DONE **2d-iv**（原子移动）：`Flow`/`GraphNode`/`NodeType` + builder + accessor 已在
        `agentflow-graph/src/flow.rs`（带 `with_checkpoint_config` 无校验 setter + 5 个 pub accessor）；
        7 个公开执行方法在 core 改为 `pub trait FlowExt`（`impl FlowExt for Flow` 委托 FlowExecutor），
        core lib re-export `Flow`/`GraphNode`/`NodeType` + prelude 含 `FlowExt`，call sites（agents 等）
        改 `use agentflow_core::FlowExt`。**`agents→core` 已烧**：agents 实依赖 `agentflow-graph`（IR）
        + `FlowRunner` 契约，`agentflow-core` 仅 dev-dep；`CoreFlowRunner` 由 surface 注入。check-arch
        现 0 tracked / **空 allowlist**（2026-06-23 审核确认：build/test/check-arch 全绿）。
      - 历史分析（flow.rs 3246 行）：
      1. graph：`NodeType`/`GraphNode`/`Flow` struct（`checkpoint_manager:
         Option<Arc<CheckpointManager>>` 字段改为 `checkpoint_config: Option<CheckpointConfig>`）
         + 7 个 builder（行 65–122，`with_checkpointing` 保留 `-> Result<Self>` 签名但只存
         config）+ `execution_order`（纯拓扑）+ 新增 executor 需要的 **pub accessor**
         （`nodes()`/`checkpoint_config()`/`event_listener()`/`state_size_observer()`）。
      2. core：`pub trait FlowExt` + `impl FlowExt for Flow` 收 7 个执行方法（run/resume/
         resume_with_options/load_resume_plan/execute_from_inputs*），第二个 impl 块（行
         722–1489 私有调度 helper）改为 core 内**自由函数** `fn helper(flow: &Flow, …)`；
         ~23 处 `self.<field>` 改走 accessor；`FlowExt::run()` 用 `checkpoint_config` 在
         运行时 `CheckpointManager::new(cfg)`。
      3. core lib.rs re-export `Flow`/`GraphNode`/`NodeType`/`NodeStatus` from graph；
         加 prelude 含 `FlowExt`，call sites（`agentflow-agents` 10+ 文件等）加
         `use agentflow_core::FlowExt`（或 prelude glob）。
      4. flow.rs 测试（行 1490–3246，~1756 行）跟执行逻辑迁到 core。
      完成后 `agents→graph` 干净、烧掉 allowlist 第 1 条（agents→core）。
- P-A1.1 `agentflow-agent-spi`（依赖 P-A1.2 store-spi 已解锁）分两步：
  - DONE **1/2 运行时契约**（提交 `2abf420`）：`runtime.rs` 整体（AgentRuntime /
    AgentEvent / AgentStep / AgentContext / RuntimeLimits / 取消令牌 / event+memory
    hook / AgentRuntimeError）移入新 `agentflow-agent-spi`；`Message`/`MemoryStore`
    指向 store-spi；agents 依赖 agent-spi 并按原 `agentflow_agents::runtime` 路径
    re-export——**消费方零改动**。react/reflection 仅是 intra-doc 链接，降级为普通
    code span。agent-spi 19 + agents 159 测试过、check-arch green（20 members）。
    `agent-spi→llm` 是过渡（仅为 `AgentContext.trace_context: LlmTraceContext`），
    待 R6 trace-context 契约消除。
  - DONE **2/2 harness 契约移入 agent-spi**：把 harness 的 `HarnessEvent`（+全部
    payload 类型）/ `Approval*`（Request/Decision/Risk/Scope/Outcome + `ApprovalProvider`
    trait）/ `PreToolHook` / `PostToolHook` / `HarnessEventSink` trait / `ContextProvider`
    （+ `HarnessContext`/`HarnessProfile`/`HarnessRuntimeKind`/`ContextItem`/
    `ContextPriority`）+ 共享的 `HarnessError` 整体移入新 `agentflow-agent-spi::harness`
    子模块。**忠实绞杀**：harness 的 `error`/`approval`/`context`/`hooks`/`event` 五个文件
    降为 `pub use agentflow_agent_spi::harness::<mod>::*` re-export shim；`persistence.rs`
    保留具体 sink 实现（Jsonl/Stdout/InMemory/SinkChain），仅 trait 移走并 re-export——
    **消费方（server/cli）零改动**。agent-spi **零新增依赖**（chrono/serde/async-trait/
    thiserror/tools 全已在）。redaction（`params_summary.rs`→`agentflow_tracing`）刻意留
    harness（契约类型只持已脱敏字符串、不调 redaction），故本步**不烧** `harness→tracing`
    边——那需把 redaction 下沉到 value/agent-spi，留后续。验证：agent-spi 38（含迁入的 19
    个契约测试）+ harness 74 + envelope_contract 6 + server 180 + cli 173 + agents 163
    全过、`cargo build --workspace --all-targets` clean、clippy(-D warnings) clean、
    `cargo doc` clean、check-arch OK（agent-spi 无新边）。
  - 拆出（留 P-A4.3）：RFC §2 的 `Capability`/`Lowered` trait 是推测性新设计，与
    skills 降解（P-A4.3 `Capability::lower`）强绑定——待有真实消费者时随 P-A4.3 落地，
    不在本契约抽取步里空写。
- DONE P-A1.2 `agentflow-store-spi` 已抽出（提交 `be9a148`）：`MemoryStore` +
  `Message`/`Role`/`TokenCounter` + `MemoryError` 从 `agentflow-memory` 移入新契约
  crate；具体 store 实现（SessionMemory/SqliteMemory/SemanticMemory/preference/
  entity）留 memory，memory 依赖 store-spi 并按原路径 re-export——**消费方零改动**。
  这给 `Message` 一个契约家，解锁 P-A1.1（agent-spi 可依赖 store-spi::Message 而非
  memory 实现 crate）。store-spi 6 + memory 53 测试通过、workspace check clean、
  check-arch green（19 members）。
  **未做（留 follow-up）**：(a) `EmbeddingProvider`（R6，`memory→rag` 边）需先统一
  rag/memory 错误面再收进 store-spi；(b) `MemoryError` 目前把 `sqlx` 钉进契约 crate
  （orphan rule），后续可瘦身解耦；(c) `KnowledgeBackend` 是 P-A4 RAG 归位时新写。
- DONE P-A1.4 `agentflow-async-util` 已抽出（提交 `d5b4f26`）：retry + timeout 组合子
  从 core 移入新 crate，core re-export `agentflow_core::{retry,timeout}`——消费方零改动；
  retry_executor 留 core。加 `observability` feature 并从 core 传播。async-util→graph
  （AgentFlowError），待通用错误重构解耦。**五个新内核 crate 全部就位**（value / graph /
  store-spi / agent-spi / async-util）。与 agents 重复实现的合并是 P-A3.2。
- DONE P-A1.6 dynamic-workflow 垂直切片 spike 已落地（提交见下）：
  `agentflow-agents/examples/dynamic_workflow_spike.rs` 演示 toy planner 运行时
  生成 `Flow`（graph IR，shape 运行时定）→ core 经 `FlowExt` 执行，二者只经 graph
  契约相遇。已接入 examples-smoke CI gate。验证内核可承载 dynamic workflow；P-A4
  产品化（PlanExecuteAgent 产出真 Flow）。

### P-A2 — 运行时解耦（去横向依赖）

- DONE P-A2.1 `harness`→`agents` 边已烧（提交 `17f9b7e`）：turn-driven 契约
  （TurnDrivenRuntime/LoopSession/TurnProgress）从 agents/react 移入 agent-spi；
  harness 依赖 agent-spi 契约，具体 ReActAgent 由前门注入（smoke 测试保留 agents 为
  dev-dep）。**check-arch tracked 4→3**，allowlist 移除该条。
- DONE(MVP) P-A2.2 harness 治理 `Flow` 执行已落地（分支 `feat/p-a2.2-harness-governs-flow`）：
  `HarnessRuntime::for_flow()` + `run_flow(flow, runner, inputs, options)` —— 用注入的
  `FlowRunner`（P-A4.3 契约）驱动 Flow，外包 Harness 信封（`session_started` runtime=`flow` →
  `stopped`，按 per-node result map 分类 completed/failed/timed-out）。工具治理走 registry seam:
  Flow 节点的 registry 经 `wrap_registry` + `HookConfig`(共享 runtime 的 seq_counter + sinks)
  包裹后,approval/hook/audit 事件与信封交织在同一单调流上。新增 `HarnessRuntimeKind::Flow` 变体 +
  `InnerRuntime::None`。新增 harness→agentflow-graph 依赖(runtime→contract,executor 经 FlowRunner
  留外,check-arch 绿)。2 集成测(信封 + AutoDeny 阻断节点工具调用并 fail run)+ harness 74 测全绿,
  clippy(-D)/fmt/全 workspace build 绿。
- DONE P-A2.2-FU1 节点级 `step_started` 事件已落地（分支 `feat/p-a2.2-flow-node-events`）：
  `run_flow` 给 flow 挂 `agentflow-graph::EventListener`，把每个节点的 NodeStarted（node_id）
  经 channel 转出，与 run 并发 drain（biased `select!`），实时发 `step_started`
  （`step_type = "node:<id>"`），与工具/审批事件在 `session_started`↔`stopped` 间实时交错。
  经现有 `Flow::with_event_listener` seam 观测，零 executor 耦合。+1 集成测（2 节点→2 step_started，
  seq gap-free）。
- DONE P-A2.2-FU2 CLI surface 已落地（分支 `feat/p-a2.2-harness-flow-cli`）：新子命令
  `agentflow harness run-flow <workflow.yaml>` —— build_flow_from_yaml → `HarnessRuntime::run_flow`
  + `CoreFlowRunner`,把 Harness 信封(session_started runtime=flow → 每节点 step_started → stopped)
  按 agent session 一样持久化为 JSONL。flag:`--input k=v`(可重复)/`--model`/`--profile`/`--output
  text|json|stream-json|json-envelope`/`--workspace`/`--run-dir`/`--timeout-ms`/`--session`/
  `--max-concurrency`;非 completed 退出非零。`--runtime flow` 解析已加。+1 assert_cmd e2e(模板 DAG,
  无 LLM,断言 completed + JSONL 信封 + 2 step_started)。clippy(-D)/fmt/check-arch 绿。
- DONE P-A2.2-FU2-server harness 治理 Flow 的 HTTP route 已落地（分支
  `feat/p-a2.2-harness-flow-server`）：`POST /v1/harness/sessions` 接受 `runtime_kind: "flow"` +
  `workflow`(YAML)字段；`LiveHarnessExecutor::live_execute` 分流到 `live_execute_flow`——
  build_flow_from_yaml → `HarnessRuntime::run_flow` 经同一 `ServerHarnessEventSink`(DB + SSE broker)
  发信封,故既有 events/history/status 路由对 flow session 直接可用。flow runtime 需 `workflow`(否则
  400),`user_input` 不再必填。因 `HarnessRuntime` `!Sync`,在 spawn_blocking + current-thread runtime
  跑(同 agent 路径)。校验抽成 `validate_session_inputs` 纯函数 + 单测;181 server 测全绿,clippy(-D)/
  fmt/check-arch/全 workspace build 绿。**剩余 follow-up**:flow session `:resume`(workflow 未存 row,
  需加列)+ 工具级审批(config 节点内嵌工具,故 route 同 CLI 只给信封+节点事件)。
- DONE P-A2.2-FU3a dynamic plan 支持 agent 步（分支 `feat/p-a2.2-dynamic-agentnode-steps`）：`WorkflowPlanStep` 加 `kind`（tool 默认 | agent）；`compile_plan_to_flow` 把 agent 步编译成 `AgentNode`（包 `ReActAgent`，params.model 必填/persona 选填/prompt→message），共享 plan 的 registry 故继承同一治理。依赖 input_mapping 用各步真实输出 key（tool→result，agent→response）。planner prompt 教 LLM agent 步形态，故 `workflow dynamic` 可产出。4 新测（编译/校验/输出 key 接线）+ 175 agents 测全绿。**3b 已落地**（分支 `feat/p-a2.2-planexecute-emits-flow`）：`PlanExecuteAgent::compile_plan_to_flow(steps)` 复用 dynamic 编译器,把 plan 的 tool 步编译成 `Flow`(继承 retry/checkpoint/timeout/tracing/replay + 可并行),替代手写顺序循环。`PlanExecuteStep` 加可选 `depends_on`:空=链在前一 tool 步后(默认保序),非空=并行 DAG;reasoning 步(tool=None)丢弃。3 新测(默认链式/显式 deps/跳过 reasoning)+ 178 agents 测全绿。legacy `run_with_context` 顺序路径不变。
- DONE P-A2.2-FU3b-e2e `run_as_flow` 端到端已落地（分支 `feat/p-a2.2-planexecute-emits-flow`）：
  `PlanExecuteAgent::run_as_flow(context, runner)` —— LLM 规划 → compile_plan_to_flow → 经注入
  `FlowRunner` 在确定性引擎执行,返回带 Observe→Plan→逐节点 ToolCall/ToolResult→FinalAnswer
  trace 的 `AgentRunResult`(节点失败→`AgentStopReason::Error`)。复用既有 call_planner/parse_plan/
  memory 私有 helper,不动 sequential 路径(零回归);同一 cancel/timeout/token/step/tool-call 预算。
  `PlanExecuteError` 加 `Flow(#[from] AgentFlowError)`。+1 mock-LLM e2e(plan→compile→execute→answer)
  + 179 agents 测全绿。clippy(-D)/fmt/check-arch/全 workspace build 绿。
- DONE P-A2.2-FU3b-e2e `run_as_flow` 端到端已落地（分支 `feat/p-a2.2-planexecute-emits-flow`+续）：`PlanExecuteAgent::run_as_flow(context, runner)` —— LLM 规划 → compile_plan_to_flow → 经注入 `FlowRunner` 在确定性引擎执行,返回带 Observe→Plan→逐节点 ToolCall/ToolResult→FinalAnswer trace 的 `AgentRunResult`(节点失败→`AgentStopReason::Error`)。复用既有 call_planner/parse_plan/memory 等私有 helper,不动 sequential 路径(零回归)。同一 cancel/timeout/token/step/tool-call 预算。`PlanExecuteError` 加 `Flow(#[from] AgentFlowError)`。+1 mock-LLM e2e(plan→compile→execute→answer)+ 179 agents 测全绿。
- DONE P-A2.3 抽 `agentflow-worker-proto`，烧 `worker→server`（PR #25）。新 crate 收
  WorkerProtocol 契约 + 全部 wire 类型 + SchedulerError + InMemoryWorkerProtocol +
  NodeExecutionPayload + GrpcWorkerProtocol(client) + proto↔domain 转换 + traceparent
  helpers + worker.proto codegen(build.rs+tonic-build→pb)。worker 依赖 worker-proto（server
  降 dev-dep）；server 留 control plane(WorkerControlPlane/scheduler) + gRPC server
  (WorkerControlServer/WorkerControl)，从 worker-proto import 契约+pb+转换并按原 scheduler::*
  路径 re-export（消费方零改动）。**check-arch 现仅 1 tracked：agents→core**（最后的
  runtime-isolation 边）。全绿：workspace all-targets / clippy -D / server 180+集成(含 gRPC
  cross-hop e2e) / worker 16+集成。**P-A2 运行时解耦全段闭环。**。**已详细 scoping（2026-06-21）**：
  非简单搬迁，是 **gRPC codegen crate 迁移**。`worker` 用 `GrpcWorkerProtocol`/`InMemoryWorkerProtocol`/`WorkerId`。`scheduler/mod.rs`（1562 行）既是协议又是聚合器
  （`pub use admission/distributed/grpc`）；`grpc.rs` 的 `GrpcWorkerProtocol` 依赖
  `build.rs` + `proto/agentflow/*.proto` + `tonic-build`/`prost` codegen。烧边需：
  (a) 新建 `agentflow-worker-proto`，迁 `build.rs` + `.proto` + `tonic-build`/`prost` 依赖；
  (b) 从 mod.rs carve ~18 协议类型（WorkerProtocol trait + WorkerTask/WorkerTaskResult/
  WorkerHeartbeat/WorkerId/WorkerCapabilities/ClaimHints/SchedulerError/WorkerControlPlane/
  InMemoryWorkerProtocol/trace 类型）；(c) `admission/distributed/jwt` 留 server，从
  worker-proto 反向 import；(d) 解开 mod.rs 聚合 re-export。**大且涉 codegen，建议独立 fresh session。**
- DONE(step 1/2) P-A2.4 抽共享 assembly，删 `server→cli`（PR #22 = step 1）：
  新建 `agentflow-config` crate（config schema v2/schema + executor build_flow_from_yaml
  + node factories），从 cli 外迁；cli `pub use agentflow_config::{config, executor}` 兼容
  + feature 转发（plugin/rag/mcp）；server `runs.rs`(build_flow_from_yaml) + `scheduler::
  distributed`(V2 schema) 改 import agentflow-config。**3 个 server→cli 用点已repoint 2 个**。
  **剩余 step 2（烧边）**：doctor `build_report`（server `/diagnostics` 用）——需把 report
  builder 从 doctor 命令拆出（`execute` 耦合 cli `json_envelope`，故 build_report + report
  model + DoctorProfile → agentflow-config 的 diagnostics 模块；execute/OutputFormat 留 cli），
  repoint server `diagnostics.rs`，然后从 ARCH_ALLOWLIST 移除 server→cli 烧边。
- DONE(step 2/2 — 烧边完成) P-A2.4 server→cli 已烧（PR #23）：doctor report builder
  （build_report/DoctorProfile/DoctorReport+report model/print_text_report）移入
  `agentflow_config::diagnostics`；execute（耦合 json_envelope）+ probe_top_level_mcp_config
  （读 McpConfigFile）留 cli；build_report 加 `top_level_mcp` 参数由 caller 注入（cli 传真
  probe，server 传空）——行为不变。server **完全不再依赖 agentflow-cli**（Cargo 移除 dep）。
  check-arch 从 ARCH_ALLOWLIST 删 server→cli，现 **2 tracked**（agents→core / worker→server），
  测试断言已更新。cli `pub use agentflow_config::diagnostics::*` 兼容，doctor 命令不变。
  **P-A2.4 全闭环。** 剩余 P-A2 烧边：worker→server（P-A2.3 worker-proto，大、独立 session）。

### P-A3 — 可靠性合并 + 类型加固

- DONE P-A3.1（前置）加厚 `agentflow-agents/src/react/agent.rs` 循环测试覆盖（PR #19）：
  补齐 timeout/cancellation **racing** 路径——既有测试只覆盖 pre-signalled cancel +
  batch max-tool-calls，新增 4 个确定性测试覆盖 `run_turn_llm_call`/`run_turn_tool_call`
  的四臂 `select!`（LLM-call timeout / LLM-call cancel / tool-call timeout / tool-call
  cancel）。用"永不完成的慢操作"（10s sleep >> ~50ms deadline）保证结果确定、不依赖调度
  时序；tool-cancel 用 started 标志确保取消落在工具在飞行时。配套：Mock provider 认
  `AGENTFLOW_MOCK_DELAY_MS` env（registry 路径可模拟慢往返）+ panic-safe `EnvVarGuard`。
  **这是 P-A3.2（race_with_limits 抽取）的安全网前置**。
- DONE(部分) P-A3.2 timeout×cancellation `select!` 抽 `async-util::race_with_limits`
  （PR #20）：新增 `race_with_limits(fut, remaining, cancel) -> RaceOutcome::{Completed,
  TimedOut, Cancelled}` 收敛四臂 `(Option<Duration>, Option<CancelSignal>)` 矩阵 + 双层
  `tokio::select!`；ReActAgent 的 LLM-call + tool-call 两个单调用点改为委托，重复的
  timeout/cancel 分支各只写一次（LLM 点 88→38 行）。经 `agentflow_core::{race_with_limits,
  RaceOutcome}` re-export；async-util 加 tokio `macros` feature。行为不变——P-A3.1 racing
  测试不改即通过 + 6 个组合子单测。**batch follow-up 已闭环（PR #21）**：concurrent
  `join_all` + serial per-call 两个 batch 矩阵也改为委托 race_with_limits——先加 4 个
  batch racing 测试（concurrent/serial × timeout/cancel，characterize-then-repoint），
  ReActAgent 热路径已无任何 timeout/cancel select! 矩阵（4 处全收敛）。**剩余**：
  `agentflow-core` shutdown 路径的 select!（不同语义，独立评估）——非本任务核心，按需再做。
- DONE P-A3.3 `ReActLoopSession` consuming typestate（SessionFinished 提前到编译期，
  提交 `28e68f0`）：原 `next_turn(&mut self)` 带 `finished: bool`，finish 后再调返回运行时
  `ReActError::SessionFinished`。改为 `next_turn(self)` **consume session** 返回
  `ReActTurn::{Continued(ReActLoopSession), Finished{result, agent}}`——Continued 交还活
  session，Finished 交还结果+agent 借用（调用方仍可读 memory）且**无 session**，故"finish
  后再调"是**编译错误**（use-of-moved-value）而非运行时错。删 `finished` 字段 + `SessionFinished`
  变体。**关键边界**：harness 经 `Box<dyn LoopSession>`（object-safe，`&mut self`）治理，
  typestate **无法跨 dyn**（&mut self 不能 move out），故 trait 路径保留运行时守卫，下沉到
  小 adapter `ReActTurnDriver`（活 session 装 `Option`，finish 后 None；保留 agent 借用让
  finish 后 memory()/turn_index() 仍可答）。测试改驱动 consuming API（旧 SessionFinished
  运行时断言换成"等价调用已不可编译"注释）。agents lib 179+集成 + harness 77+ 全绿（经
  adapter 驱动），workspace --all-targets build + clippy(-D) + check-arch 绿。
  **P-A3 类型加固全段闭环。**
- DONE P-A3.4 `Seq` newtype + `SeqAllocator::stamp`（消事件 seq-vs-write 乱序 race）
  （提交 `33feacf`）：每个 emit 点原本 `counter.fetch_add` → build → `dispatch().await`
  三步独立；`fetch_add` 只保证 seq **数字**单调，但 dispatch 是 `.await`——并发 emitter
  （并行工具调用经 hook 层 / live AgentEvent bridge / 多个后台任务）可后分配者先落 sink，
  `SinkChain::dispatch` 不跨调用串行化，故乱序写穿透到每个 sink，破坏 Beta 冻结信封的
  "单调无 gap" 线序承诺。新增 `Seq`（透明 u64 newtype，wire 字段不变）+ `SeqAllocator`，
  其 `stamp`/`stamp_lossy` 在 (分配, build, dispatch) 全程持 emit 锁；Clone 共享 counter+锁，
  共享一个 allocator 的写者彼此串行。所有 emit 点改走它（runtime 信封+bridge+context refresh、
  hook `emit_event`、后台任务、flow 治理）；post-loop `translate_inner_events` 纯构建保留裸
  counter（分配与 dispatch 同序不会 race）。向后兼容：`with_seq_counter`/`seq_counter()` 留
  shim，新 `with_seq_allocator`/`seq_allocator()` 全保证 API，server `harness_live` + cli `chat`
  迁移共享一个 allocator。回归测试：16 并发 `stamp` 经"按 seq 反向延迟"的 sink，无锁会乱序、
  有锁保持 seq 序。harness 全测（77+集成）+ server 全测绿，clippy(-D)/check-arch 绿。
- DONE P-A3.5 消 UTF-8 切片 panic（提交 `9527797`）：审计全部裸字节切片，**唯一真实**
  panic 是 `llm/models.rs` 的 `truncate(s,max)` 用 `&s[..max]`——其 `truncate(&body,200)`
  调用方截断任意远端 HTTP 响应体（错误路径），非 ASCII payload 即 panic（报错时再崩）。
  改为 `chars().count()`/`chars().take()`（全仓库主流安全写法），byte→char 预算对显示/
  错误预览无害且对可视宽度更正确。其余裸切片审计为已安全：`slugify_skill_name` 只产 ASCII
  slug（`truncate(64)` 永不落非边界）、`extract_direction_section` 在 `\n` 锚定的 find 索引切、
  `backup.rs` 用 ASCII-find 字节偏移。**否决 `ByteSafeStr` newtype**：单个残留点上属过度工程，
  仓库约定已是 chars().take。+1 多字节回归测试（旧字节切片会从中间劈开 codepoint）。clippy 绿。
- DONE P-A3.6 chat REPL 切换失败脏状态已消（提交 `7bbbb5b`，2026-06-23 审核确认 TODO 滞后）：
  `/model` + `/skill`（及 `/new`/`/clear`）改为 **commit-on-success**——只在 `build_chat_runtime`
  成功后才改 `cur_model`/`cur_skill`/`runtime`/`model`，失败保留原模型，不留半切换脏态。
  **否决 `Validated<ModelId>` newtype**：底层 bug（失败留脏态）已由 commit-on-success 控制流消除,
  newtype 是冗余 type ceremony。
- DONE P-A3.7 契约 enum 加 `#[non_exhaustive]`（提交 `3e9ad0e`）：前向兼容硬化——给
  non-exhaustive enum 加变体不再强制下游每个 `match` 破裂。已标记：边界 error 枚举
  （thiserror 面）`MemoryError`(store-spi) / `ToolError`·`SecurityProfileError`·
  `SandboxError`(tools)，加上既有的 `AgentFlowError`/`KnowledgeError`/`CapabilityError`/
  `AgentRuntimeError`/`HarnessError`（error 消费方用 `?`/to_string/通配，零 ripple）；
  观测事件枚举 `WorkflowEvent`(graph) + `AgentEvent`(agent-spi)。ripple 由通配 arm 修复
  且保持现行为：3 个 supervisor `rewrite_event_step_index` + ReAct resume offset 循环
  （无 step_index 的事件本就 no-op）+ server `workflow_event_payload`（未知变体回空 JSON）。
  **刻意保留 exhaustive**：穷尽匹配是特性的闭语义枚举——`FlowValue`/`NodeType`/`ExprValue`/
  `RaceOutcome`/审批·profile 协议枚举/`ToolIdempotency`/Beta 冻结的 `HarnessEventBody`
  "closed kind set"。**刻意不 seal** L0 SPI traits（AgentRuntime/Tool/MemoryStore/
  KnowledgeBackend/ApprovalProvider/…）——它们是内核的下游扩展点。workspace --all-targets
  build + clippy(-D) + 受影响 crate 测试（tools 179 / agents 85+ / agent-spi / graph /
  store-spi）+ check-arch 全绿。

### P-A4 — Dynamic workflow + RAG 归位（收口）

- DONE P-A4.0（落地 P-A0.5/R3）`agentflow-nodes` 拆分完成（PR #28，分支
  `feat/p-a-nodes-decomposition`）：tool 层 `agentflow-nodes`（7 个：template/file/
  http/batch/conditional/arxiv/markmap，仅依赖 IR + `agentflow-tools`，去掉 llm/mcp/
  rag deps 与 feature）+ 新 `agentflow-nodes-ai`（能力适配 9 个：llm/asr/tts/
  text_to_image/image_to_image/image_understand/image_edit + mcp/rag feature-gated，
  依赖 `agentflow-nodes` 复用 common/error）。dispatch 不变：`agentflow-config::
  executor::factory` 分别从两个 crate import；cli 把 mcp/rag 转发给 config；worker
  保留 tool 层、仅为 llm/mcp payload 拉 nodes-ai（带 mcp）。`nodes→{llm,mcp,rag}` 三条
  latent 边消解、从 `ARCH_LATENT_EDGES` 剪除（`nodes→core` 保留，留 core→graph repoint）。
  build/clippy(-D)/fmt/check-arch 全绿。
- DONE(部分) P-A4.1 `rag` impl `KnowledgeBackend` + `rag_search` 工具已落地（分支
  `feat/p-a4.1-rag-knowledge-backend`）：L0 `agentflow-store-spi` 新增 `KnowledgeBackend`
  trait + `KnowledgeChunk` + `KnowledgeError`（`#[non_exhaustive]`，与 `MemoryStore` 同层，
  让 `skills`⟷`rag` 共享契约而不互依实现）。`agentflow-rag` 两个实现：`Bm25KnowledgeBackend`
  （内存 BM25、可单测、bundled-files 层）+ `VectorStoreKnowledgeBackend`（任意 `VectorStore`
  + `RetrievalStrategy` 的语义检索层）；并暴露 `RagSearchTool`（`rag_search`，idempotent 只读，
  包 `Arc<dyn KnowledgeBackend>`）。rag 新增向下依赖 store-spi + tools（均 L0），check-arch OK。
  store-spi 2 测 + rag 9 测全绿，clippy(-D)/fmt/全 workspace build 绿。
- DONE P-A4.1b `rag search/index/collections` CLI 降为运维子命令（分支
  `feat/p-a4.1b-rag-ops-cli`，接在 #32 上）：三者移到 `agentflow rag ops <cmd>`（新
  `RagOpsCommands` enum + `RagCommands::Ops` 变体），`rag eval` 保留顶层（质量门）。
  agent 面向的检索路径是 Skill 暴露的 `rag_search` 工具,`ops` 仅供运维直连向量库。
  dispatch 改为 `Ops(ops) => match ops {...}`;help 文案说明降级原因。改了 1 个 assert_cmd
  测试（`rag search --help` → `rag ops search --help`)+ ARCHITECTURE/RAG_EVAL/MEMORY_LAYERING
  /CLAUDE 文档。**破坏性**:旧 `rag search|index|collections` 路径失效(flag 不变),已记 CHANGELOG。
  clippy(-D)/fmt/cli 测试绿。
- DONE P-A4.2 `skills` 分层知识解析已落地（分支 `feat/p-a4.2-skill-knowledge-backend`，
  接在 #30 上）：`KnowledgeConfig` 加 `backend: KnowledgeBackendKind`（`#[serde(default)]`，
  `files` 默认 / `rag` opt-in）。`SkillBuilder` 分流：`build_persona` 只 inline `files` 层；
  新 `register_knowledge_backends` 把 `rag` 层文件用 `Bm25KnowledgeBackend` 建内存索引、注册
  单个共享 `rag_search` 工具（坐在 P-A4.1 契约上，零向量库/网络）。`references/` 仍属 files 层。
  4 新测（默认值 / rag 层注册工具且不进 persona / files 层 inline 且无工具 / 混合层各自路由）+
  112 skills 测全绿，clippy(-D)/fmt/check-arch 绿。docs/SKILLS.md 增补 backend 分层。每个 rag
  文件暂按整文件索引,细粒度 chunking 留作后续 refinement。
- DONE P-A4.3 `Capability::lower` 已落地（分支 `feat/p-a4.3-capability-lower`，接在 #31 上）：
  RFC §2 第二根契约 trait 进 `agentflow-agent-spi`——`Capability` trait + `Lowered { tools,
  context }` + `CapabilityError`（`#[non_exhaustive]`）。`Lowered.context` 复用既有 `ContextItem`
  （priority + token_estimate），直接接 harness/runtime 的 prompt 预算机制；`Lowered::merge` =
  capability flatten 组合。与 OS-sandbox `agentflow_tools::Capability` enum（进程权限）同名不同
  物，永不同位。`agentflow-skills` 落实现：`SkillCapability`（manifest + skill_dir）`lower()` →
  build_registry 的工具（built-in + MCP + P-A4.2 rag_search）+ persona 作单个 Critical context
  fragment。新增 skills→agent-spi 直接 L0 依赖（capability→contract，check-arch 绿）。agent-spi
  2 测 + skills 2 测全绿，clippy(-D)/fmt/全 workspace build 绿。**剩余**：surface 全面采用
  （用 merge 替换直接 `SkillBuilder::build` 路径、多 capability 合并）留后续；契约+实现+组合原语
  本次落地。
- DONE(部分) P-A4.4 plan→Flow 编译器已落地（提交 `4fa70df`）：`agentflow_agents::dynamic::
  compile_plan_to_flow`——声明式 `WorkflowPlan`（LLM 产的 JSON：`{id,tool,params,depends_on}`）
  编译成真工具调用的 `Flow`，`depends_on`→图依赖（独立步并行、依赖步收 deps 输出）。校验
  重复 id/悬空依赖;环由拓扑排序兜。`dynamic_workflow_plan` 示例进 smoke gate;3 单测覆盖
  diamond DAG + 校验。**+ DynamicWorkflowAgent**（提交 `3e3b5ab`，PR #14）：`plan(goal)` 经 LLM 产 WorkflowPlan、`run(goal)` plan→compile→并行执行;mock-LLM 端到端测试过。docs reality-check 已更新（dynamic workflow = 真库路径）。**剩余 follow-up（P-A4.5）**：接 CLI surface + 支持 `AgentNode` 步 + `PlanExecuteAgent` 改产 Flow。
- DONE P-A4.5 dynamic-workflow CLI surface 已落地：`agentflow workflow dynamic
  --goal <G> --model <M> [--allow-path P]* [--allow-domain D]* [--approve none|cli|
  auto-allow|auto-deny] [--profile dev|production] [--dry-run] [--max-concurrency N]
  [--output text|json]`。复用库路径 `DynamicWorkflowAgent::plan` + `compile_plan_to_flow`
  + `FlowExt`（一次 LLM 规划 → 编译成 Flow → 并发执行）。**治理落点（P-A4.5 关键洞察）**：
  内置工具表（FileTool + HttpTool，shell 不注册）默认挂 restrictive `SandboxPolicy`——
  路径/域名必须经 `--allow-path`/`--allow-domain` 显式授予；`--dry-run` 只打印 plan 不执行；
  `--approve != none` 时经 harness `wrap_registry` 把同一个 `Arc<ToolRegistry>`（planner 与
  compiler 共享）裹上审批/审计管线，**不需先做 P-A2.2**。新文件
  `agentflow-cli/src/commands/workflow/dynamic.rs`（7 单测覆盖 policy/approve/渲染）+
  `tests/workflow_dynamic_tests.rs`（4 个 e2e：dry-run 不执行 / 未授权路径被 sandbox 拒
  且退出非零 / 授权后写入成功 / 缺 --model 报错）。fmt + clippy(-D warnings) + check-arch
  全绿（cli 仅复用既有 agents/harness/tools 依赖，零新边）。剩余 follow-up：plan 支持
  `AgentNode` 步 + 并行 verifier/收敛判定（归入 P-A4.6 文档与后续增强）。
- DONE P-A4.6 文档已更新（分支 `feat/p-a4.6-docs`，接在 #33 上）：`docs/HYBRID_WORKFLOW.md`
  新增 "Dynamic Workflow" 章节（WorkflowPlan→compile_plan_to_flow→FlowRunner 流程图 +
  `DynamicWorkflowAgent` + `agentflow workflow dynamic` CLI + sandbox/approval 治理）+ 改
  intro 三桥 + Current Boundaries。`docs/ARCHITECTURE.md` 四范式 reality-check 刷新（dynamic
  workflow ✅ library+CLI、契约层 ✅ 0 tracked edges、governance shell 正交 ✅）、gaps-map 表更新、
  契约内核图加 `KnowledgeBackend`/`Capability`、L2 加 `nodes-ai`、Axis 2 注明 `Capability` lowering
  + RAG 归位。纯文档,无代码改动。

  **P-A4 收口 ✅ —— 整个 P-A 契约内核轨道全部完成**（0 tracked 依赖违规、空 allowlist；
  四条 runtime/surface 边全烧；RAG 归位 + Capability 降解 + dynamic workflow 产品化）。
  唯一剩余正交治理项 P-A2.2（harness 直接治理 Flow）独立于本轨道收尾。


## S — 沙箱与代码执行安全演进（sandbox & code-execution hardening）

> 来源：2026-07-23 sandbox 复审（对话式代码走查，非全量审计）。范围：
> `agentflow-tools/src/sandbox/*` + `builtin/{shell,script,file}.rs` +
> `agentflow-skills/src/builder.rs` + `agentflow-agents/src/dynamic.rs`。
> 核心结论：双层防御（in-process `SandboxPolicy` + OS `SandboxBackend`）骨架
> 成立、Q1 波次的针对性修复扎实；但 **"谁写的代码、信任级别是什么"这个维度
> 未显式建模**——skill 作者代码与 LLM 现场生成内容在执行路径上未被区分对待，
> 且存在一条未按此威胁模型设防的组合链（见 S0.2）。
>
> 依赖：S0 → S1 →（S2 ∥ S3）→ S4。S0 是小改动大收益的快速修复波，先做；
> S4 需独立 RFC，且仅在明确要支持 LLM code-interpreter 场景时才启动。
> 沿用 Execution Notes / Quality Gates：每项配 regression test，commit
> 引用 task ID（`Refs S0.2`）。

### S0 — 威胁模型定调 + 切断组合链（快速修复波）

- DONE S0.1 RFC：代码来源信任分级（`docs/RFC_CODE_EXECUTION_TRUST.md`，提交
  `7a66123`）：定义三级信任——author-signed（skill 安装时已存在的
  `scripts/`、manifest 声明的 MCP server/plugin 二进制）/ user-provided
  （操作者运行时显式授予，如 `--allow-path`）/ llm-generated（`file.write`
  内容、工具调用参数、dynamic workflow plan step params）。核心原则：**工具的
  执行通道只能解释在注册时（author-signed）或显式授权时（user-provided）就
  已固定的内容；llm-generated 内容可以被存储、可以作为数据传给已受信通道，
  但永远不能自己变成被执行的代码路径。** 推导出两条判据：(1) 一个工具自己的
  执行边界默认值不能泄漏进另一个工具也会读取的 policy 对象；(2) 执行边界按
  信任级别界定，不只按路径——路径 allow-list 只是信任不变量的近似，S1
  （脚本清单+内容 hash）才是精确版本。S1–S4 均引用此 RFC。
- DONE S0.2 切断 file+script 组合链（提交 `65a6c19`）：`build_sandbox_policy`
  （`agentflow-skills/src/builder.rs`）曾把全部工具约束合并成一个共享
  `SandboxPolicy`，`has_script` 时把解释器（python3/bash/node）和 `scripts/`
  隐式注入这个共享对象——不仅 file+script 组合可被 FileTool 写入 scripts/ 后
  由 ScriptTool 执行，file+shell+script 组合更严重：解释器泄漏进 shell 的
  `allowed_commands` 后，ShellTool 的 argv 解析只查命令名不查路径参数，可直接
  `bash <任意 allowed 路径>`，完全绕过 ScriptTool 自身的边界检查。修复：新增
  `finalize_script_policy`（把 script 专属默认值叠加到私有 policy 副本，不写回
  共享对象）+ `exclude_scripts_dir`（file 工具的 policy 无条件排除
  `scripts/`，不论该路径是隐式默认还是显式配置进来的）。回归测试
  `file_plus_script_skill_cannot_write_then_execute_a_new_script`
  证明写入被拒、且已存在的 author-signed 脚本仍可正常执行；skills 117 +
  cli 全测试绿，clippy(-D)/fmt/check-arch 绿。
- DONE S0.3 dynamic workflow 的 registry 同链核查（提交 `4a07b7e`）：审计
  `compile_plan_to_flow`/`DynamicWorkflowAgent`（共享同一
  `Arc<ToolRegistry>`）+ `agentflow workflow dynamic` CLI 的工具表构造——
  确认该 CLI 只注册 `file` + `http`，从不注册 `script`/`shell`/MCP
  （P-A4.5 治理落点原文即如此，本次为其补上机器可验证的回归而非停留在
  "现状描述"）。`compile_plan_to_flow` 本身不做 tool 存在性编译期校验，未注册
  的工具名在执行期以 `Tool not found` 报错——即今天这条链是"因为压根没有
  执行通道"而非"因为 policy 挡住了"被关闭的。新增
  `plan_cannot_chain_a_file_write_into_script_execution`：plan 先
  file.write 写脚本（在授权路径内，写入本身合法且成功），再一步
  `tool:"script"` 尝试执行，断言该步以 `Tool not found: script` 失败、命令整体
  非零退出——把这个"因为没注册"的现状钉成回归，未来若给这条路径加执行类工具
  会立刻被测试打断，倒逼重新审视 S0.2 的防线是否也要搬过来。cli 全测试绿
  （159+集成套件），clippy(-D)/fmt 绿。

### S1 — Skill 脚本完整性（安装时信任 → 执行时信任）— DONE（2026-07-24）

- DONE S1.1 manifest 脚本清单 + 内容 hash（提交 `94994af`）：`SkillManifest`
  新增 `scripts: Vec<ScriptIntegrityEntry>`（`[[scripts]]`，`name` + `sha256`，
  install 时固化）。`SkillLoader::validate` 新分两类问题：**已声明但不对**
  （清单里的文件在磁盘缺失，或内容 hash 对不上）——不论哪个 profile 都是硬
  错误，这是篡改/过期的证据；**压根没声明**（`script` 工具存在但 `scripts`
  清单为空，或 `scripts/` 里有清单没覆盖的可执行文件）——按
  `SecurityProfile` 分级（`Dev` 静默 / `Local` warn / `Production` 拒载），
  走新增 `SkillLoader::validate_with_profile(manifest, dir, profile)`；旧
  `validate()` 签名不变，内部用 `SecurityProfile::from_env()` 解析。
  `SKILL.md` 暂无 `[[scripts]]` 对应的 frontmatter 语法，声明 script 工具的
  SKILL.md skill 一律走"未声明"回退路径。skills loader 测试 22 + builder
  31 全绿（含 3 个新 loader 回归：完整声明零 warning / 未声明按 profile 分级
  / 篡改声明脚本任意 profile 下硬拒）。
- DONE S1.2 ScriptTool 执行前逐文件校验（提交 `49489be`）：`ScriptTool` 新增
  `script_hashes: Option<HashMap<filename, sha256>>` + `with_script_hashes`
  builder；`execute()` 在 canonicalize+policy 检查后、拼解释器之前插入校验——
  脚本名不在 map 里或内容 hash 不匹配 → `SandboxViolation` fail-closed；成功
  时发 `tracing::info!(event="script_integrity_verified", ...)`（复用既有
  observability 模式，未新增 `SandboxStatus` 字段——判定认为"发 trace 事件"
  已满足"校验结果进 trace"的意图，无需为此扩展 OS-sandbox 强度概念）。未配置
  `script_hashes`（`None`）保持历史行为，供 skill-manifest 路径之外的调用方
  用。agentflow-tools 新增 `sha2` 依赖。5 个新单测（含篡改回归 + 未列入回归 +
  未配置时不受影响）+ 既有 89 测全绿。
- DONE S1.3 profile 开关落地为"清单驱动、非 profile 驱动"的执行期 gate
  （提交 `94994af`）：`SkillBuilder::build_registry` 把 `manifest.scripts`
  一路传到 `build_tool_registry`；`script` 分支非空时调用
  `ScriptTool::with_script_hashes`。**与原始设想的差异**（有意为之，见
  `docs/RFC_CODE_EXECUTION_TRUST.md`）：`SecurityProfile` 只决定"未声明清单
  的 skill 能不能被加载"（S1.1，load 时一次性判定），不需要在 `ScriptTool`
  或 `build_tool_registry` 里再查一遍 profile——一旦 skill 声明了
  `[[scripts]]`，不论哪个 profile 都无条件强制执行；清单为空则
  `ScriptTool` 保持宽松（`None`），与 S1.1 未声明分支的"是否允许加载"疊加
  后即得到 dev/local/production 的行为差异，语义比"再造一个
  `ScriptIntegrityDefaults` profile 分支"更简单也更难出现 load 时判了但
  execute 时没管住的缝隙。回归测试
  `declared_script_hash_is_enforced_through_build_registry` 证明端到端（经
  `SkillBuilder::build_registry`，非直接构造 `ScriptTool`）— 先跑通、后篡改、
  再拒绝。skills 全 crate 测试（150+）+ workspace `--lib --bins` 全绿，
  clippy(-D)/fmt/check-arch 绿。

### S2 — Per-skill 依赖环境（支持"项目代码"的能力前置）— DONE（2026-07-24，Python-only，离线波）

> 落地前用 Explore agent 核实了 TODO 原文的三个隐含假设，发现均不成立：**不存在
> "skill 数据目录"概念**（只有内存 db 走 `~/.agentflow/memory/<name>.db` 这种弱
> 绑定 flat 命名空间，install 目录本身是纯 `WalkDir` 拷贝无 hook）；**CLI 里没有
> 任何 install 时确认/capability 门禁机制**（连交互式 prompt 依赖都没有，而且
> `marketplace install` 今天已经在**无确认**的情况下摸网络，先例本身就不一致）；
> `interpreter_for` 现搬到 `builtin/script.rs:397`（TODO 原文行号已过期）。据此
> 与用户对齐后**收窄**：本波只做离线（vendor 目录 + pip 自带 `--require-hashes`
> 锁定，不新造 lockfile 格式）+ Python-only（venv）；"网络安装 + 确认 UX"独立记
> 为 **S2.2b**（见下）；Node/npm 留后续波次。

- DONE S2.1 manifest 声明 Python 依赖（提交 `aecbdb5`）：`SkillManifest` 新增
  `[dependencies] python = "requirements.txt"`（相对路径引用，不是内联依赖
  列表）。`SkillLoader::validate` 强制每条 requirement **必须**同时有 `==`
  精确 pin 和 `--hash=sha256:...`（直接借用 pip 自带的 hash-checking 模式，
  没有另造 lockfile 格式）——不论哪个 profile 都是硬错误（这属于"声明了但不对"，
  不是 S1.1 的"没声明"分支，不做 profile 分级）。支持 pip 风格的反斜杠续行
  语法。9 个新 loader 测试全绿。
- DONE S2.2 安装时/首次加载时离线构建 per-skill venv（提交 `aecbdb5`）：新模块
  `agentflow-skills/src/python_env.rs::ensure_python_venv`——`python3 -m venv
  <skill_dir>/.venv` + `pip install --no-index --find-links vendor/
  --require-hashes -r requirements.txt`，**AgentFlow 自身完全不摸网络**（skill
  自己把 wheel/sdist 放进 `vendor/`）；`.venv/` 与 `scripts/` 同级而非其子目录，
  天然不进 S1.1 的 `[[scripts]]` 扫描范围、也不会被打进 marketplace 签名包。
  幂等：`.venv/.agentflow-lock-sha256` marker 记 requirements 文件 sha256，命中
  则跳过重建。**未做**：TODO 原文"lockfile hash 固化进 S1.1 完整性范围"——判断
  pip 自身的 `--require-hashes` 在 install 时逐包校验已经是等价的强制点，没有
  必要在 S1.1 之上再叠一层重复的 hash 记账。5 个单测（含 1 个真实跑通
  `python3 -m venv` + 手搓 wheel + `pip install --require-hashes` 全链路的
  gated smoke test，`python3` 不在 PATH 时跳过，跟随
  `agentflow-tools/tests/sandbox_linux.rs` 已有的 skip 惯例）。
- DONE S2.3 ScriptTool 解释器指向 per-skill venv（提交 `488b80c` + `aecbdb5`）：
  `ScriptTool` 新增 `python_interpreter: Option<PathBuf>` + `with_python_interpreter`；
  `.py` 脚本改 spawn 该二进制而非全局 `python3`（`.sh`/`.js` 不受影响）；sandbox
  policy 的 `is_command_allowed` 检查**仍然**只认逻辑名 `"python3"`——是否配置
  venv 只改"实际 spawn 哪个二进制"，不改策略语义。`build_script_scope` 把 venv
  根目录加进 OS sandbox 的 read+write 集合。`SkillBuilder::build_registry` 把
  `manifest.dependencies.python` 一路传给 `python_env::ensure_python_venv`
  再喂给 `with_python_interpreter`。**与 TODO 原文的刻意偏离**：不做
  "宿主机全局解释器仅作 fallback（dev profile）"——声明了 `[dependencies].python`
  但装不上（vendor/ 缺失或 hash 不过）在**任何** profile 下都是硬错误，不悄悄退回
  全局解释器（跟 S1.3 的推理一致：声明后没管住是缺陷，不是"还没采用"）。
  agentflow-tools 6 个新测（含手搓 fake interpreter 证明 spawn 目标正确）+
  agentflow-skills 3 个新测（含 1 个经 `SkillBuilder::build_registry` 全链路、
  真实 venv+wheel 的 gated smoke test）。全绿：skills 133 + tools 92 + workspace
  `--lib --bins` 全绿，clippy(-D)/fmt/check-arch 绿。

- DEFERRED S2.2b 网络安装 + 确认 UX（本次未做，非本波范围）
  - 现状：`marketplace install`（`agentflow-cli/src/commands/marketplace.rs`）
    今天已经会摸网络下 artifact，且**零确认**；CLI 里完全没有交互式 y/n prompt
    依赖（`dialoguer`/`inquire` 之类都不是现有依赖）。
  - 目标（留后续）：给"skill 声明的依赖需要联网抓取"这条路径设计一致的确认
    UX——是走新交互 prompt，还是走 `--allow-network`/`--yes` 这类可脚本化 flag
    更符合现有 CLI 风格（无交互式 prompt 先例），并评估是否顺手把
    `marketplace install` 自身的静默摸网络也收进同一套门禁,消除现存的不一致。
    需要先有专门讨论,不在这批里做决定。

### S3 — OS 后端强化（让 `os_sandbox: true` 可默认开启）

> 执行会话最初是 macOS，且既没有可用的 Linux 交叉编译工具链
> （`x86_64-unknown-linux-gnu` target 装了，但 `cargo check --target` 在
> `openssl-sys` 这步因缺 sysroot 失败）也没有本地容器运行时（docker/podman/
> colima/lima 均未安装）——S3.1/S3.2 一度跳过。2026-07-26 与用户对齐后确认
> 本机已装 Apple 官方 `container` CLI（`container system status` 显示
> running），经排查它不是共享内核的轻量命名空间，而是经 Virtualization.framework
> 起的**真实 Linux 6.12.28 aarch64 VM**（cgroups v2 挂载、`cpuset cpu io
> memory hugetlb pids` 控制器齐全）——足以承载真实编译+内核强制验证，遂用
> `container run` 起一个持久开发容器（`agentflow-linux-dev`）完成 S3.1/S3.2。
> S3.3（macOS，本机可测）此前已闭环。

- DONE S3.1 Linux：Landlock 补路径粒度（提交 `5f51279`，2026-07-25）
  - 现状（修复前）：seccomp 是 syscall 粒度（`sandbox/linux.rs` 模块文档自述
    Limits），`FsRead` 无法限制到子树——子进程内任意代码可读宿主机用户可读的
    一切（`~/.ssh`、`~/.aws` 等）；路径限制只在 in-process 层，对子进程内代码
    无效。
  - 落地：`LinuxSeccompBackend` 叠加 Landlock ruleset（`landlock` crate
    0.4，目标 `ABI::V1`，构造时用一次侧效应为零的 `SYS_landlock_create_ruleset`
    探测调用探测内核支持），read-only 授予 `scope.read_paths` +
    `scope.working_directory`，`Capability::FsWrite` 时对 `scope.write_paths`
    授予读写（否则只读）。`enforcement_level()` 改为三态：arch 不支持 →
    `Permissive`；arch 支持 + Landlock 可用 → `Enforcing`；arch 支持但内核无
    Landlock（`CONFIG_SECURITY_LANDLOCK` 未开）→ `Permissive`（复用既有三态
    模式，不是新概念）。`pre_exec` 内 Landlock `restrict_self()` 紧跟在既有
    seccomp `apply_filter` 之后，同一 async-signal-safety 纪律（ruleset 在
    parent 里建好，child 内只做 `restrict_self`）。
  - **环境限定发现**：本次开发容器（Apple `container` CLI 起的 Linux 6.12.28
    VM）内核 `/proc/config.gz` 显示 `CONFIG_SECURITY_LANDLOCK is not set`
    （尽管 `/proc/kallsyms` 里 syscall 符号存在——是 dead/stub，运行时
    `ENOSYS`），即这台机器上 Landlock 实测走的是 `Permissive` 降级分支，不是
    `Enforcing`——`sandbox_linux.rs` 两个新 Landlock 集成测试对此有专门
    skip-guard（`landlock_enforcing()`，检测不到就 `eprintln!` 跳过而非误报
    失败），单测层面（`build_landlock_ruleset_*`）不依赖内核支持,照常跑绿。
  - 回归：`agentflow-tools/tests/sandbox_linux.rs` 新增
    `linux_landlock_blocks_reads_outside_the_allowed_scope` /
    `linux_landlock_allows_reads_inside_the_allowed_scope`；`sandbox/linux.rs`
    单测新增 4 个（ABI 探测、空 scope、真实路径、不存在路径优雅忽略）。容器内
    `agentflow-tools` 全量测试 + fmt/clippy(-D)/check-arch 绿。
- DONE S3.2 cgroups v2 / RLIMIT_* 资源限额（提交 `e9071e0`，2026-07-26）
  - 现状（修复前）：仅 wall-clock 超时（`max_exec_time_secs`）；无内存/CPU/
    pids 限制,fork 炸弹（Exec 授予时）/内存耗尽均不设防。
  - 落地：`SandboxPolicy`/`SandboxScope` 新增 `max_memory_bytes`/`max_pids`/
    `max_cpu_secs`，经 `ShellTool::build_scope_from_policy` /
    `ScriptTool::build_script_scope` 投影进 `SandboxScope`。
    - **Linux**：`max_memory_bytes`/`max_pids` 经 per-spawn cgroup v2 叶子
      节点（`memory.max`/`pids.max`）落地，子进程从 `pre_exec` 内用纯裸
      syscall（`libc::open`/`write`/`close` + 手搓无分配 pid 转 ASCII，不用
      `std::fs`）迁移进去，同一套既有 async-signal-safety 纪律。root 走
      `/sys/fs/cgroup/agentflow`，非 root 走 systemd user-session delegated
      slice（`man systemd.resource-control` Delegation 节的
      `user@<uid>.service` 默认 `Delegate=yes` 路径）。`max_cpu_secs` **不**
      走 cgroup（`cpu.max` 是"每周期配额"式速率上限，语义对不上字段名暗示的
      "累计 CPU 秒数"），两平台统一走 `RLIMIT_CPU`（`setrlimit`）。
    - **macOS**：三个字段统一映射到 `RLIMIT_AS`/`RLIMIT_NPROC`/`RLIMIT_CPU`,
      在 `sandbox-exec` 的 `rewrite_command_with_sandbox_exec`（整体替换
      `Command`）**之后**再挂 `pre_exec`（沿用 S3.3 定下的调用约定：`wrap_command`
      之后才能安全追加 stdio/`pre_exec`）。
  - **环境限定发现**：本次开发容器无 systemd，root cgroup 自身有直接挂载的
    进程（PID1 只是 `sleep infinity`，没有像 systemd 那样把自己隔离进叶子
    scope），触发 cgroup v2 "no internal processes" 规则——对 root
    `subtree_control` 写入返回 `EBUSY`。据此把 `resolve_cgroup_root()` 改为
    每级 `enable_controllers` 调用都检查返回值,任一级失败立即 `None`（而不是
    乐观建出一个永远用不上的叶子 cgroup 再等后续写 `memory.max` 失败）；新增
    `pub fn cgroup_v2_delegation_available()` 探测函数（镜像
    `probe_landlock_abi` 对 Landlock 测试的角色），两个 cgroup 强制测试据此
    skip-guard，本容器内实测确认二者都会 skip（预期内——非 systemd 环境无法
    delegate）。
  - 回归：`sandbox_linux.rs` 新增 3 测试（内存超限 OOM-kill、pids 超限 fork
    炸弹压制、无限额请求正常 spawn，均用测试时 `cc -O2` 编译的极简 C fixture,
    不用 `/usr/bin/yes`——独立验证过 `yes` 的高频 write 循环使 `RLIMIT_CPU`
    杀伐不可靠）；`sandbox_macos.rs` 新增 1 测试
    （`macos_sandbox_enforces_max_cpu_secs_via_rlimit`，直接驱动
    `MacosSandboxExecBackend::wrap_command`，同一 C busy-loop fixture）。
    容器内 `agentflow-tools` 全量测试（103 lib + 9 sandbox_linux 全绿,含
    skip）+ fmt/clippy(-D)/check-arch 绿；macOS host 侧 101 lib + 6
    sandbox_macos 全绿,`cargo build --workspace --features rag,code-chunking`
    无回归。
- DONE S3.3 macOS：profile 适配真实解释器环境（提交见下）：先在本机用手搓
  SBPL profile + `sandbox-exec` 实测复现问题（不是纯代码走查）——venv 的
  `bin/python3` 几乎总是指向 Homebrew Cellar 路径的符号链接（如
  `/opt/homebrew/Cellar/python@3.14/3.14.6/Frameworks/.../bin/python3.14`），
  只放行 venv 目录本身（S2.3 已给）不够，dyld 加载不了解释器自己的运行时库；
  经验证，同时放行 venv 目录 + 解释器 realpath 后的安装前缀两者，脚本才能在
  enforcing 下正常 import 已装依赖。落地：`ScriptTool::execute` 新增
  `resolve_interpreter_real_path`（bare name 走 PATH 查找，已是具体路径的直接
  canonicalize），解出的前缀经 `build_script_scope` 新参数并入 read 集合；
  覆盖 venv 场景与"全局 Homebrew python3 未走 venv"两种情形，不只是 S2 的
  venv 分支。
  **过程中顺手揪出两个与本任务描述无关、但本任务是第一个真正触发它们的
  独立 pre-existing bug**（`ScriptTool` + `os_sandbox` 组合此前没有任何测试
  覆盖过）：
  1. **`build_profile` 从不 canonicalize `SandboxScope` 里的路径**——macOS
     `/tmp`→`/private/tmp`、`/var`→`/private/var`（含 `tempfile::TempDir`/
     `std::env::temp_dir()` 落地的 `/var/folders/...`）均为符号链接，Seatbelt
     `subpath` 按内核实际解析到的路径做字面前缀匹配,用未 canonicalize 的
     `/var/folders/...` 生成的授权在运行时完全不命中——**profile 文本看着
     对，实际全被拒**,此前从未被抓到是因为唯二两个"应该成功"的既有测试都不
     经过这条路径（`ShellTool` 走 `Command::output()` 从不显式设 stdio，
     `ScriptTool` 此前也从没配合 `os_sandbox` 测过）。修复：`macos.rs` 新增
     `canonicalize_for_sbpl`,在写入 profile 前对 `read_paths`/`write_paths`/
     `working_directory` 统一 canonicalize（不存在则原样回退）。这是通用修复,
     不只服务 S3.3——任何未来往 `SandboxScope` 塞临时目录路径的调用方都受益。
  2. **`ScriptTool::execute` 在调用 `wrap_command` 之前就设了
     `stdin/stdout/stderr(piped)`**,而 macOS 的 `wrap_command`
     实现会整个替换掉 `Command` 对象（重指向 `sandbox-exec` 包装二进制）,
     `std::process::Command` 又没有 stdio 的 getter 可以读回——之前设的 piped
     配置被静默丢弃,子进程的 stdout 直接继承到测试进程本身（能在终端看到
     "42"）,但 `ScriptTool` 自己 `cmd.output()` 拿到的是空。修复：把
     `.stdin()/.stdout()/.stderr()` 挪到 `wrap_command` **之后**再设;在
     `SandboxBackend::wrap_command` trait doc 上补了这条调用约定,防止未来任何
     新工具踩同一个坑。
  回归：`agentflow-tools/tests/sandbox_macos.rs` 新增 2 个集成测试（真实
  `python3 -m venv` + 手搓 wheel + `pip install --require-hashes`，enforcing
  下 import 成功；venv 场景下 scope 外读取仍被拒，证明新授权没变成万能后门）+
  `sandbox/macos.rs` 单测 2 个（canonicalize 生效 / 不存在路径原样回退）。
  agentflow-tools 94 测全绿,workspace `--lib --bins` 全绿,clippy(-D)/fmt/
  check-arch 绿。
- DONE S3.4 `os_sandbox` 默认值翻转（提交 `937c0ad`，2026-07-27）
  - **先厘清评估阶段发现的一个关键事实**：`SecurityProfileDefaults.sandboxing.
    require_os_sandbox`（`agentflow-tools/src/security_profile.rs`）和
    `SkillManifest`/`SecurityConfig` 的 `security.os_sandbox`
    （`agentflow-skills/src/manifest.rs`）是**两个独立字段**——前者只被
    `agentflow doctor` 报告读取（`agentflow-config/src/diagnostics.rs:1118`），
    从未真正 gate 任何执行路径；真正决定 `ShellTool`/`ScriptTool` 是否套
    `.with_os_sandbox()` 的是后者（消费点在
    `agentflow-skills/src/builder.rs`）。TODO 原文两者都提，但只有翻转后者
    才有实际行为变化——已与用户对齐，只翻转 `SecurityConfig::os_sandbox`
    的默认值，`SecurityProfileDefaults` 保持不动（仅诊断展示，留给后续按需
    决定是否要接上真实 gate）。
  - 落地：`SecurityConfig::os_sandbox` 默认 `false` → `true`。技术细节：
    `#[serde(default)]`（不带路径）在字段级别只会退回 `bool::default()`
    （`false`），与 `SecurityConfig::default()` 的值无关——所以只改
    `Default impl` 不够，manifest 声明了 `[security]` 表但没提
    `os_sandbox` 键时仍会拿到 `false`。新增 `default_os_sandbox() -> bool
    { true }`，同时接到 `#[serde(default = "default_os_sandbox")]`（字段级）
    和 `Default for SecurityConfig`（整表缺失时）两处，才能覆盖"完全没
    `[security]`"和"有 `[security]` 但没提这个键"两种真实场景。
  - **回归中揪出两个真实的、S3.3 遗留的沙箱授权盲区**（不是测试断言层面的
    小修，是 `build_script_scope` 授权逻辑本身不够）：
    1. Homebrew 装的解释器（如 `brew install bash`）通过
       `<prefix>/opt/<pkg>/...` 符号链接farm 链接*兄弟* Cellar 包的动态库
       （Homebrew bash 运行时需要
       `<prefix>/opt/readline/lib/libreadline.8.dylib`,这库属于完全不同的
       包自己的安装前缀）——S3.3 只授予解释器自己的 resolved 前缀,不够。
       新增 `package_manager_root()`：识别 `<prefix>/Cellar/<pkg>/<version>/
       ...` 这种层级布局,命中时把整个 package-manager root（仍是只读）
       并入授权。
    2. 从 `PATH` 解析出的解释器本身可能是一个 venv shim（比如 shell 里
       激活了某个 venv,plain `python3` 指向 `<venv>/bin/python3`——不同于
       S2.3 显式 `[dependencies]` 配置的那个 venv）,其 canonicalize 后的
       目标是完全不同的系统安装。Python 的 `site` 模块要读的 `pyvenv.cfg`
       在*原始*（canonicalize 前）路径旁边,不是 resolved target 旁边——
       S3.3 只授权 resolved 路径,漏了这个。修复：`resolve_interpreter_real_path`
       改为返回 `ResolvedInterpreter { original, real }`,`build_script_scope`
       两者都要判断,原始路径与 resolved 路径不同时,原始 venv root 也授予
       只读。
    两处都是被真实端到端测试失败揪出来的（`declared_script_hash_is_enforced
    _through_build_registry` / `file_plus_script_skill_cannot_write_then_
    execute_a_new_script` / `skill_init_creates_valid_skill_scaffold`,分别
    对应本机 Homebrew bash 场景和本 Claude Code 会话自身 `.venv` 激活场景）,
    不是靠代码走查猜出来的,证明这两个盲区是真实存在的,不是假设性的。
  - `skill_cli_tests.rs` 两个 sandbox-profile 测试（针对 `rust_expert`——声明
    shell+script 但未设 `[security]`——和 `mcp-basic`——无 sandboxable 工具）
    更新为断言新默认值 `security.os_sandbox = true` 及对应的 notes 文案。
  - 回归：`agentflow-tools`（101 lib + 开发容器内 9 sandbox_linux + 6
    sandbox_macos）、`agentflow-skills`（137 lib + 全部集成测试）、
    `agentflow-cli`（全部测试套件，含 `skill_cli_tests` 29 个）全绿;
    fmt/clippy(-D)/check-arch 绿;`cargo build --workspace --features
    rag,code-chunking` 无回归。

### S4 — LLM code-exec 工具（条件启动：需独立 RFC）

- DONE S4.1 RFC：是否正式支持 LLM 生成代码执行（**采纳**，2026-07-27，
  `docs/RFC_LLM_CODE_EXECUTION.md`）
  - 依赖 S0.1 的信任分级（`docs/RFC_CODE_EXECUTION_TRUST.md`）——RFC 正文
    明确这是该文档"llm-generated 内容永不直接可执行"默认值**唯一**允许被
    抬升的地方,且只在这份独立 RFC 之下抬升。
  - 用户决策：采纳,启动 S4.2。`code_exec` 定为**独立新工具**,不复用
    ScriptTool 的信任模型（skill 作者代码 ≠ LLM 生成内容，二者结构上不共享
    trust tier，见 RFC "Relationship to ScriptTool" 对照表）。
  - RFC 覆盖的 5 条约束（S4.2 实现时是硬约束,不是建议）：(1) `code_exec`
    独立 `Tool` impl,只复用 `SandboxBackend`/`SandboxScope` 机制,不复用
    ScriptTool 的信任分类；(2) 临时工作目录每次调用新建,用后即焚,不与
    `scripts_dir`/其他工具的 `allowed_paths` 重叠,复用 S3.2 的资源限额模式
    做磁盘配额；(3) 默认无网（比 ScriptTool 现有默认更严——ScriptTool 继承
    policy 的 `allowed_domains`，`code_exec` 默认空集,网络访问需显式
    opt-in,且要等 S4.2 的 egress 白名单代理落地才能安全开放）；(4) 产物只
    经工具自己的 `ToolOutput`（`ToolOutputPart::{Text,Image,Resource}`）
    结构化通道带回,不给模型 ambient 文件系统访问；(5) 注册为
    `ToolIdempotency::NonIdempotent`,在 harness production profile 下自动
    走 `wrap_registry`/`HookConfig`（P-H.2）既有审批升级路径,不需要新审批
    机制。
- DONE S4.2 强隔离 `ContainerBackend` + `code_exec` 工具（提交 `422b9d8`
  RFC 文档 + `044a830` 实现，2026-07-27）
  - 落地：`ContainerBackend`（`agentflow-tools/src/sandbox/container.rs`）
    shell 出真实容器引擎——优先 Apple `container` CLI（真实 per-invocation
    Linux microVM，经 Virtualization.framework，本机已验证工作）,否则
    rootless Podman（Linux）。**无部分强制档位**（不同于 seccomp+Landlock
    组合——Landlock 缺失时 seccomp 自己仍是真实强制;容器引擎缺失时没有降级
    可选,`enforcement_level()` 直接 `Disabled`,`wrap_command()` 硬拒绝
    `Err(SandboxError::Unsupported)`)。新工具
    `code_exec`（`agentflow-tools/src/builtin/code_exec.rs`）：v1 仅
    Python（`{code: string}` 参数,内容内联——llm-generated 从不是预先存在
    的文件）,每次调用新建 `tempfile::TempDir`,调用返回时自动销毁,硬编码
    资源限额（256 MiB / 30 CPU 秒 / 32 pids,v1 不做 manifest 可配置）,
    结果只经 `ToolOutput` 带回,注册为 `NonIdempotent`（harness production
    profile 下自动走既有审批升级路径,不需要新机制）。**强制强隔离**：不同于
    ScriptTool/ShellTool 的 opt-in `os_sandbox`,`code_exec` 在
    backend 不 `is_enforcing()` 时直接拒绝执行——且这个检查是显式做的,不能
    只信任 `wrap_command` 的错误路径,因为 `NoopSandboxBackend::wrap_command`
    设计上就是成功且什么都不做（把"要不要拒绝"这个决定留给调用方）——真实
    测试当场抓到这个漏洞：注入 Noop backend 时,若没有这层显式检查,`code_exec`
    会在宿主机上无沙箱直接跑 llm 生成的代码。接入
    `SkillBuilder::build_tool_registry` 新增 `"code_exec"` 分支,**刻意不消费**
    共享 `SandboxPolicy`——不同于 `script` 的 opt-in OS 沙箱,`code_exec` 的
    约束在 v1 里没有一条是作者可配置的。
  - **每条资源限额/网络隔离 flag 都用真实 CLI 验证过,不是从 `--help`
    文本假设的,过程中推翻了好几处假设**：
    1. Apple `container` CLI 不传 `--network` 会挂默认 NAT 网络,拿到完整
       公网访问——必须显式 `--network none`（`--help` 里未文档化,但实测确认
       彻底挡住 DNS/HTTP/裸 socket）。
    2. Linux 内核对 root（uid 0）完全豁免 `RLIMIT_NPROC`——`--ulimit nproc=`
       对 root 进程静默不生效（实测：root 下 `nproc=8:8` 时 50 次
       `fork()` 全部成功;换成 uid 1000 跑同一 fixture,限额正确生效）。
       `code_exec` 因此**始终以非 root uid 运行**。
    3. Apple `container` CLI 有 200 MiB VM 内存下限（`-m 32m` 直接报错
       "minimum memory amount allowed is 200 MiB"）——256 MiB 默认值留出
       余量。
    4. spawn 超时不能等于 `max_cpu_secs`——容器启动开销 + vCPU 调度抖动
       会让真实 CPU-bound busy loop 在 `--ulimit cpu=30:30` 下花费略超过
       30 墙钟秒,与相等的外层超时赛跑。加了 15 秒余量。
    5. `--uid` 是 Apple `container` CLI 专属 flag 名——Podman 要用
       `-u`/`--user`。**这是在 Linux 开发容器里跑真实编译出的 Rust 代码打到
       Podman 时才抓到的**,不是靠猜的。
    6. 有个测试断言"OSError 或低 fork 计数都算证明 pids 限额生效"过于宽松——
       它把上面第 5 条的 `--uid` bug 误判成"限额生效的证据"（因为 `is_error`
       为真,但原因不对）。收紧为检查 fork 相关的具体错误特征。
  - **Linux 验证**（复用 S3 会话搭的 `agentflow-linux-dev` 开发容器,装了
    rootless Podman）：引擎探测、基础执行、网络阻断、pids 限额都经真实编译的
    Rust 代码确认工作。内存限额撞上和 S3.2 完全相同的 cgroup v2 delegation
    缺口（无 systemd、root cgroup 有直接挂载进程）——复用 S3.2 已经建好的
    公开探测函数 `cgroup_v2_delegation_available()` 作为
    `tests/code_exec_linux.rs` 的 skip-guard,而不是让 `code_exec` 静默降级
    隔离保证,也不是另起一套探测逻辑。有真实 cgroup v2 delegation 的宿主
    （比如标准 systemd Linux 服务器）不会撞上这个问题。
  - 回归：`agentflow-tools/tests/code_exec_macos.rs` 新增 8 个测试,证明的是
    真实隔离而不只是编译通过——代码确实跑在独立 Linux 内核里（即使在 macOS
    宿主上,`platform.system()` 报告 "Linux"）、网络彻底阻断
    （DNS/HTTP/裸socket）、宿主文件系统不可达（`/Users` 在容器里不存在）、
    内存/CPU/pids 限额被内核真实强制、调用返回后临时工作目录被清理（这里有个
    真实 flake——用全量系统临时目录 diff 在默认并行测试下和"兄弟测试自己
    还在跑的临时目录"赛跑,用可区分前缀 + `tokio::sync::Mutex` 序列化这个
    文件的测试修复,不能用 std `Mutex`——clippy 正确标记了跨 `await` 持有
    std `MutexGuard`）。`code_exec_linux.rs` 3 个测试覆盖该环境下可验证的
    部分。`agentflow-skills` 新增测试通过真实 skill manifest 注册并调用
    `code_exec`。macOS 上 agentflow-tools 108 lib + 8 macos + 3 linux
    （本机 skip-guard）、agentflow-skills 138、agentflow-cli 全量测试套件
    全绿;Linux 开发 VM 内 110 lib + 3 linux 测试全绿;两端 fmt/clippy(-D)/
    check-arch 绿;`cargo build --workspace --features rag,code-chunking`
    无回归。
  - 范围裁剪（不阻塞字面验收标准,是明确的后续项）：egress 白名单代理未做
    （RFC 明确网络访问在代理落地前必须保持硬关闭,这正是当前状态）；
    bash/node 语言支持留 v2；manifest 级资源限额覆盖留后续；gVisor/
    Firecracker 未评估（本机唯二可用的引擎是 Apple `container` CLI 和
    Podman,两者都已验证）。
  - 与 Deferred "Native dynamic library plugins" 的边界：本项只针对
    `code_exec` 工具的执行后端,不改 plugin runtime（仍是 subprocess
    JSON-RPC）。
  - **收尾追加**（提交 `382a7e1`,2026-07-27）：宣称"完成"后实际跑了一遍
    `agentflow skill inspect --explain-permissions` 针对真实 code_exec
    skill,当场发现三处运维可见性缺口，均已修复：(1) `SkillLoader::
    KNOWN_TOOLS` 没列 `"code_exec"`,声明它的 skill 在 manifest 校验阶段
    就直接报错 "Unknown tool",在 CLI 路径下完全不可用（虽然 registry
    层面已经能注册）；(2) `skill inspect` 的 capability-decision 打印逻辑
    （agentflow-cli 里一份独立于 `agentflow-tools::ToolPermissionSet::
    builtin` 的硬编码 name→capability 映射）不认识 code_exec,报
    "unknown built-in tool, skipped"；(3) `skill inspect` 的 sandbox
    profile 段和 `agentflow doctor` 都只查询 `default_backend()`
    （shell/script 用的 OS 沙箱层）,对 code_exec 用的 `ContainerBackend`
    完全没有可见性。新增独立的 "Code-exec isolation" 展示段（不并进
    os_sandbox 那个 opt-in 语境的 block,因为 code_exec 是强制而非
    opt-in）；`doctor` 新增 `CodeExecReport`,纯信息性展示,不影响
    `DoctorStatus`（code_exec 是 per-skill opt-in,doctor 的全机扫描视角
    看不出有没有 skill 真的声明它）。同时补了 `CLAUDE.md` 里
    agentflow-tools 段落和 "Last Updated" 行（此前落后于 S3/S4 好几个
    版本）。这次是先做完整链路手测才抓到的,不是靠代码走查。
  - **最后一块缺口收口**（提交 `97292bf`,2026-07-27）：此前 harness
    审批集成只验证过通用机制（`production_profile_escalates_non_idempotent_
    tools` 用的是手搓的 `ProbeTool` 测试替身,`code_exec` 自己的单测也只
    检查 `metadata()` 自报的 `NonIdempotent`,没有任何测试真正把 `CodeExecTool`
    本体推过 `wrap_registry`/`HookConfig` 走一遍）。新增
    `production_profile_escalates_code_exec`（`agentflow-harness/src/
    hooks_runtime.rs`）：注册真实 `CodeExecTool`,`Production` profile +
    `AutoDenyApprovalProvider`,断言调用在 `CodeExecTool::execute` 自己的
    强制隔离检查/spawn 逻辑跑之前就被审批闸拦下（deny 而非 allow,故本机
    有没有装容器引擎都不影响这条测试——审批闸短路发生在那些检查之前）。
    `agentflow-harness` 79 lib 测试全绿。至此 S4.2 收尾时列出的三类缺口
    （运维可见性 + harness 审批集成）全部补齐。
  - **第二轮扫尾**（提交 `7cdd6b8` / `b6ab22c`，2026-07-27）：既然连续三次
    在"枚举 shell/script 工具名"的地方漏了 code_exec,索性系统性扫了一遍全仓库
    同类模式,又抓到两处：(1) `agentflow-agents/src/project_memory.rs` 的
    `DeterministicProjectFactGenerator::extract`（L3.1 项目记忆的事实抽取器）
    只匹配 `"shell"`/`"script"`——code_exec 调用成功执行后从未被记成
    `ProjectFact`,不报错,就是安静地漏记,补了 `"code_exec"` 分支（提取
    `code` 参数,和 shell/script 一样原样存,不做摘要,保持这个抽取器
    LLM-free 的既有设计原则）+ 回归测试；(2) 用户面文档
    `docs/SKILLS.md`（"Supported values" 明确写"shell/file/http/script"，
    遗漏 code_exec）和 `docs/ARCHITECTURE.md`（agentflow-tools 那行 crate
    摘要同样没提）——技能作者/新贡献者会真的读这两份文档,已修复,
    `SKILLS.md` 新增一整节 "Code Execution Tool" 参照 "Script Tools" 的
    详细程度。**确认了两处不需要动**：`agentflow-config` 的 workflow YAML
    node factory/schema（`shell`/`script` 在那边是完全独立的 DAG 节点类型
    命名空间,code_exec 设计上就不是 workflow node,这是刻意的范围排除,
    不是遗漏）；`RoadMap.md` 的 S 段描述（该文件全篇没有任何"已关闭"标记
    的先例,连已经全部收口的 P-A 段落都没标,不单独给 S 破例）。
    agentflow-agents 231 + agentflow-memory 7 lib 测试全绿,workspace
    fmt/clippy(-D)/check-arch 全绿。
  - **对抗性代码审计 + 三处真实修复**（提交 `c7c1a10` / `b12fd69`，
    2026-07-27）：前几轮都是"扫同类模式"式的静态排查,这轮换成派 code-reviewer
    agent 对 `cdd7f0d..HEAD`（S3+S4 整段 diff，约 2800 行）做对抗性审计
    （假设攻击者控制 llm-generated 内容、主动想越狱/耗尽资源/外泄数据）。
    三个发现,均先手动复现验证是真问题、再修、再写回归测试证明修复有效——
    不是照审计报告直接改：
    1. **【严重】超时后容器变孤儿**：`code_exec` 的 wall-clock 超时分支只是
       `drop` 掉 `Child` future,`tokio::process::Command` 默认
       `kill_on_drop(false)`;即使设了它也不够——**实测确认**（`SIGKILL`
       杀掉正在跑的 `container run`/`podman run` **客户端进程**后,
       `container list`/`podman ps` 显示容器依然 `running`/`Up`，两个引擎
       都复现了同一问题）。根因：这些 CLI 只是瘦客户端,容器/VM 的生命周期
       由独立运行的引擎自己管,杀客户端进程不等于杀容器。一个只 `sleep()`
       不burn CPU/内存的 payload（两个限额都不触发）会在超时后继续在宿主机
       上无限期运行。修复：`SandboxBackend` 新增 `terminate(&self, scope)`
       方法（默认 no-op——macOS/Linux 的 OS 沙箱后端不需要,它们的 `Child`
       本身就是真正的工作)；`ContainerBackend` 重写它,跑
       `<engine> stop -t 2 <name>`（2 秒宽限期而非两个引擎默认的
       5-10 秒——已经超时的内容不该再白得几秒钟去无视 SIGTERM）；
       `SandboxScope` 新增 `container_name` 字段供 `--name` 使用；
       `code_exec.rs` 生成稳定名字,用一个 RAII guard
       （`TerminateOnDrop`）包住 spawn+wait,除了"干净拿到 wait_with_output
       结果"这一条路径外,所有退出路径（超时、IO 错误、未来任何新增的提前
       return）都会在 Drop 时触发 terminate。新增回归测试
       `code_exec_orphaned_container_is_stopped_on_timeout`：真跑一个
       120 秒 sleep 的 payload,确认 45 秒超时正确触发,再确认
       `container list` 里对应容器真的没了——不是只测超时报错,是测清理
       动作本身生效。
    2. **【中】内存/CPU 限额测试断言过松**：`code_exec_enforces_memory_limit`
       / `code_exec_enforces_cpu_limit` 原先接受"任何 `is_error`"当作限额
       生效的证明——和这次会话已经因为掩盖过一个真实 Podman `--uid` bug 而
       改掉的 pids 测试是同一种毛病。先手动跑一遍分别捕获两种真实失败的
       content 字符串（内存：`MemoryError` traceback；CPU：exit code 137,
       无 Python 层输出),再把断言收紧到检查这些限额特有的信号,而不是任意
       错误。
    3. **【中，代码审查标为"plausible"，验证后确认为真】捕获输出无上限**：
       `wait_with_output()` 把 stdout/stderr 各自攒进不设上限的 `Vec` ——
       `MAX_MEMORY_BYTES` 只约束 VM/容器内部,不约束宿主机自己这个 Rust
       进程的缓冲区。一个只顾疯狂 print、不 sleep（不会撞见发现 1 的超时
       路径,也不一定很快撞上 CPU 限额）的 payload 可以在宿主机上无限膨胀
       内存。改用 `tokio::join!` 并发读 stdout/stderr（各自 64 KiB 上限,
       复用 `agentflow-harness::tasks` 已有的 `DEFAULT_MAX_OUTPUT_BYTES`
       惯例）+ `child.wait()`,`read_capped()` 超过上限后继续排空而非停止
       读（否则子进程会因为管道写满而卡住,把"限制输出"变成"卡到超时"）。
       新增测试 `code_exec_output_is_capped_...`：真的打印 1 MiB,确认
       捕获内容被截到 64 KiB 以内。
    审计过程中也确认了几处**读了但没问题**：cgroup 在 Linux 上的写入顺序
    （父进程里先写限额,子进程 `pre_exec` 里再迁移,没有 TOCTOU）、
    `pre_exec` 的 async-signal-safety、容器 flag 构造的引擎差异化逻辑、
    mandatory-refuse 分支——不是走查一遍就全放行,是每条都对着当前文件内容
    核实过。agentflow-tools 108 lib + 10 code_exec_macos（原 8）测试
    macOS 全绿;Linux 开发 VM 内 110 lib + 9 sandbox_linux + 3
    code_exec_linux 全绿;两端 fmt/clippy(-D)/check-arch 干净。

## L — 长程任务与检索增强（long-horizon & retrieval hardening）

> **L1（L1.1+L1.2）/ L2（L2.1）/ L3（L3.1）已全部 DONE，2026-07-24。**
> 下一波是 **L4**（RAG 检索面补强）或 **L5**（subagent 委托契约），两者互相
> 独立，任选其一拾起。
>
> 来源：2026-07-23 外部课程大纲对照评审（AI Agent 训练营大纲 vs 本仓库的
> gap 分析，对话式评审，非全量审计）。大纲绝大多数主题本仓库已覆盖且更深
> （tool runtime / 治理 / trace / replay / 灰度等不再跟进）；本段只收敛
> **五个确证的能力缺口**。与 S 段正交、可并行；建议 S0 快速修复波之后拾起。
> 方向性暂存项（任务级状态机、模型路由/fallback、缓存层）留在
> `docs/ROADMAP_v2.md` §K，成熟一个晋升一个。
>
> 依赖：L1 / L2 / L3 相互独立；L4 承接 P-A4.2 自留的 chunking refinement；
> L5 坐在 agent-spi 既有 capability 交集合并机制上。L2.1 是 L3.1 写入点的
> 前置。沿用 Execution Notes / Quality Gates：每项配 regression test，
> commit 引用 task ID（`Refs L1.1`）。

### L1 — Dynamic workflow 重规划闭环（plan revision）

- DONE L1.1 失败驱动的 replan loop（提交见下）：`DynamicWorkflowAgent` 新增
  `run_with_replan(goal, max_replans) -> DynamicWorkflowRunOutcome`
  （`{state, plan, revisions}`）。判定"哪些 step 还没搞定"复用了对
  `agentflow-core` 并发执行器语义的精确核实（专门起了个 Explore agent 查证）：
  concurrent 模式下失败节点的下游根本不会进 state map（既不是 key 也不是
  `Err`，是彻底缺席）——所以判据是"state 里没有 `Ok(_)`"而非"state 里是
  `Err`"，两者都算"还没做完"，缺一不可。已成功的 step 结果存进
  `precomputed: HashMap<id, HashMap<String,FlowValue>>`，喂给新增的
  `compile_plan_to_flow_with_precomputed`（`compile_plan_to_flow` 降级为对它
  的薄封装,零调用方改动）——命中 `precomputed` 的 step 编译成新
  `PrecomputedResultNode`（直接回放存好的值，完全不碰 registry，**真正做到
  不重跑**而不只是"重跑但幂等"）。重规划 prompt 明确告诉 LLM 哪些 id 已完成
  （可 `depends_on` 但不能重新定义）+ 哪些失败及原因；下一轮 plan = 已完成
  step（原样，变 precomputed 节点）+ LLM 新提的替换 step，而不是要求 LLM 完整
  重放整个 plan JSON（省 token、也不给 LLM"顺手改坏已成功 step"的机会）。
  `revisions >= max_replans` 时返回最后一次的部分 state,不报错——跟
  `run()` 的"node 失败不算顶层 Err"语义一致。**范围裁剪**（有意为之）：
  "预算"就是 `max_replans` 参数本身,没有接入 `RuntimeLimits`
  （`DynamicWorkflowAgent` 本来就不用那套,不为这一个特性单独引入）；
  "trace 记 plan-revision 事件"用普通 `tracing::info!`
  （`event="dynamic_workflow_plan_revision"`）而非 Harness 信封——从
  `agentflow-agents` 内部往 `HarnessEvent` 发送需要新依赖边,dynamic workflow
  今天的 harness 集成只在 CLI surface 层（`wrap_registry`）,不从 agents crate
  内部发,维持这个边界。
  **顺手挖出一个真实的既有测试基础设施 bug**：`AGENTFLOW_MOCK_RESPONSES`
  （复数,JSON 数组,FIFO 消费）比 `AGENTFLOW_MOCK_RESPONSE`（单数）优先级高,
  而写它的测试助手若不主动清空,会在整个测试二进制的后续任意 mock-model
  测试里残留生效——不清空时全量跑 `cargo test -p agentflow-agents` 会
  随机连累 `plan_execute.rs` 里两个不相关测试失败（三次独立复现,失败的
  具体测试因并发调度而不同）。修复：新增 `MockResponsesGuard`（Drop 时清
  `AGENTFLOW_MOCK_RESPONSES`,即使测试 panic 也会清）,比这个文件里已有的
  "测试末尾手动 `remove_var`"惯例（`plan_execute.rs:1235`）更 panic-safe。
  连续 3 次全量跑 `cargo test -p agentflow-agents`（188 测）零 flake。
  回归：`run_with_replan_reuses_completed_steps_and_recovers_from_failure`
  （mock LLM 两轮,首轮必败节点,断言成功 step 的调用计数跨两轮仍是 1、
  失败 id 不出现在最终 state/plan 里）+
  `run_with_replan_stops_after_max_replans_and_returns_partial_state`
  （持续失败 → 耗尽预算后返回部分 state 而非报错或死循环）+
  `precomputed_step_replays_stored_value_without_touching_registry`
  （隔离验证 precomputed 编译路径,registry 里压根没注册对应工具也能跑）。
  agentflow-agents 188 测 + workspace clippy(-D)/fmt/check-arch 绿。
- DONE L1.2 循环签名检测（死循环识别）（提交见下）：`ReActConfig` 新增
  `loop_detection: Option<LoopDetectionConfig>{window,threshold}`，默认
  `Some({window:6, threshold:3})`——**TODO 原文一个关键假设是错的**：
  `AgentStopReason` **不是** `#[non_exhaustive]`（P-A3.7 当时刻意把它列进
  "保留 exhaustive 的封闭 kind set"，跟 `AgentEvent`/`WorkflowEvent` 不同处理；
  加 `LoopDetected` 变体前先专门起 Explore agent 查证过，`TODOs.md` 这条原文
  "加变体无 ripple" 是过期表述）。加变体触发 4 个 exhaustive match 编译错误
  （`agentflow-server/src/harness_live.rs` / `agentflow-agents/src/eval/
  runner.rs` / `agentflow-cli/src/commands/agent/replay.rs` /
  `agentflow-cli/src/commands/harness/run.rs`）+ `cargo check --workspace`
  又额外揪出 2 个 Explore agent 没扫到的（`agentflow-harness/src/runtime.rs` +
  `agentflow-agents/src/react/agent.rs` 自己的 `answer_from_result`）——6 处
  全部手工补 `LoopDetected` 分支，未走"顺手 seal 成 non_exhaustive"这条路
  （尊重 P-A3.7 当年的刻意选择，不在这个不相关的特性里单方面推翻）。
  检测算法：`LoopState` 新增 `recent_tool_calls: VecDeque<(tool,params)>`
  （单/批两条 dispatch 路径都喂）,在下一轮 `check_turn_limits`（跟
  max_steps/timeout/token budget 同处）里线性扫窗口找"同一 signature 出现
  次数最多的是谁",达到 threshold 即停——不是 F-A2-13 那种只比较"上一次"的
  单槽位比较,能抓住交替模式（A,B,A,B,...）不只是连续重复。**threshold 默认
  设 3 而非 2 是有意为之**：F-A2-13 已经在第 2 次重复时注入 steering note
  给模型一次自我纠正的机会（现有回归测试
  `repeat_tool_call_appends_steering_note_to_memory` 明确断言"两次重复工具
  必须真的跑两次,steering 只是提示不是拦截"），loop detection 的职责是"模型
  没理会提示、继续重复"的兜底,threshold=2 会跟 F-A2-13 的既有语义冲突。
  `serde_json::Value` 没实现 `Hash`，窗口线性扫描（O(window²)，window 是
  个位数）而非哈希去重，判断依据是研究阶段就确认过的"直接 PartialEq 比对，
  别为小窗口引入哈希"惯例。回归 3 个：`loop_detection_stops_before_budget_
  exhausted_on_repeated_identical_calls`（同一 action 无限重复,断言
  `LoopDetected{tool,repeats:3}` 在 `max_iterations=20` 远未耗尽前触发）+
  `loop_detection_catches_alternating_signature_pattern`（A,B 交替 8 次,证明
  不局限于连续重复）+ `loop_detection_can_be_disabled`（`without_loop_
  detection()` 后行为退回纯 max_iterations,验证关闭开关生效）。
  agentflow-agents 191 测（连续 3 次全量跑零 flake）+ agentflow-server 78 +
  agentflow-harness 180 + workspace `--lib --bins` 全绿，clippy(-D)/fmt/
  check-arch 绿。

### L2 — 任务摘要与状态恢复（context-truncation recovery）

- DONE L2.1 agent-loop 级任务摘要 checkpoint（提交见下）：落地前先起 Explore
  agent 核实了 TODO 原文的三个假设，两个不成立——**`ContextItem`/
  `ContextProvider`（"priority 机制"）是纯 Harness 侧概念**（定义在
  `agentflow-agent-spi/src/harness/context.rs`），裸 `ReActAgent`/
  `AgentContext` 完全没有等价机制，`grep agentflow-agents/src/` 零命中；
  **CLI 今天压根没有接上既有的 message-level compaction 机制**
  （`memory_prompt_token_budget`/`memory_summary_strategy` 从未被
  `harness run`/`chat` 设置过）——"resume 后 agent 能答出截断前事实"这个
  场景在**当前 CLI 用法下从未真正发生过**，`--session <id>` 续用今天读的是
  完整原始历史，没有任何压缩。跟用户对齐后确定范围：(1) 生成器走确定性
  提取而非 LLM（跟 `agentflow-harness::ContextSummarizer` 的既有先例一致，
  可插拔，未来可换 LLM 版本）；(2) 注入机制不能只挂 Harness 专属的
  `ContextItem`，要能覆盖裸 `ReActAgent` 场景。
  落地：`agentflow-store-spi` 新增 `TaskSummary`（goal/completed_steps/
  key_results/open_questions/next_steps/updated_at）+ `TaskSummaryStore`
  trait（契约按 TODO 原文明确指示挂 store-spi，这是相对于 `PreferenceStore`/
  `EntityFactStore` 现有先例——那两个契约其实定义在 `agentflow-memory` 实现
  crate 里——的刻意提升，因为 `agent-spi` 已经依赖 store-spi 拿 `Message`，
  让 `TaskSummary` 也挂在这一层能被 agent-spi 直接引用）。`agentflow-memory`
  两个具体实现：`InMemoryTaskSummaryStore`（会话级，配 `SessionMemory`）+
  `SqliteTaskSummaryStore`（持久化，独立 `task_summaries` 表，UPSERT 语义，
  配 `SqliteMemory`）——**没有**把 trait 反向塞进现有 `SessionMemory`/
  `SqliteMemory` 结构体本身：任务摘要持久化是独立、可选的关注点（没开
  compaction 的调用方压根没有摘要要存），跟 `MemoryStore`/`KnowledgeBackend`
  两两独立、由调用方自行组合的既有模式一致。
  `agentflow-agents` 新增 `task_summary` 模块：`TaskSummaryGenerator` trait +
  `DeterministicTaskSummaryGenerator`（镜像 `ContextSummarizer` 设计——
  从被丢弃的 `Assistant`/`Tool` 角色消息里确定性抽取 completed_steps/
  key_results，`open_questions`/`next_steps` 原样透传上一轮摘要不猜测——
  确定性抽取器做不到语义推断，诚实留空好过悄悄编造；跨多轮压缩累加而非
  覆盖，按 `MAX_ENTRIES_PER_LIST=20` 从旧到新裁剪防止无界增长）。
  `ReActAgent` 新增 `with_task_summary_store`/`with_task_summary_generator`；
  `apply_memory_prompt_budget` 压缩丢消息时同步生成+持久化摘要（新增
  `AgentEvent::TaskSummaryUpdated`——`AgentEvent` 本身是 `#[non_exhaustive]`，
  加变体安全，`cargo check --workspace` 复核零连锁）；`build_llm_messages`
  每轮从 store 读一次摘要、注入到系统 prompt 里（system prompt 之后、
  瞬时压缩摘要之前）——**这是"resume 路径注入"决策的实际落点**：不是单独
  挂在 `resume_with_context`（进程内中断恢复，今天 CLI 几乎不触发)，而是
  每轮 `build_llm_messages` 本来就会跑的路径，天然同时覆盖"真正恢复的
  运行"和"复用同一 session_id 开新运行"两种场景，不需要在两条路径分别
  实现一遍。
  Harness `/clear` 对齐：`TaskSummaryStore` 是独立于 `MemoryStore` 的存储,
  `/clear` 现有实现只调 `memory.clear_session()`,天然不碰任务摘要——"原地
  清空、可选保留摘要"这个语义**不用改代码就已经成立**。把 `with_task_
  summary_store` 接进 `harness run`/`chat` CLI 留后续（CLI 连底层 compaction
  机制本身都没接,这个功能今天还是纯库能力,不做投机性 CLI 铺线）。
  回归 6 个：`compaction_persists_a_task_summary`（丢弃消息里的事实进
  key_results）+ **`resumed_agent_still_sees_facts_established_before_
  truncation`**（TODO 原句对应的核心回归——两个完全独立的 `ReActAgent`
  实例,同 session_id 同 store,第二个的 `MemoryStore` 是全新空白（模拟原始
  历史真的没了),`preview_llm_messages()` 仍能看到第一个实例建立的事实)+
  `task_summary_is_a_no_op_when_not_configured`（未配置不 panic 不改变行为)+
  `task_summary` 模块自身 7 个单测（累加/goal 固定不被覆盖/open_questions
  透传/长度裁剪/截断）+ store-spi/agentflow-memory 各自的存储层单测
  （serde round-trip / 会话隔离 / SQLite UPSERT / 断开重连持久化)。
  agentflow-agents 201 测 + store-spi 10 + memory 全绿 + workspace `--lib
  --bins` 全绿，clippy(-D)/fmt/check-arch 绿。

### L3 — 项目级记忆（project memory）

- DONE L3.1 ProjectMemory 层（提交见下）：落地前 Explore agent 核实发现
  TODO 原文的关键假设站不住——**"复用 L2.1 的 TaskSummary 抽取"是范畴错配**：
  `TaskSummaryGenerator` 只在 `apply_memory_prompt_budget` 压缩丢消息时触发
  （运行**中途**、而非"run 收尾时"）,输入只是即将被逐出窗口的
  `Assistant`/`Tool` 消息子集,不是完整 run trace;而 L3.1 要的"run 收尾时提炼"
  钩子**在代码库里完全不存在**（`run_with_context` 唯一的收尾点是
  `TurnStep::Stop(result) => return Ok(result)`,turn-driven 的
  `ReActLoopSession::next_turn` 路径压根不经过这行）。`TaskSummary` 的
  更新语义也是整份覆盖、按 session_id 键;`ProjectMemory` 要的是跨会话
  按 project_key 累加/去重——结构上更贴近本仓库已有的 `EntityFact`/
  `EntityFactStore`（人物实体的结构化事实,`agentflow-memory/src/layer.rs`）
  而非 `TaskSummary`。跟用户对齐后确定：只复用"确定性抽取器 + 可插拔
  generator trait"这个**解法形状**（跟 `ContextSummarizer`→
  `TaskSummaryGenerator`一脉相承）,不复用 `TaskSummary` 类型/store 本身。
  另确认"key 维度 = 项目根路径 hash"在本仓库也是全新概念——没有任何既有
  代码按路径 hash 做存储键（唯一的哈希键先例是 `hash_mcp_servers` 对 MCP
  server 配置 JSON 做 sha256，不是路径）。范围裁剪（跟 L2.1 同一套理由）：
  确定性抽取器只老实记录"这条 shell/script 命令真的跑过",不猜测
  build/test/deploy 分类或技术栈——分类需要判断力,确定性抽取器给不出;
  `memory.project = true` skill manifest 字段 + CLI 接入（8 个
  `SkillBuilder` 调用点分布在 3 个 crate,只有 `harness run`/`chat` 有
  "workspace root" 概念）留后续,本次只做库能力（precedent: L2.1 自己也是
  这么收的）。
  落地：`agentflow-memory` 新增 `project` 模块（**不挂 store-spi**——跟
  `TaskSummary` 的刻意提升相反，这次跟 `EntityFactStore`/`PreferenceStore`
  的既有先例走，因为 `ProjectFact` 今天没有跨 crate 消费方需要在 store-spi
  层引用它）：`ProjectFact{tool, command, first_seen, last_seen,
  observation_count}` + `ProjectMemoryStore` trait（`get_project_facts`/
  `record_project_fact` 语义是 upsert——重复命令累加计数不重复入行/
  `clear_project_facts`）+ `project_key_for_path`（sha256 of 
  canonicalized 路径,新建的约定,已发现没有先例可循）+ 两个实现
  `InMemoryProjectMemoryStore`/`SqliteProjectMemoryStore`（独立
  `project_facts` 表,`PRIMARY KEY (project_key, tool, command)`）。
  `agentflow-agents` 新增 `project_memory` 模块：`ProjectFactGenerator`
  trait + `DeterministicProjectFactGenerator`——扫 `AgentStep` 里的
  `ToolCall{tool: "shell"|"script", params}`,取 `command`/`script`
  字段,run 内去重（run 间去重交给 store 的 upsert）。`ReActAgent` 新增
  `with_project_memory(store, project_key)`/`with_project_fact_generator`；
  **收尾钩子是全新的**——`run_with_context` 的 `TurnStep::Stop` 分支现在
  先调 `record_project_facts(&result.steps)` 再返回（只覆盖
  `run_with_context`/`run`/`run_with_trace`,不覆盖 turn-driven
  `LoopSession` 路径,已在文档里写明这个已知缝隙）；`build_llm_messages`
  每轮读一次累积事实,注入系统 prompt（系统 prompt 之后、L2.1 任务摘要
  **之前**——项目事实比单次会话的任务摘要更"底层"）。
  回归 7 个：`second_agent_sees_project_facts_established_by_first_run`
  （TODO 原句对应的核心回归——第一个 agent 经真实 `run_with_context` 跑
  `cargo build --release`,第二个全新 agent 实例（不同 session、不共享
  内存）仅凭同一 `project_key`+store 就能在 `preview_llm_messages()`
  里看到这条命令,不需要重新探索）+ `project_facts_are_isolated_by_
  project_key`（不同 project_key 互不可见）+
  `project_memory_is_a_no_op_when_not_configured`（未配置不 panic）+
  `project_memory` 模块自身 5 个单测（抽取 shell/script、忽略其他工具、
  忽略非 ToolCall step、run 内去重）+ store 层 7 个单测（sha256 稳定性、
  不同路径不同 key、upsert 计数、跨 project 隔离、SQLite 断开重连持久化）。
  agentflow-agents 209 测（2 次连续全量跑零 flake）+ agentflow-memory 65 +
  workspace `--lib --bins` 全绿，clippy(-D)/fmt/check-arch 绿。

### L4 — RAG 检索面补强（承接 P-A4.2 refinement）

- DONE L4.1 细粒度 / 代码感知 chunking
  - `agentflow-rag/src/chunking/{paragraph,heading,code_ast}.rs`：新增
    `ParagraphChunker`（段落为原子单元，从不切割段落内部，小段落合并到
    `chunk_size`，超大段落单独成 chunk）、`HeadingChunker`（按 markdown
    ATX 标题分节，一节一 chunk，超大节回退到定长子切分并保留父标题
    metadata）、`CodeAstChunker`（Rust 源码，基于 `syn::parse_file` +
    `proc-macro2`(`span-locations`) 按顶层 item 分 chunk：fn/struct/enum/
    impl/trait/mod/const/static/type/use/macro，超大 item 回退定长子
    切分；解析失败是响亮的 `RAGError::ChunkingError`，不静默降级）。三者
    都在自己的扫描过程中直接算出 `start_line`/`end_line` 并写入
    `TextChunk.metadata`——不复用现有 chunker 的 `start_idx`/`end_idx`
    语义，因为发现 `FixedSizeChunker` 用字符偏移、`RecursiveChunker` 用
    字节偏移，两者不一致；新 chunker 完全绕开这个歧义。
  - `code-chunking` 是新增的 Cargo feature（`syn` + `proc-macro2`，均为
    新依赖），非默认开启（避免所有 RAG 消费者被迫链接 syn）；
    `agentflow-skills` 和 `agentflow-cli`（`rag` feature 下）显式打开它，
    否则 `chunk_strategy = "code_ast"` 会在运行时确定性报错——Cargo
    feature 是编译期的，库默认关闭时清单里的这个设置永远不可能生效。
  - `crate::types::ChunkingStrategy` 加 `Paragraph`/`Heading`/`CodeAst`
    三个 variant（`create_chunker` 工厂对应扩展；`CodeAst` 分支在
    `code-chunking` 未启用时返回明确错误而非编译失败）。
  - `Bm25KnowledgeBackend::from_chunked_documents`（新构造器，复用早已
    存在但未被暴露调用的 `BM25Retriever::add_document_with_metadata`）+
    `chunking::chunk_document_for_knowledge_backend`（对单文档分块并打包
    成 `(id, content, metadata)` 三元组，多 chunk 时 id 加
    `#chunk{idx}` 后缀，metadata 补 `source`）。
  - `agentflow-skills`：`KnowledgeConfig` 加 `chunk_strategy` /
    `chunk_size` / `chunk_overlap`（均 `Option`，默认 `None` 保持
    pre-L4.1 整文件索引行为不变）。`register_knowledge_backends` 按条目
    路由：设了 `chunk_strategy` 走分块路径，否则走原整文件路径；两者
    共享同一个 BM25 backend / `rag_search` 工具，LLM 侧无感知差异。
    未知 `chunk_strategy` 字符串是 build 期 `SkillError::ValidationError`
    （不是静默回退）。
  - `rag eval`：`chunk_dataset` 泛化出 `chunk_dataset_with_strategy`
    (`ChunkedDataset` 加 `strategy` 字段)；`chunk_dataset` 保留原签名/
    错误变体作为 `FixedSize` 的薄包装，完全向后兼容。CLI
    `agentflow rag eval --chunk-size N --chunk-strategy <name>`
    新增（默认 `fixed_size`），让同一数据集可按策略跑多次形成对照组。
  - **范围裁剪**（比照 L2.1/L3.1 的"先库能力、CLI/更广接线留后续"模式）：
    `VectorStoreKnowledgeBackend` 本身没有任何 ingestion 路径（今天只能
    搜索已存在的 collection），不属于"换个 chunker"的范畴，未动；
    `agentflow rag ops index` CLI（逐文档整篇 embed）也未接入分块——
    两者都是合理的后续项，不阻塞 L4.1 的字面验收标准（chunking 策略 +
    KnowledgeBackend 参数化 + citable metadata + eval 对照组）。
  - 测试：`agentflow-rag` 每个新 chunker 5-6 个单测（段落不跨切分 /
    小段落合并 / start_line-end_line metadata / 空文本 / try_new 校验 /
    标题分节 / 无标题文档单 chunk / 超大 item 回退子切分并保留父信息 /
    非法 Rust 源码响亮报错），`chunking::tests` 加 `create_chunker` 覆盖
    新策略 + `chunk_document_for_knowledge_backend` 的 id/metadata 行为
    2 个测试，`knowledge::tests` 加端到端测试证明分块索引后搜索命中带
    `source`+`start_line`/`end_line`；`eval::chunking_eval::tests` 加
    `chunk_dataset_with_strategy` 段落策略 + 默认策略回归 2 个测试；
    `agentflow-cli` 加 `parse_chunk_strategy` 覆盖全部 6 个 clap 取值；
    `agentflow-skills::builder::tests` 加字面场景端到端测试——同一份
    manifest 分块后搜索命中收窄到匹配段落（不含无关填充段落，内容明显
    小于整文件），以及未知策略字符串是 build 期错误而非静默回退。
    `agentflow-rag`(--features code-chunking) 195 测、`agentflow-skills`
    135 测、`agentflow-cli`(--features rag) 160 测全绿；
    fmt/clippy(-D,两种 feature 组合)/check-arch/cargo doc 全绿。
    分 3 个提交：`agentflow-rag` chunking 核心、`agentflow-skills` 接线、
    `agentflow-cli` eval 对照组接线。
- DONE L4.2 检索后处理链（rerank + 压缩 + 证据筛选）
  - `agentflow-rag/src/postprocess/mod.rs`（新模块）：`PostProcessor` trait
    （async，`Vec<KnowledgeChunk> -> Vec<KnowledgeChunk>`）+
    `PostProcessorChain`（按序跑一串 `Arc<dyn PostProcessor>`，空链是
    passthrough）+ `PostProcessedKnowledgeBackend`（包一个
    `Arc<dyn KnowledgeBackend>` + chain，自身也实现 `KnowledgeBackend`）。
    选择在 `KnowledgeChunk`（kernel SPI 类型）层面而非字面上"`RetrievalStrategy`
    之后"接线——研究阶段确认 `RetrievalStrategy` 只有
    `VectorStoreKnowledgeBackend` 用，`Bm25KnowledgeBackend` 完全绕开它，
    在 chunk 层面接线才能让同一条链对两种 backend 都生效。
  - `RelevanceScorer` trait（async，给 query+chunks 打分）是 rerank 与
    evidence filtering 共用的注入点：`RerankProcessor`（按分数降序重排）
    和 `RelevanceFilterProcessor`（丢弃低于阈值的 chunk）都基于它构建。
    `ScoreRelevanceScorer` 是零依赖的确定性兜底实现（复用 chunk 自带的
    检索 score）。**关键架构决定**：`agentflow-rag` 没有引入
    `agentflow-llm` 依赖——研究阶段确认这是仓库既有的、被审计文档明确
    认可的"capability crate 互不依赖"纪律（`docs/RFC_CRATE_ARCHITECTURE.md`
    capability→capability 边在 `check-arch` 的 latent-law 列表里，加是要
    刻意决定的架构变更，不是顺手加一个 dep）。所以 LLM rerank（TODO 原文
    要求的目标，cross-encoder 明确留作后续）通过 `RelevanceScorer` 的
    可注入设计支持，具体的 LLM 实现留给已有 `agentflow-llm` 依赖的调用方
    （如 `agentflow-cli`）提供——这是范围裁剪，不是没做：composable chain
    本身完整可用，只是"某个具体 scorer 调 LLM"这一层没有内置。
  - `TruncateCompressor`：确定性字符预算截断（+ 截断标记），是"上下文
    压缩"这条腿的第一版——不需要 LLM；LLM 摘要式压缩留作后续（同样的
    scorer 风格注入点可以后续补）。
  - `agentflow-rag/src/eval/postprocess_eval.rs`（新文件）：
    `PrecomputedRetriever`（`query text -> ranked ids` 的纯查表 `Retriever`
    实现）+ `build_post_processed_retriever`（async 辅助函数：对数据集每条
    query 先跑 base retriever 拿到 id 排名，合成带 `score = 1/(rank+1)`
    的 `KnowledgeChunk`（这个约定复用了 `HybridEval` RRF 融合已经在用的
    倒数排名记分法），过一遍 `PostProcessorChain`，把结果整体烘焙进
    `PrecomputedRetriever`）——用预计算的方式桥接 eval harness 天生同步
    的 `Retriever` trait 和天生异步的 `PostProcessor` chain，不改动
    `runner.rs` 一行代码。
  - `eval::compare` 加 `requires_gain(cmp, metric, threshold_gain,
    threshold_p_value) -> GainDecision`：是 `agentflow-cli` 里
    `evaluate_regression`（"禁止倒退"）的镜像版本（"要求前进"），复用同一套
    `ComparisonReport`/配对符号检验统计机制而不是另起一套。核心技巧：
    `compare()` 自带的 `paired_sign_p_value` 回答"candidate 是否更差"，
    要证明"确实更好"需要反过来问——利用 Binomial(n,0.5) 关于 n/2 对称的
    性质，交换参数调用同一个 `paired_sign_lower_tail_p_value(losses, wins)`
    即可得到"胜率是否显著高于随机"的答案，不需要新写一套 p-value 公式。
  - 测试：`postprocess` 模块 8 个单测（rerank 重排序 / relevance filter
    按阈值过滤 / compressor 只压缩超预算内容 / scorer 返回分数数量不匹配是
    响亮报错而非静默错位 / chain 按序执行且前一步的过滤对后一步生效 /
    空链 passthrough / wrapped backend 应用链并透传 backend name / wrapped
    backend 透传底层错误）；`eval::postprocess_eval` 3 个测试（chain 输出
    正确反映到 retriever / 端到端证明一条真实能纠偏的 rerank chain 在
    `compare()` 视角下确有 recall 增益且 `requires_gain` 判定为
    confirmed / 一条空操作链的 `requires_gain` 判定为 not confirmed——
    这条是"无增益不合入"字面场景的回归测试）；`eval::compare` 加 5 个
    `requires_gain` 单测（两条件都满足才 confirm / 单独指标增益不够 /
    单独胜率不显著 / 目标指标缺失 / 一个"处处获胜"的极端场景验证
    `requires_gain` 与 `compare()` 自身 p-value 极性相反这一关键假设不回归）。
    `agentflow-rag`(--features code-chunking) 211 测（较 L4.1 完工时 +16）
    全绿；fmt/clippy(-D，两种 feature 组合)/check-arch/cargo doc 全绿；
    `agentflow-skills`/`agentflow-cli`(--features rag) 重新 build 确认
    未受影响（只消费 `agentflow-rag` 的既有导出，未使用新增 API）。
  - **范围裁剪**：(a) 具体的 LLM-backed `RelevanceScorer` 实现 + `rag eval`
    暴露一个用它跑对照组的 CLI flag，留作后续——需要在某个已有
    `agentflow-llm` 依赖的 crate（`agentflow-cli` 是最自然的落点）里实现，
    本轮只交付了 library 能力和证明机制；(b) cross-encoder scorer 依 TODO
    原文本就是"作后续可选"；(c) LLM 摘要式压缩（相对于当前的确定性截断）
    留作后续。三者都不阻塞 L4.2 字面验收标准（可组合 post-processor 链 +
    LLM rerank 有可注入接口 + 压缩 + 证据过滤 + eval 能证明增益）。
- DONE L4.3 Query rewrite / decomposition
  - `agentflow-rag/src/rewrite/mod.rs`（新模块）：`QueryRewriter` trait
    （async，一个 query 进，一个或多个 query 出——"改写"是 1→1，"拆分子
    查询"是 1→N，同一个接口形状覆盖两种场景）+ `IdentityQueryRewriter`
    （默认，原样返回）+ `SplitQueryRewriter`（确定性、零依赖，按
    " and "/" or "/","/";"（大小写不敏感）拆分复合查询，无拆分点时原样
    返回，从不返回空列表）+ `MultiQueryKnowledgeBackend`（包一个
    `Arc<dyn KnowledgeBackend>` + rewriter：改写 -> 对每个改写后的 query
    各搜一遍 -> RRF 融合去重，"多路召回合并"这条腿就是这一步）。RRF
    融合复用 `HybridEval` already 在用的同一个配方（`1/(rrf_k+rank)`
    求和），从"融合两个 retriever"泛化成"融合一个 retriever 的 N 个
    query 变体"，融合后的分数会覆盖每个 chunk 原来的单查询分数，供下游
    `postprocess` 链读取真实分数而非过期值。
  - 与 L4.2 同样的架构决定：`agentflow-rag` 不引入 `agentflow-llm`，
    LLM 语义级改写通过 `QueryRewriter` 的可注入设计支持，具体实现留给
    已有该依赖的调用方——`SplitQueryRewriter` 是句法级（非语义级）的
    "拆分子查询"真实实现，不是占位符。
  - `agentflow-rag/src/eval/rewrite_eval.rs`（新文件）：
    `build_multi_query_retriever`，与 L4.2 的
    `build_post_processed_retriever` 同构——预先对数据集每条 query 跑
    改写 + 逐个子查询搜索 + RRF 融合，烘焙进 `PrecomputedRetriever`，
    照样免费复用 `evaluate()`/`compare()`/`requires_gain()`，一行
    harness 代码没改。
  - `agentflow-skills`：`KnowledgeConfig` 加 `query_rewrite: Option<String>`
    （当前仅支持 `"split"`）。`register_knowledge_backends` 用"清单顺序里
    第一个设置了 `query_rewrite` 的 rag 条目生效"这一语义——因为所有
    rag-tier 条目共享同一个 backend/tool（P-A4.2 既有不变量），这个开关
    本质上是技能级而非条目级的；单 rag 条目技能（常见情况）完全没有
    歧义，文档里写清楚了这个语义而不是假装它是条目级的。未知
    `query_rewrite` 字符串是 build 期 `SkillError::ValidationError`。
  - 测试：`rewrite` 模块 10 个单测（identity 原样返回 / split 按 and 拆分
    / 按逗号拆分+大小写不敏感 / 无拆分点原样返回 / 从不返回空 / RRF 融合
    提升多路命中的文档 / 融合分数覆盖而非保留过期值 / 融合尊重 top_k /
    多查询 backend 扇出并融合 / identity rewriter 下行为等价于单次搜索）；
    `eval::rewrite_eval` 2 个测试（构造的 retriever 通过拆分找回分散在两个
    文档里的答案 / 端到端证明改写后 recall 相对未改写基线有确认的增益——
    "eval 数据集加改写前后对照"字面场景的回归测试）；
    `agentflow-skills::builder::tests` 加 2 个端到端测试（复合查询被拆分后
    同时召回两个各自只含一半关键词的文档 / 未知 query_rewrite 是 build
    期错误）。`agentflow-rag`(--features code-chunking) 223 测（较 L4.2
    完工时 +12）全绿；`agentflow-skills` 137 测全绿；fmt/clippy(-D，两种
    feature 组合)/check-arch/cargo doc 全绿。分 2 个提交：`agentflow-rag`
    query rewrite 核心、`agentflow-skills` 接线。
  - **范围裁剪**：LLM 语义级改写的具体实现（相对于 `SplitQueryRewriter`
    的句法级拆分）留作后续，理由与 L4.2 的 `RelevanceScorer` 完全一致——
    不阻塞字面验收标准（改写 + 拆分子查询 + 多路召回合并 + skill manifest
    开关 + eval 改写前后对照，五项全部交付）。
- DONE L4.4 Citation 一致性校验
  - **架构决定（与 L4.1-4.3 不同）**：research 阶段确认——校验一个"已生成
    的最终回答"而非"检索本身"，天然是 agent 循环层面的关注点（需要读
    `AgentStep` 历史找最近一次 `rag_search` 结果），且 `agentflow-agents`
    本来就依赖 `agentflow-llm`（不像 `agentflow-rag` 刻意零 LLM 依赖）。
    所以这次没有把逻辑放进 `agentflow-rag`，而是新建
    `agentflow-agents/src/citation.rs`——并且这次"LLM rerank"式的具体
    LLM-as-judge 实现是**真正交付的**，不是像 L4.2/L4.3 那样又留了一个
    可注入接口给"更上层"（`agentflow-agents` 已经就是那个更上层）。
  - `Citation`（marker/source/content）+ `CitationVerdict`（Supported /
    Unsupported{reason}）+ `CitationChecker` trait（async，一批 citation
    进，一批 verdict 出，形状与 L4.2 `RelevanceScorer::score` 一致，保持
    这几个 L4 子任务的设计语言统一）。`parse_citations_from_tool_result`
    用正则把 `RagSearchTool::render()` 产生的
    `"[n] (source: ..., score: ...)\n<content>"` 文本格式解析回结构化
    `Citation`——research 确认这是目前唯一能拿到 chunk provenance 的地方
    （`ToolOutputPart::Text` 只有裸文本，没有结构化字段）；
    `citations_referenced_in_answer` 只保留答案里真正出现的 marker。
  - `KeywordOverlapCitationChecker`（默认/测试用，零依赖，答案与引用内容
    的词汇重叠率过阈值判 Supported）+ `LlmCitationChecker`（真正的
    "轻量 LLM-as-judge"：`AgentFlow::model(...).prompt(...).json_schema(
    "citation_verdicts", schema).execute()` 一次结构化调用批量判定所有
    引用；判官没返回某个 marker 的 verdict 时该 marker 判定为
    Unsupported——fail closed，不会被静默放行）。
  - `ReActAgent` 加 `with_citation_checker(...)` + `apply_citation_check`
    钩子：候选回答通过 `VerificationStrategy` 门（或没配置校验策略）之后、
    run 真正停止之前触发。判定不通过时：`downgrade_answer` 剥离**答案里
    引用到的全部** marker（不只是不支持的那些——一旦有一个引用错了，整套
    编号对读者就不再可信，这是"降级为无引用回答"字面意思的取舍），并复用
    既有的 `AgentStepKind::Verify`/`AgentEvent::VerificationCompleted`
    记录这次降级——`AgentStepKind` 文档明确写着"设计上是封闭的，复用现有
    variant 而不是新开一个"，所以特意没有新增 step/event 变体。Citation
    校验与 `VerificationStrategy` 的循环重试机制是两条独立路径：验证不通过
    会让 loop 回去重新生成；citation 校验不通过只会降级，不会重试——这是
    刻意的设计分离，不是漏做了重试。checker 报错按 `record_verification`
    同样的哲学处理：非致命，记日志，原样放行原始回答。
  - `CitationAccuracyReport`（`record(&CitationReport)` 累加 + `accuracy()`
    返回 supported/total）就是"Citation Accuracy 指标"——设计成跨任意多次
    校验累加的聚合器，而不是绑死在 `rag eval` 的 `Dataset`/`Judgment` 数据
    模型里（research 确认那个模型纯粹是"query -> ranked doc ids"，完全没有
    generated-answer-text 概念，把 LLM-judge 指标硬塞进去比这次该做的范围
    大得多）。
  - 测试：`citation` 模块 13 个单测（解析 render() 格式 / 按答案引用过滤 /
    关键词重叠 checker 判定支持与不支持 / `verify_citations` 找最近一次
    rag_search 结果 / 答案不引用任何东西时返回 None / 没有 rag_search
    步骤时返回 None / 失败的 rag_search 步骤不被当作可引用 / downgrade
    剥离全部引用而非只剥离不支持的 / verdict 数量不匹配是响亮报错 /
    accuracy 聚合器跨多次 record 正确累计 / 空聚合器 vacuously 1.0）；
    `react::agent` 加 2 个端到端集成测试（mock LLM：调用 rag_search 后
    最终答案引用了一段完全不相关的内容——citation 被剥离、答案文本正确
    降级、记录为失败的 Verify step 和 VerificationCompleted(approved=
    false) 事件；对照组：引用内容确实支持结论时答案原样不变、不产生
    降级 step——这两条是"不通过才降级，通过则不动"字面场景的回归测试）。
    `agentflow-agents` 224 测（较改动前 209 + 13 单测 + 2 集成测试）全绿，
    连续两次跑确认无 flake；fmt/clippy(-D)/check-arch/cargo doc 全绿；
    `agentflow-skills`/`agentflow-harness`/`agentflow-cli` 重新 build 确认
    未受影响。
  - **范围裁剪**：(a) "Citation Accuracy 指标进 `rag eval`"——指标类型本身
    交付了（`CitationAccuracyReport`），但没有把它接进 `agentflow rag eval`
    CLI 子命令或 `agentflow-rag` 的 `EvalReport`/`compare()` 机制，理由见上
    （数据模型不匹配，属于比这次该做的范围更大的改动）；这是留给后续的
    CLI/harness 接线，不是没做核心能力。(b) Citation 校验目前只识别
    `RagSearchTool::render()` 的文本格式（唯一存在的 provenance 载体）；
    如果未来给 `ToolOutputPart` 加结构化 chunk 字段，可以换成更精确的解析
    而不必再靠正则猜文本格式，但这不阻塞当前校验能力的正确性。

### L5 — Subagent 委托契约与结果聚合

- DONE L5.1 结构化 delegation contract + per-subagent capability 收窄
  - **架构决定**：research 阶段确认 Handoff/Blackboard/Debate 三种
    supervisor 模式里没有任何 per-sub-agent 配置结构——调用方手工构建每个
    子 agent 的 `ToolRegistry`，supervisor 从不触碰它。`DelegationSpec`
    因此放进 `agentflow-agent-spi`（而不是 `agentflow-agents`），因为它是
    纯数据契约（不带 `ToolRegistry`/`ReActAgent` 引用），和 `RuntimeLimits`/
    `AgentContext` 一样属于"跑一次调用需要的配置"这层共享词汇表，供任何
    未来的 runtime 复用，不止三个 supervisor 模式。
  - `agentflow-tools::ToolRegistry::narrowed(allowed_tools, allowed_capabilities)`
    ——研究确认 `ToolRegistry` 原本没有任何"从大 registry 切出子集"的
    方法，这是真正新写的部分。刻意复用两个已有机制而不是发明新的合并
    算法：`ToolPolicy::allow_tools` 做工具名子集（在每次 `execute()` 都
    生效，不只是构造时过滤一次），`with_skill_capabilities` 装
    capability 层——这正是 `agentflow-skills/src/builder.rs` 里 Skill
    自己的 `security.tool_permission_allowlist` 早就在用的同一条
    `EffectiveCapabilities::resolve` 交集路径，没有引入第二套合并逻辑。
    narrowed 出的 registry 有自己独立的 audit trail（新的
    `policy_audit`/`capability_audit`），不与 parent 共享。
  - `agentflow-agent-spi::delegation`：`DelegationSpec`（goal / input_context
    / allowed_tools / allowed_capabilities / expected_output_schema /
    timeout_ms / budget_tokens / evaluation_criteria，全部 builder 风格
    `with_*`）+ `validate_output`/`SchemaValidation`（把子 agent 的自由
    文本回答按 JSON 解析后用 `jsonschema::JSONSchema::options()`——与
    `agentflow-tools` 校验工具参数完全相同的调用方式——校验进
    `expected_output_schema`）。`DelegationSpec::narrow_registry` 是对
    `ToolRegistry::narrowed` 的一层自文档化薄包装。
  - `agentflow-agents::delegation`：`build_delegated_agent`（按 spec 收窄
    父 registry 后构造 `ReActAgent`）+ `run_delegated`（把 goal+
    input_context 拼成任务文本，把 spec 的 timeout_ms/budget_tokens 叠加在
    `RuntimeLimits::react_defaults()` 之上，跑完后校验 schema）。刻意做成
    独立原语，不揉进任一 supervisor 的 dispatch 循环——research 确认三种
    supervisor 内部状态机都比较精细，直接改动风险大于收益；任何
    supervisor 或直接调用方现在都能用这两个函数，把 `DelegationSpec`
    字段接进 `HandoffSupervisorBuilder::add_agent` 这类具体 supervisor
    留作更小的后续任务。
  - 回归测试（字面场景，"子 agent 尝试调用 spec 外工具 → 被拒且 trace
    可见"）：mock LLM 驱动一个收窄到 `["echo"]` 的子 agent 仍然尝试调用
    `http`，断言这次调用在 `outcome.result.steps` 里以失败的 `ToolResult`
    step 出现（不是被静默吞掉）。另有 `agentflow-tools` 侧 7 个单测
    （按工具名收窄 / None 保留全部 / 收窄后仍在 registry 里的工具被拒
    / 防御性验证——即使工具意外仍在 registry 里 policy 依然生效 / 装载
    capability 层 / capability 层通过 `EffectiveCapabilities::resolve`
    拒绝未授权能力的工具 / 独立 audit trail）+ `agentflow-agent-spi` 侧
    7 个单测（builder 设置全部字段 / 默认不收窄不校验 / narrow_registry
    委托正确 / 无 schema 时不校验 / 通过校验 / 非 JSON 回答拒绝 / 违反
    schema 拒绝）+ `agentflow-agents` 侧 4 个单测（收窄到指定工具 / 不收窄
    保留全部 / 校验通过的端到端跑通 / 校验失败的端到端跑通）。
  - **重要副产品**：在跑 L5 新增测试时发现 L4.4 引入的两个 citation 测试
    设置了 `AGENTFLOW_MOCK_RESPONSES` 却从未清理，导致该环境变量泄漏进
    进程内下一个持有 `LLM_TEST_LOCK` 的测试——`AGENTFLOW_MOCK_RESPONSES`
    优先于 `AGENTFLOW_MOCK_RESPONSE`，一旦泄漏会静默劫持之后所有用单条
    mock 响应的测试，直到进程退出。表现为 `dynamic::tests::
    llm_plans_then_engine_executes_in_parallel` 确定性失败（不是偶发
    race——单线程跑也复现），根因是测试卫生问题不是并发问题。已用
    `agentflow-agents` 里既有的 `EnvVarGuard` drop-guard 模式修复（独立
    commit，`fix(agents): plug AGENTFLOW_MOCK_RESPONSES leak in L4.4
    citation tests`），并给本次新增的所有 mock-LLM 测试也套上了同样的
    guard，避免重犯。连续 4 次跑（3 次默认并行 + 1 次
    `--test-threads=1`）确认稳定，229/229 全绿。
  - `agentflow-tools`/`agentflow-agent-spi`/`agentflow-agents` 全绿；
    fmt/clippy(-D)/check-arch/cargo doc 全绿；workspace 全量 build 确认
    未受影响。分 3 个 commit：`agentflow-tools` narrowed 原语、
    `agentflow-agent-spi` DelegationSpec 契约、`agentflow-agents` 应用层。
- DONE L5.2 结果聚合与冲突仲裁
  - 精读 TODO 原文后的关键设计决定：仲裁模型不是"自动选一个赢家"，是
    "识别冲突后显式标记交主 agent 复核"（"交主 agent 复核"=hand to main
    agent for review）——比最初设想的"LLM judge 自动仲裁"更保守也更贴合
    字面要求，因此整个聚合原语保持确定性、零 LLM 依赖，延续本次 L4.2/
    L4.3 "库能力优先，具体 LLM 判定逻辑留给已有 LLM 依赖的调用方"的
    一贯设计语言（虽然这里其实不需要 LLM 依赖，纯确定性算法就够）。
  - `agentflow-agent-spi::aggregation`：`SubagentAnswer`（agent_name +
    answer + `SchemaValidation`）+ `aggregate_answers`——去重 key 依赖
    L5.1 的 schema 校验结果：通过 schema 校验的回答按解析后的 JSON
    `Value` 结构相等去重（`{"a":1,"b":2}` 与 `{"b":2,"a":1}` 视为同一个
    答案，字段顺序不同不会造成假冲突）；没过校验或没配 schema 的回答退回
    trim 后的字符串相等。按支持人数降序排序（并列保留先出现顺序，
    `sort_by_key`+`Reverse` 保证稳定排序）；`groups.len() > 1` 即视为
    冲突。`AggregationReport::flagged_for_review()` 返回除第一名（多数）
    外所有分组的 agent 名单；`render_summary()` 生成人类/LLM 可读的报告
    文本，就是"交主 agent 复核"这句话字面要求的交付载体。聚合决策本身
    是纯函数，不碰 `AgentStep`/`AgentEvent`——是否写入 trace、写成什么
    形状，留给调用方决定（与 `DelegationSpec` 本身不带
    `ToolRegistry`/`ReActAgent` 引用的克制风格一致）。
  - `agentflow-agents::delegation` 加 `subagent_answer_from_outcome`
    桥接函数：把 `DelegationOutcome`（L5.1 产出）转成 `aggregate_answers`
    吃的 `SubagentAnswer`，回答缺失（run 没走到 FinalAnswer）时返回
    `None` 而不是构造一个假答案。
  - 端到端回归测试：3 个子 agent 跑同一个带 `expected_output_schema` 的
    `DelegationSpec`（"这个 PR 能不能合"），2 个答"approved:true"、1 个答
    "approved:false"，经桥接函数转换后喂给 `aggregate_answers`，断言
    冲突被正确识别、少数派 agent 被 `flagged_for_review()` 点名、
    `render_summary()` 包含"Conflict"字样——这是 L5.1→L5.2 依赖关系
    （"依赖 L5.1 的输出 schema 约定"）第一次被端到端验证，不只是分别
    测试两层。`agentflow-agent-spi::aggregation` 模块自身 11 个单测覆盖
    空输入 / 文本去重（含空白规整）/ 文本冲突识别与标记 / JSON 结构去重
    忽略字段顺序 / JSON 值不同判冲突 / schema 校验失败时退回文本比较
    （即使碰巧是合法 JSON 也不放心结构化解析）/ 并列时先出现者排前 /
    两种 render_summary 分支（一致 / 冲突）/ 空输入的 render_summary。
  - `agentflow-agent-spi` 57 测、`agentflow-agents` 230 测全绿（连续 3 次
    跑无 flake）；fmt/clippy(-D)/check-arch/cargo doc 全绿；workspace
    全量 build 确认未受影响。分 2 个 commit：`agentflow-agent-spi`
    aggregation 核心、`agentflow-agents` 桥接 + 端到端测试。
  - **范围裁剪**：没有把 `AggregationReport` 接进任何一个 supervisor 的
    trace 记录（如 `DebateSupervisor` 的 `AgentStepKind::DebateVerdict`），
    理由与 L5.1 相同——先交付经过测试的核心原语，具体 supervisor 接线是
    更小、更聚焦的后续任务，且"接进哪个 supervisor、用哪个 trace 变体"
    本身是需要单独决定的设计问题，不该在这一轮里顺带猜一个。

## Recently Closed

- **2026-06-20 — 架构透镜评估 + 文档归位（docs-only）**：新增
  `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`（16 crate 依赖图实证，验证 RFC
  方向成立，给出 R1–R6 修订）；RFC 加 §13 修订附录；`RoadMap.md`（过时 5/14）
  重写为四范式 + 契约内核方向并加 P-A 段；`docs/ARCHITECTURE.md` 加方向横幅；
  P-A0.1/P-A0.2 标 DONE，新增 P-A0.4（完整边图）/ P-A0.5（nodes 拆分决策），
  P-A1 重排为 value-first。
- **2026-06-20 — Q1–Q5 整体外迁**：2026-05-24 深度审计修复波次全部闭环
  （108 DONE / 0 TODO），含 Audit Assessment Summary，归档到
  [`docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md`](docs/archive/TODOs-archive-2026-06-20-q1-q5-audit-remediation.md)。

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

### P-A4.5 — dynamic workflow CLI surface (scoped 2026-06-21)
- 库能力已完整（compile_plan_to_flow #13 + DynamicWorkflowAgent #14，全测试过）。
- CLI surface 是跨 crate 产品 plumbing：clap 子命令 + 工具表构造（复用 `build_agent`/
  SkillBuilder 范式）+ 模型解析 + 输出。**安全要点**：让 LLM 产 plan 再执行工具，必须治理——
  关键洞察：`compile_plan_to_flow` 收 `Arc<ToolRegistry>`，传入 harness `wrap_registry`
  包过的 registry 即自动获得审批/sandbox 治理（经现有 `HookedTool` 组合，**不需先做 P-A2.2**）。
  cli 同时依赖 agents（DynamicWorkflowAgent）+ harness（wrap_registry），是治理化 dynamic
  workflow 的天然落点。
- 建议：fresh session 做；`agentflow workflow dynamic --goal ... [--model M]`，默认用
  harness-wrapped 内置工具表（shell 默认禁用）。
