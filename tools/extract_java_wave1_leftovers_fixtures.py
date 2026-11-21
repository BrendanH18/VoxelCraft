#!/usr/bin/env python3
"""Extract the v0.16 Track H facts (wave 1 leftovers) from the pinned server.

Covers:

- potent sulfur (`PotentSulfurBlock`, `PotentSulfurBlockEntity`): the
  constants, the geyser block and entity tags and the sound event names,
  plus the countdown draws of `geyserPositional`, made by the jar's own
  `XoroshiroRandomSource` through the JDK oracle `JavaGeyserVectors.java`;
- speleothems (`SpeleothemBlock`, `SulfurSpikeBlock`,
  `PointedDripstoneBlock`): growth, falling and landing constants, which
  class declares `fallOn`, the `speleothems` tag;
- sulfur springs: the `sulfur_spring` selector (weights, the tuff scatter,
  templates and their offset), the template sizes (read from the NBT
  header; no block is copied), `rooted_sulfur_spring` (the root system)
  and its placement, and `azalea_root_replaceable`;
- the Wither's goals (`WitherBoss.registerGoals`,
  `WaterAvoidingRandomFlyingGoal.getPosition`, `RandomStrollGoal`'s
  default interval, `Goal.reducedTickDelay`, `Mob.serverAiStep`'s cadence)
  and `WitherSkullBlock`'s dispenser patterns.

Requires `javap` and `javac`/`java` (a JDK 21 or newer). Each number is read
from a named place in the bytecode, and an unrecognized form fails rather
than regenerating a guessed fixture. No VoxelCraft source or existing
fixture is consulted.
"""

import argparse
import gzip
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
from extract_java_new_mob_fixtures import constants, method, number, ops, strip

LIBRARIES = ["datafixerupper", "guava", "failureaccess", "fastutil", "slf4j-api", "commons-lang3", "logging", "joml"]
NETTY = ["netty-codec", "netty-common", "netty-buffer", "netty-transport"]


def nbt_size(data):
    """The `size` list of a structure template (gzipped NBT)."""
    stream = io.BytesIO(gzip.decompress(data))

    def take(fmt):
        size = struct.calcsize(">" + fmt)
        return struct.unpack(">" + fmt, stream.read(size))[0]

    def text():
        return stream.read(take("H")).decode("utf8")

    def payload(tag):
        if tag == 1:
            return take("b")
        if tag == 2:
            return take("h")
        if tag == 3:
            return take("i")
        if tag == 4:
            return take("q")
        if tag == 5:
            return take("f")
        if tag == 6:
            return take("d")
        if tag == 7:
            return stream.read(take("i"))
        if tag == 8:
            return text()
        if tag == 9:
            kind = take("b")
            return [payload(kind) for _ in range(take("i"))]
        if tag == 10:
            out = {}
            while True:
                kind = take("b")
                if kind == 0:
                    return out
                name = text()
                out[name] = payload(kind)
        if tag == 11:
            return [take("i") for _ in range(take("i"))]
        if tag == 12:
            return [take("q") for _ in range(take("i"))]
        raise ValueError(f"unknown NBT tag {tag}")

    kind = take("b")
    text()
    return payload(kind)["size"]


def pushed(listing, call, count):
    """The `count` constants pushed (in order) right before the first op
    containing `call`."""
    for i, op in enumerate(listing):
        if call in op:
            found = []
            for prior in reversed(listing[:i]):
                value = number(prior)
                if value is not None:
                    found.append(value)
                elif prior.startswith(("aload", "getfield", "dload", "fload", "iload")):
                    continue
                else:
                    break
                if len(found) == count:
                    return list(reversed(found))
            raise ValueError(f"expected {count} constants before {call}: {listing[max(0, i - 12) : i + 1]}")
    raise ValueError(f"missing call {call}")


def goals(listing):
    """`addGoal(priority, new Goal(...))` in order: (selector, priority, goal)."""
    out = []
    for i, op in enumerate(listing):
        if "GoalSelector.addGoal" not in op:
            continue
        start = max(j for j in range(i) if re.search(r"Field (goalSelector|targetSelector):", listing[j]))
        selector = re.search(r"Field (goalSelector|targetSelector):", listing[start])[1]
        priority = number(listing[start + 1])
        built = next(
            re.search(r"Method net/minecraft/[\w/]+/(\w+(?:\$\w+)?)\.\"<init>\"", listing[j])[1]
            for j in range(i - 1, start, -1)
            if '"<init>"' in listing[j]
        )
        out.append({"selector": selector, "priority": priority, "goal": built})
    return out


