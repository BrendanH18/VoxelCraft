#!/usr/bin/env python3
"""Extract thrown-item facts from the pinned official Java 26.3 artifact.

Covers the Bottle o' Enchanting: its `Items` registration (rarity and glint
override), `ExperienceBottleItem.use` (throw sound, `spawnProjectileFromRotation`
pitch offset, power and uncertainty) and `createDispenseConfig`,
`ThrownExperienceBottle` (gravity, hit level events, the experience roll and
the orb direction), the shared `ThrowableProjectile` drag, the
`Projectile.getMovementToShoot` noise scale, the `ThrowableItemProjectile`
spawn height, the `ExperienceOrb` scatter, the dispenser defaults
(`ProjectileItem.DispenseConfig.Builder`), the cleric's master offer and the
chest-loot entries that name the bottle.

Requires javap. Unknown bytecode forms fail rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


def f32(value):
    """`value` rounded to a Java float, printed back to its shortest decimal."""
    single = struct.unpack("f", struct.pack("f", value))[0]
    return float(f"{single:.7g}")


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


def constants(code, kind):
    """Every `ldc` constant of `kind` (float, double or int), in order."""
    return [float(v) if kind != "int" else int(v) for v in re.findall(r"// " + kind + r" (-?[0-9.E-]+)[fd]?\b", code)]


def registration(code, field):
    """The `Items` static-initializer slice that registers `field`."""
    block = code.split("  static {};")[1]
    start = block.index(f"ItemIds.{field}:")
    end = block.index(f"putstatic", start)
    return block[start:end]


def entity_size(code, field):
    """`EntityType.Builder.sized(width, height)` of the entity registered as `field`."""
    block = code.split("  static {};")[1]
    start = block.index(f"EntityTypeIds.{field}:")
    end = block.index("putstatic", start)
    match = re.search(r"// float ([0-9.]+)f\n\s+\d+: ldc\w*\s+#\d+\s+// float ([0-9.]+)f\n\s+\d+: invokevirtual[^\n]*Builder\.sized",
                      block[start:end])
    if not match:
        raise ValueError(f"unrecognized size of {field}")
    return [float(match[1]), float(match[2])]


def loot_entry(jar, table, item):
    data = json.loads(jar.read(f"data/minecraft/loot_table/{table}.json"))
    found = []
    for index, pool in enumerate(data["pools"]):
        entries = pool["entries"]
        for entry in entries:
            if entry.get("name") != f"minecraft:{item}":
                continue
            if any("condition" in e for e in entries) or "conditions" in pool:
                raise ValueError(f"{table}: conditional pool not modelled")
            modifiers = entry.get("modifier", entry.get("functions", []))
            if isinstance(modifiers, dict):
                modifiers = [modifiers]
            count = {"min": 1, "max": 1}
            for modifier in modifiers:
                if modifier["type"] != "minecraft:set_count" or modifier["count"]["type"] != "minecraft:uniform":
                    raise ValueError(f"{table}: unmodelled modifier {modifier}")
                count = {"min": int(modifier["count"]["min"]), "max": int(modifier["count"]["max"])}
            rolls = pool["rolls"]
            if isinstance(rolls, dict):
                rolls = {"min": int(rolls["min"]), "max": int(rolls["max"])}
            else:
                rolls = {"min": int(rolls), "max": int(rolls)}
            found.append({
                "table": table,
                "pool": index,
                "rolls": rolls,
                "weight": entry.get("weight", 1),
                "pool_weight": sum(e.get("weight", 1) for e in entries),
                "count": count,
            })
    if len(found) != 1:
        raise ValueError(f"{table}: expected one {item} entry, found {len(found)}")
    return found[0]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-thrown-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name],
                text=True,
            )

        items = registration(inspect("world.item.Items"), "EXPERIENCE_BOTTLE")
        require_sequence(items, [
            "invokedynamic",
            "Rarity.UNCOMMON", "Item$Properties.rarity",
            "DataComponents.ENCHANTMENT_GLINT_OVERRIDE", "iconst_1", "Boolean.valueOf",
            "registerItem",
        ], "Items.EXPERIENCE_BOTTLE")
        if "stacksTo" in items:
            raise ValueError("experience bottle stack size changed")

        bottle_item = inspect("world.item.ExperienceBottleItem")
        use = method(bottle_item, "public net.minecraft.world.InteractionResult use(")
        require_sequence(use, [
            "SoundEvents.EXPERIENCE_BOTTLE_THROW", "SoundSource.NEUTRAL",
            "float 0.5f", "float 0.4f", "RandomSource.nextFloat", "float 0.4f", "fmul", "float 0.8f", "fadd", "fdiv",
            "Level.playSound",
            "float -20.0f", "float 0.7f", "fconst_1", "Projectile.spawnProjectileFromRotation",
            "Stats.ITEM_USED", "iconst_1", "ItemStack.consume",
        ], "ExperienceBottleItem.use")
        dispense = method(bottle_item, "createDispenseConfig()")
        require_sequence(dispense, [
            "DispenseConfig.DEFAULT", "DispenseConfig.uncertainty", "float 0.5f", "fmul", "Builder.uncertainty",
            "DispenseConfig.DEFAULT", "DispenseConfig.power", "float 1.25f", "fmul", "Builder.power", "Builder.build",
        ], "ExperienceBottleItem.createDispenseConfig")

        builder = inspect("world.item.ProjectileItem$DispenseConfig$Builder")
        init = method(builder, "public net.minecraft.world.item.ProjectileItem$DispenseConfig$Builder();")
        default_uncertainty, default_power = constants(init, "float")
        position = method(builder, "lambda$new$0(")
        require_sequence(position, ["double 0.7d", "dconst_0", "double 0.1d", "dconst_0",
                                    "DispenserBlock.getDispensePosition"], "DispenseConfig position")

        thrown = inspect("world.entity.projectile.throwableitemprojectile.ThrownExperienceBottle")
        gravity = constants(method(thrown, "getDefaultGravity()"), "double")
        hit = method(thrown, "protected void onHit(")
        require_sequence(hit, [
            "ThrowableItemProjectile.onHit",
            "sipush 2002", "blockPosition", "int -13083194", "levelEvent",
            "isSilent", "sipush 1053", "blockPosition", "iconst_0", "levelEvent",
            "iconst_3", "iconst_5", "RandomSource.nextInt", "iadd", "iconst_5", "RandomSource.nextInt", "iadd",
            "instanceof", "BlockHitResult.getDirection", "Direction.getUnitVec3", "HitResult.getLocation",
            "ExperienceOrb.awardWithDirection",
            "HitResult.getLocation", "getDeltaMovement", "double -1.0d", "Vec3.scale",
            "ExperienceOrb.awardWithDirection", "discard",
        ], "ThrownExperienceBottle.onHit")

        throwable = inspect("world.entity.projectile.ThrowableProjectile")
        tick = method(throwable, "public void tick()")
        require_sequence(tick, ["applyGravity", "applyInertia", "ProjectileUtil.getHitResultOnMoveVector"],
                         "ThrowableProjectile.tick")
        inertia = method(throwable, "private void applyInertia()")
        require_sequence(inertia, ["isInWater", "float 0.8f", "getAirDrag", "Vec3.scale"], "applyInertia")
        air_drag = constants(method(throwable, "protected float getAirDrag()"), "float")

        projectile = inspect("world.entity.projectile.Projectile")
        noise = sorted(set(constants(method(projectile, "public net.minecraft.world.phys.Vec3 getMovementToShoot("),
                                     "double")))
        rotation = method(projectile, "public void shootFromRotation(")
        require_sequence(rotation, ["Mth.sin", "Mth.cos", "fload", "fadd", "Mth.sin", "fneg", "Mth.cos", "Mth.cos",
                                    "shoot", "getKnownMovement", "onGround"], "Projectile.shootFromRotation")

        item_projectile = inspect("world.entity.projectile.throwableitemprojectile.ThrowableItemProjectile")
        eye = method(item_projectile, "LivingEntity, net.minecraft.world.level.Level, net.minecraft.world.item.ItemStack)")
        require_sequence(eye, ["LivingEntity.getEyeY", "double 0.10000000149011612d", "dsub", "setOwner"],
                         "ThrowableItemProjectile spawn height")

        orb = inspect("world.entity.ExperienceOrb")
        ctor = method(orb, "net.minecraft.world.phys.Vec3, net.minecraft.world.phys.Vec3, int);")
        require_sequence(ctor, [
            "nextDouble", "double 0.2d", "double 0.1d", "double 2.0d",
            "nextDouble", "double 0.2d", "double 2.0d",
            "nextDouble", "double 0.2d", "double 0.1d", "double 2.0d",
            "Vec3.dot", "dconst_0", "double -1.0d", "Vec3.scale",
            "AABB.getSize", "Vec3.normalize", "double 0.5d",
        ], "ExperienceOrb(Level, Vec3, Vec3, int)")
        award = method(orb, "public static void awardWithDirection(")
        require_sequence(award, ["getExperienceValue", "tryMergeToExisting", "\"<init>\""],
                         "ExperienceOrb.awardWithDirection")

        entity_types = inspect("world.entity.EntityTypes")

        behaviours = inspect("core.dispenser.DispenseItemBehavior")
        if "Items.EXPERIENCE_BOTTLE:Lnet/minecraft/world/item/Item;\n" not in behaviours or not re.search(
            r"Items\.EXPERIENCE_BOTTLE:[^\n]*\n\s+\d+: invokestatic[^\n]*DispenserBlock\.registerProjectileBehavior",
            behaviours,
        ):
            raise ValueError("the experience bottle is no longer a dispenser projectile")

        with zipfile.ZipFile(path) as jar:
            trade = json.loads(jar.read("data/minecraft/villager_trade/cleric/5/emerald_experience_bottle.json"))
            level = json.loads(jar.read("data/minecraft/tags/villager_trade/cleric/level_5.json"))
            loot = [loot_entry(jar, f"chests/{t}", "experience_bottle")
                    for t in ["ancient_city", "pillager_outpost", "shipwreck_treasure"]]
            tables = sorted(
                n[len("data/minecraft/loot_table/"):-len(".json")]
                for n in jar.namelist()
                if n.startswith("data/minecraft/loot_table/") and b"minecraft:experience_bottle" in jar.read(n)
            )
    if len(gravity) != 1 or len(air_drag) != 1 or len(noise) != 1:
        raise ValueError("unrecognized projectile constants")
    if sorted(trade) != ["gives", "max_uses", "reputation_discount", "wants", "xp"]:
        raise ValueError(f"unmodelled trade fields {sorted(trade)}")
    return {
        "experience_bottle": {
            "item": {"max_stack": 64, "rarity": "uncommon", "glint_override": True},
            "use": {
                "sound": "entity.experience_bottle.throw",
                "sound_source": "neutral",
                "volume": 0.5,
                "pitch": "0.4 / (nextFloat * 0.4 + 0.8)",
                "y_offset_degrees": -20.0,
                "power": f32(0.7),
                "uncertainty": 1.0,
                "consumes": 1,
            },
            "entity": {
                "size": entity_size(entity_types, "EXPERIENCE_BOTTLE"),
                "gravity": gravity[0],
                "spawn_below_eye": 0.10000000149011612,
            },
            "hit": {
                "particle_event": 2002,
                "colour": -13083194,
                "colour_rgb": [(-13083194 >> 16) & 0xFF, (-13083194 >> 8) & 0xFF, -13083194 & 0xFF],
                "sound_event": 1053,
                "xp_base": 3,
                "xp_rolls": [5, 5],
                "block_direction": "hit face normal",
                "entity_direction": "minus delta movement",
            },
            "dispense": {
                "uncertainty": f32(f32(default_uncertainty) * f32(0.5)),
                "power": f32(f32(default_power) * f32(1.25)),
                "position_scale": 0.7,
                "position_offset": [0.0, 0.1, 0.0],
            },
            "trade": {
                "id": "cleric/5/emerald_experience_bottle",
                "in_level_tag": "minecraft:cleric/5/emerald_experience_bottle" in level["values"],
                "level_offers": len(level["values"]),
                "wants": {"id": trade["wants"]["id"].removeprefix("minecraft:"), "count": trade["wants"]["count"]},
                "gives": {"id": trade["gives"]["id"].removeprefix("minecraft:"), "count": trade["gives"].get("count", 1)},
                "max_uses": trade["max_uses"],
                "xp": trade["xp"],
                "reputation_discount": trade["reputation_discount"],
            },
            "loot": loot,
            "loot_tables": tables,
        },
        "throwable": {
            "air_drag": air_drag[0],
            "water_drag": 0.8,
            "shoot_noise": noise[0],
            "gravity_before_drag": True,
        },
        "dispense_default": {"uncertainty": default_uncertainty, "power": f32(default_power)},
        "experience_orb": {
            "size": entity_size(entity_types, "EXPERIENCE_ORB"),
            "scatter": {"horizontal": 0.2, "vertical_max": 0.4},
            "offset_along_direction": "box size * 0.5",
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
