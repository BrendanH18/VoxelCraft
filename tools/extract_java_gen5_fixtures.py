#!/usr/bin/env python3
"""Extract the generation 5 Overworld facts of v0.16 Track D.

Reads the pinned official Java 26.3 server artifact and writes the data
VoxelCraft's poplar trees, dappled forests, sulfur caves and nether gold
ore follow:

- the three poplar tree features (`orange_poplar`, `red_poplar`,
  `yellow_poplar`) and their `_leaf_litter` variants: trunk and foliage
  placer parameters, decorators and the leaf provider;
- `fallen_poplar_tree`: log length and decorators;
- `trees_dappled_forest`: the weighted selector and its placement count;
- `patch_red_shrub`: count and spread;
- the `dappled_forest` and `sulfur_caves` biomes: temperature, downfall,
  colours, feature lists and spawn settings;
- `sulfur_cave_bands` thresholds and the `sulfur_cave_gradient` noise;
- the `sulfur_spike`, `sulfur_spike_cluster` and `sulfur_pool` placements;
- `ore_gold_nether` and `ore_gold_deltas`.

Facts only: no Mojang template or texture is copied.
"""

import argparse
import hashlib
import io
import json
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

PREFIX = "data/minecraft/worldgen/"


def strip(value):
    """`minecraft:` ids without the namespace, recursively."""
    if isinstance(value, str):
        return value[len("minecraft:") :] if value.startswith("minecraft:") else value
    if isinstance(value, list):
        return [strip(v) for v in value]
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items()}
    return value


def tree_facts(tree):
    return {
        "trunk_placer": tree["trunk_placer"],
        "foliage_placer": tree["foliage_placer"],
        "foliage": tree["foliage_provider"]["id"],
        "trunk": tree["trunk_provider"]["id"],
        "decorators": [
            {k: v for k, v in d.items() if k != "block_state_provider"}
            | ({"litter_states": len(d["block_state_provider"]["entries"])} if "block_state_provider" in d else {})
            for d in tree["decorators"]
        ],
    }


def biome_facts(biome):
    spawns = biome["attributes"]["minecraft:gameplay/natural_mob_spawns"]["argument"]["spawns_by_category"]
    return {
        "temperature": biome["temperature"],
        "downfall": biome["downfall"],
        "effects": biome["effects"],
        "features": biome["features"],
        "spawns": spawns,
    }


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(path):
            return strip(json.loads(jar.read(PREFIX + path)))

        trees = {}
        for colour in ["orange", "red", "yellow"]:
            for suffix in ["", "_leaf_litter"]:
                name = f"{colour}_poplar{suffix}"
                trees[name] = tree_facts(read(f"feature/{name}.json"))
        fallen = read("feature/fallen_poplar_tree.json")
        selector = read("feature/trees_dappled_forest.json")
        placed_trees = read("placed_feature/trees_dappled_forest.json")
        shrub = read("placed_feature/patch_red_shrub.json")
        bands = read("material_rule/overworld/sulfur_cave_bands.json")
        return {
            "reference": {
                "version": "26.3",
                "server_sha1": SERVER_SHA1,
                "sources": [
                    "data/minecraft/worldgen/feature/{orange,red,yellow}_poplar{,_leaf_litter}.json",
                    "data/minecraft/worldgen/feature/fallen_poplar_tree.json",
                    "data/minecraft/worldgen/{feature,placed_feature}/trees_dappled_forest.json",
                    "data/minecraft/worldgen/placed_feature/patch_red_shrub.json",
                    "data/minecraft/worldgen/biome/{dappled_forest,sulfur_caves}.json",
                    "data/minecraft/worldgen/material_rule/overworld/sulfur_cave_bands.json",
                    "data/minecraft/worldgen/noise/sulfur_cave_gradient.json",
                    "data/minecraft/worldgen/placed_feature/{sulfur_spike,sulfur_spike_cluster,sulfur_pool}.json",
                    "data/minecraft/worldgen/placed_feature/ore_gold_{nether,deltas}.json",
                ],
            },
            "poplar_trees": trees,
            "fallen_poplar_tree": {
                "log_length": fallen["log_length"],
                "trunk": fallen["trunk_provider"]["id"],
                "log_decorators": fallen["log_decorators"],
            },
            "trees_dappled_forest": {
                "selector": [{"feature": e["data"], "weight": e["weight"]} for e in selector["features"]],
                "count": next(p["count"] for p in placed_trees["placement"] if p["type"] == "count"),
            },
            "patch_red_shrub": {
                "count": next(p["count"] for p in shrub["placement"] if p["type"] == "count"),
            },
            "biomes": {name: biome_facts(read(f"biome/{name}.json")) for name in ["dappled_forest", "sulfur_caves"]},
            "sulfur_cave_bands": [
                {
                    "block": r["then_run"]["result_state"],
                    "min": r["if_true"]["min_threshold"],
                    "max": r["if_true"]["max_threshold"],
                }
                for r in bands["sequence"]
            ],
            "sulfur_cave_gradient": read("noise/sulfur_cave_gradient.json"),
            "placements": {
                name: read(f"placed_feature/{name}.json")["placement"]
                for name in [
                    "sulfur_spike",
                    "sulfur_spike_cluster",
                    "sulfur_pool",
                    "ore_gold_nether",
                    "ore_gold_deltas",
                ]
            },
            "ore_nether_gold": read("feature/ore_nether_gold.json"),
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
