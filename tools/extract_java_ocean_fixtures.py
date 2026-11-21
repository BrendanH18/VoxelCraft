#!/usr/bin/env python3
"""Extract v0.16 Track F's ocean structure and conduit facts from the pinned
Java 26.3 server.

Reads, from the official bundler's inner `server-26.3.jar`:

- every `data/minecraft/structure/shipwreck/*.nbt` template, keeping only its
  size, its data markers (structure blocks in `DATA` mode: the marker name,
  its position and the facing of the chest one block below it), how many
  cells it stores and the block names of its first palette, plus the wood
  each of its eight palettes swaps in. No block layout is copied; VoxelCraft
  draws its own ships (fidelity exception FE-02);
- the `shipwreck`, `shipwreck_beached` and `monument` structures, the
  `shipwrecks` and `ocean_monuments` structure sets and the biome tags they
  name (expanded to biome ids);
- `ShipwreckPieces`' template lists, rotation pivot and size limit and
  `ShipwreckStructure`'s start height (read from the bytecode);
- `OceanMonumentPieces`' and `OceanMonumentStructure`'s constants;
- `ElderGuardian`'s Mining Fatigue constants;
- `ConduitBlockEntity`'s constants and the numbers its tick pushes, the
  `conduit` block's and item's properties, the `conduit_effect_block` tag,
  the conduit recipe and block loot table;
- every structure of the `surface_structures` step in registry order (the
  index `ChunkGenerator.applyBiomeDecoration` seeds each with) and the
  `GenerationStep.Decoration` order.

Requires javap. Each number is read from a named place in the bytecode and an
unrecognized form fails rather than writing a guessed fixture. No VoxelCraft
source or existing fixture is consulted.
"""

import argparse
import hashlib
import io
import json
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_end_fixtures import nbt, strip
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_new_mob_fixtures import constants, method, number, ops

INNER = "META-INF/versions/26.3/server-26.3.jar"
DATA = "data/minecraft/"
PIECES = "world.level.levelgen.structure.structures."
CLASSES = {
    "shipwreck_pieces": PIECES + "ShipwreckPieces",
    "shipwreck_piece": PIECES + "ShipwreckPieces$ShipwreckPiece",
    "shipwreck_structure": PIECES + "ShipwreckStructure",
    "monument_pieces": PIECES + "OceanMonumentPieces$OceanMonumentPiece",
    "monument_building": PIECES + "OceanMonumentPieces$MonumentBuilding",
    "monument_structure": PIECES + "OceanMonumentStructure",
    "elder_guardian": "world.entity.monster.ElderGuardian",
    "conduit": "world.level.block.entity.ConduitBlockEntity",
    "blocks": "world.level.block.Blocks",
    "items": "world.item.Items",
    "decoration": "world.level.levelgen.GenerationStep$Decoration",
}
WOODS = ["birch", "dark_oak", "jungle", "oak", "spruce"]


def expand(jar, tag, seen=None):
    """A biome tag's biome ids, following `#` references."""
    seen = set() if seen is None else seen
    out = []
    for value in strip(json.loads(jar.read(DATA + "tags/worldgen/biome/" + tag + ".json")))["values"]:
        if value.startswith("#"):
            if value not in seen:
                seen.add(value)
                out += expand(jar, strip(value[1:]), seen)
        else:
            out.append(value)
    return sorted(set(out))


def wood(name):
    """The wood of a palette block name (`oak_planks` -> `oak`)."""
    found = [w for w in WOODS if name.startswith(w + "_")]
    if not found:
        raise ValueError("a shipwreck palette block of no known wood: " + name)
    return max(found, key=len)


