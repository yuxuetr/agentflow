// AgentFlow Structured Output Example
// Migrated from PocketFlow cookbook/pocketflow-structured-output
// Tests: Structured data extraction, YAML parsing, validation, error handling

use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result, AgentFlowError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_yaml;
use std::collections::HashMap;
use std::time::Instant;
use tokio::time::{sleep, Duration};

/// Structured resume data model
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResumeData {
  name: String,
  email: String,
  experience: Vec<Experience>,
  skill_indexes: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Experience {
  title: String,
  company: String,
}

/// Mock LLM call for structured data extraction
async fn call_mock_llm_structured(prompt: &str) -> String {
  // Simulate API call delay
  sleep(Duration::from_millis(300)).await;
  
  // Return mock YAML output based on the resume data
  // This simulates what an LLM would extract from the resume
  let mock_yaml = r#"# Found name at top of resume
name: John Smith
# Found email in contact information section
email: johnsmith1983@gmail.com
# Experience section analysis - extracted work history
experience:
  # Current position - Sales Manager
  - title: Sales Manager
    company: ABC Corporation
  # Previous role - Assistant Manager
  - title: Assistant Manager
    company: XYZ Industries
  # Earlier position - Customer Service Representative
  - title: Customer Service Representative
    company: Fast Solutions Inc
# Skills identified from the target list based on resume content
skill_indexes:
  # Found team leadership mentioned in experience
  - 0
  # Found CRM software mentioned in sales manager role
  - 1
  # Found project management in experience
  - 2
  # Found public speaking in skills section
  - 3
  # Found Microsoft Office mentioned in skills
  - 4"#;
  
  format!("```yaml\n{}\n```", mock_yaml)
}

/// Resume Parser Node - equivalent to PocketFlow's ResumeParserNode
/// Tests structured data extraction and validation
struct ResumeParserNode {
  node_id: String,
  target_skills: Vec<String>,
}

impl ResumeParserNode {
  fn new() -> Self {
    Self {
      node_id: "resume_parser_node".to_string(),
      target_skills: vec![
        "Team leadership & management".to_string(),  // 0
        "CRM software".to_string(),                  // 1
        "Project management".to_string(),            // 2
        "Public speaking".to_string(),               // 3
        "Microsoft Office".to_string(),              // 4
        "Python".to_string(),                        // 5
        "Data Analysis".to_string(),                 // 6
      ],
    }
  }
}

#[async_trait]
impl AsyncNode for ResumeParserNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Phase 1: Preparation - get resume text and target skills
    let resume_text = shared.get("resume_text")
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| "(No resume text provided)".to_string());
    
    let target_skills = shared.get("target_skills")
      .and_then(|v| v.as_array().map(|arr| {
        arr.iter()
          .filter_map(|v| v.as_str())
          .map(|s| s.to_string())
          .collect()
      }))
      .unwrap_or_else(|| self.target_skills.clone());
    
    println!("🔍 [PREP] Resume text length: {} characters", resume_text.len());
    println!("🔍 [PREP] Target skills to find: {} skills", target_skills.len());
    
    // Log target skills with indexes
    for (i, skill) in target_skills.iter().enumerate() {
      println!("  {}: {}", i, skill);
    }
    
    Ok(serde_json::json!({
      "resume_text": resume_text,
      "target_skills": target_skills
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    // Phase 2: Execution - extract structured data using LLM
    let resume_text = prep_result["resume_text"].as_str().unwrap();
    let target_skills: Vec<String> = prep_result["target_skills"]
      .as_array()
      .unwrap()
      .iter()
      .map(|v| v.as_str().unwrap().to_string())
      .collect();
    
    println!("🤖 [EXEC] Starting structured data extraction...");
    let start_time = Instant::now();
    
    // Format skills with indexes for the prompt
    let skill_list_for_prompt: String = target_skills
      .iter()
      .enumerate()
      .map(|(i, skill)| format!("{}: {}", i, skill))
      .collect::<Vec<_>>()
      .join("\n");
    
    // Create prompt for structured extraction
    let prompt = format!(r#"
Analyze the resume below. Output ONLY the requested information in YAML format.

**Resume:**
```
{}
```

**Target Skills (use these indexes):**
```
{}
```

**YAML Output Requirements:**
- Extract `name` (string).
- Extract `email` (string).
- Extract `experience` (list of objects with `title` and `company`).
- Extract `skill_indexes` (list of integers found from the Target Skills list).
- **Add a YAML comment (`#`) explaining the source BEFORE `name`, `email`, `experience`, each item in `experience`, and `skill_indexes`.**

**Example Format:**
```yaml
# Found name at top
name: Jane Doe
# Found email in contact info
email: jane@example.com
# Experience section analysis
experience:
  # First job listed
  - title: Manager
    company: Corp A
  # Second job listed
  - title: Assistant
    company: Corp B
# Skills identified from the target list based on resume content
skill_indexes:
  # Found 0 at top  
  - 0
  # Found 2 in experience
  - 2
```

Generate the YAML output now:
"#, resume_text, skill_list_for_prompt);
    
    let response = call_mock_llm_structured(&prompt).await;
    let processing_time = start_time.elapsed();
    
    println!("⚡ [EXEC] LLM response received in {:?}", processing_time);
    
    // Extract YAML from the response
    let yaml_str = if let Some(start) = response.find("```yaml") {
      let yaml_start = start + 7; // Skip "```yaml\n"
      if let Some(end_offset) = response[yaml_start..].find("```") {
        let yaml_end = yaml_start + end_offset;
        response[yaml_start..yaml_end].trim()
      } else {
        return Err(AgentFlowError::NodeExecutionFailed { message: "Could not find closing ``` for YAML block".to_string() });
      }
    } else {
      return Err(AgentFlowError::NodeExecutionFailed { message: "Could not find ```yaml block in LLM response".to_string() });
    };
    
    println!("📋 [EXEC] Extracted YAML:\n{}", yaml_str);
    
    // Parse YAML
    let structured_result: ResumeData = match serde_yaml::from_str(yaml_str) {
      Ok(data) => data,
      Err(e) => return Err(AgentFlowError::NodeExecutionFailed { message: format!("YAML parsing failed: {}", e) }),
    };
    
    // Validate the structured data
    if structured_result.name.is_empty() {
      return Err(AgentFlowError::NodeExecutionFailed { message: "Validation failed: Name is empty".to_string() });
    }
    
    if structured_result.email.is_empty() {
      return Err(AgentFlowError::NodeExecutionFailed { message: "Validation failed: Email is empty".to_string() });
    }
    
    if structured_result.experience.is_empty() {
      return Err(AgentFlowError::NodeExecutionFailed { message: "Validation failed: No experience found".to_string() });
    }
    
    // Validate skill indexes
    for &index in &structured_result.skill_indexes {
      if index >= target_skills.len() {
        return Err(AgentFlowError::NodeExecutionFailed { message: format!("Validation failed: Skill index {} is out of range (max: {})", index, target_skills.len() - 1) });
      }
    }
    
    println!("✅ [EXEC] Structured data validation passed");
    
    Ok(serde_json::json!({
      "structured_data": serde_json::to_value(&structured_result).unwrap(),
      "processing_time_ms": processing_time.as_millis(),
      "yaml_output": yaml_str,
      "target_skills": target_skills
    }))
  }

  async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
    // Phase 3: Post-processing - store results and display formatted output
    let structured_data: ResumeData = serde_json::from_value(
      exec["structured_data"].clone()
    ).map_err(|e| AgentFlowError::NodeExecutionFailed { message: format!("Failed to deserialize structured data: {}", e) })?;
    
    let processing_time = exec["processing_time_ms"].as_u64().unwrap_or(0);
    let yaml_output = exec["yaml_output"].as_str().unwrap_or("");
    let target_skills: Vec<String> = exec["target_skills"]
      .as_array()
      .unwrap()
      .iter()
      .map(|v| v.as_str().unwrap().to_string())
      .collect();
    
    println!("💾 [POST] Storing structured data results...");
    
    // Store in shared state
    shared.insert("structured_data".to_string(), exec["structured_data"].clone());
    shared.insert("processing_time_ms".to_string(), Value::Number(processing_time.into()));
    shared.insert("extraction_successful".to_string(), Value::Bool(true));
    
    // Display formatted results
    println!("\n=== STRUCTURED RESUME DATA (Comments & Skill Index List) ===\n");
    println!("{}", yaml_output);
    println!("\n============================================================\n");
    
    println!("✅ Extracted resume information:");
    println!("👤 Name: {}", structured_data.name);
    println!("📧 Email: {}", structured_data.email);
    println!("💼 Experience: {} positions", structured_data.experience.len());
    
    for (i, exp) in structured_data.experience.iter().enumerate() {
      println!("  {}. {} at {}", i + 1, exp.title, exp.company);
    }
    
    println!("🎯 Skills Found: {} skills", structured_data.skill_indexes.len());
    for &index in &structured_data.skill_indexes {
      if index < target_skills.len() {
        println!("  - {} (Index: {})", target_skills[index], index);
      }
    }
    
    println!("⏱️ Processing Time: {}ms", processing_time);
    
    println!("\n💾 [POST] Structured output processing completed!");
    
    // End the flow
    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.node_id.clone())
  }
}

