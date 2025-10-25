# AgentFlow Short-Term Improvements (2-4 Weeks)

**Status**: In Progress
**Start Date**: 2025-10-26
**Target Completion**: 2025-11-23

## Overview

This document tracks the implementation of Phase 1 short-term improvements focusing on:
1. Error handling enhancement
2. Workflow debugging tools
3. Resource management

## 1. Error Handling Enhancement

### 1.1 Retry Mechanism Architecture

**Goal**: Provide robust retry capabilities for transient failures

**Components**:
- `RetryPolicy` - Configuration for retry behavior
- `RetryStrategy` - Different retry strategies (exponential backoff, fixed delay, etc.)
- `RetryContext` - Track retry attempts and state
- Integration with AsyncNode execution

**Implementation Plan**:

```rust
// agentflow-core/src/retry.rs

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Retry strategy to use
    pub strategy: RetryStrategy,
    /// Which errors should trigger retries
    pub retryable_errors: Vec<ErrorPattern>,
    /// Maximum total retry duration
    pub max_duration: Option<Duration>,
}

/// Retry strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed { delay_ms: u64 },
    /// Exponential backoff with jitter
    ExponentialBackoff {
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
        jitter: bool,
    },
    /// Linear backoff
    Linear {
        initial_delay_ms: u64,
        increment_ms: u64,
    },
}

/// Pattern matching for retryable errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorPattern {
    /// Match by error type
    ErrorType(String),
    /// Match by error message substring
    MessageContains(String),
    /// Network-related errors
    NetworkError,
    /// Timeout errors
    TimeoutError,
    /// Rate limit errors
    RateLimitError,
}
```

**YAML Configuration Example**:
```yaml
nodes:
  - name: api_call
    type: http
    url: "https://api.example.com/data"
    retry:
      max_attempts: 3
      strategy:
        type: exponential_backoff
        initial_delay_ms: 100
        max_delay_ms: 5000
        multiplier: 2.0
        jitter: true
      retryable_errors:
        - NetworkError
        - TimeoutError
        - MessageContains: "503"
      max_duration: 30s
```

### 1.2 Error Context Enhancement

**Goal**: Provide detailed error context for debugging

**Features**:
- Error chain tracking
- Node execution context
- Input/output snapshots
- Timestamp tracking
- Stack trace capture

**Implementation**:
```rust
// agentflow-core/src/error_context.rs

#[derive(Debug, Clone, Serialize)]
pub struct ErrorContext {
    /// Node that failed
    pub node_name: String,
    /// Workflow run ID
    pub run_id: String,
    /// Error timestamp
    pub timestamp: SystemTime,
    /// Error chain (root cause to current)
    pub error_chain: Vec<ErrorInfo>,
    /// Node inputs at failure time
    pub inputs: Option<HashMap<String, FlowValue>>,
    /// Execution duration before failure
    pub duration: Duration,
    /// Previous successful nodes
    pub execution_history: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorInfo {
    pub error_type: String,
    pub message: String,
    pub source: Option<String>,
}
```

### 1.3 Failure Recovery

**Goal**: Allow workflows to resume from failure points

**Features**:
- Checkpoint creation
- State persistence
- Resume from checkpoint
- Idempotent node execution

## 2. Workflow Debugging Tools

### 2.1 Debug Command

**Goal**: Interactive workflow debugging and inspection

**CLI Interface**:
```bash
# Visualize workflow DAG
agentflow workflow debug <workflow.yml> --visualize

# Dry run with detailed output
agentflow workflow debug <workflow.yml> --dry-run --verbose

# Analyze workflow structure
agentflow workflow debug <workflow.yml> --analyze

# Validate workflow configuration
agentflow workflow debug <workflow.yml> --validate

# Show execution plan
agentflow workflow debug <workflow.yml> --plan
```

### 2.2 DAG Visualization

**Text-based DAG visualization**:
```
Workflow: data_pipeline
├─ [start] fetch_data
│  └─ [http] GET api.example.com
├─ [map:parallel] process_items (parallelism: 4)
│  ├─ transform_item
│  ├─ validate_item
│  └─ enrich_item
├─ [llm] summarize
│  └─ model: gpt-4o, temp: 0.7
└─ [end] save_results
   └─ [file] output: results.json

Dependencies:
  process_items → fetch_data
  summarize → process_items
  save_results → summarize

Estimated execution: ~15-30s
```

### 2.3 Execution Profiling

