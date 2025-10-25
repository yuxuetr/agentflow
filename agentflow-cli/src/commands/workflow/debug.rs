//! Workflow debugging and inspection commands

use crate::config::v2::FlowDefinitionV2;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;

/// Execute workflow debug command
pub async fn execute(
  workflow_file: String,
  visualize: bool,
  dry_run: bool,
  analyze: bool,
  validate: bool,
  plan: bool,
  verbose: bool,
) -> Result<()> {
  // If no specific flags, show all info
  let show_all = !visualize && !dry_run && !analyze && !validate && !plan;

  println!("🔍 Debugging workflow: {}\n", workflow_file);

  // Read and parse workflow file
  let yaml_content = fs::read_to_string(&workflow_file)
    .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

  let flow_def: FlowDefinitionV2 = serde_yaml::from_str(&yaml_content)
    .with_context(|| "Failed to parse workflow YAML")?;

  // Workflow validation
  if validate || show_all {
    println!("═══════════════════════════════════════════════════════════");
    println!("📋 WORKFLOW VALIDATION");
    println!("═══════════════════════════════════════════════════════════\n");
    validate_workflow(&flow_def, verbose)?;
    println!();
  }

  // Workflow visualization
  if visualize || show_all {
    println!("═══════════════════════════════════════════════════════════");
    println!("🌳 WORKFLOW VISUALIZATION");
    println!("═══════════════════════════════════════════════════════════\n");
    visualize_workflow(&flow_def, verbose)?;
    println!();
  }

  // Workflow analysis
  if analyze || show_all {
    println!("═══════════════════════════════════════════════════════════");
    println!("📊 WORKFLOW ANALYSIS");
    println!("═══════════════════════════════════════════════════════════\n");
    analyze_workflow(&flow_def, verbose)?;
    println!();
  }

  // Execution plan
  if plan || show_all {
    println!("═══════════════════════════════════════════════════════════");
    println!("📅 EXECUTION PLAN");
    println!("═══════════════════════════════════════════════════════════\n");
    show_execution_plan(&flow_def, verbose)?;
    println!();
  }

  // Dry run
  if dry_run {
    println!("═══════════════════════════════════════════════════════════");
    println!("🧪 DRY RUN");
    println!("═══════════════════════════════════════════════════════════\n");
    dry_run_workflow(&flow_def, verbose)?;
    println!();
  }

  println!("✅ Debug analysis complete!");
  Ok(())
}

/// Validate workflow configuration
fn validate_workflow(flow_def: &FlowDefinitionV2, verbose: bool) -> Result<()> {
  let mut issues = Vec::new();
  let mut warnings = Vec::new();

  // Check for empty workflow
  if flow_def.nodes.is_empty() {
    issues.push("Workflow has no nodes defined".to_string());
  }

  // Check for duplicate node IDs
  let mut seen_ids = HashSet::new();
  for node in &flow_def.nodes {
    if !seen_ids.insert(&node.id) {
      issues.push(format!("Duplicate node ID: '{}'", node.id));
    }
  }

  // Check for invalid dependencies
  let valid_ids: HashSet<_> = flow_def.nodes.iter().map(|n| &n.id).collect();
  for node in &flow_def.nodes {
    for dep in &node.dependencies {
      if !valid_ids.contains(dep) {
        issues.push(format!(
          "Node '{}' depends on non-existent node '{}'",
          node.id, dep
        ));
      }
    }
  }

  // Check for circular dependencies
  if let Err(e) = detect_cycles(flow_def) {
    issues.push(format!("Circular dependency detected: {}", e));
  }

  // Check for unreachable nodes (warnings)
  let reachable = find_reachable_nodes(flow_def);
  for node in &flow_def.nodes {
    if !reachable.contains(&node.id) {
      warnings.push(format!("Node '{}' may be unreachable", node.id));
    }
  }

  // Print results
  println!("Workflow: {}", flow_def.name);
  println!("Total nodes: {}", flow_def.nodes.len());
  println!();

  if issues.is_empty() && warnings.is_empty() {
    println!("✅ No validation issues found");
  } else {
    if !issues.is_empty() {
      println!("❌ Issues found: {}", issues.len());
      for (i, issue) in issues.iter().enumerate() {
        println!("  {}. {}", i + 1, issue);
      }
      println!();
    }

    if !warnings.is_empty() {
      println!("⚠️  Warnings: {}", warnings.len());
      for (i, warning) in warnings.iter().enumerate() {
        println!("  {}. {}", i + 1, warning);
      }
      println!();
    }
  }

  if verbose {
    println!("Node types summary:");
    let mut type_counts: HashMap<&str, usize> = HashMap::new();
    for node in &flow_def.nodes {
      *type_counts.entry(&node.node_type).or_insert(0) += 1;
    }
    for (node_type, count) in type_counts {
      println!("  - {}: {}", node_type, count);
    }
  }

  Ok(())
}

