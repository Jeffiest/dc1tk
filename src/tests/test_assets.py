from c2s import assets, notes


def test_asset_names_come_from_glosses_and_stay_apart():
    stand, stand_2, wave = "\u7acb\u3061", "\u7acb\u3061\uff12", "\u624b\u3092\u632f\u308b"
    glosses = {notes.string_key(stand): "stand", notes.string_key(stand_2): "stand", notes.string_key(wave): "wave (loop)"}
    found = {("MOTION", stand), ("MOTION", stand_2), ("MOTION", wave), ("ANIM", "\u7b11\u9854")}
    names = assets.names(found, glosses, taken={"MOTION_WAVE_LOOP"})
    assert names == {("MOTION", stand): "MOTION_STAND", ("MOTION", stand_2): "MOTION_STAND_2",
                     ("MOTION", wave): "MOTION_WAVE_LOOP_2"}
    assert '#define MOTION_STAND "\u7acb\u3061"' in assets.header_text(names)


def test_asset_strings_passed_to_known_parameters_are_replaced():
    found = {"by_original": {"_SET_MOTION": {"name": "set_motion", "params": [{"name": "motion_name"}]}},
             "asset_names": {("MOTION", "\u7acb\u3061"): "MOTION_STAND"}}
    node = {"kind": "native", "name": "_SET_MOTION", "args": [{"kind": "string", "text": "\u7acb\u3061"}]}
    assert assets.named([node], found)[0]["args"] == [{"kind": "var", "name": "MOTION_STAND"}]
