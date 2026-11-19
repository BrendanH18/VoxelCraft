#!/usr/bin/env python3
"""Extract the illagers' facts from the pinned official Java 26.3 artifact.

Reads `Vindicator`, `Evoker` (and its spell goals), `Illusioner` (and its
spell goals), `Pillager`, `Vex` (and its goals and move control), `Ravager`,
`EvokerFangs`, `SpellcasterIllager`, `RangedCrossbowAttackGoal`,
`CrossbowItem`, the shared `Mob`, `Monster`, `Zombie` and `AbstractSkeleton`
constants the shared mob combat uses, the `EntityTypes` builders, the
`entities/*` loot tables and the illager entity type tags.

Numbers inside method bodies are matched against a template of the method's
literal values in order: fixed entries anchor the bytecode's shape and `$name`
entries capture the values the fixture records. A changed shape fails rather
than regenerating a guessed fixture. Requires javap. No VoxelCraft source or
existing fixture is consulted.
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
from extract_java_small_mob_fixtures import attributes, goals, literal, method, ops, tag

MOBS = {
    "vindicator": ("monster.illager.Vindicator", "VINDICATOR"),
    "evoker": ("monster.illager.Evoker", "EVOKER"),
    "illusioner": ("monster.illager.Illusioner", "ILLUSIONER"),
    "pillager": ("monster.illager.Pillager", "PILLAGER"),
    "vex": ("monster.Vex", "VEX"),
    "ravager": ("monster.Ravager", "RAVAGER"),
}


def literals(code, signature):
    """The values a method pushes, in order."""
    return [v for v in (literal(op) for op in ops(method(code, signature))) if v is not None]


def capture(code, signature, template):
    """Match `signature`'s literals against `template`; return the `$name` slots."""
    found = literals(code, signature)
    if len(found) != len(template):
        raise ValueError(f"unrecognized {signature}: {found}")
    out = {}
    for value, want in zip(found, template):
        if isinstance(want, str) and want.startswith("$"):
            out[want[1:]] = value
        elif isinstance(want, float) and isinstance(value, float):
            if abs(value - want) > 1e-6 * max(1.0, abs(want)):
                raise ValueError(f"unrecognized {signature}: {found}")
        elif value != want:
            raise ValueError(f"unrecognized {signature}: {found}")
    return out


def returns(code, signature):
    """The constant a one-instruction getter returns."""
    found = literals(code, signature)
    if len(found) != 1:
        raise ValueError(f"unrecognized {signature}")
    return found[0]


def entity_type(code, field):
    """The `EntityType.Builder` calls registering `EntityTypes.<field>`;
    the eye height is Java's default `0.85 * height` when not set."""
    block = code.split("static {};")[1]
    start = block.find(f"EntityTypeIds.{field}:")
    end = block.find("putstatic", start)
    if start < 0 or end < 0 or f"Field {field}:" not in block[end : block.find(chr(10), end)]:
        raise ValueError(f"missing EntityTypes.{field}")
    listing = ops(block[block.rfind(chr(10), 0, start) + 1 : end])
    out = {"fire_immune": False, "allowed_in_peaceful": True, "no_loot_table": False}
    for i, op in enumerate(listing):
        match = re.search(r"MobCategory\.(\w+):", op)
        if match:
            out["category"] = match[1].lower()
        if "Builder.sized:(FF)" in op:
            out["width"], out["height"] = literal(listing[i - 2]), literal(listing[i - 1])
        if "Builder.eyeHeight:(F)" in op:
            out["eye_height"] = literal(listing[i - 1])
        if "Builder.notInPeaceful:" in op:
            out["allowed_in_peaceful"] = False
        if "Builder.fireImmune:" in op:
            out["fire_immune"] = True
        if "Builder.noLootTable:" in op:
            out["no_loot_table"] = True
        if "Builder.ridingOffset:(F)" in op:
            out["riding_offset"] = literal(listing[i - 1])
    if "category" not in out or "width" not in out:
        raise ValueError(f"unrecognized EntityTypes.{field} builder")
    out.setdefault("eye_height", None)
    return out


def experience(code, cls):
    """`xpReward` set in the constructor, or `Monster`'s 5 when not."""
    init = method(code, f"public net.minecraft.world.entity.{cls}(")
    match = re.search(r"(iconst_\d|bipush\s+\d+)\n\s+\d+: putfield\s+#\d+\s+// Field xpReward:I", init)
    return literal(match[1]) if match else None


