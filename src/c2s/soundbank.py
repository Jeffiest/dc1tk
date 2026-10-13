"""Dark Chronicle (Dark Cloud 2, PS2) sound banks.

A bank is a `.snd` pack (the game's chain-of-members format) holding:

- `NAME.hd`   Sony hardware-synth header ("SCEI" chunks Vers, Head, Vagi,
              Smpl, Sset, Prog): programs -> key splits -> sample sets ->
              samples -> VAG infos (offset into the .bd, sample rate, loop).
- `NAME.bd`   the VAG bodies, PS2 SPU2 4-bit ADPCM, back to back.
- `NAME.cfg`  the game's sound-effect table (text, cp932): row n is
              play_se entry n and says KeyOn prog/key, a .sq or a .mid.
- `NAME.vol`  default volume per entry, optional REVERB line.
- `*.sq`      Sony sequences (music, ambient loops), kept as bytes.
- `*.mid`     SMF format 0 sound-effect sequences played by the EE.

Standard library only.  See soundbank.md for the format.

    bank = Bank.from_snd("SP_000.snd")
    for e in bank.entries(): print(e.index, e.name, e.kind, bank.voices(e))
    bank.replace_entry(0, read_wav("new.wav"))
    n = bank.add_entry(read_wav("extra.wav"), name="MY_SOUND")
    open("SP_000.snd", "wb").write(bank.build())
"""

import math
import struct
import sys

# ---------------------------------------------------------------------------
# Pack (.snd container)
# ---------------------------------------------------------------------------

PACK_NAME = 64
PACK_SLOT = 0x50
PACK_ALIGN = 16


class Member:
    """One pack member: its name, the raw 64-byte name field (which holds
    stale bytes after the terminating NUL on the disc), the slot's unused
    fourth word (same leftover value in every slot of a pack; 0 for new
    members, as the project's pack writer does) and its data."""

    def __init__(self, name, data, raw_name=None, slot_tail=bytes(4)):
        self.name = name
        self.data = bytes(data)
        self.slot_tail = bytes(slot_tail)
        if raw_name is None:
            enc = name.encode("cp932")
            if len(enc) >= PACK_NAME:
                raise ValueError("member name too long: %r" % name)
            raw_name = enc + bytes(PACK_NAME - len(enc))
        self.raw_name = bytes(raw_name)

    @property
    def ext(self):
        return self.name.rsplit(".", 1)[-1].lower() if "." in self.name else ""

    def __repr__(self):
        return "Member(%r, %d bytes)" % (self.name, len(self.data))


class Pack:
    """A chain of 0x50-byte slots (64-byte name, data offset 0x50, size,
    offset of the next slot, an unused word), each followed by its data padded to 16 with
    zeros, ended by a slot whose name starts with NUL.  The end slot holds
    leftover bytes on the disc; they are kept for byte-exact rewriting."""

    def __init__(self, members, terminator=None):
        self.members = members
        if terminator is None:
            terminator = bytes(PACK_NAME) + struct.pack("<iii", PACK_SLOT, -1, -1) + bytes(4)
        self.terminator = bytes(terminator)

    @classmethod
    def parse(cls, data):
        members = []
        off = 0
        while True:
            if off + PACK_SLOT > len(data):
                raise ValueError("pack ends without an end slot")
            if data[off] == 0:
                if off + PACK_SLOT != len(data):
                    raise ValueError("bytes after the pack's end slot")
                return cls(members, data[off:off + PACK_SLOT])
            raw = data[off:off + PACK_NAME]
            name = raw[:raw.index(0)].decode("cp932")
            doff, size, nxt = struct.unpack_from("<iii", data, off + PACK_NAME)
            if doff != PACK_SLOT or nxt != PACK_SLOT + size + (-size % PACK_ALIGN):
                raise ValueError("bad pack slot at 0x%x" % off)
            start = off + doff
            pad = data[start + size:start + size + (-size % PACK_ALIGN)]
            if any(pad):
                raise ValueError("non-zero member padding at 0x%x" % (start + size))
            members.append(Member(name, data[start:start + size], raw, data[off + 76:off + 80]))
            off += nxt

    def build(self):
        out = bytearray()
        for m in self.members:
            size = len(m.data)
            pad = -size % PACK_ALIGN
            out += m.raw_name + struct.pack("<iii", PACK_SLOT, size, PACK_SLOT + size + pad) + m.slot_tail
            out += m.data + bytes(pad)
        out += self.terminator
        return bytes(out)

    def find(self, ext):
        """Members with an extension, in pack order (GetPackFileExt matches
        extensions ignoring case)."""
        return [m for m in self.members if m.ext == ext.lower()]


# ---------------------------------------------------------------------------
# .hd records
# ---------------------------------------------------------------------------

class Record:
    """A fixed little-endian struct with named fields."""
    FIELDS = ()

    def __init__(self, **kw):
        for name, _ in self.FIELDS:
            setattr(self, name, kw.pop(name, 0))
        if kw:
            raise TypeError("unknown fields %s" % sorted(kw))

    @classmethod
    def fmt(cls):
        return "<" + "".join(f for _, f in cls.FIELDS)

    @classmethod
    def size(cls):
        return struct.calcsize(cls.fmt())

    @classmethod
    def unpack(cls, data, off=0):
        vals = struct.unpack_from(cls.fmt(), data, off)
        return cls(**dict(zip((n for n, _ in cls.FIELDS), vals)))

    def pack(self):
        return struct.pack(self.fmt(), *(getattr(self, n) for n, _ in self.FIELDS))

    def copy(self, **changes):
        new = self.__class__(**{n: getattr(self, n) for n, _ in self.FIELDS})
        for k, v in changes.items():
            if not hasattr(new, k):
                raise AttributeError(k)
            setattr(new, k, v)
        return new

    def as_dict(self):
        return {n: getattr(self, n) for n, _ in self.FIELDS}

    def __eq__(self, other):
        return type(self) is type(other) and self.as_dict() == other.as_dict()

    def __repr__(self):
        return "%s(%s)" % (self.__class__.__name__,
                           ", ".join("%s=%r" % kv for kv in self.as_dict().items()))


class VagInfo(Record):
    """Vagi record, 8 bytes."""
    FIELDS = (("offset", "I"),       # byte offset of the VAG in the .bd
              ("rate", "H"),         # sample rate in Hz (pitch at base note)
              ("attribute", "B"),    # 1 = the VAG loops, 0 = one-shot
              ("reserved", "B"))     # always 0xFF


