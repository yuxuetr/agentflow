# AgentFlow RAG - 后续开发任务清单

## 当前状态

✅ **已完成模块** (截至 2025-01-04):
- Phase 1: 基础架构 (类型系统、错误处理)
- Phase 2: Qdrant 向量存储集成
- Phase 3: OpenAI Embeddings 集成、高级过滤、成本跟踪
- Phase 4: 文档处理 (分块策略、文档加载器、索引管道)
- Phase 5: 高级检索策略 (BM25、混合搜索、MMR重排序) ✨ **新完成！**

**完成度**: ~85%
**测试**: 73 passed, 0 failed, 4 ignored
**代码行数**: ~8,500 lines

---

## Phase 5: 高级检索策略 ✅ **已完成！** (2025-01-04)

### 5.1 混合搜索 (Hybrid Search) ✅
- [x] 关键词搜索实现 (BM25算法)
- [x] 语义搜索 + 关键词搜索融合
- [x] 可配置的融合权重 (alpha参数)
- [x] RRF (Reciprocal Rank Fusion) 算法

**实际时间**: <1周
**文件**: `src/retrieval/bm25.rs`, `src/retrieval/hybrid.rs`
**测试**: 13 BM25 tests + 11 hybrid tests = 24 tests ✅

### 5.2 重排序 (Re-ranking) ✅
- [x] MMR (Maximal Marginal Relevance) 完整实现
  - Jaccard相似度计算
  - 迭代选择算法
  - Lambda参数控制相关性/多样性权衡
- [x] Score重排序 (升序/降序)
- [x] 自定义重排序策略接口 (ReRankingStrategy trait)
- [ ] Cross-encoder 重排序 (未来可选)
- [ ] LLM-based 重排序 (未来可选)

**实际时间**: <1周
**文件**: `src/reranking/mod.rs`
**测试**: 8 reranking tests ✅

### 5.3 查询扩展 (Query Expansion) - **可选，延后**
- [ ] 同义词扩展
- [ ] LLM 生成的查询变体
- [ ] 多查询融合策略

**状态**: 延后到未来版本
**理由**: Phase 5核心功能已完成，查询扩展可作为增强功能

---

## Phase 6: 本地嵌入模型 (优先级: 中)

### 6.1 ONNX Runtime 集成
- [ ] ONNX 模型加载器
- [ ] Sentence-transformers 模型支持
- [ ] 批处理和性能优化
- [ ] 模型缓存管理

**预计时间**: 2-3周
**文件**: `src/embeddings/onnx.rs`
**依赖**: onnxruntime, ndarray

### 6.2 其他 Embedding 提供商
- [ ] Cohere Embeddings
- [ ] HuggingFace Inference API
- [ ] Vertex AI Embeddings (Google)
- [ ] Azure OpenAI

**预计时间**: 1-2周
**文件**: `src/embeddings/{cohere,huggingface,vertex}.rs`

---

## Phase 7: 高级文档处理 (优先级: 中)

### 7.1 语义分块 (Semantic Chunking)
- [ ] 基于嵌入相似度的智能分块
- [ ] 主题边界检测
- [ ] 上下文保持策略

**预计时间**: 1-2周
**文件**: `src/chunking/semantic.rs`

### 7.2 文档预处理
- [ ] 文本清理 (去除噪音、格式化)
- [ ] 语言检测
- [ ] 文档去重
- [ ] 元数据提取增强

**预计时间**: 1周
**文件**: `src/sources/preprocessing.rs`

### 7.3 额外文档格式
- [ ] Microsoft Word (.docx)
- [ ] PowerPoint (.pptx)
- [ ] Excel (.xlsx)
- [ ] 图片 OCR (Tesseract)

**预计时间**: 1-2周
**文件**: `src/sources/{docx,pptx,xlsx,ocr}.rs`

---

## Phase 8: 性能优化 (优先级: 中)

### 8.1 缓存层
- [ ] Embedding 缓存 (Redis/内存)
- [ ] 查询结果缓存
- [ ] TTL 和失效策略
- [ ] 缓存预热

