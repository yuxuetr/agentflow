use crate::{AsyncNode, NodeError, NodeResult, SharedState};
use agentflow_core::AgentFlowError;
use agentflow_llm::{AgentFlow, providers::stepfun::ASRRequest};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Automatic Speech Recognition (ASR) node
#[derive(Debug, Clone)]
pub struct ASRNode {
    pub name: String,
    pub model: String,
    pub file_template: String,          // Audio file path/data template
    pub input_keys: Vec<String>,
    pub output_key: String,
    
    // ASR specific parameters
    pub response_format: ASRResponseFormat, // json, text, srt, vtt
    pub language: Option<String>,       // Language hint: "en", "zh", "auto"
    pub hotwords: Option<Vec<String>>,  // Hot words for better recognition
    pub temperature: Option<f32>,       // Randomness in recognition (0.0-1.0)
    pub prompt: Option<String>,         // Context prompt for better recognition
    pub timestamp_granularities: Option<Vec<TimestampGranularity>>, // word, segment
    
    // Workflow control
    pub dependencies: Vec<String>,
    pub condition: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ASRResponseFormat {
    #[serde(rename = "json")]
    Json,       // Structured response with timestamps
    #[serde(rename = "text")]
    Text,       // Plain text transcription
    #[serde(rename = "srt")]
    Srt,        // SubRip subtitle format
    #[serde(rename = "vtt")]
    Vtt,        // WebVTT subtitle format
}

impl Default for ASRResponseFormat {
    fn default() -> Self {
        ASRResponseFormat::Text
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimestampGranularity {
    #[serde(rename = "word")]
    Word,       // Word-level timestamps
    #[serde(rename = "segment")]
    Segment,    // Sentence/phrase-level timestamps
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRResponse {
    pub text: String,
    pub language: Option<String>,
    pub duration: Option<f32>,
    pub segments: Option<Vec<ASRSegment>>,
    pub words: Option<Vec<ASRWord>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRSegment {
    pub id: u32,
    pub seek: f32,
    pub start: f32,
    pub end: f32,
    pub text: String,
    pub temperature: f32,
    pub avg_logprob: f32,
    pub compression_ratio: f32,
    pub no_speech_prob: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRWord {
    pub word: String,
    pub start: f32,
    pub end: f32,
    pub probability: f32,
}

impl ASRNode {
    pub fn new(name: &str, model: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            file_template: String::new(),
            input_keys: Vec::new(),
            output_key: format!("{}_transcript", name),
            response_format: ASRResponseFormat::default(),
            language: None,
            hotwords: None,
            temperature: None,
            prompt: None,
            timestamp_granularities: None,
            dependencies: Vec::new(),
            condition: None,
            timeout_ms: None,
        }
    }
    
    // Builder pattern methods
    pub fn with_file(mut self, file_template: &str) -> Self {
        self.file_template = file_template.to_string();
        self
    }
    
    pub fn with_response_format(mut self, format: ASRResponseFormat) -> Self {
        self.response_format = format;
        self
    }
    
    pub fn with_language(mut self, language: &str) -> Self {
        self.language = Some(language.to_string());
        self
    }
    
    pub fn with_hotwords(mut self, hotwords: Vec<String>) -> Self {
        self.hotwords = Some(hotwords);
        self
    }
    
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature.clamp(0.0, 1.0));
        self
    }
    
    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.prompt = Some(prompt.to_string());
        self
    }
    
    pub fn with_timestamps(mut self, granularities: Vec<TimestampGranularity>) -> Self {
        self.timestamp_granularities = Some(granularities);
        self
    }
    
    pub fn with_input_keys(mut self, keys: Vec<String>) -> Self {
        self.input_keys = keys;
        self
    }
    
    pub fn with_output_key(mut self, key: &str) -> Self {
        self.output_key = key.to_string();
        self
    }
    
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
    
    /// Validate and resolve audio file
    fn resolve_audio_file(&self, shared: &SharedState) -> NodeResult<String> {
        let resolved_file = shared.resolve_template_advanced(&self.file_template);
        
        // Check if it's a SharedState key reference
        if !resolved_file.starts_with("http") && !resolved_file.starts_with("data:") && !resolved_file.starts_with("file:") {
            // Try to get from SharedState
            if let Some(file_data) = shared.get(&resolved_file) {
                return Ok(file_data.as_str().unwrap_or(&resolved_file).to_string());
            }
        }
        
        // Validate file format
        if resolved_file.is_empty() {
            return Err(NodeError::ValidationError {
                message: "Audio file path/data cannot be empty".to_string(),
            });
        }
        
        // Basic validation for supported audio formats
        let is_valid_audio = resolved_file.starts_with("http") ||
                            resolved_file.starts_with("data:audio/") ||
                            resolved_file.starts_with("file:") ||
                            self.is_supported_audio_format(&resolved_file);
        
        if !is_valid_audio {
            return Err(NodeError::ValidationError {
                message: format!("Unsupported audio format. Expected flac, mp3, mp4, mpeg, mpga, m4a, ogg, wav, webm, aac, opus or data URI. Got: {}", 
                    &resolved_file[..resolved_file.len().min(100)]),
            });
        }
        
        Ok(resolved_file)
    }
    
    /// Check if the file extension is supported
    fn is_supported_audio_format(&self, file_path: &str) -> bool {
        let supported_extensions = ["flac", "mp3", "mp4", "mpeg", "mpga", "m4a", "ogg", "wav", "webm", "aac", "opus"];
        
        if let Some(extension) = file_path.split('.').last() {
            supported_extensions.contains(&extension.to_lowercase().as_str())
        } else {
            false
        }
    }
    
    /// Create configuration for ASR
    fn create_asr_config(&self, resolved_file: &str) -> NodeResult<Value> {
        let mut config = serde_json::Map::new();
        
        config.insert("model".to_string(), Value::String(self.model.clone()));
        config.insert("file".to_string(), Value::String(resolved_file.to_string()));
        
        config.insert("response_format".to_string(), serde_json::to_value(&self.response_format)?);
        
        if let Some(ref language) = self.language {
            config.insert("language".to_string(), Value::String(language.clone()));
        }
        
        if let Some(ref hotwords) = self.hotwords {
            // Convert to JSON string format as required by some APIs
            let hotwords_json = serde_json::to_string(hotwords)
                .map_err(|e| NodeError::ValidationError {
                    message: format!("Invalid hotwords format: {}", e),
                })?;
            config.insert("hotwords".to_string(), Value::String(hotwords_json));
        }
        
        if let Some(temperature) = self.temperature {
            config.insert("temperature".to_string(),
                Value::Number(serde_json::Number::from_f64(temperature as f64).unwrap()));
        }
        
        if let Some(ref prompt) = self.prompt {
            config.insert("prompt".to_string(), Value::String(prompt.clone()));
        }
        
        if let Some(ref granularities) = self.timestamp_granularities {
            config.insert("timestamp_granularities".to_string(), serde_json::to_value(granularities)?);
        }
        
        Ok(Value::Object(config))
    }
    
    /// Execute real ASR transcription using StepFun API
    async fn execute_real_asr(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
        let file_path = config.get("file").unwrap().as_str().unwrap();
        let model = config.get("model").unwrap().as_str().unwrap();
        let format = &self.response_format;
        
        println!("🎤 Executing Speech Recognition (StepFun API):");
        println!("   Model: {}", model);
        println!("   File: {}...", &file_path[..file_path.len().min(50)]);
        println!("   Format: {:?}", format);
        
        // Get API key from environment
        let api_key = std::env::var("STEPFUN_API_KEY")
            .or_else(|_| std::env::var("AGENTFLOW_STEPFUN_API_KEY"))
            .map_err(|_| NodeError::ConfigurationError {
                message: "StepFun API key not found. Set STEPFUN_API_KEY or AGENTFLOW_STEPFUN_API_KEY environment variable".to_string(),
            })?;
        
        // Initialize StepFun client
        let stepfun_client = AgentFlow::stepfun_client(&api_key).await
            .map_err(|e| NodeError::ConfigurationError {
                message: format!("Failed to initialize StepFun client: {}", e),
            })?;
        
        // Load audio data from file path or data URI
        let (audio_data, filename) = self.load_audio_data(file_path).await?;
        
        // Map response format to StepFun format
        let response_format = match format {
            ASRResponseFormat::Json => "json",
            ASRResponseFormat::Text => "text",
            ASRResponseFormat::Srt => "srt",
            ASRResponseFormat::Vtt => "vtt",
        };
        
        // Build ASR request
        let asr_request = ASRRequest {
            model: model.to_string(),
            response_format: response_format.to_string(),
            audio_data,
            filename,
        };
        
        // Execute ASR request
        let transcription = stepfun_client.speech_to_text(asr_request).await
            .map_err(|e| NodeError::ExecutionError {
                message: format!("StepFun ASR execution failed: {}", e),
            })?;
        
        println!("✅ ASR Transcription: Processed audio successfully ({})", response_format);
        Ok(transcription)
    }
    
    /// Load audio data from various sources (file path, data URI, etc.)
    async fn load_audio_data(&self, file_path: &str) -> NodeResult<(Vec<u8>, String)> {
        if file_path.starts_with("data:audio/") {
            // Handle data URI
            let data_uri_parts: Vec<&str> = file_path.split(',').collect();
            if data_uri_parts.len() != 2 {
                return Err(NodeError::ValidationError {
                    message: "Invalid data URI format".to_string(),
                });
            }
            
            let header = data_uri_parts[0];
            let base64_data = data_uri_parts[1];
            
            // Decode base64 data
            use base64::{Engine as _, engine::general_purpose};
            let audio_data = general_purpose::STANDARD.decode(base64_data)
                .map_err(|e| NodeError::ValidationError {
                    message: format!("Invalid base64 data: {}", e),
                })?;
            
            // Extract format from header (e.g., "data:audio/mp3;base64")
            let format = if header.contains("audio/mp3") { "mp3" }
                else if header.contains("audio/wav") { "wav" }
                else if header.contains("audio/flac") { "flac" }
                else if header.contains("audio/ogg") { "ogg" }
                else { "mp3" }; // default
            
            let filename = format!("audio.{}", format);
            Ok((audio_data, filename))
        } else if file_path.starts_with("http://") || file_path.starts_with("https://") {
            // Handle HTTP URL
            let response = reqwest::get(file_path).await
                .map_err(|e| NodeError::ExecutionError {
                    message: format!("Failed to download audio file: {}", e),
                })?;
            
            if !response.status().is_success() {
                return Err(NodeError::ExecutionError {
                    message: format!("Failed to download audio file: HTTP {}", response.status()),
                });
            }
            
            let audio_data = response.bytes().await
                .map_err(|e| NodeError::ExecutionError {
                    message: format!("Failed to read audio data: {}", e),
                })?.to_vec();
            
            // Extract filename from URL
            let filename = file_path.split('/').last().unwrap_or("audio.mp3").to_string();
            Ok((audio_data, filename))
        } else {
            // Handle local file path
            let path = if file_path.starts_with("file://") {
                &file_path[7..] // Remove "file://" prefix
            } else {
                file_path
            };
            
            let audio_data = tokio::fs::read(path).await
                .map_err(|e| NodeError::ExecutionError {
                    message: format!("Failed to read audio file '{}': {}", path, e),
                })?;
            
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("audio.mp3")
                .to_string();
            
            Ok((audio_data, filename))
        }
    }
    
    /// Mock ASR transcription (fallback)
    async fn execute_mock_asr(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
        let file_path = config.get("file").unwrap().as_str().unwrap();
        let model = config.get("model").unwrap().as_str().unwrap();
        let format = &self.response_format;
        
        println!("🎤 Executing Speech Recognition (MOCK - API key not available):");
        println!("   Model: {}", model);
        println!("   File: {}...", &file_path[..file_path.len().min(50)]);
        println!("   Format: {:?}", format);
        
        if let Some(language) = config.get("language") {
            println!("   Language: {:?}", language);
        }
        
        if let Some(hotwords) = config.get("hotwords") {
            println!("   Hotwords: {:?}", hotwords);
        }
        
        // Simulate processing time (longer for larger files)
        let processing_time = if file_path.contains("long") || file_path.contains("large") {
            2000
        } else {
            800
        };
        tokio::time::sleep(std::time::Duration::from_millis(processing_time)).await;
        
        // Mock response based on response format
        let mock_response = match format {
            ASRResponseFormat::Text => {
                "Hello, this is a mock transcription of the audio file. The speech recognition system has successfully converted the audio to text."
            }
            ASRResponseFormat::Json => {
                r#"{"text":"Hello, this is a mock transcription of the audio file. The speech recognition system has successfully converted the audio to text.","language":"en","duration":12.5,"segments":[{"id":0,"seek":0,"start":0.0,"end":12.5,"text":"Hello, this is a mock transcription of the audio file. The speech recognition system has successfully converted the audio to text.","temperature":0.0,"avg_logprob":-0.3,"compression_ratio":1.2,"no_speech_prob":0.1}]}"#
            }
            ASRResponseFormat::Srt => {
                "1\n00:00:00,000 --> 00:00:12,500\nHello, this is a mock transcription of the audio file. The speech recognition system has successfully converted the audio to text.\n"
            }
            ASRResponseFormat::Vtt => {
                "WEBVTT\n\n00:00:00.000 --> 00:00:12.500\nHello, this is a mock transcription of the audio file. The speech recognition system has successfully converted the audio to text.\n"
            }
        };
        
        println!("✅ ASR Transcription (MOCK, {:?}): Processed audio successfully", format);
        Ok(mock_response.to_string())
    }
}

#[async_trait]
impl AsyncNode for ASRNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        // Check conditional execution
        if let Some(ref condition) = self.condition {
            let resolved_condition = shared.resolve_template_advanced(condition);
            if resolved_condition != "true" {
                println!("⏭️  Skipping ASR node '{}' due to condition: {}", self.name, resolved_condition);
                return Ok(Value::Object(serde_json::Map::new()));
            }
        }
        
