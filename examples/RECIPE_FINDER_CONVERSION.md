# PocketFlow to AgentFlow Conversion: Recipe Finder

This document demonstrates the conversion of the PocketFlow `cookbook/pocketflow-async-basic` example to both AgentFlow code-first and configuration-first implementations.

## Original PocketFlow Example

The original PocketFlow async-basic example implements a Recipe Finder with:

- **FetchRecipes**: Gets ingredient from user, fetches recipes via async API
- **SuggestRecipe**: Uses LLM to suggest best recipe from the list
- **GetApproval**: Gets user approval, loops back on rejection
- **Async Flow Control**: Uses PocketFlow's async node system with action-based transitions

### PocketFlow Structure
```python
# nodes.py
class FetchRecipes(AsyncNode):
    async def prep_async(self, shared): # Get user input
    async def exec_async(self, ingredient): # Fetch recipes
    async def post_async(self, shared, prep_res, recipes): # Store & continue

class SuggestRecipe(AsyncNode):
    async def prep_async(self, shared): # Get recipes from shared
    async def exec_async(self, recipes): # LLM suggestion
    async def post_async(self, shared, prep_res, suggestion): # Store & continue

class GetApproval(AsyncNode):
    async def prep_async(self, shared): # Get current suggestion
    async def exec_async(self, suggestion): # Get user approval
    async def post_async(self, shared, prep_res, answer): # Handle decision

# flow.py
fetch - "suggest" >> suggest
suggest - "approve" >> approve  
approve - "retry" >> suggest    # Loop back
approve - "accept" >> end       # End flow
```

## AgentFlow Code-First Implementation

**File**: `agentflow-core/examples/recipe_finder_workflow.rs`

### Key Features
- **AsyncNode Trait**: Direct port of PocketFlow's async node pattern
- **SharedState**: Replaces PocketFlow's shared dictionary
- **Mock LLM/API**: Simulates async operations with tokio delays
- **Retry Logic**: Implements conditional looping with max retry limits
- **Approval Simulation**: Uses probabilistic approval for demo purposes

### Node Structure
```rust
// Four main nodes mirroring PocketFlow structure
struct FetchRecipesNode    // Fetches recipes from mock API
struct SuggestRecipeNode   // LLM suggests best recipe
struct GetApprovalNode     // Simulates user approval
struct RetryNode          // Handles retry logic

// Each implements AsyncNode trait
#[async_trait]
impl AsyncNode for FetchRecipesNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError>
    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError>
    async fn post_async(&self, shared: &SharedState, prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError>
}
```

### Execution Results
```
🍳 AgentFlow Core - Recipe Finder Workflow Demo
📝 Starting with ingredient: chicken
🚀 Starting Recipe Finder Workflow...

🔍 Fetching recipes for chicken...
✅ Found 5 recipes for chicken
🧠 Getting LLM suggestion for best recipe...
💡 LLM suggests: chicken Stir Fry
👤 Asking user about: chicken Stir Fry
❌ User rejected: chicken Stir Fry
🔄 Retry attempt 1 of 4
... (continues with retry logic)
⚠️ Maximum retries (4) reached. No suitable recipe found.

📊 Final Workflow State:
📈 Status: max_retries_reached
🔄 Rejected recipes: 4
```

## AgentFlow Configuration-First Implementation

**File**: `examples/workflows/recipe_finder_simple.yml`

### Key Features
- **YAML Configuration**: Declarative workflow definition
- **Template Substitution**: Uses `{{ variable }}` syntax for dynamic content
- **Sequential Flow**: Simplified linear flow (loops would require enhanced runtime)
- **LLM Integration**: Direct LLM calls via configuration
- **Structured Outputs**: Named outputs for result collection

