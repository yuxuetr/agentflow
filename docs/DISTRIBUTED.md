# Distributed Scheduling

AgentFlow's distributed scheduler is being introduced behind a transport-neutral
protocol boundary. The first implementation milestone defines the control-plane
contract and keeps the existing `/v1/runs` behavior unchanged.

## Scope

The distributed control plane has four responsibilities:

- Submit DAG node work as worker tasks.
- Let worker processes claim tasks when they have capacity.
- Accept terminal task results and trace fragments from workers.
- Track worker heartbeats so stale work can later be retried or marked failed.

The protocol lives in `agentflow-server/src/scheduler/` as `WorkerProtocol`.
It is intentionally independent of HTTP routing and database persistence so the
transport can evolve without changing run submission semantics.

## Transport Choice

The selected v1.0-rc transport is **gRPC with tonic**.

Rationale:

- It gives a strongly typed API contract for worker binaries.
- HTTP/2 streaming fits task claim loops, heartbeat streams, and trace fragment
  upload without introducing a separate message broker.
- It keeps local and Kubernetes deployments simple because workers only need to
  reach the `agentflow-server` control plane.
- NATS and Redis Streams remain viable adapters for larger installations, but
  they add external infrastructure that is not required for the first distributed
  milestone.

The current code records this selection as
`agentflow_server::SELECTED_TRANSPORT == WorkerTransport::Grpc`.

## Protocol Contract

`WorkerProtocol` defines the durable semantics every transport adapter must
preserve:

```rust
#[async_trait]
pub trait WorkerProtocol: Send + Sync {
  async fn submit_task(&self, task: WorkerTask) -> Result<(), SchedulerError>;
  async fn claim_task(&self, worker_id: WorkerId) -> Result<Option<WorkerTask>, SchedulerError>;
  async fn report_result(
    &self,
    worker_id: WorkerId,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), SchedulerError>;
  async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), SchedulerError>;
}
```

`WorkerTask` is the unit of scheduling. It carries `task_id`, `run_id`,
`node_id`, `attempt`, and an opaque JSON payload. The payload will hold the
serialized node execution request once the Flow scheduler is wired in.

`WorkerTaskResult` is terminal: either `Succeeded { output, events }` or
`Failed { error, retryable, events }`. The `events` field carries worker-local
trace fragments so the control plane can persist and later export one coherent
run trace.

`WorkerHeartbeat` records liveness, current active task, free slots, and a
timestamp. Later retry logic will use missed heartbeats to decide whether to
requeue in-flight tasks.

## Current Implementation

`InMemoryWorkerProtocol` is a single-process implementation for unit tests and
local prototyping. It provides FIFO claims, claiming-worker validation on result
submission, and heartbeat snapshots.

It is not durable and must not be used as a multi-process scheduler. Its purpose
is to keep protocol semantics covered without requiring a network service.

The first tonic adapter is now available in `agentflow-server`:

- `proto/agentflow/scheduler/v1/worker.proto` defines `SubmitTask`,
  `ClaimTask`, `ReportResult`, and `Heartbeat`.
- `WorkerControlServer` exposes any `WorkerControl` implementation as a gRPC
  service.
- `WorkerControlPlane<P>` implements `WorkerControl`, so remote worker results
  still pass through assignment validation, run counters, and stitched trace
  aggregation.
- `GrpcWorkerProtocol` implements the same `WorkerProtocol` trait from the
  client side and is used by worker processes.

`agentflow-worker` is now a minimal worker process and library runtime. The
runtime is protocol-agnostic and performs one loop of:

1. send heartbeat;
2. claim a task;
3. execute the current stub node runner;
4. report a terminal result with worker-local trace fragments.

The binary supports local smoke tests against `memory://local`:

```bash
cargo run -p agentflow-worker -- --once --worker-id worker-a
```

It can also connect to a gRPC control-plane endpoint:

```bash
cargo run -p agentflow-worker -- \
  --control-plane grpc://127.0.0.1:50051 \
  --worker-id worker-a
```

`WorkerControlPlane<P: WorkerProtocol>` is the server-side scheduling façade
that sits above the protocol. It currently provides:

- `schedule_task`: submit a task and mark the run queued.
- `claim_task`: let a worker claim FIFO work and record the assignment.
- `report_result`: validate ownership through the protocol, then aggregate
  successful outputs, failed task counts, retryable failure counts, and worker
  trace fragments on the run snapshot. The control plane also produces
  `StitchedWorkerTraceEvent` entries with global sequence, worker id, task id,
  node id, attempt, local sequence, kind, payload, and stitch timestamp. The
  same snapshot can be converted into `agentflow-tracing::OtelSpan` values with
  `WorkerControlPlane::stitched_otel_spans(...)`: one distributed-run root span
  plus one child span per task attempt, with worker-local fragments preserved as
  span events.
- `heartbeat`: record worker liveness and free-slot capacity.

`DistributedDagScheduler<P: WorkerProtocol>` sits above the control plane for
the first executable distributed DAG milestone. It parses config-first DAGs
into ready sets, emits one `WorkerTask` per ready node, gathers mapped inputs
from completed upstream outputs, and folds worker results back into a state
pool.

### Supported `NodeExecutionPayload` node types (P2.8)

The portable `NodeExecutionPayload` schema covers the following node types.
The same dispatcher is used by the local `DistributedDagScheduler` smokes and
the standalone `agentflow-worker` binary.

| `node_type` | Dispatcher | Test coverage |
|-------------|------------|---------------|
| `template`  | `agentflow_nodes::nodes::template::TemplateNode` | `run_once_executes_distributed_template_payload` |
| `file`      | `agentflow_nodes::nodes::file::FileNode` | `run_once_executes_distributed_file_payload` |
| `mock`      | inline (`fail_until_attempt` / `fail` / `value`) | `distributed_scheduler_runs_100_mock_nodes_with_two_workers`, retry + heartbeat tests |
| `llm`       | `agentflow_nodes::nodes::llm::LlmNode` (uses `agentflow-llm` registry + mock provider in tests) | `dispatch_llm_and_agent::llm_payload_returns_mock_response` |
| `http`      | `agentflow_nodes::nodes::http::HttpNode` | `dispatch_simple::http_payload_routes_to_http_node_dispatcher` |
| `mcp`       | `agentflow_nodes::nodes::mcp::MCPNode` (stdio server) | `dispatch_simple::mcp_payload_routes_to_mcp_node_dispatcher` |
| `agent`     | minimal `agentflow_agents::react::ReActAgent` loop, empty tool registry | `dispatch_llm_and_agent::agent_payload_runs_react_loop_to_completion` |

Unknown `node_type` values produce a non-retryable
`AgentFlowError::FlowDefinitionError` so a typo in the YAML never silently
hot-loops the worker pool. `dispatch_simple::unsupported_node_type_returns_structured_failure`
locks this in.

The distributed `agent` dispatcher today runs a deliberately minimal ReAct
loop: the worker reads `message` / `model` / optional `persona` /
`max_iterations` from the gathered inputs and runs against a fresh
`SessionMemory` plus an empty `ToolRegistry`. Richer tool distribution
(shared sandbox + admission) rides on the same `NodeExecutionPayload`
plumbing once P5.5 worker admission lands.

The scheduler includes bounded retry for retryable worker failures and stale
heartbeat requeue for claimed tasks whose worker heartbeat exceeds the
configured timeout. The 100-node two-worker smoke test verifies task claim,
result reporting, state-pool completion, and stitched trace aggregation.

The snapshot is still in-memory. The next persistence step is to project these
state transitions into `runs`, `steps`, and `events` rows so `/v1/runs` and SSE
streams can observe real distributed execution.

## Planned Control-Plane Flow

1. `POST /v1/runs` persists a queued run as it does today.
2. A distributed run executor parses the workflow and uses
   `DistributedDagScheduler` to emit ready DAG nodes as `WorkerTask`s.
3. Workers connect to the control plane and call `claim_task`.
4. The control plane marks claimed steps as running and records worker ownership.
5. Workers execute node work in-process, then call `report_result`.
6. The control plane persists outputs, appends trace events, advances dependent
   nodes, and eventually marks the run succeeded or failed.

## Worker Admission (P5.5)

