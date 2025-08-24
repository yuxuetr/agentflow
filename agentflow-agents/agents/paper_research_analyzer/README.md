# Paper Research Analyzer Agent

A standalone PDF research paper analysis agent built with AgentFlow. This agent provides comprehensive analysis capabilities including summarization, key insights extraction, mind map generation, and multi-language translation.

## 🚀 Features

- **PDF Processing**: Upload and extract text from PDF research papers using StepFun Document Parser API
- **Intelligent Summarization**: Generate comprehensive research paper summaries with structured sections
- **Key Insights Extraction**: Extract metadata, methodology, findings, and contributions in JSON format  
- **Mind Map Generation**: Create Mermaid mind map visualizations of research concepts
- **Multi-language Translation**: Translate summaries to various target languages
- **Batch Processing**: Process multiple PDFs concurrently with progress reporting
- **Structured Output**: Save results in multiple formats (Markdown, JSON, Mermaid)

## 📋 Prerequisites

- **StepFun API Key**: Required for PDF processing and LLM operations
- **Rust Environment**: Latest stable Rust and Cargo
- **PDF Files**: Research papers in PDF format (max 64MB per file)

## 🔧 Installation

### From Source

```bash
# Clone the AgentFlow repository
git clone <repository-url>
cd agentflow

# Build the agent
cargo build --release --bin paper-research-analyzer

# Install globally (optional)
cargo install --path agentflow-agents/agents/paper_research_analyzer
```

## 🎯 Usage

### Environment Setup

```bash
export STEP_API_KEY="your-stepfun-api-key"
```

### Single Paper Analysis

```bash
# Basic analysis
paper-research-analyzer analyze --pdf ./research_paper.pdf

# Comprehensive analysis with translation
paper-research-analyzer analyze \
  --pdf ./research_paper.pdf \
  --depth comprehensive \
  --language zh \
  --model step-2-16k \
  --output ./analysis_results
```

### Batch Processing

```bash
# Analyze all PDFs in a directory
paper-research-analyzer batch \
  --directory ./research_papers/ \
  --output ./batch_results \
  --depth summary \
  --model step-2-mini \
  --concurrency 3
```

## ⚙️ Configuration Options

### Analysis Depth
- `summary`: Generate summary only
- `insights`: Extract key insights and metadata  
- `comprehensive`: Full analysis (summary + insights + mind map)
- `translation`: Everything + translation to target language

### Supported Models
- `step-1-256k`: High capacity model (256k tokens)
- `step-2-16k`: Balanced model (16k tokens) - **Default**
- `step-2-mini`: Fast model for batch processing
- `step-3`: Advanced reasoning model
- `qwen-turbo-latest`: Ultra-high capacity (1M tokens)

### Target Languages
- `en`: English (default)
- `zh`: Chinese
- `es`: Spanish  
- `fr`: French
- `de`: German
- `ja`: Japanese
- `ko`: Korean

## 📊 Output Structure

### Single Analysis Output
```
analysis_output/
├── summary.md              # Structured research summary
├── key_insights.json       # Extracted metadata and insights
├── mind_map.mermaid        # Visual concept relationships  
├── summary_zh.md          # Translated summary (if requested)
└── complete_analysis.json  # Full analysis results
```

### Batch Analysis Output
```
batch_analysis_20240320_143022/
├── paper1/                 # Individual analysis results
│   ├── summary.md
│   ├── key_insights.json
│   └── ...
├── paper2/
│   └── ...
└── batch_analysis_report.json  # Processing summary
```

## 🏗️ Architecture

The agent is built using AgentFlow's workflow orchestration system with the following components:

### Workflow Nodes
1. **PDFParserNode**: Extracts text content from PDF files
2. **SummaryNode**: Generates comprehensive summaries
3. **InsightsNode**: Extracts structured metadata
4. **MindMapNode**: Creates visual mind maps  
5. **TranslationNode**: Multi-language translation
6. **ResultsCompilerNode**: Aggregates final results

### AgentFlow Integration
- **Core**: Workflow execution with `AsyncFlow` and `AsyncNode`
- **LLM**: Unified LLM provider interface with StepFun integration
- **Shared Utilities**: Common PDF parsing, batch processing, and output formatting

## 📈 Performance

### Single Paper Analysis
- **Small Papers** (<50 pages): ~30-60 seconds
- **Large Papers** (100+ pages): ~2-5 minutes  
- **Memory Usage**: ~100-500MB depending on content size

### Batch Processing  
- **Concurrency**: Configurable (default: 3 concurrent papers)
- **Throughput**: ~10-20 papers per minute (depending on size and complexity)
- **Error Handling**: Continue-on-error strategy with detailed reporting

## 🛠️ Development

### Project Structure
```
paper_research_analyzer/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library exports
│   ├── analyzer.rs       # Core analyzer implementation
│   ├── config.rs         # Configuration structures
│   └── nodes/            # Workflow node implementations
│       ├── pdf_parser.rs
│       ├── summarizer.rs
│       ├── insights_extractor.rs
│       ├── mind_mapper.rs
│       ├── translator.rs
│       └── results_compiler.rs
├── workflows/            # YAML workflow definitions (future)
└── examples/             # Usage examples
```

### Testing

```bash
# Run tests
cargo test --package paper-research-analyzer

# Check code
cargo check --bin paper-research-analyzer

# Build optimized binary
cargo build --release --bin paper-research-analyzer
```

## 🔍 Example Output

### Generated Summary (Markdown)
```markdown
# 研究论文摘要

## 标题和作者
"Attention Is All You Need" by Ashish Vaswani et al.

## 摘要总结  
本论文提出了Transformer架构，完全基于注意力机制进行序列到序列的建模...

## 研究问题
现有的循环神经网络和卷积神经网络在处理长序列时存在并行化困难的问题...

## 主要发现
1. Transformer在翻译任务上取得了最先进的性能
2. 训练速度显著提升，支持更好的并行化
3. 更有效地处理长距离依赖关系
```

### Key Insights (JSON)
```json
{
  "title": "Attention Is All You Need",
  "authors": ["Ashish Vaswani", "Noam Shazeer", "..."],
  "field_of_study": "Natural Language Processing",
  "research_type": "experimental",
  "key_contributions": [
    "Transformer architecture",
    "Self-attention mechanism"
  ],
  "impact_potential": "high",
  "reproducibility": "high"
}
```

## 🤝 Contributing

This agent serves as a reference implementation for building document analysis applications with AgentFlow. Contributions are welcome for:

- Additional output formats
- Support for more document types
- Enhanced analysis capabilities
- Performance optimizations

## 📝 License

MIT License - see the main AgentFlow project for details.