"""Checks the Rust port (c2) gives the Python tools' (cli.py) bytes. Each check runs both on the
same input in a work folder and compares; exit status 1 when anything differs.

  python rust/parity.py extract pal.iso WORK          extract with both, compare the trees
  python rust/parity.py write-disc pal.iso WORK       write a set of files touching every branch of
                                                      write_disc with both, compare the images
  python rust/parity.py write-disc pal.iso WORK --mod castle-showcase
                                                      the files cli.py build writes for a mod,
                                                      written by both
  python rust/parity.py cp932 WORK                    every code and character through both codecs
  python rust/parity.py disassemble extracted WORK    every script disassembled by both, and c2
                                                      assembling each listing back to the script
  python rust/parity.py decompile extracted WORK      every script's program tree (before names), as JSON
  python rust/parity.py decompile extracted WORK --source   the same written as C

Run from the repository folder, after cargo build --release in rust/. Images are 4.7 GB each; the
work folder needs room for two.
"""
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
C2 = REPO / "rust" / "target" / "release" / ("c2.exe" if os.name == "nt" else "c2")
sys.path.insert(0, str(REPO / "src"))


def run(command, **options):
    start = time.time()
    result = subprocess.run([str(part) for part in command], capture_output=True, text=True, encoding="utf-8", **options)
    if result.returncode != 0:
        raise SystemExit("%s failed:\n%s%s" % (" ".join(map(str, command)), result.stdout, result.stderr))
    return result.stdout, time.time() - start


def listing(root):
    out = {}
    for folder, dirs, files in os.walk(root):
        for name in dirs + files:
            path = Path(folder) / name
            out[path.relative_to(root).as_posix()] = path
    return out


def compare_trees(a, b):
    left, right = listing(a), listing(b)
    problems = ["only in %s: %s" % (a.name, path) for path in sorted(set(left) - set(right))]
    problems += ["only in %s: %s" % (b.name, path) for path in sorted(set(right) - set(left))]
    for path in sorted(set(left) & set(right)):
        x, y = left[path], right[path]
        if x.is_dir() != y.is_dir() or (x.is_file() and (x.stat().st_size != y.stat().st_size or x.read_bytes() != y.read_bytes())):
            problems.append("different: %s" % path)
    print("%d entries compared" % len(left))
    return problems


def compare_files(a, b, chunk=1 << 24):
    if a.stat().st_size != b.stat().st_size:
        return ["%s is %d bytes, %s is %d" % (a.name, a.stat().st_size, b.name, b.stat().st_size)]
    with open(a, "rb") as x, open(b, "rb") as y:
        offset = 0
        while True:
            left, right = x.read(chunk), y.read(chunk)
            if left != right:
                at = next(i for i in range(len(left)) if left[i] != right[i])
                return ["%s and %s first differ at byte %d" % (a.name, b.name, offset + at)]
            if not left:
                return []
            offset += len(left)


def check_extract(args):
    work = Path(args.work)
    py, rs = work / "py_extracted", work / "rs_extracted"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    py_out, py_time = run([sys.executable, REPO / "cli.py", "extract", args.iso, py])
    rs_out, rs_time = run([C2, "extract", args.iso, rs])
    print("python %.1fs, rust %.1fs" % (py_time, rs_time))
    problems = [] if py_out.replace(str(py), "OUT").replace(py.name, "OUT") == rs_out.replace(str(rs), "OUT").replace(rs.name, "OUT") \
        else ["printed output differs:\n%s\n%s" % (py_out, rs_out)]
    return problems + compare_trees(py, rs)


def save_files(files, folder):
    folder.mkdir(parents=True, exist_ok=True)
    paths = sorted(files)
    for number, path in enumerate(paths):
        (folder / ("%d.bin" % number)).write_bytes(files[path])
    (folder / "files.json").write_text(json.dumps(paths, ensure_ascii=False), encoding="utf-8")