def string_constant(code, value):
    if f"// String {value}" not in code:
        raise ValueError(f"missing string {value}")
    return value


def spells(inspect):
    base = inspect("world.entity.monster.illager.SpellcasterIllager$SpellcasterUseSpellGoal")
    out = {"default_warmup": returns(base, "protected int getCastWarmupTime()")}
    for key, cls in [
        ("evoker_fangs", "Evoker$EvokerAttackSpellGoal"),
        ("evoker_summon", "Evoker$EvokerSummonSpellGoal"),
        ("evoker_wololo", "Evoker$EvokerWololoSpellGoal"),
        ("illusioner_mirror", "Illusioner$IllusionerMirrorSpellGoal"),
        ("illusioner_blindness", "Illusioner$IllusionerBlindnessSpellGoal"),
    ]:
        code = inspect("world.entity.monster.illager." + cls)
        spell = {
            "casting_time": returns(code, "protected int getCastingTime()"),
            "interval": returns(code, "protected int getCastingInterval()"),
        }
        if "protected int getCastWarmupTime()" in code:
            spell["warmup"] = returns(code, "protected int getCastWarmupTime()")
        else:
            spell["warmup"] = out["default_warmup"]
        match = re.search(
            r"getSpell\(\);\n\s+Code:\n\s+\d+: getstatic\s+#\d+\s+// Field [\w/$]+IllagerSpell\.(\w+):", code
        )
        if not match:
            raise ValueError(f"unrecognized {cls}.getSpell")
        spell["spell"] = match[1].lower()
        out[key] = spell
        if key == "evoker_fangs":
            out[key].update(
                capture(
                    code,
                    "protected void performSpellCasting()",
                    [
                        1.0,
                        "$close_distance_sqr",
                        0,
                        "$inner_count",
                        3.1415927,
                        "$inner_step_pi",
                        "$inner_radius",
                        "$inner_radius_z",
                        0,
                        0,
                        "$outer_count",
                        3.1415927,
                        2.0,
                        8.0,
                        "$outer_offset",
                        "$outer_radius",
                        "$outer_radius_z",
                        "$outer_delay",
                        0,
                        "$line_count",
                        "$line_spacing",
                        1,
                        1,
                    ],
                )
            )
        if key == "evoker_summon":
            out[key].update(
                capture(code, "public boolean canUse()", [0, "class:Vex", "$count_range", "$roll", 1, 1, 0])
            )
            out[key].update(
                capture(
                    code,
                    "protected void performSpellCasting()",
                    [
                        0,
                        "$vexes",
                        "$offset_min",
                        "$offset_span",
                        "$offset_y",
                        -2,
                        5,
                        0.0,
                        0.0,
                        "$ticks_per_second",
                        "$life_min_seconds",
                        "$life_spread_seconds",
                    ],
                )
            )
        if key == "evoker_wololo":
            out[key].update(
                capture(
                    code,
                    "public boolean canUse()",
                    [0, 0, 0, 0, "class:Sheep", "$box_horizontal", "$box_vertical", 16.0, 0, 1],
                )
            )
            out[key]["range"] = returns(code, "EvokerWololoSpellGoal(net")
            if "DyeColor.BLUE:" not in code or "DyeColor.RED:" not in code or "MOB_GRIEFING:" not in code:
                raise ValueError("unrecognized wololo colours or game rule")
            out[key]["from"], out[key]["to"], out[key]["needs_mob_griefing"] = "blue", "red", True
        if key == "illusioner_mirror":
            out[key]["invisibility_ticks"] = returns(code, "protected void performSpellCasting()")
        if key == "illusioner_blindness":
            out[key]["blindness_ticks"] = returns(code, "protected void performSpellCasting()")
            if "Difficulty.NORMAL:" not in code or "isHarderThan:" not in code:
                raise ValueError("unrecognized blindness difficulty check")
            out[key]["harder_than"] = "normal"
    return out


