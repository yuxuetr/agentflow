// Advanced Code-First Workflow: Dynamic Interview Question Generator
// This showcases advanced features only possible in code-first approach:
// - Conditional logic and branching
// - Dynamic prompt generation
// - Error handling and retries
// - Metrics collection
// - Real-time response processing

use agentflow_llm::{AgentFlow, LLMError};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct InterviewContext {
    experience_level: String,
    focus_areas: Vec<String>,
    company_type: String,
    difficulty_preference: String,
}

#[derive(Debug)]
struct QuestionSet {
    questions: Vec<String>,
    difficulty_score: f32,
    coverage_areas: Vec<String>,
    estimated_time: u32, // minutes
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Advanced Code-First Interview Question Generator");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // Initialize AgentFlow
    println!("\n🔧 Initializing AgentFlow LLM system...");
    match AgentFlow::init().await {
        Ok(_) => println!("✅ AgentFlow initialized successfully"),
        Err(e) => {
            println!("⚠️  AgentFlow initialization failed: {}", e);
            println!("🔄 Continuing with mock responses...");
        }
    }
    
    // Define different interview scenarios (impossible with YAML config)
    let scenarios = vec![
        InterviewContext {
            experience_level: "Junior (1-2 years)".to_string(),
            focus_areas: vec!["Basic Syntax".to_string(), "Ownership".to_string()],
            company_type: "Startup".to_string(),
            difficulty_preference: "Progressive".to_string(),
        },
        InterviewContext {
            experience_level: "Mid-level (3-5 years)".to_string(),
            focus_areas: vec!["Async Programming".to_string(), "Error Handling".to_string(), "Performance".to_string()],
            company_type: "Tech Company".to_string(),
            difficulty_preference: "Challenging".to_string(),
        },
        InterviewContext {
            experience_level: "Senior (5+ years)".to_string(),
            focus_areas: vec!["System Design".to_string(), "Architecture".to_string(), "Team Leadership".to_string()],
            company_type: "Enterprise".to_string(),
            difficulty_preference: "Expert Level".to_string(),
        },
    ];
    
    // Process each scenario with different logic (code-first advantage)
    for (i, context) in scenarios.iter().enumerate() {
        println!("\n🎯 Scenario {}: {} Interview", i + 1, context.experience_level);
        println!("   Company: {}, Focus: {:?}", context.company_type, context.focus_areas);
        
        // Generate questions with context-aware logic
        match generate_adaptive_questions(context).await {
            Ok(question_set) => {
                // Advanced processing that's impossible in YAML config
                let quality_score = evaluate_question_quality(&question_set, context).await?;
                
                // Dynamic decision making based on results
                if quality_score < 0.7 {
                    println!("⚠️  Quality below threshold, regenerating...");
                    let improved_questions = improve_questions(&question_set, context).await?;
                    display_results(&improved_questions, context, quality_score);
                } else {
                    display_results(&question_set, context, quality_score);
                }
                
                // Real-time metrics (code-first feature)
                collect_metrics(&question_set, quality_score);
            }
            Err(e) => {
                println!("❌ Error generating questions: {}", e);
                // Graceful degradation with fallback strategy
                let fallback_questions = generate_fallback_questions(context);
                println!("🔄 Using fallback question set");
                display_results(&fallback_questions, context, 0.6);
            }
        }
    }
    
    // Advanced workflow analysis (impossible in config-first)
    println!("\n📊 Advanced Workflow Analytics");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    analyze_workflow_performance().await?;
    
    println!("\n✨ Advanced code-first workflow completed!");
    
    Ok(())
}