def shipwreck_template(data):
    """A template's size, markers, stored cells, first palette's block names,
    and per palette the wood replacing each first-palette wood."""
    root = nbt(data)
    palettes = root["palettes"]
    first = palettes[0]
    markers = []
    by_pos = {tuple(b["pos"]): b for b in root["blocks"]}
    for block in root["blocks"]:
        name = strip(first[block["state"]]["id"])
        if name != "structure_block":
            continue
        tag = block.get("nbt", {})
        if tag.get("mode") != "DATA":
            raise ValueError("a shipwreck structure block outside DATA mode")
        x, y, z = block["pos"]
        below = by_pos.get((x, y - 1, z))
        chest = strip(first[below["state"]]) if below else None
        if not chest or chest["id"] != "chest":
            raise ValueError("a shipwreck marker without a chest below it")
        markers.append({"name": tag["metadata"], "pos": block["pos"], "chest_facing": chest["properties"]["facing"]})
    swaps = []
    for palette in palettes:
        swap = {}
        for a, b in zip(first, palette):
            a, b = strip(a), strip(b)
            if a.get("properties") != b.get("properties"):
                raise ValueError("a shipwreck palette changes block properties")
            if a["id"] in ("chest", "structure_block"):
                if a["id"] != b["id"]:
                    raise ValueError("a shipwreck palette replaces a chest or marker")
                continue
            if swap.setdefault(wood(a["id"]), wood(b["id"])) != wood(b["id"]):
                raise ValueError("a shipwreck palette maps one wood to two")
        swaps.append(swap)
    blocks = sorted({strip(first[b["state"]]["id"]) for b in root["blocks"]} - {"structure_block", "air"})
    return {
        "size": root["size"],
        "stored": len(root["blocks"]),
        "markers": sorted(markers, key=lambda m: (m["pos"][1], m["pos"][2], m["pos"][0])),
        "blocks": blocks,
        "entities": len(root.get("entities", [])),
    }, swaps


def template_lists(listing):
    """`ShipwreckPieces.<clinit>`: each `Identifier[]` field's strings in order,
    and the `PIVOT` `BlockPos`."""
    lists, current, pivot = {}, [], None
    for i, op in enumerate(listing):
        found = re.search(r"// String (shipwreck/\w+)", op)
        if found:
            current.append(found[1])
        field = re.search(r"putstatic .*Field (STRUCTURE_LOCATION_\w+):", op)
        if field:
            lists[field[1]] = current
            current = []
        if re.search(r"putstatic .*Field PIVOT:", op):
            pivot = [number(listing[i - 4]), number(listing[i - 3]), number(listing[i - 2])]
    if set(lists) != {"STRUCTURE_LOCATION_BEACHED", "STRUCTURE_LOCATION_OCEAN"} or None in (pivot or [None]):
        raise ValueError("unrecognized ShipwreckPieces.<clinit>")
    return lists, pivot


def pushed(listing, wanted):
    """The constants `listing` pushes, checking each of `wanted` is among them."""
    values = [n for n in (number(op) for op in listing) if n is not None]
    for w in wanted:
        if w not in values:
            raise ValueError(f"expected {w} among {values}")
    return values


