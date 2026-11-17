"""Sets the version of a release, called by semantic-release before it commits.

    python3 scripts/set-version.py 0.1.0

- The workspace version in Cargo.toml, which every crate inherits, and the
  matching entries of Cargo.lock.
- CHANGELOG.md: the notes under "Unreleased" become the notes of this
  release, and a new empty "Unreleased" section is opened above them.
- The Obsidian plugin's manifest, which ships with every release.

The reference plugins keep their own versions; they are released with their
packages, not with the binary.
"""

import datetime
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    if len(sys.argv) != 2 or not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?", sys.argv[1]):
        sys.exit("usage: set-version.py X.Y.Z")
    version = sys.argv[1]

    cargo = ROOT / "Cargo.toml"
    text = cargo.read_text(encoding="utf-8")
    section = re.search(r"(?ms)^\[workspace\.package\]\n(.*?)(?=^\[)", text)
    if not section:
        sys.exit("Cargo.toml has no [workspace.package] section")
    body, count = re.subn(r'(?m)^version = "[^"]*"$', f'version = "{version}"', section.group(1), count=1)
    if count != 1:
        sys.exit("[workspace.package] has no version")
    cargo.write_text(text[: section.start(1)] + body + text[section.end(1) :], encoding="utf-8", newline="\n")
    # Only the workspace's own crates change in the lockfile.
    subprocess.run(["cargo", "update", "--workspace"], cwd=ROOT, check=True)

    manifest = ROOT / "integrations/obsidian/manifest.json"
    text = manifest.read_text(encoding="utf-8")
    text, count = re.subn(r'"version": "[^"]*"', f'"version": "{version}"', text, count=1)
    if count != 1:
        sys.exit("the Obsidian manifest has no version")
    manifest.write_text(text, encoding="utf-8", newline="\n")

    changelog = ROOT / "CHANGELOG.md"
    text = changelog.read_text(encoding="utf-8")
    heading = "## Unreleased\n"
    if heading not in text:
        sys.exit('CHANGELOG.md has no "## Unreleased" section')
    today = datetime.date.today().isoformat()
    text = text.replace(heading, f"{heading}\n## {version} ({today})\n", 1)
    changelog.write_text(text, encoding="utf-8", newline="\n")
    print(f"version {version}")


if __name__ == "__main__":
    main()
