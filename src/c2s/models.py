import json
import tempfile
from pathlib import Path

from c2s import archive, from_gltf, im3, info_cfg, rebuild, textures, to_gltf

# Character models as glTF for mods, to edit in Blender or any tool that reads glTF 2.0. A
# character pack (.chr) holds its model (.mds mesh, .wgt skin weights, .bbp bind pose), its
# motions and its textures, so a mod gives a folder named after the pack plus .gltf
# (c02a.chr.gltf/) holding what an export writes: c02a.gltf with its .bin, the skeleton, skin and
# each motion as an animation, and textures/ with a PNG per texture the model uses (the pack's own
# pictures where it has them).
#
# build rebuilds the mesh, weights and bind pose from the edited glTF, with the disc's model as the
# template: the skeleton (frame names, order and flags) stays, since motions, info.cfg and cloth
# find frames by name, and meshes are matched to frames by name (Blender's ".001" suffixes are
# ignored). A texture PNG that differs from the export goes into the pack's texture file under its
# picture name. Parts of the folder left as exported change nothing, so an unedited export builds
# back to the disc's own bytes. See docs/assets/models.md for the formats and limits (32 bones per
# mesh, 4 weights per vertex).

TEXTURES = "textures"


def members(data):
    found = archive.parse_pack(data)
    if found is None:
        raise ValueError("not a character pack")
    return {member["name"]: data[member["offset"]:member["offset"] + member["size"]] for member in found}


# The pack's model, weights and bind pose member names, from its info.cfg.
def model_files(folder):
    if not (Path(folder) / "info.cfg").is_file():
        raise ValueError("the pack has no info.cfg")
    info = info_cfg.read(str(Path(folder) / "info.cfg"))
    if not info.model:
        raise ValueError("the pack's info.cfg names no model")
    stem = Path(info.model).stem
    return {"model": info.model, "weights": stem + ".wgt", "pose": info.motions.get(0, {}).get("bbp"), "stem": stem}


def unpacked(data, folder):
    for name, content in members(data).items():
        (Path(folder) / name).write_bytes(content)
    return model_files(folder)


# {name: PNG bytes} for every picture of the pack's texture files.
def pack_pictures(data):
    out = {}
    for name, content in members(data).items():
        if name.lower().endswith(".img"):
            for image in im3.read(content):
                if not image.no_image:
                    out.setdefault(image.name, (name, im3.to_png(image)))
    return out


# {relative path: bytes}: the glTF, its buffer and its textures for a character pack.
def export(data):
    pictures = pack_pictures(data)
    with tempfile.TemporaryDirectory() as temp:
        source, out = Path(temp) / "pack", Path(temp) / "out"
        source.mkdir()
        out.mkdir()
        files = unpacked(data, source)
        to_gltf.export(str(source), str(out / (files["stem"] + ".gltf")))
        textures_dir = out / TEXTURES
        if textures_dir.is_dir():
            for png in textures_dir.glob("*.png"):
                if png.stem in pictures:
                    png.write_bytes(pictures[png.stem][1])
        return {path.relative_to(out).as_posix(): path.read_bytes() for path in sorted(out.rglob("*")) if path.is_file()}


def folder_files(folder):
    return {path.relative_to(folder).as_posix(): path.read_bytes() for path in sorted(Path(folder).rglob("*")) if path.is_file()}


# {mesh name: what the mesh holds}: its node's transform and skin, and its primitives' vertex data
# and materials by value, so a tool that writes the same mesh another way still matches.
def mesh_contents(gltf_path):
    gltf, buffers = from_gltf.load_gltf(str(gltf_path))
    out = {}
    for node in gltf.get("nodes", []):
        if "mesh" not in node:
            continue
        mesh = gltf["meshes"][node["mesh"]]
        name = from_gltf.clean(node.get("name") or mesh.get("name"))
        if name.endswith("_skin"):
            name = name[:-len("_skin")]
        parts = [json.dumps({key: node.get(key) for key in ("translation", "rotation", "scale", "matrix", "skin")},
                            sort_keys=True)]
        for primitive in mesh["primitives"]:
            for _, index in sorted(primitive["attributes"].items()) + [("indices", primitive.get("indices"))]:
                if index is not None:
                    parts.append(repr(from_gltf.accessor(gltf, buffers, index)))
            material = gltf["materials"][primitive["material"]] if "material" in primitive else None
            parts.append(json.dumps(material, sort_keys=True))
        out[name] = "\n".join(parts)
    return out


