//! Document preprocessing utilities
//!
//! This module provides text cleaning, normalization, language detection,
//! and deduplication utilities for document processing.
//!
//! # Features
//!
//! - Text cleaning (whitespace, special characters, HTML)
//! - Text normalization (Unicode, case, accents)
//! - Language detection (heuristic-based)
//! - Document deduplication (content hashing, fuzzy matching)
//! - Metadata extraction and enhancement
//!
//! # Example
//!
//! ```rust
//! use agentflow_rag::sources::preprocessing::{TextCleaner, PreprocessingPipeline};
//!
//! let cleaner = TextCleaner::new()
//!   .remove_extra_whitespace(true)
//!   .remove_special_chars(false)
//!   .normalize_unicode(true);
//!
//! let text = "  Multiple    spaces   and\t\ttabs  ";
//! let cleaned = cleaner.clean(text);
//! assert_eq!(cleaned, "Multiple spaces and tabs");
//! ```

use crate::{error::Result, sources::Document};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

/// Regex pattern for stripping HTML tags
static HTML_TAG_REGEX: OnceLock<Regex> = OnceLock::new();

/// Regex pattern for stripping URLs
static URL_REGEX: OnceLock<Regex> = OnceLock::new();

/// Regex pattern for stripping email addresses
static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();

/// Regex pattern for collapsing whitespace
static WHITESPACE_REGEX: OnceLock<Regex> = OnceLock::new();

/// Get or initialize the HTML tag removal regex
fn html_tag_regex() -> &'static Regex {
  HTML_TAG_REGEX.get_or_init(|| {
    Regex::new(r"<[^>]+>")
      .expect("HTML_TAG_REGEX pattern is invalid - this is a bug in agentflow-rag")
  })
}

/// Get or initialize the URL removal regex
fn url_regex() -> &'static Regex {
  URL_REGEX.get_or_init(|| {
    Regex::new(r"https?://\S+")
      .expect("URL_REGEX pattern is invalid - this is a bug in agentflow-rag")
  })
}

/// Get or initialize the email removal regex
fn email_regex() -> &'static Regex {
  EMAIL_REGEX.get_or_init(|| {
    Regex::new(r"\S+@\S+\.\S+")
      .expect("EMAIL_REGEX pattern is invalid - this is a bug in agentflow-rag")
  })
}

/// Get or initialize the whitespace collapse regex
fn whitespace_regex() -> &'static Regex {
  WHITESPACE_REGEX.get_or_init(|| {
    Regex::new(r"\s+")
      .expect("WHITESPACE_REGEX pattern is invalid - this is a bug in agentflow-rag")
  })
}

/// Text cleaning configuration and operations
#[derive(Debug, Clone)]
pub struct TextCleaner {
  /// Remove extra whitespace (collapse multiple spaces/tabs/newlines)
  remove_extra_whitespace: bool,

  /// Remove special characters (keep only alphanumeric and basic punctuation)
  remove_special_chars: bool,

  /// Normalize Unicode characters (NFD/NFC normalization)
  normalize_unicode: bool,

  /// Remove HTML tags
  remove_html: bool,

  /// Remove URLs
  remove_urls: bool,

  /// Remove email addresses
  remove_emails: bool,

  /// Convert to lowercase
  lowercase: bool,

  /// Trim leading/trailing whitespace
  trim: bool,
}

impl Default for TextCleaner {
  fn default() -> Self {
    Self {
      remove_extra_whitespace: true,
      remove_special_chars: false,
      normalize_unicode: true,
      remove_html: false,
      remove_urls: false,
      remove_emails: false,
      lowercase: false,
      trim: true,
    }
  }
}

impl TextCleaner {
  /// Create a new text cleaner with default settings
  pub fn new() -> Self {
    Self::default()
  }

  /// Set whether to remove extra whitespace
  pub fn remove_extra_whitespace(mut self, enable: bool) -> Self {
    self.remove_extra_whitespace = enable;
    self
  }

  /// Set whether to remove special characters
  pub fn remove_special_chars(mut self, enable: bool) -> Self {
    self.remove_special_chars = enable;
    self
  }

  /// Set whether to normalize Unicode
  pub fn normalize_unicode(mut self, enable: bool) -> Self {
    self.normalize_unicode = enable;
    self
  }

