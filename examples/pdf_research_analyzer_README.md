# PDF Research Paper Analyzer - AgentFlow Implementation

A comprehensive PDF research paper analysis system built with `agentflow-core` and `agentflow-llm`.

## ✅ Successfully Fixed Compilation Issues

All compilation errors have been resolved:
- ✅ Added missing dependencies (`reqwest`, `chrono`) to `Cargo.toml` 
- ✅ Fixed `AsyncFlow::new()` API usage (requires start node)
- ✅ Fixed `add_node()` method calls (requires node ID parameter)
- ✅ Fixed error type usage (`AsyncExecutionError` vs `ExecutionError`)
- ✅ Fixed borrow checker issues with shared state access
- ✅ Fixed concurrency issues in batch processing
- ✅ Fixed error type compatibility (`String` vs `Box<dyn std::error::Error>`)

## 🚀 Features

- **PDF Upload & Text Extraction** via StepFun Document Parser API
- **Intelligent Summarization** with structured output
- **Key Insights Extraction** (methodology, findings, contributions)
- **Mind Map Generation** in Mermaid format
- **Multi-language Translation** support
- **Batch Processing** with concurrency control
- **Structured JSON Output** with comprehensive metadata

## 📋 Prerequisites

1. **StepFun API Key**: Set `STEP_API_KEY` environment variable
2. **PDF Files**: Research papers to analyze (max 64MB per file)
3. **Rust Environment**: Cargo and Rust installed

## 🔧 Usage

### Single Paper Analysis

```rust
use pdf_research_analyzer::{PDFAnalyzer, AnalysisDepth};

#[tokio::main]
async fn main() -> Result<(), String> {
    let analyzer = PDFAnalyzer::new(std::env::var("STEP_API_KEY")?)
        .analysis_depth(AnalysisDepth::Comprehensive)
        .target_language("en")
        .model("step-2-16k");

    let result = analyzer.analyze_paper("./paper.pdf").await?;
    result.save_to_files("./analysis_output").await?;
    Ok(())
}
```

### Batch Processing

```rust
let batch_analyzer = PDFAnalyzer::new(api_key)
    .analysis_depth(AnalysisDepth::Summary)
    .model("step-2-mini");

let batch_result = batch_analyzer.analyze_batch("./papers_directory/").await?;
batch_result.save_to_directory("./batch_output").await?;
```

### Run the Example

```bash
# Set API key
export STEP_API_KEY="your-stepfun-api-key"

# Run the example (modify the PDF path in main function)
cargo run --example pdf_research_analyzer
```

## 🏗️ Architecture

### Workflow Nodes

1. **`PDFParserNode`**: Uploads PDF to StepFun API and extracts text
2. **`SummaryNode`**: Generates comprehensive research paper summaries  
3. **`InsightsNode`**: Extracts structured metadata and key insights
4. **`MindMapNode`**: Creates Mermaid mind map visualizations
5. **`TranslationNode`**: Multi-language translation support
6. **`ResultsCompilerNode`**: Aggregates all results into structured output

### AgentFlow Integration

- **`agentflow-core`**: Workflow orchestration with `AsyncFlow` and `AsyncNode`
- **`agentflow-llm`**: LLM integration with StepFun provider
- **Concurrency**: Batch processing with semaphore-controlled concurrency
- **Error Handling**: Comprehensive error handling with proper error types

## 📊 Output Structure

### Individual Analysis
- `summary.md` - Structured research paper summary
- `key_insights.json` - Extracted metadata and insights  
- `mind_map.mermaid` - Visual concept relationships
- `summary_{lang}.md` - Translated versions (optional)
- `complete_analysis.json` - Full analysis results

### Batch Analysis
- Individual results in separate directories
- `batch_analysis_report.json` - Processing summary
- Success/failure statistics and error details

## 🔄 Analysis Types

