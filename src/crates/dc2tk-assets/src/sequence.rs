//! Sony sequences (.sq): the music of the BG_* banks and the ambient loops of the SR_* banks, which
//! the IOP's MIDI driver plays with the programs of the bank holding them (docs/game/music.md).
//!
//! A Vers chunk (1.0), a Sequ chunk naming one Midi chunk, the Midi chunk with one data block: the
//! offset of the events in the block (6) and the division, then one track of MIDI events in Sony's
//! compressed form, padded with 0xFF to 16 bytes. The compression: running status (a meta event
//! ends it), note-offs that carry only the key, and a delta time of 0 left out when the channel
//! event before wrote its status byte and set bit 7 of its last data byte (an event in running
//! status never sets it, as a one-byte message would then read as a status byte, nor does a meta
//! event). Every one of the disc's
//! 447 sequences reads to a Sequence and writes back byte for byte, so a sequence is its events.

use crate::{bail, Result};

/// The division (ticks per quarter note) of every sequence on the disc.
pub const DIVISION: u16 = 480;
pub const META_TEXT: u8 = 0x01;
pub const META_NAME: u8 = 0x03;
pub const META_MARKER: u8 = 0x06;
pub const META_END: u8 = 0x2F;
pub const META_TEMPO: u8 = 0x51;
/// The controllers that mark a loop: NRPN 0 (controller 99 = 0, then data entry 6 = 0) starts it,
/// NRPN 1 (99 = 1, 6 = 0, 38 = 0) jumps back to the start.
pub const NRPN_MSB: u8 = 99;
pub const DATA_ENTRY: u8 = 6;
pub const DATA_ENTRY_LSB: u8 = 38;
pub const LOOP_START: u8 = 0;
pub const LOOP_END: u8 = 1;

const CREATOR: &[u8; 4] = b"IECS";
const NONE: u32 = 0xFFFF_FFFF;
const MIDI_CHUNK: usize = 0x30;
const EVENTS: usize = 0x4A;

/// A MIDI message as a sequence holds it. A note-off has no velocity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    NoteOff {
        channel: u8,
        key: u8,
    },
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    KeyPressure {
        channel: u8,
        key: u8,
        pressure: u8,
    },
    Control {
        channel: u8,
        controller: u8,
        value: u8,
    },
    Program {
        channel: u8,
        program: u8,
    },
    ChannelPressure {
        channel: u8,
        pressure: u8,
    },
    /// 14 bits, 0x2000 centre.
    PitchBend {
        channel: u8,
        value: u16,
    },
    /// A meta event: tempo (0x51, three bytes of microseconds per quarter note), end (0x2F) and
    /// any other kind, as SMF writes them.
    Meta {
        kind: u8,
        data: Vec<u8>,
    },
}

impl Message {
    pub fn tempo(microseconds: u32) -> Message {
        Message::Meta { kind: META_TEMPO, data: microseconds.to_be_bytes()[1..].to_vec() }
    }

    pub fn end() -> Message {
        Message::Meta { kind: META_END, data: Vec::new() }
    }

    pub fn is_end(&self) -> bool {
        matches!(self, Message::Meta { kind: META_END, .. })
    }

    pub fn channel(&self) -> Option<u8> {
        match *self {
            Message::NoteOff { channel, .. }
            | Message::NoteOn { channel, .. }
            | Message::KeyPressure { channel, .. }
            | Message::Control { channel, .. }
            | Message::Program { channel, .. }
            | Message::ChannelPressure { channel, .. }
            | Message::PitchBend { channel, .. } => Some(channel),
            Message::Meta { .. } => None,
        }
    }

