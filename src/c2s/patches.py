import difflib
import fnmatch
import re
from pathlib import Path

from c2s import additions, asm, definitions, index, library, names, numbers, parse, pathnames, sdk

# Patches: C files in a mod's folder that change scripts without copying them. A patch says which
# scripts it changes with #pragma target lines and holds only what it adds or replaces:
#
#   #pragma target map s51                    every language of map s51's script (s51_1.c to s51_5.c)
#   #pragma target script dungeon/monster/e33a.c   scripts by path without readable names, * and ? allowed
#   #pragma target monster golem              the AI script of a monster kind (MONSTER_GOLEM in disc.h)
#   #pragma target library lib/map/x.c        every script that includes a library
#
# build merges the patch into each of those scripts after the script's libraries are included, so
# a patch reaches a function the script gets from a library without forking the library:
#
# - A function after #pragma override replaces the script's function of that name for that script
#   alone; original(...) inside it calls the function it replaced. Several mods can override one
#   function when the later ones call original(): the last mod's runs first.
# - Other functions and globals are added. A name the script already has is refused (the patch most
#   likely meant to override it); a name another mod adds as well is renamed for this one, so mods
#   never need to agree on names. A patch's libraries can declare globals of their own.
# - PROGRAM lines add programs. A program the script already has is changed by overriding its
#   function instead, so that several mods can change it.
# - A function after #pragma every_frame is called before every wait; in the script, so it runs
#   once a frame whatever the script is waiting in (a monster's AI keeping a timer, say). It takes
#   no parameters and must not wait.
# - NEW_FLAG(name), NEW_PROGRAM(name) and NEW_MONSTER_VAR(name), in any file of the mod, are numbers
#   build gives the mod (numbers.py); every patch of the mod sees them as constants.
# - LANGUAGE is the language of the script being patched (s51_2.c is LANGUAGE_FRENCH), for #if and in
#   code, so one patch can carry each language's text. Scripts that are not language variants
#   (monster AI, say) have LANGUAGE -1.

LANGUAGES = {"LANGUAGE_JAPANESE": 0, "LANGUAGE_ENGLISH": 1, "LANGUAGE_FRENCH": 2, "LANGUAGE_GERMAN": 3,
             "LANGUAGE_ITALIAN": 4, "LANGUAGE_SPANISH": 5}
NO_LANGUAGE = -1
VARIANT = re.compile(r"_([1-5])\.c$")
TARGET_LINE = re.compile(r"^[ \t]*#[ \t]*pragma[ \t]+target\b(.*)$", re.M)
MONSTER_LINE = re.compile(r"^\s*MONSTER_(\w+)\s*=\s*(\d+),\s*//\s*model\s+\S+,\s*AI\s+(\w+)", re.M)
ORIGINAL = "original"


def language_of(plain):
    match = VARIANT.search(plain)
    if match:
        return int(match.group(1))
    return LANGUAGES["LANGUAGE_JAPANESE"] if plain.startswith("map/") else NO_LANGUAGE


def targets_in(text):
    found = []
    for line in TARGET_LINE.findall(index.without_comments(text)):
        words = [word["string"] if isinstance(word, dict) else word for word in asm.tokenize(line)]
        if not words or words[0] not in parse.TARGET_KINDS or len(words) < 2:
            raise SystemExit("#pragma target takes %s, then what to pick: %s" % (", ".join(parse.TARGET_KINDS), line.strip()))
        found.append({"kind": words[0], "values": words[1:]})
    return found


def is_patch(text):
    return bool(TARGET_LINE.search(index.without_comments(text)))


# {name in upper case: AI script code} from the tree's disc.h, with the kind's number as a name too.
def monster_scripts(tree):
    path = Path(tree) / "include" / "disc.h"
    found = {}
    if path.is_file():
        for name, number, script in MONSTER_LINE.findall(path.read_text(encoding="utf-8")):
            found[name] = script
            found.setdefault(number, script)
    return found