def registered(listing, field, holder):
    """The ops from `XIds.FIELD` to the `putstatic FIELD` of a registration."""
    start = next(i for i, op in enumerate(listing) if re.search(rf"{holder}\.{field}:", op))
    end = next(i for i in range(start, len(listing)) if "putstatic" in listing[i] and f"Field {field}:" in listing[i])
    return listing[start:end]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read(INNER)
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-ocean-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        code = {key: inspect(name) for key, name in CLASSES.items()}

    lists, pivot = template_lists(ops(method(code["shipwreck_pieces"], "static {}")))
    if pushed(ops(method(code["shipwreck_structure"], "generatePieces(")), [90]).count(90) != 1:
        raise ValueError("unrecognized ShipwreckStructure.generatePieces")
    pushed(ops(method(code["shipwreck_piece"], "calculateBeachedPosition(")), [2, 3])
    pushed(ops(method(code["monument_structure"], "findGenerationPoint(")), [9, 29])
    pushed(ops(method(code["monument_structure"], "createTopPiece(")), [29])

    conduit = code["conduit"]
    server_tick = ops(method(conduit, "serverTick("))
    effects = ops(method(conduit, "applyEffects("))
    attack = ops(method(conduit, "updateAndAttackTarget("))
    target = ops(method(conduit, "updateDestroyTarget("))
    destroy_box = ops(method(conduit, "getDestroyRangeAABB("))
    pushed(server_tick, [40, 80, 60, 42])
    pushed(effects, [7, 16, 260])
    pushed(attack, [4.0])
    pushed(target, [8.0])
    pushed(destroy_box, [8.0])
    elder = ops(method(code["elder_guardian"], "customServerAiStep("))
    pushed(elder, [1200, 6000, 2, 50.0, 16])

    block = registered(ops(code["blocks"]), "CONDUIT", "BlockItemIds")
    strength = [n for n in (number(op) for op in block) if isinstance(n, float)]
    item = registered(ops(code["items"]), "CONDUIT", "BlockItemIds")
    rarity = next(re.search(r"Rarity\.(\w+):", op)[1].lower() for op in item if "Rarity." in op)

    decoration = [
        m[1].lower()
        for m in re.finditer(r"public static final \S+GenerationStep\$Decoration (\w+);", code["decoration"])
    ]

    out = {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "note": "Shipwreck template sizes, markers, palettes and block names only; no layout is copied (FE-02).",
        "sources": [
            "data/minecraft/structure/shipwreck/*.nbt (size, DATA structure blocks, chests, palettes, block names)",
            "data/minecraft/worldgen/structure/{shipwreck,shipwreck_beached,monument}.json",
            "data/minecraft/worldgen/structure_set/{shipwrecks,ocean_monuments}.json",
            "data/minecraft/worldgen/structure/*.json (step, for the surface_structures order)",
            "data/minecraft/tags/worldgen/biome/{has_structure/*,required_ocean_monument_surrounding}.json",
            "data/minecraft/tags/block/conduit_effect_block.json",
            "data/minecraft/recipe/conduit.json, loot_table/blocks/conduit.json",
            "net.minecraft.world.level.levelgen.structure.structures.ShipwreckPieces, ShipwreckStructure",
            "net.minecraft.world.level.levelgen.structure.structures.OceanMonumentPieces, OceanMonumentStructure",
            "net.minecraft.world.entity.monster.ElderGuardian (customServerAiStep)",
            "net.minecraft.world.level.block.entity.ConduitBlockEntity",
            "net.minecraft.world.level.block.Blocks, net.minecraft.world.item.Items (CONDUIT)",
            "net.minecraft.world.level.levelgen.GenerationStep$Decoration",
        ],
        "shipwreck_pieces": {
            "pivot": pivot,
            "start_y": 90,
            "max_region_size": constants(code["shipwreck_pieces"])["NUMBER_OF_BLOCKS_ALLOWED_IN_WORLD_GEN_REGION"],
            "beached": lists["STRUCTURE_LOCATION_BEACHED"],
            "ocean": lists["STRUCTURE_LOCATION_OCEAN"],
            "beached_drop_bound": 3,
        },
        "monument": {
            "constants": {
                **constants(code["monument_pieces"]),
                **{"building_" + k: v for k, v in constants(code["monument_building"]).items()},
            },
            "start_offset_in_chunk": 9,
            "biome_check_radius": 29,
            "top_piece_offset": -29,
        },
        "elder_guardian": {
            **{k.lower(): v for k, v in constants(code["elder_guardian"]).items() if k.startswith("EFFECT_")},
            "home_radius": 16,
        },
        "conduit": {
            "constants": constants(conduit),
            "shape_interval": 40,
            "ambient_interval": 80,
            "short_ambient": [60, 40],
            "range_divisor": 7,
            "range_multiplier": 16,
            "effect_ticks": 260,
            "attack_damage": 4.0,
            "kill_range": 8.0,
            "block_strength": strength,
            "item_rarity": rarity,
        },
    }
    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(name):
            return strip(json.loads(jar.read(DATA + name + ".json")))

        templates, swaps = {}, None
        for name in sorted(jar.namelist()):
            if name.startswith(DATA + "structure/shipwreck/") and name.endswith(".nbt"):
                facts, palette_swaps = shipwreck_template(jar.read(name))
                templates[Path(name).stem] = facts
                if swaps is None:
                    swaps = palette_swaps
                elif swaps != palette_swaps:
                    raise ValueError("shipwreck templates disagree on their palettes")
        out["shipwreck_templates"] = templates
        out["shipwreck_palettes"] = swaps
        out["structures"] = {s: read("worldgen/structure/" + s) for s in ["shipwreck", "shipwreck_beached", "monument"]}
        out["structure_sets"] = {s: read("worldgen/structure_set/" + s) for s in ["shipwrecks", "ocean_monuments"]}
        out["biome_tags"] = {
            t: expand(jar, t)
            for t in [
                "has_structure/shipwreck",
                "has_structure/shipwreck_beached",
                "has_structure/ocean_monument",
                "required_ocean_monument_surrounding",
            ]
        }
        prefix = DATA + "worldgen/structure/"
        steps = {}
        for name in sorted(jar.namelist()):
            if name.startswith(prefix) and name.endswith(".json"):
                steps[name[len(prefix) : -len(".json")]] = json.loads(jar.read(name))["step"]
        out["generation_steps"] = decoration
        out["surface_structures"] = [s for s in sorted(steps) if steps[s] == "surface_structures"]
        out["conduit_effect_block"] = read("tags/block/conduit_effect_block")["values"]
        out["recipe"] = read("recipe/conduit")
        out["loot_table"] = read("loot_table/blocks/conduit")
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
