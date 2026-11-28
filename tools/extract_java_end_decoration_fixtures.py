#!/usr/bin/env python3
"""Extract the End's decoration from a verified official Java 26.3 server jar.

v1.0 gap-closing Track 4b (GEN-04b). Runs the pinned dedicated server itself
on a fresh world for each seed, force-loads End chunks, saves, and reads the
region files back:

- a grid of chunks out past the void (every decorated chunk whose eight
  neighbours were decorated too): per chunk, the number of end stone blocks
  and an FNV-1a hash of their positions (the terrain plus `end_island`), the
  chorus blocks' count and hash (`chorus_plant`), whether a city stands in it
  (any block but end stone, chorus, gateways and bedrock), and the biome Java stored at its middle;
- the chorus plants' bases (a `chorus_plant` on end stone) in the grid;
- every `end_gateway` block in the chunks the grid covers and in the chunks
  where `end_gateway_return`'s rarity filter passes within 128 chunks of
  the centre (with both of its possible feature indices; the chunks are
  computed here with `WorldgenRandom`, as `ChunkGenerator.applyBiomeDecoration`
  seeds it).

Needs `java` (JDK 25) on `PATH`, about 8 GB of memory and several minutes.
Never reads VoxelCraft source; nothing decompiled is written.
"""

import argparse
import hashlib
import io
import json
import re
import struct
import subprocess
import tempfile
import threading
import time
import zipfile
import zlib
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

SEEDS = [0, 42]
# The grid of chunks force-loaded for each seed, inclusive; the compared
# chunks are its interior (one chunk in from each side).
GRID = {0: ((62, -18), (97, 17)), 42: ((-97, 62), (-62, 97))}
GATEWAY_REACH = 128
# Blocks the End's terrain and features place; anything else is an End city.
PLAIN = ("end_stone", "end_gateway", "bedrock")
MASK = (1 << 64) - 1


def signed(v):
    v &= MASK
    return v - (1 << 64) if v >= 1 << 63 else v


def rotl(v, n):
    return ((v << n) | (v >> (64 - n))) & MASK


def mix_stafford13(z):
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


class WorldgenRandom:
    """`WorldgenRandom` over a `XoroshiroRandomSource`."""

    def __init__(self, seed):
        self.set_seed(seed)

    def set_seed(self, seed):
        lo = (seed & MASK) ^ 0x6A09E667F3BCC909
        hi = (lo + 0x9E3779B97F4A7C15) & MASK
        self.lo, self.hi = mix_stafford13(lo), mix_stafford13(hi)

    def next_long_raw(self):
        lo, hi = self.lo, self.hi
        result = (rotl((lo + hi) & MASK, 17) + lo) & MASK
        hi ^= lo
        self.lo = rotl(lo, 49) ^ hi ^ ((hi << 21) & MASK)
        self.hi = rotl(hi, 28)
        return result

    def next(self, bits):
        v = self.next_long_raw() >> (64 - bits)
        return v - (1 << bits) if bits == 32 and v >= 1 << 31 else v

    def next_long(self):
        return signed((self.next(32) << 32) + self.next(32))

    def next_float(self):
        return self.next(24) * 5.9604645e-8

    def set_decoration_seed(self, seed, x, z):
        self.set_seed(seed)
        xs = self.next_long() | 1
        zs = self.next_long() | 1
        result = signed(x * xs + z * zs) ^ seed
        self.set_seed(result)
        return result

    def set_feature_seed(self, decoration, index, step):
        self.set_seed(decoration + index + 10000 * step)


def gateway_candidates(seed):
    """Chunks where `end_gateway_return` (step 4, index 0 or 1) passes its
    `rarity_filter` of 700."""
    out = set()
    r = WorldgenRandom(0)
    for cx in range(-GATEWAY_REACH, GATEWAY_REACH):
        for cz in range(-GATEWAY_REACH, GATEWAY_REACH):
            if cx * cx + cz * cz <= 4096:
                continue
            decoration = r.set_decoration_seed(seed, cx * 16, cz * 16)
            for index in (0, 1):
                r.set_feature_seed(decoration, index, 4)
                if r.next_float() < 1.0 / 700:
                    out.add((cx, cz))
    return sorted(out)


