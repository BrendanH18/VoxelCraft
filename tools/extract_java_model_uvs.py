#!/usr/bin/env python3
"""Extract the explicit face UVs of Java's door, trapdoor, glass pane and
glazed terracotta models (v1.0 gap track 6e), and the whole element layout
of the bars, hopper, end rod and lantern models (track 6f) and the
repeater, comparator and cocoa models (v1.0 final F10).

Reads an unpacked Java client jar (its `assets/minecraft/models/block` and
`blockstates`), emulates `FaceBakery`'s vertex order and UV assignment for
each blockstate variant (the model turned by its `x` and `y` without
`uvlock`), and writes:

- `src/mesh/java_uv_tables.rs`: per state and world-space face rectangle,
  the face's UV *op*: which of the square's eight symmetries and which texel
  offset turn the game's planar default UVs into Java's (`src/mesh/java_uv.rs`);
- in the same file, `BARS`, `HOPPERS`, `END_RODS` and `LANTERNS`: every
  face of the bars' multipart model for each set of connections and of the
  hopper's, end rod's and lantern's models for each state (a lantern's
  turned handle left out), with its UV symmetry and texel
  offset, texture, cullface and whether it is inset
  (Java's planes at 0.001 and 15.999 facing into the cell);
- `tests/fixtures/java/26.3/block_model_uvs.json`: every face's world
  rectangle and each vertex's position and Java UV, for the mesh test.

Usage: extract_java_model_uvs.py JAR_DIR TABLES_RS FIXTURE_JSON
Numbers only are read from the jar; no texture is opened.
"""

import json
import os
import sys

