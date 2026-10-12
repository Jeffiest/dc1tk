import json
import re
import shutil
from pathlib import Path

from c2s import index, pathnames, source, tree

# Mods: folders of edited and added files, layered over a decompiled tree. A mod is a folder in the
# mods folder (mods/castle-golems) holding FILES, laid out like the tree but with only the files the
# mod changes or adds, and INDEX, what is known about each file: the hash of the decompiler's version
# it was made from and its path without readable names. Edit a mod's files where they are (the
# folder has its own compile_flags.txt), or edit the tree and save the changes into a mod.
#
# build --mod NAME layers mods over the tree without changing it: each mod file stands in for the
# tree's, libraries included, and two chosen mods may not change the same file. mod apply writes
# them into the tree instead.

INDEX = "mod.json"
FILES = "files"
NAME = re.compile(r"^[A-Za-z0-9_.-]+$")


def folder(mods_dir, name):
    if not NAME.match(name):
        raise SystemExit("a mod name uses letters, digits, '.', '_' and '-' only: %s" % name)
    return Path(mods_dir) / name


def entries(mod_dir):
    path = Path(mod_dir) / INDEX
    return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {"files": {}, "includes": {}}


def write_entries(mod_dir, found):
    note = ("A mod: files edited or added in a decompiled tree, under files/. cli.py build --mod uses them in place "
            "of the tree's; cli.py mod apply copies them into a tree.")
    (Path(mod_dir) / INDEX).write_text(json.dumps(dict(found, note=note), indent=1, sort_keys=True) + "\n",
                                       encoding="utf-8")


def mod_files(mod_dir):
    root = Path(mod_dir) / FILES
    if not root.is_dir():
        return []
    return sorted(path.relative_to(root).as_posix() for path in root.rglob("*")
                  if path.is_file() and path.name != source.EDITOR_FLAGS
                  and not any(part.startswith(".") for part in path.relative_to(root).parts))


def write_editor_flags(mod_dir, tree_dir, sdk_dir, definition_sets):
    root = Path(mod_dir) / FILES
    root.mkdir(parents=True, exist_ok=True)
    (root / source.EDITOR_FLAGS).write_text(source.editor_flags(sdk_dir, definition_sets, tree_dir), encoding="utf-8")


# Copies tree files into the mod with what the tree knows about them: their hash as the decompiler
# wrote them, their path without names, and the unnamed paths of the libraries they include.
def add(tree_dir, mod_dir, paths, overwrite=True):
    found = entries(mod_dir)
    recorded = tree.originals(tree_dir) or {}
    copied = []
    for path in paths:
        target = Path(mod_dir) / FILES / path
        if target.exists() and not overwrite:
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(Path(tree_dir) / path, target)
        copied.append(path)
        found["files"][path] = {"original": recorded.get(path), "unnamed": pathnames.unnamed(tree_dir, path)}
        if path.endswith(source.EXTENSION):
            for included in index.INCLUDE.findall(target.read_text(encoding="utf-8")):
                found.setdefault("includes", {})[included] = pathnames.unnamed(tree_dir, included)
    write_entries(mod_dir, found)
    return copied


# Starts or extends a mod with tree files to edit; files the mod already has are kept.
def new(tree_dir, mod_dir, paths):
    missing = [path for path in paths if not (Path(tree_dir) / path).is_file()]
    if missing:
        raise SystemExit("not in %s:\n  %s" % (tree_dir, "\n  ".join(missing)))
    Path(mod_dir).mkdir(parents=True, exist_ok=True)
    return add(tree_dir, mod_dir, paths, overwrite=False)


# Copies every file edited or added in the tree into the mod, replacing the mod's copies of them.
def save(tree_dir, mod_dir):
    found = tree.status(tree_dir)
    if found is None:
        raise SystemExit("%s has no %s; decompile it again with this version to save a mod from it" % (tree_dir, tree.ORIGINALS))
    mod = Path(mod_dir)
    if mod.exists() and not (mod / INDEX).is_file() and any(mod.iterdir()):
        raise SystemExit("%s is not empty and holds no mod; choose another name" % mod)
    mod.mkdir(parents=True, exist_ok=True)
    copied = add(tree_dir, mod_dir, found["edited"] + found["added"])
    return {"saved": copied, "removed": found["removed"]}