def fangs(inspect, types):
    code = inspect("world.entity.projectile.EvokerFangs")
    tick = capture(
        code,
        "public void tick()",
        [
            1,
            "$particle_tick",
            0,
            12,
            2.0,
            1.0,
            0.5,
            0.05,
            2.0,
            1.0,
            0.5,
            2.0,
            1.0,
            0.3,
            0.3,
            0.3,
            2.0,
            1.0,
            0.3,
            1.0,
            1,
            "$attack_at_warmup",
            "class:LivingEntity",
            "$inflate_x",
            0.0,
            "$inflate_z",
            4,
            1,
            1,
        ],
    )
    damage = capture(code, "private void dealDamageTo(", ["$damage", "$damage_owned"])
    if damage["damage"] != damage["damage_owned"]:
        raise ValueError("fang damage differs by owner")
    life = capture(code, "EvokerFangs(net.minecraft.world.entity.EntityType", [0, "$life_ticks"])
    if "isAlliedTo:" not in code or "indirectMagic:" not in code:
        raise ValueError("unrecognized fang ally rule")
    return {
        "entity_type": types("EVOKER_FANGS"),
        "life_ticks": life["life_ticks"],
        "attack_at_warmup": tick["attack_at_warmup"],
        "inflate": [tick["inflate_x"], 0.0, tick["inflate_z"]],
        "damage": damage["damage"],
        "spares_owner_allies": True,
        "attack_duration": int(re.search(r"ATTACK_DURATION = (\d+);", code)[1]),
    }


def vex(inspect):
    code = inspect("world.entity.monster.Vex")
    life = capture(code, "public void tick()", [1, 0, 1, 1, "$starve_interval", "$starve_damage"])
    charge_can = capture(
        inspect("world.entity.monster.Vex$VexChargeAttackGoal"),
        "public boolean canUse()",
        ["$roll", "$min_distance_sqr", 1, 0, 0],
    )
    charge_tick = capture(
        inspect("world.entity.monster.Vex$VexChargeAttackGoal"), "public void tick()", [0, "$reaim_distance_sqr", 1.0]
    )
    wander = capture(
        inspect("world.entity.monster.Vex$VexRandomMoveGoal"),
        "public void tick()",
        [
            0,
            "$attempts",
            "$span_xz",
            "$offset_xz",
            "$span_y",
            "$offset_y",
            15,
            7,
            0.5,
            0.5,
            0.5,
            "$speed",
            0.5,
            0.5,
            0.5,
            180.0,
            20.0,
        ],
    )
    wander["roll"] = capture(
        inspect("world.entity.monster.Vex$VexRandomMoveGoal"), "public boolean canUse()", ["$roll", 1, 0]
    )["roll"]
    move = capture(
        inspect("world.entity.monster.Vex$VexMoveControl"),
        "public void tick()",
        ["$arrive_damping", "$acceleration", 57.295776, 57.295776],
    )
    if "Items.IRON_SWORD:" not in code or "setDropChance" not in code:
        raise ValueError("unrecognized vex equipment")
    return {
        "starve_interval": life["starve_interval"],
        "starve_damage": life["starve_damage"],
        "charge": {**charge_can, **charge_tick},
        "wander": wander,
        "move": move,
        "main_hand": "iron_sword",
        "main_hand_drop_chance": 0.0,
        "save_keys": [k for k in ("bound_pos", "life_ticks", "owner") if f"// String {k}" in code],
    }


def ravager(inspect):
    code = inspect("world.entity.monster.Ravager")
    ai = capture(
        code,
        "public void aiStep()",
        [
            0.0,
            "$attack_speed",
            "$base_speed",
            "$speed_lerp",
            0,
            "$grief_inflate",
            1,
            1,
            0,
            1,
            "$roar_at",
            1,
            1,
            1.0,
            1.0,
            "$roar_ticks",
        ],
    )
    blocked = capture(code, "protected void blockedByItem(", ["$stun_chance", "$stun_ticks", 1.0, 1.0, 39, 1])
    roar = capture(code, "private void roar()", ["class:LivingEntity", "$roar_radius", "$roar_damage", 69])
    push = capture(code, "private void strongKnockback(", ["$min_distance_sqr", "$push", "$push_up", 4.0])
    hit = capture(code, "public boolean doHurtTarget(", ["$attack_ticks", 4, 1.0, 1.0])
    box = capture(code, "protected net.minecraft.world.phys.AABB getAttackBoundingBox(", ["$deflate", 0.0, 0.05])
    for anchor in ("LeavesBlock", "MOB_GRIEFING:", "AbstractIllager", "ARMOR_STAND:", "jumpFromGround"):
        if anchor not in code:
            raise ValueError(f"unrecognized ravager: missing {anchor}")
    crop = inspect("world.level.block.CropBlock")
    if "Ravager" not in method(crop, "protected void entityInside(") or "destroyBlock:" not in crop:
        raise ValueError("unrecognized crop trampling by ravagers")
    return {
        **ai,
        **blocked,
        **roar,
        "knockback": push,
        "attack_ticks": hit["attack_ticks"],
        "attack_box_deflate": box["deflate"],
        "roar_spares": "illagers take no damage; players get no server push",
        "griefs": ["leaves", "crops"],
        "save_keys": [k for k in ("AttackTick", "StunTick", "RoarTick") if f"// String {k}" in code],
    }


