//! Sound banks: a .snd pack holding NAME.hd (Sony's hardware-synth header: programs, key splits,
//! sample sets, samples, VAG infos), NAME.bd (the VAG bodies, SPU2 ADPCM), NAME.cfg (the
//! sound-effect table play_se reads), NAME.vol (default volumes), and .sq and .mid sequences.

use std::collections::BTreeSet;

use dc2tk_disc::archive::{self, Pack};
use dc2tk_disc::text;

use crate::bytes::{clamped, u16_at, u32_at, Reader};
use crate::{bail, numeric, Error, Result};

/// The Sony chunks' alignment: a table chunk is padded with 0xFF to a multiple of 16 bytes.
const CHUNK_ALIGN: usize = 16;

fn pad16(size: usize) -> usize {
    (CHUNK_ALIGN - size % CHUNK_ALIGN) % CHUNK_ALIGN
}

/// A sound bank's pack (archive::Pack), read strictly: besides holding together as a pack, every
/// member's padding is zeros, as the disc's banks are, so the bank written back is the file read.
pub fn read_pack(data: &[u8]) -> Result<Pack> {
    let Some(pack) = Pack::read(data) else {
        bail!("not a pack: its slots do not chain to an end slot at its end");
    };
    let mut at = 0;
    for member in &pack.members {
        let end = at + archive::PACK_SLOT + member.data.len();
        let padding = archive::padding(member.data.len());
        if data[end..end + padding].iter().any(|&b| b != 0) {
            bail!("{}: non-zero padding at 0x{end:x}", member.name);
        }
        at = end + padding;
    }
    Ok(pack)
}

/// A member name's extension, lower case ("" without a dot).
pub fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, ext)) => ext.to_lowercase(),
        None => String::new(),
    }
}

/// The positions of a pack's members with an extension, in pack order (ignoring case).
pub fn find(pack: &Pack, ext: &str) -> Vec<usize> {
    let ext = ext.to_lowercase();
    (0..pack.members.len()).filter(|&i| extension(&pack.members[i].name) == ext).collect()
}

/// The layout of a fixed little-endian record: field names and struct format characters.
pub struct Layout {
    pub fields: &'static [(&'static str, u8)],
}

impl Layout {
    pub fn size(&self) -> usize {
        self.fields.iter().map(|(_, f)| format_size(*f)).sum()
    }

    fn position(&self, name: &str) -> usize {
        self.fields.iter().position(|(n, _)| *n == name).unwrap_or_else(|| panic!("no field {name}"))
    }
}

fn format_size(format: u8) -> usize {
    match format {
        b'B' | b'b' => 1,
        b'H' | b'h' => 2,
        _ => 4,
    }
}

/// A record of a Layout, its values as wide ints (each field's range is checked when written).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub values: Vec<i64>,
    layout: LayoutKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutKind {
    VagInfo,
    Sample,
    Split,
    Program,
}

impl LayoutKind {
    fn layout(self) -> &'static Layout {
        match self {
            LayoutKind::VagInfo => &VAG_INFO,
            LayoutKind::Sample => &SAMPLE,
            LayoutKind::Split => &SPLIT,
            LayoutKind::Program => &PROGRAM,
        }
    }
}

/// Vagi record, 8 bytes.
pub static VAG_INFO: Layout =
    Layout { fields: &[("offset", b'I'), ("rate", b'H'), ("attribute", b'B'), ("reserved", b'B')] };

/// Smpl record (sceHardSynthSampleParam), 0x2A bytes; vag_index 0xFFFF means no VAG.
pub static SAMPLE: Layout = Layout {
    fields: &[
        ("vag_index", b'H'),
        ("vel_range_low", b'B'),
        ("vel_cross_fade", b'B'),
        ("vel_range_high", b'B'),
        ("vel_follow_pitch", b'b'),
        ("vel_follow_pitch_center", b'B'),
        ("vel_follow_pitch_curve", b'B'),
        ("vel_follow_amp", b'b'),
        ("vel_follow_amp_center", b'B'),
        ("vel_follow_amp_curve", b'B'),
        ("base_note", b'B'),
        ("detune", b'b'),
        ("panpot", b'B'),
        ("group", b'B'),
        ("priority", b'B'),
        ("volume", b'B'),
        ("reserved", b'B'),
        ("adsr1", b'H'),
        ("adsr2", b'H'),
        ("key_follow_ar", b'b'),
        ("key_follow_ar_center", b'B'),
        ("key_follow_dr", b'b'),
        ("key_follow_dr_center", b'B'),
        ("key_follow_sr", b'b'),
        ("key_follow_sr_center", b'B'),
        ("key_follow_rr", b'b'),
        ("key_follow_rr_center", b'B'),
        ("key_follow_sl", b'b'),
        ("key_follow_sl_center", b'B'),
        ("pitch_lfo_delay", b'H'),
        ("pitch_lfo_fade", b'H'),
        ("amp_lfo_delay", b'H'),
        ("amp_lfo_fade", b'H'),
        ("lfo_attribute", b'B'),
        ("spu_attribute", b'B'),
    ],
};

/// Split block of a program (sceHardSynthSplitBlock), 0x14 bytes.
pub static SPLIT: Layout = Layout {
    fields: &[
        ("sample_set", b'H'),
        ("range_low", b'B'),
        ("cross_fade", b'B'),
        ("range_high", b'B'),
        ("number", b'B'),
        ("bend_range_low", b'H'),
        ("bend_range_high", b'H'),
        ("key_follow_pitch", b'b'),
        ("key_follow_pitch_center", b'B'),
        ("key_follow_amp", b'b'),
        ("key_follow_amp_center", b'B'),
        ("key_follow_pan", b'b'),
        ("key_follow_pan_center", b'B'),
        ("volume", b'B'),
        ("panpot", b'B'),
        ("transpose", b'b'),
        ("detune", b'b'),
    ],
};

/// Prog record (sceHardSynthProgramParam), 0x24 bytes.
pub static PROGRAM: Layout = Layout {
    fields: &[
        ("split_block_addr", b'I'),
        ("n_split", b'B'),
        ("split_block_size", b'B'),
        ("volume", b'B'),
        ("panpot", b'B'),
        ("transpose", b'b'),
        ("detune", b'b'),
        ("key_follow_pan", b'b'),
        ("key_follow_pan_center", b'B'),
        ("attribute", b'B'),
        ("reserved", b'B'),
        ("lfo_wave", b'B'),
        ("lfo_wave2", b'B'),
        ("lfo_start_phase", b'B'),
        ("lfo_start_phase2", b'B'),
        ("lfo_phase_random", b'B'),
        ("lfo_phase_random2", b'B'),
        ("lfo_cycle", b'H'),
        ("lfo_cycle2", b'H'),
        ("lfo_pitch_depth", b'h'),
        ("lfo_pitch_depth2", b'h'),
        ("lfo_midi_pitch_depth", b'h'),
        ("lfo_midi_pitch_depth2", b'h'),
        ("lfo_amp_depth", b'b'),
        ("lfo_amp_depth2", b'b'),
        ("lfo_midi_amp_depth", b'b'),
        ("lfo_midi_amp_depth2", b'b'),
    ],
};

impl Record {
    fn new(layout: LayoutKind, values: &[(&str, i64)]) -> Record {
        let mut record = Record { values: vec![0; layout.layout().fields.len()], layout };
        for (name, value) in values {
            record.set(name, *value);
        }
        record
    }

    fn unpack(layout: LayoutKind, data: &[u8], at: usize) -> Result<Record> {
        let fields = layout.layout().fields;
        let mut values = Vec::with_capacity(fields.len());
        let mut reader = Reader::at(data, at, "the .hd file");
        for (_, format) in fields {
            values.push(match format {
                b'B' => reader.u8()? as i64,
                b'b' => reader.u8()? as i8 as i64,
                b'H' => reader.u16()? as i64,
                b'h' => reader.i16()? as i64,
                b'I' => reader.u32()? as i64,
                _ => reader.i32()? as i64,
            });
        }
        Ok(Record { values, layout })
    }

    pub fn pack(&self) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(self.layout.layout().size());
        for ((_, format), &v) in self.layout.layout().fields.iter().zip(&self.values) {
            let (low, high) = match format {
                b'B' => (0, 0xFF),
                b'b' => (-0x80, 0x7F),
                b'H' => (0, 0xFFFF),
                b'h' => (-0x8000, 0x7FFF),
                b'I' => (0, 0xFFFF_FFFF),
                _ => (-0x8000_0000, 0x7FFF_FFFF),
            };
            if v < low || v > high {
                bail!("a value does not fit the record's field, which holds {low} to {high}");
            }
            out.extend(&(v as u32).to_le_bytes()[..format_size(*format)]);
        }
        Ok(out)
    }

    pub fn get(&self, name: &str) -> i64 {
        self.values[self.layout.layout().position(name)]
    }

    pub fn set(&mut self, name: &str, value: i64) {
        let at = self.layout.layout().position(name);
        self.values[at] = value;
    }

    /// A copy with some fields changed.
    fn with(&self, changes: &[(&str, i64)]) -> Record {
        let mut new = self.clone();
        for (name, value) in changes {
            new.set(name, *value);
        }
        new
    }
}

