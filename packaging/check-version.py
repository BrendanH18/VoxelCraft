#!/usr/bin/env python3
"""Reject a release tag that does not match Cargo's package version."""

import os
from pathlib import Path
import json
import subprocess

repo = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=repo))
version = next(package["version"] for package in metadata["packages"] if package["name"] == "voxelcraft")
if os.environ.get("GITHUB_REF_TYPE") == "tag":
    tag = os.environ["GITHUB_REF_NAME"]
    if tag != f"v{version}":
        raise SystemExit(f"Tag {tag!r} does not match Cargo.toml version v{version}")
print(f"Packaging VoxelCraft {version}")