# FaceInfo's vertex extents per face (x, y, z): 0 the element's min, 1 its max.
FACE = {
    "down": [(0, 0, 1), (0, 0, 0), (1, 0, 0), (1, 0, 1)],
    "up": [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    "north": [(1, 1, 0), (1, 0, 0), (0, 0, 0), (0, 1, 0)],
    "south": [(0, 1, 1), (0, 0, 1), (1, 0, 1), (1, 1, 1)],
    "west": [(0, 1, 0), (0, 0, 0), (0, 0, 1), (0, 1, 1)],
    "east": [(1, 1, 1), (1, 0, 1), (1, 0, 0), (1, 1, 0)],
}
NORMAL = {
    "down": (0, -1, 0),
    "up": (0, 1, 0),
    "north": (0, 0, -1),
    "south": (0, 0, 1),
    "west": (-1, 0, 0),
    "east": (1, 0, 0),
}
# The game's face order: +X, -X, +Y, -Y, +Z, -Z.
GAME_FACE = {(1, 0, 0): 0, (-1, 0, 0): 1, (0, 1, 0): 2, (0, -1, 0): 3, (0, 0, 1): 4, (0, 0, -1): 5}
# The shader's JAVA_UV_M rows and JAVA_UV_OFFSET texels.
SYMMETRIES = [
    (1, 0, 0, 1),
    (-1, 0, 0, 1),
    (1, 0, 0, -1),
    (-1, 0, 0, -1),
    (0, 1, 1, 0),
    (0, -1, 1, 0),
    (0, 1, -1, 0),
    (0, -1, -1, 0),
]
OFFSETS = [(0, 0), (3, 0), (13, 0), (0, 3), (0, 13), (0, 7), (0, 9), (1, 0)]


class Jar:
    def __init__(self, root):
        self.models = os.path.join(root, "assets/minecraft/models/block")
        self.states = os.path.join(root, "assets/minecraft/blockstates")

    def model(self, name):
        name = name.replace("minecraft:", "").replace("block/", "")
        with open(os.path.join(self.models, name + ".json")) as f:
            return json.load(f)

    def elements(self, name):
        m = self.model(name)
        while "elements" not in m:
            m = self.model(m["parent"])
        return m["elements"]

    def blockstate(self, name):
        with open(os.path.join(self.states, name + ".json")) as f:
            return json.load(f)


def default_uv(face, lo, hi):
    """`FaceBakery.defaultFaceUV`."""
    x0, y0, z0 = lo
    x1, y1, z1 = hi
    return {
        "down": [x0, 16 - z1, x1, 16 - z0],
        "up": [x0, z0, x1, z1],
        "north": [16 - x1, 16 - y1, 16 - x0, 16 - y0],
        "south": [x0, 16 - y1, x1, 16 - y0],
        "west": [z0, 16 - y1, z1, 16 - y0],
        "east": [16 - z1, 16 - y1, 16 - z0, 16 - y0],
    }[face]


def turn(p, xr, yr, size=16):
    """A blockstate's rotation: `x` 90 takes north down, `y` 90 north east."""
    x, y, z = p
    for _ in range(xr // 90):
        y, z = z, size - y
    for _ in range(yr // 90):
        x, z = size - z, x
    return (x, y, z)


def game_uv(face, p):
    """The game's planar default (`chunk.wgsl`'s `face_uv`) in texels."""
    x, y, z = p
    return [(-z, -y), (z, -y), (x, z), (x, -z), (x, -y), (-x, -y)][face]


def quads(jar, model, xr, yr, extra=False):
    """Each face of the turned model: game face, rect, vertices and Java UVs
    (and with `extra`, its texture variable and turned cullface). With
    `extra`, elements turned by their own `rotation` (a lantern's handle)
    are left out: the game's quads can't draw them; so are elements on the
    half-sixteenth grid (the glow planes round a lit repeater's or
    comparator's torches), which a detail quad can't place."""
    out = []
    for e in jar.elements(model):
        if extra and e.get("rotation", {}).get("angle", 0):
            continue
        if extra and any(abs(c * 2 - round(c * 2)) < 1e-6 and c != round(c) for c in e["from"] + e["to"]):
            continue
        lo, hi = e["from"], e["to"]
        for face, fd in e["faces"].items():
            uv = fd.get("uv") or default_uv(face, lo, hi)
            shift = fd.get("rotation", 0) // 90
            verts, uvs = [], []
            for i, ext in enumerate(FACE[face]):
                p = tuple(hi[a] if ext[a] else lo[a] for a in range(3))
                j = (i + shift) % 4
                uvs.append((uv[0] if j in (0, 1) else uv[2], uv[1] if j in (0, 3) else uv[3]))
                verts.append(turn(p, xr, yr))
            gf = GAME_FACE[turn(NORMAL[face], xr, yr, 0)]
            rlo = [min(v[a] for v in verts) for a in range(3)]
            rhi = [max(v[a] for v in verts) for a in range(3)]
            if extra:
                cull = fd.get("cullface")
                cull = GAME_FACE[turn(NORMAL[cull], xr, yr, 0)] if cull else 6
                out.append((gf, rlo, rhi, verts, uvs, fd["texture"].lstrip("#"), cull))
            else:
                out.append((gf, rlo, rhi, verts, uvs))
    return out


def model_rows(jar, family, applied_per_row, textures, lines, fixture, name):
    """v1.0 gap track 6f: a `[&[ModelFace]; N]` table of whole models, one
    row per (label, [(model, x, y)]), each face with its UV symmetry and
    texel offset, texture slot (index in `textures`), turned cullface (6 for
    none) and whether it is one of Java's planes just inside the cell's edge
    facing into it."""
    lines.append(f"pub(super) const {name}: [&[ModelFace]; {len(applied_per_row)}] = [")
    for label, applied in applied_per_row:
        faces = []
        for model, xr, yr in applied:
            faces.extend(quads(jar, model, xr, yr, extra=True))
        fixture.setdefault(family, {})[label] = [
            [gf, lo, hi, [[list(p), list(t)] for p, t in zip(verts, uvs)]] for gf, lo, hi, verts, uvs, _, _ in faces
        ]
        entries = []
        for gf, lo, hi, verts, uvs, tex, cull in faces:
            axis, positive = gf // 2, gf % 2 == 0
            depth = (hi if positive else lo)[axis]
            rlo = [int(round(x)) for x in lo]
            rhi = [int(round(x)) for x in hi]
            if any(abs(a - b) > 0.01 for a, b in zip(lo + hi, rlo + rhi)):
                raise ValueError(f"{family} face {gf} {lo}-{hi} off the grid")
            # A plane just inside the cell's edge facing out is drawn on
            # the edge; facing in, at its inset (depth codes 17 and 18).
            inset = depth != round(depth) and round(depth) != (16 if positive else 0)
            m, (du, dv) = transform(gf, rlo, rhi, verts, uvs)
            if m not in SYMMETRIES:
                raise ValueError(f"{family} face {gf} {lo}-{hi}: {m} isn't a symmetry")
            entry = (gf, rlo, rhi, SYMMETRIES.index(m), du, dv, textures.index(tex), cull, inset)
            if entry not in entries:
                entries.append(entry)
        body = ", ".join(
            f"mf({f}, {lo}, {hi}, {m}, [{du}, {dv}], {t}, {c}, {str(i).lower()})"
            for f, lo, hi, m, du, dv, t, c, i in entries
        )
        lines.append(f"    // {label}\n    &[{body}],")
    lines.append("];")


def transform(gf, lo, hi, verts, uvs):
    """The 2 x 2 map and texel offset taking the game's planar UVs to Java's."""
    g = [game_uv(gf, p) for p in verts]
    d1 = (g[1][0] - g[0][0], g[1][1] - g[0][1])
    d3 = (g[3][0] - g[0][0], g[3][1] - g[0][1])
    j1 = (uvs[1][0] - uvs[0][0], uvs[1][1] - uvs[0][1])
    j3 = (uvs[3][0] - uvs[0][0], uvs[3][1] - uvs[0][1])
    det = d1[0] * d3[1] - d1[1] * d3[0]
    if not det:
        raise ValueError(f"degenerate face {gf} {lo}-{hi}")
    a = (j1[0] * d3[1] - j3[0] * d1[1]) / det
    b = (j3[0] * d1[0] - j1[0] * d3[0]) / det
    c = (j1[1] * d3[1] - j3[1] * d1[1]) / det
    d = (j3[1] * d1[0] - j1[1] * d3[0]) / det
    m = tuple(int(round(x)) + 0 for x in (a, b, c, d))
    off = (
        int(round(uvs[0][0] - a * g[0][0] - b * g[0][1])) % 16,
        int(round(uvs[0][1] - c * g[0][0] - d * g[0][1])) % 16,
    )
    return m, off


def op_of(gf, lo, hi, verts, uvs):
    """The face's op (bits 0-2 symmetry, 3-5 offset, 6 on the cell's edge),
    or None for the default UVs; raises if the game can't store it."""
    m, off = transform(gf, lo, hi, verts, uvs)
    if m == (1, 0, 0, 1) and off == (0, 0):
        return None
    axis = gf // 2
    depth = hi[axis] if gf % 2 == 0 else lo[axis]
    edge = depth == (16 if gf % 2 == 0 else 0)
    if m not in SYMMETRIES or off not in OFFSETS or (not edge and off != (0, 0)):
        raise ValueError(f"face {gf} {lo}-{hi}: {m} {off} can't be stored")
    return SYMMETRIES.index(m) | OFFSETS.index(off) << 3 | (64 if edge else 0)


def main():
    jar = Jar(sys.argv[1])
    lines = []
    fixture = {}

    def state_ops(family, label, v):
        faces = quads(jar, v["model"], v.get("x", 0), v.get("y", 0))
        fixture.setdefault(family, {}).setdefault(label, []).extend(
            [gf, lo, hi, [[list(p), list(t)] for p, t in zip(verts, uvs)]] for gf, lo, hi, verts, uvs in faces
        )
        ops = []
        for gf, lo, hi, verts, uvs in faces:
            o = op_of(gf, lo, hi, verts, uvs)
            if o is not None:
                ops.append((gf, [int(x) for x in lo], [int(x) for x in hi], o))
        return ops

    def emit(name, rows):
        lines.append(f"pub(super) const {name}: [&[Op]; {len(rows)}] = [")
        for label, ops in rows:
            body = ", ".join(f"op({f}, {lo}, {hi}, {o})" for f, lo, hi, o in ops)
            lines.append(f"    // {label}\n    &[{body}],")
        lines.append("];")

    facings = ["south", "north", "east", "west"]
    # Doors: row = facing * 8 + upper * 4 + hinge_right * 2 + open.
    rows = []
    variants = jar.blockstate("oak_door")["variants"]
    for fac in facings:
        for half in ("lower", "upper"):
            for hinge in ("left", "right"):
                for is_open in ("false", "true"):
                    label = f"facing={fac},half={half},hinge={hinge},open={is_open}"
                    v = variants[label]
                    note = f"{label} {v['model'].split('/')[-1]} y{v.get('y', 0)}"
                    rows.append((note, state_ops("doors", label, v)))
    emit("DOORS", rows)
    # Trapdoors: row = facing * 4 + top * 2 + open; oak's template and the
    # orientable one (acacia's).
    for name, state, family in (
        ("TRAPDOORS", "oak_trapdoor", "trapdoors"),
        ("ORIENTABLE_TRAPDOORS", "acacia_trapdoor", "orientable_trapdoors"),
    ):
        rows = []
        variants = jar.blockstate(state)["variants"]
        for fac in facings:
            for half in ("bottom", "top"):
                for is_open in ("false", "true"):
                    label = f"facing={fac},half={half},open={is_open}"
                    v = variants[label]
                    note = f"{label} {v['model'].split('/')[-1]} x{v.get('x', 0)} y{v.get('y', 0)}"
                    rows.append((note, state_ops(family, label, v)))
        emit(name, rows)
    # Panes: one table keyed by rect over every part, and the fixture per
    # connection set.
    parts = jar.blockstate("glass_pane")["multipart"]
    pane = {}
    for bits in range(16):
        sides = {"north": bits & 1, "east": bits >> 1 & 1, "south": bits >> 2 & 1, "west": bits >> 3 & 1}
        label = "".join(k[0] for k in ("north", "east", "south", "west") if sides[k]) or "-"
        for part in parts:
            when = part.get("when")
            if when and not all(str(bool(sides[k])).lower() == want for k, want in when.items()):
                continue
            applied = part["apply"] if isinstance(part["apply"], list) else [part["apply"]]
            for v in applied:
                for f, lo, hi, o in state_ops("panes", label, v):
                    key = (f, tuple(lo), tuple(hi))
                    if pane.get(key, o) != o:
                        raise ValueError(f"pane faces disagree at {key}")
                    pane[key] = o
    lines.append(f"pub(super) const PANES: [Op; {len(pane)}] = [")
    for (f, lo, hi), o in sorted(pane.items()):
        lines.append(f"    op({f}, {list(lo)}, {list(hi)}, {o}),")
    lines.append("];")
    # v1.0 gap track 6f: bars, every face of every applied part, row =
    # north | east << 1 | south << 2 | west << 3 (copper bars' blockstates
    # are iron bars' with their own textures).
    parts = jar.blockstate("iron_bars")["multipart"]
    rows = []
    for bits in range(16):
        sides = {"north": bits & 1, "east": bits >> 1 & 1, "south": bits >> 2 & 1, "west": bits >> 3 & 1}
        label = "".join(k[0] for k in ("north", "east", "south", "west") if sides[k]) or "-"
        applied = []
        for part in parts:
            when = part.get("when")
            if when and not all(str(bool(sides[k])).lower() == want for k, want in when.items()):
                continue
            for v in part["apply"] if isinstance(part["apply"], list) else [part["apply"]]:
                applied.append((v["model"], v.get("x", 0), v.get("y", 0)))
        rows.append((label, applied))
    model_rows(jar, "bars", rows, ["bars", "edge"], lines, fixture, "BARS")
    # Hoppers: row = down, north, south, east, west.
    variants = jar.blockstate("hopper")["variants"]
    rows = []
    for fac in ("down", "north", "south", "east", "west"):
        v = variants[f"facing={fac}"]
        rows.append((f"facing={fac}", [(v["model"], v.get("x", 0), v.get("y", 0))]))
    model_rows(jar, "hoppers", rows, ["side", "top", "inside"], lines, fixture, "HOPPERS")
    # End rods: row = Java's `Direction` order (down, up, north, south,
    # west, east).
    variants = jar.blockstate("end_rod")["variants"]
    rows = []
    for fac in ("down", "up", "north", "south", "west", "east"):
        v = variants[f"facing={fac}"]
        rows.append((f"facing={fac}", [(v["model"], v.get("x", 0), v.get("y", 0))]))
    model_rows(jar, "end_rods", rows, ["end_rod"], lines, fixture, "END_RODS")
    # Lanterns (and soul and copper lanterns, the same templates): row =
    # standing, hanging; the handle's turned planes are left out.
    variants = jar.blockstate("lantern")["variants"]
    rows = []
    for hanging in ("false", "true"):
        v = variants[f"hanging={hanging}"]
        rows.append((f"hanging={hanging}", [(v["model"], v.get("x", 0), v.get("y", 0))]))
    model_rows(jar, "lanterns", rows, ["lantern"], lines, fixture, "LANTERNS")
    # v1.0 final F10: repeaters, row = facing (south, north, east, west) *
    # 16 + (delay - 1) * 4 + locked * 2 + powered; comparators, row = facing * 4 + subtract * 2 + powered; cocoa,
    # row = facing * 3 + age.
    variants = jar.blockstate("repeater")["variants"]
    rows = []
    for fac in facings:
        for delay in range(1, 5):
            for locked in ("false", "true"):
                for powered in ("false", "true"):
                    label = f"delay={delay},facing={fac},locked={locked},powered={powered}"
                    v = variants[label]
                    rows.append((label, [(v["model"], v.get("x", 0), v.get("y", 0))]))
    model_rows(jar, "repeaters", rows, ["slab", "top", "unlit", "lit", "lock"], lines, fixture, "REPEATERS")
    variants = jar.blockstate("comparator")["variants"]
    rows = []
    for fac in facings:
        for mode in ("compare", "subtract"):
            for powered in ("false", "true"):
                label = f"facing={fac},mode={mode},powered={powered}"
                v = variants[label]
                rows.append((label, [(v["model"], v.get("x", 0), v.get("y", 0))]))
    model_rows(jar, "comparators", rows, ["slab", "top", "unlit", "lit"], lines, fixture, "COMPARATORS")
    variants = jar.blockstate("cocoa")["variants"]
    rows = []
    for fac in facings:
        for age in range(3):
            label = f"age={age},facing={fac}"
            v = variants[label]
            rows.append((label, [(v["model"], v.get("x", 0), v.get("y", 0))]))
    model_rows(jar, "cocoa", rows, ["cocoa"], lines, fixture, "COCOA")
    # Glazed terracotta: the fixture only (the game turns its layers).
    for label, v in jar.blockstate("white_glazed_terracotta")["variants"].items():
        state_ops("glazed", label, v)

    header = [
        "// Generated by tools/extract_java_model_uvs.py from the Java 26.3 client jar's",
        "// block models and blockstates (v1.0 gap tracks 6e and 6f, final F10). Do not edit by hand.",
        "// Each entry: a face (0 +X .. 5 -Z), its rect in sixteenths, and its UV op",
        "// (bits 0-2 the transform, 3-5 the offset, bit 6 on the cell's edge);",
        "// a model face has its symmetry, texel offset, texture slot, cullface (6 none) and whether",
        "// it is a plane inset from the cell's edge.",
        "",
    ]
    with open(sys.argv[2], "w") as f:
        f.write("\n".join(header + lines) + "\n")
    with open(sys.argv[3], "w") as f:
        json.dump(fixture, f, separators=(",", ":"))


if __name__ == "__main__":
    main()
