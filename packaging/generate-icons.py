#!/usr/bin/env python3
"""Generate the procedural grass-block app icon using only the standard library."""

from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parent / "icons"


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def render(size):
    pixels = bytearray(size * size * 4)

    def polygon(vertices, colour):
        points = [(x * size / 256, y * size / 256) for x, y in vertices]
        for y in range(max(0, int(min(p[1] for p in points))), min(size, int(max(p[1] for p in points)) + 1)):
            scan = y + 0.5
            hits = []
            for a, b in zip(points, points[1:] + points[:1]):
                if min(a[1], b[1]) <= scan < max(a[1], b[1]):
                    hits.append(a[0] + (scan - a[1]) * (b[0] - a[0]) / (b[1] - a[1]))
            hits.sort()
            for left, right in zip(hits[::2], hits[1::2]):
                start = max(0, int(left + 0.5))
                end = min(size, int(right + 0.5))
                offset = (y * size + start) * 4
                pixels[offset:offset + (end - start) * 4] = bytes(colour) * (end - start)

    def face(origin, along, down, side):
        def point(u, v):
            return (origin[0] + along[0] * u + down[0] * v, origin[1] + along[1] * u + down[1] * v)

        for row in range(8):
            for col in range(8):
                noise = ((col * 31 + row * 17 + side * 13) % 19) - 9
                grass = side == 0 or row < 1 + (col * 7 % 3)
                base = ((107, 177, 66), (75, 133, 43), (54, 105, 38))[side] if grass else (
                    (0, 0, 0), (141, 96, 60), (108, 70, 46)
                )[side]
                colour = tuple(max(0, min(255, c + noise)) for c in base) + (255,)
                u, v = col / 8, row / 8
                polygon([point(u, v), point(u + 1 / 8, v), point(u + 1 / 8, v + 1 / 8), point(u, v + 1 / 8)], colour)

    polygon([(128, 134), (246, 188), (128, 252), (10, 188)], (12, 22, 14, 35))
    face((128, 20), (110, 62), (-110, 62), 0)
    face((18, 82), (110, 62), (0, 98), 1)
    face((128, 144), (110, -62), (0, 98), 2)
    rows = b"".join(b"\0" + pixels[y * size * 4:(y + 1) * size * 4] for y in range(size))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)) + chunk(
        b"IDAT", zlib.compress(rows, 9)
    ) + chunk(b"IEND", b"")


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    images = {size: render(size) for size in (16, 32, 48, 64, 128, 256, 512, 1024)}
    (ROOT / "VoxelCraft.png").write_bytes(images[256])
    sizes = (16, 32, 48, 64, 128, 256)
    offset = 6 + 16 * len(sizes)
    entries = bytearray()
    payload = bytearray()
    for size in sizes:
        png = images[size]
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset)
        payload += png
        offset += len(png)
    (ROOT / "VoxelCraft.ico").write_bytes(struct.pack("<HHH", 0, 1, len(sizes)) + entries + payload)
    payload = bytearray()
    for size, kind in ((16, b"icp4"), (32, b"icp5"), (64, b"icp6"), (128, b"ic07"), (256, b"ic08"), (512, b"ic09"), (1024, b"ic10")):
        png = images[size]
        payload += kind + struct.pack(">I", 8 + len(png)) + png
    (ROOT / "VoxelCraft.icns").write_bytes(b"icns" + struct.pack(">I", 8 + len(payload)) + payload)


if __name__ == "__main__":
    main()
