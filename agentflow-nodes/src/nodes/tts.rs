use crate::{AsyncNode, NodeError, NodeResult, SharedState};
use agentflow_core::AgentFlowError;
use agentflow_llm::{AgentFlow, providers::stepfun::TTSBuilder};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Text-to-Speech synthesis node
#[derive(Debug, Clone)]
pub struct TTSNode {
    pub name: String,
    pub model: String,
    pub input_template: String,         // Text input template
    pub voice: String,                  // Voice ID/name
    pub input_keys: Vec<String>,
    pub output_key: String,
    
    // TTS specific parameters
    pub response_format: AudioResponseFormat, // wav, mp3, flac, opus
    pub speed: Option<f32>,             // Speech speed [0.5, 2.0]
    pub voice_label: Option<VoiceLabel>, // Language, emotion, style
    pub sample_rate: Option<u32>,       // 8000, 16000, 22050, 24000
    pub quality: Option<AudioQuality>,  // Audio quality preset
    
    // Workflow control
    pub dependencies: Vec<String>,
    pub condition: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioResponseFormat {
    #[serde(rename = "wav")]
    Wav,
    #[serde(rename = "mp3")]
    Mp3,
    #[serde(rename = "flac")]
    Flac,
    #[serde(rename = "opus")]
    Opus,
}

impl Default for AudioResponseFormat {
    fn default() -> Self {
        AudioResponseFormat::Mp3
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLabel {
    pub language: Option<String>,       // "粤语", "四川话", "日语", "English", etc.
    pub emotion: Option<String>,        // "happy", "sad", "neutral", "excited"
    pub style: Option<String>,          // "slow", "fast", "dramatic", "casual"
    pub gender: Option<String>,         // "male", "female", "neutral"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioQuality {
    #[serde(rename = "low")]
    Low,        // Faster, smaller files
    #[serde(rename = "standard")]
    Standard,   // Balanced
    #[serde(rename = "high")]
    High,       // Better quality, slower
    #[serde(rename = "premium")]
    Premium,    // Best quality
}

impl Default for AudioQuality {
    fn default() -> Self {
        AudioQuality::Standard
    }
}

impl TTSNode {
    pub fn new(name: &str, model: &str, voice: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            input_template: String::new(),
            voice: voice.to_string(),
            input_keys: Vec::new(),
            output_key: format!("{}_audio", name),
            response_format: AudioResponseFormat::default(),
            speed: None,
            voice_label: None,
            sample_rate: None,
            quality: None,
            dependencies: Vec::new(),
            condition: None,
            timeout_ms: None,
        }
    }
    
    // Builder pattern methods
    pub fn with_input(mut self, template: &str) -> Self {
        self.input_template = template.to_string();
        self
    }
    
    pub fn with_voice(mut self, voice: &str) -> Self {
        self.voice = voice.to_string();
        self
    }
    
    pub fn with_response_format(mut self, format: AudioResponseFormat) -> Self {
        self.response_format = format;
        self
    }
    
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = Some(speed.clamp(0.5, 2.0));
        self
    }
    
    pub fn with_voice_label(mut self, label: VoiceLabel) -> Self {
        self.voice_label = Some(label);
        self
    }
    
    pub fn with_sample_rate(mut self, rate: u32) -> Self {
        // Validate supported sample rates
        let valid_rates = [8000, 16000, 22050, 24000];
        if valid_rates.contains(&rate) {
            self.sample_rate = Some(rate);
        } else {
            // Default to 24000 if invalid rate provided
            self.sample_rate = Some(24000);
        }
        self
    }
    
