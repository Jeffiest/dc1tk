#!/usr/bin/env python3
"""Writes THIRD-PARTY-LICENCES.md: the licences of every crate built into the dc2tk executable.

A release zip carries it beside the executable, since the MIT, Apache, BSD, Zlib and Unicode
licences of those crates ask for their notices to travel with the binary. The crates are the
normal (not build or dev) dependencies of the dc2tk package, followed from cargo's resolve for the
release's targets; each one's licence files are copied from its source in cargo's registry, and
crates whose files are identical share one copy of the text.

    python3 tools/third_party_licences.py [--target TRIPLE]... [-o FILE]

Without --target it lists every platform's crates; give --target twice for a universal binary.
The default output is THIRD-PARTY-LICENCES.md. Fails when a crate has no licence file, so a new
dependency's notice is never left out silently.
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT_PACKAGE = "dc2tk"
LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|notice|unlicense)([-._].*)?$", re.IGNORECASE)


def metadata(target):
    command = ["cargo", "metadata", "--format-version", "1", "--locked"]
    if target:
        command += ["--filter-platform", target]
    # cargo runs in src/, the workspace
    workspace = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src")
    return json.loads(subprocess.run(command, check=True, capture_output=True, text=True, cwd=workspace).stdout)


def packages_for(targets):
    """The packages linked for any of the targets (for every platform when there are none)."""
    found = {}
    for target in targets or [None]:
        for package in linked_packages(metadata(target)):
            found[package["id"]] = package
    return sorted(found.values(), key=lambda package: (package["name"], package["version"]))


def linked_packages(meta):
    """The packages the dc2tk executable links: its normal dependencies, followed transitively."""
    packages = {package["id"]: package for package in meta["packages"]}
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    root = next(id for id in members if packages[id]["name"] == ROOT_PACKAGE)
    seen, stack = set(), [root]
    while stack:
        id = stack.pop()
        if id in seen:
            continue
        seen.add(id)
        for dep in nodes[id]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    return sorted((packages[id] for id in seen - members), key=lambda package: (package["name"], package["version"]))


def bundled_licence_files(folder):
    """A -sys crate's bundled C library's licence files (libz-sys's src/zlib/LICENSE, mimalloc's
    c_src/mimalloc/v2/LICENSE), up to three folders down, leaving out contrib/ code it does not build."""
    found = []
    for path, folders, files in os.walk(folder):
        depth = os.path.relpath(path, folder).count(os.sep) + 1
        folders[:] = [name for name in folders if name != "contrib" and depth < 3]
        if path != folder:
            relative = os.path.relpath(path, folder).replace(os.sep, "/")
            found += [f"{relative}/{name}" for name in files if LICENCE_FILE.match(name)]
    return found


def licence_texts(package):
    folder = os.path.dirname(package["manifest_path"])
    names = sorted(name for name in os.listdir(folder) if LICENCE_FILE.match(name))
    if package.get("license_file"):
        names = sorted(set(names) | {package["license_file"]})
    if package["name"].endswith("-sys"):
        names += sorted(bundled_licence_files(folder))
    texts = []
    for name in names:
        path = os.path.join(folder, name)
        if os.path.isfile(path):
            with open(path, encoding="utf-8", errors="replace") as file:
                texts.append((name, file.read().replace("\r\n", "\n").strip("\n")))
    return texts


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--target", action="append", default=[], help="a target triple to list the dependencies of (default: every platform)")
    parser.add_argument("-o", "--output", default="THIRD-PARTY-LICENCES.md")
    args = parser.parse_args()

    packages = packages_for(args.target)
    groups = {}  # the licence files' texts -> the crates that carry them
    missing = []
    for package in packages:
        texts = licence_texts(package)
        if not texts:
            missing.append(f"{package['name']} {package['version']} ({package.get('license')})")
            continue
        groups.setdefault(tuple(texts), []).append(package)
    if missing:
        sys.exit("no licence file in: " + ", ".join(missing))

    lines = [
        "# Third-party licences",
        "",
        f"dc2tk is under the MIT licence (LICENCE). It is built with these {len(packages)} crates, whose",
        "licences follow, with those of the C libraries the -sys crates bundle. A crate offered under a",
        "choice of licences (\"MIT OR Apache-2.0\") is used under any one of them; all the texts its",
        "authors ship are given.",
        "",
    ]
    for package in packages:
        lines.append(f"- {package['name']} {package['version']}: {package.get('license') or 'see its licence file'}")
    for texts, crates in sorted(groups.items(), key=lambda group: (group[1][0]["name"], group[1][0]["version"])):
        lines += ["", "## " + ", ".join(f"{crate['name']} {crate['version']}" for crate in crates)]
        for name, text in texts:
            lines += ["", f"### {name}", "", "```text", text, "```"]
    with open(args.output, "w", encoding="utf-8", newline="\n") as file:
        file.write("\n".join(lines) + "\n")
    print(f"{args.output}: {len(packages)} crates, {len(groups)} licence texts")


if __name__ == "__main__":
    main()