def extract(server, oracle):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-wave1-leftovers-") as temporary:
        temporary = Path(temporary)
        libraries = []
        with zipfile.ZipFile(io.BytesIO(data)) as bundler:
            jar_bytes = bundler.read("META-INF/versions/26.3/server-26.3.jar")
            for name in bundler.namelist():
                base = name.rsplit("/", 1)[-1]
                if name.endswith(".jar") and "/libraries/" in name:
                    if any(base.startswith(lib + "-") for lib in LIBRARIES + NETTY):
                        target = temporary / base
                        target.write_bytes(bundler.read(name))
                        libraries.append(str(target))
        path = temporary / "reference.jar"
        path.write_bytes(jar_bytes)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        classpath = ":".join([str(path)] + libraries)
        subprocess.check_call(["javac", "-d", str(temporary), "-cp", classpath, str(oracle)])
        lines = subprocess.check_output(["java", "-cp", classpath + ":" + str(temporary), oracle.stem], text=True)
        vectors = []
        for line in lines.splitlines():
            seed, x, y, z, dormant, erupting = json.loads(line)
            vectors.append({"seed": seed, "x": x, "y": y, "z": z, "dormant_draw": dormant, "erupting_draw": erupting})

        with zipfile.ZipFile(io.BytesIO(jar_bytes)) as jar:

            def read(name):
                return json.loads(jar.read("data/minecraft/" + name))

            def tag(name):
                return [strip(v) for v in read(f"tags/{name}.json")["values"]]

            entity = constants(inspect("world.level.block.entity.PotentSulfurBlockEntity"))
            block = constants(inspect("world.level.block.PotentSulfurBlock"))
            sounds = inspect("sounds.SoundEvents")
            potent = {
                "allowed_water_blocks_above": block["ALLOWED_WATER_BLOCKS_ABOVE"],
                "effect_application_frequency_ticks": entity["EFFECT_APPLICATION_FREQUENCY_TICKS"],
                "effect_duration_in_ticks": entity["EFFECT_DURATION_IN_TICKS"],
                "effect_range": entity["EFFECT_RANGE"],
                "particle_frequency_ticks": entity["PARTICLE_FREQUENCY_TICKS"],
                "sound_frequency_ticks": entity["SOUND_FREQUENCY_TICKS"],
                "geyser_base_launch_speed": entity["GEYSER_BASE_LAUNCH_SPEED"],
                "geyser_launch_force": entity["GEYSER_LAUNCH_FORCE"],
                "geyser_salt": entity["GEYSER_SALT"],
                "continuous_tag": tag("block/causes_continuous_geyser_eruptions"),
                "periodic_tag": tag("block/causes_periodic_geyser_eruptions"),
                "not_affected_by_geysers": tag("entity_type/not_affected_by_geysers"),
                "sounds": sorted(
                    set(re.findall(r"String (block\.potent_sulfur\.(?:geyser_[a-z_]+|noxious_gas))", sounds))
                ),
            }

            speleothem_code = inspect("world.level.block.SpeleothemBlock")
            spike_code = inspect("world.level.block.SulfurSpikeBlock")
            dripstone_code = inspect("world.level.block.PointedDripstoneBlock")
            speleothem = constants(speleothem_code)
            dripstone = constants(dripstone_code)

            def landing(code):
                return pushed(ops(method(code, "int getStalactiteLandingSound()")), "ireturn", 1)[0]

            speleothems = {
                "growth_probability_per_random_tick": speleothem["GROWTH_PROBABILITY_PER_RANDOM_TICK"],
                "max_growth_length": speleothem["MAX_GROWTH_LENGTH"],
                "max_stalagmite_search_range_when_growing": speleothem["MAX_STALAGMITE_SEARCH_RANGE_WHEN_GROWING"],
                "delay_before_falling": speleothem["DELAY_BEFORE_FALLING"],
                "min_trident_velocity_to_break": speleothem["MIN_TRIDENT_VELOCITY_TO_BREAK_SPELEOTHEM"],
                "stalactite_damage_per_fall_distance_and_size": speleothem[
                    "STALACTITE_DAMAGE_PER_FALL_DISTANCE_AND_SIZE"
                ],
                "stalactite_max_damage": speleothem["STALACTITE_MAX_DAMAGE"],
                "sulfur_spike_max_growing_length": constants(spike_code)["MAX_GROWING_LENGTH"],
                "sulfur_spike_landing_event": landing(spike_code),
                "dripstone_landing_event": landing(dripstone_code),
                "stalagmite_fall_distance_offset": dripstone["STALAGMITE_FALL_DISTANCE_OFFSET"],
                "stalagmite_fall_damage_modifier": dripstone["STALAGMITE_FALL_DAMAGE_MODIFIER"],
                "fall_on_declared_by": [
                    name
                    for name, code in [
                        ("SpeleothemBlock", speleothem_code),
                        ("SulfurSpikeBlock", spike_code),
                        ("PointedDripstoneBlock", dripstone_code),
                    ]
                    if re.search(r"\bvoid fallOn\(", code)
                ],
                "tag": tag("block/speleothems"),
            }

            selector = read("worldgen/feature/sulfur_spring.json")
            entries = []
            for entry in selector["features"]:
                parts = entry["data"]["feature"]["features"]
                tuff, template = parts[0], parts[1]
                placement = {p["type"]: p for p in tuff["placement"]}
                offset = placement["minecraft:offset"]
                names = [strip(t["data"]["id"]) for t in template["feature"]["templates"]]
                entries.append(
                    {
                        "weight": entry["weight"],
                        "tuff": strip(tuff["feature"]["to_place"]["id"]),
                        "tuff_count": placement["minecraft:count"]["count"],
                        "tuff_xz": [offset["x"]["min"], offset["x"]["max"]],
                        "tuff_y": [offset["y"]["min"], offset["y"]["max"]],
                        "tuff_scan": placement["minecraft:environment_scan"]["max_steps"],
                        "templates": names,
                        "template_offset_y": template["placement"][0]["y"],
                    }
                )
            sizes = {}
            for entry in entries:
                for name in entry["templates"]:
                    sizes[name] = nbt_size(jar.read(f"data/minecraft/structure/{name}.nbt"))
            rooted = read("worldgen/feature/rooted_sulfur_spring.json")
            placed = read("worldgen/placed_feature/rooted_sulfur_spring.json")
            biome = read("worldgen/biome/sulfur_caves.json")
            springs = {
                "selector": entries,
                "template_sizes": sizes,
                "root_system": {
                    k: (strip(v) if isinstance(v, str) else v)
                    for k, v in rooted.items()
                    if k not in ("type", "feature", "allowed_tree_position") and not k.endswith("_state_provider")
                },
                "root_state": strip(rooted["root_state_provider"]["id"]),
                "hanging_root_state": strip(rooted["hanging_root_state_provider"]["id"]),
                "allowed_tree_position": strip(rooted["allowed_tree_position"]["tag"]),
                "placement": placed["placement"],
                "biome_step": next(
                    i for i, step in enumerate(biome["features"]) if "minecraft:rooted_sulfur_spring" in step
                ),
                "biome_step_features": [
                    strip(f)
                    for f in next(step for step in biome["features"] if "minecraft:rooted_sulfur_spring" in step)
                ],
                "azalea_root_replaceable": tag("block/azalea_root_replaceable"),
                "base_stone_overworld": tag("block/base_stone_overworld"),
            }

            wither = inspect("world.entity.boss.wither.WitherBoss")
            flying = ops(method(inspect("world.entity.ai.goal.WaterAvoidingRandomFlyingGoal"), "getPosition()"))
            stroll = inspect("world.entity.ai.goal.RandomStrollGoal")
            mob = ops(method(inspect("world.entity.Mob"), "void serverAiStep()"))
            goal = ops(method(inspect("world.entity.ai.goal.Goal"), "int reducedTickDelay(int)"))
            skull = inspect("world.level.block.WitherSkullBlock")
            cadence = next(number(mob[i - 1]) for i, op in enumerate(mob) if op.startswith("irem"))
            wither_goals = {
                "goals": goals(ops(method(wither, "void registerGoals()"))),
                "hover_random_pos": pushed(flying, "HoverRandomPos.getPos", 5),
                "air_and_water_random_pos": pushed(flying, "AirAndWaterRandomPos.getPos", 4),
                "random_stroll_interval": pushed(
                    ops(method(stroll, "RandomStrollGoal(net.minecraft.world.entity.PathfinderMob, double);")),
                    '"<init>"',
                    1,
                )[0],
                "reduced_tick_delay_divisor": pushed(goal, "positiveCeilDiv", 1)[0],
                "selector_cadence": cadence,
                "skull_patterns": {
                    name: re.findall(r"String ([~#^ ]{3})$", method(skull, sig), re.M)
                    for name, sig in [("full", "getOrCreateWitherFull()"), ("base", "getOrCreateWitherBase()")]
                },
                "can_spawn_mob_min_y_offset": pushed(ops(method(skull, "boolean canSpawnMob(")), "iadd", 1)[0],
            }

            return {
                "reference": {
                    "version": "26.3",
                    "server_sha1": SERVER_SHA1,
                    "sources": [
                        "net/minecraft/world/level/block/{PotentSulfurBlock,SpeleothemBlock,SulfurSpikeBlock,"
                        "PointedDripstoneBlock,WitherSkullBlock}.class",
                        "net/minecraft/world/level/block/entity/PotentSulfurBlockEntity.class",
                        "net/minecraft/world/entity/boss/wither/WitherBoss.class",
                        "net/minecraft/world/entity/ai/goal/{WaterAvoidingRandomFlyingGoal,RandomStrollGoal,"
                        "Goal}.class",
                        "net/minecraft/world/entity/Mob.class",
                        "net/minecraft/sounds/SoundEvents.class",
                        "data/minecraft/worldgen/{feature,placed_feature}/rooted_sulfur_spring.json",
                        "data/minecraft/worldgen/feature/sulfur_spring.json",
                        "data/minecraft/worldgen/biome/sulfur_caves.json",
                        "data/minecraft/structure/spring/sulfur_spring_*.nbt (size only)",
                        "data/minecraft/tags/{block,entity_type}/...",
                        "tools/JavaGeyserVectors.java run against the jar",
                    ],
                },
                "potent_sulfur": potent,
                "geyser_countdowns": vectors,
                "speleothems": speleothems,
                "sulfur_springs": springs,
                "wither": wither_goals,
            }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--oracle", type=Path, default=Path(__file__).with_name("JavaGeyserVectors.java"))
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server, args.oracle.resolve()), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
