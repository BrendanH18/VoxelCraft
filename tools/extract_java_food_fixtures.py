#!/usr/bin/env python3
"""Extract every food from the SHA-pinned Java 26.3 server, using javap.

For each `Items` registration that calls `Item$Properties.food`, records the
`Foods` properties (nutrition, saturation modifier, the saturation
`FoodConstants.saturationByModifier` gives, `alwaysEdible`), the `Consumables`
entry (consume seconds, eat or drink, status effects and their chance,
removed effects, clear-all, random teleport), the stack size and the
`usingConvertsTo` remainder.

Reads no VoxelCraft source. Unknown registration or bytecode forms fail closed.
"""

import argparse
import hashlib
import io
import json
import re
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

FIELD = re.compile(r"putstatic\s+#\d+\s+// Field (\w+):")


def f32(value):
    """Round to an IEEE single, as Java's float arithmetic does."""
    return struct.unpack("<f", struct.pack("<f", value))[0]


def static_blocks(code):
    """The static initializer split at each `putstatic`, as (field, bytecode)."""
    body = code.split("  static {};")[1]
    blocks, start = [], 0
    for match in FIELD.finditer(body):
        blocks.append((match.group(1), body[start : match.start()]))
        start = match.end()
    return blocks


def method(code, signature):
    parts = code.split(signature)
    if len(parts) != 2:
        raise ValueError(f"missing reference method: {signature}")
    return parts[1].split("\n\n")[0]


def int_before(block, call):
    """The int constant pushed right before `call`."""
    match = re.search(
        r"(?:iconst_(\d)|bipush\s+(\d+)|sipush\s+(\d+))\n\s+\d+: invoke\w+\s+#\d+\s+// Method [^\n]*" + call, block
    )
    if not match:
        raise ValueError(f"unrecognized int argument to {call}")
    return int(next(group for group in match.groups() if group is not None))


def float_before(block, call):
    match = re.search(
        r"(?:// float ([0-9.]+)f|: fconst_(\d))\n\s+\d+: invoke\w+\s+#\d+\s+// Method [^\n]*" + call, block
    )
    if not match:
        raise ValueError(f"unrecognized float argument to {call}")
    return float(next(group for group in match.groups() if group is not None))


def foods(code):
    """`Foods` constants: nutrition, saturation modifier and `alwaysEdible`."""
    stew = method(code, "FoodProperties$Builder stew(int);")
    stew_modifier = float_before(stew, "saturationModifier:")
    out = {}
    for field, block in static_blocks(code):
        if "FoodProperties$Builder.build:" not in block:
            raise ValueError(f"unrecognized food properties: {field}")
        if "Method stew:" in block:
            nutrition, modifier = int_before(block, "stew:"), stew_modifier
        else:
            nutrition, modifier = int_before(block, "nutrition:"), float_before(block, "saturationModifier:")
        out[field] = {
            "nutrition": nutrition,
            "saturation_modifier": modifier,
            # `FoodConstants.saturationByModifier`: `nutrition * modifier * 2.0F` in floats.
            "saturation": f32(f32(nutrition * f32(modifier)) * 2.0),
            "always_edible": "alwaysEdible:" in block,
        }
    return out


def effect_list(block, constructor):
    """`MobEffectInstance(effect, duration, amplifier)` arguments in `block`."""
    lines = block.split("\n")
    effects = []
    for index, line in enumerate(lines):
        match = re.search(r"Field net/minecraft/world/effect/MobEffects\.(\w+):", line)
        if match:
            values = []
            for following in lines[index + 1 : index + 3]:
                pushed = re.search(
                    r": (?:bipush|sipush)\s+(\d+)|: iconst_(\d)\b|: ldc\s+#\d+\s+// int (\d+)", following
                )
                if not pushed:
                    raise ValueError(f"unrecognized effect instance in {constructor}")
                values.append(int(next(group for group in pushed.groups() if group is not None)))
            effects.append({"effect": match.group(1).lower(), "amplifier": values[1], "ticks": values[0]})
    return effects