### YAML Structure
```yaml
name: "Simple Recipe Finder"
description: "Find and suggest a recipe based on user ingredient"

inputs:
  ingredient:
    type: string
    required: true
    default: "chicken"

workflow:
  - name: fetch_and_suggest
    type: llm
    model: "step-2-mini"
    prompt: |
      You are a helpful chef assistant. I need you to:
      1. Generate 5 different {{ ingredient }} recipes
      2. Choose the most appealing one
      3. Return your recommendation
    outputs:
      recipe_recommendation: response

  - name: user_feedback
    type: llm
    prompt: |
      Simulate user response to: {{ recipe_recommendation }}
    outputs:
      user_response: response

  - name: chef_response
    type: llm
    prompt: |
      User said: "{{ user_response }}"
      Your recommendation: {{ recipe_recommendation }}
      Provide helpful follow-up response.
    outputs:
      final_response: response

outputs:
  recommended_recipe:
    from: fetch_and_suggest.response
  user_feedback:
    from: user_feedback.response
  chef_final_advice:
    from: chef_response.response
```

### Execution Results
```
🚀 Starting workflow execution: recipe_finder_simple.yml
📝 Input parameters: ingredient: chicken
🚀 Running configuration-first workflow: Simple Recipe Finder

▶️ Executing node: fetch_and_suggest
🔧 LLM Node prepared with ingredient substitution
✅ LLM Response: [Mock response]
▶️ Executing node: user_feedback  
✅ LLM Response: [Mock response]
▶️ Executing node: chef_response
✅ LLM Response: [Mock response]

🎯 Workflow completed successfully!
📊 Results:
  - recommended_recipe: [Response]
  - user_feedback: [Response] 
  - chef_final_advice: [Response]
```

## Comparison Summary

| Aspect | PocketFlow Original | AgentFlow Code-First | AgentFlow Config-First |
|--------|-------------------|---------------------|---------------------|
| **Definition** | Python classes | Rust structs | YAML configuration |
| **Async Support** | Native async/await | Tokio async/await | Handled by runtime |
| **Flow Control** | Action-based transitions | Manual loop control | Sequential execution |
| **State Management** | Shared dictionary | SharedState (Arc<DashMap>) | Automatic via runtime |
| **LLM Integration** | OpenAI async client | Mock implementation | Built-in LLM nodes |
| **Template System** | String formatting | Manual substitution | `{{ variable }}` syntax |
| **Error Handling** | Python exceptions | Result<T, E> types | Runtime error handling |
| **Type Safety** | Runtime (Python) | Compile-time (Rust) | Schema validation |
| **Complexity** | Medium | High (manual control) | Low (declarative) |
| **Flexibility** | High | Very High | Medium |
| **Learning Curve** | Medium | Steep | Gentle |

## Key Insights

### Code-First Advantages
- **Full Control**: Complete control over execution logic and flow
- **Type Safety**: Compile-time guarantees and error checking  
- **Performance**: Zero-overhead abstractions and efficient execution
- **Complex Logic**: Can implement sophisticated retry patterns and conditional flows
- **Testing**: Rich unit testing capabilities with mock data

### Configuration-First Advantages  
- **Simplicity**: Easy to understand and modify by non-programmers
- **Rapid Prototyping**: Quick workflow development and iteration
- **Declarative**: Clear intent without implementation details
- **Version Control**: Easy to track changes in workflow logic
- **Visual Tools**: Potential for GUI workflow builders

### Current Limitations
- **Config-First Flow Control**: Limited support for complex loops and conditionals
- **Template Resolution**: Inter-node variable substitution needs enhancement  
- **Mock LLM**: Both implementations use mock responses currently
- **Error Recovery**: Configuration workflows need better error handling patterns

## Running the Examples

### Code-First
```bash
cargo run -p agentflow-core --example recipe_finder_workflow
```

### Configuration-First
```bash
/Users/hal/.target/debug/agentflow workflow run examples/workflows/recipe_finder_simple.yml --input ingredient="salmon"
```

## Future Enhancements

1. **Real LLM Integration**: Replace mock responses with actual API calls
2. **Enhanced Flow Control**: Add conditional nodes and loop constructs to config system
3. **Template Engine**: Improve variable substitution between workflow steps
4. **Visual Editor**: Build GUI tools for configuration-first workflow creation
5. **Hybrid Approach**: Allow embedding code-first nodes in configuration workflows
6. **Error Recovery**: Add try/catch patterns and fallback strategies
7. **Monitoring**: Add execution tracing and performance metrics

This conversion demonstrates AgentFlow's dual approach: powerful code-first development for complex scenarios and accessible configuration-first development for simpler use cases.