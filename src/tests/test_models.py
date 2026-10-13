import json
import struct
import tempfile
from pathlib import Path

from c2s import im3, mds, models, rebuild
from tests import build

QUAD = [(0.0, 0.0, 0.0, 1.0), (1.0, 0.0, 0.0, 1.0), (1.0, 1.0, 0.0, 1.0), (0.0, 1.0, 0.0, 1.0)]


def sample_pack():
    model = mds.MDS()
    model.nodes.append(mds.Node("root", -1))
    mesh = mds.MDT()
    mesh.vertices = list(QUAD)
    mesh.normals = [(0.0, 0.0, 1.0, 1.0)]
    mesh.uvs = [(0.0, 0.0, 1.0, 0.0), (1.0, 0.0, 1.0, 0.0), (1.0, 1.0, 1.0, 0.0), (0.0, 1.0, 1.0, 0.0)]
    mesh.materials = [mds.Material("tex01")]
    mesh.prims = [mds.Prim(mds.PRIM_TRIANGLES, 0, [(0, 0, 0), (1, 0, 1), (2, 0, 2), (0, 0, 0), (2, 0, 2), (3, 0, 3)])]
    model.nodes.append(mds.Node("body", 0, mdt=mesh))
    picture = im3.Image("tex01", "PSMCT32", 8, 8)
    picture.pixels = bytes([200, 100, 50, 0x80]) * 64
    info = b'v2;\nMODEL "c.mds";\n'
    return build.pack([("info.cfg", info), ("c.mds", mds.build(model)), ("c.img", im3.write([picture]))])


def written(files, folder):
    for name, content in files.items():
        (Path(folder) / name).parent.mkdir(parents=True, exist_ok=True)
        (Path(folder) / name).write_bytes(content)


def test_an_unedited_export_builds_back_to_the_same_pack():
    pack = sample_pack()
    exported = models.export(pack)
    assert "c.gltf" in exported and "c.bin" in exported and "textures/tex01.png" in exported
    # the texture is the pack's own picture, not a placeholder
    assert exported["textures/tex01.png"] == im3.to_png(im3.read(rebuild.read_member(pack, ["c.img"]))[0])
    with tempfile.TemporaryDirectory() as folder:
        written(exported, folder)
        assert models.build(folder, pack) == pack


def test_an_edited_mesh_and_texture_reach_the_pack():
    pack = sample_pack()
    with tempfile.TemporaryDirectory() as folder:
        written(models.export(pack), folder)
        gltf = json.loads((Path(folder) / "c.gltf").read_text(encoding="utf-8"))
        buffer = bytearray((Path(folder) / "c.bin").read_bytes())
        primitive = gltf["meshes"][0]["primitives"][0]
        accessor = gltf["accessors"][primitive["attributes"]["POSITION"]]
        base = gltf["bufferViews"][accessor["bufferView"]].get("byteOffset", 0) + accessor.get("byteOffset", 0)
        for i in range(accessor["count"]):
            x, y, z = struct.unpack_from("<3f", buffer, base + 12 * i)
            struct.pack_into("<3f", buffer, base + 12 * i, x * 2, y * 2, z)
        (Path(folder) / "c.bin").write_bytes(bytes(buffer))
        (Path(folder) / "textures" / "tex01.png").write_bytes(im3.write_png(8, 8, "RGB", bytes([0, 0, 255]) * 64))
        built = models.build(folder, pack)
    model = mds.parse(rebuild.read_member(built, ["c.mds"]))
    corners = {tuple(vertex[:2]) for vertex in model.nodes[1].mdt.vertices}
    assert corners == {(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)}
    assert model.nodes[0].name == "root" and model.nodes[1].name == "body"
    assert im3.read(rebuild.read_member(built, ["c.img"]))[0].pixels == bytes([0, 0, 255, 0x80]) * 64
    assert rebuild.read_member(built, ["info.cfg"]) == rebuild.read_member(pack, ["info.cfg"])
