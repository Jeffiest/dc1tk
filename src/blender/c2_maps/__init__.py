"""Dark Chronicle maps: import a map exported by `c2 map export`, edit its placements, trigger
points and collision, check it, and export it into a mod (docs/assets/maps.md, section 8)."""

bl_info = {
    "name": "Dark Chronicle Maps (c2)",
    "author": "ChronicleTwo-Script",
    "version": (1, 0, 0),
    "blender": (3, 6, 0),
    "location": "File > Import > Dark Chronicle map; View3D > Sidebar > Dark Chronicle",
    "description": "Edit Dark Chronicle (PS2) maps exported by c2 and export them into a mod",
    "category": "Import-Export",
}

if "bpy" in locals():
    import importlib

    for _module in (schema, c2data, prefs, props, runner, import_map, edit, validate, export_mod, overlay, ui):  # noqa: F821
        importlib.reload(_module)

import bpy  # noqa: E402

from . import schema, c2data, prefs, props, runner, import_map, edit, validate, export_mod, overlay, ui  # noqa: E402

_modules = (prefs, props, import_map, edit, validate, export_mod, overlay, ui)


def register():
    for module in _modules:
        module.register()


def unregister():
    for module in reversed(_modules):
        module.unregister()
