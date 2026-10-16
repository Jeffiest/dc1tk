import bpy

from . import c2data, schema

ERROR = "ERROR"
WARNING = "WARNING"
NOTE = "INFO"


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
    for obj in c2data.map_objects(root):
        if c2data.is_stringified(obj):
            found.append((WARNING, obj.name, "extras.c2 is kept as text (Blender could not store it); not editable"))
            continue
        data = c2data.get(obj)
        if data is None:
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
            _len_check(found, obj, "Placement name", c2data.exact_name(obj, role))
            _len_check(found, obj, "Template name", str(data.get("template", "")))
        elif role == "piece":
            _len_check(found, obj, "Piece name", c2data.exact_name(obj, role))
            if data.get("kind") not in schema.PIECE_KINDS:
                found.append((WARNING, obj.name, "Unknown piece kind %r" % (data.get("kind"),)))
        elif role == "point":
            check_point(found, obj, data)
        elif role == "mds" and "mds_index" not in data:
            piece = c2data.piece_of(obj)

            if piece is not None and any("mds_index" in (c2data.get(o) or {}) for o in c2data.descendants(piece)):
                found.append((WARNING, obj.name, "An MDS node without mds_index (a new node)"))

    seen = {}
    for obj in placements:
        seen.setdefault(c2data.exact_name(obj, "placement"), []).append(obj)
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

    tree = c2data.map_settings(root)
    if isinstance(tree, dict):
        for key, value in tree.items():
            if str(key).lower() == "time_light_num":
                number = value[0] if isinstance(value, list) and value else value
                if isinstance(number, (int, float)) and number > schema.TIME_LIGHT_NUM_LIMIT:
                    found.append((ERROR, root.name, "TIME_LIGHT_NUM is %s (8 at most)" % number))

    for obj in c2data.collision_meshes(root):
        check_collision(found, obj)
    check_pictures(found, root)
    check_templates(found, placements)
    check_script_names(found, root)
    found.extend(left_out(root))
    return found


def check_templates(found, placements):
    """Templates whose placements differ: the build splits them, which is right when meant."""
    seen = set()
    for obj in placements:
        template = c2data.template_name(obj)
        if c2data.is_preview(obj) or template in seen:
            continue
        seen.add(template)
        groups = c2data.instance_groups(obj)
        if len(groups) > 1:
            found.append((
                NOTE, groups[1][0].name,
                "Template %s: its %d placements differ, so %d get templates of their own (Apply to All to keep one)"
                % (template, sum(len(g) for g in groups), sum(len(g) for g in groups[1:])),
            ))


def check_script_names(found, root):
    """Names the map's scripts reach that the scene no longer has (renamed or deleted)."""
    from . import scripts

    report = scripts.info(root)
    if not report:
        return
    places = {c2data.exact_name(obj, "placement"): obj for obj in c2data.placements(root)}
    slots = {c2data.lookup(obj, ("slot",)) for obj in places.values()}
    own_points = {c2data.exact_name(obj, "point") for obj in c2data.points(root) if c2data.placement_of(obj) is None}
    reported = set()
    for use in report.get("names") or []:
        what, part, name = use.get("what"), use.get("part"), use.get("name")
        where = "%s:%s" % (str(use.get("path", "?")).rsplit("/", 1)[-1], use.get("line", "?"))
        problem = None
        if isinstance(part, int):
            if what == "placement" and part not in slots:
                problem = "placement slot %d" % part
        elif isinstance(part, str) and part and part not in places and what in ("placement", "piece", "point"):
            problem = "placement %s" % part
        elif what == "point" and part == "" and name not in own_points:
            problem = "the map's point %s" % name
        elif what == "point" and isinstance(part, str) and part in places and name:
            names = {c2data.exact_name(o, "point") for o in places[part].children if c2data.role(o) == "point"}
            if name not in names:
                problem = "point %s of %s" % (name, part)
        elif what == "piece" and isinstance(part, str) and part in places and name:
            names = {c2data.exact_name(o, "piece") for o in places[part].children if c2data.role(o) == "piece"}
            if name not in names:
                problem = "piece %s of %s" % (name, part)
        if problem and problem not in reported:
            reported.add(problem)
            found.append((WARNING, root.name, "Scripts name %s, which the scene no longer has (%s)" % (problem, where)))


# collections of what the add-on shows beside a map and never exports: the player for scale, a
# dungeon layout's preview, a library's templates
DISPLAY_PREFIX = "c2 "