class Scripts:
    """The tree's scripts, and what #pragma target lines pick among them."""

    def __init__(self, tree):
        self.tree = tree
        self.paths = sorted(path for path in (index.relative(tree, file) for file in index.source_files(tree))
                            if not index.is_library(path))
        self.plain = {path: pathnames.unnamed(tree, path) for path in self.paths}
        self._monsters = None
        self._includers = None

    def in_map(self, code):
        found = [path for path in self.paths if additions.in_map(self.plain[path], code)]
        # the PAL game loads the numbered language variants; the unnumbered Japanese script only
        # when a map has nothing else
        variants = [path for path in found if VARIANT.search(self.plain[path])]
        return variants or found

    def monster(self, value):
        if self._monsters is None:
            self._monsters = monster_scripts(self.tree)
        key = value.upper()
        key = key[len("MONSTER_"):] if key.startswith("MONSTER_") else key
        script = self._monsters.get(key)
        if script is None:
            return []
        return [path for path in self.paths if self.plain[path] == "dungeon/monster/%s.c" % script]

    def users(self, path):
        if self._includers is None:
            self._includers = index.includers(index.outlines(self.tree))
        return self._includers.get(path, []) or self._includers.get(pathnames.named(self.tree, path), [])

    def picked(self, target):
        found = set()
        for value in target["values"]:
            if target["kind"] == "map":
                found |= set(self.in_map(value))
            elif target["kind"] == "script":
                found |= {path for path in self.paths
                          if fnmatch.fnmatchcase(self.plain[path], value) or fnmatch.fnmatchcase(path, value)}
            elif target["kind"] == "monster":
                found |= set(self.monster(value))
            elif target["kind"] == "library":
                found |= set(self.users(value))
        return found


# Which scripts each patch of the chosen mods changes, and the numbers the mods ask for. mod_list
# is a list of {"name", "texts": {path: text of each of its source files}, "written": [texts of
# its other files]}; written holds texts of mods left out, whose hand-written numbers are avoided
# too. lock is the numbers file (numbers.LOCK), None to give numbers without keeping them. Returns {"scripts": {tree path: [patch, ...] in mod order}, "patches": [patch, ...],
# "numbers": {mod: {name: number}}}.
def plan(mod_list, tree, consts=None, lock=None, written=()):
    by_script, patches = located(mod_list, tree)
    wanted = {mod["name"]: numbers.requests(mod["texts"].values()) for mod in mod_list}
    wanted = {name: asked for name, asked in wanted.items() if asked}
    given = {}
    if wanted:
        kinds = {kind for asked in wanted.values() for kind in asked}
        if consts is None:
            consts = index.constants(index.tree_headers(tree, definitions.load([definitions.DEFAULT_SET])))
        used = numbers.used_by_tree(tree, consts, kinds)
        by_hand = numbers.written([text for mod in mod_list for text in list(mod["texts"].values()) + mod.get("written", [])]
                                  + list(written))
        used = {kind: used[kind] | by_hand[kind] for kind in numbers.POOLS}
        held = numbers.read_lock(lock) if lock else numbers.read_lock("")
        given, changed = numbers.assign(wanted, held, used)
        if changed and lock:
            numbers.write_lock(lock, held)
    for patch in patches:
        patch["numbers"] = given.get(patch["mod"], {})
    return {"scripts": by_script, "patches": patches, "numbers": given}


# The chosen mods' patches and the scripts each changes: ({tree path: [patch, ...]}, [patch, ...]).
def located(mod_list, tree):
    scripts = None
    by_script = {}
    patches = []
    for mod in mod_list:
        for path, text in sorted(mod["texts"].items()):
            targets = targets_in(text)
            if not targets:
                continue
            scripts = scripts or Scripts(tree)
            chosen = set()
            for target in targets:
                found = scripts.picked(target)
                if not found:
                    raise SystemExit("%s: #pragma target %s %s picks no script in %s"
                                     % (path, target["kind"], " ".join(target["values"]), tree))
                chosen |= found
            patch = {"mod": mod["name"], "path": Path(path), "text": text, "scripts": sorted(chosen),
                     "plain": {script: scripts.plain[script] for script in chosen}}
            patches.append(patch)
            for script in chosen:
                by_script.setdefault(script, []).append(patch)
    return by_script, patches


