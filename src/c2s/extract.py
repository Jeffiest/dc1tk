import json
from pathlib import Path

from c2s import archive, iso

MAX_PACK_DEPTH = 4
PACK_SUFFIX = ".d"


def write_file(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def write_tree(out, path, data, depth, files):
    write_file(out / path, data)
    files.append({"path": path, "size": len(data), "packed": depth > 0})
    if depth >= MAX_PACK_DEPTH:
        return
    members = archive.parse_pack(data)
    if members is None:
        return
    for member in members:
        member_data = data[member["offset"]:member["offset"] + member["size"]]
        write_tree(out, path + PACK_SUFFIX + "/" + member["name"], member_data, depth + 1, files)


def boot_name(f):
    record = iso.find(f, "SYSTEM.CNF")
    if record is None:
        return None
    for line in iso.read_file(f, record).decode("ascii", "replace").splitlines():
        if line.replace(" ", "").startswith("BOOT2="):
            return line.split("\\")[-1].split(";")[0].strip()
    return None


def extract_boot(iso_path, out_dir):
    with open(iso_path, "rb") as f:
        name = boot_name(f)
        record = iso.find(f, name) if name else None
        if record is None:
            return None
        write_file(Path(out_dir) / name, iso.read_file(f, record))
    return name


def extract(iso_path, out_dir):
    out = Path(out_dir)
    files = []
    with open(iso_path, "rb") as f:
        disc = archive.open_archive(f)
        for entry in disc["entries"]:
            write_tree(out, entry["path"], archive.read_entry(f, disc, entry), 0, files)
    manifest = {
        "files": files,
        "scripts": [item["path"] for item in files if item["path"].lower().endswith(".stb")],
    }
    out.mkdir(parents=True, exist_ok=True)
    manifest["boot"] = extract_boot(iso_path, out_dir)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1, ensure_ascii=False))
    return manifest
