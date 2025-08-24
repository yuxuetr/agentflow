# AgentFlow MCP Integration Guide

## Overview

AgentFlow now includes comprehensive Model Context Protocol (MCP) integration, enabling seamless connection to external tools and services. This guide demonstrates how to use the MCP integration for visual output generation, specifically converting mind maps to images using MarkMap.

## Architecture

### New Components

1. **agentflow-mcp** - Dedicated MCP client/server library
2. **MCPToolNode** - Generic MCP tool execution node
3. **MarkMapVisualizerNode** - Specialized MarkMap integration
4. **Visual workflow configurations** - Enhanced YAML workflows

### MCP Integration Layers

```
┌─────────────────────┐
│   Workflow YAML     │ ← High-level configuration
├─────────────────────┤
│  Specialized Nodes  │ ← MarkMapVisualizerNode, PosterGeneratorNode
├─────────────────────┤
│   Generic MCPNode   │ ← Universal MCP tool caller
├─────────────────────┤
│   agentflow-mcp     │ ← MCP protocol implementation
├─────────────────────┤
│  MCP Servers        │ ← External tools (MarkMap, DALL-E, etc.)
└─────────────────────┘
```

## Quick Start

### 1. Install MarkMap MCP Server

```bash
# Install the MarkMap MCP server
npm install -g @jinzcdev/markmap-mcp-server

# Or use via npx (no installation needed)
npx -y @jinzcdev/markmap-mcp-server
```

### 2. Run Paper Analysis with Visual Output

```bash
# Analyze a PDF with visual mind map generation
cd agentflow-agents/agents/paper_research_analyzer

cargo run -- \
  --pdf-path "../../../assets/2312.07104v2.pdf" \
  --output-dir "./analysis_output" \
  --depth comprehensive \
  --mind-map \
  --model step-2-16k
```

### 3. Check Visual Outputs

```bash
ls analysis_output/
# Expected files:
# - summary.md
# - key_insights.json  
# - mind_map.md
# - mind_map.png        ← NEW: Visual mind map!
# - complete_analysis.json
```

## Enhanced Workflow Configuration

### Visual Analysis Workflow

The new `visual_analysis.yml` workflow includes:

```yaml
# Enhanced workflow with visual outputs
config:
  generate_mind_map: true
  generate_visual_mindmap: true  # NEW
  mindmap_format: "png"          # png, svg, html
  generate_poster: false         # Future feature

nodes:
  - name: "markmap_visualizer"   # NEW NODE
    type: "markmap_visualizer"
    dependencies: ["mind_mapper"]
    config:
      export_format: "${mindmap_format}"
      auto_open: false
      output_dir: "${output_dir}"

mcp_servers:                     # NEW: MCP server definitions
  markmap:
    type: "stdio"
    command: ["npx", "-y", "@jinzcdev/markmap-mcp-server"]
```

## MCP Tool Integration

### Generic MCP Tool Node

Use any MCP-compatible tool:

```rust
// Create a generic MCP tool node
let mcp_node = MCPToolNode::new("tool_name", vec![
    "server_command".to_string(), 
    "args".to_string()
])
.with_parameters(json!({
    "param1": "value1",
    "param2": "value2"
}))
.with_parameter_templates(HashMap::from([
    ("dynamic_param".to_string(), "{{shared_state_key}}".to_string())
]));
```

### Specialized Nodes

Create domain-specific nodes for better ergonomics:

```rust
// MarkMap visualizer (already implemented)
let markmap = MarkMapVisualizerNode::new("png".to_string())
    .with_auto_open(false)
    .with_output_dir("./output");

// Future: Image generation node
let poster_gen = ImageGeneratorNode::new("dall-e-3")
    .with_style("academic_poster")
    .with_size("1024x1792");
```

## Available MCP Integrations

### Current: MarkMap Visualization

- **Server**: `@jinzcdev/markmap-mcp-server`
- **Tool**: `markdown-to-mindmap`
- **Outputs**: PNG, SVG, HTML mind maps
- **Status**: ✅ Fully implemented

