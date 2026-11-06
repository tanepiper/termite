#!/usr/bin/env python3
"""The extra tools for test machine levels 2 and up, pinned by hash.

The machine has no network card, so these reach it on the read-only seed disk.

Usage: packages.py lock INSTALLED.txt   write packages.lock from the Debian index
       packages.py fetch FOLDER         download the locked files and check each hash
"""
import hashlib
import lzma
import re
import sys
import tarfile
import urllib.request
from pathlib import Path

# The security archive comes second, because the image already holds its updates.
ARCHIVES = (
    ("https://deb.debian.org/debian/", "bookworm"),
    ("https://deb.debian.org/debian-security/", "bookworm-security"),
)
LOCK = Path(__file__).parent / "packages.lock"
WANTED = """git jq ripgrep lsof tree rsync bc zip unzip xxd net-tools dnsutils tmux
ncal psmisc python3-pip bind9-libs bind9-host ffmpeg""".split()


def read_index():
    found = {}
    provided = {}
    for mirror, suite in ARCHIVES:
        url = f"{mirror}dists/{suite}/main/binary-arm64/Packages.xz"
        with urllib.request.urlopen(url, timeout=120) as reply:
            text = lzma.decompress(reply.read()).decode("utf-8", "replace")
        for block in text.split("\n\n"):
            fields = dict(re.findall(r"^([A-Za-z0-9-]+): (.*)$", block, re.M))
            if "Package" not in fields:
                continue
            fields["Url"] = mirror + fields["Filename"]
            found[fields["Package"]] = fields
            for name in re.findall(r"[a-z0-9.+-]+", re.sub(r"\(.*?\)", "", fields.get("Provides", ""))):
                provided.setdefault(name, fields["Package"])
    return found, provided


def needs(fields):
    """The first choice of each dependency, without versions."""
    wanted = fields.get("Pre-Depends", "") + "," + fields.get("Depends", "")
    for item in wanted.split(","):
        name = item.split("|")[0].split("(")[0].split(":")[0].strip()
        if name:
            yield name


def closure(found, provided, installed):
    chosen, queue = {}, list(WANTED)
    while queue:
        name = queue.pop()
        name = name if name in found else provided.get(name, name)
        if name in chosen or (name in installed and name not in WANTED):
            continue
        if name not in found:
            sys.exit(f"packages: {name} is not in the index")
        chosen[name] = found[name]
        queue.extend(needs(found[name]))
    return chosen


def lock(installed_path):
    installed = {line.split("=")[0].split(":")[0] for line in Path(installed_path).read_text().split()}
    found, provided = read_index()
    chosen = closure(found, provided, installed)
    rows = [
        "\t".join((name, item["Version"], item["SHA256"], item["Size"], item["Url"]))
        for name, item in sorted(chosen.items())
    ]
    LOCK.write_text("\n".join(rows) + "\n", encoding="utf-8")
    total = sum(int(item["Size"]) for item in chosen.values())
    print(f"{len(rows)} packages, {total / 1e6:.1f} MB")


def fetch(folder):
    folder = Path(folder)
    debs = folder / "debs"
    debs.mkdir(parents=True, exist_ok=True)
    for line in LOCK.read_text(encoding="utf-8").splitlines():
        name, _, sha256, _, url = line.split("\t")
        path = debs / f"{name}.deb"
        if not path.exists():
            with urllib.request.urlopen(url, timeout=120) as reply:
                path.write_bytes(reply.read())
        if hashlib.sha256(path.read_bytes()).hexdigest() != sha256:
            path.unlink()
            sys.exit(f"packages: {name} does not match its pinned hash")
    with tarfile.open(folder / "tools.tar", "w") as archive:
        for path in sorted(debs.glob("*.deb")):
            archive.add(path, arcname=path.name)
    print("every package matches its pinned hash")


if __name__ == "__main__":
    {"lock": lock, "fetch": fetch}[sys.argv[1]](sys.argv[2])
