#!/usr/bin/env python3
"""Normalize Java 26.3 ice support tags and verify the associated block methods."""
import argparse
import hashlib
import io
import json
import math
import re
import struct
from pathlib import Path
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_turtle_fixtures import field_initialization


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        visited = set()

        def expand(name, stack=()):
            if name in stack:
                raise ValueError("reference tag cycle")
            path = f"data/minecraft/tags/block/{name}.json"
            visited.add(path)
            values = set()
            for value in json.loads(jar.read(path))["values"]:
                if not isinstance(value, str):
                    raise ValueError("unknown tag entry form")
                if value.startswith("#minecraft:"):
                    values.update(expand(value.removeprefix("#minecraft:"), (*stack, name)))
                elif value.startswith("minecraft:"):
                    values.add(value.removeprefix("minecraft:"))
                else:
                    raise ValueError("unexpected namespace")
            return values

        blocks = sorted(expand("ice_melts_when_destroyed_above"))
        enchantment_path = "data/minecraft/tags/enchantment/prevents_ice_melting.json"
        enchantments = json.loads(jar.read(enchantment_path))["values"]
        dimensions = {}
        for name in ("overworld", "the_nether", "the_end"):
            path = f"data/minecraft/dimension_type/{name}.json"
            visited.add(path)
            dimensions[name] = json.loads(jar.read(path))["attributes"].get("minecraft:gameplay/water_evaporates", False)
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-mining-") as temporary:
        jar_path = Path(temporary) / "reference.jar"
        jar_path.write_bytes(inner)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(jar_path), "-c", "-p", name], text=True)

        ice = inspect("net.minecraft.world.level.block.IceBlock").split(" playerDestroy(")[1].split("\n\n")[0]
        for token in ("EnchantmentTags.PREVENTS_ICE_MELTING", "EnvironmentAttributes.WATER_EVAPORATES",
                      "BlockTags.ICE_MELTS_WHEN_DESTROYED_ABOVE", "BlockState.liquid:"):
            if token not in ice:
                raise ValueError(f"ice destruction changed: {token}")
        egg = inspect("net.minecraft.world.level.block.DragonEggBlock").split(" attack(")[1].split("\n\n")[0]
        if "Method teleport:" not in egg:
            raise ValueError("dragon egg attack changed")
        creative = inspect("net.minecraft.server.level.ServerPlayerGameMode")
        if "Abilities.instabuild:Z" not in creative or "String creative destroy" not in creative:
            raise ValueError("creative destruction changed")
        progress = inspect("net.minecraft.world.level.block.state.BlockBehaviour").split(" getDestroyProgress(")[1].split("\n\n")[0]
        divisors = [int(v) for v in re.findall(r"bipush\s+(\d+)", progress)]
        if len(divisors) != 2 or progress.count(" fdiv") != 2 or "Player.hasCorrectToolForDrops:" not in progress:
            raise ValueError("destroy progress formula changed")
        player = inspect("net.minecraft.world.entity.player.Player").split(" getDestroySpeed(")[1].split("\n\n")[0]
        fatigue = re.search(r"// double ([0-9.]+)d", player)
        if not fatigue or "java/lang/Math.pow:(DD)D" not in player or "Attributes.SUBMERGED_MINING_SPEED:" not in player:
            raise ValueError("player mining modifiers changed")
        fatigue_base = float(fatigue[1])
        ground_penalties = re.findall(r"// float ([0-9.]+)f", player)
        if not ground_penalties or "Method onGround:()Z" not in player:
            raise ValueError("airborne mining changed")
        airborne_divisor = float(ground_penalties[-1])
        attributes = field_initialization(inspect("net.minecraft.world.entity.ai.attributes.Attributes"), "SUBMERGED_MINING_SPEED")
        defaults = re.findall(r"// double ([0-9.]+)d", attributes)
        if not defaults:
            raise ValueError("submerged mining attribute changed")
        submerged_factor = float(defaults[0])
        block_range = field_initialization(inspect("net.minecraft.world.entity.ai.attributes.Attributes"), "BLOCK_INTERACTION_RANGE")
        block_range = float(re.findall(r"// double ([0-9.]+)d", block_range)[0])
        creative_range = field_initialization(inspect("net.minecraft.server.level.ServerPlayer"), "CREATIVE_BLOCK_INTERACTION_RANGE_MODIFIER")
        if "Operation.ADD_VALUE:" not in creative_range:
            raise ValueError("creative block range modifier operation changed")
        creative_bonus = float(re.findall(r"// double ([0-9.]+)d", creative_range)[0])
    def f32(value):
        return struct.unpack("f", struct.pack("f", value))[0]
    factors = [{"amplifier": amp, "factor": f32(math.pow(fatigue_base, amp + 1))} for amp in range(256)]
    cases = []
    # Explicit scenario inputs. The expected calculation reads only the reference
    # operation order/divisors, never VoxelCraft mining code or output.
    for tool, base_speed, correct in (("hand", 1., False), ("wooden_pickaxe", 2., True), ("iron_pickaxe", 6., True)):
        for water, ground in ((False, True), (True, True), (True, False)):
            speed = f32(base_speed)
            if water:
                speed = f32(speed * f32(submerged_factor))
            if not ground:
                speed = f32(speed / f32(airborne_divisor))
            delta = f32(f32(speed / f32(1.5)) / f32(divisors[0 if correct else 1]))
            cases.append({"block": "stone", "tool": tool, "setup_hardness": 1.5, "setup_speed": base_speed,
                          "eyes_in_water": water, "on_ground": ground, "delta": delta})
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["IceBlock.playerDestroy", "DragonEggBlock.attack", "ServerPlayerGameMode.handleBlockBreakAction",
                        "BlockBehaviour.getDestroyProgress", "Player.getDestroySpeed", "Attributes.SUBMERGED_MINING_SPEED",
                        "Attributes.BLOCK_INTERACTION_RANGE", "ServerPlayer.CREATIVE_BLOCK_INTERACTION_RANGE_MODIFIER",
                        *sorted(visited), enchantment_path],
            "ice_support_blocks": blocks, "ice_melting_prevented_by": [v.removeprefix("minecraft:") for v in enchantments],
            "water_evaporates": dimensions, "fatigue_factors": factors, "progress_cases": cases,
            "block_reach": {"survival": block_range, "creative": block_range + creative_bonus}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = extract(args.server)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
