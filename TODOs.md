# AgentFlow 下一阶段开发实施计划

最后更新: 2026-05-09

## 维护约定

- 旧执行计划已归档为 `TODOs-archive-2026-05-09-n1-n10.md`。
- 本文件是从 2026-05-09 复评后开始执行的新计划，聚焦把已有能力从“功能闭环”推进到“平台可验证、生态可扩展、生产可部署”。
- `RoadMap.md` 保留中长期路线；本文件只记录近期可执行开发任务。
- 任务状态只使用:
  - `TODO`: 未开始或正在执行。
  - `DONE`: 已完成、已测试、已提交。
- 除非明确要求，完成任务时只修改状态和必要的进度说明，不重写任务内容。

## 当前基线

复评结论:

- 当前 workspace 包含 `agentflow-core`、`agentflow-nodes`、`agentflow-llm`、`agentflow-tools`、`agentflow-mcp`、`agentflow-rag`、`agentflow-memory`、`agentflow-agents`、`agentflow-skills`、`agentflow-cli`、`agentflow-tracing`、`agentflow-viz`、`agentflow-db`、`agentflow-server`、`agentflow-worker`，另有 `agentflow-ui`。
- 默认 feature 下已验证:

```bash
cargo check --workspace --all-targets --target-dir /tmp/agentflow-target
cargo test --workspace --all-targets --target-dir /tmp/agentflow-target
```

- 总体判断:
  - DAG 内核成熟。
  - Agent Runtime / Skill / MCP / Tool / Trace 已形成统一框架。
  - CLI 产品化程度较高。
  - Server / DB / Web UI / Worker 已有基础闭环，但 server run execution、distributed DAG、sandbox 评测、live provider 测试仍是下一阶段重点。

## 总体目标

下一阶段按 P0-P4 推进:

- P0: 平台运行闭环与配置兼容性。
- P1: 安全、Sandbox、Marketplace、Plugin 攻防评测。
- P2: 真实 Provider / 多模态 API live tests。
- P3: 分布式 Worker 与 Web UI 产品化。
- P4: 生态发布、文档收敛、v1 稳定性边界。

---

## P0: 平台运行闭环与配置兼容性

目标:

- 让 `agentflow-server` 的 `/v1/runs` 真正执行 workflow，而不是停留在 stub executor。
- 让 `~/.agentflow/models.yml`、历史 `~/.agentflow/models.yaml`、`~/.agentflow/.env` 的配置约定兼容稳定。
- 让 server、CLI、live tests 都使用同一套模型配置加载逻辑。

### P0.1 配置文件兼容性

状态: DONE

问题:

- 当前代码主路径是 `~/.agentflow/models.yml`。
- 早期项目约定可能使用 `~/.agentflow/models.yaml`。
- live provider tests 需要明确、可覆盖、可诊断的配置加载优先级。

方案:

- 增加模型配置解析优先级:

```text
AGENTFLOW_MODELS_CONFIG
-> ~/.agentflow/models.yml
-> ~/.agentflow/models.yaml
-> built-in default_models.yml
```

- 保持 `~/.agentflow/.env` 作为默认本地 API Key 文件。
- 如果 `models.yml` 和 `models.yaml` 同时存在，使用 `models.yml` 并给出 warning。
- `agentflow config show`、`agentflow config validate`、`agentflow doctor` 显示实际加载的配置文件路径。

子任务:

- [x] 在 `agentflow-llm` 配置初始化路径中支持 `AGENTFLOW_MODELS_CONFIG`。
- [x] 支持 `models.yaml` fallback。
- [x] CLI config/doctor 输出实际配置来源。
- [x] 增加测试覆盖:
  - [x] 只存在 `models.yml`。
  - [x] 只存在 `models.yaml`。
  - [x] 两者同时存在。
  - [x] `AGENTFLOW_MODELS_CONFIG` 覆盖默认路径。
  - [x] `.env` 中 API key 可被加载但不会在输出中泄露。