def synthetic_files(iso_path):
    """Files that take write_disc through each branch: in place (smaller, same size), moved into
    the DMMY filler, past the end of the volume, and new files enough for the index to outgrow its
    sectors."""
    from c2s import archive, iso
    with open(iso_path, "rb") as f:
        disc = archive.open_archive(f)
        filler = iso.find(f, "DMMY")
        by_path = {entry["path"]: entry for entry in disc["entries"]}
        files = {
            "snd2/event/EV_000_130.snd": archive.read_entry(f, disc, by_path["snd2/event/EV_000_130.snd"])[:400000],
            "edit.cfg": archive.read_entry(f, disc, by_path["edit.cfg"]),
            "mainchr/c02fuku1.chr": archive.read_entry(f, disc, by_path["mainchr/c02fuku1.chr"]) + bytes(5000),
            "mods/synthetic/huge.bin": b"\xab" * (filler["size"] + 4096),
        }
        for number in range(120):
            files["mods/synthetic/new_file_%03d.bin" % number] = bytes([number]) * (1000 + number)
    return files


def check_write_disc(args):
    from c2s import __main__ as c2s_main
    from c2s import rebuild
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    folder = work / "files"
    py_iso, rs_iso = work / "py.iso", work / "rs.iso"
    if args.mod:
        captured = {}
        real = rebuild.write_disc

        def capture(iso_path, out_path, files):
            captured.update(files)
            return real(iso_path, out_path, files)

        rebuild.write_disc = capture
        saved = sys.argv
        sys.argv = ["cli.py", "build", args.source, args.iso, "-o", str(py_iso), "--mod", args.mod]
        start = time.time()
        try:
            if c2s_main.main():
                raise SystemExit("cli.py build failed")
        finally:
            sys.argv = saved
            rebuild.write_disc = real
        py_time = time.time() - start
        files = captured
    else:
        files = synthetic_files(args.iso)
        start = time.time()
        rebuild.write_disc(args.iso, str(py_iso), files)
        py_time = time.time() - start
    save_files(files, folder)
    _, rs_time = run([C2, "write-disc", args.iso, rs_iso, folder])
    print("%d files; python %.1fs%s, rust %.1fs" % (len(files), py_time, " (whole build)" if args.mod else "", rs_time))
    problems = compare_files(py_iso, rs_iso)
    if not args.keep:
        py_iso.unlink()
        rs_iso.unlink()
    return problems


def check_disassemble(args):
    """Every script under an extracted disc disassembled by both, compared; then every listing
    assembled by c2 and compared with the script it came from."""
    from c2s import asm, definitions, natives, stb, verify
    work = Path(args.work)
    py, rs = work / "py_asm", work / "rs_asm"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    root = Path(args.extracted)
    defs = definitions.load([definitions.DEFAULT_SET])
    tables = natives.load()
    start = time.time()
    scripts = verify.find_scripts(root)
    for relative in scripts:
        path = root / relative
        table = natives.table_for(path.resolve().as_posix(), defs["tables"])
        target = py / (relative + ".asm")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(asm.disassemble(stb.read(path.read_bytes()), table, tables).encode("utf-8"))
    py_time = time.time() - start
    _, rs_time = run([C2, "disassemble-tree", root, rs])
    print("%d scripts; python %.1fs, rust %.1fs" % (len(scripts), py_time, rs_time))
    problems = compare_trees(py, rs)
    out, rs_time = run_status([C2, "assemble-tree", root, py])
    print("rust assembled python's listings in %.1fs: %s" % (rs_time, out.strip().splitlines()[-1]))
    problems += [line for line in out.splitlines() if line.startswith("differs")]
    return problems


def check_decompile(args):
    """Every script's program tree from decompile.decompile, before names and docs, compared as
    JSON."""
    from c2s import decompile, definitions, names, natives, notes, source, stb, verify
    work = Path(args.work)
    py, rs = work / "py_tree", work / "rs_tree"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    root = Path(args.extracted)
    defs = definitions.load([definitions.DEFAULT_SET])
    tables = natives.load()
    start = time.time()
    scripts = verify.find_scripts(root)
    for relative in scripts:
        path = root / relative
        table = natives.table_for(path.resolve().as_posix(), defs["tables"])
        program = decompile.decompile(stb.read(path.read_bytes()), table, tables)
        if args.named:
            entries = names.with_loose(names.merged(defs["names"]), names.loose_map(program))
            program = names.apply(program, entries, set(), {})
            program = names.with_script(program, definitions.script_entry(defs, str(path)))
            program = notes.annotate(program, defs["strings"])
        if args.source:
            target, text = py / (relative + ".c"), source.text(program)
        else:
            target, text = py / (relative + ".json"), json.dumps(program, ensure_ascii=False, separators=(",", ":"))
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(text.encode("utf-8"))
    py_time = time.time() - start
    _, rs_time = run([C2, "decompile-dump", root, rs] + (["--source"] if args.source else []) + (["--named"] if args.named else []))
    print("%d scripts; python %.1fs, rust %.1fs" % (len(scripts), py_time, rs_time))
    return compare_trees(py, rs)


