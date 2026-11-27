#!/usr/bin/env python3
"""Extract v1.0 Track 6's data facts from the pinned official Java 26.3 artifact.

Reads the item tags the farm animals and companions eat, breed with and
follow (`cow_food` to `camel_food`, with `#meat` expanded into `wolf_food`),
the farm animals' entity loot tables (each item with its `set_count` range)
and the `spawn_overrides` bounding-box kinds and lists of swamp huts,
monuments, outposts, ancient cities and trial chambers.

Only data files are read; the Java constants the tests also check
(taming odds, follow and leash distances, the witch's heal goal, bells and
celebrations) come from the decompiled classes named in
`docs/verification/2026-11-25-v1.0-track-06-companions-villages.md`. No
VoxelCraft source or existing fixture is consulted.
"""

import argparse
import hashlib
import io
import json
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

TAGS = [
    "cow_food",
    "pig_food",
    "sheep_food",
    "chicken_food",
    "rabbit_food",
    "goat_food",
    "horse_food",
    "horse_tempt_items",
    "llama_food",
    "llama_tempt_items",
    "camel_food",
    "wolf_food",
    "cat_food",
    "parrot_food",
    "parrot_poisonous_food",
]
LOOT = ["cow", "pig", "sheep", "chicken", "rabbit"]
STRUCTURES = ["swamp_hut", "monument", "pillager_outpost", "ancient_city", "trial_chambers"]


def strip(name):
    return name.removeprefix("minecraft:")


def tag(jar, name):
    """The tag's items, nested tags expanded, in file order."""
    out = []
    for value in json.loads(jar.read(f"data/minecraft/tags/item/{name}.json"))["values"]:
        if value.startswith("#"):
            out.extend(tag(jar, strip(value[1:])))
        else:
            out.append(strip(value))
    return out


def count_range(entry):
    """The entry's `set_count` modifier as `[min, max]` (1 without one)."""
    for function in entry.get("modifier", []):
        if strip(function["type"]) != "set_count":
            continue
        count = function["count"]
        if isinstance(count, (int, float)):
            return [count, count]
        if strip(count.get("type", "")) != "uniform":
            raise ValueError(f"unexpected count {count}")
        return [count["min"], count["max"]]
    return [1, 1]


def loot(jar, name):
    """Every item entry of `entities/<name>` with its count range."""
    table = json.loads(jar.read(f"data/minecraft/loot_table/entities/{name}.json"))
    out = []
    for pool in table.get("pools", []):
        for entry in pool["entries"]:
            if strip(entry["type"]) == "item":
                out.append({"item": strip(entry["name"]), "count": count_range(entry)})
    return out


def overrides(jar, name):
    data = json.loads(jar.read(f"data/minecraft/worldgen/structure/{name}.json"))
    out = {}
    for category, rule in sorted(data["spawn_overrides"].items()):
        out[category] = {
            "bounding_box": rule["bounding_box"],
            "spawns": [strip(s["type"]) for s in rule["spawns"]],
        }
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("server jar is not the selected official Java 26.3 artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        return {
            "source": {
                "server_sha1": SERVER_SHA1,
                "read": "server-26.3.jar data/minecraft: tags/item, loot_table/entities, worldgen/structure",
            },
            "tags": {name: tag(jar, name) for name in TAGS},
            "loot": {name: loot(jar, name) for name in LOOT},
            "spawn_overrides": {name: overrides(jar, name) for name in STRUCTURES},
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = extract(args.server)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2) + "\n")


if __name__ == "__main__":
    main()
