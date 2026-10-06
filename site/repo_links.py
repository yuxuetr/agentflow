#!/usr/bin/env python3
"""mdBook preprocessor: send links that leave the book to GitHub.

Pages under docs/ are written to be read on GitHub, so they link with relative
paths to source files (../yanshi-core/src/flow.rs) and to docs that are
deliberately not chapters (archive/, dated evaluations). Rendered as-is those
links 404 on the site. Every relative link whose target is not a chapter (or a
non-Markdown file mdBook copies alongside the pages) is rewritten to the file's
GitHub URL. A relative link whose target does not exist fails the build, so a
renamed source file shows up in CI instead of as a dead link on the site.
"""

import json
import re
import sys
from pathlib import Path
from typing import Any, Iterator
from urllib.parse import unquote

# `](target` of an inline link or image; the target ends at `)` or whitespace
# (whitespace introduces an optional "title").
LINK = re.compile(r"\]\(([^)\s]+)")
# CommonMark fence: a run of 3+ backticks or tildes, optionally followed by an
# info string. Only a run of the same character, at least as long, with no info
# string closes a block — so a stray "```yaml" inside a block is content.
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
URL_SCHEME = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")


def chapters(items: list[Any]) -> Iterator[dict[str, Any]]:
  for item in items:
    if isinstance(item, dict) and "Chapter" in item:
      yield item["Chapter"]
      yield from chapters(item["Chapter"]["sub_items"])


class LinkRewriter:
  def __init__(self, src_dir: Path, repo_root: Path, repo_url: str, book_paths: set[str]) -> None:
    self.src_dir = src_dir
    self.repo_root = repo_root
    self.repo_url = repo_url
    self.book_paths = book_paths
    self.broken: list[str] = []

  def rewrite_chapter(self, chapter: dict[str, Any]) -> None:
    page = chapter["path"]
    lines = chapter["content"].split("\n")
    open_fence: str | None = None
    for i, line in enumerate(lines):
      fence = FENCE.match(line)
      if open_fence is None:
        if fence:
          open_fence = fence.group(1)
        else:
          lines[i] = LINK.sub(lambda m: "](" + self.rewrite_target(page, m.group(1)), line)
      elif (
        fence
        and fence.group(1)[0] == open_fence[0]
        and len(fence.group(1)) >= len(open_fence)
        and not fence.group(2).strip()
      ):
        open_fence = None
    chapter["content"] = "\n".join(lines)

  def rewrite_target(self, page: str, target: str) -> str:
    if URL_SCHEME.match(target) or target.startswith(("#", "/", "<")):
      return target
    path, hash_sign, fragment = target.partition("#")
    resolved = (self.src_dir / Path(page).parent / unquote(path)).resolve()
    if not resolved.exists():
      self.broken.append(f"{page}: {target}")
      return target
    try:
      repo_path = resolved.relative_to(self.repo_root).as_posix()
    except ValueError:
      self.broken.append(f"{page}: {target} (outside the repository)")
      return target
    if resolved.is_relative_to(self.src_dir):
      in_book = resolved.relative_to(self.src_dir).as_posix() in self.book_paths
      if in_book or (resolved.is_file() and resolved.suffix != ".md"):
        return target
    kind = "tree" if resolved.is_dir() else "blob"
    return f"{self.repo_url}/{kind}/main/{repo_path}{hash_sign}{fragment}"


def main() -> None:
  if len(sys.argv) > 1:  # `repo_links.py supports <renderer>`: any renderer
    sys.exit(0)
  context, book = json.load(sys.stdin)
  book_root = Path(context["root"]).resolve()
  config = context["config"]
  rewriter = LinkRewriter(
    src_dir=(book_root / config["book"].get("src", "src")).resolve(),
    # site/ sits at the repository root.
    repo_root=book_root.parent,
    repo_url=config["output"]["html"]["git-repository-url"].rstrip("/"),
    book_paths={chapter["path"] for chapter in chapters(book["items"]) if chapter["path"]},
  )
  for chapter in chapters(book["items"]):
    if chapter["path"]:
      rewriter.rewrite_chapter(chapter)
  if rewriter.broken:
    print(f"repo-links: {len(rewriter.broken)} relative link(s) point at missing files:", file=sys.stderr)
    for entry in rewriter.broken:
      print(f"  {entry}", file=sys.stderr)
    sys.exit(1)
  json.dump(book, sys.stdout)


if __name__ == "__main__":
  main()