def run_status(command):
    start = time.time()
    result = subprocess.run([str(part) for part in command], capture_output=True, text=True, encoding="utf-8")
    return result.stdout + result.stderr, time.time() - start


def check_cp932(args):
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    decode, _ = run(["cargo", "run", "--release", "-q", "-p", "c2-disc", "--example", "cp932_table"], cwd=REPO / "rust")
    encode, _ = run(["cargo", "run", "--release", "-q", "-p", "c2-disc", "--example", "cp932_table", "encode"], cwd=REPO / "rust")

    def points(text):
        return " ".join("%04X" % ord(c) for c in text)

    problems = []
    for line in decode.splitlines():
        code, strict, lossy = line.split("\t")
        data = bytes.fromhex(code)
        try:
            expected = points(data.decode("cp932"))
        except UnicodeDecodeError:
            expected = "ERR"
        if expected != strict or points(data.decode("cp932", "replace")) != lossy:
            problems.append("decoding %s differs" % code)
    for line in encode.splitlines():
        point, got = line.split("\t")
        try:
            expected = chr(int(point, 16)).encode("cp932").hex().upper()
        except UnicodeEncodeError:
            expected = "ERR"
        if expected != got:
            problems.append("encoding U+%s differs: python %s, rust %s" % (point, expected, got))
    print("%d codes decoded, %d characters encoded" % (len(decode.splitlines()), len(encode.splitlines())))
    return problems


# The asset converters by the common format's suffix: the game files each converts and its module.
ASSET_KINDS = {"png": ".img", "wav": ".snd", "gltf": ".chr"}


def asset_module(kind):
    from c2s import models, sounds, textures
    return {"png": textures, "wav": sounds, "gltf": models}[kind]


def game_files(root, kind):
    found = []
    for folder, _, files in os.walk(root):
        for name in files:
            if name.lower().endswith(ASSET_KINDS[kind]):
                found.append((Path(folder) / name).relative_to(root).as_posix())
    return sorted(found)


def write_lines(path, lines):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes("".join(line + "\n" for line in sorted(lines)).encode("utf-8"))


def python_export(kind, root, out, files):
    """What c2 asset-export does: each game file exported to out/<path>.<kind>/, the ones that
    cannot be listed in out/failed.txt."""
    module = asset_module(kind)
    failed = []
    for relative in files:
        try:
            exported = module.export((root / relative).read_bytes())
        except Exception:
            failed.append(relative)
            continue
        folder = out / (relative + "." + kind)
        for name, content in exported.items():
            (folder / name).parent.mkdir(parents=True, exist_ok=True)
            (folder / name).write_bytes(content)
    write_lines(out / "failed.txt", failed)
    return failed


def python_build(kind, root, folders, out, files):
    """What c2 asset-build does: each game file folders has a <path>.<kind>/ for built from it into
    out/<path>, failures (path, tab, message) in out/failed.txt."""
    module = asset_module(kind)
    failed = []
    for relative in files:
        folder = folders / (relative + "." + kind)
        if not folder.is_dir():
            continue
        try:
            built = module.build(folder, (root / relative).read_bytes())
        except Exception as error:
            failed.append("%s\t%s" % (relative, error))
            continue
        (out / relative).parent.mkdir(parents=True, exist_ok=True)
        (out / relative).write_bytes(built)
    write_lines(out / "failed.txt", failed)
    return failed


def compare_builds(py, rs):
    """The built trees compared, failures by path only (the messages are printed, not compared)."""
    def failures(folder):
        lines = (folder / "failed.txt").read_text(encoding="utf-8").splitlines()
        return {line.split("\t")[0]: line for line in lines}
    left, right = failures(py), failures(rs)
    problems = ["failed in %s only: %s" % (name, path) for name, a, b in (("python", left, right), ("rust", right, left))
                for path in sorted(set(a) - set(b))]
    for path in sorted(set(left) & set(right)):
        if left[path] != right[path]:
            print("both failed, other messages:\n  %s\n  %s" % (left[path], right[path]))
    return problems + compare_trees(py, rs)


def fresh(*folders):
    for folder in folders:
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)


