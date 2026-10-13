import struct
from pathlib import Path

from c2s import archive, rebuild

# Game files a mod replaces: models, textures, sounds and anything else on the disc. A mod keeps
# them under disc/, laid out as extract lays the disc out: a file at its path in DATA.DAT
# (snd2/sp/sp_000.snd), or a member of a pack under the pack's name plus ".d"
# (chara/c01_base.chr.d/c01b.img for the texture inside Max's character pack). build writes each
# over the disc's own; members of packs inside packs work the same way (a.pac.d/b.chr.d/c.img).

#
# A game file can also be given in a common format, named after the file it stands for plus the
# format's suffix (c01b.img.png): build converts it, with the disc's own file as the template
# (palette size, sample rate, layout), and mod asset --as exports to it.

DISC = "disc"
PACK = ".d/"

# Converters between game formats and common ones, by the common format's suffix: "game", the
# suffixes of game files it converts; "build", (common file's bytes, the disc's own file) -> game
# file; "export", the disc's own file -> common file's bytes.
CONVERTERS = {}


def converter_for(path):
    for suffix, converter in CONVERTERS.items():
        stem = path[:-len(suffix)]
        if path.lower().endswith(suffix) and stem.lower().endswith(converter["game"]):
            return stem, converter
    return path, None


# {disc path: file} for the game files a mod holds; a file in a common format is keyed by the game
# file it stands for.
def mod_files(mod_dir):
    root = Path(mod_dir) / DISC
    if not root.is_dir():
        return {}
    found = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            target, _ = converter_for(path.relative_to(root).as_posix())
            if target in found:
                raise ValueError("%s: %s is given twice, as %s and %s" % (Path(mod_dir).name, target, found[target].name, path.name))
            found[target] = path
    return found


# The game file's bytes for a mod file: the file itself, or converted from its common format.
def game_bytes(file, original):
    _, converter = converter_for(Path(file).as_posix())
    data = Path(file).read_bytes()
    return converter["build"](data, original) if converter else data


# The file of DATA.DAT a disc path is in, and the members down to it: chara/c01_base.chr.d/c01b.img
# is member c01b.img of chara/c01_base.chr.
def location(path):
    parts = path.split(PACK)
    return parts[0], parts[1:]


# {disc path: (mod name, file)} for the chosen mods, and the paths two of them replace.
def collect(mod_dirs):
    found, clashes = {}, {}
    for mod_dir in mod_dirs:
        for path, file in mod_files(mod_dir).items():
            if path in found:
                clashes.setdefault(path, [found[path][0]]).append(Path(mod_dir).name)
            else:
                found[path] = (Path(mod_dir).name, file)
    return found, clashes


def clash_text(clashes):
    return "\n".join("%s is replaced by more than one chosen mod: %s" % (path, ", ".join(names))
                     for path, names in sorted(clashes.items()))


MAX_MEMBER_NAME = archive.PACK_NAME - 1


# The pack with a new member added at the end of its chain (or of the chain of a pack inside it),
# so the members it had keep their positions: the engine finds members by name, ignoring case
# (GetPackFile in dataread.cpp), but some loaders walk a pack by position or take every member with
# an extension.
def added_member(data, names, new):
    members = archive.parse_pack(data)
    if members is None:
        raise ValueError("not a pack")
    if len(names) > 1:
        member = rebuild.member_named(members, names[0], False)
        if member is None:
            raise ValueError("no pack %s to add %s to" % (names[0], "/".join(names[1:])))
        return rebuild.replaced_member(data, [names[0]], added_member(data[member["offset"]:member["offset"] + member["size"]],
                                                                      names[1:], new))
    name = names[0]
    if any(member["name"].lower() == name.lower() for member in members):
        raise ValueError("the pack has a member %s already, in other case; the game would find that one" % name)
    raw = name.encode("cp932")
    if len(raw) > MAX_MEMBER_NAME:
        raise ValueError("member names are at most %d bytes" % MAX_MEMBER_NAME)
    end = members[-1]["offset"] + members[-1]["size"] + (-members[-1]["size"] % archive.PACK_ALIGN)
    entry = raw.ljust(archive.PACK_NAME, b"\x00")
    entry += struct.pack("<iii", archive.PACK_SLOT, len(new), archive.PACK_SLOT + len(new) + (-len(new) % archive.PACK_ALIGN))
    entry = entry.ljust(archive.PACK_SLOT, b"\x00")
    return data[:end] + entry + new + bytes(-len(new) % archive.PACK_ALIGN) + data[end:]


