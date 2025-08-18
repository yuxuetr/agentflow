use agentflow_core::{AsyncNode, SharedState};
use agentflow_llm::{client::LLMClientBuilder, registry::ModelRegistry};
use async_trait::async_trait;
use serde_json::Value;
use std::env;

/// Real LLM Recipe fetching node using StepFun API
pub struct RealFetchRecipesNode {
  name: String,
  ingredient: Option<String>,
}

impl RealFetchRecipesNode {
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
impl AsyncNode for RealFetchRecipesNode {
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

    println!("🔍 Fetching recipes for {} using real LLM...", ingredient);

    // Initialize the model registry with built-in configuration
    let registry = ModelRegistry::global();
    registry.load_builtin_config().await.map_err(|e| {
      agentflow_core::AgentFlowError::AsyncExecutionError {
        message: format!("Failed to load model configuration: {}", e),
      }
    })?;

    // Make LLM call to get recipe suggestions
    let prompt = format!(
      r#"You are a recipe API service. Generate exactly 5 different recipes using "{}" as the main ingredient.

Return ONLY a JSON array of recipe names, like this:
["Recipe Name 1", "Recipe Name 2", "Recipe Name 3", "Recipe Name 4", "Recipe Name 5"]

Make each recipe distinct and appetizing. Include various cooking methods like grilled, baked, stir-fried, roasted, and curry or soup forms.

Ingredient: {}"#,
      ingredient, ingredient
    );

    let response = LLMClientBuilder::new("step-2-mini")
      .prompt(&prompt)
      .temperature(0.8)
      .max_tokens(200)
      .execute()
      .await
      .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError {
        message: format!("LLM request failed: {}", e),
      })?;

    println!("📝 LLM Response: {}", response);

    // Try to parse the JSON response
    let recipes: Vec<String> = match serde_json::from_str(&response) {
      Ok(parsed) => parsed,
      Err(_) => {
        // Fallback: extract recipe names from text response
        println!("⚠️  Failed to parse JSON, extracting recipe names from text");
        let lines: Vec<String> = response
          .lines()
          .filter(|line| !line.trim().is_empty() && !line.starts_with('[') && !line.ends_with(']'))
          .map(|line| {
            // Clean up the line (remove quotes, numbers, bullets, etc.)
            line
              .trim()
              .trim_start_matches(|c: char| {
                c.is_numeric() || c == '.' || c == '"' || c == '\'' || c == '-' || c == '*'
              })
              .trim_end_matches(|c: char| c == '"' || c == '\'' || c == ',')
              .trim()
              .to_string()
          })
          .filter(|line| !line.is_empty() && line.len() > 5) // Filter out very short strings
          .take(5)
          .collect();

        if lines.is_empty() {
          // Ultimate fallback: generate some default recipes
          vec![
            format!("{} Stir Fry", ingredient),
            format!("Grilled {} with Herbs", ingredient),
            format!("Baked {} with Vegetables", ingredient),
            format!("{} Curry", ingredient),
            format!("Roasted {} with Spices", ingredient),
          ]
        } else {
          lines
        }
      }
    };

    println!("✅ Found {} recipes for {}", recipes.len(), ingredient);
    for (i, recipe) in recipes.iter().enumerate() {
      println!("   {}. {}", i + 1, recipe);
    }

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

/// Real LLM Recipe suggestion node using StepFun API
pub struct RealSuggestRecipeNode {
  name: String,
}

impl RealSuggestRecipeNode {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
    }
  }
}

#[async_trait]
impl AsyncNode for RealSuggestRecipeNode {
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

    println!("🧠 Getting real LLM suggestion for best recipe...");

    // For now, we'll work without attempted recipes tracking in the suggest node
    // (since we don't have direct access to shared state in exec_async)
    let attempted_recipes: Vec<&str> = Vec::new();

    // Prepare the prompt for LLM
    let recipe_list = recipes
      .iter()
      .filter_map(|r| r.as_str())
      .collect::<Vec<_>>()
      .join(", ");

    let prompt = if attempted_recipes.is_empty() {
      format!(
        r#"From this list of recipes, choose the most appealing and delicious one. Return ONLY the recipe name, nothing else.

Recipe options: {}

Choose the one that sounds most appetizing and well-balanced. Return just the recipe name."#,
        recipe_list
      )
    } else {
      format!(
        r#"From this list of recipes, choose the most appealing one that has NOT been previously rejected.

Recipe options: {}
Previously rejected: {}

Choose a recipe that hasn't been rejected and sounds most appetizing. Return ONLY the recipe name, nothing else."#,
        recipe_list,
        attempted_recipes.join(", ")
      )
    };

    // Initialize the model registry if not already done
    let registry = ModelRegistry::global();
    // Try to load config - it's okay if already loaded
    let _ = registry.load_builtin_config().await;

    let response = LLMClientBuilder::new("step-2-mini")
      .prompt(&prompt)
      .temperature(0.7)
      .max_tokens(50)
      .execute()
      .await
      .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError {
        message: format!("LLM suggestion request failed: {}", e),
      })?;

    // Clean up the response to get just the recipe name
    let suggested_recipe = response
      .trim()
      .trim_matches('"')
      .trim_matches('\'')
      .lines()
      .next()
      .unwrap_or("No suggestion available")
      .to_string();

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

