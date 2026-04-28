use agentflow_llm::AgentFlow;
use anyhow::Result;

pub async fn execute(force: bool) -> Result<()> {
  println!("🚀 Initializing AgentFlow configuration...");

  // Check if config directory already exists
  let home_dir =
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
  let config_dir = home_dir.join(".agentflow");
  let config_file = config_dir.join("models.yml");
  let env_file = config_dir.join(".env");

  // Check for existing files if not forcing
  if !force && (config_file.exists() || env_file.exists()) {
    println!("⚠️  Configuration files already exist in ~/.agentflow/");
    if config_file.exists() {
      println!("   • models.yml found");
    }
    if env_file.exists() {
      println!("   • .env found");
    }
    println!();
    println!("Use --force to overwrite existing configuration files.");
    println!("Or run 'agentflow config show' to view current configuration.");
    return Ok(());
  }

  // Generate the configuration files
  match AgentFlow::generate_config().await {
    Ok(_) => {
      println!();
      println!("✅ Configuration initialized successfully!");
      println!();
      println!("📁 Files created:");
      println!("   • ~/.agentflow/models.yml  (model configurations)");
      println!("   • ~/.agentflow/.env        (API key templates)");
      println!();
      println!("🔧 Next steps:");
      println!("   1. Edit ~/.agentflow/.env and add your API keys");
      println!("   2. Uncomment the API keys you want to use:");
      println!("      # OPENAI_API_KEY=sk-your-key-here");
      println!("      OPENAI_API_KEY=sk-your-actual-key-here");
      println!();
      println!("💡 Available providers:");
      println!("   • OpenAI (GPT models)    → OPENAI_API_KEY");
      println!("   • Anthropic (Claude)     → ANTHROPIC_API_KEY");
      println!("   • Google (Gemini)        → GEMINI_API_KEY");
      println!("   • MoonShot (Kimi)        → MOONSHOT_API_KEY");
      println!("   • Alibaba (Qwen)         → DASHSCOPE_API_KEY");
      println!("   • StepFun (Step)         → STEPFUN_API_KEY");
      println!();
      println!("🧪 Test your setup:");
      println!("   agentflow llm models                 # List available models");
      println!("   agentflow llm prompt \"Hello world\"   # Test a simple prompt");
      println!();
    }
    Err(e) => {
      return Err(anyhow::anyhow!("Failed to generate configuration: {}", e));
    }
  }

  Ok(())
}
