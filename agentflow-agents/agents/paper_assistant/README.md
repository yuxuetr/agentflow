# Paper Assistant

An AI-powered agent for comprehensive arXiv paper processing with Chinese translation, summarization, mind mapping, and poster generation capabilities.

## Features

- 📄 **ArXiv Paper Fetching**: Automatically downloads and processes LaTeX source content from arXiv URLs
- 🇨🇳 **Chinese Translation**: Full paper translation to Chinese using Qwen-Turbo model
- 📝 **Chinese Summarization**: Intelligent summarization of key research points in Chinese
- 🧠 **Mind Mapping**: Interactive mind maps for each paper section using MarkMap
- 🎨 **Poster Generation**: AI-generated research posters using Qwen-VL model
- ⚙️ **Flexible Configuration**: Customizable processing modes and parameters
- 📊 **Multiple Output Formats**: Markdown, HTML, and JSON outputs

## Architecture

The Paper Assistant is built using the AgentFlow framework with a modular workflow design:

```
ArxivNode → LLMNode (Summary) → LLMNode (Translation) → LLMNode (Sections) → MarkMapNode → TextToImageNode
```

### Core Components

- **ArxivNode**: Fetches and processes LaTeX source from arXiv
- **LLMNode**: Handles Chinese summarization and translation using Qwen models
- **MarkMapNode**: Generates interactive mind maps for paper sections
- **TextToImageNode**: Creates research posters using AI image generation

## Installation

### Prerequisites

- Rust 1.75+ 
- DashScope API key for Qwen models
- Internet connection for arXiv and API access

### Build from Source

```bash
# Clone the repository
git clone <repository-url>
cd agentflow/agentflow-agents/agents/paper_assistant

# Build the project
cargo build --release

# The binary will be available at:
# target/release/paper-assistant
```

### Environment Variables

Set up your API keys and configuration:

```bash
export DASHSCOPE_API_KEY="your-dashscope-api-key"
export PAPER_ASSISTANT_OUTPUT_DIR="./output"  # Optional
export RUST_LOG="info"  # Optional, for logging
```

## Usage

### Basic Usage

```bash
# Process a paper by URL
./paper-assistant process https://arxiv.org/abs/2312.07104

# Process a paper by ID
./paper-assistant process 2312.07104

# Specify custom output directory
./paper-assistant process 2312.07104 -o ./my_results
```

### Processing Modes

```bash
# Fast mode (skip image generation, fewer sections)
./paper-assistant process 2312.07104 --fast

# Comprehensive mode (detailed analysis, more sections)
./paper-assistant process 2312.07104 --comprehensive

# Skip specific features
./paper-assistant process 2312.07104 --no-mindmaps --no-poster

# Limit number of sections for mind mapping
./paper-assistant process 2312.07104 --max-sections 5
```

### Configuration Management

```bash
# Show default configuration
./paper-assistant config show

# Create configuration file
./paper-assistant config create -t comprehensive -o my-config.json

# Use custom configuration
./paper-assistant process 2312.07104 -c my-config.json
```

### Examples and Help

```bash
# Show detailed usage examples
./paper-assistant examples

# Get help
./paper-assistant --help
./paper-assistant process --help
```

## Configuration

### Default Configuration

The Paper Assistant uses sensible defaults optimized for Chinese academic content:

```json
{
  "qwen_turbo_model": "qwen-turbo",
  "qwen_image_model": "qwen-vl-plus",
  "temperature": 0.3,
  "max_tokens": 4000,
  "output_directory": "./paper_assistant_output",
  "enable_mind_maps": true,
  "enable_poster_generation": true,
  "max_sections_for_mind_maps": 10
}
```

### Custom Configuration

Create a custom configuration file:

```json
{
  "qwen_turbo_model": "qwen-plus",
  "qwen_image_model": "qwen-vl-max",
  "temperature": 0.2,
  "max_tokens": 6000,
  "output_directory": "./custom_output",
  "chinese_summary_prompt": "请生成详细的中文摘要...",
  "enable_mind_maps": true,
  "enable_poster_generation": false,
  "max_sections_for_mind_maps": 8
}
```

### Environment Variables

Override configuration with environment variables:

- `QWEN_TURBO_MODEL`: Override the Qwen text model
- `QWEN_IMAGE_MODEL`: Override the Qwen image model  
- `PAPER_ASSISTANT_OUTPUT_DIR`: Default output directory
- `PAPER_ASSISTANT_TEMPERATURE`: LLM temperature (0.0-2.0)
- `PAPER_ASSISTANT_MAX_TOKENS`: Maximum tokens per request
- `DASHSCOPE_API_KEY`: Required API key for Qwen models

## Output Structure

The Paper Assistant creates a comprehensive output structure:

```
paper_assistant_output/
├── 2312.07104_paper_assistant_summary.md          # Chinese summary
├── 2312.07104_paper_assistant_translation.md     # Full Chinese translation
├── 2312.07104_paper_assistant_complete_results.json  # Complete results
├── 2312.07104_paper_assistant_mindmap_01_引言.html   # Section mind maps
├── 2312.07104_paper_assistant_mindmap_02_方法.html
├── ...
└── poster_image.png                               # Generated poster (if enabled)
```

### Output Files

- **Summary**: Detailed Chinese summary with research background, methods, and conclusions
- **Translation**: Complete Chinese translation maintaining academic structure
- **Mind Maps**: Interactive HTML mind maps for each paper section
- **Poster**: AI-generated research poster based on the Chinese summary
- **JSON Results**: Machine-readable complete results for further processing

