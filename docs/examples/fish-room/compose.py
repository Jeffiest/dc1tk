"""The Fish Room's tune, "Round and Round": a dopey, bubbly loop in F major for a fish that has not
noticed it is spinning, and the four instruments it plays, synthesized here (no game samples).

    python docs/examples/fish-room/compose.py mods/fish-room/music

writes fish.mid and the instruments' WAVs (vibes.wav, marimba.wav, bass.wav, bubble.wav) into the
folder; mod.json's music line gives the WAVs as programs 100 to 103, the numbers the tune's program
changes name.

The tune: 24 bars at 100 beats a minute, looping whole (markers loopStart and loopEnd): A (8 bars),
B (8 bars: the same few notes over and over, then a long held note, staring), A again. Channel 1
the bass, 2 the marimba's broken chords, 3 the vibes' tune, 4 the odd bubble.
"""

import math
import os
import random
import struct
import sys
import wave

RATE = 32000
DIVISION = 480  # ticks a beat
TEMPO = 100  # beats a minute
BAR = 4 * DIVISION

NOTE = {"C": 0, "C#": 1, "Db": 1, "D": 2, "Eb": 3, "E": 4, "F": 5, "F#": 6, "G": 7, "Ab": 8, "A": 9, "Bb": 10, "B": 11}


def pitch(name):
    """MIDI note of "Bb4" (C4 is 60)."""
    return 12 * (int(name[-1]) + 1) + NOTE[name[:-1]]


# --- the instruments --------------------------------------------------------------------------


