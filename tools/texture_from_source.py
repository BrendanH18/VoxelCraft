#!/usr/bin/env python3
"""Build an authored 16 x 16 texture PNG from its palette-and-pixels source.

A source (such as assets/textures/stone.source.json) has a `palette` of RGBA
colours and 16 `pixels` rows of 16 palette indices (0-9, then a-z). The PNG is
RGBA8 with unfiltered rows and no extra chunks; `--check` and
tools/validate_assets.py compare decoded pixels, so another zlib build's
compressed bytes don't matter.

    python3 tools/texture_from_source.py assets/textures/stone.source.json assets/textures/stone.png
    python3 tools/texture_from_source.py --check SOURCE PNG   exit 1 if the PNG is out of date
"""

import argparse
import json
import struct
import sys
import zlib
from pathlib import Path

DIGITS = "0123456789abcdefghijklmnopqrstuvwxyz"


def rgba_rows(source):
    palette = source["palette"]
    rows = source["pixels"]
    if len(rows) != 16 or any(len(r) != 16 for r in rows):
        raise ValueError("pixels must be 16 rows of 16 indices")
    out = []
    for row in rows:
        data = bytearray()
        for ch in row:
            colour = palette[DIGITS.index(ch)]
            if len(colour) != 4 or not all(0 <= c <= 255 for c in colour):
                raise ValueError("palette entries must be RGBA bytes")
            data.extend(colour)
        out.append(bytes(data))
    return out


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def scanlines(source):
    """The PNG's uncompressed image data: each row behind filter type 0."""
    return b"".join(b"\x00" + row for row in rgba_rows(source))


def png_bytes(source):
    raw = scanlines(source)
    header = struct.pack(">IIBBBBB", 16, 16, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def decoded(blob):
    """A PNG's uncompressed image data (its IDAT chunks, inflated)."""
    at, data = 8, bytearray()
    while at < len(blob):
        size = struct.unpack(">I", blob[at : at + 4])[0]
        if blob[at + 4 : at + 8] == b"IDAT":
            data.extend(blob[at + 8 : at + 8 + size])
        at += size + 12
    return zlib.decompress(bytes(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source", type=Path)
    parser.add_argument("png", type=Path)
    parser.add_argument("--check", action="store_true", help="only check that the PNG matches its source")
    args = parser.parse_args()
    source = json.loads(args.source.read_text(encoding="utf-8"))
    if args.check:
        if not args.png.is_file() or decoded(args.png.read_bytes()) != scanlines(source):
            print("{} is out of date; rebuild it from {}".format(args.png, args.source), file=sys.stderr)
            return 1
        return 0
    args.png.write_bytes(png_bytes(source))
    return 0


if __name__ == "__main__":
    sys.exit(main())
