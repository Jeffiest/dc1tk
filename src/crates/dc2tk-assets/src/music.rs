//! Sequences (.sq) as standard MIDI files, and back: the music and ambient loops of the sound
//! banks for mods (docs/game/music.md).
//!
//! A sequence exports as an SMF of format 0 holding its events as they are, at its division, with
//! the sequence's name and markers `loopStart` and `loopEnd` where its loop is; program numbers
//! are the bank's programs (soundfont.rs writes them as a SoundFont to hear the file with). A MIDI
//! file builds back with the disc's sequence as the template: an unedited export gives the disc's
//! bytes, as a sequence is nothing but its events. Any other MIDI file (format 0 or 1, any
//! division) is merged into one track at 480 ticks a quarter note and keeps what the game's
//! driver plays: notes, controllers, program changes, pitch bends and tempo. It loops at the
//! `loopStart` and `loopEnd` markers (or the disc's controllers for them); without either, where
//! the disc's sequence loops, the whole tune loops. It comes out shaped as the disc's sequences
//! are: channel 0 keeps the loop and no notes, every note channel is given volume, expression, pan
//! and pitch bend before its first note, and the first notes come after that set-up.

use std::collections::BTreeSet;
use std::path::Path;

use crate::bytes::{be16_at, be32_at};
use crate::sequence::{
    read_vlq, Event, Message, Sequence, DATA_ENTRY, DATA_ENTRY_LSB, DIVISION, LOOP_END, LOOP_START, META_END,
    META_MARKER, META_NAME, META_TEMPO, META_TEXT, NRPN_MSB,
};
use crate::soundbank::{self, Bank};
use crate::{bail, Context, Error, Result};
use dc2tk_disc::archive::PackMember;
use dc2tk_disc::files;

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
        let field = |at: usize| be16_at(data, at).context("the MIDI file's header");
        let header = be32_at(data, 4).context("the MIDI file's header")? as usize;
        let (format, count, division) = (field(8)?, field(10)?, field(12)?);
        if header < 6 || format > 2 {
            bail!("not a MIDI file of format 0, 1 or 2");
        }
        if division & 0x8000 != 0 || division == 0 {
            bail!("the MIDI file counts time in SMPTE frames; save it with ticks per quarter note");
        }
        let mut tracks = Vec::new();
        let mut at = 8 + header;
        while at + 8 <= data.len() && tracks.len() < count as usize {
            let size = be32_at(data, at + 4).context("the MIDI file")? as usize;
            let Some(chunk) = data.get(at + 8..at + 8 + size) else {
                bail!("track {} runs past the end of the file", tracks.len());
            };
            if &data[at..at + 4] == b"MTrk" {
                tracks.push(read_track(chunk).with_context(|| format!("track {}", tracks.len()))?);
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

/// How far a MIDI file's events move to leave room for the set-up before its first notes, when a
/// note comes before FIRST_NOTE or the tune loops whole: the disc's sequences set their channels
/// up at ticks 10 to 50 and start their first notes at 50 or later, at 120 in most.
pub const LEAD: u64 = 120;
pub const FIRST_NOTE: u64 = 50;

/// What every channel that plays notes is given before its first note, as every one of the disc's
/// 2,024 note channels is (volume in all but 19): volume 100 (MIDI's), expression 127, pan centre,
/// pitch bend centre. A channel left without them may not sound (the driver's defaults are not
/// known).
fn setup(channel: u8) -> [Message; 4] {
    [
        Message::Control { channel, controller: 7, value: 100 },
        Message::Control { channel, controller: 11, value: 127 },
        Message::Control { channel, controller: 10, value: 64 },
        Message::PitchBend { channel, value: 0x2000 },
    ]
}

/// Gives each channel that plays notes the set-up it lacks before its first note (setup), at ticks
/// 20, 30, ... as the disc's sequences place theirs.
fn add_setup(events: &mut Vec<(u64, Message)>) {
    for channel in 0..16u8 {
        let Some(first) = events
            .iter()
            .position(|(_, message)| matches!(message, Message::NoteOn { channel: on, .. } if *on == channel))
        else {
            continue;
        };
        let note = events[first].0;
        let before: Vec<Message> =
            events[..first].iter().filter(|(_, m)| m.channel() == Some(channel)).map(|(_, m)| m.clone()).collect();
        let given = |wanted: &Message| {
            before.iter().any(|message| match (message, wanted) {
                (Message::Control { controller: a, .. }, Message::Control { controller: b, .. }) => a == b,
                (Message::PitchBend { .. }, Message::PitchBend { .. }) => true,
                _ => false,
            })
        };
        let missing: Vec<Message> = setup(channel).into_iter().filter(|message| !given(message)).collect();
        for (k, message) in missing.into_iter().enumerate() {
            insert(events, (20 + 10 * k as u64).min(note), message, false);
        }
    }
}

/// The message on another channel.
fn on_channel(message: &Message, channel: u8) -> Message {
    let (status, data) = message.bytes();
    Message::from_bytes(status & 0xF0 | channel, &data)
}

/// Moves channel 0's messages to a channel the file does not use when channel 0 plays notes: the
/// disc's sequences never do, and keep it for the loop's controllers. Refused when all 16 are used.
fn move_off_channel_0(events: &mut [(u64, Message)]) -> Result<()> {
    if !events.iter().any(|(_, message)| matches!(message, Message::NoteOn { channel: 0, .. })) {
        return Ok(());
    }
    let used: BTreeSet<u8> = events.iter().filter_map(|(_, message)| message.channel()).collect();
    let Some(free) = (1..16u8).find(|channel| !used.contains(channel)) else {
        bail!("channel 1 plays notes and all 16 channels are used; the game keeps channel 1 for the loop, so free one");
    };
    for (_, message) in events.iter_mut() {
        let loop_control =
            matches!(message, Message::Control { controller: NRPN_MSB | DATA_ENTRY | DATA_ENTRY_LSB, .. });
        if message.channel() == Some(0) && !loop_control {
            *message = on_channel(message, free);
        }
    }
    Ok(())
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
    let looped = template_start.is_some() && template_end.is_some();
    // shaped as the disc's sequences are: channel 0 carries the loop and no notes, and the first
    // notes come a little after the start, so a tune looped whole has its set-up before the loop
    move_off_channel_0(&mut merged.events)?;
    let whole = !merged.nrpn_loop && merged.markers == (None, None) && looped;
    let earliest = merged.events.iter().find(|(_, message)| matches!(message, Message::NoteOn { .. }));
    let lead = if whole || earliest.is_some_and(|(tick, _)| *tick < FIRST_NOTE) { LEAD } else { 0 };
    for (tick, message) in &mut merged.events {
        let setup = !matches!(
            message,
            Message::NoteOn { .. }
                | Message::NoteOff { .. }
                | Message::Meta { .. }
                | Message::Control { controller: NRPN_MSB | DATA_ENTRY | DATA_ENTRY_LSB, .. }
        );
        *tick = match (*tick, message) {
            // a tempo at the start stays there, the channels' set-up there goes before the notes
            (0, Message::Meta { kind: META_TEMPO, .. }) => 0,
            (0, _) if setup && lead > 0 => 10,
            _ => *tick + lead,
        };
    }
    merged.events.sort_by_key(|(tick, _)| *tick);
    merged.markers = (merged.markers.0.map(|tick| tick + lead), merged.markers.1.map(|tick| tick + lead));
    merged.end += lead;
    let mut end = merged.end;
    if !merged.nrpn_loop {
        let (start, stop) = match merged.markers {
            (Some(start), Some(stop)) => (start, stop),
            (None, None) if looped => (lead, merged.end),
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
    add_setup(&mut merged.events);
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
    for index in soundbank::find(&bank.pack, "sq") {
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
    let mut pack = soundbank::read_pack(original)?;
    let bank = Bank::from_bytes(original)?;
    let sequences = soundbank::find(&pack, "sq");
    let mut changed = false;
    let mut paths: Vec<_> = files::read_dir(folder)?.flatten().map(|entry| entry.path()).collect();
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
        let built = build(&std::fs::read(&path)?, &pack.members[index].data, Some(&bank)).context(&file)?;
        if built != pack.members[index].data {
            pack.members[index].data = built;
            changed = true;
        }
    }
    Ok(if changed { pack.write()? } else { original.to_vec() })
}

/// The SPU2 memory the music banks (BG_*) load into, from 0x5210 to the field banks at 0x7D210
/// (CSound::Init; docs/game/memory.md section 6). A bank takes its samples' bytes plus 16.
pub const MUSIC_REGION: usize = 0x7_8000;

/// One sample of a new instrument: a WAV file, the keys it plays, the key at which it sounds at
/// its own pitch, whether it loops (None: as the WAV says; a WAV without a loop then plays once),
/// and its volume (0 to 128, 128 the sample's own level).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstrumentZone {
    pub path: std::path::PathBuf,
    pub low: u8,
    pub high: u8,
    pub root: u8,
    pub looped: Option<bool>,
    pub volume: u8,
}

/// A program a mod adds to (or replaces in) a music bank: its number, the program the tune's MIDI
/// file names; its samples by key range; and a program of the bank whose envelope and settings it
/// takes (`like`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instrument {
    pub program: u8,
    pub zones: Vec<InstrumentZone>,
    pub like: Option<u8>,
}

/// The bank with new instruments and a sequence (the member `sequence`, BG_000_000.sq) built from
/// a MIDI file that may play them. The instruments go in first, so the tune's programs are checked
/// against them; a program of the bank the instrument's number names is replaced. When the bank
/// then outgrows `region` (the SPU2 memory it loads into, MUSIC_REGION for a BG bank), the
/// samples nothing plays any more (the old tune's instruments, a replaced program's) are cut to
/// silent stubs to make room; if it still does not fit, the build is refused with the sizes.
/// Returns the bank's bytes and what it did, for the build to report.
pub fn build_with_instruments(
    midi: &[u8],
    bank_data: &[u8],
    sequence: &str,
    instruments: &[Instrument],
    region: Option<usize>,
) -> Result<(Vec<u8>, Vec<String>)> {
    instrumented(midi, Bank::from_bytes(bank_data)?, sequence, instruments, region, false)
}

/// What build_with_instruments does, to a parsed bank; `cut_all` cuts every sample nothing plays
/// any more to a stub whether or not the bank needs the room (a new bank keeps only its tune's).
fn instrumented(
    midi: &[u8],
    mut bank: Bank,
    sequence: &str,
    instruments: &[Instrument],
    region: Option<usize>,
    cut_all: bool,
) -> Result<(Vec<u8>, Vec<String>)> {
    let before = bank.samples_size();
    let mut notes = Vec::new();
    let mut old_vags = BTreeSet::new();
    for instrument in instruments {
        let mut zones = Vec::new();
        for zone in &instrument.zones {
            let file = zone.path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let sound = crate::soundbank::read_wav(&std::fs::read(&zone.path)?).context(&file)?;
            zones.push(crate::soundbank::Zone {
                sound,
                low: zone.low as i64,
                high: zone.high as i64,
                root: zone.root as i64,
                looped: zone.looped,
                volume: zone.volume as i64,
            });
        }
        let replaced = bank.hd.programs.get(instrument.program as usize).is_some_and(Option::is_some);
        old_vags.extend(bank.set_program(instrument.program as usize, &zones, instrument.like.map(usize::from))?);
        notes.push(format!(
            "instrument: program {} {} ({} sample{})",
            instrument.program,
            if replaced { "replaced" } else { "added" },
            zones.len(),
            if zones.len() == 1 { "" } else { "s" }
        ));
    }
    let Some(index) = soundbank::find(&bank.pack, "sq")
        .into_iter()
        .find(|&index| bank.pack.members[index].name.eq_ignore_ascii_case(sequence))
    else {
        bail!("the bank has no sequence {sequence}");
    };
    let built = build(midi, &bank.pack.members[index].data, Some(&bank))?;
    bank.pack.members[index].data = built;
    let fits = |bank: &Bank| region.is_none_or(|region| bank.samples_size() + 16 <= region);
    if cut_all || !fits(&bank) {
        // the samples nothing plays now: the old tune's, and the replaced programs'
        let all: BTreeSet<usize> = (0..bank.vag_data.len()).collect();
        let size = bank.samples_size();
        crate::sounds::free_unused(&mut bank, &all)?;
        notes.push(format!(
            "music memory: {} bytes of samples nothing plays any more cut to stubs",
            size - bank.samples_size()
        ));
    } else {
        crate::sounds::free_unused(&mut bank, &old_vags)?;
    }
    let after = bank.samples_size();
    if let Some(region) = region.filter(|_| !fits(&bank)) {
        bail!(
            "the bank's samples take {after:#X} bytes with the new instruments ({before:#X} before); the music memory holds \
             {region:#X} (with 16 bytes the loader adds), {:#X} too many: use shorter or lower-rate WAVs, or fewer of the \
             tune's programs",
            after + 16 - region
        );
    }
    if let Some(region) = region {
        notes.push(format!(
            "music memory: samples {before:#X} -> {after:#X} bytes of {region:#X}, {:#X} free",
            region - after - 16
        ));
    }
    Ok((bank.build()?, notes))
}

/// The file of background music number `number` (snd2/bgm/BG_200.snd): the engine's path for
/// load_bgm(number) (CScene::GetBgmFile, at least three digits).
pub fn bgm_path(number: u32) -> String {
    format!("snd2/bgm/{}.snd", bgm_name(number))
}

/// A music bank's name for its number: BG_200.
pub fn bgm_name(number: u32) -> String {
    format!("BG_{number:03}")
}

/// A new music bank, `name` (BG_200), made like one of the disc's (`like_data`): play_bgm's entry
/// 0 plays a sequence built from a MIDI file with the like bank's entry 0 sequence as the template,
/// as a music: line builds it (the layout, the loop, ...). The bank keeps the like bank's
/// instruments, default volume (.vol) and reverb; its members are renamed after it (BG_200.hd,
/// .bd, .cfg, .vol and BG_200_000.sq, which the .cfg's row 0 names), its other sequences and
/// their rows are dropped, the instruments given go in, and every sample the new tune does not
/// play is cut to a silent stub. Returns the bank's bytes and what it did, as
/// build_with_instruments does.
pub fn new_bank(
    midi: &[u8],
    like_data: &[u8],
    name: &str,
    instruments: &[Instrument],
    region: Option<usize>,
) -> Result<(Vec<u8>, Vec<String>)> {
    let like = Bank::from_bytes(like_data)?;
    let entries = like.entries()?;
    let Some((first, seq)) = entries.first().and_then(|entry| match entry.kind {
        soundbank::EntryKind::Sq { seq } => Some((entry, seq)),
        _ => None,
    }) else {
        bail!("the bank's entry 0 plays no sequence, so it is not a music bank");
    };
    let sequences = soundbank::find(&like.pack, "sq");
    let kept = sequences[seq];
    let old_sequence = like.pack.members[kept].name.clone();
    let sequence = format!("{name}_000.sq");
    let mut pack = like.pack.clone();
    let mut members = Vec::new();
    for (at, mut member) in pack.members.into_iter().enumerate() {
        let ext = soundbank::extension(&member.name);
        if ext == "sq" && at != kept {
            continue;
        }
        let renamed = match ext.as_str() {
            "sq" => sequence.clone(),
            "hd" | "bd" | "cfg" | "vol" => {
                let suffix = member.name.rsplit_once('.').map_or("", |(_, suffix)| suffix).to_string();
                format!("{name}.{suffix}")
            }
            _ => member.name.clone(),
        };
        if ext == "cfg" {
            member.data = new_cfg(&member.data, first.line, &old_sequence, &sequence, name)?;
        }
        if renamed != member.name {
            // the name and zeros after it in the name field; the slot's tail stays
            member = PackMember { slot_tail: member.slot_tail, ..PackMember::new(&renamed, member.data)? };
        }
        members.push(member);
    }
    pack.members = members;
    let bank = Bank::from_bytes(&pack.write()?)?;
    instrumented(midi, bank, &sequence, instruments, region, true)
}

/// A new music bank's .cfg: the like bank's header (its number column set to the new bank's) and
/// END kept, its entry 0's row naming the renamed sequence, the other rows dropped.
fn new_cfg(data: &[u8], row: usize, old_sequence: &str, sequence: &str, name: &str) -> Result<Vec<u8>> {
    let mut cfg = crate::soundbank::TextFile::parse(data);
    let end = cfg.end_index();
    let number = name.rsplit('_').next().unwrap_or(name).trim_start_matches('0');
    let number = if number.is_empty() { "0" } else { number };
    let mut lines = Vec::new();
    for (at, (text, eol)) in cfg.lines.into_iter().enumerate() {
        let digit = text.first().is_some_and(u8::is_ascii_digit);
        if at < end && digit && at != row {
            continue;
        }
        let mut columns: Vec<Vec<u8>> = text.split(|&c| c == b'\t').map(<[u8]>::to_vec).collect();
        if at == row {
            let Some(column) = columns.iter_mut().find(|column| column.eq_ignore_ascii_case(old_sequence.as_bytes()))
            else {
                bail!("the .cfg's row for entry 0 does not name {old_sequence} between tabs");
            };
            *column = sequence.as_bytes().to_vec();
        } else if at < end
            && !digit
            && columns.len() > 2
            && !columns[2].is_empty()
            && columns[2].iter().all(u8::is_ascii_digit)
        {
            // the header: prefix, title, number
            columns[2] = number.as_bytes().to_vec();
        }
        lines.push((columns.join(&b'\t'), eol));
    }
    cfg.lines = lines;
    Ok(cfg.build())
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
                note(40, 60, true),
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
        assert_eq!(markers, [(20, Some(true)), (1040, Some(false))]);
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
        let three = |controller, value| Message::Control { channel: 3, controller, value };
        // moved on by LEAD, as its first note came at 0, but for its set-up at 0, now at 10, and
        // given the set-up it lacked
        assert_eq!(
            messages,
            [
                (0, Message::tempo(600_000)),
                (10, Message::Program { channel: 3, program: 1 }),
                (20, three(7, 100)),
                (30, three(11, 127)),
                (40, three(10, 64)),
                (50, Message::PitchBend { channel: 3, value: 0x2000 }),
                (110, control(NRPN_MSB, LOOP_START)),
                (120, control(DATA_ENTRY, 0)),
                (120, Message::NoteOn { channel: 3, key: 64, velocity: 100 }),
                (600, Message::NoteOff { channel: 3, key: 64 }),
                (1060, control(NRPN_MSB, LOOP_END)),
                (1070, control(DATA_ENTRY, 0)),
                (1080, control(DATA_ENTRY_LSB, 0)),
                (1560, Message::end()),
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
        assert!(error.to_string().contains("channel 2 plays a note"), "{error}");
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
        assert_eq!(loop_ticks(&built), (Some(LEAD), Some(1920 + LEAD)));
        // channel 0's notes moved to a free channel; the set-up comes before the loop
        assert!(shaped_as_the_disc_s(&built), "{built:?}");
        assert!(built.events.iter().any(|e| matches!(e.message, Message::NoteOn { channel: 1, .. })));
        assert!(built.events.iter().any(|e| e.message == Message::tempo(DEFAULT_TEMPO)));
        let one_shot = Sequence { division: DIVISION, events: vec![Event::new(0, Message::end())] };
        let built = from_midi(&smf.write(), &one_shot, &|_| true).unwrap();
        assert_eq!(loop_ticks(&built), (None, None));
    }

    /// Whether a sequence is laid out as every one of the disc's is: no notes on channel 0, every
    /// note channel given program, volume, expression, pan and bend before its first note, and no
    /// note before tick FIRST_NOTE.
    fn shaped_as_the_disc_s(sequence: &Sequence) -> bool {
        let ticks = sequence.ticks();
        let mut seen: [BTreeSet<&str>; 16] = Default::default();
        let mut started = [false; 16];
        for (event, tick) in sequence.events.iter().zip(ticks) {
            match event.message {
                Message::NoteOn { channel: 0, .. } => return false,
                Message::NoteOn { channel, .. } if !started[channel as usize] => {
                    started[channel as usize] = true;
                    let wanted = ["program", "7", "11", "10", "bend"];
                    if tick < FIRST_NOTE || !wanted.iter().all(|kind| seen[channel as usize].contains(kind)) {
                        return false;
                    }
                }
                Message::Program { channel, .. } => _ = seen[channel as usize].insert("program"),
                Message::PitchBend { channel, .. } => _ = seen[channel as usize].insert("bend"),
                Message::Control { channel, controller: 7, .. } => _ = seen[channel as usize].insert("7"),
                Message::Control { channel, controller: 11, .. } => _ = seen[channel as usize].insert("11"),
                Message::Control { channel, controller: 10, .. } => _ = seen[channel as usize].insert("10"),
                _ => {}
            }
        }
        true
    }

    #[test]
    fn channel_0_is_freed_or_refused() {
        let note = |channel| (0, Message::NoteOn { channel, key: 60, velocity: 90 });
        let program = |channel| (0, Message::Program { channel, program: 0 });
        let mut events: Vec<(u64, Message)> = (0..16).flat_map(|channel| [program(channel), note(channel)]).collect();
        let full = Smf { format: 0, division: 480, tracks: vec![Track { events: events.clone(), end: 480 }] };
        let error = from_midi(&full.write(), &tune(), &|_| true).unwrap_err();
        assert!(error.to_string().contains("all 16 channels are used"), "{error}");
        events.retain(|(_, message)| message.channel() != Some(9));
        let some = Smf { format: 0, division: 480, tracks: vec![Track { events, end: 480 }] };
        let built = from_midi(&some.write(), &tune(), &|_| true).unwrap();
        assert!(shaped_as_the_disc_s(&built));
        assert!(built.events.iter().any(|e| matches!(e.message, Message::NoteOn { channel: 9, .. })));
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
        let loop_control = |e: &&Event| {
            matches!(e.message, Message::Control { controller: NRPN_MSB | DATA_ENTRY | DATA_ENTRY_LSB, .. })
        };
        let controls = built.events.iter().filter(loop_control).count();
        assert_eq!(controls, 5);
    }

    /// Every sequence of an extracted disc (DC2TK_CORPUS) exports to MIDI and builds back to the same
    /// bytes, and names only programs its bank has.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_sequences_round_trip_through_midi() {
        let corpus = dc2tk_disc::samples::corpus();
        let banks: Vec<std::path::PathBuf> = crate::files_under(std::path::Path::new(&corpus))
            .unwrap()
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("snd")))
            .collect();
        let (mut sequences, mut missing) = (0, Vec::new());
        for path in &banks {
            let data = std::fs::read(path).unwrap();
            let bank = Bank::from_bytes(&data).unwrap();
            for index in soundbank::find(&bank.pack, "sq") {
                let member = &bank.pack.members[index];
                let midi = export(&member.data, stem(&member.name)).unwrap();
                let built = build(&midi, &member.data, Some(&bank)).unwrap();
                assert_eq!(built, member.data, "{} {}", path.display(), member.name);
                let sequence = Sequence::read(&member.data).unwrap();
                // taken as a new tune (another template), it comes out shaped as the disc's are
                let one_shot = Sequence { division: DIVISION, events: vec![Event::new(0, Message::end())] };
                let anew = from_midi(&midi, &one_shot, &|_| true).unwrap();
                assert!(shaped_as_the_disc_s(&anew), "{} {}", path.display(), member.name);
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

    /// Every music bank of the disc makes a new bank of its own tune: entry 0 plays the same
    /// sequence, renamed, and the bank fits the music memory.
    #[test]
    #[ignore = "needs DC2TK_CORPUS"]
    fn corpus_music_banks_make_new_banks() {
        let corpus = dc2tk_disc::samples::corpus();
        let folder = std::path::Path::new(&corpus).join("snd2").join("bgm");
        let Ok(found) = std::fs::read_dir(&folder) else {
            return;
        };
        let mut made = 0;
        for path in found.flatten().map(|entry| entry.path()) {
            if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("snd")) {
                continue;
            }
            let data = std::fs::read(&path).unwrap();
            let like = Bank::from_bytes(&data).unwrap();
            let crate::soundbank::EntryKind::Sq { seq: first } = like.entries().unwrap()[0].kind else {
                panic!("entry 0 plays no sequence")
            };
            let original = &like.pack.members[soundbank::find(&like.pack, "sq")[first]];
            let midi = export(&original.data, stem(&original.name)).unwrap();
            let (built, _) = new_bank(&midi, &data, "BG_999", &[], Some(MUSIC_REGION))
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let bank = Bank::from_bytes(&built).unwrap();
            let entries = bank.entries().unwrap();
            assert_eq!((entries.len(), entries[0].file.as_str()), (1, "BG_999_000.sq"), "{}", path.display());
            assert_eq!(soundbank::find(&bank.pack, "sq").len(), 1);
            assert_eq!(
                bank.pack.members[soundbank::find(&bank.pack, "sq")[0]].data,
                original.data,
                "{}",
                path.display()
            );
            assert!(bank.samples_size() <= like.samples_size());
            assert_eq!(bank.volumes(), (vec![like.volumes().0[0], like.volumes().0[1]], like.volumes().1));
            made += 1;
        }
        eprintln!("{made} music banks made new banks of their tunes");
    }

    /// A bank with program 1 (a saw wave, from test_bank) and the sequence x_000.sq (tune(), which
    /// plays program 1) named by a row of its .cfg, as the BG banks name theirs.
    fn bank_with_tune() -> Vec<u8> {
        let mut bank = Bank::from_bytes(&crate::soundbank::test_bank(&[22050], 100)).unwrap();
        let at = bank.cfg.end_index();
        let eol = bank.cfg.eol();
        bank.cfg.lines.insert(at, (b"2\tBGM\tTUNE\t0\tx_000.sq\t0\t0\t".to_vec(), eol));
        let data = bank.build().unwrap();
        let mut members: Vec<(String, Vec<u8>)> =
            soundbank::read_pack(&data).unwrap().members.into_iter().map(|member| (member.name, member.data)).collect();
        members.push(("x_000.sq".to_string(), tune().write().unwrap()));
        crate::models::write_pack(&members).unwrap()
    }

    /// The tune with its program 1 changed to `program`, as a MIDI file.
    fn tune_playing(program: u8) -> Vec<u8> {
        let mut sequence = tune();
        for event in &mut sequence.events {
            if let Message::Program { program: number, .. } = &mut event.message {
                *number = program;
            }
        }
        to_midi(&sequence, "x_000")
    }

    fn wav(folder: &Path, name: &str, length: usize, looped: bool) -> std::path::PathBuf {
        let sound = crate::soundbank::Sound {
            rate: 22050,
            samples: (0..length as i64).map(|n| if n % 50 < 25 { 8000 } else { -8000 }).collect(),
            loop_start: looped.then_some(0),
        };
        let path = folder.join(name);
        std::fs::write(&path, crate::soundbank::wav_bytes(&sound).unwrap()).unwrap();
        path
    }

    fn zone(path: &Path, low: u8, high: u8, root: u8) -> InstrumentZone {
        InstrumentZone { path: path.to_path_buf(), low, high, root, looped: None, volume: 128 }
    }

    #[test]
    fn a_tune_plays_instruments_made_from_wavs() {
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-instruments-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let low = wav(&folder, "low.wav", 2800, true);
        let high = wav(&folder, "high.wav", 1400, false);
        let original = bank_with_tune();
        // a program the bank lacks is refused without the instrument
        let bank = Bank::from_bytes(&original).unwrap();
        let sq = &bank.pack.members[soundbank::find(&bank.pack, "sq")[0]].data;
        assert!(build(&tune_playing(100), sq, Some(&bank)).unwrap_err().to_string().contains("no program 100"));
        // two samples split at middle C, the second a fifth up from its own pitch at 60
        let instrument =
            Instrument { program: 100, zones: vec![zone(&low, 0, 59, 48), zone(&high, 60, 127, 60)], like: Some(1) };
        let (built, notes) =
            build_with_instruments(&tune_playing(100), &original, "x_000.sq", &[instrument], Some(MUSIC_REGION))
                .unwrap();
        assert!(notes[0].starts_with("instrument: program 100 added (2 samples)"), "{notes:?}");
        let bank = Bank::from_bytes(&built).unwrap();
        let program = bank.hd.programs[100].as_ref().unwrap();
        let ranges: Vec<(u8, u8)> = program.splits.iter().map(|s| (s.range_low, s.range_high)).collect();
        assert_eq!(ranges, [(0, 59), (60, 127)]);
        let voices = bank.resolve(100, 67, 90);
        assert_eq!(voices.len(), 1);
        let sample = bank.hd.samples[voices[0].sample].as_ref().unwrap();
        assert_eq!((sample.base_note, sample.volume), (60, 128));
        let played = bank.decode_vag(voices[0].vag).unwrap();
        assert_eq!((played.rate, played.loop_start), (22050, None));
        let low_voice = bank.resolve(100, 40, 90)[0];
        assert!(bank.decode_vag(low_voice.vag).unwrap().loop_start.is_some());
        // the sequence plays it, and the old tune's program 1 is still there for others
        let sequence = Sequence::read(&bank.pack.members[soundbank::find(&bank.pack, "sq")[0]].data).unwrap();
        assert_eq!(sequence.programs(), vec![100]);
        assert!(bank.hd.programs[1].is_some());
        // a program number taken by the bank is replaced, and its old sample, which nothing plays
        // now, cut to a stub
        let instrument = Instrument { program: 1, zones: vec![zone(&high, 0, 127, 60)], like: None };
        let replace = |region| {
            build_with_instruments(&tune_playing(1), &original, "x_000.sq", std::slice::from_ref(&instrument), region)
        };
        let (built, notes) = replace(None).unwrap();
        assert!(notes[0].starts_with("instrument: program 1 replaced"), "{notes:?}");
        let fitted = Bank::from_bytes(&built).unwrap().samples_size();
        // a bank too big for its memory: what nothing plays is cut first, then it is refused
        let (built, notes) = replace(Some(fitted + 16)).unwrap();
        assert!(notes.iter().any(|note| note.contains("cut to stubs")), "{notes:?}");
        assert!(notes.last().unwrap().ends_with(", 0x0 free"), "{notes:?}");
        assert_eq!(Bank::from_bytes(&built).unwrap().samples_size(), fitted);
        let error = replace(Some(fitted)).unwrap_err().to_string();
        assert!(error.contains("0x10 too many"), "{error}");
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// A music bank as the disc's BG banks are: entry 0 plays x_000.sq (the tune on program 1),
    /// entry 1 x_001.sq (on program 2); programs 1 and 2 are saw waves of their own samples.
    fn music_bank() -> Vec<u8> {
        let mut bank = Bank::from_bytes(&crate::soundbank::test_bank(&[22050, 11025], 100)).unwrap();
        let eol = bank.cfg.eol();
        bank.cfg.lines = [
            "BG\tTEST\t7\t\t\t\t\t",
            "0\tBGM\tA\t0\tx_000.sq\t*\t*\t",
            "1\tBGM\tB\t0\tx_001.sq\t*\t*\t",
            "END\t\t\t\t\t\t\t",
        ]
        .iter()
        .map(|line| (line.as_bytes().to_vec(), eol.clone()))
        .collect();
        let data = bank.build().unwrap();
        let mut members: Vec<(String, Vec<u8>)> =
            soundbank::read_pack(&data).unwrap().members.into_iter().map(|member| (member.name, member.data)).collect();
        members.push(("x_000.sq".to_string(), tune().write().unwrap()));
        let mut second = tune();
        for event in &mut second.events {
            if let Message::Program { program, .. } = &mut event.message {
                *program = 2;
            }
        }
        members.push(("x_001.sq".to_string(), second.write().unwrap()));
        crate::models::write_pack(&members).unwrap()
    }

    #[test]
    fn a_new_music_bank_plays_the_midi_file_as_entry_0() {
        let like = music_bank();
        let old = Bank::from_bytes(&like).unwrap();
        let program_vag = |bank: &Bank, program: i64| bank.resolve(program, 60, 90)[0].vag;
        let stub_size = crate::soundbank::encode_adpcm(&[0; 28], None, soundbank::Search::Exhaustive).unwrap().len();
        assert!(old.vag_data[program_vag(&old, 1)].len() > stub_size);
        // a tune on program 2, the bank's second tune's instrument
        let midi = tune_playing(2);
        let (built, notes) = new_bank(&midi, &like, "BG_200", &[], Some(MUSIC_REGION)).unwrap();
        assert!(notes.iter().any(|note| note.contains("cut to stubs")), "{notes:?}");
        let bank = Bank::from_bytes(&built).unwrap();
        let names: Vec<&str> = bank.pack.members.iter().map(|member| member.name.as_str()).collect();
        assert_eq!(names, ["BG_200.hd", "BG_200.bd", "BG_200.cfg", "BG_200.vol", "BG_200_000.sq"]);
        // one entry, playing the one sequence, which is the MIDI file's built like x_000
        let entries = bank.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            (entries[0].kind, entries[0].file.as_str()),
            (crate::soundbank::EntryKind::Sq { seq: 0 }, "BG_200_000.sq")
        );
        let sq = &bank.pack.members[4].data;
        assert_eq!(*sq, build(&midi, &tune().write().unwrap(), Some(&old)).unwrap());
        assert_eq!(Sequence::read(sq).unwrap().programs(), vec![2]);
        let cfg = String::from_utf8(bank.pack.members[2].data.clone()).unwrap();
        assert!(cfg.starts_with("BG\tTEST\t200\t"), "{cfg}");
        assert!(!cfg.contains("x_00"), "{cfg}");
        // the old tune's sample is cut, the new one's kept; the default volume is the like bank's
        assert_eq!(bank.vag_data[program_vag(&bank, 1)].len(), stub_size);
        assert_eq!(bank.vag_data[program_vag(&bank, 2)], old.vag_data[program_vag(&old, 2)]);
        assert_eq!(bank.volumes().0[0], old.volumes().0[0]);
        // the tune exports and builds back to itself
        assert_eq!(build(&export(sq, "BG_200_000").unwrap(), sq, Some(&bank)).unwrap(), *sq);
        // a bank whose entry 0 plays no sequence is no music bank
        let error = new_bank(&midi, &crate::soundbank::test_bank(&[22050], 100), "BG_200", &[], None).unwrap_err();
        assert!(error.to_string().contains("plays no sequence"), "{error}");
    }
}
