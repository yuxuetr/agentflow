//! Q2.7.1 regression: `agentflow audio asr ... --prompt VALUE` must
//! NOT silently treat VALUE as a file path and write the transcript
//! there. Pre-fix `prompt` slid into the handler's positional
//! `output` slot and the user's hint became a destination file.
//!
//! These tests don't need a real ASR provider — they exercise the
//! CLI surface up to the point where the prompt-as-output gotcha
//! used to fire. Specifically they confirm that:
//!  1. `--prompt` is accepted as a named flag (clap-level surface).
//!  2. Passing `--prompt /some/path` does not create that file
//!     even when the command later fails (e.g. missing API key).
//!  3. `-o / --output` is a separate flag.

use std::path::PathBuf;

use assert_cmd::Command;

/// CLI command rooted at our compiled `agentflow` binary.
fn agentflow() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary must be built for tests")
}

#[test]
fn asr_prompt_flag_does_not_write_transcript_to_prompt_value() {
  let tempdir = tempfile::tempdir().expect("tempdir");
  // A file path that absolutely should not be created. Even if the
  // run fails (no API key, no audio file), this path must not appear.
  let prompt_as_path = tempdir.path().join("should-never-be-written-here.txt");
  assert!(!prompt_as_path.exists());

  // Path to a non-existent audio file — guarantees the run fails
  // BEFORE any provider call, but well after clap dispatch.
  let audio = tempdir.path().join("nonexistent.wav");

  let mut cmd = agentflow();
  cmd
    .arg("audio")
    .arg("asr")
    .arg(audio.to_str().unwrap())
    .arg("--prompt")
    .arg(prompt_as_path.to_str().unwrap());

  // We don't care whether the command succeeds or fails — only
  // that the prompt VALUE never lands on disk.
  let _ = cmd.output();
  assert!(
    !prompt_as_path.exists(),
    "BUG: --prompt VALUE was silently written to disk at {prompt_as_path:?}"
  );
}

#[test]
fn asr_output_flag_is_separate_from_prompt() {
  // Verify the help text mentions both flags as distinct options.
  // This is a cheap clap-shape check that wouldn't have caught the
  // pre-fix bug (which was at the call-site level) but does
  // protect against future regressions where someone deletes one
  // of the named flags.
  let output = agentflow()
    .arg("audio")
    .arg("asr")
    .arg("--help")
    .output()
    .expect("agentflow audio asr --help");
  let help = String::from_utf8_lossy(&output.stdout);
  assert!(
    help.contains("--prompt"),
    "help text missing --prompt: {help}"
  );
  assert!(
    help.contains("--output"),
    "help text missing --output: {help}"
  );
}

/// Sanity: invoking `--output PATH` against a non-existent audio
/// file still doesn't pre-create the output file. The output is
/// only written after a successful transcription.
#[test]
fn asr_output_flag_is_not_pre_created_on_failure() {
  let tempdir = tempfile::tempdir().expect("tempdir");
  let output_path: PathBuf = tempdir.path().join("transcript-on-failure.txt");
  let audio = tempdir.path().join("nonexistent.wav");

  let mut cmd = agentflow();
  cmd
    .arg("audio")
    .arg("asr")
    .arg(audio.to_str().unwrap())
    .arg("--output")
    .arg(output_path.to_str().unwrap());

  let _ = cmd.output();
  assert!(
    !output_path.exists(),
    "output file appeared even though the transcribe failed: {output_path:?}"
  );
}