def int_node(value):
    return {"kind": "int", "value": value}


# The patch as merged into one script: parsed with that script's LANGUAGE, its libraries included
# (looked for from the patch's folder up, then in the tree, then in the SDK), with the constants
# it alone knows: its own #defines and enums, LANGUAGE and the mod's numbers.
def prepared(patch, script, tree, headers, overlay=None, sdk_dir=None):
    language = language_of(patch["plain"].get(script, script))
    defines = dict({name: str(value) for name, value in LANGUAGES.items()}, LANGUAGE=str(language))
    parsed = parse.parse(patch["text"], defines)
    if "asm" in parsed:
        raise ValueError("a patch cannot be assembly")
    if parsed["main"] is not None:
        raise ValueError("a patch has no MAIN; the script keeps its own")
    texts = library.read_includes(parsed, patch["path"], overlay, sdk_dir, [tree])
    # the script's own libraries never declare globals; a mod's can
    own_globals = [dict(declared, source=None) for declared in parsed["globals"]]
    for path, text in texts.items():
        try:
            own_globals += [dict(declared, source=path) for declared in parse.parse(text, defines)["globals"]]
        except ValueError:
            pass
    expanded = library.expand(parsed, texts, headers)
    consts = {declared["name"]: declared["value"] for declared in expanded["declarations"]["constants"]}
    for declared in expanded["declarations"]["programs"]:
        consts.setdefault(declared["name"], int_node(declared["no"]))
    consts.update({name: int_node(value) for name, value in LANGUAGES.items()})
    consts["LANGUAGE"] = int_node(language)
    consts.update({name: int_node(value) for name, value in patch["numbers"].items()})
    return dict(expanded, globals=own_globals), consts


def fresh_name(base, taken):
    name, number = base, 2
    while name in taken:
        name = "%s_%d" % (base, number)
        number += 1
    taken.add(name)
    return name


def mod_identifier(mod):
    return re.sub(r"\W", "_", mod)


def renamed(node, calls, variables, shadowed):
    if isinstance(node, list):
        return [renamed(item, calls, variables, shadowed) for item in node]
    if not isinstance(node, dict):
        return node
    out = {key: renamed(value, calls, variables, shadowed) for key, value in node.items()}
    if node.get("kind") == "call" and node["name"] in calls:
        out["name"] = calls[node["name"]]
    if node.get("kind") in ("var", "index") and node["name"] in variables and node["name"] not in shadowed:
        out["name"] = variables[node["name"]]
    return out


def own_names(function):
    return {param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}


# A patch function ready to join the script: the patch's constants filled in, its renamed functions
# and globals followed, and original() pointed at before when it is an override.
def joined(function, consts, calls, variables, global_names, mod, before=None):
    shadowed = own_names(function)
    body = sdk.with_constants(function["body"], consts, shadowed | global_names)
    if before is not None:
        calls = dict(calls, **{ORIGINAL: before})
    elif ORIGINAL in names.callees(body, []):
        raise ValueError("%s calls original(), which only a #pragma override function can" % function["name"])
    locals_ = [dict(declared, size=consts[declared["size"]]["value"])
               if isinstance(declared.get("size"), str) and consts.get(declared["size"], {}).get("kind") == "int"
               else declared for declared in function["locals"]]
    out = dict(function, name=calls.get(function["name"], function["name"]), locals=locals_,
               body=renamed(body, calls, variables, shadowed), padding=None, unused_strings=[], mod=mod)
    for key in ("override", "every_frame", "library"):
        out.pop(key, None)
    return out


def program_number(number, numbering, consts):
    if isinstance(number, int):
        return number
    if number in numbering:
        return numbering[number]
    value = consts.get(number)
    return value["value"] if value and value["kind"] == "int" else None


def parameter_count(function):
    return function["arg"] if function["kind"] == "asm" else len(function["params"])


