//! Sequences (.sq) as standard MIDI files, and back: the music and ambient loops of the sound
//! banks for mods (docs/assets/music.md).
//!
//! A sequence exports as an SMF of format 0 holding its events as they are, at its division, with
//! the sequence's name and markers `loopStart` and `loopEnd` where its loop is; program numbers
//! are the bank's programs (soundfont.rs writes them as a SoundFont to hear the file with). A MIDI
//! file builds back with the disc's sequence as the template: an unedited export gives the disc's
//! bytes, as a sequence is nothing but its events. Any other MIDI file (format 0 or 1, any
//! division) is merged into one track at 480 ticks a quarter note and keeps what the game's
//! driver plays: notes, controllers, program changes, pitch bends and tempo. It loops at the
//! `loopStart` and `loopEnd` markers (or the disc's controllers for them); without either, where
//! the disc's sequence loops, the whole tune loops.

use std::collections::BTreeSet;
use std::path::Path;

use crate::sequence::{
    Event, Message, Sequence, DATA_ENTRY, DATA_ENTRY_LSB, DIVISION, LOOP_END, LOOP_START, META_END, META_MARKER,
    META_NAME, META_TEMPO, META_TEXT, NRPN_MSB,
};
use crate::soundbank::{Bank, Pack};
use crate::{bail, Error, Result};

pub const SUFFIX: &str = ".mid";
pub const MARKER_START: &str = "loopStart";
pub const MARKER_END: &str = "loopEnd";
/// The tempo a MIDI file without one plays at (120 quarter notes a minute), given to the game's
/// driver as an event.
pub const DEFAULT_TEMPO: u32 = 500_000;

/// A standard MIDI file: its format, division and tracks of (tick, message), end-of-track events
/// left out and their ticks kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Smf {
    pub format: u16,
    pub division: u16,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub events: Vec<(u64, Message)>,
    pub end: u64,
}

fn be16(data: &[u8], at: usize) -> Result<u16> {
    match data.get(at..at + 2) {
        Some(b) => Ok(u16::from_be_bytes([b[0], b[1]])),
        None => bail!("the MIDI file ends early"),
    }
}

fn be32(data: &[u8], at: usize) -> Result<u32> {
    match data.get(at..at + 4) {
        Some(b) => Ok(u32::from_be_bytes(b.try_into().unwrap())),
        None => bail!("the MIDI file ends early"),
    }
}

fn read_vlq(data: &[u8], mut at: usize) -> Result<(u32, usize)> {
    let mut value = 0u32;
    for _ in 0..4 {
        let Some(&byte) = data.get(at) else {
            bail!("a track ends early");
        };
        at += 1;
        value = value << 7 | (byte & 0x7F) as u32;
        if byte & 0x80 == 0 {
            return Ok((value, at));
        }
    }
    bail!("a delta time of more than four bytes")
}

/// The MIDI data of a file: an SMF, or the SMF inside a RIFF MIDI file (.rmi).
fn smf_bytes(data: &[u8]) -> Result<&[u8]> {
    if data.get(..4) == Some(b"RIFF") && data.get(8..12) == Some(b"RMID") {
        let mut at = 12;
        while at + 8 <= data.len() {
            let size = u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap()) as usize;
            if &data[at..at + 4] == b"data" {
                return data.get(at + 8..at + 8 + size).ok_or_else(|| Error::invalid("the RIFF MIDI file ends early"));
            }
            at += 8 + size + size % 2;
        }
        bail!("a RIFF MIDI file without its data chunk");
    }
    Ok(data)
}

impl Smf {
    pub fn read(data: &[u8]) -> Result<Smf> {
        let data = smf_bytes(data)?;
        if data.get(..4) != Some(b"MThd") {
            bail!("not a MIDI file (no MThd)");
        }
        let header = be32(data, 4)? as usize;
        let (format, count, division) = (be16(data, 8)?, be16(data, 10)?, be16(data, 12)?);
        if header < 6 || format > 2 {
            bail!("not a MIDI file of format 0, 1 or 2");
        }
        if division & 0x8000 != 0 || division == 0 {
            bail!("the MIDI file counts time in SMPTE frames; save it with ticks per quarter note");
        }
        let mut tracks = Vec::new();
        let mut at = 8 + header;
        while at + 8 <= data.len() && tracks.len() < count as usize {
            let size = be32(data, at + 4)? as usize;
            let Some(chunk) = data.get(at + 8..at + 8 + size) else {
                bail!("track {} runs past the end of the file", tracks.len());
            };
            if &data[at..at + 4] == b"MTrk" {
                tracks.push(
                    read_track(chunk).map_err(|error| Error::invalid(format!("track {}: {error}", tracks.len())))?,
                );
            }
            at += 8 + size;
        }
        Ok(Smf { format, division, tracks })
    }