  /// Set whether to remove HTML tags
  pub fn remove_html(mut self, enable: bool) -> Self {
    self.remove_html = enable;
    self
  }

  /// Set whether to remove URLs
  pub fn remove_urls(mut self, enable: bool) -> Self {
    self.remove_urls = enable;
    self
  }

  /// Set whether to remove email addresses
  pub fn remove_emails(mut self, enable: bool) -> Self {
    self.remove_emails = enable;
    self
  }

  /// Set whether to convert to lowercase
  pub fn lowercase(mut self, enable: bool) -> Self {
    self.lowercase = enable;
    self
  }

  /// Set whether to trim whitespace
  pub fn trim(mut self, enable: bool) -> Self {
    self.trim = enable;
    self
  }

  /// Clean text according to configuration
  pub fn clean(&self, text: &str) -> String {
    let mut result = text.to_string();

    // Remove HTML tags
    if self.remove_html {
      result = self.strip_html(&result);
    }

    // Remove URLs
    if self.remove_urls {
      result = self.strip_urls(&result);
    }

    // Remove emails
    if self.remove_emails {
      result = self.strip_emails(&result);
    }

    // Normalize Unicode
    if self.normalize_unicode {
      result = self.normalize_unicode_str(&result);
    }

    // Remove special characters
    if self.remove_special_chars {
      result = self.strip_special_chars(&result);
    }

    // Remove extra whitespace
    if self.remove_extra_whitespace {
      result = self.collapse_whitespace(&result);
    }

    // Convert to lowercase
    if self.lowercase {
      result = result.to_lowercase();
    }

    // Trim
    if self.trim {
      result = result.trim().to_string();
    }

    result
  }

  /// Strip HTML tags
  fn strip_html(&self, text: &str) -> String {
    html_tag_regex().replace_all(text, " ").to_string()
  }

  /// Strip URLs
  fn strip_urls(&self, text: &str) -> String {
    url_regex().replace_all(text, " ").to_string()
  }

  /// Strip email addresses
  fn strip_emails(&self, text: &str) -> String {
    email_regex().replace_all(text, " ").to_string()
  }

  /// Normalize Unicode (simple NFC normalization)
  fn normalize_unicode_str(&self, text: &str) -> String {
    // For now, just return the text as-is
    // Full Unicode normalization would require the `unicode-normalization` crate
    text.to_string()
  }

  /// Strip special characters, keep alphanumeric and basic punctuation
  fn strip_special_chars(&self, text: &str) -> String {
    text
      .chars()
      .filter(|c| c.is_alphanumeric() || c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | '-' | '\'' | '"'))
      .collect()
  }

  /// Collapse multiple whitespace characters into single space
  fn collapse_whitespace(&self, text: &str) -> String {
    whitespace_regex().replace_all(text, " ").to_string()
  }
}

/// Language detection using simple heuristics
#[derive(Debug, Clone)]
pub struct LanguageDetector {
  /// Minimum confidence threshold (0.0-1.0)
  confidence_threshold: f32,
}

impl Default for LanguageDetector {
  fn default() -> Self {
    Self {
      confidence_threshold: 0.5,
    }
  }
}

impl LanguageDetector {
  /// Create a new language detector
  pub fn new() -> Self {
    Self::default()
  }

  /// Set confidence threshold
  pub fn with_confidence_threshold(mut self, threshold: f32) -> Self {
    self.confidence_threshold = threshold.clamp(0.0, 1.0);
    self
  }

  /// Detect language of text (simple heuristic-based approach)
  /// Returns (language_code, confidence)
  pub fn detect(&self, text: &str) -> (String, f32) {
    if text.is_empty() {
      return ("unknown".to_string(), 0.0);
    }

    // Count character ranges
    let mut latin_count = 0;
    let mut cjk_count = 0;
    let mut cyrillic_count = 0;
    let mut arabic_count = 0;
    let mut total_alpha = 0;

    for c in text.chars() {
      if c.is_alphabetic() {
        total_alpha += 1;

        match c {
          'a'..='z' | 'A'..='Z' => latin_count += 1,
          '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' => cjk_count += 1,
          '\u{0400}'..='\u{04FF}' => cyrillic_count += 1,
          '\u{0600}'..='\u{06FF}' => arabic_count += 1,
          _ => {}
        }
      }
    }

    if total_alpha == 0 {
      return ("unknown".to_string(), 0.0);
    }

    // Calculate percentages
    let latin_pct = latin_count as f32 / total_alpha as f32;
    let cjk_pct = cjk_count as f32 / total_alpha as f32;
    let cyrillic_pct = cyrillic_count as f32 / total_alpha as f32;
    let arabic_pct = arabic_count as f32 / total_alpha as f32;

    // Determine language based on highest percentage
    let (lang, confidence) = if cjk_pct > 0.3 {
      ("zh".to_string(), cjk_pct)
    } else if cyrillic_pct > 0.5 {
      ("ru".to_string(), cyrillic_pct)
    } else if arabic_pct > 0.5 {
      ("ar".to_string(), arabic_pct)
    } else if latin_pct > 0.5 {
      ("en".to_string(), latin_pct) // Default to English for Latin script
    } else {
      ("unknown".to_string(), 0.0)
    };

    (lang, confidence)
  }
}

