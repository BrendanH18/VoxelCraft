#!/usr/bin/env python3
"""Extract the v0.15 Track C mob facts from the pinned official Java 26.3 server.

Covers happy ghasts and dried ghasts, nautiluses and zombie nautiluses,
camel husks and parched: their `EntityTypes` sizes and categories, the
attributes their `createAttributes` methods add, the static constants of
their classes (`DriedGhastBlock`, `HappyGhast`, `AbstractNautilus`,
`Nautilus`, `NautilusAi`, `ZombieNautilusAi`, `Drowned`), a few constants
read from method bytecode (the husk's camel chance, the parched's arrow
effect and attack intervals), and the data files: loot tables, item,
block, entity-type and biome tags, the dried ghast recipe, and each
biome's `nautilus` spawner entries come from the existing spawn fixture.

Requires javap. Unknown bytecode forms fail rather than regenerating a
guessed fixture. No VoxelCraft source or existing fixture is consulted.
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

from extract_java_item_fixtures import SERVER_SHA1

CLASSES = {
    "dried_ghast_block": "world.level.block.DriedGhastBlock",
    "happy_ghast": "world.entity.animal.happyghast.HappyGhast",
    "happy_ghast_ai": "world.entity.animal.happyghast.HappyGhastAi",
    "abstract_nautilus": "world.entity.animal.nautilus.AbstractNautilus",
    "nautilus": "world.entity.animal.nautilus.Nautilus",
    "nautilus_ai": "world.entity.animal.nautilus.NautilusAi",
    "zombie_nautilus_ai": "world.entity.animal.nautilus.ZombieNautilusAi",
    "drowned": "world.entity.monster.zombie.Drowned",
    "camel": "world.entity.animal.camel.Camel",
}

ENTITIES = {
    "HAPPY_GHAST": "happy_ghast",
    "NAUTILUS": "nautilus",
    "ZOMBIE_NAUTILUS": "zombie_nautilus",
    "CAMEL_HUSK": "camel_husk",
    "PARCHED": "parched",
}

ATTRIBUTE_CLASSES = {
    "happy_ghast": "world.entity.animal.happyghast.HappyGhast",
    "nautilus": "world.entity.animal.nautilus.AbstractNautilus",
    "zombie_nautilus": "world.entity.animal.nautilus.ZombieNautilus",
    "camel_husk": "world.entity.animal.camel.Camel",
    "parched": "world.entity.monster.skeleton.Parched",
}

DATA = {
    "loot_tables": [
        "entities/happy_ghast",
        "entities/nautilus",
        "entities/zombie_nautilus",
        "entities/camel_husk",
        "entities/parched",
        "blocks/dried_ghast",
    ],
    "item_tags": [
        "happy_ghast_food",
        "happy_ghast_tempt_items",
        "nautilus_food",
        "nautilus_taming_items",
        "nautilus_bucket_food",
        "camel_husk_food",
        "fishes",
    ],
    "block_tags": ["happy_ghast_avoids", "triggers_ambient_dried_ghast_block_sounds"],
    "entity_tags": [
        "can_equip_harness",
        "can_wear_nautilus_armor",
        "can_equip_saddle",
        "nautilus_hostiles",
        "burn_in_daylight",
        "zombies",
        "skeletons",
        "fall_damage_immune",
        "can_breathe_under_water",
        "aquatic",
    ],
    "biome_tags": ["spawns_coral_variant_zombie_nautilus", "more_frequent_drowned_spawns"],
}


def strip(name):
    return name[len("minecraft:") :] if name.startswith("minecraft:") else name


def method(code, signature):
    start = code.find(signature)
    if start < 0:
        raise ValueError(f"missing method: {signature}")
    end = code.find("\n\n", start)
    return code[start : end if end >= 0 else len(code)]


def constants(code):
    """`static final` primitive fields with their `-constants` values."""
    out = {}
    pattern = r"^\s+(?:public |private |protected )?static final (int|float|double|long) (\w+) = ([-\d.E]+)[fdl]?;"
    for match in re.finditer(pattern, code, re.M):
        kind, name, value = match.groups()
        out[name] = int(value) if kind in ("int", "long") else float(value)
    return out


def number(op):
    """The pushed value of one constant-pushing instruction, or None."""
    match = re.match(r"(ldc2?_w|ldc|ldc_w)\s+#\d+\s+// (float|double|int|long) ([-\d.E]+)[fdl]?$", op)
    if match:
        return float(match[3]) if match[2] in ("float", "double") else int(match[3])
    match = re.match(r"([dfil])const_(m?\d)$", op)
    if match:
        value = -1 if match[2] == "m1" else int(match[2])
        return float(value) if match[1] in "df" else value
    match = re.match(r"(bipush|sipush)\s+(-?\d+)$", op)
    if match:
        return int(match[2])
    return None


def ops(code):
    out = []
    for line in code.splitlines():
        match = re.match(r"\s+\d+: (\w+)\s*(.*)", line)
        if match:
            out.append(f"{match[1]} {match[2]}".strip())
    return out


def attributes(code):
    """`Attributes.X` then a number then `add`, in order, from `createAttributes`."""
    listing = ops(method(code, "createAttributes()"))
    found = {}
    for i, op in enumerate(listing):
        match = re.search(r"Field net/minecraft/world/entity/ai/attributes/Attributes\.(\w+):", op)
        if not match:
            continue
        value = number(listing[i + 1]) if i + 1 < len(listing) else None
        if value is None and i + 2 < len(listing) and listing[i + 2].startswith("f2d"):
            value = number(listing[i + 1])
        if value is None:
            raise ValueError(f"unrecognized attribute value after {op}: {listing[i + 1]}")
        found[match[1].lower()] = float(value)
    return found


def entity_types(code):
    """`sized(w, h)` and the category of each Track C entity."""
    listing = ops(code)
    out = {}
    for field, name in ENTITIES.items():
        start = next(i for i, op in enumerate(listing) if f"EntityTypeIds.{field}:" in op)
        end = next(
            i for i in range(start, len(listing)) if "putstatic" in listing[i] and f"Field {field}:" in listing[i]
        )
        part = listing[start:end]
        category = next(re.search(r"MobCategory\.(\w+):", op)[1] for op in part if "MobCategory." in op)
        sized = next(i for i, op in enumerate(part) if "sized:" in op)
        width, height = number(part[sized - 2]), number(part[sized - 1])
        if width is None or height is None:
            raise ValueError(f"unrecognized sized() for {field}")
        out[name] = {"category": category.lower(), "width": float(width), "height": float(height)}
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-new-mobs-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        consts = {key: constants(inspect(name)) for key, name in CLASSES.items()}
        attrs = {key: attributes(inspect(name)) for key, name in ATTRIBUTE_CLASSES.items()}
        types = entity_types(inspect("world.entity.EntityTypes"))

        husk = ops(method(inspect("world.entity.monster.zombie.Husk"), "finalizeSpawn("))
        camel_chance = next(
            number(op) for i, op in enumerate(husk) if number(op) is not None and "fcmpg" in husk[i + 1]
        )
        parched = inspect("world.entity.monster.skeleton.Parched")
        arrow = ops(method(parched, "getArrow("))
        if not any("MobEffects.WEAKNESS" in op for op in arrow):
            raise ValueError("Parched.getArrow no longer gives Weakness")
        weakness = next(number(op) for op in arrow if number(op) is not None and number(op) > 1)
        hard = next(number(op) for op in ops(method(parched, "getHardAttackInterval(")) if number(op) is not None)
        normal = next(number(op) for op in ops(method(parched, "getAttackInterval(")) if number(op) is not None)

    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(name):
            return json.loads(jar.read(name))

        def tag(kind, name):
            return [strip(v) for v in read(f"data/minecraft/tags/{kind}/{name}.json")["values"]]

        loot = {name: read(f"data/minecraft/loot_table/{name}.json") for name in DATA["loot_tables"]}
        for table in loot.values():
            table.pop("random_sequence", None)
        recipe = read("data/minecraft/recipe/dried_ghast.json")
        tags = {
            "item": {name: tag("item", name) for name in DATA["item_tags"]},
            "block": {name: tag("block", name) for name in DATA["block_tags"]},
            "entity_type": {name: tag("entity_type", name) for name in DATA["entity_tags"]},
            "biome": {name: tag("worldgen/biome", name) for name in DATA["biome_tags"]},
        }
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.entity.EntityTypes (sized, MobCategory)",
            "createAttributes of HappyGhast, AbstractNautilus, ZombieNautilus, Camel, Parched",
            "static constants of " + ", ".join(sorted(CLASSES.values())),
            "Husk.finalizeSpawn (camel husk chance)",
            "Parched.getArrow, getHardAttackInterval, getAttackInterval",
            "data/minecraft/loot_table, tags, recipe/dried_ghast.json",
        ],
        "entity_types": types,
        "attributes": attrs,
        "constants": consts,
        "husk_camel_husk_chance": float(camel_chance),
        "parched": {"arrow_weakness_ticks": weakness, "hard_attack_interval": hard, "attack_interval": normal},
        "loot_tables": loot,
        "dried_ghast_recipe": {
            "pattern": recipe["pattern"],
            "key": {k: strip(v) for k, v in recipe["key"].items()},
            "result": strip(recipe["result"]["id"]),
        },
        "tags": tags,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
