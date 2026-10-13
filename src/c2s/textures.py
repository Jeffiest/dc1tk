import re
from pathlib import Path

from c2s import im3

# Textures as PNG files for mods. A texture file (.img) holds many named pictures, and models find
# theirs by name, so a mod gives a folder named after the texture file plus .png
# (c02a.img.png/) with one PNG per picture it changes, named after the picture (c02a01.png).
# Pictures the folder leaves out stay as they are. build quantises a picture to its palette size
# when it has more colours, keeps the original palette when every colour is in it, and makes the
# picture's mip levels again; a picture may change size (see im3.md for the GS limits).

SUFFIX = ".png"


def file_name(name):
    return re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", name) + SUFFIX


# {file name: PNG bytes} for each picture of a texture file that has pixels; render targets
# (pictures with no pixels) are left out.
def export(data):
    out = {}
    for image in im3.read(data):
        if image.no_image:
            continue
        name = file_name(image.name)
        if name in out:
            name = file_name("%s_%d" % (image.name, image.index))
        out[name] = im3.to_png(image)
    return out


# The texture file with the pictures a folder of PNGs gives in place of the original's.
def build(folder, original):
    images = im3.read(original)
    by_name = {}
    for position, image in enumerate(images):
        by_name.setdefault(file_name(image.name), position)
        by_name.setdefault(file_name("%s_%d" % (image.name, image.index)), position)
    for png in sorted(Path(folder).glob("*" + SUFFIX)):
        if png.name not in by_name:
            names = ", ".join(sorted(file_name(image.name)[:-len(SUFFIX)] for image in images if not image.no_image))
            raise ValueError("%s names no picture of the texture file (it has %s)" % (png.name, names))
        position = by_name[png.name]
        if images[position].no_image:
            raise ValueError("%s is a render target with no pixels" % png.name)
        try:
            images[position] = im3.from_png(png.read_bytes(), like=images[position])
        except (ValueError, KeyError) as error:
            raise ValueError("%s: %s" % (png.name, error)) from None
    return im3.write(images, like=original)
