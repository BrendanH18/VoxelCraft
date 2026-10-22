#!/usr/bin/env python3
"""Reject a release tag that does not name Cargo's package version.

Release tags are two-part, with or without a leading `v`: `v0.9`, `0.9` and
`1.0` all name Cargo version `0.9.0` / `1.0.0`. A three-part tag such as
`v0.9.1` still works for a fix release.
"""

import os
from pathlib import Path
import json
import subprocess


def tag_version(tag: str) -> str:
    """The Cargo version a tag names, or an error for a malformed tag."""
    parts = tag.removeprefix("v").split(".")
    if not 2 <= len(parts) <= 3 or not all(p.isdigit() for p in parts):
        raise SystemExit(f"Tag {tag!r} is not a version like v0.9, 1.0 or v0.9.1")
    return ".".join(str(int(p)) for p in parts + ["0"] * (3 - len(parts)))


if __name__ == "__main__":
    repo = Path(__file__).resolve().parent.parent
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=repo)
    )
    version = next(package["version"] for package in metadata["packages"] if package["name"] == "voxelcraft")
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        tag = os.environ["GITHUB_REF_NAME"]
        if tag_version(tag) != version:
            raise SystemExit(f"Tag {tag!r} names version {tag_version(tag)}, but Cargo.toml is {version}")
    print(f"Packaging VoxelCraft {version}")