**预计时间**: 1周
**文件**: `src/cache/mod.rs`

### 8.2 并发和批处理
- [ ] 并行 embedding 生成
- [ ] 批量索引优化
- [ ] 连接池管理
- [ ] 流式处理大文件

**预计时间**: 1-2周
**文件**: 贯穿各模块

### 8.3 向量压缩
- [ ] 量化 (Quantization)
- [ ] PQ (Product Quantization)
- [ ] 维度降低

**预计时间**: 2周
**文件**: `src/vectorstore/compression.rs`

---

## Phase 9: 其他向量数据库 (优先级: 低)

### 9.1 额外向量存储支持
- [ ] Pinecone 集成
- [ ] Weaviate 集成
- [ ] Chroma 集成
- [ ] Milvus 集成
- [ ] PostgreSQL pgvector

**预计时间**: 各1周
**文件**: `src/vectorstore/{pinecone,weaviate,chroma,milvus,pgvector}.rs`

---

## Phase 10: 监控和可观测性 (优先级: 中)

### 10.1 指标收集
- [ ] Prometheus metrics 导出
- [ ] 检索质量指标 (precision, recall, MRR)
- [ ] 性能指标 (延迟、吞吐量)
- [ ] 成本跟踪仪表板

**预计时间**: 1周
**文件**: `src/metrics/mod.rs`

### 10.2 日志和追踪
- [ ] 结构化日志
- [ ] OpenTelemetry 集成
- [ ] 分布式追踪
- [ ] 查询审计日志

**预计时间**: 1周
**文件**: `src/tracing/mod.rs`

---

## Phase 11: 高级功能 (优先级: 低)

### 11.1 增量索引
- [ ] 文档更新检测
- [ ] 增量重新索引
- [ ] 版本控制

**预计时间**: 1-2周

### 11.2 多模态 RAG
- [ ] 图像-文本联合检索
- [ ] 音频转录和索引
- [ ] 视频内容提取

**预计时间**: 3-4周

### 11.3 GraphRAG
- [ ] 知识图谱构建
- [ ] 图遍历检索
- [ ] 关系感知检索

**预计时间**: 4-6周

---

## 技术债务和改进

### 代码质量
- [ ] 修复所有 clippy 警告
- [ ] 增加代码覆盖率到 90%+
- [ ] 性能基准测试套件
- [ ] 集成测试覆盖所有功能

### 文档
- [ ] API 文档完善
- [ ] 用户指南和教程
- [ ] 架构设计文档
- [ ] 最佳实践指南

### CI/CD
- [ ] GitHub Actions 工作流
- [ ] 自动化测试
- [ ] 自动化发布
- [ ] Docker 镜像构建

---

## 优先级建议

**立即执行** (1-2周):
1. Phase 5.1 - 混合搜索 (高价值，相对简单)
2. Phase 5.2 - 完成 MMR 重排序
3. 代码质量改进 (修复警告，增加测试)

**短期** (1-2个月):
1. Phase 6.1 - ONNX 本地嵌入 (降低成本)
2. Phase 8.1 - 缓存层 (提升性能)
3. Phase 10 - 监控和指标

**中期** (3-6个月):
1. Phase 7 - 高级文档处理
2. Phase 9 - 多向量数据库支持
3. Phase 11.1 - 增量索引

**长期** (6个月+):
1. Phase 11.2 - 多模态 RAG
2. Phase 11.3 - GraphRAG

---

## 估算总工作量

- **核心功能完成**: ~8-12周
- **性能优化**: ~4-6周
- **文档和测试**: ~2-4周
- **总计**: ~14-22周 (3.5-5.5个月)

---

**最后更新**: 2025-01-04
**当前版本**: v0.3.0-alpha
**状态**: Phase 5 完成！Phase 6+ 待开发

**Phase 5 完成时间**: <1天 (比预估1-2周快很多)
**Phase 5 成果**:
- 3个新模块 (BM25, Hybrid, Re-ranking增强)
- 32个新测试 (全部通过)
- ~3,000行新代码
- 完整示例演示 (phase5_advanced_retrieval.rs)