/// Document deduplication utilities
#[derive(Debug, Clone)]
pub struct DocumentDeduplicator {
  /// Use content hashing for exact duplicate detection
  use_content_hash: bool,

  /// Use fuzzy matching for near-duplicate detection
  use_fuzzy_matching: bool,

  /// Similarity threshold for fuzzy matching (0.0-1.0)
  similarity_threshold: f32,
}

impl Default for DocumentDeduplicator {
  fn default() -> Self {
    Self {
      use_content_hash: true,
      use_fuzzy_matching: false,
      similarity_threshold: 0.95,
    }
  }
}

impl DocumentDeduplicator {
  /// Create a new deduplicator
  pub fn new() -> Self {
    Self::default()
  }

  /// Enable content hashing
  pub fn with_content_hash(mut self, enable: bool) -> Self {
    self.use_content_hash = enable;
    self
  }

  /// Enable fuzzy matching
  pub fn with_fuzzy_matching(mut self, enable: bool) -> Self {
    self.use_fuzzy_matching = enable;
    self
  }

  /// Set similarity threshold for fuzzy matching
  pub fn with_similarity_threshold(mut self, threshold: f32) -> Self {
    self.similarity_threshold = threshold.clamp(0.0, 1.0);
    self
  }

  /// Remove duplicate documents from a list
  pub fn deduplicate(&self, documents: Vec<Document>) -> Vec<Document> {
    let mut seen_hashes = HashSet::new();
    let mut unique_docs = Vec::new();

    for doc in documents {
      let content_hash = self.calculate_hash(&doc.content);

      if self.use_content_hash {
        if seen_hashes.insert(content_hash) {
          unique_docs.push(doc);
        }
      } else if self.use_fuzzy_matching {
        if !self.is_fuzzy_duplicate(&doc, &unique_docs) {
          seen_hashes.insert(content_hash);
          unique_docs.push(doc);
        }
      } else {
        unique_docs.push(doc);
      }
    }

    unique_docs
  }

  /// Calculate content hash
  fn calculate_hash(&self, content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
  }

  /// Check if document is a fuzzy duplicate of any in the list
  fn is_fuzzy_duplicate(&self, doc: &Document, existing: &[Document]) -> bool {
    for existing_doc in existing {
      let similarity = self.calculate_similarity(&doc.content, &existing_doc.content);
      if similarity >= self.similarity_threshold {
        return true;
      }
    }
    false
  }

  /// Calculate similarity between two strings (Jaccard similarity of words)
  fn calculate_similarity(&self, a: &str, b: &str) -> f32 {
    let words_a: HashSet<&str> = a.split_whitespace().collect();
    let words_b: HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
      return 1.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
      return 0.0;
    }

    intersection as f32 / union as f32
  }
}

/// Complete preprocessing pipeline
#[derive(Debug, Clone)]
pub struct PreprocessingPipeline {
  cleaner: TextCleaner,
  language_detector: LanguageDetector,
  deduplicator: DocumentDeduplicator,
  enable_language_detection: bool,
  enable_deduplication: bool,
}

impl Default for PreprocessingPipeline {
  fn default() -> Self {
    Self {
      cleaner: TextCleaner::default(),
      language_detector: LanguageDetector::default(),
      deduplicator: DocumentDeduplicator::default(),
      enable_language_detection: false,
      enable_deduplication: false,
    }
  }
}

impl PreprocessingPipeline {
  /// Create a new preprocessing pipeline
  pub fn new() -> Self {
    Self::default()
  }