    /// The file: format 0 (one track) or 1, running status, a note-off with velocity 64.
    pub fn write(&self) -> Vec<u8> {
        let mut out = b"MThd".to_vec();
        out.extend(6u32.to_be_bytes());
        out.extend(self.format.to_be_bytes());
        out.extend((self.tracks.len() as u16).to_be_bytes());
        out.extend(self.division.to_be_bytes());
        for track in &self.tracks {
            let mut body = Vec::new();
            let mut running = None;
            let mut tick = 0;
            let end = (track.end, Message::end());
            for (at, message) in track.events.iter().chain(std::iter::once(&end)) {
                body.extend(crate::sequence::vlq((at - tick) as u32));
                tick = *at;
                let (status, mut data) = message.bytes();
                if let Message::NoteOff { .. } = message {
                    data.push(64);
                }
                if status == 0xFF {
                    running = None;
                    body.push(status);
                } else if Some(status) != running {
                    running = Some(status);
                    body.push(status);
                }
                body.extend(data);
            }
            out.extend(b"MTrk");
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
        out
    }
}

fn read_track(data: &[u8]) -> Result<Track> {
    let mut track = Track::default();
    let (mut at, mut tick) = (0, 0u64);
    let mut running: Option<u8> = None;
    while at < data.len() {
        let (delta, next) = read_vlq(data, at)?;
        at = next;
        tick += delta as u64;
        let Some(&first) = data.get(at) else {
            bail!("the track ends early");
        };
        let status = if first & 0x80 != 0 {
            at += 1;
            first
        } else {
            running.ok_or_else(|| Error::invalid(format!("a data byte with no status at 0x{at:x}")))?
        };
        match status {
            0xFF => {
                let kind = *data.get(at).ok_or_else(|| Error::invalid("the track ends early"))?;
                let (length, next) = read_vlq(data, at + 1)?;
                let body =
                    data.get(next..next + length as usize).ok_or_else(|| Error::invalid("the track ends early"))?;
                at = next + length as usize;
                if kind == META_END {
                    track.end = tick;
                    break;
                }
                track.events.push((tick, Message::Meta { kind, data: body.to_vec() }));
            }
            0xF0 | 0xF7 => {
                let (length, next) = read_vlq(data, at)?;
                at = next + length as usize;
            }
            0xF1..=0xFE => bail!("system message 0x{status:02X} in a track"),
            _ => {
                running = Some(status);
                // a note-off has its velocity here
                let length = if status & 0xF0 == 0x80 { 2 } else { crate::sequence::data_length(status) };
                let Some(bytes) = data.get(at..at + length) else {
                    bail!("the track ends early");
                };
                if bytes.iter().any(|&byte| byte & 0x80 != 0) {
                    bail!("a data byte with bit 7 set at 0x{at:x}");
                }
                at += length;
                track.events.push((tick, Message::from_bytes(status, bytes)));
            }
        }
    }
    track.end = track.end.max(track.events.last().map_or(0, |(tick, _)| *tick));
    Ok(track)
}

fn meta_text(kind: u8, text: &str) -> Message {
    Message::Meta { kind, data: text.as_bytes().to_vec() }
}

/// Where a run of messages loops from and to: the data entries after NRPN 0 and NRPN 1
/// (controller 6 after 99 = 0; 38, else 6, after 99 = 1), by index.
fn loop_events<'a>(messages: impl Iterator<Item = &'a Message>) -> (Option<usize>, Option<usize>) {
    let mut selected = [None; 16];
    let (mut start, mut end) = (None, None);
    for (index, message) in messages.enumerate() {
        if let Message::Control { channel, controller, value } = *message {
            let channel = channel as usize;
            match (controller, selected[channel]) {
                (NRPN_MSB, _) => selected[channel] = Some(value),
                (DATA_ENTRY, Some(LOOP_START)) if start.is_none() => start = Some(index),
                (DATA_ENTRY, Some(LOOP_END)) if end.is_none() => end = Some(index),
                (DATA_ENTRY_LSB, Some(LOOP_END)) => end = Some(index),
                _ => {}
            }
        }
    }
    (start, end)
}

