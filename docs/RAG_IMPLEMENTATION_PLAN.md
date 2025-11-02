# AgentFlow RAG System Implementation Plan

**Version**: v0.3.0-alpha
**Status**: Phase 3 - In Progress
**Start Date**: 2025-11-02
**Target Completion**: 3-6 months

---

## Executive Summary

This document outlines the comprehensive implementation plan for integrating Retrieval-Augmented Generation (RAG) capabilities into AgentFlow. The RAG system will enable workflows to leverage pre-indexed knowledge bases for semantic search and context-aware LLM operations.

### Goals
- ✅ Enable knowledge-augmented workflows
- ✅ Support multiple vector database backends
- ✅ Provide flexible document chunking and embedding strategies
- ✅ Integrate seamlessly with existing workflow system
- ✅ Deliver production-ready RAG capabilities

---

## Architecture Overview

### System Components

```
┌─────────────────────────────────────────────────────────────┐
│                     AgentFlow RAG System                     │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │  Embeddings  │───▶│ Vector Store │◀───│   Retrieval  │  │
│  │  Generation  │    │  Abstraction │    │   Strategies │  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
│         │                    │                     │         │
│         ▼                    ▼                     ▼         │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │   Chunking   │    │   Indexing   │    │  Re-ranking  │  │
│  │  Strategies  │    │   Pipeline   │    │   & Filter   │  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                    Integration Layer                         │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│   ┌──────────┐      ┌──────────┐      ┌──────────┐         │
│   │ RAGNode  │      │ RAG CLI  │      │ LLM+RAG  │         │
│   │Workflow  │      │ Commands │      │Integration│         │
│   └──────────┘      └──────────┘      └──────────┘         │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Foundation (Weeks 1-2)

### 1.1 Create `agentflow-rag` Crate

**Tasks**:
- [ ] Initialize crate structure with Cargo.toml
- [ ] Define module structure
- [ ] Set up error types and result types
- [ ] Create basic traits and abstractions

**Deliverables**:
```
agentflow-rag/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── types.rs
│   ├── embeddings/
│   ├── vectorstore/
│   ├── retrieval/
│   ├── indexing/
│   ├── chunking/
│   ├── reranking/
│   └── sources/
├── tests/
└── examples/
```

### 1.2 Define Core Abstractions

**VectorStore Trait**:
```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn create_collection(&self, name: &str, config: CollectionConfig) -> Result<()>;
    async fn delete_collection(&self, name: &str) -> Result<()>;

    async fn add_documents(&self, collection: &str, docs: Vec<Document>) -> Result<Vec<String>>;
    async fn delete_documents(&self, collection: &str, ids: Vec<String>) -> Result<()>;

    async fn similarity_search(
        &self,
        collection: &str,
        query: &str,
        top_k: usize,
        filter: Option<Filter>
    ) -> Result<Vec<SearchResult>>;

    async fn hybrid_search(
        &self,
        collection: &str,
        query: &str,
        top_k: usize,
        alpha: f32  // Balance between semantic and keyword search
    ) -> Result<Vec<SearchResult>>;
}
```

**Embedding Trait**:
```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

**Chunking Strategy Trait**:
```rust
pub trait ChunkingStrategy: Send + Sync {
    fn chunk(&self, text: &str) -> Result<Vec<TextChunk>>;
    fn chunk_size(&self) -> usize;
    fn overlap(&self) -> usize;
}
```

---

## Phase 2: Vector Store Integration (Weeks 3-4)

### 2.1 Choose Initial Vector Database

**Decision Matrix**:

| Database | Pros | Cons | Priority |
|----------|------|------|----------|
| **Qdrant** | Rust-native, fast, local-first | Newer ecosystem | ⭐⭐⭐ HIGH |
| **Chroma** | Simple, Python-native | Requires Python runtime | ⭐⭐ MEDIUM |
| **Milvus** | Feature-rich, scalable | Complex setup | ⭐ LOW |
| **Weaviate** | GraphQL API, modular | Heavier resource usage | ⭐ LOW |

**Recommendation**: Start with **Qdrant** (local-first, Rust client available)

### 2.2 Implement Qdrant Integration

**Tasks**:
- [ ] Add qdrant-client dependency
- [ ] Implement QdrantStore struct
- [ ] Implement VectorStore trait for QdrantStore
- [ ] Write integration tests
- [ ] Add connection pooling
- [ ] Add retry and timeout logic

