// Specialized AI Nodes Example
// This demonstrates the specialized node architecture with proper parameters
// for different AI model types (LLM, TextToImage, ImageToImage, TTS, ASR)

use agentflow_core::{SharedState, AsyncNode};
use agentflow_nodes::{
    LlmNode, TextToImageNode, ImageToImageNode, ImageEditNode, ImageUnderstandNode, TTSNode, ASRNode
};
use serde_json::json;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Specialized AI Nodes Architecture Demo");
    println!("==========================================");
    println!();
    println!("✅ SOLUTION: Each AI model type now has its own specialized node with");
    println!("   appropriate parameters, avoiding the complexity of a unified interface.");
    println!();

    let shared = SharedState::new();

    // ===================================================================================
    // 1. LLM Node - Specialized for Text Generation Only
    // ===================================================================================
    println!("📝 Example 1: LLM Node (Text Generation Only)");
    println!("Parameters: model, prompt, temperature, max_tokens, tools, response_format");
    
    let llm_node = LlmNode::new("content_generator", "gpt-4")
        .with_prompt("Write a product description for: {{product_name}}")
        .with_temperature(0.7)
        .with_max_tokens(200)
        .with_json_response(Some(json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "description": {"type": "string"},
                "features": {"type": "array", "items": {"type": "string"}}
            }
        })))
        .with_input_keys(vec!["product_name".to_string()]);

    shared.insert("product_name".to_string(), json!("AI-powered wireless headphones"));
    
    let result = llm_node.run_async(&shared).await?;
    println!("✅ LLM Result: {:?}", result);
    
    if let Some(content) = shared.get(&llm_node.output_key) {
        println!("📋 Generated Content: {}", 
            serde_json::to_string_pretty(&content).unwrap_or_else(|_| content.to_string()));
    }
    println!();

    // ===================================================================================
    // 2. TextToImage Node - Specialized for Image Generation  
    // ===================================================================================
    println!("🎨 Example 2: TextToImage Node (Image Generation)");
    println!("Parameters: model, prompt, size, steps, cfg_scale, style_reference, response_format");
    
    let image_gen_node = TextToImageNode::artistic_generator("product_visualizer", "dalle-3")
        .with_prompt("Professional product photography of {{product_name}}: {{description}}")
        .with_size("1024x1024")
        .with_steps(50)
        .with_cfg_scale(7.5)
        .with_input_keys(vec!["product_name".to_string(), "description".to_string()]);

    shared.insert("description".to_string(), json!("sleek modern design, premium materials"));
    
    let result = image_gen_node.run_async(&shared).await?;
    println!("✅ Image Generation Result: {:?}", result);
    
    if let Some(image) = shared.get(&image_gen_node.output_key) {
        println!("🖼️  Generated Image: {}...", 
            &image.as_str().unwrap_or("")[..50.min(image.as_str().unwrap_or("").len())]);
    }
    println!();

    // ===================================================================================
    // 3. ImageToImage Node - Specialized for Image Transformation
    // ===================================================================================
    println!("🔄 Example 3: ImageToImage Node (Image Transformation)");
    println!("Parameters: model, prompt, source_url, source_weight, size, strength, cfg_scale");
    
    let image_transform_node = ImageToImageNode::style_transfer("style_transformer", "stable-diffusion", "product_visualizer_image")
        .with_prompt("Transform to {{art_style}} artistic style")
        .with_strength(0.7)
        .with_cfg_scale(7.0)
        .with_input_keys(vec!["art_style".to_string()]);

    shared.insert("art_style".to_string(), json!("watercolor painting"));
    
    let result = image_transform_node.run_async(&shared).await?;
    println!("✅ Image Transformation Result: {:?}", result);
    
    if let Some(transformed) = shared.get(&image_transform_node.output_key) {
        println!("🎭 Transformed Image: {}...", 
            &transformed.as_str().unwrap_or("")[..50.min(transformed.as_str().unwrap_or("").len())]);
    }
    println!();

    // ===================================================================================
    // 3.5. ImageEditNode - Specialized for Image Editing
    // ===================================================================================
    println!("✏️  Example 3.5: ImageEdit Node (Image Editing)");
    println!("Parameters: model, image, mask, prompt, size, response_format, steps, cfg_scale");
    
    let image_edit_node = ImageEditNode::photo_retoucher("photo_editor", "dall-e-2", "product_visualizer_image")
        .with_prompt("Remove background and enhance lighting for {{product_name}}")
        .with_response_format(agentflow_nodes::nodes::image_edit::ImageEditResponseFormat::Url)
        .with_input_keys(vec!["product_name".to_string()]);

    let result = image_edit_node.run_async(&shared).await?;
    println!("✅ Image Edit Result: {:?}", result);
    
    if let Some(edited) = shared.get(&image_edit_node.output_key) {
        println!("✨ Edited Image: {}...", 
            &edited.as_str().unwrap_or("")[..50.min(edited.as_str().unwrap_or("").len())]);
    }
    println!();

    // ===================================================================================
    // 3.6. ImageUnderstand Node - Specialized for Multimodal Vision Models
    // ===================================================================================
    println!("🔍 Example 3.6: ImageUnderstand Node (Vision Models)");
    println!("Parameters: model, text_prompt, image_source, system_message, max_tokens, temperature");
    
    let vision_node = ImageUnderstandNode::image_analyzer("image_analyzer", "gpt-4o", "product_visualizer_image")
        .with_text_prompt("Analyze this product image and provide insights about {{analysis_focus}}")
        .with_system_message("You are an expert product analyst. Provide detailed insights about visual design, marketing appeal, and technical aspects.")
        .with_max_tokens(800)
        .with_temperature(0.4)
        .with_input_keys(vec!["analysis_focus".to_string()]);

    shared.insert("analysis_focus".to_string(), json!("design aesthetics and market positioning"));
    
    let result = vision_node.run_async(&shared).await?;
    println!("✅ Vision Analysis Result: {:?}", result);
    
    if let Some(analysis) = shared.get(&vision_node.output_key) {
        println!("🧠 Vision Analysis: {}...", 
            &analysis.as_str().unwrap_or("")[..100.min(analysis.as_str().unwrap_or("").len())]);
    }
    println!();

    // ===================================================================================
    // 4. TTS Node - Specialized for Audio Synthesis
    // ===================================================================================
    println!("🔊 Example 4: TTS Node (Text-to-Speech)");
    println!("Parameters: model, input, voice, response_format, speed, voice_label, sample_rate");
    
    let tts_node = TTSNode::narrator("product_narrator", "openai-tts", "nova")
        .with_input("{{content_generator_output}}")
        .with_speed(1.0)
        .with_response_format(agentflow_nodes::nodes::tts::AudioResponseFormat::Mp3)
        .with_sample_rate(24000);

    let result = tts_node.run_async(&shared).await?;
    println!("✅ TTS Result: {:?}", result);
    
    if let Some(audio) = shared.get(&tts_node.output_key) {
        println!("🎵 Generated Audio: {}...", 
            &audio.as_str().unwrap_or("")[..50.min(audio.as_str().unwrap_or("").len())]);
    }
    println!();

    // ===================================================================================
    // 5. ASR Node - Specialized for Speech Recognition
    // ===================================================================================
    println!("🎤 Example 5: ASR Node (Speech Recognition)");
    println!("Parameters: model, file, response_format, language, hotwords, temperature");
    
    let asr_node = ASRNode::detailed_transcriber("audio_transcriber", "whisper-1", "product_narrator_audio")
        .with_language("en")
        .with_response_format(agentflow_nodes::nodes::asr::ASRResponseFormat::Json)
        .with_hotwords(vec!["headphones".to_string(), "wireless".to_string(), "AI".to_string()]);

    let result = asr_node.run_async(&shared).await?;
    println!("✅ ASR Result: {:?}", result);
    
    if let Some(transcript) = shared.get(&asr_node.output_key) {
        println!("📝 Transcription: {}", transcript.as_str().unwrap_or(""));
    }
    println!();

    // ===================================================================================
    // Summary & Benefits
    // ===================================================================================
    println!("🎉 SPECIALIZED NODE ARCHITECTURE BENEFITS:");
    println!("==========================================");
    println!();
    println!("✅ TYPE SAFETY:");
    println!("   • Each node has parameters specific to its AI model type");
    println!("   • No invalid parameter combinations (e.g., 'voice' on LLM node)");
    println!("   • Compile-time validation of node configurations");
    println!();
    println!("✅ CLARITY & MAINTAINABILITY:");
    println!("   • LlmNode: temperature, max_tokens, tools, response_format");
    println!("   • TextToImageNode: size, steps, cfg_scale, style_reference");  
    println!("   • ImageToImageNode: source_url, source_weight, strength");
    println!("   • ImageEditNode: image, mask, prompt, size, steps, cfg_scale");
    println!("   • ImageUnderstandNode: text_prompt, image_source, system_message, max_tokens");
    println!("   • TTSNode: voice, speed, voice_label, sample_rate");
    println!("   • ASRNode: file, language, hotwords, timestamp_granularities");
    println!();
    println!("✅ OPTIMAL DEVELOPER EXPERIENCE:");
    println!("   • Autocomplete shows only relevant parameters");
    println!("   • Helper constructors for common use cases");
    println!("   • Clear documentation per node type");
    println!();
    println!("✅ WORKFLOW COMPOSITION:");
    println!("   • Text → LLM → TTS (content creation pipeline)");  
    println!("   • Text → TextToImage → ImageEdit → ImageToImage (visual pipeline)");
    println!("   • Image → ImageUnderstand → LLM → TTS (vision analysis pipeline)");
    println!("   • Audio → ASR → LLM → TTS (audio processing pipeline)");
    println!("   • Each node has clear input/output contracts");
    println!();
    println!("✅ FUTURE EXTENSIBILITY:");
    println!("   • Easy to add VideoGenerateNode, AudioEditNode, etc.");
    println!("   • Each node type can evolve independently");
    println!("   • No breaking changes when adding new AI capabilities");

    Ok(())
}