#!/usr/bin/env python3
"""Extract mob equipment facts from the pinned official Java 26.3 artifact.

Covers `EquipmentSlot`, `DropChances`, `Mob.dropCustomDeathLoot` (the drop
rule and the wear reroll), `Mob.populateDefaultEquipmentSlots` and
`populateDefaultEquipmentEnchantments`, the drowned's override, the
`keepEquipment` flag of each conversion, the Looting `equipment_drops`
effect, the curses' equipment effects and the trial spawners' slot drop
chances.

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
    """Each instruction as `mnemonic operand-comment`, in order."""
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


def static_constant(code, name):
    match = re.search(r"static final \w+ " + name + r" = ([-0-9.]+)f?;", code)
    if not match:
        raise ValueError(f"missing constant {name}")
    return float(match[1])


def slots(code):
    block = code.split("  static {};")[1]
    found = []
    for match in re.finditer(
        r"ldc\s+#\d+\s+// String ([A-Z]+)\n(.*?)invokespecial[^\n]*\n\s+\d+: putstatic[^\n]*Field ([A-Z]+):",
        block,
        re.S,
    ):
        body = match[2]
        kind = re.search(r"EquipmentSlot\$Type\.([A-Z_]+)", body)[1]
        values = [int(v[0] or v[1]) for v in re.findall(r"(?:iconst_(\d)|bipush\s+(\d+))", body)]
        name = re.search(r"// String ([a-z]+)", body)[1]
        # Enum ordinal, then index, [count limit,] id.
        if len(values) == 3:
            ordinal, index, ident = values
            limit = 0
        elif len(values) == 4:
            ordinal, index, limit, ident = values
        else:
            raise ValueError(f"unrecognized slot initialization: {match[1]}")
        if match[1] != match[3] or ordinal != len(found):
            raise ValueError("slot initialization out of order")
        found.append({"name": name, "type": kind.lower(), "index": index, "count_limit": limit, "id": ident})
    if [s["name"] for s in found][:1] != ["mainhand"] or len(found) != 8:
        raise ValueError("unrecognized EquipmentSlot values")
    return found


def single_conversion(code, signature):
    """The `keepEquipment` and `preserveCanPickUpLoot` booleans passed to
    `ConversionParams.single` in one method."""
    body = method(code, signature)
    match = re.search(r"iconst_([01])\n\s+\d+: iconst_([01])\n\s+\d+: invokestatic[^\n]*ConversionParams\.single", body)
    if not match:
        raise ValueError(f"unrecognized conversion: {signature}")
    return {"keep_equipment": match[1] == "1", "preserve_can_pick_up_loot": match[2] == "1"}


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-equipment-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft.world.entity." + name],
                text=True,
            )

        drop = inspect("DropChances")
        chances = {
            "default": static_constant(drop, "DEFAULT_EQUIPMENT_DROP_CHANCE"),
            "preserve_threshold": static_constant(drop, "PRESERVE_ITEM_DROP_CHANCE_THRESHOLD"),
            "guaranteed": static_constant(drop, "PRESERVE_ITEM_DROP_CHANCE"),
        }
        if "fcmpl" not in method(drop, "public boolean isPreserved("):
            raise ValueError("isPreserved no longer compares above the threshold")
        equipment_slots = slots(inspect("EquipmentSlot"))

        mob = inspect("Mob")
        death = method(mob, "protected void dropCustomDeathLoot(")
        require_sequence(death, [
            "EquipmentSlot.VALUES",
            "DropChances.byEquipment",
            "fconst_0", "fcmpl",
            "DropChances.isPreserved",
            "EnchantmentHelper.processEquipmentDropChance",
            "ItemStack.isEmpty",
            "EnchantmentEffectComponents.PREVENT_EQUIPMENT_DROP",
            "RandomSource.nextFloat", "fcmpg",
            "ItemStack.isDamageableItem",
            "ItemStack.getMaxDamage",
            "iconst_1",
            "ItemStack.getMaxDamage", "iconst_3", "isub", "iconst_1", "Math.max",
            "RandomSource.nextInt", "iadd", "RandomSource.nextInt", "isub",
            "ItemStack.setDamageValue",
            "spawnAtLocation",
            "ItemStack.EMPTY", "setItemSlot",
        ], "Mob.dropCustomDeathLoot")
        # Player kill (argument 3) or preserved (local 8) gates the roll.
        if not re.search(r"iload_3\n\s+\d+: ifne\s+\d+\n\s+\d+: iload\s+8\n\s+\d+: ifeq", death):
            raise ValueError("dropCustomDeathLoot kill gate changed")
        experience = method(mob, "protected int getBaseExperienceReward(")
        require_sequence(experience, [
            "Field xpReward", "ifle", "EquipmentSlot.VALUES", "EquipmentSlot.canIncreaseExperience",
            "getItemBySlot", "ItemStack.isEmpty", "DropChances.byEquipment", "fconst_1", "fcmpg", "ifgt",
            "iconst_1", "iconst_3", "RandomSource.nextInt", "iadd",
        ], "Mob.getBaseExperienceReward")
        living = inspect("LivingEntity")
        require_sequence(method(living, "protected void dropAllDeathLoot("), [
            "shouldDropLoot", "dropFromLootTable", "dropCustomDeathLoot", "dropEquipment", "dropExperience",
        ], "LivingEntity.dropAllDeathLoot")
        order_block = mob.split("  static {};")[1]
        order_block = order_block[:order_block.index("// Field EQUIPMENT_POPULATION_ORDER")]
        order_block = order_block[:order_block.rindex("putstatic")]
        order_block = order_block[order_block.rindex("putstatic"):]
        population = [s.lower() for s in re.findall(r"EquipmentSlot\.([A-Z]+):", order_block)]
        populate = method(mob, "protected void populateDefaultEquipmentSlots(")
        require_sequence(populate, [
            "RandomSource.nextFloat", "float 0.15f", "getSpecialMultiplier",
            "iconst_3", "RandomSource.nextInt",
            "float 3.0f", "RandomSource.nextFloat", "float 0.1087f",
            "Difficulty.HARD", "float 0.1f", "float 0.25f",
            "EQUIPMENT_POPULATION_ORDER",
            "RandomSource.nextFloat",
            "getEquipmentForSlot",
        ], "Mob.populateDefaultEquipmentSlots")
        tiers = method(mob, "public static net.minecraft.world.item.Item getEquipmentForSlot(")
        head_tiers = re.findall(r"Field net/minecraft/world/item/Items\.([A-Z_]+)_HELMET:", tiers)
        enchants = method(mob, "protected void populateDefaultEquipmentEnchantments(")
        require_sequence(enchants, ["enchantSpawnedWeapon", "EquipmentSlot.VALUES", "EquipmentSlot$Type.HUMANOID_ARMOR",
                                    "enchantSpawnedArmor"], "Mob.populateDefaultEquipmentEnchantments")
        weapon = method(mob, "protected void enchantSpawnedWeapon(")
        armor = method(mob, "protected void enchantSpawnedArmor(")
        if "EquipmentSlot.MAINHAND" not in weapon or "float 0.25f" not in weapon or "float 0.5f" not in armor:
            raise ValueError("spawn enchantment chances changed")
        spawned = method(mob, "private void enchantSpawnedEquipment(")
        require_sequence(spawned, ["RandomSource.nextFloat", "getSpecialMultiplier", "fmul",
                                   "VanillaEnchantmentProviders.MOB_SPAWN_EQUIPMENT"], "Mob.enchantSpawnedEquipment")
        spawn = {
            "max_wearing_armor_chance": static_constant(mob, "MAX_WEARING_ARMOR_CHANCE"),
            "upgrade_material_chance": static_constant(mob, "WEARING_ARMOR_UPGRADE_MATERIAL_CHANCE"),
            "upgrade_material_attempts": static_constant(mob, "WEARING_ARMOR_UPGRADE_MATERIAL_ATTEMPTS"),
            "partial_chance_hard": 0.1,
            "partial_chance_other": 0.25,
            "population_order": population,
            "armor_tiers": [t.lower() for t in head_tiers],
            "max_enchanted_armor_chance": static_constant(mob, "MAX_ENCHANTED_ARMOR_CHANCE"),
            "max_enchanted_weapon_chance": static_constant(mob, "MAX_ENCHANTED_WEAPON_CHANCE"),
        }
        drowned = method(inspect("monster.zombie.Drowned"), "protected void populateDefaultEquipmentSlots(")
        if "populateDefaultEquipmentSlots" in "".join(ops(drowned)) or "Items.TRIDENT" not in drowned:
            raise ValueError("drowned equipment override changed")
        zombie = inspect("monster.zombie.Zombie")
        zombie_populate = method(zombie, "protected void populateDefaultEquipmentSlots(")
        require_sequence(zombie_populate, [
            "Monster.populateDefaultEquipmentSlots", "RandomSource.nextFloat", "Difficulty.HARD", "float 0.05f",
            "float 0.01f", "bipush 6", "RandomSource.nextInt", "ifne", "EquipmentSlot.MAINHAND", "Items.IRON_SWORD",
            "iconst_1", "if_icmpne", "EquipmentSlot.MAINHAND", "Items.IRON_SPEAR", "EquipmentSlot.MAINHAND",
            "Items.IRON_SHOVEL",
        ], "Zombie.populateDefaultEquipmentSlots")
        zombie_weapon = {
            "chance_hard": 0.05,
            "chance_other": 0.01,
            "roll": 6,
            "by_roll": {"0": "iron_sword", "1": "iron_spear", "other": "iron_shovel"},
        }
        conversions = {
            "zombie_to_drowned_husk_to_zombie_skeleton_to_stray": single_conversion(
                inspect("ConversionTracker"), "private void doConversion("),
            "villager_to_zombie_villager": single_conversion(zombie, "public boolean convertVillagerToZombieVillager("),
            "zombie_villager_cure": single_conversion(inspect("monster.zombie.ZombieVillager"),
                                                      "private void finishConversion("),
            "villager_to_witch": single_conversion(inspect("npc.villager.Villager"), "public void thunderHit("),
            "piglin_to_zombified_piglin": single_conversion(inspect("monster.piglin.AbstractPiglin"),
                                                            "protected void finishConversion("),
            "hoglin_to_zoglin": single_conversion(inspect("monster.hoglin.Hoglin"), "private void finishConversion()"),
        }
        convert = inspect("ConversionType$1")
        require_sequence(convert, ["keepEquipment", "EquipmentSlot.VALUES", "getItemBySlot", "copyAndClear",
                                   "setItemSlot", "getDropChances", "DropChances.byEquipment", "setDropChance"],
                         "ConversionType.SINGLE")
        cure = method(inspect("monster.zombie.ZombieVillager"), "lambda$finishConversion$")
        require_sequence(cure, ["dropPreservedEquipment", "getIndex", "sipush", "getSlot"], "ZombieVillager cure")
        preserved = method(mob, "public java.util.Set<net.minecraft.world.entity.EquipmentSlot> dropPreservedEquipment(")
        require_sequence(preserved, ["Predicate.test", "DropChances.isPreserved", "setItemSlot", "spawnAtLocation"],
                         "Mob.dropPreservedEquipment")
        cure_filter = method(inspect("monster.zombie.ZombieVillager"), "lambda$finishConversion$1(")
        if "PREVENT_ARMOR_CHANGE" not in cure_filter:
            raise ValueError("cure no longer keeps binding-cursed items")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(name):
            return json.loads(jar.read(name))

        looting = read("data/minecraft/enchantment/looting.json")["effects"]["minecraft:equipment_drops"]
        if len(looting) != 1:
            raise ValueError("unrecognized looting equipment_drops")
        effect = looting[0]
        value = effect["effect"]["value"]
        if effect["effect"]["type"] != "minecraft:add" or value["type"] != "minecraft:linear":
            raise ValueError("unrecognized looting effect form")
        requirement = effect["requirements"]
        vanishing = read("data/minecraft/enchantment/vanishing_curse.json")["effects"]
        binding = read("data/minecraft/enchantment/binding_curse.json")["effects"]
        trial_chances = set()
        for name in jar.namelist():
            if name.startswith("data/minecraft/trial_spawner/") and name.endswith(".json"):
                text = jar.read(name).decode()
                for match in re.finditer(r'"slot_drop_chances":\s*([-0-9.]+)', text):
                    trial_chances.add(float(match[1]))
                if '"equipment"' in text and '"slot_drop_chances"' not in text:
                    raise ValueError(f"trial equipment without drop chances: {name}")
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.entity.EquipmentSlot static initializer",
            "net.minecraft.world.entity.DropChances constants and isPreserved",
            "net.minecraft.world.entity.Mob.dropCustomDeathLoot, dropPreservedEquipment, getBaseExperienceReward",
            "net.minecraft.world.entity.LivingEntity.dropAllDeathLoot",
            "net.minecraft.world.entity.Mob.populateDefaultEquipmentSlots, getEquipmentForSlot, "
            "EQUIPMENT_POPULATION_ORDER, populateDefaultEquipmentEnchantments",
            "net.minecraft.world.entity.monster.zombie.Drowned.populateDefaultEquipmentSlots",
            "net.minecraft.world.entity.monster.zombie.Zombie.populateDefaultEquipmentSlots",
            "net.minecraft.world.entity.ConversionType.SINGLE and each ConversionParams.single call",
            "net.minecraft.world.entity.monster.zombie.ZombieVillager.finishConversion",
            "data/minecraft/enchantment/looting.json",
            "data/minecraft/enchantment/vanishing_curse.json",
            "data/minecraft/enchantment/binding_curse.json",
            "data/minecraft/trial_spawner/**",
        ],
        "slots": equipment_slots,
        "drop_chances": chances,
        "death_drop": {
            "skip_zero_chance": True,
            "requires_player_kill_unless_preserved": True,
            "prevent_drop_effect": "minecraft:prevent_equipment_drop",
            "wear_reroll": "max_damage - nextInt(1 + nextInt(max(max_damage - 3, 1)))",
            "preserved_keep_damage": True,
            "before_experience": True,
        },
        "experience": {
            "requires_base_reward": True,
            "per_filled_slot": "1 + nextInt(3)",
            "slot_needs_drop_chance_at_most": 1.0,
        },
        "looting_equipment_drops": {
            "effect": "add",
            "base": value["base"],
            "per_level_above_first": value["per_level_above_first"],
            "enchanted": effect["enchanted"],
            "attacker_must_be": requirement["predicate"]["minecraft:entity_type"],
        },
        "curses": {
            "vanishing_curse": sorted(vanishing),
            "binding_curse": sorted(binding),
        },
        "spawn": spawn,
        "drowned_skips_armor": True,
        "zombie_weapon": zombie_weapon,
        "conversions": conversions,
        "cure_keeps_binding_cursed_at_slot_index": True,
        "trial_spawner_slot_drop_chances": sorted(trial_chances),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
