#!/usr/bin/env python3
"""Extract the complete registered cooking-fuel inventory from Java 26.3.

Follows nested Items property callbacks and expands colored collections.
The exception list also checks implemented lookalike materials that are not fuels.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from java_item_associations import COLORS, provider_ids


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-fuels-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)
        providers = provider_ids(path, "COOKING_TIME_")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(path):
            return json.loads(jar.read(path))
        if (read("data/minecraft/context_int_provider/cooking/normal_burn_time_reduction_factor.json"),
                read("data/minecraft/context_int_provider/cooking/fast_burn_time_reduction_factor.json")) != (1, 2):
            raise ValueError("cooking reduction factors changed")
        fuels = [{"item": f"minecraft:{name}", "provider": provider,
                  "ticks": read(f"data/minecraft/context_int_provider/cooking/time_{provider}.json")["left"]}
                 for name, provider in sorted(providers.items())]
    exceptions = [f"{wood}_{form}" for wood in ("crimson", "warped")
                  for form in ("planks", "stairs", "slab", "fence", "fence_gate", "door", "trapdoor", "button",
                               "pressure_plate", "stem", "hyphae", "sign", "hanging_sign")]
    exceptions += [f"stripped_{wood}_{form}" for wood in ("crimson", "warped") for form in ("stem", "hyphae")]
    exceptions += [f"{color}_bed" for color in COLORS]
    exceptions += ["stone", "oak_leaves", "spruce_leaves", "moss_carpet", "pale_moss_carpet", "muddy_mangrove_roots"]
    if any(name in providers for name in exceptions):
        raise ValueError("a reviewed non-fuel exception now burns; reassess")
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["net.minecraft.world.item.Items cookingFuel registrations, including nested callbacks",
                        "data/minecraft/context_int_provider/cooking/time_*.json"],
            "fuels": fuels, "non_fuels": sorted(f"minecraft:{name}" for name in exceptions)}


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
