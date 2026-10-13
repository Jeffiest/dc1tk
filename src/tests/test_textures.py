import tempfile
from pathlib import Path

from c2s import im3, textures


def sample_texture_file():
    image = im3.Image("tex01", "PSMT8", 16, 16)
    image.palette = [(i, 255 - i, 0, 0x80) for i in range(256)]
    image.indices = bytes((x + y) % 256 for y in range(16) for x in range(16))
    other = im3.Image("tex02", "PSMCT32", 8, 8)
    other.pixels = bytes([10, 20, 30, 0x80]) * 64
    return im3.write([image, other])


def test_a_texture_file_exports_one_png_per_picture_and_builds_back_unchanged():
    data = sample_texture_file()
    exported = textures.export(data)
    assert sorted(exported) == ["tex01.png", "tex02.png"]
    with tempfile.TemporaryDirectory() as folder:
        for name, png in exported.items():
            (Path(folder) / name).write_bytes(png)
        assert textures.build(folder, data) == data


def test_a_folder_changes_only_the_pictures_it_holds():
    data = sample_texture_file()
    with tempfile.TemporaryDirectory() as folder:
        # tex02 in blue; tex01 is left out and keeps its pixels
        (Path(folder) / "tex02.png").write_bytes(im3.write_png(8, 8, "RGB", bytes([0, 0, 255]) * 64))
        built = im3.read(textures.build(folder, data))
    assert built[1].pixels == bytes([0, 0, 255, 0x80]) * 64
    assert built[0].indices == im3.read(data)[0].indices


def test_a_png_for_no_picture_is_refused_with_the_names_there_are():
    data = sample_texture_file()
    with tempfile.TemporaryDirectory() as folder:
        (Path(folder) / "tex99.png").write_bytes(im3.write_png(8, 8, "RGB", bytes(192)))
        try:
            textures.build(folder, data)
        except ValueError as error:
            assert "tex01" in str(error) and "tex02" in str(error)
        else:
            raise AssertionError("built a picture the file does not have")
