//! Smoke tests for the blog-to-podcast app.
//!
//! - **Compile-only** assertions about the node config builders run
//!   anywhere (no env, no network).
//! - **End-to-end** tests against real Moonshot + MiniMax / Edge
//!   self-skip when the relevant API keys are absent. CI without keys
//!   will compile this file and run the compile-only tests; nothing
//!   blocks on live providers.

use std::path::PathBuf;
use std::process::Command;

/// Locate the manifest dir so we can find fixtures regardless of cwd.
fn manifest_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn binary_path() -> PathBuf {
  // Cargo provides CARGO_BIN_EXE_<name> for integration tests.
  PathBuf::from(env!("CARGO_BIN_EXE_blog-to-podcast"))
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
  let output = Command::new(binary_path())
    .arg("--help")
    .output()
    .expect("spawn binary");
  assert!(output.status.success(), "exit status: {:?}", output.status);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(stdout.contains("blog-to-podcast"));
  assert!(stdout.contains("--blog"));
  assert!(stdout.contains("MOONSHOT_API_KEY"));
}

#[test]
fn missing_blog_flag_errors_with_hint() {
  let output = Command::new(binary_path())
    .arg("--output")
    .arg("/tmp/unused.wav")
    .output()
    .expect("spawn binary");
  assert!(!output.status.success(), "expected non-zero exit");
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(
    stderr.contains("--blog"),
    "stderr should mention --blog; got: {stderr}"
  );
}

#[test]
fn unknown_flag_errors() {
  let output = Command::new(binary_path())
    .arg("--bogus")
    .output()
    .expect("spawn binary");
  assert!(!output.status.success());
}

/// Live end-to-end test. Self-skips unless BOTH `MOONSHOT_API_KEY` and
/// (`MINIMAX_API_KEY` or `EDGE_TTS_OK=1` env opt-in) are set. Edge TTS
/// hits an anonymous Microsoft endpoint, so we gate it behind an
/// explicit env flag rather than running it by default in CI.
#[test]
#[ignore = "requires MOONSHOT_API_KEY + (MINIMAX_API_KEY or EDGE_TTS_OK=1); run with --ignored"]
fn live_blog_to_podcast_produces_audio_and_srt() {
  let moonshot = std::env::var("MOONSHOT_API_KEY");
  if moonshot.is_err() {
    eprintln!("skipping: MOONSHOT_API_KEY not set");
    return;
  }

  let (tts_flag, ok) = if std::env::var("MINIMAX_API_KEY").is_ok() {
    ("minimax", true)
  } else if std::env::var("EDGE_TTS_OK").is_ok() {
    ("edge", true)
  } else {
    ("", false)
  };
  if !ok {
    eprintln!("skipping: neither MINIMAX_API_KEY nor EDGE_TTS_OK is set");
    return;
  }

  let tmp = tempfile::tempdir().expect("tempdir");
  let output = tmp.path().join("episode.wav");
  let srt = tmp.path().join("episode.srt");

  let blog = manifest_dir().join("fixtures/short_blog.md");
  let result = Command::new(binary_path())
    .arg("--blog")
    .arg(&blog)
    .arg("--output")
    .arg(&output)
    .arg("--segments")
    .arg("4") // keep cost / time small for smoke
    .arg("--tts")
    .arg(tts_flag)
    .output()
    .expect("spawn binary");

  let stdout = String::from_utf8_lossy(&result.stdout);
  let stderr = String::from_utf8_lossy(&result.stderr);
  assert!(
    result.status.success(),
    "binary exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
  );

  let audio_meta = std::fs::metadata(&output).expect("audio file missing");
  assert!(
    audio_meta.len() > 1024,
    "audio file unexpectedly small ({} bytes)",
    audio_meta.len()
  );
  let srt_meta = std::fs::metadata(&srt).expect("srt file missing");
  assert!(srt_meta.len() > 16, "srt file unexpectedly small");
}
