#!/usr/bin/env python3
"""Extract the v0.16 Track C facts from the pinned official Java 26.3 server.

Covers the Wither (`WitherBoss`: size, attributes, its charge-up, heads,
skulls and targeting), its skulls (`WitherSkull` on
`AbstractHurtingProjectile`), the wither rose, mob heads (`Items` rarities
and `loweredMobVisibility`, `Blocks` strengths, note block instruments) and
beacons (`BeaconBlockEntity`'s powers, levels, cadence and ranges), with the
data files they read: tags, recipes and loot tables.

Requires javap. Each number is read from a named place in the bytecode
(the constant pushed before a known call or field), and an unrecognized
form fails rather than regenerating a guessed fixture. No VoxelCraft source
or existing fixture is consulted.
"""

import argparse
import hashlib
import io
import json
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1
from extract_java_new_mob_fixtures import attributes, constants, method, number, ops, strip

HEADS = [
    "SKELETON_SKULL",
    "WITHER_SKELETON_SKULL",
    "PLAYER_HEAD",
    "ZOMBIE_HEAD",
    "CREEPER_HEAD",
    "DRAGON_HEAD",
    "PIGLIN_HEAD",
]

DATA = {
    "loot_tables": [
        "entities/wither",
        "entities/wither_skeleton",
        "charged_creeper/root",
        "charged_creeper/creeper",
        "charged_creeper/piglin",
        "charged_creeper/skeleton",
        "charged_creeper/wither_skeleton",
        "charged_creeper/zombie",
        "blocks/beacon",
        "blocks/wither_rose",
        "blocks/potted_wither_rose",
        "blocks/skeleton_skull",
        "blocks/wither_skeleton_skull",
        "blocks/player_head",
        "blocks/zombie_head",
        "blocks/creeper_head",
        "blocks/dragon_head",
        "blocks/piglin_head",
    ],
    "recipes": ["beacon", "creeper_banner_pattern"],
    "item_tags": ["beacon_payment_items", "noteblock_top_instruments", "skulls"],
    "block_tags": [
        "beacon_base_blocks",
        "wither_summon_base_blocks",
        "wither_immune",
        "supports_wither_rose",
        "supports_vegetation",
    ],
    "entity_tags": ["wither_friends", "undead"],
    "damage_tags": ["wither_immune_to"],
}


def numbers(listing):
    return [n for n in (number(op) for op in listing) if n is not None]


def before(listing, pattern, count=1, kind=None, window=8):
    """The last `count` constants (of `kind`, if given) pushed within
    `window` ops before the first op matching `pattern`."""
    for i, op in enumerate(listing):
        if re.search(pattern, op):
            found = [n for n in numbers(listing[max(0, i - window) : i]) if kind is None or isinstance(n, kind)]
            if len(found) < count:
                raise ValueError(f"expected {count} constants before {pattern}: {listing[max(0, i - window) : i + 1]}")
            return found[-count:]
    raise ValueError(f"missing op {pattern}")


def entity_type(listing, field):
    start = next(i for i, op in enumerate(listing) if f"EntityTypeIds.{field}:" in op)
    end = next(i for i in range(start, len(listing)) if "putstatic" in listing[i] and f"Field {field}:" in listing[i])
    part = listing[start:end]
    sized = next(i for i, op in enumerate(part) if "sized:" in op)
    category = next(re.search(r"MobCategory\.(\w+):", op)[1] for op in part if "MobCategory." in op)
    return {
        "width": float(number(part[sized - 2])),
        "height": float(number(part[sized - 1])),
        "category": category.lower(),
        "fire_immune": any("fireImmune" in op for op in part),
    }