- [x] 更新 `docs/CONFIGURATION.md`、`docs/SECRET_MANAGEMENT.md`、`docs/LLM_PROVIDERS_MATRIX.md`。

涉及文件:

- `agentflow-llm/src/lib.rs`
- `agentflow-cli/src/commands/config/*`
- `agentflow-cli/src/commands/doctor.rs`
- `docs/CONFIGURATION.md`
- `docs/SECRET_MANAGEMENT.md`

验证:

```bash
cargo test -p agentflow-llm -p agentflow-cli --target-dir /tmp/agentflow-target
agentflow config validate
agentflow doctor --format json
```

验收标准:

- 老用户保留 `~/.agentflow/models.yaml` 也能正常运行。
- 新用户继续使用 `~/.agentflow/models.yml`。
- 所有命令都能说清楚实际加载了哪个配置文件。
- API Key 只从环境或 `.env` 读取，不写入模型配置。

### P0.2 Server 接入真实 FlowRunExecutor

状态: DONE

问题:

- `agentflow-server` 已经有 run routes、DB、SSE、event broker、Web UI，但默认 `RunExecutor` 仍偏 stub。
- `/v1/runs` 需要真正执行 workflow，才能成为平台入口。

方案:

- 新增 `FlowRunExecutor`:
  - 解析 request 中的 workflow YAML。
  - 复用 CLI config-first executor 或抽取共享 executor 层。
  - 调用 `agentflow-core::Flow` 执行。
  - 将 `WorkflowEvent` 经 `WorkflowEventListener` 写入 DB 并推送 SSE。
  - 更新 run 状态: `queued -> running -> succeeded/failed`。

子任务:

- [x] 梳理 `agentflow-cli/src/executor` 与 server 可复用边界。
- [x] 将 config-first workflow 构建逻辑抽成 library 级 API，避免 server 依赖 CLI main 层。
- [x] 实现 `FlowRunExecutor`。
- [x] 支持 `run_dir` 策略:
  - [x] server 默认使用 `AGENTFLOW_RUN_DIR` 或 DB/run-id 派生目录。
  - [x] 每个 run 的 artifacts 路径可查询。
- [x] 将 workflow event 持久化到 `events` 表。
- [x] 将 node 状态映射到 `/v1/runs/{id}/graph`。
- [x] 补充无外部 API 的 HTTP e2e 测试。

涉及文件:

- `agentflow-server/src/runs.rs`
- `agentflow-server/src/events_stream.rs`
- `agentflow-cli/src/executor/*`
- `agentflow-db/src/repo.rs`
- `agentflow-core/src/events.rs`

验证:

```bash
cargo test -p agentflow-server -p agentflow-cli --target-dir /tmp/agentflow-target
cargo run -p agentflow-server
```

手动验收:

```bash
curl -X POST http://localhost:8080/v1/runs \
  -H "Content-Type: application/json" \
  -d @examples/server/fixed_dag_run.json

curl -N http://localhost:8080/v1/runs/<run_id>/events
curl http://localhost:8080/v1/runs/<run_id>
curl http://localhost:8080/v1/runs/<run_id>/graph
```

验收标准:

- 提交 fixed DAG 后 server 真正执行节点。
- SSE 能看到 workflow started、node started、node completed、workflow completed。
- DB 中 run 终态正确。
- Web UI 能显示 DAG 状态变化。

### P0.3 Run Cancellation

状态: DONE

问题:

- 长 workflow / agent run 需要可控停止。
- Agent runtime 已有 cancellation token，但 server API 未形成平台能力。

方案:

- 增加:

```http
POST /v1/runs/{id}:cancel
```

- run cancellation 需要幂等。
- 已完成 run 返回当前终态，不报错。
- running run 收到 cancel 后进入 `cancelled`，并发出事件。

子任务:

- [x] 设计 `RunCancellationRegistry` 或在 executor 中管理 cancellation token。
- [x] Flow/Agent 执行路径接收 cancellation。
- [x] 增加 cancel route。
- [x] 增加 SSE cancellation event。
- [x] 增加 DB 状态更新。
- [x] 增加测试:
  - [x] cancel running run。
  - [x] cancel unknown run。
  - [x] cancel completed run。
  - [x] repeated cancel is idempotent。

涉及文件:

- `agentflow-server/src/runs.rs`
- `agentflow-core/src/flow.rs`
- `agentflow-agents/src/runtime.rs`
- `agentflow-db/src/models.rs`

验证:

```bash
cargo test -p agentflow-server -p agentflow-core -p agentflow-agents --target-dir /tmp/agentflow-target
```

---

## P1: 安全、Sandbox、Marketplace、Plugin 攻防评测

目标:

- 系统化评测 Script/Shell/Plugin/MCP/Marketplace 的安全边界。
- 明确不同平台 sandbox 能力与降级行为。
- 给用户可诊断、可解释的安全报告。

### P1.1 Sandbox 测试矩阵

状态: DONE

问题:

- 当前已有 capability、policy、macOS sandbox、script validation 等基础测试。
- 仍需要更完整的逃逸测试与跨平台测试矩阵。

方案:

- 建立 `agentflow-tools/tests/sandbox_matrix.rs` 或拆分为平台文件。
- 覆盖:
  - path traversal。
  - symlink / hardlink escape。
  - absolute path read/write。
  - `$HOME/.ssh` / `/etc/passwd` 访问尝试。
  - 未允许 command 执行。
  - env secret 泄露尝试。
  - network deny。
  - stdout/stderr 大输出。
  - timeout / killed process。

子任务:

- [x] 定义 `SandboxTestCase` 辅助结构。
- [x] macOS `sandbox-exec` 测试扩展。
- [x] Linux sandbox 测试扩展。
- [x] Noop sandbox 降级测试: 必须输出 warning/doctor risk。
- [x] `ToolPolicyDecision` 记录 deny reason。
- [x] `agentflow doctor` 增加 sandbox capability 报告。

涉及文件:

- `agentflow-tools/src/sandbox/*`
- `agentflow-tools/tests/sandbox_*.rs`
- `agentflow-cli/src/commands/doctor.rs`
- `docs/TOOL_PERMISSIONS.md`

验证:

```bash
cargo test -p agentflow-tools --target-dir /tmp/agentflow-target
agentflow doctor --format json
```

验收标准:

- 越权读写被阻断。
- 不支持强 sandbox 的平台不会静默通过，而是明确提示风险。
- 每次拒绝都有可解释的 capability / policy / sandbox 原因。

### P1.2 Marketplace / Plugin 供应链安全测试

状态: DONE

问题:

- Marketplace 已有 checksum、signature verifier、safe archive unpack。
- 仍需要更系统的恶意包测试。

方案:

- 增加恶意 archive fixtures:
  - absolute path。
  - `..` traversal。
  - symlink。
  - hardlink。
  - duplicate manifest。
  - oversized file。
  - plugin manifest 指向安装目录外 binary。

子任务:

- [x] 扩展 marketplace CLI tests。
- [x] 扩展 remote marketplace cache tests。
- [x] 增加 plugin install 安全校验。
- [x] `agentflow marketplace verify --strict` 设计或实现。
- [x] 文档写明签名策略与 bootstrap checksum verifier 边界。

涉及文件:

- `agentflow-skills/src/remote_marketplace.rs`
- `agentflow-cli/src/commands/marketplace.rs`
- `agentflow-cli/tests/marketplace_cli_tests.rs`
- `docs/MARKETPLACE.md`

验证:

```bash
cargo test -p agentflow-skills -p agentflow-cli --target-dir /tmp/agentflow-target
```

验收标准:

- 恶意 archive 全部拒绝。
- 错误信息说明拒绝原因。
- 签名缺失、签名不匹配、checksum 不匹配的行为可配置且可解释。

