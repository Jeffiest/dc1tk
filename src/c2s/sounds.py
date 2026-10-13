import re
from pathlib import Path

from c2s import soundbank

# Sound effects as WAV files for mods. A sound bank (.snd) holds the entries play_se plays by
# number, so a mod gives a folder named after the bank plus .wav (sp_000.snd.wav/) with one WAV
# per entry it changes, named by the entry's number (003.wav, or 003_door.wav). Entries the folder
# leaves out stay as they are, and a WAV numbered on from the bank's last entry adds one.
#
# A changed entry's samples are overwritten in place when no other entry plays them, so the bank
# does not grow; when others share them, the entry gets samples of its own. WAVs can be any rate
# (at most 48000 Hz is kept) and are mixed to mono; a 'smpl' loop makes a looping sound. A WAV that
# decodes to the entry's sound as it is changes nothing, so an unedited export builds back to the
# disc's own bytes.

SUFFIX = ".wav"
NAME = re.compile(r"^(\d+)(?:_(.*))?\.wav$", re.IGNORECASE)
NEW_ENTRY_NAME = "MOD_SE"


def file_name(index):
    return "%03d%s" % (index, SUFFIX)


# {file name: WAV bytes}: each entry that plays one sound, as the sample it plays. Entries that play
# a sequence (a short tune of the bank's instruments, music, ambient loops) are left out: a WAV
# given for one replaces the whole sequence with that sound.
def export(data):
    bank = soundbank.Bank.from_bytes(data)
    out = {}
    for entry in bank.entries():
        voices = bank.voices(entry)
        if entry.kind == soundbank.KIND_KEYON and voices:
            out[file_name(entry.index)] = soundbank.wav_bytes(bank.decode_vag(voices[0].vag))
    return out


def numbered(folder):
    found = []
    for path in sorted(Path(folder).iterdir()):
        if path.is_file() and path.suffix.lower() == SUFFIX:
            match = NAME.match(path.name)
            if not match:
                raise ValueError("%s: name a WAV by the entry number it replaces or adds (003.wav, 003_door.wav)"
                                 % path.name)
            found.append((int(match.group(1)), match.group(2), path))
    numbers = [number for number, _, _ in found]
    if len(set(numbers)) != len(numbers):
        raise ValueError("an entry is given by more than one WAV")
    return sorted(found)


def same_sound(a, b):
    return a.rate == b.rate and a.samples == b.samples and a.loop_start == b.loop_start


# The bank with the entries a folder of WAVs gives in place of the original's, and the entries it
# adds.
def build(folder, original):
    bank = soundbank.Bank.from_bytes(original)
    changed = False
    for number, name, path in numbered(folder):
        try:
            sound = soundbank.read_wav(path.read_bytes())
        except (ValueError, IndexError) as error:
            raise ValueError("%s: %s" % (path.name, error)) from None
        entries = bank.entries()
        if number >= len(entries):
            if number != len(entries):
                raise ValueError("%s: the bank has entries 0 to %d, so a new one is %s"
                                 % (path.name, len(entries) - 1, file_name(len(entries))))
            bank.add_entry(sound, name=name or NEW_ENTRY_NAME)
            changed = True
            continue
        entry = entries[number]
        voices = bank.voices(entry) if entry.kind == soundbank.KIND_KEYON else []
        if voices and same_sound(bank.decode_vag(voices[0].vag), sound):
            continue
        # in place only for an entry that plays one sound no other entry shares; a sequence entry
        # becomes the one sound (its tune would otherwise play the new sound at each note)
        vags = {voice.vag for voice in voices}
        shared = any(set(bank.users_of_vag(vag)) - {number} for vag in vags)
        bank.replace_entry(number, sound, mode="inplace" if voices and not shared else "detach")
        changed = True
    return bank.build() if changed else original
