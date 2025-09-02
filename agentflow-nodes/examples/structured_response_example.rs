//! Structured JSON Response Example
//! 
//! This example demonstrates how to use agentflow-nodes to get structured
//! JSON responses from LLMs with strict schema validation.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::{LlmNode, ResponseFormat};
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Structured JSON Response Example");
  println!("====================================\n");

  let shared = SharedState::new();

  // Add sample data for analysis
  shared.insert("customer_review".to_string(), Value::String(
    "I absolutely love this product! The quality is amazing and it arrived so quickly. \
     The customer service team was super helpful when I had questions. However, the price \
     is a bit steep compared to competitors, and the packaging could be more eco-friendly. \
     Overall, I'd definitely recommend it to friends and family.".to_string()
  ));

  shared.insert("product_data".to_string(), json!({
    "name": "UltraWidget Pro",
    "category": "Electronics",
    "price": 299.99,
    "launch_date": "2024-01-15",
    "features": ["wireless", "waterproof", "5-year warranty"]
  }));

  // 1. Sentiment Analysis with Strict Schema
  println!("😊 Step 1: Sentiment Analysis with Structured Output");
  
  let sentiment_schema = json!({
    "type": "object",
    "properties": {
      "overall_sentiment": {
        "type": "string",
        "enum": ["very_positive", "positive", "neutral", "negative", "very_negative"],
        "description": "Overall sentiment classification"
      },
      "sentiment_score": {
        "type": "number",
        "minimum": -1.0,
        "maximum": 1.0,
        "description": "Numerical sentiment score from -1 (very negative) to 1 (very positive)"
      },
      "confidence": {
        "type": "number",
        "minimum": 0.0,
        "maximum": 1.0,
        "description": "Confidence in the sentiment analysis"
      },
      "key_emotions": {
        "type": "array",
        "items": {
          "type": "string",
          "enum": ["joy", "anger", "fear", "sadness", "surprise", "disgust", "trust", "anticipation"]
        },
        "description": "Detected emotions in the text"
      },
      "positive_aspects": {
        "type": "array",
        "items": {"type": "string"},
        "description": "Specific positive mentions"
      },
      "negative_aspects": {
        "type": "array", 
        "items": {"type": "string"},
        "description": "Specific negative mentions"
      },
      "actionable_insights": {
        "type": "array",
        "items": {"type": "string"},
        "description": "Business insights and recommended actions"
      }
    },
    "required": ["overall_sentiment", "sentiment_score", "confidence", "key_emotions"]
  });

  let sentiment_node = LlmNode::new("sentiment_analyzer", "gpt-4o")
    .with_prompt("Analyze the sentiment of this customer review: {{customer_review}}")
    .with_system("You are an expert sentiment analysis system. Provide accurate, structured analysis of customer feedback.")
    .with_temperature(0.1) // Very low for consistent analysis
    .with_max_tokens(500)
    .with_json_response(Some(sentiment_schema));

  match sentiment_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("sentiment_analyzer_output") {
        println!("✅ Sentiment Analysis Result:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        }
      }
    }
    Err(e) => {
      println!("❌ Sentiment analysis failed: {}", e);
    }
  }

  // 2. Product Feature Extraction
  println!("🔍 Step 2: Product Feature Extraction");

  let feature_schema = json!({
    "type": "object",
    "properties": {
      "mentioned_features": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "feature_name": {"type": "string"},
            "sentiment": {"type": "string", "enum": ["positive", "neutral", "negative"]},
            "importance_score": {"type": "number", "minimum": 1, "maximum": 10}
          },
          "required": ["feature_name", "sentiment"]
        }
      },
      "quality_rating": {
        "type": "integer",
        "minimum": 1,
        "maximum": 5,
        "description": "Inferred quality rating from review"
      },
      "value_perception": {
        "type": "string",
        "enum": ["excellent_value", "good_value", "fair_value", "poor_value", "overpriced"]
      },
      "recommendation_likelihood": {
        "type": "integer",
        "minimum": 0,
        "maximum": 10,
        "description": "Likelihood to recommend (0-10)"
      },
      "improvement_suggestions": {
        "type": "array",
        "items": {"type": "string"},
        "description": "Specific areas for product improvement"
      }
    },
    "required": ["mentioned_features", "quality_rating", "value_perception", "recommendation_likelihood"]
  });

  let feature_node = LlmNode::new("feature_extractor", "claude-3-5-sonnet")
    .with_prompt("Extract product features and insights from this review: {{customer_review}}\n\nProduct context: {{product_data}}")
    .with_system("You are a product analysis expert. Extract detailed insights about product features, quality, and customer satisfaction.")
    .with_temperature(0.2)
    .with_max_tokens(600)
    .with_json_response(Some(feature_schema));

  match feature_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("feature_extractor_output") {
        println!("✅ Feature Extraction Result:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        }
      }
    }
    Err(e) => {
      println!("❌ Feature extraction failed: {}", e);
    }
  }

  // 3. Comprehensive Business Report Generation
  println!("📊 Step 3: Business Intelligence Report");

  let report_schema = json!({
    "type": "object",
    "properties": {
      "executive_summary": {
        "type": "string",
        "description": "Brief executive summary of findings"
      },
      "customer_satisfaction": {
        "type": "object",
        "properties": {
          "overall_score": {"type": "number", "minimum": 0, "maximum": 100},
          "key_drivers": {"type": "array", "items": {"type": "string"}},
          "risk_factors": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["overall_score"]
      },
      "competitive_positioning": {
        "type": "object",
        "properties": {
          "strengths": {"type": "array", "items": {"type": "string"}},
          "weaknesses": {"type": "array", "items": {"type": "string"}},
          "market_opportunities": {"type": "array", "items": {"type": "string"}}
        }
      },
      "action_items": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "priority": {"type": "string", "enum": ["high", "medium", "low"]},
            "department": {"type": "string"},
            "action": {"type": "string"},
            "timeline": {"type": "string"},
            "expected_impact": {"type": "string"}
          },
          "required": ["priority", "action"]
        }
      },
      "metrics_to_track": {
        "type": "array",
        "items": {"type": "string"},
        "description": "KPIs to monitor based on this analysis"
      }
    },
    "required": ["executive_summary", "customer_satisfaction", "action_items"]
  });

  let report_node = LlmNode::new("business_reporter", "gpt-4o")
    .with_prompt(r#"
Create a comprehensive business intelligence report based on:

Customer Review: {{customer_review}}
Product Data: {{product_data}}  
Sentiment Analysis: {{sentiment_analyzer_output}}
Feature Analysis: {{feature_extractor_output}}

Provide strategic insights and actionable recommendations for business leadership.
"#)
    .with_system("You are a senior business analyst. Create executive-level reports with clear insights and actionable recommendations.")
    .with_temperature(0.3)
    .with_max_tokens(800)
    .with_top_p(0.9)
    .with_json_response(Some(report_schema));

  match report_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("business_reporter_output") {
        println!("✅ Business Intelligence Report:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        }
      }
    }
    Err(e) => {
      println!("❌ Business report generation failed: {}", e);
    }
  }

  // 4. Simple JSON Response (loose schema)
  println!("📝 Step 4: Simple JSON Response (No Strict Schema)");

  let simple_node = LlmNode::new("simple_analyzer", "gpt-4o-mini")
    .with_prompt("Summarize this customer feedback in simple key-value pairs: {{customer_review}}")
    .with_system("Provide a simple JSON summary with basic key-value pairs.")
    .with_temperature(0.4)
    .with_max_tokens(200)
    .with_response_format(ResponseFormat::loose_json()); // Loose JSON without strict schema

  match simple_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("simple_analyzer_output") {
        println!("✅ Simple JSON Summary:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        }
      }
    }
    Err(e) => {
      println!("❌ Simple analysis failed: {}", e);
    }
  }

  // Summary of Results
  println!("📋 Results Summary:");
  let analyses = [
    ("Sentiment Analysis", "sentiment_analyzer_output"),
    ("Feature Extraction", "feature_extractor_output"),  
    ("Business Report", "business_reporter_output"),
    ("Simple Summary", "simple_analyzer_output")
  ];

  for (name, key) in analyses {
    if let Some(result) = shared.get(key) {
      if let Ok(_) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
        println!("   ✅ {}: Valid JSON structure", name);
      } else {
        println!("   ⚠️  {}: Invalid JSON (may be text response)", name);
      }
    } else {
      println!("   ❌ {}: No result generated", name);
    }
  }

  println!("\n🏁 Structured response example completed!");
  println!("💡 This example demonstrated:");
  println!("   • Strict JSON schema validation for consistent responses");
  println!("   • Complex nested object structures");
  println!("   • Enum constraints for controlled values");
  println!("   • Number range validation");
  println!("   • Array type specifications");
  println!("   • Required vs optional fields");
  println!("   • Loose JSON mode for simple structures");
  println!("   • Chaining structured analyses for complex workflows");

  Ok(())
}