/// The sequence as a MIDI file (see the module's doc).
pub fn to_midi(sequence: &Sequence, name: &str) -> Vec<u8> {
    let (start, end) = loop_events(sequence.events.iter().map(|event| &event.message));
    let mut track = Track::default();
    track.events.push((0, meta_text(META_NAME, name)));
    for ((index, event), tick) in sequence.events.iter().enumerate().zip(sequence.ticks()) {
        if Some(index) == start {
            track.events.push((tick, meta_text(META_MARKER, MARKER_START)));
        }
        if Some(index) == end {
            track.events.push((tick, meta_text(META_MARKER, MARKER_END)));
        }
        if event.message.is_end() {
            track.end = tick;
        } else {
            track.events.push((tick, event.message.clone()));
        }
    }
    Smf { format: 0, division: sequence.division, tracks: vec![track] }.write()
}

/// The sequence a .sq file's bytes hold, as a MIDI file.
pub fn export(sq: &[u8], name: &str) -> Result<Vec<u8>> {
    Ok(to_midi(&Sequence::read(sq)?, name))
}

/// Whether a marker or text event names a loop's start (Some(true)) or end: loopStart, loop_end,
/// "LOOP START", ignoring case.
fn loop_marker(message: &Message) -> Option<bool> {
    let Message::Meta { kind: META_MARKER | META_TEXT, data } = message else {
        return None;
    };
    let name: String = String::from_utf8_lossy(data).chars().filter(char::is_ascii_alphanumeric).collect();
    match name.to_lowercase().as_str() {
        "loopstart" => Some(true),
        "loopend" => Some(false),
        _ => None,
    }
}

/// What a MIDI file gives a sequence: its events at the file's own times, merged and kept as the
/// game's driver plays them, and its end, loop markers and whether it named its loop by the
/// disc's controllers.
struct Merged {
    events: Vec<(u64, Message)>,
    end: u64,
    markers: (Option<u64>, Option<u64>),
    nrpn_loop: bool,
}

fn merged(smf: &Smf) -> Result<Merged> {
    if smf.format == 2 {
        bail!("a MIDI file of format 2 holds separate songs; save it as format 0 or 1");
    }
    let scale = |tick: u64| (tick * DIVISION as u64 + smf.division as u64 / 2) / smf.division as u64;
    let mut all: Vec<(u64, usize, usize, &Message)> = Vec::new();
    let mut end = 0;
    for (number, track) in smf.tracks.iter().enumerate() {
        end = end.max(scale(track.end));
        all.extend(
            track.events.iter().enumerate().map(|(index, (tick, message))| (scale(*tick), number, index, message)),
        );
    }
    all.sort_by_key(|&(tick, track, index, _)| (tick, track, index));
    let mut out = Merged { events: Vec::new(), end, markers: (None, None), nrpn_loop: false };
    // the parameter each channel's data entries go to: Some(the disc's loop NRPN), or None for
    // RPNs and other NRPNs, which the driver would misread, and are left out with their entries
    let mut selected: [Option<u8>; 16] = [None; 16];
    for (tick, _, _, message) in all {
        let kept = match *message {
            Message::NoteOn { channel, key, velocity: 0 } => Some(Message::NoteOff { channel, key }),
            Message::NoteOn { .. } | Message::NoteOff { .. } | Message::Program { .. } | Message::PitchBend { .. } => {
                Some(message.clone())
            }
            Message::KeyPressure { .. } | Message::ChannelPressure { .. } => None,
            Message::Control { channel, controller, value } => {
                let selected = &mut selected[channel as usize];
                match controller {
                    // bank select: a port has one bank
                    0 | 32 => None,
                    NRPN_MSB if value == LOOP_START || value == LOOP_END => {
                        *selected = Some(value);
                        out.nrpn_loop = true;
                        Some(message.clone())
                    }
                    NRPN_MSB | 98 | 100 | 101 => {
                        *selected = None;
                        None
                    }
                    DATA_ENTRY | DATA_ENTRY_LSB if selected.is_some() => Some(message.clone()),
                    DATA_ENTRY | DATA_ENTRY_LSB | 96 | 97 => None,
                    _ => Some(message.clone()),
                }
            }
            Message::Meta { kind: META_TEMPO, ref data } if data.len() == 3 => Some(message.clone()),
            Message::Meta { .. } => {
                match loop_marker(message) {
                    Some(true) if out.markers.0.is_none() => out.markers.0 = Some(tick),
                    Some(false) if out.markers.1.is_none() => out.markers.1 = Some(tick),
                    _ => {}
                }
                None
            }
        };
        if let Some(message) = kept {
            out.events.push((tick, message));
        }
    }
    out.end = out.end.max(out.events.last().map_or(0, |(tick, _)| *tick));
    Ok(out)
}