/// A Prog record and the split blocks after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub record: Record,
    pub splits: Vec<Record>,
}

/// Sset record: vel_curve, vel_limit_low, vel_limit_high, count, then count u16 sample indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleSet {
    pub samples: Vec<i64>,
    pub vel_curve: i64,
    pub vel_limit_low: i64,
    pub vel_limit_high: i64,
}

impl SampleSet {
    fn unpack(data: &[u8], at: usize) -> Result<SampleSet> {
        let Some(head) = data.get(at..at + 4) else {
            bail!("the data is shorter than the {} bytes read here", at + 4);
        };
        let samples = (0..head[3] as usize).map(|i| Ok(u16_at(data, at + 4 + 2 * i)? as i64)).collect::<Result<_>>()?;
        Ok(SampleSet {
            samples,
            vel_curve: head[0] as i64,
            vel_limit_low: head[1] as i64,
            vel_limit_high: head[2] as i64,
        })
    }

    fn pack(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for v in [self.vel_curve, self.vel_limit_low, self.vel_limit_high, self.samples.len() as i64] {
            if !(0..=0xFF).contains(&v) {
                bail!("a value does not fit in a byte");
            }
            out.push(v as u8);
        }
        for &s in &self.samples {
            if !(0..=0xFFFF).contains(&s) {
                bail!("a value does not fit in an unsigned 16-bit int");
            }
            out.extend((s as u16).to_le_bytes());
        }
        Ok(out)
    }
}

const CREATOR: &[u8; 4] = b"IECS";
const NONE: u32 = 0xFFFF_FFFF;
const CHUNK_ORDER: [&str; 6] = ["Vers", "Head", "Vagi", "Smpl", "Sset", "Prog"];

fn tag(name: &str) -> Vec<u8> {
    name.bytes().rev().collect()
}

/// The chunks of a Sony file: (kind, offset, size).
fn chunks(data: &[u8]) -> Result<Vec<(String, usize, usize)>> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < data.len() {
        if off + 12 > data.len() {
            bail!("the data is shorter than the {} bytes read here", off + 12);
        }
        let size = u32_at(data, off + 8)? as usize;
        if &data[off..off + 4] != CREATOR || size < 12 || off + size > data.len() {
            bail!("bad chunk at 0x{off:x}");
        }
        let kind: Vec<u8> = data[off + 4..off + 8].iter().rev().copied().collect();
        if !kind.is_ascii() {
            bail!("a chunk name is not ASCII");
        }
        out.push((String::from_utf8(kind).unwrap(), off, size));
        off += size;
    }
    Ok(out)
}

fn table(data: &[u8], off: usize) -> Result<Vec<u32>> {
    let count = u32_at(data, off + 12)? as usize + 1;
    (0..count).map(|i| u32_at(data, off + 16 + 4 * i)).collect()
}

/// Chunk header, max index, offset table, records back to back in index order (absent ones NONE),
/// 0xFF padding to 16 bytes.
fn table_chunk(name: &str, records: &[Option<Vec<u8>>]) -> Result<Vec<u8>> {
    let n = records.len();
    if n == 0 {
        bail!("{name} table cannot be empty");
    }
    let head = 16 + 4 * n;
    let mut offsets = Vec::with_capacity(4 * n);
    let mut body: Vec<u8> = Vec::new();
    for record in records {
        match record {
            None => offsets.extend(NONE.to_le_bytes()),
            Some(record) => {
                offsets.extend(((head + body.len()) as u32).to_le_bytes());
                body.extend(record);
            }
        }
    }
    let size = head + body.len();
    let pad = pad16(size);
    let mut out = CREATOR.to_vec();
    out.extend(tag(name));
    out.extend(((size + pad) as u32).to_le_bytes());
    out.extend(((n - 1) as u32).to_le_bytes());
    out.extend(offsets);
    out.extend(body);
    out.extend(std::iter::repeat_n(0xFF, pad));
    Ok(out)
}

/// The .hd file.
#[derive(Clone, Debug)]
pub struct Hd {
    pub version: Vec<u8>,
    pub se_timbre_addr: u32,
    pub head_reserved: Vec<u8>,
    pub body_size: u32,
    pub vags: Vec<Record>,
    pub samples: Vec<Option<Record>>,
    pub sample_sets: Vec<Option<SampleSet>>,
    pub programs: Vec<Option<Program>>,
}

impl Hd {
    pub fn parse(data: &[u8]) -> Result<Hd> {
        let found = chunks(data)?;
        let names: Vec<&str> = found.iter().map(|(name, _, _)| name.as_str()).collect();
        if names != CHUNK_ORDER {
            let quoted: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
            bail!("unexpected chunk order ({}{})", quoted.join(", "), if quoted.len() == 1 { "," } else { "" });
        }
        let pos = |name: &str| found.iter().find(|(n, _, _)| n == name).unwrap().1;
        let h = pos("Head");
        let head: Vec<u32> = (0..7).map(|i| u32_at(data, h + 12 + 4 * i)).collect::<Result<_>>()?;
        let (hsize, bsize, prog, sset, smpl, vagi, timbre) =
            (head[0], head[1], head[2], head[3], head[4], head[5], head[6]);
        if hsize as usize != data.len()
            || (prog as usize, sset as usize, smpl as usize, vagi as usize)
                != (pos("Prog"), pos("Sset"), pos("Smpl"), pos("Vagi"))
        {
            bail!("Head chunk does not match the file");
        }
        let o = pos("Vagi");
        let vags = table(data, o)?
            .into_iter()
            .map(|x| Record::unpack(LayoutKind::VagInfo, data, o + x as usize))
            .collect::<Result<_>>()?;
        let o = pos("Smpl");
        let samples =
            table(data, o)?
                .into_iter()
                .map(|x| {
                    if x == NONE {
                        Ok(None)
                    } else {
                        Record::unpack(LayoutKind::Sample, data, o + x as usize).map(Some)
                    }
                })
                .collect::<Result<_>>()?;
        let o = pos("Sset");
        let sample_sets = table(data, o)?
            .into_iter()
            .map(|x| if x == NONE { Ok(None) } else { SampleSet::unpack(data, o + x as usize).map(Some) })
            .collect::<Result<_>>()?;
        let o = pos("Prog");
        let mut programs = Vec::new();
        for x in table(data, o)? {
            if x == NONE {
                programs.push(None);
                continue;
            }
            let record = Record::unpack(LayoutKind::Program, data, o + x as usize)?;
            let base = o + x as usize + record.get("split_block_addr") as usize;
            let size = record.get("split_block_size") as usize;
            let splits = (0..record.get("n_split") as usize)
                .map(|i| Record::unpack(LayoutKind::Split, data, base + i * size))
                .collect::<Result<_>>()?;
            programs.push(Some(Program { record, splits }));
        }
        Ok(Hd {
            version: clamped(data, 12, 16).to_vec(),
            se_timbre_addr: timbre,
            head_reserved: clamped(data, h + 40, h + 64).to_vec(),
            body_size: bsize,
            vags,
            samples,
            sample_sets,
            programs,
        })
    }

    /// The .hd bytes; sets each program's n_split to its splits, so the count always matches.
    pub fn build(&mut self, body_size: usize) -> Result<Vec<u8>> {
        let mut vers = CREATOR.to_vec();
        vers.extend(tag("Vers"));
        vers.extend(16u32.to_le_bytes());
        vers.extend(&self.version);
        let vagi = table_chunk("Vagi", &self.vags.iter().map(|v| v.pack().map(Some)).collect::<Result<Vec<_>>>()?)?;
        let smpl = table_chunk(
            "Smpl",
            &self.samples.iter().map(|s| s.as_ref().map(Record::pack).transpose()).collect::<Result<Vec<_>>>()?,
        )?;
        let sset = table_chunk(
            "Sset",
            &self
                .sample_sets
                .iter()
                .map(|s| s.as_ref().map(SampleSet::pack).transpose())
                .collect::<Result<Vec<_>>>()?,
        )?;
        let mut progs = Vec::new();
        for program in &mut self.programs {
            let Some(program) = program else {
                progs.push(None);
                continue;
            };
            if program.record.get("split_block_addr") != PROGRAM.size() as i64
                || program.record.get("split_block_size") != SPLIT.size() as i64
            {
                bail!("only programs with splits right after them can be written");
            }
            program.record.set("n_split", program.splits.len() as i64);
            let mut bytes = program.record.pack()?;
            for split in &program.splits {
                bytes.extend(split.pack()?);
            }
            progs.push(Some(bytes));
        }
        let prog = table_chunk("Prog", &progs)?;
        let o_vagi = 16 + 64;
        let o_smpl = o_vagi + vagi.len();
        let o_sset = o_smpl + smpl.len();
        let o_prog = o_sset + sset.len();
        let total = o_prog + prog.len();
        let mut out = vers;
        out.extend(CREATOR);
        out.extend(tag("Head"));
        out.extend(64u32.to_le_bytes());
        for v in [total, body_size, o_prog, o_sset, o_smpl, o_vagi] {
            out.extend((v as u32).to_le_bytes());
        }
        out.extend(self.se_timbre_addr.to_le_bytes());
        out.extend(&self.head_reserved);
        out.extend(vagi);
        out.extend(smpl);
        out.extend(sset);
        out.extend(prog);
        Ok(out)
    }
}

