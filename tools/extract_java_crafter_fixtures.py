#!/usr/bin/env python3
"""Extract v0.17 Track C1's crafter facts from the pinned official Java 26.3 server.

Covers `CrafterBlock` (states, placement, tick delays, crafting time, the
ejection and its level events), `CrafterBlockEntity` (slot constants, saved
keys, the slot-toggle rule), `CrafterMenu` (slot layout), `FrontAndTop`
(the orientation order), the server's slot-toggle packet handler, the
crafter's sound events, `Blocks.CRAFTER`'s strength, and the data files:
recipe, loot table, the tags that name the crafter, and every crafting
recipe whose type is a special one (`crafting_special_*`, decorated pots,
transmutes), which the crafter accepts like any crafting recipe.

Requires javap. Each number is read from a named place in the bytecode (the
constant pushed before a known call or field) and an unrecognized form fails
rather than writing a guessed fixture. No VoxelCraft source or existing
fixture is consulted.
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
from extract_java_new_mob_fixtures import constants, method, number, ops, strip

BLOCK = "world.level.block.CrafterBlock"
ENTITY = "world.level.block.entity.CrafterBlockEntity"
MENU = "world.inventory.CrafterMenu"
PLAIN_RECIPES = {"minecraft:crafting_shaped", "minecraft:crafting_shapeless"}


def numbers(listing):
    return [n for n in (number(op) for op in listing) if n is not None]


def before(listing, pattern, count=1, kind=None, window=10, occurrence=0):
    """The last `count` constants (of `kind`, if given) pushed within
    `window` ops before the `occurrence`th op matching `pattern`."""
    seen = 0
    for i, op in enumerate(listing):
        if re.search(pattern, op):
            if seen < occurrence:
                seen += 1
                continue
            found = [n for n in numbers(listing[max(0, i - window) : i]) if kind is None or isinstance(n, kind)]
            if len(found) < count:
                raise ValueError(f"expected {count} constants before {pattern}: {listing[max(0, i - window) : i + 1]}")
            return found[-count:]
    raise ValueError(f"missing op {pattern}")


def all_before(listing, pattern, kind=int):
    """The constant pushed right before every op matching `pattern`."""
    out = []
    for i, op in enumerate(listing):
        if re.search(pattern, op):
            found = [n for n in numbers(listing[max(0, i - 4) : i]) if isinstance(n, kind)]
            if not found:
                raise ValueError(f"no constant before {pattern}: {listing[max(0, i - 4) : i + 1]}")
            out.append(found[0])
    return out


def strings(listing):
    return [m[1] for op in listing if (m := re.search(r"// String (\S+)$", op))]


def offsets(code):
    """`offset -> op` for one method's raw javap text."""
    out = {}
    for line in code.splitlines():
        m = re.match(r"\s+(\d+): (\w+)\s*(.*)", line)
        if m:
            out[int(m[1])] = f"{m[2]} {m[3]}".strip()
    return out


def placement_tops(block, switch_map):
    """`getStateForPlacement`'s `top` for each front: the switch on the
    front's ordinal (`CrafterBlock$1`'s map) jumps to a branch that reads
    the horizontal direction, its opposite, or `Direction.UP`."""
    code = method(block, "getStateForPlacement(")
    cases = {int(c): int(t) for c, t in re.findall(r"^\s+(\d+): (\d+)$", code, re.M)}
    if not cases:
        raise ValueError("getStateForPlacement has no tableswitch")
    at = offsets(code)
    order = sorted(at)

    def branch(start):
        found = []
        for off in order[order.index(start) :]:
            op = at[off]
            if "getHorizontalDirection" in op:
                found.append("horizontal")
            elif "getOpposite" in op and found:
                found.append("opposite")
            elif "Direction.UP" in op:
                return "up"
            if op.startswith("goto") or op.startswith("astore"):
                break
        if found == ["horizontal"]:
            return "horizontal"
        if found == ["horizontal", "opposite"]:
            return "horizontal_opposite"
        raise ValueError(f"unrecognized placement branch at {start}: {found}")

    return {direction: branch(cases[case]) for direction, case in switch_map.items()}


