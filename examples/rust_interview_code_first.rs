// Code-First Rust Interview Questions Workflow
// This demonstrates pure Rust API usage without YAML configuration
// Contrast with configuration-first approach in examples/workflows/rust_interview_questions.yml

use agentflow_llm::{AgentFlow, LLMError};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Code-First Rust Interview Questions Workflow");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // Initialize AgentFlow LLM system
    // This loads built-in configuration for supported models
    println!("\n🔧 Initializing AgentFlow LLM system...");
    match AgentFlow::init().await {
        Ok(_) => println!("✅ AgentFlow initialized successfully"),
        Err(e) => {
            println!("⚠️  AgentFlow initialization failed: {}", e);
            println!("🔄 Continuing with mock responses for demonstration...");
        }
    }
    
    // STEP 1: Generate interview questions using direct API calls
    println!("\n📝 Step 1: Generating Rust Backend Interview Questions");
    println!("   Using: step-2-mini model");
    println!("   Role: Senior Rust engineer with extensive backend development experience");
    
    let question_generator_response = match generate_interview_questions().await {
        Ok(response) => {
            println!("✅ Questions generated successfully from StepFun API");
            println!("🤖 Using REAL AI-generated content (not mock data)");
            response
        }
        Err(e) => {
            println!("⚠️  API call failed: {}", e);
            println!("🎭 Falling back to mock response for demonstration");
            println!("💡 To use real API: Set STEPFUN_API_KEY environment variable");
            
            let mock_questions = r#"[MOCK DATA] Here are 5 Rust backend interview questions for 3-5 years experience:

1. **Ownership and Memory Management**: Explain the difference between `Box<T>`, `Rc<T>`, and `Arc<T>`. When would you use each in a backend service?

2. **Async Programming**: How would you handle database connection pooling in an async Rust web service? Discuss connection lifecycle and error handling.

3. **Error Handling**: Design a comprehensive error handling strategy for a REST API that needs to return appropriate HTTP status codes and detailed error messages.

4. **Performance Optimization**: You have a web service that's experiencing high latency. What Rust-specific profiling and optimization techniques would you use?

5. **Concurrency Patterns**: Implement a thread-safe cache with TTL (time-to-live) functionality that can be shared across multiple request handlers."#;
            
            mock_questions.to_string()
        }
    };
    
    // Display generated questions
    println!("\n📋 Generated Questions:");
    println!("{}", question_generator_response);
    
    // STEP 2: Evaluate the questions using the first step's output
    println!("\n\n🔍 Step 2: Evaluating Question Quality");
    println!("   Using: step-2-mini model");
    println!("   Role: Senior Rust backend interviewer");
    println!("   Input: Output from Step 1");
    
    let evaluation_response = match evaluate_questions(&question_generator_response).await {
        Ok(response) => {
            println!("✅ Evaluation completed successfully from StepFun API");
            println!("🤖 Using REAL AI-generated evaluation (not mock data)");
            response
        }
        Err(e) => {
            println!("⚠️  API call failed: {}", e);
            println!("🎭 Falling back to mock evaluation for demonstration");
            println!("💡 To use real API: Set STEPFUN_API_KEY environment variable");
            
            let mock_evaluation = r#"[MOCK DATA] ## Interview Questions Quality Evaluation

**Overall Assessment**: These questions are well-suited for 3-5 years Rust backend experience level.

**Strengths**:
- **Good technical depth**: Questions cover core Rust concepts (ownership, async) and practical backend scenarios
- **Appropriate complexity**: Not too basic (avoiding simple syntax questions) nor too advanced (not requiring deep compiler internals)
- **Real-world relevance**: Each question relates to actual backend development challenges
- **Skill coverage**: Covers memory management, concurrency, error handling, performance, and system design

**Specific Question Analysis**:
1. **Smart pointers question**: Excellent for assessing memory management understanding
2. **Async/database question**: Perfect for modern Rust backend development
3. **Error handling question**: Essential for production-ready code
4. **Performance question**: Tests practical optimization skills
5. **Concurrency question**: Combines multiple concepts in a realistic scenario

**Recommendations**:
- Consider adding a question about trait design or generic programming
- Could include a system design question about microservices architecture
- Questions are appropriate for mid-level developers (3-5 years experience)

**Grade**: A- (Excellent questions for the target experience level)"#;
            
            mock_evaluation.to_string()
        }
    };
    
    // Display evaluation
    println!("\n📊 Quality Evaluation:");
    println!("{}", evaluation_response);
    
    // SUMMARY: Compare approaches
    println!("\n\n🎯 Code-First vs Configuration-First Comparison");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    println!("\n🔧 Code-First Approach (this example):");
    println!("   ✅ Full programmatic control");
    println!("   ✅ Type safety and IDE support");
    println!("   ✅ Complex logic and branching possible");
    println!("   ✅ Direct access to all API features");
    println!("   ❌ Requires Rust programming knowledge");
    println!("   ❌ More verbose for simple workflows");
    
    println!("\n📄 Configuration-First Approach (YAML):");
    println!("   ✅ Declarative and easy to read");
    println!("   ✅ No programming knowledge required");
    println!("   ✅ Version control friendly");
    println!("   ✅ Quick prototyping");
    println!("   ❌ Limited to predefined node types");
    println!("   ❌ Less flexibility for complex logic");
    
    println!("\n✨ Workflow completed successfully!");
    println!("📁 See examples/workflows/rust_interview_questions.yml for config-first version");
    
    Ok(())
}

/// Generate Rust backend interview questions using AgentFlow LLM API
async fn generate_interview_questions() -> Result<String, LLMError> {
    let response = AgentFlow::model("step-2-mini")
        .prompt("Please help me create 5 Rust backend interview questions")
        .temperature(0.7)
        .max_tokens(800)
        .execute()
        .await?;
    
    Ok(response)
}

/// Evaluate the quality of interview questions for 3-5 years experience level
async fn evaluate_questions(questions: &str) -> Result<String, LLMError> {
    let evaluation_prompt = format!(
        "You are a senior Rust backend interviewer. Please evaluate whether the following interview questions meet the standards for 3-5 years of Rust backend development experience:\n\n{}", 
        questions
    );
    
    let response = AgentFlow::model("step-2-mini")
        .prompt(&evaluation_prompt)
        .temperature(0.6)
        .max_tokens(600)
        .execute()
        .await?;
    
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_workflow_functions() {
        // Test that our functions compile and can handle errors gracefully
        match generate_interview_questions().await {
            Ok(_) => println!("Generate function works"),
            Err(_) => println!("Generate function handles errors"),
        }
        
        match evaluate_questions("test questions").await {
            Ok(_) => println!("Evaluate function works"),
            Err(_) => println!("Evaluate function handles errors"),
        }
    }
    
    #[test]
    fn test_agentflow_builder_pattern() {
        // Test that AgentFlow builder pattern works
        let builder = AgentFlow::model("test-model")
            .prompt("test prompt")
            .temperature(0.7)
            .max_tokens(100);
        
        // Just verify the builder compiles
        drop(builder);
    }
}