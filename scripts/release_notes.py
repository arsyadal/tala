import re
import sys
import tomllib
from datetime import date
from pathlib import Path

SEMVER = re.compile(
    r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
HEADING = re.compile(r"^## \[([^\]]+)\](?: - ([0-9]{4}-[0-9]{2}-[0-9]{2}))?$", re.M)


def release_notes(root: Path, tag: str) -> str:
    package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    name, version = package["name"], package["version"]
    if tag != f"v{version}":
        raise ValueError(f"tag {tag!r} does not match Cargo.toml: expected v{version}")

    match = SEMVER.fullmatch(version)
    if match is None or any(
        part.isdigit() and len(part) > 1 and part.startswith("0")
        for part in (match.group(1) or "").split(".")
    ):
        raise ValueError("use a SemVer release or prerelease version without build metadata")

    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    locked = [item["version"] for item in lock["package"] if item["name"] == name and "source" not in item]
    if locked != [version]:
        raise ValueError(f"Cargo.lock must contain the local {name} package at {version}; run cargo check")

    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    headings = list(HEADING.finditer(changelog))
    matching = [index for index, heading in enumerate(headings) if heading.group(1) == version]
    if len(matching) != 1 or headings[matching[0]].group(2) is None:
        raise ValueError(f"CHANGELOG.md must contain exactly one dated heading: ## [{version}] - YYYY-MM-DD")
    index = matching[0]
    heading = headings[index]
    try:
        date.fromisoformat(heading.group(2))
    except ValueError as error:
        raise ValueError(f"CHANGELOG.md contains an invalid release date: {heading.group(2)}") from error
    end = headings[index + 1].start() if index + 1 < len(headings) else len(changelog)
    notes = changelog[heading.end():end].strip()
    if not any(line.startswith("- ") and line[2:].strip() for line in notes.splitlines()):
        raise ValueError(f"CHANGELOG.md release {version} must list at least one change")
    return notes


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: python scripts/release_notes.py vMAJOR.MINOR.PATCH[-PRERELEASE]")
    try:
        notes = release_notes(Path(__file__).resolve().parents[1], sys.argv[1])
    except (KeyError, OSError, ValueError) as error:
        sys.exit(f"release validation: {error}")
    sys.stdout.reconfigure(encoding="utf-8")
    print(notes)