### Planned: Image Generation

- **Server**: Custom DALL-E/Midjourney MCP server
- **Tool**: `generate_research_poster`
- **Outputs**: PNG research posters
- **Status**: 🔄 Architecture ready

### Planned: Document Processing

- **Server**: Custom PDF/text processing server
- **Tools**: `extract_images`, `ocr_document`, `chunk_text`
- **Outputs**: Processed document components
- **Status**: 📋 Future enhancement

## Integration Examples

### Example 1: Research Paper → Visual Mind Map

```bash
# Input: PDF research paper
# Output: Interactive mind map visualization
cargo run -- --pdf-path paper.pdf --depth comprehensive --mind-map
```

**Process Flow:**
1. PDF → Text extraction
2. Text → LLM analysis
3. Analysis → Structured insights  
4. Insights → Markdown mind map
5. **NEW:** Markdown → Visual PNG via MarkMap MCP

### Example 2: Batch Analysis with Visuals

```bash
# Process multiple papers with visual outputs
cargo run -- \
  --batch-dir ./papers/ \
  --output-dir ./analysis_output/ \
  --depth comprehensive \
  --mind-map \
  --generate-visuals
```

**Output Structure:**
```
analysis_output/
├── paper1/
│   ├── summary.md
│   ├── mind_map.md
│   └── mind_map.png        ← Visual output
├── paper2/
│   ├── summary.md  
│   ├── mind_map.md
│   └── mind_map.png        ← Visual output
└── batch_analysis_report.json
```

## Configuration Options

### MarkMap Visualization

```yaml
markmap_visualizer:
  export_format: "png"      # png, svg, html
  auto_open: false          # Auto-open in browser
  output_dir: "./output"    # Output directory
```

### MCP Server Configuration

```yaml
mcp_servers:
  markmap:
    type: "stdio"           # stdio or http
    command: ["npx", "-y", "@jinzcdev/markmap-mcp-server"]
    timeout: 30000          # Timeout in milliseconds
    
  custom_server:
    type: "http" 
    base_url: "http://localhost:8080/mcp"
    headers:
      authorization: "Bearer ${API_KEY}"
```

## Troubleshooting

### Common Issues

1. **MarkMap server not found**
   ```bash
   # Solution: Install MarkMap MCP server
   npm install -g @jinzcdev/markmap-mcp-server
   ```

2. **Permission denied errors**
   ```bash
   # Solution: Check Node.js and npm permissions
   npm config get prefix
   # Or use npx instead of global install
   ```

3. **Mind map generation fails**
   - Check that mind map markdown is generated first
   - Verify MCP server is accessible
   - Check server logs for errors

### Debug Mode

Enable detailed MCP communication logging:

```bash
RUST_LOG=debug cargo run -- --pdf-path paper.pdf --mind-map
```

## Future Enhancements

### Planned Features

1. **Image Generation Integration**
   - Research poster generation from summaries
   - Diagram creation from insights
   - Custom visual styles

2. **Additional MCP Servers**
   - Web scraping tools
   - Database connectors  
   - API integrations

3. **Advanced Workflows**
   - Conditional visual generation
   - Multi-format outputs
   - Interactive visualizations

### Extension Points

The MCP integration is designed for easy extension:

```rust
// Add new MCP tool
impl AsyncNode for CustomMCPNode {
    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let client = MCPClient::stdio(self.server_command.clone());
        let result = client.call_tool_simple(&self.tool_name, params).await?;
        // Process result...
    }
}
```

## Performance Considerations

- MCP servers run as separate processes
- stdio transport has lower overhead than HTTP
- Visual generation adds ~2-5 seconds per mind map
- Batch processing benefits from concurrent server connections

## Security Notes

- MCP servers run with same permissions as AgentFlow
- External servers should be trusted sources only
- Network-based MCP servers require additional security review
- Template parameter injection is protected against

---

**AgentFlow MCP Integration** enables powerful visual workflows while maintaining the simplicity and flexibility of the core system. Start with MarkMap visualization and expand to custom MCP tools as needed.