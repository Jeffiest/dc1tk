from pathlib import Path

from c2s import archive, rebuild

# Game files a mod replaces: models, textures, sounds and anything else on the disc. A mod keeps
# them under disc/, laid out as extract lays the disc out: a file at its path in DATA.DAT
# (snd2/sp/sp_000.snd), or a member of a pack under the pack's name plus ".d"
# (chara/c01_base.chr.d/c01b.img for the texture inside Max's character pack). build writes each
# over the disc's own; members of packs inside packs work the same way (a.pac.d/b.chr.d/c.img).

DISC = "disc"
PACK = ".d/"


def mod_files(mod_dir):
    root = Path(mod_dir) / DISC
    if not root.is_dir():
        return {}
    return {path.relative_to(root).as_posix(): path for path in sorted(root.rglob("*")) if path.is_file()}


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
            if old is None:
                raise ValueError("%s (mod %s): %s has no member %s" % (path, mod, top, "/".join(members)))
            new = Path(file).read_bytes()
            files[top] = rebuild.replaced_member(data, members, new)
            report.append({"path": path, "mod": mod, "old": len(old), "new": len(new)})
    return files, report


# Copies files and pack members off the disc into a mod's disc/ folder, to edit or replace there.
# Returns the paths written.
def copy_out(iso_path, mod_dir, paths):
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
            target = Path(mod_dir) / DISC / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            written.append(path)
    return written


# The replaced files read back out of a built image differently from what the mods hold.
def check(out_path, wanted):
    wrong = []
    with open(out_path, "rb") as f:
        disc = archive.open_archive(f)
        index = {entry["path"]: entry for entry in disc["entries"]}
        for path, (mod, file) in sorted(wanted.items()):
            top, members = location(path)
            if rebuild.read_member(archive.read_entry(f, disc, index[top]), members) != Path(file).read_bytes():
                wrong.append(path)
    return wrong
