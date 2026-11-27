"""trim_chunks.py IN OUT CHUNK_Y: copies a chunks.bin keeping only the records whose chunk y is CHUNK_Y.

Records are copied byte for byte; only the header's record count changes. Prints kept/total."""

import struct
import sys

src, dst, keep_y = sys.argv[1], sys.argv[2], int(sys.argv[3])
b = open(src, "rb").read()
n = struct.unpack_from("<I", b, 4)[0]
at, kept = 8, []
for _ in range(n):
    _x, y, _z, length = struct.unpack_from("<iiiI", b, at)
    rec = b[at : at + 16 + length]
    at += 16 + length
    if y == keep_y:
        kept.append(rec)
open(dst, "wb").write(b[:4] + struct.pack("<I", len(kept)) + b"".join(kept))
print(f"{len(kept)} {n}")