The control plane gates every authenticated worker call through
`AuthenticatedControlPlane`, which sits in front of `WorkerControlPlane`
and consults a [`WorkerAdmissionPolicy`](../agentflow-server/src/scheduler/admission.rs).
The policy decides three orthogonal questions before letting a worker
heartbeat, claim a task, or report a result.

| Knob | Type | Default | Notes |
|------|------|---------|-------|
| `allowed_workers` | `Option<HashSet<WorkerId>>` | `None` (any worker) | When `Some`, only listed worker IDs are admitted. |
| `pre_shared_keys` | `HashMap<WorkerId, HashSet<String>>` | empty (no PSK required) | Each worker may have **multiple valid PSKs** to support overlap-add-then-remove rotation. |
| `jwt` | `Option<JwtPolicy>` | `None` | Global JWT verification policy (issuer / audience / key pool / leeway). Only consulted for workers in `jwt_workers`. P10.16.1. |
| `jwt_workers` | `HashSet<WorkerId>` | empty | Workers that authenticate with a JWT against `jwt` instead of a PSK. A worker in both `pre_shared_keys` and `jwt_workers` is a config error; PSK is treated as authoritative to avoid silent downgrade. P10.16.1. |
| `max_workers` | `Option<usize>` | unbounded | Cap on distinct admitted workers. Re-admitting an existing worker is a no-op (idempotent). |
| `max_concurrent_tasks_per_worker` | `Option<u32>` | unbounded | Cap on simultaneously-claimed tasks per worker. Enforced inside `claim_task`. |

Rejection paths map onto closed `AdmissionError` variants
(`UnknownWorker`, `MissingCredential`, `InvalidCredential { reason }`,
`WorkerFleetExhausted`, `WorkerQuotaExhausted`). `InvalidCredential.reason`
carries the verifier-specific message — for PSK it's typically
`"psk did not match any rotation entry"`; for JWT it's the
`JwtVerifyError` `Display` output (issuer mismatch, audience mismatch,
expired, etc.). The gRPC adapter forwards the `Display` of the whole
error to `tonic::Status::permission_denied` once admission-token
metadata propagation lands (deferred follow-up).

**PSK rotation flow:**

1. Operator stages the new key by adding it to the worker's PSK set.
2. Worker rolls over and authenticates with the new key.
3. Operator removes the old key — in-flight tasks are unaffected
   because admission is checked per-call, not per-task.

**JWT identity flow (P10.16.1):**

1. Operator configures `WorkerAdmissionPolicy.jwt = Some(JwtPolicy)`
   with the IdP issuer, audience, and at least one
   `JwtVerificationKey` (HS256 secret or RS256 public-key PEM).
2. Workers that should authenticate via JWT are added to
   `jwt_workers`. They present a token signed for that issuer +
   audience with `sub = worker_id` and a future `exp`.
3. The control plane verifies the token on every admission-gated
   call. Required claims: `iss`, `aud`, `sub`, `exp`. Optional but
   honored: `nbf`. Audience may be a string or string-array (RFC
   §4.1.3). A configurable clock-skew `leeway_seconds` (default 30s)
   applies to `exp` / `nbf`.
4. Key rotation: append a new `JwtVerificationKey` to the pool, flip
   the IdP to sign with it, drop the old key. Verification tries each
   key in order; the first that verifies wins.

**HS256 vs RS256.** HS256 (shared secret) is appropriate when the
operator administers both the signing IdP and the control plane.
RS256 (asymmetric, PEM public key in the policy) is the production
path: an external IdP holds the signing key, the control plane only
needs the public key. Both algorithms can coexist in the same key
pool during a migration.

The contract is **experimental** until N10 closes (see
`docs/STABILITY.md` for the wire-shape promise). gRPC-metadata
propagation of admission tokens is still deferred to the broader
auth story.

Test references:

- `agentflow-server/src/scheduler/admission.rs#tests` — policy units
  (allowlist, PSK match, rotation overlap, fleet cap, per-worker
  concurrency cap, JWT happy-path + every documented failure mode,
  PSK-takes-precedence-over-JWT misconfiguration).
- `agentflow-server/src/scheduler/jwt.rs#tests` — JWT verifier
  units (HS256 round-trip, signature mismatch, issuer / audience /
  subject mismatch with operator-actionable error fields, expired
  after leeway, just-expired within leeway, nbf in future, key
  rotation pool, multi-aud string-vs-array parsing).
