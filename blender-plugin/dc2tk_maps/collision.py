"""Collision tools: a collision copy made from what is drawn (simplified, so the floor queries
stay cheap), and a collision node split into compact ones, since the engine looks through every
node whose box a query touches (docs/game/maps.md section 4.3)."""

import bmesh
import bpy
from bpy.props import BoolProperty, FloatProperty, IntProperty

from . import c2data, edit, import_map, schema


def triangulated(mesh, simplify_angle=0.0):
    """mesh triangulated in place, its flat areas first merged when an angle is given."""
    shape = bmesh.new()
    try:
        shape.from_mesh(mesh)
        if simplify_angle > 0.0:
            bmesh.ops.dissolve_limit(
                shape, angle_limit=simplify_angle, use_dissolve_boundaries=False,
                verts=shape.verts[:], edges=shape.edges[:],
            )
        bmesh.ops.triangulate(shape, faces=shape.faces[:])
        shape.to_mesh(mesh)
    finally:
        shape.free()


class DC2TK_OT_collision_from_mesh(bpy.types.Operator):
    """Give the selected drawn meshes' placement floor and wall collision made from them:
    triangulated copies (simplified where flat), area 6, in a new collision piece of a new model,
    replacing the copies this made before"""

    bl_idname = "dc2tk.collision_from_mesh"
    bl_label = "Collision from Mesh"
    bl_options = {"REGISTER", "UNDO"}

    simplify: FloatProperty(
        name="Merge flat areas", description="Merge faces within this angle of each other first (0 keeps every face)",
        subtype="ANGLE", default=0.0872665, min=0.0, max=0.5,
    )
    camera: BoolProperty(name="Camera collision too", default=False)

    @classmethod
    def poll(cls, context):
        return any(obj.type == "MESH" and c2data.piece_of(obj) is not None and not c2data.is_collision_mesh(obj)
                   for obj in context.selected_objects)

    def execute(self, context):
        meshes = [obj for obj in context.selected_objects
                  if obj.type == "MESH" and c2data.piece_of(obj) is not None and not c2data.is_collision_mesh(obj)]
        root = c2data.map_root(meshes[0])
        placement = c2data.placement_of(meshes[0])
        meshes = [obj for obj in meshes if c2data.placement_of(obj) is placement]
        base = c2data.exact_name(c2data.piece_of(meshes[0]), "piece")
        base = base[:-2] if base.endswith("-m") else base
        mode = context.window_manager.dc2tk_maps.collision_colour
        made = 0
        for kind, letter, wanted in (("collision", "a", True), ("camera", "c", self.camera)):
            if not wanted:
                continue
            name = "%s-%s" % (base, letter)
            old = next((child for child in placement.children if c2data.role(child) == "piece"
                        and c2data.exact_name(child, "piece") == name), None)
            if old is not None and any("mds_index" in (c2data.get(o) or {}) for o in c2data.descendants(old)):
                self.report({"WARNING"}, "%s is the disc's own collision; edit its meshes instead" % name)
                continue
            if old is not None:
                bpy.data.batch_remove(c2data.descendants(old))
            order = sum(1 for child in placement.children if c2data.role(child) == "piece")
            piece = edit.create_model_piece(context, placement, base, kind, letter, order)
            edit.create_collision_meshes(context, root, piece, meshes, kind, letter, mode)
            for obj in c2data.descendants(piece):
                if obj.type == "MESH":
                    triangulated(obj.data, self.simplify)
                    made += len(obj.data.polygons)
        import_map.colour_collision(root, mode)
        self.report({"INFO"}, "Collision of %d triangles for %s" % (made, placement.name))
        return {"FINISHED"}


class DC2TK_OT_split_collision(bpy.types.Operator):
    """Split the active collision mesh into compact nodes, one per square of the grid, so each
    floor query looks through fewer triangles"""

    bl_idname = "dc2tk.split_collision"
    bl_label = "Split Collision"
    bl_options = {"REGISTER", "UNDO"}

    cell: FloatProperty(name="Square", description="The size of each node's square, in game units", default=320.0, min=10.0)
    most: IntProperty(name="At most", description="Split only meshes with more triangles than this", default=schema.QUERY_TRIANGLES, min=1)

    @classmethod
    def poll(cls, context):
        obj = context.active_object
        return obj is not None and obj.type == "MESH" and c2data.is_collision_mesh(obj)

    def execute(self, context):
        obj = context.active_object
        if obj.mode != "OBJECT":
            bpy.ops.object.mode_set(mode="OBJECT")
        mesh = obj.data
        if len(mesh.polygons) <= self.most:
            self.report({"INFO"}, "%s has %d triangles; nothing to split" % (obj.name, len(mesh.polygons)))
            return {"CANCELLED"}
        world = c2data.world_matrix(obj)
        cells = {}
        for polygon in mesh.polygons:
            centre = world @ polygon.center
            key = (int(centre.x // self.cell), int(centre.y // self.cell))
            cells.setdefault(key, []).append(polygon.index)
        if len(cells) < 2:
            self.report({"INFO"}, "%s fits one square of %g" % (obj.name, self.cell))
            return {"CANCELLED"}
        keys = sorted(cells)
        base = schema.strip_suffix(obj.name)
        # the mesh is the model's: every piece placing the model gets the new nodes too, since
        # the build takes a model from the first piece placing it
        sharing = [other for other in bpy.data.objects if other is not obj and other.data is mesh]
        made = []
        for number, key in enumerate(keys):
            target = obj if number == 0 else obj.copy()
            if number:
                target.data = mesh.copy()
                target.name = "%s_%d" % (base, number)
                for collection in obj.users_collection:
                    collection.objects.link(target)
                target.parent = obj.parent
                target.matrix_parent_inverse = obj.matrix_parent_inverse.copy()
                target.matrix_basis = obj.matrix_basis.copy()
                c2data.make_new_node(target)
                for other in sharing:
                    twin = target.copy()
                    for collection in other.users_collection:
                        collection.objects.link(twin)
                    twin.parent = other.parent
                    twin.matrix_parent_inverse = other.matrix_parent_inverse.copy()
                    twin.matrix_basis = other.matrix_basis.copy()
            made.append((target, set(cells[key])))
        for target, keep in made:
            shape = bmesh.new()
            try:
                shape.from_mesh(target.data)
                shape.faces.ensure_lookup_table()
                gone = [face for face in shape.faces if face.index not in keep]
                bmesh.ops.delete(shape, geom=gone, context="FACES")
                loose = [vert for vert in shape.verts if not vert.link_faces]
                bmesh.ops.delete(shape, geom=loose, context="VERTS")
                shape.to_mesh(target.data)
            finally:
                shape.free()
        self.report({"INFO"}, "%s split into %d nodes" % (base, len(made)))
        return {"FINISHED"}


classes = (DC2TK_OT_collision_from_mesh, DC2TK_OT_split_collision)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
