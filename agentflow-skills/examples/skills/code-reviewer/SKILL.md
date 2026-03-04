---
name: code-reviewer
description: Review code for quality, security, and best practices. Use when the user wants code reviewed, audited, or analysed for bugs, smells, or improvements.
license: MIT
compatibility: Requires python3 for script-based analysis helpers
metadata:
  author: agentflow-examples
  version: "1.0"
allowed-tools: shell file script
---

# Code Reviewer

## When to use this skill
Use this skill when the user wants to:
- Review code for bugs, security issues, or anti-patterns
- Get a quality assessment of a file or function
- Understand the complexity or maintainability of code

## How to review code

### Step 1 — Understand the context
Read the files the user mentions. Use the `file` tool to read source files.

### Step 2 — Run static analysis (optional)
Use the `script` tool to run `analyse.py` for automated metrics:

```
script: analyse.py
args: {"path": "<file to analyse>"}
```

### Step 3 — Provide structured feedback
Structure your review with these sections:

1. **Summary** — one paragraph overview of the code's purpose and quality
2. **Issues** — numbered list, most critical first
3. **Suggestions** — concrete improvements with example code
4. **Verdict** — overall rating: Excellent / Good / Needs Work / Critical Issues

## Common edge cases
- If the user provides a git diff rather than a full file, focus on changed lines only
- For large files (> 500 lines), ask the user to narrow the scope
- Always check for hardcoded secrets, SQL injection, and unsafe deserialization

## Examples

**Input**: "Review my auth.py for security issues"
**Action**: Read the file, run `analyse.py {"path": "auth.py"}`, then provide structured feedback.
