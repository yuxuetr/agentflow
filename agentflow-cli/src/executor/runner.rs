// Main workflow execution logic
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
// Removed unused Arc import
use std::time::Duration;
use tokio::fs;

use agentflow_core::{AsyncFlow, SharedState};
// Removed unused LLM client imports

use crate::config::workflow::{NodeDefinition, NodeType, WorkflowConfig, WorkflowType};
use crate::executor::context::ExecutionContext;
use crate::executor::nodes::{
  batch::BatchNode, file::FileNode, http::HttpNode, llm::LlmNode, template::TemplateNode,
};

pub struct WorkflowRunner {
  config: WorkflowConfig,
  execution_context: ExecutionContext,
}

impl WorkflowRunner {
  pub async fn new(workflow_file: &str) -> Result<Self> {
    // Load and parse workflow configuration
    let config_content = fs::read_to_string(workflow_file)
      .await
      .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    let config: WorkflowConfig = serde_yaml::from_str(&config_content)
      .with_context(|| format!("Failed to parse workflow file: {}", workflow_file))?;

    // Create execution context
    let execution_context = ExecutionContext::new(&config)?;

    Ok(Self {
      config,
      execution_context,
    })
  }

  pub async fn run(
    &self,
    inputs: HashMap<String, String>,
  ) -> Result<HashMap<String, serde_json::Value>> {
    println!("🔄 Starting workflow execution...");

    // Initialize shared state with inputs
    let shared_state = SharedState::new();
    println!("✅ Shared state initialized");

    self.populate_inputs(&shared_state, inputs)?;
    println!("✅ Inputs populated in shared state");

    // Create and execute the async flow
    println!("🔨 Building async flow...");
    let mut async_flow = self.build_async_flow().await?;
    println!("✅ Async flow built successfully");

    // Set execution configuration
    if let Some(config) = &self.config.config {
      if let Some(timeout_str) = &config.timeout {
        let timeout = self.parse_duration(timeout_str)?;
        async_flow.set_timeout(timeout);
      }
      if let Some(batch_size) = config.batch_size {
        async_flow.set_batch_size(batch_size);
      }
      if let Some(parallel_limit) = config.parallel_limit {
        async_flow.set_max_concurrent_batches(parallel_limit);
      }
    }

    // Enable tracing
    async_flow.enable_tracing(self.config.name.clone());

    // Execute the workflow
    println!("🚀 Starting async flow execution...");
    let execution_result = async_flow
      .run(&shared_state)
      .await
      .map_err(|e| {
        println!("❌ Async flow execution failed: {:?}", e);
        e
      })
      .context("Workflow execution failed")?;
    println!("✅ Async flow execution completed: {:?}", execution_result);

    // Process outputs
    let outputs = self.process_outputs(&shared_state).await?;

    Ok(outputs)
  }

  async fn build_async_flow(&self) -> Result<AsyncFlow> {
    println!("🏗️  Building nodes...");
    let nodes = self.build_nodes().await?;
    println!("✅ Built {} nodes", nodes.len());

    println!("📋 Workflow type: {:?}", self.config.workflow.workflow_type);
    match self.config.workflow.workflow_type {
      WorkflowType::Sequential => {
        if nodes.is_empty() {
          return Err(anyhow::anyhow!("No nodes defined in sequential workflow"));
        }

        // Create sequential flow with first node as start
        let start_node = nodes.into_iter().next().unwrap().1;
        let async_flow = AsyncFlow::new(start_node);

        // Add remaining nodes (this needs to be updated when we implement the nodes)
        // For now, this is a placeholder
        Ok(async_flow)
      }
      WorkflowType::Parallel => {
        // Create parallel flow with all nodes
        let node_list = nodes.into_iter().map(|(_, node)| node).collect();
        Ok(AsyncFlow::new_parallel(node_list))
      }
      WorkflowType::Conditional => {
        // Implement conditional logic based on first node's condition
        todo!("Conditional workflows not yet implemented")
      }
      WorkflowType::Mixed => {
        // Complex mixed workflow logic
        todo!("Mixed workflows not yet implemented")
      }
    }
  }

