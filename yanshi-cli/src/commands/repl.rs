//! Shared interactive line reader for the chat REPLs (`harness chat` +
//! `skill chat`) — H.4.1.
//!
//! On a TTY it wraps [`rustyline`] for line editing + up/down history. When
//! stdin is not a TTY (piped input, integration tests) it falls back to a plain
//! async line reader so scripted/captured behaviour is identical to the
//! pre-H.4.1 REPL — the editing features only matter to a human at a terminal.
//!
//! `rustyline::Editor::readline` is blocking and owns the terminal in raw mode,
//! so the TTY path runs it on [`tokio::task::spawn_blocking`], moving the editor
//! onto the blocking thread and back each call to preserve the history.

use anyhow::{Context, Result, anyhow};
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin};

/// One line of REPL input.
pub enum ReadLine {
  /// A line the user submitted (terminating newline already stripped).
  Line(String),
  /// Ctrl-C at the prompt — abandon the current line, keep the REPL running.
  Interrupted,
  /// Ctrl-D / end of piped input — leave the REPL.
  Eof,
}

enum Backend {
  /// Interactive: a rustyline editor, taken onto the blocking thread per read
  /// and put back (so history survives across reads). `None` only transiently
  /// while a read is in flight. Boxed — the editor is far larger than the
  /// piped variant.
  Editor(Option<Box<rustyline::DefaultEditor>>),
  /// Non-interactive (piped / tests): plain async line reader — the exact
  /// pre-H.4.1 behaviour.
  Piped(Lines<BufReader<Stdin>>),
}

/// Reads REPL input lines, with editing + history on a TTY and a transparent
/// plain-reader fallback otherwise. Both chat REPLs share one of these so the
/// experience (and the H.2.1 approval prompt that reads from the same input)
/// stays consistent.
pub struct LineReader {
  backend: Backend,
}

impl Default for LineReader {
  fn default() -> Self {
    Self::new()
  }
}

impl LineReader {
  /// Build a reader: the interactive editor when stdin is a TTY and rustyline
  /// initialises, else the piped fallback.
  pub fn new() -> Self {
    if std::io::IsTerminal::is_terminal(&std::io::stdin())
      && let Ok(editor) = rustyline::DefaultEditor::new()
    {
      return Self {
        backend: Backend::Editor(Some(Box::new(editor))),
      };
    }
    Self {
      backend: Backend::Piped(BufReader::new(tokio::io::stdin()).lines()),
    }
  }

  /// Read one line, showing `prompt`. Editing + history apply on the TTY path;
  /// the piped path mirrors the prompt to stderr and reads a raw line.
  pub async fn read_line(&mut self, prompt: &str) -> Result<ReadLine> {
    match &mut self.backend {
      Backend::Editor(slot) => {
        let editor = slot
          .take()
          .ok_or_else(|| anyhow!("line editor missing between reads"))?;
        let prompt = prompt.to_string();
        let (editor, result) = tokio::task::spawn_blocking(move || {
          let mut editor = editor;
          let line = editor.readline(&prompt);
          (editor, line)
        })
        .await
        .context("line editor task panicked")?;
        *slot = Some(editor);
        use rustyline::error::ReadlineError;
        match result {
          Ok(line) => {
            if !line.trim().is_empty()
              && let Some(editor) = slot.as_mut()
            {
              let _ = editor.add_history_entry(line.as_str());
            }
            Ok(ReadLine::Line(line))
          }
          Err(ReadlineError::Interrupted) => Ok(ReadLine::Interrupted),
          Err(ReadlineError::Eof) => Ok(ReadLine::Eof),
          Err(err) => Err(anyhow!("line editor error: {err}")),
        }
      }
      Backend::Piped(reader) => {
        // The editor prints its own prompt; mirror it to stderr here so an
        // interactive-over-pipe session still shows it (tests assert on stdout).
        use std::io::Write;
        eprint!("{prompt}");
        std::io::stderr().flush().ok();
        match reader.next_line().await.context("failed to read stdin")? {
          Some(line) => Ok(ReadLine::Line(line)),
          None => Ok(ReadLine::Eof),
        }
      }
    }
  }
}