**Example Usage**:
```rust
use agentflow_rag::vectorstore::{VectorStore, QdrantStore};

let store = QdrantStore::new("http://localhost:6334").await?;
store.create_collection("docs", CollectionConfig {
    dimension: 384,
    distance: Distance::Cosine,
}).await?;
```

### 2.3 Implement Local Fallback (Optional)

For development without external dependencies:
- [ ] Implement SimpleVectorStore (in-memory)
- [ ] Use HNSW or simple brute-force search
- [ ] Serialize/deserialize to disk

---

## Phase 3: Embeddings Integration (Weeks 5-6)

### 3.1 Implement Embedding Providers

**Priority Order**:
1. **OpenAI Embeddings** (most common)
2. **Local Embeddings** (sentence-transformers via ONNX)
3. **Anthropic/Google** (if APIs available)

### 3.2 OpenAI Embeddings

**Tasks**:
- [ ] Create OpenAIEmbedding struct
- [ ] Implement EmbeddingProvider trait
- [ ] Support text-embedding-3-small and text-embedding-3-large
- [ ] Add batch processing with rate limiting
- [ ] Implement caching layer

**Example**:
```rust
use agentflow_rag::embeddings::{EmbeddingProvider, OpenAIEmbedding};

let embedder = OpenAIEmbedding::new("text-embedding-3-small")?;
let vector = embedder.embed_text("Hello, world!").await?;
println!("Dimension: {}", embedder.dimension()); // 1536
```

### 3.3 Local Embeddings (ONNX Runtime)

**Tasks**:
- [ ] Add onnxruntime dependency
- [ ] Integrate sentence-transformers models
- [ ] Implement LocalEmbedding struct
- [ ] Add model download/caching
- [ ] Optimize for CPU/GPU

**Benefits**:
- No API costs
- No rate limits
- Privacy-preserving
- Offline capability

---

## Phase 4: Document Processing (Weeks 7-8)

### 4.1 Implement Chunking Strategies

**Strategies to Implement**:

1. **Fixed-Size Chunking**:
   - Simple, predictable
   - Configurable size and overlap

2. **Sentence-Based Chunking**:
   - Preserves semantic boundaries
   - Uses sentence tokenization

3. **Recursive Character Chunking**:
   - Hierarchical splitting
   - Respects document structure

4. **Semantic Chunking** (advanced):
   - Uses embeddings to find natural breaks
   - More expensive but higher quality

### 4.2 Implement Document Loaders

**Source Types**:
- [ ] Plain text files (.txt, .md)
- [ ] PDF documents
- [ ] HTML/Web pages
- [ ] JSON/JSONL
- [ ] CSV/TSV
- [ ] Code files (with syntax awareness)

### 4.3 Indexing Pipeline

**Pipeline Components**:
```rust
pub struct IndexingPipeline {
    chunker: Box<dyn ChunkingStrategy>,
    embedder: Box<dyn EmbeddingProvider>,
    store: Box<dyn VectorStore>,
    metadata_enricher: Option<Box<dyn MetadataEnricher>>,
}

impl IndexingPipeline {
    pub async fn index_document(&self, doc: Document) -> Result<IndexResult> {
        // 1. Load document
        // 2. Chunk document
        // 3. Generate embeddings
        // 4. Enrich metadata
        // 5. Store in vector database
        // 6. Return statistics
    }
}
```

---

## Phase 5: Retrieval & Re-ranking (Weeks 9-10)

### 5.1 Implement Retrieval Strategies

**Basic Retrieval**:
- [ ] Similarity search (cosine, euclidean, dot product)
- [ ] Filtered search (metadata-based)
- [ ] Hybrid search (semantic + keyword)

**Advanced Retrieval**:
- [ ] Multi-query retrieval
- [ ] Parent-child retrieval
- [ ] Self-query retrieval

### 5.2 Implement Re-ranking

**Re-ranking Methods**:
1. **Cross-encoder re-ranking**:
   - More accurate but slower
   - Uses sentence-transformers cross-encoder

2. **MMR (Maximal Marginal Relevance)**:
   - Balances relevance and diversity
   - Reduces redundancy

3. **Metadata-based filtering**:
   - Post-retrieval filtering
   - Based on document properties

---

## Phase 6: Workflow Integration (Weeks 11-12)

### 6.1 Create RAGNode