class Sample(Record):
    """Smpl record (sceHardSynthSampleParam), 0x2A bytes.  vag_index 0xFFFF
    means no VAG."""
    FIELDS = (("vag_index", "H"),
              ("vel_range_low", "B"), ("vel_cross_fade", "B"), ("vel_range_high", "B"),
              ("vel_follow_pitch", "b"), ("vel_follow_pitch_center", "B"), ("vel_follow_pitch_curve", "B"),
              ("vel_follow_amp", "b"), ("vel_follow_amp_center", "B"), ("vel_follow_amp_curve", "B"),
              ("base_note", "B"), ("detune", "b"), ("panpot", "B"),
              ("group", "B"), ("priority", "B"), ("volume", "B"), ("reserved", "B"),
              ("adsr1", "H"), ("adsr2", "H"),
              ("key_follow_ar", "b"), ("key_follow_ar_center", "B"),
              ("key_follow_dr", "b"), ("key_follow_dr_center", "B"),
              ("key_follow_sr", "b"), ("key_follow_sr_center", "B"),
              ("key_follow_rr", "b"), ("key_follow_rr_center", "B"),
              ("key_follow_sl", "b"), ("key_follow_sl_center", "B"),
              ("pitch_lfo_delay", "H"), ("pitch_lfo_fade", "H"),
              ("amp_lfo_delay", "H"), ("amp_lfo_fade", "H"),
              ("lfo_attribute", "B"), ("spu_attribute", "B"))


class Split(Record):
    """Split block of a program (sceHardSynthSplitBlock), 0x14 bytes."""
    FIELDS = (("sample_set", "H"),
              ("range_low", "B"), ("cross_fade", "B"), ("range_high", "B"), ("number", "B"),
              ("bend_range_low", "H"), ("bend_range_high", "H"),
              ("key_follow_pitch", "b"), ("key_follow_pitch_center", "B"),
              ("key_follow_amp", "b"), ("key_follow_amp_center", "B"),
              ("key_follow_pan", "b"), ("key_follow_pan_center", "B"),
              ("volume", "B"), ("panpot", "B"), ("transpose", "b"), ("detune", "b"))


class Program(Record):
    """Prog record (sceHardSynthProgramParam), 0x24 bytes, followed by
    n_split split blocks of split_block_size bytes at +split_block_addr."""
    FIELDS = (("split_block_addr", "I"), ("n_split", "B"), ("split_block_size", "B"),
              ("volume", "B"), ("panpot", "B"), ("transpose", "b"), ("detune", "b"),
              ("key_follow_pan", "b"), ("key_follow_pan_center", "B"),
              ("attribute", "B"), ("reserved", "B"),
              ("lfo_wave", "B"), ("lfo_wave2", "B"),
              ("lfo_start_phase", "B"), ("lfo_start_phase2", "B"),
              ("lfo_phase_random", "B"), ("lfo_phase_random2", "B"),
              ("lfo_cycle", "H"), ("lfo_cycle2", "H"),
              ("lfo_pitch_depth", "h"), ("lfo_pitch_depth2", "h"),
              ("lfo_midi_pitch_depth", "h"), ("lfo_midi_pitch_depth2", "h"),
              ("lfo_amp_depth", "b"), ("lfo_amp_depth2", "b"),
              ("lfo_midi_amp_depth", "b"), ("lfo_midi_amp_depth2", "b"))

    def __init__(self, splits=None, **kw):
        super().__init__(**kw)
        self.splits = list(splits or [])

    def copy(self, **changes):
        splits = changes.pop("splits", [s.copy() for s in self.splits])
        new = super().copy(**changes)
        new.splits = splits
        return new

    def __eq__(self, other):
        return super().__eq__(other) and self.splits == other.splits


class SampleSet:
    """Sset record: vel_curve, vel_limit_low, vel_limit_high, count, then
    count u16 sample indices (4 + 2n bytes, not padded)."""

    def __init__(self, samples, vel_curve=0, vel_limit_low=1, vel_limit_high=127):
        self.vel_curve = vel_curve
        self.vel_limit_low = vel_limit_low
        self.vel_limit_high = vel_limit_high
        self.samples = list(samples)

    @classmethod
    def unpack(cls, data, off):
        curve, low, high, n = struct.unpack_from("<BBBB", data, off)
        return cls(struct.unpack_from("<%dH" % n, data, off + 4), curve, low, high)

    def pack(self):
        return struct.pack("<BBBB%dH" % len(self.samples), self.vel_curve, self.vel_limit_low,
                           self.vel_limit_high, len(self.samples), *self.samples)

    def copy(self):
        return SampleSet(self.samples, self.vel_curve, self.vel_limit_low, self.vel_limit_high)

    def __eq__(self, other):
        return isinstance(other, SampleSet) and self.pack() == other.pack()

    def __repr__(self):
        return "SampleSet(%r, curve=%d, vel=%d..%d)" % (self.samples, self.vel_curve,
                                                       self.vel_limit_low, self.vel_limit_high)


# ---------------------------------------------------------------------------
# .hd file
# ---------------------------------------------------------------------------

CREATOR = b"IECS"          # "SCEI" stored little-endian
NONE = 0xFFFFFFFF
CHUNK_ORDER = ("Vers", "Head", "Vagi", "Smpl", "Sset", "Prog")


def _tag(name):
    return name[::-1].encode("ascii")


def _chunks(data):
    out = []
    off = 0
    while off < len(data):
        creator, kind, size = struct.unpack_from("<4s4sI", data, off)
        if creator != CREATOR or size < 12 or off + size > len(data):
            raise ValueError("bad chunk at 0x%x" % off)
        out.append((kind[::-1].decode("ascii"), off, size))
        off += size
    return out


def _table(data, off):
    count = struct.unpack_from("<I", data, off + 12)[0] + 1
    return list(struct.unpack_from("<%dI" % count, data, off + 16))


def _table_chunk(name, records):
    """Chunk header, max index, offset table, records back to back in index
    order (absent ones NONE), 0xFF padding to 16 bytes."""
    n = len(records)
    if n == 0:
        raise ValueError("%s table cannot be empty" % name)
    head = 16 + 4 * n
    offsets = []
    body = bytearray()
    for rec in records:
        if rec is None:
            offsets.append(NONE)
        else:
            offsets.append(head + len(body))
            body += rec
    size = head + len(body)
    pad = -size % 16
    return (CREATOR + _tag(name) + struct.pack("<II", size + pad, n - 1)
            + struct.pack("<%dI" % n, *offsets) + body + b"\xff" * pad)


