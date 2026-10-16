import bpy

_icons = None


def icons():
    global _icons
    if _icons is None:
        _icons = set(bpy.types.UILayout.bl_rna.functions["label"].parameters["icon"].enum_items.keys())
    return _icons


def operator_props(idname):
    module, name = idname.split(".")
    try:
        op = getattr(getattr(bpy.ops, module), name)
        return {prop.identifier for prop in op.get_rna_type().properties}
    except (AttributeError, KeyError):
        return None


class FakeOperatorProps:
    def __init__(self, idname, allowed, problems):
        object.__setattr__(self, "_idname", idname)
        object.__setattr__(self, "_allowed", allowed)
        object.__setattr__(self, "_problems", problems)

    def __setattr__(self, key, value):
        if key not in self._allowed:
            self._problems.append("%s has no property %s" % (self._idname, key))


class FakeLayout:
    def __init__(self, problems):
        self.problems = problems

    def _icon(self, kwargs):
        icon = kwargs.get("icon")
        if icon and icon not in icons():
            self.problems.append("unknown icon %s" % icon)

    def _child(self, *args, **kwargs):
        return FakeLayout(self.problems)

    row = column = box = split = grid_flow = column_flow = _child

    def separator(self, **kwargs):
        pass

    def label(self, text="", **kwargs):
        self._icon(kwargs)

    def prop(self, data, name, **kwargs):
        self._icon(kwargs)
        if data is None:
            self.problems.append("prop %s on None" % name)
            return
        if name not in data.bl_rna.properties:
            self.problems.append("%s has no property %s" % (type(data).__name__, name))
            return
        try:
            getattr(data, name)
        except Exception as error:
            self.problems.append("reading %s: %s" % (name, error))

    def prop_search(self, data, name, search_data, search_name, **kwargs):
        self.prop(data, name, **kwargs)
        self.prop(search_data, search_name)

    def operator(self, idname, **kwargs):
        self._icon(kwargs)
        allowed = operator_props(idname)
        if allowed is None:
            self.problems.append("no operator %s" % idname)
            allowed = set()
        return FakeOperatorProps(idname, allowed, self.problems)

    def operator_menu_enum(self, idname, prop, **kwargs):
        self._icon(kwargs)
        allowed = operator_props(idname)
        if allowed is None or prop not in allowed:
            self.problems.append("operator_menu_enum %s.%s" % (idname, prop))

    def menu(self, idname, **kwargs):
        self._icon(kwargs)
        if not hasattr(bpy.types, idname):
            self.problems.append("no menu %s" % idname)


class FakePanel:
    def __init__(self, layout):
        self.layout = layout


def draw_all(context):
    import dc2tk_maps.ui as ui

    problems = []
    for cls in ui.classes:
        try:
            cls.draw(FakePanel(FakeLayout(problems)), context)
        except Exception as error:
            import traceback

            traceback.print_exc()
            problems.append("%s.draw: %s" % (cls.__name__, error))
    return problems