def write_wav(path, samples):
    peak = max(1e-9, max(abs(s) for s in samples))
    with wave.open(path, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(b"".join(struct.pack("<h", int(32000 * s / peak)) for s in samples))


def vibes(f=523.25, seconds=1.8):
    """A vibraphone-like tone (root C5): a sine with soft overtones, a slow wobble, a long fade."""
    out = []
    for i in range(int(RATE * seconds)):
        t = i / RATE
        tone = math.sin(2 * math.pi * f * t) + 0.22 * math.sin(2 * math.pi * 4 * f * t) * math.exp(-t * 9)
        tone += 0.08 * math.sin(2 * math.pi * 2 * f * t)
        wobble = 1.0 + 0.22 * math.sin(2 * math.pi * 5.2 * t)
        attack = min(1.0, t / 0.004)
        out.append(tone * wobble * attack * math.exp(-t * 1.7))
    return out


def marimba(f=523.25, seconds=0.55):
    """A wooden bar (root C5): the fundamental and the bar's high partial, dying quickly."""
    out = []
    for i in range(int(RATE * seconds)):
        t = i / RATE
        tone = math.sin(2 * math.pi * f * t) * math.exp(-t * 7.5)
        tone += 0.45 * math.sin(2 * math.pi * 3.93 * f * t) * math.exp(-t * 28)
        tone += 0.15 * math.sin(2 * math.pi * 9.2 * f * t) * math.exp(-t * 60)
        out.append(tone * min(1.0, t / 0.002))
    return out


def bass(f=130.81, seconds=1.0):
    """A plump, round bass (root C3) that bloops: it starts a little sharp and settles."""
    out = []
    phase = 0.0
    for i in range(int(RATE * seconds)):
        t = i / RATE
        freq = f * (1.0 + 0.07 * math.exp(-t * 40))
        phase += 2 * math.pi * freq / RATE
        tone = math.sin(phase) + 0.3 * math.sin(2 * phase) + 0.1 * math.sin(3 * phase)
        out.append(tone * min(1.0, t / 0.006) * math.exp(-t * 2.6))
    return out


def bubble(seconds=0.12):
    """A bubble (root C4): a sine sweeping up fast, popping off."""
    out = []
    phase = 0.0
    for i in range(int(RATE * seconds)):
        t = i / RATE
        freq = 261.63 * (1.0 + 2.2 * (t / seconds) ** 1.5)
        phase += 2 * math.pi * freq / RATE
        envelope = min(1.0, t / 0.003) * (1.0 - t / seconds) ** 2
        out.append(math.sin(phase) * envelope)
    return out


INSTRUMENTS = {
    # program: (file, root, sound)
    100: ("vibes.wav", 72, vibes),
    101: ("marimba.wav", 72, marimba),
    102: ("bass.wav", 48, bass),
    103: ("bubble.wav", 60, bubble),
}


# --- the tune ---------------------------------------------------------------------------------

# Bars of the tune: (chords as (root, kind, beats), melody as (note or None for a rest, beats)).
MAJOR, MINOR, SEVENTH = (0, 4, 7), (0, 3, 7), (0, 4, 7, 10)
A_PART = [
    ([("F", MAJOR, 4)], [("A4", 1), ("C5", 1), ("F5", 1.5), ("E5", 0.5)]),
    ([("D", MINOR, 4)], [("D5", 1), ("C5", 0.5), ("A4", 0.5), ("F4", 2)]),
    ([("G", MINOR, 4)], [("G4", 1), ("Bb4", 1), ("D5", 1.5), ("C5", 0.5)]),
    ([("C", SEVENTH, 4)], [("Bb4", 1), ("A4", 0.5), ("G4", 0.5), ("C5", 2)]),
    ([("F", MAJOR, 4)], [("A4", 1), ("C5", 1), ("F5", 1), ("A5", 1)]),
    ([("D", MINOR, 4)], [("G5", 0.5), ("F5", 0.5), ("D5", 1), ("F5", 2)]),
    ([("Bb", MAJOR, 4)], [("D5", 1), ("Bb4", 1), ("G4", 1), ("Bb4", 1)]),
]
A_HALF_CADENCE = ([("C", SEVENTH, 4)], [("A4", 1), ("G4", 0.5), ("E4", 0.5), ("C5", 2)])
A_HOME = ([("C", SEVENTH, 2), ("F", MAJOR, 2)], [("A4", 1), ("G4", 1), ("F4", 2)])
B_PART = [
    ([("Bb", MAJOR, 4)], [("F5", 0.5), ("F5", 0.5), ("F5", 0.5), ("D5", 0.5), ("Bb4", 2)]),
    ([("F", MAJOR, 4)], [("A4", 0.5), ("A4", 0.5), ("A4", 0.5), ("C5", 0.5), ("F5", 2)]),
    ([("G", MINOR, 4)], [("G5", 0.5), ("F5", 0.5), ("D5", 0.5), ("Bb4", 0.5), ("G4", 1), ("Bb4", 1)]),
    ([("C", MAJOR, 4)], [("C5", 3), (None, 1)]),  # the blank stare
    ([("Bb", MAJOR, 4)], [("D5", 1), ("F5", 1), ("Bb5", 1.5), ("A5", 0.5)]),
    ([("A", SEVENTH, 4)], [("G5", 1), ("E5", 1), ("C#5", 1), ("A4", 1)]),
    ([("D", MINOR, 2), ("G", SEVENTH, 2)], [("D5", 1), ("F5", 1), ("B4", 1), ("D5", 1)]),
    ([("C", SEVENTH, 4)], [("G4", 1), ("A4", 0.5), ("Bb4", 0.5), ("C5", 2)]),
]
SONG = A_PART + [A_HALF_CADENCE] + B_PART + A_PART + [A_HOME]

BASS_CHANNEL, MARIMBA_CHANNEL, VIBES_CHANNEL, BUBBLE_CHANNEL = 1, 2, 3, 4


def events():
    """(tick, order, bytes): the tune's MIDI events, note-offs before note-ons at one tick."""
    found = []

    def note(channel, key, start, beats, velocity):
        end = start + int(beats * DIVISION) - 10
        found.append((start, 1, bytes((0x90 | channel, key, velocity))))
        found.append((end, 0, bytes((0x80 | channel, key, 0))))

    for channel, program in ((BASS_CHANNEL, 102), (MARIMBA_CHANNEL, 101), (VIBES_CHANNEL, 100), (BUBBLE_CHANNEL, 103)):
        found.append((0, 0, bytes((0xC0 | channel, program))))
    volumes = {BASS_CHANNEL: 110, MARIMBA_CHANNEL: 80, VIBES_CHANNEL: 112, BUBBLE_CHANNEL: 70}
    pans = {BASS_CHANNEL: 64, MARIMBA_CHANNEL: 44, VIBES_CHANNEL: 74, BUBBLE_CHANNEL: 90}
    for channel in volumes:
        found.append((0, 0, bytes((0xB0 | channel, 7, volumes[channel]))))
        found.append((0, 0, bytes((0xB0 | channel, 10, pans[channel]))))
    bubbles = random.Random(7)
    for number, (chords, melody) in enumerate(SONG):
        bar = number * BAR
        at = bar
        for root, kind, beats in chords:
            low = pitch(root + "2")
            if low < pitch("E2"):
                low += 12
            # bass: root, a bounce on the fifth, the root an octave up
            if beats == 4:
                for offset, step, length in ((0, 0, 1.5), (1.5, 7, 0.5), (2, 12, 1), (3, 7, 1)):
                    note(BASS_CHANNEL, low + step, at + int(offset * DIVISION), length, 100)
            else:
                note(BASS_CHANNEL, low, at, 1, 100)
                note(BASS_CHANNEL, low + 7, at + DIVISION, 1, 90)
            # marimba: the chord broken in eighths, up and back
            chord = [pitch(root + "4") + interval for interval in kind]
            if chord[0] > pitch("G4"):
                chord = [key - 12 for key in chord]
            pattern = [chord[0], chord[1], chord[2], chord[1], chord[0] + 12, chord[2], chord[1], chord[2]]
            for eighth in range(int(beats * 2)):
                note(MARIMBA_CHANNEL, pattern[eighth % 8], at + eighth * DIVISION // 2, 0.5, 58 + (8 if eighth % 2 == 0 else 0))
            at += int(beats * DIVISION)
        at = bar
        for key, beats in melody:
            if key is not None:
                note(VIBES_CHANNEL, pitch(key), at, beats, 100)
            at += int(beats * DIVISION)
        # bubbles now and then, rising in a little run at the end of the bar
        if number % 2 == 1 or bubbles.random() < 0.3:
            start = bar + int(3.5 * DIVISION)
            for blip in range(bubbles.choice((2, 3, 3, 4))):
                note(BUBBLE_CHANNEL, bubbles.choice((67, 72, 74, 76, 79, 84)), start + blip * DIVISION // 4, 0.25, 72)
    return found, len(SONG) * BAR


def variable(number):
    out = [number & 0x7F]
    number >>= 7
    while number:
        out.append(0x80 | (number & 0x7F))
        number >>= 7
    return bytes(reversed(out))


def meta(kind, data):
    return bytes((0xFF, kind)) + variable(len(data)) + data


def midi():
    found, end = events()
    found.append((0, 0, meta(0x51, (60_000_000 // TEMPO).to_bytes(3, "big"))))
    found.append((0, 0, meta(0x58, bytes((4, 2, 24, 8)))))
    found.append((0, 0, meta(0x06, b"loopStart")))
    found.append((end, 0, meta(0x06, b"loopEnd")))
    found.sort(key=lambda event: (event[0], event[1]))
    track = bytearray(variable(0) + meta(0x03, b"Round and Round"))
    now = 0
    for tick, _order, data in found:
        track += variable(tick - now) + data
        now = tick
    track += variable(0) + meta(0x2F, b"")
    header = b"MThd" + struct.pack(">IHHH", 6, 0, 1, DIVISION)
    return header + b"MTrk" + struct.pack(">I", len(track)) + bytes(track)


def write(folder):
    """Writes fish.mid and the instruments' WAVs into folder."""
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "fish.mid"), "wb") as out:
        out.write(midi())
    for _program, (name, _root, sound) in INSTRUMENTS.items():
        write_wav(os.path.join(folder, name), sound())


if __name__ == "__main__":
    write(sys.argv[1] if len(sys.argv) > 1 else ".")