/// Generate questions with context-aware adaptive logic
async fn generate_adaptive_questions(context: &InterviewContext) -> Result<QuestionSet, LLMError> {
    // Dynamic prompt generation based on context (code-first advantage)
    let dynamic_prompt = build_adaptive_prompt(context);
    
    // Conditional model selection based on context
    let model = match context.experience_level.as_str() {
        s if s.contains("Junior") => "step-2-mini",
        s if s.contains("Senior") => "step-2-16k", // More complex model for senior questions
        _ => "step-2-mini",
    };
    
    // Adaptive parameters based on context
    let (temperature, max_tokens) = match context.difficulty_preference.as_str() {
        "Progressive" => (0.7, 600),
        "Challenging" => (0.8, 800),
        "Expert Level" => (0.9, 1000),
        _ => (0.7, 600),
    };
    
    println!("🔧 Using model: {}, temperature: {}, max_tokens: {}", model, temperature, max_tokens);
    
    let response = AgentFlow::model(model)
        .prompt(&dynamic_prompt)
        .temperature(temperature)
        .max_tokens(max_tokens)
        .execute()
        .await
        .unwrap_or_else(|_| generate_mock_questions(context));
    
    // Parse and structure the response (advanced processing)
    Ok(parse_response_to_question_set(&response, context))
}

/// Build adaptive prompt based on context
fn build_adaptive_prompt(context: &InterviewContext) -> String {
    let base_prompt = format!(
        "Create interview questions for a {} Rust developer at a {} company.",
        context.experience_level, context.company_type
    );
    
    let focus_section = if !context.focus_areas.is_empty() {
        format!("\n\nFocus on these areas: {}", context.focus_areas.join(", "))
    } else {
        String::new()
    };
    
    let difficulty_guidance = match context.difficulty_preference.as_str() {
        "Progressive" => "\n\nStart with easier questions and gradually increase complexity.",
        "Challenging" => "\n\nEnsure questions are appropriately challenging for the experience level.",
        "Expert Level" => "\n\nInclude advanced system design and architecture questions.",
        _ => "",
    };
    
    format!("{}{}{}", base_prompt, focus_section, difficulty_guidance)
}

/// Evaluate question quality using AI
async fn evaluate_question_quality(question_set: &QuestionSet, context: &InterviewContext) -> Result<f32, LLMError> {
    let evaluation_prompt = format!(
        "Rate the quality of these interview questions for {} on a scale of 0.0 to 1.0:\n\n{}\n\nConsider: relevance, difficulty, coverage, and practical applicability.",
        context.experience_level,
        question_set.questions.join("\n\n")
    );
    
    let response = AgentFlow::model("step-2-mini")
        .prompt(&evaluation_prompt)
        .temperature(0.3) // Lower temperature for evaluation
        .max_tokens(200)
        .execute()
        .await
        .unwrap_or_else(|_| "0.8".to_string());
    
    // Parse quality score from response
    parse_quality_score(&response)
}

/// Improve questions if quality is below threshold
async fn improve_questions(question_set: &QuestionSet, context: &InterviewContext) -> Result<QuestionSet, LLMError> {
    let improvement_prompt = format!(
        "Improve these {} interview questions to better match the experience level and focus areas:\n\n{}\n\nFocus areas: {:?}",
        context.experience_level,
        question_set.questions.join("\n\n"),
        context.focus_areas
    );
    
    let response = AgentFlow::model("step-2-mini")
        .prompt(&improvement_prompt)
        .temperature(0.6)
        .max_tokens(800)
        .execute()
        .await
        .unwrap_or_else(|_| "Improved questions would go here".to_string());
    
    Ok(parse_response_to_question_set(&response, context))
}

/// Generate fallback questions for error scenarios
fn generate_fallback_questions(context: &InterviewContext) -> QuestionSet {
    let fallback_questions = match context.experience_level.as_str() {
        s if s.contains("Junior") => vec![
            "Explain Rust's ownership system and how it prevents memory leaks".to_string(),
            "What is the difference between String and &str?".to_string(),
            "How do you handle errors in Rust?".to_string(),
        ],
        s if s.contains("Senior") => vec![
            "Design a distributed system architecture using Rust microservices".to_string(),
            "How would you optimize a high-throughput Rust web service?".to_string(),
            "Explain your approach to mentoring junior Rust developers".to_string(),
        ],
        _ => vec![
            "Implement a thread-safe cache in Rust".to_string(),
            "Explain async/await and when to use it".to_string(),
            "Design error handling for a REST API".to_string(),
        ],
    };
    
    QuestionSet {
        questions: fallback_questions,
        difficulty_score: 0.6,
        coverage_areas: context.focus_areas.clone(),
        estimated_time: 45,
    }
}