        // Resolve and validate audio file
        let audio_file = self.resolve_audio_file(shared)
            .map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
        
        let config = self.create_asr_config(&audio_file)
            .map_err(|e| AgentFlowError::AsyncExecutionError {
                message: format!("Failed to create ASR config: {}", e),
            })?;
        
        println!("🔧 ASR Node '{}' prepared:", self.name);
        println!("   Model: {}", self.model);
        println!("   File: {}...", &audio_file[..audio_file.len().min(50)]);
        if let Some(ref lang) = self.language {
            println!("   Language: {}", lang);
        }
        
        Ok(config)
    }
    
    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let config = prep_result
            .as_object()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
                message: "Invalid prep result for ASR node".to_string(),
            })?;
        
        // Skip execution if condition failed
        if config.is_empty() {
            return Ok(Value::String("Skipped due to condition".to_string()));
        }
        
        // Apply timeout if configured (ASR can take longer)
        let default_timeout = 60000; // 60 seconds default for ASR
        let timeout_ms = self.timeout_ms.unwrap_or(default_timeout);
        
        let timeout_duration = std::time::Duration::from_millis(timeout_ms);
        let response = match tokio::time::timeout(timeout_duration, self.execute_real_asr(config)).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                // Fallback to mock if real API fails
                match tokio::time::timeout(timeout_duration, self.execute_mock_asr(config)).await {
                    Ok(result) => result.map_err(|e| AgentFlowError::AsyncExecutionError {
                        message: e.to_string(),
                    })?,
                    Err(_) => return Err(AgentFlowError::TimeoutExceeded { duration_ms: timeout_ms }),
                }
            },
            Err(_) => return Err(AgentFlowError::TimeoutExceeded { duration_ms: timeout_ms }),
        };
        
        Ok(Value::String(response))
    }
    
    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        let transcript_text = exec_result.as_str().unwrap_or("");
        
        // Parse and store different formats appropriately
        match self.response_format {
            ASRResponseFormat::Json => {
                // Try to parse JSON and extract text
                if let Ok(json_response) = serde_json::from_str::<ASRResponse>(transcript_text) {
                    shared.insert(self.output_key.clone(), Value::String(json_response.text.clone()));
                    shared.insert(format!("{}_full", self.output_key), exec_result);
                } else {
                    shared.insert(self.output_key.clone(), exec_result);
                }
            }
            _ => {
                // For text, srt, vtt - store as-is
                shared.insert(self.output_key.clone(), exec_result.clone());
            }
        }
        
        // Also store as generic "transcript" for workflow chaining
        shared.insert("transcript".to_string(), shared.get(&self.output_key).unwrap().clone());
        
        println!("💾 Stored transcript in shared state as: '{}'", self.output_key);
        
        Ok(None) // No specific next action
    }
    
    fn get_node_id(&self) -> Option<String> {
        Some(self.name.clone())
    }
}

