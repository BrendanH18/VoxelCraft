#!/usr/bin/env python3
"""Extract woodland mansion and explorer map facts from the pinned Java 26.3 server.

Reads, from the official bundler's inner `server-26.3.jar`:

- every `data/minecraft/structure/woodland_mansion/*.nbt` template, keeping
  only its size, its data markers (structure blocks in `DATA` mode: the
  marker name and its position in the template), its containers with fixed
  contents (a spawner's mob, a chest's items), how many empty chests it holds,
  whether every cell of its box is stored, and which room-kind block families
  (bookshelves, wool, crops and so on, see `FAMILIES`) it contains. No block
  layout of any template is copied; VoxelCraft draws its own rooms (fidelity
  exception FE-02);
- the `woodland_mansion` structure and its structure set (registered in code,
  read with `javap`), its start offset and lowest start y, the
  `has_structure/woodland_mansion` biome tag and the `on_*_maps` structure
  tags the explorer maps search;
- the `chests/woodland_mansion` loot table;
- every `villager_trade` offer and loot table entry that gives one of the
  explorer map items, with its `exploration_map` modifier, and the cartographer
  trade tags that list those offers;
- the `clonable_maps` item tag.

The placement and layout constants that live in code
(`WoodlandMansionStructure`, `WoodlandMansionPieces`,
`ExplorationMapFunction`, `MapDecorationTypes`) are cited in
`docs/verification/2026-11-18-v0.15-mansions.md` and checked by the Rust
tests. No VoxelCraft source or existing fixture is consulted.
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
MAPS = [
    "woodland_mansion_map",
    "desert_pyramid_map",
    "warm_ocean_ruins_map",
    "buried_mineshaft_map",
    "buried_ancient_city_map",
]


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
        if kind == 1:
            return self.take(">b")
        if kind == 2:
            return self.take(">h")
        if kind == 3:
            return self.take(">i")
        if kind == 4:
            return self.take(">q")
        if kind == 5:
            return self.take(">f")
        if kind == 6:
            return self.take(">d")
        if kind == 7:
            length = self.take(">i")
            self.at += length
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


# Block families that tell one room kind from another. A template lists the
# families its blocks belong to, so original rooms can be checked against
# Java's room kinds without copying any layout.
FAMILIES = [
    ("anvil", lambda n: n.endswith("anvil")),
    ("banner", lambda n: n.endswith("_banner")),
    ("bookshelf", lambda n: n == "bookshelf"),
    ("carpet", lambda n: n.endswith("_carpet")),
    ("carved_pumpkin", lambda n: n == "carved_pumpkin"),
    ("cauldron", lambda n: n.endswith("cauldron")),
    ("chest", lambda n: n == "chest"),
    ("cobweb", lambda n: n == "cobweb"),
    ("crops", lambda n: n in ("farmland", "wheat")),
    ("diamond_block", lambda n: n == "diamond_block"),
    ("flower_pot", lambda n: n.startswith("potted_")),
    ("glass", lambda n: n in ("glass", "glass_pane")),
    ("infested", lambda n: n.startswith("infested_")),
    ("iron_bars", lambda n: n == "iron_bars"),
    ("iron_door", lambda n: n == "iron_door"),
    ("ladder", lambda n: n == "ladder"),
    ("lapis_block", lambda n: n == "lapis_block"),
    ("lava", lambda n: n == "lava"),
    ("leaves", lambda n: n.endswith("_leaves")),
    ("lever", lambda n: n == "lever"),
    ("mushroom", lambda n: n.endswith("_mushroom")),
    ("obsidian", lambda n: n == "obsidian"),
    ("pumpkin_melon", lambda n: n in ("pumpkin", "melon") or n.endswith("_stem")),
    ("rail", lambda n: n == "rail"),
    ("redstone", lambda n: n == "redstone_wire"),
    ("sapling", lambda n: n.endswith("_sapling")),
    ("spawner", lambda n: n == "spawner"),
    ("tnt", lambda n: n == "tnt"),
    ("torch", lambda n: n.endswith("torch")),
    ("trapped_chest", lambda n: n == "trapped_chest"),
    ("vine", lambda n: n == "vine"),
    ("water", lambda n: n in ("water", "water_cauldron")),
    ("wool", lambda n: n.endswith("_wool")),
]


def contents(tag):
    """A container's fixed items or a spawner's mob, without its other block data."""
    if "SpawnData" in tag:
        keys = (
            "MaxNearbyEntities",
            "RequiredPlayerRange",
            "SpawnCount",
            "SpawnRange",
            "MinSpawnDelay",
            "MaxSpawnDelay",
        )
        out = {k: tag[k] for k in keys if k in tag}
        out["entity"] = strip(tag["SpawnData"]["entity"]["id"])
        return {"spawner": out}
    items = []
    for item in tag.get("Items", []):
        entry = {"slot": item["Slot"], "id": strip(item["id"]), "count": item["count"]}
        enchants = item.get("components", {}).get("minecraft:enchantments")
        if enchants:
            entry["enchantments"] = strip(enchants)
        items.append(entry)
    return {"items": items}


def template(data):
    """A template's size, data markers, fixed containers and room-kind families."""
    root = nbt(data)
    palette = root["palette"] if "palette" in root else root["palettes"][0]
    markers = []
    containers = []
    empty_chests = 0
    families = set()
    for block in root["blocks"]:
        state = palette[block["state"]]
        name = strip(state.get("id", state.get("Name")))
        tag = block.get("nbt", {})
        if name == "structure_block":
            if tag.get("mode") != "DATA":
                raise ValueError("a mansion structure block outside DATA mode")
            markers.append({"pos": block["pos"], "marker": tag["metadata"]})
            continue
        families.update(f for f, test in FAMILIES if test(name))
        if name in ("chest", "trapped_chest", "spawner") and tag:
            found = contents(tag)
            if found.get("items") == []:
                empty_chests += 1
            else:
                containers.append({"pos": block["pos"], "block": name, **found})
    markers.sort(key=lambda m: (m["pos"][1], m["pos"][2], m["pos"][0], m["marker"]))
    containers.sort(key=lambda c: (c["pos"][1], c["pos"][2], c["pos"][0]))
    return {
        "size": root["size"],
        "markers": markers,
        "containers": containers,
        "empty_chests": empty_chests,
        "families": sorted(families),
        "filled": len(root["blocks"]) == root["size"][0] * root["size"][1] * root["size"][2],
    }


