#!/usr/bin/env python3
"""Extract the sulfur cube facts of v0.16 Track D from the pinned Java 26.3 server.

Covers `EntityTypes.SULFUR_CUBE` (category, size, eye height), the static
constants of `SulfurCube` and `AbstractCubeMob`, the attributes
`createSulfurCubeAttributes` adds, the twelve `sulfur_cube_archetype`
registry entries with their item tags expanded to item names, the
`sulfur_cube_swallowable` and `sulfur_cube_food` item tags, the
`sulfur_cube_with_block_immune_to` damage-type tag (expanded), the
`sulfur_cube_hot` damage type, the `entities/sulfur_cube` loot table, the
sulfur caves' monster entry and the bucket item's class.

Requires javap. Unknown bytecode forms fail rather than regenerating a
guessed fixture. No VoxelCraft source or existing fixture is consulted.
"""

import argparse
import hashlib
import io
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_gen5_fixtures import strip
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_new_mob_fixtures import constants, number, ops


def entity_type(code):
    """`sized`, `eyeHeight`, `spawnDimensionsScale` and the category of the sulfur cube."""
    listing = ops(code)
    start = next(i for i, op in enumerate(listing) if "EntityTypeIds.SULFUR_CUBE:" in op)
    end = next(
        i for i in range(start, len(listing)) if "putstatic" in listing[i] and "Field SULFUR_CUBE:" in listing[i]
    )
    part = listing[start:end]
    category = next(op.split("MobCategory.")[1].split(":")[0] for op in part if "MobCategory." in op)

    def before(call, count):
        i = next(i for i, op in enumerate(part) if call in op)
        values = [number(op) for op in part[i - count : i]]
        if any(v is None for v in values):
            raise ValueError(f"unrecognized {call} arguments")
        return [float(v) for v in values]

    width, height = before("sized:", 2)
    return {
        "category": category.lower(),
        "width": width,
        "height": height,
        "eye_height": before("eyeHeight:", 1)[0],
        "spawn_dimensions_scale": before("spawnDimensionsScale:", 1)[0],
        "fire_immune": any("fireImmune:" in op for op in part),
    }


def attributes(code):
    listing = ops(code[code.index("createSulfurCubeAttributes()") :])
    found = {}
    for i, op in enumerate(listing):
        if op.startswith("areturn"):
            break
        if "Attributes." in op and "Field" in op:
            name = op.split("Attributes.")[1].split(":")[0].lower()
            found[name] = float(number(listing[i + 1]))
    return found


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-sulfur-cube-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        cube = inspect("world.entity.monster.cubemob.SulfurCube")
        consts = {
            "sulfur_cube": constants(cube),
            "abstract_cube_mob": constants(inspect("world.entity.monster.cubemob.AbstractCubeMob")),
        }
        attrs = attributes(cube)
        kind = entity_type(inspect("world.entity.EntityTypes"))
        items = inspect("world.item.Items")
        lambda_start = items.index("EntityTypes.SULFUR_CUBE:")
        bucket_class = items.rfind("// class ", 0, lambda_start)
        bucket = items[bucket_class:].split("// class ")[1].split()[0].split("/")[-1]

    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(name):
            return json.loads(jar.read(name))

        def expand(kind, name):
            out = []
            for value in read(f"data/minecraft/tags/{kind}/{name}.json")["values"]:
                value = value["id"] if isinstance(value, dict) else value
                if value.startswith("#"):
                    out.extend(expand(kind, strip(value[1:])))
                else:
                    out.append(strip(value))
            return out

        archetypes = {}
        prefix = "data/minecraft/sulfur_cube_archetype/"
        for entry in sorted(n for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")):
            name = entry[len(prefix) : -len(".json")]
            arch = strip(read(entry))
            arch["items"] = expand("item", strip(arch["items"][1:]))
            for modifier in arch["attribute_modifiers"]:
                modifier.pop("id")
            archetypes[name] = arch
        loot = read("data/minecraft/loot_table/entities/sulfur_cube.json")
        loot.pop("random_sequence", None)
        biome = read("data/minecraft/worldgen/biome/sulfur_caves.json")
        spawns = biome["attributes"]["minecraft:gameplay/natural_mob_spawns"]["argument"]["spawns_by_category"]
        monster = [strip(e) for e in spawns["monster"] if e["type"] == "minecraft:sulfur_cube"]
        out = {
            "reference": "Minecraft Java Edition 26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "net.minecraft.world.entity.EntityTypes (SULFUR_CUBE)",
                "net.minecraft.world.entity.monster.cubemob.SulfurCube and AbstractCubeMob (constants)",
                "SulfurCube.createSulfurCubeAttributes",
                "net.minecraft.world.item.Items (SULFUR_CUBE_BUCKET)",
                "data/minecraft/sulfur_cube_archetype, tags/item, tags/damage_type, damage_type",
                "data/minecraft/loot_table/entities/sulfur_cube.json",
                "data/minecraft/worldgen/biome/sulfur_caves.json (monster spawns)",
            ],
            "entity_type": kind,
            "attributes": attrs,
            "constants": consts,
            "bucket_item_class": bucket,
            "archetypes": archetypes,
            "tags": {
                "swallowable": expand("item", "sulfur_cube_swallowable"),
                "food": expand("item", "sulfur_cube_food"),
                "immune_with_block": expand("damage_type", "sulfur_cube_with_block_immune_to"),
            },
            "damage_type_hot": read("data/minecraft/damage_type/sulfur_cube_hot.json"),
            "loot_table": loot,
            "sulfur_caves_monster": monster,
        }
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
