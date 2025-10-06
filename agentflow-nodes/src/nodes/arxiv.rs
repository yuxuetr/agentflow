use crate::common::utils::flow_value_to_string;
use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use regex::Regex;
use flate2::read::GzDecoder;
use std::io::Read;
use tar::Archive;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivNode {
    pub name: String,
    pub url: String,
    pub fetch_source: Option<bool>,
    pub simplify_latex: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ArxivPaper {
    pub paper_id: String,
    pub version: Option<u32>,
}

#[derive(Debug)]
pub struct LatexSource {
    pub main_content: String,
    pub expanded_content: Option<String>,
}

#[async_trait]
impl AsyncNode for ArxivNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let resolved_url = self.resolve_arxiv_url(inputs)?;
        let paper_info = self.fetch_arxiv_paper(&resolved_url).await?;

        let mut outputs = HashMap::new();
        let source_url = format!("https://arxiv.org/abs/{}", paper_info.paper_id);
        let version = paper_info.version.unwrap_or(1);

        outputs.insert("paper_id".to_string(), FlowValue::Json(Value::String(paper_info.paper_id.clone())));
        if let Some(version) = paper_info.version {
            outputs.insert("version".to_string(), FlowValue::Json(Value::String(version.to_string())));
        }
        outputs.insert("source_url".to_string(), FlowValue::Json(Value::String(source_url)));
        outputs.insert("original_url".to_string(), FlowValue::Json(Value::String(resolved_url.clone())));

        if self.fetch_source.unwrap_or(false) {
            match self.download_and_extract_latex(&paper_info.paper_id, version).await {
                Ok(latex_info) => {
                    if let Some(expanded_content) = latex_info.expanded_content {
                        outputs.insert("expanded_content".to_string(), FlowValue::Json(Value::String(expanded_content)));
                    }

                    if self.simplify_latex.unwrap_or(false) {
                        let simple_latex_content = self.simplify_latex_content(&latex_info.main_content);
                        outputs.insert("simple_latex_content".to_string(), FlowValue::Json(Value::String(simple_latex_content)));
                    }

                    // Insert main_content last after it has been borrowed
                    outputs.insert("main_content".to_string(), FlowValue::Json(Value::String(latex_info.main_content)));
                }
                Err(e) => {
                    // Paper doesn't have LaTeX source available, insert empty strings
                    println!("⚠️  Warning: Could not fetch LaTeX source for paper {}: {}", paper_info.paper_id, e);
                    outputs.insert("simple_latex_content".to_string(), FlowValue::Json(Value::String(String::new())));
                    outputs.insert("main_content".to_string(), FlowValue::Json(Value::String(String::new())));
                }
            }
        }

        Ok(outputs)
    }
}

impl ArxivNode {
    fn resolve_arxiv_url(&self, inputs: &AsyncNodeInputs) -> Result<String, AgentFlowError> {
        let mut url = self.url.clone();
        for (key, value) in inputs {
            // Support both {{ key }} and {{key}}
            let placeholder_with_spaces = format!("{{{{ {} }}}}", key);
            let placeholder_without_spaces = format!("{{{{{}}}}}", key);
            let value_str = flow_value_to_string(value);

            if url.contains(&placeholder_with_spaces) {
                url = url.replace(&placeholder_with_spaces, &value_str);
            }
            if url.contains(&placeholder_without_spaces) {
                url = url.replace(&placeholder_without_spaces, &value_str);
            }
        }
        Ok(url)
    }

    async fn fetch_arxiv_paper(&self, url: &str) -> Result<ArxivPaper, AgentFlowError> {
        // Check if it's a valid arXiv URL
        let re = Regex::new(r"arxiv\.org/(?:abs|pdf)/(\d{4}\.\d{4,5})(?:v(\d+))?").unwrap();

        if let Some(caps) = re.captures(url) {
            let paper_id = caps.get(1).map(|m| m.as_str().to_string()).ok_or_else(|| AgentFlowError::NodeInputError {
                message: "Could not parse paper ID from arXiv URL".to_string(),
            })?;
            let version = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
            return Ok(ArxivPaper { paper_id, version });
        }

        // If not a URL, treat as search query
        self.search_arxiv(url).await
    }

    async fn search_arxiv(&self, query: &str) -> Result<ArxivPaper, AgentFlowError> {
        // Use arXiv API to search for papers
        let encoded_query = urlencoding::encode(query);
        let api_url = format!("http://export.arxiv.org/api/query?search_query=all:{}&start=0&max_results=1", encoded_query);

        let response = reqwest::get(&api_url).await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to search arXiv: {}", e),
        })?;

        let body = response.text().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to read arXiv search response: {}", e),
        })?;

        // Parse XML response to extract paper ID
        // Look for <id>http://arxiv.org/abs/XXXX.XXXXX</id>
        let id_re = Regex::new(r"<id>http://arxiv\.org/abs/(\d{4}\.\d{4,5})(?:v(\d+))?</id>").unwrap();
        let caps = id_re.captures(&body).ok_or_else(|| AgentFlowError::NodeInputError {
            message: format!("No papers found for query: {}", query),
        })?;

        let paper_id = caps.get(1).map(|m| m.as_str().to_string()).ok_or_else(|| AgentFlowError::NodeInputError {
            message: "Could not parse paper ID from search results".to_string(),
        })?;
        let version = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());

        Ok(ArxivPaper { paper_id, version })
    }

    async fn download_and_extract_latex(&self, paper_id: &str, version: u32) -> Result<LatexSource, AgentFlowError> {
        let url = format!("https://arxiv.org/e-print/{v_id}v{v_num}", v_id = paper_id, v_num = version);
        let response = reqwest::get(&url).await.map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
        let compressed_bytes = response.bytes().await.map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
        let mut decoder = GzDecoder::new(&compressed_bytes[..]);
        let mut decompressed_data = Vec::new();
        decoder.read_to_end(&mut decompressed_data).map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
        let mut archive = Archive::new(&decompressed_data[..]);

        let mut main_content = String::new();
        let mut all_tex_files = String::new();

        for file in archive.entries().map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })? {
            let mut entry = file.map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
            let path = entry.path().map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
            if let Some(ext) = path.extension() {
                if ext == "tex" {
                    let mut content = String::new();
                    entry.read_to_string(&mut content).map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
                    all_tex_files.push_str(&content);
                    all_tex_files.push_str("\n\n"); // Separator

                    if content.contains(r"\\begin{document}") {
                        main_content = content;
                    }
                }
            }
        }

        if main_content.is_empty() {
            main_content = all_tex_files.clone(); // Fallback to all content
        }

        Ok(LatexSource { main_content, expanded_content: Some(all_tex_files) })
    }

    fn simplify_latex_content(&self, latex: &str) -> String {
        let comment_re = Regex::new(r"(?m)%.*$").unwrap();
        let begin_re = Regex::new(r"\\begin\{.*?\}").unwrap();
        let end_re = Regex::new(r"\\end\{.*?\}").unwrap();
        let tag_re = Regex::new(r"\\[a-zA-Z@]+\s*(?:\\\[.*?\])?\s*(?:\{.*?\})?").unwrap();

        let no_comments = comment_re.replace_all(latex, "");
        let no_begin = begin_re.replace_all(&no_comments, "");
        let no_end = end_re.replace_all(&no_begin, "");
        let no_tags = tag_re.replace_all(&no_end, "");
        no_tags.trim().to_string()
    }
}