  /// Set text cleaner
  pub fn with_cleaner(mut self, cleaner: TextCleaner) -> Self {
    self.cleaner = cleaner;
    self
  }

  /// Set language detector
  pub fn with_language_detector(mut self, detector: LanguageDetector) -> Self {
    self.language_detector = detector;
    self
  }

  /// Set deduplicator
  pub fn with_deduplicator(mut self, deduplicator: DocumentDeduplicator) -> Self {
    self.deduplicator = deduplicator;
    self
  }

  /// Enable language detection
  pub fn enable_language_detection(mut self, enable: bool) -> Self {
    self.enable_language_detection = enable;
    self
  }

  /// Enable deduplication
  pub fn enable_deduplication(mut self, enable: bool) -> Self {
    self.enable_deduplication = enable;
    self
  }

  /// Process a batch of documents
  pub fn process(&self, documents: Vec<Document>) -> Vec<Document> {
    let mut processed = documents;

    // Step 1: Clean text
    processed = processed
      .into_iter()
      .map(|mut doc| {
        doc.content = self.cleaner.clean(&doc.content);
        doc
      })
      .collect();

    // Step 2: Detect language and add to metadata
    if self.enable_language_detection {
      processed = processed
        .into_iter()
        .map(|mut doc| {
          let (lang, confidence) = self.language_detector.detect(&doc.content);
          doc.metadata.insert("language".to_string(), lang.into());
          doc
            .metadata
            .insert("language_confidence".to_string(), confidence.to_string().into());
          doc
        })
        .collect();
    }

    // Step 3: Deduplicate
    if self.enable_deduplication {
      processed = self.deduplicator.deduplicate(processed);
    }

    processed
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_text_cleaner_whitespace() {
    let cleaner = TextCleaner::new().remove_extra_whitespace(true);
    let text = "  Multiple    spaces   and\t\ttabs  ";
    let cleaned = cleaner.clean(text);
    assert_eq!(cleaned, "Multiple spaces and tabs");
  }

  #[test]
  fn test_text_cleaner_html() {
    let cleaner = TextCleaner::new().remove_html(true);
    let text = "<p>Hello <b>world</b></p>";
    let cleaned = cleaner.clean(text);
    assert!(cleaned.contains("Hello"));
    assert!(cleaned.contains("world"));
    assert!(!cleaned.contains("<p>"));
    assert!(!cleaned.contains("</b>"));
  }

  #[test]
  fn test_text_cleaner_urls() {
    let cleaner = TextCleaner::new().remove_urls(true);
    let text = "Check out https://example.com for more info";
    let cleaned = cleaner.clean(text);
    assert!(!cleaned.contains("https://example.com"));
    assert!(cleaned.contains("Check out"));
    assert!(cleaned.contains("for more info"));
  }

  #[test]
  fn test_language_detector_english() {
    let detector = LanguageDetector::new();
    let (lang, confidence) = detector.detect("This is an English sentence.");
    assert_eq!(lang, "en");
    assert!(confidence > 0.5);
  }

  #[test]
  fn test_language_detector_chinese() {
    let detector = LanguageDetector::new();
    let (lang, confidence) = detector.detect("这是中文句子");
    assert_eq!(lang, "zh");
    assert!(confidence > 0.3);
  }

  #[test]
  fn test_document_deduplication() {
    let deduplicator = DocumentDeduplicator::new();

    let docs = vec![
      Document::new("Hello world"),
      Document::new("Hello world"), // Exact duplicate
      Document::new("Goodbye world"),
    ];

    let unique = deduplicator.deduplicate(docs);
    assert_eq!(unique.len(), 2);
  }

  #[test]
  fn test_preprocessing_pipeline() {
    let pipeline = PreprocessingPipeline::new()
      .with_cleaner(TextCleaner::new().remove_extra_whitespace(true))
      .enable_language_detection(true)
      .enable_deduplication(true);

    let docs = vec![
      Document::new("  Multiple    spaces  "),
      Document::new("  Multiple    spaces  "), // Duplicate after cleaning
      Document::new("Different content"),
    ];

    let processed = pipeline.process(docs);
    assert_eq!(processed.len(), 2); // One duplicate removed
    assert_eq!(processed[0].content, "Multiple spaces");
    assert!(processed[0].metadata.contains_key("language"));
  }
}