def strip(value):
    """Drop the `minecraft:` namespace from every string in `value`."""
    if isinstance(value, str):
        return value[len("minecraft:") :] if value.startswith("minecraft:") else value
    if isinstance(value, list):
        return [strip(v) for v in value]
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items()}
    return value


def map_entries(table):
    """Loot entries in `table` that give one of the explorer maps."""
    out = []
    for pool in table.get("pools", []):
        for entry in pool.get("entries", []):
            name = strip(entry.get("name", ""))
            if name in MAPS:
                out.append(
                    {"item": name, "weight": entry.get("weight", 1), "modifier": strip(entry.get("modifier", []))}
                )
    return out


def block(code, start, end):
    """The bytecode lines from the first line naming `start` to the next naming `end`."""
    lines = code.splitlines()
    first = next(i for i, line in enumerate(lines) if start in line)
    last = next(i for i in range(first + 1, len(lines)) if end in lines[i])
    return lines[first : last + 1]


def ints(lines):
    """Integer literals pushed by `lines`, in order."""
    out = []
    for line in lines:
        found = re.search(r"(?:bipush|sipush)\s+(-?\d+)|// int (-?\d+)|iconst_(m1|\d)", line)
        if found:
            value = found.group(1) or found.group(2) or found.group(3)
            out.append(-1 if value == "m1" else int(value))
    return out


def structure_set(code):
    """`StructureSets.bootstrap`'s woodland mansion placement."""
    lines = block(code, "BuiltinStructureSets.WOODLAND_MANSIONS", "BootstrapContext.register")
    if not any("BuiltinStructures.WOODLAND_MANSION:" in line for line in lines):
        raise ValueError("the woodland mansion set no longer holds the woodland mansion")
    if not any('RandomSpreadStructurePlacement."<init>":(IILnet' in line for line in lines):
        raise ValueError("unrecognized woodland mansion placement")
    spread = [re.search(r"RandomSpreadType\.(\w+)", line) for line in lines]
    spread = [m.group(1).lower() for m in spread if m]
    values = ints(lines)
    if len(values) != 3 or len(spread) != 1:
        raise ValueError("unrecognized woodland mansion placement: " + str(values))
    return {
        "type": "random_spread",
        "spacing": values[0],
        "separation": values[1],
        "spread_type": spread[0],
        "salt": values[2],
        "frequency": 1.0,
        "exclusion_zone": None,
        "locate_offset": [0, 0, 0],
    }


def structure(code, settings):
    """`Structures.bootstrap`'s woodland mansion and its default settings."""
    lines = block(code, "BuiltinStructures.WOODLAND_MANSION:", "BootstrapContext.register")
    if not any("BiomeTags.HAS_WOODLAND_MANSION" in line for line in lines):
        raise ValueError("the woodland mansion no longer uses has_structure/woodland_mansion")
    if not any('StructureSettings."<init>":(Lnet/minecraft/core/HolderSet;)V' in line for line in lines):
        raise ValueError("the woodland mansion no longer takes the default structure settings")
    default = block(settings, "static {}", "putstatic")
    step = [re.search(r"Decoration\.(\w+)", line) for line in default]
    adapt = [re.search(r"TerrainAdjustment\.(\w+)", line) for line in default]
    step = [m.group(1).lower() for m in step if m]
    adapt = [m.group(1).lower() for m in adapt if m]
    if len(step) != 1 or len(adapt) != 1 or not any("java/util/Map.of:()" in line for line in default):
        raise ValueError("unrecognized default structure settings")
    return {
        "type": "woodland_mansion",
        "biomes": "#has_structure/woodland_mansion",
        "spawn_overrides": {},
        "step": step[0],
        "terrain_adaptation": adapt[0],
    }


