#!/usr/bin/env python3
"""Extract every Java 26.3 compostable item and its composter chance.

Items register `Item.Properties.compostable(ContextIntProviders.COMPOSTABLE_*)`
(followed through nested callbacks and colour collections, with each `Items`
field resolved to its registry ID); the chance is the provider's default
weighted roll in `data/minecraft/context_int_provider/compostable/`. Every
provider is checked to always add the first layer to an empty composter.
Never reads VoxelCraft source.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from java_item_associations import provider_ids

FIRST_LAYER = [{"condition": {"type": "minecraft:match_block", "blocks": "minecraft:composter",
                              "state": {"level": "0"}}, "value": 1}]


def chance_percent(provider):
    if provider == 1:
        return 100
    if provider.get("type") != "minecraft:number_dispatcher" or provider.get("cases") != FIRST_LAYER:
        raise ValueError(f"unknown compostable provider form: {provider}")
    weighted = provider["default"]
    if weighted.get("type") != "minecraft:weighted_list" or {e["data"] for e in weighted["distribution"]} != {0, 1}:
        raise ValueError(f"unknown compostable roll: {weighted}")
    total = sum(entry["weight"] for entry in weighted["distribution"])
    hits = sum(entry["weight"] for entry in weighted["distribution"] if entry["data"] == 1)
    if hits * 100 % total:
        raise ValueError("compost chance is not a whole percent")
    return hits * 100 // total


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-compost-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)
        providers = provider_ids(path, "COMPOSTABLE_")
        composter = subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p",
                                             "net.minecraft.world.level.block.ComposterBlock"], text=True)
        add_layer = composter.split(" addLayer(")[1].split("\n\n")[0]
        if ("Compostable.layers:" not in add_layer or "ResolvableInt.get:" not in add_layer
                or "LootContextParams.BLOCK_STATE:" not in add_layer):
            raise ValueError("ComposterBlock.addLayer no longer rolls the item's compostable layers")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def provider(name):
            return json.loads(jar.read(f"data/minecraft/context_int_provider/compostable/{name}.json"))
        compost = [{"item": f"minecraft:{name}", "provider": p, "chance_percent": chance_percent(provider(p))}
                   for name, p in sorted(providers.items())]
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["net.minecraft.world.item.Items compostable registrations, including nested callbacks",
                        "net.minecraft.references.BlockItemIds and ItemIds (registry IDs)",
                        "data/minecraft/context_int_provider/compostable/*.json", "ComposterBlock.addLayer"],
            "empty_composter_always_adds_a_layer": True, "compost": compost}


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
