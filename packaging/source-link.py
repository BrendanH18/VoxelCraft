#!/usr/bin/env python3
"""Point the bundled notices' VoxelCraft source archive at one commit.

`licenses.hbs` links VoxelCraft's source as the tag `v<Cargo version>`
(`v0.9.0`), but releases are tagged with two parts (`v0.9`), so CI rewrites
the link to the archive of the commit being built, which always exists.
"""

import re
import sys

path, commit = sys.argv[1], sys.argv[2]
if not re.fullmatch(r"[0-9a-f]{40}", commit):
    raise SystemExit(f"not a commit id: {commit!r}")
html = open(path, encoding="utf-8").read()
pattern = r"https://github\.com/BrendanH18/VoxelCraft/archive/refs/tags/v[0-9.]+\.tar\.gz"
html, count = re.subn(pattern, f"https://github.com/BrendanH18/VoxelCraft/archive/{commit}.tar.gz", html)
if count == 0:
    raise SystemExit("no VoxelCraft source link found in the notices")
open(path, "w", encoding="utf-8").write(html)
print(f"Linked VoxelCraft's source to {commit[:12]} ({count} link(s))")
