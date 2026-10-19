#!/usr/bin/env python3
"""Extract consumption timing from the SHA-pinned Java 26.3 server, using javap.

Reads no VoxelCraft source. Unknown registration/bytecode forms fail closed.
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
from extract_java_turtle_fixtures import field_initialization


def seconds(block):
    matches = re.findall(r"// float ([0-9.]+)f\n\s+\d+: invokevirtual[^\n]*consumeSeconds:", block)
    if len(matches) != 1:
        raise ValueError("unrecognized reference consumption duration")
    return float(matches[0])


def push_value(line):
    match = re.search(r": (?:bipush|sipush)\s+(\d+)|: iconst_(\d)\b|: ldc\s+#\d+\s+// int (\d+)", line)
    if not match:
        raise ValueError(f"unrecognized constant: {line.strip()}")
    return int(next(group for group in match.groups() if group is not None))


def food_effects(block):
    """Status effects `(id, amplifier, ticks)` and probability of one consume effect."""
    lines = block.split("\n")
    effects = []
    for index, line in enumerate(lines):
        match = re.search(r"Field net/minecraft/world/effect/MobEffects\.(\w+):", line)
        if match:
            duration, amplifier = push_value(lines[index + 1]), push_value(lines[index + 2])
            effects.append({"effect": match.group(1).lower(), "amplifier": amplifier, "ticks": duration})
    if not effects or len(re.findall(r"onConsume:", block)) != 1:
        raise ValueError("unrecognized food consume effects")
    chance = re.search(r"// float ([0-9.]+)f\n\s+\d+: invokespecial[^\n]*ApplyStatusEffectsConsumeEffect.\"<init>\":"
                       r"\([^)]*;F\)V", block)
    return {"chance": float(chance.group(1)) if chance else 1.0, "effects": effects}


def always_edible(foods):
    """Foods built with `alwaysEdible()`, in registration order."""
    names = []
    start = 0
    for match in re.finditer(r"putstatic\s+#\d+\s+// Field (\w+):", foods):
        if "alwaysEdible" in foods[start:match.start()]:
            names.append(match.group(1).lower())
        start = match.end()
    return names


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-consumption-") as temporary:
        jar = Path(temporary) / "reference.jar"
        jar.write_bytes(inner)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(jar), "-c", "-p", name], text=True)

        code = inspect("net.minecraft.world.item.component.Consumables")
        defaults = {}
        for name in ("Food", "Drink"):
            block = code.split(f" default{name}();")[1].split("\n\n")[0]
            defaults[name.lower()] = seconds(block)
        kelp = seconds(field_initialization(code, "DRIED_KELP"))
        milk = field_initialization(code, "MILK_BUCKET")
        if "defaultDrink:" not in milk or "ClearAllStatusEffectsConsumeEffect.INSTANCE" not in milk:
            raise ValueError("milk consumption rule changed")
        items = inspect("net.minecraft.world.item.Items")
        for item, provider in (("DRIED_KELP", "DRIED_KELP"), ("MILK_BUCKET", "MILK_BUCKET"),
                               ("POTION", "DEFAULT_DRINK")):
            block = field_initialization(items, item)
            if f"Consumables.{provider}:" not in block:
                raise ValueError(f"consumable registration changed: {item}")
        properties = inspect("net.minecraft.world.item.Item$Properties")
        default_food = properties.split(" food(net.minecraft.world.food.FoodProperties);")[1].split("\n\n")[0]
        if "Consumables.DEFAULT_FOOD:" not in default_food:
            raise ValueError("default food registration changed")
        food_cases = {}
        for item, provider in (("SPIDER_EYE", "SPIDER_EYE"), ("POISONOUS_POTATO", "POISONOUS_POTATO"),
                               ("PUFFERFISH", "PUFFERFISH"), ("ROTTEN_FLESH", "ROTTEN_FLESH"),
                               ("CHICKEN", "CHICKEN"), ("GOLDEN_APPLE", "GOLDEN_APPLE"),
                               ("ENCHANTED_GOLDEN_APPLE", "ENCHANTED_GOLDEN_APPLE")):
            food_cases["raw_chicken" if item == "CHICKEN" else item.lower()] = food_effects(
                field_initialization(code, provider))
        edible = always_edible(inspect("net.minecraft.world.food.Foods"))
        for item in ("APPLE", "MUSHROOM_STEW"):
            block = field_initialization(items, item)
            if f"Foods.{item}:" not in block or "Properties.food:(Lnet/minecraft/world/food/FoodProperties;)" not in block:
                raise ValueError(f"default food association changed: {item}")
    cases = [("apple", defaults["food"]), ("mushroom_stew", defaults["food"]),
             ("dried_kelp", kelp), ("milk_bucket", defaults["drink"]),
             ("potion", defaults["drink"])]
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["net.minecraft.world.item.component.Consumables",
                        "net.minecraft.world.item.Item$Properties.food", "net.minecraft.world.item.Items",
                        "net.minecraft.world.food.Foods"],
            "cases": [{"item": item, "seconds": value, "ticks": round(value * 20)} for item, value in cases],
            "food_effects": food_cases, "always_edible": edible}


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
