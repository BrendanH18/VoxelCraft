#!/usr/bin/env python3
"""Extract the Nether's generation facts from a verified official Java 26.3 server jar.

v1.0 Track 8 (GEN-04a). Writes, for the five Nether biomes:

- the `nether` multi-noise preset's parameter points, read by compiling and
  running `JavaNetherVectors.java` against the jar (the preset is code, not
  data), as Java's quantized ranges (`Climate.quantizeCoord`: value x 10000);
- each biome's feature list by generation step (`worldgen/biome/*.json`);
- every placed feature those lists name, and the configured feature each
  one places, as the jar's JSON (`worldgen/placed_feature`, `worldgen/feature`);
- the Nether structure sets and structures (`nether_complexes`,
  `nether_fossils`; `fortress`, `bastion_remnant`, `nether_fossil`,
  `ruined_portal_nether`) and the biomes their `has_structure` tags hold;
- the block tags the placements and features test (`nylium`,
  `base_stone_nether`, `overrides_mushroom_light_requirement`,
  `cannot_place_basalt_pillar_on`);
- v1.0 gap Track 5: the Nether's surface rules as 26.3 holds them
  (`worldgen/material_rule/nether.json` and the rules and conditions it
  names, `worldgen/noise_settings/nether.json` without its noise router) and
  the noises those rules read (`worldgen/noise`).

Needs `javac` and `java` (JDK 25) on `PATH`. Never reads VoxelCraft source;
nothing decompiled is written.
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

HARNESS = Path(__file__).with_name("JavaNetherVectors.java")
DATA = "data/minecraft/"
BIOMES = ["nether_wastes", "soul_sand_valley", "crimson_forest", "warped_forest", "basalt_deltas"]
STRUCTURE_SETS = ["nether_complexes", "nether_fossils"]
STRUCTURES = ["fortress", "bastion_remnant", "nether_fossil", "ruined_portal_nether"]
BLOCK_TAGS = ["nylium", "base_stone_nether", "overrides_mushroom_light_requirement", "cannot_place_basalt_pillar_on"]
SURFACE_NOISES = [
    "surface",
    "soul_sand_layer",
    "gravel_layer",
    "patch",
    "netherrack",
    "nether_wart",
    "nether_state_selector",
]


def material_rules(jar):
    """The Nether's material rule and every rule and condition it names."""
    rules = {"nether": read_json(jar, "worldgen/material_rule/nether.json")}
    conditions = {}
    pending = ["nether"]
    while pending:
        rule = rules[pending.pop()]
        for name in sorted(refs(rule)):
            if jar_has(jar, f"worldgen/material_rule/{name}.json"):
                if name not in rules:
                    rules[name] = read_json(jar, f"worldgen/material_rule/{name}.json")
                    pending.append(name)
            elif jar_has(jar, f"worldgen/material_condition/{name}.json"):
                conditions[name] = read_json(jar, f"worldgen/material_condition/{name}.json")
    settings = read_json(jar, "worldgen/noise_settings/nether.json")
    settings.pop("noise_router", None)
    settings.pop("debug_functions", None)
    return rules, conditions, settings


def refs(value):
    """Bare `minecraft:` strings in a rule (named rules and conditions); block
    states, biomes and noises sit under their own keys."""
    out = set()
    if isinstance(value, str):
        if value.startswith("minecraft:"):
            out.add(strip(value))
    elif isinstance(value, dict):
        for key, inner in value.items():
            if key in ("type", "result_state", "biome_is", "noise", "random_name"):
                continue
            out |= refs(inner)
    elif isinstance(value, list):
        for inner in value:
            out |= refs(inner)
    return out


def jar_has(jar, path):
    try:
        jar.getinfo(DATA + path)
        return True
    except KeyError:
        return False


def read_json(jar, path):
    return json.loads(jar.read(DATA + path))


def strip(name):
    return name.removeprefix("minecraft:")


def expand_tag(jar, kind, name, stack=()):
    if name in stack:
        raise ValueError("tag cycle")
    out = set()
    for value in read_json(jar, f"tags/{kind}/{name}.json")["values"]:
        if isinstance(value, dict):
            value = value["id"]
        if value.startswith("#minecraft:"):
            out |= set(expand_tag(jar, kind, value.removeprefix("#minecraft:"), (*stack, name)))
        else:
            out.add(strip(value))
    return sorted(out)