def crossbow(inspect):
    goal = inspect("world.entity.ai.goal.RangedCrossbowAttackGoal")
    tick = capture(
        goal,
        "public void tick()",
        [
            1,
            0,
            0,
            1,
            1,
            "$see_time_min",
            1,
            0,
            1,
            "$walk_while_loaded",
            0,
            30.0,
            30.0,
            1,
            "$attack_delay_base",
            "$attack_delay_spread",
            0,
            1,
            1.0,
        ],
    )
    delay = capture(goal, "static {}", ["$path_delay_min_seconds", "$path_delay_max_seconds"])
    item = inspect("world.item.CrossbowItem")
    charge = float(re.search(r"MAX_CHARGE_DURATION = ([\d.]+)f;", item)[1])
    power = float(re.search(r"MOB_ARROW_POWER = ([\d.]+)f;", item)[1])
    pillager = inspect("world.entity.monster.illager.Pillager")
    radius = [g for g in goals(pillager) if g["goal"] == "RangedCrossbowAttackGoal"]
    if len(radius) != 1 or "performCrossbowAttack" not in pillager:
        raise ValueError("unrecognized pillager crossbow goal")
    return {
        **tick,
        **delay,
        "charge_seconds": charge,
        "mob_arrow_power": power,
        "speed_modifier": radius[0]["args"][0],
        "attack_radius": radius[0]["args"][1],
    }


def mob_combat(inspect):
    mob = inspect("world.entity.Mob")
    reach = capture(mob, "static {}", ["class:Mob", "$px", "$py", "$pz", "$reach_sqrt", "$reach_minus"])
    replace = capture(
        mob, "public net.minecraft.world.item.ItemStack equipItemIfPossible(", ["$replace_drop_offset", 0.0, 1]
    )
    zombie = capture(
        inspect("world.entity.monster.zombie.Zombie"),
        "public net.minecraft.world.entity.SpawnGroupData finalizeSpawn(",
        [
            "$loot_chance",
            1,
            0,
            1,
            1,
            0.05,
            "class:Chicken",
            5.0,
            3.0,
            5.0,
            0,
            1,
            0,
            0,
            0.05,
            0.0,
            1,
            0,
            0,
            0.1,
            1,
            0,
            0.25,
            0.1,
            0.0,
        ],
    )
    skeleton = capture(
        inspect("world.entity.monster.skeleton.AbstractSkeleton"),
        "public net.minecraft.world.entity.SpawnGroupData finalizeSpawn(",
        ["$loot_chance", 1, 0, 0.25, 0.1, 0.0],
    )
    if "Field ITEM_PICKUP_REACH:" not in method(mob, "static {}"):
        raise ValueError("unrecognized pickup reach")
    for anchor in ("CanPickUpLoot", "MOB_GRIEFING:", "hasPickUpDelay:", "setGuaranteedDrop"):
        if anchor not in mob:
            raise ValueError(f"unrecognized Mob: missing {anchor}")
    pillager = inspect("world.entity.monster.illager.Pillager")
    if "setCanPickUpLoot" not in method(pillager, "protected void readAdditionalSaveData("):
        raise ValueError("pillagers no longer set CanPickUpLoot on load")
    return {
        "attack_reach": {"sqrt_of": reach["reach_sqrt"], "minus": reach["reach_minus"]},
        "pickup_reach": [reach["px"], reach["py"], reach["pz"]],
        "replace_drop_offset": replace["replace_drop_offset"],
        "zombie_can_pick_up_loot_chance": zombie["loot_chance"],
        "skeleton_can_pick_up_loot_chance": skeleton["loot_chance"],
        "pillager_can_pick_up_loot_after_load": True,
    }


