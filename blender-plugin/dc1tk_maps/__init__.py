bl_info = {
    "name": "Dark Cloud 1 Maps (dc1tk)",
    "author": "DC2-Toolkit",
    "version": (0, 1, 0),
    "blender": (4, 2, 0),
    "location": "File > Import > Dark Cloud 1; View3D > Sidebar > Dark Cloud 1",
    "description": "Import Dark Cloud 1 floors, Georama scenes and layouts exported by dc1tk, and export edits back",
    "category": "Import-Export",
}

if "bpy" in locals():
    import importlib

    importlib.reload(gltf_io)
    importlib.reload(maps)

import os
import subprocess
import sys

import bpy
from bpy.props import StringProperty
from bpy_extras.io_utils import ImportHelper

from . import gltf_io, maps

PACKAGE = __package__


class Dc1Preferences(bpy.types.AddonPreferences):
    bl_idname = PACKAGE

    dc2tk_path: StringProperty(name="dc2tk executable", subtype="FILE_PATH",
                               description="dc1tk's dc2tk.exe (build/release/dc2tk.exe); empty to look beside the add-on")
    data_dir: StringProperty(name="Game data", subtype="DIR_PATH",
                             description="The Chronicle data folder (C:/Users/<you>/AppData/Local/chronicle/data)")
    build_dir: StringProperty(name="Build output", subtype="DIR_PATH",
                              description="Where Build writes the .cfg/.mpd, .scn or .edt")

    def draw(self, context):
        for name in ("dc2tk_path", "data_dir", "build_dir"):
            self.layout.prop(self, name)
        self.layout.label(text="dc2tk: %s" % (dc2tk(self) or "not found"))


def prefs(context):
    return context.preferences.addons[PACKAGE].preferences


def dc2tk(p):
    if p.dc2tk_path:
        path = bpy.path.abspath(p.dc2tk_path)
        return path if os.path.isfile(path) else None
    repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.realpath(__file__))))
    path = os.path.join(repo, "build", "release", "dc2tk.exe" if sys.platform.startswith("win") else "dc2tk")
    return path if os.path.isfile(path) else None


def active_root(context):
    return gltf_io.top_root(context.active_object) if context.active_object else None


def map_root(obj):
    while obj is not None and "dc1_kind" not in obj:
        obj = obj.parent
    return obj


class DC1_OT_import_floor(bpy.types.Operator, ImportHelper):
    """Import a dungeon floor folder from dc2tk maps-export (floors/<floor>, holding floor.json)"""
    bl_idname = "dc1.import_floor"
    bl_label = "Import DC1 Floor"
    bl_options = {"REGISTER", "UNDO"}
    filename_ext = ".json"
    filter_glob: StringProperty(default="floor.json", options={"HIDDEN"})

    def execute(self, context):
        folder = self.filepath if os.path.isdir(self.filepath) else os.path.dirname(self.filepath)
        if not os.path.isfile(os.path.join(folder, "floor.json")):
            self.report({"ERROR"}, "no floor.json in %s" % folder)
            return {"CANCELLED"}
        maps.import_floor(context, folder)
        return {"FINISHED"}


class DC1_OT_import_scene(bpy.types.Operator, ImportHelper):
    """Import a Georama scene folder from dc2tk maps-export (scenes/.../<name>.scn, holding scene.json)"""
    bl_idname = "dc1.import_scene"
    bl_label = "Import DC1 Georama Scene"
    bl_options = {"REGISTER", "UNDO"}
    filename_ext = ".json"
    filter_glob: StringProperty(default="scene.json", options={"HIDDEN"})

    def execute(self, context):
        folder = self.filepath if os.path.isdir(self.filepath) else os.path.dirname(self.filepath)
        if not os.path.isfile(os.path.join(folder, "scene.json")):
            self.report({"ERROR"}, "no scene.json in %s" % folder)
            return {"CANCELLED"}
        maps.import_scene(context, folder)
        return {"FINISHED"}


class DC1_OT_import_layout(bpy.types.Operator, ImportHelper):
    """Import a Georama town layout (layouts/.../gdata0.edt.json) as placed empties"""
    bl_idname = "dc1.import_layout"
    bl_label = "Import DC1 Georama Layout"
    bl_options = {"REGISTER", "UNDO"}
    filename_ext = ".json"
    filter_glob: StringProperty(default="*.edt.json", options={"HIDDEN"})

    def execute(self, context):
        maps.import_layout(context, self.filepath)
        return {"FINISHED"}


def export_root(operator, root, target):
    counts = maps.EXPORTERS[root["dc1_kind"]](root, target, operator.report)
    root["dc1_exported"] = target
    operator.report({"INFO"}, "%s exported to %s: %s" % (root.name, target,
                                                          ", ".join("%s %s" % (v, k) for k, v in counts.items())))
    return counts


