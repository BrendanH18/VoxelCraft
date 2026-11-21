#!/usr/bin/env python3
"""Extract the v0.16 M3 exit corpus facts from the pinned Java 26.3 server.

Reads, from the official bundler's inner `server-26.3.jar`, the data every
GEN-05 structure family is placed and filled from:

- every `worldgen/structure_set/*.json`: its placement (random spread,
  concentric rings, frequency and its reduction method, exclusion zone,
  locate offset) and its structures with their weights;
- every `worldgen/structure/*.json`: its type, biome tag, generation step,
  terrain adaptation and spawn overrides (category, bounding box type and
  the mobs it spawns), but not its start pool or template data;
- the placed features of the structures Java builds as features rather than
  structures (`monster_room`, `monster_room_deep`, `fossil_upper`,
  `fossil_lower`, `desert_well`): their placement modifiers, and the
  feature they place;
- the name of every chest loot table (`loot_table/chests/**`, without the
  `chests/` prefix) and of every barrel, dispenser and pot table (with their
  folder), so the corpus can check that each table a generated container
  names exists in Java.

No template, layout or VoxelCraft source is read. The output is
`tests/fixtures/java/26.3/m3_corpus.json`; `src/world/m3_corpus.rs` checks
VoxelCraft's placement and containers against it.
"""

import argparse
import hashlib
import io
import json
import zipfile
from pathlib import Path

from extract_java_end_fixtures import strip
from extract_java_item_fixtures import SERVER_SHA1

INNER = "META-INF/versions/26.3/server-26.3.jar"
DATA = "data/minecraft/"
OTHER_CONTAINERS = ["barrels", "dispensers", "pots"]
FEATURES = ["monster_room", "monster_room_deep", "fossil_upper", "fossil_lower", "desert_well"]


def listed(jar, folder):
    """The `.json` names under `folder`, without the folder or extension."""
    prefix = DATA + folder + "/"
    return sorted(
        n[len(prefix) : -len(".json")] for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")
    )


def structure(raw):
    """A structure's placement-relevant fields."""
    biomes = raw["biomes"]
    if isinstance(biomes, str):
        biomes = biomes.replace("#minecraft:", "#")
    out = {"type": raw["type"], "biomes": biomes, "step": raw["step"]}
    out["terrain_adaptation"] = raw.get("terrain_adaptation", "none")
    out["spawn_overrides"] = {
        category: {
            "bounding_box": o["bounding_box"],
            "spawns": sorted(s["type"] for s in o["spawns"]),
        }
        for category, o in raw.get("spawn_overrides", {}).items()
    }
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("server jar is not the selected official Java 26.3 artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read(INNER)
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:

        def read(path):
            return strip(json.loads(jar.read(DATA + path + ".json")))

        sets = {}
        for name in listed(jar, "worldgen/structure_set"):
            raw = read("worldgen/structure_set/" + name)
            sets[name] = {
                "placement": raw["placement"],
                "structures": [[s["structure"], s["weight"]] for s in raw["structures"]],
            }
        structures = {name: structure(read("worldgen/structure/" + name)) for name in listed(jar, "worldgen/structure")}
        for name, s in sets.items():
            for member, _ in s["structures"]:
                if member not in structures:
                    raise ValueError(f"structure set {name} names an unknown structure {member}")
        features = {}
        for name in FEATURES:
            raw = read("worldgen/placed_feature/" + name)
            features[name] = {"feature": raw["feature"], "placement": raw["placement"]}
        chests = listed(jar, "loot_table/chests")
        if not chests:
            raise ValueError("no chest loot tables")
        others = sorted(
            f"{folder}/{name}" for folder in OTHER_CONTAINERS for name in listed(jar, "loot_table/" + folder)
        )
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "data/minecraft/worldgen/structure_set/*.json",
                "data/minecraft/worldgen/structure/*.json",
                "data/minecraft/worldgen/placed_feature/{" + ",".join(FEATURES) + "}.json",
                "data/minecraft/loot_table/{chests,barrels,dispensers,pots}/**.json (names only)",
            ],
        },
        "structure_sets": sets,
        "structures": structures,
        "placed_features": features,
        "chest_loot_tables": chests,
        "other_container_loot_tables": others,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
