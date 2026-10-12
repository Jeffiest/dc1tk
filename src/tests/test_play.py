from c2s import play


def test_pcsx2_is_found_where_given_then_in_the_environment_path_and_install_folders():
    nowhere = lambda name: None
    assert play.find_pcsx2("C:/emu/pcsx2.exe", {}, nowhere, lambda path: True) == "C:/emu/pcsx2.exe"
    assert play.find_pcsx2(None, {"PCSX2": "/opt/pcsx2"}, nowhere, lambda path: True) == "/opt/pcsx2"
    assert play.find_pcsx2(None, {}, lambda name: "/bin/" + name, lambda path: False) == "/bin/pcsx2-qt"
    installed = play.INSTALLS[0]
    assert play.find_pcsx2(None, {}, nowhere, lambda path: path == installed) == installed
    assert play.find_pcsx2(None, {}, nowhere, lambda path: False) is None


def test_the_image_boots_without_the_ps2_screens():
    command = play.command("pcsx2-qt", "game.iso")
    assert command[:3] == ["pcsx2-qt", "-fastboot", "--"] and command[3].endswith("game.iso")