---

## P2: 真实 Provider / 多模态 API Live Tests

目标:

- 使用真实 API 验证文本、视觉、多模态、音频、图像生成、视频生成 provider 行为。
- live tests 默认不进入普通 CI，必须显式 opt-in。
- 所有 key 只从 `~/.agentflow/.env` 或环境变量读取。

### P2.1 Live Test 基础设施

状态: DONE

方案:

- 环境变量开关:

```bash
AGENTFLOW_LIVE_LLM_TESTS=1
AGENTFLOW_LIVE_MULTIMODAL_TESTS=1
AGENTFLOW_LIVE_IMAGE_TESTS=1
AGENTFLOW_LIVE_AUDIO_TESTS=1
AGENTFLOW_LIVE_VIDEO_TESTS=1
```

- Provider-specific model override:

```bash
AGENTFLOW_LIVE_STEPFUN_TEXT_MODEL=...
AGENTFLOW_LIVE_GLM_TEXT_MODEL=...
AGENTFLOW_LIVE_STEPFUN_VISION_MODEL=...
AGENTFLOW_LIVE_GLM_VISION_MODEL=...
```

- 默认:
  - 不设置开关时测试 clean skip。
  - 每个测试小输入、小输出、短 timeout。
  - 测试输出不得包含 API Key。

子任务:

- [x] 统一 live test helper。
- [x] 从 `AgentFlow::init()` 加载 `~/.agentflow/.env` 和 models config。
- [x] 增加 provider capability probing。
- [x] 增加 skip reason 输出。
- [x] 增加费用/速率限制文档。

涉及文件:

- `agentflow-llm/tests/provider_consistency_live.rs`
- `agentflow-llm/src/providers/*`
- `docs/LLM_PROVIDERS_MATRIX.md`

验证:

```bash
cargo test -p agentflow-llm --test provider_consistency_live --target-dir /tmp/agentflow-target
AGENTFLOW_LIVE_LLM_TESTS=1 cargo test -p agentflow-llm --test provider_consistency_live --target-dir /tmp/agentflow-target
```

### P2.2 StepFun Live Tests

状态: DONE

测试能力:

- [x] 文本生成。
- [x] streaming。
- [x] native tool calling 或兼容 fallback。
- [x] 视觉理解。
- [x] 图像生成。
- [x] ASR。
- [x] TTS。
- [x] 视频生成，如果 API 支持（当前 StepFun provider 未实现，矩阵标记 `unsupported`）。

验收标准:

- [x] StepFun provider matrix 标记每项能力:
  - `supported`
  - `live_tested`
  - `mock_only`
  - `unsupported`
  - `flaky`

### P2.3 GLM Live Tests

状态: DONE

测试能力:

- [x] 文本生成。
- [x] streaming。
- [x] OpenAI-compatible chat path。
- [x] tool calling，如果支持。
- [x] 视觉理解，如果支持。
- [x] 图像生成，如果支持（BigModel 服务支持，当前 AgentFlow GLM profile 未接入，矩阵标记 `unsupported`）。
- [x] 音频或视频能力，如果当前 GLM 服务支持（BigModel 服务支持独立音频/视频端点，当前 AgentFlow GLM profile 未接入，矩阵标记 `unsupported`）。

验收标准:

- [x] GLM provider 或 OpenAI-compatible provider profile 可稳定调用。
- [x] base URL、model name、capability matrix 文档化。

### P2.4 你需要准备的内容

状态: DONE

请准备:

- [x] `~/.agentflow/.env` 中的真实 API Key:

```bash
STEPFUN_API_KEY=...
GLM_API_KEY=...
```

- [x] 如果有其他 provider，也可以加入:

```bash
OPENAI_API_KEY=...
ANTHROPIC_API_KEY=...
GOOGLE_API_KEY=...
```

