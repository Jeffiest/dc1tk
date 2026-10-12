import os
import shutil
import subprocess
from pathlib import Path

# Boots a disc image in PCSX2, for trying a build: cli.py play IMAGE, or build --play.

NAMES = ("pcsx2-qt", "pcsx2-qtx64", "pcsx2")
INSTALLS = ("C:/Program Files/PCSX2/pcsx2-qt.exe", "C:/Program Files (x86)/PCSX2/pcsx2-qt.exe",
            "/Applications/PCSX2.app/Contents/MacOS/PCSX2", "/usr/bin/pcsx2-qt", "/usr/local/bin/pcsx2-qt")


# PCSX2's executable: the one given, else the PCSX2 environment variable, else one on the PATH,
# else a usual install folder. None when there is none.
def find_pcsx2(given=None, environment=os.environ, which=shutil.which, exists=os.path.isfile):
    for candidate in (given, environment.get("PCSX2")):
        if candidate:
            return candidate if exists(candidate) else None
    for name in NAMES:
        found = which(name)
        if found:
            return found
    for candidate in INSTALLS:
        if exists(candidate):
            return candidate
    return None


def command(pcsx2, image, fast_boot=True):
    return [pcsx2] + (["-fastboot"] if fast_boot else []) + ["--", str(Path(image).resolve())]


def launch(image, pcsx2=None, fast_boot=True):
    found = find_pcsx2(pcsx2)
    if found is None:
        raise SystemExit("PCSX2 not found; pass --pcsx2 PATH or set the PCSX2 environment variable")
    if not Path(image).is_file():
        raise SystemExit("no image at %s" % image)
    subprocess.Popen(command(found, image, fast_boot), close_fds=True)
    return found
