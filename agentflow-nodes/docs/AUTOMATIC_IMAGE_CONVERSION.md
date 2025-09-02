# Automatic Image Conversion in AgentFlow Multimodal Models

## Overview

AgentFlow's multimodal LLM nodes automatically handle both local image files and remote HTTP/HTTPS URLs, with intelligent conversion based on the image source type. This eliminates the need for manual base64 encoding while providing a seamless developer experience.

## How It Works

### Automatic Detection

When you provide image paths to a multimodal LLM node, AgentFlow automatically detects the image source type:

- **Remote URLs**: Images starting with `http://` or `https://` are used directly
- **Local Files**: All other paths are treated as local files and automatically converted to base64 data URLs

### Implementation Details

```rust
// In LlmNode::execute_real_llm()
if image_path.starts_with("http://") || image_path.starts_with("https://") {
    // Remote URL - use as is
    content.push(MessageContent::image_url(image_path));
} else {
    // Local file - convert to base64 data
    match Self::convert_local_image_to_base64(image_path).await {
        Ok((data_url, media_type)) => {
            content.push(MessageContent::image_data(data_url, media_type));
        }
        Err(e) => {
            println!("⚠️  Failed to read local image '{}': {}", image_path, e);
            continue; // Skip this image
        }
    }
}
```

### Base64 Conversion Process

For local files, AgentFlow:

1. **Reads the file** using async file I/O
2. **Detects MIME type** from file extension:
   - `.jpg`, `.jpeg` → `image/jpeg`
   - `.png` → `image/png`
   - `.gif` → `image/gif`
   - `.webp` → `image/webp`
   - `.bmp` → `image/bmp`
   - `.svg` → `image/svg+xml`
3. **Encodes to base64** using standard base64 encoding
4. **Creates data URL** in format: `data:{mime_type};base64,{base64_data}`

## Usage Examples

### Basic Usage

```rust
use agentflow_nodes::LlmNode;

// Works with local files (auto-converted to base64)
let analyzer = LlmNode::new("analyzer", "step-1o-turbo-vision")
    .with_prompt("Analyze this image")
    .with_images(vec!["./my_image.jpg".to_string()]);

// Works with remote URLs (used directly)
let analyzer = LlmNode::new("analyzer", "step-1o-turbo-vision")
    .with_prompt("Analyze this image")
    .with_images(vec!["https://example.com/image.png".to_string()]);
```

### Mixed Sources

```rust
// Mix local and remote images in the same request
let analyzer = LlmNode::new("analyzer", "step-1o-turbo-vision")
    .with_prompt("Compare these images")
    .with_images(vec![
        "./local_diagram.jpg".to_string(),
        "https://example.com/remote_chart.png".to_string()
    ]);
```

### With Shared State

```rust
use agentflow_core::SharedState;
use serde_json::Value;

let shared = SharedState::new();
shared.insert("image_path".to_string(), Value::String("./architecture.png".to_string()));

let analyzer = LlmNode::new("analyzer", "step-1o-turbo-vision")
    .with_prompt("Analyze this architecture: {{description}}")
    .with_images(vec!["image_path".to_string()]); // References shared state key
```

## Supported Image Formats

### Local Files (Auto-converted to Base64)
- JPEG (`.jpg`, `.jpeg`)
- PNG (`.png`)
- GIF (`.gif`)
- WebP (`.webp`)
- BMP (`.bmp`)
- SVG (`.svg`)

### Remote URLs (Used Directly)
- Any HTTP/HTTPS URL pointing to an image
- No format restrictions (handled by the LLM provider)

## Error Handling

### Local File Issues
- **File not found**: Warning logged, image skipped
- **Read permission denied**: Warning logged, image skipped
- **Large file size**: May hit API limits (varies by provider)

### Remote URL Issues
- **Network errors**: Handled by LLM provider
- **Invalid URLs**: Handled by LLM provider
- **Authentication**: Must be publicly accessible

## Performance Considerations

### Base64 Encoding
- **Size increase**: ~33% larger than original file
- **Memory usage**: File loaded into memory during conversion
- **Processing time**: Minimal overhead for typical image sizes

### Recommendations
- **Local files**: Keep under 10MB for best performance
- **Remote URLs**: Preferred for large images or frequently accessed images
- **Caching**: Consider hosting frequently used images remotely

## Examples

### Updated Examples
All multimodal examples have been updated to demonstrate automatic conversion:

1. **`multimodal_base64_example.rs`**: Shows automatic local file conversion
2. **`multimodal_http_example.rs`**: Shows remote URL usage
3. **`multimodal_llm_example.rs`**: Shows comprehensive multimodal workflow
4. **`mixed_image_sources_example.rs`**: Shows mixed local/remote usage

### Running Examples

```bash
# Test automatic base64 conversion
cargo run --example multimodal_base64_example

# Test mixed image sources
cargo run --example mixed_image_sources_example

# Test remote URL usage
cargo run --example multimodal_http_example
```

## Migration from Manual Base64

### Before (Manual Base64)
```rust
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::fs;

// Manual base64 conversion
let image_bytes = fs::read("image.jpg")?;
let base64_data = STANDARD.encode(&image_bytes);
let data_url = format!("data:image/jpeg;base64,{}", base64_data);

shared.insert("image".to_string(), Value::String(data_url));
```

### After (Automatic Conversion)
```rust
// Just use the file path directly
shared.insert("image".to_string(), Value::String("image.jpg".to_string()));
```

## Benefits

### Developer Experience
- **No manual encoding**: AgentFlow handles base64 conversion automatically
- **Consistent API**: Same API for local files and remote URLs
- **Error resilience**: Graceful handling of missing files or network issues
- **Format detection**: Automatic MIME type detection from file extensions

### Performance
- **Lazy conversion**: Only converts when actually needed
- **Memory efficient**: Streaming approach for large files
- **Async I/O**: Non-blocking file operations

### Flexibility
- **Mixed workflows**: Combine local and remote images seamlessly
- **Template support**: Use shared state variables for dynamic image paths
- **Production ready**: Suitable for both development and production environments

## Troubleshooting

### Common Issues

1. **Local image not found**
   ```
   ⚠️  Failed to read local image './missing.jpg': No such file or directory
   ```
   **Solution**: Verify file path and existence

2. **Large file warnings**
   ```
   ⚠️  Warning: Image is quite large. Some APIs have size limits.
   ```
   **Solution**: Resize image or use remote hosting

3. **Permission denied**
   ```
   ⚠️  Failed to read local image './secure.jpg': Permission denied
   ```
   **Solution**: Check file permissions

### Best Practices

1. **File Paths**: Use relative paths from your working directory
2. **Size Limits**: Keep local images under 10MB
3. **Format Support**: Stick to common formats (JPEG, PNG)
4. **Error Handling**: Check logs for image processing warnings
5. **Testing**: Test with both local and remote images

---

This automatic image conversion feature makes multimodal AI workflows much more developer-friendly while maintaining full flexibility for production use cases.