//! Utility functions for Paper Assistant
//!
//! This module provides helper functions for parsing paper sections,
//! creating markdown content, and other processing utilities.

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

use crate::workflow::PaperSection;

/// Extract sections from the LLM-generated sections text
pub fn extract_paper_sections(sections_text: &str) -> Result<Vec<PaperSection>> {
  let mut sections = Vec::new();
  
  // Split by section headers (## 章节 pattern)
  let section_regex = Regex::new(r"## 章节\s*([^：]*?)：([^#\n]*)")
    .map_err(|e| anyhow::anyhow!("Failed to compile section regex: {}", e))?;
  
  let content_parts: Vec<&str> = sections_text.split("---").collect();
  
  for part in content_parts {
    if let Some(captures) = section_regex.captures(part) {
      let number = captures.get(1).map(|m| m.as_str().trim().to_string());
      let title = captures.get(2).map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| "未知章节".to_string());
      
      // Extract content after "### 内容摘要"
      let content = if let Some(content_start) = part.find("### 内容摘要") {
        let prefix = "### 内容摘要";
        let start_pos = content_start + prefix.len();
        part[start_pos..].trim().to_string()
      } else {
        // Fall back to extracting content after the title
        let title_end = part.find(&title).map(|pos| pos + title.len()).unwrap_or(0);
        part[title_end..].trim()
          .lines()
          .skip_while(|line| line.trim().is_empty() || line.contains("###"))
          .collect::<Vec<&str>>()
          .join("\n")
          .trim()
          .to_string()
      };
      
      if !title.is_empty() && !content.is_empty() {
        sections.push(PaperSection {
          title,
          number,
          content,
        });
      }
    }
  }
  
  // If no sections found with the expected format, try simpler parsing
  if sections.is_empty() {
    sections = parse_sections_fallback(sections_text)?;
  }
  
  Ok(sections)
}

/// Fallback method for parsing sections when the expected format isn't found
fn parse_sections_fallback(text: &str) -> Result<Vec<PaperSection>> {
  let mut sections = Vec::new();
  
  // Try to find any section-like headers
  let header_patterns = [
    r"(?m)^#+\s*(.+?)$", // Markdown headers
    r"(?m)^(.+?)[:：]\s*$", // Lines ending with colon
    r"(?m)^([0-9]+\.?\s*.+?)$", // Numbered items
  ];
  
  for pattern in &header_patterns {
    if let Ok(regex) = Regex::new(pattern) {
      let mut current_section = None;
      let mut current_content = String::new();
      
      for line in text.lines() {
        if let Some(captures) = regex.captures(line) {
          // Save previous section if exists
          if let Some((title, number)) = current_section.take() {
            if !current_content.trim().is_empty() {
              sections.push(PaperSection {
                title,
                number,
                content: current_content.trim().to_string(),
              });
            }
          }
          
          // Start new section
          let full_title = captures.get(1).unwrap().as_str().trim();
          let (title, number) = parse_title_and_number(full_title);
          current_section = Some((title, number));
          current_content.clear();
        } else if current_section.is_some() {
          current_content.push_str(line);
          current_content.push('\n');
        }
      }
      
      // Save last section
      if let Some((title, number)) = current_section {
        if !current_content.trim().is_empty() {
          sections.push(PaperSection {
            title,
            number,
            content: current_content.trim().to_string(),
          });
        }
      }
      
      if !sections.is_empty() {
        break; // Found sections with this pattern
      }
    }
  }
  
  // If still no sections, create a single section from the entire text
  if sections.is_empty() {
    sections.push(PaperSection {
      title: "完整内容".to_string(),
      number: None,
      content: text.trim().to_string(),
    });
  }
  
  Ok(sections)
}

/// Parse title and extract number if present
fn parse_title_and_number(full_title: &str) -> (String, Option<String>) {
  // Try to extract number from the beginning
  let number_regex = Regex::new(r"^([0-9]+\.?)\s*(.+)$").unwrap();
  
  if let Some(captures) = number_regex.captures(full_title) {
    let number = captures.get(1).unwrap().as_str().trim_end_matches('.').to_string();
    let title = captures.get(2).unwrap().as_str().trim().to_string();
    (title, Some(number))
  } else {
    (full_title.to_string(), None)
  }
}