def loot(jar, name):
    data = json.loads(jar.read(f"data/minecraft/loot_table/entities/{name}.json"))
    pools = []
    for pool in data.get("pools", []):
        conditions = [c.get("type", "").removeprefix("minecraft:") for c in [pool.get("condition")] if c]
        conditions += [c["type"].removeprefix("minecraft:") for c in pool.get("conditions", [])]
        if pool.get("condition", {}).get("predicate", {}).get("minecraft:type_specific/raider", {}).get("is_captain"):
            conditions = ["raid_captain"]
        entries = []
        for entry in pool["entries"]:
            mods = entry.get("modifier", [])
            mods = mods if isinstance(mods, list) else [mods]
            count = [1, 1]
            looting = None
            for m in mods:
                kind = m["type"].removeprefix("minecraft:")
                if kind == "set_count":
                    c = m["count"]
                    count = [c, c] if isinstance(c, (int, float)) else [c["min"], c["max"]]
                elif kind == "enchanted_count_increase":
                    looting = [m["count"]["min"], m["count"]["max"]]
            entries.append({"item": entry["name"].removeprefix("minecraft:"), "count": count, "looting": looting})
        pools.append({"rolls": pool["rolls"], "conditions": conditions, "entries": entries})
    return {"pools": pools}


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-illagers-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)
        cache = {}

        def inspect(name):
            if name not in cache:
                cache[name] = subprocess.check_output(
                    ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
                )
            return cache[name]

        entity_types = inspect("world.entity.EntityTypes")
        monster = attributes(inspect("world.entity.monster.Monster"), "createMonsterAttributes()")
        base = attributes(inspect("world.entity.Mob"), "createMobAttributes()")
        mobs = {}
        with zipfile.ZipFile(path) as jar:
            for key, (cls, field) in MOBS.items():
                code = inspect("world.entity." + cls)
                attrs = {**base, **monster, **attributes(code, "createAttributes()")}
                mobs[key] = {
                    "entity_type": entity_type(entity_types, field),
                    "attributes": attrs,
                    "experience": experience(code, cls) or 5,
                    "goals": goals(code),
                    "loot_table": loot(jar, key),
                }
            tags = {name: sorted(tag(jar, name)) for name in ("illager", "illager_friends", "raiders")}
        vindicator = inspect("world.entity.monster.illager.Vindicator")
        door = inspect("world.entity.monster.illager.Vindicator$VindicatorBreakDoorGoal")
        for anchor in ("Difficulty.NORMAL:", "Difficulty.HARD:"):
            if anchor not in vindicator:
                raise ValueError(f"unrecognized vindicator door rule: missing {anchor}")
        if "hasActiveRaid:" not in door:
            raise ValueError("vindicator doors no longer need an active raid")
        breaking = inspect("world.entity.ai.goal.BreakDoorGoal")
        door_time = int(re.search(r"DEFAULT_DOOR_BREAK_TIME = (\d+);", breaking)[1])
        out = {
            "reference": "Minecraft Java Edition 26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "net.minecraft.world.entity.monster.illager.{Vindicator,Evoker,Illusioner,Pillager,SpellcasterIllager}",
                "net.minecraft.world.entity.monster.{Vex,Ravager,Monster}",
                "net.minecraft.world.entity.projectile.EvokerFangs",
                "net.minecraft.world.entity.ai.goal.{RangedCrossbowAttackGoal,BreakDoorGoal}",
                "net.minecraft.world.item.CrossbowItem",
                "net.minecraft.world.entity.Mob",
                "net.minecraft.world.entity.monster.zombie.Zombie.finalizeSpawn",
                "net.minecraft.world.entity.monster.skeleton.AbstractSkeleton.finalizeSpawn",
                "net.minecraft.world.level.block.CropBlock.entityInside",
                "net.minecraft.world.entity.EntityTypes",
                "data/minecraft/loot_table/entities/{vindicator,evoker,illusioner,pillager,vex,ravager}.json",
                "data/minecraft/tags/entity_type/{illager,illager_friends,raiders}.json",
            ],
            "mobs": mobs,
            "spells": spells(inspect),
            "fangs": fangs(inspect, lambda f: entity_type(entity_types, f)),
            "vex": vex(inspect),
            "ravager": ravager(inspect),
            "crossbow": crossbow(inspect),
            "mob_combat": mob_combat(inspect),
            "vindicator": {
                "johnny_name": string_constant(vindicator, "Johnny"),
                "johnny_save_key": "Johnny",
                "door_break_difficulties": ["normal", "hard"],
                "door_break_needs_active_raid": True,
                "door_break_time": door_time,
                "main_hand_outside_raid": "iron_axe",
            },
            "entity_type_tags": tags,
        }
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