# The script program with one patch merged in. program has had its libraries included; patch and
# consts come from prepared. program["patched"] remembers what earlier patches did: the names they
# added, the libraries they brought and the functions they override.
def merged(program, patch, consts, mod):
    if patch["natives"] and program["natives"] and patch["natives"] != program["natives"]:
        raise ValueError("the patch uses %s natives, but the script is an %s script" % (patch["natives"], program["natives"]))
    done = program.get("patched", {"added": set(), "libraries": {}, "overrides": {}, "every_frame": []})
    done = {"added": set(done["added"]), "libraries": dict(done["libraries"]), "overrides": dict(done["overrides"]),
            "every_frame": list(done["every_frame"])}
    functions = list(program["functions"])
    script_libraries = {function.get("library") for function in functions} - {None}
    function_names = {function["name"] for function in functions}
    global_names = {declared["name"] for declared in program["globals"]}
    taken = function_names | global_names
    calls, variables = {}, {}
    mine, libraries = [], {}
    for function in patch["functions"]:
        if function["kind"] != "function":
            raise ValueError("a patch cannot hold asm functions (%s)" % function["name"])
        source = function.get("library")
        if source in script_libraries:
            continue
        if source in done["libraries"]:
            # an earlier mod brought this library in; use its copy, under the names it was given
            calls.update(done["libraries"][source]["calls"])
            continue
        mine.append(function)
        if source is not None:
            libraries.setdefault(source, {"calls": {}, "variables": {}})
    for function in mine:
        name = function["name"]
        if function.get("override") or name not in taken:
            continue
        if name not in done["added"]:
            raise ValueError("the script already has %s; put #pragma override before it to replace it, or rename it" % name)
        calls[name] = fresh_name("%s_%s" % (name, mod_identifier(mod)), taken)
    taken |= {calls.get(function["name"], function["name"]) for function in mine if not function.get("override")}
    added_globals = []
    for declared in patch["globals"]:
        name, source = declared["name"], declared.get("source")
        if source in done["libraries"]:
            variables.update(done["libraries"][source]["variables"])
            continue
        if name in global_names and name not in done["added"]:
            raise ValueError("the script already has a global %s; rename the patch's" % name)
        if name in taken:
            variables[name] = fresh_name("%s_%s" % (name, mod_identifier(mod)), taken)
        else:
            taken.add(name)
        if source is not None:
            libraries.setdefault(source, {"calls": {}, "variables": {}})["variables"][name] = variables.get(name, name)
        added_globals.append({key: value for key, value in dict(declared, name=variables.get(name, name)).items()
                              if key != "source"})
    all_globals = global_names | {declared["name"] for declared in patch["globals"]}
    for function in mine:
        source = function.get("library")
        if source is not None and not function.get("override"):
            libraries[source]["calls"][function["name"]] = calls.get(function["name"], function["name"])
    position = {function["name"]: number for number, function in enumerate(functions)}
    appended = []
    for function in mine:
        name = function["name"]
        if function.get("every_frame"):
            if function["params"] or function.get("override"):
                raise ValueError("#pragma every_frame %s: an every-frame function takes no parameters and overrides nothing"
                                 % name)
            done["every_frame"].append(calls.get(name, name))
        if not function.get("override"):
            appended.append(joined(function, consts, calls, variables, all_globals, mod))
            done["added"].add(calls.get(name, name))
            continue
        if name not in position:
            close = difflib.get_close_matches(name, sorted(position), 3)
            raise ValueError("#pragma override %s: the script has no function %s%s"
                             % (name, name, "; did the definitions rename it? close: " + ", ".join(close) if close else ""))
        old = functions[position[name]]
        if parameter_count(old) != len(function["params"]):
            raise ValueError("override %s takes %d parameters; the script's %s takes %d"
                             % (name, len(function["params"]), name, parameter_count(old)))
        earlier = done["overrides"].get(name, [])
        chains = ORIGINAL in names.callees(function["body"], [])
        if earlier and not chains:
            raise ValueError("%s replaces %s, which mod %s overrides too; call original() in one of them to run both"
                             % (mod, name, ", ".join(earlier)))
        if chains:
            before = fresh_name("%s_before_%s" % (name, mod_identifier(mod)), taken)
            functions[position[name]] = dict(old, name=before)
            position[before] = position.pop(name)
            appended.append(joined(function, consts, calls, variables, all_globals, mod, before))
        else:
            functions[position[name]] = joined(function, consts, calls, variables, all_globals, mod, None)
        done["overrides"][name] = earlier + [mod]
    numbering = {declared["name"]: declared["no"] for declared in program["declarations"]["programs"]}
    numbering.update({declared["name"]: declared["no"] for declared in patch["declarations"]["programs"]})
    present = {}
    for entry in program["programs"]:
        present[program_number(entry["no"], numbering, {})] = entry["function"]
    programs = list(program["programs"])
    for entry in patch["programs"]:
        number = program_number(entry["no"], numbering, consts)
        if number is None:
            raise ValueError("PROGRAM(%s, %s): %s is not a program number or name" % (entry["no"], entry["function"], entry["no"]))
        if number in present:
            raise ValueError("PROGRAM(%s, %s): the script already has program %s, run by %s; override %s instead"
                             % (entry["no"], entry["function"], entry["no"], present[number], present[number]))
        present[number] = entry["function"]
        programs.append({"no": number, "function": calls.get(entry["function"], entry["function"])})
    globals_ = list(program["globals"])
    while len(globals_) < (program["global_num"] or 0):
        # slots the script counts but never names: keep them clear of the patch's
        globals_.append({"name": "unnamed_global_%d" % len(globals_), "type": "int"})
    globals_ += added_globals
    done["added"] |= {declared["name"] for declared in added_globals}
    done["libraries"].update(libraries)
    return dict(program, functions=functions + appended, programs=programs, globals=globals_, global_num=len(globals_),
                declarations=merged_declarations(program["declarations"], patch["declarations"]),
                natives=program["natives"] or patch["natives"], patched=done)


