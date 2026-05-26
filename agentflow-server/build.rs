//! Q3.3.3: compile `proto/agentflow/scheduler/v1/worker.proto` so the
//! Rust prost message structs are generated from the .proto file
//! that non-Rust language bindings consume. This eliminates the drift
//! the audit found between `worker.proto` and the hand-written
//! structs in `src/scheduler/grpc.rs::pb`.
//!
//! Only message structs are generated; the tonic `Service` impl + the
//! `WorkerControl` trait remain hand-written in `grpc.rs` because
//! they layer custom W3C-traceparent scope handling + admission
//! credential extraction on top of the generated message types.
//!
//! Q5.1: build scripts are exempt from the workspace `unwrap_used` /
//! `expect_used` deny lint by convention — a build-script panic
//! surfaces as a clear CI compile error with the panic message, which
//! is exactly what we want when `protoc` is missing or `worker.proto`
//! is malformed. Annotated at file level so the panic message reaches
//! the operator without per-call clutter.
#![allow(
  clippy::expect_used,
  reason = "build script: panics surface as CI compile errors with the message intact"
)]

fn main() {
  let proto_root = "proto";
  let proto_file = "proto/agentflow/scheduler/v1/worker.proto";

  println!("cargo:rerun-if-changed={proto_file}");
  println!("cargo:rerun-if-changed=build.rs");

  // `build_client(false).build_server(false)` skips tonic's service
  // codegen so we don't collide with the hand-written `WorkerControl`
  // trait + `WorkerControlServer<T>` Tower service in `grpc.rs`.
  // Only `prost-build` runs underneath, emitting the message structs
  // with the same `prost(string, tag = "N")` derives the hand-written
  // path used.
  tonic_build::configure()
    .build_client(false)
    .build_server(false)
    .compile_protos(&[proto_file], &[proto_root])
    .expect("compile agentflow.scheduler.v1 worker.proto");
}