/// Visualize workflow as text-based DAG
fn visualize_workflow(flow_def: &FlowDefinitionV2, verbose: bool) -> Result<()> {
  println!("Workflow: {}", flow_def.name);
  println!();

  // Build dependency graph
  let dep_graph = build_dependency_graph(flow_def);

  // Find root nodes (nodes with no dependencies)
  let roots: Vec<_> = flow_def
    .nodes
    .iter()
    .filter(|n| n.dependencies.is_empty())
    .collect();

  if roots.is_empty() {
    println!("⚠️  No root nodes found (all nodes have dependencies)");
    println!();
  }

  // Print tree structure
  let mut visited = HashSet::new();
  for root in &roots {
    print_node_tree(root.id.as_str(), flow_def, &dep_graph, &mut visited, 0, verbose);
  }

  // Print dependency summary
  println!("\nDependencies:");
  let mut has_deps = false;
  for node in &flow_def.nodes {
    if !node.dependencies.is_empty() {
      has_deps = true;
      println!("  {} ← {:?}", node.id, node.dependencies);
    }
  }
  if !has_deps {
    println!("  (No dependencies defined)");
  }

  Ok(())
}

/// Analyze workflow structure and complexity
fn analyze_workflow(flow_def: &FlowDefinitionV2, verbose: bool) -> Result<()> {
  // Calculate metrics
  let total_nodes = flow_def.nodes.len();
  let total_deps: usize = flow_def.nodes.iter().map(|n| n.dependencies.len()).sum();
  let avg_deps = if total_nodes > 0 {
    total_deps as f64 / total_nodes as f64
  } else {
    0.0
  };

  let max_deps = flow_def
    .nodes
    .iter()
    .map(|n| n.dependencies.len())
    .max()
    .unwrap_or(0);

  // Calculate workflow depth
  let depth = calculate_depth(flow_def);

  // Find bottlenecks (nodes with many dependents)
  let dependents_count = count_dependents(flow_def);
  let max_dependents = dependents_count.values().max().copied().unwrap_or(0);

  println!("Workflow Metrics:");
  println!("  Total nodes:        {}", total_nodes);
  println!("  Total dependencies: {}", total_deps);
  println!("  Average dependencies per node: {:.1}", avg_deps);
  println!("  Max dependencies on single node: {}", max_deps);
  println!("  Workflow depth:     {} levels", depth);
  println!("  Max dependents:     {}", max_dependents);
  println!();

  // Identify potential bottlenecks
  if max_dependents > 3 {
    println!("⚠️  Potential bottlenecks (nodes with many dependents):");
    for (node_id, count) in &dependents_count {
      if *count > 3 {
        println!("  - '{}': {} nodes depend on it", node_id, count);
      }
    }
    println!();
  }

  // Show parallelism opportunities
  let parallel_levels = find_parallel_levels(flow_def);
  if verbose && !parallel_levels.is_empty() {
    println!("Parallelism opportunities:");
    for (level, nodes) in parallel_levels.iter().enumerate() {
      if nodes.len() > 1 {
        println!("  Level {}: {} nodes can run in parallel", level, nodes.len());
        println!("    {:?}", nodes);
      }
    }
    println!();
  }

  // Node type distribution
  println!("Node Type Distribution:");
  let mut type_counts: HashMap<&str, usize> = HashMap::new();
  for node in &flow_def.nodes {
    *type_counts.entry(&node.node_type).or_insert(0) += 1;
  }
  for (node_type, count) in type_counts {
    let percentage = (count as f64 / total_nodes as f64) * 100.0;
    println!("  - {:12} : {:3} ({:5.1}%)", node_type, count, percentage);
  }

  Ok(())
}

