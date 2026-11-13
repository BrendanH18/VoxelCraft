#!/usr/bin/env python3
"""Extract zombie reinforcement and spawn-attribute facts from the pinned
official Java 26.3 artifact.

Covers `Zombie.hurtServer` (reinforcement calls), `Zombie.handleAttributes`
and `randomizeReinforcementsChance` (with `ZombifiedPiglin`'s override), the
caller and callee charges, `Mob.finalizeSpawn`'s random follow-range bonus,
the attribute ranges from `Attributes`, the follow ranges of
`Mob.createMobAttributes` and `Zombie.createAttributes`, which entity types
carry the `spawn_reinforcements` attribute (`DefaultAttributes`) and which
zombie classes override `canSpawnInLiquids`.

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
from extract_java_mob_equipment_fixtures import method, ops, require_sequence

ENTITY = "net.minecraft.world.entity."


def static_number(code, name):
    match = re.search(r"static final \w+ " + name + r" = ([-0-9.]+)f?;", code)
    if not match:
        raise ValueError(f"missing constant {name}")
    return float(match[1])


def literals(code):
    """Numeric constants loaded by the instructions, in order."""
    found = []
    for op in ops(code):
        match = re.match(r"(?:ldc2_w|ldc_w|ldc)\s+#\d+\s+//\s+(?:double|float) ([-0-9.Ee]+)[df]", op)
        if match:
            found.append(float(match[1]))
            continue
        match = re.match(r"([df])const_(\d)", op)
        if match:
            found.append(float(match[2]))
            continue
        match = re.match(r"(?:bipush|sipush)\s+(-?\d+)", op)
        if match:
            found.append(int(match[1]))
            continue
        match = re.match(r"iconst_(m1|\d)", op)
        if match:
            found.append(-1 if match[1] == "m1" else int(match[1]))
    return found


def ranged_attribute(attributes, name):
    """`RangedAttribute(description, default, min, max)` registered as `name`."""
    block = attributes.split("  static {};")[1]
    start = block.find(f"// String {name}\n")
    if start < 0:
        raise ValueError(f"missing attribute {name}")
    end = block.find("RangedAttribute.\"<init>\"", start)
    values = literals(block[start:end])
    if len(values) != 3:
        raise ValueError(f"unrecognized attribute {name}: {values}")
    return {"default": values[0], "min": values[1], "max": values[2]}


def add_value(code, holder):
    """The constant passed to `AttributeSupplier.Builder.add(holder, value)`."""
    match = re.search(
        r"Attributes\." + holder + r":[^\n]*\n\s+\d+: (\w+)\s*(#\d+)?\s*(?://\s*(?:double|float) ([-0-9.]+)d)?\n"
        r"\s+\d+: invokevirtual[^\n]*AttributeSupplier\$Builder\.add:\(Lnet/minecraft/core/Holder;D\)",
        code,
    )
    if not match:
        return None
    if match[1].startswith("dconst_"):
        return float(match[1][-1])
    return float(match[3])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official server-26.3.jar (the bundler)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    data = args.server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-zombie-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)
        cache = {}

        def inspect(*names):
            """javap output per fully qualified class name (batched)."""
            missing = [n for n in names if n not in cache]
            if missing:
                text = subprocess.check_output(
                    ["javap", "-classpath", str(path), "-c", "-p", "-constants", *missing], text=True
                )
                parts = re.split(r"(?m)^Compiled from [^\n]*\n", text)[1:]
                if len(parts) != len(missing):
                    raise ValueError("javap output did not split per class")
                for name, part in zip(missing, parts):
                    cache[name] = part
            return [cache[n] for n in names]

        zombie, piglin, mob, attributes, defaults = inspect(
            ENTITY + "monster.zombie.Zombie",
            ENTITY + "monster.zombie.ZombifiedPiglin",
            ENTITY + "Mob",
            ENTITY + "ai.attributes.Attributes",
            ENTITY + "ai.attributes.DefaultAttributes",
        )

        hurt = method(zombie, "public boolean hurtServer(")
        require_sequence(hurt, [
            "Monster.hurtServer", "getTarget", "DamageSource.getEntity", "instanceof", "LivingEntity",
            "Difficulty.HARD", "RandomSource.nextFloat", "Attributes.SPAWN_REINFORCEMENTS_CHANCE",
            "getAttributeValue", "dcmpg", "isSpawningMonsters",
            "Mth.floor", "Mth.floor", "Mth.floor", "getType", "EntitySpawnReason.REINFORCEMENT", "EntityType.create",
            "bipush 50",
            "bipush 7", "bipush 40", "Mth.nextInt", "iconst_m1", "iconst_1", "Mth.nextInt", "imul", "iadd",
            "bipush 7", "bipush 40", "Mth.nextInt", "iconst_m1", "iconst_1", "Mth.nextInt", "imul", "iadd",
            "bipush 7", "bipush 40", "Mth.nextInt", "iconst_m1", "iconst_1", "Mth.nextInt", "imul", "iadd",
            "SpawnPlacements.isSpawnPositionOk", "EntitySpawnReason.REINFORCEMENT", "SpawnPlacements.checkSpawnRules",
            "setPos", "double 7.0d", "hasNearbyAlivePlayer", "isUnobstructed", "noCollision",
            "canSpawnInLiquids", "containsAnyLiquid",
            "setTarget", "getCurrentDifficultyAt", "EntitySpawnReason.REINFORCEMENT", "finalizeSpawn",
            "addFreshEntityWithPassengers",
            "REINFORCEMENT_CALLER_CHARGE_ID", "AttributeModifier.amount", "dconst_0",
            "REINFORCEMENT_CALLER_CHARGE_ID", "removeModifier",
            "REINFORCEMENT_CALLER_CHARGE_ID", "double 0.05d", "dsub", "Operation.ADD_VALUE", "addPermanentModifier",
            "ZOMBIE_REINFORCEMENT_CALLEE_CHARGE", "addPermanentModifier",
        ], "Zombie.hurtServer")
        # The first successful try ends the loop (`break` to the return).
        if not re.search(r"addPermanentModifier[^\n]*\n\s+\d+: goto\s+(\d+)\n\s+\d+: iinc", hurt):
            raise ValueError("reinforcement loop no longer stops at the first spawn")
        caller = re.search(r"// double ([-0-9.]+)d\n\s+\d+: dsub", hurt)

        static = zombie.split("  static {};")[1]
        callee = re.search(
            r"// String reinforcement_callee_charge\n[^\n]*withDefaultNamespace[^\n]*\n\s+\d+: ldc2_w\s+#\d+\s+"
            r"// double ([-0-9.Ee]+)d\n\s+\d+: getstatic[^\n]*Operation\.ADD_VALUE", static)
        if not callee:
            raise ValueError("unrecognized callee charge")
        modifier_ids = re.findall(r"// String ([a-z_]+)\n[^\n]*withDefaultNamespace[^\n]*\n[^\n]*putstatic[^\n]*Field "
                                  r"([A-Z_]+_ID):", static)

        handle = method(zombie, "protected void handleAttributes(")
        require_sequence(handle, [
            "randomizeReinforcementsChance",
            "Attributes.KNOCKBACK_RESISTANCE", "RANDOM_SPAWN_BONUS_ID", "RandomSource.nextDouble", "dmul",
            "Operation.ADD_VALUE", "addOrReplacePermanentModifier",
            "RandomSource.nextDouble", "dmul", "fload_1", "f2d", "dmul", "dconst_1", "dcmpl", "ifle",
            "Attributes.FOLLOW_RANGE", "ZOMBIE_RANDOM_SPAWN_BONUS_ID", "Operation.ADD_MULTIPLIED_TOTAL",
            "addOrReplacePermanentModifier",
            "RandomSource.nextFloat", "fload_1", "fmul", "fcmpg", "ifge",
            "Attributes.SPAWN_REINFORCEMENTS_CHANCE", "LEADER_ZOMBIE_BONUS_ID", "RandomSource.nextDouble", "dmul",
            "dadd", "Operation.ADD_VALUE", "addOrReplacePermanentModifier",
            "Attributes.MAX_HEALTH", "LEADER_ZOMBIE_BONUS_ID", "RandomSource.nextDouble", "dmul", "dconst_1", "dadd",
            "Operation.ADD_MULTIPLIED_TOTAL", "addOrReplacePermanentModifier",
            "EntitySpawnReason.CONVERSION", "EntitySpawnReason.LOAD", "EntitySpawnReason.DIMENSION_TRAVEL",
            "getMaxHealth", "setHealth", "iconst_1", "setCanBreakDoors",
        ], "Zombie.handleAttributes")
        values = literals(handle)
        # knockback scale, follow scale, threshold 1, leader chance,
        # reinforcement scale and offset, health scale and offset, then the door flag.
        if len(values) != 9 or values[2] != 1.0 or values[7] != 1.0 or values[8] != 1:
            raise ValueError(f"unrecognized handleAttributes constants: {values}")
        knockback_scale, follow_scale, follow_threshold, leader_chance = values[0:4]
        leader_reinforce_scale, leader_reinforce_offset, leader_health_scale, leader_health_offset = values[4:8]
        if leader_chance != static_number(zombie, "ZOMBIE_LEADER_CHANCE"):
            raise ValueError("leader chance no longer ZOMBIE_LEADER_CHANCE")

        randomize = method(zombie, "protected void randomizeReinforcementsChance(")
        require_sequence(randomize, ["Attributes.SPAWN_REINFORCEMENTS_CHANCE", "RandomSource.nextDouble", "dmul",
                                     "setBaseValue"], "Zombie.randomizeReinforcementsChance")
        (base_scale,) = literals(randomize)
        piglin_randomize = method(piglin, "protected void randomizeReinforcementsChance(")
        if literals(piglin_randomize) != [0.0] or "RandomSource" in piglin_randomize:
            raise ValueError("unrecognized ZombifiedPiglin.randomizeReinforcementsChance")

        finalize = method(zombie, "public net.minecraft.world.entity.SpawnGroupData finalizeSpawn(")
        require_sequence(finalize, ["Monster.finalizeSpawn", "getSpecialMultiplier", "setCanBreakDoors",
                                    "handleAttributes"], "Zombie.finalizeSpawn")
        if not re.search(r"getSpecialMultiplier[\s\S]*RandomSource\.nextFloat[^\n]*\n\s+\d+: fload\s+\d+\n\s+\d+: "
                         r"ldc(?:_w)?\s+#\d+\s+// float 0\.1f\n\s+\d+: fmul\n[\s\S]*setCanBreakDoors", finalize):
            raise ValueError("unrecognized door-breaking roll")
        door_chance = static_number(zombie, "BREAK_DOOR_CHANCE")

        mob_finalize = method(mob, "public net.minecraft.world.entity.SpawnGroupData finalizeSpawn(")
        require_sequence(mob_finalize, [
            "Attributes.FOLLOW_RANGE", "RANDOM_SPAWN_BONUS_ID", "hasModifier", "ifne",
            "RANDOM_SPAWN_BONUS_ID", "dconst_0", "RandomSource.triangle:(DD)D", "Operation.ADD_MULTIPLIED_BASE",
            "addPermanentModifier",
        ], "Mob.finalizeSpawn")
        triangle = literals(mob_finalize[:mob_finalize.index("RandomSource.triangle")])
        if len(triangle) != 2:
            raise ValueError(f"unrecognized follow-range triangle: {triangle}")
        random_spawn_id = re.search(r"// String ([a-z_]+)\n[^\n]*withDefaultNamespace[^\n]*\n[^\n]*putstatic[^\n]*"
                                    r"Field RANDOM_SPAWN_BONUS_ID:", mob.split("  static {};")[1])

        mob_follow = add_value(method(mob, "createMobAttributes("), "FOLLOW_RANGE")
        zombie_create = method(zombie, "public static net.minecraft.world.entity.ai.attributes."
                                       "AttributeSupplier$Builder createAttributes(")
        zombie_follow = add_value(zombie_create, "FOLLOW_RANGE")
        piglin_create = method(piglin, "public static net.minecraft.world.entity.ai.attributes."
                                       "AttributeSupplier$Builder createAttributes(")
        if "Zombie.createAttributes" not in piglin_create:
            raise ValueError("zombified piglin attributes no longer extend the zombie's")

        # Every entity type whose attribute supplier carries SPAWN_REINFORCEMENTS_CHANCE,
        # following `createAttributes` chains through parent classes.
        registered = re.findall(
            r"Field net/minecraft/world/entity/EntityTypes\.([A-Z_]+):[^\n]*\n\s+\d+: invokestatic\s+#\d+\s+"
            r"// Method ([\w/$]+)\.(\w+):\(\)Lnet/minecraft/world/entity/ai/attributes/AttributeSupplier\$Builder;",
            defaults.split("  static {};")[1],
        )
        if len(registered) < 80:
            raise ValueError("unrecognized DefaultAttributes table")

        def supplier(owner, name, depth=0):
            """(has spawn_reinforcements, its base value) along the chain."""
            if depth > 8:
                raise ValueError("attribute chain too deep")
            (code,) = inspect(owner.replace("/", "."))
            signature = f"static net.minecraft.world.entity.ai.attributes.AttributeSupplier$Builder {name}("
            if signature not in code:
                # Called through a subclass: the method is declared further up.
                parent_class = re.search(r"class [\w.$]+ extends ([\w.$]+)", code)
                if not parent_class:
                    raise ValueError(f"missing {owner}.{name}")
                return supplier(parent_class[1], name, depth + 1)
            body = method(code, signature)
            here = "Attributes.SPAWN_REINFORCEMENTS_CHANCE" in body
            base = add_value(body, "SPAWN_REINFORCEMENTS_CHANCE")
            parent = re.search(r"invokestatic\s+#\d+\s+// Method ([\w/$]+)\.(\w+):\(\)Lnet/minecraft/world/entity/"
                               r"ai/attributes/AttributeSupplier\$Builder;", body)
            if parent:
                inherited, inherited_base = supplier(parent[1], parent[2], depth + 1)
                here = here or inherited
                base = base if base is not None else inherited_base
            return here, base

        owners = sorted({o.replace("/", ".") for _, o, _ in registered})
        inspect(*owners)
        carriers = {}
        for field, owner, name in registered:
            has, base = supplier(owner, name)
            if has:
                carriers[field.lower()] = base if base is not None else 0.0

        liquids = []
        for cls in ["Zombie", "Husk", "Drowned", "ZombieVillager", "ZombifiedPiglin"]:
            (code,) = inspect(ENTITY + "monster.zombie." + cls)
            header = code.split("{", 1)[0]
            if cls != "Zombie" and "extends net.minecraft.world.entity.monster.zombie.Zombie" not in header:
                raise ValueError(f"{cls} no longer extends Zombie")
            for override in ["hurtServer(", "handleAttributes("]:
                if cls != "Zombie" and re.search(r"\n  \w[^\n]* " + re.escape(override), code):
                    raise ValueError(f"{cls} overrides {override}")
            if "boolean canSpawnInLiquids();" in code:
                body = method(code, "protected boolean canSpawnInLiquids(")
                if literals(body) == [1]:
                    liquids.append(re.sub(r"(?<!^)(?=[A-Z])", "_", cls).lower())

    fixture = {
        "source": "Minecraft Java Edition 26.3 server bytecode (javap); see tools/extract_java_zombie_attribute_fixtures.py",
        "attributes": {name: ranged_attribute(attributes, name)
                       for name in ["spawn_reinforcements", "max_health", "follow_range", "knockback_resistance"]},
        "follow_range": {"mob": mob_follow, "zombie": zombie_follow},
        "spawn_reinforcements_carriers": dict(sorted(carriers.items())),
        "modifier_ids": dict(sorted((field.lower(), ident) for ident, field in modifier_ids)),
        "random_spawn_bonus": {
            "id": random_spawn_id[1] if random_spawn_id else None,
            "attribute": "follow_range",
            "operation": "add_multiplied_base",
            "triangle_mean": triangle[0],
            "triangle_spread": triangle[1],
            "only_when_absent": True,
        },
        "handle_attributes": {
            "reinforcement_base_scale": base_scale,
            "zombified_piglin_reinforcement_base": 0.0,
            "knockback_bonus_scale": knockback_scale,
            "knockback_operation": "add_value",
            "follow_bonus_scale": follow_scale,
            "follow_bonus_threshold": follow_threshold,
            "follow_operation": "add_multiplied_total",
            "leader_chance": leader_chance,
            "leader_reinforcement_scale": leader_reinforce_scale,
            "leader_reinforcement_offset": leader_reinforce_offset,
            "leader_reinforcement_operation": "add_value",
            "leader_health_scale": leader_health_scale,
            "leader_health_offset": leader_health_offset,
            "leader_health_operation": "add_multiplied_total",
            "leader_heals_unless": ["conversion", "load", "dimension_travel"],
            "leader_breaks_doors": True,
            "break_door_chance": door_chance,
        },
        "reinforcements": {
            "difficulty": "hard",
            "needs_spawning_monsters": True,
            "attempts": int(static_number(zombie, "REINFORCEMENT_ATTEMPTS")),
            "range_min": int(static_number(zombie, "REINFORCEMENT_RANGE_MIN")),
            "range_max": int(static_number(zombie, "REINFORCEMENT_RANGE_MAX")),
            "sign_min": -1,
            "sign_max": 1,
            "player_clearance": 7.0,
            "spawn_reason": "reinforcement",
            "caller_charge": -float(caller[1]),
            "callee_charge": float(callee[1]),
            "charge_operation": "add_value",
            "can_spawn_in_liquids": liquids,
        },
    }
    args.output.write_text(json.dumps(fixture, indent=1) + "\n")


if __name__ == "__main__":
    main()
