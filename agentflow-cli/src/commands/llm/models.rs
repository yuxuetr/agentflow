use anyhow::Result;
use agentflow_llm::{AgentFlow, registry::{ModelRegistry, model_registry::ModelInfo}};
use colored::*;

pub async fn execute(provider: Option<String>, detailed: bool) -> Result<()> {
    // Initialize AgentFlow with builtin configuration
    AgentFlow::init_with_builtin_config().await?;
    
    let registry = ModelRegistry::global();
    let model_names = registry.list_models();
    
    // Convert model names to ModelInfo structs
    let models: Result<Vec<ModelInfo>, _> = model_names
        .iter()
        .map(|name| registry.get_model_info(name))
        .collect();
    let models = models?;

    if models.is_empty() {
        println!("No models found. Run 'agentflow config init' to set up your configuration.");
        return Ok(());
    }

    // Filter by provider if specified
    let filtered_models: Vec<_> = if let Some(ref provider_filter) = provider {
        models
            .into_iter()
            .filter(|model| {
                model.vendor.to_lowercase().contains(&provider_filter.to_lowercase())
            })
            .collect()
    } else {
        models
    };

    if filtered_models.is_empty() {
        println!(
            "No models found for provider: {}",
            provider.unwrap_or_default()
        );
        return Ok(());
    }

    if detailed {
        print_detailed_models(&filtered_models);
    } else {
        print_simple_models(&filtered_models);
    }

    Ok(())
}

fn print_simple_models(models: &[ModelInfo]) {
    println!("{}", "Available Models:".bold().blue());
    println!();

    let mut current_provider = String::new();
    for model in models {
        if model.vendor != current_provider {
            current_provider = model.vendor.clone();
            println!("{}", format!("{}:", current_provider).bold().green());
        }
        
        let model_name = if model.model_id.starts_with(&model.vendor) {
            model.model_id.clone()
        } else {
            format!("{}/{}", model.vendor, model.model_id)
        };

        println!("  • {}", model_name);
    }
}

fn print_detailed_models(models: &[ModelInfo]) {
    println!("{}", "Available Models (Detailed):".bold().blue());
    println!();

    let mut current_provider = String::new();
    for model in models {
        if model.vendor != current_provider {
            current_provider = model.vendor.clone();
            println!("{}", format!("{}:", current_provider).bold().green());
            println!();
        }

        let model_name = if model.model_id.starts_with(&model.vendor) {
            model.model_id.clone()
        } else {
            format!("{}/{}", model.vendor, model.model_id)
        };

        println!("  {}", model_name.bold());
        
        println!("    Vendor: {}", model.vendor);
        println!("    Model ID: {}", model.model_id);
        println!("    Base URL: {}", model.base_url);
        
        if let Some(temperature) = model.temperature {
            println!("    Temperature: {}", temperature.to_string().yellow());
        }
        
        if let Some(max_tokens) = model.max_tokens {
            println!("    Max Tokens: {}", max_tokens.to_string().yellow());
        }
        
        println!("    Supports Streaming: {}", model.supports_streaming);
        
        println!();
    }
}