"""Checks a map against what the game and c2's importer accept (docs/assets/maps.md 4.3, 7, 8)."""

import bpy

from . import c2data, schema

ERROR = "ERROR"
WARNING = "WARNING"

# The last results, for the panel: [(level, object name, message)].
results = []


def _len_check(found, obj, what, text):
    size = schema.name_bytes(text)
    if size > schema.NAME_LIMIT:
        found.append((ERROR, obj.name, "%s \"%s\" is %d bytes (31 at most)" % (what, text, size)))
    if not text:
        found.append((ERROR, obj.name, "%s is empty" % what))


def check(root):
    found = []
    name = c2data.map_name(root)
    if c2data.role(root) != "map":
        found.append((WARNING, root.name, "The map root has no extras.c2 with role \"map\""))
    _len_check(found, root, "Map name", name)

    placements = []
    for obj in c2data.descendants(root):
        if c2data.is_stringified(obj):
            found.append((WARNING, obj.name, "extras.c2 is kept as text (Blender could not store it); not editable"))
            continue
        data = c2data.get(obj)
        if data is None:
            # MDS nodes and anything added by hand: the exporter cannot tell what they are.
            found.append((WARNING, obj.name, "No extras.c2: the importer cannot tell what this is"))
            continue
        role = data.get("role")
        if role not in schema.ROLES:
            found.append((ERROR, obj.name, "Unknown role %r" % (role,)))
            continue
        prefix = schema.PREFIX.get(role)
        if prefix and not obj.name.startswith(prefix):
            found.append((ERROR, obj.name, "A %s's name must start with \"%s\"" % (role, prefix)))
        if role == "placement":
            placements.append(obj)
            _len_check(found, obj, "Placement name", schema.game_name(obj.name, role))
            _len_check(found, obj, "Template name", str(data.get("template", "")))
        elif role == "piece":
            _len_check(found, obj, "Piece name", schema.game_name(obj.name, role))
            if data.get("kind") not in schema.PIECE_KINDS:
                found.append((WARNING, obj.name, "Unknown piece kind %r" % (data.get("kind"),)))
        elif role == "point":
            check_point(found, obj, data)
        elif role == "mds" and "mds_index" not in data:
            piece = c2data.piece_of(obj)
            # a part added to a model of the disc's (a model made here has none with an index)
            if piece is not None and any("mds_index" in (c2data.get(o) or {}) for o in c2data.descendants(piece)):
                found.append((WARNING, obj.name, "An MDS node without mds_index (a new node)"))

    # Placement names: unique, and the slot limit.
    seen = {}
    for obj in placements:
        seen.setdefault(schema.game_name(obj.name, "placement"), []).append(obj)
    for game, objects in seen.items():
        if len(objects) > 1:
            new = [obj for obj in objects if c2data.lookup(obj, ("slot",)) is None]
            level = ERROR if new else WARNING
            found.append(
                (level, objects[-1].name, "Placement name \"%s\" is used %d times" % (game, len(objects)))
            )
    limit = schema.PLACEMENT_LIMIT_DUNGEON if name.startswith("d") else schema.PLACEMENT_LIMIT
    if len(placements) > limit:
        found.append((ERROR, root.name, "%d placements (%d at most)" % (len(placements), limit)))
    elif len(placements) > schema.PLACEMENT_LIMIT:
        found.append((WARNING, root.name, "%d placements: over 320, only dungeons have 400" % len(placements)))

    # Map settings.
    tree = c2data.to_py(c2data.lookup(root, ("map",))) or {}
    if isinstance(tree, dict):
        for key, value in tree.items():
            if str(key).lower() == "time_light_num":
                number = value[0] if isinstance(value, list) and value else value
                if isinstance(number, (int, float)) and number > schema.TIME_LIGHT_NUM_LIMIT:
                    found.append((ERROR, root.name, "TIME_LIGHT_NUM is %s (8 at most)" % number))

    for obj in c2data.collision_meshes(root):
        check_collision(found, obj)
    check_pictures(found, root)
    found.extend(left_out(root))
    return found


def left_out(root):
    """What the export would leave out without a word, as warnings: meshes outside every map
    (modelled but never made a model), and the map in the scene twice (only `root` is exported)."""
    found = []
    scene = bpy.context.scene
    name = c2data.map_name(root)
    twins = [other for other in c2data.scene_map_roots(scene) if other != root and c2data.map_name(other) == name]
    if twins:
        found.append((WARNING, twins[0].name, "The scene holds map %s %d times; only %s is exported" % (name, len(twins) + 1, root.name)))
    for obj in scene.objects:
        if obj.type == "MESH" and c2data.map_root(obj) is None:
            found.append(
                (WARNING, obj.name, "Not part of any map, so not exported: select it and press New Model from Selection")
            )
    return found


def picture_name(image):
    """The game picture an image stands for, as the export names it: without Blender's .001 and
    without a file suffix."""
    name = schema.strip_suffix(image.name)
    stem, dot, suffix = name.rpartition(".")
    return stem if dot and suffix.lower() in ("png", "jpg", "jpeg", "tga", "bmp") else name


def is_exported_picture(image):
    """An image loaded from the textures/ folder of an exported map (a picture of the disc)."""
    path = bpy.path.abspath(image.filepath or "").replace("\\", "/").lower()
    return "/textures/" in path and path.endswith(".png") and not image.is_dirty


