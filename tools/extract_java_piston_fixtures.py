#!/usr/bin/env python3
"""Extract Java 26.3 piston behaviour from the pinned official server jar.

Writes `tests/fixtures/java/26.3/pistons.json`: every block's `PushReaction`
with whether it is an `EntityBlock` and its destroy time (the inputs of
`PistonBaseBlock.isPushable`), the slime and honey adhesion rules of
`PistonStructureResolver`, the progress and entity push constants of
`PistonMovingBlockEntity` and `Entity`, `PistonBaseBlock`'s block event
triggers, and the entities that ignore pistons. Never reads VoxelCraft source.

Usage: extract_java_piston_fixtures.py server.jar --output pistons.json
(`javap` must be on PATH).
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
from java_block_properties import block_push_properties

PISTON = "net.minecraft.world.level.block.piston."
BLOCKS_FIELD = r"Field net/minecraft/world/level/block/Blocks\.([A-Z_]+):"
# Entity classes whose `getPistonPushReaction` may differ from `Entity`'s.
ENTITY_CLASSES = [
    "Entity",
    "Display",
    "Marker",
    "Interaction",
    "AreaEffectCloud",
    "OminousItemSpawner",
    "decoration.ArmorStand",
]


def javap(jar, name, *flags):
    return subprocess.check_output(["javap", "-p", *flags, "-classpath", str(jar), name], text=True)


def method(code, header):
    """The bytecode of the method whose declaration line contains `header`."""
    for block in code.split("\n\n"):
        lines = [line for line in block.split("\n") if line.strip()]
        if lines and header in lines[0]:
            return block
    raise ValueError(f"no method {header}")


def constant(text, name):
    match = re.search(r"static final \w+ " + name + r" = ([-\w.]+?)[dfL]?;", text)
    if not match:
        raise ValueError(f"no constant {name}")
    value = match[1]
    return float(value) if "." in value else int(value)


def literals(code, kind):
    return [float(v) for v in re.findall(r"// " + kind + r" ([-\d.E]+)[df]", code)]


def blocks(code):
    return [name.lower() for name in re.findall(BLOCKS_FIELD, code)]


def only(values, what):
    values = list(dict.fromkeys(values))
    if len(values) != 1:
        raise ValueError(f"{what}: expected one value, found {values}")
    return values[0]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar = Path(directory) / "reference.jar"
        jar.write_bytes(inner)
        props = block_push_properties(jar)

        resolver = javap(jar, PISTON + "PistonStructureResolver", "-c", "-constants")
        moving = javap(jar, PISTON + "PistonMovingBlockEntity", "-c", "-constants")
        base = javap(jar, PISTON + "PistonBaseBlock", "-c", "-constants")
        entity = javap(jar, "net.minecraft.world.entity.Entity", "-c")

        sticky = blocks(method(resolver, "boolean isSticky("))
        stick = method(resolver, "boolean canStickToEachOther(")
        stick_blocks = blocks(stick)
        # `canStickToEachOther` rejects (honey, slime) and (slime, honey),
        # then accepts any pair with a sticky block.
        if stick_blocks[:4] != ["honey_block", "slime_block", "slime_block", "honey_block"]:
            raise ValueError(f"reassess canStickToEachOther: {stick_blocks}")
        if "isSticky" not in stick:
            raise ValueError("canStickToEachOther no longer falls back to isSticky")

        tick = method(moving, "void tick(net.minecraft.world.level.Level")
        collided = method(moving, "void moveCollidedEntities(")
        stuck = method(moving, "void moveStuckEntities(")
        related = method(moving, "getCollisionRelatedBlockState(")
        check = method(base, "void checkIfExtend(")
        restriction = method(entity, "double applyPistonMovementRestriction(")

        ignoring = []
        for name in ENTITY_CLASSES:
            code = javap(jar, "net.minecraft.world.entity." + name, "-c")
            body = method(code, "PushReaction getPistonPushReaction()")
            first = re.search(r"Field net/minecraft/world/level/material/PushReaction\.(\w+)", body)
            if name == "Entity":
                if first is None or first[1] != "PUSH_PULL":
                    raise ValueError("Entity's default push reaction changed")
                continue
            if first is None or first[1] != "IGNORE_ENTITY":
                raise ValueError(f"{name}: reassess its push reaction")
            conditional = "isMarker" in body
            ignoring.append({"entity": name.rsplit(".", 1)[-1], "only_markers": conditional})

    bounds = literals(restriction, "double")
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "Blocks static initializer: BlockBehaviour.Properties.pushReaction and each block class",
            "PistonBaseBlock.isPushable, checkIfExtend, triggerEvent and moveBlocks",
            "PistonStructureResolver.MAX_PUSH_DEPTH, isSticky and canStickToEachOther",
            "PistonMovingBlockEntity.tick, moveCollidedEntities, moveStuckEntities and constants",
            "Entity.applyPistonMovementRestriction and getPistonPushReaction overrides",
        ],
        "constants": {
            "max_push": constant(resolver, "MAX_PUSH_DEPTH"),
            "ticks_to_extend": constant(moving, "TICKS_TO_EXTEND"),
            "progress_step": only(literals(tick, "float"), "tick progress step"),
            "push_offset": constant(moving, "PUSH_OFFSET"),
            "tick_movement": constant(moving, "TICK_MOVEMENT"),
            "entity_axis_limit": only([abs(v) for v in bounds], "piston movement restriction"),
            "sticky_top": only(literals(stuck, "double"), "honey carry height"),
            "short_head_progress": only(literals(related, "float"), "short head progress"),
            "spit_progress": only(literals(check, "float"), "drop progress"),
            "trigger_extend": constant(base, "TRIGGER_EXTEND"),
            "trigger_contract": constant(base, "TRIGGER_CONTRACT"),
            "trigger_drop": constant(base, "TRIGGER_DROP"),
        },
        "adhesion": {
            "sticky": sticky,
            "never_stick": [["honey_block", "slime_block"]],
            "bounces_entities": only(blocks(collided), "bouncing block"),
            "carries_entities": only(blocks(method(moving, "boolean isStickyForEntities(")), "carrying block"),
        },
        "entities_ignoring_pistons": ignoring,
        "block_push_reactions": {name: v["push_reaction"] for name, v in sorted(props.items())},
        "block_entities": sorted(name for name, v in props.items() if v["block_entity"]),
        "unbreakable": sorted(name for name, v in props.items() if v["destroy_time"] == -1.0),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + "\n")


if __name__ == "__main__":
    main()