- [x] 每个 provider 的 base URL。
- [x] 每个 provider 的可用模型名:
  - [x] 文本模型。
  - [x] 视觉/多模态模型。
  - [x] 图像生成模型。
  - [x] ASR 模型。
  - [x] TTS 模型。
  - [x] 视频生成模型。
- [x] 官方最小调用样例或文档链接。
- [x] 是否允许 live tests 产生真实费用。
- [x] 单轮手动测试预算上限。
- [x] 是否允许把 live tests 放入 nightly CI；默认建议不放入普通 CI。

建议本地配置样例:

```yaml
# ~/.agentflow/models.yml 或 ~/.agentflow/models.yaml
providers:
  stepfun:
    api_key_env: STEPFUN_API_KEY
    base_url: https://api.stepfun.com/v1
  glm:
    api_key_env: GLM_API_KEY
    base_url: https://open.bigmodel.cn/api/paas/v4

models:
  stepfun-text:
    provider: stepfun
    model: replace-with-real-text-model
  glm-text:
    provider: glm
    model: replace-with-real-text-model
```

---

## P3: 分布式 Worker 与 Web UI 产品化

目标:

- 把 distributed foundation 推进到可演示的 2-worker DAG 执行。
- 让 Web UI 从 trace debugger 扩展到基本 run console。

### P3.1 gRPC Worker Adapter

状态: DONE

问题:

- 当前 `WorkerProtocol`、`WorkerControlPlane`、`agentflow-worker` 已存在。
- 远程 tonic adapter 和真实 DAG scheduler 接入尚未完成。

方案:

- 使用 tonic 实现:
  - `Heartbeat`
  - `ClaimTask`
  - `ReportResult`
  - 后续可加 trace streaming。

子任务:

- [x] 定义 proto。
- [x] 实现 server-side adapter。
- [x] 实现 worker client adapter。
- [x] 保留 in-memory adapter 作为单元测试默认实现。
- [x] 增加 2-worker integration test 或本地 smoke script。

涉及文件:

- `agentflow-server/src/scheduler/*`
- `agentflow-worker/src/*`
- `docs/DISTRIBUTED.md`

验证:

```bash
cargo test -p agentflow-server -p agentflow-worker --target-dir /tmp/agentflow-target
```

验收标准:

- 两个 worker 可连接同一 control plane。
- worker 可 claim task 并 report result。
- control plane 能 stitch trace。

### P3.2 Distributed DAG Scheduler

状态: DONE

方案:

- 控制面维护 DAG ready-set。
- ready node -> `WorkerTask`。
- worker result -> 写回 state pool。
- 下游依赖满足后继续派发。

子任务:

- [x] 定义 node execution payload schema。
- [x] 支持 template/file/mock nodes 的 remote execution。
- [x] 支持 failure/retry。
- [x] 支持 heartbeat lost 后 requeue。
- [x] 100+ node DAG 两 worker 验收。

验收标准:

- 2 worker 集群执行 100+ mock/template node workflow。
- trace 可跨 worker 拼接。
- retryable failure 可重试。

### P3.3 Web UI Run Console

状态: DONE

问题:

- 当前 Web UI 更接近 debugger。
- 平台化需要提交 run、查看 run、取消 run、看事件和图。

子任务:

- [x] Run submission form。
- [x] Run cancellation button。
- [x] Auth token 配置入口或说明。
- [x] Provider/config status panel。
- [x] DAG node detail panel。
- [x] Agent step / tool policy detail panel。
- [x] Live event reconnect。

涉及文件:

- `agentflow-ui/src/main.tsx`
- `agentflow-ui/src/styles.css`
- `agentflow-server/src/ui.rs`
- `docs/WEB_UI.md`

验证:

```bash
cargo test -p agentflow-server ui::tests --target-dir /tmp/agentflow-target
cd agentflow-ui && npm test
```

---

## P4: 生态发布、文档收敛、v1 稳定性边界

目标:

- 明确哪些 API/manifest/schema 是稳定扩展点。
- 发布官方 Skill/Plugin/Marketplace 示例。
- 避免文档与代码状态漂移。

