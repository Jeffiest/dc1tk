import json
import os
import sys

import bpy

from dc2tk_maps import c2data, edit, export_mod, look, props, runner, schema, validate


export_materials = look.export_materials


def activate(context, obj):
    for selected in context.selected_objects:
        selected.select_set(False)
    obj.select_set(True)
    context.view_layer.objects.active = obj


def check_value_editor(root):
    values = [
        ["t01.img"],
        ["foo.img", "null.img", "123.img"],
        ["a,b", "", " quoted ", 'a"b', "家"],
        [True, False, None, 1, 2.5, "1"],
        [[1, 2], {"key": "value"}],
        [],
    ]
    for value in values:
        text = edit.show_value(value)
        assert edit.parse_like(text, value) == value, (value, text)
    try:
        edit.parse_like('"t01.img"', ["t01.img"])
    except ValueError:
        pass
    else:
        raise AssertionError("A list editor accepted a scalar")
    original = c2data.to_py(c2data.lookup(root, ("map", "images")))
    try:
        for value in values[:3]:
            c2data.assign(root, ("map", "images"), value)
            result = bpy.ops.dc2tk.edit_value(
                object_name=root.name,
                path=json.dumps(["map", "images"]),
                value=edit.show_value(value),
            )
            assert result == {"FINISHED"}
            assert c2data.to_py(c2data.lookup(root, ("map", "images"))) == value
    finally:
        c2data.assign(root, ("map", "images"), original)
    print("PASS: JSON list editing")


def check_light_limits(root):
    original = c2data.to_py(c2data.lookup(root, ("map",)))
    try:
        for legacy in (False, True):
            for number in (8, 9):
                settings = dict(original["settings"], time_light_num=number)
                tree = settings if legacy else dict(original, settings=settings)
                c2data.assign(root, ("map",), tree)
                errors = [message for level, name, message in validate.check(root) if level == validate.ERROR]
                assert any("TIME_LIGHT_NUM" in message for message in errors) == (number > 8), errors
                assert look.active_light_set(root)["index"] == settings["active_light_set"]
    finally:
        c2data.assign(root, ("map",), original)
    print("PASS: current and legacy light settings")


def check_byte_names(context, root):
    base = "家" * 20
    first = edit.unique_game_name(base, set())
    second = edit.unique_game_name(base, {first})
    third = edit.unique_game_name(base, {first, second})
    assert len(first.encode("cp932")) == 30
    assert second.endswith("_2") and third.endswith("_3")
    assert len({first, second, third}) == 3
    assert all(len(name.encode("cp932")) <= 31 for name in (first, second, third))
    assert edit.unique_game_name("a" * 31, {"a" * 31}) == "a" * 29 + "_2"
    created = []
    try:
        for expected in (first, second, third):
            activate(context, root)
            assert bpy.ops.dc2tk.add_point(preset="SPAWN", name=base, in_placement=False) == {"FINISHED"}
            obj = context.active_object
            created.append(obj)
            assert c2data.exact_name(obj, "point") == expected
        names = {obj.name for obj in created}
        assert not [result for result in validate.check(root) if result[0] == validate.ERROR and result[1] in names]
    finally:
        for obj in created:
            bpy.data.objects.remove(obj, do_unlink=True)
    print("PASS: CP932 name limits and suffixes")


def check_callbacks(context, root):
    settings = context.window_manager.dc2tk_maps
    tested = set()
    for obj in c2data.points(root):
        kind = c2data.lookup(obj, ("kind",))
        if kind in tested or c2data.is_stringified(obj):
            continue
        tested.add(kind)
        original = c2data.get(obj)
        activate(context, obj)
        try:
            for field in schema.DATA_FIELDS[kind]:
                name = props.data_prop_name(kind, field)
                value = getattr(settings, name)
                if field["size"] > 1:
                    value = tuple(value)
                setattr(settings, name, value)
                actual = getattr(settings, name)
                if field["size"] > 1:
                    actual = tuple(actual)
                assert actual == value, (kind, name)
        finally:
            c2data.put(obj, original)
    assert tested == set(schema.POINT_KINDS), tested
    print("PASS: callbacks for all point kinds")


