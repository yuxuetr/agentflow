use anyhow::{Context, Result};
use std::path::Path;
use mime::Mime;

pub fn detect_file_type(path: &Path) -> Result<String> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        // Text files
        "txt" | "md" | "rst" | "log" | "csv" | "json" | "yaml" | "yml" | "toml" | "xml" => {
            Ok("text".to_string())
        }
        
        // Code files
        "rs" | "py" | "js" | "ts" | "java" | "c" | "cpp" | "h" | "hpp" | "go" | "rb" | "php" 
        | "swift" | "kt" | "scala" | "clj" | "hs" | "ml" | "fs" | "elm" | "dart" | "r" 
        | "sql" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => {
            Ok("text".to_string())
        }

        // Image files
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "svg" | "webp" | "ico" => {
            Ok("image".to_string())
        }

        // Audio files
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" => {
            Ok("audio".to_string())
        }

        // Video files
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" | "m4v" => {
            Ok("video".to_string())
        }

        // Default to unknown
        _ => Ok("unknown".to_string()),
    }
}

pub fn get_mime_type(path: &Path) -> Result<Mime> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mime_str = match extension.as_str() {
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "yaml" | "yml" => "application/x-yaml",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "html" => "text/html",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    };

    mime_str.parse()
        .with_context(|| format!("Failed to parse MIME type: {}", mime_str))
}