### P4.1 v1 稳定接口清单

状态: DONE

需要冻结或标注稳定级别:

- [x] `AsyncNode`
- [x] `FlowValue` checkpoint schema
- [x] `Tool`
- [x] `ToolMetadata`
- [x] `AgentRuntime`
- [x] `AgentStep` / `AgentEvent`
- [x] `SKILL.md` frontmatter
- [x] `skill.toml`
- [x] Plugin manifest
- [x] Marketplace manifest
- [x] Trace schema
- [x] Server REST API envelope

产出:

- [x] `docs/STABILITY.md`
- [x] `docs/API_COMPATIBILITY.md`

### P4.2 官方生态样板

状态: TODO

建议准备:

- [x] 3 个官方 Skills:
  - [x] code-reviewer
  - [x] research-assistant
  - [x] multimodal-content-analyzer
- [ ] 2 个官方 Plugins:
  - [ ] echo/plugin smoke
  - [ ] data-transform plugin
- [ ] 1 个 remote marketplace 示例。
- [ ] 1 个 hybrid demo:
  - DAG + Agent + MCP + RAG + Trace + Web UI。

验收标准:

- 新用户按教程能完成 install -> inspect -> run -> trace replay -> Web UI 查看。
- 所有示例默认可用 mock/offline 模式。
- live provider 模式可选启用。

### P4.3 文档收敛

状态: TODO

问题:

- 项目快速推进后，旧评估和旧 roadmap 容易与代码漂移。

方案:

- 保留历史报告，但新增当前权威状态文档。
- 每轮大变更后只更新一个入口文档，避免多处冲突。

子任务:

- [ ] 新建或更新 `docs/CURRENT_STATUS.md`。
- [ ] 在 `README.md` 指向 `docs/CURRENT_STATUS.md`。
- [ ] 标注 `PROJECT_EVALUATION_2026-05-01.md` 为历史报告。
- [ ] `RoadMap.md` 只保留未来路线，不再混入已完成实施细节。
- [ ] `TODOs.md` 只保留短期执行队列。

---

## 推荐执行顺序

1. P0.1 配置兼容性。
2. P0.2 Server 真实执行 workflow。
3. P0.3 Run cancellation。
4. P1.1 Sandbox 测试矩阵。
5. P1.2 Marketplace / Plugin 安全测试。
6. P2.1 Live test 基础设施。
7. P2.2 / P2.3 StepFun + GLM live tests。
8. P3.1 gRPC Worker Adapter。
9. P3.2 Distributed DAG Scheduler。
10. P3.3 Web UI Run Console。
11. P4 稳定性、生态样板、文档收敛。

## 你需要准备的工作

请准备以下信息，但不要提交到仓库:

- [x] 本地 `~/.agentflow/.env`。
- [x] StepFun API Key。
- [x] GLM API Key。
- [x] 每个 provider 的 base URL。
- [x] 每个能力对应的模型名:
  - [x] 文本。
  - [x] 视觉/多模态。
  - [x] 图像生成。
  - [x] ASR。
  - [x] TTS。
  - [x] 视频生成。
- [x] 官方最小调用样例或文档链接。
- [x] live test 是否允许产生费用。
- [x] 单次手动 live test 预算上限。
- [x] 是否允许 nightly CI 使用真实 key；默认建议只本地手动跑。

本地建议:

```bash
chmod 700 ~/.agentflow
chmod 600 ~/.agentflow/.env ~/.agentflow/models.yml 2>/dev/null || true
```

## 质量门禁

每个开发任务完成前至少运行相关子集测试。阶段完成时运行:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets --target-dir /tmp/agentflow-target
```

涉及 live provider 的任务默认不进入普通 CI，使用显式 opt-in:

```bash
AGENTFLOW_LIVE_LLM_TESTS=1 cargo test -p agentflow-llm --test provider_consistency_live --target-dir /tmp/agentflow-target
```