class HD:
    def __init__(self):
        self.version = b"\x00\x00\x02\x00"     # u16 reserved, u8 major, u8 minor
        self.se_timbre_addr = NONE
        self.head_reserved = b"\xff" * 24
        self.vags = []        # VagInfo
        self.samples = []     # Sample or None
        self.sample_sets = []  # SampleSet or None
        self.programs = []    # Program or None

    @classmethod
    def parse(cls, data):
        hd = cls()
        chunks = _chunks(data)
        names = tuple(c[0] for c in chunks)
        if names != CHUNK_ORDER:
            raise ValueError("unexpected chunk order %s" % (names,))
        pos = {n: o for n, o, _ in chunks}
        hd.version = data[12:16]
        h = pos["Head"]
        (hsize, bsize, prog, sset, smpl, vagi, timbre) = struct.unpack_from("<7I", data, h + 12)
        if hsize != len(data) or (prog, sset, smpl, vagi) != (pos["Prog"], pos["Sset"], pos["Smpl"], pos["Vagi"]):
            raise ValueError("Head chunk does not match the file")
        hd.body_size = bsize
        hd.se_timbre_addr = timbre
        hd.head_reserved = data[h + 40:h + 64]
        o = pos["Vagi"]
        hd.vags = [VagInfo.unpack(data, o + x) for x in _table(data, o)]
        o = pos["Smpl"]
        hd.samples = [None if x == NONE else Sample.unpack(data, o + x) for x in _table(data, o)]
        o = pos["Sset"]
        hd.sample_sets = [None if x == NONE else SampleSet.unpack(data, o + x) for x in _table(data, o)]
        o = pos["Prog"]
        hd.programs = []
        for x in _table(data, o):
            if x == NONE:
                hd.programs.append(None)
                continue
            p = Program.unpack(data, o + x)
            base = o + x + p.split_block_addr
            p.splits = [Split.unpack(data, base + i * p.split_block_size) for i in range(p.n_split)]
            hd.programs.append(p)
        return hd

    def build(self, body_size):
        vers = CREATOR + _tag("Vers") + struct.pack("<I", 16) + self.version
        vagi = _table_chunk("Vagi", [v.pack() for v in self.vags])
        smpl = _table_chunk("Smpl", [None if s is None else s.pack() for s in self.samples])
        sset = _table_chunk("Sset", [None if s is None else s.pack() for s in self.sample_sets])
        progs = []
        for p in self.programs:
            if p is None:
                progs.append(None)
                continue
            if p.split_block_addr != Program.size() or p.split_block_size != Split.size():
                raise ValueError("only programs with splits right after them can be written")
            p.n_split = len(p.splits)
            progs.append(p.pack() + b"".join(s.pack() for s in p.splits))
        prog = _table_chunk("Prog", progs)
        o_vagi = 16 + 64
        o_smpl = o_vagi + len(vagi)
        o_sset = o_smpl + len(smpl)
        o_prog = o_sset + len(sset)
        total = o_prog + len(prog)
        head = (CREATOR + _tag("Head") + struct.pack("<I", 64)
                + struct.pack("<7I", total, body_size, o_prog, o_sset, o_smpl, o_vagi, self.se_timbre_addr)
                + self.head_reserved)
        return vers + head + vagi + smpl + sset + prog


# ---------------------------------------------------------------------------
# PS2 SPU2 ADPCM
# ---------------------------------------------------------------------------

BLOCK = 16
BLOCK_SAMPLES = 28
COEFS = ((0, 0), (60, 0), (115, -52), (98, -55), (122, -60))
FLAG_END = 1       # last block: stop (or jump to the loop address when FLAG_REPEAT)
FLAG_REPEAT = 2    # with FLAG_END: jump back to the loop address and keep playing
FLAG_START = 4     # sets the loop address to this block
SILENT_TAIL = b"\x00\x07" + b"\x77" * 14   # block after a one-shot's end block
ZERO_BLOCK = bytes(16)                      # first block of every VAG


class Sound:
    """Mono 16-bit PCM: rate in Hz, samples as ints, loop_start (sample index
    the sound loops back to after its last sample) or None."""

    def __init__(self, rate, samples, loop_start=None):
        self.rate = int(rate)
        self.samples = list(samples)
        self.loop_start = loop_start

    def __repr__(self):
        return "Sound(%d Hz, %d samples, loop=%r)" % (self.rate, len(self.samples), self.loop_start)


def decode_adpcm(data, skip_zero_block=True):
    """Decode a VAG body.  Returns (samples, loop_start or None).  Decoding
    stops after the first block with FLAG_END; the leading all-zero block
    (28 samples of silence on the SPU) is skipped when asked."""
    out = []
    h1 = h2 = 0
    loop_start = None
    looped = False
    start = 1 if skip_zero_block and data[:16] == ZERO_BLOCK else 0
    for b in range(start * BLOCK, len(data) - BLOCK + 1, BLOCK):
        head = data[b]
        flags = data[b + 1]
        shift = head & 15
        filt = head >> 4
        if filt > 4:
            raise ValueError("ADPCM filter %d at 0x%x" % (filt, b))
        if shift > 12:
            shift = 9          # the SPU treats shifts 13..15 as 9 (psx-spx); never on this disc
        c1, c2 = COEFS[filt]
        if flags & FLAG_START:
            loop_start = len(out)
        for i in range(2, 16):
            byte = data[b + i]
            for nib in (byte & 15, byte >> 4):
                if nib >= 8:
                    nib -= 16
                s = ((nib << 12) >> shift) + ((h1 * c1 + h2 * c2 + 32) >> 6)
                if s > 32767:
                    s = 32767
                elif s < -32768:
                    s = -32768
                out.append(s)
                h2 = h1
                h1 = s
        if flags & FLAG_END:
            looped = bool(flags & FLAG_REPEAT)
            break
    return out, (loop_start if looped else None)


def _quantize(block, h1, h2, c1, c2, step, want_nibbles):
    err = 0
    nibs = [] if want_nibbles else None
    for x in block:
        pred = (h1 * c1 + h2 * c2 + 32) >> 6
        q = (x - pred + (step >> 1)) // step
        if q > 7:
            q = 7
        elif q < -8:
            q = -8
        y = q * step + pred
        if y > 32767:
            y = 32767
        elif y < -32768:
            y = -32768
        d = x - y
        err += d * d
        h2 = h1
        h1 = y
        if want_nibbles:
            nibs.append(q & 15)
    return err, nibs, h1, h2


def _encode_block(block, h1, h2, full=True):
    """Pick filter and shift for 28 samples by closed-loop squared error.
    full: all 5 filters x 13 shifts.  Otherwise, for each filter, the shift
    from the open-loop residual peak and one more bit of range."""
    best = None
    for f, (c1, c2) in enumerate(COEFS):
        if full:
            ks = range(13)
        else:
            p1, p2, peak = h1, h2, 0
            for x in block:
                r = x - ((p1 * c1 + p2 * c2 + 32) >> 6)
                if r < 0:
                    r = -r
                if r > peak:
                    peak = r
                p2 = p1
                p1 = x
            k = 0
            while k < 12 and (7 << k) < peak:
                k += 1
            ks = (k, k + 1) if k < 12 else (k,)
        for kk in ks:
            err = _quantize(block, h1, h2, c1, c2, 1 << kk, False)[0]
            if best is None or err < best[0]:
                best = (err, f, kk)
        if best[0] == 0:
            break
    _, f, k = best
    c1, c2 = COEFS[f]
    _, nibs, h1, h2 = _quantize(block, h1, h2, c1, c2, 1 << k, True)
    data = bytes([(f << 4) | (12 - k), 0]) + bytes(nibs[i] | (nibs[i + 1] << 4) for i in range(0, 28, 2))
    return data, h1, h2