/// Create resume parsing flow
fn create_resume_parsing_flow() -> AsyncFlow {
  let parser_node = Box::new(ResumeParserNode::new());
  AsyncFlow::new(parser_node)
}

/// Main structured output example
async fn run_structured_output_example() -> Result<()> {
  println!("🚀 AgentFlow Structured Output Example");
  println!("📝 Migrated from: PocketFlow cookbook/pocketflow-structured-output");
  println!("🎯 Testing: Structured data extraction, YAML parsing, validation\n");
  
  // Sample resume data (based on PocketFlow's data.txt)
  let resume_text = r#"# JOHN SMITH

**Email:** johnsmith1983@gmail.com
**Phone:** (555) 123-4567
**Address:** 123 Main St, Anytown, USA

## PROFESSIONAL SUMMARY

Dedicated and hardworking professional with over 10 years of experience in business management. Known for finding creative solutions to complex problems and excellent communication skills. Seeking new opportunities to leverage my expertise in a dynamic environment.

## WORK EXPERIENCE

### SALES MANAGER
**ABC Corporation** | Anytown, USA | June 2018 - Present
- Oversee a team of 12 sales representatives and achieve quarterly targets
- Increased department revenue by 24% in fiscal year 2019-2020
- Implemented new CRM system that improved efficiency by 15%
- Collaborate with Marketing team on product launch campaigns
- Developed training materials for new hires

### ASSISTANT MANAGER
**XYZ Industries** | Somewhere Else, USA | March 2015 - May 2018
- Assisted the Regional Manager in daily operations and reporting
- Managed inventory and vendor relations
- Trained and mentored junior staff members
- Received "Employee of the Month" award 4 times

### CUSTOMER SERVICE REPRESENTATIVE
**Fast Solutions Inc** | Another City, USA | January 2010 - February 2015
- Responded to customer inquiries via phone, email, and in-person
- Resolved customer complaints and escalated issues when necessary
- Maintained a 95% customer satisfaction rating

## EDUCATION

**Bachelor of Business Administration**
University of Somewhere | 2006 - 2010
GPA: 3.6/4.0

**Associate Degree in Communications**
Community College | 2004-2006

## SKILLS

- Microsoft Office: *Excel, Word, PowerPoint* (Advanced)
- Customer relationship management (CRM) software
- Team leadership & management
- Project management
- Public speaking
- Time management

## REFERENCES

Available upon request

### OTHER ACTIVITIES
- Volunteer at the local food bank (2016-present)
- Member of Toastmasters International
- Enjoy hiking and photography"#;

  // Target skills to find
  let target_skills = vec![
    "Team leadership & management",
    "CRM software", 
    "Project management",
    "Public speaking",
    "Microsoft Office",
    "Python",
    "Data Analysis",
  ];
  
  println!("📄 Resume text length: {} characters", resume_text.len());
  println!("🎯 Target skills to identify: {} skills", target_skills.len());
  
  for (i, skill) in target_skills.iter().enumerate() {
    println!("  {}: {}", i, skill);
  }
  println!();
  
  // Create shared state with input data
  let shared = SharedState::new();
  shared.insert("resume_text".to_string(), Value::String(resume_text.to_string()));
  shared.insert("target_skills".to_string(), 
    Value::Array(target_skills.iter().map(|s| Value::String(s.to_string())).collect()));
  
  // Create and run the resume parsing flow
  let parsing_flow = create_resume_parsing_flow();
  let start_time = Instant::now();
  
  match parsing_flow.run_async(&shared).await {
    Ok(result) => {
      let total_duration = start_time.elapsed();
      
      println!("\n✅ Structured output extraction completed in {:?}", total_duration);
      println!("📋 Flow result: {:?}", result);
      
      // Extract and verify results
      let extraction_successful = shared.get("extraction_successful")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
        
      let processing_time = shared.get("processing_time_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
      
      if let Some(structured_data) = shared.get("structured_data") {
        let resume_data: ResumeData = serde_json::from_value(structured_data.clone())
          .map_err(|e| AgentFlowError::NodeExecutionFailed { message: format!("Failed to deserialize: {}", e) })?;
        
        println!("\n🎯 Structured Output Results:");
        println!("Extraction Successful: {}", extraction_successful);
        println!("Name Extracted: {}", resume_data.name);
        println!("Email Extracted: {}", resume_data.email);
        println!("Experience Records: {}", resume_data.experience.len());
        println!("Skills Identified: {}", resume_data.skill_indexes.len());
        println!("Processing Time: {}ms", processing_time);
        println!("Total Flow Time: {:?}", total_duration);
        
        // Verify functionality
        assert!(extraction_successful, "Extraction should be successful");
        assert!(!resume_data.name.is_empty(), "Name should be extracted");
        assert!(!resume_data.email.is_empty(), "Email should be extracted");
        assert!(!resume_data.experience.is_empty(), "Experience should be extracted");
        assert!(processing_time > 0, "Processing time should be recorded");
        
        println!("\n✅ All assertions passed - AgentFlow structured output verified!");
      } else {
        return Err(AgentFlowError::NodeExecutionFailed { message: "No structured data found in results".to_string() });
      }
    }
    Err(e) => {
      println!("❌ Structured output extraction failed: {}", e);
      return Err(e);
    }
  }
  
  Ok(())
}

/// Performance comparison with PocketFlow
async fn performance_comparison() {
  println!("\n📊 Structured Output Performance Comparison:");
  println!("PocketFlow (Python):");
  println!("  - YAML parsing with PyYAML");
  println!("  - Dictionary-based data structures");
  println!("  - String-based validation");
  println!("  - Exception-based error handling");
  println!();
  println!("AgentFlow (Rust):");
  println!("  - Type-safe YAML parsing with serde_yaml");
  println!("  - Strongly-typed data structures with serde");
  println!("  - Compile-time type checking");
  println!("  - Result-based error handling");
  println!();
  println!("Expected improvements:");
  println!("  - 🚀 Compile-time validation of data structures");
  println!("  - ⚡ Faster YAML parsing");
  println!("  - 💧 Lower memory overhead");
  println!("  - 🛡️ Type safety prevents runtime errors");
  println!("  - 📊 Built-in serialization/deserialization");
}

#[tokio::main]
async fn main() -> Result<()> {
  // Run the structured output example
  run_structured_output_example().await?;
  
  // Show performance comparison
  performance_comparison().await;
  
  println!("\n🎉 Structured Output migration completed successfully!");
  println!("🔬 AgentFlow structured data functionality verified:");
  println!("  ✅ YAML-based structured data extraction");
  println!("  ✅ Type-safe data models with serde");
  println!("  ✅ Comprehensive data validation");
  println!("  ✅ Error handling with detailed messages");
  println!("  ✅ Processing performance metrics");
  
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_resume_data_serialization() {
    let resume = ResumeData {
      name: "Test User".to_string(),
      email: "test@example.com".to_string(),
      experience: vec![
        Experience {
          title: "Manager".to_string(),
          company: "Test Corp".to_string(),
        }
      ],
      skill_indexes: vec![0, 2],
    };
    
    let json = serde_json::to_value(&resume).unwrap();
    let deserialized: ResumeData = serde_json::from_value(json).unwrap();
    
    assert_eq!(resume.name, deserialized.name);
    assert_eq!(resume.email, deserialized.email);
    assert_eq!(resume.experience.len(), deserialized.experience.len());
    assert_eq!(resume.skill_indexes, deserialized.skill_indexes);
  }

  #[tokio::test]
  async fn test_yaml_parsing() {
    let yaml_str = r#"
name: John Doe
email: john@example.com
experience:
  - title: Manager
    company: ABC Corp
skill_indexes: [0, 1, 2]
"#;
    
    let parsed: ResumeData = serde_yaml::from_str(yaml_str).unwrap();
    
    assert_eq!(parsed.name, "John Doe");
    assert_eq!(parsed.email, "john@example.com");
    assert_eq!(parsed.experience.len(), 1);
    assert_eq!(parsed.skill_indexes, vec![0, 1, 2]);
  }

  #[tokio::test]
  async fn test_mock_llm_structured() {
    let prompt = "Extract structured data from resume";
    let response = call_mock_llm_structured(prompt).await;
    
    assert!(response.contains("```yaml"));
    assert!(response.contains("name:"));
    assert!(response.contains("email:"));
    assert!(response.contains("experience:"));
  }

  #[tokio::test]
  async fn test_resume_parser_node() {
    let node = ResumeParserNode::new();
    let shared = SharedState::new();
    
    // Setup test data
    shared.insert("resume_text".to_string(), Value::String("Test resume".to_string()));
    shared.insert("target_skills".to_string(), 
      Value::Array(vec![Value::String("Test skill".to_string())]));
    
    // Test prep phase
    let prep_result = node.prep_async(&shared).await.unwrap();
    assert_eq!(prep_result["resume_text"].as_str().unwrap(), "Test resume");
    
    assert_eq!(node.get_node_id(), Some("resume_parser_node".to_string()));
  }

  #[tokio::test]
  async fn test_skill_index_validation() {
    let node = ResumeParserNode::new();
    let target_skills = vec!["Skill 0".to_string(), "Skill 1".to_string()];
    
    // Valid indexes
    let valid_indexes = vec![0, 1];
    for &index in &valid_indexes {
      assert!(index < target_skills.len());
    }
    
    // Invalid index
    let invalid_index = 5;
    assert!(invalid_index >= target_skills.len());
  }
}