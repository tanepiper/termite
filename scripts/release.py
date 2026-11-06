#!/usr/bin/env python3
"""The steps of a release that change files. .github/workflows/release.yml runs these.

  scripts/release.py version                     print the version in Cargo.toml
  scripts/release.py notes                       print the changelog's Unreleased section
  scripts/release.py record COMMIT DATE FOLDER   add this version to the release record
  scripts/release.py bump DATE                   date the changelog, and move to the next version

The record chains each release to the hash of the one before it, so history
can't be changed without it showing.
"""
import hashlib, json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
HEADING = "## Unreleased"


def version(root):
    found = re.search(r'^version = "(\d+\.\d+\.\d+)"$', (root / "Cargo.toml").read_text(), re.M)
    if not found:
        sys.exit("release.py: no version in Cargo.toml")
    return found.group(1)


def split_changelog(root):
    """The text before Unreleased, its section, and the text after that."""
    text = (root / "CHANGELOG.md").read_text()
    if HEADING not in text:
        sys.exit("release.py: the changelog has no Unreleased section")
    before, rest = text.split(HEADING + "\n", 1)
    later = re.search(r"^## ", rest, re.M)
    end = later.start() if later else len(rest)
    return before, rest[:end].strip(), rest[end:]


def notes(root):
    section = split_changelog(root)[1]
    if not section:
        sys.exit("release.py: the Unreleased section is empty")
    return section


def digest(record):
    return hashlib.sha256(json.dumps(record, sort_keys=True).encode()).hexdigest()


def add_record(root, commit, date, folder):
    path = root / "site/public/releases.json"
    data = json.loads(path.read_text())
    number = version(root)
    if any(entry["version"] == number for entry in data["releases"]):
        sys.exit(f"release.py: {number} is already in the record")
    files = {
        file.name: hashlib.sha256(file.read_bytes()).hexdigest()
        for file in sorted(pathlib.Path(folder).glob("*.tar.gz"))
    }
    if not files:
        sys.exit("release.py: nothing to record")
    previous = digest(data["releases"][0]) if data["releases"] else None
    entry = {"version": number, "date": date, "commit": commit, "files": files, "previous": previous}
    data["releases"].insert(0, entry)
    path.write_text(json.dumps(data, indent=2) + "\n")


def next_version(number):
    major, minor, _ = number.split(".")
    return f"{major}.{int(minor) + 1}.0"


def replace_once(path, old, new):
    text = path.read_text()
    if old not in text:
        sys.exit(f"release.py: {old!r} is not in {path.name}")
    path.write_text(text.replace(old, new, 1))


def bump(root, date):
    number = version(root)
    upcoming = next_version(number)
    before, section, after = split_changelog(root)
    released = f"{HEADING}\n\n## {number} - {date}\n\n{section}\n"
    (root / "CHANGELOG.md").write_text(before + released + ("\n" + after if after else ""))
    replace_once(root / "Cargo.toml", f'version = "{number}"', f'version = "{upcoming}"')
    replace_once(
        root / "Cargo.lock",
        f'name = "termite"\nversion = "{number}"',
        f'name = "termite"\nversion = "{upcoming}"',
    )
    return upcoming


def main(arguments):
    match arguments:
        case ["version"]:
            print(version(ROOT))
        case ["notes"]:
            print(notes(ROOT))
        case ["record", commit, date, folder]:
            add_record(ROOT, commit, date, folder)
        case ["bump", date]:
            print(bump(ROOT, date))
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
