use agentflow_core::{AsyncNode, SharedState};
use async_trait::async_trait;
use serde_json::Value;
use tokio::time::Duration;

/// Recipe fetching node that demonstrates async I/O operations
pub struct FetchRecipesNode {
  name: String,
  ingredient: Option<String>,
}

impl FetchRecipesNode {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      ingredient: None,
    }
  }

  pub fn with_ingredient(mut self, ingredient: &str) -> Self {
    self.ingredient = Some(ingredient.to_string());
    self
  }
}

#[async_trait]
impl AsyncNode for FetchRecipesNode {
  async fn prep_async(
    &self,
    shared: &SharedState,
  ) -> Result<Value, agentflow_core::AgentFlowError> {
    // Get ingredient from shared state or use default
    let ingredient = if let Some(ing) = &self.ingredient {
      ing.clone()
    } else if let Some(val) = shared.get("ingredient") {
      val.as_str().unwrap_or("chicken").to_string()
    } else {
      "chicken".to_string()
    };

    println!("🍳 Preparing to fetch recipes for: {}", ingredient);

    Ok(Value::String(ingredient))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, agentflow_core::AgentFlowError> {
    let ingredient = prep_result.as_str().unwrap();

    println!("🔍 Fetching recipes for {}...", ingredient);

    // Simulate async API call with delay
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Mock recipe API response
    let recipes = vec![
      format!("{} Stir Fry", ingredient),
      format!("Grilled {} with Herbs", ingredient),
      format!("Baked {} with Vegetables", ingredient),
      format!("{} Curry", ingredient),
      format!("Roasted {} with Spices", ingredient),
    ];

    println!("✅ Found {} recipes for {}", recipes.len(), ingredient);

    Ok(Value::Array(
      recipes.into_iter().map(Value::String).collect(),
    ))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, agentflow_core::AgentFlowError> {
    // Store both ingredient and recipes in shared state
    let ingredient = prep_result.as_str().unwrap();
    shared.insert(
      "ingredient".to_string(),
      Value::String(ingredient.to_string()),
    );
    shared.insert("recipes".to_string(), exec_result);

    println!("💾 Stored recipes and ingredient in shared state");

    Ok(Some("suggest".to_string())) // Continue to suggestion phase
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

/// Recipe suggestion node using LLM
pub struct SuggestRecipeNode {
  name: String,
  attempted_recipes: Vec<String>,
}

impl SuggestRecipeNode {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      attempted_recipes: Vec::new(),
    }
  }
}

#[async_trait]
impl AsyncNode for SuggestRecipeNode {
  async fn prep_async(
    &self,
    shared: &SharedState,
  ) -> Result<Value, agentflow_core::AgentFlowError> {
    // Get recipes from shared state
    let recipes =
      shared
        .get("recipes")
        .ok_or_else(|| agentflow_core::AgentFlowError::SharedStateError {
          message: "No recipes found in shared state".to_string(),
        })?;

    let ingredient = if let Some(ingredient_val) = shared.get("ingredient") {
      if let Some(s) = ingredient_val.as_str() {
        s.to_string()
      } else {
        "unknown ingredient".to_string()
      }
    } else {
      "unknown ingredient".to_string()
    };

    println!("🤔 Analyzing recipes for {}", ingredient);

    Ok(recipes)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, agentflow_core::AgentFlowError> {
    let recipes = prep_result.as_array().unwrap();

    println!("🧠 Getting LLM suggestion for best recipe...");

    // Simulate LLM call with delay
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Mock LLM response - choose a recipe that hasn't been attempted
    let mut available_recipes: Vec<String> = recipes
      .iter()
      .filter_map(|r| r.as_str())
      .map(|s| s.to_string())
      .filter(|r| !self.attempted_recipes.contains(r))
      .collect();

    if available_recipes.is_empty() {
      // Reset if all have been tried
      available_recipes = recipes
        .iter()
        .filter_map(|r| r.as_str())
        .map(|s| s.to_string())
        .collect();
    }

    // Choose the first available recipe (in a real app, LLM would choose intelligently)
    let suggested_recipe = available_recipes
      .first()
      .cloned()
      .unwrap_or_else(|| "No recipes available".to_string());

    println!("💡 LLM suggests: {}", suggested_recipe);

    Ok(Value::String(suggested_recipe))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, agentflow_core::AgentFlowError> {
    // Store the current suggestion
    shared.insert("current_suggestion".to_string(), exec_result);

    println!("💾 Stored recipe suggestion in shared state");

    Ok(Some("approval".to_string())) // Continue to approval phase
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

/// User approval node (simulated)
pub struct GetApprovalNode {
  name: String,
  auto_approve: bool,
  approval_rate: f32, // 0.0 to 1.0 - simulates user approval rate
}

impl GetApprovalNode {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      auto_approve: false,
      approval_rate: 0.7, // 70% approval rate by default
    }
  }

  pub fn with_auto_approve(mut self, auto_approve: bool) -> Self {
    self.auto_approve = auto_approve;
    self
  }

  pub fn with_approval_rate(mut self, rate: f32) -> Self {
    self.approval_rate = rate.clamp(0.0, 1.0);
    self
  }
}

#[async_trait]
impl AsyncNode for GetApprovalNode {
  async fn prep_async(
    &self,
    shared: &SharedState,
  ) -> Result<Value, agentflow_core::AgentFlowError> {
    // Get current suggestion
    let suggestion = shared.get("current_suggestion").ok_or_else(|| {
      agentflow_core::AgentFlowError::SharedStateError {
        message: "No suggestion found".to_string(),
      }
    })?;

    if let Some(recipe) = suggestion.as_str() {
      println!("👤 Asking user about: {}", recipe);
    }

    Ok(suggestion)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, agentflow_core::AgentFlowError> {
    let suggestion = prep_result.as_str().unwrap();

    println!("⏳ Waiting for user decision...");

    // Simulate user thinking time
    tokio::time::sleep(Duration::from_millis(300)).await;

    let approved = if self.auto_approve {
      // Simulate approval based on rate
      use std::collections::hash_map::DefaultHasher;
      use std::hash::{Hash, Hasher};

      let mut hasher = DefaultHasher::new();
      suggestion.hash(&mut hasher);
      let hash = hasher.finish();

      (hash as f32 / u64::MAX as f32) < self.approval_rate
    } else {
      // In a real implementation, this would get actual user input
      // For demo, we'll approve every other suggestion
      suggestion.contains("Grilled") || suggestion.contains("Curry")
    };

    Ok(Value::Bool(approved))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, agentflow_core::AgentFlowError> {
    let approved = exec_result.as_bool().unwrap();
    let suggestion = prep_result.as_str().unwrap();

    if approved {
      println!("✅ User approved: {}", suggestion);
      shared.insert(
        "final_recipe".to_string(),
        Value::String(suggestion.to_string()),
      );
      shared.insert(
        "workflow_status".to_string(),
        Value::String("completed".to_string()),
      );

      // Display final result
      if let Some(ingredient_val) = shared.get("ingredient") {
        if let Some(ingredient) = ingredient_val.as_str() {
          println!("\n🎉 Great choice! Here's your recipe:");
          println!("🥘 Recipe: {}", suggestion);
          println!("🛒 Main ingredient: {}", ingredient);
        }
      }

      Ok(None) // End the workflow
    } else {
      println!("❌ User rejected: {}", suggestion);
      println!("🔄 Let's try another recipe...");

      // Track attempted recipe to avoid suggesting it again
      let mut attempted = if let Some(attempted_val) = shared.get("attempted_recipes") {
        if let Some(arr) = attempted_val.as_array() {
          arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
        } else {
          Vec::new()
        }
      } else {
        Vec::new()
      };

      attempted.push(suggestion.to_string());
      shared.insert(
        "attempted_recipes".to_string(),
        Value::Array(attempted.iter().map(|s| Value::String(s.clone())).collect()),
      );

      Ok(Some("retry".to_string())) // Loop back to get another suggestion
    }
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

/// Conditional node that handles retry logic
pub struct RetryNode {
  name: String,
  max_retries: usize,
}

impl RetryNode {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      max_retries: 3,
    }
  }

  pub fn with_max_retries(mut self, max_retries: usize) -> Self {
    self.max_retries = max_retries;
    self
  }
}

#[async_trait]
impl AsyncNode for RetryNode {
  async fn prep_async(
    &self,
    shared: &SharedState,
  ) -> Result<Value, agentflow_core::AgentFlowError> {
    let attempted = if let Some(attempted_val) = shared.get("attempted_recipes") {
      if let Some(arr) = attempted_val.as_array() {
        arr.len()
      } else {
        0
      }
    } else {
      0
    };

    println!("🔄 Retry attempt {} of {}", attempted, self.max_retries);

    Ok(Value::Number(serde_json::Number::from(attempted)))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, agentflow_core::AgentFlowError> {
    let attempts = prep_result.as_u64().unwrap() as usize;

    // Check if we should continue or stop
    let should_continue = attempts < self.max_retries;

    if !should_continue {
      println!(
        "⚠️  Maximum retries ({}) reached. No suitable recipe found.",
        self.max_retries
      );
    }

    Ok(Value::Bool(should_continue))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, agentflow_core::AgentFlowError> {
    let should_continue = exec_result.as_bool().unwrap();

    if should_continue {
      Ok(Some("suggest".to_string())) // Try another suggestion
    } else {
      shared.insert(
        "workflow_status".to_string(),
        Value::String("max_retries_reached".to_string()),
      );
      println!("❌ Workflow ended - too many rejections");
      Ok(None) // End the workflow
    }
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

/// Recipe Finder Workflow Demo
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🍳 AgentFlow Core - Recipe Finder Workflow Demo");
  println!("===============================================\n");

  // Create shared state and populate inputs
  let shared_state = SharedState::new();
  shared_state.insert(
    "ingredient".to_string(),
    Value::String("chicken".to_string()),
  );

  println!("📝 Starting with ingredient: chicken");
  println!("🎯 Goal: Find an approved recipe\n");

  // Create workflow nodes
  let fetch_node = FetchRecipesNode::new("fetch_recipes").with_ingredient("chicken");

  let suggest_node = SuggestRecipeNode::new("suggest_recipe");

  let approval_node = GetApprovalNode::new("get_approval").with_approval_rate(0.6); // 60% approval rate for demo

  let retry_node = RetryNode::new("retry_handler").with_max_retries(4);

  // Execute workflow step by step with conditional logic
  println!("🚀 Starting Recipe Finder Workflow...\n");

  // Step 1: Fetch recipes
  match fetch_node.run_async(&shared_state).await {
    Ok(_) => println!("✅ Recipe fetching completed\n"),
    Err(e) => {
      println!("❌ Recipe fetching failed: {:?}", e);
      return Ok(());
    }
  }

  // Step 2: Suggestion and approval loop
  let mut workflow_complete = false;
  let mut iteration = 1;

  while !workflow_complete && iteration <= 5 {
    println!("🔄 Suggestion iteration {}", iteration);

    // Get suggestion
    match suggest_node.run_async(&shared_state).await {
      Ok(_) => println!("✅ Recipe suggestion completed"),
      Err(e) => {
        println!("❌ Recipe suggestion failed: {:?}", e);
        break;
      }
    }

    // Get approval
    match approval_node.run_async(&shared_state).await {
      Ok(_) => {
        // Check if workflow completed (user approved)
        if let Some(status) = shared_state.get("workflow_status") {
          if status.as_str() == Some("completed") {
            workflow_complete = true;
            break;
          }
        }

        // If not completed, check if we should retry
        match retry_node.run_async(&shared_state).await {
          Ok(_) => {
            if let Some(status) = shared_state.get("workflow_status") {
              if status.as_str() == Some("max_retries_reached") {
                break;
              }
            }
          }
          Err(e) => {
            println!("❌ Retry logic failed: {:?}", e);
            break;
          }
        }
      }
      Err(e) => {
        println!("❌ Approval process failed: {:?}", e);
        break;
      }
    }

    iteration += 1;
    println!(); // Add spacing between iterations
  }

  // Display final results
  println!("\n📊 Final Workflow State:");
  if let Some(final_recipe) = shared_state.get("final_recipe") {
    println!(
      "🏆 Selected Recipe: {}",
      final_recipe.as_str().unwrap_or("N/A")
    );
  }

  if let Some(status) = shared_state.get("workflow_status") {
    println!("📈 Status: {}", status.as_str().unwrap_or("unknown"));
  }

  if let Some(attempted) = shared_state.get("attempted_recipes") {
    if let Some(arr) = attempted.as_array() {
      if !arr.is_empty() {
        println!("🔄 Rejected recipes: {}", arr.len());
        for (i, recipe) in arr.iter().enumerate() {
          if let Some(name) = recipe.as_str() {
            println!("   {}. {}", i + 1, name);
          }
        }
      }
    }
  }

  println!("\n✅ Recipe Finder Workflow completed!");

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio;

  #[tokio::test]
  async fn test_fetch_recipes_node() {
    let shared_state = SharedState::new();
    shared_state.insert("ingredient".to_string(), Value::String("beef".to_string()));

    let fetch_node = FetchRecipesNode::new("test_fetch");

    let result = fetch_node.run_async(&shared_state).await;
    assert!(result.is_ok());

    // Check that recipes were stored
    assert!(shared_state.contains_key("recipes"));

    let recipes = shared_state.get("recipes").unwrap();
    assert!(recipes.is_array());
    assert!(recipes.as_array().unwrap().len() > 0);
  }

  #[tokio::test]
  async fn test_suggest_recipe_node() {
    let shared_state = SharedState::new();

    // Setup prerequisite data
    let recipes = vec![
      Value::String("Beef Stir Fry".to_string()),
      Value::String("Grilled Beef with Herbs".to_string()),
    ];
    shared_state.insert("recipes".to_string(), Value::Array(recipes));
    shared_state.insert("ingredient".to_string(), Value::String("beef".to_string()));

    let suggest_node = SuggestRecipeNode::new("test_suggest");

    let result = suggest_node.run_async(&shared_state).await;
    assert!(result.is_ok());

    // Check that suggestion was stored
    assert!(shared_state.contains_key("current_suggestion"));
  }

  #[tokio::test]
  async fn test_approval_node_auto_approve() {
    let shared_state = SharedState::new();
    shared_state.insert(
      "current_suggestion".to_string(),
      Value::String("Test Recipe".to_string()),
    );

    let approval_node = GetApprovalNode::new("test_approval")
      .with_auto_approve(true)
      .with_approval_rate(1.0); // Always approve

    let result = approval_node.run_async(&shared_state).await;
    assert!(result.is_ok());

    // Should have approved and set final recipe
    assert!(shared_state.contains_key("final_recipe"));
    assert_eq!(
      shared_state
        .get("workflow_status")
        .unwrap()
        .as_str()
        .unwrap(),
      "completed"
    );
  }

  #[tokio::test]
  async fn test_retry_node_logic() {
    let shared_state = SharedState::new();

    // Setup attempted recipes
    let attempted = vec![
      Value::String("Recipe 1".to_string()),
      Value::String("Recipe 2".to_string()),
    ];
    shared_state.insert("attempted_recipes".to_string(), Value::Array(attempted));

    let retry_node = RetryNode::new("test_retry").with_max_retries(3);

    let result = retry_node.run_async(&shared_state).await;
    assert!(result.is_ok());

    // Should continue since 2 < 3
    // (This test would need more sophisticated flow control to fully test)
  }

  #[tokio::test]
  async fn test_full_workflow_simulation() {
    let shared_state = SharedState::new();
    shared_state.insert(
      "ingredient".to_string(),
      Value::String("chicken".to_string()),
    );

    // Test fetch phase
    let fetch_node = FetchRecipesNode::new("fetch");
    assert!(fetch_node.run_async(&shared_state).await.is_ok());
    assert!(shared_state.contains_key("recipes"));

    // Test suggest phase
    let suggest_node = SuggestRecipeNode::new("suggest");
    assert!(suggest_node.run_async(&shared_state).await.is_ok());
    assert!(shared_state.contains_key("current_suggestion"));

    // Test approval phase with auto-approve
    let approval_node = GetApprovalNode::new("approve")
      .with_auto_approve(true)
      .with_approval_rate(1.0);
    assert!(approval_node.run_async(&shared_state).await.is_ok());
    assert!(shared_state.contains_key("final_recipe"));
  }
}
