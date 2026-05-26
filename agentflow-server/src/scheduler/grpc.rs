//! gRPC transport adapter for distributed workers.
//!
//! The protobuf schema is checked in under
//! `proto/agentflow/scheduler/v1/worker.proto`. As of Q3.3.3 the
//! prost message structs in `pb` are generated from that .proto by
//! `build.rs` (`tonic-build`) so non-Rust language bindings stay
//! byte-compatible with the Rust path. Only the message structs are
//! generated — the tonic `Service` impl + the `WorkerControl` trait
//! stay hand-written below because they wire in custom W3C
//! traceparent scope handling + admission credential extraction
//! that the generated stubs do not model.

use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
///
/// Q3.3.3: generated from `proto/agentflow/scheduler/v1/worker.proto`
/// at build time via `tonic-build` in `build.rs`. The .proto file is
/// the source of truth — to add or change a field, edit the .proto
/// and rebuild. Non-Rust language bindings (Python `grpcio-tools`, Go
/// `protoc-gen-go`, …) regenerate from the same file so the wire
/// shape stays consistent across stacks.
pub mod pb {
  tonic::include_proto!("agentflow.scheduler.v1");
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

/// Q1.6.1: extract the `authorization` metadata from a tonic request as
/// a bare token string. Returns `None` when the header is absent or
/// malformed (caller decides whether absence is fatal). Strips the
/// `Bearer ` prefix when present so callers receive only the PSK value.
fn extract_admission_token<T>(request: &Request<T>) -> Option<String> {
  let header = request.metadata().get("authorization")?;
  let value = header.to_str().ok()?;
  let token = value.strip_prefix("Bearer ").unwrap_or(value);
  if token.is_empty() {
    None
  } else {
    Some(token.to_string())
  }
}

/// Server-side authenticated worker gRPC service. Each call extracts
/// `authorization` metadata, builds a [`WorkerCredential`], and routes
/// through [`AuthenticatedControlPlane`] which enforces PSK / JWT /
/// fleet-cap admission before delegating to the inner protocol.
///
/// Closes Q1.6.1: the unauthenticated [`GrpcWorkerService`] is still
/// available for the `memory://local` and in-process test paths, but
/// production gRPC deployments wire this struct instead.
#[derive(Debug, Clone)]
pub struct AuthenticatedGrpcWorkerService<P> {
  authenticated: Arc<crate::scheduler::admission::AuthenticatedControlPlane<P>>,
}

impl<P> AuthenticatedGrpcWorkerService<P> {
  pub fn new(
    authenticated: Arc<crate::scheduler::admission::AuthenticatedControlPlane<P>>,
  ) -> Self {
    Self { authenticated }
  }
}

#[async_trait]
impl<P> WorkerControl for AuthenticatedGrpcWorkerService<P>
where
  P: WorkerProtocol + Clone + 'static,
{
  /// `submit_task` is gateway → server (not worker → server), so it
  /// requires admission only if the deployment configured one. We
  /// accept it without credentials to match the existing wire shape;
  /// the `tonic::Request` is already constrained by the bearer
  /// middleware applied to the parent HTTP server.
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
      .authenticated
      .inner()
      .schedule_task(task)
      .await
      .map_err(status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }

  async fn claim_task(
    &self,
    request: Request<pb::ClaimTaskRequest>,
  ) -> Result<Response<pb::ClaimTaskResponse>, Status> {
    let token = extract_admission_token(&request);
    let inner = request.into_inner();
    let worker_id = WorkerId::new(inner.worker_id.clone())
      .map_err(|err| Status::invalid_argument(err.to_string()))?;
    let credential = crate::scheduler::admission::WorkerCredential::new(worker_id.clone(), token);
    let task = self
      .authenticated
      .claim_task(credential)
      .await
      .map_err(authn_status_from_error)?
      .map(worker_task_to_proto);
    Ok(Response::new(pb::ClaimTaskResponse { task }))
  }

  async fn report_result(
    &self,
    request: Request<pb::ReportResultRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let token = extract_admission_token(&request);
    let inner = request.into_inner();
    let worker_id =
      WorkerId::new(inner.worker_id).map_err(|err| Status::invalid_argument(err.to_string()))?;
    let task_id = parse_uuid(&inner.task_id, "task_id").map_err(|e| *e)?;
    let result = inner
      .result
      .ok_or_else(|| Box::new(Status::invalid_argument("result is required")))
      .and_then(worker_task_result_from_proto)
      .map_err(|e| *e)?;
    let credential = crate::scheduler::admission::WorkerCredential::new(worker_id, token);
    self
      .authenticated
      .report_result(credential, task_id, result)
      .await
      .map_err(authn_status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }

  async fn heartbeat(
    &self,
    request: Request<pb::HeartbeatRequest>,
  ) -> Result<Response<pb::Empty>, Status> {
    let token = extract_admission_token(&request);
    let inner = request.into_inner();
    let worker_id = WorkerId::new(inner.worker_id.clone())
      .map_err(|err| Status::invalid_argument(err.to_string()))?;
    let heartbeat = worker_heartbeat_from_proto(inner).map_err(|e| *e)?;
    let credential = crate::scheduler::admission::WorkerCredential::new(worker_id, token);
    self
      .authenticated
      .heartbeat(credential, heartbeat)
      .await
      .map_err(authn_status_from_error)?;
    Ok(Response::new(pb::Empty {}))
  }
}

/// Map [`crate::scheduler::admission::ControlError`] to a tonic Status,
/// surfacing admission failures as `permission_denied` so the client
/// distinguishes auth rejection from transport / state errors.
fn authn_status_from_error(err: crate::scheduler::admission::ControlError) -> Status {
  match err {
    crate::scheduler::admission::ControlError::Admission(adm) => {
      Status::permission_denied(adm.to_string())
    }
    crate::scheduler::admission::ControlError::Scheduler(sched) => status_from_error(sched),
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
    let request = request.into_inner();
    let worker_id = WorkerId::new(request.worker_id.clone())
      .map_err(|err| Status::invalid_argument(err.to_string()))?;
    // P10.16.2-FU1: route through `claim_task_with_hints` so the
    // worker's advertised capabilities + locality preference
    // filter the queue. Pre-FU1 workers send empty fields, which
    // `ClaimHints::default()` interprets as "no preference" —
    // identical to the bare `claim_task` path.
    let hints = claim_hints_from_proto(&request).map_err(|e| *e)?;
    let task = self
      .protocol
      .claim_task_with_hints(worker_id, &hints)
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
    let request = request.into_inner();
    let worker_id = WorkerId::new(request.worker_id.clone())
      .map_err(|err| Status::invalid_argument(err.to_string()))?;
    let hints = claim_hints_from_proto(&request).map_err(|e| *e)?;
    let task = self
      .claim_task_with_hints(worker_id, &hints)
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
///
/// Q3.3.1: pre-fix this struct held `Arc<Mutex<Grpc<Channel>>>`,
/// serializing every outbound RPC behind a single async mutex.
/// `tonic::transport::Channel` is `Clone + Send + Sync` and is
/// explicitly designed for many concurrent in-flight RPCs over one
/// HTTP/2 connection; the mutex defeated that and the Q3.3.2
/// per-worker free_slots parallelism would have collapsed back to
/// serial as soon as the worker tried to fire its heartbeat and
/// report_result concurrently. Store the `Channel` directly and
/// build a fresh `Grpc::new(channel.clone())` per call — `Grpc` is
/// just a thin wrapper around the channel + codec, cloning the
/// channel is an `Arc::clone`.
#[derive(Debug, Clone)]
pub struct GrpcWorkerProtocol {
  channel: Channel,
  /// Q1.6.1: admission token sent as `authorization: Bearer <token>`
  /// gRPC metadata on every call. `None` keeps the legacy "no auth"
  /// path for the `memory://local` and dev profiles.
  admission_token: Option<Arc<str>>,
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
      channel,
      admission_token: None,
    })
  }

  pub fn from_channel(channel: Channel) -> Self {
    Self {
      channel,
      admission_token: None,
    }
  }

  /// Pin the admission token attached to every outgoing call as
  /// `authorization: Bearer <token>` metadata. Required when the
  /// server side runs [`AuthenticatedGrpcWorkerService`]; harmless
  /// (just ignored) when the server is the unauthenticated
  /// [`GrpcWorkerService`].
  pub fn with_admission_token(mut self, token: impl Into<String>) -> Self {
    self.admission_token = Some(Arc::from(token.into()));
    self
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
    // Q3.3.1: fresh `Grpc::new` per call — cloning the underlying
    // `Channel` is an `Arc::clone`, and `Grpc` itself just wraps
    // the channel + codec. The `.ready()` ping is preserved because
    // tonic's `Channel` uses an internal `Buffer<...>` whose
    // `Service::call` contract requires `poll_ready` to have been
    // observed first; the previous serializing mutex hid that
    // requirement from the call site but the constraint is real.
    // `ready()` itself is cheap — it just polls the buffer state.
    let mut inner = Grpc::new(self.channel.clone());
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
    if let Some(token) = self.admission_token.as_deref() {
      // Q1.6.1: every outbound RPC carries the admission credential.
      // We rebuild the value each call instead of storing an
      // AsciiMetadataValue because tonic's metadata map clones
      // cheaply but the value itself isn't `Sync` in all versions.
      let header = format!("Bearer {token}");
      match tonic::metadata::AsciiMetadataValue::try_from(header.as_str()) {
        Ok(value) => {
          req.metadata_mut().insert("authorization", value);
        }
        Err(_) => {
          // A token containing non-ASCII bytes is an operator error;
          // refuse to send instead of letting the server pin the
          // failure on a malformed header.
          return Err(SchedulerError::Transport {
            message: "admission token contains non-ASCII bytes; refusing to send".into(),
          });
        }
      }
    }
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
    // Pre-FU1 path: no hints. Equivalent to
    // `claim_task_with_hints(worker_id, &ClaimHints::none())`.
    self
      .claim_task_with_hints(worker_id, &crate::scheduler::ClaimHints::none())
      .await
  }

  async fn claim_task_with_hints(
    &self,
    worker_id: WorkerId,
    hints: &crate::scheduler::ClaimHints,
  ) -> Result<Option<WorkerTask>, SchedulerError> {
    let response = self
      .unary::<_, pb::ClaimTaskResponse>(
        "/agentflow.scheduler.v1.WorkerControl/ClaimTask",
        pb::ClaimTaskRequest {
          worker_id: worker_id.0,
          accepted_node_types: hints.capabilities.node_types.clone(),
          locality_run_id: hints
            .locality_run_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
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
    // P10.16.2-FU1: capability label round-trips across the wire.
    // `None` ↔ empty string so a pre-FU1 worker that ignores the
    // field still sees a coherent task.
    node_type: task.node_type.unwrap_or_default(),
  }
}

fn worker_task_from_proto(task: pb::WorkerTask) -> BoxedStatusResult<WorkerTask> {
  Ok(WorkerTask {
    task_id: parse_uuid(&task.task_id, "task_id")?,
    run_id: parse_uuid(&task.run_id, "run_id")?,
    node_id: task.node_id,
    attempt: task.attempt,
    payload: parse_json(&task.payload_json, "payload_json")?,
    // P10.16.2-FU1: empty string → None preserves the
    // "untagged task" semantic that
    // `WorkerCapabilities::accepts` always accepts.
    node_type: if task.node_type.is_empty() {
      None
    } else {
      Some(task.node_type)
    },
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
    // P10.16.2-FU1: heartbeats round-trip capability
    // advertisements. Empty `node_types` ↔ empty repeated
    // string, so pre-FU1 workers (which never set this field)
    // produce the same wire bytes as a worker explicitly
    // advertising "any task."
    accepted_node_types: heartbeat.capabilities.node_types,
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
    // P10.16.2-FU1: capability advertisement reads from the
    // wire. An empty list means "any task" (the pre-FU1 default)
    // — the policy layer in `InMemoryWorkerProtocol` and
    // `WorkerCapabilities::accepts` already handle that case.
    capabilities: crate::scheduler::WorkerCapabilities {
      node_types: heartbeat.accepted_node_types,
    },
  })
}

/// Convert a `pb::ClaimTaskRequest` into a [`ClaimHints`]
/// (P10.16.2-FU1). Empty values map cleanly to "no preference"
/// so pre-FU1 workers continue to get FIFO dispatch.
fn claim_hints_from_proto(
  request: &pb::ClaimTaskRequest,
) -> BoxedStatusResult<crate::scheduler::ClaimHints> {
  let locality_run_id = if request.locality_run_id.is_empty() {
    None
  } else {
    Some(parse_uuid(&request.locality_run_id, "locality_run_id")?)
  };
  Ok(crate::scheduler::ClaimHints {
    capabilities: crate::scheduler::WorkerCapabilities {
      node_types: request.accepted_node_types.clone(),
    },
    locality_run_id,
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
      accepted_node_types: Vec::new(),
      locality_run_id: String::new(),
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

#[cfg(test)]
mod hint_proto_tests {
  //! P10.16.2-FU1: wire-shape conversions for capability +
  //! locality hints. The transport-layer policy-routing test
  //! (capability filter actually filtering, locality preference
  //! actually winning) lives in `scheduler::mod.rs` against
  //! `InMemoryWorkerProtocol`; here we only assert the bytes
  //! round-trip cleanly so the trait method sees the same data
  //! the worker sent.
  use super::*;
  use crate::scheduler::{ClaimHints, WorkerCapabilities};

  #[test]
  fn worker_task_round_trip_preserves_node_type() {
    let task = WorkerTask::new(Uuid::new_v4(), "node-a", serde_json::json!({"k": "v"}))
      .with_node_type("template");
    let wire = worker_task_to_proto(task.clone());
    assert_eq!(wire.node_type, "template");
    let back = worker_task_from_proto(wire).expect("decode");
    assert_eq!(back.node_type.as_deref(), Some("template"));
    assert_eq!(back.task_id, task.task_id);
  }

  #[test]
  fn worker_task_round_trip_preserves_untagged() {
    // The pre-FU1 wire path: a worker that doesn't set
    // `node_type` ends up with `None`, not `Some("")` —
    // important because `WorkerCapabilities::accepts(None)`
    // always returns true (the untagged-task-always-accepted
    // invariant) while `accepts(Some(""))` would unhelpfully
    // never match.
    let task = WorkerTask::new(Uuid::new_v4(), "node-a", serde_json::json!({}));
    let wire = worker_task_to_proto(task);
    assert_eq!(wire.node_type, "", "untagged tasks encode as empty string");
    let back = worker_task_from_proto(wire).expect("decode");
    assert!(
      back.node_type.is_none(),
      "empty string decodes back to None"
    );
  }

  #[test]
  fn claim_hints_round_trip_carries_capabilities_and_locality() {
    let run = Uuid::new_v4();
    let hints = ClaimHints::default()
      .with_capabilities(WorkerCapabilities::for_node_types(["template", "file"]))
      .with_locality(run);
    let wire = pb::ClaimTaskRequest {
      worker_id: "w".into(),
      accepted_node_types: hints.capabilities.node_types.clone(),
      locality_run_id: hints
        .locality_run_id
        .map(|id| id.to_string())
        .unwrap_or_default(),
    };
    let decoded = claim_hints_from_proto(&wire).expect("decode");
    assert_eq!(
      decoded.capabilities.node_types,
      vec!["template".to_string(), "file".to_string()]
    );
    assert_eq!(decoded.locality_run_id, Some(run));
  }

  #[test]
  fn claim_hints_from_proto_default_means_no_hints() {
    // Pre-FU1 worker sends a bare `ClaimTaskRequest { worker_id }`
    // with default values everywhere else. The decoder must
    // produce a `ClaimHints` that's behaviorally identical to
    // `ClaimHints::none()`.
    let wire = pb::ClaimTaskRequest {
      worker_id: "legacy-worker".into(),
      accepted_node_types: Vec::new(),
      locality_run_id: String::new(),
    };
    let decoded = claim_hints_from_proto(&wire).expect("decode");
    assert!(decoded.capabilities.node_types.is_empty());
    assert!(decoded.locality_run_id.is_none());
    // The capability check on the decoded hint accepts any
    // task — same as `ClaimHints::none()` semantics.
    assert!(decoded.capabilities.accepts(Some("anything")));
    assert!(decoded.capabilities.accepts(None));
  }

  #[test]
  fn claim_hints_from_proto_rejects_malformed_locality_uuid() {
    let wire = pb::ClaimTaskRequest {
      worker_id: "w".into(),
      accepted_node_types: Vec::new(),
      locality_run_id: "not-a-uuid".into(),
    };
    let err = claim_hints_from_proto(&wire).expect_err("malformed uuid rejected");
    let status = *err;
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    assert!(status.message().contains("locality_run_id"));
  }

  #[test]
  fn heartbeat_round_trip_preserves_capabilities() {
    let hb = WorkerHeartbeat::now(WorkerId::new("w").unwrap(), None, 4)
      .with_capabilities(WorkerCapabilities::for_node_types(["template"]));
    let wire = worker_heartbeat_to_proto(hb);
    assert_eq!(
      wire.accepted_node_types,
      vec!["template".to_string()],
      "capability advertisement encodes to repeated string"
    );
    let back = worker_heartbeat_from_proto(wire).expect("decode");
    assert_eq!(
      back.capabilities.node_types,
      vec!["template".to_string()],
      "round-trip preserves the capability set"
    );
  }

  #[test]
  fn heartbeat_pre_fu1_default_decodes_as_any_capability() {
    // A heartbeat from a pre-FU1 worker has an empty
    // `accepted_node_types`. The decoder produces a default
    // `WorkerCapabilities` which accepts everything — preserves
    // the pre-FU1 admission semantics.
    let wire = pb::HeartbeatRequest {
      worker_id: "legacy".into(),
      active_task_id: String::new(),
      free_slots: 0,
      timestamp_rfc3339: String::new(),
      accepted_node_types: Vec::new(),
    };
    let back = worker_heartbeat_from_proto(wire).expect("decode");
    assert!(back.capabilities.node_types.is_empty());
    assert!(back.capabilities.accepts(Some("any")));
  }
}

#[cfg(test)]
mod proto_schema_drift_tests {
  //! Q3.3.3: pin `worker.proto` against the prost types so the
  //! audit-found drift (proto missing `node_type` /
  //! `accepted_node_types` / `locality_run_id`) cannot silently
  //! re-occur. Two layers of protection:
  //!
  //! 1. Build-time: `build.rs` generates `pb::*` from the .proto via
  //!    `tonic-build`. A field removed from the .proto fails to
  //!    compile because the rest of `grpc.rs` references it.
  //! 2. Test-time (this module): textually pin the .proto file to
  //!    the expected field/tag triples. Catches the inverse drift —
  //!    someone editing prost-side code by hand (after a future
  //!    refactor away from include_proto!) and forgetting the .proto.
  use super::*;
  use prost::Message as _;

  const WORKER_PROTO: &str = include_str!("../../proto/agentflow/scheduler/v1/worker.proto");

  /// Each tuple is `(field_name, tag = "N")`. Order doesn't matter
  /// but every entry must appear as a `... = N;` line in
  /// `worker.proto` for the wire shape to match the prost structs
  /// the rest of the file relies on.
  fn assert_proto_has_field(field: &str, tag: u32) {
    let needle = format!("{field} = {tag};");
    assert!(
      WORKER_PROTO.contains(&needle),
      "worker.proto missing field declaration `{needle}` — \
       drift between .proto and prost::pb detected (Q3.3.3)"
    );
  }

  #[test]
  fn worker_task_fields_pinned_to_proto_tags() {
    // Pre-FU1 fields.
    assert_proto_has_field("string task_id", 1);
    assert_proto_has_field("string run_id", 2);
    assert_proto_has_field("string node_id", 3);
    assert_proto_has_field("uint32 attempt", 4);
    assert_proto_has_field("string payload_json", 5);
    // P10.16.2-FU1 — these are the three the audit found missing
    // from the .proto before Q3.3.3.
    assert_proto_has_field("string node_type", 6);
  }

  #[test]
  fn claim_task_request_carries_capability_and_locality_fields() {
    assert_proto_has_field("string worker_id", 1);
    assert_proto_has_field("repeated string accepted_node_types", 2);
    assert_proto_has_field("string locality_run_id", 3);
  }

  #[test]
  fn heartbeat_request_carries_capability_advertisement() {
    assert_proto_has_field("string worker_id", 1);
    assert_proto_has_field("string active_task_id", 2);
    assert_proto_has_field("uint32 free_slots", 3);
    assert_proto_has_field("string timestamp_rfc3339", 4);
    assert_proto_has_field("repeated string accepted_node_types", 5);
  }

  /// The .proto declares `repeated WorkerTraceEvent events = 5` on
  /// `WorkerTaskResult`. The audit didn't flag this one but it's
  /// load-bearing — `report_result` round-trips event lists across
  /// the wire, so renumbering the field would silently drop events
  /// in mixed-deployment scenarios.
  #[test]
  fn worker_task_result_pins_status_and_events_tags() {
    assert_proto_has_field("Status status", 1);
    assert_proto_has_field("string output_json", 2);
    assert_proto_has_field("string error", 3);
    assert_proto_has_field("bool retryable", 4);
    assert_proto_has_field("repeated WorkerTraceEvent events", 5);
  }

  /// Proto3 + prost interop guarantee: encoding a message with the
  /// new fields populated and decoding into the same struct
  /// round-trips bit-for-bit. The build path uses
  /// `tonic::include_proto!`, so a successful build already implies
  /// the prost derive matches; we still re-prove the contract here
  /// against a representative non-default payload so a future change
  /// (e.g. swapping `string` ↔ `bytes`) would break a focused test
  /// before it broke wire compatibility.
  #[test]
  fn claim_task_request_with_new_fields_round_trips() {
    let original = pb::ClaimTaskRequest {
      worker_id: "worker-q333".into(),
      accepted_node_types: vec!["template".into(), "file".into()],
      locality_run_id: "11111111-1111-1111-1111-111111111111".into(),
    };
    let bytes = original.encode_to_vec();
    let decoded = pb::ClaimTaskRequest::decode(bytes.as_slice()).expect("decode");
    assert_eq!(decoded.worker_id, "worker-q333");
    assert_eq!(decoded.accepted_node_types, vec!["template", "file"]);
    assert_eq!(
      decoded.locality_run_id,
      "11111111-1111-1111-1111-111111111111"
    );
  }

  #[test]
  fn heartbeat_request_with_capabilities_round_trips() {
    let original = pb::HeartbeatRequest {
      worker_id: "worker-q333".into(),
      active_task_id: String::new(),
      free_slots: 4,
      timestamp_rfc3339: "2025-01-01T00:00:00Z".into(),
      accepted_node_types: vec!["llm".into()],
    };
    let bytes = original.encode_to_vec();
    let decoded = pb::HeartbeatRequest::decode(bytes.as_slice()).expect("decode");
    assert_eq!(decoded.free_slots, 4);
    assert_eq!(decoded.accepted_node_types, vec!["llm"]);
  }

  #[test]
  fn worker_task_with_node_type_round_trips() {
    let original = pb::WorkerTask {
      task_id: "00000000-0000-0000-0000-000000000001".into(),
      run_id: "00000000-0000-0000-0000-000000000002".into(),
      node_id: "node-x".into(),
      attempt: 2,
      payload_json: "{\"k\":1}".into(),
      node_type: "template".into(),
    };
    let bytes = original.encode_to_vec();
    let decoded = pb::WorkerTask::decode(bytes.as_slice()).expect("decode");
    assert_eq!(decoded.node_type, "template");
    assert_eq!(decoded.attempt, 2);
  }

  /// Pre-FU1 wire bytes (empty `node_type` / `accepted_node_types` /
  /// `locality_run_id`) must decode to the same default values
  /// `ClaimHints::none()` represents. Proto3 default semantics make
  /// these the zero value, but pinning the assumption with a
  /// regression test keeps a future codec swap honest.
  #[test]
  fn pre_fu1_empty_payload_decodes_to_defaults() {
    let empty_claim: Vec<u8> = pb::ClaimTaskRequest {
      worker_id: "legacy".into(),
      accepted_node_types: Vec::new(),
      locality_run_id: String::new(),
    }
    .encode_to_vec();
    let decoded = pb::ClaimTaskRequest::decode(empty_claim.as_slice()).expect("decode");
    assert_eq!(decoded.worker_id, "legacy");
    assert!(decoded.accepted_node_types.is_empty());
    assert!(decoded.locality_run_id.is_empty());
  }
}

#[cfg(test)]
mod grpc_concurrency_tests {
  //! Q3.3.1: prove that `GrpcWorkerProtocol` actually fires
  //! concurrent unary RPCs over the shared HTTP/2 channel.
  //! Pre-fix the inner `Mutex<Grpc<Channel>>` serialized every
  //! call; the test would see peak in-flight = 1. Post-fix the
  //! channel is cloned per call and tonic muxes the requests
  //! onto the same H/2 connection, so we observe peak ≥ 2.
  use super::*;
  use crate::scheduler::{WorkerHeartbeat, WorkerId, WorkerProtocol};
  use std::sync::atomic::{AtomicU32, Ordering};
  use std::time::Duration;
  use tokio::net::TcpListener;
  use tokio::sync::oneshot;
  use tonic::transport::Server;

  /// Counters held in Arc so the test can read them after handing
  /// `SlowConcurrencyServer` to `WorkerControlServer::new` (which
  /// consumes by value).
  #[derive(Clone, Default)]
  struct ConcurrencyCounters {
    in_flight: Arc<AtomicU32>,
    peak: Arc<AtomicU32>,
    total: Arc<AtomicU32>,
  }

  #[derive(Clone)]
  struct SlowConcurrencyServer {
    delay: Duration,
    counters: ConcurrencyCounters,
  }

  #[async_trait]
  impl WorkerControl for SlowConcurrencyServer {
    async fn submit_task(
      &self,
      _request: Request<pb::SubmitTaskRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
      // Not exercised by this test; reply Ok so the server type
      // is well-formed even if a stray RPC arrives.
      Ok(Response::new(pb::Empty {}))
    }
    async fn claim_task(
      &self,
      _request: Request<pb::ClaimTaskRequest>,
    ) -> Result<Response<pb::ClaimTaskResponse>, Status> {
      Ok(Response::new(pb::ClaimTaskResponse { task: None }))
    }
    async fn report_result(
      &self,
      _request: Request<pb::ReportResultRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
      Ok(Response::new(pb::Empty {}))
    }
    async fn heartbeat(
      &self,
      _request: Request<pb::HeartbeatRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
      let now = self.counters.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
      self.counters.peak.fetch_max(now, Ordering::SeqCst);
      tokio::time::sleep(self.delay).await;
      self.counters.in_flight.fetch_sub(1, Ordering::SeqCst);
      self.counters.total.fetch_add(1, Ordering::SeqCst);
      Ok(Response::new(pb::Empty {}))
    }
  }

  async fn unused_local_addr() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    addr
  }

  async fn connect_with_retry(endpoint: &str) -> GrpcWorkerProtocol {
    for _ in 0..20 {
      if let Ok(protocol) = GrpcWorkerProtocol::connect(endpoint).await {
        return protocol;
      }
      tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("connect retry exhausted");
  }

  /// Q3.3.1 regression — one `GrpcWorkerProtocol` cloned across 8
  /// async tasks must fire heartbeats concurrently. Pre-fix this
  /// pinned at peak in-flight = 1 because the inner Mutex
  /// serialized every call; post-fix the H/2 channel mux drives
  /// peak above 1 and 8 × 200ms RPCs finish well under the 1.6s
  /// serial bound.
  #[tokio::test]
  async fn grpc_protocol_fires_concurrent_heartbeats() {
    let counters = ConcurrencyCounters::default();
    let server = SlowConcurrencyServer {
      delay: Duration::from_millis(200),
      counters: counters.clone(),
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let addr = unused_local_addr().await;
    let server_task = tokio::spawn(async move {
      Server::builder()
        .add_service(WorkerControlServer::new(server))
        .serve_with_shutdown(addr, async {
          let _ = shutdown_rx.await;
        })
        .await
    });

    let endpoint = format!("http://{addr}");
    let protocol = connect_with_retry(&endpoint).await;
    let worker_id = WorkerId::new("worker-q331").unwrap();

    let started = std::time::Instant::now();
    let mut handles = Vec::with_capacity(8);
    for _ in 0..8 {
      let p = protocol.clone();
      let wid = worker_id.clone();
      handles.push(tokio::spawn(async move {
        p.heartbeat(WorkerHeartbeat::now(wid, None, 0)).await
      }));
    }
    for h in handles {
      h.await.expect("join").expect("heartbeat ok");
    }
    let elapsed = started.elapsed();

    let peak = counters.peak.load(Ordering::SeqCst);
    let total = counters.total.load(Ordering::SeqCst);
    assert_eq!(total, 8, "server must have received every heartbeat");
    assert!(
      peak >= 2,
      "Q3.3.1: peak in-flight must exceed 1 (concurrent RPCs); got {peak}"
    );
    // Serial wall clock bound: 8 × 200ms = 1.6s. Allow generous
    // headroom for handshake + scheduling; assert well below the
    // serial bound to leave no doubt the parallelism is real.
    assert!(
      elapsed < Duration::from_millis(1200),
      "Q3.3.1: 8 concurrent 200ms heartbeats must finish < 1.2s (serial would need 1.6s); took {elapsed:?}"
    );

    let _ = shutdown_tx.send(());
    server_task.await.unwrap().unwrap();
  }
}