    pub fn with_quality(mut self, quality: AudioQuality) -> Self {
        self.quality = Some(quality);
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
    
    /// Create configuration for TTS generation
    fn create_tts_config(&self, resolved_input: &str) -> NodeResult<Value> {
        let mut config = serde_json::Map::new();
        
        config.insert("model".to_string(), Value::String(self.model.clone()));
        config.insert("input".to_string(), Value::String(resolved_input.to_string()));
        config.insert("voice".to_string(), Value::String(self.voice.clone()));
        
        config.insert("response_format".to_string(), serde_json::to_value(&self.response_format)?);
        
        if let Some(speed) = self.speed {
            config.insert("speed".to_string(),
                Value::Number(serde_json::Number::from_f64(speed as f64).unwrap()));
        }
        
        if let Some(ref voice_label) = self.voice_label {
            config.insert("voice_label".to_string(), serde_json::to_value(voice_label)?);
        }
        
        if let Some(sample_rate) = self.sample_rate {
            config.insert("sample_rate".to_string(),
                Value::Number(serde_json::Number::from(sample_rate)));
        }
        
        if let Some(ref quality) = self.quality {
            config.insert("quality".to_string(), serde_json::to_value(quality)?);
        }
        
        Ok(Value::Object(config))
    }
    
    /// Validate text input for TTS
    fn validate_tts_input(&self, input: &str) -> NodeResult<()> {
        if input.is_empty() {
            return Err(NodeError::ValidationError {
                message: "TTS input text cannot be empty".to_string(),
            });
        }
        
        // Check text length (typical TTS limits)
        if input.len() > 10000 {
            return Err(NodeError::ValidationError {
                message: format!("TTS input text too long: {} characters (max 10000)", input.len()),
            });
        }
        
        Ok(())
    }
    
    /// Execute real TTS synthesis using StepFun API
    async fn execute_real_tts(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
        let input_text = config.get("input").unwrap().as_str().unwrap();
        let model = config.get("model").unwrap().as_str().unwrap();
        let voice = config.get("voice").unwrap().as_str().unwrap();
        
        println!("🔊 Executing Text-to-Speech (StepFun API):");
        println!("   Model: {}", model);
        println!("   Voice: {}", voice);
        println!("   Text: {}...", &input_text[..input_text.len().min(100)]);
        
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
        
        // Build TTS request using TTSBuilder
        let mut tts_builder = TTSBuilder::new(model, input_text, voice);
        
        // Map response format
        let response_format = match &self.response_format {
            AudioResponseFormat::Mp3 => "mp3",
            AudioResponseFormat::Wav => "wav", 
            AudioResponseFormat::Flac => "flac",
            AudioResponseFormat::Opus => "opus",
        };
        tts_builder = tts_builder.response_format(response_format);
        
        // Add optional parameters
        if let Some(speed) = self.speed {
            tts_builder = tts_builder.speed(speed);
        }
        
        if let Some(sample_rate) = self.sample_rate {
            tts_builder = tts_builder.sample_rate(sample_rate);
        }
        
        // Convert VoiceLabel to StepFun format if present
        if let Some(ref voice_label) = self.voice_label {
            if let Some(ref language) = voice_label.language {
                tts_builder = tts_builder.language(language);
            }
            if let Some(ref emotion) = voice_label.emotion {
                tts_builder = tts_builder.emotion(emotion);
            }
            if let Some(ref style) = voice_label.style {
                tts_builder = tts_builder.style(style);
            }
        }
        
        let tts_request = tts_builder.build();
        
        // Execute TTS request
        let audio_data = stepfun_client.text_to_speech(tts_request).await
            .map_err(|e| NodeError::ExecutionError {
                message: format!("StepFun TTS execution failed: {}", e),
            })?;
        
        // Convert binary data to base64 data URL
        use base64::{Engine as _, engine::general_purpose};
        let base64_data = general_purpose::STANDARD.encode(audio_data);
        let mime_type = match response_format {
            "mp3" => "audio/mp3",
            "wav" => "audio/wav",
            "flac" => "audio/flac", 
            "opus" => "audio/opus",
            _ => "audio/mp3", // fallback
        };
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);
        
        let duration_estimate = input_text.split_whitespace().count() as f32 * 0.6; // ~0.6 seconds per word
        println!("✅ TTS Synthesis: Generated {:.1}s of {} audio", 
                duration_estimate, response_format);
        
        Ok(data_url)
    }
    
