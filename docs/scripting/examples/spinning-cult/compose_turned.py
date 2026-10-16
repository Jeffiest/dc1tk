"""The Spinning Cult's music, "The Turning": the Fish Room's tune gone wrong. A detuned music box
picks out the melody in F minor over a breathing low drone, a wordless choir and a deep bell; in
the middle it all but stops, and the end never resolves, so the loop never ends. Its five
instruments are synthesized here (no game samples).

    python docs/coding/examples/spinning-cult/compose_turned.py mods/spinning-cult/music

writes turning.mid and the instruments' WAVs (musicbox, drone, choir, bell, whisper) into the
folder; mod.json gives them as programs 100 to 104, the numbers the tune's program changes name.

16 bars at 60 beats a minute (64 seconds), looping whole (markers loopStart and loopEnd). Channel
1 the drone, 2 the choir, 3 the music box, 4 the bell, 5 the whispers.
"""

import math
import os
import random
import struct
import sys
import wave

DIVISION = 480
TEMPO = 60
BAR = 4 * DIVISION

NOTE = {"C": 0, "C#": 1, "Db": 1, "D": 2, "Eb": 3, "E": 4, "F": 5, "F#": 6, "Gb": 6, "G": 7, "Ab": 8, "A": 9, "Bb": 10, "B": 11}


def pitch(name):
    """MIDI note of "Ab5" (C4 is 60)."""
    return 12 * (int(name[-1]) + 1) + NOTE[name[:-1]]


def frequency(note):
    return 440.0 * 2 ** ((note - 69) / 12)


# --- the instruments --------------------------------------------------------------------------