pub const BLOCK: usize = 16;
pub const BLOCK_SAMPLES: usize = 28;
const COEFS: [(i64, i64); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
/// Last block: stop (or jump to the loop address when FLAG_REPEAT).
pub const FLAG_END: u8 = 1;
/// With FLAG_END: jump back to the loop address and keep playing.
pub const FLAG_REPEAT: u8 = 2;
/// Sets the loop address to this block.
pub const FLAG_START: u8 = 4;
/// The block after a one-shot's end block.
const SILENT_TAIL: [u8; 16] =
    [0, 7, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77];

/// Mono 16-bit PCM: rate in Hz, samples, and the sample the sound loops back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    pub rate: i64,
    pub samples: Vec<i64>,
    pub loop_start: Option<i64>,
}

/// A VAG body decoded: its samples and loop start. Decoding stops after the first block with
/// FLAG_END; the leading all-zero block is skipped when asked.
pub fn decode_adpcm(data: &[u8], skip_zero_block: bool) -> Result<(Vec<i64>, Option<i64>)> {
    let mut out: Vec<i64> = Vec::new();
    let (mut h1, mut h2) = (0i64, 0i64);
    let mut loop_start = None;
    let mut looped = false;
    let start = if skip_zero_block && clamped(data, 0, 16) == [0u8; 16] { 1 } else { 0 };
    let mut b = start * BLOCK;
    while b + BLOCK <= data.len() {
        let head = data[b];
        let flags = data[b + 1];
        let mut shift = (head & 15) as u32;
        let filter = (head >> 4) as usize;
        if filter > 4 {
            bail!("ADPCM filter {filter} at 0x{b:x}");
        }
        if shift > 12 {
            shift = 9;
        }
        let (c1, c2) = COEFS[filter];
        if flags & FLAG_START != 0 {
            loop_start = Some(out.len() as i64);
        }
        for &byte in &data[b + 2..b + 16] {
            for nibble in [byte & 15, byte >> 4] {
                let nibble = if nibble >= 8 { nibble as i64 - 16 } else { nibble as i64 };
                let s = ((nibble << 12) >> shift) + ((h1 * c1 + h2 * c2 + 32) >> 6);
                let s = s.clamp(-32768, 32767);
                out.push(s);
                h2 = h1;
                h1 = s;
            }
        }
        if flags & FLAG_END != 0 {
            looped = flags & FLAG_REPEAT != 0;
            break;
        }
        b += BLOCK;
    }
    Ok((out, if looped { loop_start } else { None }))
}

/// Squared error, nibbles (when asked) and the history after quantising a block.
fn quantize(
    block: &[i64],
    mut h1: i64,
    mut h2: i64,
    c1: i64,
    c2: i64,
    k: u32,
    nibbles: bool,
) -> (i64, Vec<u8>, i64, i64) {
    let step = 1i64 << k;
    let mut err = 0i64;
    let mut nibs = Vec::new();
    for &x in block {
        let pred = (h1 * c1 + h2 * c2 + 32) >> 6;
        let q = ((x - pred + (step >> 1)) >> k).clamp(-8, 7);
        let y = (q * step + pred).clamp(-32768, 32767);
        let d = x - y;
        err += d * d;
        h2 = h1;
        h1 = y;
        if nibbles {
            nibs.push((q & 15) as u8);
        }
    }
    (err, nibs, h1, h2)
}

/// Filter and shift for 28 samples by closed-loop squared error. full: all 5 filters x 13 shifts;
/// otherwise the shift from the open-loop residual peak and one more bit of range.
fn encode_block(block: &[i64], h1: i64, h2: i64, full: bool) -> ([u8; 16], i64, i64) {
    let mut best: Option<(i64, usize, u32)> = None;
    for (f, &(c1, c2)) in COEFS.iter().enumerate() {
        let shifts: Vec<u32> = if full {
            (0..13).collect()
        } else {
            let (mut p1, mut p2, mut peak) = (h1, h2, 0i64);
            for &x in block {
                let r = (x - ((p1 * c1 + p2 * c2 + 32) >> 6)).abs();
                peak = peak.max(r);
                p2 = p1;
                p1 = x;
            }
            let mut k = 0;
            while k < 12 && (7i64 << k) < peak {
                k += 1;
            }
            if k < 12 {
                vec![k, k + 1]
            } else {
                vec![k]
            }
        };
        for k in shifts {
            let err = quantize(block, h1, h2, c1, c2, k, false).0;
            if best.is_none_or(|(e, _, _)| err < e) {
                best = Some((err, f, k));
            }
        }
        if best.is_some_and(|(e, _, _)| e == 0) {
            break;
        }
    }
    let (_, f, k) = best.unwrap();
    let (c1, c2) = COEFS[f];
    let (_, nibs, h1, h2) = quantize(block, h1, h2, c1, c2, k, true);
    let mut data = [0u8; 16];
    data[0] = ((f as u8) << 4) | (12 - k as u8);
    for i in 0..14 {
        data[2 + i] = nibs[2 * i] | (nibs[2 * i + 1] << 4);
    }
    (data, h1, h2)
}

/// PCM encoded to a VAG body in the layout every VAG of the game has. One-shot: zero block, first
/// block flags 4, last flags 1, then the 00 07 77.. block. Looped: zero block, every block flags
/// 2, the loop block 6, the last 3.
pub fn encode_adpcm(samples: &[i64], loop_start: Option<i64>, full: bool) -> Result<Vec<u8>> {
    let mut samples: Vec<i64> = samples.iter().map(|&s| s.clamp(-32768, 32767)).collect();
    if samples.is_empty() {
        samples.push(0);
    }
    samples.resize(samples.len().next_multiple_of(BLOCK_SAMPLES), 0);
    let blocks = samples.len() / BLOCK_SAMPLES;
    let mut loop_block = 0;
    if let Some(start) = loop_start {
        if start.rem_euclid(BLOCK_SAMPLES as i64) != 0 || !(0..samples.len() as i64).contains(&start) {
            bail!("loop start must be a block start inside the sound");
        }
        loop_block = start as usize / BLOCK_SAMPLES;
    }
    let mut out = vec![0u8; 16];
    let (mut h1, mut h2) = (0, 0);
    for i in 0..blocks {
        let (mut block, n1, n2) = encode_block(&samples[i * 28..(i + 1) * 28], h1, h2, full);
        (h1, h2) = (n1, n2);
        let end = if i == blocks - 1 { FLAG_END } else { 0 };
        block[1] = match loop_start {
            None => (if i == 0 { FLAG_START } else { 0 }) | end,
            Some(_) => FLAG_REPEAT | (if i == loop_block { FLAG_START } else { 0 }) | end,
        };
        out.extend(block);
    }
    if loop_start.is_none() {
        out.extend(SILENT_TAIL);
    }
    Ok(out)
}

/// Bytes of a VAG up to and including its end block (and the silent block after a one-shot's end
/// block, when present).
pub fn vag_length(data: &[u8]) -> usize {
    for b in (0..data.len()).step_by(BLOCK) {
        let flags = data.get(b + 1).copied().unwrap_or(0);
        if flags & FLAG_END != 0 {
            let mut end = b + BLOCK;
            if flags & FLAG_REPEAT == 0 && clamped(data, end, end + BLOCK) == SILENT_TAIL {
                end += BLOCK;
            }
            return end;
        }
    }
    data.len()
}

