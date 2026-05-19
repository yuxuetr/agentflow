//! `agentflow harness list` — enumerate stored session logs.

use anyhow::Result;

use agentflow_harness::default_session_dir;

use super::{OutputFormat, resolve_run_dir};

pub async fn execute(run_dir_override: Option<String>, output: String) -> Result<()> {
  let output = OutputFormat::parse(&output)?;
  let run_root = resolve_run_dir(run_dir_override)?;
  let session_dir = default_session_dir(&run_root);

  let mut sessions: Vec<SessionEntry> = Vec::new();
  match std::fs::read_dir(&session_dir) {
    Ok(rd) => {
      for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
          continue;
        }
        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
          Some(name) => name.to_owned(),
          None => continue,
        };
        let metadata = entry.metadata().ok();
        let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata.and_then(|m| m.modified().ok());
        let event_count = count_lines(&path).unwrap_or(0);
        sessions.push(SessionEntry {
          session_id,
          size_bytes,
          event_count,
          modified_secs_epoch: modified
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        });
      }
    }
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
    Err(err) => return Err(anyhow::anyhow!("read {}: {err}", session_dir.display())),
  }
  sessions.sort_by_key(|e| std::cmp::Reverse(e.modified_secs_epoch.unwrap_or(0)));

  match output {
    OutputFormat::Text => {
      if sessions.is_empty() {
        println!("(no sessions found under {})", session_dir.display());
        return Ok(());
      }
      println!("{:<40} {:>10} {:>10}", "SESSION_ID", "EVENTS", "BYTES");
      for entry in &sessions {
        println!(
          "{:<40} {:>10} {:>10}",
          entry.session_id, entry.event_count, entry.size_bytes
        );
      }
    }
    OutputFormat::Json | OutputFormat::StreamJson | OutputFormat::JsonEnvelope => {
      let payload = serde_json::json!({
        "session_dir": session_dir,
        "sessions": sessions
          .iter()
          .map(|e| serde_json::json!({
            "session_id": e.session_id,
            "event_count": e.event_count,
            "size_bytes": e.size_bytes,
            "modified_secs_epoch": e.modified_secs_epoch,
          }))
          .collect::<Vec<_>>(),
      });
      if matches!(output, OutputFormat::JsonEnvelope) {
        // P3.3 migration: wrap the same summary `json` mode emits in
        // the canonical envelope. `stream-json` keeps emitting the
        // bare body because consumers in that mode already expect a
        // single JSON object stream-shaped — wrapping each call in
        // an envelope would defeat the purpose.
        let envelope = crate::json_envelope::CliJsonEnvelope::ok("harness list", &payload);
        println!("{}", serde_json::to_string_pretty(&envelope)?);
      } else {
        println!("{}", serde_json::to_string_pretty(&payload)?);
      }
    }
  }
  Ok(())
}

#[derive(Debug)]
struct SessionEntry {
  session_id: String,
  size_bytes: u64,
  event_count: usize,
  modified_secs_epoch: Option<u64>,
}

fn count_lines(path: &std::path::Path) -> Result<usize> {
  use std::io::BufRead;
  let file = std::fs::File::open(path)?;
  let reader = std::io::BufReader::new(file);
  Ok(
    reader
      .lines()
      .map_while(Result::ok)
      .filter(|line| !line.trim().is_empty())
      .count(),
  )
}
