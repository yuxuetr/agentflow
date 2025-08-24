# AgentFlow Agents

A collection of reusable AI agent applications built with the AgentFlow framework. This crate provides both a library of shared utilities for building agents and a set of standalone agent applications.

## 🏗️ Architecture

### Core Components

- **`agentflow-agents`** (Library): Shared utilities, traits, and common components
- **`agents/`** (Applications): Individual agent applications as standalone binaries

### Agent Applications

| Agent | Description | Status |
|-------|-------------|--------|
| [Paper Research Analyzer](agents/paper_research_analyzer/) | PDF research paper analysis with summarization, insights, and translation | ✅ Complete |
| Document Processor | General document processing and analysis | 🚧 Planned |
| Web Scraper | Intelligent web scraping and content extraction | 🚧 Planned |
| Data Analyzer | Structured data analysis and reporting | 🚧 Planned |

## 🚀 Quick Start

### Building Agents

```bash
# Build all agents
cargo build --release

# Build specific agent
cargo build --release --package paper-research-analyzer

# Install agent globally
cargo install --path agents/paper_research_analyzer
```

### Running Agents

```bash
# Run paper research analyzer
paper-research-analyzer analyze --pdf ./research_paper.pdf

# Or run directly with cargo
cargo run --bin paper-research-analyzer -- analyze --pdf ./research_paper.pdf
```

## 🛠️ Shared Library Features

The `agentflow-agents` crate provides common utilities for building agent applications:

### Agent Traits
- **`AgentApplication`**: Core interface for all agents
- **`FileAgent`**: Specialized interface for file-processing agents  
- **`BatchAgent`**: Interface for batch processing capabilities
- **`AgentConfig`**: Configuration management trait

### Common Utilities
- **PDF Processing**: StepFun API integration for document parsing
- **Batch Processing**: Concurrent processing with progress reporting
- **File Handling**: Utilities for file discovery and management
- **Output Formatting**: Structured output in multiple formats (JSON, Markdown, etc.)

### Example Usage

```rust
use agentflow_agents::{AgentApplication, FileAgent, AgentResult};

#[derive(Default)]
struct MyAgent {
    config: MyConfig,
}

#[async_trait]
impl AgentApplication for MyAgent {
    type Config = MyConfig;
    type Result = MyResult;

    async fn initialize(config: Self::Config) -> AgentResult<Self> {
        Ok(Self { config })
    }

    async fn execute(&self, input: &str) -> AgentResult<Self::Result> {
        // Agent implementation
        todo!()
    }

    fn name(&self) -> &'static str {
        "my-agent"
    }
}
```

## 📁 Project Structure

```
agentflow-agents/
├── src/                              # Shared library code
│   ├── lib.rs                       # Main library exports
│   ├── traits/                      # Agent traits and interfaces
│   │   ├── agent.rs                # Core agent traits
│   │   └── mod.rs
│   └── common/                      # Shared utilities
│       ├── pdf_parser.rs           # PDF processing utilities
│       ├── file_utils.rs           # File handling utilities
│       ├── output_formatter.rs     # Output formatting
│       ├── batch_processor.rs      # Batch processing
│       └── mod.rs
├── agents/                          # Individual agent applications
│   └── paper_research_analyzer/     # PDF research analysis agent
│       ├── src/
│       │   ├── main.rs             # CLI entry point
│       │   ├── lib.rs              # Library exports
│       │   ├── analyzer.rs         # Core implementation
│       │   ├── config.rs           # Configuration
│       │   └── nodes/              # Workflow nodes
│       ├── workflows/              # YAML configurations
│       └── README.md               # Agent documentation
├── examples/                        # Usage examples
└── README.md                       # This file
```

## 🧩 Building New Agents

### 1. Create Agent Directory

```bash
mkdir -p agents/my_agent/src/nodes
```

### 2. Create Cargo.toml

```toml
[package]
name = "my-agent"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "my-agent"
path = "src/main.rs"

[dependencies]
agentflow-core = { path = "../../../agentflow-core" }
agentflow-llm = { path = "../../../agentflow-llm" }
agentflow-agents = { path = "../.." }
# Add agent-specific dependencies
```

### 3. Implement Agent Traits

```rust
use agentflow_agents::{AgentApplication, AgentResult};

pub struct MyAgent {
    // Agent state
}

#[async_trait]
impl AgentApplication for MyAgent {
    type Config = MyConfig;
    type Result = MyResult;
    
    async fn initialize(config: Self::Config) -> AgentResult<Self> {
        // Initialize agent with configuration
    }
    
    async fn execute(&self, input: &str) -> AgentResult<Self::Result> {
        // Main agent logic
    }
    
    fn name(&self) -> &'static str {
        "my-agent"
    }
}
```

### 4. Create CLI Interface

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "my-agent")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Process {
        #[arg(short, long)]
        input: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Process { input } => {
            // Handle processing
        }
    }
    
    Ok(())
}
```

### 5. Update Workspace

Add your agent to the root `Cargo.toml`:

```toml
[workspace]
members = [
  # ... existing members
  "agentflow-agents/agents/my_agent"
]
```

## 🔧 Development

### Testing

```bash
# Test all agents
cargo test --package agentflow-agents --package paper-research-analyzer

# Test specific agent
cargo test --package my-agent
```

### Code Quality

```bash
# Format code
cargo fmt

# Run lints
cargo clippy

# Check compilation
cargo check --all
```

## 📦 Distribution

### Individual Binaries

Each agent can be distributed as a standalone binary:

```bash
# Install from source
cargo install --path agents/paper_research_analyzer

# Use installed binary
paper-research-analyzer --help
```

### Container Images

Agents can be packaged as container images:

```dockerfile
FROM rust:1.70 as builder
COPY . .
RUN cargo build --release --bin paper-research-analyzer

FROM debian:bookworm-slim
COPY --from=builder /target/release/paper-research-analyzer /usr/local/bin/
ENTRYPOINT ["paper-research-analyzer"]
```

## 🤝 Contributing

1. **New Agents**: Follow the structure outlined above
2. **Shared Utilities**: Add common functionality to `src/common/`
3. **Agent Traits**: Extend interfaces in `src/traits/`
4. **Documentation**: Update README files and add examples

### Guidelines

- Each agent should be self-contained and independently runnable
- Shared functionality should be extracted to the common library
- Follow the established naming conventions (`kebab-case` for binaries)
- Provide comprehensive documentation and examples

## 📝 License

MIT License - see the main AgentFlow project for details.