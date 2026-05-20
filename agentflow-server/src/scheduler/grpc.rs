//! gRPC transport adapter for distributed workers.
//!
//! The protobuf schema is checked in under
//! `proto/agentflow/scheduler/v1/worker.proto`. This module keeps the generated
//! surface small and hand-written so the scheduler crate does not need a build
//! script in the first transport milestone.

use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use tonic::body::BoxBody;
use tonic::client::Grpc;
use tonic::codegen::{BoxFuture, Service, StdError, http};
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::{
  SchedulerError, WorkerControlPlane, WorkerHeartbeat, WorkerId, WorkerProtocol, WorkerTask,
  WorkerTaskResult, WorkerTraceEvent,
};

/// Boxed-`Status` result used by the proto <-> domain conversion helpers in
/// this module. `tonic::Status` is ~176 bytes (clippy::result_large_err), so
/// every Result-returning helper that bubbles a `Status` through `?` boxes it.
/// The public tonic trait surface still returns `Result<_, Status>` (unboxed)
/// — callers unbox with `.map_err(|e| *e)` at the boundary.
type BoxedStatusResult<T> = std::result::Result<T, Box<Status>>;

/// Protobuf wire messages for `agentflow.scheduler.v1`.
pub mod pb {
  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct Empty {}

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct WorkerTask {
    #[prost(string, tag = "1")]
    pub task_id: String,
    #[prost(string, tag = "2")]
    pub run_id: String,
    #[prost(string, tag = "3")]
    pub node_id: String,
    #[prost(uint32, tag = "4")]
    pub attempt: u32,
    #[prost(string, tag = "5")]
    pub payload_json: String,
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct WorkerTraceEvent {
    #[prost(int64, tag = "1")]
    pub seq: i64,
    #[prost(string, tag = "2")]
    pub kind: String,
    #[prost(string, tag = "3")]
    pub payload_json: String,
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct WorkerTaskResult {
    #[prost(enumeration = "worker_task_result::Status", tag = "1")]
    pub status: i32,
    #[prost(string, tag = "2")]
    pub output_json: String,
    #[prost(string, tag = "3")]
    pub error: String,
    #[prost(bool, tag = "4")]
    pub retryable: bool,
    #[prost(message, repeated, tag = "5")]
    pub events: Vec<WorkerTraceEvent>,
  }

  pub mod worker_task_result {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum Status {
      Unspecified = 0,
      Succeeded = 1,
      Failed = 2,
    }
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct SubmitTaskRequest {
    #[prost(message, optional, tag = "1")]
    pub task: Option<WorkerTask>,
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct ClaimTaskRequest {
    #[prost(string, tag = "1")]
    pub worker_id: String,
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct ClaimTaskResponse {
    #[prost(message, optional, tag = "1")]
    pub task: Option<WorkerTask>,
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct ReportResultRequest {
    #[prost(string, tag = "1")]
    pub worker_id: String,
    #[prost(string, tag = "2")]
    pub task_id: String,
    #[prost(message, optional, tag = "3")]
    pub result: Option<WorkerTaskResult>,
  }

  #[derive(Clone, PartialEq, ::prost::Message)]
  pub struct HeartbeatRequest {
    #[prost(string, tag = "1")]
    pub worker_id: String,
    #[prost(string, tag = "2")]
    pub active_task_id: String,
    #[prost(uint32, tag = "3")]
    pub free_slots: u32,
    #[prost(string, tag = "4")]
    pub timestamp_rfc3339: String,
  }
}

/// Server-side implementation of the worker gRPC service.
#[derive(Debug, Clone)]
pub struct GrpcWorkerService<P> {
  protocol: P,
}

impl<P> GrpcWorkerService<P> {
  pub fn new(protocol: P) -> Self {
    Self { protocol }
  }
}

#[async_trait]
pub trait WorkerControl: Send + Sync + 'static {
  async fn submit_task(
    &self,
    request: Request<pb::SubmitTaskRequest>,
  ) -> Result<Response<pb::Empty>, Status>;

  async fn claim_task(
    &self,
    request: Request<pb::ClaimTaskRequest>,
  ) -> Result<Response<pb::ClaimTaskResponse>, Status>;

  async fn report_result(
    &self,
    request: Request<pb::ReportResultRequest>,
  ) -> Result<Response<pb::Empty>, Status>;

  async fn heartbeat(
    &self,
    request: Request<pb::HeartbeatRequest>,
  ) -> Result<Response<pb::Empty>, Status>;
}

#[async_trait]
impl<P> WorkerControl for GrpcWorkerService<P>
where
  P: WorkerProtocol + 'static,
{
  async fn submit_task(
    &self,
    request: Request<pb::SubmitTaskRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let task = request
      .into_inner()
      .task
      .ok_or_else(|| Box::new(Status::invalid_argument("task is required")))
      .and_then(worker_task_from_proto)
      .map_err(|e| *e)?;
    self
      .protocol
      .submit_task(task)
      .await
      .map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }

  async fn claim_task(
    &self,
    request: Request<pb::ClaimTaskRequest>,
  ) -> Result<Response<pb::ClaimTaskResponse>, Status> {
    let worker_id = WorkerId::new(request.into_inner().worker_id)
      .map_err(|err| Status::invalid_argument(err.to_string()))?;
    let task = self
      .protocol
      .claim_task(worker_id)
      .await
      .map_err(status_from_error)?
      .map(worker_task_to_proto);
    Ok(Response::new(pb::ClaimTaskResponse { task }))
  }

  async fn report_result(
    &self,
    request: Request<pb::ReportResultRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let request = request.into_inner();
    let worker_id =
      WorkerId::new(request.worker_id).map_err(|err| Status::invalid_argument(err.to_string()))?;
    let task_id = parse_uuid(&request.task_id, "task_id").map_err(|e| *e)?;
    let result = request
      .result
      .ok_or_else(|| Box::new(Status::invalid_argument("result is required")))
      .and_then(worker_task_result_from_proto)
      .map_err(|e| *e)?;
    self
      .protocol
      .report_result(worker_id, task_id, result)
      .await
      .map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }

  async fn heartbeat(
    &self,
    request: Request<pb::HeartbeatRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let heartbeat = worker_heartbeat_from_proto(request.into_inner()).map_err(|e| *e)?;
    self
      .protocol
      .heartbeat(heartbeat)
      .await
      .map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }
}

#[async_trait]
impl<P> WorkerControl for WorkerControlPlane<P>
where
  P: WorkerProtocol + 'static,
{
  async fn submit_task(
    &self,
    request: Request<pb::SubmitTaskRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let task = request
      .into_inner()
      .task
      .ok_or_else(|| Box::new(Status::invalid_argument("task is required")))
      .and_then(worker_task_from_proto)
      .map_err(|e| *e)?;
    self.schedule_task(task).await.map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }

  async fn claim_task(
    &self,
    request: Request<pb::ClaimTaskRequest>,
  ) -> Result<Response<pb::ClaimTaskResponse>, Status> {
    let worker_id = WorkerId::new(request.into_inner().worker_id)
      .map_err(|err| Status::invalid_argument(err.to_string()))?;
    let task = self
      .claim_task(worker_id)
      .await
      .map_err(status_from_error)?
      .map(worker_task_to_proto);
    Ok(Response::new(pb::ClaimTaskResponse { task }))
  }

  async fn report_result(
    &self,
    request: Request<pb::ReportResultRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let request = request.into_inner();
    let worker_id =
      WorkerId::new(request.worker_id).map_err(|err| Status::invalid_argument(err.to_string()))?;
    let task_id = parse_uuid(&request.task_id, "task_id").map_err(|e| *e)?;
    let result = request
      .result
      .ok_or_else(|| Box::new(Status::invalid_argument("result is required")))
      .and_then(worker_task_result_from_proto)
      .map_err(|e| *e)?;
    self
      .report_result(worker_id, task_id, result)
      .await
      .map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }

  async fn heartbeat(
    &self,
    request: Request<pb::HeartbeatRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let heartbeat = worker_heartbeat_from_proto(request.into_inner()).map_err(|e| *e)?;
    self.heartbeat(heartbeat).await.map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }
}

/// Tonic server wrapper for [`WorkerControl`].
#[derive(Debug, Clone)]
pub struct WorkerControlServer<T> {
  inner: Arc<T>,
}

impl<T> WorkerControlServer<T> {
  pub fn new(inner: T) -> Self {
    Self {
      inner: Arc::new(inner),
    }
  }
}

impl<T, B> Service<http::Request<B>> for WorkerControlServer<T>
where
  T: WorkerControl,
  B: tonic::codegen::Body + Send + 'static,
  B::Error: Into<StdError> + Send + 'static,
{
  type Response = http::Response<BoxBody>;
  type Error = std::convert::Infallible;
  type Future = BoxFuture<Self::Response, Self::Error>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: http::Request<B>) -> Self::Future {
    let inner = self.inner.clone();
    match req.uri().path() {
      "/agentflow.scheduler.v1.WorkerControl/SubmitTask" => {
        struct SubmitTaskSvc<T>(Arc<T>);
        impl<T> tonic::server::UnaryService<pb::SubmitTaskRequest> for SubmitTaskSvc<T>
        where
          T: WorkerControl,
        {
          type Response = pb::Empty;
          type Future = BoxFuture<Response<Self::Response>, Status>;

          fn call(&mut self, request: Request<pb::SubmitTaskRequest>) -> Self::Future {
            let inner = self.0.clone();
            // P3.8: install upstream traceparent for the duration of
            // the trait dispatch so any tracing/agent code the
            // handler triggers stitches onto the caller's span.
            let traceparent = extract_traceparent_from_grpc_request(&request);
            Box::pin(run_in_traceparent_scope(traceparent, async move {
              inner.submit_task(request).await
            }))
          }
        }
        Box::pin(async move {
          let codec = tonic::codec::ProstCodec::default();
          let mut grpc = tonic::server::Grpc::new(codec);
          Ok(grpc.unary(SubmitTaskSvc(inner), req).await)
        })
      }
      "/agentflow.scheduler.v1.WorkerControl/ClaimTask" => {
        struct ClaimTaskSvc<T>(Arc<T>);
        impl<T> tonic::server::UnaryService<pb::ClaimTaskRequest> for ClaimTaskSvc<T>
        where
          T: WorkerControl,
        {
          type Response = pb::ClaimTaskResponse;
          type Future = BoxFuture<Response<Self::Response>, Status>;

          fn call(&mut self, request: Request<pb::ClaimTaskRequest>) -> Self::Future {
            let inner = self.0.clone();
            let traceparent = extract_traceparent_from_grpc_request(&request);
            Box::pin(run_in_traceparent_scope(traceparent, async move {
              inner.claim_task(request).await
            }))
          }
        }
        Box::pin(async move {
          let codec = tonic::codec::ProstCodec::default();
          let mut grpc = tonic::server::Grpc::new(codec);
          Ok(grpc.unary(ClaimTaskSvc(inner), req).await)
        })
      }
      "/agentflow.scheduler.v1.WorkerControl/ReportResult" => {
        struct ReportResultSvc<T>(Arc<T>);
        impl<T> tonic::server::UnaryService<pb::ReportResultRequest> for ReportResultSvc<T>
        where
          T: WorkerControl,
        {
          type Response = pb::Empty;
          type Future = BoxFuture<Response<Self::Response>, Status>;

          fn call(&mut self, request: Request<pb::ReportResultRequest>) -> Self::Future {
            let inner = self.0.clone();
            let traceparent = extract_traceparent_from_grpc_request(&request);
            Box::pin(run_in_traceparent_scope(traceparent, async move {
              inner.report_result(request).await
            }))
          }
        }
        Box::pin(async move {
          let codec = tonic::codec::ProstCodec::default();
          let mut grpc = tonic::server::Grpc::new(codec);
          Ok(grpc.unary(ReportResultSvc(inner), req).await)
        })
      }
      "/agentflow.scheduler.v1.WorkerControl/Heartbeat" => {
        struct HeartbeatSvc<T>(Arc<T>);
        impl<T> tonic::server::UnaryService<pb::HeartbeatRequest> for HeartbeatSvc<T>
        where
          T: WorkerControl,
        {
          type Response = pb::Empty;
          type Future = BoxFuture<Response<Self::Response>, Status>;

          fn call(&mut self, request: Request<pb::HeartbeatRequest>) -> Self::Future {
            let inner = self.0.clone();
            let traceparent = extract_traceparent_from_grpc_request(&request);
            Box::pin(run_in_traceparent_scope(traceparent, async move {
              inner.heartbeat(request).await
            }))
          }
        }
        Box::pin(async move {
          let codec = tonic::codec::ProstCodec::default();
          let mut grpc = tonic::server::Grpc::new(codec);
          Ok(grpc.unary(HeartbeatSvc(inner), req).await)
        })
      }
      _ => Box::pin(async move {
        Ok(
          http::Response::builder()
            .status(200)
            .header("grpc-status", "12")
            .header("content-type", "application/grpc")
            .body(empty_body())
            .unwrap_or_else(|_| http::Response::new(empty_body())),
        )
      }),
    }
  }
}

impl<T> tonic::server::NamedService for WorkerControlServer<T> {
  const NAME: &'static str = "agentflow.scheduler.v1.WorkerControl";
}

/// WorkerProtocol implementation backed by a remote tonic service.
#[derive(Debug, Clone)]
pub struct GrpcWorkerProtocol {
  inner: Arc<Mutex<Grpc<Channel>>>,
}

impl GrpcWorkerProtocol {
  pub async fn connect(endpoint: impl AsRef<str>) -> Result<Self, SchedulerError> {
    let channel = Endpoint::from_shared(endpoint.as_ref().to_string())
      .map_err(|err| SchedulerError::Transport {
        message: err.to_string(),
      })?
      .connect()
      .await
      .map_err(|err| SchedulerError::Transport {
        message: err.to_string(),
      })?;
    Ok(Self {
      inner: Arc::new(Mutex::new(Grpc::new(channel))),
    })
  }

  pub fn from_channel(channel: Channel) -> Self {
    Self {
      inner: Arc::new(Mutex::new(Grpc::new(channel))),
    }
  }

  async fn unary<Req, Resp>(
    &self,
    path: &'static str,
    request: Req,
  ) -> Result<Response<Resp>, SchedulerError>
  where
    Req: prost::Message + Default + 'static,
    Resp: prost::Message + Default + 'static,
  {
    let mut inner = self.inner.lock().await;
    inner
      .ready()
      .await
      .map_err(|err| SchedulerError::Transport {
        message: err.to_string(),
      })?;
    let path = http::uri::PathAndQuery::from_static(path);
    let codec = tonic::codec::ProstCodec::default();

    // P3.8: cross-hop W3C traceparent propagation. When the caller
    // is running inside an `agentflow_tracing::context::scope`,
    // inject the active value as a gRPC `traceparent` metadata
    // entry. Mirrors the lowercase HTTP-header spelling so OTel
    // tools that already grep "traceparent" match without
    // translation. Outside any scope, the metadata is omitted —
    // consumers can tell apart "no upstream trace" from "upstream
    // trace exists but is malformed".
    let mut req = Request::new(request);
    inject_traceparent_into_grpc_request(&mut req);
    inner
      .unary(req, path, codec)
      .await
      .map_err(scheduler_error_from_status)
  }
}

/// Inject the active `agentflow_tracing::context::current_traceparent`
/// into a tonic `Request`'s gRPC metadata as the `traceparent` key.
/// No-op when there is no active context — see the module-level
/// comment for the rationale behind omitting (not emitting an empty
/// value).
///
/// Exposed as `pub` so cross-hop integration tests (and any
/// downstream consumers wiring custom RPC paths) can reuse the
/// canonical injection logic without re-implementing it.
pub fn inject_traceparent_into_grpc_request<T>(request: &mut Request<T>) {
  let Some(traceparent) = agentflow_tracing::context::current_traceparent() else {
    return;
  };
  match tonic::metadata::AsciiMetadataValue::try_from(traceparent.as_str()) {
    Ok(value) => {
      request.metadata_mut().insert("traceparent", value);
    }
    Err(_err) => {
      // Defensive: a traceparent that contains non-ASCII bytes
      // would be a bug upstream. Drop it silently rather than
      // poisoning the entire RPC — tracing is observability, not
      // a correctness path.
      tracing::warn!("skipping traceparent gRPC injection: value contains non-ASCII bytes");
    }
  }
}

/// Extract the `traceparent` gRPC metadata value from an incoming
/// `Request`. Returns `None` when the metadata key is absent or
/// holds a non-ASCII value. Servers use this to install the parent
/// context via `agentflow_tracing::context::scope` before dispatch.
pub fn extract_traceparent_from_grpc_request<T>(request: &Request<T>) -> Option<String> {
  request
    .metadata()
    .get("traceparent")
    .and_then(|value| value.to_str().ok())
    .map(str::to_owned)
}

/// Run `fut` inside `agentflow_tracing::context::scope` when
/// `traceparent` is `Some`, or plain otherwise. Centralises the
/// scope-vs-no-scope branch the gRPC handler stubs all need.
///
/// Public-in-crate so the four inner unary handler `Svc` structs
/// share one implementation. Generic over the future type so each
/// `WorkerControl` method (which returns a different `Response<R>`)
/// can use it without boxing.
pub(crate) async fn run_in_traceparent_scope<F, T>(traceparent: Option<String>, fut: F) -> T
where
  F: std::future::Future<Output = T>,
{
  match traceparent {
    Some(tp) => agentflow_tracing::context::scope(tp, fut).await,
    None => fut.await,
  }
}

#[async_trait]
impl WorkerProtocol for GrpcWorkerProtocol {
  async fn submit_task(&self, task: WorkerTask) -> Result<(), SchedulerError> {
    self
      .unary::<_, pb::Empty>(
        "/agentflow.scheduler.v1.WorkerControl/SubmitTask",
        pb::SubmitTaskRequest {
          task: Some(worker_task_to_proto(task)),
        },
      )
      .await?;
    Ok(())
  }

  async fn claim_task(&self, worker_id: WorkerId) -> Result<Option<WorkerTask>, SchedulerError> {
    let response = self
      .unary::<_, pb::ClaimTaskResponse>(
        "/agentflow.scheduler.v1.WorkerControl/ClaimTask",
        pb::ClaimTaskRequest {
          worker_id: worker_id.0,
        },
      )
      .await?
      .into_inner();
    response
      .task
      .map(worker_task_from_proto)
      .transpose()
      .map_err(|boxed| scheduler_error_from_status(*boxed))
  }

  async fn report_result(
    &self,
    worker_id: WorkerId,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), SchedulerError> {
    self
      .unary::<_, pb::Empty>(
        "/agentflow.scheduler.v1.WorkerControl/ReportResult",
        pb::ReportResultRequest {
          worker_id: worker_id.0,
          task_id: task_id.to_string(),
          result: Some(worker_task_result_to_proto(result)),
        },
      )
      .await?;
    Ok(())
  }

  async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), SchedulerError> {
    self
      .unary::<_, pb::Empty>(
        "/agentflow.scheduler.v1.WorkerControl/Heartbeat",
        worker_heartbeat_to_proto(heartbeat),
      )
      .await?;
    Ok(())
  }
}

fn worker_task_to_proto(task: WorkerTask) -> pb::WorkerTask {
  pb::WorkerTask {
    task_id: task.task_id.to_string(),
    run_id: task.run_id.to_string(),
    node_id: task.node_id,
    attempt: task.attempt,
    payload_json: task.payload.to_string(),
  }
}

fn worker_task_from_proto(task: pb::WorkerTask) -> BoxedStatusResult<WorkerTask> {
  Ok(WorkerTask {
    task_id: parse_uuid(&task.task_id, "task_id")?,
    run_id: parse_uuid(&task.run_id, "run_id")?,
    node_id: task.node_id,
    attempt: task.attempt,
    payload: parse_json(&task.payload_json, "payload_json")?,
    // P10.16.2 capability label is not yet plumbed across the gRPC
    // wire (follow-up TODO `P10.16.2-FU1`). Until then, every task
    // arriving over gRPC is untagged; the capability filter
    // unconditionally accepts untagged tasks so the upgrade is
    // additive at the protocol boundary.
    node_type: None,
  })
}

fn worker_trace_event_to_proto(event: WorkerTraceEvent) -> pb::WorkerTraceEvent {
  pb::WorkerTraceEvent {
    seq: event.seq,
    kind: event.kind,
    payload_json: event.payload.to_string(),
  }
}

fn worker_trace_event_from_proto(
  event: pb::WorkerTraceEvent,
) -> BoxedStatusResult<WorkerTraceEvent> {
  Ok(WorkerTraceEvent {
    seq: event.seq,
    kind: event.kind,
    payload: parse_json(&event.payload_json, "payload_json")?,
  })
}

fn worker_task_result_to_proto(result: WorkerTaskResult) -> pb::WorkerTaskResult {
  match result {
    WorkerTaskResult::Succeeded { output, events } => pb::WorkerTaskResult {
      status: pb::worker_task_result::Status::Succeeded as i32,
      output_json: output.to_string(),
      error: String::new(),
      retryable: false,
      events: events
        .into_iter()
        .map(worker_trace_event_to_proto)
        .collect(),
    },
    WorkerTaskResult::Failed {
      error,
      retryable,
      events,
    } => pb::WorkerTaskResult {
      status: pb::worker_task_result::Status::Failed as i32,
      output_json: String::new(),
      error,
      retryable,
      events: events
        .into_iter()
        .map(worker_trace_event_to_proto)
        .collect(),
    },
  }
}

fn worker_task_result_from_proto(
  result: pb::WorkerTaskResult,
) -> BoxedStatusResult<WorkerTaskResult> {
  let events = result
    .events
    .into_iter()
    .map(worker_trace_event_from_proto)
    .collect::<Result<Vec<_>, _>>()?;
  match pb::worker_task_result::Status::try_from(result.status) {
    Ok(pb::worker_task_result::Status::Succeeded) => Ok(WorkerTaskResult::Succeeded {
      output: parse_json(&result.output_json, "output_json")?,
      events,
    }),
    Ok(pb::worker_task_result::Status::Failed) => Ok(WorkerTaskResult::Failed {
      error: result.error,
      retryable: result.retryable,
      events,
    }),
    Ok(pb::worker_task_result::Status::Unspecified) | Err(_) => Err(Box::new(
      Status::invalid_argument("result status is required"),
    )),
  }
}

fn worker_heartbeat_to_proto(heartbeat: WorkerHeartbeat) -> pb::HeartbeatRequest {
  pb::HeartbeatRequest {
    worker_id: heartbeat.worker_id.0,
    active_task_id: heartbeat
      .active_task
      .map(|task_id| task_id.to_string())
      .unwrap_or_default(),
    free_slots: heartbeat.free_slots,
    timestamp_rfc3339: heartbeat.ts.to_rfc3339(),
  }
}

fn worker_heartbeat_from_proto(
  heartbeat: pb::HeartbeatRequest,
) -> BoxedStatusResult<WorkerHeartbeat> {
  let worker_id = WorkerId::new(heartbeat.worker_id)
    .map_err(|err| Box::new(Status::invalid_argument(err.to_string())))?;
  let active_task = if heartbeat.active_task_id.is_empty() {
    None
  } else {
    Some(parse_uuid(&heartbeat.active_task_id, "active_task_id")?)
  };
  let ts = if heartbeat.timestamp_rfc3339.is_empty() {
    Utc::now()
  } else {
    DateTime::parse_from_rfc3339(&heartbeat.timestamp_rfc3339)
      .map_err(|err| {
        Box::new(Status::invalid_argument(format!(
          "invalid timestamp_rfc3339: {err}"
        )))
      })?
      .with_timezone(&Utc)
  };
  Ok(WorkerHeartbeat {
    worker_id,
    active_task,
    free_slots: heartbeat.free_slots,
    ts,
    // P10.16.2 capability advertisement is not yet plumbed across
    // the gRPC wire (follow-up TODO `P10.16.2-FU1`). Heartbeats
    // over gRPC default to "any task" so existing gRPC workers
    // keep behaving as before.
    capabilities: crate::scheduler::WorkerCapabilities::default(),
  })
}

fn parse_uuid(value: &str, field: &str) -> BoxedStatusResult<Uuid> {
  Uuid::parse_str(value)
    .map_err(|err| Box::new(Status::invalid_argument(format!("invalid {field}: {err}"))))
}

fn parse_json(value: &str, field: &str) -> BoxedStatusResult<serde_json::Value> {
  if value.is_empty() {
    return Ok(serde_json::Value::Null);
  }
  serde_json::from_str(value)
    .map_err(|err| Box::new(Status::invalid_argument(format!("invalid {field}: {err}"))))
}

fn status_from_error(error: SchedulerError) -> Status {
  match error {
    SchedulerError::InvalidWorkerId => Status::invalid_argument(error.to_string()),
    SchedulerError::TaskNotClaimed { .. } | SchedulerError::WorkerMismatch { .. } => {
      Status::failed_precondition(error.to_string())
    }
    SchedulerError::Transport { .. } => Status::unavailable(error.to_string()),
  }
}

fn scheduler_error_from_status(status: Status) -> SchedulerError {
  SchedulerError::Transport {
    message: status.to_string(),
  }
}

fn empty_body() -> BoxBody {
  tonic::body::empty_body()
}

#[cfg(test)]
mod traceparent_tests {
  //! Unit coverage for P3.8 worker gRPC traceparent propagation.
  //!
  //! Inject and extract are pure metadata-manipulation helpers, so
  //! the contract can be verified without spinning up a real
  //! channel. End-to-end coverage (real tonic client + server with
  //! cross-process scope propagation) lives in the integration test
  //! suite under `agentflow-worker/tests/` so it can race a real
  //! channel against admission + heartbeat flows that already exist
  //! there.
  use super::*;
  use tonic::Request;

  #[test]
  fn inject_outside_any_scope_leaves_metadata_empty() {
    let mut req = Request::new(pb::SubmitTaskRequest { task: None });
    inject_traceparent_into_grpc_request(&mut req);
    assert!(
      req.metadata().get("traceparent").is_none(),
      "no scope ⇒ no traceparent key; consumers rely on absence"
    );
  }

  #[tokio::test]
  async fn inject_inside_scope_writes_traceparent_metadata() {
    let mut req = Request::new(pb::ClaimTaskRequest {
      worker_id: "worker-1".into(),
    });
    agentflow_tracing::context::scope("00-trace-id-span-01".to_string(), async {
      inject_traceparent_into_grpc_request(&mut req);
    })
    .await;
    let value = req
      .metadata()
      .get("traceparent")
      .expect("traceparent metadata populated inside scope");
    assert_eq!(value.to_str().unwrap(), "00-trace-id-span-01");
  }

  #[test]
  fn extract_returns_none_for_missing_traceparent_metadata() {
    let req = Request::new(pb::SubmitTaskRequest { task: None });
    assert!(extract_traceparent_from_grpc_request(&req).is_none());
  }

  #[test]
  fn extract_returns_traceparent_when_metadata_populated() {
    let mut req = Request::new(pb::SubmitTaskRequest { task: None });
    req
      .metadata_mut()
      .insert("traceparent", "00-deadbeef-cafebabe-01".parse().unwrap());
    assert_eq!(
      extract_traceparent_from_grpc_request(&req).as_deref(),
      Some("00-deadbeef-cafebabe-01")
    );
  }

  #[tokio::test]
  async fn run_in_traceparent_scope_with_some_installs_context_for_future() {
    let observed = run_in_traceparent_scope(Some("00-scoped-test-01".to_string()), async {
      agentflow_tracing::context::current_traceparent()
    })
    .await;
    assert_eq!(observed.as_deref(), Some("00-scoped-test-01"));
  }

  #[tokio::test]
  async fn run_in_traceparent_scope_with_none_runs_future_outside_any_scope() {
    let observed = run_in_traceparent_scope(None, async {
      agentflow_tracing::context::current_traceparent()
    })
    .await;
    assert!(observed.is_none());
  }

  #[tokio::test]
  async fn round_trip_inject_then_extract_round_trips_value_under_active_scope() {
    let mut req = Request::new(pb::HeartbeatRequest::default());
    agentflow_tracing::context::scope("00-roundtrip-test-01".to_string(), async {
      inject_traceparent_into_grpc_request(&mut req);
    })
    .await;
    assert_eq!(
      extract_traceparent_from_grpc_request(&req).as_deref(),
      Some("00-roundtrip-test-01")
    );
  }
}
