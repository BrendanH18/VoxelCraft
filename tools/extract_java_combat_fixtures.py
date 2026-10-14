#!/usr/bin/env python3
"""Extract player melee facts from the pinned official Java 26.3 artifact.

Requires javap. Records tool attack attributes (`Item$Properties` sword/axe/...
registrations plus `ToolMaterial` damage bonuses), each material's durability,
speed, enchantability and incorrect-block/repair tags, player base attributes and
interaction ranges, and the attack-strength constants used by `Player.attack`.
Unknown bytecode forms fail rather than regenerating a guessed fixture.
No VoxelCraft source or existing fixture is consulted.
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

TOOLS = ("sword", "shovel", "pickaxe", "axe", "hoe")
MATERIALS = ("WOOD", "STONE", "COPPER", "IRON", "GOLD", "DIAMOND", "NETHERITE")


def float_operands(code):
    values = []
    for line in code.splitlines():
        match = re.search(r"\d+: (?:fconst_([0-2])|ldc(?:_w)?\s+#\d+\s+// float (-?[\d.]+)f)", line)
        if match:
            values.append(float(match[1] if match[1] is not None else match[2]))
    return values


def int_operands(code):
    values = []
    for line in code.splitlines():
        match = re.search(r"\d+: (?:iconst_([0-5])|bipush\s+(-?\d+)|sipush\s+(-?\d+))\s*$", line)
        if match:
            values.append(int(next(g for g in match.groups() if g is not None)))
    return values


def method(code, signature):
    parts = code.split("  " + signature)
    if len(parts) != 2:
        raise ValueError(f"missing reference method: {signature}")
    return parts[1].split("\n\n")[0]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-combat-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name, constants=False):
            args = ["javap", "-classpath", str(path), "-c", "-p"] + (["-constants"] if constants else []) + [name]
            return subprocess.check_output(args, text=True)

        materials = {}
        block = inspect("net.minecraft.world.item.ToolMaterial").split("  static {};")[1]
        pieces = re.split(r"putstatic\s+#\d+\s+// Field ([A-Z_]+):Lnet/minecraft/world/item/ToolMaterial;", block)
        for code, name in zip(pieces[0::2], pieces[1::2]):
            floats = float_operands(code)
            ints = int_operands(code)
            incorrect = re.search(r"BlockTags\.(\w+):", code)
            repair = re.search(r"ItemTags\.(\w+):", code)
            if len(floats) != 2 or len(ints) != 2 or not incorrect or not repair:
                raise ValueError(f"unrecognized tool material initialization: {name}")
            materials[name] = {"speed": floats[0], "attack_damage_bonus": floats[1], "durability": ints[0],
                               "enchantability": ints[1], "incorrect_blocks": incorrect[1].lower(),
                               "repair_items": repair[1].lower()}
        if set(MATERIALS) - set(materials):
            raise ValueError("missing tool materials")
        with zipfile.ZipFile(path) as inner:
            for facts in materials.values():
                tag = json.loads(inner.read(f"data/minecraft/tags/block/{facts['incorrect_blocks']}.json"))
                facts["incorrect_block_tag_values"] = tag["values"]

        items = inspect("net.minecraft.world.item.Items").split("  static {};")[1]
        tools = []
        pattern = (r"// Field net/minecraft/references/ItemIds\.([A-Z_]+):[^\n]*\n((?:(?![^\n]*ItemIds\.)[^\n]*\n)*?)"
                   r"[^\n]*Item\$Properties\.(" + "|".join(TOOLS) + r"):\(Lnet/minecraft/world/item/ToolMaterial;FF\)")
        for match in re.finditer(pattern, items):
            material = re.search(r"ToolMaterial\.([A-Z_]+):", match[2])
            floats = float_operands(match[2])
            if not material or len(floats) != 2:
                raise ValueError(f"unrecognized tool registration: {match[1]}")
            bonus = materials[material[1]]["attack_damage_bonus"]
            tools.append({"id": match[1].lower(), "tool": match[3], "material": material[1].lower(),
                          "attack_damage_modifier": floats[0], "attack_speed_modifier": floats[1],
                          "attack_damage_with_material": floats[0] + bonus})
        if len(tools) != len(TOOLS) * len(MATERIALS):
            raise ValueError(f"expected every tool registration, found {len(tools)}")

        player = inspect("net.minecraft.world.entity.player.Player", constants=True)
        ranges = {}
        for name in ("DEFAULT_BLOCK_INTERACTION_RANGE", "DEFAULT_ENTITY_INTERACTION_RANGE",
                     "CREATIVE_ENTITY_INTERACTION_RANGE_MODIFIER_VALUE"):
            found = re.search(r"static final float " + name + r" = (-?[\d.]+)f;", player)
            if not found:
                raise ValueError(f"missing player constant: {name}")
            ranges[name.lower()] = float(found[1])
        attributes = method(player, "public static net.minecraft.world.entity.ai.attributes.AttributeSupplier$Builder "
                                    "createAttributes();")
        if not re.search(r"Attributes\.ATTACK_DAMAGE:[^\n]*\n[^\n]*dconst_1", attributes):
            raise ValueError("unrecognized player base attack damage")
        if re.search(r"Attributes\.ATTACK_SPEED:[^\n]*\n[^\n]*(?:dconst|ldc2_w)", attributes):
            raise ValueError("player now overrides the attack speed default")
        registry = inspect("net.minecraft.world.entity.ai.attributes.Attributes")
        speed = re.search(r"String attribute\.name\.attack_speed\n[^\n]*double ([\d.]+)d", registry)
        if not speed:
            raise ValueError("unrecognized attack speed attribute default")
        base = {"attack_damage": 1.0, "attack_speed": float(speed[1])}
        scale = method(player, "private float baseDamageScaleFactor();")
        if float_operands(scale) != [0.5, 0.2, 0.8]:
            raise ValueError("unrecognized attack strength damage scale")
        delay = method(player, "public float getCurrentItemAttackStrengthDelay();")
        if "ATTACK_SPEED" not in delay or "double 20.0d" not in delay:
            raise ValueError("unrecognized attack strength delay")
        attack = method(player, "public void attack(net.minecraft.world.entity.Entity);")
        if float_operands(attack) != [0.5, 0.0, 0.0, 0.9, 1.5, 0.0, 0.5, 0.0, 0.1]:
            raise ValueError("unrecognized Player.attack constants")
        screens = {}
        for key, cls, sig, call in [
            ("block_container", "net.minecraft.world.Container",
             "public static boolean stillValidBlockEntity(net.minecraft.world.level.block.entity.BlockEntity, "
             "net.minecraft.world.entity.player.Player);", "stillValidBlockEntity"),
            ("vehicle_container", "net.minecraft.world.entity.vehicle.ContainerEntity",
             "public default boolean isChestVehicleStillValid(net.minecraft.world.entity.player.Player);",
             "isWithinEntityInteractionRange"),
            ("merchant", "net.minecraft.world.entity.npc.villager.AbstractVillager",
             "public boolean stillValid(net.minecraft.world.entity.player.Player);", "isWithinEntityInteractionRange"),
        ]:
            code = method(inspect(cls), sig)
            found = re.search(r"ldc(?:2_w|_w)?\s+#\d+\s+// (?:float|double) ([\d.]+)[fd]\n[^\n]*" + call, code)
            if not found:
                raise ValueError(f"unrecognized screen range: {key}")
            screens[key] = float(found[1])
        block_range = method(player, "public boolean isWithinBlockInteractionRange(net.minecraft.core.BlockPos, double);")
        entity_range = method(player, "public boolean isWithinEntityInteractionRange(net.minecraft.world.phys.AABB, double);")
        if not all("dadd" in c and "AABB.distanceToSqr" in c and "dmul" in c for c in (block_range, entity_range)):
            raise ValueError("unrecognized interaction range check")
        sweep = method(player, "private boolean isSweepAttack(boolean, boolean, boolean);")
        if "double 2.5d" not in sweep or "ItemTags.SWORDS" not in sweep:
            raise ValueError("unrecognized sweep condition")
    return {
        "source": "official Java 26.3 server artifact, SHA-1 " + SERVER_SHA1,
        "player": {**base, **ranges},
        "attack_strength": {
            "delay_ticks": "20 / attack_speed",
            "scale": "clamp((ticker + 0.5) / delay, 0, 1)",
            "base_damage_factor": "0.2 + 0.8 * scale^2",
            "enchantment_damage_factor": "scale",
            "strong_threshold": 0.9,
            "critical_multiplier": 1.5,
            "sprint_knockback_bonus": 0.5,
            "food_exhaustion": 0.1,
            "sweep_max_speed_factor": 2.5,
        },
        "screen_ranges": {
            "check": "eye-to-box squared distance < (interaction range + extra)^2",
            "block_interaction_range_extra": {"block_container": screens["block_container"]},
            "entity_interaction_range_extra": {k: screens[k] for k in ("vehicle_container", "merchant")},
        },
        "materials": {k.lower(): v for k, v in sorted(materials.items())},
        "tools": tools,
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