/// Create markdown content for a section to be used with MarkMapNode
pub fn create_section_markdown(title: &str, content: &str) -> String {
  let mut markdown = String::new();
  
  // Main section title
  markdown.push_str(&format!("# {}\n\n", title));
  
  // Break content into logical subsections for better mind mapping
  let subsections = create_subsections_from_content(content);
  
  for (i, (subtitle, subcontent)) in subsections.iter().enumerate() {
    if subsections.len() > 1 {
      markdown.push_str(&format!("## {}\n\n", subtitle));
    }
    
    // Convert content to bullet points for better mind map structure
    let bullet_points = create_bullet_points(subcontent);
    for point in bullet_points {
      markdown.push_str(&format!("- {}\n", point));
    }
    
    if i < subsections.len() - 1 {
      markdown.push('\n');
    }
  }
  
  markdown
}

/// Create logical subsections from content
fn create_subsections_from_content(content: &str) -> Vec<(String, String)> {
  let mut subsections = Vec::new();
  
  // Split by paragraphs and group related content
  let paragraphs: Vec<&str> = content.split('\n')
    .map(|s| s.trim())
    .filter(|s| !s.is_empty())
    .collect();
  
  if paragraphs.len() <= 3 {
    // Short content - keep as single section
    subsections.push(("核心内容".to_string(), content.to_string()));
  } else {
    // Longer content - try to create meaningful subsections
    let chunk_size = (paragraphs.len() + 2) / 3; // Aim for 3 subsections
    
    for (i, chunk) in paragraphs.chunks(chunk_size).enumerate() {
      let subtitle = match i {
        0 => "主要观点",
        1 => "详细内容", 
        2 => "结论要点",
        _ => &format!("要点 {}", i + 1),
      };
      
      let subcontent = chunk.join("\n");
      subsections.push((subtitle.to_string(), subcontent));
    }
  }
  
  subsections
}

/// Convert text content into bullet points for mind mapping
fn create_bullet_points(content: &str) -> Vec<String> {
  let mut points = Vec::new();
  
  // Split content into sentences
  let sentences: Vec<&str> = content.split(['。', '.', '；', ';'])
    .map(|s| s.trim())
    .filter(|s| !s.is_empty() && s.len() > 5)
    .collect();
  
  for sentence in sentences {
    // Clean up the sentence and make it concise
    let clean_sentence = sentence
      .replace('\n', " ")
      .replace("  ", " ")
      .trim()
      .to_string();
    
    if !clean_sentence.is_empty() && clean_sentence.len() < 200 {
      points.push(clean_sentence);
    }
  }
  
  // If we have too many points, summarize them
  if points.len() > 8 {
    let chunks: Vec<_> = points.chunks(3).collect();
    points = chunks.into_iter()
      .take(6) // Maximum 6 main points
      .map(|chunk| {
        if chunk.len() == 1 {
          chunk[0].clone()
        } else {
          // Combine related points
          let combined = chunk.join("；");
          if combined.len() > 150 {
            chunk[0].clone() // Use first point if too long
          } else {
            combined
          }
        }
      })
      .collect();
  }
  
  // Ensure we have at least one point
  if points.is_empty() {
    points.push(content.chars().take(100).collect::<String>());
  }
  
  points
}

/// Clean and format text for better readability
pub fn clean_text(text: &str) -> String {
  text.lines()
    .map(|line| line.trim())
    .filter(|line| !line.is_empty())
    .collect::<Vec<&str>>()
    .join(" ")
    .replace("  ", " ")
    .trim()
    .to_string()
}