def left_out(root):
    found = []
    scene = bpy.context.scene
    name = c2data.map_name(root)
    twins = [other for other in c2data.scene_map_roots(scene) if other != root and c2data.map_name(other) == name]
    if twins:
        found.append((WARNING, twins[0].name, "The scene holds map %s %d times; only %s is exported" % (name, len(twins) + 1, root.name)))
    for obj in scene.objects:
        if any(collection.name.startswith(DISPLAY_PREFIX) for collection in obj.users_collection):
            continue
        if obj.type == "MESH" and c2data.map_root(obj) is None:
            found.append(
                (WARNING, obj.name, "Not part of any map, so not exported: select it and press New Model from Selection")
            )
    return found


def picture_name(image):
    name = schema.strip_suffix(image.name)
    stem, dot, suffix = name.rpartition(".")
    return stem if dot and suffix.lower() in ("png", "jpg", "jpeg", "tga", "bmp") else name


def is_exported_picture(image):
    path = bpy.path.abspath(image.filepath or "").replace("\\", "/").lower()
    return "/textures/" in path and path.endswith(".png") and not image.is_dirty


def check_pictures(found, root):
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
                elif w < 16:
                    found.append(
                        (WARNING, obj.name, "Picture \"%s\" is %d wide: most maps take new pictures 16 wide or more" % (name, w))
                    )


def check_point(found, obj, data):
    _len_check(found, obj, "Point name", c2data.exact_name(obj, "point"))
    kind = data.get("kind")
    if kind not in schema.POINT_KINDS:
        found.append((ERROR, obj.name, "Unknown point kind %r" % (kind,)))
        return
    point_data = data.get("data")
    if schema.DATA_FIELDS[kind] and point_data is None:
        found.append((WARNING, obj.name, "A %s point without data" % kind))
    elif isinstance(point_data, dict):
        point_data = schema.upgrade_data(kind, point_data)
        missing = [f["key"] for f in schema.DATA_FIELDS[kind] if not f["optional"] and f["key"] not in point_data]
        if missing:
            found.append((WARNING, obj.name, "Missing %s data: %s" % (kind, ", ".join(missing))))
    if kind == "anime":
        target, problem = c2data.anime_target(obj)
        if target is None:
            found.append((WARNING, obj.name, "The animation moves nothing: %s" % problem))
    if kind == "invent" and c2data.placement_of(obj) is None:
        found.append(
            (WARNING, obj.name, "An invent box among the map's own points is never photographed: put it in a placement")
        )
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
    # the engine boxes each node and queries the triangles of every node a box overlaps, up to
    # the caller's buffer (128 triangles for monsters and objects): keep new nodes compact
    if "mds_index" not in (c2data.get(obj) or {}) and len(mesh.polygons) > NEW_NODE_TRIANGLES:
        found.append((
            WARNING, obj.name,
            "A new collision node of %d triangles: split it (Split Collision) so queries near it stay under %d"
            % (len(mesh.polygons), schema.QUERY_TRIANGLES),
        ))


# 99% of the disc's floor collision nodes hold fewer than about 400 triangles
NEW_NODE_TRIANGLES = 512


class DC2TK_OT_validate(bpy.types.Operator):
    """Check the map: name lengths, unique placements, the placement limit, point kinds, collision
    triangles and materials, nodes without extras.c2"""

    bl_idname = "dc2tk.validate"
    bl_label = "Validate Map"
    bl_options = {"REGISTER"}

    @classmethod
    def poll(cls, context):
        return c2data.current_map_root(context) is not None

    def execute(self, context):
        root = c2data.current_map_root(context)
        results[:] = check(root)
        text = bpy.data.texts.get("dc2tk validation") or bpy.data.texts.new("dc2tk validation")
        text.clear()
        for level, name, message in results:
            text.write("%s\t%s\t%s\n" % (level, name, message))
        errors = sum(1 for r in results if r[0] == ERROR)
        warnings = len(results) - errors
        if not results:
            text.write("No problems.\n")
        level = {"ERROR"} if errors else ({"WARNING"} if warnings else {"INFO"})
        self.report(level, "%s: %d errors, %d warnings (text \"dc2tk validation\")" % (root.name, errors, warnings))
        return {"FINISHED"}


class DC2TK_OT_select_problem(bpy.types.Operator):
    """Select the object this message is about"""

    bl_idname = "dc2tk.select_problem"
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


classes = (DC2TK_OT_validate, DC2TK_OT_select_problem)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