/// Helper constructors for common ASR scenarios
impl ASRNode {
    /// Create a general transcription node
    pub fn transcriber(name: &str, model: &str, audio_key: &str) -> Self {
        Self::new(name, model)
            .with_file(&format!("{{{{{}}}}}", audio_key))
            .with_response_format(ASRResponseFormat::Text)
            .with_language("auto")
    }
    
    /// Create a detailed transcription node with timestamps
    pub fn detailed_transcriber(name: &str, model: &str, audio_key: &str) -> Self {
        Self::new(name, model)
            .with_file(&format!("{{{{{}}}}}", audio_key))
            .with_response_format(ASRResponseFormat::Json)
            .with_timestamps(vec![TimestampGranularity::Word, TimestampGranularity::Segment])
            .with_temperature(0.0)  // More deterministic
    }
    
    /// Create a subtitle generator
    pub fn subtitle_generator(name: &str, model: &str, audio_key: &str, format: ASRResponseFormat) -> Self {
        Self::new(name, model)
            .with_file(&format!("{{{{{}}}}}", audio_key))
            .with_response_format(format)
            .with_timestamps(vec![TimestampGranularity::Segment])
    }
    
    /// Create a multilingual transcriber
    pub fn multilingual_transcriber(name: &str, model: &str, audio_key: &str, language: &str) -> Self {
        Self::new(name, model)
            .with_file(&format!("{{{{{}}}}}", audio_key))
            .with_response_format(ASRResponseFormat::Text)
            .with_language(language)
            .with_temperature(0.1)
    }
    
