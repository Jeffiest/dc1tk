import os
import tempfile
from pathlib import Path

from c2s import definitions, natives, verify
from tests import build

UNREGISTERED = {
    "action:6", "action:7", "action:13", "action:14", "action:44", "action:80",
    "effect:250",
    "event:492", "event:800",
    "monster:17", "monster:18", "monster:20", "monster:67", "monster:68", "monster:69",
    "monster:141", "monster:142", "monster:143", "monster:154", "monster:155", "monster:157", "monster:159",
}


def test_reports_every_script():
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        (root / "map").mkdir()
        (root / "map/t01.stb").write_bytes(build.sample_script())
        (root / "map/T02.STB").write_bytes(build.sample_script())
        (root / "map/t01.cfg").write_bytes(b"not a script")
        report = verify.verify(root, natives.load())
        summary = report["summary"]
        assert summary["scripts"] == 2
        assert summary["stages"]["format"]["pass"] == 2
        assert summary["stages"]["natives"]["pass"] == 2
        assert summary["stages"]["assembly"]["pass"] == 2
        assert summary["fallbacks"] == 0
        assert summary["stages"]["source"]["pass"] == 2
        assert summary["asm_functions"] == 4
        assert summary["native_calls"] == 2
        assert summary["unregistered"] == {}
        assert summary["ok"]
        assert summary["coverage"] == 1.0
        assert "OK" in verify.format_report(report, 5)


def test_corpus_round_trip():
    corpus = os.environ.get("C2S_CORPUS")
    if not corpus:
        return "skip"
    defs = definitions.load([definitions.DEFAULT_SET])
    report = verify.verify(corpus, natives.load(), defs, definitions.header_texts(defs, definitions.sdk.SDK_DIR))
    assert report["summary"]["scripts"] > 0, "no .stb files under " + corpus
    assert report["summary"]["ok"], verify.format_report(report, 20)
    assert report["summary"]["coverage"] == 1.0, verify.format_report(report, 20)
    assert set(report["summary"]["unregistered"]) == UNREGISTERED, verify.format_report(report, 20)
    assert report["summary"]["fallbacks"] == 0, verify.format_report(report, 20)
    assert report["summary"]["asm_functions"] == 0, verify.format_report(report, 20)
