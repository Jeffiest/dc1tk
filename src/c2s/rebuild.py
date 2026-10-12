import shutil
import struct
from pathlib import Path

from c2s import archive, compile, definitions, iso, library, natives, parse

SCRIPT_SUFFIXES = (".stb", ".STB")
VOLUME_SIZE = 80
FILLER = "DMMY"


def compiled(source_path, defs, headers, tables):
    program = parse.parse(Path(source_path).read_text(encoding="utf-8"))
    headers = dict(headers, **definitions.source_headers(source_path))
    program = library.expand(program, library.read_includes(program, str(source_path)), headers)
    if "asm" not in program and program["natives"] is None:
        program["natives"] = natives.table_for(Path(source_path).resolve().as_posix(), defs["tables"])
    return compile.compile_program(program, tables)


def source_scripts(source_dir):
    root = Path(source_dir)
    return sorted(path.relative_to(root).as_posix() for path in root.rglob("*.c2s")
                  if path.relative_to(root).parts[0] != "lib")


def disc_location(relative, index):
    stem = relative[:-len(".c2s")]
    parts = stem.split(".d/")
    if len(parts) == 1:
        for suffix in SCRIPT_SUFFIXES:
            if stem + suffix in index:
                return {"top": stem + suffix, "members": []}
        return None
    if parts[0] not in index:
        return None
    return {"top": parts[0], "members": parts[1:-1] + [parts[-1]]}


def member_named(members, name, last):
    for member in members:
        if member["name"] == name or (last and member["name"].rsplit(".", 1)[0] == name
                                      and member["name"].endswith(SCRIPT_SUFFIXES)):
            return member
    return None


def read_member(data, names):
    if not names:
        return data
    members = archive.parse_pack(data)
    if members is None:
        return None
    member = member_named(members, names[0], len(names) == 1)
    if member is None:
        return None
    return read_member(data[member["offset"]:member["offset"] + member["size"]], names[1:])


def replaced_member(data, names, new):
    if not names:
        return new
    members = archive.parse_pack(data)
    member = member_named(members, names[0], len(names) == 1)
    inner = replaced_member(data[member["offset"]:member["offset"] + member["size"]], names[1:], new)
    entry = member["offset"] - archive.PACK_SLOT
    old_end = member["offset"] + member["size"] + (-member["size"] % archive.PACK_ALIGN)
    header = bytearray(data[entry:member["offset"]])
    struct.pack_into("<ii", header, archive.PACK_NAME + 4, len(inner), archive.PACK_SLOT + len(inner) + (-len(inner) % archive.PACK_ALIGN))
    return data[:entry] + bytes(header) + inner + bytes(-len(inner) % archive.PACK_ALIGN) + data[old_end:]


def changes(source_dir, iso_path, defs, headers, tables):
    with open(iso_path, "rb") as f:
        disc = archive.open_archive(f)
        index = {entry["path"]: entry for entry in disc["entries"]}
        tops = {}
        found = []
        skipped = []
        for relative in source_scripts(source_dir):
            where = disc_location(relative, index)
            if where is None:
                skipped.append(relative)
                continue
            if where["top"] not in tops:
                tops[where["top"]] = archive.read_entry(f, disc, index[where["top"]])
            original = read_member(tops[where["top"]], where["members"])
            if original is None:
                skipped.append(relative)
                continue
            new = compiled(Path(source_dir) / relative, defs, headers, tables)
            if new != original:
                found.append(dict(where, source=relative, data=new))
    rebuilt = {}
    for change in found:
        data = rebuilt.get(change["top"], tops[change["top"]])
        rebuilt[change["top"]] = replaced_member(data, change["members"], change["data"])
    return {"scripts": found, "files": rebuilt, "skipped": skipped}


def sectors(size):
    return -(-size // iso.SECTOR)


def allocate(space, count):
    if space["limit"] is not None and space["next"] + count > space["limit"]:
        space["next"] = space["volume"]
        space["limit"] = None
    lba = space["next"]
    space["next"] += count
    space["volume"] = max(space["volume"], space["next"])
    return lba


def write_disc(iso_path, out_path, files):
    shutil.copyfile(iso_path, out_path)
    with open(out_path, "r+b") as f:
        index_record = iso.find(f, "DATA.HD4")
        data_record = iso.find(f, "DATA.DAT")
        filler = iso.find(f, FILLER)
        index_data = bytearray(iso.read_file(f, index_record))
        entries = archive.parse_index(bytes(index_data))
        position = {entry["path"]: number for number, entry in enumerate(entries)}
        pvd = bytearray(iso.read_at(f, iso.PVD_SECTOR * iso.SECTOR, iso.SECTOR))
        volume = max(struct.unpack_from("<I", pvd, VOLUME_SIZE)[0], sectors(f.seek(0, 2)))
        if filler is None:
            space = {"next": volume, "limit": None, "volume": volume}
        else:
            space = {"next": filler["extent"], "limit": filler["extent"] + sectors(filler["size"]), "volume": volume}
        placed = []
        for path, data in sorted(files.items()):
            entry = entries[position[path]]
            moved = sectors(len(data)) > sectors(entry["size"])
            lba = allocate(space, sectors(len(data))) if moved else data_record["extent"] + entry["sector"]
            f.seek(lba * iso.SECTOR)
            f.write(data + bytes(sectors(len(data)) * iso.SECTOR - len(data)))
            struct.pack_into("<ii", index_data, position[path] * archive.INDEX_RECORD + 4, len(data), lba - data_record["extent"])
            placed.append({"path": path, "size": len(data), "moved": moved})
        f.seek(index_record["extent"] * iso.SECTOR)
        f.write(bytes(index_data))
        if space["volume"] > volume:
            struct.pack_into("<I", pvd, VOLUME_SIZE, space["volume"])
            struct.pack_into(">I", pvd, VOLUME_SIZE + 4, space["volume"])
            f.seek(iso.PVD_SECTOR * iso.SECTOR)
            f.write(bytes(pvd))
    return {"placed": placed, "grown": space["volume"] - volume}


def check(out_path, found):
    wrong = []
    with open(out_path, "rb") as f:
        disc = archive.open_archive(f)
        index = {entry["path"]: entry for entry in disc["entries"]}
        for change in found:
            data = read_member(archive.read_entry(f, disc, index[change["top"]]), change["members"])
            if data != change["data"]:
                wrong.append(change["source"])
    return wrong
