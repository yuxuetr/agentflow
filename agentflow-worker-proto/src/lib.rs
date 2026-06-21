//! `agentflow-worker-proto` — the worker control-plane contract.
//!
//! The narrow interface the distributed-execution server control plane and the
//! `agentflow-worker` process agree on: the [`WorkerProtocol`] trait, its wire
//! types ([`WorkerTask`], [`WorkerTaskResult`], [`WorkerHeartbeat`], …), the
//! [`SchedulerError`] surface, the in-memory protocol used in tests, the
//! `NodeExecutionPayload`, and the gRPC client ([`GrpcWorkerProtocol`]) plus its
//! generated [`pb`] message types.
//!
//! Extracted from `agentflow-server` in P-A2.3 so `agentflow-worker` depends on
//! the shared contract instead of the gateway crate (burning the `worker ->
//! server` edge). The gateway keeps the *control plane* (`WorkerControlPlane`,
//! the distributed scheduler) and the gRPC *server* (`WorkerControlServer`),
//! re-exporting the contract from here under their original paths.

/// Generated protobuf message structs for `agentflow.scheduler.v1`.
///
/// Only the message structs are generated (`build_client(false)
/// .build_server(false)`); the tower `Service` impl + the `WorkerControl`
/// trait stay hand-written in `agentflow-server`.
pub mod pb {
  tonic::include_proto!("agentflow.scheduler.v1");
}

pub mod grpc;
pub mod protocol;

pub use grpc::{
  GrpcWorkerProtocol, extract_traceparent_from_grpc_request, inject_traceparent_into_grpc_request,
  run_in_traceparent_scope,
};
pub use protocol::*;