    /// The status byte of a channel message (0xFF for a meta event) and its data bytes, a
    /// note-off's with no velocity.
    pub fn bytes(&self) -> (u8, Vec<u8>) {
        match self {
            &Message::NoteOff { channel, key } => (0x80 | channel, vec![key]),
            &Message::NoteOn { channel, key, velocity } => (0x90 | channel, vec![key, velocity]),
            &Message::KeyPressure { channel, key, pressure } => (0xA0 | channel, vec![key, pressure]),
            &Message::Control { channel, controller, value } => (0xB0 | channel, vec![controller, value]),
            &Message::Program { channel, program } => (0xC0 | channel, vec![program]),
            &Message::ChannelPressure { channel, pressure } => (0xD0 | channel, vec![pressure]),
            &Message::PitchBend { channel, value } => (0xE0 | channel, vec![(value & 0x7F) as u8, (value >> 7) as u8]),
            Message::Meta { kind, data } => (0xFF, [vec![*kind], vlq(data.len() as u32), data.clone()].concat()),
        }
    }

    /// A channel message from its status byte and data bytes (bit 7 cleared); a note-off reads
    /// only the key.
    pub fn from_bytes(status: u8, data: &[u8]) -> Message {
        let channel = status & 0x0F;
        let at = |i: usize| data.get(i).copied().unwrap_or(0);
        match status & 0xF0 {
            0x80 => Message::NoteOff { channel, key: at(0) },
            0x90 => Message::NoteOn { channel, key: at(0), velocity: at(1) },
            0xA0 => Message::KeyPressure { channel, key: at(0), pressure: at(1) },
            0xB0 => Message::Control { channel, controller: at(0), value: at(1) },
            0xC0 => Message::Program { channel, program: at(0) },
            0xD0 => Message::ChannelPressure { channel, pressure: at(0) },
            _ => Message::PitchBend { channel, value: at(0) as u16 | (at(1) as u16) << 7 },
        }
    }
}

/// The data bytes a channel message of this status takes in a sequence (a note-off one).
pub fn data_length(status: u8) -> usize {
    match status & 0xF0 {
        0x80 | 0xC0 | 0xD0 => 1,
        _ => 2,
    }
}

/// An event: its delta time in ticks and its message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub delta: u32,
    pub message: Message,
}

impl Event {
    pub fn new(delta: u32, message: Message) -> Event {
        Event { delta, message }
    }
}

/// A sequence: its division and its events, the last the end of the track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub division: u16,
    pub events: Vec<Event>,
}

/// A variable-length quantity: 7 bits a byte, most significant first, bit 7 set on all but the
/// last.
pub fn vlq(value: u32) -> Vec<u8> {
    let mut out = vec![(value & 0x7F) as u8];
    let mut rest = value >> 7;
    while rest > 0 {
        out.insert(0, (rest & 0x7F) as u8 | 0x80);
        rest >>= 7;
    }
    out
}

/// The value of a variable-length quantity at `at` and where it ends: at most four bytes, as a
/// MIDI file has them (the reader of sequences and of MIDI files).
pub(crate) fn read_vlq(data: &[u8], mut at: usize) -> Result<(u32, usize)> {
    let start = at;
    let mut value = 0u32;
    for _ in 0..4 {
        let Some(&byte) = data.get(at) else {
            bail!("the data ends inside a variable-length number at 0x{start:x}");
        };
        at += 1;
        value = value << 7 | (byte & 0x7F) as u32;
        if byte & 0x80 == 0 {
            return Ok((value, at));
        }
    }
    bail!("a variable-length number of more than four bytes at 0x{start:x}")
}

/// read_vlq, refusing a number written in more bytes than it needs, which write would not give
/// back (a sequence is read only as the disc's are written).
fn read_canonical_vlq(data: &[u8], at: usize) -> Result<(u32, usize)> {
    let (value, end) = read_vlq(data, at)?;
    if end - at != vlq(value).len() {
        bail!("a variable-length number written in more bytes than it needs at 0x{at:x}");
    }
    Ok((value, end))
}

