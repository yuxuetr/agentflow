# New Nodes: MarkMapNode and ArxivNode

This document describes the two new specialized content processing nodes added to the `agentflow-nodes` crate.

## MarkMapNode

Converts Markdown content into interactive mind map HTML files using the markmap-api service.

### Features

- **Template Support**: Supports variable substitution using `{{variable}}` syntax
- **Configurable API**: Uses the markmap-api service with customizable endpoint
- **Rich Configuration**: Supports themes, color freeze levels, expansion levels, and more
- **File Output**: Can save generated HTML files to disk
- **Shared State Integration**: Stores results in workflow shared state

### Configuration Options

```rust
pub struct MarkMapConfig {
  pub api_url: Option<String>,           // Default: https://markmap-api.jinpeng-ti.workers.dev
  pub title: Option<String>,             // Mind map title
  pub theme: Option<String>,             // "light", "dark", or "auto"
  pub color_freeze_level: Option<u8>,    // 0-10, controls color variation
  pub initial_expand_level: Option<i8>,  // -1 to 10, initial expansion level
  pub max_width: Option<u32>,            // Max node width in pixels
  pub timeout_seconds: Option<u64>,      // Request timeout
}
```

### Usage Example

```rust
use agentflow_core::SharedState;
use agentflow_nodes::{MarkMapNode, MarkMapConfig, AsyncNode};

let shared_state = SharedState::new();
shared_state.insert("project_name".to_string(), json!("AgentFlow"));

let node = MarkMapNode::new(
    "project_overview",
    r#"# {{project_name}}
## Core Features
### Workflow Engine
- Async execution
- Node composition
### LLM Integration  
- Multiple providers
- Streaming support"#
)
.with_output_key("mindmap_result")
.with_file_output("overview.html");

let result = node.run_async(&shared_state).await?;
```

### API Integration