def item_facts(listing, field):
    start = next(i for i, op in enumerate(listing) if re.search(rf"(BlockItemIds|ItemIds)\.{field}:", op))
    end = next(i for i in range(start, len(listing)) if "putstatic" in listing[i] and f"Field {field}:" in listing[i])
    part = listing[start:end]
    rarity = next((re.search(r"Rarity\.(\w+):", op)[1].lower() for op in part if "Rarity." in op), "common")
    hidden = [m[1].lower() for op in part if (m := re.search(r"EntityTypeIds\.(\w+):", op))]
    return {
        "rarity": rarity,
        "lowered_mob_visibility": hidden if any("loweredMobVisibility" in op for op in part) else None,
        "equips_head": any("EquipmentSlot.HEAD" in op for op in part),
        "glint": any("ENCHANTMENT_GLINT_OVERRIDE" in op for op in part),
        "damage_resistant": any("DAMAGE_RESISTANT" in op for op in part),
    }


def block_strength(listing, field, id_kind):
    start = next(i for i, op in enumerate(listing) if re.search(rf"{id_kind}\.{field}:", op))
    end = next(i for i in range(start, len(listing)) if "putstatic" in listing[i] and f"Field {field}:" in listing[i])
    part = listing[start:end]
    if any("instabreak" in op for op in part):
        return 0.0
    return float(before(part, r"strength:\(F\)", kind=float)[0])


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-wither-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        boss = inspect("world.entity.boss.wither.WitherBoss")
        server_step = ops(method(boss, "customServerAiStep("))
        heals = [i for i, op in enumerate(server_step) if "heal:(F)V" in op]
        wither = {
            "constants": constants(boss),
            "attributes": attributes(boss),
            "xp_reward": before(
                ops(method(boss, "WitherBoss(net.minecraft.world.entity.EntityType")), r"putfield .*xpReward"
            )[0],
            "ranged_attack_goal": before(ops(method(boss, "registerGoals()")), r"RangedAttackGoal.\"<init>\"", 3),
            "spawn_explosion_power": before(server_step, r"explode:", kind=float)[0],
            "spawn_level_event": before(server_step, r"globalLevelEvent:", count=2, kind=int)[0],
            "charge_heal": float(number(server_step[heals[0] - 1])),
            "heal": float(number(server_step[heals[1] - 1])),
            "head_attack_range_sqr": next(n for n in numbers(server_step) if n == 900.0),
            "head_targeting_box": before(server_step, r"AABB.inflate:", count=3, kind=float),
            "invulnerable_health_divisor": before(ops(method(boss, "makeInvulnerable()")), r"fdiv", kind=float)[0],
            "head_y": numbers(ops(method(boss, "getHeadY(")))[:2],
            "head_offset": [n for n in numbers(ops(method(boss, "getHeadX("))) if isinstance(n, float) and n > 1][0],
            "dangerous_chance": [
                n
                for n in numbers(ops(method(boss, "performRangedAttack(int, net.minecraft.world.entity.LivingEntity)")))
                if isinstance(n, float) and 0 < n < 0.01
            ][0],
            "targeting_range": before(ops(method(boss, "static {}")), r"TargetingConditions.range:", kind=float)[0],
            "powered_divisor": before(ops(method(boss, "isPowered()")), r"fdiv", kind=float)[0],
        }
        skull = inspect("world.entity.projectile.hurtingprojectile.WitherSkull")
        hit = ops(method(skull, "onHitEntity("))
        hurting = inspect("world.entity.projectile.hurtingprojectile.AbstractHurtingProjectile")
        skulls = {
            "owner_damage": before(hit, r"hurtServer:", kind=float)[0],
            "ownerless_damage": [n for n in numbers(hit) if n == 5.0][0],
            "kill_heal": before(hit, r"heal:\(F\)V", kind=float)[0],
            "wither_seconds": [n for n in numbers(hit) if n in (10, 40)],
            "wither_amplifier": before(hit, r"MobEffectInstance.\"<init>\"")[-1],
            "dangerous_inertia": [n for n in numbers(ops(method(skull, "getInertia()"))) if n != 0][0],
            "explosion_power": before(ops(method(skull, "onHit(")), r"explode:", kind=float)[0],
            "dangerous_resistance_cap": [
                n for n in numbers(ops(method(skull, "getBlockExplosionResistance("))) if n > 0
            ][0],
            "acceleration_power": constants(hurting)["INITAL_ACCELERATION_POWER"],
            "inertia": numbers(ops(method(hurting, "getInertia()")))[0],
            "liquid_inertia": numbers(ops(method(hurting, "getLiquidInertia()")))[0],
        }
        rose = inspect("world.level.block.WitherRoseBlock")
        beacon_entity = inspect("world.level.block.entity.BeaconBlockEntity")
        effects_init = ops(method(beacon_entity, "static {}"))
        tiers, tier = [], []
        for op in effects_init:
            if "putstatic" in op and "BEACON_EFFECTS" in op:
                break
            m = re.search(r"MobEffects\.(\w+):", op)
            if m:
                tier.append(m[1].lower())
            elif "java/util/List.of:" in op and tier:
                tiers.append(tier)
                tier = []
        apply = ops(method(beacon_entity, "applyEffects("))
        beacon = {
            "constants": constants(beacon_entity),
            "powers": tiers,
            "update_interval": next(n for n in numbers(ops(method(beacon_entity, "tick("))) if n == 80),
            "apply_effects_numbers": numbers(apply),
            "payment_slot_max": numbers(
                ops(method(inspect("world.inventory.BeaconMenu$PaymentSlot"), "getMaxStackSize()"))
            )[0],
        }
        item_ops = ops(method(inspect("world.item.Items"), "static {}"))
        items = {h.lower(): item_facts(item_ops, h) for h in HEADS + ["NETHER_STAR", "BEACON", "WITHER_ROSE"]}
        block_ops = ops(method(inspect("world.level.block.Blocks"), "static {}"))
        strengths = {h.lower(): block_strength(block_ops, h, "BlockItemIds") for h in HEADS + ["BEACON", "WITHER_ROSE"]}
        instruments = re.findall(
            r"^\s+public static final .*NoteBlockInstrument (\w+);",
            inspect("world.level.block.state.properties.NoteBlockInstrument"),
            re.M,
        )
        types = ops(inspect("world.entity.EntityTypes"))
        entity_types = {"wither": entity_type(types, "WITHER"), "wither_skull": entity_type(types, "WITHER_SKULL")}

    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(name):
            return json.loads(jar.read(name))

        def tag(kind, name):
            return [strip(v) for v in read(f"data/minecraft/tags/{kind}/{name}.json")["values"]]

        loot = {name: read(f"data/minecraft/loot_table/{name}.json") for name in DATA["loot_tables"]}
        for table in loot.values():
            table.pop("random_sequence", None)
        recipes = {name: read(f"data/minecraft/recipe/{name}.json") for name in DATA["recipes"]}
        tags = {
            "item": {name: tag("item", name) for name in DATA["item_tags"]},
            "block": {name: tag("block", name) for name in DATA["block_tags"]},
            "entity_type": {name: tag("entity_type", name) for name in DATA["entity_tags"]},
            "damage_type": {name: tag("damage_type", name) for name in DATA["damage_tags"]},
        }
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.entity.EntityTypes (WITHER, WITHER_SKULL)",
            "WitherBoss (createAttributes, constructor, registerGoals, customServerAiStep, makeInvulnerable, "
            "getHeadX/Y, performRangedAttack, isPowered, static init)",
            "WitherSkull (onHitEntity, onHit, getInertia, getBlockExplosionResistance), AbstractHurtingProjectile",
            "WitherRoseBlock.getBeeInteractionEffect",
            "BeaconBlockEntity (constants, BEACON_EFFECTS, tick, applyEffects), BeaconMenu.PaymentSlot",
            "Items and Blocks static init (head, Nether star, beacon and wither rose properties)",
            "NoteBlockInstrument",
            "data/minecraft/loot_table, recipe, tags",
        ],
        "entity_types": entity_types,
        "wither": wither,
        "wither_skull": skulls,
        "wither_rose_contact_ticks": before(
            ops(method(rose, "getBeeInteractionEffect()")), r"MobEffectInstance.\"<init>\""
        )[0],
        "beacon": beacon,
        "items": items,
        "block_strength": strengths,
        "note_block_instruments": [i.lower() for i in instruments],
        "loot_tables": loot,
        "recipes": recipes,
        "tags": tags,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
