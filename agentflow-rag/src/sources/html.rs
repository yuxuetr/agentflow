//! HTML document loader
//!
//! Supports loading HTML files and extracting text content.

use crate::{error::Result, sources::DocumentLoader, types::Document};
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};
use std::path::Path;
use std::sync::OnceLock;
use tokio::fs;

/// Regex pattern for removing script tags
static SCRIPT_REGEX: OnceLock<Regex> = OnceLock::new();

/// Regex pattern for removing style tags
static STYLE_REGEX: OnceLock<Regex> = OnceLock::new();

/// Get or initialize the script removal regex
fn script_regex() -> &'static Regex {
  SCRIPT_REGEX.get_or_init(|| {
    Regex::new(r"<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>")
      .expect("SCRIPT_REGEX pattern is invalid - this is a bug in agentflow-rag")
  })
}

/// Get or initialize the style removal regex
fn style_regex() -> &'static Regex {
  STYLE_REGEX.get_or_init(|| {
    Regex::new(r"<style\b[^<]*(?:(?!<\/style>)<[^<]*)*<\/style>")
      .expect("STYLE_REGEX pattern is invalid - this is a bug in agentflow-rag")
  })
}

/// HTML document loader
///
/// Extracts text content from HTML files, with options to filter by selectors.
///
/// # Example
/// ```rust,no_run
/// use agentflow_rag::sources::{DocumentLoader, html::HtmlLoader};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let loader = HtmlLoader::new();
/// let doc = loader.load(Path::new("document.html")).await?;
/// # Ok(())
/// # }
/// ```
pub struct HtmlLoader {
  /// CSS selector to extract specific elements (e.g., "article", "main")
  content_selector: Option<String>,
  /// Remove script and style tags
  remove_scripts: bool,
}

impl HtmlLoader {
  /// Create a new HTML loader
  pub fn new() -> Self {
    Self {
      content_selector: None,
      remove_scripts: true,
    }
  }

  /// Set a CSS selector to extract specific content
  ///
  /// # Example
  /// ```rust
  /// use agentflow_rag::sources::html::HtmlLoader;
  ///
  /// let loader = HtmlLoader::new()
  ///     .with_selector("article");
  /// ```
  pub fn with_selector(mut self, selector: impl Into<String>) -> Self {
    self.content_selector = Some(selector.into());
    self
  }

  /// Include script and style tags in content
  pub fn include_scripts(mut self) -> Self {
    self.remove_scripts = false;
    self
  }

  /// Extract text from HTML content
  fn extract_text(&self, html_content: &str) -> Result<String> {
    let document = Html::parse_document(html_content);

    // Remove script and style elements if requested
    let cleaned_html = if self.remove_scripts {
      let mut html = html_content.to_string();

      // Remove scripts
      if let Ok(script_selector) = Selector::parse("script") {
        let scripts: Vec<_> = document.select(&script_selector).collect();
        for _ in scripts {
          // Note: scraper doesn't provide element removal, so we use regex as fallback
          html = script_regex().replace_all(&html, "").to_string();
        }
      }

      // Remove styles
      if let Ok(style_selector) = Selector::parse("style") {
        let styles: Vec<_> = document.select(&style_selector).collect();
        for _ in styles {
          html = style_regex().replace_all(&html, "").to_string();
        }
      }

      Html::parse_document(&html)
    } else {
      document
    };

    // Extract text based on selector
    let text = if let Some(ref selector_str) = self.content_selector {
      let selector =
        Selector::parse(selector_str).map_err(|e| crate::error::RAGError::DocumentError {
          message: format!("Invalid CSS selector '{}': {:?}", selector_str, e),
        })?;

      let texts: Vec<String> = cleaned_html
        .select(&selector)
        .map(|el| el.text().collect::<Vec<_>>().join(" "))
        .collect();

      texts.join("\n\n")
    } else {
      // Extract all text
      cleaned_html
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
    };

    // Clean up whitespace
    let cleaned_text = text
      .lines()
      .map(|line| line.trim())
      .filter(|line| !line.is_empty())
      .collect::<Vec<_>>()
      .join("\n");

    Ok(cleaned_text)
  }
}