  async fn build_nodes(&self) -> Result<HashMap<String, Box<dyn agentflow_core::AsyncNode>>> {
    let mut nodes = HashMap::new();

    for node_def in &self.config.workflow.nodes {
      println!(
        "🔧 Creating node: {} (type: {:?})",
        node_def.name, node_def.node_type
      );
      let node = self.create_node(node_def).await?;
      println!("✅ Node '{}' created successfully", node_def.name);
      nodes.insert(node_def.name.clone(), node);
    }

    Ok(nodes)
  }

  async fn create_node(
    &self,
    node_def: &NodeDefinition,
  ) -> Result<Box<dyn agentflow_core::AsyncNode>> {
    match node_def.node_type {
      NodeType::Llm => {
        let llm_node = LlmNode::new(node_def).await?;
        Ok(Box::new(llm_node))
      }
      NodeType::Template => {
        let template_node = TemplateNode::new(node_def)?;
        Ok(Box::new(template_node))
      }
      NodeType::File => {
        let file_node = FileNode::new(node_def)?;
        Ok(Box::new(file_node))
      }
      NodeType::Http => {
        let http_node = HttpNode::new(node_def)?;
        Ok(Box::new(http_node))
      }
      NodeType::Batch => {
        let batch_node = BatchNode::new(node_def).await?;
        Ok(Box::new(batch_node))
      }
      NodeType::Conditional => {
        todo!("Conditional nodes not yet implemented")
      }
    }
  }

  fn populate_inputs(
    &self,
    shared_state: &SharedState,
    inputs: HashMap<String, String>,
  ) -> Result<()> {
    println!("📥 Populating inputs...");

    // Check required environment variables (but don't override existing ones)
    if let Some(env) = &self.config.environment {
      println!("🌍 Checking {} required environment variables", env.len());
      for (key, value) in env {
        if value == "required" {
          // This is just documentation - check if the env var exists
          if std::env::var(key).is_err() {
            println!(
              "⚠️  Warning: Required environment variable '{}' not set",
              key
            );
            println!("   Please set it in your environment or .env file");
          } else {
            println!("✅ Required environment variable '{}' is set", key);
          }
        } else {
          // Only set if it's not already set and not "required"
          if std::env::var(key).is_err() {
            println!("  Setting env var: {} = {}", key, value);
            std::env::set_var(key, value);
          } else {
            println!("  Environment variable '{}' already set, skipping", key);
          }
        }
      }
    }

    // Process and validate inputs
    if let Some(input_defs) = &self.config.inputs {
      println!("📋 Processing {} input definitions", input_defs.len());
      for (key, input_def) in input_defs {
        println!(
          "  Processing input: {} (type: {})",
          key, input_def.input_type
        );
        if let Some(value) = inputs.get(key) {
          // Parse value based on input type
          let parsed_value = match input_def.input_type.as_str() {
            "string" => serde_json::Value::String(value.clone()),
            "number" => {
              let num: f64 = value
                .parse()
                .with_context(|| format!("Invalid number for input '{}': {}", key, value))?;
              serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap())
            }
            "boolean" => {
              let bool_val: bool = value
                .parse()
                .with_context(|| format!("Invalid boolean for input '{}': {}", key, value))?;
              serde_json::Value::Bool(bool_val)
            }
            _ => serde_json::Value::String(value.clone()),
          };
          println!(
            "    Setting shared state: input_{} = {:?}",
            key, parsed_value
          );
          shared_state.insert(format!("input_{}", key), parsed_value);
        } else if input_def.required.unwrap_or(false) {
          println!("    ERROR: Required input '{}' not provided", key);
          return Err(anyhow::anyhow!("Required input '{}' not provided", key));
        } else if let Some(default_value) = &input_def.default {
          println!(
            "    Using default value for input_{}: {:?}",
            key, default_value
          );
          shared_state.insert(format!("input_{}", key), default_value.clone());
        } else {
          println!("    Input '{}' is optional and not provided", key);
        }
      }
    }