/// The bytes of the header up to the Midi chunk's data block, for a file of `size` bytes.
fn header(size: usize, division: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(EVENTS);
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], size: u32| {
        out.extend(CREATOR);
        out.extend(kind.iter().rev());
        out.extend(size.to_le_bytes());
    };
    chunk(&mut out, b"Vers", 0x10);
    out.extend([0, 0, 1, 0]); // reserved, version 1.0
    chunk(&mut out, b"Sequ", 0x20);
    out.extend((size as u32).to_le_bytes());
    // song, MIDI, SE sequence and SE song chunks: only MIDI
    for offset in [NONE, MIDI_CHUNK as u32, NONE, NONE] {
        out.extend(offset.to_le_bytes());
    }
    chunk(&mut out, b"Midi", (size - MIDI_CHUNK) as u32);
    out.extend(0u32.to_le_bytes()); // max index: one data block
    out.extend(0x14u32.to_le_bytes()); // its offset from the chunk
    out.extend(6u32.to_le_bytes()); // the events' offset from the block
    out.extend(division.to_le_bytes());
    out
}

impl Sequence {
    /// Reads a .sq file. Refuses one laid out otherwise than the disc's, or one that write would
    /// not give back byte for byte.
    pub fn read(data: &[u8]) -> Result<Sequence> {
        if data.len() < EVENTS + 4 || !data.len().is_multiple_of(16) {
            bail!("not a sequence: {} bytes, not a multiple of 16 past the header", data.len());
        }
        let division = u16::from_le_bytes([data[EVENTS - 2], data[EVENTS - 1]]);
        if data[..EVENTS] != header(data.len(), division)[..] {
            bail!("not a sequence laid out as the disc's are (Vers 1.0, one Midi chunk of one block)");
        }
        if division == 0 || division & 0x8000 != 0 {
            bail!("division {division} is not ticks per quarter note");
        }
        let mut events: Vec<Event> = Vec::new();
        let mut at = EVENTS;
        let mut running: Option<u8> = None;
        // whether the event before set its flag (this one's delta of 0 is left out), and whether
        // it wrote its status byte, which it must to set the flag
        let mut flagged = false;
        let mut could_flag = false;
        loop {
            let start = at;
            let delta = if flagged {
                0
            } else {
                let (delta, next) = read_canonical_vlq(data, at)?;
                if could_flag && delta == 0 {
                    bail!("a delta time of 0 written out at 0x{start:x}, where the event before leaves it out");
                }
                at = next;
                delta
            };
            let Some(&first) = data.get(at) else {
                bail!("the events run past the end of the file at 0x{start:x}");
            };
            let written = first & 0x80 != 0;
            let status = if written {
                if Some(first) == running {
                    bail!("a status byte repeated where running status holds at 0x{at:x}");
                }
                at += 1;
                first
            } else {
                running.ok_or_else(|| crate::Error::invalid(format!("a data byte with no status at 0x{at:x}")))?
            };
            let message = if status == 0xFF {
                let Some(&kind) = data.get(at) else {
                    bail!("the events run past the end of the file at 0x{at:x}");
                };
                let (length, next) = read_canonical_vlq(data, at + 1)?;
                let Some(body) = data.get(next..next + length as usize) else {
                    bail!("a meta event runs past the end of the file at 0x{at:x}");
                };
                at = next + length as usize;
                running = None;
                (flagged, could_flag) = (false, false);
                Message::Meta { kind, data: body.to_vec() }
            } else if status >= 0xF0 {
                bail!("system message 0x{status:02X} at 0x{start:x}: sequences hold channel messages and meta events");
            } else {
                running = Some(status);
                let length = data_length(status);
                let Some(bytes) = data.get(at..at + length) else {
                    bail!("the events run past the end of the file at 0x{at:x}");
                };
                if bytes[..length - 1].iter().any(|&byte| byte & 0x80 != 0) {
                    bail!("a data byte with bit 7 set before the last at 0x{at:x}");
                }
                at += length;
                flagged = bytes[length - 1] & 0x80 != 0;
                if flagged && !written {
                    bail!("a flag set on an event in running status at 0x{start:x}");
                }
                could_flag = written;
                let clean: Vec<u8> = bytes.iter().map(|&byte| byte & 0x7F).collect();
                Message::from_bytes(status, &clean)
            };
            let end = message.is_end();
            events.push(Event { delta, message });
            if end {
                break;
            }
        }
        if data.len() - at >= 16 || data[at..].iter().any(|&byte| byte != 0xFF) {
            bail!("{} bytes after the end of the track, not 0xFF padding to 16", data.len() - at);
        }
        Ok(Sequence { division, events })
    }