impl Default for HtmlLoader {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl DocumentLoader for HtmlLoader {
  async fn load(&self, path: &Path) -> Result<Document> {
    let html_content = fs::read_to_string(path).await?;
    let text = self.extract_text(&html_content)?;

    let mut doc = Document::new(text);

    // Add metadata
    doc.metadata.insert(
      "source".to_string(),
      path.to_string_lossy().to_string().into(),
    );
    doc
      .metadata
      .insert("file_type".to_string(), "html".to_string().into());

    if let Some(file_name) = path.file_name() {
      doc.metadata.insert(
        "file_name".to_string(),
        file_name.to_string_lossy().to_string().into(),
      );
    }

    // Try to extract title
    let document = Html::parse_document(&html_content);
    if let Ok(title_selector) = Selector::parse("title") {
      if let Some(title_el) = document.select(&title_selector).next() {
        let title: String = title_el.text().collect();
        if !title.trim().is_empty() {
          doc
            .metadata
            .insert("title".to_string(), title.trim().to_string().into());
        }
      }
    }

    Ok(doc)
  }

  async fn load_directory(&self, dir: &Path, recursive: bool) -> Result<Vec<Document>> {
    let mut documents = Vec::new();
    let supported_exts = self.supported_extensions();

    if !dir.is_dir() {
      return Err(crate::error::RAGError::DocumentError {
        message: format!("Path is not a directory: {}", dir.display()),
      });
    }

    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
      let path = entry.path();

      if path.is_file() {
        if let Some(ext) = path.extension() {
          let ext_str = ext.to_string_lossy();
          if supported_exts.contains(&ext_str.as_ref()) {
            match self.load(&path).await {
              Ok(doc) => documents.push(doc),
              Err(e) => {
                tracing::warn!("Failed to load HTML {}: {}", path.display(), e);
              }
            }
          }
        }
      } else if path.is_dir() && recursive {
        match self.load_directory(&path, recursive).await {
          Ok(mut subdocs) => documents.append(&mut subdocs),
          Err(e) => {
            tracing::warn!("Failed to load directory {}: {}", path.display(), e);
          }
        }
      }
    }

    Ok(documents)
  }

  fn supported_extensions(&self) -> Vec<&'static str> {
    vec!["html", "htm"]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  #[tokio::test]
  async fn test_load_simple_html() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.html");

    let html_content = r#"
      <!DOCTYPE html>
      <html>
        <head><title>Test</title></head>
        <body>
          <h1>Hello</h1>
          <p>World</p>
        </body>
      </html>
    "#;
    fs::write(&file_path, html_content).await.unwrap();

    let loader = HtmlLoader::new();
    let doc = loader.load(&file_path).await.unwrap();

    assert!(doc.content.contains("Hello"));
    assert!(doc.content.contains("World"));
    assert_eq!(
      doc.metadata.get("file_type").unwrap().to_string(),
      "\"html\""
    );
  }

  #[tokio::test]
  async fn test_html_with_selector() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.html");

    let html_content = r#"
      <html>
        <body>
          <nav>Navigation</nav>
          <article>Main Content</article>
          <footer>Footer</footer>
        </body>
      </html>
    "#;
    fs::write(&file_path, html_content).await.unwrap();

    let loader = HtmlLoader::new().with_selector("article");
    let doc = loader.load(&file_path).await.unwrap();

    assert!(doc.content.contains("Main Content"));
    assert!(!doc.content.contains("Navigation"));
    assert!(!doc.content.contains("Footer"));
  }

  #[tokio::test]
  async fn test_html_removes_scripts() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.html");

    let html_content = r#"
      <html>
        <head>
          <script>console.log('test');</script>
          <style>body { color: red; }</style>
        </head>
        <body>
          <p>Content</p>
        </body>
      </html>
    "#;
    fs::write(&file_path, html_content).await.unwrap();

    let loader = HtmlLoader::new();
    let doc = loader.load(&file_path).await.unwrap();

    assert!(doc.content.contains("Content"));
    assert!(!doc.content.contains("console.log"));
    assert!(!doc.content.contains("color: red"));
  }

  #[tokio::test]
  async fn test_html_title_extraction() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.html");

    let html_content = r#"
      <html>
        <head><title>Page Title</title></head>
        <body>Content</body>
      </html>
    "#;
    fs::write(&file_path, html_content).await.unwrap();

    let loader = HtmlLoader::new();
    let doc = loader.load(&file_path).await.unwrap();

    assert_eq!(
      doc.metadata.get("title").unwrap().to_string(),
      "\"Page Title\""
    );
  }

  #[test]
  fn test_supported_extensions() {
    let loader = HtmlLoader::new();
    let exts = loader.supported_extensions();
    assert_eq!(exts, vec!["html", "htm"]);
  }
}
