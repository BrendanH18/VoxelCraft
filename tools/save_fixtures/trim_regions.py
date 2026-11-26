"""trim_regions.py IN_DIR OUT_DIR CHUNK_Y: copies a world's region/ folder keeping only chunk row CHUNK_Y.

The region-file counterpart of trim_chunks.py (v1.0 Track 1b): each kept record
(VXK1 frame, payload and checksum) is copied byte for byte into the same table slot
of a new file laid out as the game lays one out (docs/saves.md, Region files); a
region with nothing kept gets no file. Prints `kept total`, counted in records.
"""

import struct
import sys
from pathlib import Path

SECTOR = 4096
SLOTS = 16 * 16 * 16
HEADER_SECTORS = 1 + SLOTS * 8 // SECTOR
MAGIC = b"VXREGION"


def records(data):
    """(slot, frame) for every table entry of a region file's bytes."""
    if data[:8] != MAGIC or struct.unpack_from("<I", data, 8)[0] != 1:
        raise SystemExit("not a version 1 region file")
    for slot in range(SLOTS):
        off, size = struct.unpack_from("<II", data, SECTOR + slot * 8)
        if size:
            yield slot, data[off * SECTOR : off * SECTOR + size]


def row(slot):
    """The chunk row a table slot holds (rows 14 and 15 are -2 and -1)."""
    y = slot // 256
    return y - 16 if y >= 14 else y


def main(src, dst, keep_y):
    dst.mkdir(parents=True, exist_ok=True)
    kept = total = 0
    for path in sorted(src.glob("r.*.vxr")):
        table = bytearray(SECTOR * HEADER_SECTORS)
        table[:8] = MAGIC
        struct.pack_into("<I", table, 8, 1)
        body = bytearray()
        for slot, frame in records(path.read_bytes()):
            total += 1
            if row(slot) != keep_y:
                continue
            first = HEADER_SECTORS + len(body) // SECTOR
            struct.pack_into("<II", table, SECTOR + slot * 8, first, len(frame))
            body += frame + bytes(-len(frame) % SECTOR)
            kept += 1
        if body:
            (dst / path.name).write_bytes(bytes(table) + bytes(body))
    print(f"{kept} {total}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3]))