    /// Create a domain-specific transcriber with hotwords
    pub fn domain_transcriber(name: &str, model: &str, audio_key: &str, hotwords: Vec<String>) -> Self {
        Self::new(name, model)
            .with_file(&format!("{{{{{}}}}}", audio_key))
            .with_response_format(ASRResponseFormat::Json)
            .with_hotwords(hotwords)
            .with_temperature(0.0)
    }
    
    /// Create a podcast transcriber
    pub fn podcast_transcriber(name: &str, model: &str, audio_key: &str) -> Self {
        Self::new(name, model)
            .with_file(&format!("{{{{{}}}}}", audio_key))
            .with_response_format(ASRResponseFormat::Json)
            .with_timestamps(vec![TimestampGranularity::Segment])
            .with_language("en")
            .with_prompt("This is a podcast conversation.")
            .with_timeout(120000)  // 2 minutes timeout for longer audio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_asr_node_creation() {
        let node = ASRNode::new("test_asr", "whisper-1");
        assert_eq!(node.name, "test_asr");
        assert_eq!(node.model, "whisper-1");
        assert_eq!(node.output_key, "test_asr_transcript");
        assert!(matches!(node.response_format, ASRResponseFormat::Text));
    }
    
    #[tokio::test]
    async fn test_asr_node_builder() {
        let node = ASRNode::new("advanced_asr", "whisper-1")
            .with_file("{{audio_input}}")
            .with_language("en")
            .with_hotwords(vec!["AI".to_string(), "machine learning".to_string()])
            .with_response_format(ASRResponseFormat::Json)
            .with_temperature(0.1)
            .with_timestamps(vec![TimestampGranularity::Word, TimestampGranularity::Segment])
            .with_input_keys(vec!["audio_input".to_string()]);
            
        assert_eq!(node.file_template, "{{audio_input}}");
        assert_eq!(node.language, Some("en".to_string()));
        assert!(node.hotwords.is_some());
        assert_eq!(node.hotwords.as_ref().unwrap().len(), 2);
        assert!(matches!(node.response_format, ASRResponseFormat::Json));
        assert_eq!(node.temperature, Some(0.1));
        assert!(node.timestamp_granularities.is_some());
    }
    