def check_renames(context, root, work):
    settings = context.window_manager.dc2tk_maps
    placement = bpy.data.objects["place:house02"]
    piece = next(child for child in placement.children if c2data.role(child) == "piece")
    point = bpy.data.objects["point:river"]
    targets = [(placement, "place_name", "renamed_house"), (piece, "piece_name", "renamed_piece"), (point, "point_name", "renamed_point")]
    original = [(obj, obj.name, c2data.get(obj)) for obj, prop, name in targets]
    copy = None
    try:
        for obj, prop, name in targets:
            activate(context, obj)
            setattr(settings, prop, name)
            assert c2data.lookup(obj, ("name",)) == name
            assert getattr(settings, prop) == name
            assert schema.game_name(obj.name, c2data.role(obj)) == name
        activate(context, placement)
        assert bpy.ops.dc2tk.duplicate_placement() == {"FINISHED"}
        copy = context.active_object
        assert c2data.exact_name(copy, "placement") == "renamed_house_2"
        assert c2data.lookup(copy, ("slot",)) is None
        copy.name = "place:unrelated_display_name"
        c2data.assign(copy, ("name",), "renamed_house")
        errors = [message for level, name, message in validate.check(root) if level == validate.ERROR]
        assert any('Placement name "renamed_house" is used' in message for message in errors), errors
        c2data.rename(copy, "renamed_house_2")
        point.name = "point:short_display_name"
        c2data.assign(point, ("name",), "x" * 32)
        assert any(level == validate.ERROR and name == point.name and "bytes" in message for level, name, message in validate.check(root))
        c2data.rename(point, "renamed_point")
        path, manifest, fixed = export_mod.export_map(context, root, os.path.join(work, "rename_regression"))
        with open(path, encoding="utf-8") as handle:
            gltf = json.load(handle)
        exported = {node["name"]: node.get("extras", {}).get("c2", {}) for node in gltf["nodes"]}
        for obj, prop, name in targets:
            assert exported[obj.name]["name"] == name
        assert exported[copy.name]["name"] == "renamed_house_2"
        for obj, prop, name in targets:
            c2data.remove(obj, ("name",))
            activate(context, obj)
            assert getattr(settings, prop) == name
            setattr(settings, prop, name + "_old")
            assert c2data.lookup(obj, ("name",)) == name + "_old"
    finally:
        if copy is not None:
            for obj in reversed(c2data.descendants(copy)):
                bpy.data.objects.remove(obj, do_unlink=True)
        for obj, name, data in original:
            obj.name = name
            c2data.put(obj, data)
        activate(context, root)
    print("PASS: renames, duplication, validation and exported metadata")


def fail_material_export(swapped):
    export_materials(swapped)
    raise RuntimeError("Export failure test")


def check_export_failure(context, root, work):
    activate(context, bpy.data.objects["place:house01"])
    selected = {obj.name for obj in context.selected_objects}
    active = context.active_object
    states = []
    for obj in c2data.descendants(root):
        states.append((obj, obj.hide_get(), obj.hide_viewport, obj.hide_select))
    materials = []
    for material in bpy.data.materials:
        if material.get(look.FLAG):
            surface = look._output(material.node_tree).inputs["Surface"]
            materials.append((material, surface.links[0].from_node.name))
    try:
        look.export_materials = fail_material_export
        try:
            export_mod.export_map(context, root, os.path.join(work, "failed_export"))
        except RuntimeError as error:
            assert str(error) == "Export failure test"
        else:
            raise AssertionError("Export did not fail")
    finally:
        look.export_materials = export_materials
    assert context.active_object == active
    assert {obj.name for obj in context.selected_objects} == selected
    for obj, hidden, hide_viewport, hide_select in states:
        assert (obj.hide_get(), obj.hide_viewport, obj.hide_select) == (hidden, hide_viewport, hide_select)
    for material, shader_name in materials:
        surface = look._output(material.node_tree).inputs["Surface"]
        assert surface.links[0].from_node.name == shader_name
    assert not os.path.exists(os.path.join(work, "failed_export", "mod.json"))
    print("PASS: export failure restores scene state")


def check_jobs(work):
    for returncode in (0, 7):
        command = [sys.executable, "-c", "print('job output'); raise SystemExit(%d)" % returncode]
        job = runner.run_blocking("Regression", command, work)
        assert runner.job_done(job)
        assert job["returncode"] == returncode
        assert runner.output_lines(job) == ["job output"]
        assert runner.tail() == ["job output"]
        assert "[exit code %d]" % returncode in bpy.data.texts[runner.TEXT_NAME].as_string()
    print("PASS: subprocess output and exit codes")


def run(context, root, work):
    check_value_editor(root)
    check_light_limits(root)
    check_byte_names(context, root)
    check_callbacks(context, root)
    check_renames(context, root, work)
    check_export_failure(context, root, work)
    check_jobs(work)
