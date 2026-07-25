//! Rust-source AST-aware chunking strategy (L4.1, `code-chunking` feature).
//!
//! One chunk per top-level item (`fn`, `struct`, `enum`, `impl`, `trait`,
//! `mod`, `const`, `static`, `type`, `use`, `macro_rules!`, ...), so a chunk
//! never straddles a function boundary. An oversized single item (e.g. a
//! very long function) falls back to line-bounded fixed-size sub-splitting.
//! Every chunk carries `start_line` / `end_line` (from the real `syn` /
//! `proc-macro2` source span) plus `item_kind` and, where available,
//! `item_name` metadata.

use crate::{
  chunking::ChunkingStrategy,
  error::{RAGError, Result},
  types::{MetadataValue, TextChunk},
};
use std::collections::HashMap;
use syn::spanned::Spanned;

fn item_kind(item: &syn::Item) -> &'static str {
  match item {
    syn::Item::Fn(_) => "fn",
    syn::Item::Struct(_) => "struct",
    syn::Item::Enum(_) => "enum",
    syn::Item::Impl(_) => "impl",
    syn::Item::Trait(_) => "trait",
    syn::Item::Mod(_) => "mod",
    syn::Item::Const(_) => "const",
    syn::Item::Static(_) => "static",
    syn::Item::Type(_) => "type",
    syn::Item::Use(_) => "use",
    syn::Item::Macro(_) => "macro",
    _ => "item",
  }
}

fn item_name(item: &syn::Item) -> Option<String> {
  match item {
    syn::Item::Fn(i) => Some(i.sig.ident.to_string()),
    syn::Item::Struct(i) => Some(i.ident.to_string()),
    syn::Item::Enum(i) => Some(i.ident.to_string()),
    syn::Item::Trait(i) => Some(i.ident.to_string()),
    syn::Item::Mod(i) => Some(i.ident.to_string()),
    syn::Item::Const(i) => Some(i.ident.to_string()),
    syn::Item::Static(i) => Some(i.ident.to_string()),
    syn::Item::Type(i) => Some(i.ident.to_string()),
    _ => None,
  }
}

/// Fixed-size fallback for an oversized item's source lines, returning
/// `(slice, start_line, end_line)` triples.
fn split_lines_by_size(
  lines: &[&str],
  start_line: usize,
  end_line: usize,
  chunk_size: usize,
  overlap: usize,
) -> Vec<(String, usize, usize)> {
  let content = lines[start_line.saturating_sub(1)..end_line].join("\n");
  let chars: Vec<char> = content.chars().collect();
  let total_len = chars.len();
  if total_len == 0 {
    return vec![(content, start_line, end_line)];
  }

  let mut out = Vec::new();
  let mut start = 0usize;
  while start < total_len {
    let end = (start + chunk_size).min(total_len);
    let slice: String = chars[start..end].iter().collect();
    let s_line = start_line + chars[..start].iter().filter(|&&c| c == '\n').count();
    let e_line = start_line + chars[..end].iter().filter(|&&c| c == '\n').count();
    out.push((slice, s_line, e_line));
    if end >= total_len {
      break;
    }
    start += chunk_size - overlap;
  }
  out
}

pub struct CodeAstChunker {
  chunk_size: usize,
  overlap: usize,
}

impl CodeAstChunker {
  /// Infallible constructor; clamps like [`crate::chunking::FixedSizeChunker::new`].
  pub fn new(chunk_size: usize, overlap: usize) -> Self {
    let chunk_size = chunk_size.max(1);
    let overlap = overlap.min(chunk_size.saturating_sub(1));
    Self {
      chunk_size,
      overlap,
    }
  }

  /// Fallible constructor for config-driven call sites.
  pub fn try_new(chunk_size: usize, overlap: usize) -> Result<Self> {
    if chunk_size == 0 {
      return Err(RAGError::chunking("CodeAstChunker requires chunk_size > 0"));
    }
    if overlap >= chunk_size {
      return Err(RAGError::chunking(format!(
        "CodeAstChunker requires overlap < chunk_size; got overlap={overlap} chunk_size={chunk_size}"
      )));
    }
    Ok(Self {
      chunk_size,
      overlap,
    })
  }
}

