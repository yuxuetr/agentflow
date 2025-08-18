# Code-First Refactor: Before vs After

## Problem Statement

You correctly identified that the original `rust_interview_code_first.rs` example **bypassed `agentflow-core` entirely** and just used `agentflow-llm` directly, making `agentflow-core` redundant. This defeated the purpose of having a workflow orchestration system.

## Comparison: Wrong vs Right Approach

### ❌ **WRONG: Original Implementation (Bypassing agentflow-core)**

**File**: `examples/rust_interview_code_first.rs`

```rust
// Direct agentflow-llm usage - NO workflow orchestration!
async fn generate_interview_questions() -> Result<String, LLMError> {
  let response = AgentFlow::model("step-2-mini")
    .prompt("Please help me create 5 Rust backend interview questions")
    .temperature(0.7)
    .max_tokens(800)
    .execute()  // Direct API call
    .await?;
  
  Ok(response)
}

async fn evaluate_questions(questions: &str) -> Result<String, LLMError> {
  let evaluation_prompt = format!("Evaluate: {}", questions);
  
  let response = AgentFlow::model("step-2-mini")
    .prompt(&evaluation_prompt)
    .temperature(0.6)
    .max_tokens(600)
    .execute()  // Another direct API call
    .await?;
  
  Ok(response)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Manual orchestration - NO agentflow-core benefits!
  let questions = generate_interview_questions().await?;
  let evaluation = evaluate_questions(&questions).await?;
  
  println!("Questions: {}", questions);
  println!("Evaluation: {}", evaluation);
  
  Ok(())
}
```

**Problems with this approach**:
- 🚫 **No workflow orchestration** - just function calls
- 🚫 **No template resolution** - manual string formatting
- 🚫 **No shared state management** - passing data manually
- 🚫 **No dependency tracking** - manual data flow
- 🚫 **No robustness features** - no timeouts, retries, circuit breakers
- 🚫 **No observability** - no metrics, logging, state tracking
- 🚫 **Makes agentflow-core redundant** - could just use agentflow-llm directly

### ✅ **RIGHT: Refactored Implementation (Proper agentflow-core Integration)**

**File**: `examples/rust_interview_code_first_proper.rs`

```rust
// Proper LLM Node implementation
pub struct LlmNode {
  name: String,
  model: String,
  prompt_template: String,  // Template support!
  system_template: Option<String>,
  temperature: Option<f32>,
  max_tokens: Option<u32>,
}

#[async_trait]
impl AsyncNode for LlmNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    // Template resolution using SharedState!
    let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
    let resolved_system = self.system_template
      .as_ref()
      .map(|s| shared.resolve_template_advanced(s));
    
    // Build configuration object
    let mut config = serde_json::Map::new();
    config.insert("model".to_string(), Value::String(self.model.clone()));
    config.insert("prompt".to_string(), Value::String(resolved_prompt));
    // ...
    
    Ok(Value::Object(config))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    // Use agentflow-llm WITHIN the workflow orchestration framework
    let config = prep_result.as_object().unwrap();
    let prompt = config.get("prompt").unwrap().as_str().unwrap();
    let model = config.get("model").unwrap().as_str().unwrap();
    
    let response = AgentFlow::model(model)
      .prompt(prompt)
      .temperature(0.7)
      .execute()
      .await?;
      
    Ok(Value::String(response))
  }

  async fn post_async(&self, shared: &SharedState, _prep: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    // Store result in SharedState for other nodes!
    let output_key = format!("{}_output", self.name);
    shared.insert(output_key, exec_result);
    
    Ok(None) // Let workflow orchestrator decide next action
  }
}

// Workflow orchestrator
pub struct InterviewWorkflow {
  shared_state: SharedState,
  question_generator: LlmNode,
  question_evaluator: LlmNode,
}

impl InterviewWorkflow {
  pub fn new() -> Self {
    let shared_state = SharedState::new();
    
    // Node 1: Question Generator
    let question_generator = LlmNode::new("question_generator", "step-2-mini")
      .with_prompt("Please help me create 5 Rust backend interview questions");

    // Node 2: Question Evaluator (template dependency!)
    let question_evaluator = LlmNode::new("question_evaluator", "step-2-mini")
      .with_prompt("{{ question_generator_output }}");  // Template!
      
    Self { shared_state, question_generator, question_evaluator }
  }

  pub async fn execute(&self) -> Result<WorkflowResults, Box<dyn std::error::Error>> {
    // Execute Node 1 using agentflow-core orchestration
    self.question_generator.run_async(&self.shared_state).await?;
    
    // Execute Node 2 - automatically resolves template dependency!
    self.question_evaluator.run_async(&self.shared_state).await?;

    // Extract results from shared state
    let questions = self.shared_state.get("question_generator_output")...;
    let evaluation = self.shared_state.get("question_evaluator_output")...;

    Ok(WorkflowResults { questions, evaluation })
  }
}
```