/// A WAV read: PCM 8/16/24/32-bit, float 32/64, also WAVE_FORMAT_EXTENSIBLE; channels averaged to
/// mono. A 'smpl' chunk's first loop gives loop_start (audio after its end is dropped).
pub fn read_wav(data: &[u8]) -> Result<Sound> {
    if clamped(data, 0, 4) != b"RIFF" || clamped(data, 8, 12) != b"WAVE" {
        bail!("not a WAV file");
    }
    let mut format = None;
    let mut pcm: Option<&[u8]> = None;
    let mut wav_loop = None;
    let mut off = 12;
    while off + 8 <= data.len() {
        let id = &data[off..off + 4];
        let size = u32_at(data, off + 4)? as usize;
        let body = clamped(data, off + 8, off + 8 + size);
        if id == b"fmt " {
            if body.len() < 16 {
                bail!("the data is shorter than the 16 bytes read here");
            }
            let mut tag = u16_at(body, 0)?;
            let channels = u16_at(body, 2)? as usize;
            let rate = u32_at(body, 4)? as i64;
            let align = u16_at(body, 12)? as usize;
            let bits = u16_at(body, 14)? as usize;
            if tag == 0xFFFE && body.len() >= 26 {
                tag = u16_at(body, 24)?;
            }
            format = Some((tag, channels, rate, align, bits));
        } else if id == b"data" {
            pcm = Some(body);
        } else if id == b"smpl" && body.len() >= 36 && u32_at(body, 28)? >= 1 && body.len() >= 60 {
            wav_loop = Some((u32_at(body, 44)? as usize, u32_at(body, 48)? as i64));
        }
        off += 8 + size + (size & 1);
    }
    let (Some((tag, channels, rate, align, bits)), Some(pcm)) = (format, pcm) else {
        bail!("WAV without fmt or data chunk");
    };
    let width = bits / 8;
    if align != width * channels {
        bail!("unsupported WAV block alignment");
    }
    if align == 0 {
        bail!("integer division or modulo by zero");
    }
    let frames = pcm.len() / align;
    let used = &pcm[..frames * align];
    let values: Vec<f64> = match (tag, bits) {
        (1, 8) => used.iter().map(|&b| (b as f64 - 128.0) * 256.0).collect(),
        (1, 16) => used.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f64).collect(),
        (1, 24) => used
            .as_chunks::<3>()
            .0
            .iter()
            .map(|b| (((b[2] as i8 as i32) << 16) | ((b[1] as i32) << 8) | b[0] as i32) as f64 / 256.0)
            .collect(),
        (1, 32) => used.as_chunks::<4>().0.iter().map(|b| i32::from_le_bytes(*b) as f64 / 65536.0).collect(),
        (3, 32) => used.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b) as f64 * 32768.0).collect(),
        (3, 64) => used.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b) * 32768.0).collect(),
        _ => bail!("unsupported WAV format {tag} with {bits} bits"),
    };
    let values: Vec<f64> = if channels > 1 {
        values.chunks(channels).map(|frame| numeric::compensated_sum(frame.iter().copied()) / channels as f64).collect()
    } else {
        values
    };
    let mut samples = Vec::with_capacity(values.len());
    for v in values {
        if v.is_nan() {
            bail!("cannot convert float NaN to integer");
        }
        if v.is_infinite() {
            bail!("cannot convert float infinity to integer");
        }
        samples.push((v.round_ties_even() as i64).clamp(-32768, 32767));
    }
    let mut loop_start = None;
    if let Some((start, end)) = wav_loop {
        if start < samples.len() {
            loop_start = Some(start as i64);
            samples.truncate(samples.len().min((end + 1).max(0) as usize));
        }
    }
    Ok(Sound { rate, samples, loop_start })
}

/// A 16-bit mono WAV, with a 'smpl' loop when the sound loops.
pub fn wav_bytes(sound: &Sound) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(2 * sound.samples.len());
    for &s in &sound.samples {
        let Ok(s) = i16::try_from(s) else {
            bail!("a value does not fit in a 16-bit int");
        };
        body.extend(s.to_le_bytes());
    }
    let u32_of = |v: i64| -> Result<[u8; 4]> {
        match u32::try_from(v) {
            Ok(v) => Ok(v.to_le_bytes()),
            Err(_) => bail!("a value does not fit in an unsigned 32-bit int"),
        }
    };
    let mut chunks = b"fmt ".to_vec();
    chunks.extend(16u32.to_le_bytes());
    chunks.extend(1u16.to_le_bytes());
    chunks.extend(1u16.to_le_bytes());
    chunks.extend(u32_of(sound.rate)?);
    chunks.extend(u32_of(sound.rate * 2)?);
    chunks.extend(2u16.to_le_bytes());
    chunks.extend(16u16.to_le_bytes());
    chunks.extend(b"data");
    chunks.extend((body.len() as u32).to_le_bytes());
    let odd = body.len() & 1 != 0;
    chunks.extend(body);
    if odd {
        chunks.push(0);
    }
    if let Some(loop_start) = sound.loop_start {
        if sound.rate == 0 {
            bail!("a sound with a rate of 0");
        }
        let period = (1e9 / sound.rate as f64).round_ties_even() as i64;
        let mut smpl = Vec::new();
        for v in [0, 0, period, 60, 0, 0, 0, 1, 0, 0, 0, loop_start, sound.samples.len() as i64 - 1, 0, 0] {
            smpl.extend(u32_of(v)?);
        }
        chunks.extend(b"smpl");
        chunks.extend((smpl.len() as u32).to_le_bytes());
        chunks.extend(smpl);
    }
    let mut out = b"RIFF".to_vec();
    out.extend(((4 + chunks.len()) as u32).to_le_bytes());
    out.extend(b"WAVE");
    out.extend(chunks);
    Ok(out)
}

/// Windowed-sinc resampling (Blackman window, low-pass at the lower Nyquist).
pub fn resample(samples: &[i64], src_rate: i64, dst_rate: i64) -> Vec<i64> {
    let half_width = 16.0;
    if src_rate == dst_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = dst_rate as f64 / src_rate as f64;
    let cut = numeric::min(1.0, ratio);
    let width = (half_width / cut).ceil() as i64;
    let n_out = ((samples.len() as f64 * ratio).round_ties_even() as i64).max(1);
    let n = samples.len() as i64;
    let pi = std::f64::consts::PI;
    let mut out = Vec::with_capacity(n_out as usize);
    for i in 0..n_out {
        let t = i as f64 / ratio;
        let c = t.floor() as i64;
        let mut acc = 0.0;
        for j in (c - width + 1)..(c + width + 1) {
            if 0 <= j && j < n {
                let d = t - j as f64;
                if d.abs() >= width as f64 {
                    continue;
                }
                let x = pi * d * cut;
                let sinc = if x == 0.0 { cut } else { x.sin() / x * cut };
                let w = 0.42 + 0.5 * (pi * d / width as f64).cos() + 0.08 * (2.0 * pi * d / width as f64).cos();
                acc += samples[j as usize] as f64 * sinc * w;
            }
        }
        out.push((acc.round_ties_even() as i64).clamp(-32768, 32767));
    }
    out
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// A looped sound made encodable: its start shifted with leading silence onto a 28-sample block,
/// and its loop body a whole number of blocks by repeating it (when 4 copies or fewer do) or else
/// stretched by resampling.
pub fn prepare_loop(sound: &Sound) -> Result<Sound> {
    let Some(loop_start) = sound.loop_start else {
        return Ok(Sound { rate: sound.rate, samples: sound.samples.clone(), loop_start: None });
    };
    let lead = (-loop_start).rem_euclid(BLOCK_SAMPLES as i64);
    let mut s = vec![0i64; lead as usize];
    s.extend(&sound.samples);
    let ls = loop_start + lead;
    let ls_at = ls.clamp(0, s.len() as i64) as usize;
    let mut body = s[ls_at..].to_vec();
    if body.is_empty() {
        bail!("loop starts at the end of the sound");
    }
    let k = BLOCK_SAMPLES / gcd(body.len(), BLOCK_SAMPLES);
    if k <= 4 {
        body = body.repeat(k);
    } else {
        let target =
            ((body.len() as f64 / BLOCK_SAMPLES as f64).round_ties_even() as i64 * BLOCK_SAMPLES as i64).max(28);
        body = resample(&body, body.len() as i64, target);
        body.resize((target as usize).max(body.len()), 0);
        body.truncate(target as usize);
    }
    let mut samples = s[..ls_at].to_vec();
    samples.extend(body);
    Ok(Sound { rate: sound.rate, samples, loop_start: Some(ls) })
}

/// Signal to noise of a decoded sound against its source, in dB.
pub fn snr_db(reference: &[i64], test: &[i64]) -> f64 {
    let n = reference.len().min(test.len());
    let signal: i64 = reference[..n].iter().map(|x| x * x).sum();
    let noise: i64 = (0..n).map(|i| (reference[i] - test[i]).pow(2)).sum();
    if noise == 0 {
        return f64::INFINITY;
    }
    if signal == 0 {
        return f64::NEG_INFINITY;
    }
    10.0 * (signal as f64 / noise as f64).log10()
}

/// Columns of one line as the engine's GetLine splits them: a tab, or a space not followed by
/// another space, ends a column; other spaces are dropped.
pub fn tokenize(raw: &[u8]) -> Vec<Vec<u8>> {
    let mut columns = vec![Vec::new()];
    for (i, &c) in raw.iter().enumerate() {
        if c == 9 || (c == 32 && (i + 1 >= raw.len() || raw[i + 1] != 32)) {
            columns.push(Vec::new());
        } else if c != 32 {
            columns.last_mut().unwrap().push(c);
        }
    }
    columns
}

/// C atoi.
pub fn atoi(b: &[u8]) -> i64 {
    let start = b.iter().position(|c| !b" \t\r\n".contains(c)).unwrap_or(b.len());
    let mut s = &b[start..];
    let mut sign = 1;
    if let Some(&c) = s.first() {
        if c == b'-' || c == b'+' {
            sign = if c == b'-' { -1 } else { 1 };
            s = &s[1..];
        }
    }
    let mut n: i64 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        n = n.wrapping_mul(10).wrapping_add((c - 48) as i64);
    }
    sign * n
}

