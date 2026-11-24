#!/usr/bin/env python3
"""Put before and after captures side by side, one image per scene.

    python3 tools/compare_captures.py target/captures/<before> target/captures/<after>
        [--out target/captures/compare-<before>-<after>] [--scene NAME ...]

Every PNG present in both folders (same file name, as tools/capture_scenes.py
names them) becomes <out>/<name>: the before image on the left, the after
image on the right, an 8-pixel gap between. Standard library only: 8-bit RGB
or RGBA PNGs, not interlaced.
"""

import argparse
import struct
import sys
import zlib
from pathlib import Path

GAP = 8
GAP_COLOUR = (255, 0, 255)


def read_png(path):
    """(width, height, rows of RGB bytes)."""
    blob = path.read_bytes()
    if blob[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("{}: not a PNG".format(path))
    at, data = 8, bytearray()
    width = height = channels = None
    while at < len(blob):
        size = struct.unpack(">I", blob[at : at + 4])[0]
        kind, body = blob[at + 4 : at + 8], blob[at + 8 : at + 8 + size]
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or colour not in (2, 6) or interlace:
                raise ValueError("{}: needs 8-bit RGB or RGBA without interlacing".format(path))
            channels = 3 if colour == 2 else 4
        elif kind == b"IDAT":
            data.extend(body)
        at += size + 12
    raw = zlib.decompress(bytes(data))
    stride = width * channels
    rows, prev = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind = raw[start]
        line = bytearray(raw[start + 1 : start + 1 + stride])
        unfilter(kind, line, prev, channels)
        rows.append(rgb(line, channels))
        prev = line
    return width, height, rows


def unfilter(kind, line, prev, bpp):
    n = len(line)
    if kind == 1:
        for i in range(bpp, n):
            line[i] = (line[i] + line[i - bpp]) & 255
    elif kind == 2:
        for i in range(n):
            line[i] = (line[i] + prev[i]) & 255
    elif kind == 3:
        for i in range(n):
            left = line[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
    elif kind == 4:
        for i in range(n):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            p = a + b - c
            pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
    elif kind != 0:
        raise ValueError("unknown PNG filter {}".format(kind))


def rgb(line, channels):
    if channels == 3:
        return bytes(line)
    out = bytearray(len(line) // 4 * 3)
    out[0::3] = line[0::4]
    out[1::3] = line[1::4]
    out[2::3] = line[2::4]
    return bytes(out)


def write_png(path, width, height, rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + row for row in rows)
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    path.write_bytes(png)


def side_by_side(before, after):
    (w1, h1, r1), (w2, h2, r2) = before, after
    height = max(h1, h2)
    gap = bytes(GAP_COLOUR) * GAP
    rows = []
    for y in range(height):
        left = r1[y] if y < h1 else bytes(3 * w1)
        right = r2[y] if y < h2 else bytes(3 * w2)
        rows.append(left + gap + right)
    return w1 + GAP + w2, height, rows


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--scene", action="append", default=[], help="only captures whose name starts with this")
    args = parser.parse_args()
    out = args.out or args.before.parent / "compare-{}-{}".format(args.before.name, args.after.name)
    out.mkdir(parents=True, exist_ok=True)
    names = sorted(p.name for p in args.before.glob("*.png") if (args.after / p.name).is_file())
    if args.scene:
        names = [n for n in names if any(n.startswith(s) for s in args.scene)]
    if not names:
        print("no captures in both folders", file=sys.stderr)
        return 1
    for name in names:
        image = side_by_side(read_png(args.before / name), read_png(args.after / name))
        write_png(out / name, *image)
        print(out / name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
