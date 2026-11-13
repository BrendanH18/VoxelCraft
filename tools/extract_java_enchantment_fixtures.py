#!/usr/bin/env python3
"""Extract Java 26.3's enchantment definitions and the tags they name.

Reads every `data/minecraft/enchantment/*.json` from the pinned server jar
(`Enchantment.DIRECT_CODEC`: the definition's levels, weight, costs, anvil
cost, slots and item sets, the exclusive set and the effect components),
expands each `#minecraft:enchantable/*` item tag it names (and every other
enchantable tag) into registry ids, and expands every enchantment tag in
`data/minecraft/tags/enchantment/` in Java's iteration order (entries in file
order, nested tags inline, first occurrence kept). English names come from the
pinned client's en_us.json. Unknown keys or entry forms fail. Never reads
VoxelCraft source or an existing fixture.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_registry_names import CLIENT_SHA1

KEYS = {"anvil_cost", "description", "effects", "exclusive_set", "max_cost", "max_level", "min_cost",
        "primary_items", "slots", "supported_items", "weight"}
REQUIRED = KEYS - {"effects", "exclusive_set", "primary_items"}
SLOTS = {"any", "hand", "mainhand", "offhand", "armor", "feet", "legs", "chest", "head", "body", "saddle"}
ENCHANTMENT_DIR = "data/minecraft/enchantment/"
ITEM_TAG_DIR = "data/minecraft/tags/item/"
ENCHANTMENT_TAG_DIR = "data/minecraft/tags/enchantment/"


def tag_name(value, what):
    if not isinstance(value, str) or not value.startswith("#minecraft:"):
        raise ValueError(f"{what}: expected a #minecraft: tag, got {value!r}")
    return value.removeprefix("#minecraft:")


def expand_items(jar, name, stack=()):
    """Sorted item ids in `#minecraft:<name>`."""
    if name in stack:
        raise ValueError("item tag cycle")
    tag = json.loads(jar.read(f"{ITEM_TAG_DIR}{name}.json"))
    if set(tag) - {"values", "replace"} or tag.get("replace", False):
        raise ValueError(f"item tag {name}: unknown form")
    values = set()
    for value in tag["values"]:
        if not isinstance(value, str):
            raise ValueError(f"item tag {name}: unknown entry form")
        if value.startswith("#minecraft:"):
            values |= set(expand_items(jar, value.removeprefix("#minecraft:"), (*stack, name)))
        elif value.startswith("minecraft:"):
            values.add(value.removeprefix("minecraft:"))
        else:
            raise ValueError(f"item tag {name}: unexpected namespace")
    return sorted(values)


def expand_enchantments(jar, name, known, stack=()):
    """Enchantment ids in `#minecraft:<name>`, in the order a HolderSet iterates them."""
    if name in stack:
        raise ValueError("enchantment tag cycle")
    tag = json.loads(jar.read(f"{ENCHANTMENT_TAG_DIR}{name}.json"))
    if set(tag) - {"values", "replace"} or tag.get("replace", False):
        raise ValueError(f"enchantment tag {name}: unknown form")
    out = []
    for value in tag["values"]:
        if not isinstance(value, str):
            raise ValueError(f"enchantment tag {name}: unknown entry form")
        if value.startswith("#minecraft:"):
            members = expand_enchantments(jar, value.removeprefix("#minecraft:"), known, (*stack, name))
        elif value.startswith("minecraft:"):
            members = [value.removeprefix("minecraft:")]
        else:
            raise ValueError(f"enchantment tag {name}: unexpected namespace")
        for member in members:
            if member not in known:
                raise ValueError(f"enchantment tag {name}: unknown enchantment {member}")
            if member not in out:
                out.append(member)
    return out


def cost(value, what):
    if set(value) != {"base", "per_level_above_first"} or not all(isinstance(v, int) for v in value.values()):
        raise ValueError(f"{what}: unknown cost form {value}")
    return {"base": value["base"], "per_level_above_first": value["per_level_above_first"]}


def extract(server, client):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    client_data = client.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(io.BytesIO(client_data)) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        files = sorted(n for n in jar.namelist() if n.startswith(ENCHANTMENT_DIR) and n.endswith(".json"))
        if any("/" in n[len(ENCHANTMENT_DIR):] for n in files):
            raise ValueError("nested enchantment definition folder")
        known = [n[len(ENCHANTMENT_DIR):-len(".json")] for n in files]
        enchantments = []
        for name, path in zip(known, files):
            d = json.loads(jar.read(path))
            if set(d) - KEYS or REQUIRED - set(d):
                raise ValueError(f"{name}: unknown or missing keys {sorted(set(d) ^ KEYS)}")
            key = f"enchantment.minecraft.{name}"
            if d["description"] != {"translate": key} or key not in lang:
                raise ValueError(f"{name}: unknown description form {d['description']}")
            if not d["slots"] or set(d["slots"]) - SLOTS:
                raise ValueError(f"{name}: unknown slots {d['slots']}")
            for field in ("max_level", "weight", "anvil_cost"):
                if not isinstance(d[field], int) or d[field] < 1:
                    raise ValueError(f"{name}: bad {field}")
            exclusive = d.get("exclusive_set")
            enchantments.append({
                "id": f"minecraft:{name}",
                "name": lang[key],
                "max_level": d["max_level"],
                "weight": d["weight"],
                "min_cost": cost(d["min_cost"], name),
                "max_cost": cost(d["max_cost"], name),
                "anvil_cost": d["anvil_cost"],
                "slots": d["slots"],
                "supported_items": tag_name(d["supported_items"], f"{name}.supported_items"),
                "primary_items": tag_name(d["primary_items"], name) if "primary_items" in d else None,
                "exclusive_set": tag_name(exclusive, name) if exclusive is not None else None,
                "effects": sorted(d.get("effects", {})),
            })
        item_tags = sorted({n[len(ITEM_TAG_DIR):-len(".json")] for n in jar.namelist()
                            if n.startswith(ITEM_TAG_DIR + "enchantable/") and n.endswith(".json")})
        for e in enchantments:
            for field in ("supported_items", "primary_items"):
                if e[field] is not None and e[field] not in item_tags:
                    raise ValueError(f"{e['id']}: {field} names a non-enchantable tag {e[field]}")
        enchantment_tags = sorted(n[len(ENCHANTMENT_TAG_DIR):-len(".json")] for n in jar.namelist()
                                  if n.startswith(ENCHANTMENT_TAG_DIR) and n.endswith(".json"))
        for e in enchantments:
            if e["exclusive_set"] is not None and e["exclusive_set"] not in enchantment_tags:
                raise ValueError(f"{e['id']}: exclusive_set names an unknown tag")
        items = {name: [f"minecraft:{i}" for i in expand_items(jar, name)] for name in item_tags}
        tags = {name: [f"minecraft:{e}" for e in expand_enchantments(jar, name, known)] for name in enchantment_tags}
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "client_sha1": CLIENT_SHA1,
        "sources": [f"{ENCHANTMENT_DIR}*.json ({len(enchantments)})",
                    f"{ITEM_TAG_DIR}enchantable/*.json ({len(items)}) and the item tags they include",
                    f"{ENCHANTMENT_TAG_DIR}**/*.json ({len(tags)})",
                    "assets/minecraft/lang/en_us.json enchantment.minecraft.* (client)",
                    "net.minecraft.world.item.enchantment.Enchantment (DIRECT_CODEC, isPrimaryItem, areCompatible)"],
        "enchantments": enchantments,
        "item_tags": items,
        "enchantment_tags": tags,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (the bundler)")
    parser.add_argument("--client", type=Path, required=True, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = extract(args.server, args.client)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2) + "\n")


if __name__ == "__main__":
    main()
