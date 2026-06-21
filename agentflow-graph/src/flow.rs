//! The workflow IR: `Flow`, `GraphNode`, and `NodeType`.
//!
//! These are the *data* a workflow is built from. The executor that runs a
//! `Flow` — the topological / concurrent scheduler, checkpointing, resume — lives
//! in `agentflow-core` as the `FlowExt` trait (RFC §5: IR ≠ executor). Keeping the
//! `Flow` *type* here lets a runtime construct one by depending on `agentflow-graph`
//! alone, which is the dynamic-workflow prerequisite.
//!
//! Builders that need executor logic (e.g. `with_checkpointing`, which validates
//! the checkpoint dir) live on `agentflow_core::FlowExt`; the no-validation
//! [`Flow::with_checkpoint_config`] setter here is what that builder calls.

use crate::async_node::AsyncNode;
use crate::checkpoint::CheckpointConfig;
use crate::events::EventListener;
use crate::state_size::StateSizeObserver;
use crate::value::FlowValue;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub enum NodeType {
  Standard(Arc<dyn AsyncNode>),
  Map {
    template: Vec<GraphNode>,
    parallel: bool,
    /// Upper bound on concurrently-running sub-flows when
    /// `parallel == true`. `None` means unbounded (legacy
    /// behaviour). F-A6-1: unbounded `tokio::spawn` per item
    /// shreds provider rate limits at N>~3, so production
    /// callers should always set this. Ignored when
    /// `parallel == false`.
    max_concurrent: Option<usize>,
  },
  While {
    condition: String,
    max_iterations: u32,
    template: Vec<GraphNode>,
  },
}

#[derive(Clone)]
pub struct GraphNode {
  pub id: String,
  pub node_type: NodeType,
  pub dependencies: Vec<String>,
  pub input_mapping: Option<HashMap<String, (String, String)>>,
  pub run_if: Option<String>,
  pub initial_inputs: HashMap<String, FlowValue>,
}

#[derive(Default, Clone)]
pub struct Flow {
  nodes: HashMap<String, GraphNode>,
  checkpoint_enabled: bool,
  // The checkpoint *config* (IR data), not a live manager — the executor
  // (`agentflow-core`) rebuilds the stateless `CheckpointManager` from it on
  // demand (P-A1.3 step 2d-i).
  checkpoint_config: Option<CheckpointConfig>,
  event_listener: Option<Arc<dyn EventListener>>,
  state_size_observer: Option<Arc<dyn StateSizeObserver>>,
}

impl Flow {
  pub fn new(nodes: Vec<GraphNode>) -> Self {
    let nodes_map = nodes.into_iter().map(|n| (n.id.clone(), n)).collect();
    Self {
      nodes: nodes_map,
      checkpoint_enabled: false,
      checkpoint_config: None,
      event_listener: None,
      state_size_observer: None,
    }
  }

  pub fn add_node(&mut self, node: GraphNode) {
    self.nodes.insert(node.id.clone(), node);
  }

  /// Attach a workflow event listener for tracing, metrics, or logs.
  pub fn with_event_listener(mut self, listener: Arc<dyn EventListener>) -> Self {
    self.event_listener = Some(listener);
    self
  }

  /// Attach a [`StateSizeObserver`] (P10.14.2-FU6) that receives the
  /// estimated state-pool byte count after every node completes.
  pub fn with_state_size_observer(mut self, observer: Arc<dyn StateSizeObserver>) -> Self {
    self.state_size_observer = Some(observer);
    self
  }

  /// Store a checkpoint configuration and enable checkpointing.
  ///
  /// This is the unvalidated IR setter; `agentflow_core::FlowExt::with_checkpointing`
  /// validates the config (builds a manager to fail fast) before calling it.
  pub fn with_checkpoint_config(mut self, config: CheckpointConfig) -> Self {
    self.checkpoint_enabled = true;
    self.checkpoint_config = Some(config);
    self
  }

  // Read accessors for the execution engine. The executor (in `agentflow-core`)
  // reads the flow's state through these across the crate boundary; the builders
  // above keep direct field access because they live here with the struct.
  /// The workflow's nodes, keyed by node id.
  pub fn nodes(&self) -> &HashMap<String, GraphNode> {
    &self.nodes
  }
  /// Whether checkpointing was enabled.
  pub fn is_checkpoint_enabled(&self) -> bool {
    self.checkpoint_enabled
  }
  /// The checkpoint configuration, if checkpointing is enabled.
  pub fn checkpoint_config(&self) -> Option<&CheckpointConfig> {
    self.checkpoint_config.as_ref()
  }
  /// The attached workflow event listener, if any.
  pub fn event_listener(&self) -> Option<&Arc<dyn EventListener>> {
    self.event_listener.as_ref()
  }
  /// The attached state-size observer, if any.
  pub fn state_size_observer(&self) -> Option<&Arc<dyn StateSizeObserver>> {
    self.state_size_observer.as_ref()
  }
}
