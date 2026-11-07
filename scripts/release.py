#!/usr/bin/env python3
"""The steps of a release that change files. .github/workflows/release.yml runs these.

  scripts/release.py version                     print the version in Cargo.toml
  scripts/release.py prepare DATE                drop -dev, and date the Unreleased section
  scripts/release.py notes                       print the changelog's section for this version
  scripts/release.py record COMMIT DATE FOLDER   add this version to the release record
  scripts/release.py bump                        open Unreleased, and move to the next -dev version

The record chains each release to the hash of the one before it, so history
can't be changed without it showing.
"""
import hashlib, json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
HEADING = "## Unreleased"
DEV = "-dev"


def version(root):
    found = re.search(r'^version = "(\d+\.\d+\.\d+(?:-dev)?)"$', (root / "Cargo.toml").read_text(), re.M)
    if not found:
        sys.exit("release.py: no version in Cargo.toml")
    return found.group(1)


def section_of(text, heading):
    """The text under a heading that matches this pattern, up to the next heading."""
    found = re.search(rf"^{heading}$", text, re.M)
    if not found:
        sys.exit(f"release.py: the changelog has no section for {heading}")
    later = re.search(r"^## ", text[found.end():], re.M)
    end = found.end() + later.start() if later else len(text)
    return text[found.end():end].strip()


def notes(root):
    number = re.escape(version(root))
    return section_of((root / "CHANGELOG.md").read_text(), rf"## {number} - .*")


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
    return f"{major}.{int(minor) + 1}.0-dev"


def replace_once(path, old, new):
    text = path.read_text()
    if old not in text:
        sys.exit(f"release.py: {old!r} is not in {path.name}")
    path.write_text(text.replace(old, new, 1))


def set_version(root, old, new):
    replace_once(root / "Cargo.toml", f'version = "{old}"', f'version = "{new}"')
    replace_once(root / "Cargo.lock", f'name = "termite"\nversion = "{old}"', f'name = "termite"\nversion = "{new}"')


def prepare(root, date):
    """Turn the work in progress into the release: the commit that gets the tag."""
    current = version(root)
    if not current.endswith(DEV):
        sys.exit(f"release.py: {current} is not a -dev version")
    number = current.removesuffix(DEV)
    path = root / "CHANGELOG.md"
    if not section_of(path.read_text(), HEADING):
        sys.exit("release.py: the Unreleased section is empty")
    replace_once(path, HEADING, f"## {number} - {date}")
    set_version(root, current, number)
    return number


def bump(root):
    """After the release: open a new Unreleased section, and start the next version."""
    number = version(root)
    upcoming = next_version(number)
    replace_once(root / "CHANGELOG.md", f"## {number} - ", f"{HEADING}\n\n## {number} - ")
    set_version(root, number, upcoming)
    return upcoming


def main(arguments):
    match arguments:
        case ["version"]:
            print(version(ROOT))
        case ["prepare", date]:
            print(prepare(ROOT, date))
        case ["notes"]:
            print(notes(ROOT))
        case ["record", commit, date, folder]:
            add_record(ROOT, commit, date, folder)
        case ["bump"]:
            print(bump(ROOT))
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
