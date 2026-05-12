use std::{net::IpAddr, sync::Arc};

use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url, header::LOCATION, redirect::Policy};
use serde_json::{Value, json};

use crate::{
  Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput,
  sandbox::{NetworkAddressClass, SandboxPolicy},
};

const MAX_REDIRECTS: usize = 10;
const CLOUD_METADATA_HOSTS: &[&str] = &[
  "metadata.google.internal",
  "metadata",
  "instance-data",
  "instance-data.ec2.internal",
];
const CLOUD_METADATA_IPS: &[IpAddr] = &[
  IpAddr::V4(std::net::Ipv4Addr::new(169, 254, 169, 254)),
  IpAddr::V4(std::net::Ipv4Addr::new(100, 100, 100, 200)),
];

/// Make HTTP GET / POST requests with domain sandbox enforcement.
pub struct HttpTool {
  client: Client,
  policy: Arc<SandboxPolicy>,
  /// Maximum response body size to return (truncate beyond this).
  max_response_chars: usize,
}

impl HttpTool {
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    let client_result = Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .redirect(Policy::none())
      .user_agent("AgentFlow/0.1")
      .build();
    let client = match client_result {
      Ok(client) => client,
      Err(error) => panic!("Failed to build HTTP client: {}", error),
    };

    Self {
      client,
      policy,
      max_response_chars: 8_000,
    }
  }

  pub fn default_policy() -> Self {
    Self::new(Arc::new(SandboxPolicy::default()))
  }

  fn extract_host(url: &str) -> Option<String> {
    url::Url::parse(url)
      .ok()
      .and_then(|u| u.host_str().map(String::from))
  }

  async fn validate_url_allowed(&self, url: &Url) -> Result<(), ToolError> {
    match url.scheme() {
      "http" | "https" => {}
      scheme => {
        return Err(ToolError::SandboxViolation {
          message: format!("HTTP tool does not allow '{}' URLs", scheme),
        });
      }
    }

    let host = url.host_str().ok_or_else(|| ToolError::InvalidParams {
      message: format!("Cannot parse host from URL: {}", url),
    })?;

    if is_cloud_metadata_host(host)
      && !self
        .policy
        .is_network_address_class_allowed(NetworkAddressClass::CloudMetadata)
    {
      return Err(ToolError::SandboxViolation {
        message: format!("Host '{}' is a cloud metadata endpoint", host),
      });
    }

    if !self.policy.is_domain_allowed(host) {
      return Err(ToolError::SandboxViolation {
        message: format!("Domain '{}' is not in the allowed-domains list", host),
      });
    }

    let addresses = resolve_host_ips(url, host).await?;
    for address in addresses {
      for class in classify_network_address(address) {
        if !self.policy.is_network_address_class_allowed(class) {
          return Err(ToolError::SandboxViolation {
            message: format!(
              "Address '{}' for host '{}' is denied by sandbox policy ({:?})",
              address, host, class
            ),
          });
        }
      }
    }

    Ok(())
  }
}

#[async_trait]
impl Tool for HttpTool {
  fn name(&self) -> &str {
    "http"
  }

  fn description(&self) -> &str {
    "Make HTTP GET or POST requests to fetch web content or call REST APIs. \
        Returns the response body as text (truncated to 8 000 characters)."
  }

  fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "properties": {
            "url": {
                "type": "string",
                "description": "Full URL to request"
            },
            "method": {
                "type": "string",
                "enum": ["GET", "POST"],
                "description": "HTTP method (default: GET)"
            },
            "body": {
                "type": "string",
                "description": "Request body string (for POST)"
            },
            "headers": {
                "type": "object",
                "description": "Optional key-value map of additional request headers"
            }
        },
        "required": ["url"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named(self.name())
  }

  fn idempotency(&self, params: &Value) -> ToolIdempotency {
    match params["method"].as_str().unwrap_or("GET") {
      "GET" => ToolIdempotency::Idempotent,
      "POST" => ToolIdempotency::NonIdempotent,
      _ => ToolIdempotency::Unknown,
    }
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let url = params["url"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "Missing required parameter 'url'".to_string(),
      })?;

