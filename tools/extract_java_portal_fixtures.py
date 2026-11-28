#!/usr/bin/env python3
"""Extract Nether portal travel facts from a verified official Java 26.3 server jar.

v1.0 gap Track 5 (GEN-04c). Compiles and runs `JavaPortalVectors.java` against
the jar and writes:

- `PortalForcer`'s search radii (`NETHER_PORTAL_RADIUS`, `OVERWORLD_PORTAL_RADIUS`);
- the default `WorldBorder` (every dimension's border is its own saved data
  in 26.3, created from `WorldBorder.Settings.DEFAULT`): its bounds, and
  `isWithinBounds` and `clampToBounds` for sample positions;
- `PortalShape.getRelativePosition` for sample entry rectangles, positions
  and entity sizes;
- `BlockUtil.getLargestRectangleAround` for sample portal shapes;
- each dimension type's `coordinate_scale` and `logical_height`
  (`data/minecraft/dimension_type/*.json`).

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

HARNESS = Path(__file__).with_name("JavaPortalVectors.java")
DIMENSIONS = ["overworld", "the_nether", "the_end"]


def run_harness(bundle, inner):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-portal-") as temporary:
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
            ["java", "-cp", f"{classes}:{classpath}", "JavaPortalVectors"],
            check=True,
            capture_output=True,
            text=True,
            cwd=temporary,
        )
    # The server's logger prefixes standard output with a timestamp.
    lines = []
    for line in result.stdout.splitlines():
        match = re.match(r"^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: (.*)$", line)
        if match:
            lines.append(match.group(1))
    return lines


def parse(lines):
    out = {"constants": {}, "border": None, "within": [], "clamp": [], "relative": [], "rectangles": {}}
    for line in lines:
        kind, *values = line.split()
        if kind == "constant":
            out["constants"][values[0]] = int(values[1])
        elif kind == "border":
            min_x, max_x, min_z, max_z = (float(v) for v in values)
            out["border"] = {"min_x": min_x, "max_x": max_x, "min_z": min_z, "max_z": max_z}
        elif kind == "within":
            out["within"].append({"x": int(values[0]), "z": int(values[1]), "within": values[2] == "true"})
        elif kind == "clamp":
            out["clamp"].append({"at": [float(v) for v in values[:3]], "block": [int(v) for v in values[3:]]})
        elif kind == "relative":
            out["relative"].append(
                {
                    "min_corner": [int(v) for v in values[0:3]],
                    "sizes": [int(values[3]), int(values[4])],
                    "axis": values[5],
                    "position": [float(v) for v in values[6:9]],
                    "dimensions": [float(values[9]), float(values[10])],
                    "relative": [float(v) for v in values[11:14]],
                }
            )
        elif kind == "rectangle":
            out["rectangles"][values[0]] = {
                "min_corner": [int(v) for v in values[1:4]],
                "sizes": [int(values[4]), int(values[5])],
            }
        else:
            raise ValueError(f"unexpected harness line {line!r}")
    if out["border"] is None or len(out["constants"]) != 2 or not out["relative"] or not out["rectangles"]:
        raise ValueError("the harness printed too little")
    return out


def extract(server_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
        facts = parse(run_harness(bundle, inner))
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        dimensions = {}
        for name in DIMENSIONS:
            d = json.loads(jar.read(f"data/minecraft/dimension_type/{name}.json"))
            dimensions[name] = {k: d[k] for k in ("coordinate_scale", "logical_height", "min_y", "height")}
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "tools/JavaPortalVectors.java run against the jar: PortalForcer's radii, new WorldBorder() "
                "(isWithinBounds, clampToBounds), PortalShape.getRelativePosition, "
                "BlockUtil.getLargestRectangleAround",
                "data/minecraft/dimension_type/{overworld,the_nether,the_end}.json",
            ],
        },
        **facts,
        "dimension_types": dimensions,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