/// The events at their ticks as a sequence, ending at `end`.
fn sequence_of(events: &[(u64, Message)], end: u64) -> Sequence {
    let mut out = Vec::with_capacity(events.len() + 1);
    let mut last = 0;
    for (tick, message) in events.iter().chain(std::iter::once(&(end, Message::end()))) {
        out.push(Event::new((tick - last) as u32, message.clone()));
        last = *tick;
    }
    Sequence { division: DIVISION, events: out }
}

/// Inserts a message at a tick: before the events there (`first`), or after all but their
/// note-ons.
fn insert(events: &mut Vec<(u64, Message)>, tick: u64, message: Message, first: bool) {
    let at = events
        .iter()
        .position(|(at, existing)| *at > tick || (*at == tick && (first || matches!(existing, Message::NoteOn { .. }))))
        .unwrap_or(events.len());
    events.insert(at, (tick, message));
}

/// The controllers that make the game loop from `start` to `end`, as the disc's sequences give
/// them: NRPN 0 ten ticks before the start and its data entry at the start; NRPN 1, data entry and
/// its LSB at the end, ten ticks apart.
fn add_loop(events: &mut Vec<(u64, Message)>, start: u64, end: u64) {
    let control = |controller, value| Message::Control { channel: 0, controller, value };
    insert(events, start, control(DATA_ENTRY, 0), true);
    insert(events, start.saturating_sub(10), control(NRPN_MSB, LOOP_START), true);
    insert(events, end.saturating_sub(20).max(start), control(NRPN_MSB, LOOP_END), false);
    insert(events, end.saturating_sub(10).max(start), control(DATA_ENTRY, 0), false);
    insert(events, end, control(DATA_ENTRY_LSB, 0), false);
}

/// The sequence a MIDI file gives, with the disc's sequence it replaces as the template (see the
/// module's doc). `has_program` says whether the bank has a program; a program the bank lacks
/// (and the template does not name) is refused, as is a channel that plays a note before it is
/// given a program.
pub fn from_midi(midi: &[u8], template: &Sequence, has_program: &dyn Fn(u8) -> bool) -> Result<Sequence> {
    let smf = Smf::read(midi)?;
    let mut merged = merged(&smf)?;
    // markers moved off the controllers (in an editor) move the loop
    let (start, stop) = loop_events(merged.events.iter().map(|(_, message)| message));
    let tick = |index: Option<usize>| index.map(|index| merged.events[index].0);
    if merged.markers.0.is_some() && merged.markers.1.is_some() && merged.markers != (tick(start), tick(stop)) {
        merged.events.retain(|(_, message)| {
            !matches!(message, Message::Control { controller: NRPN_MSB | DATA_ENTRY | DATA_ENTRY_LSB, .. })
        });
        merged.nrpn_loop = false;
    }
    let plain = sequence_of(&merged.events, merged.end);
    if plain == *template {
        return Ok(plain);
    }
    let (template_start, template_end) = loop_events(template.events.iter().map(|event| &event.message));
    let mut end = merged.end;
    if !merged.nrpn_loop {
        let looped = template_start.is_some() && template_end.is_some();
        let (start, stop) = match merged.markers {
            (Some(start), Some(stop)) => (start, stop),
            (None, None) if looped => (0, merged.end),
            (Some(_), None) | (None, Some(_)) => bail!("give both markers {MARKER_START} and {MARKER_END}, or neither"),
            _ => (0, 0),
        };
        if stop > start {
            add_loop(&mut merged.events, start, stop);
            // room after the jump back, as the disc's sequences leave
            end = end.max(stop + DIVISION as u64);
        } else if merged.markers.0.is_some() {
            bail!("{MARKER_END} comes before {MARKER_START}");
        }
    }
    // a MIDI file plays at 120 until its first tempo event
    let tempo_at_start = merged
        .events
        .iter()
        .take_while(|(tick, _)| *tick == 0)
        .any(|(_, message)| matches!(message, Message::Meta { kind: META_TEMPO, .. }));
    if !tempo_at_start {
        insert(&mut merged.events, 0, Message::tempo(DEFAULT_TEMPO), true);
    }
    let sequence = sequence_of(&merged.events, end);
    check_programs(&sequence, template, has_program)?;
    Ok(sequence)
}

