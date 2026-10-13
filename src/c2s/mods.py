import json
import re
import shutil
from pathlib import Path

from c2s import additions, definitions, discfiles, index, library, numbers, parse, patches, pathnames, rebuild, source, tree, vm

# Mods: changes layered over a decompiled tree. A mod is a folder in the mods folder
# (mods/castle-golems). Its patches, .c files that say which scripts they change with
# #pragma target, hold only the functions, globals and programs they add or override, and are
# merged into those scripts when they are built (patches.py); several mods can patch one script,
# and the numbers they ask for with NEW_FLAG and the like are given by build (numbers.py).
#
# A mod can also hold whole files: FILES, laid out like the tree but with only the files the mod
# replaces or adds, and INDEX, what is known about each file: the hash of the decompiler's version
# it was made from and its path without readable names. Edit a mod's files where they are (the
# folder has its own compile_flags.txt), or edit the tree and save the changes into a mod.
#
# build --mod NAME layers mods over the tree without changing it: each mod file stands in for the
# tree's, libraries included, and two chosen mods may not change the same file. A mod can also list
# additions in its mod.json (programs, includes and wrappers added to scripts it does not copy; see
# additions.py), which several mods can make to one script. mod apply writes them into the tree
# instead. "start_at" in mod.json gives the scene build --start-at uses when the mod is built.
#
# Game files the mod replaces (models, textures, sounds) are under disc/, laid out as extract lays
# the disc out (discfiles.py).

INDEX = "mod.json"
FILES = "files"
NAME = re.compile(r"^[A-Za-z0-9_.-]+$")


def folder(mods_dir, name):
    if not NAME.match(name):
        raise SystemExit("a mod name uses letters, digits, '.', '_' and '-' only: %s" % name)
    return Path(mods_dir) / name


def entries(mod_dir):
    path = Path(mod_dir) / INDEX
    found = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
    found.setdefault("files", {})
    found.setdefault("includes", {})
    return found


def exists(mod_dir):
    return ((Path(mod_dir) / FILES).is_dir() or (Path(mod_dir) / discfiles.DISC).is_dir() or (Path(mod_dir) / INDEX).is_file()
            or bool(sources(mod_dir)))


SOURCE_SUFFIXES = (".c", ".h")


# The mod's own source files, outside files/: its patches (patches.py), the libraries and headers
# they include, and tests.c. {path: text}.
def sources(mod_dir):
    root = Path(mod_dir)
    if not root.is_dir():
        return {}
    found = {}
    for path in sorted(root.rglob("*")):
        parts = path.relative_to(root).parts
        if (path.is_file() and path.suffix in SOURCE_SUFFIXES and parts[0] not in (FILES, discfiles.DISC)
                and not any(part.startswith(".") for part in parts)):
            found[path] = path.read_text(encoding="utf-8")
    return found


# The chosen mods' patches and numbers (patches.plan), with numbers kept in the mods folder's
# numbers.json when lock is true. No mod is given a number any mod in the folder writes by hand,
# chosen or not, so building them together later cannot clash.
def patch_plan(mod_dirs, tree_dir, consts=None, lock=True):
    mod_list = [{"name": Path(mod_dir).name, "texts": sources(mod_dir),
                 "written": list(contents(mod_dir, tree_dir).values())} for mod_dir in mod_dirs]
    folder = Path(mod_dirs[0]).parent if mod_dirs else None
    others = [path for path in sorted(folder.iterdir()) if path.is_dir() and path not in map(Path, mod_dirs) and exists(path)] \
        if folder is not None and folder.is_dir() else []
    written = [text for other in others for text in list(sources(other).values()) + list(contents(other, tree_dir).values())]
    held = folder / numbers.LOCK if lock and folder is not None else None
    return patches.plan(mod_list, tree_dir, consts, held, written)


