#!/usr/bin/env python3
"""Extract v0.16 Track E facts from a verified official Java 26.3 server jar.

Compiles `JavaStructureVectors.java` against the jar and its bundled
libraries and runs it: the jar's own `NetherFortressPieces`,
`StrongholdPieces` and `MineshaftPieces` lay out pieces for fixed seeds and
start chunks (Java's `setLargeFeatureSeed` sequence), and `InfestedBlock`
reports each infested block's host, hardness and blast resistance. From the
jar's data it reads the structure sets (placement of fortresses,
strongholds and mineshafts), the structures' spawn overrides, the
`ore_infested` feature with the biomes that place it, and the biomes that
take each mineshaft type. Needs `javac` and `java` (JDK 25) on `PATH`. Never
reads VoxelCraft source; nothing decompiled or copied is written.
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

HARNESS = Path(__file__).with_name("JavaStructureVectors.java")
DATA = "data/minecraft/"


def read_json(jar, path):
    return json.loads(jar.read(DATA + path))


def expand_biomes(jar, name, stack=()):
    if name in stack:
        raise ValueError("biome tag cycle")
    out = set()
    for value in read_json(jar, f"tags/worldgen/biome/{name}.json")["values"]:
        if isinstance(value, dict):
            value = value["id"]
        if value.startswith("#minecraft:"):
            out |= expand_biomes(jar, value.removeprefix("#minecraft:"), (*stack, name))
        else:
            out.add(value.removeprefix("minecraft:"))
    return out


def run_harness(bundle, inner):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-classic-structures-") as temporary:
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
            ["java", "-cp", f"{classes}:{classpath}", "JavaStructureVectors"],
            check=True,
            capture_output=True,
            text=True,
            cwd=temporary,
        )
    # The server's logger prefixes standard output with a timestamp.
    return [re.sub(r"^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: ", "", line) for line in result.stdout.splitlines()]


def parse(lines):
    infested, layouts = {}, []
    for line in lines:
        if line.startswith("infested "):
            _, name, host, hardness, resistance = line.split()
            infested[name] = {"host": host, "hardness": float(hardness), "blast_resistance": float(resistance)}
        elif line.startswith(("fortress ", "stronghold ", "mineshaft")):
            head, *pieces = line.split(" | ")
            structure, seed, cx, cz = head.split()
            rows = []
            for piece in pieces:
                kind, facing, depth, *box = piece.split()
                rows.append([kind, facing, int(depth), *map(int, box)])
            layouts.append({"structure": structure, "seed": int(seed), "chunk": [int(cx), int(cz)], "pieces": rows})
    if len(infested) != 7 or len(layouts) != 24:
        raise ValueError(f"unexpected harness output: {len(infested)} infested blocks, {len(layouts)} layouts")
    return infested, layouts


def extract(server_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
        infested, layouts = parse(run_harness(bundle, inner))
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        sets = {
            name: read_json(jar, f"worldgen/structure_set/{name}.json")
            for name in ("nether_complexes", "strongholds", "mineshafts")
        }
        overrides = {
            name: read_json(jar, f"worldgen/structure/{name}.json").get("spawn_overrides", {})
            for name in ("fortress", "mineshaft", "mineshaft_mesa", "stronghold")
        }
        feature = read_json(jar, "worldgen/feature/ore_infested.json")
        placed = read_json(jar, "worldgen/placed_feature/ore_infested.json")
        biomes = sorted(
            n.removeprefix(DATA + "worldgen/biome/").removesuffix(".json")
            for n in jar.namelist()
            if n.startswith(DATA + "worldgen/biome/")
            and n.endswith(".json")
            and "minecraft:ore_infested" in json.dumps(json.loads(jar.read(n))["features"])
        )
        overworld = expand_biomes(jar, "is_overworld")
        mineshaft = expand_biomes(jar, "has_structure/mineshaft")
        mesa = expand_biomes(jar, "has_structure/mineshaft_mesa")
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "tools/JavaStructureVectors.java run against the jar: NetherFortressPieces.StartPiece, "
            "StrongholdPieces.StartPiece, MineshaftPieces.MineShaftRoom with StructurePiecesBuilder "
            "(moveInsideHeights, moveBelowSeaLevel), WorldgenRandom.setLargeFeatureSeed; InfestedBlock",
            "data/minecraft/worldgen/structure_set/{nether_complexes,strongholds,mineshafts}.json",
            "data/minecraft/worldgen/structure/{fortress,mineshaft,mineshaft_mesa,stronghold}.json",
            "data/minecraft/worldgen/{feature,placed_feature}/ore_infested.json and worldgen/biome/*.json",
            "data/minecraft/tags/worldgen/biome/{is_overworld,has_structure/mineshaft,has_structure/mineshaft_mesa}.json",
        ],
        "infested": infested,
        "ore_infested": {
            "size": feature["size"],
            "discard_chance_on_air_exposure": feature["discard_chance_on_air_exposure"],
            "targets": [t["state"].removeprefix("minecraft:") for t in feature["targets"]],
            "placement": placed["placement"],
            "biomes": biomes,
        },
        "placements": {name: s["placement"] for name, s in sets.items()},
        "structures": {name: s["structures"] for name, s in sets.items()},
        "spawn_overrides": overrides,
        "mineshaft_biomes": {
            "normal_excludes": sorted(overworld - mineshaft),
            "mesa": sorted(mesa),
        },
        "mesa_surface": 90,
        "layouts": layouts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = extract(args.server)
    # One line per layout keeps the vectors reviewable in a diff.
    layouts = result.pop("layouts")
    text = json.dumps(result, indent=2)
    rows = ",\n".join("    " + json.dumps(layout) for layout in layouts)
    text = text[:-2] + ',\n  "layouts": [\n' + rows + "\n  ]\n}\n"
    json.loads(text)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(text)


if __name__ == "__main__":
    main()