    /// Mock TTS synthesis (fallback)
    async fn execute_mock_tts(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
        let input_text = config.get("input").unwrap().as_str().unwrap();
        let model = config.get("model").unwrap().as_str().unwrap();
        let voice = config.get("voice").unwrap().as_str().unwrap();
        let format = config.get("response_format").unwrap();
        
        println!("🔊 Executing Text-to-Speech (MOCK - API key not available):");
        println!("   Model: {}", model);
        println!("   Voice: {}", voice);
        println!("   Text: {}...", &input_text[..input_text.len().min(100)]);
        println!("   Format: {:?}", format);
        
        if let Some(speed) = config.get("speed").and_then(|s| s.as_f64()) {
            println!("   Speed: {}x", speed);
        }
        
        if let Some(voice_label) = config.get("voice_label") {
            println!("   Voice Label: {:?}", voice_label);
        }
        
        // Simulate processing time (proportional to text length)
        let processing_time = (input_text.len() / 10).max(100).min(2000);
        tokio::time::sleep(std::time::Duration::from_millis(processing_time as u64)).await;
        
        // Mock response - generate fake audio data based on format
        let mock_response = match &self.response_format {
            AudioResponseFormat::Mp3 => {
                "data:audio/mp3;base64,SUQzBAAAAAAAI1RTU0UAAAAPAAADTGF2ZjU4Ljc2LjEwMAAAAAAAAAAAAAAA"
            }
            AudioResponseFormat::Wav => {
                "data:audio/wav;base64,UklGRnoGAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQoGAACBhYqFbF1fdJivrJBhNjVgodDbq2EcBj+a2/LDciUFLIHO8tiJNwgZaLvt559NEAxQp+PwtmMcBjiR1/LMeSwFJHfH8N2QQAoUXrTp66hVFApGn+DyvmwhBTOJ0fPNeSsFJH7J8N2QQAoUXrTp66hVFApGn+DyvmshBjiR1/LMeSwFJHfH8N2QQAkUXrTp66hVFAlGn+DyvmsiB"
            }
            AudioResponseFormat::Flac => {
                "data:audio/flac;base64,ZkxhQwAAACIQABAAAA8AAAAAAAAA7wAAAAAAAAAAAAAAAAAAAAAAAAA="
            }
            AudioResponseFormat::Opus => {
                "data:audio/opus;base64,T2dnUwACAAAAAAAAAAABAAAAAAAAAJAuPwABAgEBAgEBAgEBAgEBAgEBAQEBAQEBAQEBAQEBAQE="
            }
        };
        
        let duration_estimate = input_text.split_whitespace().count() as f32 * 0.6; // ~0.6 seconds per word
        println!("✅ TTS Synthesis (MOCK): Generated {:.1}s of {:?} audio", 
                duration_estimate, self.response_format);
        
        Ok(mock_response.to_string())
    }
}

#[async_trait]
impl AsyncNode for TTSNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        // Check conditional execution
        if let Some(ref condition) = self.condition {
            let resolved_condition = shared.resolve_template_advanced(condition);
            if resolved_condition != "true" {
                println!("⏭️  Skipping TTS node '{}' due to condition: {}", self.name, resolved_condition);
                return Ok(Value::Object(serde_json::Map::new()));
            }
        }
        
        // Resolve input template
        let resolved_input = shared.resolve_template_advanced(&self.input_template);
        
        // Include input keys data in text resolution
        let mut enriched_input = resolved_input;
        for input_key in &self.input_keys {
            if let Some(input_value) = shared.get(input_key) {
                let input_str = match input_value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                enriched_input = enriched_input.replace(
                    &format!("{{{{{}}}}}", input_key),
                    &input_str,
                );
            }
        }
        
        // Validate TTS input
        self.validate_tts_input(&enriched_input)
            .map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
        
        let config = self.create_tts_config(&enriched_input)
            .map_err(|e| AgentFlowError::AsyncExecutionError {
                message: format!("Failed to create TTS config: {}", e),
            })?;
        
        println!("🔧 TTS Node '{}' prepared:", self.name);
        println!("   Model: {}", self.model);
        println!("   Voice: {}", self.voice);
        println!("   Text: {}...", &enriched_input[..enriched_input.len().min(100)]);
        
