"""Materials shown as the game draws them (Material Preview and Rendered): the picture times the
vertex colour, alpha from both (the fades painted into meshes), additive and subtractive glows,
unlit meshes and alpha tests, from the draw flags of the model's frame names (the material's
`frame_flags`, docs/assets/models.md 1.5).

Blender's glTF importer builds each material around a Principled BSDF, which is also what its
exporter reads the picture from. The game look is a shader of its own beside it, linked to the
output; `for_export` links the Principled BSDF back while a scene is written, so the written glTF
is the same either way."""

import contextlib

import bpy

from . import c2data

PREFIX = "c2 look"
SHADER = PREFIX + " shader"
FLAG = "c2_look"


def _output(tree):
    outputs = [node for node in tree.nodes if node.type == "OUTPUT_MATERIAL"]
    for node in outputs:
        if node.is_active_output:
            return node
    return outputs[0] if outputs else None


def _first(tree, kind):
    return next((node for node in tree.nodes if node.type == kind and not node.name.startswith(PREFIX)), None)


def _node(tree, kind, label, x, y):
    node = tree.nodes.new(kind)
    node.name = "%s %s" % (PREFIX, label)
    node.label = label
    node.location = (x, y)
    return node


def _math(tree, operation, a, b, label, x, y):
    node = _node(tree, "ShaderNodeMath", label, x, y)
    node.operation = operation
    for socket, value in zip(node.inputs, (a, b)):
        if isinstance(value, bpy.types.NodeSocket):
            tree.links.new(value, socket)
        else:
            socket.default_value = value
    return node.outputs[0]


def _multiply_colour(tree, a, b, label, x, y):
    node = _node(tree, "ShaderNodeMix", label, x, y)
    node.data_type = "RGBA"
    node.blend_type = "MULTIPLY"
    node.inputs["Factor"].default_value = 1.0
    sockets = [s for s in node.inputs if s.type == "RGBA"]
    tree.links.new(a, sockets[0])
    tree.links.new(b, sockets[1])
    return next(s for s in node.outputs if s.type == "RGBA")


def is_map_material(material):
    return (
        material is not None
        and material.node_tree is not None
        and not material.name.startswith("col_")
        and ("frame_flags" in material or "texture" in material or "mdt_diffuse" in material)
    )