# Starts or extends a patch of the mod: copies functions of the scripts --map/--script/... picks
# (from the English one of a map's languages) into NAME.c in the mod, each after #pragma override.
def override(tree_dir, mod_dir, kind, value, functions):
    scripts = patches.Scripts(tree_dir)
    chosen = sorted(scripts.picked({"kind": kind, "values": [value]}))
    if not chosen:
        raise SystemExit("--%s %s picks no script in %s" % (kind, value, tree_dir))
    model = next((path for path in chosen if scripts.plain[path].endswith("_1" + source.EXTENSION)), chosen[0])
    found = patches.function_texts(tree_dir, model, functions)
    missing = [name for name in functions if name not in found]
    if missing:
        raise SystemExit("%s has no function %s" % (model, ", ".join(missing)))
    # the model's other languages: where their copies differ (text, mostly), each keeps its own
    variants = [path for path in chosen if path != model and index.variant_key(path) == index.variant_key(model)
                and patches.language_of(scripts.plain[path]) > 0]
    if variants:
        copies = {path: patches.function_texts(tree_dir, path, functions) for path in variants}
        for name in functions:
            texts = {patches.language_of(scripts.plain[model]): found[name]}
            texts.update({patches.language_of(scripts.plain[path]): copies[path][name]
                          for path in variants if name in copies[path]})
            found[name] = patches.language_text(texts)
    path = Path(mod_dir) / (re.sub(r"[^A-Za-z0-9_.-]", "_", Path(value).stem) + source.EXTENSION)
    path.parent.mkdir(parents=True, exist_ok=True)
    text = path.read_text(encoding="utf-8") if path.is_file() else patches.patch_header(
        Path(mod_dir).name, kind, value, (Path(tree_dir) / model).read_text(encoding="utf-8"))
    added = [name for name in functions if not re.search(r"^\w+\s+%s\s*\(" % re.escape(name), text, re.M)]
    for name in added:
        text = text.rstrip("\n") + "\n\n#pragma override\n" + found[name].rstrip("\n") + "\n"
    path.write_text(text, encoding="utf-8")
    return {"path": path, "added": added, "scripts": chosen}


# compile_flags.txt in the mod's own folder, so an editor reads its patches with the SDK, the
# mod's libraries and the tree's.
def write_patch_flags(mod_dir, tree_dir, sdk_dir, definition_sets):
    (Path(mod_dir) / source.EDITOR_FLAGS).write_text(source.editor_flags(sdk_dir, definition_sets, tree_dir), encoding="utf-8")


# Compiles every script the chosen mods change, as build would, without a disc: the scripts their
# files replace or reach through a library, and the scripts their patches change. Returns the
# numbers the mods were given, how many scripts compiled, and [(script, error)].
def check(mod_dirs, tree_dir, defs, tables, sdk_dir=None):
    texts, clashes = overlay(mod_dirs, tree_dir)
    if clashes:
        raise SystemExit(clash_text(clashes))
    found = patch_plan(mod_dirs, tree_dir)
    scripts = {path for path in texts if path.endswith(source.EXTENSION) and not index.is_library(path)}
    scripts |= set(found["scripts"])
    changed_libraries = [path for path in texts if index.is_library(path)]
    if changed_libraries:
        users = index.includers(index.outlines(tree_dir))
        for path in changed_libraries:
            scripts |= set(users.get(path, []))
    headers = definitions.header_texts(defs, sdk_dir or patches.sdk.SDK_DIR)
    errors = []
    for path in sorted(scripts):
        patched = (tree_dir, path, found["scripts"][path]) if path in found["scripts"] else None
        try:
            rebuild.compiled(Path(tree_dir) / path, defs, headers, tables, texts.get(path), pathnames.unnamed(tree_dir, path),
                             texts, sdk_dir, patched)
        except ValueError as error:
            errors.append((path, str(error)))
    return {"compiled": len(scripts), "errors": errors, "numbers": found["numbers"]}


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
    for path, target in targets(mod_dir, tree_dir).items():
        text = (Path(mod_dir) / FILES / path).read_text(encoding="utf-8")
        result[target] = with_includes(text, includes, tree_dir) if path.endswith(source.EXTENSION) else text
    return result


# Where this tree keeps each of the mod's files: {path in the mod: path in the tree}.
def targets(mod_dir, tree_dir):
    found = entries(mod_dir)
    return {path: pathnames.named(tree_dir, found["files"].get(path, {}).get("unnamed", path))
            for path in mod_files(mod_dir)}


def tree_constants(tree_dir):
    defs = definitions.load([definitions.DEFAULT_SET])
    return index.constants(index.tree_headers(tree_dir, defs))


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
def overlay(mod_dirs, tree_dir, consts=None):
    texts, owners = {}, {}
    for mod_dir in mod_dirs:
        if not exists(mod_dir):
            raise SystemExit("no mod at %s" % mod_dir)
        for path, text in contents(mod_dir, tree_dir).items():
            owners.setdefault(path, []).append(Path(mod_dir).name)
            texts[path] = text
    clashes = {path: names for path, names in owners.items() if len(names) > 1}
    scripts = None
    for mod_dir in mod_dirs:
        listed = entries(mod_dir).get("additions", [])
        if not listed:
            continue
        consts = tree_constants(tree_dir) if consts is None else consts
        if scripts is None:
            scripts = sorted({index.relative(tree_dir, path) for path in index.source_files(tree_dir)} | set(texts))
        place = targets(mod_dir, tree_dir)
        for path in scripts:
            plain = pathnames.unnamed(tree_dir, path)
            for addition in listed:
                if additions.matches(addition, plain):
                    base = texts[path] if path in texts else (Path(tree_dir) / path).read_text(encoding="utf-8")
                    texts[path] = additions.applied(base, addition, Path(mod_dir).name, consts, place)
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
    warnings += ["%s's patch %s is merged in by build only, not written into the tree" % (Path(mod_dir).name, path.name)
                 for mod_dir in mod_dirs for path, text in sources(mod_dir).items() if patches.is_patch(text)]
    return {"applied": sorted(texts), "warnings": warnings}