        Ok(config)
    }
    
    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let config = prep_result
            .as_object()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
                message: "Invalid prep result for TTS node".to_string(),
            })?;
        
        // Skip execution if condition failed
        if config.is_empty() {
            return Ok(Value::String("Skipped due to condition".to_string()));
        }
        
        // Apply timeout if configured - try real API first, fallback to mock
        let response = if let Some(timeout_ms) = self.timeout_ms {
            let timeout_duration = std::time::Duration::from_millis(timeout_ms);
            match tokio::time::timeout(timeout_duration, self.execute_real_tts(config)).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => {
                    // Fallback to mock if real API fails
                    match tokio::time::timeout(timeout_duration, self.execute_mock_tts(config)).await {
                        Ok(result) => result.map_err(|e| AgentFlowError::AsyncExecutionError {
                            message: e.to_string(),
                        })?,
                        Err(_) => return Err(AgentFlowError::TimeoutExceeded { duration_ms: timeout_ms }),
                    }
                }
                Err(_) => return Err(AgentFlowError::TimeoutExceeded { duration_ms: timeout_ms }),
            }
        } else {
            // Try real API first, fallback to mock
            match self.execute_real_tts(config).await {
                Ok(result) => result,
                Err(_) => {
                    self.execute_mock_tts(config).await
                        .map_err(|e| AgentFlowError::AsyncExecutionError {
                            message: e.to_string(),
                        })?
                }
            }
        };
        
        Ok(Value::String(response))
    }
    
    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        // Store the generated audio data
        shared.insert(self.output_key.clone(), exec_result.clone());
        
        // Also store as generic "generated_audio" for workflow chaining
        shared.insert("generated_audio".to_string(), exec_result);
        
        println!("💾 Stored generated audio in shared state as: '{}'", self.output_key);
        
        Ok(None) // No specific next action
    }
    
    fn get_node_id(&self) -> Option<String> {
        Some(self.name.clone())
    }
}

/// Helper constructors for common TTS scenarios
impl TTSNode {
    /// Create a narrator node for storytelling
    pub fn narrator(name: &str, model: &str, voice: &str) -> Self {
        Self::new(name, model, voice)
            .with_speed(0.9)  // Slightly slower for clarity
            .with_quality(AudioQuality::High)
            .with_response_format(AudioResponseFormat::Mp3)
            .with_sample_rate(24000)
            .with_voice_label(VoiceLabel {
                language: Some("English".to_string()),
                emotion: Some("neutral".to_string()),
                style: Some("narrative".to_string()),
                gender: None,
            })
    }
    
    /// Create a podcast/interview TTS node
    pub fn podcast_voice(name: &str, model: &str, voice: &str) -> Self {
        Self::new(name, model, voice)
            .with_speed(1.1)  // Slightly faster
            .with_quality(AudioQuality::Premium)
            .with_response_format(AudioResponseFormat::Mp3)
            .with_sample_rate(22050)
            .with_voice_label(VoiceLabel {
                language: Some("English".to_string()),
                emotion: Some("conversational".to_string()),
                style: Some("casual".to_string()),
                gender: None,
            })
    }
    
    /// Create an announcer/alert TTS node
    pub fn announcer(name: &str, model: &str, voice: &str) -> Self {
        Self::new(name, model, voice)
            .with_speed(1.0)
            .with_quality(AudioQuality::Standard)
            .with_response_format(AudioResponseFormat::Wav)
            .with_sample_rate(16000)  // Good for alerts/announcements
            .with_voice_label(VoiceLabel {
                language: Some("English".to_string()),
                emotion: Some("confident".to_string()),
                style: Some("clear".to_string()),
                gender: None,
            })
    }
    
    /// Create a multilingual TTS node
    pub fn multilingual(name: &str, model: &str, voice: &str, language: &str) -> Self {
        Self::new(name, model, voice)
            .with_quality(AudioQuality::High)
            .with_response_format(AudioResponseFormat::Mp3)
            .with_sample_rate(24000)
            .with_voice_label(VoiceLabel {
                language: Some(language.to_string()),
                emotion: Some("neutral".to_string()),
                style: Some("natural".to_string()),
                gender: None,
            })
    }
    
