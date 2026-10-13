"""Dark Chronicle (Dark Cloud 2) texture archives: IM3, IM2 and IMG <-> RGBA and PNG.

Standard library only.  See im3.md for the format.

    images = read(data)                 # every picture of the archive
    png = to_png(images[0])             # base level as a PNG
    images[0] = from_png(png, like=images[0])
    data2 = write(images, like=data)    # == data when nothing was edited

An Image holds its pixels in the engine's own terms: paletted pictures keep
``indices`` (one byte per pixel, in raster order) and ``palette`` (a list of
(r, g, b, a) in index order, CSM1 swizzle already undone); true-colour
pictures keep ``pixels`` as RGBA bytes.  Alpha is the PS2's own value, where
0x80 means opaque; only the PNG side rescales it.
"""

import operator
import struct
import zlib

__all__ = ['Image', 'read', 'write', 'to_png', 'from_png', 'alpha_mode',
           'read_png', 'write_png', 'swizzle8_offsets']

# --------------------------------------------------------------------------
# Constants

FORMATS = {1: 'PSMCT16', 2: 'PSMCT24', 3: 'PSMCT32', 4: 'PSMT4', 5: 'PSMT8'}
IMAGE_TYPE = {v: k for k, v in FORMATS.items()}
BPP = {'PSMCT16': 16, 'PSMCT24': 24, 'PSMCT32': 32, 'PSMT4': 4, 'PSMT8': 8}
PALETTED = ('PSMT4', 'PSMT8')

IM3_ENTRY = 0x40
IM1_ENTRY = 0x30

# BlockConv32to8's lut_1246 (ps2/asm/pal/data/mg_texture.data.s): position,
# inside a 64-byte group of a 16x16 8-bit block, of each byte of the
# matching 32-bit column; odd groups use the second half.
_LUT = bytes.fromhex(
    '00 24 08 2c 01 25 09 2d 02 26 0a 2e 03 27 0b 2f'
    '04 20 0c 28 05 21 0d 29 06 22 0e 2a 07 23 0f 2b'
    '10 34 18 3c 11 35 19 3d 12 36 1a 3e 13 37 1b 3f'
    '14 30 1c 38 15 31 1d 39 16 32 1e 3a 17 33 1f 3b'
    '04 20 0c 28 05 21 0d 29 06 22 0e 2a 07 23 0f 2b'
    '00 24 08 2c 01 25 09 2d 02 26 0a 2e 03 27 0b 2f'
    '14 30 1c 38 15 31 1d 39 16 32 1e 3a 17 33 1f 3b'
    '10 34 18 3c 11 35 19 3d 12 36 1a 3e 13 37 1b 3f')
assert len(_LUT) == 128


def _block_table():
    """Byte of the 8x8 CT32 block (256 bytes, 32 per row) holding each
    pixel of the 16x16 PSMT8 block, as BlockConv32to8 + PageConv32to8 move
    them: 8-bit pixel j = row*16 + col  ->  32-bit block byte i."""
    inv = [0] * 256
    for i in range(256):
        group, t = divmod(i, 64)
        j = group * 64 + _LUT[(group & 1) * 64 + t]
        inv[j] = i
    assert sorted(inv) == list(range(256))
    return inv


_BLOCK8 = _block_table()


def _classic_swizzle8(x, y, w):
    """The widely used PS2 'unswizzle8' address formula, for cross-checking."""
    block = (y & ~0xF) * w + (x & ~0xF) * 2
    swap = (((y + 2) >> 2) & 1) * 4
    posy = (((y & ~3) >> 1) + (y & 1)) & 7
    column = posy * w * 2 + ((x + swap) & 7) * 4
    byte = ((y >> 1) & 1) + ((x >> 2) & 2)
    return block + column + byte


_OFFSET_CACHE = {}


def swizzle8_offsets(w, h):
    """File byte offset of every pixel (raster order) of a w x h PSMT8 level
    stored 'swizzled', i.e. as the (w/2) x (h/2) PSMCT32 image the engine
    uploads (mg_texture.cpp ReloadTexture / Conv32To8).  Offsets can point
    past the stored data when h is not a multiple of 16."""
    key = (w, h)
    got = _OFFSET_CACHE.get(key)
    if got is not None:
        return got
    if w % 16:
        raise ValueError('swizzled 8-bit width %d is not a multiple of 16' % w)
    rowbytes = 2 * w            # a PSMCT32 row of w/2 pixels
    row_parts = []
    for iy in range(16):
        part = []
        for x in range(w):
            bx, ix = divmod(x, 16)
            i = _BLOCK8[iy * 16 + ix]
            part.append(((i >> 5), bx * 32 + (i & 31)))
        row_parts.append(part)
    out = []
    for y in range(h):
        by, iy = divmod(y, 16)
        base_row = by * 8
        out.extend([(base_row + r) * rowbytes + c for r, c in row_parts[iy]])
    if len(_OFFSET_CACHE) > 64:
        _OFFSET_CACHE.clear()
    _OFFSET_CACHE[key] = out
    return out


def _gather(src, idx):
    """bytes(src[i] for i in idx), fast."""
    if not idx:
        return b''
    if len(idx) == 1:
        return bytes([src[idx[0]]])
    return bytes(operator.itemgetter(*idx)(src))


_CSM1 = [((i & 0xE7) | ((i & 8) << 1) | ((i & 16) >> 1)) for i in range(256)]