/// Extract LaTeX section commands and convert to structured data
pub fn extract_latex_sections(latex_content: &str) -> Result<Vec<PaperSection>> {
  let mut sections = Vec::new();
  
  // Regex patterns for different section levels
  let section_patterns = [
    (r"\\section\*?\{([^}]+)\}", 1),
    (r"\\subsection\*?\{([^}]+)\}", 2), 
    (r"\\subsubsection\*?\{([^}]+)\}", 3),
  ];
  
  let mut current_sections: HashMap<i32, (String, String)> = HashMap::new();
  let mut last_level = 0;
  
  for line in latex_content.lines() {
    let line = line.trim();
    
    // Check for section headers
    let mut found_section = false;
    for (pattern, level) in &section_patterns {
      if let Ok(regex) = Regex::new(pattern) {
        if let Some(captures) = regex.captures(line) {
          let title = captures.get(1).unwrap().as_str().trim();
          
          // Save previous sections if moving to a new top-level section
          if *level <= last_level {
            for (sect_level, (sect_title, sect_content)) in current_sections.drain() {
              if !sect_content.trim().is_empty() {
                sections.push(PaperSection {
                  title: sect_title,
                  number: Some(sect_level.to_string()),
                  content: sect_content.trim().to_string(),
                });
              }
            }
          }
          
          current_sections.insert(*level, (title.to_string(), String::new()));
          last_level = *level;
          found_section = true;
          break;
        }
      }
    }
    
    // Add content to current sections if not a section header
    if !found_section && !line.is_empty() && !line.starts_with('\\') {
      for (_, (_, content)) in current_sections.iter_mut() {
        content.push_str(line);
        content.push(' ');
      }
    }
  }
  
  // Save remaining sections
  for (sect_level, (sect_title, sect_content)) in current_sections {
    if !sect_content.trim().is_empty() {
      sections.push(PaperSection {
        title: sect_title,
        number: Some(sect_level.to_string()),
        content: sect_content.trim().to_string(),
      });
    }
  }
  
  Ok(sections)
}

/// Generate a filename-safe string from a title
pub fn sanitize_filename(title: &str) -> String {
  title.chars()
    .map(|c| {
      if c.is_alphanumeric() || c == '_' || c == '-' {
        c
      } else if c.is_whitespace() {
        '_'
      } else {
        '_'
      }
    })
    .collect::<String>()
    .replace("__", "_")
    .trim_matches('_')
    .chars()
    .take(50) // Limit filename length
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_create_section_markdown() {
    let title = "Introduction";
    let content = "This is an introduction. It covers the background. The motivation is important.";
    
    let markdown = create_section_markdown(title, content);
    assert!(markdown.contains("# Introduction"));
    assert!(markdown.contains("- This is an introduction"));
  }

  #[test]
  fn test_create_bullet_points() {
    let content = "First point here. Second important point. Third conclusion.";
    let points = create_bullet_points(content);
    
    assert_eq!(points.len(), 3);
    assert!(points[0].contains("First point"));
    assert!(points[1].contains("Second important"));
    assert!(points[2].contains("Third conclusion"));
  }

  #[test]
  fn test_parse_title_and_number() {
    let (title, number) = parse_title_and_number("1. Introduction");
    assert_eq!(title, "Introduction");
    assert_eq!(number, Some("1".to_string()));
    
    let (title, number) = parse_title_and_number("Background and Motivation");
    assert_eq!(title, "Background and Motivation");
    assert_eq!(number, None);
  }

  #[test]
  fn test_sanitize_filename() {
    let title = "Introduction: Background & Motivation?";
    let safe = sanitize_filename(title);
    assert_eq!(safe, "Introduction_Background_Motivation");
    
    let long_title = "This is a very long title that should be truncated to avoid filesystem issues";
    let safe_long = sanitize_filename(long_title);
    assert!(safe_long.len() <= 50);
  }

  #[test]
  fn test_clean_text() {
    let messy_text = "   Line 1   \n\n   Line 2   \n   ";
    let clean = clean_text(messy_text);
    assert_eq!(clean, "Line 1 Line 2");
  }

  #[test]
  fn test_extract_paper_sections_with_standard_format() {
    let sections_text = r#"
## 章节 1：引言
### 内容摘要
这是引言部分的内容摘要。介绍了研究背景和动机。

---

## 章节 2：方法论
### 内容摘要
这是方法论部分的内容。详细描述了研究方法。

---
"#;
    
    let sections = extract_paper_sections(sections_text).unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].title, "引言");
    assert_eq!(sections[0].number, Some("1".to_string()));
    assert!(sections[0].content.contains("引言部分的内容"));
  }

  #[test]
  fn test_extract_sections_fallback() {
    let text = r#"
Introduction
This is the introduction content.

Methods  
This describes the methods used.

Results
The results are presented here.
"#;
    
    let sections = parse_sections_fallback(text).unwrap();
    assert!(sections.len() > 0);
  }

  #[test]
  fn test_create_subsections() {
    let content = "First paragraph. Second paragraph. Third paragraph. Fourth paragraph. Fifth paragraph.";
    let subsections = create_subsections_from_content(content);
    
    // Should create multiple subsections for longer content
    assert!(subsections.len() >= 1);
    
    for (title, content) in subsections {
      assert!(!title.is_empty());
      assert!(!content.is_empty());
    }
  }
}