def encode_adpcm(samples, loop_start=None, full=True):
    """Encode PCM (ints) to a VAG body in the layout every VAG of the game
    has.  One-shot: zero block (flags 0), first block flags 4, last flags 1,
    then the 00 07 77.. block.  Looped: zero block, every block flags 2, the
    loop block 6, the last 3.  The length is padded to 28 samples with
    zeros; loop_start must be a multiple of 28 and the loop must end at a
    block end (prepare_loop does both).  full=False searches fewer shifts
    (about 4x faster, a few dB worse)."""
    samples = [max(-32768, min(32767, int(s))) for s in samples]
    if not samples:
        samples = [0]
    samples += [0] * (-len(samples) % BLOCK_SAMPLES)
    nblocks = len(samples) // BLOCK_SAMPLES
    if loop_start is not None:
        if loop_start % BLOCK_SAMPLES or not 0 <= loop_start < len(samples):
            raise ValueError("loop start must be a block start inside the sound")
        loop_block = loop_start // BLOCK_SAMPLES
    out = bytearray(ZERO_BLOCK)
    h1 = h2 = 0
    for i in range(nblocks):
        blk, h1, h2 = _encode_block(samples[i * 28:(i + 1) * 28], h1, h2, full)
        blk = bytearray(blk)
        if loop_start is None:
            flags = (FLAG_START if i == 0 else 0) | (FLAG_END if i == nblocks - 1 else 0)
        else:
            flags = FLAG_REPEAT | (FLAG_START if i == loop_block else 0) | (FLAG_END if i == nblocks - 1 else 0)
        blk[1] = flags
        out += blk
    if loop_start is None:
        out += SILENT_TAIL
    return bytes(out)


def vag_length(data):
    """Bytes of a VAG up to and including its end block (and the silent
    block after a one-shot's end block, when present)."""
    for b in range(0, len(data), BLOCK):
        if data[b + 1] & FLAG_END:
            end = b + BLOCK
            if not data[b + 1] & FLAG_REPEAT and data[end:end + BLOCK] == SILENT_TAIL:
                end += BLOCK
            return end
    return len(data)


# ---------------------------------------------------------------------------
# PCM helpers: WAV, resampling, loops, SNR
# ---------------------------------------------------------------------------

def read_wav(source):
    """Read a WAV (path or bytes): PCM 8/16/24/32-bit, float 32/64, also
    WAVE_FORMAT_EXTENSIBLE; any channel count is averaged to mono.  A 'smpl'
    chunk's first loop gives loop_start (audio after its end is dropped)."""
    data = source if isinstance(source, (bytes, bytearray)) else open(source, "rb").read()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("not a WAV file")
    fmt = None
    pcm = None
    loop = None
    off = 12
    while off + 8 <= len(data):
        cid, size = struct.unpack_from("<4sI", data, off)
        body = data[off + 8:off + 8 + size]
        if cid == b"fmt ":
            tag, channels, rate, _, align, bits = struct.unpack_from("<HHIIHH", body)
            if tag == 0xFFFE and len(body) >= 26:
                tag = struct.unpack_from("<H", body, 24)[0]
            fmt = (tag, channels, rate, align, bits)
        elif cid == b"data":
            pcm = body
        elif cid == b"smpl" and len(body) >= 36:
            if struct.unpack_from("<I", body, 28)[0] >= 1 and len(body) >= 60:
                _, _, start, end, _, _ = struct.unpack_from("<6I", body, 36)
                loop = (start, end)
        off += 8 + size + (size & 1)
    if fmt is None or pcm is None:
        raise ValueError("WAV without fmt or data chunk")
    tag, channels, rate, align, bits = fmt
    width = bits // 8
    if align != width * channels:
        raise ValueError("unsupported WAV block alignment")
    frames = len(pcm) // align
    if tag == 1 and bits == 8:
        vals = [(b - 128) * 256.0 for b in pcm[:frames * align]]
    elif tag == 1 and bits == 16:
        vals = [float(v) for v in struct.unpack("<%dh" % (frames * channels), pcm[:frames * align])]
    elif tag == 1 and bits == 24:
        vals = [int.from_bytes(pcm[i:i + 3], "little", signed=True) / 256.0
                for i in range(0, frames * align, 3)]
    elif tag == 1 and bits == 32:
        vals = [v / 65536.0 for v in struct.unpack("<%di" % (frames * channels), pcm[:frames * align])]
    elif tag == 3 and bits in (32, 64):
        vals = [v * 32768.0 for v in struct.unpack("<%d%s" % (frames * channels, "f" if bits == 32 else "d"),
                                                  pcm[:frames * align])]
    else:
        raise ValueError("unsupported WAV format %d with %d bits" % (tag, bits))
    if channels > 1:
        vals = [sum(vals[i:i + channels]) / channels for i in range(0, len(vals), channels)]
    samples = [max(-32768, min(32767, int(round(v)))) for v in vals]
    loop_start = None
    if loop is not None and loop[0] < len(samples):
        loop_start = loop[0]
        samples = samples[:min(len(samples), loop[1] + 1)]
    return Sound(rate, samples, loop_start)


def wav_bytes(sound):
    """16-bit mono WAV, with a 'smpl' loop when the sound loops."""
    body = struct.pack("<%dh" % len(sound.samples), *sound.samples)
    fmt = struct.pack("<HHIIHH", 1, 1, sound.rate, sound.rate * 2, 2, 16)
    chunks = b"fmt " + struct.pack("<I", 16) + fmt
    chunks += b"data" + struct.pack("<I", len(body)) + body + (b"\x00" if len(body) & 1 else b"")
    if sound.loop_start is not None:
        smpl = struct.pack("<9I", 0, 0, int(round(1e9 / sound.rate)), 60, 0, 0, 0, 1, 0)
        smpl += struct.pack("<6I", 0, 0, sound.loop_start, len(sound.samples) - 1, 0, 0)
        chunks += b"smpl" + struct.pack("<I", len(smpl)) + smpl
    return b"RIFF" + struct.pack("<I", 4 + len(chunks)) + b"WAVE" + chunks


def write_wav(path, sound):
    with open(path, "wb") as f:
        f.write(wav_bytes(sound))