## Workflow Details

### Step-by-Step Process

1. **Paper Retrieval**: Downloads LaTeX source from arXiv, handles various URL formats
2. **Content Extraction**: Processes LaTeX files, expands includes, extracts main content  
3. **Chinese Summary**: Generates comprehensive Chinese summary highlighting key points
4. **Chinese Translation**: Translates full paper content to fluent Chinese
5. **Section Analysis**: Extracts and analyzes paper sections for mind mapping
6. **Mind Map Generation**: Creates interactive mind maps for each section in Chinese
7. **Poster Generation**: Creates academic poster based on Chinese summary and title

### Supported arXiv Formats

- `https://arxiv.org/abs/2312.07104`
- `https://arxiv.org/abs/2312.07104v2` 
- `https://arxiv.org/pdf/2312.07104.pdf`
- `2312.07104`
- `2312.07104v2`

## API Integration

### Qwen Models (DashScope)

The Paper Assistant uses Alibaba's Qwen models through the DashScope API:

- **qwen-turbo**: For Chinese summarization and translation
- **qwen-vl-plus**: For poster image generation
- **qwen-plus**: Higher quality text model (optional)
- **qwen-vl-max**: Higher quality image model (optional)

### Mind Map Service

Uses the MarkMap API service for generating interactive mind maps from markdown content.

## Error Handling and Recovery

The Paper Assistant includes robust error handling:

- **Partial Results**: Saves intermediate results if processing fails
- **Debug Information**: Generates debug state for troubleshooting
- **Graceful Degradation**: Continues processing even if some steps fail
- **Retry Logic**: Built-in retry mechanisms for API calls

## Performance Optimization

### Fast Mode

- Reduces max tokens to 2000
- Limits sections to 5 for mind mapping
- Skips poster generation
- Uses lower temperature for faster, more focused responses

### Comprehensive Mode  

- Increases max tokens to 8000
- Processes up to 15 sections
- Enables all features including poster generation
- Uses higher temperature for more creative outputs

## Development

### Project Structure

```
src/
├── main.rs          # CLI application entry point
├── lib.rs           # Main library interface
├── workflow.rs      # Workflow orchestration logic  
├── config.rs        # Configuration management
└── utils.rs         # Utility functions for text processing
```

### Dependencies

- `agentflow-core`: Core workflow engine
- `agentflow-llm`: LLM provider integrations
- `agentflow-nodes`: Pre-built processing nodes
- `tokio`: Async runtime
- `clap`: CLI argument parsing
- `serde`: Serialization support

### Testing

```bash
# Run unit tests
cargo test

# Run with logging
RUST_LOG=debug cargo test

# Test specific module
cargo test config
```

## Troubleshooting

### Common Issues

1. **API Key Missing**
   ```
   Error: Configuration error: DASHSCOPE_API_KEY not set
   ```
   Solution: Set the `DASHSCOPE_API_KEY` environment variable

2. **arXiv Download Failed**
   ```
   Error: ArXiv fetch failed: HTTP 404
   ```
   Solution: Verify the arXiv paper ID or URL is correct

3. **Mind Map Generation Failed**
   ```
   Warning: Failed to generate mind map for section 1
   ```
   Solution: Check internet connection and MarkMap API availability

4. **Output Directory Permissions**
   ```
   Error: Failed to create output directory
   ```
   Solution: Ensure write permissions for the output directory

### Debug Mode

Enable debug logging for detailed information:

```bash
RUST_LOG=debug ./paper-assistant process 2312.07104
```

### Partial Results Recovery

If processing fails, check for partial results in the output directory:

```bash
ls -la paper_assistant_output/partial_results/
cat paper_assistant_output/partial_results/debug_state.json
```

## Examples

### Example 1: Quick Processing

```bash
# Fast processing of a recent paper
./paper-assistant process https://arxiv.org/abs/2312.07104 --fast -o ./quick_results
```

Expected output:
- Chinese summary (~300 words)
- Chinese translation (abbreviated)
- 3-5 mind maps for main sections
- No poster image

### Example 2: Comprehensive Analysis

```bash
# Detailed analysis with all features
./paper-assistant process 2312.07104 --comprehensive -o ./detailed_analysis
```

Expected output:
- Detailed Chinese summary (~800 words)
- Complete Chinese translation
- 10+ mind maps for all sections
- AI-generated research poster

### Example 3: Custom Configuration

```bash
# Create custom config
./paper-assistant config create -t comprehensive -o analysis-config.json

# Edit the config file to customize prompts and parameters
# Then use it:
./paper-assistant process 2312.07104 -c analysis-config.json
```

## Limitations

- Requires internet access for arXiv and API services
- Processing time varies based on paper length (typically 3-10 minutes)
- Mind map generation depends on external MarkMap service availability
- Chinese translation quality depends on source paper structure and language
- API rate limits may affect processing speed

## License

This project is part of the AgentFlow framework. See the main repository for license details.

## Contributing

Contributions are welcome! Please see the main AgentFlow repository for contribution guidelines.

## Changelog

### v0.1.0 (Initial Release)
- arXiv paper fetching and processing
- Chinese summarization and translation  
- Interactive mind map generation
- AI poster generation
- Flexible configuration system
- CLI interface with multiple processing modes