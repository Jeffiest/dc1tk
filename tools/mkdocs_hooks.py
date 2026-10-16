"""MkDocs hooks for the documentation site (mkdocs.yml).

The pages are GitHub Markdown, and their relative links are right in the repository. A link whose
target the site does not have (a file outside docs/, such as src/docs/ or definitions/, or a file
of docs/ the site leaves out, such as an example's sources) is pointed at that file on GitHub.
Headings get GitHub's anchors, so the links' anchors are the same on both.
"""

import os
import posixpath
import re

LINK = re.compile(r"(\]\()([^)\s]+)(\))")
FENCE = re.compile(r"^\s*(```|~~~)")


def slugify(text, separator):
    """A heading's anchor as GitHub makes it: lower case, every character but letters, digits,
    spaces, `-` and `_` dropped, each space a `-` (runs are kept, not merged)."""
    text = text.strip().lower()
    return "".join(c for c in text if c.isalnum() or c in " -_").replace(" ", separator)


def on_config(config):
    config["mdx_configs"].setdefault("toc", {})["slugify"] = slugify
    return config


def on_page_markdown(markdown, page, config, files):
    blob = config["repo_url"].rstrip("/") + "/blob/main/"
    tree = config["repo_url"].rstrip("/") + "/tree/main/"
    docs_dir = config["docs_dir"]
    repo_dir = os.path.dirname(os.path.abspath(config["config_file_path"]))
    page_dir = posixpath.dirname(page.file.src_uri)
    docs_prefix = posixpath.relpath(os.path.abspath(docs_dir), repo_dir).replace(os.sep, "/")

    def github(match):
        target = match.group(2)
        if re.match(r"[a-z][a-z0-9+.-]*:", target) or target.startswith(("#", "/")):
            return match.group(0)
        path, hash_, fragment = target.partition("#")
        within_docs = posixpath.normpath(posixpath.join(page_dir, path))
        if not within_docs.startswith("../") and files.get_file_from_path(within_docs) is not None:
            return match.group(0)
        in_repo = posixpath.normpath(posixpath.join(docs_prefix, within_docs))
        base = tree if os.path.isdir(os.path.join(repo_dir, in_repo)) else blob
        return match.group(1) + base + in_repo + hash_ + fragment + match.group(3)

    out = []
    fenced = False
    for line in markdown.split("\n"):
        if FENCE.match(line):
            fenced = not fenced
        out.append(line if fenced else LINK.sub(github, line))
    return "\n".join(github_lists(out))


ITEM = re.compile(r"^( *)([-*+]|(\d+)[.)]) +")


def interrupts(match):
    """Whether a list item can start a list right under a paragraph, as GitHub has it: a bullet,
    or a number list that starts at 1."""
    return match.group(3) is None or match.group(3) == "1"


def github_lists(lines):
    """Lists as GitHub reads them, written so Python-Markdown reads them the same: a list right
    under a paragraph, or an item after one of several paragraphs or blocks, gets the blank line
    Python-Markdown needs before it (it reads the line as more of the paragraph), and what
    belongs to an item (its next lines, nested lists, code blocks) moves to the four-space indent
    Python-Markdown wants where the item's text starts sooner (`- ` and `1. `)."""
    out = []
    in_list = False
    content = 0  # where the text of the top-level item being read starts
    fence = None  # the indent of the open code block's fence, or None
    loose = False  # whether the item being read has a blank line in it
    for line in lines:
        indent = len(line) - len(line.lstrip(" "))
        shift = max(0, 4 - content) if in_list else 0
        if fence is not None:
            if FENCE.match(line):
                fence = None
            out.append(" " * shift + line if in_list and line.strip() else line)
            continue
        if not line.strip():
            loose = loose or in_list
            out.append(line)
            continue
        if FENCE.match(line):
            fence = indent
            if in_list and indent >= content:
                out.append(" " * shift + line)
            else:
                in_list = in_list and indent > 0
                out.append(line)
            continue
        item = ITEM.match(line)
        previous = out[-1] if out else ""
        if indent == 0 and item and (interrupts(item) or (in_list and previous.strip())):
            after_text = not in_list and not previous.startswith(("#", "|", ">"))
            if previous.strip() and (after_text or (in_list and loose)):
                out.append("")
            in_list, loose = True, False
            content = len(item.group(0))
            out.append(line)
            continue
        if in_list and indent >= content:
            # an item that cannot start a list under the text before it stays that text
            lazy = item and not interrupts(item) and previous.strip()
            out.append(line if lazy else " " * shift + line)
            continue
        if indent == 0 and not previous.strip():
            in_list = loose = False
        out.append(line)
    return out