def read_nbt(data):
    pos = 0

    def take(n):
        nonlocal pos
        out = data[pos : pos + n]
        pos += n
        return out

    def payload(tag):
        if tag == 1:
            return struct.unpack(">b", take(1))[0]
        if tag == 2:
            return struct.unpack(">h", take(2))[0]
        if tag == 3:
            return struct.unpack(">i", take(4))[0]
        if tag == 4:
            return struct.unpack(">q", take(8))[0]
        if tag == 5:
            return struct.unpack(">f", take(4))[0]
        if tag == 6:
            return struct.unpack(">d", take(8))[0]
        if tag == 7:
            n = struct.unpack(">i", take(4))[0]
            return take(n)
        if tag == 8:
            n = struct.unpack(">H", take(2))[0]
            return take(n).decode("utf-8", "replace")
        if tag == 9:
            inner, n = struct.unpack(">bi", take(5))
            return [payload(inner) for _ in range(n)]
        if tag == 10:
            out = {}
            while True:
                inner = take(1)[0]
                if inner == 0:
                    return out
                n = struct.unpack(">H", take(2))[0]
                name = take(n).decode("utf-8", "replace")
                out[name] = payload(inner)
        if tag == 11:
            n = struct.unpack(">i", take(4))[0]
            return list(struct.unpack(">%di" % n, take(4 * n)))
        if tag == 12:
            n = struct.unpack(">i", take(4))[0]
            return list(struct.unpack(">%dq" % n, take(8 * n)))
        raise ValueError("bad tag %d" % tag)

    tag = take(1)[0]
    n = struct.unpack(">H", take(2))[0]
    take(n)
    return payload(tag)


def read_chunk(region_dir, cx, cz):
    path = region_dir / ("r.%d.%d.mca" % (cx >> 5, cz >> 5))
    if not path.exists():
        return None
    with path.open("rb") as f:
        header = f.read(4096)
        i = 4 * ((cx & 31) + (cz & 31) * 32)
        offset = int.from_bytes(header[i : i + 3], "big")
        if offset == 0:
            return None
        f.seek(offset * 4096)
        length, kind = struct.unpack(">ib", f.read(5))
        raw = f.read(length - 1)
    if kind != 2:
        raise ValueError("unexpected chunk compression %d" % kind)
    return read_nbt(zlib.decompress(raw))


def unpack(values, count, bits):
    per = 64 // bits
    mask = (1 << bits) - 1
    out = []
    for v in values:
        v &= MASK
        for k in range(per):
            out.append((v >> (k * bits)) & mask)
            if len(out) == count:
                return out
    return out


def state_name(p):
    """A palette entry's block name, with the chorus flower's age: a string,
    or in a list that mixes them, a compound (`id`, `properties`) or a
    string wrapped in a compound under the empty key."""
    if isinstance(p, dict) and "" in p:
        p = p[""]
    if isinstance(p, dict):
        name, props = p["id"], p.get("properties", {})
    else:
        name, _, rest = p.partition("[")
        props = dict(kv.split("=", 1) for kv in rest.rstrip("]").split(",") if "=" in kv)
    name = name.replace("minecraft:", "")
    if name == "chorus_flower":
        name += ":" + props.get("age", "0")
    return name


def chunk_blocks(nbt):
    """Each block's name (and the flower's age) by (x, y, z), air left out."""
    blocks = {}
    base_x, base_z = nbt["xPos"] * 16, nbt["zPos"] * 16
    for section in nbt.get("sections", []):
        states = section.get("block_states")
        if not states:
            continue
        palette = states["palette"]
        names = []
        for p in palette:
            names.append(state_name(p))
        if "data" in states:
            bits = max(4, (len(palette) - 1).bit_length())
            ids = unpack(states["data"], 4096, bits)
        else:
            ids = [0] * 4096
        y0 = section["Y"] * 16
        for i, k in enumerate(ids):
            name = names[k]
            if name in ("air", "cave_air", "void_air"):
                continue
            blocks[(base_x + (i & 15), y0 + (i >> 8), base_z + ((i >> 4) & 15))] = name
    return blocks


