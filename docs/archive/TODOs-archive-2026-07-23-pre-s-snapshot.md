# AgentFlow TODOs

Last updated: 2026-06-20

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

Current focus: **Q-段（2026-05-24 审计修复）已全闭环并外迁** → 下一波是 **P-A 契约
内核演进**（安全/正确性前置已清，P-A0 守卫已就位）；**H** 为可选收尾打磨 backlog。

| Segment | Theme | Status |
| --- | --- | --- |
| P0 → P9 / P-H / P-LLM / M / P10 | 历史段，全部 closed 或外迁 | ARCHIVED |
| Q1 → Q5 | 2026-05-24 深度审计修复波次（安全 / 正确性 / 产品化 / 文档 / 横切），108 DONE | **DONE — archived（6/20 外迁）** |
| **H** | **Harness Mode follow-ups**（RFC loop-ownership + `harness chat` 收尾/打磨） | **active — backlog（可选）** |
| **P-A** | **契约内核 + 架构演进**（dynamic workflow 统一；见 `docs/RFC_CRATE_ARCHITECTURE.md`） | **active — backlog（next）** |
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

- DONE H.2.1 让 `--approve cli` 在 chat 里真正可用
  - 现状：chat REPL 独占 stdin,`CliApprovalProvider`(阻塞 std stdin)会和它
    抢字节,所以 `harness chat --approve cli` 被**启动守卫拒绝**(PR #3)。
  - 目标:实现一个从 REPL 共享行读取器取输入的自定义 `ApprovalProvider`,
    审批 prompt 走同一个 stdin 通道(channel),解除守卫,交互审批在 chat 可用。

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