The node integrates with the markmap-api service (https://github.com/yuxuetr/markmap-api) which provides:
- `/api/render` endpoint for generating HTML mind maps
- Support for themes and customization options
- Interactive mind map features with zoom, pan, and expand/collapse

## ArxivNode

Retrieves LaTeX source content from arXiv papers using HTTP requests, with advanced LaTeX processing capabilities including automatic file extraction, main file detection, and complete content expansion.

### Features

- **Multiple URL Formats**: Supports various arXiv URL formats and paper ID formats
- **Template Support**: URL can contain template variables
- **Archive Processing**: Automatically extracts tar.gz archives to temporary directories
- **Smart Main File Detection**: Identifies the main LaTeX file using multiple heuristics
- **Content Expansion**: Recursively expands all `\input`, `\include`, and `\subfile` commands
- **Circular Include Protection**: Detects and prevents infinite recursion in file includes
- **Depth Limiting**: Configurable maximum recursion depth for safety
- **Bibliography Handling**: Processes `\bibliography` and `\addbibresource` commands
- **Complete Text Extraction**: Generates a single expanded document with all content

### Configuration Options

```rust
pub struct ArxivConfig {
  pub timeout_seconds: Option<u64>,     // Request timeout (default: 60)
  pub save_latex: Option<bool>,         // Save LaTeX source to file
  pub extract_files: Option<bool>,      // Extract tar.gz contents
  pub expand_content: Option<bool>,     // Expand all included files (default: true)
  pub max_include_depth: Option<u32>,   // Max recursion depth (default: 10)
  pub user_agent: Option<String>,       // HTTP User-Agent string
}
```

### Supported URL Formats

- `https://arxiv.org/abs/2312.07104` - Abstract page URL
- `https://arxiv.org/abs/2312.07104v2` - Versioned abstract URL  
- `https://arxiv.org/pdf/2312.07104.pdf` - PDF URL
- `2312.07104` - Bare paper ID
- `2312.07104v1` - Versioned paper ID

### Usage Example

```rust
use agentflow_core::SharedState;
use agentflow_nodes::{ArxivNode, ArxivConfig, AsyncNode};

let shared_state = SharedState::new();
shared_state.insert("paper_id".to_string(), json!("2312.07104"));

let config = ArxivConfig {
    extract_files: Some(true),
    save_latex: Some(true),
    expand_content: Some(true),      // Enable content expansion
    max_include_depth: Some(5),      // Limit recursion depth
    ..Default::default()
};

let node = ArxivNode::new(
    "paper_source", 
    "https://arxiv.org/abs/{{paper_id}}"
)
.with_config(config)
.with_output_key("paper_data")
.with_output_directory("./arxiv_papers");

let result = node.run_async(&shared_state).await?;
```

### Output Structure

Both nodes store comprehensive results in shared state. The ArxivNode now provides detailed LaTeX processing information:

```json
{
  "paper_id": "2312.07104",
  "version": "v2",
  "source_url": "https://arxiv.org/src/2312.07104v2", 
  "content_size": 156789,
  "latex_info": {
    // For processed archives:
    "main_file": "main.tex",
    "main_content": "\\documentclass{article}...",
    "expanded_content": "\\documentclass{article}...\n% === BEGIN INCLUDED FILE: intro.tex ===\n...",
    "extracted_files_count": 15,
    "has_expanded_content": true,
    
    // For simple LaTeX files:
    "simple_latex_content": "\\documentclass{article}...",
    "is_simple_tex": true
  },
  "saved_path": "./arxiv_papers/2312_07104.tar.gz",
  "node_name": "paper_source",
  "timestamp": "2025-09-04T10:30:00Z"
}
```

### LaTeX Processing Features

#### Main File Detection
The ArxivNode uses intelligent heuristics to find the main LaTeX file:

1. **Known filenames**: `main.tex`, `paper.tex`, `manuscript.tex`, `article.tex`, `document.tex`
2. **Document class detection**: Files containing `\documentclass`
3. **Shortest name preference**: For multiple candidates, chooses the shortest filename
4. **Fallback**: Any `.tex` file as last resort

#### Content Expansion
Recursively processes these LaTeX commands:
- `\input{file}` - Includes file content inline
- `\include{file}` - Includes file with page breaks
- `\subfile{file}` - Includes subfile content
- `\InputIfFileExists{file}` - Conditional file inclusion
- `\bibliography{file}` - Bibliography references (adds comments)
- `\addbibresource{file}` - Bibliography resources (adds comments)

#### Safety Features
- **Circular include detection**: Prevents infinite recursion loops
- **Depth limiting**: Configurable maximum include depth (default: 10)
- **Missing file handling**: Graceful handling of missing includes with informative comments
- **File variation matching**: Tries multiple filename variations (with/without .tex extension)

## Integration with AgentFlow

Both nodes implement the `AsyncNode` trait and integrate seamlessly with AgentFlow workflows:

- **Code-first**: Use directly in Rust code
- **Configuration-first**: Can be configured via YAML workflow definitions
- **Template resolution**: Support for shared state variable substitution
- **Error handling**: Comprehensive error types and propagation
- **Observability**: Built-in metrics and logging support

## Examples

See the complete examples:
- `cargo run --example markmap_example`
- `cargo run --example arxiv_example`

## Testing

Both nodes include comprehensive unit tests:
- Template resolution
- URL parsing and validation  
- Configuration handling
- Error scenarios
- Node lifecycle

Run tests with:
```bash
cargo test nodes::markmap::tests
cargo test nodes::arxiv::tests
```

## Dependencies

New dependencies added for these nodes:
- `flate2`: For gzip decompression (ArxivNode)
- `tar`: For tar archive extraction (ArxivNode)  
- `base64`: For binary content encoding (ArxivNode)
- `regex`: For LaTeX command pattern matching (ArxivNode)

The MarkMapNode uses existing HTTP client capabilities via `reqwest`.

## Example: Complete Paper Processing

Here's a complete example showing the enhanced ArxivNode capabilities:

```rust
use agentflow_core::SharedState;
use agentflow_nodes::{ArxivNode, ArxivConfig, AsyncNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shared_state = SharedState::new();
    shared_state.insert("paper_id".to_string(), json!("2312.07104"));

    let config = ArxivConfig {
        extract_files: Some(true),
        expand_content: Some(true),
        max_include_depth: Some(8),
        ..Default::default()
    };

    let node = ArxivNode::new("research_paper", "https://arxiv.org/abs/{{paper_id}}")
        .with_config(config)
        .with_output_key("paper_data")
        .with_output_directory("./papers");

    let result = node.run_async(&shared_state).await?;
    
    if let Some(paper_data) = shared_state.get("paper_data") {
        if let Some(latex_info) = paper_data.get("latex_info") {
            if let Some(expanded_content) = latex_info.get("expanded_content") {
                // Now you have the complete paper text with all includes resolved!
                println!("Complete paper content: {}", expanded_content.as_str().unwrap());
            }
        }
    }

    Ok(())
}
```

This will:
1. Download the arXiv source archive
2. Extract all files to a temporary directory  
3. Identify the main LaTeX file (e.g., `main.tex`)
4. Recursively expand all `\input`, `\include`, and `\subfile` commands
5. Handle circular includes and missing files gracefully
6. Return the complete expanded paper content as a single string