_LO = bytes(b & 15 for b in range(256))
_HI = bytes(b >> 4 for b in range(256))

# PS2 alpha (0..0x80) <-> PNG alpha (0..255)
_A2P = bytes(min(255, (a * 255 + 64) // 128) for a in range(256))
_P2A = bytes((p * 128 + 127) // 255 for p in range(256))
assert all(_P2A[_A2P[a]] == a for a in range(129))
_IDENT = bytes(range(256))


# --------------------------------------------------------------------------
# Image


class Image:
    """One picture of an archive.

    Attributes
      name        str, the name it is registered under (before the first NUL)
      index       entry number in the archive (None for a new picture)
      format      'PSMT8', 'PSMT4', 'PSMCT32', 'PSMCT24' or 'PSMCT16'
      width, height
      indices     paletted: bytes, one palette index per pixel (base level)
      palette     paletted: list of (r, g, b, a), index order, PS2 alpha
      pixels      true colour: RGBA bytes of the base level, PS2 alpha
                  (PSMCT24 pictures read alpha as 0x80)
      mips        further mip levels, each like indices/pixels
      swizzled    entry flag: 8-bit pixels are stored in PSMCT32 order
      no_image    IM3 flag: header only, no pixels or palette stored
      block       IM3 texture block group (0..31), relative to the load block
      clamp       IM3 GS CLAMP register value (u64)
      tim2        the picture's original TIM2 bytes (header kept on write)
    """

    def __init__(self, name='', format='PSMT8', width=0, height=0):
        self.name = name
        self.index = None
        self.format = format
        self.width = width
        self.height = height
        self.indices = None
        self.palette = None
        self.pixels = None
        self.mips = []
        self.swizzled = False
        self.no_image = False
        self.block = 0
        self.clamp = 5
        self.tim2 = None
        self.raw_name = None

    @property
    def bpp(self):
        return BPP[self.format]

    @property
    def paletted(self):
        return self.format in PALETTED

    @property
    def rgba(self):
        """Base level as RGBA bytes (PS2 alpha)."""
        if self.no_image:
            return None
        if not self.paletted:
            return self.pixels
        table = [bytes(c) for c in self.palette]
        table += [b'\0\0\0\0'] * (256 - len(table))
        return b''.join([table[i] for i in self.indices])

    def copy(self):
        new = Image.__new__(Image)
        new.__dict__.update(self.__dict__)
        new.mips = list(self.mips)
        if self.palette is not None:
            new.palette = list(self.palette)
        return new

    def __repr__(self):
        return '<Image %r %s %dx%d%s%s>' % (
            self.name, self.format, self.width, self.height,
            ' mips=%d' % len(self.mips) if self.mips else '',
            ' no_image' if self.no_image else '')


# --------------------------------------------------------------------------
# Archive container


class _Entry:
    __slots__ = ('index', 'pos', 'raw', 'name', 'offset', 'swizzled', 'block',
                 'no_image', 'size', 'clamp', 'script')


def _parse_archive(data):
    """Returns (kind, entries) where kind is 'IM3', 'IM2' or 'IMG'."""
    if len(data) < 16 or data[:2] != b'IM':
        if not data:
            return None, []
        raise ValueError('not an IMG/IM2/IM3 archive')
    tag = data[:3]
    if tag == b'IM3':
        kind, count, stride = 'IM3', struct.unpack_from('<I', data, 8)[0], IM3_ENTRY
    elif tag in (b'IM2', b'IMG'):
        kind, count, stride = tag.decode(), struct.unpack_from('<I', data, 4)[0], IM1_ENTRY
    else:
        raise ValueError('unknown archive tag %r' % data[:4])
    entries = []
    for i in range(count):
        pos = 0x10 + stride * i
        e = _Entry()
        e.index, e.pos, e.raw = i, pos, data[pos:pos + stride]
        e.name = e.raw[:0x20].split(b'\0', 1)[0]
        e.script = e.name[:1] == b'#'
        if kind == 'IM3':
            (_, e.offset, e.swizzled, e.block, e.no_image, _, e.size,
             e.clamp) = struct.unpack_from('<IiiihhiQ', e.raw, 0x20)
        else:
            e.offset = struct.unpack_from('<i', e.raw, 0x20)[0]
            e.swizzled = 1 if kind == 'IM2' else 0
            e.block, e.no_image, e.size, e.clamp = 0, 0, None, None
            e.script = False
        entries.append(e)
    return kind, entries


def _tim2_extent(data, off, no_image):
    """Length of the TIM2 image at off: file header, picture header, pixels
    and palette (only the header when the IM3 entry says no_image)."""
    if data[off:off + 4] != b'TIM2':
        raise ValueError('no TIM2 at 0x%x' % off)
    npic = struct.unpack_from('<H', data, off + 6)[0]
    q = off + 16
    if no_image:
        return 16 + struct.unpack_from('<H', data, q + 12)[0]
    end = q
    for _ in range(max(npic, 1)):
        total, cs, isz, hs = struct.unpack_from('<IIIH', data, end)
        end += hs + isz + cs
    return end - off


# --------------------------------------------------------------------------
# TIM2 picture <-> Image


def _decode_tim2(tim, swizzled, no_image):
    img = Image()
    q = 16
    (total, cs, isz, hs, cc, pf, mm, ct, it, w, h) = struct.unpack_from(
        '<IIIHHBBBBHH', tim, q)
    if it not in FORMATS:
        raise ValueError('unsupported TIM2 image type %d' % it)
    img.format = FORMATS[it]
    img.width, img.height = w, h
    img.swizzled = bool(swizzled)
    img.no_image = bool(no_image)
    img.tim2 = bytes(tim)
    if no_image:
        return img
    bpp = BPP[img.format]
    if mm > 1:
        sizes = list(struct.unpack_from('<%dI' % mm, tim, q + 0x40))
    else:
        sizes = [isz]
    pos = q + hs
    levels = []
    for k, size in enumerate(sizes):
        lw, lh = max(w >> k, 1), max(h >> k, 1)
        levels.append(_decode_level(tim[pos:pos + size], img.format, lw, lh, swizzled))
        pos += size
    if img.paletted:
        clut = tim[q + hs + isz:q + hs + isz + cs]
        img.palette = _decode_clut(clut, cc, ct, img.format)
        img.indices = levels[0]
    else:
        img.pixels = levels[0]
    img.mips = levels[1:]
    return img


def _decode_level(raw, fmt, w, h, swizzled):
    n = w * h
    if fmt == 'PSMT8':
        if swizzled:
            offs = swizzle8_offsets(w, h)
            need = (max(offs) + 1) if offs else 0
            src = raw + bytes(max(0, need - len(raw)))
            return _gather(src, offs)
        return bytes(raw[:n]) + bytes(max(0, n - len(raw)))
    if fmt == 'PSMT4':
        out = bytearray(2 * len(raw))
        out[0::2] = raw.translate(_LO)
        out[1::2] = raw.translate(_HI)
        out += bytes(max(0, n - len(out)))
        return bytes(out[:n])
    if fmt == 'PSMCT32':
        return bytes(raw[:4 * n])
    if fmt == 'PSMCT24':
        out = bytearray(b'\x80' * (4 * n))
        out[0::4] = raw[0:3 * n:3]
        out[1::4] = raw[1:3 * n:3]
        out[2::4] = raw[2:3 * n:3]
        return bytes(out)
    if fmt == 'PSMCT16':
        out = bytearray(4 * n)
        for i, (v,) in enumerate(struct.iter_unpack('<H', raw[:2 * n])):
            r, g, b = v & 31, (v >> 5) & 31, (v >> 10) & 31
            out[4 * i:4 * i + 4] = bytes(((r << 3) | (r >> 2), (g << 3) | (g >> 2),
                                          (b << 3) | (b >> 2), 0x80 if v & 0x8000 else 0))
        return bytes(out)
    raise ValueError(fmt)


def _encode_level(data, fmt, w, h, swizzled, base):
    """Inverse of _decode_level.  base is the original stored bytes of the
    level (or None); bytes no pixel maps to are kept from it."""
    n = w * h
    size = (n * BPP[fmt] + 7) // 8
    if base is None or len(base) != size:
        base = bytes(size)
    if fmt == 'PSMT8':
        if swizzled:
            offs = swizzle8_offsets(w, h)
            src = bytes(data) + base
            sel = list(range(n, n + size))
            for p, o in enumerate(offs):
                if o < size:
                    sel[o] = p
            return _gather(src, sel)
        return bytes(data[:n])
    if fmt == 'PSMT4':
        lo = data[0::2]
        hi = data[1::2]
        if len(hi) < len(lo):
            hi = hi + bytes([base[-1] >> 4])
        return bytes([(a & 15) | ((b & 15) << 4) for a, b in zip(lo, hi)])
    if fmt == 'PSMCT32':
        return bytes(data[:4 * n])
    if fmt == 'PSMCT24':
        out = bytearray(3 * n)
        out[0::3] = data[0::4]
        out[1::3] = data[1::4]
        out[2::3] = data[2::4]
        return bytes(out)
    if fmt == 'PSMCT16':
        out = bytearray()
        for i in range(n):
            r, g, b, a = data[4 * i:4 * i + 4]
            out += struct.pack('<H', (r >> 3) | ((g >> 3) << 5) | ((b >> 3) << 10)
                               | (0x8000 if a >= 0x40 else 0))
        return bytes(out)
    raise ValueError(fmt)


def _decode_clut(clut, cc, ct, fmt):
    if ct & 0x3F != 3:
        raise ValueError('unsupported CLUT type 0x%x' % ct)
    colours = [tuple(clut[4 * k:4 * k + 4]) for k in range(len(clut) // 4)]
    if fmt == 'PSMT8' and len(colours) >= 256:
        # CSM1: the engine uploads the CLUT as a 16x16 PSMCT32 image; colour of
        # index i sits at file position i with bits 3 and 4 swapped.
        colours = [colours[_CSM1[i]] for i in range(256)] + colours[256:]
    return colours


def _encode_clut(palette, fmt, base):
    pal = list(palette)
    if fmt == 'PSMT8' and len(pal) >= 256:
        pal = [pal[_CSM1[k]] for k in range(256)] + pal[256:]
    raw = b''.join(bytes(c) for c in pal)
    if base is not None and len(base) > len(raw):
        raw += base[len(raw):]
    return raw


def _tex0_fields(w, h, bpp):
    tw = max((w - 1).bit_length(), 0)
    th = max((h - 1).bit_length(), 0)
    tbw = max((w + 63) // 64, 1)
    return tbw, tw, th


def _build_header(img, old):
    """TIM2 file header + picture header for img.  old is the original TIM2
    (fields not derived from the picture are kept from it)."""
    fmt, w, h = img.format, img.width, img.height
    bpp = BPP[fmt]
    nlev = 1 + len(img.mips)
    sizes = [(max(w >> k, 1) * max(h >> k, 1) * bpp + 7) // 8 for k in range(nlev)]
    isz = sum(sizes)
    if img.paletted:
        cc = len(img.palette)
        cs = 4 * cc
        ct = 3
    else:
        cc = cs = ct = 0
    if old is not None:
        o_tot, o_cs, o_isz, o_hs, o_cc, o_pf, o_mm, o_ct, o_it, o_w, o_h = \
            struct.unpack_from('<IIIHHBBBBHH', old, 16)
        filehdr = bytearray(old[:16])
        tex0, tex1, regs, tclut = struct.unpack_from('<QQII', old, 16 + 0x18)
        pf = o_pf
        if old and o_ct and img.paletted:
            ct = o_ct
        o_pix = 4 * sum(max(o_w >> k, 1) * max(o_h >> k, 1) for k in range(max(o_mm, 1)))
        if o_tot == o_hs + o_isz + o_cs:
            rule = 'exact'
        elif o_tot == o_hs + o_pix:
            rule = 'rgba32'
        else:
            rule = 'keep'
        if tex0:
            # The converter fills PSM, TW and TH (TBW left 0); the engine
            # ignores the field and builds its own TEX0.
            tbw, tw, th = _tex0_fields(w, h, bpp)
            if (tex0 >> 14) & 0x3F:
                tex0 = (tex0 & ~(0x3F << 14)) | (tbw << 14)
            tex0 &= ~((0x3F << 20) | (0xF << 26) | (0xF << 30))
            tex0 |= (tw << 26) | (th << 30)
            tex0 |= {'PSMCT32': 0, 'PSMCT24': 1, 'PSMCT16': 2,
                     'PSMT8': 0x13, 'PSMT4': 0x14}[fmt] << 20
    else:
        filehdr = bytearray(b'TIM2\x03\x00\x01\x00' + bytes(8))
        tex0, tex1, regs, tclut = 0, 0xFFFFFF8F00000000, 0, 0
        pf, rule, o_tot = 0, 'exact', 0
    hs = 0x30 if nlev == 1 else 0x30 + 0x10 + ((4 * nlev + 15) // 16) * 16
    if rule == 'exact':
        tot = hs + isz + cs
    elif rule == 'rgba32':
        tot = hs + 4 * sum(max(w >> k, 1) * max(h >> k, 1) for k in range(nlev))
    else:
        tot = o_tot
    pic = struct.pack('<IIIHHBBBBHHQQII', tot, cs, isz, hs, cc, pf, nlev, ct,
                      IMAGE_TYPE[fmt], w, h, tex0, tex1, regs, tclut)
    if nlev > 1:
        mip = bytearray(hs - 0x30)
        if old is not None and len(old) >= 16 + 0x40:
            o_hs = struct.unpack_from('<H', old, 16 + 12)[0]
            if o_hs > 0x30:
                keep = old[16 + 0x30:16 + o_hs][:len(mip)]
                mip[:len(keep)] = keep
        struct.pack_into('<%dI' % nlev, mip, 0x10, *sizes)
        pic += bytes(mip)
    return bytes(filehdr) + pic, sizes


def _encode_tim2(img):
    old = img.tim2
    if img.no_image:
        if old is None:
            raise ValueError('no_image picture without a header')
        return old
    header = None
    reuse = False
    if old is not None:
        (o_tot, o_cs, o_isz, o_hs, o_cc, o_pf, o_mm, o_ct, o_it, o_w, o_h) = \
            struct.unpack_from('<IIIHHBBBBHH', old, 16)
        same = (o_it == IMAGE_TYPE[img.format] and (o_w, o_h) == (img.width, img.height)
                and o_mm == 1 + len(img.mips)
                and (not img.paletted or o_cc == len(img.palette)))
        if same:
            reuse = True
            header = old[:16 + o_hs]
            if o_mm > 1:
                sizes = list(struct.unpack_from('<%dI' % o_mm, old, 16 + 0x40))
            else:
                sizes = [o_isz]
            clut_base = old[16 + o_hs + o_isz:16 + o_hs + o_isz + o_cs]
    if header is None:
        header, sizes = _build_header(img, old)
        clut_base = None
    levels = [img.indices if img.paletted else img.pixels] + list(img.mips)
    out = [header]
    pos = len(header)
    for k, (lev, size) in enumerate(zip(levels, sizes)):
        lw, lh = max(img.width >> k, 1), max(img.height >> k, 1)
        base = old[pos:pos + size] if reuse else None
        enc = _encode_level(lev, img.format, lw, lh, img.swizzled, base)
        if len(enc) != size:
            enc = (enc + bytes(size))[:size]
        out.append(enc)
        pos += size
    if img.paletted:
        out.append(_encode_clut(img.palette, img.format, clut_base))
    return b''.join(out)


# --------------------------------------------------------------------------
# Public archive API


def read(data):
    """Every picture of an IMG/IM2/IM3 archive, in entry order.  Texture
    animation scripts ('#' entries) are not pictures and are skipped (write
    keeps them from `like`)."""
    data = bytes(data)
    kind, entries = _parse_archive(data)
    images = []
    for e in entries:
        if e.script:
            continue
        n = _tim2_extent(data, e.offset, e.no_image)
        img = _decode_tim2(data[e.offset:e.offset + n], e.swizzled, e.no_image)
        img.name = e.name.decode('latin-1')
        img.raw_name = e.raw[:0x20]
        img.index = e.index
        img.block = e.block
        img.clamp = e.clamp
        images.append(img)
    return images


def _name_field(img):
    name = img.name.encode('latin-1')
    if len(name) > 0x1F:
        raise ValueError('name %r longer than 31 bytes' % img.name)
    raw = img.raw_name
    if raw is not None and raw.split(b'\0', 1)[0] == name:
        return raw
    return name + bytes(0x20 - len(name))


def write(images, like=None):
    """Build an archive.  With `like` (the original archive bytes), pictures
    are matched to entries by Image.index and everything that is not a
    picture (scripts, entry fields, padding, stray bytes) is kept; pictures
    without an index are appended (IM3 only).  Without `like` a fresh IM3
    archive is built."""
    if like is None:
        return _write_fresh(images)
    like = bytes(like)
    kind, entries = _parse_archive(like)
    if kind is None:
        if images:
            return _write_fresh(images)
        return like
    by_index = {}
    new_images = []
    for img in images:
        if img.index is None:
            new_images.append(img)
        else:
            by_index[img.index] = img
    if new_images:
        if kind != 'IM3':
            raise ValueError('can only append pictures to IM3 archives')
        return _write_appended(images, like, entries)
    # Segments: data between consecutive distinct entry offsets.
    offsets = sorted(set(e.offset for e in entries if 0 <= e.offset <= len(like)))
    if not offsets:
        return like
    bounds = offsets + [len(like)]
    seg = {o: like[o:bounds[i + 1]] for i, o in enumerate(offsets)}
    new_seg = dict(seg)
    delta_of = {}
    for e in entries:
        img = by_index.get(e.index)
        if img is None or e.script:
            continue
        old_len = _tim2_extent(like, e.offset, e.no_image)
        tim = _encode_tim2(img)
        slack = seg[e.offset][old_len:]
        body = tim + slack
        if len(tim) != old_len and len(body) % 16:
            body += bytes(16 - len(body) % 16)
        new_seg[e.offset] = body
        delta_of[e.offset] = len(tim) - old_len
    prefix = bytearray(like[:offsets[0]])
    out_pos = {}
    pos = len(prefix)
    parts = [None]
    for o in offsets:
        out_pos[o] = pos
        parts.append(new_seg[o])
        pos += len(new_seg[o])
    for e in entries:
        if e.offset not in out_pos:
            continue
        name = None
        img = by_index.get(e.index)
        if img is not None:
            name = _name_field(img)
            prefix[e.pos:e.pos + 0x20] = name
        if kind == 'IM3':
            struct.pack_into('<i', prefix, e.pos + 0x24, out_pos[e.offset])
            if img is not None:
                struct.pack_into('<i', prefix, e.pos + 0x28, 1 if img.swizzled else 0)
                struct.pack_into('<i', prefix, e.pos + 0x2C, img.block)
                struct.pack_into('<Q', prefix, e.pos + 0x38, img.clamp)
                d = delta_of.get(e.offset, 0)
                if d:
                    struct.pack_into('<i', prefix, e.pos + 0x34, e.size + d)
        else:
            struct.pack_into('<i', prefix, e.pos + 0x20, out_pos[e.offset])
    parts[0] = bytes(prefix)
    return b''.join(parts)


def _entry_bytes(img, offset, size):
    return (_name_field(img) + struct.pack('<IiiihhiQ', 0x40, offset,
                                           1 if img.swizzled else 0, img.block,
                                           1 if img.no_image else 0, 0, size,
                                           img.clamp & 0xFFFFFFFFFFFFFFFF))


def _write_fresh(images):
    n = len(images)
    pos = 0x10 + IM3_ENTRY * n
    table, blobs = [], []
    for img in images:
        tim = _encode_tim2(img)
        table.append(_entry_bytes(img, pos, len(tim)))
        if len(tim) % 16:
            tim += bytes(16 - len(tim) % 16)
        blobs.append(tim)
        pos += len(tim)
    return b'IM3\0' + struct.pack('<III', 0x10, n, 0) + b''.join(table) + b''.join(blobs)


def _write_appended(images, like, entries):
    """IM3 with extra pictures: rewrite the existing archive, then grow the
    entry table and append the new pictures."""
    old = [i for i in images if i.index is not None]
    new = [i for i in images if i.index is None]
    body = write(old, like=like)
    n_old = len(entries)
    grow = IM3_ENTRY * len(new)
    table_end = 0x10 + IM3_ENTRY * n_old
    out = bytearray(body[:table_end])
    rest = body[table_end:]
    for k in range(n_old):
        p = 0x10 + IM3_ENTRY * k
        off = struct.unpack_from('<i', out, p + 0x24)[0]
        struct.pack_into('<i', out, p + 0x24, off + grow)
    pos = len(body) + grow
    if pos % 16:
        rest += bytes(16 - pos % 16)
        pos += 16 - pos % 16
    tail = []
    for img in new:
        tim = _encode_tim2(img)
        out += _entry_bytes(img, pos, len(tim))
        if len(tim) % 16:
            tim += bytes(16 - len(tim) % 16)
        tail.append(tim)
        pos += len(tim)
    struct.pack_into('<I', out, 8, n_old + len(new))
    return bytes(out) + rest + b''.join(tail)


# --------------------------------------------------------------------------
# Alpha


def alpha_mode(img):
    """How alpha maps to PNG for this picture:
      'ps2'    every alpha <= 0x80: PNG alpha = a * 255 / 128 (0x80 -> 255)
      'raw'    some alpha > 0x80: PNG alpha = a, unscaled
      'zero'   every alpha 0 (e.g. *_wrk render targets): PNG written opaque
      'opaque' PSMCT24, no alpha stored"""
    if img.format == 'PSMCT24':
        return 'opaque'
    if img.paletted:
        alphas = bytes(c[3] for c in img.palette)
    else:
        alphas = img.pixels[3::4]
    if not alphas:
        return 'ps2'
    top = max(alphas)
    if top > 0x80:
        return 'raw'
    if top == 0:
        return 'zero'
    return 'ps2'


def _png_alpha_table(mode):
    return _IDENT if mode == 'raw' else _A2P


def _ps2_alpha_table(mode):
    return _IDENT if mode == 'raw' else _P2A


# --------------------------------------------------------------------------
# PNG


def _chunk(tag, body):
    return (struct.pack('>I', len(body)) + tag + body
            + struct.pack('>I', zlib.crc32(tag + body) & 0xFFFFFFFF))


def write_png(width, height, mode, data, palette=None, text=None):
    """mode 'P' (data = indices, palette = list of (r,g,b,a) 0..255),
    'RGBA', or 'RGB' (data 3 bytes per pixel)."""
    if mode == 'P':
        ctype, bpp = 3, 1
    elif mode == 'RGBA':
        ctype, bpp = 6, 4
    elif mode == 'RGB':
        ctype, bpp = 2, 3
    else:
        raise ValueError(mode)
    stride = width * bpp
    raw = bytearray()
    for y in range(height):
        raw += b'\0'
        raw += data[y * stride:(y + 1) * stride]
    out = [b'\x89PNG\r\n\x1a\n',
           _chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, ctype, 0, 0, 0))]
    if mode == 'P':
        out.append(_chunk(b'PLTE', b''.join(bytes(c[:3]) for c in palette)))
        alphas = bytes(c[3] for c in palette)
        if any(a != 255 for a in alphas):
            out.append(_chunk(b'tRNS', alphas.rstrip(b'\xff') or alphas[:1]))
    for k, v in (text or {}).items():
        out.append(_chunk(b'tEXt', k.encode('latin-1') + b'\0' + v.encode('latin-1')))
    out.append(_chunk(b'IDAT', zlib.compress(bytes(raw), 6)))
    out.append(_chunk(b'IEND', b''))
    return b''.join(out)


def _unfilter(raw, width, height, bits_pp):
    """Undo PNG filtering of one (sub)image; returns rows (list of bytes)."""
    bpp = max(1, bits_pp // 8)
    stride = (width * bits_pp + 7) // 8
    rows = []
    prev = bytearray(stride)
    pos = 0
    for _ in range(height):
        ft = raw[pos]
        line = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        if ft == 0:
            pass
        elif ft == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif ft == 2:
            line = bytearray((a + b) & 255 for a, b in zip(line, prev))
        elif ft == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif ft == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                if pa <= pb and pa <= pc:
                    pr = a
                elif pb <= pc:
                    pr = b
                else:
                    pr = c
                line[i] = (line[i] + pr) & 255
        else:
            raise ValueError('bad PNG filter %d' % ft)
        rows.append(bytes(line))
        prev = line
    return rows, pos


def _samples(row, width, depth, channels):
    """Unpack one row into a list of samples (8-bit scaled for 16-bit)."""
    if depth == 8:
        return row[:width * channels]
    if depth == 16:
        return bytes(row[0:2 * width * channels:2])
    per = 8 // depth
    mask = (1 << depth) - 1
    out = bytearray()
    for b in row:
        for k in range(per):
            out.append((b >> (8 - depth * (k + 1))) & mask)
    return bytes(out[:width * channels])


def read_png(png):
    """Decode a PNG.  Returns (width, height, mode, data, palette): mode 'P'
    (data = indices, palette = list of (r,g,b,a)) or 'RGBA' (data = RGBA
    bytes, palette None).  Alpha is the PNG's own 0..255."""
    if png[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('not a PNG')
    pos = 8
    idat = []
    plte = trns = None
    hdr = None
    while pos < len(png):
        n, tag = struct.unpack_from('>I4s', png, pos)
        body = png[pos + 8:pos + 8 + n]
        pos += 12 + n
        if tag == b'IHDR':
            hdr = struct.unpack('>IIBBBBB', body)
        elif tag == b'PLTE':
            plte = body
        elif tag == b'tRNS':
            trns = body
        elif tag == b'IDAT':
            idat.append(body)
        elif tag == b'IEND':
            break
    w, h, depth, ctype, _, _, interlace = hdr
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ctype]
    bits_pp = depth * channels
    raw = zlib.decompress(b''.join(idat))
    full = [[None] * w for _ in range(h)] if interlace else None
    if not interlace:
        rows, _ = _unfilter(raw, w, h, bits_pp)
        samples = b''.join(_samples(r, w, depth, channels) for r in rows)
    else:
        passes = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4),
                  (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]
        buf = bytearray(w * h * channels)
        p = 0
        for x0, y0, dx, dy in passes:
            pw = (w - x0 + dx - 1) // dx if w > x0 else 0
            ph = (h - y0 + dy - 1) // dy if h > y0 else 0
            if not pw or not ph:
                continue
            rows, used = _unfilter(raw[p:], pw, ph, bits_pp)
            p += used
            for j, r in enumerate(rows):
                s = _samples(r, pw, depth, channels)
                y = y0 + j * dy
                for i in range(pw):
                    x = x0 + i * dx
                    buf[(y * w + x) * channels:(y * w + x + 1) * channels] = \
                        s[i * channels:(i + 1) * channels]
        samples = bytes(buf)
    n = w * h
    if ctype == 3:
        pal = [tuple(plte[3 * i:3 * i + 3]) + (255,) for i in range(len(plte) // 3)]
        if trns:
            for i, a in enumerate(trns[:len(pal)]):
                pal[i] = pal[i][:3] + (a,)
        return w, h, 'P', samples, pal
    out = bytearray(4 * n)
    if ctype == 6:
        out[:] = samples
    elif ctype == 2:
        out[0::4] = samples[0::3]
        out[1::4] = samples[1::3]
        out[2::4] = samples[2::3]
        out[3::4] = b'\xff' * n
        if trns and len(trns) >= 6:
            key = bytes(trns[1::2][:3]) if depth == 16 else bytes(trns[1:6:2])
            for i in range(n):
                if out[4 * i:4 * i + 3] == key:
                    out[4 * i + 3] = 0
    elif ctype in (0, 4):
        g = samples[0::channels]
        if depth < 8:
            scale = 255 // ((1 << depth) - 1)
            g = bytes(v * scale for v in g)
        out[0::4] = g
        out[1::4] = g
        out[2::4] = g
        if ctype == 4:
            out[3::4] = samples[1::2]
        else:
            out[3::4] = b'\xff' * n
            if trns and len(trns) >= 2:
                key = trns[1] if depth >= 8 else trns[1] * (255 // ((1 << depth) - 1))
                for i in range(n):
                    if g[i] == key:
                        out[4 * i + 3] = 0
    return w, h, 'RGBA', bytes(out), None


# --------------------------------------------------------------------------
# Image <-> PNG


def to_png(img, indexed=True):
    """The picture's base level as PNG bytes.  Paletted pictures become
    indexed PNGs (palette in index order) unless indexed=False."""
    if img.no_image:
        raise ValueError('%s has no pixels (no_image)' % img.name)
    mode = alpha_mode(img)
    text = {'Software': 'im3.py', 'im3:name': img.name, 'im3:format': img.format,
            'im3:alpha': mode}
    table = _png_alpha_table(mode)
    if img.paletted and indexed:
        pal = []
        for r, g, b, a in img.palette:
            pal.append((r, g, b, 255 if mode in ('zero', 'opaque') else table[a]))
        return write_png(img.width, img.height, 'P', img.indices, pal, text)
    rgba = bytearray(img.rgba)
    if mode in ('zero', 'opaque'):
        rgb = bytearray(3 * img.width * img.height)
        rgb[0::3] = rgba[0::4]
        rgb[1::3] = rgba[1::4]
        rgb[2::3] = rgba[2::4]
        return write_png(img.width, img.height, 'RGB', bytes(rgb), text=text)
    rgba[3::4] = bytes(rgba[3::4]).translate(table)
    return write_png(img.width, img.height, 'RGBA', bytes(rgba), text=text)


def _to_ps2_alpha(rgba, mode, fixed_alpha=None):
    out = bytearray(rgba)
    if mode == 'opaque':
        out[3::4] = b'\x80' * (len(out) // 4)
    elif mode == 'zero' and all(a == 255 for a in out[3::4]):
        out[3::4] = bytes(len(out) // 4)
    else:
        out[3::4] = bytes(out[3::4]).translate(_ps2_alpha_table(mode))
    return bytes(out)


def _median_cut(colours, n):
    """colours: dict (r,g,b,a) -> count.  Returns up to n representative
    colours."""
    boxes = [list(colours.items())]
    while len(boxes) < n:
        best, best_i, best_axis = -1, None, 0
        for i, box in enumerate(boxes):
            if len(box) < 2:
                continue
            for axis in range(4):
                vals = [c[axis] for c, _ in box]
                spread = max(vals) - min(vals)
                if spread > best:
                    best, best_i, best_axis = spread, i, axis
        if best_i is None or best <= 0:
            break
        box = sorted(boxes.pop(best_i), key=lambda cw: cw[0][best_axis])
        total = sum(w for _, w in box)
        acc, cut = 0, 1
        for k, (_, w) in enumerate(box):
            acc += w
            if acc >= total / 2:
                cut = min(max(k + 1, 1), len(box) - 1)
                break
        boxes.append(box[:cut])
        boxes.append(box[cut:])
    pal = []
    for box in boxes:
        total = sum(w for _, w in box) or 1
        pal.append(tuple(int(round(sum(c[a] * w for c, w in box) / total)) for a in range(4)))
    return pal


def _nearest_mapper(palette):
    cache = {}

    def find(c):
        got = cache.get(c)
        if got is None:
            r, g, b, a = c
            best, got = None, 0
            for i, (pr, pg, pb, pa) in enumerate(palette):
                d = (pr - r) ** 2 + (pg - g) ** 2 + (pb - b) ** 2 + 2 * (pa - a) ** 2
                if best is None or d < best:
                    best, got = d, i
                    if d == 0:
                        break
            cache[c] = got
        return got
    return find


def _quantise(rgba, n):
    """RGBA bytes (PS2 alpha) -> (indices, palette of n entries)."""
    pix = [tuple(rgba[i:i + 4]) for i in range(0, len(rgba), 4)]
    hist = {}
    for c in pix:
        hist[c] = hist.get(c, 0) + 1
    if len(hist) <= n:
        pal = sorted(hist)
    else:
        if len(hist) > 32768:
            coarse = {}
            for c, w in hist.items():
                k = (c[0] >> 3, c[1] >> 3, c[2] >> 3, c[3] >> 3)
                s = coarse.setdefault(k, [0, 0, 0, 0, 0])
                s[0] += c[0] * w; s[1] += c[1] * w; s[2] += c[2] * w; s[3] += c[3] * w
                s[4] += w
            hist2 = {}
            for s in coarse.values():
                c = (s[0] // s[4], s[1] // s[4], s[2] // s[4], s[3] // s[4])
                hist2[c] = hist2.get(c, 0) + s[4]
            pal = _median_cut(hist2, n)
        else:
            pal = _median_cut(hist, n)
    find = _nearest_mapper(pal)
    idx = bytes(find(c) for c in pix)
    pal = list(pal) + [(0, 0, 0, 0)] * (n - len(pal))
    return idx, pal


def _downsample(rgba, w, h):
    nw, nh = max(w // 2, 1), max(h // 2, 1)
    out = bytearray(4 * nw * nh)
    for y in range(nh):
        for x in range(nw):
            for c in range(4):
                s = 0
                for dy in (0, 1):
                    for dx in (0, 1):
                        sx, sy = min(2 * x + dx, w - 1), min(2 * y + dy, h - 1)
                        s += rgba[4 * (sy * w + sx) + c]
                out[4 * (y * nw + x) + c] = (s + 2) // 4
    return bytes(out), nw, nh


def from_png(png, like):
    """A picture made from PNG bytes, taking name, format, palette size,
    flags and header from `like` (normally the picture the PNG was exported
    from).  Paletted targets: an indexed PNG with at most as many colours is
    taken as is; otherwise colours are matched to like's palette, and only
    if that fails quantised (median cut) to a new palette."""
    w, h, kind, data, pal = read_png(png)
    img = like.copy()
    img.width, img.height = w, h
    mode = alpha_mode(like)
    same_size = (w, h) == (like.width, like.height)
    if like.paletted:
        n = len(like.palette)
        if kind == 'P' and len(pal) <= n and max(data, default=0) < len(pal):
            opaque_png = all(c[3] == 255 for c in pal)
            palette = []
            for r, g, b, a in pal:
                if mode == 'opaque':
                    a2 = 0x80
                elif mode == 'zero' and opaque_png:
                    a2 = 0
                else:
                    a2 = _ps2_alpha_table(mode)[a]
                palette.append((r, g, b, a2))
            palette += like.palette[len(palette):]
            img.palette = palette
            img.indices = bytes(data)
        else:
            if kind == 'P':
                table = [bytes(c) for c in pal] + [b'\0\0\0\xff'] * (256 - len(pal))
                data = b''.join(table[i] for i in data)
            rgba = _to_ps2_alpha(data, mode)
            lookup = {}
            for i, c in enumerate(like.palette):
                lookup.setdefault(tuple(c), []).append(i)
            idx = bytearray(w * h)
            ok = True
            old = like.indices if (same_size and like.indices is not None) else None
            for p in range(w * h):
                c = tuple(rgba[4 * p:4 * p + 4])
                cand = lookup.get(c)
                if cand is None:
                    ok = False
                    break
                if old is not None and old[p] in cand:
                    idx[p] = old[p]
                else:
                    idx[p] = cand[0]
            if ok:
                img.palette = list(like.palette)
                img.indices = bytes(idx)
            else:
                img.indices, img.palette = _quantise(rgba, n)
        img.pixels = None
        unchanged = (same_size and img.indices == like.indices
                     and img.palette == like.palette)
    else:
        if kind == 'P':
            table = [bytes(c) for c in pal] + [b'\0\0\0\xff'] * (256 - len(pal))
            data = b''.join(table[i] for i in data)
        img.pixels = _to_ps2_alpha(data, mode)
        img.indices = img.palette = None
        unchanged = same_size and img.pixels == like.pixels
    if like.mips and not unchanged:
        img.mips = []
        cur, cw, ch = img.rgba, w, h
        find = _nearest_mapper(img.palette) if img.paletted else None
        for _ in like.mips:
            cur, cw, ch = _downsample(cur, cw, ch)
            if img.paletted:
                img.mips.append(bytes(find(tuple(cur[i:i + 4])) for i in range(0, len(cur), 4)))
            else:
                img.mips.append(cur)
    elif unchanged:
        img.mips = list(like.mips)
    return img
