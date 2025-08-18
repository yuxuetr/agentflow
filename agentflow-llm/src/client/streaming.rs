use crate::Result;
use async_trait::async_trait;
use futures::{Future, Stream};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Represents a chunk of data from a streaming LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
  /// The content/delta for this chunk (usually text, but could be multimodal in future)
  pub content: String,
  /// Whether this is the final chunk
  pub is_final: bool,
  /// Optional metadata associated with this chunk
  pub metadata: Option<serde_json::Value>,
  /// Token usage information (if available)
  pub usage: Option<TokenUsage>,
  /// Content type hint for this chunk (e.g., "text", "image", "audio")
  pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
  pub prompt_tokens: Option<u32>,
  pub completion_tokens: Option<u32>,
  pub total_tokens: Option<u32>,
}

/// Trait for streaming LLM responses
#[async_trait]
pub trait StreamingResponse: Send + Sync {
  /// Get the next chunk from the stream
  async fn next_chunk(&mut self) -> Result<Option<StreamChunk>>;

  /// Collect all chunks into a single response
  async fn collect_all(mut self) -> Result<String>
  where
    Self: Sized,
  {
    let mut content = String::new();
    while let Some(chunk) = self.next_chunk().await? {
      content.push_str(&chunk.content);
    }
    Ok(content)
  }

  /// Convert to a Stream for use with async stream combinators
  fn into_stream(self) -> Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>
  where
    Self: Sized + Unpin + 'static,
  {
    Box::pin(StreamingResponseStream::new(self))
  }
}

/// Internal helper to convert StreamingResponse to Stream
struct StreamingResponseStream<T: StreamingResponse> {
  response: T,
}

impl<T: StreamingResponse> StreamingResponseStream<T> {
  fn new(response: T) -> Self {
    Self { response }
  }
}

impl<T: StreamingResponse + Unpin> Stream for StreamingResponseStream<T> {
  type Item = Result<StreamChunk>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    let future = self.response.next_chunk();
    tokio::pin!(future);

    match Future::poll(future, cx) {
      std::task::Poll::Ready(Ok(Some(chunk))) => std::task::Poll::Ready(Some(Ok(chunk))),
      std::task::Poll::Ready(Ok(None)) => std::task::Poll::Ready(None),
      std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Some(Err(e))),
      std::task::Poll::Pending => std::task::Poll::Pending,
    }
  }
}
