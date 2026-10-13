import tempfile
from pathlib import Path

from c2s import soundbank, sounds


def test_wavs_are_named_by_entry_number_with_an_optional_name():
    with tempfile.TemporaryDirectory() as folder:
        for name in ("003.wav", "012_door.WAV", "notes.txt"):
            (Path(folder) / name).write_bytes(b"")
        found = [(number, name) for number, name, _ in sounds.numbered(folder)]
        assert found == [(3, None), (12, "door")]
        assert sounds.file_name(7) == "007.wav"


def test_a_wav_not_named_by_number_is_refused():
    with tempfile.TemporaryDirectory() as folder:
        (Path(folder) / "door.wav").write_bytes(b"")
        try:
            sounds.numbered(folder)
        except ValueError as error:
            assert "entry number" in str(error)
        else:
            raise AssertionError("took a WAV without an entry number")


def test_one_entry_given_twice_is_refused():
    with tempfile.TemporaryDirectory() as folder:
        (Path(folder) / "003.wav").write_bytes(b"")
        (Path(folder) / "003_door.wav").write_bytes(b"")
        try:
            sounds.numbered(folder)
        except ValueError as error:
            assert "more than one" in str(error)
        else:
            raise AssertionError("took two WAVs for one entry")


def test_wav_round_trip_keeps_rate_samples_and_loop():
    sound = soundbank.Sound(22050, [0, 1000, -1000, 32767, -32768] * 10, loop_start=5)
    again = soundbank.read_wav(soundbank.wav_bytes(sound))
    assert sounds.same_sound(sound, again)


def test_adpcm_encodes_and_decodes_close_to_the_source():
    import math
    samples = [int(10000 * math.sin(i / 7.0)) for i in range(28 * 40)]
    decoded, loop = soundbank.decode_adpcm(soundbank.encode_adpcm(samples))
    assert loop is None
    assert soundbank.snr_db(samples, decoded[:len(samples)]) > 30