    let mut current_url = Url::parse(url).map_err(|error| ToolError::InvalidParams {
      message: format!("Invalid URL '{}': {}", url, error),
    })?;

    let host = Self::extract_host(url).ok_or_else(|| ToolError::InvalidParams {
      message: format!("Cannot parse host from URL: {}", url),
    })?;
    drop(host);

    let method = params["method"].as_str().unwrap_or("GET");

    for redirect_count in 0..=MAX_REDIRECTS {
      self.validate_url_allowed(&current_url).await?;

      let mut builder = match method {
        "GET" => self.client.get(current_url.clone()),
        "POST" => self.client.post(current_url.clone()),
        other => {
          return Err(ToolError::InvalidParams {
            message: format!("Unsupported HTTP method '{}'. Use GET or POST", other),
          });
        }
      };

      // Attach custom headers
      if let Some(headers) = params["headers"].as_object() {
        for (k, v) in headers {
          if let Some(v_str) = v.as_str() {
            builder = builder.header(k.as_str(), v_str);
          }
        }
      }

      // Attach body
      if method == "POST"
        && let Some(body) = params["body"].as_str()
      {
        builder = builder.body(body.to_string());
      }

      let response = builder.send().await.map_err(|e| ToolError::HttpError {
        message: e.to_string(),
      })?;

      if is_redirect(response.status()) {
        let location = response
          .headers()
          .get(LOCATION)
          .ok_or_else(|| ToolError::HttpError {
            message: format!(
              "HTTP redirect from '{}' did not include Location",
              current_url
            ),
          })?
          .to_str()
          .map_err(|error| ToolError::HttpError {
            message: format!("Invalid redirect Location header: {}", error),
          })?;

        if redirect_count == MAX_REDIRECTS {
          return Err(ToolError::HttpError {
            message: format!("Too many redirects after {}", MAX_REDIRECTS),
          });
        }

        current_url = current_url
          .join(location)
          .map_err(|error| ToolError::HttpError {
            message: format!("Invalid redirect Location '{}': {}", location, error),
          })?;
        continue;
      }

      let status = response.status();
      let body = response.text().await.map_err(|e| ToolError::HttpError {
        message: e.to_string(),
      })?;

      // Truncate very long responses
      let content = if body.len() > self.max_response_chars {
        format!(
          "{}... [truncated — total {} chars]",
          &body[..self.max_response_chars],
          body.len()
        )
      } else {
        body
      };

      return if status.is_success() {
        Ok(ToolOutput::success(content))
      } else {
        Ok(ToolOutput::error(format!("HTTP {}: {}", status, content)))
      };
    }

    Err(ToolError::HttpError {
      message: format!("Too many redirects after {}", MAX_REDIRECTS),
    })
  }
}

fn is_redirect(status: StatusCode) -> bool {
  matches!(
    status,
    StatusCode::MOVED_PERMANENTLY
      | StatusCode::FOUND
      | StatusCode::SEE_OTHER
      | StatusCode::TEMPORARY_REDIRECT
      | StatusCode::PERMANENT_REDIRECT
  )
}

async fn resolve_host_ips(url: &Url, host: &str) -> Result<Vec<IpAddr>, ToolError> {
  if let Ok(address) = host.parse::<IpAddr>() {
    return Ok(vec![address]);
  }

  let port = url
    .port_or_known_default()
    .ok_or_else(|| ToolError::InvalidParams {
      message: format!("Cannot infer port for URL: {}", url),
    })?;

  let resolved = tokio::net::lookup_host((host, port))
    .await
    .map_err(|error| ToolError::HttpError {
      message: format!("Failed to resolve host '{}': {}", host, error),
    })?
    .map(|socket_addr| socket_addr.ip())
    .collect::<Vec<_>>();

  if resolved.is_empty() {
    return Err(ToolError::HttpError {
      message: format!("Host '{}' resolved to no addresses", host),
    });
  }

  Ok(resolved)
}

