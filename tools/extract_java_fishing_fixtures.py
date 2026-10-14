#!/usr/bin/env python3
"""Extract fishing loot facts from the pinned official Java 26.3 artifact.

Requires javap. Reads `data/minecraft/loot_table/gameplay/fishing*.json` and
the `#minecraft:on_random_loot` enchantment tag, and checks the bytecode forms
the fixture's semantics rely on (`LootPool.addRandomItem` keeps only positive
weights; `SetItemDamageFunction` stores `floor((1 - clamp(value)) * max)`).
Unknown loot entry types, keys, conditions or functions fail rather than
regenerating a guessed fixture. No VoxelCraft source or existing fixture is
consulted.
"""
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

ROOT = "data/minecraft/loot_table/gameplay/"
TABLES = ("junk", "treasure", "fish")
ENTRY_KEYS = {"type", "name", "value", "weight", "quality", "condition", "modifier"}


def method(code, signature):
    parts = code.split("  " + signature)
    if len(parts) != 2:
        raise ValueError(f"missing reference method: {signature}")
    return parts[1].split("\n\n")[0]


def single_pool(table, path):
    if set(table) - {"type", "pools", "random_sequence"} or table.get("type") != "minecraft:fishing":
        raise ValueError(f"unrecognized loot table shape: {path}")
    if len(table["pools"]) != 1:
        raise ValueError(f"expected one pool: {path}")
    pool = table["pools"][0]
    if set(pool) != {"entries", "rolls"} or pool["rolls"] != 1:
        raise ValueError(f"unrecognized pool (rolls, bonus rolls, conditions or functions): {path}")
    return pool["entries"]


def condition(entry, path):
    raw = entry.get("condition")
    if raw is None:
        return None
    if raw == {"type": "minecraft:entity_properties", "entity": "this",
               "predicate": {"minecraft:type_specific/fishing_hook": {"in_open_water": True}}}:
        return "in_open_water"
    if raw.get("type") == "minecraft:location_check" and set(raw) == {"type", "predicate"} \
            and set(raw["predicate"]) == {"biomes"} and isinstance(raw["predicate"]["biomes"], list):
        return {"biomes": raw["predicate"]["biomes"]}
    raise ValueError(f"unrecognized condition in {path}: {raw}")


def modifiers(entry, path):
    raw = entry.get("modifier", [])
    raw = raw if isinstance(raw, list) else [raw]
    out = {"count": 1, "damage": None, "enchant_with_levels": None, "potion": None}
    for function in raw:
        kind = function.get("type")
        if kind == "minecraft:set_count" and set(function) == {"type", "count"} and isinstance(function["count"], int):
            out["count"] = function["count"]
        elif kind == "minecraft:set_damage" and set(function) == {"type", "damage"}:
            damage = function["damage"]
            if set(damage) != {"type", "min", "max"} or damage["type"] != "minecraft:uniform":
                raise ValueError(f"unrecognized damage provider in {path}: {damage}")
            out["damage"] = {"min": damage["min"], "max": damage["max"]}
        elif kind == "minecraft:enchant_with_levels" and set(function) == {"type", "levels", "options"} \
                and isinstance(function["levels"], int) and function["options"] == "#minecraft:on_random_loot":
            if out["damage"] is None and entry["name"] != "minecraft:book":
                raise ValueError(f"unexpected enchant without damage in {path}")
            out["enchant_with_levels"] = function["levels"]
        elif kind == "minecraft:set_potion" and set(function) == {"type", "id"}:
            out["potion"] = function["id"]
        else:
            raise ValueError(f"unrecognized loot function in {path}: {function}")
    return out


def item_entries(path, table):
    result = []
    for entry in single_pool(table, path):
        if set(entry) - ENTRY_KEYS or entry.get("type") != "minecraft:item" or "value" in entry:
            raise ValueError(f"unrecognized item entry in {path}: {entry}")
        result.append({"item": entry["name"], "weight": entry.get("weight", 1), "quality": entry.get("quality", 0),
                       "condition": condition(entry, path), **modifiers(entry, path)})
    return result