    /// The .sq file: the events compressed as the disc's are (see the module's doc).
    pub fn write(&self) -> Result<Vec<u8>> {
        let events = &self.events;
        match events.iter().position(|event| event.message.is_end()) {
            Some(at) if at + 1 == events.len() => {}
            _ => bail!("a sequence ends with one end-of-track event, last"),
        }
        let mut body = Vec::new();
        let mut running: Option<u8> = None;
        let mut flagged = false;
        for (i, event) in events.iter().enumerate() {
            if !flagged {
                if event.delta >= 1 << 28 {
                    bail!("a delta time of {} ticks; at most 2^28 - 1", event.delta);
                }
                body.extend(vlq(event.delta));
            }
            let (status, mut data) = event.message.bytes();
            if let Some(channel) = event.message.channel() {
                if channel > 15 || data.iter().any(|&byte| byte > 0x7F) {
                    bail!("event {i}: {:?} has a value out of range", event.message);
                }
                // only an event that writes its status byte sets the flag: a one-byte message in
                // running status with bit 7 set would read as a status byte
                let written = Some(status) != running;
                if written {
                    body.push(status);
                    running = Some(status);
                }
                let next = events.get(i + 1);
                flagged = written && next.is_some_and(|next| next.delta == 0);
                if flagged {
                    *data.last_mut().unwrap() |= 0x80;
                }
            } else {
                body.push(status);
                running = None;
                flagged = false;
            }
            body.extend(data);
        }
        let size = EVENTS + body.len();
        let size = size + (16 - size % 16) % 16;
        let mut out = header(size, self.division);
        out.extend(body);
        out.resize(size, 0xFF);
        Ok(out)
    }

    /// The tick each event falls on.
    pub fn ticks(&self) -> Vec<u64> {
        let mut tick = 0u64;
        self.events
            .iter()
            .map(|event| {
                tick += event.delta as u64;
                tick
            })
            .collect()
    }

    /// The ticks of the loop's start and end (controller 99 = 0 and = 1), when the sequence has
    /// them.
    pub fn loop_ticks(&self) -> (Option<u64>, Option<u64>) {
        let mut found = (None, None);
        for (event, tick) in self.events.iter().zip(self.ticks()) {
            match event.message {
                Message::Control { controller: NRPN_MSB, value: LOOP_START, .. } if found.0.is_none() => {
                    found.0 = Some(tick);
                }
                Message::Control { controller: NRPN_MSB, value: LOOP_END, .. } if found.1.is_none() => {
                    found.1 = Some(tick);
                }
                _ => {}
            }
        }
        found
    }

    /// (program, key, velocity) of each note-on with a velocity, each channel at program 0 until
    /// its first program change.
    pub fn notes(&self) -> Vec<(i64, u8, u8)> {
        let mut programs = [0i64; 16];
        let mut out = Vec::new();
        for event in &self.events {
            match event.message {
                Message::Program { channel, program } => programs[channel as usize] = program as i64,
                Message::NoteOn { channel, key, velocity } if velocity > 0 => {
                    out.push((programs[channel as usize], key, velocity));
                }
                _ => {}
            }
        }
        out
    }