# Puts the mods' files into the disc files build writes. files holds the files the build already
# rewrote (scripts compiled into their packs), keyed by DATA.DAT path; a replacement goes on top of
# those. Returns the files, and for each replacement its path, mod and old and new sizes.
def replaced(iso_path, wanted, files, script_tops=()):
    files = dict(files)
    report = []
    with open(iso_path, "rb") as f:
        disc = archive.open_archive(f)
        index = {entry["path"]: entry for entry in disc["entries"]}
        for path, (mod, file) in sorted(wanted.items()):
            top, members = location(path)
            if top not in index:
                raise ValueError("%s (mod %s): %s is not a file on the disc" % (path, mod, top))
            if not members and top in script_tops:
                raise ValueError("%s (mod %s): the build also changes scripts inside %s; replace its members "
                                 "one by one instead" % (path, mod, top))
            data = files[top] if top in files else archive.read_entry(f, disc, index[top])
            old = rebuild.read_member(data, members)
            try:
                if old is None:
                    # a member the pack does not have is added (a game file in a common format needs one
                    # to convert from, so it is given as the game file itself)
                    if converter_for(Path(file).as_posix())[1] is not None:
                        raise ValueError("%s has no member %s to convert to; give the game file itself"
                                         % (top, "/".join(members)))
                    new = Path(file).read_bytes()
                    files[top] = added_member(data, members, new)
                else:
                    new = game_bytes(file, old)
                    files[top] = rebuild.replaced_member(data, members, new)
            except ValueError as error:
                raise ValueError("%s (mod %s): %s" % (path, mod, error)) from None
            report.append({"path": path, "mod": mod, "old": None if old is None else len(old), "new": len(new), "data": new})
    return files, report


# Copies files and pack members off the disc into a mod's disc/ folder, to edit or replace there,
# in a common format when form names one (png). Returns the paths written.
def copy_out(iso_path, mod_dir, paths, form=None):
    written = []
    with open(iso_path, "rb") as f:
        disc = archive.open_archive(f)
        index = {entry["path"]: entry for entry in disc["entries"]}
        for path in paths:
            path = path.replace("\\", "/").strip("/")
            top, members = location(path)
            if top not in index:
                raise ValueError("%s is not a file on the disc" % top)
            data = rebuild.read_member(archive.read_entry(f, disc, index[top]), members)
            if data is None:
                raise ValueError("%s has no member %s" % (top, "/".join(members)))
            if form:
                suffix = "." + form.lower().lstrip(".")
                converter = CONVERTERS.get(suffix)
                if converter is None or not path.lower().endswith(converter["game"]):
                    raise ValueError("%s cannot be exported as %s" % (path, form))
                data, path = converter["export"](data), path + suffix
            target = Path(mod_dir) / DISC / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            written.append(path)
    return written


# The replaced files (replaced's report) that read back out of a built image differently.
def check(out_path, report):
    wrong = []
    with open(out_path, "rb") as f:
        disc = archive.open_archive(f)
        index = {entry["path"]: entry for entry in disc["entries"]}
        for item in report:
            top, members = location(item["path"])
            if rebuild.read_member(archive.read_entry(f, disc, index[top]), members) != item["data"]:
                wrong.append(item["path"])
    return wrong