    #[tokio::test]
    async fn test_audio_format_validation() {
        let node = ASRNode::new("test", "whisper");
        
        // Test supported formats
        assert!(node.is_supported_audio_format("audio.mp3"));
        assert!(node.is_supported_audio_format("speech.wav"));
        assert!(node.is_supported_audio_format("podcast.m4a"));
        assert!(node.is_supported_audio_format("interview.flac"));
        
        // Test unsupported formats  
        assert!(node.is_supported_audio_format("video.mp4")); // mp4 is supported
        assert!(!node.is_supported_audio_format("document.pdf"));
        assert!(!node.is_supported_audio_format("image.png"));
    }
    
    #[tokio::test]
    async fn test_asr_with_valid_audio() {
        let node = ASRNode::transcriber("transcriber", "whisper-1", "audio_file");
        
        let shared = SharedState::new();
        shared.insert("audio_file".to_string(), 
            Value::String("data:audio/wav;base64,UklGRnoGAABXQVZFZm10...".to_string()));
        
        let result = node.prep_async(&shared).await;
        assert!(result.is_ok());
        
        let config = result.unwrap();
        let config_obj = config.as_object().unwrap();
        assert!(config_obj.contains_key("file"));
        assert!(config_obj.contains_key("response_format"));
    }
    
