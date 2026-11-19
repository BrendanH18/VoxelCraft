#!/usr/bin/env python3
"""Extract the raids' facts from the pinned official Java 26.3 artifact.

Reads `Raid` (its constants, `RaiderType` wave table, group counts,
enchantment odds, bonus spawns and spawn search), `Raids`, `Raider`,
`PatrollingMonster` and its `LongDistancePatrolGoal`, `PatrolSpawner`, the
`BadOmenMobEffect` and `RaidOmenMobEffect` ticks, `ServerLevel.getRaidAt`
and `isCloseToVillage`, `BellBlockEntity`, `GiveGiftToHero` (and its
profession to gift table map), `Villager.updateSpecialPrices`, which
raiders can lead, the raid enchantment providers, the hero of the village
gift loot tables, the pillager's captain pool, the `early_game` timeline
and the biome and dimension attributes that forbid patrols and raids.

Numbers inside method bodies are matched against a template of the
method's literal values in order (see `extract_java_illager_fixtures`): a
changed shape fails rather than regenerating a guessed fixture. Requires
javap. No VoxelCraft source or existing fixture is consulted.
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

from extract_java_illager_fixtures import capture
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_small_mob_fixtures import goals, literal, method, ops

RAIDER_TYPES = ["vindicator", "evoker", "pillager", "witch", "ravager"]
PROFESSIONS = [
    "armorer",
    "butcher",
    "cartographer",
    "cleric",
    "farmer",
    "fisherman",
    "fletcher",
    "leatherworker",
    "librarian",
    "mason",
    "shepherd",
    "toolsmith",
    "weaponsmith",
]


def int_constants(code):
    """Every `static final int NAME = value;` of a class."""
    return {m[1]: int(m[2]) for m in re.finditer(r"static final int (\w+) = (-?\d+);", code)}


def wave_table(code):
    """`Raid.RaiderType`'s constructors: entity type and spawns per wave."""
    block = method(code, "static {};")
    out = {}
    for name in RAIDER_TYPES:
        start = block.find(f"// String {name.upper()}")
        end = block.find(f"Field {name.upper()}:", start)
        if start < 0 or end < 0:
            raise ValueError(f"missing RaiderType.{name}")
        listing = ops(block[start:end])
        entity = re.search(r"EntityTypes\.(\w+):", "\n".join(listing))
        if not entity or entity[1] != name.upper():
            raise ValueError(f"unrecognized RaiderType.{name}")
        values = []
        for i, op in enumerate(listing):
            if op.startswith("iastore"):
                index, value = literal(listing[i - 2]), literal(listing[i - 1])
                if index != len(values):
                    raise ValueError(f"unrecognized RaiderType.{name} array")
                values.append(value)
        out[name] = values
    order = [m[1].lower() for m in re.finditer(r"// String (\w+)\n", block)]
    if order != RAIDER_TYPES:
        raise ValueError(f"unrecognized RaiderType order {order}")
    return out


def returns_false(code, signature):
    found = [literal(op) for op in ops(method(code, signature))]
    return found == [0, None]


def gift_tables(code):
    """`GiveGiftToHero.GIFTS`: profession to loot table, in insertion order."""
    block = method(code, "static {};")
    pairs = re.findall(
        r"VillagerProfession\.(\w+):.*?\n.*?BuiltInLootTables\.(\w+):",
        "\n".join(ops(block)),
    )
    return {p.lower(): t.lower() for p, t in pairs}


def loot_table(jar, path):
    data = json.loads(jar.read(f"data/minecraft/loot_table/{path}.json"))
    pools = []
    for pool in data.get("pools", []):
        entries = []
        for entry in pool["entries"]:
            out = {"item": entry["name"].removeprefix("minecraft:"), "weight": entry.get("weight", 1)}
            mods = entry.get("modifier", entry.get("functions", []))
            for f in mods if isinstance(mods, list) else [mods]:
                kind = f["type"].removeprefix("minecraft:")
                if kind == "set_count":
                    c = f["count"]
                    out["count"] = [c, c] if isinstance(c, int) else [c["min"], c["max"]]
                elif kind == "set_potion":
                    out["potion"] = f["id"].removeprefix("minecraft:")
                else:
                    raise ValueError(f"unrecognized gift function {kind}")
            entries.append(out)
        if pool.get("conditions") or pool.get("condition"):
            raise ValueError(f"unrecognized conditions in {path}")
        pools.append({"rolls": pool["rolls"], "entries": entries})
    return pools