def check_pictures(found, root):
    """The pictures the map's materials show: names the game can hold, and new pictures (ones
    not exported from the disc) of a size a texture file takes."""
    seen = set()
    for obj in c2data.descendants(root):
        if obj.type != "MESH" or c2data.is_collision_mesh(obj):
            continue
        for slot in obj.material_slots:
            material = slot.material
            if material is None or material.node_tree is None:
                continue
            for node in material.node_tree.nodes:
                image = getattr(node, "image", None) if node.type == "TEX_IMAGE" else None
                if image is None or image.name in seen:
                    continue
                seen.add(image.name)
                name = picture_name(image)
                latin1 = all(ord(c) < 0x100 for c in name)
                if not name or not latin1 or len(name) > schema.NAME_LIMIT:
                    found.append((ERROR, obj.name, "Picture \"%s\": a picture's name is 1 to 31 Latin-1 characters" % name))
                if is_exported_picture(image):
                    continue
                w, h = image.size[:]
                power = lambda n: n > 0 and n & (n - 1) == 0
                if not (power(w) and power(h)) or max(w, h) > 1024:
                    found.append(
                        (ERROR, obj.name, "Picture \"%s\" is %dx%d: sides must be powers of two up to 1024" % (name, w, h))
                    )


def check_point(found, obj, data):
    _len_check(found, obj, "Point name", schema.game_name(obj.name, "point"))
    kind = data.get("kind")
    if kind not in schema.POINT_KINDS:
        found.append((ERROR, obj.name, "Unknown point kind %r" % (kind,)))
        return
    point_data = data.get("data")
    if schema.DATA_FIELDS[kind] and point_data is None:
        found.append((WARNING, obj.name, "A %s point without data" % kind))
    elif isinstance(point_data, dict):
        point_data = schema.upgrade_data(kind, point_data)
        missing = [f.key for f in schema.DATA_FIELDS[kind] if not f.optional and f.key not in point_data]
        if missing:
            found.append((WARNING, obj.name, "Missing %s data: %s" % (kind, ", ".join(missing))))
    if kind == "anime":
        target, problem = c2data.anime_target(obj)
        if target is None:
            found.append((WARNING, obj.name, "The animation moves nothing: %s" % problem))
    if kind == "event":
        target = c2data.data_field(obj, schema.DATA_FIELDS["event"][4])
        if isinstance(target, str) and schema.name_bytes(target) > schema.TARGET_LIMIT:
            found.append((ERROR, obj.name, "Event target \"%s\" is over 15 bytes" % target))
    flag = data.get("flag")
    if isinstance(flag, list) and len(flag) >= 4:
        for hour in flag[2:4]:
            if not 0 <= hour <= 24:
                found.append((WARNING, obj.name, "Hour %s is outside 0-24" % hour))


def check_collision(found, obj):
    mesh = obj.data
    bad = sum(1 for polygon in mesh.polygons if len(polygon.vertices) != 3)
    if bad:
        found.append((ERROR, obj.name, "%d collision faces are not triangles" % bad))
    used = {polygon.material_index for polygon in mesh.polygons}
    slots = obj.material_slots
    for index in sorted(used):
        material = slots[index].material if index < len(slots) else None
        if material is None:
            found.append((ERROR, obj.name, "Collision faces without a material (slot %d)" % index))
        elif schema.parse_col(material.name) is None:
            found.append((ERROR, obj.name, "Material \"%s\" is not a col_groundG_footF_areaA_maskM name" % material.name))
        else:
            attrs = schema.parse_col(material.name)
            if attrs[3] > 255:
                found.append((ERROR, obj.name, "Material \"%s\": the mask is 0-255" % material.name))
    if not mesh.polygons:
        found.append((WARNING, obj.name, "A collision mesh with no faces"))


class C2_OT_validate(bpy.types.Operator):
    """Check the map: name lengths, unique placements, the placement limit, point kinds, collision
    triangles and materials, nodes without extras.c2"""

    bl_idname = "c2.validate"
    bl_label = "Validate Map"
    bl_options = {"REGISTER"}

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None

    def execute(self, context):
        root = c2data.current_map_root(context)
        results[:] = check(root)
        text = bpy.data.texts.get("c2 validation") or bpy.data.texts.new("c2 validation")
        text.clear()
        for level, name, message in results:
            text.write("%s\t%s\t%s\n" % (level, name, message))
        errors = sum(1 for r in results if r[0] == ERROR)
        warnings = len(results) - errors
        if not results:
            text.write("No problems.\n")
        level = {"ERROR"} if errors else ({"WARNING"} if warnings else {"INFO"})
        self.report(level, "%s: %d errors, %d warnings (text \"c2 validation\")" % (root.name, errors, warnings))
        return {"FINISHED"}


class C2_OT_select_problem(bpy.types.Operator):
    """Select the object this message is about"""

    bl_idname = "c2.select_problem"
    bl_label = "Select"
    bl_options = {"INTERNAL"}

    object_name: bpy.props.StringProperty()

    def execute(self, context):
        obj = bpy.data.objects.get(self.object_name)
        if obj is None or obj.name not in context.view_layer.objects:
            return {"CANCELLED"}
        for other in context.selected_objects:
            other.select_set(False)
        try:
            obj.select_set(True)
        except RuntimeError:
            pass
        context.view_layer.objects.active = obj
        return {"FINISHED"}


classes = (C2_OT_validate, C2_OT_select_problem)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
