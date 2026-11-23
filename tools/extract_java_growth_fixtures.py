#!/usr/bin/env python3
"""Extract Java 26.3 random-tick growth constants from the pinned server jar.

Writes `tests/fixtures/java/26.3/growth.json`: the numbers in
`SugarCaneBlock.randomTick`, `CactusBlock.randomTick`, `StemBlock.randomTick`,
`CropBlock.getGrowthSpeed`, `BambooStalkBlock.randomTick` and
`BambooStalkBlock.growBamboo`, and the age properties of sugar cane, cactus
and stems. Each method's numeric constants are read in bytecode order and
must match the expected shape exactly, so a changed method fails here
instead of yielding a wrong value. Never reads VoxelCraft source.

Usage: extract_java_growth_fixtures.py server.jar --output growth.json
(`javap` must be on PATH).
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

BLOCK = "net.minecraft.world.level.block."
PUSH = re.compile(
    r"^\s*\d+: (?:"
    r"(?P<iconst>iconst_(?:m1|[0-5]))"
    r"|(?P<fconst>fconst_[0-2])"
    r"|(?:bipush|sipush)\s+(?P<push>-?\d+)"
    r"|ldc2?(?:_w)?\s+#\d+\s+// (?:int|float|double) (?P<ldc>[-\d.E]+)[df]?"
    r")\s*$"
)


def javap(jar, name):
    return subprocess.check_output(["javap", "-p", "-c", "-classpath", str(jar), BLOCK + name], text=True)


def method(code, header):
    """The bytecode of the method whose declaration line contains `header`."""
    for block in code.split("\n\n"):
        lines = [line for line in block.split("\n") if line.strip()]
        if lines and header in lines[0]:
            return block
    raise ValueError(f"no method {header}")


def numbers(code):
    """The numeric constants a method pushes, in bytecode order."""
    out = []
    for line in code.split("\n"):
        match = PUSH.match(line)
        if not match:
            continue
        if match["iconst"]:
            value = match["iconst"].split("_")[1]
            out.append(-1 if value == "m1" else int(value))
        elif match["fconst"]:
            out.append(float(match["fconst"].split("_")[1]))
        elif match["push"]:
            out.append(int(match["push"]))
        else:
            value = match["ldc"]
            out.append(float(value) if "." in value or "E" in value else int(value))
    return out


def expect(what, got, shape):
    if got != shape:
        raise ValueError(f"reassess {what}: constants {got}, expected {shape}")
    return got


def age_property(code):
    ages = set(
        re.findall(r"Field net/minecraft/world/level/block/state/properties/BlockStateProperties\.AGE_(\d+):", code)
    )
    if len(ages) != 1:
        raise ValueError(f"expected one AGE property, found {sorted(ages)}")
    return int(ages.pop())


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar = Path(directory) / "reference.jar"
        jar.write_bytes(inner)
        cane = javap(jar, "SugarCaneBlock")
        cactus = javap(jar, "CactusBlock")
        stem = javap(jar, "StemBlock")
        crop = javap(jar, "CropBlock")
        bamboo = javap(jar, "BambooStalkBlock")

    # height = 1; while below(height) is cane: height++; if height < 3:
    # age == 15 ? (grow above; age = 0, flags 260) : (age + 1, flags 260)
    c = expect("SugarCaneBlock.randomTick", numbers(method(cane, "void randomTick(")), [1, 3, 15, 0, 260, 1, 260])
    sugar_cane = {
        "age_max": age_property(method(cane, "static {}")),
        "max_height": c[1],
        "grows_at_age": c[2],
        "age_set_block_flags": c[4],
        "checks_water": "SUPPORTS_SUGAR_CANE_ADJACENTLY" in method(cane, "void randomTick("),
    }
    # The flower at age 8 (chance 0.25 in a stalk of 3, else 0.1), a new
    # cactus at age 15 below height 3, then age + 1 below 15.
    c = expect(
        "CactusBlock.randomTick",
        numbers(method(cactus, "void randomTick(")),
        [1, 3, 15, 8, 3, 0.25, 0.1, 15, 3, 0, 260, 0, 15, 1, 260],
    )
    cactus_facts = {
        "age_max": age_property(method(cactus, "static {}")),
        "max_height": c[1],
        "flower_at_age": c[3],
        "flower_chance_full_height": c[5],
        "flower_chance": c[6],
        "grows_at_age": c[7],
        "age_set_block_flags": c[10],
    }
    # getRawBrightness(pos, 0) >= 9; nextInt((int) (25 / speed) + 1) == 0;
    # age < 7 grows, else a fruit beside.
    c = expect("StemBlock.randomTick", numbers(method(stem, "void randomTick(")), [0, 9, 25.0, 1, 7, 1, 2])
    stem_facts = {
        "age_max": age_property(method(stem, "static {}")),
        "min_raw_brightness": c[1],
        "growth_numerator": c[2],
        "fruits_at_age": c[4],
    }
    # f = 1; per soil cell below (offset(x, 0, z)): 1, 3 when moist, a
    # quarter off-centre; halved when crowded (two boolean tests each).
    c = expect(
        "CropBlock.getGrowthSpeed",
        numbers(method(crop, "float getGrowthSpeed(")),
        [1.0, -1, 1, -1, 1, 0.0, 0, 1.0, 0, 3.0, 4.0, 1, 0, 1, 0, 2.0, 1, 0, 2.0],
    )
    growth_speed = {
        "base": c[0],
        "soil": c[7],
        "moist_soil": c[9],
        "neighbour_divisor": c[10],
        "crowded_divisor": c[15],
    }
    c = expect("BambooStalkBlock.randomTick", numbers(method(bamboo, "void randomTick(")), [3, 0, 9, 1, 16])
    g = expect(
        "BambooStalkBlock.growBamboo", numbers(method(bamboo, "void growBamboo(")), [2, 1, 1, 1, 0, 11, 0.25, 15, 1, 0]
    )
    bamboo_facts = {
        "one_in": c[0],
        "min_raw_brightness_above": c[2],
        "max_height": c[4],
        "matures_from_height": g[5],
        "matures_chance": g[6],
        "matures_at_height": g[7],
    }
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.level.block.SugarCaneBlock.randomTick",
            "net.minecraft.world.level.block.CactusBlock.randomTick",
            "net.minecraft.world.level.block.StemBlock.randomTick",
            "net.minecraft.world.level.block.CropBlock.getGrowthSpeed",
            "net.minecraft.world.level.block.BambooStalkBlock.randomTick",
            "net.minecraft.world.level.block.BambooStalkBlock.growBamboo",
        ],
        "sugar_cane": sugar_cane,
        "cactus": cactus_facts,
        "stem": stem_facts,
        "crop_growth_speed": growth_speed,
        "bamboo": bamboo_facts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the pinned Java 26.3 server jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + "\n")


if __name__ == "__main__":
    main()