def captain_pool(jar):
    data = json.loads(jar.read("data/minecraft/loot_table/entities/pillager.json"))
    (pool,) = data["pools"]
    predicate = pool["condition"]["predicate"]["minecraft:type_specific/raider"]
    (entry,) = pool["entries"]
    mods = {m["type"].removeprefix("minecraft:"): m for m in entry["modifier"]}
    amplifier = mods["set_ominous_bottle_amplifier"]["amplifier"]
    return {
        "condition": "is_captain" if predicate == {"is_captain": True} else predicate,
        "item": entry["name"].removeprefix("minecraft:"),
        "count": mods["set_count"]["count"],
        "amplifier": [amplifier["min"], amplifier["max"]],
    }


def captain_tables(jar):
    """Entity loot tables that name a raid captain."""
    out = []
    for name in sorted(jar.namelist()):
        if name.startswith("data/minecraft/loot_table/entities/") and b"is_captain" in jar.read(name):
            out.append(name.rsplit("/", 1)[1].removesuffix(".json"))
    return out


def provider(jar, name):
    data = json.loads(jar.read(f"data/minecraft/enchantment_provider/raid/{name}.json"))
    if data["type"] != "minecraft:single":
        raise ValueError(f"unrecognized provider {name}")
    return {"enchantment": data["enchantment"].removeprefix("minecraft:"), "level": data["level"]}