pub fn s8(n: i64) -> i64 {
    let n = n & 0xFF;
    if n >= 128 {
        n - 256
    } else {
        n
    }
}

/// Lines kept byte-exact as (text, end of line); "\r\n", "\r" and "\n" all end a line.
#[derive(Clone, Debug)]
pub struct TextFile {
    pub lines: Vec<(Vec<u8>, Vec<u8>)>,
}

impl TextFile {
    pub fn parse(data: &[u8]) -> TextFile {
        let mut lines = Vec::new();
        let mut p = 0;
        while p < data.len() {
            let mut q = p;
            while q < data.len() && data[q] != 10 && data[q] != 13 {
                q += 1;
            }
            let eol =
                if clamped(data, q, q + 2) == b"\r\n" { b"\r\n".to_vec() } else { clamped(data, q, q + 1).to_vec() };
            lines.push((data[p..q].to_vec(), eol.clone()));
            p = q + eol.len();
        }
        TextFile { lines }
    }

    pub fn build(&self) -> Vec<u8> {
        self.lines.iter().flat_map(|(t, e)| t.iter().chain(e).copied()).collect()
    }

    /// The END line's index (or the line count).
    pub fn end_index(&self) -> usize {
        self.lines.iter().position(|(t, _)| tokenize(t)[0] == b"END").unwrap_or(self.lines.len())
    }

    pub fn eol(&self) -> Vec<u8> {
        self.lines.iter().find(|(_, e)| !e.is_empty()).map(|(_, e)| e.clone()).unwrap_or_else(|| b"\r\n".to_vec())
    }
}

fn vlq(b: &[u8], mut p: usize) -> Result<(usize, usize)> {
    let mut v = 0usize;
    loop {
        let Some(&c) = b.get(p) else {
            bail!("a variable-length number runs past the end of the data");
        };
        p += 1;
        v = (v << 7) | (c & 0x7F) as usize;
        if c & 0x80 == 0 {
            return Ok((v, p));
        }
    }
}

fn byte_at(data: &[u8], p: usize) -> Result<u8> {
    match data.get(p) {
        Some(&b) => Ok(b),
        None => bail!("the picture's data runs past the picture it is based on"),
    }
}

/// (channel, program, key, velocity) of each note-on of an SMF format 0 file, as sndCSeSeq plays
/// it: MIDI channels 0..7 only, program 0 until a program change, velocity 0 a note-off.
pub fn midi_notes(data: &[u8]) -> Result<Vec<(u8, i64, u8, u8)>> {
    if clamped(data, 0, 4) != b"MThd" {
        bail!("not an SMF file");
    }
    let Some(hlen) = data.get(4..8) else {
        bail!("unpack requires a buffer of 4 bytes");
    };
    let mut p = 8 + u32::from_be_bytes(hlen.try_into().unwrap()) as usize;
    if clamped(data, p, p + 4) != b"MTrk" {
        bail!("no MTrk");
    }
    let Some(length) = data.get(p + 4..p + 8) else {
        bail!("unpack requires a buffer of 4 bytes");
    };
    let end = p + 8 + u32::from_be_bytes(length.try_into().unwrap()) as usize;
    p += 8;
    let mut programs = [0i64; 16];
    let mut run = 0u8;
    let mut notes = Vec::new();
    while p < end {
        (_, p) = vlq(data, p)?;
        if byte_at(data, p)? & 0x80 != 0 {
            run = data[p];
            p += 1;
        }
        let status = run;
        if status == 0xFF {
            let kind = byte_at(data, p)?;
            let (n, q) = vlq(data, p + 1)?;
            p = q + n;
            if kind == 0x2F {
                break;
            }
            continue;
        }
        if status == 0xF0 || status == 0xF7 {
            let (n, q) = vlq(data, p)?;
            p = q + n;
            continue;
        }
        let (hi, channel) = (status & 0xF0, (status & 15) as usize);
        let n = if hi == 0xC0 || hi == 0xD0 { 1 } else { 2 };
        let d = clamped(data, p, p + n).to_vec();
        p += n;
        if hi == 0xC0 {
            programs[channel] = byte_at(&d, 0)? as i64;
        } else if hi == 0x90 && byte_at(&d, 1)? != 0 && channel < 8 {
            notes.push((channel as u8, programs[channel], d[0], d[1]));
        }
    }
    Ok(notes)
}

/// What an entry plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    None,
    KeyOn,
    Sq,
    SeSeq,
}

/// One play_se entry: the n-th line of the .cfg before END starting with a digit.
#[derive(Clone, Debug)]
pub struct Entry {
    pub index: usize,
    pub line: usize,
    pub number: String,
    pub category: String,
    pub name: String,
    pub file: String,
    pub kind: EntryKind,
    pub seq_index: Option<usize>,
    pub prog: i64,
    pub key: i64,
    pub flag: bool,
    pub volume: Option<i64>,
}

/// One SPU voice an entry starts: program -> split -> sample set -> sample -> VAG, with the key and
/// velocity it is played with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Voice {
    pub prog: usize,
    pub split: usize,
    pub sample_set: usize,
    pub sample: usize,
    pub vag: usize,
    pub key: i64,
    pub velocity: i64,
}

const DEFAULT_SAMPLE: &[(&str, i64)] = &[
    ("vel_range_low", 1),
    ("vel_cross_fade", 64),
    ("vel_range_high", 127),
    ("vel_follow_pitch_center", 1),
    ("vel_follow_amp_center", 1),
    ("base_note", 60),
    ("panpot", 64),
    ("priority", 10),
    ("volume", 128),
    ("reserved", 0xFF),
    ("adsr1", 0x80FF),
    ("adsr2", 0x5FEC),
    ("key_follow_ar_center", 60),
    ("key_follow_dr_center", 60),
    ("key_follow_sr_center", 60),
    ("key_follow_rr_center", 60),
    ("key_follow_sl_center", 60),
    ("spu_attribute", 0x2F),
];
const DEFAULT_SPLIT: &[(&str, i64)] = &[
    ("range_low", 0),
    ("cross_fade", 60),
    ("range_high", 127),
    ("bend_range_low", 256),
    ("bend_range_high", 256),
    ("key_follow_pitch_center", 60),
    ("key_follow_amp_center", 60),
    ("key_follow_pan_center", 60),
    ("volume", 128),
    ("panpot", 64),
];
const DEFAULT_PROGRAM: &[(&str, i64)] = &[
    ("split_block_addr", 0x24),
    ("split_block_size", 0x14),
    ("volume", 128),
    ("panpot", 64),
    ("key_follow_pan_center", 60),
    ("reserved", 0xFF),
    ("lfo_cycle", 100),
    ("lfo_cycle2", 100),
    ("lfo_pitch_depth", 128),
    ("lfo_pitch_depth2", 128),
    ("lfo_midi_pitch_depth", 128),
    ("lfo_midi_pitch_depth2", 128),
];

/// SE_Play sends the program as a 7-bit MIDI value.
pub const MAX_PROGRAMS: usize = 128;

/// One sample of a program set_program makes: the keys it plays (low to high), the key it sounds
/// at its own pitch, whether it loops (None: as the sound says), and its volume (0 to 128, 128
/// the sample's own level).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    pub sound: Sound,
    pub low: i64,
    pub high: i64,
    pub root: i64,
    pub looped: Option<bool>,
    pub volume: i64,
}

/// How replace_entry gives an entry its new sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A VAG, sample, sample set and program of its own; no other entry changes.
    Detach,
    /// Every VAG the entry's voices use overwritten.
    InPlace,
}

