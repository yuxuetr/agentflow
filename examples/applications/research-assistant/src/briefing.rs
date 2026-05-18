//! One-shot LLM call: given a batch of new arxiv papers, produce a
//! markdown briefing that groups them, calls out the most interesting
//! ones, and cross-references obvious clusters. No ReAct, no tools —
//! just `LlmInit::model(...).prompt(...).execute()` (see A7
//! changelog-writer for the same shape; it's the right tier per the
//! L1+L3 reflection rule).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::arxiv_fetch::Paper;

/// Render the briefing prompt from the new paper batch.
pub fn build_prompt(category: &str, since: Option<DateTime<Utc>>, papers: &[Paper]) -> String {
  let mut prompt = String::new();
  prompt.push_str(&format!(
    "You are a research briefing assistant. Below are the latest NEW papers from arxiv category `{category}`"
  ));
  if let Some(since) = since {
    prompt.push_str(&format!(" since {}", since.format("%Y-%m-%d")));
  }
  prompt.push_str(" (already-seen papers have been filtered out — every paper here is new).\n\n");
  prompt.push_str(
    "Produce a markdown briefing with this exact structure:\n\
     \n\
     # Arxiv Briefing — `{cat}` ({n} new papers)\n\
     \n\
     ## 🌟 Highlights\n\
     - 2-4 bullet points calling out the most interesting / impactful papers, \
       with one-sentence why-it-matters per bullet. Reference papers by title \
       + abs URL.\n\
     \n\
     ## 📚 All new papers\n\
     For each paper, render this block:\n\
     \n\
     ### <title>\n\
     - **Authors**: <comma-separated list, truncate to first 3 + et al. if more>\n\
     - **Published**: <YYYY-MM-DD>\n\
     - **Link**: <abs_url>\n\
     - <2-3 sentence plain-English summary of the abstract — NOT the abstract verbatim>\n\
     \n\
     ## 🔗 Clusters\n\
     If 2+ papers share a clear theme (same method, same dataset, same problem), \
     group them in a one-line cluster note: '- **<theme>**: <paper1>, <paper2>'. \
     Skip if no obvious clusters.\n\
     \n\
     Rules:\n\
     - Use ONLY the data below — no fabrication, no external knowledge about authors.\n\
     - Skip the Highlights section if everything is routine / incremental.\n\
     - Skip Clusters section if nothing groups.\n\
     - Output ONLY the markdown briefing, no preamble or explanation.\n\
     \n\
     Papers:\n\n",
  );

  let header = format!("{{cat}} = {category}, {{n}} = {}", papers.len());
  // Replace template placeholders in the instructions block.
  let prompt = prompt
    .replace("{cat}", category)
    .replace("{n}", &papers.len().to_string());
  // Note: above replace also touched the header above, fine.
  let _ = header;

  let mut out = prompt;
  for (i, p) in papers.iter().enumerate() {
    out.push_str(&format!(
      "### Paper {i_plus_1}\n- paper_id: {pid}\n- title: {title}\n- authors: {auths}\n- published: {pub}\n- abs_url: {url}\n- abstract: {abs}\n\n",
      i_plus_1 = i + 1,
      pid = p.paper_id,
      title = p.title,
      auths = p.authors.join(", "),
      pub = p.published.format("%Y-%m-%d"),
      url = p.abs_url,
      abs = p.summary
    ));
  }
  out
}

/// Run the one-shot LLM call to produce the briefing markdown.
pub async fn render_briefing(
  category: &str,
  papers: &[Paper],
  model: &str,
  since: Option<DateTime<Utc>>,
) -> Result<String> {
  let prompt = build_prompt(category, since, papers);
  let response = agentflow_llm::AgentFlow::model(model)
    .prompt(&prompt)
    .execute()
    .await
    .with_context(|| format!("LLM briefing call (model {model})"))?;
  Ok(response.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn dummy_paper(id: &str, title: &str) -> Paper {
    Paper {
      paper_id: id.into(),
      abs_url: format!("http://arxiv.org/abs/{id}"),
      title: title.into(),
      summary: "A clever new method that does things.".into(),
      authors: vec!["Author A".into(), "Author B".into()],
      published: chrono::Utc::now(),
    }
  }

  #[test]
  fn prompt_substitutes_category_and_count() {
    let papers = vec![dummy_paper("2501.00001", "Foo")];
    let prompt = build_prompt("cs.AI", None, &papers);
    assert!(prompt.contains("`cs.AI`"));
    assert!(prompt.contains("(1 new papers)"));
    assert!(prompt.contains("2501.00001"));
    assert!(prompt.contains("Foo"));
  }

  #[test]
  fn prompt_includes_since_when_provided() {
    let since = chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
      .unwrap()
      .with_timezone(&Utc);
    let prompt = build_prompt("cs.AI", Some(since), &[]);
    assert!(prompt.contains("since 2026-05-01"));
  }

  #[test]
  fn empty_papers_still_produces_well_formed_prompt() {
    let prompt = build_prompt("cs.AI", None, &[]);
    assert!(prompt.contains("(0 new papers)"));
    assert!(prompt.contains("Papers:"));
  }
}