/// Show estimated execution plan
fn show_execution_plan(flow_def: &FlowDefinitionV2, verbose: bool) -> Result<()> {
  let levels = find_parallel_levels(flow_def);

  println!("Estimated Execution Plan:");
  println!("(Nodes at the same level can execute in parallel)\n");

  for (level, nodes) in levels.iter().enumerate() {
    println!("Level {} ({} node{}):",
      level,
      nodes.len(),
      if nodes.len() == 1 { "" } else { "s" }
    );

    for node_id in nodes {
      let node = flow_def.nodes.iter().find(|n| &n.id == node_id).unwrap();

      if verbose {
        let deps_str = if node.dependencies.is_empty() {
          String::from("(no dependencies)")
        } else {
          format!("depends on: {:?}", node.dependencies)
        };
        println!("  ├─ [{}] {} - {}", node.node_type, node.id, deps_str);
      } else {
        println!("  ├─ [{}] {}", node.node_type, node.id);
      }
    }
    println!();
  }

  println!("Total execution levels: {}", levels.len());
  println!("Maximum parallelism: {}", levels.iter().map(|l| l.len()).max().unwrap_or(0));

  Ok(())
}

/// Perform dry run simulation
fn dry_run_workflow(flow_def: &FlowDefinitionV2, verbose: bool) -> Result<()> {
  println!("Simulating workflow execution: {}", flow_def.name);
  println!();

  let levels = find_parallel_levels(flow_def);

  for (level, nodes) in levels.iter().enumerate() {
    println!("📍 Level {} - Executing {} node(s)", level, nodes.len());

    for node_id in nodes {
      let node = flow_def.nodes.iter().find(|n| &n.id == node_id).unwrap();

      if verbose {
        println!("  ▶️  Node: {}", node.id);
        println!("      Type: {}", node.node_type);
        if !node.dependencies.is_empty() {
          println!("      Dependencies: {:?}", node.dependencies);
        }
        if !node.parameters.is_empty() {
          println!("      Parameters: {} configured", node.parameters.len());
        }
        println!("      ✓ Simulation passed");
      } else {
        println!("  ✓ {}", node.id);
      }
    }
    println!();
  }

  println!("✅ Dry run completed successfully");
  println!("   Total levels: {}", levels.len());
  println!("   Total nodes: {}", flow_def.nodes.len());

  Ok(())
}

// Helper functions

fn build_dependency_graph(flow_def: &FlowDefinitionV2) -> HashMap<String, Vec<String>> {
  let mut graph: HashMap<String, Vec<String>> = HashMap::new();

  for node in &flow_def.nodes {
    graph.entry(node.id.clone()).or_insert_with(Vec::new);
    for dep in &node.dependencies {
      graph.entry(dep.clone())
        .or_insert_with(Vec::new)
        .push(node.id.clone());
    }
  }

  graph
}