def resample(samples, src_rate, dst_rate, half_width=16):
    """Windowed-sinc resampling (Blackman window, low-pass at the lower
    Nyquist).  Returns floats-rounded ints."""
    if src_rate == dst_rate or not samples:
        return list(samples)
    ratio = dst_rate / src_rate
    cut = min(1.0, ratio)
    width = int(math.ceil(half_width / cut))
    n_out = max(1, int(round(len(samples) * ratio)))
    out = []
    n = len(samples)
    for i in range(n_out):
        t = i / ratio
        c = int(math.floor(t))
        acc = 0.0
        for j in range(c - width + 1, c + width + 1):
            if 0 <= j < n:
                d = t - j
                if abs(d) >= width:
                    continue
                x = math.pi * d * cut
                sinc = cut if x == 0 else math.sin(x) / x * cut
                w = 0.42 + 0.5 * math.cos(math.pi * d / width) + 0.08 * math.cos(2 * math.pi * d / width)
                acc += samples[j] * sinc * w
        out.append(max(-32768, min(32767, int(round(acc)))))
    return out


def prepare_loop(sound):
    """Make a looped sound encodable: shift its start with leading silence so
    the loop starts on a 28-sample block, and make the loop body a whole
    number of blocks by repeating it (when 4 copies or fewer do) or else
    stretching it by resampling (pitch change below 28/body samples)."""
    if sound.loop_start is None:
        return Sound(sound.rate, sound.samples, None)
    s = list(sound.samples)
    ls = sound.loop_start
    lead = -ls % BLOCK_SAMPLES
    s = [0] * lead + s
    ls += lead
    body = s[ls:]
    if not body:
        raise ValueError("loop starts at the end of the sound")
    k = BLOCK_SAMPLES // math.gcd(len(body), BLOCK_SAMPLES)
    if k <= 4:
        body = body * k
    else:
        target = max(BLOCK_SAMPLES, int(round(len(body) / BLOCK_SAMPLES)) * BLOCK_SAMPLES)
        body = resample(body, len(body), target)
        body = (body + [0] * target)[:target]
    return Sound(sound.rate, s[:ls] + body, ls)


def snr_db(ref, test):
    n = min(len(ref), len(test))
    sig = sum(x * x for x in ref[:n])
    noise = sum((ref[i] - test[i]) ** 2 for i in range(n))
    if noise == 0:
        return float("inf")
    if sig == 0:
        return float("-inf")
    return 10 * math.log10(sig / noise)


# ---------------------------------------------------------------------------
# .cfg / .vol text
# ---------------------------------------------------------------------------

def tokenize(raw):
    """Columns of one line as the engine's GetLine splits them: a tab, or a
    space not followed by another space, ends a column; other spaces are
    dropped."""
    cols = [bytearray()]
    for i, c in enumerate(raw):
        if c == 9 or (c == 32 and (i + 1 >= len(raw) or raw[i + 1] != 32)):
            cols.append(bytearray())
        elif c != 32:
            cols[-1].append(c)
    return [bytes(c) for c in cols]


def atoi(b):
    """C atoi."""
    s = b.lstrip(b" \t\r\n")
    sign = 1
    if s[:1] in (b"-", b"+"):
        sign = -1 if s[:1] == b"-" else 1
        s = s[1:]
    n = 0
    for c in s:
        if not 48 <= c <= 57:
            break
        n = n * 10 + c - 48
    return sign * n


def s8(n):
    n &= 0xFF
    return n - 256 if n >= 128 else n


class TextFile:
    """Lines kept byte-exact as (text, end of line); '\\r\\n', '\\r' and
    '\\n' all end a line, as in GetLine."""

    def __init__(self, data):
        self.lines = []
        p = 0
        while p < len(data):
            q = p
            while q < len(data) and data[q] not in (10, 13):
                q += 1
            if data[q:q + 2] == b"\r\n":
                eol = b"\r\n"
            else:
                eol = data[q:q + 1]
            self.lines.append([data[p:q], eol])
            p = q + len(eol)

    def build(self):
        return b"".join(t + e for t, e in self.lines)

    def end_index(self):
        """Index of the END line (or the line count)."""
        for i, (t, _) in enumerate(self.lines):
            if tokenize(t)[0] == b"END":
                return i
        return len(self.lines)

    def eol(self):
        for _, e in self.lines:
            if e:
                return e
        return b"\r\n"


# ---------------------------------------------------------------------------
# SMF (.mid) sound-effect sequences
# ---------------------------------------------------------------------------

def _vlq(b, p):
    v = 0
    while True:
        c = b[p]
        p += 1
        v = (v << 7) | (c & 0x7F)
        if not c & 0x80:
            return v, p


def midi_notes(data):
    """(channel, program, key, velocity) of each note-on of an SMF format 0
    file, as sndCSeSeq plays it: 8 tracks = MIDI channels 0..7 (others are
    ignored), program 0 until a program change, velocity 0 is a note-off."""
    if data[:4] != b"MThd":
        raise ValueError("not an SMF file")
    hlen = struct.unpack(">I", data[4:8])[0]
    p = 8 + hlen
    if data[p:p + 4] != b"MTrk":
        raise ValueError("no MTrk")
    end = p + 8 + struct.unpack(">I", data[p + 4:p + 8])[0]
    p += 8
    prog = [0] * 16
    run = 0
    notes = []
    while p < end:
        _, p = _vlq(data, p)
        if data[p] & 0x80:
            run = data[p]
            p += 1
        st = run
        if st == 0xFF:
            kind = data[p]
            n, p = _vlq(data, p + 1)
            p += n
            if kind == 0x2F:
                break
            continue
        if st in (0xF0, 0xF7):
            n, p = _vlq(data, p)
            p += n
            continue
        hi, ch = st & 0xF0, st & 15
        n = 1 if hi in (0xC0, 0xD0) else 2
        d = data[p:p + n]
        p += n
        if hi == 0xC0:
            prog[ch] = d[0]
        elif hi == 0x90 and d[1] and ch < 8:
            notes.append((ch, prog[ch], d[0], d[1]))
    return notes


def sq_chunks(data):
    """Chunk list of a Sony .sq (Vers, Sequ, Midi); the MIDI event stream
    inside is kept as bytes."""
    return _chunks(data)


# ---------------------------------------------------------------------------
# Bank
# ---------------------------------------------------------------------------

KIND_NONE, KIND_KEYON, KIND_SQ, KIND_SESEQ = "none", "keyon", "sq", "seseq"


class Entry:
    """One play_se entry: the n-th line of the .cfg starting with a digit."""

    def __init__(self, **kw):
        self.__dict__.update(kw)

    def __repr__(self):
        return ("Entry(%d %s/%s file=%s kind=%s prog=%s key=%s vol=%s)"
                % (self.index, self.category, self.name, self.file, self.kind,
                   self.prog, self.key, self.volume))