def generation_point(code):
    """`WoodlandMansionStructure`'s start offset in its chunk and lowest start y."""
    offset = int(re.search(r"START_OFFSET_IN_CHUNK = (\d+);", code).group(1))
    lines = block(code, "findGenerationPoint(", "generatePieces(")
    values = ints(lines)
    if values[:2] != [offset, offset] or 60 not in values:
        raise ValueError("unrecognized findGenerationPoint: " + str(values))
    return {"start_offset_in_chunk": offset, "min_start_y": 60}


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read(INNER)
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-mansions-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        code = {
            "set": structure_set(inspect("data.worldgen.StructureSets")),
            "structure": structure(
                inspect("data.worldgen.Structures"),
                inspect("world.level.levelgen.structure.Structure$StructureSettings"),
            ),
            "start": generation_point(inspect("world.level.levelgen.structure.structures.WoodlandMansionStructure")),
        }
    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(path):
            return json.loads(jar.read(DATA + path))

        names = sorted(n for n in jar.namelist() if n.startswith(DATA + "structure/woodland_mansion/"))
        templates = {}
        for name in names:
            if name.endswith(".nbt"):
                templates[Path(name).stem] = template(jar.read(name))
        trades = {}
        trade_tags = {}
        loot_sources = {}
        for name in sorted(jar.namelist()):
            if not name.endswith(".json"):
                continue
            path = name[len(DATA) :] if name.startswith(DATA) else None
            if path is None:
                continue
            if path.startswith("villager_trade/"):
                offer = json.loads(jar.read(name))
                if strip(offer["gives"]["id"]) in MAPS:
                    trades[path[len("villager_trade/") : -len(".json")]] = strip(offer)
            elif path.startswith("tags/villager_trade/cartographer/"):
                trade_tags[Path(path).stem] = strip(json.loads(jar.read(name))["values"])
            elif path.startswith("loot_table/"):
                entries = map_entries(json.loads(jar.read(name)))
                if entries:
                    loot_sources[path[len("loot_table/") : -len(".json")]] = entries
        structure_tags = {}
        for tag in ("woodland_mansion", "desert_pyramid", "ocean_ruin_warm", "mineshaft", "ancient_city"):
            structure_tags["on_" + tag + "_maps"] = strip(read("tags/worldgen/structure/on_" + tag + "_maps.json"))
        out = {
            "reference": "Minecraft Java Edition 26.3",
            "server_sha1": SERVER_SHA1,
            "note": "Template sizes, markers, fixed containers and room-kind families only; no layout is copied (FE-02).",
            "sources": [
                "data/minecraft/structure/woodland_mansion/*.nbt (size, DATA structure blocks)",
                "net.minecraft.data.worldgen.Structures.bootstrap (woodland_mansion)",
                "net.minecraft.data.worldgen.StructureSets.bootstrap (woodland_mansions)",
                "net.minecraft.world.level.levelgen.structure.Structure$StructureSettings.DEFAULT",
                "net.minecraft.world.level.levelgen.structure.structures.WoodlandMansionStructure.findGenerationPoint",
                "data/minecraft/tags/worldgen/biome/has_structure/woodland_mansion.json",
                "data/minecraft/tags/worldgen/structure/on_*_maps.json",
                "data/minecraft/loot_table/chests/woodland_mansion.json",
                "data/minecraft/villager_trade/**, data/minecraft/tags/villager_trade/cartographer/*",
                "data/minecraft/loot_table/** (entries giving explorer maps)",
                "data/minecraft/tags/item/clonable_maps.json",
            ],
            "structure": code["structure"],
            "structure_set": code["set"],
            "generation_point": code["start"],
            "biome_tag": strip(read("tags/worldgen/biome/has_structure/woodland_mansion.json")["values"]),
            "structure_tags": {k: v["values"] for k, v in structure_tags.items()},
            "loot_table": strip(read("loot_table/chests/woodland_mansion.json")),
            "templates": templates,
            "maps": MAPS,
            "map_trades": trades,
            "cartographer_trade_tags": trade_tags,
            "map_loot_sources": loot_sources,
            "clonable_maps": strip(read("tags/item/clonable_maps.json")["values"]),
        }
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
