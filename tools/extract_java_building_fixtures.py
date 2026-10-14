#!/usr/bin/env python3
"""Extract bytecode-checked Java 26.3 building constants from the pinned server.

No Java implementation or assets are copied. Temporary files stay in this
checkout's ignored target directory. Tests consume the normalized facts.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import math
import struct
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
    target = Path(__file__).resolve().parent.parent / "target/java-reference"
    target.mkdir(parents=True, exist_ok=True)
    sources = []
    with tempfile.TemporaryDirectory(dir=target) as temporary:
        jar = Path(temporary) / "reference.jar"
        jar.write_bytes(inner)

        def inspect(name):
            sources.append(name)
            return subprocess.check_output(["javap", "-classpath", str(jar), "-c", "-p", name], text=True)

        heights = {}
        for block, cls in (("soul_sand", "SoulSandBlock"), ("mud", "MudBlock")):
            code = inspect("net.minecraft.world.level.block." + cls)
            static = code.split("  static {};")[1]
            if "Block.column:(DDD)" not in static or "Field SHAPE:" not in code.split(" getCollisionShape(")[1].split("\n\n")[0]:
                raise ValueError("collision construction changed")
            values = re.findall(r"// double ([0-9.]+)d", static)
            if len(values) != 2 or float(values[0]) != 16:
                raise ValueError("collision dimensions changed")
            heights[block] = float(values[1]) / 16
        hay = inspect("net.minecraft.world.level.block.HayBlock").split(" fallOn(")[1].split("\n\n")[0]
        if "Entity.causeFallDamage:(DF" not in hay:
            raise ValueError("hay fall call changed")
        multiplier = float(re.findall(r"// float ([0-9.]+)f", hay)[0])
        living = inspect("net.minecraft.world.entity.LivingEntity")
        fall = living.split(" calculateFallDamage(")[1].split("\n\n")[0]
        power = living.split(" calculateFallPower(")[1].split("\n\n")[0]
        if "Mth.floor:(D)I" not in fall or "f2d" not in fall or "double 1.0E-6d" not in power:
            raise ValueError("fall rounding changed")
        as_float = struct.unpack("f", struct.pack("f", multiplier))[0]
        cases = [{"distance": distance, "damage": max(0, math.floor((distance + 1e-6 - 3) * as_float))}
                 for distance in [3, 3.99, 7.99, 8, 20, 23, 23.5]]
        ore = inspect("net.minecraft.world.level.block.RedStoneOreBlock")
        tick = ore.split(" randomTick(")[1].split("\n\n")[0]
        if "iconst_0" not in tick or "BlockState.setValue:" not in tick or "ServerLevel.setBlockAndUpdate:" not in tick:
            raise ValueError("ore random tick changed")
        mushroom = inspect("net.minecraft.world.level.block.MushroomBlock").split(" canSurvive(")[1].split("\n\n")[0]
        if "LevelReader.getRawBrightness:" not in mushroom or not re.search(r"iconst_0\s+\d+: invokeinterface.*getRawBrightness", mushroom):
            raise ValueError("mushroom raw brightness call changed")
        threshold = int(re.findall(r"bipush\s+(\d+)", mushroom)[0])
        with zipfile.ZipFile(io.BytesIO(inner)) as z:
            tag_path = "data/minecraft/tags/block/overrides_mushroom_light_requirement.json"
            overrides = json.loads(z.read(tag_path))["values"]
        sources.append(tag_path)
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": sources, "collision_height": heights, "hay_fall_multiplier": multiplier,
            "hay_fall_cases": cases,
            "redstone_ore_extinguishes_on_random_tick": True,
            "mushroom_brightness_limit": threshold,
            "mushroom_light_overrides": [v.removeprefix("minecraft:") for v in overrides]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(extract(args.server), indent=2) + "\n")