def apply(material):
    """Give a map material the game look (again: an earlier one is replaced)."""
    tree = material.node_tree
    for node in [node for node in tree.nodes if node.name.startswith(PREFIX)]:
        tree.nodes.remove(node)
    output = _output(tree)
    principled = _first(tree, "BSDF_PRINCIPLED")
    if output is None or principled is None:
        return False
    flags = c2data.to_py(material.get("frame_flags")) or {}
    if not hasattr(flags, "get"):
        flags = {}
    diffuse = c2data.to_py(material.get("mdt_diffuse")) or [1.0, 1.0, 1.0, 1.0]
    picture = _first(tree, "TEX_IMAGE")
    vertex = _first(tree, "VERTEX_COLOR")
    x, y = output.location.x - 900, output.location.y - 420

    if picture is not None:
        colour, alpha = picture.outputs["Color"], picture.outputs["Alpha"]
    else:
        base = principled.inputs["Base Color"].default_value
        rgb = _node(tree, "ShaderNodeRGB", "colour", x, y)
        rgb.outputs[0].default_value = (base[0], base[1], base[2], 1.0)
        colour, alpha = rgb.outputs[0], 1.0
    if vertex is not None:
        colour = _multiply_colour(tree, colour, vertex.outputs["Color"], "vertex colour", x + 200, y)
        alpha = _math(tree, "MULTIPLY", alpha, vertex.outputs["Alpha"], "vertex alpha", x + 200, y - 200)
    if len(diffuse) > 3 and float(diffuse[3]) < 1.0:
        alpha = _math(tree, "MULTIPLY", alpha, float(diffuse[3]), "diffuse alpha", x + 400, y - 200)
    blend = flags.get("blend")
    alpha_ref = flags.get("alpha_ref")
    if alpha_ref and blend not in ("add", "subtract") and isinstance(alpha, bpy.types.NodeSocket):
        # the GS test drops texels under the reference (0x80 is opaque); the rest still blend
        kept = _math(tree, "GREATER_THAN", alpha, max(0.0, float(alpha_ref) - 0.5) / 128.0, "alpha test", x + 400, y - 400)
        alpha = _math(tree, "MULTIPLY", alpha, kept, "tested alpha", x + 550, y - 400)
    unlit = flags.get("lit") is False

    sx = x + 650
    if blend == "add":
        glow = _node(tree, "ShaderNodeEmission", "glow", sx, y)
        if isinstance(alpha, bpy.types.NodeSocket):
            colour = _multiply_colour(tree, colour, alpha, "glow by alpha", sx - 200, y + 200)
        tree.links.new(colour, glow.inputs["Color"])
        clear = _node(tree, "ShaderNodeBsdfTransparent", "behind", sx, y - 150)
        shader = _node(tree, "ShaderNodeAddShader", "add", sx + 220, y)
        tree.links.new(clear.outputs[0], shader.inputs[0])
        tree.links.new(glow.outputs[0], shader.inputs[1])
        method = "BLENDED"
    elif blend == "subtract":
        if isinstance(alpha, bpy.types.NodeSocket):
            colour = _multiply_colour(tree, colour, alpha, "darken by alpha", sx - 200, y + 200)
        invert = _node(tree, "ShaderNodeInvert", "subtract", sx, y)
        tree.links.new(colour, invert.inputs["Color"])
        shader = _node(tree, "ShaderNodeBsdfTransparent", "darken", sx + 220, y)
        tree.links.new(invert.outputs[0], shader.inputs["Color"])
        method = "BLENDED"
    else:
        if unlit:
            surface = _node(tree, "ShaderNodeEmission", "unlit", sx, y)
        else:
            surface = _node(tree, "ShaderNodeBsdfDiffuse", "lit", sx, y)
        tree.links.new(colour, surface.inputs["Color"])
        if blend == "opaque" or not isinstance(alpha, bpy.types.NodeSocket):
            shader = surface
        else:
            clear = _node(tree, "ShaderNodeBsdfTransparent", "behind", sx, y - 150)
            shader = _node(tree, "ShaderNodeMixShader", "coverage", sx + 220, y)
            tree.links.new(alpha, shader.inputs["Fac"])
            tree.links.new(clear.outputs[0], shader.inputs[1])
            tree.links.new(surface.outputs[0], shader.inputs[2])
        method = "DITHERED"
    shader.name = SHADER
    tree.links.new(shader.outputs[0], output.inputs["Surface"])
    if hasattr(material, "surface_render_method"):
        material.surface_render_method = method
    else:  # before Blender 4.2
        material.blend_method = "BLEND" if method == "BLENDED" else "HASHED"
    if hasattr(material, "use_transparency_overlap"):
        material.use_transparency_overlap = False
    material[FLAG] = 1
    return True


def apply_all(materials=None):
    count = 0
    for material in materials if materials is not None else bpy.data.materials:
        if is_map_material(material) and apply(material):
            count += 1
    return count


@contextlib.contextmanager
def for_export():
    """The Principled BSDFs linked to the outputs while a scene is written, the look after."""
    swapped = []
    for material in bpy.data.materials:
        tree = material.node_tree
        if tree is None or not material.get(FLAG):
            continue
        output, principled, shader = _output(tree), _first(tree, "BSDF_PRINCIPLED"), tree.nodes.get(SHADER)
        if output is None or principled is None or shader is None:
            continue
        tree.links.new(principled.outputs[0], output.inputs["Surface"])
        swapped.append((material.name, shader.name))
    try:
        yield
    finally:
        for name, shader_name in swapped:
            material = bpy.data.materials.get(name)
            if material is None:
                continue
            tree = material.node_tree
            output, shader = _output(tree), tree.nodes.get(shader_name)
            if output is not None and shader is not None:
                tree.links.new(shader.outputs[0], output.inputs["Surface"])


def shown_at(time, hour):
    """Whether a piece of hours `time` ([start, end], PIECE_TIME) is drawn at `hour`: from start
    up to end, across midnight when start is after end; [0, 0] (or start equal to end) always."""
    try:
        start, end = int(time[0]), int(time[1])
    except (TypeError, ValueError, IndexError, KeyError):
        return True
    if start == end:
        return True
    if start < end:
        return start <= hour < end
    return hour >= start or hour < end


def shown(obj):
    """Whether the game shows a placement or piece at the start (its show flag is not 0)."""
    value = c2data.to_py(c2data.lookup(obj, ("show",)))
    try:
        return value is None or int(value) != 0
    except (TypeError, ValueError):
        return True