**Node timing analysis**:
```rust
// agentflow-core/src/profiling.rs

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionProfile {
    pub workflow_name: String,
    pub total_duration: Duration,
    pub node_profiles: Vec<NodeProfile>,
    pub bottlenecks: Vec<Bottleneck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeProfile {
    pub node_name: String,
    pub node_type: String,
    pub duration: Duration,
    pub execution_count: u32,
    pub average_duration: Duration,
    pub input_size: Option<usize>,
    pub output_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bottleneck {
    pub node_name: String,
    pub percentage_of_total: f64,
    pub suggestion: String,
}
```

**Output Example**:
```
Execution Profile: data_pipeline
Total Duration: 45.2s

Node Performance:
┌─────────────────┬──────────┬───────┬─────────────┐
│ Node            │ Duration │ Count │ % of Total  │
├─────────────────┼──────────┼───────┼─────────────┤
│ fetch_data      │ 2.3s     │ 1     │ 5.1%        │
│ process_items   │ 38.1s    │ 100   │ 84.3% ⚠️    │
│ summarize       │ 3.9s     │ 1     │ 8.6%        │
│ save_results    │ 0.9s     │ 1     │ 2.0%        │
└─────────────────┴──────────┴───────┴─────────────┘

⚠️  Bottlenecks Detected:
  • process_items: 84.3% of total time
    → Consider increasing parallelism (current: 4)
    → Average per-item: 381ms
```

## 3. Resource Management

### 3.1 Memory Limits

**Goal**: Prevent unbounded memory growth

**Implementation**:
```rust
// agentflow-core/src/resource_limits.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum workflow state size in bytes
    pub max_state_size: usize,
    /// Maximum size for individual values
    pub max_value_size: usize,
    /// Maximum number of cached items
    pub max_cache_entries: usize,
    /// Memory cleanup threshold (percentage)
    pub cleanup_threshold: f64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_state_size: 100 * 1024 * 1024, // 100 MB
            max_value_size: 10 * 1024 * 1024,  // 10 MB
            max_cache_entries: 1000,
            cleanup_threshold: 0.8, // 80%
        }
    }
}
```

**YAML Configuration**:
```yaml
workflow:
  name: large_data_pipeline
  resource_limits:
    max_state_size: 100MB
    max_value_size: 10MB
    max_cache_entries: 1000
    cleanup_threshold: 0.8
  nodes:
    # ...
```

### 3.2 State Monitoring

**Goal**: Real-time resource usage tracking

**Features**:
- Current state size tracking
- Memory usage alerts
- Automatic cleanup triggers
- Resource usage metrics

**Implementation**:
```rust
// agentflow-core/src/state_monitor.rs

pub struct StateMonitor {
    limits: ResourceLimits,
    current_size: AtomicUsize,
    value_count: AtomicUsize,
    alerts: Arc<Mutex<Vec<ResourceAlert>>>,
}

#[derive(Debug, Clone)]
pub enum ResourceAlert {
    ApproachingLimit { percentage: f64, current: usize, limit: usize },
    LimitExceeded { current: usize, limit: usize },
    CleanupTriggered { freed: usize },
}
```

### 3.3 Value Streaming

**Goal**: Handle large data without loading into memory

**Approach**:
- Use `FlowValue::File` for large data
- Stream processing for transformations
- Lazy evaluation where possible

**Example**:
```yaml
nodes:
  - name: process_large_file
    type: transform
    input: "{{ large_dataset }}"
    streaming: true  # Process in chunks
    chunk_size: 1MB
```

## Implementation Schedule

### Week 1: Error Handling
- [x] Design retry architecture
- [ ] Implement RetryPolicy and strategies
- [ ] Add retry support to flow execution
- [ ] Add error context enhancement
- [ ] Write tests

### Week 2: Debugging Tools
- [ ] Create debug command structure
- [ ] Implement DAG visualization
- [ ] Add execution profiling
- [ ] Add workflow validation enhancements
- [ ] Write tests

### Week 3: Resource Management
- [ ] Implement resource limits
- [ ] Add state monitoring
- [ ] Implement cleanup strategies
- [ ] Add streaming support improvements
- [ ] Write tests

### Week 4: Integration & Documentation
- [ ] Integration testing
- [ ] Performance benchmarking
- [ ] Update documentation
- [ ] Create migration guide
- [ ] Prepare release notes

## Success Criteria

- ✅ All tests passing
- ✅ Zero compilation warnings
- ✅ Documentation complete
- ✅ Performance benchmarks meet targets:
  - Retry overhead < 5ms
  - Memory limit enforcement < 100μs
  - Debug visualization < 1s for 100-node workflows
- ✅ User testing positive feedback

## Related Issues

- Phase 1 Stabilization (#tracking)
- Improved Error Messages (#feature-request)
- Workflow Debugging (#enhancement)

---

**Last Updated**: 2025-10-26
**Next Review**: 2025-11-02