def expand_tag(read, name, seen=()):
    if name in seen:
        raise ValueError(f"recursive tag {name}")
    namespace, path = name.split(":")
    values = []
    for value in read(f"data/{namespace}/tags/enchantment/{path}.json")["values"]:
        if not isinstance(value, str):
            raise ValueError(f"unrecognized tag value in {name}: {value}")
        values += expand_tag(read, value[1:], seen + (name,)) if value.startswith("#") else [value]
    return values


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-fishing-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p", name], text=True)

        pool = inspect("net.minecraft.world.level.storage.loot.LootPool")
        filter_weight = method(pool, "private static void lambda$addRandomItem$0(")
        if not re.search(r"LootPoolEntry\.getWeight:\(F\)I\n\s+\d+: istore\s+4\n\s+\d+: iload\s+4\n\s+\d+: ifle",
                         filter_weight):
            raise ValueError("LootPool no longer keeps only positive weights")
        pick = method(pool, "private void addRandomItem(")
        if "RandomSource.nextInt:(I)I" not in pick or not re.search(r"iconst_1\n\s+\d+: if_icmpne", pick):
            raise ValueError("LootPool single-entry shortcut or nextInt roll changed")
        damage = method(inspect("net.minecraft.world.level.storage.loot.functions.SetItemDamageFunction"),
                        "public net.minecraft.world.item.ItemStack run(")
        if not re.search(r"fconst_1\n(?:.*\n){1,8}.*ContextFloatProvider\.getFloat(?:.*\n){1,3}.*fadd\n.*fconst_0\n"
                         r".*fconst_1\n.*Mth\.clamp:\(FFF\)F\n.*fsub\n(?:.*\n){1,6}.*fmul\n.*Mth\.floor:\(F\)I\n"
                         r".*ItemStack\.setDamageValue", damage):
            raise ValueError("SetItemDamageFunction formula changed")
        helper = inspect("net.minecraft.world.item.enchantment.EnchantmentHelper")
        enchant = method(helper, "public static net.minecraft.world.item.ItemStack enchantItem(net.minecraft.util"
                         ".RandomSource, net.minecraft.world.item.ItemStack, int, java.util.stream.Stream")
        if "Items.BOOK" not in enchant or "Items.ENCHANTED_BOOK" not in enchant:
            raise ValueError("enchantItem no longer converts books")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(name):
            return json.loads(jar.read(name))

        top = ROOT + "fishing.json"
        categories = []
        for entry in single_pool(read(top), top):
            if set(entry) - ENTRY_KEYS or entry.get("type") != "minecraft:loot_table" or "modifier" in entry:
                raise ValueError(f"unrecognized category entry: {entry}")
            table = entry["value"].removeprefix("minecraft:gameplay/fishing/")
            if table not in TABLES:
                raise ValueError(f"unrecognized nested fishing table: {entry['value']}")
            categories.append({"table": table, "weight": entry.get("weight", 1), "quality": entry.get("quality", 0),
                               "condition": condition(entry, top)})
        tables = {name: item_entries(ROOT + f"fishing/{name}.json", read(ROOT + f"fishing/{name}.json"))
                  for name in TABLES}
        on_random_loot = expand_tag(read, "minecraft:on_random_loot")
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": [top] + [ROOT + f"fishing/{name}.json" for name in TABLES] + [
                "data/minecraft/tags/enchantment/on_random_loot.json (expanded)",
                "net.minecraft.world.level.storage.loot.LootPool.addRandomItem",
                "net.minecraft.world.level.storage.loot.functions.SetItemDamageFunction.run",
                "net.minecraft.world.item.enchantment.EnchantmentHelper.enchantItem"],
            "weight": "max(floor(weight + quality * luck), 0); only positive weights are eligible; "
                      "a lone eligible entry is taken without a roll",
            "set_damage": "damage = floor((1 - clamp(uniform(min, max), 0, 1)) * max_damage)",
            "categories": categories, "tables": tables, "on_random_loot": on_random_loot}


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