class DC1_OT_export(bpy.types.Operator):
    """Write the active map's edits back into its exported folder (or another folder), the way dc1tk builds them"""
    bl_idname = "dc1.export"
    bl_label = "Export DC1 Map"
    directory: StringProperty(name="Folder", subtype="DIR_PATH",
                              description="Empty to write into the imported folder itself")
    filepath: StringProperty(name="File", subtype="FILE_PATH", description="For a layout: the .edt.json to write")

    @classmethod
    def poll(cls, context):
        return map_root(context.active_object) is not None

    def execute(self, context):
        root = map_root(context.active_object)
        source = root["dc1_source"]
        target = (self.filepath or source) if root["dc1_kind"] == "layout" else (self.directory or source)
        try:
            export_root(self, root, bpy.path.abspath(target).rstrip("\\/") if root["dc1_kind"] != "layout"
                        else bpy.path.abspath(target))
        except (OSError, ValueError) as error:
            self.report({"ERROR"}, str(error))
            return {"CANCELLED"}
        return {"FINISHED"}


def data_path_of(source, marker, strip=""):
    parts = os.path.normpath(source).replace("\\", "/").split("/")
    if marker not in parts:
        return None
    rel = "/".join(parts[len(parts) - parts[::-1].index(marker):])
    return rel[:-len(strip)] if strip and rel.endswith(strip) else rel


class DC1_OT_build(bpy.types.Operator):
    """Run dc2tk floor-build / scene-build / layout-build on the exported folder (prints the MoonMod size check)"""
    bl_idname = "dc1.build"
    bl_label = "Build with dc2tk"

    @classmethod
    def poll(cls, context):
        return map_root(context.active_object) is not None

    def execute(self, context):
        p = prefs(context)
        exe, data, out = dc2tk(p), bpy.path.abspath(p.data_dir), bpy.path.abspath(p.build_dir)
        if not exe or not out:
            self.report({"ERROR"}, "set dc2tk executable and Build output in the add-on's preferences")
            return {"CANCELLED"}
        root = map_root(context.active_object)
        kind, folder = root["dc1_kind"], root.get("dc1_exported", root["dc1_source"])
        os.makedirs(out, exist_ok=True)
        if kind == "floor":
            command = [exe, "floor-build", folder, data, os.path.basename(folder), "-o", out]
        elif kind == "scene":
            rel = data_path_of(root["dc1_source"], "scenes")
            command = [exe, "scene-build", folder, os.path.join(data, rel), "-o", os.path.join(out, os.path.basename(rel))]
        else:
            rel = data_path_of(root["dc1_source"], "layouts", ".json")
            command = [exe, "layout-build", folder, "-o", os.path.join(out, os.path.basename(rel or "gdata0.edt"))]
            if rel and data:
                command += ["--original", os.path.join(data, rel)]
        if kind != "layout" and not data:
            self.report({"ERROR"}, "set Game data in the add-on's preferences")
            return {"CANCELLED"}
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0) if sys.platform.startswith("win") else 0
        done = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, creationflags=flags)
        text = done.stdout.decode("utf-8", errors="replace")
        block = bpy.data.texts.get("dc2tk output") or bpy.data.texts.new("dc2tk output")
        block.from_string(" ".join(command) + "\n" + text)
        for line in text.splitlines()[-6:]:
            self.report({"WARNING"} if "larger" in line.lower() else {"INFO"}, line)
        if done.returncode:
            self.report({"ERROR"}, "dc2tk failed (exit %d); see the text 'dc2tk output'" % done.returncode)
            return {"CANCELLED"}
        return {"FINISHED"}


class DC1_PT_panel(bpy.types.Panel):
    bl_label = "Dark Cloud 1"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Dark Cloud 1"

    def draw(self, context):
        layout = self.layout
        col = layout.column(align=True)
        col.operator("dc1.import_floor", icon="IMPORT")
        col.operator("dc1.import_scene", icon="IMPORT")
        col.operator("dc1.import_layout", icon="IMPORT")
        root = map_root(context.active_object)
        if root is None:
            layout.label(text="Select an object of an imported map")
            return
        box = layout.box()
        box.label(text="%s (%s)" % (root.name, root["dc1_kind"]))
        box.label(text="from " + root["dc1_source"])
        box.operator("dc1.export", icon="EXPORT")
        box.operator("dc1.build", icon="CONSOLE")
        obj = context.active_object
        if "dc1_cells" in obj:
            box.label(text="Cells (one hex byte per cell, a row each):")
            box.prop(obj, '["dc1_cells"]', text="")
        if "dc1_layout_part" in obj:
            box.prop(obj, '["dc1_layout_part"]', text="Part")
            box.label(text="Turn about Y in quarter turns")
        if obj.type == "MESH" and obj.get("dc1_shared"):
            box.label(text="Shares its mesh with an earlier part; that one is exported", icon="INFO")
        box.label(text="Moving vertices keeps the file size;", icon="INFO")
        box.label(text="adding/removing faces rebuilds (may be refused)")


def menu_import(self, context):
    self.layout.operator("dc1.import_floor", text="Dark Cloud 1 floor (floor.json)")
    self.layout.operator("dc1.import_scene", text="Dark Cloud 1 Georama scene (scene.json)")
    self.layout.operator("dc1.import_layout", text="Dark Cloud 1 Georama layout (.edt.json)")


classes = (Dc1Preferences, DC1_OT_import_floor, DC1_OT_import_scene, DC1_OT_import_layout, DC1_OT_export,
           DC1_OT_build, DC1_PT_panel)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.TOPBAR_MT_file_import.append(menu_import)


def unregister():
    bpy.types.TOPBAR_MT_file_import.remove(menu_import)
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
