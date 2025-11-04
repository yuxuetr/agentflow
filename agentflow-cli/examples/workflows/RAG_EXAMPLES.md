# AgentFlow RAG Workflow Examples

This directory contains workflow examples demonstrating AgentFlow's RAG (Retrieval-Augmented Generation) capabilities powered by the `agentflow-rag` crate.

## Prerequisites

### 1. Install Qdrant Vector Database

```bash
# Using Docker (recommended)
docker run -p 6333:6333 -p 6334:6334 qdrant/qdrant

# Or using Docker Compose
docker-compose up -d qdrant
```

### 2. Set OpenAI API Key

RAG operations use OpenAI embeddings by default:

```bash
export OPENAI_API_KEY=sk-...
```

### 3. Build AgentFlow with RAG Feature

```bash
cargo build --release --features rag
```

## Available Examples

### 1. Simple RAG Search (`rag_simple_search.yml`)

**Purpose**: Minimal example demonstrating semantic search

**Use Case**: Quick testing of RAG search functionality

**Run**:
```bash
agentflow workflow run examples/workflows/rag_simple_search.yml --features rag
```

**Features Demonstrated**:
- Semantic search with OpenAI embeddings
- Basic query and top-k retrieval

### 2. RAG Knowledge Assistant (`rag_knowledge_assistant.yml`)

**Purpose**: Comprehensive RAG workflow demonstrating all operations

**Use Case**: Building a knowledge base Q&A system

**Run**:
```bash
agentflow workflow run examples/workflows/rag_knowledge_assistant.yml --features rag
```

**Features Demonstrated**:
- Collection creation and management
- Document indexing with metadata
- Semantic search
- Hybrid search (semantic + keyword with BM25)
- LLM integration for answer generation
- Collection statistics

**Workflow Steps**:
1. Create a new collection `rust_knowledge`
2. Index 5 documents about Rust programming
3. Perform semantic search: "How does Rust ensure memory safety?"
4. Perform hybrid search: "async programming tokio"
5. Generate answer using LLM with search context
6. Get collection statistics

## RAG Node Configuration

### Common Parameters

All RAG nodes require:

```yaml
- id: my_rag_node
  type: rag
  parameters:
    operation: <operation_type>      # Required: search, index, create_collection, delete_collection, stats
    qdrant_url: "http://localhost:6334"  # Optional: default localhost:6334
    collection: "my_collection"      # Required: collection name
    embedding_model: "text-embedding-3-small"  # Optional: OpenAI embedding model
```

### Operation-Specific Parameters

#### Search Operation

```yaml
operation: search
query: "your search query"
top_k: 5                    # Number of results to return
search_type: semantic       # semantic, hybrid, or keyword
alpha: 0.7                  # For hybrid: 0.0=keyword, 1.0=semantic (default: 0.5)
rerank: true                # Enable MMR re-ranking for diversity (default: false)
lambda: 0.5                 # For MMR: 0.0=diversity, 1.0=relevance (default: 0.5)
```

**Search Types**:
- `semantic`: Vector similarity search using embeddings (default)
- `hybrid`: Combines semantic + keyword (BM25) with RRF fusion
- `keyword`: Pure keyword search using BM25 algorithm

#### Index Operation

```yaml
operation: index
documents:
  - content: "Document text here"
    metadata:
      source: "user_input"
      category: "tech"
  - content: "Another document"
```

#### Create Collection Operation

```yaml
operation: create_collection
# No additional parameters needed
# Uses collection name and embedding model from common parameters
```

#### Delete Collection Operation

```yaml
operation: delete_collection
# No additional parameters needed
```

#### Stats Operation

```yaml
operation: stats
# Returns collection statistics (document count, index status, etc.)
```

## Search Strategy Comparison

### Semantic Search
- **Best for**: Conceptual queries, finding similar meaning
- **Example**: "How to handle errors?" → finds docs about Result types, unwrap, etc.

### Hybrid Search (α=0.7)
- **Best for**: Balanced relevance, combining meaning and keywords
- **Example**: "rust async tokio" → prioritizes semantic similarity but boosts keyword matches

### Keyword Search (BM25)
- **Best for**: Exact term matching, code identifiers
- **Example**: "tokio::spawn" → finds exact function references

## Advanced Features

### MMR Re-ranking for Diversity

Enable MMR to get diverse results instead of similar ones:

```yaml
operation: search
search_type: semantic
rerank: true
lambda: 0.3  # More diversity (0.0=max diversity, 1.0=pure relevance)
```

### Hybrid Search with Custom Alpha

Control semantic vs keyword balance:

```yaml
operation: search
search_type: hybrid
alpha: 0.8   # 80% semantic, 20% keyword
```

### Metadata Filtering (Coming Soon)

Filter search results by metadata:

```yaml
operation: search
query: "async programming"
filter:
  category: "concurrency"
  difficulty: "beginner"
```

## Integrating RAG with LLM Nodes

Use RAG search results as context for LLM answer generation:

```yaml
nodes:
  - id: search_kb
    type: rag
    parameters:
      operation: search
      collection: "knowledge_base"
      query: "{{user_question}}"
      top_k: 5

  - id: generate_answer
    type: llm
    dependencies: [search_kb]
    input_mapping:
      context: "{{ nodes.search_kb.outputs.results }}"
    parameters:
      model: "gpt-4o-mini"
      prompt: |
        Answer based on this context:
        {{ context }}

        Question: {{user_question}}
```

## Performance Tips

1. **Choose Right Search Type**:
   - Semantic: General queries, conceptual search
   - Hybrid: Best balance for most use cases
   - Keyword: When you know exact terms

2. **Tune top_k**:
   - Start with `top_k: 5` for most use cases
   - Increase for broader context, decrease for precision

3. **Embedding Model Selection**:
   - `text-embedding-3-small`: Fast, cost-effective (default)
   - `text-embedding-3-large`: Higher quality, slower, more expensive

4. **Use MMR for Diversity**:
   - Enable when you want varied results
   - Disable for finding most similar documents

## Troubleshooting

### Qdrant Connection Errors

```bash
# Check Qdrant is running
docker ps | grep qdrant

# Check Qdrant health
curl http://localhost:6333/health
```

### Empty Search Results

- Ensure collection exists and has documents
- Check query is meaningful (not too short)
- Verify embedding model matches indexed documents

### Performance Issues

- Index documents in batches (100-1000 at a time)
- Use appropriate top_k (don't retrieve more than needed)
- Consider caching frequently used queries

## Next Steps

1. **Phase 6 Complete**: RAGNode workflow integration ✅
2. **Phase 7 Planned**: Advanced chunking strategies, more document loaders
3. **Phase 8 Planned**: Caching layer, performance optimizations
4. **Future**: Multi-modal RAG, GraphRAG

## References

- [AgentFlow RAG Documentation](../../agentflow-rag/README.md)
- [Qdrant Documentation](https://qdrant.tech/documentation/)
- [OpenAI Embeddings](https://platform.openai.com/docs/guides/embeddings)