def chunk_biome(nbt):
    """The biome Java stored at the chunk's middle quart column (y 64)."""
    for section in nbt.get("sections", []):
        if section["Y"] != 4 or "biomes" not in section:
            continue
        palette = section["biomes"]["palette"]
        if "data" not in section["biomes"]:
            return palette[0].replace("minecraft:", "")
        bits = (len(palette) - 1).bit_length()
        ids = unpack(section["biomes"]["data"], 64, bits)
        return palette[ids[(0 * 4 + 2) * 4 + 2]].replace("minecraft:", "")
    return None


def fnv(items):
    h = 0xCBF29CE484222325
    for item in items:
        for b in item.encode():
            h = ((h ^ b) * 0x100000001B3) & MASK
    return "%016x" % h


class Server:
    def __init__(self, jar, root, seed):
        (root / "eula.txt").write_text("eula=true\n")
        (root / "server.properties").write_text(
            "level-seed=%d\nonline-mode=false\nmax-tick-time=-1\nspawn-protection=0\n"
            "view-distance=2\nsimulation-distance=2\nsync-chunk-writes=false\n" % seed
        )
        self.proc = subprocess.Popen(
            ["java", "-Xmx10G", "-jar", str(jar), "--nogui"],
            cwd=root,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.lines = []
        self.lock = threading.Lock()
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        for line in self.proc.stdout:
            with self.lock:
                self.lines.append(line.rstrip("\n"))

    def wait_for(self, text, since=0, timeout=1800):
        end = time.time() + timeout
        while time.time() < end:
            with self.lock:
                for line in self.lines[since:]:
                    if text in line:
                        return
            if self.proc.poll() is not None:
                raise RuntimeError("server exited")
            time.sleep(0.5)
        raise TimeoutError(text)

    def count(self, text, since):
        with self.lock:
            return sum(text in line for line in self.lines[since:])

    def send(self, command):
        self.proc.stdin.write(command + "\n")
        self.proc.stdin.flush()


def chunks_of_grid(seed):
    (x0, z0), (x1, z1) = GRID[seed]
    return [(x, z) for x in range(x0, x1 + 1) for z in range(z0, z1 + 1)]


def load_chunks(server, chunks):
    """Force-load `chunks` in the End, waiting until each is fully loaded."""
    for x, z in chunks:
        server.send("execute in minecraft:the_end run forceload add %d %d" % (x * 16, z * 16))
    pending = list(chunks)
    while pending:
        time.sleep(10)
        with server.lock:
            mark = len(server.lines)
        for x, z in pending:
            server.send("execute in minecraft:the_end if loaded %d 0 %d" % (x * 16, z * 16))
        time.sleep(2)
        server.wait_for("Test", mark)
        time.sleep(3)
        if server.count("Test passed", mark) == len(pending):
            pending = []


def extract_seed(jar, seed, keep=None):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-end-decoration-") as temporary:
        root = Path(temporary) if keep is None else keep / str(seed)
        root.mkdir(parents=True, exist_ok=True)
        server = Server(jar, root, seed)
        server.wait_for("Done (")
        grid = chunks_of_grid(seed)
        candidates = gateway_candidates(seed)
        load_chunks(server, sorted(set(grid) | set(candidates)))
        mark = len(server.lines)
        server.send("save-all flush")
        server.wait_for("Saved the game", mark)
        server.send("stop")
        server.proc.wait(timeout=600)
        region = root / "world" / "dimensions" / "minecraft" / "the_end" / "region"
        if not region.exists():
            region = root / "world" / "DIM1" / "region"
        blocks = {}
        biomes = {}
        statuses = {}
        wanted = set(grid)
        for x, z in candidates:
            wanted.update((x + dx, z + dz) for dx in (-1, 0, 1) for dz in (-1, 0, 1))
        for x, z in sorted(wanted):
            nbt = read_chunk(region, x, z)
            if nbt is None:
                continue
            statuses[(x, z)] = nbt.get("Status", "").replace("minecraft:", "")
            blocks[(x, z)] = chunk_blocks(nbt)
            biomes[(x, z)] = chunk_biome(nbt)
    (x0, z0), (x1, z1) = GRID[seed]
    chunks = []
    bases = []
    for x in range(x0 + 1, x1):
        for z in range(z0 + 1, z1):
            if statuses.get((x, z)) != "full":
                raise ValueError("chunk %d %d not generated" % (x, z))
            b = blocks[(x, z)]
            stone = sorted(p for p, n in b.items() if n == "end_stone")
            chorus = sorted((p, n) for p, n in b.items() if n.startswith("chorus"))
            city = any(n not in PLAIN and not n.startswith("chorus") for n in b.values())
            chunks.append(
                [
                    x,
                    z,
                    biomes[(x, z)],
                    len(stone),
                    fnv("%d %d %d;" % p for p in stone),
                    len(chorus),
                    fnv("%d %d %d %s;" % (p + (n,)) for p, n in chorus),
                    city,
                ]
            )
            for p, n in b.items():
                if n == "chorus_plant" and b.get((p[0], p[1] - 1, p[2])) == "end_stone":
                    bases.append(list(p))
    gateways = set()
    covered = set((x, z) for x in range(x0 + 1, x1) for z in range(z0 + 1, z1))
    for x, z in candidates:
        if statuses.get((x, z)) == "full":
            covered.add((x, z))
    for c in covered:
        for dx in (-1, 0, 1):
            for dz in (-1, 0, 1):
                for p, n in blocks.get((c[0] + dx, c[1] + dz), {}).items():
                    if n == "end_gateway" and (p[0] >> 4, p[2] >> 4) == c:
                        gateways.add(p)
    return {
        "grid": [[x0 + 1, z0 + 1], [x1 - 1, z1 - 1]],
        "chunks": chunks,
        "chorus_bases": sorted(bases),
        "gateway_chunks": sorted([list(c) for c in covered if c not in set(chunks_of_grid(seed))]),
        "gateways": sorted([list(p) for p in gateways]),
    }


# v1.0 gap-closing Track 4c: End cities. For each seed, `locate` finds the
# city nearest each of these points, and a square of chunks CITY_REACH out
# from its start chunk is force-loaded (its interior compared).
CITY_ORIGINS = {0: [(1500, 0), (0, -1500)], 42: [(-1500, 0), (0, 1500)]}
CITY_REACH = 7


def find(nbt, key):
    """`nbt[key]` under either capitalisation (26.3 renamed some keys)."""
    for k in (key, key[:1].lower() + key[1:], key[:1].upper() + key[1:]):
        if k in nbt:
            return nbt[k]
    return None


def city_starts(nbt):
    """The `end_city` structure start stored in a chunk, if any: its
    pieces as [template, rotation, overwrite, x0, y0, z0, x1, y1, z1]."""
    structures = find(nbt, "structures") or {}
    starts = find(structures, "starts") or {}
    start = starts.get("minecraft:end_city")
    if not start or find(start, "id") == "INVALID":
        return None
    pieces = []
    for child in find(start, "Children") or []:
        bb = find(child, "BB")
        pieces.append(
            [
                find(child, "Template").replace("minecraft:", ""),
                find(child, "Rot"),
                bool(find(child, "OW")),
            ]
            + list(bb)
        )
    return pieces


def city_references(nbt):
    """Start chunks of the `end_city` starts whose pieces reach this chunk."""
    structures = find(nbt, "structures") or {}
    refs = (find(structures, "References") or {}).get("minecraft:end_city", [])
    out = []
    for packed in refs:
        packed &= MASK
        x, z = packed & 0xFFFFFFFF, packed >> 32
        out.append((x - (1 << 32) if x >= 1 << 31 else x, z - (1 << 32) if z >= 1 << 31 else z))
    return out


def locate_city(server, x, z):
    mark = len(server.lines)
    server.send("execute in minecraft:the_end positioned %d 64 %d run locate structure minecraft:end_city" % (x, z))
    pattern = re.compile(r"nearest minecraft:end_city is at \[(-?\d+), ~, (-?\d+)\]")
    end = time.time() + 600
    while time.time() < end:
        with server.lock:
            for line in server.lines[mark:]:
                m = pattern.search(line)
                if m:
                    return int(m.group(1)) >> 4, int(m.group(2)) >> 4
                if "Could not find" in line:
                    raise ValueError("no End city near %d %d" % (x, z))
        time.sleep(0.5)
    raise TimeoutError("locate")


def chunk_row(x, z, biome, b, footprint):
    """A chunk's row: as `extract_seed`'s, then the end stone and chorus of
    the columns outside every city piece's box (`footprint`)."""
    stone = sorted(p for p, n in b.items() if n == "end_stone")
    chorus = sorted((p, n) for p, n in b.items() if n.startswith("chorus"))
    city = any(n not in PLAIN and not n.startswith("chorus") for n in b.values())
    stone_out = [p for p in stone if (p[0], p[2]) not in footprint]
    chorus_out = [(p, n) for p, n in chorus if (p[0], p[2]) not in footprint]
    return [
        x,
        z,
        biome,
        len(stone),
        fnv("%d %d %d;" % p for p in stone),
        len(chorus),
        fnv("%d %d %d %s;" % (p + (n,)) for p, n in chorus),
        city,
        len(stone_out),
        fnv("%d %d %d;" % p for p in stone_out),
        len(chorus_out),
        fnv("%d %d %d %s;" % (p + (n,)) for p, n in chorus_out),
    ]


def extract_city_seed(jar, seed, keep=None):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-end-cities-") as temporary:
        root = Path(temporary) if keep is None else keep / ("cities-%d" % seed)
        root.mkdir(parents=True, exist_ok=True)
        server = Server(jar, root, seed)
        server.wait_for("Done (")
        centres = []
        for x, z in CITY_ORIGINS[seed]:
            c = locate_city(server, x, z)
            if c not in centres:
                centres.append(c)
        grids = []
        loaded = set()
        for cx, cz in centres:
            grid = [
                (x, z)
                for x in range(cx - CITY_REACH, cx + CITY_REACH + 1)
                for z in range(cz - CITY_REACH, cz + CITY_REACH + 1)
            ]
            grids.append(((cx, cz), grid))
            loaded.update(grid)
        load_chunks(server, sorted(loaded))
        mark = len(server.lines)
        server.send("save-all flush")
        server.wait_for("Saved the game", mark)
        server.send("stop")
        server.proc.wait(timeout=600)
        region = root / "world" / "dimensions" / "minecraft" / "the_end" / "region"
        if not region.exists():
            region = root / "world" / "DIM1" / "region"
        nbts = {c: read_chunk(region, c[0], c[1]) for c in loaded}
        out = []
        for (cx, cz), grid in grids:
            interior = [(x, z) for x, z in grid if abs(x - cx) < CITY_REACH and abs(z - cz) < CITY_REACH]
            refs = set()
            for c in interior:
                refs.update(city_references(nbts[c]))
            cities = []
            for s in sorted(refs):
                nbt = nbts.get(s) or read_chunk(region, s[0], s[1])
                pieces = city_starts(nbt) if nbt is not None else None
                if pieces is None:
                    raise ValueError("start %s of a city in the grid wasn't saved" % (s,))
                cities.append({"start": list(s), "pieces": pieces})
            footprint = set()
            for city in cities:
                for p in city["pieces"]:
                    x0, _, z0, x1, _, z1 = p[3:]
                    footprint.update((x, z) for x in range(x0, x1 + 1) for z in range(z0, z1 + 1))
            chunks = []
            bases = []
            gateways = []
            for x, z in interior:
                nbt = nbts[(x, z)]
                if nbt is None or nbt.get("Status", "").replace("minecraft:", "") != "full":
                    raise ValueError("chunk %d %d not generated" % (x, z))
                b = chunk_blocks(nbt)
                chunks.append(chunk_row(x, z, chunk_biome(nbt), b, footprint))
                for p, n in b.items():
                    if n == "chorus_plant" and b.get((p[0], p[1] - 1, p[2])) == "end_stone":
                        bases.append(list(p))
                    if n == "end_gateway":
                        gateways.append(list(p))
            out.append(
                {
                    "centre": [cx, cz],
                    "grid": [[cx - CITY_REACH + 1, cz - CITY_REACH + 1], [cx + CITY_REACH - 1, cz + CITY_REACH - 1]],
                    "cities": cities,
                    "chunks": chunks,
                    "chorus_bases": sorted(bases),
                    "gateways": sorted(gateways),
                }
            )
        return out


def extract_cities(server_path, keep=None):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    seeds = {}
    with tempfile.TemporaryDirectory(prefix="voxelcraft-end-cities-jar-") as temporary:
        jar = Path(temporary) / "server.jar"
        jar.write_bytes(data)
        for seed in SEEDS:
            seeds[str(seed)] = extract_city_seed(jar, seed, keep)
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "the pinned dedicated server run on a new world per seed: 'execute in minecraft:the_end "
                "positioned X 64 Z run locate structure minecraft:end_city' from each origin, 'forceload add' "
                "on the chunks within %d of each city's start chunk, 'save-all flush', then the End's region "
                "files read back (chunks, their end_city References and the starts' pieces)" % CITY_REACH,
            ],
            "origins": {str(k): v for k, v in CITY_ORIGINS.items()},
            "format": "per seed, one grid per located city: centre (the located start chunk); grid (the compared "
            "chunks, inclusive); cities: every end_city start whose pieces reach the grid, with its pieces "
            "[template, rotation, overwrite, min x, y, z, max x, y, z] in Java's order; chunks: as "
            "end_decoration.json's rows, then the end stone count and hash and the chorus count and hash of the "
            "columns outside every piece's box; chorus_bases and gateways in the grid",
        },
        "seeds": seeds,
    }


