## Review summary

该 commit 实现了 A1 blog-to-podcast Plan A 薄壳封装示例：在 `examples/applications/blog-to-podcast/` 下新建独立 Cargo 项目，通过自定义 `PodcastNode`（`AsyncNode`）将 phonon-podcast 的完整 pipeline（脚本生成 → TTS → 音频拼装 → SRT）接入 AgentFlow 的 2 节点 DAG。代码质量整体良好：错误映射清晰、测试策略分层（hermetic CLI + `#[ignore]` live smoke）、tracing 完善、clippy/fmt clean。但 Cargo.toml 中的跨 workspace path dependency 结构存在严重的可移植性问题。

## Issues

### 🔴 Critical (必修，会导致问题)

- `examples/applications/blog-to-podcast/Cargo.toml` — phonon 四个 crate 的 path dep 指向了 **repo 外部目录** (`../../../../../rustspace/phonon/...`)。这意味着任何没有将 phonon 仓库放在完全相同相对路径下的开发者/CI 都无法编译该 example。README 也未提及这一外部前置条件。
  - **建议**：至少改为 `git` dep（带 rev/tag）并在 README 中说明；若必须用 path dep，提供 setup 脚本或 `cargo config` 示例；注释中硬编码的 `/Users/hal/.target` 也应改为 `$HOME/.target`。

### 🟡 Important (强烈建议修，可能引入 bug / 设计问题)

- `src/main.rs` — 末尾 `info!(summary = %json!({"status": "ok"}), ...)` 只是为了让 `json!` 宏不被编译器警告未使用，属于 hack。应去掉无意义的 `json!` 调用，或在真正需要序列化 summary 的地方使用。
- `src/podcast_node.rs:render_audio` — `MiniMax`/`Edge`/`OpenAi` 三个分支几乎完全重复（`new() → PodcastPipeline::new() → pipeline.generate()`），仅 TTS 构造器不同。应通过 `Box<dyn TtsProvider>` 或泛型参数统一调用，减少重复并降低后续新增 backend 的维护成本。
- `src/podcast_node.rs` (tests) — `unsafe { std::env::set_var/remove_var }` 直接修改进程级环境变量。虽然注释声称测试默认单线程，但 `cargo test` 可以并行运行；并发修改 env 是 UB 且会导致测试间状态污染。
  - **建议**：使用参数注入或封装一个 `TtsBackend::from_str` 以便在测试中直接传值，而非通过 env。
- `src/podcast_node.rs:override_script_voices_for_tts` — 通过 speaker `name` 字符串精确匹配来覆盖 voice_id。若 LLM 返回的 speaker name 有大小写差异或前后空格（这在 LLM 输出中很常见），voice 不会被覆盖，导致 Edge TTS 使用错误的 voice namespace 而失败。
  - **建议**：使用规范化比较（trim + case-insensitive）或在 `ScriptRequest` 阶段就绑定好 voice_id，避免事后 patch。

### 🔵 Minor (nit / style / 可读性)

- `src/podcast_node.rs:render_audio` — 参数 `output_audio: &PathBuf` 和 `output_srt: &PathBuf` 应改为 `&Path`，更符合 Rust 惯用法。
- `src/main.rs:ReadBlogNode` — `std::fs::read_to_string` 的 `io::Error` 通过 `format!` 转成了字符串，丢失了原始 error kind；example 级别可接受，但若是 core 节点建议保留 cause chain。

## Strengths

- 测试策略合理：3 个 hermetic CLI smoke 保证基础行为，1 个 `#[ignore]` live test 在有 API key 时才跑，CI 无 key 也不会阻塞。
- 错误映射完整：phonon 的 `PodcastError` 被显式映射到 `AgentFlowError::{Configuration, AsyncExecution}`，缺 env var 时提示清晰。
- tracing 埋点到位：`#[instrument]` 在 node execute 上覆盖了 backend、target_segments、language 等字段，便于运维排查。
- `TtsBackend::with_edge_tts()` 提供了零成本的免费 fallback 路径，降低了新用户上手门槛。

## Verdict

🟡 Approve with comments

核心阻塞点是 Cargo.toml 中指向 repo 外部的 phonon path deps，这使得 example 不具备可移植性；修复后可合并。其余为代码质量和可维护性建议。