def check_asset_exports(kind, args, work):
    """Every game file of a kind exported by both and compared; then c2 building each of the
    Python's unedited exports, which must give the original bytes."""
    root = Path(args.extracted)
    py, rs = work / ("py_" + kind), work / ("rs_" + kind)
    fresh(py, rs)
    files = game_files(root, kind)
    start = time.time()
    failed = python_export(kind, root, py, files)
    py_time = time.time() - start
    out, rs_time = run([C2, "asset-export", kind, root, rs])
    print("%d %s files, %d not exported; python %.1fs, rust %.1fs (%s)" % (len(files), ASSET_KINDS[kind], len(failed), py_time,
                                                                      rs_time, out.strip()))
    problems = compare_trees(py, rs)
    out, rs_time = run([C2, "asset-build", kind, root, py, work / ("rs_" + kind + "_unedited"), "--check"])
    print("rust built the unedited exports in %.1fs: %s" % (rs_time, out.strip().splitlines()[-1]))
    problems += [line for line in out.splitlines() if line.startswith("differs")]
    if args.python_builds:
        start = time.time()
        built = work / ("py_" + kind + "_unedited")
        python_build(kind, root, py, built, files)
        same = sum(1 for path in game_files(built, kind) if (built / path).read_bytes() == (root / path).read_bytes())
        print("python built the unedited exports in %.1fs: %d give the original" % (time.time() - start, same))
    return problems, files, py


def check_asset_edits(kind, args, work, files, edited):
    """Both build the edited folders; the built files compared."""
    root = Path(args.extracted)
    py, rs = work / ("py_" + kind + "_edited"), work / ("rs_" + kind + "_edited")
    fresh(py, rs)
    start = time.time()
    python_build(kind, root, edited, py, files)
    py_time = time.time() - start
    out, rs_time = run([C2, "asset-build", kind, root, edited, rs])
    print("edited: python %.1fs, rust %.1fs (%s)" % (py_time, rs_time, out.strip()))
    return compare_builds(py, rs)


