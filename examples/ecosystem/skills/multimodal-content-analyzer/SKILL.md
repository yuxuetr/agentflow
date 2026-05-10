---
name: multimodal-content-analyzer
description: Analyze image, audio, transcript, and document context supplied by AgentFlow workflows.
license: Apache-2.0
compatibility: AgentFlow v1 stability inventory
allowed-tools: file
metadata:
  version: "1.0.0"
  mode: offline-first
security:
  allow_shell: false
  allow_file_read: true
  allow_file_write: false
---

# Multimodal Content Analyzer

Inspect the text, captions, transcripts, OCR, image-understanding output, or
artifact metadata supplied by the workflow. Summarize what is present, what is
uncertain, and what should be checked with a live multimodal provider if the
current run used mock data.

Prefer structured output:

- content type
- key observations
- quality or safety concerns
- recommended next action

Do not claim to directly see or hear media unless the workflow supplied a
model-generated observation or transcript in the prompt context.
