#!/usr/bin/env python3
"""
Code analysis helper for the code-reviewer skill.

Reads a JSON object from stdin with the following fields:
  path  (str)  - path to the source file to analyse

Outputs a JSON object with:
  lines       (int)   - total line count
  blank_lines (int)   - blank line count
  complexity  (str)   - "low" / "medium" / "high" (based on line count heuristic)
  warnings    (list)  - list of warning strings (hardcoded patterns detected)
"""

import json
import sys
import os
import re


def analyse(path: str) -> dict:
    if not os.path.isfile(path):
        return {"error": f"File not found: {path}"}

    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            lines = f.readlines()
    except OSError as e:
        return {"error": str(e)}

    total = len(lines)
    blank = sum(1 for l in lines if l.strip() == "")
    content_lines = total - blank

    if content_lines < 100:
        complexity = "low"
    elif content_lines < 300:
        complexity = "medium"
    else:
        complexity = "high"

    # Simple pattern checks
    warnings = []
    danger_patterns = [
        (r'password\s*=\s*["\'][^"\']+["\']', "Possible hardcoded password"),
        (r'secret\s*=\s*["\'][^"\']+["\']', "Possible hardcoded secret"),
        (r'api_key\s*=\s*["\'][^"\']+["\']', "Possible hardcoded API key"),
        (r'eval\s*\(', "Use of eval() detected"),
        (r'exec\s*\(', "Use of exec() detected"),
        (r'TODO|FIXME|HACK|XXX', "TODO/FIXME comment found"),
    ]
    for i, line in enumerate(lines, start=1):
        for pattern, msg in danger_patterns:
            if re.search(pattern, line, re.IGNORECASE):
                warnings.append(f"Line {i}: {msg}")
                break  # one warning per line

    return {
        "lines": total,
        "blank_lines": blank,
        "complexity": complexity,
        "warnings": warnings[:10],  # cap at 10 to keep output concise
    }


def main():
    raw = sys.stdin.read().strip()
    if raw:
        try:
            params = json.loads(raw)
        except json.JSONDecodeError:
            params = {}
    else:
        params = {}

    path = params.get("path", "")
    if not path:
        print(json.dumps({"error": "Missing required parameter 'path'"}))
        sys.exit(1)

    result = analyse(path)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