/// A sound bank.
#[derive(Clone, Debug)]
pub struct Bank {
    pub pack: Pack,
    hd_member: usize,
    bd_member: usize,
    cfg_member: usize,
    vol_member: usize,
    pub hd: Hd,
    pub vag_data: Vec<Vec<u8>>,
    pub cfg: TextFile,
    pub vol: TextFile,
}

fn decode(bytes: &[u8]) -> Result<String> {
    text::decode_strict(bytes).ok_or_else(|| Error::invalid(format!("{bytes:02x?} is not cp932 text")))
}

fn encode(text_: &str) -> Result<Vec<u8>> {
    text::encode(text_).ok_or_else(|| Error::invalid(format!("{text_:?} has a character cp932 cannot encode")))
}

impl Bank {
    pub fn from_bytes(data: &[u8]) -> Result<Bank> {
        let pack = read_pack(data)?;
        let one = |ext: &str| -> Result<usize> {
            let found = find(&pack, ext);
            if found.len() != 1 {
                bail!("bank needs exactly one .{} member, has {}", ext, found.len());
            }
            Ok(found[0])
        };
        let (hd_member, bd_member, cfg_member, vol_member) = (one("hd")?, one("bd")?, one("cfg")?, one("vol")?);
        let hd = Hd::parse(&pack.members[hd_member].data)?;
        let bd = &pack.members[bd_member].data;
        if hd.body_size as usize != bd.len() {
            bail!("Head body size {} != bd size {}", hd.body_size, bd.len());
        }
        let starts: Vec<i64> = hd.vags.iter().map(|v| v.get("offset")).collect();
        let mut unique = starts.clone();
        unique.sort_unstable();
        unique.dedup();
        if !starts.is_sorted() || unique.len() != starts.len() || starts.first().is_some_and(|&s| s != 0) {
            bail!("VAG offsets are not ascending from 0");
        }
        let vag_data = (0..starts.len())
            .map(|i| {
                let end = starts.get(i + 1).map_or(bd.len(), |&e| e as usize);
                clamped(bd, starts[i] as usize, end).to_vec()
            })
            .collect();
        let cfg = TextFile::parse(&pack.members[cfg_member].data);
        let vol = TextFile::parse(&pack.members[vol_member].data);
        Ok(Bank { pack, hd_member, bd_member, cfg_member, vol_member, hd, vag_data, cfg, vol })
    }

    fn build_bd(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for (info, data) in self.hd.vags.iter_mut().zip(&self.vag_data) {
            if data.len() % BLOCK != 0 {
                bail!("VAG length not a multiple of 16");
            }
            info.set("offset", out.len() as i64);
            out.extend(data);
        }
        Ok(out)
    }

    /// The .snd pack with hd, bd, cfg and vol rebuilt; member order and name fields kept.
    pub fn build(&mut self) -> Result<Vec<u8>> {
        let bd = self.build_bd()?;
        self.pack.members[self.hd_member].data = self.hd.build(bd.len())?;
        self.pack.members[self.bd_member].data = bd;
        self.pack.members[self.cfg_member].data = self.cfg.build();
        self.pack.members[self.vol_member].data = self.vol.build();
        Ok(self.pack.write()?)
    }

    /// Size of the engine's table: cfg lines before END, header included.
    pub fn se_num(&self) -> usize {
        self.cfg.end_index()
    }

    /// Default volume per entry (0x40 when the .vol has no line), and a REVERB line's values.
    pub fn volumes(&self) -> (Vec<i64>, Option<(i64, i64)>) {
        let n = self.se_num();
        let mut volumes = vec![0x40; n];
        let mut reverb = None;
        for (text_, _) in &self.vol.lines {
            if text_.is_empty() {
                continue;
            }
            let cols = tokenize(text_);
            if cols[0] == b"END" {
                break;
            }
            if cols[0] == b"REVERB" {
                let depth = if cols.len() > 2 { atoi(&cols[2]) } else { 0 };
                reverb = Some((cols.get(1).map_or(0, |c| atoi(c)), depth));
                continue;
            }
            let i = atoi(&cols[0]);
            if 0 <= i && (i as usize) < n {
                volumes[i as usize] = s8(if cols.len() > 1 { atoi(&cols[1]) } else { 0 });
            }
        }
        (volumes, reverb)
    }

    pub fn entries(&self) -> Result<Vec<Entry>> {
        let names = |ext: &str| -> Vec<String> {
            find(&self.pack, ext).into_iter().map(|i| self.pack.members[i].name.to_lowercase()).collect()
        };
        let (sq_names, mid_names) = (names("sq"), names("mid"));
        let (volumes, _) = self.volumes();
        let mut out = Vec::new();
        for line in 0..self.cfg.end_index() {
            let mut cols = tokenize(&self.cfg.lines[line].0);
            if !cols[0].first().is_some_and(|c| c.is_ascii_digit()) {
                continue;
            }
            cols.resize(cols.len().max(8), Vec::new());
            let f = &cols[4];
            let lower = decode(f)?.to_lowercase();
            let mut seq = None;
            let kind = if f == b"KeyOn" {
                EntryKind::KeyOn
            } else if let Some(i) = sq_names.iter().position(|n| *n == lower) {
                seq = Some(i);
                EntryKind::Sq
            } else if let Some(i) = mid_names.iter().position(|n| *n == lower) {
                seq = Some(i);
                EntryKind::SeSeq
            } else if lower.contains('.') {
                EntryKind::None
            } else {
                EntryKind::KeyOn
            };
            let index = out.len();
            out.push(Entry {
                index,
                line,
                number: decode(&cols[0])?,
                category: text::decode_lossy(&cols[1]),
                name: text::decode_lossy(&cols[2]),
                file: text::decode_lossy(f),
                kind,
                seq_index: seq,
                prog: if matches!(kind, EntryKind::Sq | EntryKind::SeSeq) {
                    seq.unwrap() as i64
                } else {
                    s8(atoi(&cols[5]))
                },
                key: s8(atoi(&cols[6])),
                flag: !cols[7].is_empty(),
                volume: volumes.get(index).copied(),
            });
        }
        Ok(out)
    }

    /// Voices a key-on of (program, key, velocity) starts.
    pub fn resolve(&self, prog: i64, key: i64, velocity: i64) -> Vec<Voice> {
        let mut voices = Vec::new();
        let prog = (prog & 0x7F) as usize;
        let Some(Some(program)) = self.hd.programs.get(prog) else {
            return voices;
        };
        for (si, split) in program.splits.iter().enumerate() {
            if !(split.get("range_low") <= key && key <= split.get("range_high")) {
                continue;
            }
            let set_index = split.get("sample_set") as usize;
            let Some(Some(set)) = self.hd.sample_sets.get(set_index) else {
                continue;
            };
            if !(set.vel_limit_low <= velocity && velocity <= set.vel_limit_high) {
                continue;
            }
            for &smi in &set.samples {
                let Some(Some(sample)) = self.hd.samples.get(smi as usize) else {
                    continue;
                };
                // vag_index 0xFFFF marks a sample without a VAG
                if sample.get("vel_range_low") <= velocity
                    && velocity <= sample.get("vel_range_high")
                    && (sample.get("vag_index") as usize) < self.hd.vags.len()
                {
                    voices.push(Voice {
                        prog,
                        split: si,
                        sample_set: set_index,
                        sample: smi as usize,
                        vag: sample.get("vag_index") as usize,
                        key,
                        velocity,
                    });
                }
            }
        }
        voices
    }