def show_hour(context, hour, enabled=True, roots=None):
    """Show the pieces the game draws at `hour` (all of them when not `enabled`), in the
    viewport and in renders: not those whose hours leave it out, nor those it hides at the start
    (show 0, theirs or their placement's: deleted placements, pieces scripts show later).
    Returns how many pieces are hidden."""
    hidden = 0
    layer_objects = context.view_layer.objects
    for root in roots if roots is not None else c2data.scene_map_roots(context.scene):
        for piece in c2data.descendants(root):
            if c2data.role(piece) != "piece":
                continue
            placement = c2data.placement_of(piece)
            visible = not enabled or (
                shown_at(c2data.to_py(c2data.lookup(piece, ("time",))), hour)
                and shown(piece)
                and (placement is None or shown(placement))
            )
            hidden += not visible
            for obj in c2data.descendants(piece):
                if obj.name in layer_objects:
                    obj.hide_set(not visible)
                obj.hide_render = not visible
    return hidden


def _srgb(value):
    """A 0-255 display colour as Blender's linear value."""
    value = max(0.0, min(1.0, float(value) / 255.0))
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def active_light_set(root):
    """The light set the map starts with (ACTIVE_LIGHT_SET, by number), or None."""
    tree = c2data.to_py(c2data.lookup(root, ("map",))) or {}
    if not hasattr(tree, "get"):
        return None
    # the .map's settings, or (scenes written before they had their own key) the tree itself
    settings = tree.get("settings") if hasattr(tree.get("settings"), "get") else tree
    sets = settings.get("light_sets") or []
    wanted = settings.get("active_light_set")
    for light_set in sets:
        if hasattr(light_set, "get") and light_set.get("index") == wanted:
            return light_set
    return sets[0] if sets and hasattr(sets[0], "get") else None


def map_world(context, root):
    """The scene lit as the map's light set lights it (Rendered and F12): the background colour
    seen behind everything, the ambient colour from all around and its directional lights as suns
    (outside the map's tree, so never exported). An earlier one is replaced; a world of your own
    is kept. Returns the world, or None without a light set."""
    from mathutils import Vector

    light_set = active_light_set(root)
    if light_set is None:
        return None
    name = "c2 %s" % c2data.map_name(root)
    scene = context.scene
    world = bpy.data.worlds.get(name) or bpy.data.worlds.new(name)
    if bpy.app.version < (5, 0, 0):
        world.use_nodes = True
    tree = world.node_tree
    tree.nodes.clear()
    output = tree.nodes.new("ShaderNodeOutputWorld")
    output.location = (400, 0)
    seen = tree.nodes.new("ShaderNodeBackground")
    seen.label = "background (BGCOLOR)"
    seen.location = (0, 100)
    lit = tree.nodes.new("ShaderNodeBackground")
    lit.label = "ambient (AMBIENT)"
    lit.location = (0, -100)
    path = tree.nodes.new("ShaderNodeLightPath")
    path.location = (-250, 250)
    mix = tree.nodes.new("ShaderNodeMixShader")
    mix.location = (200, 0)
    background = light_set.get("bgcolor") or [0, 0, 0]
    seen.inputs["Color"].default_value = [_srgb(c) for c in background[:3]] + [1.0]
    ambient = light_set.get("ambient") or [64, 64, 64]
    lit.inputs["Color"].default_value = [max(0.0, float(c)) / 128.0 for c in ambient[:3]] + [1.0]
    tree.links.new(path.outputs["Is Camera Ray"], mix.inputs["Fac"])
    tree.links.new(lit.outputs[0], mix.inputs[1])
    tree.links.new(seen.outputs[0], mix.inputs[2])
    tree.links.new(mix.outputs[0], output.inputs["Surface"])
    if scene.world is None or scene.world.name in ("World", name) or scene.world.name.startswith("c2 "):
        scene.world = world

    prefix = "c2 light %s " % c2data.map_name(root)
    for obj in [obj for obj in bpy.data.objects if obj.name.startswith(prefix)]:
        bpy.data.objects.remove(obj)
    for at, light in enumerate(light_set.get("lights") or []):
        if not hasattr(light, "get") or light.get("type", "light") != "light":
            continue
        direction = light.get("dir") or [0, 1, 0]
        towards = Vector((float(direction[0]), -float(direction[2]), float(direction[1])))
        if towards.length == 0:
            continue
        colour = light.get("color") or [128, 128, 128]
        strength = max(float(c) for c in colour[:3]) / 128.0
        data = bpy.data.lights.new(prefix + str(at), "SUN")
        data.color = [max(0.0, float(c)) / max(1.0, max(float(c) for c in colour[:3])) for c in colour[:3]]
        data.energy = 3.14159 * strength
        sun = bpy.data.objects.new(prefix + str(at), data)
        sun.rotation_euler = towards.normalized().to_track_quat("Z", "Y").to_euler()
        sun.location = root.location
        scene.collection.objects.link(sun)
    return world