def environment(jar):
    timeline = json.loads(jar.read("data/minecraft/timeline/early_game.json"))
    track = timeline["tracks"]["minecraft:gameplay/can_pillager_patrol_spawn"]
    if track["modifier"] != "and" or timeline["clock"] != "minecraft:overworld":
        raise ValueError("unrecognized early_game timeline")
    no_patrols = []
    for name in sorted(jar.namelist()):
        if name.startswith("data/minecraft/worldgen/biome/") and name.endswith(".json"):
            attrs = json.loads(jar.read(name)).get("attributes", {})
            if attrs.get("minecraft:gameplay/can_pillager_patrol_spawn") is False:
                no_patrols.append(name.rsplit("/", 1)[1].removesuffix(".json"))
    no_raids = []
    for name in sorted(jar.namelist()):
        if name.startswith("data/minecraft/dimension_type/") and name.endswith(".json"):
            attrs = json.loads(jar.read(name)).get("attributes", {})
            if attrs.get("minecraft:gameplay/can_start_raid") is False:
                no_raids.append(name.rsplit("/", 1)[1].removesuffix(".json"))
    return {
        "patrol_keyframes": [[k["ticks"], k["value"]] for k in track["keyframes"]],
        "patrol_forbidden_biomes": no_patrols,
        "raid_forbidden_dimensions": no_raids,
    }


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-raids-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)
        cache = {}

        def inspect(name):
            if name not in cache:
                cache[name] = subprocess.check_output(
                    ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
                )
            return cache[name]

        raid = inspect("world.entity.raid.Raid")
        constants = int_constants(raid)
        groups = capture(raid, "getNumGroups(net", [0, "$easy", "$normal", "$hard"])
        odds = capture(
            raid,
            "getEnchantOdds()",
            [2, "$level2", 3, "$level3", 4, "$level4", 5, "$level5", 0.0],
        )
        # `getPotentialBonusSpawns`: the switch on the type, then the
        # vindicator/pillager (easy nextInt(2), normal 1, hard 2), witch
        # (none on easy or waves 1, 2 and 4, else 1) and ravager (1 on a
        # bonus wave outside easy) branches, then `nextInt(n + 1)`.
        bonus = capture(
            raid,
            "getPotentialBonusSpawns(",
            [
                1,
                0,
                1,
                0,
                "$witch_last_early_wave",
                "$witch_skipped_wave",
                "$witch",
                0,
                "$easy_bound",
                "$normal",
                "$hard",
                "$ravager_bonus_wave",
                0,
                0,
                1,
                0,
            ],
        )
        search = capture(
            raid,
            "findRandomSpawnPos(",
            [
                20,
                "$distance_scale",
                "$distance_offset",
                6.2831855,
                0,
                3.1415927,
                "$angle_step_divisor",
                "$ring_radius",
                "$jitter_bound",
                "$ring_radius_z",
                3,
                "$max_height_difference",
                "$village_seconds",
                "$loaded_margin",
                10,
                10,
                10,
                10,
            ],
        )
        capture(raid, "joinRaid(", [1, 0, 0.5, 1.0, 0.5, 0, 1])
        raids = inspect("world.entity.raid.Raids")
        create = capture(raids, "createOrExtendRaid(", ["$poi_radius", 0, 1.0])
        can_join = capture(raids, "canJoinRaid(net", ["$max_no_action", 1, 0])
        raids_tick = capture(raids, "public void tick(", [1, "$save_interval"])
        level = inspect("server.level.ServerLevel")
        raid_at = capture(level, "getRaidAt(", ["$radius_sqr"])
        close = capture(level, "isCloseToVillage(", ["$max_sections", 0, 1, 0])
        is_village = capture(level, "public boolean isVillage(net.minecraft.core.BlockPos)", ["$sections"])
        raider = inspect("world.entity.raid.Raider")
        no_action = capture(raider, "updateNoActionTime(", ["$step"])
        if "EntitySpawnReason.NATURAL" not in method(raider, "finalizeSpawn("):
            raise ValueError("unrecognized Raider.finalizeSpawn")
        celebration = inspect("world.entity.raid.Raider$RaiderCelebration")
        cheer = capture(celebration, "public void tick(", ["$sound_chance", "$jump_chance"])
        through = goals(raider)
        patrolling = inspect("world.entity.monster.PatrollingMonster")
        captain = capture(patrolling, "finalizeSpawn(", ["$captain_chance", 1, 2.0, 1])
        patrol_light = capture(patrolling, "checkPatrollingMonsterSpawnRules(", ["$max_block_light", 0])
        far = capture(patrolling, "removeWhenFarAway(", ["$despawn_distance_sqr", 1, 0])
        target = capture(patrolling, "findPatrolTarget(", ["$offset", "$spread", 0, -500, 1000, 1])
        patrol_goal = capture(patrolling, "registerGoals(", [4, "$speed", "$leader_speed"])
        long_goal = inspect("world.entity.monster.PatrollingMonster$LongDistancePatrolGoal")
        steer = capture(
            long_goal, "public void tick(", [0, "$arrive_distance", "$turn_degrees", "$turn_scale", "$step"]
        )
        companions = capture(long_goal, "findPatrolCompanions(", ["class:PatrollingMonster", "$radius"])
        wander = capture(long_goal, "moveRandomly(", ["$offset", "$spread", 0, -8, 16])
        spawner = inspect("world.level.levelgen.PatrolSpawner")
        patrol = capture(
            spawner,
            "public void tick(",
            [
                1,
                "$interval",
                "$interval_spread",
                "$chance",
                1,
                "$village_sections",
                "$min_offset",
                "$offset_spread",
                -1,
                1,
                24,
                24,
                -1,
                1,
                0,
                "$loaded_margin",
                10,
                10,
                10,
                10,
                1,
                0,
                1,
                0,
                "$wander",
                5,
                5,
                5,
            ],
        )
        bad = capture(inspect("world.effect.BadOmenMobEffect"), "applyEffectTick(", ["$raid_omen_ticks", 0, 1])
        omen = capture(inspect("world.effect.RaidOmenMobEffect"), "shouldApplyEffectTickThisTick(", ["$fires_at", 1, 0])
        bell = int_constants(inspect("world.level.block.entity.BellBlockEntity"))
        gift_code = inspect("world.entity.ai.behavior.GiveGiftToHero")
        gift = int_constants(gift_code)
        villager = inspect("world.entity.npc.villager.Villager")
        price = capture(villager, "updateSpecialPrices(", [0.0, "$base", "$per_level", 0.0, "$minimum"])
        leaders = {
            "ravager": not returns_false(inspect("world.entity.monster.Ravager"), "public boolean canBeLeader()"),
            "witch": not returns_false(inspect("world.entity.monster.Witch"), "public boolean canBeLeader()"),
        }
        with zipfile.ZipFile(path) as jar:
            gifts = {p: loot_table(jar, f"gameplay/hero_of_the_village/{p}_gift") for p in PROFESSIONS}
            gifts["baby"] = loot_table(jar, "gameplay/hero_of_the_village/baby_gift")
            gifts["unemployed"] = loot_table(jar, "gameplay/hero_of_the_village/unemployed_gift")
            providers = {
                n: provider(jar, n)
                for n in ("pillager_post_wave_3", "pillager_post_wave_5", "vindicator", "vindicator_post_wave_5")
            }
            out = {
                "reference": "Minecraft Java Edition 26.3",
                "server_sha1": SERVER_SHA1,
                "sources": [
                    "net.minecraft.world.entity.raid.{Raid,Raid$RaiderType,Raids,Raider,Raider$RaiderCelebration}",
                    "net.minecraft.world.entity.monster.{PatrollingMonster,PatrollingMonster$LongDistancePatrolGoal}",
                    "net.minecraft.world.level.levelgen.PatrolSpawner",
                    "net.minecraft.world.effect.{BadOmenMobEffect,RaidOmenMobEffect}",
                    "net.minecraft.server.level.ServerLevel.{getRaidAt,isVillage,isCloseToVillage}",
                    "net.minecraft.world.level.block.entity.BellBlockEntity",
                    "net.minecraft.world.entity.ai.behavior.GiveGiftToHero",
                    "net.minecraft.world.entity.npc.villager.Villager.updateSpecialPrices",
                    "net.minecraft.world.entity.monster.{Ravager,Witch}.canBeLeader",
                    "data/minecraft/loot_table/gameplay/hero_of_the_village/*.json",
                    "data/minecraft/loot_table/entities/pillager.json",
                    "data/minecraft/enchantment_provider/raid/*.json",
                    "data/minecraft/timeline/early_game.json",
                    "data/minecraft/worldgen/biome/*.json, data/minecraft/dimension_type/*.json",
                ],
                "raid": {
                    "constants": {k: v for k, v in constants.items()},
                    "groups": groups,
                    "enchant_odds": odds,
                    "waves": wave_table(inspect("world.entity.raid.Raid$RaiderType")),
                    "bonus_spawns": bonus,
                    "spawn_search": search,
                    "join_offset": [0.5, 1.0, 0.5],
                    "ravager_riders": {
                        "pillager_at_wave": "getNumGroups(NORMAL)",
                        "evoker_then_vindicators_from_wave": "getNumGroups(HARD)",
                    },
                    "captain_drop_chance": 2.0,
                },
                "raids": {**create, **can_join, **raids_tick, **raid_at},
                "village": {**close, "is_village_sections": is_village["sections"]},
                "raider": {
                    "no_action_step": no_action["step"],
                    "celebration": cheer,
                    "goals": through,
                    "witch_natural_spawn_cannot_join": True,
                    "can_be_leader": leaders,
                },
                "patrol": {
                    "spawner": patrol,
                    "captain_chance": captain["captain_chance"],
                    "max_block_light": patrol_light["max_block_light"],
                    "despawn_distance_sqr": far["despawn_distance_sqr"],
                    "target": target,
                    "goal": {**patrol_goal, **steer, "companion_radius": companions["radius"], "wander": wander},
                    "failed_cooldown": int_constants(long_goal)["NAVIGATION_FAILED_COOLDOWN"],
                },
                "omen": {**bad, **omen},
                "bell": bell,
                "gift": {**gift, "tables": gift_tables(gift_code), "loot": gifts},
                "hero_price": price,
                "captain_loot": captain_pool(jar),
                "captain_loot_tables": captain_tables(jar),
                "enchantment_providers": providers,
                "environment": environment(jar),
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