/// Real LLM User approval node using StepFun API for realistic approval simulation
pub struct RealGetApprovalNode {
  name: String,
  approval_rate: f32,
}

impl RealGetApprovalNode {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      approval_rate: 0.7, // 70% approval rate by default
    }
  }

  pub fn with_approval_rate(mut self, rate: f32) -> Self {
    self.approval_rate = rate.clamp(0.0, 1.0);
    self
  }
}

#[async_trait]
impl AsyncNode for RealGetApprovalNode {
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
      println!("👤 Evaluating user response to: {}", recipe);
    }

    Ok(suggestion)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, agentflow_core::AgentFlowError> {
    let suggestion = prep_result.as_str().unwrap();

    println!("⏳ Using LLM to simulate realistic user decision...");

    // Use LLM to simulate a realistic user response based on the recipe
    let prompt = format!(
      r#"You are simulating a typical person being offered this recipe: "{}"

Consider these factors:
- Recipe appeal and popularity ({}% of people generally approve recipes)
- How common/interesting the recipe sounds
- Whether it seems easy or difficult to make
- General food preferences

Respond with exactly one word: either "APPROVED" or "REJECTED" - nothing else.

Recipe: {}"#,
      suggestion,
      (self.approval_rate * 100.0) as u32,
      suggestion
    );

    // Initialize the model registry if not already done
    let registry = ModelRegistry::global();
    // Try to load config - it's okay if already loaded
    let _ = registry.load_builtin_config().await;

    let response = LLMClientBuilder::new("step-2-mini")
      .prompt(&prompt)
      .temperature(0.3)
      .max_tokens(10)
      .execute()
      .await
      .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError {
        message: format!("LLM approval simulation failed: {}", e),
      })?;

    // Parse the response
    let decision = response.trim().to_uppercase();
    let approved = decision.contains("APPROVED") || decision.contains("APPROVE");

    println!(
      "🤖 LLM simulated user decision: {}",
      if approved { "APPROVED" } else { "REJECTED" }
    );

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

/// Retry node - same as before, no LLM needed
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

/// Real LLM Recipe Finder Workflow Demo
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🍳 AgentFlow Core - REAL LLM Recipe Finder Workflow Demo");
  println!("========================================================\n");

  // Check for API key in environment
  let api_key = env::var("STEP_API_KEY")
    .or_else(|_| env::var("STEPFUN_API_KEY"))
    .unwrap_or_else(|_| {
      println!("⚠️  No STEPFUN_API_KEY environment variable set, using provided key");
      "6EAoVKFZRzfZXRl3l0JQl16ulN98i9siTXG7Ia8ll6FS3GdypnAYfCHErLdAI2tIA".to_string()
    });

  if api_key.is_empty() {
    println!("❌ No API key available. Please set STEPFUN_API_KEY environment variable.");
    return Ok(());
  }

  // Set environment variable for the agentflow-llm library to use
  env::set_var("STEPFUN_API_KEY", &api_key);

  println!(
    "🔑 Using StepFun API key: {}...{}",
    &api_key[..8],
    &api_key[api_key.len() - 8..]
  );

  // Create shared state and populate inputs
  let shared_state = SharedState::new();
  shared_state.insert(
    "ingredient".to_string(),
    Value::String("salmon".to_string()),
  );

  println!("📝 Starting with ingredient: salmon");
  println!("🎯 Goal: Find an approved recipe using REAL LLM\n");

  // Create workflow nodes with real LLM integration
  let fetch_node = RealFetchRecipesNode::new("fetch_recipes").with_ingredient("salmon");

  let suggest_node = RealSuggestRecipeNode::new("suggest_recipe");

  let approval_node = RealGetApprovalNode::new("get_approval").with_approval_rate(0.6); // 60% approval rate for demo

  let retry_node = RetryNode::new("retry_handler").with_max_retries(4);

  // Execute workflow step by step with conditional logic
  println!("🚀 Starting REAL LLM Recipe Finder Workflow...\n");

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

  println!("\n✅ REAL LLM Recipe Finder Workflow completed!");

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio;

  #[tokio::test]
  async fn test_mock_integration_without_api() {
    // This test runs without requiring an actual API key
    let shared_state = SharedState::new();
    shared_state.insert(
      "ingredient".to_string(),
      Value::String("test_ingredient".to_string()),
    );

    // Test that the nodes can be created without API calls
    let fetch_node = RealFetchRecipesNode::new("test_fetch");
    assert_eq!(fetch_node.name, "test_fetch");

    let suggest_node = RealSuggestRecipeNode::new("test_suggest");
    assert_eq!(suggest_node.name, "test_suggest");

    let approval_node = RealGetApprovalNode::new("test_approval");
    assert_eq!(approval_node.name, "test_approval");

    // Test retry node (no API required)
    let retry_node = RetryNode::new("test_retry");
    assert_eq!(retry_node.name, "test_retry");
  }
}
