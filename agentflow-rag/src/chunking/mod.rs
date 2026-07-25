//! Document chunking strategies

use crate::{error::Result, types::TextChunk};

#[cfg(feature = "code-chunking")]
pub mod code_ast;
pub mod fixed_size;
pub mod heading;
pub mod paragraph;
pub mod recursive;
pub mod semantic;
pub mod sentence;

#[cfg(feature = "code-chunking")]
pub use code_ast::CodeAstChunker;
pub use fixed_size::FixedSizeChunker;
pub use heading::HeadingChunker;
pub use paragraph::ParagraphChunker;
pub use recursive::RecursiveChunker;
pub use semantic::{SemanticChunker, SemanticChunkerBuilder};
pub use sentence::SentenceChunker;

/// Chunking strategy trait
pub trait ChunkingStrategy: Send + Sync {
  /// Split text into chunks
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>>;

  /// Get chunk size
  fn chunk_size(&self) -> usize;

  /// Get overlap size
  fn overlap(&self) -> usize;

  /// Get strategy name
  fn strategy_name(&self) -> &str;
}

/// Create a chunking strategy from configuration.
///
/// Q3.9.1: routes through each chunker's `try_new` so a
/// misconfiguration (e.g. operator wrote `chunk_size: 100, overlap: 200`
/// in YAML) surfaces as a `ChunkingError` instead of being silently
/// clamped — config callers want loud errors, not invisible fixes.
pub fn create_chunker(
  strategy: crate::types::ChunkingStrategy,
  chunk_size: usize,
  overlap: usize,
) -> Result<Box<dyn ChunkingStrategy>> {
  match strategy {
    crate::types::ChunkingStrategy::FixedSize => {
      Ok(Box::new(FixedSizeChunker::try_new(chunk_size, overlap)?))
    }
    crate::types::ChunkingStrategy::Sentence => {
      Ok(Box::new(SentenceChunker::try_new(chunk_size, overlap)?))
    }
    crate::types::ChunkingStrategy::Recursive => {
      Ok(Box::new(RecursiveChunker::try_new(chunk_size, overlap)?))
    }
    crate::types::ChunkingStrategy::Semantic => Err(crate::error::RAGError::chunking(
      "Semantic chunking not yet implemented",
    )),
    crate::types::ChunkingStrategy::Paragraph => {
      Ok(Box::new(ParagraphChunker::try_new(chunk_size, overlap)?))
    }
    crate::types::ChunkingStrategy::Heading => {
      Ok(Box::new(HeadingChunker::try_new(chunk_size, overlap)?))
    }
    crate::types::ChunkingStrategy::CodeAst => {
      #[cfg(feature = "code-chunking")]
      {
        Ok(Box::new(code_ast::CodeAstChunker::try_new(
          chunk_size, overlap,
        )?))
      }
      #[cfg(not(feature = "code-chunking"))]
      {
        Err(crate::error::RAGError::chunking(
          "CodeAst chunking requires the `code-chunking` feature",
        ))
      }
    }
  }
}

/// `(id, content, metadata)` triple produced by
/// [`chunk_document_for_knowledge_backend`], ready for
/// [`crate::knowledge::Bm25KnowledgeBackend::from_chunked_documents`].
pub type ChunkedKnowledgeDocument = (
  String,
  String,
  std::collections::HashMap<String, crate::types::MetadataValue>,
);

