#!/usr/bin/env python3
"""Extract v0.16 Track A's outer End facts from the pinned Java 26.3 server.

Reads, from the official bundler's inner `server-26.3.jar`:

- every `data/minecraft/structure/end_city/*.nbt` template, keeping only its
  size, whether every cell of its box is stored, its data markers
  (structure blocks in `DATA` mode: the marker name and its position) and the
  block names it uses. No block layout is copied; VoxelCraft draws its own
  pieces (fidelity exception FE-02);
- the `end_city` structure, the `end_cities` structure set and the
  `has_structure/end_city` biome tag;
- the End biomes' feature lists, the `chorus_plant` placed feature and the
  `supports_chorus_plant` / `supports_chorus_flower` block tags;
- the loot tables `chests/end_city_treasure`, `entities/shulker` and the
  blocks of the track;
- the recipes making purpur, End rods and popped chorus fruit;
- the static constants of `Shulker`, `ShulkerBullet`, `ChorusFlowerBlock`,
  `EndCityPieces` and `EndCityStructure` (read with `javap -constants`), the
  shulker's attributes from `Shulker.createAttributes` and the bullet's
  damage and Levitation from `ShulkerBullet.onHitEntity`.

No VoxelCraft source or existing fixture is consulted.
"""

import argparse
import gzip
import hashlib
import io
import json
import re
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path

SERVER_SHA1 = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c"
INNER = "META-INF/versions/26.3/server-26.3.jar"
DATA = "data/minecraft/"
RECIPES = [
    "purpur_block",
    "purpur_pillar",
    "purpur_slab",
    "purpur_stairs",
    "purpur_pillar_from_purpur_block_stonecutting",
    "purpur_slab_from_purpur_block_stonecutting",
    "purpur_stairs_from_purpur_block_stonecutting",
    "end_rod",
    "popped_chorus_fruit",
]
BLOCK_LOOT = [
    "purpur_block",
    "purpur_pillar",
    "purpur_slab",
    "purpur_stairs",
    "end_rod",
    "chorus_plant",
    "chorus_flower",
]
END_BIOMES = ["the_end", "end_highlands", "end_midlands", "small_end_islands", "end_barrens"]
CLASSES = {
    "shulker": "world.entity.monster.Shulker",
    "shulker_bullet": "world.entity.projectile.ShulkerBullet",
    "chorus_flower": "world.level.block.ChorusFlowerBlock",
    "end_city_pieces": "world.level.levelgen.structure.structures.EndCityPieces",
    "end_city_structure": "world.level.levelgen.structure.structures.EndCityStructure",
    "teleport_randomly": "world.item.consume_effects.TeleportRandomlyConsumeEffect",
}


class Reader:
    """A big-endian NBT reader for the tags structure templates use."""

    def __init__(self, data):
        self.data = data
        self.at = 0

    def take(self, fmt):
        size = struct.calcsize(fmt)
        value = struct.unpack_from(fmt, self.data, self.at)
        self.at += size
        return value[0] if len(value) == 1 else value

    def string(self):
        length = self.take(">H")
        text = self.data[self.at : self.at + length].decode("utf-8")
        self.at += length
        return text

    def payload(self, kind):
        simple = {1: ">b", 2: ">h", 3: ">i", 4: ">q", 5: ">f", 6: ">d"}
        if kind in simple:
            return self.take(simple[kind])
        if kind == 7:
            self.at += self.take(">i")
            return None
        if kind == 8:
            return self.string()
        if kind == 9:
            inner = self.take(">b")
            return [self.payload(inner) for _ in range(self.take(">i"))]
        if kind == 10:
            out = {}
            while True:
                tag = self.take(">b")
                if tag == 0:
                    return out
                name = self.string()
                out[name] = self.payload(tag)
        if kind == 11:
            return [self.take(">i") for _ in range(self.take(">i"))]
        if kind == 12:
            return [self.take(">q") for _ in range(self.take(">i"))]
        raise ValueError("unknown NBT tag " + str(kind))


def nbt(data):
    reader = Reader(gzip.decompress(data))
    if reader.take(">b") != 10:
        raise ValueError("a template must be a compound")
    reader.string()
    return reader.payload(10)


def strip(value):
    """Drop the `minecraft:` namespace from every string in `value`."""
    if isinstance(value, str):
        return value[len("minecraft:") :] if value.startswith("minecraft:") else value
    if isinstance(value, list):
        return [strip(v) for v in value]
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items()}
    return value


def template(data):
    """A template's size, whether it is filled, its markers and block names."""
    root = nbt(data)
    palette = root["palette"] if "palette" in root else root["palettes"][0]
    markers = []
    blocks = set()
    for block in root["blocks"]:
        state = palette[block["state"]]
        name = strip(state.get("id", state.get("Name")))
        tag = block.get("nbt", {})
        if name == "structure_block":
            if tag.get("mode") != "DATA":
                raise ValueError("an End city structure block outside DATA mode")
            markers.append({"name": tag["metadata"], "pos": block["pos"]})
            continue
        if name != "air":
            blocks.add(name)
    size = root["size"]
    return {
        "size": size,
        "filled": len(root["blocks"]) == size[0] * size[1] * size[2],
        "markers": markers,
        "blocks": sorted(blocks),
    }