fn print_node_tree(
  node_id: &str,
  flow_def: &FlowDefinitionV2,
  dep_graph: &HashMap<String, Vec<String>>,
  visited: &mut HashSet<String>,
  indent: usize,
  verbose: bool,
) {
  if visited.contains(node_id) {
    return;
  }
  visited.insert(node_id.to_string());

  let node = flow_def.nodes.iter().find(|n| n.id == node_id);
  if let Some(node) = node {
    let prefix = "  ".repeat(indent);
    let symbol = if indent == 0 { "├─" } else { "└─" };

    if verbose {
      println!("{}{} [{}] {}", prefix, symbol, node.node_type, node.id);
      if !node.parameters.is_empty() {
        println!("{}   └─ {} parameter(s)", prefix, node.parameters.len());
      }
    } else {
      println!("{}{} [{}] {}", prefix, symbol, node.node_type, node.id);
    }

    // Print dependents
    if let Some(dependents) = dep_graph.get(node_id) {
      for dependent in dependents {
        print_node_tree(dependent, flow_def, dep_graph, visited, indent + 1, verbose);
      }
    }
  }
}

fn detect_cycles(flow_def: &FlowDefinitionV2) -> Result<(), String> {
  fn visit(
    node_id: &str,
    flow_def: &FlowDefinitionV2,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
  ) -> Result<(), String> {
    visited.insert(node_id.to_string());
    rec_stack.insert(node_id.to_string());

    let node = flow_def.nodes.iter().find(|n| n.id == node_id);
    if let Some(node) = node {
      for dep in &node.dependencies {
        if !visited.contains(dep) {
          visit(dep, flow_def, visited, rec_stack)?;
        } else if rec_stack.contains(dep) {
          return Err(format!("{} -> {}", node_id, dep));
        }
      }
    }

    rec_stack.remove(node_id);
    Ok(())
  }

  let mut visited = HashSet::new();
  let mut rec_stack = HashSet::new();

  for node in &flow_def.nodes {
    if !visited.contains(&node.id) {
      visit(&node.id, flow_def, &mut visited, &mut rec_stack)?;
    }
  }

  Ok(())
}

fn find_reachable_nodes(flow_def: &FlowDefinitionV2) -> HashSet<String> {
  let mut reachable = HashSet::new();

  // Start from root nodes
  let roots: Vec<_> = flow_def
    .nodes
    .iter()
    .filter(|n| n.dependencies.is_empty())
    .map(|n| n.id.clone())
    .collect();

  let dep_graph = build_dependency_graph(flow_def);

  fn dfs(
    node_id: &str,
    dep_graph: &HashMap<String, Vec<String>>,
    reachable: &mut HashSet<String>,
  ) {
    reachable.insert(node_id.to_string());
    if let Some(dependents) = dep_graph.get(node_id) {
      for dependent in dependents {
        if !reachable.contains(dependent) {
          dfs(dependent, dep_graph, reachable);
        }
      }
    }
  }

  for root in &roots {
    dfs(root, &dep_graph, &mut reachable);
  }

  reachable
}

fn calculate_depth(flow_def: &FlowDefinitionV2) -> usize {
  let levels = find_parallel_levels(flow_def);
  levels.len()
}

fn count_dependents(flow_def: &FlowDefinitionV2) -> HashMap<String, usize> {
  let mut counts: HashMap<String, usize> = HashMap::new();

  for node in &flow_def.nodes {
    for dep in &node.dependencies {
      *counts.entry(dep.clone()).or_insert(0) += 1;
    }
  }

  counts
}

fn find_parallel_levels(flow_def: &FlowDefinitionV2) -> Vec<Vec<String>> {
  let mut levels: Vec<Vec<String>> = Vec::new();
  let mut scheduled: HashSet<String> = HashSet::new();

  loop {
    let mut current_level = Vec::new();

    for node in &flow_def.nodes {
      if scheduled.contains(&node.id) {
        continue;
      }

      // Check if all dependencies are scheduled
      let all_deps_scheduled = node.dependencies.iter().all(|d| scheduled.contains(d));

      if all_deps_scheduled {
        current_level.push(node.id.clone());
      }
    }

    if current_level.is_empty() {
      break;
    }

    for node_id in &current_level {
      scheduled.insert(node_id.clone());
    }

    levels.push(current_level);
  }

  levels
}