# The meshes of the given glTF that differ from the export's, or that the export does not have.
def changed_meshes(given_gltf, exported):
    with tempfile.TemporaryDirectory() as temp:
        for name, content in exported.items():
            (Path(temp) / name).parent.mkdir(parents=True, exist_ok=True)
            (Path(temp) / name).write_bytes(content)
        before = mesh_contents(Path(temp) / Path(given_gltf).name)
    return {name for name, content in mesh_contents(given_gltf).items() if before.get(name) != content}


# The pack with the model and textures a folder gives in place of the original's.
def build(folder, original):
    exported = export(original)
    given = folder_files(folder)
    pack = original
    with tempfile.TemporaryDirectory() as temp:
        source = Path(temp) / "pack"
        source.mkdir()
        files = unpacked(original, source)
        gltf = files["stem"] + ".gltf"
        model_parts = [path for path in given if not path.lower().endswith(".png")]
        if any(given.get(path) != exported.get(path) for path in model_parts):
            if gltf not in given:
                raise ValueError("the folder has no %s, the model's glTF" % gltf)
            out = Path(temp) / "out"
            pose = str(source / files["pose"]) if files["pose"] and (source / files["pose"]).is_file() else None
            weights = str(source / files["weights"]) if (source / files["weights"]).is_file() else None
            try:
                # only the meshes that changed are rebuilt; the rest keep the disc's own (strips and all)
                meshes = changed_meshes(Path(folder) / gltf, exported)
                from_gltf.import_gltf(str(Path(folder) / gltf), str(source / files["model"]), str(out), bbp_path=pose,
                                      wgt_path=weights, stem=files["stem"], meshes=meshes)
            except (KeyError, IndexError, ValueError) as error:
                raise ValueError("%s: %s" % (gltf, error)) from None
            written = {files["model"]: files["stem"] + ".mds", files["weights"]: files["stem"] + ".wgt"}
            if files["pose"]:
                written[files["pose"]] = files["stem"] + ".bbp"
            for member, built in written.items():
                content = (out / built).read_bytes()
                if not (source / member).is_file():
                    if not content:
                        # a model with no skinned mesh has no weights to write
                        continue
                    raise ValueError("the pack has no %s for the model's new skin weights; a model without "
                                     "skinned meshes cannot gain them" % member)
                pack = rebuild.replaced_member(pack, [member], content)
        pack = with_textures(pack, folder, given, exported)
    return pack


# The pack with the texture PNGs that differ from the export put into its texture files. A PNG
# counts wherever it is in the folder (Blender's glTF export writes them beside the .gltf), by its
# picture name.
def with_textures(pack, folder, given, exported):
    pictures = pack_pictures(pack)
    changed = {}
    for path, content in given.items():
        if path.lower().endswith(".png") and content != exported.get("%s/%s" % (TEXTURES, Path(path).name)):
            name = Path(path).stem
            if name not in pictures:
                raise ValueError("%s: the pack has no picture %s; textures from other packs are changed there" % (path, name))
            if any(png.name == Path(path).name for pngs in changed.values() for png in pngs):
                raise ValueError("%s: picture %s is given twice in the folder" % (path, name))
            changed.setdefault(pictures[name][0], []).append(Path(folder) / path)
    for member, pngs in sorted(changed.items()):
        with tempfile.TemporaryDirectory() as temp:
            for png in pngs:
                (Path(temp) / textures.file_name(png.stem)).write_bytes(png.read_bytes())
            img = rebuild.read_member(pack, [member])
            pack = rebuild.replaced_member(pack, [member], textures.build(temp, img))
    return pack