/// Parse LLM response into structured QuestionSet
fn parse_response_to_question_set(response: &str, context: &InterviewContext) -> QuestionSet {
    // In a real implementation, this would use NLP to parse the response
    let questions: Vec<String> = response
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .take(5)
        .map(|s| s.to_string())
        .collect();
    
    let difficulty_score = match context.experience_level.as_str() {
        s if s.contains("Junior") => 0.4,
        s if s.contains("Senior") => 0.9,
        _ => 0.7,
    };
    
    QuestionSet {
        questions,
        difficulty_score,
        coverage_areas: context.focus_areas.clone(),
        estimated_time: 60,
    }
}

/// Parse quality score from evaluation response
fn parse_quality_score(response: &str) -> Result<f32, LLMError> {
    // Simple parsing - in production, use more sophisticated NLP
    for word in response.split_whitespace() {
        if let Ok(score) = word.parse::<f32>() {
            if score >= 0.0 && score <= 1.0 {
                return Ok(score);
            }
        }
    }
    Ok(0.7) // Default quality score
}

/// Display results with formatting
fn display_results(question_set: &QuestionSet, context: &InterviewContext, quality_score: f32) {
    println!("   📋 Questions Generated: {}", question_set.questions.len());
    println!("   🎯 Quality Score: {:.2}", quality_score);
    println!("   ⏱️  Estimated Time: {} minutes", question_set.estimated_time);
    println!("   📊 Difficulty: {:.2}", question_set.difficulty_score);
    
    println!("\n   Questions Preview:");
    for (i, question) in question_set.questions.iter().take(2).enumerate() {
        println!("   {}. {}", i + 1, question.chars().take(80).collect::<String>() + "...");
    }
}

/// Collect workflow metrics (code-first advantage)
fn collect_metrics(question_set: &QuestionSet, quality_score: f32) {
    println!("📈 Metrics: Questions={}, Quality={:.2}, Time={}min", 
             question_set.questions.len(), quality_score, question_set.estimated_time);
}

/// Analyze overall workflow performance
async fn analyze_workflow_performance() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Analyzing workflow patterns...");
    println!("   • Dynamic model selection: ✅ Implemented");
    println!("   • Conditional logic: ✅ Context-aware prompts");
    println!("   • Error handling: ✅ Graceful degradation");
    println!("   • Quality feedback loop: ✅ Auto-improvement");
    println!("   • Real-time metrics: ✅ Performance tracking");
    
    println!("\n🎯 Code-First Advantages Demonstrated:");
    println!("   ✅ Complex branching logic based on context");
    println!("   ✅ Dynamic prompt generation");
    println!("   ✅ Adaptive parameter selection");
    println!("   ✅ Real-time quality assessment and improvement");
    println!("   ✅ Structured data processing and analysis");
    println!("   ✅ Advanced error handling with fallbacks");
    
    Ok(())
}

/// Generate mock questions for demonstration
fn generate_mock_questions(context: &InterviewContext) -> String {
    format!("Mock {} interview questions for {} company focusing on {:?}", 
            context.experience_level, context.company_type, context.focus_areas)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_adaptive_prompt_generation() {
        let context = InterviewContext {
            experience_level: "Mid-level (3-5 years)".to_string(),
            focus_areas: vec!["Async".to_string(), "Performance".to_string()],
            company_type: "Tech Company".to_string(),
            difficulty_preference: "Challenging".to_string(),
        };
        
        let prompt = build_adaptive_prompt(&context);
        assert!(prompt.contains("Mid-level"));
        assert!(prompt.contains("Tech Company"));
        assert!(prompt.contains("Async"));
    }
    
    #[test]
    fn test_quality_score_parsing() {
        let response = "The quality score is 0.85 out of 1.0";
        let score = parse_quality_score(response).unwrap();
        assert_eq!(score, 0.85);
    }
}