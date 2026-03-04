use serde_json::Value;

/// A parsed LLM response in the ReAct loop.
#[derive(Debug, Clone)]
pub enum AgentResponse {
    /// The model wants to call a tool.
    Action {
        thought: String,
        tool: String,
        params: Value,
    },
    /// The model has a final answer.
    Answer {
        thought: String,
        answer: String,
    },
    /// The model responded with text that could not be parsed as JSON.
    /// Treated as a final answer so the loop terminates gracefully.
    Malformed(String),
}

impl AgentResponse {
    /// Parse a raw LLM response string into an `AgentResponse`.
    ///
    /// Handles:
    /// - JSON objects directly in the response
    /// - JSON wrapped in ` ```json … ``` ` code fences
    /// - Plain text (returned as `Malformed`)
    pub fn parse(text: &str) -> Self {
        let json_str = extract_json(text);

        match serde_json::from_str::<Value>(&json_str) {
            Ok(v) => {
                let thought = v["thought"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();

                // Check for "action" key → tool call
                if let Some(action) = v.get("action") {
                    let tool = action["tool"].as_str().unwrap_or("").to_string();
                    let params = action.get("params").cloned().unwrap_or(Value::Object(
                        serde_json::Map::new(),
                    ));
                    if !tool.is_empty() {
                        return Self::Action {
                            thought,
                            tool,
                            params,
                        };
                    }
                }

                // Check for "answer" key → final answer
                if let Some(answer_val) = v.get("answer") {
                    let answer = match answer_val {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    return Self::Answer { thought, answer };
                }

                // JSON parsed but has neither "action" nor "answer" key
                Self::Malformed(text.to_string())
            }
            Err(_) => Self::Malformed(text.to_string()),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Answer { .. } | Self::Malformed(_))
    }
}

/// Try to pull a JSON object out of `text`, stripping markdown code fences if present.
fn extract_json(text: &str) -> String {
    // Strip ```json ... ``` blocks
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Strip ``` ... ``` blocks
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Find the outermost { … } span
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                return text[start..=end].to_string();
            }
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_action() {
        let raw = r#"{"thought": "I need to check the date", "action": {"tool": "shell", "params": {"command": "date"}}}"#;
        match AgentResponse::parse(raw) {
            AgentResponse::Action { thought, tool, .. } => {
                assert_eq!(thought, "I need to check the date");
                assert_eq!(tool, "shell");
            }
            other => panic!("Expected Action, got {:?}", other),
        }
    }

    #[test]
    fn parses_answer() {
        let raw = r#"{"thought": "I have enough info", "answer": "The answer is 42"}"#;
        match AgentResponse::parse(raw) {
            AgentResponse::Answer { answer, .. } => {
                assert_eq!(answer, "The answer is 42");
            }
            other => panic!("Expected Answer, got {:?}", other),
        }
    }

    #[test]
    fn parses_json_in_code_fence() {
        let raw = "Here is my response:\n```json\n{\"thought\": \"done\", \"answer\": \"ok\"}\n```";
        match AgentResponse::parse(raw) {
            AgentResponse::Answer { answer, .. } => assert_eq!(answer, "ok"),
            other => panic!("Expected Answer, got {:?}", other),
        }
    }
}