- **`Summary`**: Generate summary only
- **`Insights`**: Extract key insights only
- **`Comprehensive`**: Full analysis (summary + insights + mind map)
- **`WithTranslation`**: Everything + translation to target language

## ⚙️ Configuration Options

- **Model Selection**: `step-1-256k`, `step-2-16k`, `step-2-mini`, `step-3`
- **Target Languages**: `en`, `zh`, `es`, `fr`, `de`, `ja`, `ko`
- **Concurrency Control**: Configurable batch processing limits
- **Mind Map Generation**: Enable/disable visual representations

## 🛠️ Technical Implementation

### StepFun Document Parser Integration

```rust
// Upload PDF
let form = reqwest::multipart::Form::new()
    .part("file", reqwest::multipart::Part::bytes(file_data)
        .file_name(filename)
        .mime_str("application/pdf")?)
    .text("purpose", "file-extract");

let response = client
    .post("https://api.stepfun.com/v1/files")
    .header("Authorization", format!("Bearer {}", api_key))
    .multipart(form)
    .send().await?;

// Retrieve parsed content
let content_response = client
    .get(&format!("https://api.stepfun.com/v1/files/{}/content", file_id))
    .header("Authorization", format!("Bearer {}", api_key))
    .send().await?;
```

### LLM Integration

```rust
let response = AgentFlow::model(&self.model)
    .prompt(&analysis_prompt)
    .temperature(0.3)
    .max_tokens(2000)
    .execute().await?;
```

## 🚦 Error Handling

- **Network Errors**: Robust retry logic for API calls
- **File Processing**: Comprehensive error messages for upload failures
- **LLM Errors**: Graceful handling of model timeouts and rate limits
- **Batch Processing**: Continue-on-error strategy for batch jobs

## 🎯 Why This Approach Works

### Advantages of StepFun's Built-in Parsing

1. **Simpler Integration**: Direct API calls vs complex PDF libraries
2. **Better Context Handling**: Optimized for LLM processing
3. **Token Management**: Built-in counting for large documents  
4. **No Maintenance**: No PDF parsing dependencies to maintain
5. **Native Support**: Up to 64MB PDFs with pure text extraction

### AgentFlow Benefits

1. **Workflow Orchestration**: Clean separation of concerns with async nodes
2. **Error Recovery**: Built-in retry mechanisms and circuit breakers
3. **Observability**: Metrics collection and execution tracking
4. **Scalability**: Concurrent processing with resource management

## 📝 Example Output

### Generated Summary
```markdown
# Research Paper Summary

## Title and Authors
"Attention Is All You Need" by Ashish Vaswani et al.

## Abstract Summary  
Introduces the Transformer architecture for sequence-to-sequence tasks using only attention mechanisms.

## Research Problem
Sequential computation limits parallelization in RNN and CNN models for sequence modeling.

## Methodology
Self-attention mechanism with multi-head attention and positional encoding.

## Key Findings
1. Transformer achieves superior performance on translation tasks
2. Significantly faster training due to parallelization
3. Better handling of long-range dependencies

...
```

### Extracted Insights (JSON)
```json
{
  "title": "Attention Is All You Need",
  "authors": ["Ashish Vaswani", "Noam Shazeer", "..."],
  "field_of_study": "Natural Language Processing",
  "research_type": "experimental",
  "key_contributions": ["Transformer architecture", "Self-attention mechanism"],
  "impact_potential": "high",
  "reproducibility": "high"
}
```

## 🔍 Testing

The implementation compiles successfully and is ready for testing with real PDF files:

```bash
cargo build --example pdf_research_analyzer  # ✅ Success
cargo check --example pdf_research_analyzer  # ✅ No errors
```

## 🤝 Contributing

This implementation demonstrates best practices for:
- AgentFlow workflow development
- LLM integration patterns
- Error handling strategies
- Concurrent processing techniques

The code serves as a complete reference for building document analysis applications with AgentFlow!