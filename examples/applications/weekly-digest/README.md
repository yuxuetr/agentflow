# A5 — weekly-digest

**Status**: TODO (scaffold only)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A5](../../../EXAMPLES_TODOs.md#a5--weekly-digest)

## Business

On a fixed weekly schedule, query a RAG index (the
[`research-assistant`](../research-assistant/) output, a personal blog
archive, a saved-articles dump, etc.) for the past 7 days of new
content; LLM generates a digest; an HTTP node ships it via SMTP /
SendGrid / Mailgun to a configured recipient list.

## Architecture (planned)

```
schedule (weekly Mon 09:00) →
  rag search (filter: ingested in last 7 days) →
  llm write digest →
  http_node POST to SendGrid /v3/mail/send →
  on failure: retry + record "last successful run" timestamp →
  on success: update timestamp
```

## External dependencies

| Dep | Why |
| --- | --- |
| Email provider | SendGrid / Mailgun (HTTPS API) or raw SMTP |
| Persistent RAG index | Pre-existing — e.g. A3's output |
| Scheduling | OS cron, AgentFlow's `/schedule` system, or systemd timer |

## What this validates in AgentFlow

- Long-running scheduled execution (not just interactive `agentflow run`)
- RAG `search` with date / time-range filters
- HTTP node calling an external API with bearer auth
- Failure tolerance: retry + last-success timestamp persistence
- Unattended reliability (run for 4 consecutive weeks without
  hand-holding before considering DONE)

## Findings during dogfooding

_Pending implementation._