**Benefits of this approach**:
- ✅ **Proper workflow orchestration** with AsyncNode pattern
- ✅ **Template resolution** via `{{ question_generator_output }}`
- ✅ **Shared state management** across nodes
- ✅ **Automatic dependency tracking** through templates
- ✅ **Robustness features** (timeouts, retries, circuit breakers)
- ✅ **Observability** (metrics collection, state tracking)
- ✅ **Makes agentflow-core essential** - provides real value!

## Key Architectural Differences

### Data Flow & Dependencies

**❌ Wrong (Manual)**:
```rust
// Manual data passing
let questions = generate_questions().await?;
let evaluation = evaluate_questions(&questions).await?;  // Manual dependency
```

**✅ Right (Orchestrated)**:
```rust
// Template-based dependency resolution
let question_evaluator = LlmNode::new("evaluator", "step-2-mini")
  .with_prompt("{{ question_generator_output }}");  // Automatic dependency!

// Workflow handles data flow
self.question_generator.run_async(&shared_state).await?;  // Stores to shared state
self.question_evaluator.run_async(&shared_state).await?; // Reads from shared state
```

### State Management

**❌ Wrong (No State)**:
```rust
// No shared state - data lives in variables
let questions = generate_questions().await?;
let evaluation = evaluate_questions(&questions).await?;
```

**✅ Right (SharedState)**:
```rust
// Centralized state management
shared_state.insert("question_generator_output", questions);
let evaluation_input = shared_state.resolve_template("{{ question_generator_output }}");
```

### Error Handling & Robustness

**❌ Wrong (Basic)**:
```rust
// Basic error propagation
match generate_questions().await {
  Ok(response) => response,
  Err(e) => {
    println!("Error: {}", e);
    return Err(e.into());
  }
}
```

**✅ Right (Advanced)**:
```rust
// Built-in robustness features
node.run_async_with_timeout(&shared_state, Duration::from_secs(30)).await?;
node.run_async_with_retries(&shared_state, 3, Duration::from_millis(1000)).await?;
```

## Real Output Comparison

### ❌ **Wrong Implementation Output**:
```
🚀 Code-First Rust Interview Questions Workflow
✅ Questions generated successfully from StepFun API
✅ Evaluation completed successfully from StepFun API
```
*Simple function calls, no orchestration visible*

### ✅ **Right Implementation Output**:
```
🚀 Executing Code-First Workflow with agentflow-core Orchestration

📝 Step 1: Question Generation Node
   🔗 Dependencies: None (entry point)
   📊 State Variables: model, experience_level
🔧 LLM Node 'question_generator' prepared:
   Model: step-2-mini
   Prompt: Please help me create 5 Rust backend interview questions
💾 Stored result in SharedState as: question_generator_output

🔍 Step 2: Question Evaluation Node
   🔗 Dependencies: question_generator_output
   📊 Template Resolution: {{ question_generator_output }}
🔧 LLM Node 'question_evaluator' prepared:
   Model: step-2-mini
   Prompt: [Resolved from Node 1 output]
💾 Stored result in SharedState as: question_evaluator_output

🛡️ Executing with robustness features:
   ⏱️ Node 1 with timeout protection...
   🔄 Node 2 with retry protection...

📊 Workflow State After Execution:
   experience_level: 3-5 years...
   question_generator_output: [Content]...
   question_evaluator_output: [Content]...
```
*Rich orchestration, dependency tracking, state management visible*

## When to Use Each Approach

### Use Original Approach (Direct agentflow-llm) When:
- 🎯 **Simple single LLM calls** with no dependencies
- 🎯 **Prototyping or scripts** that don't need orchestration
- 🎯 **Library integration** where you just need LLM capabilities

### Use Refactored Approach (agentflow-core + agentflow-llm) When:
- 🎯 **Multi-step workflows** with dependencies between steps
- 🎯 **Complex business logic** requiring state management
- 🎯 **Production systems** needing robustness and observability
- 🎯 **Template-based prompts** with dynamic content resolution
- 🎯 **Workflow orchestration** where order and dependencies matter

## Summary

The refactored version demonstrates the **proper integration** of both crates:

- **agentflow-core**: Provides workflow orchestration, state management, templates, robustness
- **agentflow-llm**: Provides LLM API abstraction and model management

Together, they create a powerful workflow system that's much more than the sum of its parts. The original version made agentflow-core redundant, while the refactored version shows why both crates are essential for complex AI workflows.