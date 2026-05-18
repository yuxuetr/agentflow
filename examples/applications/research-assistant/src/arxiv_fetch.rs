//! Tiny arxiv search-API client. Fetches recent papers in a category via
//! the public Atom feed at `http://export.arxiv.org/api/query`. No auth.
//!
//! Atom XML is parsed via `quick-xml`'s serde deserialization. Arxiv's
//! response shape is stable enough for the small struct subset below to
//! work without a full Atom schema.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, info};

/// One arxiv paper (subset of fields we care about for briefings).
#[derive(Debug, Clone)]
pub struct Paper {
  /// Stable id without version suffix, e.g. `2501.12345`. Sourced from
  /// `<id>http://arxiv.org/abs/2501.12345v1</id>` — strip prefix + `vN`.
  pub paper_id: String,
  /// Full URL to the abs page (for citation in briefings).
  pub abs_url: String,
  pub title: String,
  pub summary: String,
  pub authors: Vec<String>,
  pub published: DateTime<Utc>,
}

/// Fetch up to `max_results` most-recently-submitted papers in `category`
/// (e.g. `cs.AI`, `cs.CL`, `math.ST`). The arxiv search API has no auth.
pub async fn fetch_recent(category: &str, max_results: u32) -> Result<Vec<Paper>> {
  let url = format!(
    "http://export.arxiv.org/api/query?search_query=cat:{cat}&start=0&max_results={n}&sortBy=submittedDate&sortOrder=descending",
    cat = category,
    n = max_results
  );
  debug!(url = %url, "GET arxiv");

  // `.no_proxy()` — see CLAUDE.md note. Avoids the macOS Clash/V2Ray
  // loopback footgun even though we're talking to a public endpoint
  // (some proxy configs intercept all outbound HTTPS).
  let client = reqwest::Client::builder()
    .no_proxy()
    .build()
    .context("build reqwest client")?;
  let xml = client
    .get(&url)
    .send()
    .await
    .with_context(|| format!("GET {url}"))?
    .error_for_status()
    .context("arxiv API returned non-success status")?
    .text()
    .await
    .context("read response body")?;

  let feed: AtomFeed =
    quick_xml::de::from_str(&xml).context("parse arxiv Atom XML; format changed?")?;

  let papers = feed
    .entries
    .into_iter()
    .filter_map(AtomEntry::into_paper)
    .collect::<Vec<_>>();

  info!(
    category = %category,
    fetched = papers.len(),
    "arxiv fetch complete"
  );
  Ok(papers)
}

// ── Atom subset ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AtomFeed {
  #[serde(rename = "entry", default)]
  entries: Vec<AtomEntry>,
}

#[derive(Debug, Deserialize)]
struct AtomEntry {
  id: String,
  title: String,
  summary: String,
  published: String,
  #[serde(rename = "author", default)]
  authors: Vec<AtomAuthor>,
}

#[derive(Debug, Deserialize)]
struct AtomAuthor {
  name: String,
}

impl AtomEntry {
  fn into_paper(self) -> Option<Paper> {
    let abs_url = self.id.trim().to_string();
    let paper_id = extract_paper_id(&abs_url)?;
    let published = chrono::DateTime::parse_from_rfc3339(self.published.trim())
      .ok()?
      .with_timezone(&Utc);
    Some(Paper {
      paper_id,
      abs_url,
      title: normalize_whitespace(&self.title),
      summary: normalize_whitespace(&self.summary),
      authors: self.authors.into_iter().map(|a| a.name).collect(),
      published,
    })
  }
}

/// Pull the arxiv id (e.g. `2501.12345`) out of an abs URL, stripping
/// the version suffix so re-submissions don't appear as "new" papers.
fn extract_paper_id(abs_url: &str) -> Option<String> {
  let prefixes = ["http://arxiv.org/abs/", "https://arxiv.org/abs/"];
  let id_with_version = prefixes.iter().find_map(|p| abs_url.strip_prefix(p))?;
  // Strip `vN` suffix (one or more digits) if present.
  let id = match id_with_version.rsplit_once('v') {
    Some((id, ver)) if ver.chars().all(|c| c.is_ascii_digit()) && !ver.is_empty() => id,
    _ => id_with_version,
  };
  Some(id.to_string())
}

/// Atom titles / abstracts often contain newlines + repeated whitespace
/// from prose wrapping. Collapse to single spaces so the LLM prompt
/// doesn't waste tokens on layout noise.
fn normalize_whitespace(s: &str) -> String {
  s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn extract_paper_id_strips_prefix_and_version() {
    assert_eq!(
      extract_paper_id("http://arxiv.org/abs/2501.12345v1"),
      Some("2501.12345".to_string())
    );
    assert_eq!(
      extract_paper_id("https://arxiv.org/abs/2501.12345v23"),
      Some("2501.12345".to_string())
    );
    assert_eq!(
      extract_paper_id("http://arxiv.org/abs/2501.12345"),
      Some("2501.12345".to_string())
    );
    // Old-style IDs (math.AT/0512345v1) — we keep the full path
    // including category, strip only the version.
    assert_eq!(
      extract_paper_id("http://arxiv.org/abs/math.AT/0512345v2"),
      Some("math.AT/0512345".to_string())
    );
    // Not an arxiv abs URL — return None.
    assert_eq!(extract_paper_id("https://example.com/2501.12345"), None);
  }

  #[test]
  fn normalize_whitespace_collapses_newlines_and_spaces() {
    assert_eq!(
      normalize_whitespace("Hello\n  world\n\tfrom\n  arxiv"),
      "Hello world from arxiv"
    );
    assert_eq!(normalize_whitespace(""), "");
    assert_eq!(normalize_whitespace("   "), "");
  }

  /// Parse a synthetic Atom feed shaped like the real arxiv response.
  #[test]
  fn parse_minimal_atom_feed() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2501.99999v1</id>
    <title>A Paper About Things</title>
    <summary>This paper describes things.</summary>
    <published>2026-05-17T12:34:56Z</published>
    <author><name>Alice Researcher</name></author>
    <author><name>Bob Scientist</name></author>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/2501.88888v3</id>
    <title>Another\n  Paper</title>
    <summary>Another\n\tsummary.</summary>
    <published>2026-05-16T10:00:00Z</published>
    <author><name>Carol</name></author>
  </entry>
</feed>"#;
    let feed: AtomFeed = quick_xml::de::from_str(xml).expect("parse atom");
    assert_eq!(feed.entries.len(), 2);
    let papers: Vec<_> = feed
      .entries
      .into_iter()
      .filter_map(AtomEntry::into_paper)
      .collect();
    assert_eq!(papers.len(), 2);
    assert_eq!(papers[0].paper_id, "2501.99999");
    assert_eq!(papers[0].authors.len(), 2);
    assert_eq!(papers[0].title, "A Paper About Things");
    assert_eq!(papers[1].paper_id, "2501.88888");
    assert_eq!(papers[1].authors, vec!["Carol".to_string()]);
  }
}
