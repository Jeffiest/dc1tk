"""Add-on preferences: where c2 is, and the paths the import, export and build use."""

import os
import sys

import bpy
from bpy.props import StringProperty

PACKAGE = __package__


class C2Preferences(bpy.types.AddonPreferences):
    bl_idname = PACKAGE

    repo_dir: StringProperty(
        name="c2 repository",
        description="The ChronicleTwo-Script folder (holding c2.bat, ./c2 and extracted/)",
        subtype="DIR_PATH",
        default="",
    )
    c2_path: StringProperty(
        name="c2 executable",
        description="c2 itself; empty to use build/release/c2(.exe), c2.bat or ./c2 in the repository",
        subtype="FILE_PATH",
        default="",
    )
    export_args: StringProperty(
        name="Map export arguments",
        description="c2's arguments to export a map; {name} is the map, {out} the folder",
        default="map-export {name} -o {out}",
    )
    maps_dir: StringProperty(
        name="Exported maps",
        description="Where c2 exports maps to; empty for extracted/maps in the repository",
        subtype="DIR_PATH",
        default="",
    )
    mod_dir: StringProperty(
        name="Mod folder",
        description="The mod the map is exported into (mods/NAME, holding mod.json)",
        subtype="DIR_PATH",
        default="",
    )
    source_dir: StringProperty(
        name="Source tree",
        description="The decompiled scripts c2 build compiles; empty for extracted/scripts in the repository",
        subtype="DIR_PATH",
        default="",
    )
    iso_path: StringProperty(
        name="Retail disc image",
        description="The game's ISO the build starts from",
        subtype="FILE_PATH",
        default="",
    )
    output_iso: StringProperty(
        name="Output disc image",
        description="Where c2 build writes the modded ISO",
        subtype="FILE_PATH",
        default="",
    )

    def draw(self, context):
        layout = self.layout
        layout.prop(self, "repo_dir")
        layout.prop(self, "c2_path")
        found = c2_command(self)
        layout.label(text="c2: " + (" ".join(found) if found else "not found"), icon="CONSOLE")
        layout.prop(self, "export_args")
        layout.prop(self, "maps_dir")
        layout.separator()
        layout.prop(self, "mod_dir")
        layout.prop(self, "source_dir")
        layout.prop(self, "iso_path")
        layout.prop(self, "output_iso")


class _Defaults:
    """Stands in for the preferences when the add-on is not enabled through Blender (tests)."""

    repo_dir = ""
    c2_path = ""
    export_args = "map-export {name} -o {out}"
    maps_dir = ""
    mod_dir = ""
    source_dir = ""
    iso_path = ""
    output_iso = ""


_fallback = _Defaults()


def get(context=None):
    context = context or bpy.context
    addon = context.preferences.addons.get(PACKAGE)
    if addon is not None and addon.preferences is not None:
        return addon.preferences
    return _fallback


def absolute(path):
    return os.path.abspath(bpy.path.abspath(path)) if path else ""


def _own_repo():
    """The repository around the add-on when it is linked from src/blender/c2_maps."""
    here = os.path.dirname(os.path.realpath(__file__))
    root = os.path.dirname(os.path.dirname(os.path.dirname(here)))
    if os.path.isfile(os.path.join(root, "c2.bat")) and os.path.isfile(os.path.join(root, "Cargo.toml")):
        return root
    return ""


def repo(prefs):
    return absolute(prefs.repo_dir) if prefs.repo_dir else _own_repo()


def c2_command(prefs):
    """The command that runs c2, as a list, or None."""
    if prefs.c2_path:
        path = absolute(prefs.c2_path)
        return [path] if os.path.exists(path) else None
    root = repo(prefs)
    if not root:
        return None
    windows = sys.platform.startswith("win")
    candidates = [os.path.join(root, "build", "release", "c2.exe" if windows else "c2")]
    candidates.append(os.path.join(root, "c2.bat" if windows else "c2"))
    for candidate in candidates:
        if os.path.isfile(candidate):
            return [candidate]
    return None


def maps_dir(prefs):
    if prefs.maps_dir:
        return absolute(prefs.maps_dir)
    root = repo(prefs)
    return os.path.join(root, "extracted", "maps") if root else ""


def source_dir(prefs):
    if prefs.source_dir:
        return absolute(prefs.source_dir)
    root = repo(prefs)
    return os.path.join(root, "extracted", "scripts") if root else ""


def working_dir(prefs):
    root = repo(prefs)
    if root and os.path.isdir(root):
        return root
    command = c2_command(prefs)
    return os.path.dirname(command[0]) if command else os.getcwd()


classes = (C2Preferences,)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
