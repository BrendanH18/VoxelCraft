#!/usr/bin/env python3
"""Extract the End's island shape from a verified official Java 26.3 server jar.

v1.0 gap-closing Track 4 (GEN-04b). Compiles and runs `JavaEndIslands.java`
against the jar, which evaluates Java's own End noise router for three seeds
and prints:

- `EndIslandFunction` (`DensityFunctions.endOuterIslands`) and the router's
  `erosion` (`end/islands`: the main island's distance falloff, max the outer
  islands) on a 31 x 31 grid of points, as raw float bits;
- the noise column `NoiseBasedChunkGenerator.getBaseColumn` generates for
  every column of a few chunks (the central island and the first island
  chunks along eight rays past the void), as solid y ranges.

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

HARNESS = Path(__file__).with_name("JavaEndIslands.java")


def run_harness(bundle, inner):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-end-islands-") as temporary:
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
            ["java", "-cp", f"{classes}:{classpath}", "JavaEndIslands"],
            check=True,
            capture_output=True,
            text=True,
            cwd=temporary,
        )
    # The server's logger prefixes standard output with a timestamp.
    return [re.sub(r"^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: ", "", line) for line in result.stdout.splitlines()]


def parse(lines):
    seeds = {}
    for line in lines:
        parts = line.split()
        if len(parts) != 5 or parts[0] not in ("outer", "islands", "column"):
            continue
        kind, seed, x, z, value = parts
        entry = seeds.setdefault(seed, {"outer": [], "islands": [], "columns": []})
        key = "columns" if kind == "column" else kind
        entry[key].append([int(x), int(z), value])
    if len(seeds) != 3 or any(len(s["outer"]) != 961 or len(s["columns"]) < 512 for s in seeds.values()):
        raise ValueError("unexpected harness output")
    return seeds


def extract(server_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
        seeds = parse(run_harness(bundle, inner))
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "tools/JavaEndIslands.java run against the jar: RandomState.create(noises, seed, "
                "NoiseGeneratorSettings.END).sampleBlockValueUncached(DensityFunctions.endOuterIslands() "
                "and noiseRouter().erosion(), x, 0, z); NoiseBasedChunkGenerator(TheEndBiomeSource, END)"
                ".getBaseColumn(x, z, (min y 0, height 256), randomState)",
            ],
            "format": "outer and islands: [x, z, raw float bits in hex]; columns: [x, z, solid y ranges "
            "'a-b' inclusive, comma-separated, or '-']",
        },
        "seeds": seeds,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    text = json.dumps(extract(args.server), indent=1, sort_keys=True)
    # One sample per line: collapse the innermost [x, z, value] arrays.
    text = re.sub(r"\[\s+(-?\d+),\s+(-?\d+),\s+(\"[^\"]*\")\s+\]", r"[\1, \2, \3]", text)
    args.output.write_text(text + "\n")


if __name__ == "__main__":
    main()
