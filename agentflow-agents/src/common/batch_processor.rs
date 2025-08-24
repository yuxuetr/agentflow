//! Batch processing utilities for agents

use std::sync::Arc;
use tokio::sync::Semaphore;

/// Batch processor with concurrency control
pub struct BatchProcessor {
  concurrency_limit: usize,
}

impl BatchProcessor {
  pub fn new(concurrency_limit: usize) -> Self {
    Self { concurrency_limit }
  }

  /// Process items concurrently with semaphore control
  pub async fn process_concurrent<T, R, F, Fut>(
    &self,
    items: Vec<T>,
    processor: F
  ) -> Vec<(T, crate::AgentResult<R>)>
  where
    T: Clone + Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = crate::AgentResult<R>> + Send,
  {
    let semaphore = Arc::new(Semaphore::new(self.concurrency_limit));
    let processor = Arc::new(processor);
    let mut handles = Vec::new();

    for item in items {
      let sem = semaphore.clone();
      let proc = processor.clone();
      let item_clone = item.clone();

      let handle = tokio::spawn(async move {
        let _permit = sem.acquire().await.unwrap();
        let result = proc(item_clone.clone()).await;
        (item_clone, result)
      });
      
      handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
      match handle.await {
        Ok((item, result)) => results.push((item, result)),
        Err(e) => {
          // Handle join error - create a synthetic error result
          // This is tricky because we don't have the original item
          // In practice, this should rarely happen
          eprintln!("Task join error: {}", e);
        }
      }
    }

    results
  }

  /// Process items with progress reporting
  pub async fn process_with_progress<T, R, F, Fut>(
    &self,
    items: Vec<T>,
    processor: F,
    progress_callback: impl Fn(usize, usize) + Send + Sync + 'static
  ) -> Vec<(T, crate::AgentResult<R>)>
  where
    T: Clone + Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = crate::AgentResult<R>> + Send,
  {
    let total = items.len();
    let semaphore = Arc::new(Semaphore::new(self.concurrency_limit));
    let processor = Arc::new(processor);
    let progress = Arc::new(progress_callback);
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut handles = Vec::new();

    for (_index, item) in items.into_iter().enumerate() {
      let sem = semaphore.clone();
      let proc = processor.clone();
      let prog = progress.clone();
      let comp = completed.clone();

      let handle = tokio::spawn(async move {
        let _permit = sem.acquire().await.unwrap();
        let result = proc(item.clone()).await;
        
        let current = comp.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        prog(current, total);
        
        (item, result)
      });
      
      handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
      if let Ok((item, result)) = handle.await {
        results.push((item, result));
      }
    }

    results
  }
}

/// Default batch processor with reasonable concurrency limit
pub fn default_batch_processor() -> BatchProcessor {
  BatchProcessor::new(3)
}