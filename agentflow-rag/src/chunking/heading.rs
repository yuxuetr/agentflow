//! Markdown-heading-based chunking strategy (L4.1).
//!
//! One chunk per section: a heading line (`#` through `######`) plus every
//! line up to (but not including) the next heading, or end of document. Text
//! before the first heading becomes an un-headed leading chunk. A section
//! larger than `chunk_size` falls back to fixed-size sub-splitting so no
//! single chunk grows unbounded, while still carrying its parent `heading`
//! in metadata. Every chunk carries `start_line` / `end_line` (1-indexed,
//! inclusive) metadata.

use crate::{
  chunking::ChunkingStrategy,
  error::{RAGError, Result},
  types::{MetadataValue, TextChunk},
};
use std::collections::HashMap;

/// Returns `Some(level)` (1-6) if `line` is a markdown ATX heading with
/// non-empty heading text, `None` otherwise.
fn heading_level(line: &str) -> Option<usize> {
  let hashes = line.chars().take_while(|&c| c == '#').count();
  if hashes == 0 || hashes > 6 {
    return None;
  }
  let rest = &line[hashes..];
  if rest.starts_with(' ') && !rest.trim().is_empty() {
    Some(hashes)
  } else {
    None
  }
}

struct Section {
  heading: Option<String>,
  start_line: usize,
  end_line: usize,
  content: String,
}

fn split_sections(text: &str) -> Vec<Section> {
  let lines: Vec<&str> = text.lines().collect();
  let mut sections = Vec::new();
  let mut current_heading: Option<String> = None;
  let mut current_start = 1usize;
  let mut current_lines: Vec<&str> = Vec::new();

  for (idx, line) in lines.iter().enumerate() {
    let line_no = idx + 1;
    if heading_level(line).is_some() {
      if !current_lines.is_empty() || current_heading.is_some() {
        sections.push(Section {
          heading: current_heading.clone(),
          start_line: current_start,
          end_line: line_no - 1,
          content: current_lines.join("\n"),
        });
      }
      current_heading = Some(line.trim().to_string());
      current_start = line_no;
      current_lines = vec![*line];
    } else {
      if current_lines.is_empty() && current_heading.is_none() {
        current_start = line_no;
      }
      current_lines.push(line);
    }
  }
  if !current_lines.is_empty() || current_heading.is_some() {
    sections.push(Section {
      heading: current_heading,
      start_line: current_start,
      end_line: lines.len().max(current_start),
      content: current_lines.join("\n"),
    });
  }
  sections
}

/// Fixed-size fallback for an oversized section's content, returning
/// `(slice, start_line, end_line)` triples anchored on `section_start_line`.
fn split_oversized(
  content: &str,
  section_start_line: usize,
  chunk_size: usize,
  overlap: usize,
) -> Vec<(String, usize, usize)> {
  let chars: Vec<char> = content.chars().collect();
  let total_len = chars.len();
  if total_len == 0 {
    return vec![(content.to_string(), section_start_line, section_start_line)];
  }

  let mut out = Vec::new();
  let mut start = 0usize;
  while start < total_len {
    let end = (start + chunk_size).min(total_len);
    let slice: String = chars[start..end].iter().collect();
    let start_line = section_start_line + chars[..start].iter().filter(|&&c| c == '\n').count();
    let end_line = section_start_line + chars[..end].iter().filter(|&&c| c == '\n').count();
    out.push((slice, start_line, end_line));
    if end >= total_len {
      break;
    }
    start += chunk_size - overlap;
  }
  out
}

pub struct HeadingChunker {
  chunk_size: usize,
  overlap: usize,
}

impl HeadingChunker {
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
      return Err(RAGError::chunking("HeadingChunker requires chunk_size > 0"));
    }
    if overlap >= chunk_size {
      return Err(RAGError::chunking(format!(
        "HeadingChunker requires overlap < chunk_size; got overlap={overlap} chunk_size={chunk_size}"
      )));
    }
    Ok(Self {
      chunk_size,
      overlap,
    })
  }
}

impl ChunkingStrategy for HeadingChunker {
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>> {
    let sections = split_sections(text);
    let mut chunks = Vec::new();

    for section in &sections {
      let section_len = section.content.chars().count();
      let pieces: Vec<(String, usize, usize)> = if section_len <= self.chunk_size {
        vec![(
          section.content.clone(),
          section.start_line,
          section.end_line,
        )]
      } else {
        split_oversized(
          &section.content,
          section.start_line,
          self.chunk_size,
          self.overlap,
        )
      };

      for (content, start_line, end_line) in pieces {
        let mut metadata = HashMap::new();
        metadata.insert(
          "start_line".to_string(),
          MetadataValue::Integer(start_line as i64),
        );
        metadata.insert(
          "end_line".to_string(),
          MetadataValue::Integer(end_line as i64),
        );
        if let Some(heading) = &section.heading {
          metadata.insert(
            "heading".to_string(),
            MetadataValue::String(heading.clone()),
          );
        }
        chunks.push(TextChunk {
          content,
          start_idx: start_line,
          end_idx: end_line,
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
    "heading"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn one_chunk_per_section() {
    let chunker = HeadingChunker::new(1000, 0);
    let text = "# Title\n\nIntro text.\n\n## Section A\n\nBody A.\n\n## Section B\n\nBody B.";
    let chunks = chunker.chunk(text).expect("chunk ok");
    assert_eq!(chunks.len(), 3);
    assert_eq!(
      chunks[0].metadata.get("heading"),
      Some(&MetadataValue::String("# Title".to_string()))
    );
    assert!(chunks[1].content.contains("Body A."));
    assert_eq!(
      chunks[1].metadata.get("heading"),
      Some(&MetadataValue::String("## Section A".to_string()))
    );
    assert!(chunks[2].content.contains("Body B."));
  }

  #[test]
  fn leading_text_before_first_heading_has_no_heading_metadata() {
    let chunker = HeadingChunker::new(1000, 0);
    let text = "Preamble.\n\n# First Heading\n\nBody.";
    let chunks = chunker.chunk(text).expect("chunk ok");
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].content.contains("Preamble."));
    assert!(!chunks[0].metadata.contains_key("heading"));
    assert!(chunks[1].metadata.contains_key("heading"));
  }

  #[test]
  fn document_with_no_headings_is_one_chunk() {
    let chunker = HeadingChunker::new(1000, 0);
    let text = "Just plain text.\nNo headings here.";
    let chunks = chunker.chunk(text).expect("chunk ok");
    assert_eq!(chunks.len(), 1);
    assert!(!chunks[0].metadata.contains_key("heading"));
  }

  #[test]
  fn oversized_section_falls_back_to_fixed_size_sub_split() {
    let chunker = HeadingChunker::new(10, 0);
    let text = "# H\n\nThis section body is much longer than ten characters for sure.";
    let chunks = chunker.chunk(text).expect("chunk ok");
    assert!(chunks.len() > 1, "oversized section must be sub-split");
    for c in &chunks {
      assert_eq!(
        c.metadata.get("heading"),
        Some(&MetadataValue::String("# H".to_string())),
        "every sub-chunk keeps the parent heading"
      );
    }
  }

  #[test]
  fn try_new_rejects_bad_inputs() {
    assert!(HeadingChunker::try_new(0, 0).is_err());
    assert!(HeadingChunker::try_new(100, 100).is_err());
    assert!(HeadingChunker::try_new(100, 20).is_ok());
  }
}