    // Add all provided inputs as-is
    println!(
      "📝 Adding {} raw input parameters to shared state",
      inputs.len()
    );
    for (key, value) in inputs {
      println!("    Adding: {} = {}", key, value);
      shared_state.insert(key, serde_json::Value::String(value));
    }

    println!("✅ populate_inputs completed successfully");
    Ok(())
  }

  async fn process_outputs(
    &self,
    shared_state: &SharedState,
  ) -> Result<HashMap<String, serde_json::Value>> {
    let mut outputs = HashMap::new();

    if let Some(output_defs) = &self.config.outputs {
      for (key, output_def) in output_defs {
        // Extract value based on source
        let value = self.extract_value_from_source(&output_def.source, shared_state)?;

        // Save to file if specified
        if let Some(file_path) = &output_def.file {
          self
            .save_output_to_file(file_path, &value, output_def.format.as_deref())
            .await?;
        }

        outputs.insert(key.clone(), value);
      }
    }

    Ok(outputs)
  }

  fn extract_value_from_source(
    &self,
    source: &str,
    shared_state: &SharedState,
  ) -> Result<serde_json::Value> {
    // Simple templating - expand later
    if source.starts_with("{{") && source.ends_with("}}") {
      let key = source
        .trim_start_matches("{{")
        .trim_end_matches("}}")
        .trim();
      shared_state
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("Source key '{}' not found in shared state", key))
    } else if source == "$" {
      // Return entire shared state
      let all_data: HashMap<String, serde_json::Value> = shared_state.iter().into_iter().collect();
      Ok(serde_json::Value::Object(serde_json::Map::from_iter(
        all_data.into_iter(),
      )))
    } else {
      // Direct key lookup
      shared_state
        .get(source)
        .ok_or_else(|| anyhow::anyhow!("Source key '{}' not found in shared state", source))
    }
  }

  async fn save_output_to_file(
    &self,
    file_path: &str,
    value: &serde_json::Value,
    format: Option<&str>,
  ) -> Result<()> {
    let content = match format {
      Some("json") => serde_json::to_string_pretty(value)?,
      Some("yaml") => serde_yaml::to_string(value)?,
      Some("text") | None => match value {
        serde_json::Value::String(s) => s.clone(),
        _ => serde_json::to_string_pretty(value)?,
      },
      _ => return Err(anyhow::anyhow!("Unsupported output format: {:?}", format)),
    };

    // Create parent directories if they don't exist
    if let Some(parent) = Path::new(file_path).parent() {
      fs::create_dir_all(parent)
        .await
        .with_context(|| format!("Failed to create output directory: {:?}", parent))?;
    }

    fs::write(file_path, content)
      .await
      .with_context(|| format!("Failed to write output file: {}", file_path))?;

    Ok(())
  }

  fn parse_duration(&self, duration_str: &str) -> Result<Duration> {
    // Simple duration parsing - extend as needed
    if duration_str.ends_with("ms") {
      let ms = duration_str.trim_end_matches("ms").parse::<u64>()?;
      Ok(Duration::from_millis(ms))
    } else if duration_str.ends_with("s") {
      let s = duration_str.trim_end_matches("s").parse::<u64>()?;
      Ok(Duration::from_secs(s))
    } else if duration_str.ends_with("m") {
      let m = duration_str.trim_end_matches("m").parse::<u64>()?;
      Ok(Duration::from_secs(m * 60))
    } else if duration_str.ends_with("h") {
      let h = duration_str.trim_end_matches("h").parse::<u64>()?;
      Ok(Duration::from_secs(h * 3600))
    } else {
      // Default to seconds
      let s = duration_str.parse::<u64>()?;
      Ok(Duration::from_secs(s))
    }
  }
}
