#!/usr/bin/env python3
"""Extract Java 26.3's block and item registry names from the official client.

Every block has a blockstate definition and every item an item model
definition under `assets/minecraft/`, so their file names are the registry
names. Display names come from `assets/minecraft/lang/en_us.json`.
"""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

CLIENT_SHA1 = "e877b6a07acd633fb3bb475002175cec036e7b87"
# Entities whose models are drawn from blockstate files; they are not blocks.
NOT_BLOCKS = {"item_frame", "glow_item_frame"}


def names(jar, folder):
    prefix = f"assets/minecraft/{folder}/"
    return sorted(n[len(prefix):-len(".json")] for n in jar.namelist()
                  if n.startswith(prefix) and n.endswith(".json") and "/" not in n[len(prefix):])


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(client) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
        blocks = [n for n in names(jar, "blockstates") if n not in NOT_BLOCKS]
        items = names(jar, "items")

    def label(kind, name):
        return lang.get(f"{kind}.minecraft.{name}") or lang.get(f"block.minecraft.{name}", "")

    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "source": "assets/minecraft/blockstates/*.json, assets/minecraft/items/*.json, assets/minecraft/lang/en_us.json",
        "blocks": {n: label("block", n) for n in blocks},
        "items": {n: label("item", n) for n in items},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.client), indent=1) + "\n")


if __name__ == "__main__":
    main()