def with_includes(text, includes, tree_dir):
    lines = []
    for line in text.split("\n"):
        match = index.INCLUDE.match(line)
        if match:
            line = '#include "%s"' % (includes.get(match.group(1)) or pathnames.named(tree_dir, match.group(1)))
        lines.append(line)
    return "\n".join(lines)


# One mod's files as the tree would hold them: {tree path: text}, placed and with includes pointed
# where this tree keeps them (a mod made on a tree with or without readable names fits either).
def contents(mod_dir, tree_dir):
    found = entries(mod_dir)
    includes = {included: pathnames.named(tree_dir, plain) for included, plain in found.get("includes", {}).items()}
    result = {}
    for path in mod_files(mod_dir):
        entry = found["files"].get(path, {})
        target = pathnames.named(tree_dir, entry.get("unnamed", path))
        text = (Path(mod_dir) / FILES / path).read_text(encoding="utf-8")
        result[target] = with_includes(text, includes, tree_dir) if path.endswith(source.EXTENSION) else text
    return result


# Files whose decompiler version in the tree differs from the one the mod was made from: the mod
# still replaces the whole file, so compare them.
def stale(mod_dir, tree_dir):
    found = entries(mod_dir)
    recorded = tree.originals(tree_dir) or {}
    result = []
    for path, entry in found["files"].items():
        target = pathnames.named(tree_dir, entry.get("unnamed", path))
        if target == path and entry.get("original") and recorded.get(target) not in (None, entry["original"]):
            result.append(target)
    return sorted(result)


# The chosen mods layered in order: {tree path: text}, and the files more than one of them changes.
def overlay(mod_dirs, tree_dir):
    texts, owners = {}, {}
    for mod_dir in mod_dirs:
        if not (Path(mod_dir) / FILES).is_dir():
            raise SystemExit("no mod at %s" % mod_dir)
        for path, text in contents(mod_dir, tree_dir).items():
            owners.setdefault(path, []).append(Path(mod_dir).name)
            texts[path] = text
    clashes = {path: names for path, names in owners.items() if len(names) > 1}
    return texts, clashes


def clash_text(clashes):
    return "these files are changed by more than one of the chosen mods:\n  " + "\n  ".join(
        "%s (%s)" % (path, ", ".join(names)) for path, names in sorted(clashes.items()))


def apply(mod_dirs, tree_dir, force=False):
    texts, clashes = overlay(mod_dirs, tree_dir)
    if clashes:
        raise SystemExit(clash_text(clashes))
    found = tree.status(tree_dir) or {"edited": [], "added": []}
    edited = set(found["edited"]) | set(found["added"])
    conflicts = [path for path, text in texts.items() if path in edited
                 and (Path(tree_dir) / path).read_text(encoding="utf-8") != text]
    if conflicts and not force:
        raise SystemExit("these files are edited in %s too; pass --force to replace them with the mods' copies:\n  %s"
                         % (tree_dir, "\n  ".join(sorted(conflicts))))
    for path, text in texts.items():
        target = Path(tree_dir) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
    warnings = ["%s changed in the decompiler's output since %s was made; compare them" % (path, Path(mod_dir).name)
                for mod_dir in mod_dirs for path in stale(mod_dir, tree_dir)]
    return {"applied": sorted(texts), "warnings": warnings}


def listing(mods_dir, tree_dir):
    root = Path(mods_dir)
    mods = sorted(path for path in root.iterdir() if (path / FILES).is_dir()) if root.is_dir() else []
    if not mods:
        return "no mods in %s" % mods_dir
    lines = []
    for mod_dir in mods:
        texts = contents(mod_dir, tree_dir)
        lines.append("%s  (%d file%s)" % (mod_dir.name, len(texts), "" if len(texts) == 1 else "s"))
        lines += ["    " + path for path in sorted(texts)]
        lines += ["    stale: " + path for path in stale(mod_dir, tree_dir)]
    _, clashes = overlay(mods, tree_dir)
    if clashes:
        lines += ["", "mods that cannot be built together:"] + ["  %s: %s" % (path, ", ".join(names))
                                                                for path, names in sorted(clashes.items())]
    return "\n".join(lines)