**Implementation**:
```rust
// agentflow-nodes/src/nodes/rag.rs
#[derive(Debug, Clone)]
pub struct RAGNode {
    pub vectorstore_uri: String,
    pub collection: String,
    pub query: String,
    pub top_k: usize,
    pub similarity_threshold: Option<f32>,
    pub rerank: bool,
    pub embedding_provider: String,
}

#[async_trait]
impl AsyncNode for RAGNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        // 1. Resolve query from inputs
        // 2. Connect to vector store
        // 3. Generate query embedding
        // 4. Perform similarity search
        // 5. Apply filters and re-ranking
        // 6. Return results
    }
}
```

### 6.2 RAGNode Factory

**Tasks**:
- [ ] Implement RAGNodeFactory
- [ ] Add to factory registry
- [ ] Define input/output schema
- [ ] Add configuration validation

### 6.3 Workflow Examples

**Example 1: Simple RAG Query**:
```yaml
name: "Simple RAG Example"
nodes:
  - id: search_docs
    type: rag
    parameters:
      vectorstore_uri: "qdrant://localhost:6334"
      collection: "documentation"
      query: "{{ user_question }}"
      top_k: 5

  - id: answer_question
    type: llm
    dependencies: ["search_docs"]
    parameters:
      model: "gpt-4"
      prompt: |
        Answer the question based on the following context:

        {{ nodes.search_docs.outputs.results }}

        Question: {{ user_question }}
```

**Example 2: Multi-Source RAG**:
```yaml
name: "Multi-Source RAG"
nodes:
  - id: search_docs
    type: rag
    parameters:
      vectorstore_uri: "qdrant://localhost:6334"
      collection: "docs"
      query: "{{ query }}"

  - id: search_code
    type: rag
    parameters:
      vectorstore_uri: "qdrant://localhost:6334"
      collection: "code"
      query: "{{ query }}"

  - id: combine_results
    type: llm
    dependencies: ["search_docs", "search_code"]
    parameters:
      model: "gpt-4"
      prompt: "Combine documentation and code examples..."
```

---

## Phase 7: CLI Commands (Weeks 13-14)

### 7.1 RAG CLI Commands

**Commands to Implement**:

1. **`agentflow rag index`** - Index documents
   ```bash
   agentflow rag index \
     --collection docs \
     --source ./documentation \
     --vectorstore qdrant://localhost:6334 \
     --embedding openai/text-embedding-3-small
   ```

2. **`agentflow rag search`** - Search vector store
   ```bash
   agentflow rag search \
     --collection docs \
     --query "How do I configure workflows?" \
     --top-k 5 \
     --vectorstore qdrant://localhost:6334
   ```

3. **`agentflow rag list-collections`** - List collections
   ```bash
   agentflow rag list-collections \
     --vectorstore qdrant://localhost:6334
   ```

4. **`agentflow rag delete`** - Delete collection
   ```bash
   agentflow rag delete \
     --collection old_docs \
     --vectorstore qdrant://localhost:6334
   ```

### 7.2 CLI Module Structure

```
agentflow-cli/src/commands/rag/
├── mod.rs
├── index.rs
├── search.rs
├── list.rs
└── delete.rs
```

---

## Phase 8: Testing & Documentation (Weeks 15-16)

### 8.1 Testing Strategy

**Unit Tests**:
- [ ] Chunking strategy tests
- [ ] Embedding provider tests
- [ ] Vector store operations tests

**Integration Tests**:
- [ ] End-to-end indexing pipeline
- [ ] RAGNode workflow execution
- [ ] CLI command tests

**Property-Based Tests** (with proptest):
- [ ] Chunking consistency
- [ ] Embedding stability
- [ ] Search result ordering

### 8.2 Documentation

**Documents to Create**:
- [ ] `RAG_GUIDE.md` - Comprehensive user guide
- [ ] `RAG_API.md` - API reference
- [ ] `RAG_ARCHITECTURE.md` - System design
- [ ] Update `COMMANDS_REFERENCE.md`
- [ ] Add RAG examples to examples/

**Example Documentation Structure**:
```
agentflow-cli/examples/documentation/
├── RAG_GUIDE.md
├── RAG_QUICKSTART.md
├── RAG_BEST_PRACTICES.md
└── RAG_TROUBLESHOOTING.md

agentflow-rag/examples/
├── basic_indexing.rs
├── custom_chunking.rs
├── hybrid_search.rs
└── local_embeddings.rs
```

---

## Phase 9: Optimization & Polish (Weeks 17-18)

### 9.1 Performance Optimization

