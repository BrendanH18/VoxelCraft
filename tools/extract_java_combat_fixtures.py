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

        armor_types = {}
        block = inspect("net.minecraft.world.item.equipment.ArmorType").split("  static {};")[1]
        for name, units in re.findall(r"String ([A-Z]+)\n[^\n]*\n[^\n]*EquipmentSlot\.\w+:[^\n]*\n[^\n]*bipush\s+(\d+)", block):
            armor_types[name.lower()] = int(units)
        if set(armor_types) < {"helmet", "chestplate", "leggings", "boots"}:
            raise ValueError("unrecognized armor type durability units")
        armor = {}
        block = inspect("net.minecraft.world.item.equipment.ArmorMaterials").split("  static {};")[1]
        pieces = re.split(r"putstatic\s+#\d+\s+// Field ([A-Z_]+):Lnet/minecraft/world/item/equipment/ArmorMaterial;", block)
        for code, name in zip(pieces[0::2], pieces[1::2]):
            ints, floats = int_operands(code), float_operands(code)
            repair = re.search(r"ItemTags\.(\w+):", code)
            if "makeDefense" not in code or len(ints) != 7 or len(floats) != 2 or not repair:
                raise ValueError(f"unrecognized armor material initialization: {name}")
            boots, leggings, chestplate, helmet, body = ints[1:6]
            armor[name.lower()] = {"durability_multiplier": ints[0],
                                   "defense": {"helmet": helmet, "chestplate": chestplate, "leggings": leggings,
                                               "boots": boots, "body": body},
                                   "enchantability": ints[6], "toughness": floats[0],
                                   "knockback_resistance": floats[1], "repair_items": repair[1].lower()}
        if not {"leather", "copper", "iron", "gold", "diamond", "netherite", "turtle_scute"} <= set(armor):
            raise ValueError(f"missing armor materials: {sorted(armor)}")

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
        # Criticals read `fallDistance` and mobility restriction, not velocity.
        critical = method(player, "private boolean canCriticalAttack(net.minecraft.world.entity.Entity);")
        for needed in ("Field fallDistance:D", "Player.onGround", "Player.onClimbable", "Player.isInWater",
                       "Player.isMobilityRestricted", "Player.isPassenger", "instanceof    #173",
                       "Player.isSprinting"):
            if needed.replace("Player.", "") not in critical:
                raise ValueError(f"unrecognized critical condition: {needed}")
        restricted = method(player, "public boolean isMobilityRestricted();")
        if "MobEffects.BLINDNESS" not in restricted:
            raise ValueError("mobility restriction is no longer Blindness")
        # `minimum_attack_charge` is only ever set by `Item$Properties.spear`.
        properties = inspect("net.minecraft.world.item.Item$Properties")
        spear = method(properties, "public net.minecraft.world.item.Item$Properties spear(net.minecraft.world.item.ToolMaterial, "
                                   + ", ".join(["float"] * 9) + ");")
        charge = re.search(r"MINIMUM_ATTACK_CHARGE:[^\n]*\n[^\n]*(fconst_1|ldc\w*\s+#\d+\s+// float ([\d.]+)f)", spear)
        if not charge or properties.count("MINIMUM_ATTACK_CHARGE") != 1 or "MINIMUM_ATTACK_CHARGE" in items:
            raise ValueError("unrecognized minimum attack charge component")
        spears = [m[1].lower() for m in re.finditer(
            r"// Field net/minecraft/references/ItemIds\.([A-Z_]+):[^\n]*\n(?:(?![^\n]*ItemIds\.)[^\n]*\n)*?"
            r"[^\n]*Item\$Properties\.spear:", items)]
        if not spears:
            raise ValueError("no spear registrations")
        gate = method(player, "public boolean cannotAttackWithItem(net.minecraft.world.item.ItemStack, int);")
        if "MINIMUM_ATTACK_CHARGE" not in gate or "getCurrentItemAttackStrengthDelay" not in gate:
            raise ValueError("unrecognized cannotAttackWithItem")
        effects = inspect("net.minecraft.world.effect.MobEffects").split("  static {};")[1]
        effect_modifiers = {}
        previous = 0
        for end in re.finditer(r"putstatic\s+#\d+\s+// Field ([A-Z_]+):Lnet/minecraft/core/Holder;", effects):
            code = effects[previous:end.start()]
            previous = end.end()
            found = re.search(r"Attributes\.ATTACK_SPEED:[^\n]*\n[^\n]*\n[^\n]*\n[^\n]*double (-?[\d.]+)d\n"
                              r"[^\n]*Operation\.(\w+):", code)
            if found and end[1] in ("HASTE", "MINING_FATIGUE"):
                effect_modifiers[end[1].lower()] = {"attribute": "attack_speed", "amount_per_level": float(found[1]),
                                                    "operation": found[2].lower()}
        if set(effect_modifiers) != {"haste", "mining_fatigue"}:
            raise ValueError(f"unrecognized attack speed effects: {effect_modifiers}")
        bounds = re.search(r"String attribute\.name\.attack_speed\n[^\n]*double [\d.]+d\n[^\n]*(dconst_0)\n[^\n]*double ([\d.]+)d", registry)
        if not bounds:
            raise ValueError("unrecognized attack speed attribute range")
        speed_range = [0.0, float(bounds[2])]
        # Vehicles take `damage * 10` and break above 40; dragon parts other than head and neck take a quarter + min(damage, 1).
        vehicle = method(inspect("net.minecraft.world.entity.vehicle.VehicleEntity"),
                         "public boolean hurtServer(net.minecraft.server.level.ServerLevel, "
                         "net.minecraft.world.damagesource.DamageSource, float);")
        if float_operands(vehicle) != [10.0, 40.0]:
            raise ValueError(f"unrecognized vehicle damage: {float_operands(vehicle)}")
        dragon = inspect("net.minecraft.world.entity.boss.enderdragon.EnderDragon")
        part_hurt = method(dragon, "public boolean hurt(net.minecraft.server.level.ServerLevel, "
                                   "net.minecraft.world.entity.boss.enderdragon.EnderDragonPart, "
                                   "net.minecraft.world.damagesource.DamageSource, float);")
        if "Field head:" not in part_hurt or "Field neck:" not in part_hurt or float_operands(part_hurt)[:3] != [4.0, 1.0, 0.01]:
            raise ValueError("unrecognized dragon part damage")
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
        "criticals": {
            "needs": ["fall_distance > 0", "not on ground", "not on climbable", "not in water",
                      "not mobility restricted (Blindness)", "not a passenger", "living target", "not sprinting"],
            "strong_threshold": 0.9,
        },
        "minimum_attack_charge": {
            "default": 0.0,
            "set_only_by": "Item$Properties.spear",
            "spear_value": float(charge[2] or 1.0),
            "spear_items": sorted(spears),
            "server_lead_ticks": 5,
        },
        "attack_speed_effects": effect_modifiers,
        "attack_speed_range": speed_range,
        "vehicle_hurt": {"damage_factor": 10.0, "destroy_above": 40.0},
        "dragon_part_hurt": {"full_damage_parts": ["head", "neck"], "other_parts": "damage / 4 + min(damage, 1)",
                              "minimum_damage": 0.01},
        "screen_ranges": {
            "check": "eye-to-box squared distance < (interaction range + extra)^2",
            "block_interaction_range_extra": {"block_container": screens["block_container"]},
            "entity_interaction_range_extra": {k: screens[k] for k in ("vehicle_container", "merchant")},
        },
        "materials": {k.lower(): v for k, v in sorted(materials.items())},
        "armor_durability_units": armor_types,
        "armor_materials": dict(sorted(armor.items())),
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
