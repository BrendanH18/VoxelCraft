#!/usr/bin/env python3
"""Extract the v0.16 Track G facts from the pinned official Java 26.3 server.

Covers End crystals (`EndCrystal`, `EndCrystalItem`, the `end_crystal`
entity type, item and recipe), the dragon fight and its respawn ritual
(`EnderDragonFight`, `DragonRespawnStage`, `EnderDragon.tickDeath` and
`checkCrystals`), the obsidian spikes (`EndSpikeFeature`), End gateways
(`TheEndGatewayBlockEntity`, `EndIslandFeature`) and the Nether portal
forcer (`PortalForcer`).

Requires javap. Each number is read from a named place in the bytecode
(a `static final` constant, or the constants a named method pushes, in
order), and an unrecognized form fails rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
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
from extract_java_new_mob_fixtures import constants, method, number, ops
from extract_java_wither_beacon_fixtures import before, entity_type, item_facts

# The progression mobs whose drops the M5 audit checks.
LOOT_TABLES = ["entities/blaze", "entities/enderman", "entities/ghast"]


def numbers(code, signature):
    """Every constant the method pushes, in order."""
    return [n for n in (number(op) for op in ops(method(code, signature))) if n is not None]


def stage_names(code):
    """`DragonRespawnStage`'s serialized names, in declaration order."""
    init = ops(method(code, "static {}"))
    names = []
    for i, op in enumerate(init):
        match = re.search(r"// String ([a-z_]+)$", op)
        if match and i + 1 < len(init) and "DragonRespawnStage$" in init[i + 1]:
            names.append(match[1])
    if len(names) != 5:
        raise ValueError(f"expected five respawn stages, found {names}")
    return names


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-dragon-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        crystal = inspect("world.entity.boss.enderdragon.EndCrystal")
        item = inspect("world.item.EndCrystalItem")
        use_on = method(item, "public net.minecraft.world.InteractionResult useOn(")
        fight = inspect("world.level.dimension.end.EnderDragonFight")
        stages = inspect("world.level.dimension.end.DragonRespawnStage")
        dragon = inspect("world.entity.boss.enderdragon.EnderDragon")
        spikes = inspect("world.level.levelgen.feature.EndSpikeFeature")
        spike_loader = inspect("world.level.levelgen.feature.EndSpikeFeature$SpikeCacheLoader")
        gateway = inspect("world.level.block.entity.TheEndGatewayBlockEntity")
        island = inspect("world.level.levelgen.feature.EndIslandFeature")
        forcer = inspect("world.level.portal.PortalForcer")
        tick = "public void tick(net.minecraft.server.level.ServerLevel"
        end_crystal = {
            "explosion_power": before(
                ops(method(crystal, "public final boolean hurtServer(")), r"explode:", kind=float
            )[0],
            "placeable_on": [b.lower() for b in re.findall(r"Blocks\.(\w+):", use_on)],
            "placement_numbers": numbers(item, "public net.minecraft.world.InteractionResult useOn("),
        }
        respawn = {
            "stages": stage_names(stages),
            "edge_distance": numbers(fight, "public void tryRespawn()")[-1],
            "preparing_to_summon_pillars": numbers(inspect("world.level.dimension.end.DragonRespawnStage$2"), tick),
            "summoning_pillars": numbers(inspect("world.level.dimension.end.DragonRespawnStage$3"), tick),
            "summoning_dragon": numbers(inspect("world.level.dimension.end.DragonRespawnStage$4"), tick),
        }
        new_gateway = numbers(fight, "private void spawnNewGateway()")
        fight_facts = {
            "constants": constants(fight),
            "valid_player_distance": [n for n in numbers(fight, "public void init(") if isinstance(n, float)][0],
            "gateway_y": new_gateway[-1],
            "dragon_spawn_numbers": numbers(
                fight, "private net.minecraft.world.entity.boss.enderdragon.EnderDragon createNewDragon()"
            ),
        }
        death = numbers(dragon, "protected void tickDeath()")
        dragon_facts = {
            "first_kill_xp": next(n for n in death if n == 12000),
            "later_kill_xp": next(n for n in death if n == 500),
            "death_numbers": death,
            "check_crystals": numbers(dragon, "private void checkCrystals()"),
            "crystal_destroy_range": before(
                ops(method(dragon, "static {}")), r"TargetingConditions.range:", kind=float
            )[0],
        }
        spike_facts = {
            "constants": constants(spikes),
            "cache_key_mask": numbers(spikes, "getSpikesForLevel(")[0],
            "loader_numbers": numbers(spike_loader, "public java.util.List<"),
        }
        gateway_facts = {
            "constants": constants(gateway),
            "tentative": numbers(gateway, "private static net.minecraft.world.phys.Vec3 findExitPortalXZPosTentative("),
            "valid_spawn": numbers(gateway, "private static net.minecraft.core.BlockPos findValidSpawnInChunk("),
            "teleport_pos": numbers(
                gateway, "private static net.minecraft.core.BlockPos findOrCreateValidTeleportPos("
            ),
            "exit_position": numbers(gateway, "private static net.minecraft.core.BlockPos findExitPosition("),
            "island": numbers(island, "public boolean place("),
        }
        forcer_facts = {
            "constants": constants(forcer),
            "create_portal": numbers(forcer, "createPortal(net.minecraft.core.BlockPos"),
        }
        items = {"end_crystal": item_facts(ops(method(inspect("world.item.Items"), "static {}")), "END_CRYSTAL")}
        entity_types = {"end_crystal": entity_type(ops(inspect("world.entity.EntityTypes")), "END_CRYSTAL")}

    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        recipe = json.loads(jar.read("data/minecraft/recipe/end_crystal.json"))
        loot = {name: json.loads(jar.read(f"data/minecraft/loot_table/{name}.json")) for name in LOOT_TABLES}
        for table in loot.values():
            table.pop("random_sequence", None)
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.entity.EntityTypes (END_CRYSTAL), Items (END_CRYSTAL)",
            "EndCrystal.hurtServer, EndCrystalItem.useOn",
            "EnderDragonFight (constants, init, tryRespawn, spawnNewGateway, createNewDragon)",
            "DragonRespawnStage (names; PREPARING_TO_SUMMON_PILLARS, SUMMONING_PILLARS, SUMMONING_DRAGON tick)",
            "EnderDragon (tickDeath, checkCrystals, CRYSTAL_DESTROY_TARGETING)",
            "EndSpikeFeature (constants, getSpikesForLevel, SpikeCacheLoader.load)",
            "TheEndGatewayBlockEntity (constants, findExitPortalXZPosTentative, findValidSpawnInChunk, "
            "findOrCreateValidTeleportPos, findExitPosition), EndIslandFeature.place",
            "PortalForcer (constants, createPortal)",
            "data/minecraft/recipe/end_crystal.json",
            "data/minecraft/loot_table/entities/blaze, enderman, ghast",
        ],
        "entity_types": entity_types,
        "items": items,
        "recipe": recipe,
        "loot_tables": loot,
        "end_crystal": end_crystal,
        "fight": fight_facts,
        "respawn": respawn,
        "dragon": dragon_facts,
        "spikes": spike_facts,
        "gateway": gateway_facts,
        "portal_forcer": forcer_facts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
