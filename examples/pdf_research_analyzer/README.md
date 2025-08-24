# PDF Research Paper Analyzer

A comprehensive AgentFlow application for analyzing PDF research papers using StepFun's document parsing capabilities and LLM processing.

## Features

- 📄 **PDF Upload & Parsing**: Native PDF text extraction via StepFun Document Parser API
- 📝 **Summary Generation**: Intelligent summarization of research papers
- 🔍 **Key Insights Extraction**: Identifies main findings, methodology, and contributions
- 🌍 **Translation Support**: Multi-language translation of papers and summaries
- 🧠 **Mind Map Generation**: Visual representation of paper concepts and relationships
- 🔄 **Batch Processing**: Process multiple research papers in parallel

## Architecture

The system uses AgentFlow's workflow orchestration with these key components:

### Core Workflows

1. **`pdf_upload_and_parse.yml`** - PDF document parsing using StepFun API
2. **`paper_summary.yml`** - Generate structured summaries of research papers  
3. **`key_insights_extraction.yml`** - Extract methodology, findings, and contributions
4. **`translation_workflow.yml`** - Multi-language translation capabilities
5. **`mind_map_generation.yml`** - Generate structured mind maps from paper content
6. **`batch_paper_analysis.yml`** - Process multiple papers concurrently

### StepFun Integration

- **Document Parser**: `/files` endpoint for PDF text extraction
- **LLM Models**: `step-3` (multimodal), `step-2-16k` (text), `step-1-256k` (long context)
- **Vision Models**: `step-1v-32k` for diagram/figure understanding (future enhancement)

## Prerequisites

- AgentFlow CLI installed and configured
- StepFun API key set as `STEP_API_KEY` environment variable
- PDF files to analyze (max 64MB per file)

## Quick Start

```bash
# Set up environment
export STEP_API_KEY="your-stepfun-api-key"

# Analyze a single research paper
agentflow workflow run examples/pdf_research_analyzer/workflows/paper_analysis_complete.yml \
  --input pdf_path="path/to/paper.pdf" \
  --input analysis_type="comprehensive" \
  --input target_language="en"

# Batch process multiple papers
agentflow workflow run examples/pdf_research_analyzer/workflows/batch_paper_analysis.yml \
  --input pdf_directory="path/to/papers/" \
  --input output_directory="./analysis_results/"
```

## Example Outputs

- `summary.md` - Structured paper summary
- `key_insights.json` - Extracted insights and metadata
- `mind_map.mermaid` - Visual mind map diagram
- `translation/` - Translated versions (if requested)
- `analysis_report.json` - Complete analysis metadata

## Workflows Overview

### Complete Paper Analysis
Single workflow that orchestrates all analysis steps for one research paper.

### Batch Processing
Parallel processing of multiple research papers with progress tracking.

### Individual Components
- Summary generation only
- Key insights extraction only  
- Translation only
- Mind map generation only

## Customization

Edit workflow YAML files to:
- Modify analysis depth and focus areas
- Change output formats and structures
- Add new analysis types (citations, methodology critique, etc.)
- Configure different LLM models and parameters

## Error Handling

- PDF parsing failures (unsupported formats, size limits)
- Rate limiting and API error recovery
- Partial analysis completion with detailed error reports