**Tasks**:
- [ ] Benchmark embedding generation
- [ ] Optimize batch processing
- [ ] Implement connection pooling
- [ ] Add caching layers
- [ ] Profile memory usage

### 9.2 Production Readiness

**Checklist**:
- [ ] Error handling review
- [ ] Logging and observability
- [ ] Configuration validation
- [ ] Resource cleanup (Drop traits)
- [ ] Thread safety verification
- [ ] Documentation completeness

---

## Dependencies

### Required Crates

```toml
[dependencies]
# Vector databases
qdrant-client = "1.7"

# Embeddings
reqwest = { version = "0.11", features = ["json"] }
tokenizers = "0.15"  # For text processing

# Document processing
pdf-extract = "0.7"
scraper = "0.18"  # HTML parsing
csv = "1.3"

# Machine learning (optional local embeddings)
onnxruntime = { version = "0.0.14", optional = true }
ndarray = "0.15"

# Utilities
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.35", features = ["full"] }
async-trait = "0.1"
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"

# Testing
proptest = "1.4"
tempfile = "3.8"
```

---

## Risk Analysis

### Technical Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Qdrant API changes | HIGH | Pin version, monitor releases |
| Embedding API rate limits | MEDIUM | Implement caching, batch processing |
| Large document processing | MEDIUM | Streaming, chunking optimization |
| Memory usage with large indexes | MEDIUM | Pagination, lazy loading |
| Embedding quality issues | LOW | Support multiple providers |

### Timeline Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Complexity underestimation | MEDIUM | Phased approach, MVP focus |
| Dependency issues | LOW | Use stable, well-maintained crates |
| Testing overhead | MEDIUM | Automated CI/CD, property tests |

---

## Success Metrics

### Functional Metrics
- ✅ Support at least 2 vector databases (Qdrant + fallback)
- ✅ Support 2+ embedding providers (OpenAI + local)
- ✅ 3+ chunking strategies available
- ✅ RAGNode integrated in workflow system
- ✅ 4+ CLI commands implemented

### Quality Metrics
- ✅ 80%+ test coverage
- ✅ All examples working
- ✅ Documentation complete
- ✅ Zero compiler warnings
- ✅ Performance benchmarks documented

### User Experience Metrics
- ✅ Simple indexing in < 5 commands
- ✅ Search latency < 500ms (for typical queries)
- ✅ Clear error messages
- ✅ Comprehensive examples

---

## Milestone Schedule

### Month 1
- **Week 1-2**: Foundation & abstractions
- **Week 3-4**: Qdrant integration
- **Deliverable**: Basic vector store working

### Month 2
- **Week 5-6**: Embeddings integration
- **Week 7-8**: Document processing
- **Deliverable**: Indexing pipeline working

### Month 3
- **Week 9-10**: Retrieval & re-ranking
- **Week 11-12**: Workflow integration
- **Deliverable**: RAGNode in workflows

### Month 4 (if needed)
- **Week 13-14**: CLI commands
- **Week 15-16**: Testing & documentation
- **Week 17-18**: Optimization & polish
- **Deliverable**: v0.3.0 release

---

## Decision Points

### Week 4: Vector Database Choice
- **Question**: Stick with Qdrant or add alternatives?
- **Criteria**: Performance, ease of use, community adoption

### Week 8: Embedding Provider Priority
- **Question**: Focus on API or local embeddings?
- **Criteria**: User needs, cost considerations

### Week 12: Feature Scope
- **Question**: Include advanced features (hybrid search, re-ranking)?
- **Criteria**: Timeline, complexity, user demand

---

## Next Steps

### Immediate Actions (This Week)
1. ✅ Create agentflow-rag crate skeleton
2. ✅ Define core traits and types
3. ✅ Set up initial testing infrastructure
4. ✅ Create basic examples

### Week 2 Actions
1. Research Qdrant client usage
2. Implement basic VectorStore trait
3. Create mock implementations for testing
4. Begin Qdrant integration

---

## References

- **Qdrant Documentation**: https://qdrant.tech/documentation/
- **LangChain RAG Patterns**: https://python.langchain.com/docs/use_cases/question_answering/
- **Vector Database Comparison**: https://benchmark.vectorview.ai/
- **Embedding Models Leaderboard**: https://huggingface.co/spaces/mteb/leaderboard

---

**Last Updated**: 2025-11-02
**Version**: 1.0
**Status**: Initial Draft
**Next Review**: After Week 4