def switch_map(code):
    """`CrafterBlock$1`: `Direction.X.ordinal()` stored as switch case n."""
    listing = ops(method(code, "static {}"))
    out = {}
    for i, op in enumerate(listing):
        m = re.search(r"Direction\.(\w+):", op)
        if m and i + 2 < len(listing) and "ordinal" in listing[i + 1]:
            out[m[1].lower()] = number(listing[i + 2])
    if len(out) != 6:
        raise ValueError(f"expected six directions in the switch map: {out}")
    return out


def front_and_top(code):
    listing = ops(method(code, "static {}"))
    out = []
    for i, op in enumerate(listing):
        if '"<init>":(Ljava/lang/String;ILjava/lang/String;' in op:
            part = listing[max(0, i - 6) : i]
            name = [s for s in strings(part) if s.islower()]
            dirs = [m[1].lower() for p in part if (m := re.search(r"Direction\.(\w+):", p))]
            if len(name) != 1 or len(dirs) != 2:
                raise ValueError(f"unrecognized FrontAndTop constant: {part}")
            out.append({"name": name[0], "front": dirs[0], "top": dirs[1]})
    if len(out) != 12:
        raise ValueError(f"expected 12 FrontAndTop values, got {len(out)}")
    return out


def block_strength(listing, field):
    start = next(i for i, op in enumerate(listing) if re.search(rf"(?:BlockIds|BlockItemIds)\.{field}:", op))
    end = next(i for i in range(start, len(listing)) if "putstatic" in listing[i] and f"Field {field}:" in listing[i])
    part = listing[start:end]
    hardness, resistance = before(part, r"strength:\(FF\)", count=2, kind=float)
    return {
        "hardness": hardness,
        "blast_resistance": resistance,
        "requires_correct_tool": any("requiresCorrectToolForDrops" in op for op in part),
        "map_color": next(m[1].lower() for op in part if (m := re.search(r"MapColor\.(\w+):", op))),
    }