def run_harness(bundle, inner):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-nether-") as temporary:
        root = Path(temporary)
        server = root / "server.jar"
        server.write_bytes(inner)
        libraries = []
        for name in bundle.namelist():
            if name.startswith("META-INF/libraries/") and name.endswith(".jar"):
                target = root / "libraries" / Path(name).name
                target.parent.mkdir(exist_ok=True)
                target.write_bytes(bundle.read(name))
                libraries.append(str(target))
        classpath = ":".join([str(server), *libraries])
        classes = root / "classes"
        classes.mkdir()
        subprocess.run(
            ["javac", "-nowarn", "-Xlint:-deprecation", "-cp", classpath, "-d", str(classes), str(HARNESS)], check=True
        )
        result = subprocess.run(
            ["java", "-cp", f"{classes}:{classpath}", "JavaNetherVectors"],
            check=True,
            capture_output=True,
            text=True,
            cwd=temporary,
        )
    # The server's logger prefixes standard output with a timestamp.
    return [re.sub(r"^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: ", "", line) for line in result.stdout.splitlines()]


def parse_points(lines):
    names = ["temperature", "humidity", "continentalness", "erosion", "depth", "weirdness"]
    points = {}
    for line in lines:
        if not line.startswith("point "):
            continue
        _, biome, *values = line.split()
        ranges = [int(v) for v in values[:-1]]
        point = {n: [ranges[2 * i], ranges[2 * i + 1]] for i, n in enumerate(names)}
        point["offset"] = int(values[-1])
        points[biome] = point
    if sorted(points) != sorted(BIOMES):
        raise ValueError(f"unexpected preset biomes {sorted(points)}")
    return points


def extract(server_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
        points = parse_points(run_harness(bundle, inner))
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        biomes = {}
        placed_names = set()
        for b in BIOMES:
            steps = [[strip(f) for f in step] for step in read_json(jar, f"worldgen/biome/{b}.json")["features"]]
            biomes[b] = {"features": steps}
            placed_names |= {f for step in steps for f in step}
        placed = {n: read_json(jar, f"worldgen/placed_feature/{n}.json") for n in sorted(placed_names)}
        features = {}
        for p in placed.values():
            name = p["feature"]
            if isinstance(name, str):
                features[strip(name)] = read_json(jar, f"worldgen/feature/{strip(name)}.json")
        structure_sets = {n: read_json(jar, f"worldgen/structure_set/{n}.json") for n in STRUCTURE_SETS}
        structures = {n: read_json(jar, f"worldgen/structure/{n}.json") for n in STRUCTURES}
        structure_biomes = {}
        for s in structures.values():
            tag = s["biomes"].removeprefix("#minecraft:")
            structure_biomes[tag] = expand_tag(jar, "worldgen/biome", tag)
        block_tags = {t: expand_tag(jar, "block", t) for t in BLOCK_TAGS}
        rules, conditions, settings = material_rules(jar)
        noises = {n: read_json(jar, f"worldgen/noise/{n}.json") for n in SURFACE_NOISES}
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "tools/JavaNetherVectors.java run against the jar: MultiNoiseBiomeSourceParameterList.Preset.NETHER",
                "data/minecraft/worldgen/biome/{nether_wastes,soul_sand_valley,crimson_forest,warped_forest,"
                "basalt_deltas}.json (features by step)",
                "data/minecraft/worldgen/placed_feature/*.json and worldgen/feature/*.json (those the biomes name)",
                "data/minecraft/worldgen/structure_set/{nether_complexes,nether_fossils}.json",
                "data/minecraft/worldgen/structure/{fortress,bastion_remnant,nether_fossil,ruined_portal_nether}.json",
                "data/minecraft/tags/worldgen/biome/has_structure/*.json and tags/block/*.json",
                "data/minecraft/worldgen/material_rule/{nether,bedrock_floor,bedrock_roof}.json, "
                "worldgen/material_condition/*.json (those they name), worldgen/noise_settings/nether.json "
                "(without its noise router) and worldgen/noise/*.json (the surface rules' noises)",
            ],
        },
        "parameter_points": points,
        "biomes": biomes,
        "placed_features": placed,
        "features": dict(sorted(features.items())),
        "structure_sets": structure_sets,
        "structures": structures,
        "structure_biomes": structure_biomes,
        "block_tags": block_tags,
        "material_rules": rules,
        "material_conditions": conditions,
        "noise_settings": settings,
        "surface_noises": noises,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