/// Refuses a program the bank lacks (unless the template names it too) and a note on a channel
/// that has had no program change.
fn check_programs(sequence: &Sequence, template: &Sequence, has_program: &dyn Fn(u8) -> bool) -> Result<()> {
    let known: BTreeSet<u8> = template.programs().into_iter().collect();
    let missing: Vec<u8> =
        sequence.programs().into_iter().filter(|&program| !has_program(program) && !known.contains(&program)).collect();
    if !missing.is_empty() {
        let shown: Vec<String> = missing.iter().map(u8::to_string).collect();
        bail!(
            "the bank has no program {}: a sequence plays the bank's programs (export the bank as .sf2 to hear them)",
            shown.join(", ")
        );
    }
    let mut given = [false; 16];
    for event in &sequence.events {
        match event.message {
            Message::Program { channel, .. } => given[channel as usize] = true,
            Message::NoteOn { channel, .. } if !given[channel as usize] => {
                bail!(
                    "channel {} plays a note before any program change; give each channel one of the bank's programs",
                    channel + 1
                )
            }
            _ => {}
        }
    }
    Ok(())
}

/// The .sq bytes a MIDI file gives in place of the disc's sequence (`original`), the bank's
/// programs checked when the bank is given.
pub fn build(midi: &[u8], original: &[u8], bank: Option<&Bank>) -> Result<Vec<u8>> {
    let template = Sequence::read(original)?;
    let has_program = |program: u8| match bank {
        Some(bank) => bank.hd.programs.get(program as usize).is_some_and(Option::is_some),
        None => true,
    };
    let sequence = from_midi(midi, &template, &has_program)?;
    if sequence == template {
        return Ok(original.to_vec());
    }
    sequence.write()
}

/// The stem of a member's name (BG_000_000 for BG_000_000.sq).
fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// A bank's sequences as MIDI files, named after them (BG_000_000.mid), and its programs as a
/// SoundFont (`<name>.sf2`) to hear them with. Empty for a bank without sequences.
pub fn export_bank(data: &[u8], name: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let bank = Bank::from_bytes(data)?;
    let mut out = Vec::new();
    for index in bank.pack.find("sq") {
        let member = &bank.pack.members[index];
        out.push((format!("{}{SUFFIX}", stem(&member.name)), export(&member.data, stem(&member.name))?));
    }
    if !out.is_empty() {
        out.push((format!("{name}{}", crate::soundfont::SUFFIX), crate::soundfont::write(&bank, name)?));
    }
    Ok(out)
}

