use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};
use std::path::Path;

use super::error_context::mcp_context;
use agentflow_llm::AgentFlow;
use agentflow_skills::{SkillBuilder, SkillLoader};

const HELP_TEXT: &str = "\
Commands:
  /exit, /quit  — end the session
  /reset        — start a new session (clears memory)
  /tokens       — show estimated token count for this session
  /session      — show the current session ID
  /help         — show this help message
  (empty line)  — skipped
";

pub async fn execute(skill_dir: String, session_id: Option<String>) -> Result<()> {
  let dir = Path::new(&skill_dir);

  // ── Load + validate ───────────────────────────────────────────────────────
  let manifest =
    SkillLoader::load(dir).with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;

  let warnings =
    SkillLoader::validate(&manifest, dir).with_context(|| "Skill validation failed")?;
  for w in &warnings {
    eprintln!("⚠  {}", w);
  }

  // ── Init LLM ──────────────────────────────────────────────────────────────
  AgentFlow::init()
    .await
    .context("Failed to initialise AgentFlow — is your API key configured?")?;

  // ── Build agent ───────────────────────────────────────────────────────────
  let mut agent = SkillBuilder::build(&manifest, dir)
    .await
    .with_context(|| mcp_context("Failed to build agent from skill manifest", &manifest))?;

  if let Some(sid) = session_id {
    agent = agent.with_session_id(sid);
  }

  // ── Welcome banner ────────────────────────────────────────────────────────
  println!("╔══════════════════════════════════════════════════╗");
  println!("║  🤖  Skill Chat — {}  ", manifest.skill.name);
  println!("║  Model: {}  ", manifest.model.resolved_model());
  println!("║  Session: {}  ", agent.session_id);
  println!("╚══════════════════════════════════════════════════╝");
  println!("Type a message or /help for commands. Ctrl-C to exit.\n");

  // ── REPL ──────────────────────────────────────────────────────────────────
  let stdin = io::stdin();
  let mut stdout = io::stdout();

  for line in stdin.lock().lines() {
    let line = line.context("Failed to read from stdin")?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
      continue;
    }

    // ── Built-in commands ──────────────────────────────────────────────
    match trimmed {
      "/exit" | "/quit" => {
        println!("👋 Bye!");
        break;
      }
      "/reset" => {
        agent.reset().await.context("Failed to reset session")?;
        println!("🔄 Session reset. New session: {}", agent.session_id);
        continue;
      }
      "/tokens" => {
        match agent.token_count().await {
          Ok(n) => println!("📊 Estimated tokens in session: {}", n),
          Err(e) => println!("⚠  Could not get token count: {}", e),
        }
        continue;
      }
      "/session" => {
        println!("🔑 Session ID: {}", agent.session_id);
        continue;
      }
      "/help" => {
        print!("{}", HELP_TEXT);
        continue;
      }
      _ => {}
    }

    // ── Send to agent ──────────────────────────────────────────────────
    print!("⏳ Thinking...\r");
    stdout.flush().ok();

    let start = std::time::Instant::now();
    match agent.run(trimmed).await {
      Ok(answer) => {
        let elapsed = start.elapsed();
        // Clear the "Thinking..." line
        print!("\r                    \r");
        println!("🤖  {}", answer);
        println!("    ⏱  {:.2?}\n", elapsed);
      }
      Err(e) => {
        print!("\r                    \r");
        eprintln!("❌  Agent error: {}", e);
        eprintln!("    Use /reset to start a fresh session or /exit to quit.\n");
      }
    }

    // Prompt for next input
    print!("You: ");
    stdout.flush().ok();
  }

  Ok(())
}