def sound(listing, field):
    for i, op in enumerate(listing):
        if "putstatic" in op and f"Field {field}:" in op:
            found = strings(listing[max(0, i - 3) : i])
            if found:
                return found[-1]
    raise ValueError(f"missing sound {field}")


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-crafter-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        block = inspect(BLOCK)
        entity = inspect(ENTITY)
        menu = inspect(MENU)
        neighbor = ops(method(block, "neighborChanged("))
        dispense = ops(method(block, "dispenseFrom("))
        eject = ops(method(block, "dispenseItem("))
        events = all_before(dispense, r"levelEvent:") + all_before(eject, r"levelEvent:")
        if len(events) != 4 or events[0] != events[1]:
            raise ValueError(f"unexpected crafter level events: {events}")
        crafter_block = {
            "constants": constants(block),
            "properties": [
                m[1].lower()
                for op in ops(method(block, "createBlockStateDefinition("))
                if (m := re.search(r"Field (\w+):Lnet/minecraft/world/level/block/state/properties", op))
            ],
            "neighbor_changed_delay": before(neighbor, r"Level\.scheduleTick:")[0],
            "neighbor_changed_signal": "hasNeighborSignal"
            if any("hasNeighborSignal" in op for op in neighbor)
            else None,
            "placed_by_delay": before(ops(method(block, "setPlacedBy(")), r"Level\.scheduleTick:")[0],
            "placement_signal": "hasNeighborSignal"
            if "hasNeighborSignal" in method(block, "getStateForPlacement(")
            else None,
            "placement_top": placement_tops(block, switch_map(inspect("world.level.block.CrafterBlock$1"))),
            "crafting_ticks": before(dispense, r"setCraftingTicksRemaining:")[0],
            "fail_event": events[0],
            "craft_event": events[2],
            "craft_particles_event": events[3],
            "eject_speed": before(eject, r"DefaultDispenseItemBehavior\.spawnItem:", kind=int)[0],
            "eject_offset": before(eject, r"Vec3\.relative:\(Lnet/minecraft/core/Direction;D\)", kind=float)[0],
            "eject_one_at_a_time_into_crafters": any("instanceof" in op and "CrafterBlockEntity" in op for op in eject),
            "eject_into_container": any("HopperBlockEntity.addItem" in op for op in eject),
            "remainders_ejected": any("getRemainingItems" in op for op in dispense),
            "recipe_cache_size": before(ops(method(block, "static {}")), r"RecipeCache\.\"<init>\"")[0],
            "analog_output": "getRedstoneSignal" in method(block, "getAnalogOutputSignal("),
        }
        crafter_entity = {
            "constants": constants(entity),
            "saved_keys": sorted(
                set(strings(ops(method(entity, "loadAdditional("))))
                | set(strings(ops(method(entity, "addDisabledSlots("))))
                | set(strings(ops(method(entity, "addTriggered("))))
            ),
            "name": strings(ops(method(entity, "static {}")))[0],
            "set_slot_state_requires_empty_slot": "slotCanBeDisabled" in method(entity, "setSlotState("),
            "set_item_reenables_disabled_slot": "setSlotState" in method(entity, "setItem("),
            "can_place_checks_later_smaller_stacks": "smallerStackExist" in method(entity, "canPlaceItem("),
            "signal_counts_filled_or_disabled": all(
                call in method(entity, "getRedstoneSignal(") for call in ("isEmpty", "isSlotDisabled")
            ),
        }
        add_slots = ops(method(menu, "addSlots("))
        crafter_menu = {
            "constants": constants(menu),
            "grid_origin": [n for n in before(add_slots, r"CrafterSlot\.\"<init>\"", count=4, window=16) if n != 18],
            "result_slot": before(add_slots, r"NonInteractiveResultSlot\.\"<init>\"", count=2),
            "inventory_origin": before(add_slots, r"addStandardInventorySlots:", count=2),
            "slot_may_place_checks_disabled": "isSlotDisabled" in inspect("world.inventory.CrafterSlot"),
        }
        handler = method(inspect("server.network.ServerGamePacketListenerImpl"), "handleContainerSlotStateChanged(")
        call = re.search(r"Method net/minecraft/[\w/]+/(\w+)\.setSlotState:", handler)
        sounds = ops(method(inspect("sounds.SoundEvents"), "static {}"))
        blocks = ops(method(inspect("world.level.block.Blocks"), "static {}"))
        orientations = front_and_top(inspect("core.FrontAndTop"))

        with zipfile.ZipFile(path) as jar:

            def read(name):
                return json.loads(jar.read(name))

            tags = {"block": [], "item": []}
            for name in jar.namelist():
                m = re.match(r"data/minecraft/tags/(block|item)/(.+)\.json$", name)
                if m and "minecraft:crafter" in read(name).get("values", []):
                    tags[m[1]].append(m[2])
            special = {}
            for name in jar.namelist():
                m = re.match(r"data/minecraft/recipe/(.+)\.json$", name)
                if not m:
                    continue
                kind = read(name).get("type", "")
                if kind.startswith("minecraft:crafting") and kind not in PLAIN_RECIPES:
                    special[m[1]] = strip(kind)
            recipe = read("data/minecraft/recipe/crafter.json")
            loot = read("data/minecraft/loot_table/blocks/crafter.json")

    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            f"net.minecraft.{BLOCK} (and $1, its placement switch map)",
            f"net.minecraft.{ENTITY} (and $1, its ContainerData)",
            f"net.minecraft.{MENU}, net.minecraft.world.inventory.CrafterSlot",
            "net.minecraft.core.FrontAndTop",
            "net.minecraft.server.network.ServerGamePacketListenerImpl.handleContainerSlotStateChanged",
            "net.minecraft.sounds.SoundEvents (CRAFTER_CRAFT, CRAFTER_FAIL)",
            "net.minecraft.world.level.block.Blocks (CRAFTER)",
            "data/minecraft/recipe/crafter.json, data/minecraft/loot_table/blocks/crafter.json",
            "data/minecraft/tags/{block,item}/*.json naming minecraft:crafter",
            "data/minecraft/recipe/*.json with a special crafting type",
        ],
        "block": crafter_block,
        "block_entity": crafter_entity,
        "menu": crafter_menu,
        "orientations": orientations,
        "slot_state_packet_calls": call[1] if call else None,
        "sounds": {"craft": sound(sounds, "CRAFTER_CRAFT"), "fail": sound(sounds, "CRAFTER_FAIL")},
        "strength": block_strength(blocks, "CRAFTER"),
        "recipe": recipe,
        "loot_table": loot,
        "tags": {kind: sorted(names) for kind, names in tags.items()},
        "special_recipes": dict(sorted(special.items())),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