def edited_pngs(exported, edited, every):
    """Deterministic edits of every n-th texture file's PNGs: colours inverted (true colour, so a
    paletted picture is matched or quantised), the palette's red and blue swapped, pixels moved to
    other colours of the palette, and pictures halved in size."""
    from c2s import im3
    folders = sorted(path for path in exported.rglob("*.img.png") if path.is_dir())
    count = 0
    for number, folder in enumerate(folders[::every]):
        target = edited / folder.relative_to(exported)
        target.mkdir(parents=True)
        for k, png in enumerate(sorted(folder.glob("*.png"))):
            w, h, mode, data, palette = im3.read_png(png.read_bytes())
            if mode == "P":
                table = [bytes(c) for c in palette] + [b"\0\0\0\xff"] * (256 - len(palette))
                rgba = b"".join(table[i] for i in data)
            else:
                rgba = data
            how = (number + k) % 5
            if how == 0 and w * h <= 128 * 128:
                out = im3.write_png(w, h, "RGBA", bytes(255 - v if i % 4 != 3 else v for i, v in enumerate(rgba)))
            elif how == 1 and mode == "P":
                out = im3.write_png(w, h, "P", data, [(b, g, r, a) for r, g, b, a in palette])
            elif how == 2 and mode == "P":
                out = im3.write_png(w, h, "RGBA", b"".join(table[(v + 1) % len(palette)] for v in data))
            elif how == 3 and w > 1 and h > 1:
                half = b"".join(rgba[4 * (2 * y * w + 2 * x):4 * (2 * y * w + 2 * x) + 4] for y in range(h // 2) for x in range(w // 2))
                out = im3.write_png(w // 2, h // 2, "RGBA", half)
            else:
                continue
            (target / png.name).write_bytes(out)
            count += 1
    print("%d edited PNGs in %d texture files" % (count, len(folders[::every])))


def check_textures(args):
    work = Path(args.work)
    problems, files, exported = check_asset_exports("png", args, work)
    edited = work / "png_edits"
    fresh(edited)
    edited_pngs(exported, edited, args.every)
    return problems + check_asset_edits("png", args, work, files, edited)


def wav_file(rate, channels, bits, values, loop=None, floating=False):
    """A WAV of interleaved sample values (-1.0..1.0), PCM of the bits or float, with a 'smpl' loop
    (start, end) when given."""
    import struct
    if floating:
        body = struct.pack("<%d%s" % (len(values), "f" if bits == 32 else "d"), *values)
    elif bits == 8:
        body = bytes(max(0, min(255, int(128 + v * 127))) for v in values)
    elif bits == 24:
        body = b"".join(int(v * 8388607).to_bytes(3, "little", signed=True) for v in values)
    else:
        body = struct.pack("<%d%s" % (len(values), "h" if bits == 16 else "i"),
                           *(int(v * (32767 if bits == 16 else 2147483647)) for v in values))
    align = channels * bits // 8
    chunks = b"fmt " + struct.pack("<IHHIIHH", 16, 3 if floating else 1, channels, rate, rate * align, align, bits)
    chunks += b"data" + struct.pack("<I", len(body)) + body + (b"\0" if len(body) % 2 else b"")
    if loop:
        smpl = struct.pack("<9I", 0, 0, 0, 60, 0, 0, 0, 1, 0) + struct.pack("<6I", 0, 0, loop[0], loop[1], 0, 0)
        chunks += b"smpl" + struct.pack("<I", len(smpl)) + smpl
    return b"RIFF" + struct.pack("<I", 4 + len(chunks)) + b"WAVE" + chunks


def synthetic_sounds(seed):
    """WAVs to put into a bank: a tone, stereo noise at 96 kHz (resampled to 48 kHz), a looped
    sweep whose loop needs stretching, and 8-bit, 24-bit and float ones at other rates."""
    import math
    state = [seed * 7919 + 1]

    def noise():
        state[0] = (state[0] * 1103515245 + 12345) % (1 << 31)
        return state[0] / (1 << 30) - 1.0
    tone = [0.6 * math.sin(2 * math.pi * 440 * i / 22050) for i in range(6000 + 37 * seed)]
    stereo = [0.3 * noise() for _ in range(2 * 4000)]
    sweep = [0.5 * math.sin(i * i / 9000.0) for i in range(5000)]
    return [
        wav_file(22050, 1, 16, tone),
        wav_file(96000, 2, 16, stereo),
        wav_file(32000, 1, 16, sweep, loop=(1234 + seed, 4999)),
        wav_file(11025, 1, 8, tone[:3000]),
        wav_file(44100, 2, 24, [v * 0.5 for v in tone[:4000] for _ in range(2)]),
        wav_file(16000, 1, 32, tone[:2500], floating=True),
    ]


def edited_wavs(root, exported, edited, every):
    """Deterministic WAV sets for every n-th bank: exported entries replaced by synthetic sounds
    (in place or detached, as the bank's sharing decides), one left as exported, a sequence entry
    replaced, and a new entry added."""
    from c2s import soundbank
    folders = sorted(path for path in exported.rglob("*.snd.wav") if path.is_dir())
    count = 0
    for number, folder in enumerate(folders[::every]):
        relative = folder.relative_to(exported).as_posix()[:-len(".wav")]
        bank = soundbank.Bank.from_bytes((root / relative).read_bytes())
        entries = bank.entries()
        target = edited / folder.relative_to(exported)
        target.mkdir(parents=True)
        sounds = synthetic_sounds(number)
        wavs = sorted(folder.glob("*.wav"))
        for k, wav in enumerate(wavs[:5]):
            if k == 1:
                (target / wav.name).write_bytes(wav.read_bytes())
            else:
                (target / wav.name).write_bytes(sounds[(number + k) % len(sounds)])
            count += 1
        exported_numbers = {int(wav.name[:3]) for wav in wavs}
        others = [entry.index for entry in entries if entry.index not in exported_numbers]
        if others:
            (target / ("%03d_sequence.wav" % others[number % len(others)])).write_bytes(sounds[number % len(sounds)])
            count += 1
        (target / ("%03d_new_tone.wav" % len(entries))).write_bytes(sounds[(number + 2) % len(sounds)])
        count += 1
    print("%d edited WAVs in %d banks" % (count, len(folders[::every])))


def check_sounds(args):
    work = Path(args.work)
    problems, files, exported = check_asset_exports("wav", args, work)
    edited = work / "wav_edits"
    fresh(edited)
    edited_wavs(Path(args.extracted), exported, edited, args.every)
    return problems + check_asset_edits("wav", args, work, files, edited)


def edited_models(exported, edited, every):
    """Deterministic edits of every n-th character pack's export: the positions of every other mesh
    scaled (rigid and skinned ones), one rigid mesh's node moved, and one texture recoloured."""
    import shutil
    import struct
    from c2s import im3
    folders = sorted(path for path in exported.rglob("*.chr.gltf") if path.is_dir())
    count = 0
    for number, folder in enumerate(folders[::every]):
        target = edited / folder.relative_to(exported)
        shutil.copytree(folder, target)
        gltf_path = next(target.glob("*.gltf"))
        gltf = json.loads(gltf_path.read_text(encoding="utf-8"))
        bin_path = target / gltf["buffers"][0]["uri"]
        buffer = bytearray(bin_path.read_bytes())
        for m, mesh in enumerate(gltf.get("meshes", [])):
            if (m + number) % 2:
                continue
            for primitive in mesh["primitives"]:
                accessor = gltf["accessors"][primitive["attributes"]["POSITION"]]
                base = gltf["bufferViews"][accessor["bufferView"]].get("byteOffset", 0) + accessor.get("byteOffset", 0)
                for i in range(accessor["count"]):
                    x, y, z = struct.unpack_from("<3f", buffer, base + 12 * i)
                    struct.pack_into("<3f", buffer, base + 12 * i, x * 1.25, y * 0.75, z + 0.5)
            count += 1
        bin_path.write_bytes(bytes(buffer))
        rigid = [node for node in gltf["nodes"] if "mesh" in node and "skin" not in node]
        if rigid and number % 3 == 0:
            rigid[0]["translation"] = [v + 0.25 for v in rigid[0].get("translation", [0.0, 0.0, 0.0])]
            gltf_path.write_text(json.dumps(gltf, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
            count += 1
        pngs = sorted((target / "textures").glob("*.png")) if (target / "textures").is_dir() else []
        if pngs:
            png = pngs[number % len(pngs)]
            w, h, mode, data, palette = im3.read_png(png.read_bytes())
            if mode == "P":
                table = [bytes(c) for c in palette] + [b"\0\0\0\xff"] * (256 - len(palette))
                data = b"".join(table[i] for i in data)
            png.write_bytes(im3.write_png(w, h, "RGBA", bytes(255 - v if i % 4 != 3 else v for i, v in enumerate(data))))
            count += 1
    print("%d edits in %d character packs" % (count, len(folders[::every])))


def check_models(args):
    work = Path(args.work)
    problems, files, exported = check_asset_exports("gltf", args, work)
    edited = work / "gltf_edits"
    fresh(edited)
    edited_models(exported, edited, args.every)
    return problems + check_asset_edits("gltf", args, work, files, edited)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    checks = parser.add_subparsers(dest="check", required=True)
    command = checks.add_parser("extract")
    command.add_argument("iso")
    command.add_argument("work")
    command = checks.add_parser("write-disc")
    command.add_argument("iso")
    command.add_argument("work")
    command.add_argument("--mod", help="the files cli.py build writes for this mod")
    command.add_argument("--source", default="decompiled", help="the decompiled tree build uses")
    command.add_argument("--keep", action="store_true", help="keep the two images")
    command = checks.add_parser("cp932")
    command.add_argument("work")
    command = checks.add_parser("disassemble")
    command.add_argument("extracted")
    command.add_argument("work")
    command = checks.add_parser("decompile")
    command.add_argument("extracted")
    command.add_argument("work")
    command.add_argument("--source", action="store_true", help="compare the source text instead of the tree")
    command.add_argument("--named", action="store_true", help="with the definitions' names, script entries and "
                         "translations (without the SDK's names)")
    for name, every in (("textures", 25), ("sounds", 20), ("models", 20)):
        command = checks.add_parser(name)
        command.add_argument("extracted")
        command.add_argument("work")
        command.add_argument("--every", type=int, default=every, help="edit every n-th file's export (default %d)" % every)
        command.add_argument("--python-builds", action="store_true", help="also build every unedited export with the "
                             "Python and count the ones giving the original")
    args = parser.parse_args()
    if not C2.is_file():
        raise SystemExit("%s is missing; run cargo build --release in rust/" % C2)
    checks = {"extract": check_extract, "write-disc": check_write_disc, "cp932": check_cp932, "disassemble": check_disassemble,
              "decompile": check_decompile, "textures": check_textures,
              "sounds": check_sounds, "models": check_models}
    problems = checks[args.check](args)
    for problem in problems[:40]:
        print(problem)
    print("%s: %s" % (args.check, "same bytes" if not problems else "%d differences" % len(problems)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