def merged_declarations(script, patch):
    out = dict(script)
    for key in ("natives", "inlines", "programs"):
        have = {declared["name"] for declared in script.get(key, [])}
        out[key] = list(script.get(key, [])) + [declared for declared in patch.get(key, []) if declared["name"] not in have]
    out["kinds"] = dict(patch.get("kinds", {}), **script.get("kinds", {}))
    return out


# The script's program with every patch that targets it merged in, in mod order.
def applied(program, patches, script, tree, headers, overlay=None, sdk_dir=None):
    if "asm" in program:
        raise ValueError("a patch cannot change a script kept as assembly")
    for patch in patches:
        try:
            parsed, consts = prepared(patch, script, tree, headers, overlay, sdk_dir)
            program = merged(program, parsed, consts, patch["mod"])
        except ValueError as error:
            raise ValueError("%s (mod %s): %s" % (patch["path"].name, patch["mod"], error)) from None
    return with_frame_calls(program)


# A call to each #pragma every_frame function before every wait; in the script, so it runs once a
# frame whatever the script is waiting in. The functions themselves, and what they call, are left
# alone, and must not wait.
def with_frame_calls(program):
    hooks = program.get("patched", {}).get("every_frame", [])
    if not hooks:
        return program
    by_name = {function["name"]: function for function in program["functions"]}
    exempt = set()
    pending = list(hooks)
    while pending:
        name = pending.pop()
        if name in exempt or name not in by_name:
            continue
        exempt.add(name)
        if by_name[name]["kind"] == "function":
            if waits(by_name[name]["body"]):
                raise ValueError("%s waits, so it cannot run every frame (it is %s or called by one)"
                                 % (name, "an every_frame function" if name in hooks else "an every_frame function's callee"))
            pending += names.callees(by_name[name]["body"], [])
    calls = [{"kind": "expr", "expr": {"kind": "call", "name": hook, "args": []}} for hook in hooks]
    functions = [dict(function, body=before_waits(function["body"], calls))
                 if function["kind"] == "function" and function["name"] not in exempt else function
                 for function in program["functions"]]
    return dict(program, functions=functions)


def waits(node):
    if isinstance(node, list):
        return any(waits(item) for item in node)
    if isinstance(node, dict):
        return node.get("kind") == "wait" or any(waits(value) for value in node.values())
    return False


def before_waits(node, calls):
    if isinstance(node, list):
        out = []
        for item in node:
            if isinstance(item, dict) and item.get("kind") == "wait":
                out += [dict(call, line=item.get("line")) for call in calls]
            out.append(before_waits(item, calls))
        return out
    if isinstance(node, dict):
        return {key: before_waits(value, calls) for key, value in node.items()}
    return node


