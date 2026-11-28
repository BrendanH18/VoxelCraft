#!/usr/bin/env python3
"""v1.0 final track F3: Java 26.3's random block model variants.

Reads an unpacked Java client jar (its `assets/minecraft/blockstates` and
`models/block`) and writes the fixture of every blockstate entry that lists
more than one model: each variant's weight and, for each world face of the
turned model (the game's order +X, -X, +Y, -Y, +Z, -Z), the symmetry of its
UVs against the game's planar default (`chunk.wgsl`'s `face_uv`), as
`tools/extract_java_model_uvs.py` emulates `FaceBakery`. Java picks a
variant by `ModelBlockRenderer.tesselateBlock`: a `SingleThreadedRandomSource`
seeded with `Mth.getSeed(pos)`, then `WeightedList.getRandomOrThrow`'s
`nextInt(totalWeight)`.

Usage: extract_java_block_variants.py JAR_DIR FIXTURE_JSON
Numbers only are read from the jar; no texture is opened.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_java_model_uvs import SYMMETRIES, Jar, game_uv, quads  # noqa: E402

# Full cubes the terrain mesh draws whose blockstate lists random variants.
CUBES = [
    "bedrock",
    "deepslate",
    "dirt",
    "grass_block",
    "infested_deepslate",
    "infested_stone",
    "mycelium",
    "netherrack",
    "podzol",
    "red_sand",
    "rooted_dirt",
    "sand",
    "sculk",
    "stone",
] + [
    c + "_concrete_powder"
    for c in (
        "white orange magenta light_blue yellow lime pink gray light_gray cyan purple blue brown green red black"
    ).split()
]


def symmetry(gf, verts, uvs):
    """The face's UV symmetry (`SYMMETRIES` index) against the planar default."""
    g = [game_uv(gf, p) for p in verts]
    d1 = (g[1][0] - g[0][0], g[1][1] - g[0][1])
    d3 = (g[3][0] - g[0][0], g[3][1] - g[0][1])
    j1 = (uvs[1][0] - uvs[0][0], uvs[1][1] - uvs[0][1])
    j3 = (uvs[3][0] - uvs[0][0], uvs[3][1] - uvs[0][1])
    det = d1[0] * d3[1] - d1[1] * d3[0]
    a = (j1[0] * d3[1] - j3[0] * d1[1]) / det
    b = (j3[0] * d1[0] - j1[0] * d3[0]) / det
    c = (j1[1] * d3[1] - j3[1] * d1[1]) / det
    d = (j3[1] * d1[0] - j1[1] * d3[0]) / det
    m = tuple(int(round(x)) + 0 for x in (a, b, c, d))
    # A whole face: the offset is whole textures, which tile.
    for k in range(2):
        off = uvs[0][k] - m[2 * k] * g[0][0] - m[2 * k + 1] * g[0][1]
        if off % 16:
            raise ValueError(f"face {gf}: offset {off}")
    return SYMMETRIES.index(m)


def main():
    jar = Jar(sys.argv[1])
    out = {}
    for name in CUBES:
        for key, lst in jar.blockstate(name)["variants"].items():
            if not isinstance(lst, list) or len(lst) < 2:
                continue
            rows = []
            for v in lst:
                faces = [None] * 6
                for gf, lo, hi, verts, uvs in quads(jar, v["model"], v.get("x", 0), v.get("y", 0)):
                    if [hi[a] - lo[a] for a in range(3) if a != gf // 2] != [16, 16]:
                        raise ValueError(f"{name}: not a cube")
                    faces[gf] = symmetry(gf, verts, uvs)
                rows.append(
                    {
                        "model": v["model"],
                        "x": v.get("x", 0),
                        "y": v.get("y", 0),
                        "weight": v.get("weight", 1),
                        "faces": faces,
                    }
                )
            out[f"{name}[{key}]"] = rows
    with open(sys.argv[2], "w") as f:
        json.dump(out, f, indent=1, sort_keys=True)
        f.write("\n")


if __name__ == "__main__":
    main()
