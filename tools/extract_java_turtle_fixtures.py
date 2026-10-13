#!/usr/bin/env python3
"""Extract turtle gear/potion facts from the pinned official Java 26.3 artifact.

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


def integer_constants(code):
    result = []
    for line in code.splitlines():
        match = re.search(r"\d+: (iconst_([0-5])|(?:bipush|sipush)\s+(-?\d+))", line)
        if match:
            result.append(int(match[2] or match[3]))
    return result


def field_initialization(code, field):
    block = code.split("  static {};")[1]
    end = re.search(r"putstatic\s+#[^\n]*// Field " + field + r":", block)
    if not end:
        raise ValueError(f"missing reference initialization: {field}")
    start = [m.end() for m in re.finditer(r"putstatic[^\n]*", block[:end.start()])]
    return block[start[-1] if start else 0:end.start()]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-turtle-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p", name], text=True)

        material = field_initialization(inspect("net.minecraft.world.item.equipment.ArmorMaterials"), "TURTLE_SCUTE")
        constants = integer_constants(material)
        if len(constants) != 7 or "REPAIRS_TURTLE_HELMET" not in material:
            raise ValueError("unrecognized turtle armor material initialization")
        multiplier, boots, legs, chest, helmet, body, enchantability = constants
        armor_type = inspect("net.minecraft.world.item.equipment.ArmorType")
        helmet_type = field_initialization(armor_type, "HELMET")
        helmet_constants = integer_constants(helmet_type)
        # Enum ordinal and base durability are the two integer operands.
        if len(helmet_constants) != 2:
            raise ValueError("unrecognized helmet base durability")
        helmet_base = helmet_constants[-1]
        player = inspect("net.minecraft.world.entity.player.Player")
        renewal = player.split("  private void turtleHelmetTick();")[1].split("\n\n")[0]
        if "MobEffects.WATER_BREATHING" not in renewal:
            raise ValueError("helmet no longer grants water breathing")
        breathing = integer_constants(renewal)[0]
        registrations = inspect("net.minecraft.world.item.alchemy.Potions")
        potions = []
        for name in ("TURTLE_MASTER", "LONG_TURTLE_MASTER", "STRONG_TURTLE_MASTER"):
            block = field_initialization(registrations, name)
            effects = []
            for match in re.finditer(r"// Field net/minecraft/world/effect/MobEffects\.([A-Z_]+):", block):
                end = block.index("invokespecial", match.end())
                values = integer_constants(block[match.end():end])
                if len(values) != 2:
                    raise ValueError(f"unrecognized effect registration: {name}")
                effects.append({"effect": match[1].lower(), "amplifier": values[1], "ticks": values[0]})
            if len(effects) != 2:
                raise ValueError(f"expected both turtle master effects: {name}")
            potions.append({"id": name.lower(), "effects": effects})
        splash = inspect("net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownSplashPotion")
        if not re.search(r"DataComponents\.POTION_DURATION_SCALE[^\n]*\n\s+\d+: fconst_1", splash):
            raise ValueError("default splash duration scale changed")
        if "AABB.distanceToSqr" not in splash or "ProjectileUtil.computeMargin" not in splash:
            raise ValueError("splash distance calculation changed; reassess geometry tests")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(path):
            return json.loads(jar.read(path))

        recipe = read("data/minecraft/recipe/turtle_helmet.json")
        repair = read("data/minecraft/tags/item/repairs_turtle_helmet.json")
        brewing = []
        for bottle in ("potion", "splash_potion"):
            for suffix in ("awkward_turtle_helmet", "turtle_master_redstone", "turtle_master_glowstone_dust"):
                path = f"data/minecraft/recipe/brewing/{bottle}_{suffix}.json"
                rule = read(path)
                brewing.append({"path": path, "bottle": bottle,
                                "input": rule["input"]["potion_contents"]["potions"],
                                "ingredient": rule["reagent"]["item"],
                                "output": rule["output"]["components"]["minecraft:potion_contents"]["potion"]})
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["data/minecraft/recipe/turtle_helmet.json",
                        "data/minecraft/tags/item/repairs_turtle_helmet.json",
                        "net.minecraft.world.item.equipment.ArmorMaterials.TURTLE_SCUTE",
                        "net.minecraft.world.item.equipment.ArmorType.HELMET",
                        "net.minecraft.world.entity.player.Player.turtleHelmetTick",
                        "net.minecraft.world.item.alchemy.Potions turtle registrations",
                        "net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownSplashPotion default duration scale"],
            "helmet": {"pattern": recipe["pattern"], "ingredients": recipe["key"],
                       "result": recipe["result"]["id"], "durability": multiplier * helmet_base,
                       "defense": helmet, "enchantability": enchantability,
                       "repair_items": repair["values"], "water_breathing_ticks": breathing},
            "potions": potions, "brewing": brewing, "splash_default_duration_scale": 1}


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