def extract(server_path, keep=None):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    seeds = {}
    with tempfile.TemporaryDirectory(prefix="voxelcraft-end-decoration-jar-") as temporary:
        jar = Path(temporary) / "server.jar"
        jar.write_bytes(data)
        with zipfile.ZipFile(io.BytesIO(data)):
            pass
        for seed in SEEDS:
            seeds[str(seed)] = extract_seed(jar, seed, keep)
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "the pinned dedicated server run on a new world per seed: 'execute in minecraft:the_end run "
                "forceload add' on every chunk listed, 'save-all flush', then the End's region files read back",
            ],
            "format": "chunks: [x, z, biome at the middle (y 64), end stone count, FNV-1a 64 of the end "
            "stone positions 'x y z;' sorted by (x, y, z), chorus block count, FNV-1a 64 of 'x y z name;' "
            "for chorus_plant and chorus_flower:age, any other block but bedrock and end_gateway (an End city)]; chorus_bases: chorus_plant "
            "blocks on end stone in the grid's interior; gateways: end_gateway blocks in the grid's interior "
            "and in gateway_chunks (fully generated chunks where end_gateway_return's rarity filter passes)",
        },
        "seeds": seeds,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--keep", type=Path, help="keep each seed's server folder here")
    parser.add_argument(
        "--cities", action="store_true", help="write the End city grids (end_city_decoration.json) instead"
    )
    args = parser.parse_args()
    run = extract_cities if args.cities else extract
    args.output.write_text(dump(run(args.server, args.keep)))


def dump(data):
    """The fixture's text: one row per line."""
    text = json.dumps(data, indent=1, sort_keys=True)
    flat = re.compile(r"\[\s+([^\[\]{}]*?)\s+\]")
    text = flat.sub(lambda m: "[" + re.sub(r",\s+", ", ", m.group(1)) + "]", text)
    return text + "\n"


if __name__ == "__main__":
    main()