/// Chunk a single document's content and package each resulting
/// [`TextChunk`] as an `(id, content, metadata)` triple, ready for
/// [`crate::knowledge::Bm25KnowledgeBackend::from_chunked_documents`] (L4.1).
///
/// `source_id` becomes the `source` metadata key on every chunk (unless the
/// chunker already set one) and the chunk id prefix, so downstream citation
/// checks (L4.4) can trace a chunk back to the file — and, combined with the
/// `start_line`/`end_line` metadata every new chunker populates, the exact
/// line range — it came from.
pub fn chunk_document_for_knowledge_backend(
  strategy: crate::types::ChunkingStrategy,
  chunk_size: usize,
  overlap: usize,
  source_id: &str,
  content: &str,
) -> Result<Vec<ChunkedKnowledgeDocument>> {
  let chunker = create_chunker(strategy, chunk_size, overlap)?;
  let chunks = chunker.chunk(content)?;
  let multi = chunks.len() > 1;
  Ok(
    chunks
      .into_iter()
      .map(|chunk| {
        let id = if multi {
          format!("{source_id}#chunk{}", chunk.chunk_index)
        } else {
          source_id.to_string()
        };
        let mut metadata = chunk.metadata;
        metadata
          .entry("source".to_string())
          .or_insert_with(|| crate::types::MetadataValue::String(source_id.to_string()));
        (id, chunk.content, metadata)
      })
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Q3.9.1: `try_new` must reject `overlap >= chunk_size` for every
  /// chunker. Pre-fix the constructors accepted these values and the
  /// chunker either looped forever (`overlap == chunk_size`) or
  /// panicked on usize underflow (`overlap > chunk_size`).
  #[test]
  fn try_new_rejects_overlap_at_or_above_chunk_size() {
    assert!(FixedSizeChunker::try_new(100, 100).is_err());
    assert!(FixedSizeChunker::try_new(100, 200).is_err());
    assert!(SentenceChunker::try_new(100, 100).is_err());
    assert!(SentenceChunker::try_new(100, 200).is_err());
    assert!(RecursiveChunker::try_new(100, 100).is_err());
    assert!(RecursiveChunker::try_new(100, 200).is_err());
  }

  #[test]
  fn try_new_rejects_zero_chunk_size() {
    assert!(FixedSizeChunker::try_new(0, 0).is_err());
    assert!(SentenceChunker::try_new(0, 0).is_err());
    assert!(RecursiveChunker::try_new(0, 0).is_err());
  }

  #[test]
  fn try_new_accepts_valid_inputs() {
    assert!(FixedSizeChunker::try_new(100, 20).is_ok());
    assert!(FixedSizeChunker::try_new(100, 0).is_ok());
    assert!(SentenceChunker::try_new(200, 50).is_ok());
    assert!(RecursiveChunker::try_new(512, 64).is_ok());
  }

  /// Q3.9.1: the infallible `new()` API must not panic on bad
  /// inputs — instead it clamps so existing call sites keep working.
  /// Confirms `FixedSizeChunker::new(100, 100)` does not loop
  /// forever by actually invoking `chunk()` on real text.
  #[test]
  fn fixed_size_new_clamps_overlap_at_chunk_size() {
    let chunker = FixedSizeChunker::new(10, 10); // pre-fix: infinite loop
    let chunks = chunker
      .chunk("Hello world, this is a test string that must be sliced into multiple chunks.")
      .expect("clamped chunker must produce chunks without panicking");
    assert!(chunks.len() > 1, "must actually produce multiple chunks");
    assert!(
      chunker.overlap() < chunker.chunk_size(),
      "clamp invariant: overlap < chunk_size"
    );
  }

  #[test]
  fn fixed_size_new_clamps_overlap_above_chunk_size() {
    let chunker = FixedSizeChunker::new(50, 500); // pre-fix: usize underflow panic
    let chunks = chunker
      .chunk("Some text long enough to produce a couple of chunks given the clamped overlap.")
      .expect("clamped chunker must not panic on overlap > chunk_size");
    assert!(!chunks.is_empty());
    assert_eq!(chunker.overlap(), 49);
  }

  #[test]
  fn create_chunker_surfaces_invalid_overlap_as_error() {
    // The factory path used by YAML config — must NOT silently
    // clamp, since the operator likely typo'd the config.
    let result = create_chunker(crate::types::ChunkingStrategy::FixedSize, 100, 150);
    assert!(result.is_err());
  }

  #[test]
  fn create_chunker_supports_the_l4_1_strategies() {
    assert!(create_chunker(crate::types::ChunkingStrategy::Paragraph, 100, 0).is_ok());
    assert!(create_chunker(crate::types::ChunkingStrategy::Heading, 100, 0).is_ok());
    #[cfg(feature = "code-chunking")]
    assert!(create_chunker(crate::types::ChunkingStrategy::CodeAst, 100, 0).is_ok());
    #[cfg(not(feature = "code-chunking"))]
    assert!(create_chunker(crate::types::ChunkingStrategy::CodeAst, 100, 0).is_err());
  }

  #[test]
  fn chunk_document_for_knowledge_backend_ids_and_tags_source() {
    let text = "Para one is short.\n\nPara two is also short.";
    let triples = chunk_document_for_knowledge_backend(
      crate::types::ChunkingStrategy::Paragraph,
      1000,
      0,
      "docs/guide.md",
      text,
    )
    .expect("chunk ok");
    // Small paragraphs merge into a single chunk at this chunk_size, so the
    // id must be the bare source_id (no #chunkN suffix).
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0].0, "docs/guide.md");
    assert_eq!(
      triples[0].2.get("source"),
      Some(&crate::types::MetadataValue::String(
        "docs/guide.md".to_string()
      ))
    );
  }

  #[test]
  fn chunk_document_for_knowledge_backend_suffixes_ids_when_multiple_chunks() {
    let text = "Para one.\n\nPara two.\n\nPara three.";
    let triples = chunk_document_for_knowledge_backend(
      crate::types::ChunkingStrategy::Paragraph,
      10,
      0,
      "docs/guide.md",
      text,
    )
    .expect("chunk ok");
    assert!(triples.len() > 1);
    for (id, _, _) in &triples {
      assert!(id.starts_with("docs/guide.md#chunk"));
    }
  }
}