fn is_cloud_metadata_host(host: &str) -> bool {
  let lower = host.trim_end_matches('.').to_ascii_lowercase();
  CLOUD_METADATA_HOSTS
    .iter()
    .any(|metadata_host| lower == *metadata_host || lower.ends_with(&format!(".{}", metadata_host)))
}

fn classify_network_address(address: IpAddr) -> Vec<NetworkAddressClass> {
  let mut classes = Vec::new();

  if CLOUD_METADATA_IPS.contains(&address) {
    classes.push(NetworkAddressClass::CloudMetadata);
  }

  match address {
    IpAddr::V4(address) => {
      if address.is_loopback() {
        classes.push(NetworkAddressClass::Loopback);
      }
      if address.is_link_local() {
        classes.push(NetworkAddressClass::LinkLocal);
      }
      if address.is_private() {
        classes.push(NetworkAddressClass::Private);
      }
    }
    IpAddr::V6(address) => {
      if address.is_loopback() {
        classes.push(NetworkAddressClass::Loopback);
      }
      if (address.segments()[0] & 0xffc0) == 0xfe80 {
        classes.push(NetworkAddressClass::LinkLocal);
      }
      if (address.segments()[0] & 0xfe00) == 0xfc00 {
        classes.push(NetworkAddressClass::Private);
      }
    }
  }

  classes
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
  };

  #[tokio::test]
  async fn default_policy_blocks_loopback_ip() {
    let tool = HttpTool::default_policy();

    let result = tool
      .execute(json!({
        "url": "http://127.0.0.1:9"
      }))
      .await;

    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
    assert!(result.unwrap_err().to_string().contains("Loopback"));
  }

  #[tokio::test]
  async fn default_policy_blocks_localhost_dns_resolution() {
    let tool = HttpTool::default_policy();

    let result = tool
      .execute(json!({
        "url": "http://localhost:9"
      }))
      .await;

    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
    assert!(result.unwrap_err().to_string().contains("Loopback"));
  }

  #[tokio::test]
  async fn default_policy_blocks_private_ip() {
    let tool = HttpTool::default_policy();

    let result = tool
      .execute(json!({
        "url": "http://10.0.0.1"
      }))
      .await;

    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
    assert!(result.unwrap_err().to_string().contains("Private"));
  }

  #[tokio::test]
  async fn default_policy_blocks_cloud_metadata_ip() {
    let tool = HttpTool::default_policy();

    let result = tool
      .execute(json!({
        "url": "http://169.254.169.254/latest/meta-data/"
      }))
      .await;

    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
    let message = result.unwrap_err().to_string();
    assert!(message.contains("CloudMetadata") || message.contains("LinkLocal"));
  }

  #[tokio::test]
  async fn explicit_policy_allows_loopback() {
    let (url, server_task) =
      spawn_one_response_server("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await;
    let policy = Arc::new(SandboxPolicy {
      allow_loopback_network_access: true,
      ..SandboxPolicy::default()
    });
    let tool = HttpTool::new(policy);

    let output = tool.execute(json!({ "url": url })).await.unwrap();

    assert_eq!(output.content, "ok");
    server_task.await.unwrap();
  }

  #[tokio::test]
  async fn redirect_destination_is_checked_before_following() {
    let (url, server_task) = spawn_one_response_server(
      "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data/\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    let policy = Arc::new(SandboxPolicy {
      allow_loopback_network_access: true,
      ..SandboxPolicy::default()
    });
    let tool = HttpTool::new(policy);

    let result = tool.execute(json!({ "url": url })).await;

    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
    server_task.await.unwrap();
  }

  async fn spawn_one_response_server(
    response: &'static str,
  ) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let mut buffer = [0_u8; 1024];
      let _ = stream.read(&mut buffer).await.unwrap();
      stream.write_all(response.as_bytes()).await.unwrap();
    });

    (format!("http://{}", address), task)
  }
}
