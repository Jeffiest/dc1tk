"""Round trip for the dc1tk_maps add-on, run headless:

blender -b --factory-startup --python blender-plugin/tests/dc1_roundtrip.py -- <maps export> <work folder> [edit]

Imports floors/d01main_a, scenes/gedit/e01/in/0101/0101.scn and layouts/gedit/e01/gdata0.edt.json from the maps
export, exports each into <work>/out/..., and with `edit` also moves a vertex, a torch, a scene part, a cell and a
layout part first. The builds are then run by the caller.
"""

import os
import sys

import addon_utils
import bpy

args = sys.argv[sys.argv.index("--") + 1:]
maps_dir, work = args[0], args[1]
edit = "edit" in args[2:]

name = next((m.__name__ for m in addon_utils.modules() if m.__name__.endswith("dc1tk_maps")), None)
assert name, "the add-on is not installed"
addon_utils.enable(name, default_set=True)
assert hasattr(bpy.ops.dc1, "import_floor"), "the add-on did not register"


def roots():
    return [o for o in bpy.data.objects if "dc1_kind" in o]


def export(root, target):
    bpy.context.view_layer.objects.active = root
    if root["dc1_kind"] == "layout":
        result = bpy.ops.dc1.export(filepath=target)
    else:
        result = bpy.ops.dc1.export(directory=target)
    assert result == {"FINISHED"}, result


floor = os.path.join(maps_dir, "floors", "d01main_a")
scene = os.path.join(maps_dir, "scenes", "gedit", "e01", "in", "0101", "0101.scn")
layout = os.path.join(maps_dir, "layouts", "gedit", "e01", "gdata0.edt.json")
out = os.path.join(work, "out")

bpy.ops.dc1.import_floor(filepath=os.path.join(floor, "floor.json"))
bpy.ops.dc1.import_scene(filepath=os.path.join(scene, "scene.json"))
bpy.ops.dc1.import_layout(filepath=layout)
kinds = {r["dc1_kind"]: r for r in roots()}
assert set(kinds) == {"floor", "scene", "layout"}, kinds

meshes = [o for o in kinds["floor"].children_recursive if o.type == "MESH"]
fires = [o for o in kinds["floor"].children_recursive if "dc1_fire" in o]
print("floor: %d objects, %d meshes, %d torches" % (len(kinds["floor"].children_recursive), len(meshes), len(fires)))
assert meshes and fires
assert any(m.name.endswith("_a1") or "col_" in (m.active_material.name if m.active_material else "") for m in meshes)

if edit:
    wall = next(o for o in meshes if os.path.basename(o["dc1_file"]) == "d01g01_0.gltf" if "dc1_file" in o)
    wall.data.vertices[0].co.y += 3.0
    fires[0].location.y += 1.0
    part = next(o for o in kinds["scene"].children if "dc1_part" in o)
    part.location.x += 10.0
    grid = next(o for o in part.children if "dc1_cells" in o)
    cells = list(grid["dc1_cells"])
    cells[0] = "81" + cells[0][2:]
    grid["dc1_cells"] = cells
    placed = next(o for o in kinds["layout"].children if "dc1_layout_part" in o)
    placed.location.z += 50.0

export(kinds["floor"], os.path.join(out, "floors", "d01main_a"))
export(kinds["scene"], os.path.join(out, "scenes", "0101.scn"))
export(kinds["layout"], os.path.join(out, "layouts", "gdata0.edt.json"))
print("ROUNDTRIP OK")