/// The bank with the sequences a folder's MIDI files give (BG_000_000.mid for BG_000_000.sq) in
/// place of its own; other files (the exported .sf2) are not read.
pub fn build_bank(folder: &Path, original: &[u8]) -> Result<Vec<u8>> {
    let mut pack = Pack::parse(original)?;
    let bank = Bank::from_bytes(original)?;
    let sequences = pack.find("sq");
    let mut changed = false;
    let mut paths: Vec<_> = std::fs::read_dir(folder)?.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let file = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if !path.is_file() || !file.to_lowercase().ends_with(SUFFIX) {
            continue;
        }
        let wanted = &file[..file.len() - SUFFIX.len()];
        let Some(&index) =
            sequences.iter().find(|&&index| stem(&pack.members[index].name).eq_ignore_ascii_case(wanted))
        else {
            let names: Vec<&str> = sequences.iter().map(|&index| stem(&pack.members[index].name)).collect();
            bail!(
                "{file}: the bank has no sequence {wanted} (it has {})",
                if names.is_empty() { "none".to_string() } else { names.join(", ") }
            );
        };
        let built = build(&std::fs::read(&path)?, &pack.members[index].data, Some(&bank))
            .map_err(|error| Error::invalid(format!("{file}: {error}")))?;
        if built != pack.members[index].data {
            pack.members[index].data = built;
            changed = true;
        }
    }
    Ok(if changed { pack.build() } else { original.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(delta: u32, controller: u8, value: u8) -> Event {
        Event::new(delta, Message::Control { channel: 0, controller, value })
    }

    fn tune() -> Sequence {
        let note = |delta, key, on: bool| {
            let message = if on {
                Message::NoteOn { channel: 2, key, velocity: 90 }
            } else {
                Message::NoteOff { channel: 2, key }
            };
            Event::new(delta, message)
        };
        Sequence {
            division: DIVISION,
            events: vec![
                Event::new(0, Message::tempo(400_000)),
                Event::new(10, Message::Program { channel: 2, program: 1 }),
                control(0, NRPN_MSB, LOOP_START),
                control(10, DATA_ENTRY, 0),
                note(0, 60, true),
                note(480, 60, false),
                note(0, 62, true),
                Event::new(0, Message::PitchBend { channel: 2, value: 0x2100 }),
                note(480, 62, false),
                control(0, NRPN_MSB, LOOP_END),
                control(10, DATA_ENTRY, 0),
                control(10, DATA_ENTRY_LSB, 0),
                Event::new(1000, Message::end()),
            ],
        }
    }

    #[test]
    fn an_unedited_export_builds_back_to_the_same_bytes() {
        let original = tune().write().unwrap();
        let midi = export(&original, "BG_999_000").unwrap();
        let smf = Smf::read(&midi).unwrap();
        assert_eq!((smf.format, smf.division, smf.tracks.len()), (0, 480, 1));
        let markers: Vec<(u64, Option<bool>)> = smf.tracks[0]
            .events
            .iter()
            .filter_map(|(tick, m)| loop_marker(m).map(|start| (*tick, Some(start))))
            .collect();
        assert_eq!(markers, [(20, Some(true)), (1000, Some(false))]);
        assert_eq!(build(&midi, &original, None).unwrap(), original);
    }

    /// An SMF of format 1, at 96 ticks a quarter note, with GM bank selects, an RPN and markers.
    fn foreign() -> Vec<u8> {
        let smf = Smf {
            format: 1,
            division: 96,
            tracks: vec![
                Track {
                    events: vec![
                        (0, Message::tempo(600_000)),
                        (0, meta_text(META_MARKER, "Loop Start")),
                        (192, meta_text(META_MARKER, "loop_end")),
                    ],
                    end: 192,
                },
                Track {
                    events: vec![
                        (0, Message::Control { channel: 3, controller: 0, value: 0 }),
                        (0, Message::Control { channel: 3, controller: 101, value: 0 }),
                        (0, Message::Control { channel: 3, controller: 6, value: 12 }),
                        (0, Message::Program { channel: 3, program: 1 }),
                        (0, Message::NoteOn { channel: 3, key: 64, velocity: 100 }),
                        (96, Message::NoteOn { channel: 3, key: 64, velocity: 0 }),
                        (96, Message::ChannelPressure { channel: 3, pressure: 5 }),
                    ],
                    end: 192,
                },
            ],
        };
        smf.write()
    }

    #[test]
    fn another_midi_file_keeps_what_the_driver_plays_and_loops_at_its_markers() {
        let template = tune();
        let built = from_midi(&foreign(), &template, &|program| program < 2).unwrap();
        let messages: Vec<(u64, Message)> =
            built.ticks().into_iter().zip(built.events.iter().map(|e| e.message.clone())).collect();
        let control = |controller, value| Message::Control { channel: 0, controller, value };
        assert_eq!(
            messages,
            [
                (0, control(NRPN_MSB, LOOP_START)),
                (0, control(DATA_ENTRY, 0)),
                (0, Message::tempo(600_000)),
                (0, Message::Program { channel: 3, program: 1 }),
                (0, Message::NoteOn { channel: 3, key: 64, velocity: 100 }),
                (480, Message::NoteOff { channel: 3, key: 64 }),
                (940, control(NRPN_MSB, LOOP_END)),
                (950, control(DATA_ENTRY, 0)),
                (960, control(DATA_ENTRY_LSB, 0)),
                (1440, Message::end()),
            ]
        );
        assert_eq!(Sequence::read(&built.write().unwrap()).unwrap(), built);
    }

    #[test]
    fn programs_the_bank_lacks_and_channels_without_one_are_refused() {
        let template = tune();
        // the template names program 1, so it may stay though the bank lacks it
        assert!(from_midi(&foreign(), &template, &|program| program == 0).is_ok());
        let one_shot = Sequence { division: DIVISION, events: vec![Event::new(0, Message::end())] };
        let error = from_midi(&foreign(), &one_shot, &|program| program == 0).unwrap_err();
        assert!(error.to_string().contains("no program 1"), "{error}");
        let smf = Smf {
            format: 0,
            division: 480,
            tracks: vec![Track { events: vec![(0, Message::NoteOn { channel: 0, key: 60, velocity: 1 })], end: 480 }],
        };
        let error = from_midi(&smf.write(), &template, &|_| true).unwrap_err();
        assert!(error.to_string().contains("channel 1 plays a note"), "{error}");
    }

    #[test]
    fn a_tune_without_a_loop_loops_whole_where_the_disc_s_did() {
        let smf = Smf {
            format: 0,
            division: 480,
            tracks: vec![Track {
                events: vec![
                    (0, Message::Program { channel: 0, program: 0 }),
                    (0, Message::NoteOn { channel: 0, key: 60, velocity: 1 }),
                    (1920, Message::NoteOff { channel: 0, key: 60 }),
                ],
                end: 1920,
            }],
        };
        let built = from_midi(&smf.write(), &tune(), &|_| true).unwrap();
        assert_eq!(loop_ticks(&built), (Some(0), Some(1920)));
        assert!(built.events.iter().any(|e| e.message == Message::tempo(DEFAULT_TEMPO)));
        let one_shot = Sequence { division: DIVISION, events: vec![Event::new(0, Message::end())] };
        let built = from_midi(&smf.write(), &one_shot, &|_| true).unwrap();
        assert_eq!(loop_ticks(&built), (None, None));
    }

    fn loop_ticks(sequence: &Sequence) -> (Option<u64>, Option<u64>) {
        let (start, end) = loop_events(sequence.events.iter().map(|event| &event.message));
        let ticks = sequence.ticks();
        (start.map(|at| ticks[at]), end.map(|at| ticks[at]))
    }

    #[test]
    fn markers_moved_in_an_editor_move_the_loop() {
        let original = tune().write().unwrap();
        let mut smf = Smf::read(&export(&original, "BG_999_000").unwrap()).unwrap();
        for (tick, message) in &mut smf.tracks[0].events {
            match loop_marker(message) {
                Some(true) => *tick = 480,
                Some(false) => *tick = 960,
                None => {}
            }
        }
        smf.tracks[0].events.sort_by_key(|(tick, _)| *tick);
        let built = Sequence::read(&build(&smf.write(), &original, None).unwrap()).unwrap();
        assert_eq!(loop_ticks(&built), (Some(480), Some(960)));
        let controls = built.events.iter().filter(|e| matches!(e.message, Message::Control { .. })).count();
        assert_eq!(controls, 5);
    }

    /// Every sequence of an extracted disc (C2S_CORPUS) exports to MIDI and builds back to the same
    /// bytes, and names only programs its bank has.
    #[test]
    fn corpus_sequences_round_trip_through_midi() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let banks: Vec<std::path::PathBuf> = crate::files_under(std::path::Path::new(&corpus))
            .unwrap()
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("snd")))
            .collect();
        let (mut sequences, mut missing) = (0, Vec::new());
        for path in &banks {
            let data = std::fs::read(path).unwrap();
            let bank = Bank::from_bytes(&data).unwrap();
            for index in bank.pack.find("sq") {
                let member = &bank.pack.members[index];
                let midi = export(&member.data, stem(&member.name)).unwrap();
                let built = build(&midi, &member.data, Some(&bank)).unwrap();
                assert_eq!(built, member.data, "{} {}", path.display(), member.name);
                let sequence = Sequence::read(&member.data).unwrap();
                for program in sequence.programs() {
                    if !bank.hd.programs.get(program as usize).is_some_and(Option::is_some) {
                        missing.push(format!("{} {} program {program}", path.display(), member.name));
                    }
                }
                sequences += 1;
            }
        }
        eprintln!(
            "{sequences} sequences in {} banks round-trip through MIDI; programs missing: {missing:?}",
            banks.len()
        );
    }
}