def consumable(block, defaults):
    """One `Consumables` entry, failing on any consume effect it doesn't know."""
    base = "food" if "defaultFood:" in block else "drink" if "defaultDrink:" in block else None
    if base is None:
        raise ValueError("unrecognized consumable base")
    seconds = float_before(block, "consumeSeconds:") if "consumeSeconds:" in block else defaults[base]
    out = {
        "animation": "eat" if base == "food" else "drink",
        "consume_seconds": seconds,
        "consume_ticks": round(seconds * 20),
        "effect_chance": 1.0,
        "effects": [],
        "removes_effects": [],
        "clears_effects": False,
        "teleports": False,
    }
    effects = re.findall(r"class net/minecraft/world/item/consume_effects/(\w+)\n", block)
    effects += re.findall(r"Field net/minecraft/world/item/consume_effects/(\w+)\.INSTANCE:", block)
    for effect in effects:
        if effect == "ApplyStatusEffectsConsumeEffect":
            out["effects"] = effect_list(block, effect)
            chance = re.search(
                r"// float ([0-9.]+)f\n\s+\d+: invokespecial[^\n]*ApplyStatusEffectsConsumeEffect.\"<init>\":"
                r"\([^)]*;F\)V",
                block,
            )
            out["effect_chance"] = float(chance.group(1)) if chance else 1.0
        elif effect == "RemoveStatusEffectsConsumeEffect":
            out["removes_effects"] = [
                name.lower() for name in re.findall(r"Field net/minecraft/world/effect/MobEffects\.(\w+):", block)
            ]
        elif effect == "ClearAllStatusEffectsConsumeEffect":
            out["clears_effects"] = True
        elif effect == "TeleportRandomlyConsumeEffect":
            out["teleports"] = True
        else:
            raise ValueError(f"unrecognized consume effect: {effect}")
    if not effects and "onConsume:" in block:
        raise ValueError("unrecognized consume effect")
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-foods-") as temporary:
        jar = Path(temporary) / "reference.jar"
        jar.write_bytes(inner)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(jar), "-c", "-p", name], text=True)

        food_table = foods(inspect("net.minecraft.world.food.Foods"))
        code = inspect("net.minecraft.world.item.component.Consumables")
        defaults = {
            name: float_before(method(code, f"Consumable$Builder default{name.title()}();"), "consumeSeconds:")
            for name in ("food", "drink")
        }
        consumables = {field: consumable(block, defaults) for field, block in static_blocks(code)}
        properties = inspect("net.minecraft.world.item.Item$Properties")
        if "Consumables.DEFAULT_FOOD:" not in method(properties, " food(net.minecraft.world.food.FoodProperties);"):
            raise ValueError("default food consumable changed")
        items = []
        for field, block in static_blocks(inspect("net.minecraft.world.item.Items")):
            if "Item$Properties.food:" not in block:
                continue
            food = re.findall(r"Field net/minecraft/world/food/Foods\.(\w+):", block)
            uses = re.findall(r"Field net/minecraft/world/item/component/Consumables\.(\w+):", block)
            if len(food) != 1 or len(uses) > 1 or block.count("Item$Properties.food:") != 1:
                raise ValueError(f"unrecognized food registration: {field}")
            converts = re.search(r"getstatic\s+#\d+\s+// Field (\w+):[^\n]*\n[^\n]*usingConvertsTo:", block)
            entry = {"item": field.lower(), "food": food[0], **food_table[food[0]]}
            entry["consumable"] = uses[0] if uses else "DEFAULT_FOOD"
            entry.update(consumables[entry["consumable"]])
            entry["max_stack"] = int_before(block, "stacksTo:") if "stacksTo:" in block else 64
            entry["converts_to"] = converts.group(1).lower() if converts else None
            items.append(entry)
    unused = sorted(set(food_table) - {entry["food"] for entry in items})
    if unused:
        raise ValueError(f"food properties no item registers: {unused}")
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.food.Foods",
            "net.minecraft.world.food.FoodConstants.saturationByModifier",
            "net.minecraft.world.item.component.Consumables",
            "net.minecraft.world.item.Item$Properties.food",
            "net.minecraft.world.item.Items",
        ],
        "foods": items,
    }


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
