#!/usr/bin/env python3
"""Extract pressure-plate contact sizes from the pinned Java 26.3 server."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar = Path(directory) / "reference.jar"
        jar.write_bytes(inner)
        def inspect(name):
            return subprocess.check_output(["javap", "-c", "-p", "-classpath", str(jar), name], text=True)
        code = inspect("net.minecraft.world.entity.EntityTypes").split("  static {};")[1]
        names = {"ARROW", "EXPERIENCE_ORB", "FISHING_BOBBER", "SMALL_FIREBALL", "FIREBALL", "TNT",
                 "ITEM", "SPLASH_POTION", "EGG", "SNOWBALL", "ENDER_PEARL", "EYE_OF_ENDER"}
        sizes, start = {}, 0
        for match in re.finditer(r"putstatic[^\n]*// Field ([A-Z_]+):", code):
            block = code[start:match.start()]
            start = match.end()
            if match[1] not in names:
                continue
            instructions = block.split("\n")
            index = next(i for i, line in enumerate(instructions) if "EntityType$Builder.sized:" in line)
            def value(line):
                if "fconst_1" in line:
                    return 1.0
                return float(re.search(r"// float ([\d.]+)f", line)[1])
            sizes[match[1].lower()] = [value(line) for line in instructions[index-2:index]]
        if set(sizes) != {name.lower() for name in names}:
            raise ValueError("missing entity registration")
        plate = inspect("net.minecraft.world.level.block.BasePressurePlateBlock")
        initializer = plate.split("  static {};")[1]
        if "double 14.0d" not in initializer or "double 4.0d" not in initializer or "Field TOUCH_AABB" not in initializer:
            raise ValueError("reassess plate touch column")
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["EntityTypes static sized registrations", "BasePressurePlateBlock TOUCH_AABB = Block.column(14, 0, 4)"],
            "touch_min": [1/16, 0, 1/16], "touch_max": [15/16, 4/16, 15/16],
            "entity_sizes": sizes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + "\n")


if __name__ == "__main__":
    main()
