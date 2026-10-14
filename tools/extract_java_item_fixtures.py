#!/usr/bin/env python3
"""Normalize the first Java 26.3 item fixtures from a verified official server jar."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile


SERVER_SHA1 = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c"
SOURCE = f"https://piston-data.mojang.com/v1/objects/{SERVER_SHA1}/server.jar"
# These associations were checked against Items' cookingFuel registrations.
# Derive durations from the artifact, never from VoxelCraft's implementation.
FUELS = [
    ("bamboo", "bamboo"),
    ("bamboo_block", "wood_blocks"),
    ("bamboo_planks", "wood_blocks"),
    ("bamboo_mosaic", "wood_blocks"),
    ("dried_kelp_block", "dried_kelp_block"),
    ("oak_planks", "wood_blocks"),
    ("pale_oak_planks", "wood_blocks"),
    ("oak_slab", "wood_slabs"),
    ("oak_stairs", "wood_blocks"),
    ("oak_door", "wood_items_large"),
    ("ladder", "wood_blocks"),
    ("coal", "coal"),
    ("charcoal", "coal"),
    ("blaze_rod", "blaze_rod"),
    ("lava_bucket", "lava_bucket"),
]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("server jar is not the selected official Java 26.3 artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(path):
            return json.loads(jar.read(path))

        def ticks(provider):
            return read(f"data/minecraft/context_int_provider/cooking/time_{provider}.json")["left"]

        normal = read("data/minecraft/context_int_provider/cooking/normal_burn_time_reduction_factor.json")
        fast = read("data/minecraft/context_int_provider/cooking/fast_burn_time_reduction_factor.json")
        if (normal, fast) != (1, 2):
            raise ValueError("unexpected furnace fuel reduction factors")
        fixture = {
            "reference": "Minecraft Java Edition 26.3",
            "source": SOURCE,
            "server_sha1": SERVER_SHA1,
            "fuel_sources": [
                "net.minecraft.world.item.Items (CookingFuel registrations)",
                "data/minecraft/context_int_provider/cooking/time_*.json",
            ],
            "fuel_cases": [{"item": f"minecraft:{item}", "ticks": ticks(provider)} for item, provider in FUELS]
            + [{"item": f"minecraft:{name}", "ticks": None} for name in ("crimson_planks", "warped_planks", "stone")],
            "brewing": [],
            "pending_recipes": [],
        }
        for name in ("potion_awkward_pufferfish", "splash_potion_awkward_pufferfish"):
            path = f"data/minecraft/recipe/brewing/{name}.json"
            recipe = read(path)
            fixture["brewing"].append({
                "path": path,
                "bottle": recipe["input"]["item"],
                "input": recipe["input"]["potion_contents"]["potions"],
                "ingredient": recipe["reagent"]["item"],
                "output": recipe["output"]["components"]["minecraft:potion_contents"]["potion"],
            })
        for name in ("lead", "saddle"):
            path = f"data/minecraft/recipe/{name}.json"
            recipe = read(path)
            fixture["pending_recipes"].append({
                "path": path,
                "pattern": recipe["pattern"],
                "ingredients": recipe["key"],
                "result": recipe["result"]["id"],
                "count": recipe["result"].get("count", 1),
            })
    return fixture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        fixture = extract(args.server)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