- `agentflow-server/tests/worker_admission.rs` — the three TODO-mandated
  end-to-end scenarios: unknown worker rejected, admitted worker can
  poll/heartbeat/report, and token rotation does not drop in-flight
  tasks.

## Worker Capability + Locality Hints (P10.16.2)

The control plane supports two optional dispatch hints alongside
the static admission caps:

- **Capability-aware dispatch.** Workers advertise which task
  labels they accept via `WorkerCapabilities.node_types`
  (e.g. `["template", "file"]`). The in-memory protocol filters
  the queue per worker so a `template`-only worker never claims
  an `llm` task. Untagged tasks (no `node_type`) and untagged
  workers (empty capability set) preserve the pre-P10.16.2
  behavior — the upgrade is fully additive.
- **Locality preference.** When a worker has recently claimed
  tasks for a `run_id`, the protocol prefers same-run tasks on
  the next claim (warm filesystem, warm context, warm model
  cache). The locality cache is per-worker, in-memory, and
  tracks the most-recently-claimed `run_id` (a future LRU set
  could remember the last N runs if real workloads ask for
  broader locality).

Wire shape:

| Type | Field | Purpose |
|------|-------|---------|
| `WorkerTask` | `node_type: Option<String>` | Capability label. `None` = "any worker." |
| `WorkerHeartbeat` | `capabilities: WorkerCapabilities` | Default empty = "accepts anything." |
| `ClaimHints` | `capabilities`, `locality_run_id` | Optional per-claim hints. |
| `WorkerProtocol::claim_task_with_hints(worker_id, hints)` | new trait method | Default impl falls back to `claim_task(worker_id)`. |

`WorkerControlPlane::claim_task_with_hints` is the public entry
point — it forwards to the protocol and updates the run snapshot
the same way the bare `claim_task` does, so existing
control-plane invariants hold.

**Wire-extension status:** the in-memory protocol implements
capability + locality dispatch end-to-end. The gRPC adapter has
*not* yet extended `pb::ClaimTaskRequest` / `pb::HeartbeatRequest`
to carry the new fields — workers talking gRPC effectively ask
for "no hints" and get pre-P10.16.2 FIFO behavior. Tracked as
follow-up `P10.16.2-FU1` in `TODOs.md`. The trait surface stays
forward-compatible: when the gRPC adapter grows the wire fields,
no caller-side change is needed beyond plumbing them through.

Test references:

- `agentflow-server/src/scheduler/mod.rs#tests` — capability
  filter, locality preference, FIFO fallback when no match,
  cached last-run locality, combined capability + locality, and
  the `WorkerControlPlane::claim_task_with_hints` end-to-end
  invariant that run snapshots still increment.

## Worker Resource Limits (P5.6)

Each worker enforces an in-process resource envelope around every
dispatched node via [`WorkerResourceLimits`](../agentflow-worker/src/lib.rs):

| Knob | Default | Behavior |
|------|---------|----------|
| `default_timeout` | `WorkerConfig::new` → unlimited (legacy smokes); `WorkerResourceLimits::default()` → 300s | Wraps the inner dispatcher in `tokio::time::timeout`. On expiry the worker reports `Failed { retryable: true }` so the scheduler can reattempt under a longer budget. |
| `max_output_bytes` | `default()` → 1 MiB; `unlimited()` → off | When the serialized success output exceeds the cap, the worker replaces it with `{"truncated": true, "limit_bytes": N, "size_bytes": M}` and emits a `worker.task.output_truncated` trace event. |
| Cancellation (`WorkerCancellationToken`) | n/a | Cooperative shutdown. `cancel()` flips an `AtomicBool`; the dispatcher races the inner future against the flag and reports `Failed { retryable: false }` with a `worker.task.cancelled` trace event. |
| Retry semantics | scheduler `with_max_attempts` | Timeouts surface as retryable, cancellations as terminal, definition errors as terminal. The `DistributedDagScheduler` budget bounds total reattempts. |

**Memory caps:** intentionally **not** implemented in v0.4.0. The Linux
cgroups path (and the equivalent macOS `setrlimit` workaround) lives
inside the supervising process, not the worker binary. Until the
deployment story stabilizes, operators should:

- Run each worker under a systemd unit / Kubernetes Pod with the
  appropriate `MemoryMax` / `resources.limits.memory` set.
- Rely on the existing `default_timeout` to bound long-running tasks
  that would otherwise leak memory in a stuck dispatcher.

This is a documented gap for macOS / Windows operators: the worker
itself enforces wall-clock and output bytes; out-of-process memory
caps come from the container / cgroup runtime.

### Synthetic runaway fixture

The `mock` payload now supports two test-only knobs so the resource
caps are testable hermetically:

```yaml
parameters:
  sleep_ms: 5000           # makes the dispatcher take this long
  output_size_bytes: 8192  # appends an "x" * N blob to the output
```

`agentflow-worker/tests/resource_limits.rs` exercises every guarantee
above: timeout cut-off, mid-dispatch cancellation, output truncation
with the matching trace event, and retry semantics through the
distributed scheduler.

## Failure Domains (P5.7)

Six distributed failure scenarios are pinned down by integration
tests in `agentflow-worker/tests/failure_domains.rs`:

| Scenario | Trigger | Recovery | Test |
|----------|---------|----------|------|
| Stale heartbeat | Worker stops heartbeating while holding a task | Scheduler `requeue_stale_tasks()` reaps the assignment and flips the node back to `Pending` with `attempt += 1` | `stale_heartbeat_redistributes_to_another_worker` |
| Worker crash mid-task | Externally identical to stale heartbeat (the process is gone, no fresh heartbeats) | Same reap path; the surviving worker claims and completes the redispatched task | `worker_crash_midtask_is_reattempted_elsewhere` |
| Retryable failure | `WorkerTaskResult::Failed { retryable: true }` (e.g. timeout, transport hiccup) | Scheduler requeues until `max_attempts` is exhausted; reattempts may land on a different worker | `retryable_failure_retries_on_another_worker` |
| Non-retryable failure | `WorkerTaskResult::Failed { retryable: false }` (e.g. unknown node type, cancellation) | Node moves to `DistributedNodeStatus::Failed` immediately; no further attempts | `non_retryable_failure_is_terminal` |
| Duplicate completion | Same `task_id` reported twice by the worker (e.g. gRPC retry on the wire) | `WorkerProtocol::report_result` returns `SchedulerError::TaskNotClaimed` for the second call; run accounting stays consistent | `duplicate_completion_is_idempotent` |
| Trace stitching across reattempts | Multiple attempts of the same node | Each attempt contributes worker trace fragments; the control plane appends them in global-order with monotonic `global_seq`, so a single OTel span tree covers the full lineage | `trace_stitching_preserves_both_attempts` |

The mock node's `fail_until_attempt` / `sleep_ms` / `output_size_bytes`
knobs (worker side) plus `with_heartbeat_timeout` / `with_max_attempts`
(scheduler side) keep these tests hermetic — no real process crashes
or wall-clock races are needed.

## Two-Worker Deployment Shape

The target deployment shape is one control plane plus N workers:

```bash
agentflow-server --bind 0.0.0.0:8080 --worker-grpc 0.0.0.0:50051
agentflow-worker --control-plane grpc://agentflow-server:50051 --worker-id worker-a
agentflow-worker --control-plane grpc://agentflow-server:50051 --worker-id worker-b
```

The library-level gRPC adapter and two-worker smoke coverage are in place. The
server binary still needs a CLI flag/listener wiring step before the deployment
shape above is a complete end-user command.

## Failure Semantics

The scheduler will distinguish three cases:

- Worker reports `Failed { retryable: true }`: requeue until the attempt budget
  is exhausted.
- Worker reports `Failed { retryable: false }`: mark the task and run failed.
- Worker stops heartbeating while holding a task: requeue or fail based on
  attempt budget and node idempotency.

Trace fragments already travel in `WorkerTaskResult` and are stitched by the
control plane into a global per-run order. The OTel span mapping is available at
the control-plane boundary; the remaining production step is wiring those spans
to the configured OTLP exporter. When retries are added, each attempt will be
preserved so OTel export can show the full cross-worker lineage.