class Voice:
    """One SPU voice an entry starts: the chain program -> split -> sample
    set -> sample -> VAG, and the key and velocity it is played with."""

    def __init__(self, prog, split, sample_set, sample, vag, key, velocity):
        self.prog, self.split, self.sample_set = prog, split, sample_set
        self.sample, self.vag, self.key, self.velocity = sample, vag, key, velocity

    def key_tuple(self):
        return (self.prog, self.split, self.sample_set, self.sample, self.vag, self.key, self.velocity)

    def __eq__(self, other):
        return isinstance(other, Voice) and self.key_tuple() == other.key_tuple()

    def __hash__(self):
        return hash(self.key_tuple())

    def __repr__(self):
        return "Voice(prog=%d split=%d sset=%d smpl=%d vag=%d key=%d vel=%d)" % self.key_tuple()


DEFAULT_SAMPLE = dict(vel_range_low=1, vel_cross_fade=64, vel_range_high=127,
                      vel_follow_pitch_center=1, vel_follow_amp_center=1, base_note=60,
                      panpot=64, priority=10, volume=128, reserved=0xFF, adsr1=0x80FF, adsr2=0x5FEC,
                      key_follow_ar_center=60, key_follow_dr_center=60, key_follow_sr_center=60,
                      key_follow_rr_center=60, key_follow_sl_center=60, spu_attribute=0x2F)
DEFAULT_SPLIT = dict(range_low=0, cross_fade=60, range_high=127, bend_range_low=256, bend_range_high=256,
                     key_follow_pitch_center=60, key_follow_amp_center=60, key_follow_pan_center=60,
                     volume=128, panpot=64)
DEFAULT_PROGRAM = dict(split_block_addr=0x24, split_block_size=0x14, volume=128, panpot=64,
                       key_follow_pan_center=60, reserved=0xFF, lfo_cycle=100, lfo_cycle2=100,
                       lfo_pitch_depth=128, lfo_pitch_depth2=128, lfo_midi_pitch_depth=128,
                       lfo_midi_pitch_depth2=128)