    /// Voices an entry plays: KeyOn resolves (prog, key, 127); a .mid or .sq every note-on.
    pub fn voices(&self, entry: &Entry) -> Result<Vec<Voice>> {
        match entry.kind {
            EntryKind::KeyOn => Ok(self.resolve(entry.prog, entry.key, 127)),
            EntryKind::SeSeq | EntryKind::Sq => {
                let notes = if entry.kind == EntryKind::Sq {
                    let member = find(&self.pack, "sq")[entry.seq_index.unwrap()];
                    crate::sequence::Sequence::read(&self.pack.members[member].data)?.notes()
                } else {
                    let member = find(&self.pack, "mid")[entry.seq_index.unwrap()];
                    midi_notes(&self.pack.members[member].data)?.into_iter().map(|(_, p, k, v)| (p, k, v)).collect()
                };
                let mut seen = Vec::new();
                for (prog, key, velocity) in notes {
                    for v in self.resolve(prog, key as i64, velocity as i64) {
                        if !seen.contains(&v) {
                            seen.push(v);
                        }
                    }
                }
                Ok(seen)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Programs named by KeyOn rows, .mid program changes and .sq program changes.
    pub fn referenced_programs(&self) -> Result<BTreeSet<i64>> {
        let mut used = BTreeSet::new();
        for e in self.entries()? {
            if e.kind == EntryKind::KeyOn {
                used.insert(e.prog & 0x7F);
            }
        }
        for m in find(&self.pack, "mid") {
            for (_, prog, _, _) in midi_notes(&self.pack.members[m].data)? {
                used.insert(prog & 0x7F);
            }
        }
        for m in find(&self.pack, "sq") {
            let sequence = crate::sequence::Sequence::read(&self.pack.members[m].data)?;
            used.extend(sequence.programs().into_iter().map(i64::from));
        }
        Ok(used)
    }

    /// The VAG as a Sound (rate from Vagi, loop from the block flags).
    pub fn decode_vag(&self, index: usize) -> Result<Sound> {
        let (samples, loop_start) = decode_adpcm(&self.vag_data[index], true)?;
        Ok(Sound { rate: self.hd.vags[index].get("rate"), samples, loop_start })
    }

    /// ADPCM bytes, rate and whether it loops for a Sound: resampled to `rate` (default the
    /// sound's, at most 48000); loop Some(true/false), or None for the sound's own loop point.
    fn encode(&self, sound: &Sound, rate: Option<i64>, wanted: Option<bool>) -> Result<(Vec<u8>, i64, bool)> {
        let rate = rate.unwrap_or(sound.rate.min(48000));
        let samples =
            if rate != sound.rate { resample(&sound.samples, sound.rate, rate) } else { sound.samples.clone() };
        let mut ls = sound.loop_start;
        if let Some(start) = ls {
            if rate != sound.rate {
                ls = Some(((start * rate) as f64 / sound.rate as f64).round_ties_even() as i64);
            }
        }
        if wanted == Some(true) && ls.is_none() {
            ls = Some(0);
        }
        if wanted == Some(false) {
            ls = None;
        }
        let s = prepare_loop(&Sound { rate, samples, loop_start: ls })?;
        if !(1..=0xFFFF).contains(&rate) {
            bail!("rate out of range");
        }
        Ok((encode_adpcm(&s.samples, s.loop_start, true)?, rate, s.loop_start.is_some()))
    }

    /// One VAG replaced in place: every sample, program and entry using it plays the new sound.
    pub fn replace_vag(
        &mut self,
        index: usize,
        sound: &Sound,
        rate: Option<i64>,
        mut wanted: Option<bool>,
    ) -> Result<()> {
        if wanted.is_none() && sound.loop_start.is_none() && self.hd.vags[index].get("attribute") != 0 {
            wanted = Some(true);
        }
        let (data, rate, looped) = self.encode(sound, rate, wanted)?;
        self.vag_data[index] = data;
        self.hd.vags[index].set("rate", rate);
        self.hd.vags[index].set("attribute", looped as i64);
        Ok(())
    }

    /// Entries whose voices use a VAG.
    pub fn users_of_vag(&self, index: usize) -> Result<Vec<usize>> {
        let mut users = Vec::new();
        for e in self.entries()? {
            if self.voices(&e)?.iter().any(|v| v.vag == index) {
                users.push(e.index);
            }
        }
        Ok(users)
    }

    /// VAG, sample, sample set and a program at a free number for one sound played at `key`;
    /// returns the program number.
    fn new_chain(
        &mut self,
        sound: &Sound,
        key: i64,
        template: Option<Voice>,
        rate: Option<i64>,
        mut wanted: Option<bool>,
    ) -> Result<usize> {
        let number = self.free_program()?;
        if wanted.is_none()
            && sound.loop_start.is_none()
            && template.is_some_and(|t| self.hd.vags[t.vag].get("attribute") != 0)
        {
            wanted = Some(true);
        }
        let (data, rate, looped) = self.encode(sound, rate, wanted)?;
        let hd = &mut self.hd;
        hd.vags.push(Record::new(
            LayoutKind::VagInfo,
            &[("offset", 0), ("rate", rate), ("attribute", looped as i64), ("reserved", 0xFF)],
        ));
        self.vag_data.push(data);
        let vag = hd.vags.len() as i64 - 1;
        let (sample, split, program) = match template {
            Some(t) => {
                let program = hd.programs[t.prog].clone().unwrap();
                (hd.samples[t.sample].clone().unwrap(), program.splits[t.split].clone(), program)
            }
            None => (
                Record::new(LayoutKind::Sample, DEFAULT_SAMPLE),
                Record::new(LayoutKind::Split, DEFAULT_SPLIT),
                Program { record: Record::new(LayoutKind::Program, DEFAULT_PROGRAM), splits: Vec::new() },
            ),
        };
        // plays at the VAG's own rate for `key`: base note = key, no transpose or detune
        let sample = sample.with(&[
            ("vag_index", vag),
            ("vel_range_low", 1),
            ("vel_range_high", 127),
            ("base_note", key.clamp(0, 127)),
            ("detune", 0),
        ]);
        hd.samples.push(Some(sample));
        hd.sample_sets.push(Some(SampleSet {
            samples: vec![hd.samples.len() as i64 - 1],
            vel_curve: 0,
            vel_limit_low: 1,
            vel_limit_high: 127,
        }));
        let split = split.with(&[
            ("sample_set", hd.sample_sets.len() as i64 - 1),
            ("range_low", 0),
            ("range_high", 127),
            ("number", 0),
            ("transpose", 0),
            ("detune", 0),
        ]);
        if hd.programs.len() < number + 1 {
            hd.programs.resize(number + 1, None);
        }
        hd.programs[number] =
            Some(Program { record: program.record.with(&[("transpose", 0), ("detune", 0)]), splits: vec![split] });
        Ok(number)
    }

    /// Program `number` made of zones, one split each, in place of any program of that number
    /// (its samples stay until nothing plays them; see sounds::free_unused). Each zone's sample
    /// plays at its own pitch at its root key, over its key range, with the default envelope (no
    /// attack, full sustain, a short release) unless `like` names a program of the bank whose
    /// first sample's and split's settings (envelope, pan, priority) it takes. Returns the VAGs the
    /// program it replaced played.
    pub fn set_program(&mut self, number: usize, zones: &[Zone], like: Option<usize>) -> Result<BTreeSet<usize>> {
        if number >= MAX_PROGRAMS {
            bail!("program {number}: a program is 0 to {}", MAX_PROGRAMS - 1);
        }
        let mut old = BTreeSet::new();
        if let Some(Some(program)) = self.hd.programs.get(number) {
            for split in &program.splits {
                if let Some(Some(set)) = self.hd.sample_sets.get(split.get("sample_set") as usize) {
                    for &sample in &set.samples {
                        if let Some(Some(sample)) = self.hd.samples.get(sample as usize) {
                            old.insert(sample.get("vag_index") as usize);
                        }
                    }
                }
            }
        }
        let template = match like {
            Some(like) => {
                let Some(Some(program)) = self.hd.programs.get(like) else {
                    bail!("program {number}: \"like\" names program {like}, which the bank lacks");
                };
                let split = program.splits.first().cloned();
                let sample = split
                    .as_ref()
                    .and_then(|split| self.hd.sample_sets.get(split.get("sample_set") as usize).cloned().flatten())
                    .and_then(|set| {
                        set.samples.first().and_then(|&at| self.hd.samples.get(at as usize).cloned().flatten())
                    });
                Some((program.record.clone(), split, sample))
            }
            None => None,
        };
        let mut splits = Vec::new();
        for (at, zone) in zones.iter().enumerate() {
            if !(0..=zone.high).contains(&zone.low) || zone.high > 127 || !(0..=127).contains(&zone.root) {
                bail!("program {number}: keys run from 0 to 127, low to high, and the root is one of them");
            }
            if !(0..=128).contains(&zone.volume) {
                bail!("program {number}: a volume is 0 to 128 (128 plays the sample at its own level)");
            }
            let (data, rate, looped) = self.encode(&zone.sound, None, zone.looped)?;
            self.hd.vags.push(Record::new(
                LayoutKind::VagInfo,
                &[("offset", 0), ("rate", rate), ("attribute", looped as i64), ("reserved", 0xFF)],
            ));
            self.vag_data.push(data);
            let vag = self.hd.vags.len() as i64 - 1;
            let sample = template
                .as_ref()
                .and_then(|(_, _, sample)| sample.clone())
                .unwrap_or_else(|| Record::new(LayoutKind::Sample, DEFAULT_SAMPLE));
            let sample = sample.with(&[
                ("vag_index", vag),
                ("vel_range_low", 1),
                ("vel_range_high", 127),
                ("base_note", zone.root),
                ("detune", 0),
                ("volume", zone.volume),
            ]);
            self.hd.samples.push(Some(sample));
            self.hd.sample_sets.push(Some(SampleSet {
                samples: vec![self.hd.samples.len() as i64 - 1],
                vel_curve: 0,
                vel_limit_low: 1,
                vel_limit_high: 127,
            }));
            let split = template
                .as_ref()
                .and_then(|(_, split, _)| split.clone())
                .unwrap_or_else(|| Record::new(LayoutKind::Split, DEFAULT_SPLIT));
            splits.push(split.with(&[
                ("sample_set", self.hd.sample_sets.len() as i64 - 1),
                ("range_low", zone.low),
                ("range_high", zone.high),
                ("number", at as i64),
                ("transpose", 0),
                ("detune", 0),
            ]));
        }
        let record = template
            .map(|(record, _, _)| record)
            .unwrap_or_else(|| Record::new(LayoutKind::Program, DEFAULT_PROGRAM))
            .with(&[("transpose", 0), ("detune", 0)]);
        if self.hd.programs.len() < number + 1 {
            self.hd.programs.resize(number + 1, None);
        }
        self.hd.programs[number] = Some(Program { record, splits });
        Ok(old)
    }

    /// The bytes the bank's samples take in SPU2 memory when loaded (the .bd).
    pub fn samples_size(&self) -> usize {
        self.vag_data.iter().map(Vec::len).sum()
    }

    /// A program number no KeyOn row and no .mid program change names: numbers past the table
    /// first, then holes.
    pub fn free_program(&self) -> Result<usize> {
        let used = self.referenced_programs()?;
        let n = self.hd.programs.len();
        for i in (n..MAX_PROGRAMS).chain(0..n.min(MAX_PROGRAMS)) {
            if !used.contains(&(i as i64)) && (i >= n || self.hd.programs[i].is_none()) {
                return Ok(i);
            }
        }
        bail!("no free program number below {MAX_PROGRAMS}");
    }

    fn set_columns(&mut self, line: usize, changes: &[(usize, String)]) -> Result<()> {
        let (text_, eol) = self.cfg.lines[line].clone();
        let mut cols = tokenize(&text_);
        if !text_.contains(&b'\t') && text_.contains(&b' ') {
            bail!("cfg line is space-separated; rewrite it by hand");
        }
        cols.resize(cols.len().max(8), Vec::new());
        for (i, v) in changes {
            cols[*i] = encode(v)?;
        }
        self.cfg.lines[line] = (cols.join(&b'\t'), eol);
        Ok(())
    }

    pub fn set_volume(&mut self, index: usize, volume: i64) {
        let line = format!("{index}\t{volume}").into_bytes();
        for i in 0..self.vol.lines.len() {
            let (text_, eol) = &self.vol.lines[i];
            let cols = tokenize(text_);
            if cols[0] == b"END" {
                break;
            }
            if !text_.is_empty() && cols[0] != b"REVERB" && atoi(&cols[0]) == index as i64 {
                self.vol.lines[i] = (line, eol.clone());
                return;
            }
        }
        let at = self.vol.end_index();
        let eol = self.vol.eol();
        self.vol.lines.insert(at, (line, eol));
    }

    /// Gives an entry a new sound (see Mode).
    pub fn replace_entry(&mut self, index: usize, sound: &Sound, mode: Mode, volume: Option<i64>) -> Result<()> {
        let entry = self.entries()?.swap_remove(index);
        let voices = self.voices(&entry)?;
        match mode {
            Mode::InPlace => {
                if voices.is_empty() {
                    bail!("entry {index} plays no VAG; use mode='detach'");
                }
                let vags: BTreeSet<usize> = voices.iter().map(|v| v.vag).collect();
                for vag in vags {
                    self.replace_vag(vag, sound, None, None)?;
                }
            }
            Mode::Detach => {
                let key = if entry.kind == EntryKind::KeyOn && (0..=127).contains(&entry.key) { entry.key } else { 60 };
                let prog = self.new_chain(sound, key, voices.first().copied(), None, None)?;
                self.set_columns(entry.line, &[(4, "KeyOn".into()), (5, prog.to_string()), (6, key.to_string())])?;
            }
        }
        if let Some(volume) = volume {
            self.set_volume(index, volume);
        }
        Ok(())
    }

    /// A KeyOn entry appended before END with neutral defaults; returns its number.
    pub fn add_entry(&mut self, sound: &Sound, name: &str, category: &str, volume: i64, key: i64) -> Result<usize> {
        for text_ in [name, category] {
            if encode(text_)?.len() > 63 {
                bail!("{text_:?} is longer than 63 bytes");
            }
        }
        let prog = self.new_chain(sound, key, None, None, None)?;
        let index = self.entries()?.len();
        if index.to_string().len() > 7 {
            bail!("too many entries");
        }
        let line = [
            index.to_string(),
            category.into(),
            name.into(),
            "0".into(),
            "KeyOn".into(),
            prog.to_string(),
            key.to_string(),
            String::new(),
        ]
        .join("\t");
        let at = self.cfg.end_index();
        let eol = self.cfg.eol();
        self.cfg.lines.insert(at, (encode(&line)?, eol));
        self.set_volume(index, volume);
        Ok(index)
    }
}

/// A small bank for tests: a placeholder entry 0 that plays a silent sample, then one entry for
/// each rate (a short saw wave played at it), entry 1 at the given volume. Public for other
/// crates' tests.
#[doc(hidden)]
pub fn test_bank(rates: &[i64], volume: i64) -> Vec<u8> {
    let vag = encode_adpcm(&[0; 28], None, true).unwrap();
    let mut hd = Hd {
        version: vec![0, 0, 2, 0],
        se_timbre_addr: u32::MAX,
        head_reserved: vec![0xFF; 24],
        body_size: vag.len() as u32,
        vags: vec![Record::new(
            LayoutKind::VagInfo,
            &[("offset", 0), ("rate", 22050), ("attribute", 0), ("reserved", 0xFF)],
        )],
        samples: vec![Some(Record::new(LayoutKind::Sample, DEFAULT_SAMPLE))],
        sample_sets: vec![Some(SampleSet { samples: vec![0], vel_curve: 0, vel_limit_low: 1, vel_limit_high: 127 })],
        programs: vec![Some(Program {
            record: Record::new(LayoutKind::Program, DEFAULT_PROGRAM),
            splits: vec![Record::new(LayoutKind::Split, DEFAULT_SPLIT)],
        })],
    };
    let members = [
        ("x.hd".to_string(), hd.build(vag.len()).unwrap()),
        ("x.bd".to_string(), vag),
        ("x.cfg".to_string(), b"SP\tx\t0\t0\r\n0\t-\t-\t0\tKeyOn\t0\t60\t\r\nEND\r\n".to_vec()),
        ("x.vol".to_string(), b"END\n".to_vec()),
    ];
    let mut bank = Bank::from_bytes(&crate::models::write_pack(&members).unwrap()).unwrap();
    for (at, &rate) in rates.iter().enumerate() {
        let sound = Sound { rate, samples: (0..280).map(|n| (n % 40) * 500 - 10000).collect(), loop_start: None };
        bank.add_entry(&sound, "A", "B", if at == 0 { volume } else { 100 }, 60).unwrap();
    }
    bank.build().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trip_keeps_rate_samples_and_loop() {
        let sound = Sound { rate: 22050, samples: [0, 1000, -1000, 32767, -32768].repeat(10), loop_start: Some(5) };
        assert_eq!(read_wav(&wav_bytes(&sound).unwrap()).unwrap(), sound);
    }

    #[test]
    fn adpcm_encodes_and_decodes_close_to_the_source() {
        let samples: Vec<i64> = (0..28 * 40).map(|i| (10000.0 * (i as f64 / 7.0).sin()) as i64).collect();
        let (decoded, loop_start) = decode_adpcm(&encode_adpcm(&samples, None, true).unwrap(), true).unwrap();
        assert_eq!(loop_start, None);
        assert!(snr_db(&samples, &decoded[..samples.len()]) > 30.0);
    }

    #[test]
    fn a_loop_is_kept_through_the_encoder() {
        let samples: Vec<i64> = (0..28 * 4).map(|i| (i % 50) * 100).collect();
        let prepared = prepare_loop(&Sound { rate: 22050, samples, loop_start: Some(30) }).unwrap();
        assert_eq!(prepared.loop_start, Some(56));
        let data = encode_adpcm(&prepared.samples, prepared.loop_start, false).unwrap();
        assert_eq!(decode_adpcm(&data, true).unwrap().1, Some(56));
        assert_eq!(vag_length(&data), data.len());
    }

    #[test]
    fn columns_split_as_the_engine_does() {
        assert_eq!(tokenize(b"1\tA  B C"), vec![b"1".to_vec(), b"A".to_vec(), b"B".to_vec(), b"C".to_vec()]);
        assert_eq!(tokenize(b"A  "), vec![b"A".to_vec(), Vec::new()]);
        assert_eq!(atoi(b"  -12x"), -12);
        assert_eq!(s8(200), -56);
    }
}