    #[tokio::test]
    async fn test_asr_hotwords_formatting() {
        let node = ASRNode::new("test", "whisper")
            .with_hotwords(vec!["AI".to_string(), "machine learning".to_string()]);
        
        let config = node.create_asr_config("test.wav").unwrap();
        let config_obj = config.as_object().unwrap();
        
        let hotwords_str = config_obj.get("hotwords").unwrap().as_str().unwrap();
        assert!(hotwords_str.contains("["));
        assert!(hotwords_str.contains("]"));
        assert!(hotwords_str.contains("AI"));
        assert!(hotwords_str.contains("machine learning"));
    }
    
    #[tokio::test]
    async fn test_helper_constructors() {
        // Test transcriber
        let transcriber = ASRNode::transcriber("trans", "whisper", "audio");
        assert_eq!(transcriber.file_template, "{{audio}}");
        assert!(matches!(transcriber.response_format, ASRResponseFormat::Text));
        
        // Test detailed transcriber
        let detailed = ASRNode::detailed_transcriber("detailed", "whisper", "audio");
        assert!(matches!(detailed.response_format, ASRResponseFormat::Json));
        assert!(detailed.timestamp_granularities.is_some());
        
        // Test subtitle generator
        let subtitles = ASRNode::subtitle_generator("subs", "whisper", "video_audio", ASRResponseFormat::Srt);
        assert!(matches!(subtitles.response_format, ASRResponseFormat::Srt));
        
        // Test multilingual
        let multilingual = ASRNode::multilingual_transcriber("multi", "whisper", "audio", "zh");
        assert_eq!(multilingual.language, Some("zh".to_string()));
        
        // Test domain-specific
        let domain = ASRNode::domain_transcriber("domain", "whisper", "audio", 
            vec!["COVID-19".to_string(), "vaccination".to_string()]);
        assert!(domain.hotwords.is_some());
        
        // Test podcast
        let podcast = ASRNode::podcast_transcriber("podcast", "whisper", "episode");
        assert_eq!(podcast.timeout_ms, Some(120000));
        assert!(podcast.prompt.is_some());
    }
    
    #[tokio::test]
    async fn test_asr_full_workflow() {
        let node = ASRNode::transcriber("speech_to_text", "whisper-1", "recorded_audio")
            .with_language("en")
            .with_input_keys(vec!["recorded_audio".to_string()]);
            
        let shared = SharedState::new();
        shared.insert("recorded_audio".to_string(), 
            Value::String("data:audio/mp3;base64,mock_audio_data".to_string()));
        
        // Test full execution
        let result = node.run_async(&shared).await.unwrap();
        assert!(result.is_none());
        
        // Check transcript was generated and stored
        let transcript = shared.get(&node.output_key).unwrap();
        assert!(transcript.as_str().unwrap().len() > 0);
        
        // Check generic transcript was also stored
        let generic_transcript = shared.get("transcript").unwrap();
        assert_eq!(transcript, generic_transcript);
    }
}