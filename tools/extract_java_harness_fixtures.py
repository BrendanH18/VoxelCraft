#!/usr/bin/env python3
"""Extract the harness items' facts from the pinned official Java 26.3 artifact.

Covers the `harnesses` item tag (its order is `DyeColor` order), the
`Items.HARNESS` registration (`stacksTo(1)` and `Equippable.harness`),
`Equippable.harness` (slot, sounds, allowed entities, equip on interact,
shearing), the sound event ids, the `can_equip_harness` entity tag, the
happy ghast's tempt tag, each harness recipe's group and category, and every
other data file that names a harness (none outside recipes, recipe
advancements and tags: the only way to get one is crafting).

Requires javap. Unknown bytecode forms fail rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
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


def method(code, signature):
    """The bytecode of the first method whose declaration contains `signature`."""
    start = code.find(signature)
    if start < 0:
        raise ValueError(f"missing method: {signature}")
    end = code.find("\n\n", start)
    return code[start:end if end >= 0 else len(code)]


def ops(code):
    out = []
    for line in code.splitlines():
        match = re.match(r"\s+\d+: (\w+)\s*(.*)", line)
        if match:
            out.append(f"{match[1]} {match[2]}".strip())
    return out


def require_sequence(code, pieces, what):
    """`pieces` appear in this order among the instructions."""
    at = 0
    listing = ops(code)
    for piece in pieces:
        while at < len(listing) and piece not in listing[at]:
            at += 1
        if at == len(listing):
            raise ValueError(f"unrecognized {what}: missing {piece!r}")
        at += 1


def sound_id(sounds, field):
    match = re.search(r"// String ([a-z0-9_.]+)\n[^\n]*\n\s+\d+: putstatic[^\n]*Field " + field + ":", sounds)
    if not match:
        raise ValueError(f"unrecognized SoundEvents.{field}")
    return match[1]


def strip(name):
    if not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name}")
    return name[len("minecraft:"):]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-harness-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        items = inspect("world.item.Items")
        require_sequence(items, [
            "Field net/minecraft/references/ItemIds.HARNESS",
            "invokedynamic",
            "ColorCollection.registerItems",
            "putstatic",
        ], "Items.HARNESS registration")
        if not re.search(r"ItemIds\.HARNESS:[^\n]*\n[^\n]*invokedynamic[^\n]*\n[^\n]*registerItems[^\n]*\n"
                         r"[^\n]*putstatic[^\n]*Field HARNESS:", items):
            raise ValueError("Items.HARNESS is no longer registered as one colour collection")
        lambdas = [m for m in re.split(r"\n\n", items)
                   if "Equippable.harness:" in m and "private static net.minecraft.world.item.Item lambda$" in m]
        if len(lambdas) != 1:
            raise ValueError("expected one item lambda building Equippable.harness")
        properties = lambdas[0]
        require_sequence(properties, [
            "Item$Properties.\"<init>\"", "iconst_1", "Item$Properties.stacksTo",
            "DataComponents.EQUIPPABLE", "Equippable.harness", "Item$Properties.component", "registerItem",
        ], "harness item properties")
        stack_size = 1
        if re.search(r"Item\$Properties\.(durability|food|fireResistant|rarity|enchantable)", properties):
            raise ValueError("harness properties gained a component this extractor does not record")

        equippable = method(inspect("world.item.equipment.Equippable"),
                            "public static net.minecraft.world.item.equipment.Equippable harness(")
        require_sequence(equippable, [
            "EquipmentSlot.BODY", "Method builder:",
            "SoundEvents.HARNESS_EQUIP", "setEquipSound",
            "EquipmentAssets.HARNESSES", "setAsset",
            "EntityTypeTags.CAN_EQUIP_HARNESS", "setAllowedEntities",
            "iconst_1", "setEquipOnInteract",
            "iconst_1", "setCanBeSheared",
            "SoundEvents.HARNESS_UNEQUIP", "setShearingSound",
            "build",
        ], "Equippable.harness")
        for setter in ("setSwappable", "setDamageOnHurt", "setCameraOverlay", "setDispensable"):
            if setter in equippable:
                raise ValueError(f"Equippable.harness now calls {setter}")
        sounds = inspect("sounds.SoundEvents")
        equip_sound = sound_id(sounds, "HARNESS_EQUIP")
        shear_sound = sound_id(sounds, "HARNESS_UNEQUIP")

    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(name):
            return json.loads(jar.read(name))

        tag = [strip(v) for v in read("data/minecraft/tags/item/harnesses.json")["values"]]
        allowed = [strip(v) for v in read("data/minecraft/tags/entity_type/can_equip_harness.json")["values"]]
        tempt = [v for v in read("data/minecraft/tags/item/happy_ghast_tempt_items.json")["values"]]
        colours = []
        for name in tag:
            if not name.endswith("_harness"):
                raise ValueError(f"unexpected harness tag entry {name}")
            colours.append(name[:-len("_harness")])
        recipes = {}
        for colour in colours:
            for key in (f"{colour}_harness", f"dye_{colour}_harness"):
                recipe = read(f"data/minecraft/recipe/{key}.json")
                recipes[key] = {
                    "type": strip(recipe["type"]),
                    "group": recipe.get("group", ""),
                    "category": recipe.get("category", ""),
                }
        others = []
        for name in jar.namelist():
            if not name.startswith("data/minecraft/") or not name.endswith(".json"):
                continue
            if name.startswith(("data/minecraft/recipe/", "data/minecraft/advancement/recipes/")):
                continue
            if name in ("data/minecraft/tags/item/harnesses.json", "data/minecraft/tags/entity_type/can_equip_harness.json"):
                continue
            if "_harness" in jar.read(name).decode():
                others.append(name)
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.item.Items HARNESS (ColorCollection.registerItems and its item lambda)",
            "net.minecraft.world.item.equipment.Equippable.harness",
            "net.minecraft.sounds.SoundEvents HARNESS_EQUIP, HARNESS_UNEQUIP",
            "data/minecraft/tags/item/harnesses.json",
            "data/minecraft/tags/entity_type/can_equip_harness.json",
            "data/minecraft/tags/item/happy_ghast_tempt_items.json",
            "data/minecraft/recipe/{colour}_harness.json, dye_{colour}_harness.json",
        ],
        "items": tag,
        "colours": colours,
        "stack_size": stack_size,
        "equippable": {
            "slot": "body",
            "equip_sound": equip_sound,
            "shearing_sound": shear_sound,
            "allowed_entities": allowed,
            "equip_on_interact": True,
            "can_be_sheared": True,
        },
        "happy_ghast_tempt_items": tempt,
        "recipes": recipes,
        "other_data_files_naming_a_harness": sorted(others),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
