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
            let latex_info = self.download_and_extract_latex(&paper_info.paper_id, version).await?;
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

        Ok(outputs)
    }
}

impl ArxivNode {
    fn resolve_arxiv_url(&self, inputs: &AsyncNodeInputs) -> Result<String, AgentFlowError> {
        let mut url = self.url.clone();
        for (key, value) in inputs {
            let placeholder = format!("{{{{{}}}}}", key);
            if url.contains(&placeholder) {
                let replacement = match value {
                    FlowValue::Json(Value::String(s)) => s.clone(),
                    FlowValue::Json(v) => v.to_string().trim_matches('"').to_string(),
                    FlowValue::File { path, .. } => path.to_string_lossy().to_string(),
                    FlowValue::Url { url, .. } => url.clone(),
                };
                url = url.replace(&placeholder, &replacement);
            }
        }
        Ok(url)
    }

    async fn fetch_arxiv_paper(&self, url: &str) -> Result<ArxivPaper, AgentFlowError> {
        let re = Regex::new(r"arxiv\.org/(?:abs|pdf)/(\d{4}\.\d{4,5})(?:v(\d+))?").unwrap();
        let caps = re.captures(url).ok_or_else(|| AgentFlowError::NodeInputError {
            message: format!("Invalid arXiv URL: {}", url),
        })?;

        let paper_id = caps.get(1).unwrap().as_str().to_string();
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
        let tag_re = Regex::new(r"\\[a-zA-Z@]+\s*(?:\\[.?\])?\s*(?:\{.*?\})?").unwrap();

        let no_comments = comment_re.replace_all(latex, "");
        let no_begin = begin_re.replace_all(&no_comments, "");
        let no_end = end_re.replace_all(&no_begin, "");
        let no_tags = tag_re.replace_all(&no_end, "");
        no_tags.trim().to_string()
    }
}