def constants(code):
    """`static final` primitive constants javap printed with their values."""
    out = {}
    for line in code.splitlines():
        found = re.match(
            r"\s*(?:public |protected |private )?static final (int|float|double|byte|long) (\w+) = ([^;]+);", line
        )
        if found:
            value = found.group(3).rstrip("fdL")
            out[found.group(2)] = float(value) if "." in value else int(value)
    return out


def method(code, name):
    """The bytecode lines of the first method whose signature names `name`."""
    lines = code.splitlines()
    first = next(i for i, line in enumerate(lines) if re.match(r"  \S", line) and name + "(" in line)
    out = []
    for line in lines[first + 1 :]:
        # The next member's declaration, indented two spaces.
        if line.strip() == "" or re.match(r"  \S", line):
            break
        out.append(line)
    return out


def doubles(lines):
    """Double literals pushed by `lines`, in order."""
    return [float(m.group(1)) for m in (re.search(r"// double ([-0-9.Ee]+)d?", line) for line in lines) if m]


def ints(lines):
    """Integer literals pushed by `lines`, in order."""
    out = []
    for line in lines:
        found = re.search(r"(?:bipush|sipush)\s+(-?\d+)|// int (-?\d+)|iconst_(m1|\d)", line)
        if found:
            value = found.group(1) or found.group(2) or found.group(3)
            out.append(-1 if value == "m1" else int(value))
    return out


def floats(lines):
    out = []
    for line in lines:
        found = re.search(r"// float ([-0-9.Ee]+)f|fconst_(\d)", line)
        if found:
            out.append(float(found.group(1) or found.group(2)))
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read(INNER)
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-end-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        code = {key: inspect(name) for key, name in CLASSES.items()}
    shulker = code["shulker"]
    attributes = doubles(method(shulker, "createAttributes"))
    if attributes != [30.0]:
        raise ValueError("unrecognized Shulker.createAttributes: " + str(attributes))
    covered = doubles([line for line in shulker.splitlines() if "double" in line and "20.0" in line])
    if 20.0 not in covered:
        raise ValueError("the covered armor modifier is not 20")
    hit = method(code["shulker_bullet"], "onHitEntity")
    hit_floats, hit_ints = floats(hit), ints(hit)
    if 4.0 not in hit_floats or 200 not in hit_ints:
        raise ValueError("unrecognized ShulkerBullet.onHitEntity: " + str((hit_floats, hit_ints)))
    out = {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "note": "Template sizes, markers and block names only; no layout is copied (FE-02).",
        "sources": [
            "data/minecraft/structure/end_city/*.nbt (size, DATA structure blocks, block names)",
            "data/minecraft/worldgen/structure/end_city.json",
            "data/minecraft/worldgen/structure_set/end_cities.json",
            "data/minecraft/tags/worldgen/biome/has_structure/end_city.json",
            "data/minecraft/worldgen/biome/{the_end,end_highlands,end_midlands,small_end_islands,end_barrens}.json",
            "data/minecraft/worldgen/placed_feature/chorus_plant.json",
            "data/minecraft/tags/block/supports_chorus_{plant,flower}.json",
            "data/minecraft/loot_table/{chests/end_city_treasure,entities/shulker,blocks/*}.json",
            "data/minecraft/recipe/{purpur_*,end_rod,popped_chorus_fruit}.json",
            "net.minecraft.world.entity.monster.Shulker (constants, createAttributes, COVERED_ARMOR_MODIFIER)",
            "net.minecraft.world.entity.projectile.ShulkerBullet (constants, onHitEntity)",
            "net.minecraft.world.level.block.ChorusFlowerBlock, EndCityPieces, EndCityStructure, "
            "TeleportRandomlyConsumeEffect (constants)",
        ],
        "constants": {key: constants(text) for key, text in code.items()},
        "shulker": {"max_health": attributes[0], "covered_armor": 20.0},
        "shulker_bullet_hit": {"damage": 4.0, "levitation_ticks": 200},
    }
    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(name):
            return strip(json.loads(jar.read(DATA + name + ".json")))

        templates = {}
        for name in sorted(jar.namelist()):
            if name.startswith(DATA + "structure/end_city/") and name.endswith(".nbt"):
                templates[Path(name).stem] = template(jar.read(name))
        out["templates"] = templates
        out["structure"] = read("worldgen/structure/end_city")
        out["structure_set"] = read("worldgen/structure_set/end_cities")
        out["biome_tag"] = read("tags/worldgen/biome/has_structure/end_city")["values"]
        out["biome_features"] = {b: read("worldgen/biome/" + b)["features"] for b in END_BIOMES}
        out["chorus_plant_feature"] = read("worldgen/placed_feature/chorus_plant")
        out["supports_chorus_plant"] = read("tags/block/supports_chorus_plant")["values"]
        out["supports_chorus_flower"] = read("tags/block/supports_chorus_flower")["values"]
        out["loot_tables"] = {
            "chests/end_city_treasure": read("loot_table/chests/end_city_treasure"),
            "entities/shulker": read("loot_table/entities/shulker"),
            **{"blocks/" + b: read("loot_table/blocks/" + b) for b in BLOCK_LOOT},
        }
        out["recipes"] = {r: read("recipe/" + r) for r in RECIPES}
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