impl ChunkingStrategy for CodeAstChunker {
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>> {
    let file = syn::parse_file(text)
      .map_err(|e| RAGError::chunking(format!("failed to parse Rust source: {e}")))?;
    let lines: Vec<&str> = text.lines().collect();
    let mut chunks = Vec::new();

    for item in &file.items {
      let span = item.span();
      let start_line = span.start().line.max(1);
      let end_line = span.end().line.max(start_line).min(lines.len().max(1));
      let content = lines[start_line.saturating_sub(1)..end_line].join("\n");
      if content.trim().is_empty() {
        continue;
      }

      let mut base_metadata = HashMap::new();
      base_metadata.insert(
        "item_kind".to_string(),
        MetadataValue::String(item_kind(item).to_string()),
      );
      if let Some(name) = item_name(item) {
        base_metadata.insert("item_name".to_string(), MetadataValue::String(name));
      }

      let pieces: Vec<(String, usize, usize)> = if content.chars().count() <= self.chunk_size {
        vec![(content, start_line, end_line)]
      } else {
        split_lines_by_size(&lines, start_line, end_line, self.chunk_size, self.overlap)
      };

      for (piece_content, piece_start, piece_end) in pieces {
        let mut metadata = base_metadata.clone();
        metadata.insert(
          "start_line".to_string(),
          MetadataValue::Integer(piece_start as i64),
        );
        metadata.insert(
          "end_line".to_string(),
          MetadataValue::Integer(piece_end as i64),
        );
        chunks.push(TextChunk {
          content: piece_content,
          start_idx: piece_start,
          end_idx: piece_end,
          metadata,
          chunk_index: chunks.len(),
          total_chunks: 0,
        });
      }
    }

    let total = chunks.len();
    for c in &mut chunks {
      c.total_chunks = total;
    }
    Ok(chunks)
  }

  fn chunk_size(&self) -> usize {
    self.chunk_size
  }

  fn overlap(&self) -> usize {
    self.overlap
  }

  fn strategy_name(&self) -> &str {
    "code_ast"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE: &str = r#"
use std::fmt;

/// Adds two numbers.
fn add(a: i32, b: i32) -> i32 {
  a + b
}

struct Point {
  x: i32,
  y: i32,
}

impl Point {
  fn origin() -> Self {
    Point { x: 0, y: 0 }
  }
}
"#;

  #[test]
  fn one_chunk_per_top_level_item() {
    let chunker = CodeAstChunker::new(1000, 0);
    let chunks = chunker.chunk(SAMPLE).expect("valid rust source parses");
    let kinds: Vec<&str> = chunks
      .iter()
      .map(|c| match c.metadata.get("item_kind") {
        Some(MetadataValue::String(s)) => s.as_str(),
        _ => panic!("every chunk must carry item_kind"),
      })
      .collect();
    assert_eq!(kinds, vec!["use", "fn", "struct", "impl"]);
  }

  #[test]
  fn captures_item_name_and_line_range() {
    let chunker = CodeAstChunker::new(1000, 0);
    let chunks = chunker.chunk(SAMPLE).expect("valid rust source parses");
    let add_chunk = chunks
      .iter()
      .find(|c| c.metadata.get("item_kind") == Some(&MetadataValue::String("fn".to_string())))
      .expect("fn chunk present");
    assert_eq!(
      add_chunk.metadata.get("item_name"),
      Some(&MetadataValue::String("add".to_string()))
    );
    assert!(add_chunk.content.contains("fn add(a: i32, b: i32) -> i32"));
    assert!(add_chunk.content.contains("a + b"));
    assert!(matches!(
      add_chunk.metadata.get("start_line"),
      Some(MetadataValue::Integer(_))
    ));
  }

  #[test]
  fn invalid_rust_source_is_a_loud_error() {
    let chunker = CodeAstChunker::new(1000, 0);
    let err = chunker
      .chunk("fn broken( {{{ not rust")
      .expect_err("invalid source must not silently produce chunks");
    assert!(matches!(err, RAGError::ChunkingError { .. }));
  }

  #[test]
  fn oversized_item_falls_back_to_sub_split() {
    let chunker = CodeAstChunker::new(20, 0);
    let chunks = chunker.chunk(SAMPLE).expect("valid rust source parses");
    let fn_pieces: Vec<_> = chunks
      .iter()
      .filter(|c| c.metadata.get("item_kind") == Some(&MetadataValue::String("fn".to_string())))
      .collect();
    assert!(
      fn_pieces.len() > 1,
      "the fn item exceeds chunk_size and must be sub-split"
    );
    for piece in &fn_pieces {
      assert_eq!(
        piece.metadata.get("item_name"),
        Some(&MetadataValue::String("add".to_string())),
        "sub-chunks keep the parent item's name"
      );
    }
  }

  #[test]
  fn try_new_rejects_bad_inputs() {
    assert!(CodeAstChunker::try_new(0, 0).is_err());
    assert!(CodeAstChunker::try_new(100, 100).is_err());
    assert!(CodeAstChunker::try_new(100, 20).is_ok());
  }
}
