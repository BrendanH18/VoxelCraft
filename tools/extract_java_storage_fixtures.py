#!/usr/bin/env python3
"""Extract storage transaction facts from the pinned Java 26.3 server.

v0.17 Track D2 (AUT-05). Reads `quickMoveStack` in each storage menu for
the slot ranges and directions it passes to `moveItemStackTo`, the
potion and furnace fuel slot limits, the hopper cooldown, the comparator
formula's float arithmetic, and where container contents and shulker box
drops come from. Needs `javap` (a JDK) on the PATH.

    python3 tools/extract_java_storage_fixtures.py server.jar \
        --output tests/fixtures/java/26.3/storage_transactions.json
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

MENUS = {
    "chest": "net.minecraft.world.inventory.ChestMenu",
    "shulker_box": "net.minecraft.world.inventory.ShulkerBoxMenu",
    "hopper": "net.minecraft.world.inventory.HopperMenu",
    "dispenser": "net.minecraft.world.inventory.DispenserMenu",
    "furnace": "net.minecraft.world.inventory.AbstractFurnaceMenu",
    "brewing_stand": "net.minecraft.world.inventory.BrewingStandMenu",
}

PUSH = re.compile(r"^\s*\d+: (iconst_(\d)|bipush\s+(-?\d+)|sipush\s+(-?\d+))\s*$")


def method(code, name):
    """The bytecode of the first method whose declaration names `name`."""
    lines = code.split("\n")
    header = re.compile(rf"^  \S.*\b{name}\(.*\);$")
    for i, line in enumerate(lines):
        if header.match(line):
            body = []
            for line in lines[i + 1 :]:
                if line.strip() == "" or re.match(r"^  \S", line):
                    break
                body.append(line)
            return body
    raise ValueError(f"no method {name}")


def constant(line):
    match = PUSH.match(line)
    if not match:
        return None
    return int(next(g for g in match.groups()[1:] if g is not None))


def moves(body):
    """`[start, end, backwards]` per `moveItemStackTo` call; computed bounds are null."""
    out = []
    for i, line in enumerate(body):
        if "moveItemStackTo:(" not in line:
            continue
        backwards = constant(body[i - 1])
        if backwards not in (0, 1):
            raise ValueError(f"unexpected direction before {line}")
        end = constant(body[i - 2])
        # A bound built from the container size isn't a constant push.
        start = constant(body[i - 3]) if end is not None else None
        out.append([start, end if start is not None else None, backwards == 1])
    return out


def returns_one(body):
    return any(line.strip().endswith("iconst_1") for line in body) and any(
        line.strip().endswith("ireturn") for line in body
    )


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        loot = {
            name: json.loads(jar.read(f"data/minecraft/loot_table/blocks/{name}.json"))
            for name in ("shulker_box", "chest")
        }
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "reference.jar"
        path.write_bytes(inner)

        def inspect(name, *flags):
            args = ["javap", "-c", "-p", *flags, "-classpath", str(path), name]
            return subprocess.check_output(args, text=True)

        quick_move = {key: moves(method(inspect(name), "quickMoveStack")) for key, name in MENUS.items()}
        potion = inspect("net.minecraft.world.inventory.BrewingStandMenu$PotionSlot")
        fuel = inspect("net.minecraft.world.inventory.FurnaceFuelSlot")
        hopper = inspect("net.minecraft.world.level.block.entity.HopperBlockEntity", "-constants")
        speed = re.search(r"static final int MOVE_ITEM_SPEED = (\d+);", hopper)
        signal = method(
            inspect("net.minecraft.world.inventory.AbstractContainerMenu"), "getRedstoneSignalFromContainer"
        )
        lerp = method(inspect("net.minecraft.util.Mth"), "lerpDiscrete")
        block_entity = method(inspect("net.minecraft.world.level.block.entity.BlockEntity"), "preRemoveSideEffects")
        containers = inspect("net.minecraft.world.Containers")
        brewing = method(inspect("net.minecraft.world.level.block.entity.BrewingStandBlockEntity"), "serverTick")
        furnace_set = method(inspect("net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity"), "setItem")
    if not speed:
        raise ValueError("no HopperBlockEntity.MOVE_ITEM_SPEED")
    fuel_load = next(i for i, line in enumerate(brewing) if "ItemStack.shrink" in line)
    shulker_conditions = [
        p.get("conditions", []) + ([p["condition"]] if "condition" in p else []) for p in loot["shulker_box"]["pools"]
    ]
    chest_conditions = [
        p.get("conditions", []) + ([p["condition"]] if "condition" in p else []) for p in loot["chest"]["pools"]
    ]
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "quickMoveStack in ChestMenu, ShulkerBoxMenu, HopperMenu, DispenserMenu, AbstractFurnaceMenu, BrewingStandMenu",
            "BrewingStandMenu$PotionSlot.getMaxStackSize, FurnaceFuelSlot.getMaxStackSize",
            "HopperBlockEntity.MOVE_ITEM_SPEED",
            "AbstractContainerMenu.getRedstoneSignalFromContainer, Mth.lerpDiscrete",
            "BlockEntity.preRemoveSideEffects, Containers",
            "BrewingStandBlockEntity.serverTick, AbstractFurnaceBlockEntity.setItem",
            "loot_table/blocks/shulker_box.json, loot_table/blocks/chest.json",
        ],
        # [start, end, backwards] menu slot ranges; null bounds are computed
        # from the container size (rows * 9 or the container's size).
        "quick_move": quick_move,
        "potion_slot_max": 1 if returns_one(method(potion, "getMaxStackSize")) else None,
        "furnace_fuel_bucket_max": 1 if returns_one(method(fuel, "getMaxStackSize")) else None,
        "hopper_cooldown": int(speed[1]),
        "comparator_float": any("fdiv" in line for line in signal) and any("fmul" in line for line in lerp),
        "contents_drop_without_rules": any("Containers.dropContents" in line for line in block_entity)
        and "GameRules" not in containers,
        "brewing_fuel_load_sets_changed": any("setChanged" in line for line in brewing[fuel_load:]),
        "furnace_input_compares_components": any("isSameItemSameComponents" in line for line in furnace_set),
        "shulker_box_loot_conditions": [c["type"] if isinstance(c, dict) else c for p in shulker_conditions for c in p],
        "chest_loot_conditions": [c["type"] if isinstance(c, dict) else c for p in chest_conditions for c in p],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + "\n")


if __name__ == "__main__":
    main()