def listing(mods_dir, tree_dir):
    root = Path(mods_dir)
    mods = sorted(path for path in root.iterdir() if exists(path)) if root.is_dir() else []
    if not mods:
        return "no mods in %s" % mods_dir
    lines = []
    held = numbers.read_lock(root / numbers.LOCK)
    for mod_dir in mods:
        texts = contents(mod_dir, tree_dir)
        found = entries(mod_dir)
        own = sources(mod_dir)
        _, mine = patches.located([{"name": mod_dir.name, "texts": own}], tree_dir)
        counted = "%d file%s" % (len(texts), "" if len(texts) == 1 else "s")
        if mine:
            counted = "%d patch%s, " % (len(mine), "" if len(mine) == 1 else "es") + counted
        lines.append("%s  (%s)" % (mod_dir.name, counted))
        lines += ["    patch " + patches.summary(patch) for patch in mine]
        for kind, names in sorted(numbers.requests(own.values()).items()):
            for name in sorted(names):
                number = held[kind].get("%s/%s" % (mod_dir.name, name))
                lines.append("    %s %s = %s" % (numbers.WORDS[kind], name, "given at its next build" if number is None else number))
        lines += ["    " + path for path in sorted(texts)]
        lines += ["    replaces %s" % path for path in discfiles.mod_files(mod_dir)]
        for addition in found.get("additions", []):
            where = addition.get("map") or ", ".join(addition.get("scripts", []))
            parts = ["program %s" % program for program in addition.get("programs", {})]
            parts += ["include %s" % path for path in addition.get("includes", [])]
            parts += ["wrap %s" % program for program in addition.get("wrap", {})]
            lines.append("    adds to %s: %s" % (where, ", ".join(parts)))
        if found.get("start_at"):
            lines.append("    starts at " + found["start_at"])
        lines += ["    stale: " + path for path in stale(mod_dir, tree_dir)]
    _, clashes = overlay(mods, tree_dir)
    _, disc_clashes = discfiles.collect(mods)
    clashes = dict(clashes, **disc_clashes)
    if clashes:
        lines += ["", "mods that cannot be built together:"] + ["  %s: %s" % (path, ", ".join(names))
                                                                for path, names in sorted(clashes.items())]
    return "\n".join(lines)


TESTS = "tests.c"


# Runs a mod's tests: the test_ functions of its tests.c (int, no parameters), compiled with the mod
# layered over the tree and run in the script interpreter (vm.py), where natives do nothing and
# controller input is whatever the test passes in. A test passes when it returns nonzero. tests.c
# can include the mod's own libraries and use its NEW_FLAG and kin names. Returns (name, passed,
# printed lines, error) for each.
def run_tests(mod_dir, tree_dir, defs, sdk_dir):
    path = Path(mod_dir) / TESTS
    if not path.is_file():
        raise SystemExit("%s has no %s; write int test_...(void) functions there" % (mod_dir, TESTS))
    texts, _ = overlay([mod_dir], tree_dir)
    given = patch_plan([mod_dir], tree_dir)["numbers"].get(Path(mod_dir).name, {})
    program = parse.parse(path.read_text(encoding="utf-8"))
    names = [function["name"] for function in program["functions"] if function["kind"] == "function"]
    program["main"] = program["main"] or (names[0] if names else None)
    standing_in = Path(tree_dir) / TESTS
    headers = dict(definitions.header_texts(defs, sdk_dir), **definitions.source_headers(standing_in))
    program = library.expand(program, library.read_includes(program, standing_in, texts, sdk_dir, [mod_dir]), headers)
    program["declarations"]["constants"] = program["declarations"]["constants"] + [
        {"name": name, "value": {"kind": "int", "value": number}, "enum": None} for name, number in given.items()]
    table = vm.table_of(program)
    results = []
    for name in sorted(name for name in table if name.startswith("test_")):
        printed = []
        try:
            value = vm.run_function(table, name, [], [("int", 0)] * 256, printed)[1]
            results.append((name, bool(value), printed, None))
        except (ValueError, KeyError, IndexError, ZeroDivisionError) as error:
            results.append((name, False, printed, str(error)))
    return results
