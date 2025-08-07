use std::env;

fn main() {
  println!("=== Environment Debug ===");

  // Load all possible env files
  dotenvy::from_filename(".env").ok();
  dotenvy::from_filename("examples/.env").ok();
  dotenvy::from_filename("examples/demo.env").ok();

  // Check all relevant environment variables
  let vars = [
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY", 
    "CLAUDE_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "DEMO_OPENAI_API_KEY",
    "DEMO_ANTHROPIC_API_KEY",
  ];

  for var in &vars {
    match env::var(var) {
      Ok(value) => {
        let masked = if value.len() > 10 {
          format!("{}...{}", &value[..5], &value[value.len()-5..])
        } else {
          "***masked***".to_string()
        };
        println!("{}: {}", var, masked);
      }
      Err(_) => println!("{}: (not set)", var),
    }
  }

  // Test the logic for detecting real keys
  let has_real_openai = env::var("OPENAI_API_KEY")
    .map(|key| !key.is_empty() && !key.starts_with("demo-key"))
    .unwrap_or(false);

  println!("\nLogic test:");
  println!("has_real_openai: {}", has_real_openai);
}