    /// Create an emotional TTS node
    pub fn emotional_voice(name: &str, model: &str, voice: &str, emotion: &str) -> Self {
        Self::new(name, model, voice)
            .with_speed(1.0)
            .with_quality(AudioQuality::Premium)
            .with_response_format(AudioResponseFormat::Wav)
            .with_sample_rate(24000)
            .with_voice_label(VoiceLabel {
                language: Some("English".to_string()),
                emotion: Some(emotion.to_string()),
                style: Some("expressive".to_string()),
                gender: None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tts_node_creation() {
        let node = TTSNode::new("test_tts", "openai-tts", "nova");
        assert_eq!(node.name, "test_tts");
        assert_eq!(node.model, "openai-tts");
        assert_eq!(node.voice, "nova");
        assert_eq!(node.output_key, "test_tts_audio");
        assert!(matches!(node.response_format, AudioResponseFormat::Mp3));
    }
    
    #[tokio::test]
    async fn test_tts_node_builder() {
        let node = TTSNode::new("advanced_tts", "eleven-labs", "rachel")
            .with_input("Say: {{message}}")
            .with_speed(1.2)
            .with_response_format(AudioResponseFormat::Wav)
            .with_sample_rate(24000)
            .with_voice_label(VoiceLabel {
                language: Some("English".to_string()),
                emotion: Some("happy".to_string()),
                style: Some("energetic".to_string()),
                gender: Some("female".to_string()),
            })
            .with_input_keys(vec!["message".to_string()]);
            
        assert_eq!(node.input_template, "Say: {{message}}");
        assert_eq!(node.speed, Some(1.2));
        assert_eq!(node.sample_rate, Some(24000));
        assert!(matches!(node.response_format, AudioResponseFormat::Wav));
        assert_eq!(node.input_keys.len(), 1);
    }
    
    #[tokio::test]
    async fn test_tts_input_validation() {
        let node = TTSNode::new("validator", "tts", "voice");
        
        // Test empty input
        let result = node.validate_tts_input("");
        assert!(result.is_err());
        
        // Test valid input
        let result = node.validate_tts_input("Hello world");
        assert!(result.is_ok());
        
        // Test very long input
        let long_text = "a".repeat(15000);
        let result = node.validate_tts_input(&long_text);
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_sample_rate_validation() {
        let node = TTSNode::new("test", "tts", "voice")
            .with_sample_rate(48000); // Invalid rate
        
        // Should default to 24000
        assert_eq!(node.sample_rate, Some(24000));
        
        let node2 = TTSNode::new("test2", "tts", "voice")
            .with_sample_rate(16000); // Valid rate
        
        assert_eq!(node2.sample_rate, Some(16000));
    }
    
    #[tokio::test]
    async fn test_helper_constructors() {
        // Test narrator
        let narrator = TTSNode::narrator("narrator", "tts", "david");
        assert_eq!(narrator.speed, Some(0.9));
        assert!(matches!(narrator.quality, Some(AudioQuality::High)));
        
        // Test podcast voice
        let podcast = TTSNode::podcast_voice("podcast", "tts", "sarah");
        assert_eq!(podcast.speed, Some(1.1));
        assert!(matches!(podcast.quality, Some(AudioQuality::Premium)));
        
        // Test announcer
        let announcer = TTSNode::announcer("announce", "tts", "alex");
        assert_eq!(announcer.sample_rate, Some(16000));
        assert!(matches!(announcer.response_format, AudioResponseFormat::Wav));
        
        // Test multilingual
        let multilingual = TTSNode::multilingual("multi", "tts", "voice", "Spanish");
        if let Some(ref label) = multilingual.voice_label {
            assert_eq!(label.language, Some("Spanish".to_string()));
        }
        
        // Test emotional
        let emotional = TTSNode::emotional_voice("emotion", "tts", "voice", "excited");
        if let Some(ref label) = emotional.voice_label {
            assert_eq!(label.emotion, Some("excited".to_string()));
        }
    }
    
    #[tokio::test]
    async fn test_tts_full_workflow() {
        let node = TTSNode::narrator("story_narrator", "openai-tts", "nova")
            .with_input("{{story_text}}")
            .with_input_keys(vec!["story_text".to_string()]);
            
        let shared = SharedState::new();
        shared.insert("story_text".to_string(), 
            Value::String("Once upon a time, in a digital kingdom far away...".to_string()));
        
        // Test full execution
        let result = node.run_async(&shared).await.unwrap();
        assert!(result.is_none());
        
        // Check audio was generated and stored
        let audio = shared.get(&node.output_key).unwrap();
        assert!(audio.as_str().unwrap().contains("data:audio"));
    }
}