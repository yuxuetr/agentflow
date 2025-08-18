use agentflow_llm::{AgentFlow, LLMError};
use std::env;

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  println!("=== AgentFlow LLM Configuration Management ===\n");

  let args: Vec<String> = env::args().collect();

  if args.len() < 2 {
    print_usage();
    return Ok(());
  }

  match args[1].as_str() {
    "init" => {
      println!("🚀 Initializing AgentFlow LLM with auto-discovery...");
      match AgentFlow::init().await {
        Ok(()) => println!("✅ Successfully initialized with configuration"),
        Err(e) => println!("❌ Initialization failed: {}", e),
      }
    }

    "init-builtin" => {
      println!("🔧 Initializing with built-in defaults...");
      match AgentFlow::init_with_builtin_config().await {
        Ok(()) => println!("✅ Successfully initialized with built-in config"),
        Err(e) => println!("❌ Initialization failed: {}", e),
      }
    }

    "generate" => {
      let path = args.get(2).unwrap_or(&"models.yml".to_string()).clone();
      println!("📝 Generating configuration file: {}", path);
      match AgentFlow::generate_config(&path).await {
        Ok(()) => {
          println!("✅ Configuration file generated successfully!");
          println!("💡 Next steps:");
          println!("   1. Edit {} to add your API keys", path);
          println!("   2. Set environment variables (OPENAI_API_KEY, etc.)");
          println!("   3. Run: cargo run --example config_management init");
        }
        Err(e) => println!("❌ Failed to generate config: {}", e),
      }
    }

    "generate-user" => {
      println!("🏠 Generating user-specific configuration...");
      match AgentFlow::generate_user_config().await {
        Ok(()) => {
          println!("✅ User configuration generated in ~/.agentflow/models.yml");
          println!("💡 This config will be used globally for all your projects");
        }
        Err(e) => println!("❌ Failed to generate user config: {}", e),
      }
    }

    "generate-env" => {
      let path = args.get(2).unwrap_or(&".env".to_string()).clone();
      println!("🔑 Generating environment file: {}", path);
      match AgentFlow::generate_env(&path).await {
        Ok(()) => {
          println!("✅ Environment file generated successfully!");
          println!("🔒 IMPORTANT: Add your real API keys to {}", path);
          println!("⚠️  SECURITY: Ensure {} is in your .gitignore!", path);
        }
        Err(e) => println!("❌ Failed to generate env file: {}", e),
      }
    }

    "generate-env-user" => {
      println!("🏠 Generating user-specific environment file...");
      match AgentFlow::generate_user_env().await {
        Ok(()) => {
          println!("✅ User environment file generated in ~/.agentflow/.env");
          println!("💡 This will be used globally for all your projects");
        }
        Err(e) => println!("❌ Failed to generate user env: {}", e),
      }
    }

    "setup" => {
      println!("🚀 Complete setup for new project...");
      println!("📝 Generating models.yml...");
      match AgentFlow::generate_config("models.yml").await {
        Ok(()) => println!("✅ models.yml generated"),
        Err(e) => println!("❌ Failed to generate models.yml: {}", e),
      }

      println!("🔑 Generating .env...");
      match AgentFlow::generate_project_env().await {
        Ok(()) => {
          println!("✅ .env generated");
          println!("\n🎉 Setup complete! Next steps:");
          println!("   1. Add your API keys to .env");
          println!("   2. Customize models.yml if needed");
          println!("   3. Run: cargo run --example config_management init");
        }
        Err(e) => println!("❌ Failed to generate .env: {}", e),
      }
    }

    "init-env" => {
      println!("🔄 Initializing with environment auto-loading...");
      match AgentFlow::init_with_env().await {
        Ok(()) => println!("✅ Successfully initialized with environment loading"),
        Err(e) => println!("❌ Initialization failed: {}", e),
      }
    }

    "demo" => {
      println!("🧪 Demonstrating configuration priority...");
      println!("Configuration search order:");
      println!("  1. ./models.yml (project-specific)");
      println!("  2. ~/.agentflow/models.yml (user-specific)");
      println!("  3. Built-in defaults (bundled in crate)");

      // Initialize logging for demo
      AgentFlow::init_logging().ok();

      match AgentFlow::init().await {
        Ok(()) => {
          println!("✅ Configuration loaded successfully");

          // Show which models are available
          use agentflow_llm::ModelRegistry;
          let registry = ModelRegistry::global();
          let models = registry.list_models();

          println!("\n📋 Available models:");
          for model in models {
            if let Ok(info) = registry.get_model_info(&model) {
              println!("  • {} ({})", model, info.vendor);
            }
          }

          println!("\n📈 Logging initialized - use RUST_LOG=debug for detailed logs");
        }
        Err(e) => println!("❌ Configuration failed: {}", e),
      }
    }

    "test-json" => {
      println!("📝 Testing JSON output capabilities...");

      AgentFlow::init_logging().ok();

      match AgentFlow::init_with_env().await {
        Ok(()) => {
          println!("✅ Testing JSON mode with gpt-4o-mini...");

          let result = AgentFlow::model("gpt-4o-mini")
            .prompt("Return a JSON object with fields: name='AgentFlow', version='0.1.0', status='testing'")
            .json_mode()
            .enable_logging(true)
            .execute().await;

          match result {
            Ok(response) => {
              println!("✅ JSON response received:");
              if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
              } else {
                println!("⚠️ Response not valid JSON: {}", response);
              }
            }
            Err(e) => println!("❌ JSON test failed: {}", e),
          }
        }
        Err(e) => println!("❌ Initialization failed: {}", e),
      }
    }

    _ => {
      println!("❌ Unknown command: {}", args[1]);
      print_usage();
    }
  }

  Ok(())
}

fn print_usage() {
  println!("Usage: cargo run --example config_management <command>");
  println!();
  println!("Commands:");
  println!("  init               - Initialize with auto-discovered config");
  println!("  init-builtin       - Initialize with built-in defaults only");
  println!("  init-env           - Initialize with environment auto-loading");
  println!("  generate [path]    - Generate config file (default: models.yml)");
  println!("  generate-user      - Generate user config in ~/.agentflow/");
  println!("  generate-env [path]- Generate .env file (default: .env)");
  println!("  generate-env-user  - Generate user .env in ~/.agentflow/");
  println!("  setup              - Complete setup (models.yml + .env + .gitignore)");
  println!("  demo               - Show configuration priority demo");
  println!("  test-json          - Test JSON output capabilities");
  println!();
  println!("Examples:");
  println!("  cargo run --example config_management setup      # Complete setup");
  println!("  cargo run --example config_management generate-env");
  println!("  cargo run --example config_management generate-user");
  println!("  cargo run --example config_management init-env");
}
