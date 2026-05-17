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