class Bank:
    MAX_PROGRAMS = 128      # SE_Play sends the program as a 7-bit MIDI value

    def __init__(self, pack):
        self.pack = pack
        self.hd_member = self._one("hd")
        self.bd_member = self._one("bd")
        self.cfg_member = self._one("cfg")
        self.vol_member = self._one("vol")
        self.hd = HD.parse(self.hd_member.data)
        bd = self.bd_member.data
        if self.hd.body_size != len(bd):
            raise ValueError("Head body size %d != bd size %d" % (self.hd.body_size, len(bd)))
        self.vag_data = self._split_bd(bd)
        self.cfg = TextFile(self.cfg_member.data)
        self.vol = TextFile(self.vol_member.data)

    def _one(self, ext):
        found = self.pack.find(ext)
        if len(found) != 1:
            raise ValueError("bank needs exactly one .%s member, has %d" % (ext, len(found)))
        return found[0]

    def _split_bd(self, bd):
        starts = [v.offset for v in self.hd.vags]
        if starts != sorted(starts) or len(set(starts)) != len(starts) or (starts and starts[0] != 0):
            raise ValueError("VAG offsets are not ascending from 0")
        ends = starts[1:] + [len(bd)]
        return [bd[s:e] for s, e in zip(starts, ends)]

    # -- loading and saving -------------------------------------------------

    @classmethod
    def from_bytes(cls, data):
        return cls(Pack.parse(data))

    @classmethod
    def from_snd(cls, path):
        with open(path, "rb") as f:
            return cls.from_bytes(f.read())

    def build_bd(self):
        out = bytearray()
        for vi, data in zip(self.hd.vags, self.vag_data):
            if len(data) % BLOCK:
                raise ValueError("VAG length not a multiple of 16")
            vi.offset = len(out)
            out += data
        return bytes(out)

    def build(self):
        """The .snd pack with hd, bd, cfg and vol rebuilt; member order and
        name fields are kept."""
        bd = self.build_bd()
        self.hd_member.data = self.hd.build(len(bd))
        self.bd_member.data = bd
        self.cfg_member.data = self.cfg.build()
        self.vol_member.data = self.vol.build()
        return self.pack.build()

    def write_dir(self, path):
        """Write the members to a folder, like the extracted NAME.snd.d."""
        import os
        self.build()
        os.makedirs(path, exist_ok=True)
        for m in self.pack.members:
            with open(os.path.join(path, m.name), "wb") as f:
                f.write(m.data)

    # -- entries --------------------------------------------------------------

    def se_num(self):
        """Size of the engine's table: cfg lines before END, header included."""
        return self.cfg.end_index()

    def volumes(self):
        """Default volume per entry as LoadVolInfoTxt sets it (0x40 when the
        .vol has no line), and the (type, depth) of a REVERB line or None."""
        n = self.se_num()
        vols = [0x40] * n
        reverb = None
        for text, _ in self.vol.lines:
            if not text:
                continue
            cols = tokenize(text)
            if cols[0] == b"END":
                break
            if cols[0] == b"REVERB":
                reverb = (atoi(cols[1]), atoi(cols[2]) if len(cols) > 2 else 0)
                continue
            i = atoi(cols[0])
            if 0 <= i < n:
                vols[i] = s8(atoi(cols[1]) if len(cols) > 1 else 0)
        return vols, reverb

    def entries(self):
        sq_names = [m.name.lower() for m in self.pack.find("sq")]
        mid_names = [m.name.lower() for m in self.pack.find("mid")]
        vols, _ = self.volumes()
        out = []
        end = self.cfg.end_index()
        for li in range(end):
            cols = tokenize(self.cfg.lines[li][0])
            if not 48 <= (cols[0][:1] or b"x")[0] <= 57:
                continue
            cols += [b""] * (8 - len(cols))
            f = cols[4]
            fl = f.decode("cp932").lower()
            seq = None
            if f == b"KeyOn":
                kind = KIND_KEYON
            elif fl in sq_names:
                kind, seq = KIND_SQ, sq_names.index(fl)
            elif fl in mid_names:
                kind, seq = KIND_SESEQ, mid_names.index(fl)
            elif "." in fl:
                kind = KIND_NONE
            else:
                kind = KIND_KEYON
            idx = len(out)
            out.append(Entry(index=idx, line=li, number=cols[0].decode("cp932"),
                             category=cols[1].decode("cp932", "replace"),
                             name=cols[2].decode("cp932", "replace"),
                             file=f.decode("cp932", "replace"), kind=kind, seq_index=seq,
                             prog=s8(atoi(cols[5])) if kind != KIND_SQ and kind != KIND_SESEQ else seq,
                             key=s8(atoi(cols[6])), flag=cols[7] != b"",
                             volume=vols[idx] if idx < len(vols) else None))
        return out

    def resolve(self, prog, key, velocity=127):
        """Voices a key-on of (program, key, velocity) starts: every split
        whose key range holds the key, every sample of its set whose velocity
        range holds the velocity (Sony's hardware-synth rules)."""
        voices = []
        progs = self.hd.programs
        prog &= 0x7F
        if prog >= len(progs) or progs[prog] is None:
            return voices
        for si, sp in enumerate(progs[prog].splits):
            if not sp.range_low <= key <= sp.range_high:
                continue
            if sp.sample_set >= len(self.hd.sample_sets) or self.hd.sample_sets[sp.sample_set] is None:
                continue
            ss = self.hd.sample_sets[sp.sample_set]
            if not ss.vel_limit_low <= velocity <= ss.vel_limit_high:
                continue
            for smi in ss.samples:
                sm = self.hd.samples[smi] if smi < len(self.hd.samples) else None
                # vag_index 0xFFFF marks a sample without a VAG (111 samples on the disc)
                if (sm is not None and sm.vel_range_low <= velocity <= sm.vel_range_high
                        and sm.vag_index < len(self.hd.vags)):
                    voices.append(Voice(prog, si, sp.sample_set, smi, sm.vag_index, key, velocity))
        return voices

    def voices(self, entry):
        """Voices an entry plays: KeyOn -> resolve(prog, key, 127); a .mid ->
        every note-on; a .sq -> not decoded (empty list)."""
        if isinstance(entry, int):
            entry = self.entries()[entry]
        if entry.kind == KIND_KEYON:
            return self.resolve(entry.prog, entry.key, 127)
        if entry.kind == KIND_SESEQ:
            data = self.pack.find("mid")[entry.seq_index].data
            seen = []
            for _, prog, key, vel in midi_notes(data):
                for v in self.resolve(prog, key, vel):
                    if v not in seen:
                        seen.append(v)
            return seen
        return []

    def referenced_programs(self):
        """Programs named by KeyOn rows and .mid program changes."""
        used = set()
        for e in self.entries():
            if e.kind == KIND_KEYON:
                used.add(e.prog & 0x7F)
        for m in self.pack.find("mid"):
            for _, prog, _, _ in midi_notes(m.data):
                used.add(prog & 0x7F)
        return used

    # -- samples --------------------------------------------------------------

    def decode_vag(self, index):
        """The VAG as a Sound (rate from Vagi, loop from the block flags)."""
        samples, loop = decode_adpcm(self.vag_data[index])
        return Sound(self.hd.vags[index].rate, samples, loop)

    def _encode(self, sound, rate=None, loop=None):
        """ADPCM bytes and the rate for a Sound: resampled to `rate` (default
        the sound's, at most 48000), loop True/False/None (None: the sound's
        own loop point)."""
        if rate is None:
            rate = min(sound.rate, 48000)
        samples = resample(sound.samples, sound.rate, rate) if rate != sound.rate else sound.samples
        ls = sound.loop_start
        if ls is not None and rate != sound.rate:
            ls = int(round(ls * rate / sound.rate))
        if loop is True and ls is None:
            ls = 0
        if loop is False:
            ls = None
        s = prepare_loop(Sound(rate, samples, ls))
        if not 1 <= rate <= 0xFFFF:
            raise ValueError("rate out of range")
        return encode_adpcm(s.samples, s.loop_start), rate, s.loop_start is not None

    def replace_vag(self, index, sound, rate=None, loop=None):
        """Replace one VAG in place: every sample, program and entry using it
        plays the new sound.  loop=None keeps the WAV's loop, or loops the
        whole sound when the old VAG looped and the WAV has no loop point."""
        if loop is None and sound.loop_start is None and self.hd.vags[index].attribute:
            loop = True
        data, rate, looped = self._encode(sound, rate, loop)
        self.vag_data[index] = data
        self.hd.vags[index].rate = rate
        self.hd.vags[index].attribute = 1 if looped else 0

    def users_of_vag(self, index):
        """Entries whose voices use a VAG."""
        return [e.index for e in self.entries() if any(v.vag == index for v in self.voices(e))]

    def _new_chain(self, sound, key, template, rate=None, loop=None):
        """Append VAG, sample and sample set for one sound played at `key`,
        and a program at a free number; returns the program number."""
        hd = self.hd
        number = self.free_program()
        if loop is None and sound.loop_start is None and template and hd.vags[template.vag].attribute:
            loop = True
        data, rate, looped = self._encode(sound, rate, loop)
        hd.vags.append(VagInfo(offset=0, rate=rate, attribute=1 if looped else 0, reserved=0xFF))
        self.vag_data.append(data)
        vag = len(hd.vags) - 1
        if template:
            smpl = hd.samples[template.sample].copy()
            split = hd.programs[template.prog].splits[template.split].copy()
            prog = hd.programs[template.prog].copy()
        else:
            smpl = Sample(**DEFAULT_SAMPLE)
            split = Split(**DEFAULT_SPLIT)
            prog = Program(**DEFAULT_PROGRAM)
        # plays at the VAG's own rate for `key` (base note = key, no transpose
        # or detune anywhere in the chain)
        smpl = smpl.copy(vag_index=vag, vel_range_low=1, vel_range_high=127,
                         base_note=max(0, min(127, key)), detune=0)
        hd.samples.append(smpl)
        hd.sample_sets.append(SampleSet([len(hd.samples) - 1], 0, 1, 127))
        split = split.copy(sample_set=len(hd.sample_sets) - 1, range_low=0, range_high=127,
                           number=0, transpose=0, detune=0)
        hd.programs += [None] * (number + 1 - len(hd.programs))
        hd.programs[number] = prog.copy(transpose=0, detune=0, splits=[split])
        return number

    def free_program(self):
        """A program number no KeyOn row and no .mid program change names:
        rows of some banks name programs the bank lacks (silent in this bank,
        e.g. the shared BS_999 layout), and filling such a number would make
        them sound.  Numbers past the table come first, then holes.  (.sq
        sequences are not decoded; their programs are not known.)"""
        used = self.referenced_programs()
        n = len(self.hd.programs)
        for i in list(range(n, self.MAX_PROGRAMS)) + list(range(min(n, self.MAX_PROGRAMS))):
            if i not in used and (i >= n or self.hd.programs[i] is None):
                return i
        raise ValueError("no free program number below %d" % self.MAX_PROGRAMS)

    def _set_columns(self, line, changes):
        text, eol = self.cfg.lines[line]
        cols = tokenize(text)
        if b"\t" not in text and b" " in text:
            raise ValueError("cfg line is space-separated; rewrite it by hand")
        cols += [b""] * (8 - len(cols))
        for i, v in changes.items():
            cols[i] = v.encode("cp932")
        self.cfg.lines[line] = [b"\t".join(cols), eol]

    def _set_volume(self, index, volume):
        vol = self.vol
        for i, (text, eol) in enumerate(vol.lines):
            cols = tokenize(text)
            if cols[0] == b"END":
                break
            if text and cols[0] != b"REVERB" and atoi(cols[0]) == index:
                vol.lines[i] = [b"%d\t%d" % (index, volume), eol]
                return
        vol.lines.insert(vol.end_index(), [b"%d\t%d" % (index, volume), vol.eol()])

    def replace_entry(self, index, sound, mode="detach", rate=None, loop=None, volume=None):
        """Give entry `index` a new sound.

        mode="detach" (default): append a VAG, sample, sample set and a new
        program (a copy of the entry's first voice's, with one split over all
        keys) and point the cfg row at it as KeyOn; no other entry changes.
        A .mid or .sq entry becomes a single KeyOn sound.
        mode="inplace": overwrite every VAG the entry's voices use; entries
        sharing them change too (see users_of_vag), the bank does not grow
        by the old sound's size.
        """
        entry = self.entries()[index]
        voices = self.voices(entry)
        if mode == "inplace":
            if not voices:
                raise ValueError("entry %d plays no VAG; use mode='detach'" % index)
            for vag in sorted({v.vag for v in voices}):
                self.replace_vag(vag, sound, rate, loop)
        elif mode == "detach":
            key = entry.key if entry.kind == KIND_KEYON and 0 <= entry.key <= 127 else 60
            prog = self._new_chain(sound, key, voices[0] if voices else None, rate, loop)
            self._set_columns(entry.line, {4: "KeyOn", 5: str(prog), 6: str(key)})
        else:
            raise ValueError("mode is 'detach' or 'inplace'")
        if volume is not None:
            self._set_volume(index, volume)

    def add_entry(self, sound, name="MOD_SE", category="MOD", volume=100, key=60,
                  template=None, rate=None, loop=None):
        """Append a KeyOn entry before END and return its number.  `template`
        (an entry number) copies that entry's envelope, priority and program
        settings; otherwise neutral defaults are used."""
        for text, limit in ((name, 63), (category, 63)):
            if len(text.encode("cp932")) > limit:
                raise ValueError("%r is longer than %d bytes" % (text, limit))
        tv = None
        if template is not None:
            vs = self.voices(template)
            tv = vs[0] if vs else None
        prog = self._new_chain(sound, key, tv, rate, loop)
        index = len(self.entries())
        if len(str(index)) > 7:
            raise ValueError("too many entries")
        line = "\t".join([str(index), category, name, "0", "KeyOn", str(prog), str(key), ""])
        self.cfg.lines.insert(self.cfg.end_index(), [line.encode("cp932"), self.cfg.eol()])
        self._set_volume(index, volume)
        return index

    # -- budgets --------------------------------------------------------------

    def ee_table_bytes(self):
        """Bytes LoadSeInfoTxt and sndLoadSound take from the port's memory
        stack: the entry table, sq name list, SE sequence table, names and
        converted MIDI events (estimate from the allocation sizes)."""
        q = -(-self.se_num() * 12 // 16) + 2
        sq = self.pack.find("sq")
        if sq:
            q += -(-len(sq) * 4 // 16) + 2 + sum(-(-(len(m.name.encode("cp932")) + 1) // 16) for m in sq)
        mid = self.pack.find("mid")
        if mid:
            q += len(mid) + 2
            for m in mid:
                q += -(-(len(m.name.encode("cp932")) + 1) // 16) + -(-_smf_events(m.data) * 6 // 16)
        return q * 16

    def spu_bytes(self):
        """SPU2 memory the bank takes: bd size + 16."""
        return sum(len(d) for d in self.vag_data) + 16


def _smf_events(data):
    hlen = struct.unpack(">I", data[4:8])[0]
    p = 8 + hlen + 8
    n = 0
    run = 0
    while True:
        _, p = _vlq(data, p)
        if data[p] & 0x80:
            run = data[p]
            p += 1
        if run == 0xFF:
            kind = data[p]
            ln, p = _vlq(data, p + 1)
            p += ln
            if kind == 0x2F:
                return n + 1
            continue
        p += 1 if run & 0xF0 in (0xC0, 0xD0) else 2
        n += 1


# ---------------------------------------------------------------------------
# Command line
# ---------------------------------------------------------------------------

def _cmd_info(bank, args):
    hd = bank.hd
    print("programs %d, splits %d, sample sets %d, samples %d, VAGs %d, bd %d bytes"
          % (sum(p is not None for p in hd.programs), sum(len(p.splits) for p in hd.programs if p),
             sum(s is not None for s in hd.sample_sets), sum(s is not None for s in hd.samples),
             len(hd.vags), sum(map(len, bank.vag_data))))
    print("members:", ", ".join(m.name for m in bank.pack.members))
    for e in bank.entries():
        vs = bank.voices(e)
        print("%3d %-6s %-14s vol %3s  %s  %s" % (
            e.index, e.kind, e.file if e.kind != KIND_KEYON else "prog %d key %d" % (e.prog, e.key),
            e.volume, e.name, " ".join("vag%d" % v for v in sorted({v.vag for v in vs}))))


def _cmd_export(bank, args):
    import os
    out = args[0]
    os.makedirs(out, exist_ok=True)
    for i in range(len(bank.hd.vags)):
        write_wav(os.path.join(out, "vag_%03d.wav" % i), bank.decode_vag(i))
    print("wrote %d WAVs to %s" % (len(bank.hd.vags), out))


def main(argv):
    usage = ("usage: soundbank.py info BANK.snd\n"
             "       soundbank.py export BANK.snd OUTDIR\n"
             "       soundbank.py replace BANK.snd ENTRY WAV OUT.snd [inplace]\n"
             "       soundbank.py add BANK.snd WAV OUT.snd NAME [VOLUME]")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if len(argv) < 2:
        print(usage)
        return 2
    cmd, path = argv[0], argv[1]
    bank = Bank.from_snd(path)
    if cmd == "info":
        _cmd_info(bank, argv[2:])
    elif cmd == "export" and len(argv) == 3:
        _cmd_export(bank, argv[2:])
    elif cmd == "replace" and len(argv) in (5, 6):
        bank.replace_entry(int(argv[2]), read_wav(argv[3]), mode=argv[5] if len(argv) == 6 else "detach")
        open(argv[4], "wb").write(bank.build())
    elif cmd == "add" and len(argv) in (5, 6):
        n = bank.add_entry(read_wav(argv[2]), name=argv[4], volume=int(argv[5]) if len(argv) == 6 else 100)
        open(argv[3], "wb").write(bank.build())
        print("added entry", n)
    else:
        print(usage)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
