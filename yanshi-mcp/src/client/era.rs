//! Era selection for a connecting [`super::MCPClient`] (W5.8-4).

use crate::protocol::modern::McpEra;
use crate::transport::TransportType;

/// Classify which era `MCPClient::connect()` should speak, from the
/// transport's type.
///
/// This is a deliberate narrowing from the RFC's full design
/// (`docs/RFC_MCP_PROTOCOL_MODERNIZATION.md`), which describes genuine
/// runtime probing — stdio: try `server/discover` first, fall back to
/// `initialize` on an unrecognized response; Streamable HTTP: attempt a
/// modern POST first, fall back on an unrecognized `400`/`404`/`405` —
/// so a single transport could in principle serve either era depending
/// on what the remote process/origin actually speaks.
///
/// This crate's [`crate::transport::StdioTransport`] has only ever
/// implemented the Legacy `initialize()` handshake, and plenty of
/// real-world stdio MCP servers (including this crate's own test
/// fixtures throughout `transport/stdio.rs` and `client/*.rs`) assume
/// the very first line they read is an `initialize` request — sending
/// an unsolicited `server/discover` probe first is not proven safe
/// against those without testing against a real Modern stdio server,
/// which this environment has no access to. Rather than risk silently
/// changing wire behavior for every existing Legacy stdio consumer to
/// implement a probe that can't be verified end-to-end, era is instead
/// determined by transport type: [`TransportType::StreamableHttp`] is
/// exclusively the Modern-era transport in this crate (built in W5.8-3
/// specifically for `2026-07-28`'s stateless shape) — every other
/// transport (stdio, and the unimplemented placeholder `Http`/
/// `HttpWithSSE` variants) is Legacy.
///
/// This still ships real Phase 2 value — Modern-era requests now work
/// end-to-end over `StreamableHttpTransport` — with zero behavior change
/// to any existing Legacy stdio code path or downstream consumer. True
/// cross-era probing on a single transport is left as a documented
/// follow-up (see `TODOs.md` W5.8).
pub(super) fn era_for_transport(transport_type: TransportType) -> McpEra {
  match transport_type {
    TransportType::StreamableHttp => McpEra::Modern,
    TransportType::Stdio | TransportType::Http | TransportType::HttpWithSSE => McpEra::Legacy,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn streamable_http_is_modern() {
    assert_eq!(
      era_for_transport(TransportType::StreamableHttp),
      McpEra::Modern
    );
  }

  #[test]
  fn every_other_transport_is_legacy() {
    assert_eq!(era_for_transport(TransportType::Stdio), McpEra::Legacy);
    assert_eq!(era_for_transport(TransportType::Http), McpEra::Legacy);
    assert_eq!(
      era_for_transport(TransportType::HttpWithSSE),
      McpEra::Legacy
    );
  }
}