# The source text of a function, with the doc comment above it, or None.
def function_text(text, name):
    lines = text.split("\n")
    head = re.compile(r"^(?:void|int|float|string)\s+\**%s\s*\(" % re.escape(name))
    for number, line in enumerate(lines):
        if head.match(line):
            start = number
            while start > 0 and lines[start - 1].startswith("//"):
                start -= 1
            end = next((later for later in range(number, len(lines)) if lines[later] == "}"), None)
            return None if end is None else "\n".join(lines[start:end + 1]) + "\n"
    return None


# {name: source text} of the wanted functions a tree script defines or gets from its libraries.
def function_texts(tree, script, wanted):
    found = {}
    seen = set()
    pending = [script]
    while pending and len(found) < len(wanted):
        path = pending.pop(0)
        if path in seen or not (Path(tree) / path).is_file():
            continue
        seen.add(path)
        text = (Path(tree) / path).read_text(encoding="utf-8")
        for name in wanted:
            if name not in found:
                piece = function_text(text, name)
                if piece is not None:
                    found[name] = piece
        pending += index.INCLUDE.findall(text)
    return found


LANGUAGE_NAMES = {value: name for name, value in LANGUAGES.items()}


# One text for a function whose copies differ between a map's languages ({language: text}, English
# first): where the copies line up, the lines that differ get an #if LANGUAGE branch each; where
# they do not, each language gets its whole copy.
def language_text(copies):
    order = sorted(copies, key=lambda language: (language != LANGUAGES["LANGUAGE_ENGLISH"], language))
    if len({copies[language] for language in order}) == 1:
        return copies[order[0]]
    split = {language: copies[language].rstrip("\n").split("\n") for language in order}

    def branches(pieces):
        lines = []
        for number, language in enumerate(order[1:]):
            lines.append("#%s LANGUAGE == %s" % ("if" if number == 0 else "elif", LANGUAGE_NAMES[language]))
            lines += pieces[language]
        return lines + ["#else"] + pieces[order[0]] + ["#endif"]

    if len({len(lines) for lines in split.values()}) != 1:
        return "\n".join(branches(split)) + "\n"
    out = []
    for number in range(len(split[order[0]])):
        found = {language: split[language][number] for language in order}
        if len(set(found.values())) == 1:
            out.append(found[order[0]])
        else:
            out += branches({language: [line] for language, line in found.items()})
    return "\n".join(out) + "\n"


# The start of a new patch file: what it is, its target and the headers the script includes.
def patch_header(mod, kind, value, script_text):
    includes = [line for line in script_text.split("\n") if re.match(r"#include\s*<", line)]
    return "\n".join([
        "// Mod %s: a patch, merged into the scripts below when the mod is built. A function after" % mod,
        "// #pragma override replaces the script's own, and original() inside it runs the one it replaced;",
        "// other functions, globals and PROGRAM lines are added.",
        "#pragma target %s %s" % (kind, value),
        "",
    ] + includes) + "\n"


# One line per patch for mod list and build: what it targets, overrides, adds and asks for.
def summary(patch):
    text = index.without_comments(patch["text"])
    overrides = re.findall(r"#[ \t]*pragma[ \t]+override\s+(?:\w+\s+)*?(\w+)\s*\(", text)
    every_frame = re.findall(r"#[ \t]*pragma[ \t]+every_frame\s+(?:\w+\s+)*?(\w+)\s*\(", text)
    added = index.PROGRAM.findall(text)
    parts = ["%d script%s" % (len(patch["scripts"]), "" if len(patch["scripts"]) == 1 else "s")]
    if overrides:
        parts.append("overrides " + ", ".join(overrides))
    if every_frame:
        parts.append("runs %s every frame" % ", ".join(every_frame))
    if added:
        parts.append("adds program%s %s" % ("" if len(added) == 1 else "s", ", ".join(number for number, _ in added)))
    return "%s: %s" % (patch["path"].name, "; ".join(parts))