    /// The programs its program changes name, in order, once each.
    pub fn programs(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for event in &self.events {
            if let Message::Program { program, .. } = event.message {
                if !out.contains(&program) {
                    out.push(program);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(delta: u32, key: u8) -> Event {
        Event::new(delta, Message::NoteOn { channel: 1, key, velocity: 100 })
    }

    #[test]
    fn events_compress_as_the_disc_writes_them() {
        let sequence = Sequence {
            division: DIVISION,
            events: vec![
                Event::new(0, Message::tempo(500_000)),
                Event::new(10, Message::Program { channel: 1, program: 3 }),
                note(0, 60),
                note(0, 64),
                Event::new(0, Message::NoteOff { channel: 1, key: 60 }),
                Event::new(200, Message::NoteOff { channel: 1, key: 64 }),
                Event::new(0, Message::end()),
            ],
        };
        let written = sequence.write().unwrap();
        assert_eq!(written.len() % 16, 0);
        let events: &[u8] = &[
            0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // tempo; a meta event never sets the flag
            0x0A, 0xC1, 0x83, // program 3, flag: the next delta is left out
            0x91, 0x3C, 0xE4, // note-on, flag
            0x40, 0x64, // running status: no flag, so the next delta is written though 0
            0x00, 0x81, 0x3C, // note-off: the key only
            0x81, 0x48, 0x40, // delta 200, running status
            0x00, 0xFF, 0x2F, 0x00,
        ];
        assert_eq!(&written[EVENTS..EVENTS + events.len()], events);
        assert!(written[EVENTS + events.len()..].iter().all(|&byte| byte == 0xFF));
        assert_eq!(Sequence::read(&written).unwrap(), sequence);
    }

    #[test]
    fn variable_length_numbers_read_up_to_four_bytes_and_sequences_only_the_shortest() {
        assert_eq!(read_vlq(&[0x81, 0x48, 0x40], 0).unwrap(), (200, 2));
        assert_eq!(read_vlq(&[0xFF, 0xFF, 0xFF, 0x7F], 0).unwrap(), (0x0FFF_FFFF, 4));
        assert!(read_vlq(&[0x80, 0x80, 0x80, 0x80, 0x00], 0).unwrap_err().to_string().contains("more than four"));
        assert!(read_vlq(&[0x81], 0).unwrap_err().to_string().contains("ends inside"));
        // 0 in two bytes: a MIDI file may have it, a sequence the disc's way may not
        assert_eq!(read_vlq(&[0x80, 0x00], 0).unwrap(), (0, 2));
        assert!(read_canonical_vlq(&[0x80, 0x00], 0).unwrap_err().to_string().contains("more bytes than it needs"));
        assert_eq!(read_canonical_vlq(&[0x81, 0x48], 0).unwrap(), (200, 2));
    }

    #[test]
    fn a_sequence_without_an_end_is_refused() {
        let sequence = Sequence { division: DIVISION, events: vec![note(0, 60)] };
        assert!(sequence.write().is_err());
    }

    #[test]
    fn loops_are_found_by_their_nrpn() {
        let control = |delta, controller, value| Event::new(delta, Message::Control { channel: 0, controller, value });
        let sequence = Sequence {
            division: DIVISION,
            events: vec![
                control(5, NRPN_MSB, LOOP_START),
                control(0, DATA_ENTRY, 0),
                note(100, 60),
                control(20, NRPN_MSB, LOOP_END),
                Event::new(30, Message::end()),
            ],
        };
        assert_eq!(sequence.loop_ticks(), (Some(5), Some(125)));
        assert_eq!(Sequence::read(&sequence.write().unwrap()).unwrap(), sequence);
    }

    /// Every .sq of an extracted disc (DC2TK_CORPUS) reads and writes back byte for byte.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_sequences_round_trip() {
        let corpus = dc2tk_disc::samples::corpus();
        let paths: Vec<std::path::PathBuf> = crate::files_under(std::path::Path::new(&corpus))
            .unwrap()
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("sq")))
            .collect();
        assert!(!paths.is_empty(), "no .sq files under {corpus:?}");
        let mut events = 0;
        for path in &paths {
            let data = std::fs::read(path).unwrap();
            let sequence = Sequence::read(&data).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(sequence.write().unwrap(), data, "{}", path.display());
            events += sequence.events.len();
        }
        eprintln!("{} sequences, {events} events round-trip", paths.len());
    }
}