def write_wav(path, samples, rate):
    peak = max(1e-9, max(abs(s) for s in samples))
    with wave.open(path, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(rate)
        out.writeframes(b"".join(struct.pack("<h", int(30000 * s / peak)) for s in samples))


def musicbox():
    """A music box tine (root C6), two of them a few cents apart so the note shimmers and beats:
    the bright inharmonic partials of a tine, dying away."""
    rate, f = 32000, frequency(84)
    out = []
    for i in range(int(rate * 2.2)):
        t = i / rate
        tone = 0.0
        for detune in (1.0, 1.0035):
            g = f * detune
            tone += math.sin(2 * math.pi * g * t) * math.exp(-t * 2.2)
            tone += 0.35 * math.sin(2 * math.pi * 2.76 * g * t) * math.exp(-t * 7)
            tone += 0.12 * math.sin(2 * math.pi * 5.40 * g * t) * math.exp(-t * 16)
        out.append(tone * min(1.0, t / 0.001))
    return out, rate


def drone():
    """A low drone (root C2) that loops: its rate makes one period exactly 476 samples (17 ADPCM
    blocks), so 60 whole periods loop without a click, and its slow swell (once a loop) is a
    breath."""
    period = 476
    rate = round(frequency(36) * period)
    count = 60 * period
    out = []
    for i in range(count):
        phase = 2 * math.pi * i / period
        tone = math.sin(phase) + 0.5 * math.sin(2 * phase) + 0.25 * math.sin(3 * phase) + 0.15 * math.sin(5 * phase)
        breath = 0.75 + 0.25 * math.sin(2 * math.pi * i / count)
        out.append(tone * breath)
    return out, rate


def choir():
    """A wordless "ooh" (root C4): harmonics shaped by the vowel's formants, with a slow vibrato,
    swelling in and fading."""
    rate, f = 22050, frequency(60)
    formants = ((320, 90, 1.0), (800, 120, 0.4), (2400, 200, 0.08))
    harmonics = []
    for n in range(1, 24):
        h = n * f
        if h > rate / 2 - 500:
            break
        weight = sum(level * math.exp(-((h - centre) / width) ** 2) for centre, width, level in formants)
        harmonics.append((n, weight))
    out = []
    phase = 0.0
    seconds = 4.6
    for i in range(int(rate * seconds)):
        t = i / rate
        phase += 2 * math.pi * f * (1.0 + 0.004 * math.sin(2 * math.pi * 4.8 * t)) / rate
        tone = sum(weight * math.sin(n * phase) for n, weight in harmonics)
        envelope = min(1.0, t / 0.9) * min(1.0, (seconds - t) / 1.4)
        out.append(tone * envelope)
    return out, rate


def bell():
    """A deep bell (root C2): a church bell's partials (hum, prime, tierce, quint, nominal),
    tolling and dying slowly."""
    rate, f = 22050, frequency(36)
    partials = ((0.5, 0.6, 0.35), (1.0, 1.0, 0.5), (1.19, 0.5, 0.8), (1.5, 0.35, 1.0), (2.0, 0.45, 1.3), (2.51, 0.2, 2.0))
    out = []
    for i in range(int(rate * 6.0)):
        t = i / rate
        tone = sum(level * math.sin(2 * math.pi * ratio * f * t) * math.exp(-t * decay) for ratio, level, decay in partials)
        out.append(tone * min(1.0, t / 0.004))
    return out, rate


def whisper():
    """A breath (root C4): noise through a soft band-pass, swelling and falling away, as if
    someone just behind you breathed out."""
    rate = 16000
    noise = random.Random(13)
    out = []
    low = band = 0.0
    seconds = 1.6
    for i in range(int(rate * seconds)):
        t = i / rate
        x = noise.uniform(-1.0, 1.0)
        # a state-variable filter around 1.4 kHz, a little resonant
        f = 2 * math.sin(math.pi * 1400 / rate)
        low += f * band
        high = x - low - 0.7 * band
        band += f * high
        envelope = math.sin(math.pi * min(1.0, t / seconds)) ** 2
        out.append(band * envelope)
    return out, rate


INSTRUMENTS = {
    # program: (file, root, loop, sound)
    100: ("musicbox.wav", 84, False, musicbox),
    101: ("drone.wav", 36, True, drone),
    102: ("choir.wav", 60, False, choir),
    103: ("bell.wav", 36, False, bell),
    104: ("whisper.wav", 60, False, whisper),
}


# --- the tune ---------------------------------------------------------------------------------

MINOR, MAJOR, DIMINISHED = (0, 3, 7), (0, 4, 7), (0, 3, 6)
# (chord root and kind, the music box's notes as (note or None, beats))
BARS = [
    (("F", MINOR), [("C6", 1), ("Ab5", 1), ("F5", 2)]),
    (("Db", MAJOR), [("F5", 1), ("Ab5", 1), ("Db6", 1.5), ("C6", 0.5)]),
    (("Bb", MINOR), [("Bb5", 1), ("Db6", 1), ("F5", 2)]),
    (("C", MAJOR), [("E5", 1), ("G5", 1), ("C6", 2)]),
    (("F", MINOR), [("C6", 1), ("Ab5", 1), ("F5", 1), ("Ab5", 1)]),
    (("Db", MAJOR), [("Db6", 0.5), ("C6", 0.5), ("Ab5", 1), ("F5", 2)]),
    (("Bb", MINOR), [("Bb5", 1), ("Ab5", 1), ("Db5", 2)]),
    (("C", MAJOR), [("E5", 3), (None, 1)]),
    # it all but stops: the old "round and round" figure, once, then nothing
    (("F", MINOR), [(None, 4)]),
    (("Db", MAJOR), [("F5", 0.5), ("F5", 0.5), ("F5", 0.5), ("Db5", 0.5), (None, 2)]),
    (("B", DIMINISHED), [(None, 4)]),
    (("C", MAJOR), [("E5", 1), ("F5", 1), ("E5", 2)]),
    # again, slower to resolve, and it never does
    (("F", MINOR), [("C6", 1), ("Ab5", 1), ("F5", 2)]),
    (("Db", MAJOR), [("F5", 1), ("Ab5", 1), ("Db6", 1.5), ("C6", 0.5)]),
    (("Bb", MINOR), [("Bb5", 1), ("Db6", 1), ("F5", 2)]),
    (("C", MAJOR), [("E5", 2), ("B4", 2)]),
]

DRONE, CHOIR, BOX, BELL, WHISPER = 1, 2, 3, 4, 5


def events():
    found = []

    def note(channel, key, start, beats, velocity):
        end = start + int(beats * DIVISION) - 10
        found.append((start, 1, bytes((0x90 | channel, key, velocity))))
        found.append((end, 0, bytes((0x80 | channel, key, 0))))

    programs = {DRONE: 101, CHOIR: 102, BOX: 100, BELL: 103, WHISPER: 104}
    volumes = {DRONE: 96, CHOIR: 70, BOX: 104, BELL: 92, WHISPER: 76}
    pans = {DRONE: 64, CHOIR: 56, BOX: 72, BELL: 60, WHISPER: 100}
    for channel, program in programs.items():
        found.append((0, 0, bytes((0xC0 | channel, program))))
        found.append((0, 0, bytes((0xB0 | channel, 7, volumes[channel]))))
        found.append((0, 0, bytes((0xB0 | channel, 10, pans[channel]))))
    breaths = random.Random(5)
    for number, ((root, kind), melody) in enumerate(BARS):
        bar = number * BAR
        # the drone: F and C under everything, struck again every four bars
        if number % 4 == 0:
            note(DRONE, pitch("F1"), bar, 16, 100)
            note(DRONE, pitch("C2"), bar, 16, 80)
            note(BELL, pitch("F2"), bar, 4, 110)
        # the choir: the chord, low and close
        chord = [pitch(root + "3") + interval for interval in kind]
        chord = [key + 12 if key < pitch("E3") else key for key in chord]
        for key in chord:
            note(CHOIR, key, bar, 4, 62)
        at = bar
        for key, beats in melody:
            if key is not None:
                note(BOX, pitch(key), at, beats, 96)
            at += int(beats * DIVISION)
        # a breath now and then, nearer in the empty bars
        if number in (2, 6, 8, 10, 14) or (number > 8 and breaths.random() < 0.4):
            start = bar + int(breaths.choice((1.5, 2.0, 2.5, 3.0)) * DIVISION)
            note(WHISPER, breaths.choice((55, 58, 60, 62)), start, 1.5, 100 if number in (8, 10) else 80)
    return found, len(BARS) * BAR


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
    track = bytearray(variable(0) + meta(0x03, b"The Turning"))
    now = 0
    for tick, _order, data in found:
        track += variable(tick - now) + data
        now = tick
    track += variable(0) + meta(0x2F, b"")
    header = b"MThd" + struct.pack(">IHHH", 6, 0, 1, DIVISION)
    return header + b"MTrk" + struct.pack(">I", len(track)) + bytes(track)


def write(folder):
    """Writes turning.mid and the instruments' WAVs into folder."""
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "turning.mid"), "wb") as out:
        out.write(midi())
    for _program, (name, _root, _loop, sound) in INSTRUMENTS.items():
        samples, rate = sound()
        write_wav(os.path.join(folder, name), samples, rate)


if __name__ == "__main__":
    write(sys.argv[1] if len(sys.argv) > 1 else ".")
