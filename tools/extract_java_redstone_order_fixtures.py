#!/usr/bin/env python3
"""Extract Java 26.3 redstone update order facts and model order fixtures.

Reads the pinned server jar with javap and checks the bytecode forms the
order depends on: `NeighborUpdater.UPDATE_ORDER`,
`BlockBehaviour.UPDATE_SHAPE_ORDER`, the `Direction` and `Direction.Plane`
orders, `TickPriority` values, the chained-update and scheduled-tick caps,
`Level.setBlock` and `LevelChunk.setBlockState` call order, the
`DefaultRedstoneWireEvaluator` update set, `Vec3i.hashCode`, the diode,
comparator, torch, lamp and observer delays and priorities, and the
`ServerLevel.tick` phase order. Unknown bytecode forms fail rather than
regenerating a guessed fixture.

It then runs a small independent model of the Java classes
(`CollectingNeighborUpdater`, `Level.setBlock`, `SignalGetter`,
`RedStoneWireBlock` with `DefaultRedstoneWireEvaluator`, `DiodeBlock`,
`RepeaterBlock`, `ComparatorBlock`, `RedstoneTorchBlock`,
`RedstoneWallTorchBlock`, `RedstoneLampBlock`, `LeverBlock`,
`ObserverBlock`, `PistonBaseBlock.checkIfExtend`, `LevelTicks` and
`ServerLevel.runBlockEvents`), written from the decompiled 26.3 classes,
over seven small contraptions, and records each one's per-tick states and
the order its updates and ticks ran. The game is compared with the fixture
in `tests/redstone_order.rs`. No VoxelCraft source or existing fixture is
consulted.

Model limits (the fixtures avoid them): wire connections are derived from
the neighbours as `getConnectionState` would set them, so wire shape
updates never change a wire; pistons stop at the extension event; there are
no containers, so `updateNeighbourForOutputSignal` does nothing.
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

DIRS = {
    "down": (0, -1, 0),
    "up": (0, 1, 0),
    "north": (0, 0, -1),
    "south": (0, 0, 1),
    "west": (-1, 0, 0),
    "east": (1, 0, 0),
}
OPPOSITE = {"down": "up", "up": "down", "north": "south", "south": "north", "west": "east", "east": "west"}
CLOCKWISE = {"north": "east", "east": "south", "south": "west", "west": "north"}
AXIS = {"down": "y", "up": "y", "north": "z", "south": "z", "west": "x", "east": "x"}
UPPER = {name.upper(): name for name in DIRS}


# ---------------------------------------------------------------- javap facts


def method(text, signature):
    start = text.find(signature)
    if start < 0:
        raise ValueError(f"missing method {signature}")
    end = text.find("\n\n", start)
    return text[start : end if end >= 0 else len(text)]


def static_init(text):
    return method(text, "static {};")


def direction_fields(body, field):
    """Direction constants loaded before `putstatic field`."""
    before = body.split(f"// Field {field}:")[0]
    last = before.rfind("anewarray")
    names = re.findall(r"getstatic\s+#\d+\s+// Field net/minecraft/core/Direction\.([A-Z]+):", before[last:])
    return [UPPER[n] for n in names]


def calls(body):
    return re.findall(r"invoke\w+\s+#\d+\s+// (?:Interface)?Method ([\w/$.]+?):", body)


def in_order(found, wanted, what):
    position = 0
    for name in wanted:
        try:
            position = found.index(name, position) + 1
        except ValueError:
            raise ValueError(f"{what}: {name} missing or out of order in {found}") from None


def expect(condition, what):
    if not condition:
        raise ValueError(f"reassess {what}")


def facts(inspect):
    out = {}
    out["update_order"] = direction_fields(static_init(inspect("world.level.redstone.NeighborUpdater")), "UPDATE_ORDER")
    out["update_shape_order"] = direction_fields(
        static_init(inspect("world.level.block.state.BlockBehaviour")), "UPDATE_SHAPE_ORDER"
    )
    direction = static_init(inspect("core.Direction"))
    out["direction_values"] = [
        UPPER[n] for n in re.findall(r"putstatic\s+#\d+\s+// Field ([A-Z]+):Lnet/minecraft/core/Direction;", direction)
    ]
    plane = static_init(inspect("core.Direction$Plane"))
    horizontal, vertical = plane.split("// Field HORIZONTAL:")[0], plane.split("// Field HORIZONTAL:")[1]
    out["horizontal"] = [
        UPPER[n] for n in re.findall(r"getstatic\s+#\d+\s+// Field net/minecraft/core/Direction\.([A-Z]+):", horizontal)
    ]
    out["vertical"] = [
        UPPER[n]
        for n in re.findall(
            r"getstatic\s+#\d+\s+// Field net/minecraft/core/Direction\.([A-Z]+):",
            vertical.split("// Field VERTICAL:")[0],
        )
    ]
    priority = static_init(inspect("world.ticks.TickPriority"))
    values = {}
    for block in priority.split("new ")[1:]:
        name = re.search(r"// String ([A-Z_]+)", block)
        number = re.findall(r"(iconst_m1|iconst_(\d)|bipush\s+(-?\d+))", block)
        if name and len(number) >= 2:
            raw = number[1]
            values[name[1]] = -1 if raw[0] == "iconst_m1" else int(raw[1] or raw[2])
    out["tick_priority"] = values
    server = method(inspect("server.MinecraftServer"), "public int getMaxChainedNeighborUpdates();")
    out["max_chained_neighbor_updates"] = int(re.search(r"// int (\d+)", server)[1])
    level_tick = method(inspect("server.level.ServerLevel"), "public void tick(java.util.function.BooleanSupplier);")
    out["max_scheduled_ticks"] = int(re.search(r"// int (\d+)", level_tick)[1])
    phases = calls(level_tick)
    in_order(
        phases,
        [
            "net/minecraft/world/ticks/LevelTicks.tick",
            "net/minecraft/world/ticks/LevelTicks.tick",
            "net/minecraft/world/entity/raid/Raids.tick",
            "net/minecraft/server/level/ServerChunkCache.tick",
            "runBlockEvents",
            "net/minecraft/world/level/entity/EntityTickList.forEach",
            "tickBlockEntities",
        ],
        "ServerLevel.tick phases",
    )
    out["server_level_tick_phases"] = [
        "block_ticks",
        "fluid_ticks",
        "raids",
        "chunk_source",
        "block_events",
        "entities",
        "block_entities",
    ]
    set_block = method(
        inspect("world.level.Level"),
        "public boolean setBlock(net.minecraft.core.BlockPos, net.minecraft.world.level.block.state.BlockState, int, int);",
    )
    in_order(
        [c.rsplit(".", 1)[-1] for c in calls(set_block)],
        [
            "setBlockState",
            "updateNeighborsAt",
            "updateNeighbourForOutputSignal",
            "updateIndirectNeighbourShapes",
            "updateNeighbourShapes",
            "updateIndirectNeighbourShapes",
        ],
        "Level.setBlock",
    )
    expect("bipush        -34" in set_block, "Level.setBlock neighbour flags mask")
    chunk = method(
        inspect("world.level.chunk.LevelChunk"),
        "public net.minecraft.world.level.block.state.BlockState setBlockState(",
    )
    in_order(
        [c.rsplit(".", 1)[-1] for c in calls(chunk)],
        ["affectNeighborsAfterRemoval", "onPlace"],
        "LevelChunk.setBlockState",
    )
    out["set_block_order"] = [
        "affect_neighbors_after_removal",
        "on_place",
        "update_neighbors_at",
        "update_neighbour_for_output_signal",
        "old_indirect_shapes",
        "neighbour_shapes",
        "new_indirect_shapes",
    ]
    evaluator = method(inspect("world.level.redstone.DefaultRedstoneWireEvaluator"), "public void updatePowerStrength(")
    in_order(
        [c.rsplit(".", 1)[-1] for c in calls(evaluator)],
        ["calculateTargetStrength", "setBlock", "newHashSet", "values", "updateNeighborsAt"],
        "DefaultRedstoneWireEvaluator.updatePowerStrength",
    )
    expect("iconst_2" in evaluator, "wire power set flags")
    vec = method(inspect("core.Vec3i"), "public int hashCode();")
    expect(vec.count("bipush        31") == 2, "Vec3i.hashCode")
    diode = inspect("world.level.block.DiodeBlock")
    check = method(diode, "protected void checkTickOnNeighbor(")
    out["diode_priorities"] = re.findall(r"Field net/minecraft/world/ticks/TickPriority\.([A-Z_]+):", check)
    expect(out["diode_priorities"] == ["HIGH", "EXTREMELY_HIGH", "VERY_HIGH"], "DiodeBlock.checkTickOnNeighbor")
    diode_tick = method(diode, "protected void tick(")
    expect(re.findall(r"TickPriority\.([A-Z_]+):", diode_tick) == ["VERY_HIGH"], "DiodeBlock.tick")
    repeater = method(inspect("world.level.block.RepeaterBlock"), "protected int getDelay(")
    expect("iconst_2" in repeater and "imul" in repeater, "RepeaterBlock.getDelay")
    comparator = inspect("world.level.block.ComparatorBlock")
    check = method(comparator, "protected void checkTickOnNeighbor(")
    out["comparator_priorities"] = re.findall(r"TickPriority\.([A-Z_]+):", check)
    expect(out["comparator_priorities"] == ["HIGH", "NORMAL"] and "iconst_2" in check, "ComparatorBlock priorities")
    torch = inspect("world.level.block.RedstoneTorchBlock")
    torch_tick = method(torch, "protected void tick(")
    expect("// long 60l" in torch_tick and "sipush        160" in torch_tick, "RedstoneTorchBlock.tick")
    expect("bipush        8" in method(torch, "private static boolean isToggledTooFrequently("), "torch toggle count")
    expect("iconst_2" in method(torch, "protected void neighborChanged("), "torch delay")
    out["torch"] = {"delay": 2, "toggle_window": 60, "toggle_limit": 8, "burnout_delay": 160}
    lamp = method(inspect("world.level.block.RedstoneLampBlock"), "protected void neighborChanged(")
    expect("iconst_4" in lamp, "RedstoneLampBlock delay")
    observer = inspect("world.level.block.ObserverBlock")
    expect("iconst_2" in method(observer, "private void startSignal("), "ObserverBlock.startSignal")
    expect("iconst_2" in method(observer, "protected void tick("), "ObserverBlock.tick")
    out["delays"] = {"comparator": 2, "repeater_per_level": 2, "lamp_off": 4, "observer": 2}
    events = inspect("server.level.ServerLevel")
    expect("ObjectLinkedOpenHashSet" in events, "ServerLevel.blockEvents set")
    expect("removeFirst" in method(events, "private void runBlockEvents();"), "runBlockEvents drain")
    return out


# ------------------------------------------------------------------- model


def add(p, d):
    return (p[0] + d[0], p[1] + d[1], p[2] + d[2])


def rel(p, name):
    return add(p, DIRS[name])


AIR = {"b": "air"}


class Model:
    """The Java classes named in the module docstring, for these blocks only."""

    def __init__(self, f):
        self.f = f
        self.blocks = {}
        self.time = 0
        self.sub = 0
        self.ticks = []
        self.to_run_set = set()
        self.events = []
        self.output = {}
        self.toggles = []
        self.stack = []
        self.added = []
        self.count = 0
        self.should_signal = True
        self.log = None

    # --- state helpers
    def get(self, p):
        return self.blocks.get(p, AIR)

    def conductor(self, s):
        return s["b"] in ("stone", "lamp")

    def sturdy(self, s):
        return s["b"] in ("stone", "lamp", "redstone_block", "observer", "piston")

    def signal_source(self, s):
        if s["b"] == "wire":
            return self.should_signal
        return s["b"] in ("lever", "repeater", "comparator", "torch", "wall_torch", "redstone_block", "observer")

    def is_diode(self, s):
        return s["b"] in ("repeater", "comparator")

    def name(self, s):
        b = s["b"]
        if b == "wire":
            return f"wire/{s['power']}"
        if b == "lever":
            return f"lever/{int(s['on'])}"
        if b == "repeater":
            return f"repeater/{s['facing']}/{s['delay']}/{int(s['on'])}"
        if b == "comparator":
            return f"comparator/{s['facing']}/{s['mode']}/{int(s['on'])}"
        if b == "torch":
            return f"torch/{int(s['lit'])}"
        if b == "wall_torch":
            return f"wall_torch/{s['facing']}/{int(s['lit'])}"
        if b == "lamp":
            return f"lamp/{int(s['lit'])}"
        if b == "observer":
            return f"observer/{s['facing']}/{int(s['on'])}"
        if b == "piston":
            return f"piston/{s['facing']}/{int(s['extended'])}"
        return b

    def trace(self, kind, p):
        s = self.get(p)
        if self.log is not None and s["b"] not in ("air", "stone"):
            self.log.append([kind, *p, self.name(s)])

    # --- signals (`SignalGetter`, block `getSignal`/`getDirectSignal`)
    def block_signal(self, s, p, d):
        b = s["b"]
        if b == "redstone_block":
            return 15
        if b == "lever":
            return 15 if s["on"] else 0
        if b == "wire":
            if not self.should_signal or d == "down" or s["power"] == 0:
                return 0
            if d != "up" and self.wire_sides(p)[OPPOSITE[d]] == "none":
                return 0
            return s["power"]
        if b == "torch":
            return 15 if s["lit"] and d != "up" else 0
        if b == "wall_torch":
            return 15 if s["lit"] and s["facing"] != d else 0
        if b == "repeater":
            return 15 if s["facing"] == d and s["on"] else 0
        if b == "comparator":
            return self.output.get(p, 0) if s["facing"] == d and s["on"] else 0
        if b == "observer":
            return 15 if s["facing"] == d and s["on"] else 0
        return 0

    def block_direct(self, s, p, d):
        b = s["b"]
        if b == "wire":
            return self.block_signal(s, p, d) if self.should_signal else 0
        if b == "lever":
            connected = "up" if s["face"] == "floor" else s["facing"]
            return 15 if s["on"] and connected == d else 0
        if b in ("torch", "wall_torch"):
            return self.block_signal(s, p, d) if d == "down" else 0
        if b in ("repeater", "comparator", "observer"):
            return self.block_signal(s, p, d)
        return 0

    def direct_signal_to(self, p):
        best = 0
        for d in self.f["direction_values"]:
            best = max(best, self.block_direct(self.get(rel(p, d)), rel(p, d), d))
            if best >= 15:
                return best
        return best

    def signal(self, p, d):
        s = self.get(p)
        value = self.block_signal(s, p, d)
        return max(value, self.direct_signal_to(p)) if self.conductor(s) else value

    def has_neighbor_signal(self, p):
        return any(self.signal(rel(p, d), d) > 0 for d in self.f["direction_values"])

    def best_neighbor_signal(self, p):
        best = 0
        for d in self.f["direction_values"]:
            value = self.signal(rel(p, d), d)
            if value >= 15:
                return 15
            best = max(best, value)
        return best

    def control_input(self, p, d, diodes_only):
        s = self.get(p)
        if diodes_only:
            return self.block_direct(s, p, d) if self.is_diode(s) else 0
        if s["b"] == "redstone_block":
            return 15
        if s["b"] == "wire":
            return s["power"]
        return self.block_direct(s, p, d) if self.signal_source(s) else 0

    # --- wire (`RedStoneWireBlock`, `RedstoneWireEvaluator`)
    def connects(self, s, d):
        if s["b"] == "wire":
            return True
        if s["b"] == "repeater":
            return d in (s["facing"], OPPOSITE[s["facing"]])
        if s["b"] == "observer":
            return d == s["facing"]
        return self.signal_source(s) and d is not None

    def connecting_side(self, p, d, can_up):
        q = rel(p, d)
        s = self.get(q)
        if can_up and self.sturdy(s) and self.connects(self.get(rel(q, "up")), None):
            return "up" if self.sturdy(s) else "side"
        if not self.connects(s, d) and (self.conductor(s) or not self.connects(self.get(rel(q, "down")), None)):
            return "none"
        return "side"

    def wire_sides(self, p):
        can_up = not self.conductor(self.get(rel(p, "up")))
        sides = {d: self.connecting_side(p, d, can_up) for d in self.f["horizontal"]}
        ns_empty = sides["north"] == "none" and sides["south"] == "none"
        ew_empty = sides["east"] == "none" and sides["west"] == "none"
        for d, empty in (("west", ns_empty), ("east", ns_empty), ("north", ew_empty), ("south", ew_empty)):
            if sides[d] == "none" and empty:
                sides[d] = "side"
        return sides

    def wire_target(self, p):
        self.should_signal = False
        block = self.best_neighbor_signal(p)
        self.should_signal = True
        if block == 15:
            return 15
        wire = 0
        for d in self.f["horizontal"]:
            q = rel(p, d)
            s = self.get(q)
            if s["b"] == "wire":
                wire = max(wire, s["power"])
            above = rel(p, "up")
            if self.conductor(s) and not self.conductor(self.get(above)):
                t = self.get(rel(q, "up"))
                if t["b"] == "wire":
                    wire = max(wire, t["power"])
            elif not self.conductor(s):
                t = self.get(rel(q, "down"))
                if t["b"] == "wire":
                    wire = max(wire, t["power"])
        return max(block, wire - 1)

    def update_power(self, p, s):
        target = self.wire_target(p)
        if s["power"] == target:
            return
        if self.get(p) == s:
            self.set_block(p, dict(s, power=target), 2)
        cells = [p] + [rel(p, d) for d in self.f["direction_values"]]
        for q in java_hash_set(cells):
            self.update_neighbors_at(q)

    def check_corner(self, p):
        if self.get(p)["b"] == "wire":
            self.update_neighbors_at(p)
            for d in self.f["direction_values"]:
                self.update_neighbors_at(rel(p, d))

    def neighboring_wires(self, p):
        for d in self.f["horizontal"]:
            self.check_corner(rel(p, d))
        for d in self.f["horizontal"]:
            q = rel(p, d)
            self.check_corner(rel(q, "up") if self.conductor(self.get(q)) else rel(q, "down"))

    def wire_indirect(self, p, s, flags, limit):
        if s["b"] != "wire":
            return
        sides = self.wire_sides(p)
        for d in self.f["horizontal"]:
            q = rel(p, d)
            if sides[d] != "none" and self.get(q)["b"] != "wire":
                for r in (rel(q, "down"), rel(q, "up")):
                    if self.get(r)["b"] == "wire":
                        self.neighbor_shape_changed(OPPOSITE[d], r, flags, limit)

    # --- updater (`CollectingNeighborUpdater`)
    def add_and_run(self, update):
        running = self.count > 0
        too_many = self.count >= self.f["max_chained_neighbor_updates"]
        self.count += 1
        if not too_many:
            (self.added if running else self.stack).append(update)
        if not running:
            self.run_updates()

    def run_updates(self):
        while self.stack or self.added:
            for update in reversed(self.added):
                self.stack.append(update)
            self.added.clear()
            top = self.stack[-1]
            while not self.added:
                if not self.run_next(top):
                    self.stack.pop()
                    break
        self.stack.clear()
        self.added.clear()
        self.count = 0

    def run_next(self, u):
        if u[0] == "neighbor":
            self.neighbor_changed(u[1])
            return False
        if u[0] == "shape":
            _, p, d, flags, limit = u
            self.trace("s", p)
            current = self.get(p)
            new = self.update_shape(current, p, d)
            if new != current:
                if new["b"] == "air":
                    self.set_block(p, AIR, 3, limit)
                else:
                    self.set_block(p, new, flags & ~32, limit)
            return False
        order = self.f["update_order"]
        d = order[u[3]]
        u[3] += 1
        self.neighbor_changed(rel(u[1], d))
        if u[3] < 6 and order[u[3]] == u[2]:
            u[3] += 1
        return u[3] < 6

    def update_neighbors_at(self, p, skip=None):
        update = ["multi", p, skip, 0]
        if self.f["update_order"][0] == skip:
            update[3] = 1
        self.add_and_run(update)

    def neighbor_changed_at(self, p):
        self.add_and_run(("neighbor", p))

    def neighbor_shape_changed(self, d, p, flags, limit):
        self.add_and_run(("shape", p, d, flags, limit))

    # --- `Level.setBlock` and `LevelChunk.setBlockState`
    def set_block(self, p, s, flags, limit=512):
        old = self.get(p)
        if old == s:
            return False
        self.blocks[p] = s
        moved = flags & 64 != 0
        if old["b"] != s["b"] and (flags & 1 or moved):
            self.after_removal(old, p, moved)
        if self.get(p)["b"] != s["b"]:
            return False
        if not flags & 512:
            self.on_place(s, p, old)
        if self.get(p) != s:
            return True
        if flags & 1:
            self.update_neighbors_at(p)
        if not flags & 16 and limit > 0:
            neighbour_flags = flags & -34
            self.wire_indirect(p, old, neighbour_flags, limit - 1)
            for d in self.f["update_shape_order"]:
                self.neighbor_shape_changed(OPPOSITE[d], rel(p, d), neighbour_flags, limit - 1)
            self.wire_indirect(p, s, neighbour_flags, limit - 1)
        return True

    def in_front(self, p, out, skip):
        front = rel(p, out)
        self.neighbor_changed_at(front)
        self.update_neighbors_at(front, skip)

    def after_removal(self, s, p, moved):
        b = s["b"]
        if b == "wire" and not moved:
            for d in self.f["direction_values"]:
                self.update_neighbors_at(rel(p, d))
            self.update_power(p, s)
            self.neighboring_wires(p)
        elif b in ("torch", "wall_torch") and not moved:
            for d in self.f["direction_values"]:
                self.update_neighbors_at(rel(p, d))
        elif b in ("repeater", "comparator") and not moved:
            self.in_front(p, OPPOSITE[s["facing"]], s["facing"])
        elif b == "observer" and s["on"] and self.has_tick(p, "observer"):
            self.in_front(p, OPPOSITE[s["facing"]], s["facing"])
        elif b == "lever" and not moved and s["on"]:
            self.lever_neighbours(p, s)

    def on_place(self, s, p, old):
        b = s["b"]
        if b == "wire" and old["b"] != "wire":
            self.update_power(p, s)
            for d in self.f["vertical"]:
                self.update_neighbors_at(rel(p, d))
            self.neighboring_wires(p)
        elif b in ("torch", "wall_torch"):
            for d in self.f["direction_values"]:
                self.update_neighbors_at(rel(p, d))
        elif b in ("repeater", "comparator"):
            self.in_front(p, OPPOSITE[s["facing"]], s["facing"])
        elif b == "observer" and old["b"] != "observer" and s["on"] and not self.has_tick(p, "observer"):
            self.set_block(p, dict(s, on=False), 18)
            self.in_front(p, OPPOSITE[s["facing"]], s["facing"])
        elif b == "piston" and old["b"] != "piston":
            self.check_extend(p, s)

    # --- `neighborChanged`
    def neighbor_changed(self, p):
        self.trace("n", p)
        s = self.get(p)
        b = s["b"]
        if b == "wire":
            self.update_power(p, s)
        elif b in ("repeater", "comparator"):
            self.diode_check(p, s)
        elif b in ("torch", "wall_torch"):
            if s["lit"] == self.torch_signal(p, s) and not self.will_tick(p, b):
                self.schedule(p, b, 2)
        elif b == "lamp":
            if s["lit"] != self.has_neighbor_signal(p):
                if s["lit"]:
                    self.schedule(p, b, 4)
                else:
                    self.set_block(p, dict(s, lit=True), 2)
        elif b == "piston":
            self.check_extend(p, s)

    # --- `updateShape`
    def update_shape(self, s, p, d):
        b = s["b"]
        if b == "observer" and s["facing"] == d and not s["on"]:
            if not self.has_tick(p, "observer"):
                self.schedule(p, "observer", 2)
        if b == "repeater":
            if d == "down" and not self.sturdy(self.get(rel(p, d))):
                return AIR
            if AXIS[d] != AXIS[s["facing"]]:
                return dict(s, locked=self.repeater_locked(p, s))
        return s

    # --- diodes
    def diode_input(self, p, s):
        q = rel(p, s["facing"])
        value = self.signal(q, s["facing"])
        if value >= 15:
            return value
        t = self.get(q)
        return max(value, t["power"] if t["b"] == "wire" else 0)

    def alternate(self, p, s, diodes_only):
        cw = CLOCKWISE[s["facing"]]
        ccw = OPPOSITE[cw]
        return max(self.control_input(rel(p, cw), cw, diodes_only), self.control_input(rel(p, ccw), ccw, diodes_only))

    def repeater_locked(self, p, s):
        return self.alternate(p, s, True) > 0

    def comparator_output(self, p, s):
        value = self.diode_input(p, s)
        if value == 0:
            return 0
        side = self.alternate(p, s, False)
        if side > value:
            return 0
        return value - side if s["mode"] == "subtract" else value

    def should_turn_on(self, p, s):
        if s["b"] == "repeater":
            return self.diode_input(p, s) > 0
        value = self.diode_input(p, s)
        if value == 0:
            return False
        side = self.alternate(p, s, False)
        return value > side or (value == side and s["mode"] == "compare")

    def prioritized(self, p, s):
        out = OPPOSITE[s["facing"]]
        front = self.get(rel(p, out))
        return self.is_diode(front) and front["facing"] != out

    def diode_check(self, p, s):
        if s["b"] == "repeater":
            if self.repeater_locked(p, s):
                return
            should = self.should_turn_on(p, s)
            if s["on"] != should and not self.will_tick(p, "repeater"):
                if self.prioritized(p, s):
                    priority = "EXTREMELY_HIGH"
                elif s["on"]:
                    priority = "VERY_HIGH"
                else:
                    priority = "HIGH"
                self.schedule(p, "repeater", 2 * s["delay"], priority)
        elif not self.will_tick(p, "comparator"):
            if self.comparator_output(p, s) != self.output.get(p, 0) or s["on"] != self.should_turn_on(p, s):
                priority = "HIGH" if self.prioritized(p, s) else "NORMAL"
                self.schedule(p, "comparator", 2, priority)

    def comparator_refresh(self, p, s):
        value = self.comparator_output(p, s)
        old = self.output.get(p, 0)
        self.output[p] = value
        if old != value or s["mode"] == "compare":
            should = self.should_turn_on(p, s)
            if s["on"] != should:
                self.set_block(p, dict(s, on=should), 2)
            self.in_front(p, OPPOSITE[s["facing"]], s["facing"])

    # --- torches
    def torch_signal(self, p, s):
        toward = "down" if s["b"] == "torch" else OPPOSITE[s["facing"]]
        return self.signal(rel(p, toward), toward) > 0

    def too_frequent(self, p, record):
        if record:
            self.toggles.append((p, self.time))
        return sum(1 for q, _ in self.toggles if q == p) >= 8

    # --- lever and pistons
    def lever_neighbours(self, p, s):
        support = "down" if s["face"] == "floor" else OPPOSITE[s["facing"]]
        self.update_neighbors_at(p)
        self.update_neighbors_at(rel(p, support))

    def piston_signal(self, p, facing):
        values = self.f["direction_values"]
        if any(d != facing and self.signal(rel(p, d), d) > 0 for d in values):
            return True
        if self.signal(p, "down") > 0:
            return True
        above = rel(p, "up")
        return any(d != "down" and self.signal(rel(above, d), d) > 0 for d in values)

    def check_extend(self, p, s):
        should = self.piston_signal(p, s["facing"])
        if should and not s["extended"]:
            self.block_event(p, "piston", 0, self.f["direction_values"].index(s["facing"]))
        elif not should and s["extended"]:
            self.block_event(p, "piston", 1, self.f["direction_values"].index(s["facing"]))

    # --- scheduled ticks (`LevelTicks`) and block events
    def has_tick(self, p, block):
        return any(t[3] == p and t[4] == block for t in self.ticks)

    def will_tick(self, p, block):
        return (p, block) in self.to_run_set

    def schedule(self, p, block, delay, priority="NORMAL"):
        self.sub += 1
        if not self.has_tick(p, block):
            self.ticks.append((self.time + delay, self.f["tick_priority"][priority], self.sub, p, block))

    def block_event(self, p, block, a, b):
        event = (p, block, a, b)
        if event not in self.events:
            self.events.append(event)

    def tick(self):
        self.time += 1
        due = sorted(t for t in self.ticks if t[0] <= self.time)[: self.f["max_scheduled_ticks"]]
        for t in due:
            self.ticks.remove(t)
        run = [(t[3], t[4]) for t in due]
        self.to_run_set = set(run)
        for p, block in run:
            self.to_run_set.discard((p, block))
            if self.get(p)["b"] == block:
                self.trace("t", p)
                self.scheduled_tick(p, self.get(p))
        self.to_run_set = set()
        while self.events:
            p, block, a, _ = self.events.pop(0)
            if self.get(p)["b"] == block:
                self.trace("e", p)
                s = self.get(p)
                should = self.piston_signal(p, s["facing"])
                if a == 0 and should and not s["extended"]:
                    # The model stops at the extension (no moving blocks).
                    self.blocks[p] = dict(s, extended=True)

    def scheduled_tick(self, p, s):
        b = s["b"]
        if b == "repeater":
            if self.repeater_locked(p, s):
                return
            should = self.should_turn_on(p, s)
            if s["on"] and not should:
                self.set_block(p, dict(s, on=False), 2)
            elif not s["on"]:
                self.set_block(p, dict(s, on=True), 2)
                if not should:
                    self.schedule(p, b, 2 * s["delay"], "VERY_HIGH")
        elif b == "comparator":
            self.comparator_refresh(p, s)
        elif b in ("torch", "wall_torch"):
            powered = self.torch_signal(p, s)
            while self.toggles and self.time - self.toggles[0][1] > 60:
                self.toggles.pop(0)
            if s["lit"]:
                if powered:
                    self.set_block(p, dict(s, lit=False), 3)
                    if self.too_frequent(p, True):
                        self.schedule(p, b, 160)
            elif not powered and not self.too_frequent(p, False):
                self.set_block(p, dict(s, lit=True), 3)
        elif b == "lamp":
            if s["lit"] and not self.has_neighbor_signal(p):
                self.set_block(p, dict(s, lit=False), 2)
        elif b == "observer":
            if s["on"]:
                self.set_block(p, dict(s, on=False), 2)
            else:
                self.set_block(p, dict(s, on=True), 2)
                self.schedule(p, b, 2)
            self.in_front(p, OPPOSITE[s["facing"]], s["facing"])

    # --- player actions
    def use(self, p):
        s = self.get(p)
        if s["b"] == "lever":
            s = dict(s, on=not s["on"])
            self.set_block(p, s, 3)
            self.lever_neighbours(p, s)
        elif s["b"] == "comparator":
            s = dict(s, mode="subtract" if s["mode"] == "compare" else "compare")
            self.set_block(p, s, 2)
            if self.get(p)["b"] == "comparator":
                self.comparator_refresh(p, s)
        else:
            raise ValueError(f"no use for {s}")


def java_hash_set(cells):
    """Iteration order of a fresh `HashSet` (16 buckets) holding `cells`."""

    def java_hash(p):
        x, y, z = p
        h = ((y + z * 31) * 31 + x) & 0xFFFFFFFF
        return (h ^ (h >> 16)) & 15

    buckets = [[] for _ in range(16)]
    for p in cells:
        if p not in buckets[java_hash(p)]:
            buckets[java_hash(p)].append(p)
    return [p for bucket in buckets for p in bucket]


def parse_state(model, spec):
    """Fixture state names (Java property values) to model states."""
    parts = spec.split("/")
    b = parts[0]
    if b in ("air", "stone", "redstone_block"):
        return {"b": b}
    if b == "wire":
        return {"b": b, "power": int(parts[1])}
    if b == "lever":
        if parts[1] == "floor":
            return {"b": b, "face": "floor", "facing": "north", "on": parts[2] == "1"}
        return {"b": b, "face": "wall", "facing": parts[2], "on": parts[3] == "1"}
    if b == "repeater":
        return {"b": b, "facing": parts[1], "delay": int(parts[2]), "on": parts[3] == "1", "locked": False}
    if b == "comparator":
        return {"b": b, "facing": parts[1], "mode": parts[2], "on": parts[3] == "1"}
    if b == "torch":
        return {"b": b, "lit": parts[1] == "1"}
    if b == "wall_torch":
        return {"b": b, "facing": parts[1], "lit": parts[2] == "1"}
    if b == "lamp":
        return {"b": b, "lit": parts[1] == "1"}
    if b == "observer":
        return {"b": b, "facing": parts[1], "on": parts[2] == "1"}
    if b == "piston":
        return {"b": b, "facing": parts[1], "extended": parts[2] == "1"}
    raise ValueError(spec)


def run_case(f, case):
    model = Model(f)
    for x, y, z, spec in case["setup"]:
        state = parse_state(model, spec)
        if state["b"] == "repeater":
            model.blocks[(x, y, z)] = state
            state = dict(state, locked=model.repeater_locked((x, y, z), state))
            del model.blocks[(x, y, z)]
        model.set_block((x, y, z), state, 3)
    for _ in range(case["settle"]):
        model.tick()
    trace, logs = [], {}
    actions = {}
    for tick, action in case["actions"]:
        actions.setdefault(tick, []).append(action)
    for tick in range(1, case["ticks"] + 1):
        logged = tick in case.get("log_ticks", [])
        if logged:
            model.log = []
        for action in actions.get(tick, []):
            kind, x, y, z = action[:4]
            if kind == "place":
                model.set_block((x, y, z), parse_state(model, action[4]), 3)
            else:
                model.use((x, y, z))
        model.tick()
        if logged:
            kept = []
            for entry in model.log:
                if entry[0] in case["compare"]:
                    kept.append(entry)
                if entry[0] == "e" and case.get("stop_at_event"):
                    break
            logs[str(tick)] = kept
            model.log = None
        trace.append([model.name(model.get(tuple(p))) for p in case["watch"]])
    return {**case, "trace": trace, "logs": logs}


def stone_under(cells):
    return [[x, y - 1, z, "stone"] for x, y, z in cells]


def cases():
    wire = [(3, 145, 4), (4, 145, 4), (5, 145, 4)]
    lock = {"lever": (2, 145, 7), "a": (4, 145, 8), "b": (4, 145, 7), "lamp": (5, 145, 8)}
    lock_wires = [(3, 145, 8), (3, 145, 7), (3, 145, 6), (4, 145, 6)]
    torch_support = (4, 145, 12)
    piston = (8, 145, 4)
    comparator = (4, 145, 20)
    race_lever = (8, 145, 8)
    out = [
        {
            "name": "wire_line_order",
            "about": "A redstone block placed beside a three-wire line, then removed: "
            "DefaultRedstoneWireEvaluator's HashSet order, the depth-first updater and shape updates.",
            "setup": stone_under([(2, 145, 4)] + wire) + [[*p, "wire/0"] for p in wire],
            "settle": 2,
            "actions": [[1, ["place", 2, 145, 4, "redstone_block"]], [3, ["place", 2, 145, 4, "air"]]],
            "ticks": 4,
            "log_ticks": [1, 3],
            "compare": ["n", "s", "t", "e"],
            "watch": [list(p) for p in wire],
        },
        {
            "name": "repeater_lock_race",
            "about": "One lever powers a repeater and the side repeater that locks it, both delay 1: "
            "the locking repeater's EXTREMELY_HIGH tick runs first and the locked one never turns on.",
            "setup": stone_under([lock["lever"], lock["a"], lock["b"], *lock_wires])
            + [[*lock["lever"], "lever/floor/0"]]
            + [[*p, "wire/0"] for p in lock_wires]
            + [[*lock["a"], "repeater/west/1/0"], [*lock["b"], "repeater/north/1/0"], [*lock["lamp"], "lamp/0"]],
            "settle": 2,
            "actions": [[1, ["use", *lock["lever"]]], [7, ["use", *lock["lever"]]]],
            "ticks": 12,
            "log_ticks": [2],
            # Java's stored LOCKED property adds front updates VoxelCraft derives away.
            "compare": ["t"],
            "watch": [list(lock["a"]), list(lock["b"]), list(lock["lamp"])],
        },
        {
            "name": "torch_burnout",
            "about": "Eight off-toggles within 60 ticks burn a wall-lever-driven torch out. A neighbour "
            "update after the toggles age out cannot relight it early: the pending 160-tick tick blocks "
            "a new tick for the same block, so it relights 160 ticks after the eighth toggle.",
            "setup": [[*torch_support, "stone"], [4, 146, 12, "torch/1"], [5, 145, 12, "lever/wall/east/0"]],
            "settle": 2,
            "actions": [[1 + 3 * k, ["use", 5, 145, 12]] for k in range(16)] + [[120, ["place", 3, 146, 12, "stone"]]],
            "ticks": 210,
            "watch": [[4, 146, 12]],
        },
        {
            "name": "piston_qc_bud",
            "about": "A redstone block diagonally above a piston powers it only through quasi-connectivity "
            "and sends it no update; a later neighbour update fires the extend block event at the end of that tick.",
            "setup": [[8, 144, 4, "stone"], [*piston, "piston/north/0"]],
            "settle": 2,
            "actions": [[1, ["place", 9, 146, 4, "redstone_block"]], [4, ["place", 7, 145, 4, "stone"]]],
            "ticks": 4,
            "log_ticks": [4],
            "compare": ["n", "s", "e"],
            "stop_at_event": True,
            "watch": [list(piston)],
        },
        {
            "name": "observer_clock",
            "about": "Two observers facing each other: shape updates start each pulse and the "
            "setBlock-then-schedule order sets the clock's phase.",
            "setup": [[4, 145, 16, "observer/east/0"]],
            "settle": 0,
            "actions": [[1, ["place", 5, 145, 16, "observer/west/0"]]],
            "ticks": 16,
            "log_ticks": [2, 4],
            "compare": ["n", "s", "t", "e"],
            "watch": [[4, 145, 16], [5, 145, 16]],
        },
        {
            "name": "comparator_modes",
            "about": "A three-tick lever pulse into a comparator whose side reads a wire at 14: "
            "compare mode passes 15, then (after a use) subtract mode passes 1.",
            "setup": stone_under([(2, 145, 20), (3, 145, 20), comparator, (5, 145, 20), (4, 145, 21), (4, 145, 22)])
            + [
                [2, 145, 20, "lever/floor/0"],
                [3, 145, 20, "wire/0"],
                [*comparator, "comparator/west/compare/0"],
                [5, 145, 20, "wire/0"],
                [4, 145, 21, "wire/0"],
                [4, 145, 22, "wire/0"],
                [4, 145, 23, "redstone_block"],
            ],
            "settle": 4,
            "actions": [
                [1, ["use", 2, 145, 20]],
                [4, ["use", 2, 145, 20]],
                [8, ["use", *comparator]],
                [9, ["use", 2, 145, 20]],
                [12, ["use", 2, 145, 20]],
            ],
            "ticks": 16,
            "watch": [list(comparator), [5, 145, 20]],
        },
        {
            "name": "priority_race",
            "about": "One lever pull schedules an EXTREMELY_HIGH repeater (another repeater in front), "
            "a HIGH repeater (a lamp in front), a NORMAL comparator and a NORMAL wall torch; "
            "the pull back gives the turning-off priorities.",
            "setup": stone_under([race_lever, (9, 145, 8), (8, 145, 9), (8, 145, 10), (8, 145, 7)])
            + [
                [*race_lever, "lever/floor/0"],
                [9, 145, 8, "repeater/west/1/0"],
                [10, 145, 8, "lamp/0"],
                [8, 145, 9, "repeater/north/1/0"],
                [8, 145, 10, "repeater/north/1/0"],
                [8, 145, 11, "lamp/0"],
                [8, 145, 7, "comparator/south/compare/0"],
                [8, 145, 6, "lamp/0"],
                [7, 144, 8, "wall_torch/west/1"],
            ],
            "settle": 2,
            "actions": [[1, ["use", *race_lever]], [7, ["use", *race_lever]]],
            "ticks": 14,
            "log_ticks": [2, 3, 4, 8, 9, 10],
            "compare": ["t", "e"],
            "watch": [
                [9, 145, 8],
                [10, 145, 8],
                [8, 145, 9],
                [8, 145, 10],
                [8, 145, 11],
                [8, 145, 7],
                [8, 145, 6],
                [7, 144, 8],
            ],
        },
    ]
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar = Path(directory) / "reference.jar"
        jar.write_bytes(inner)
        cache = {}

        def inspect(name):
            if name not in cache:
                cache[name] = subprocess.check_output(
                    ["javap", "-c", "-p", "-constants", "-classpath", str(jar), "net.minecraft." + name], text=True
                )
            return cache[name]

        f = facts(inspect)
    expect(f["update_order"] == ["west", "east", "down", "up", "north", "south"], "UPDATE_ORDER")
    expect(f["update_shape_order"] == ["west", "east", "north", "south", "down", "up"], "UPDATE_SHAPE_ORDER")
    expect(f["direction_values"] == ["down", "up", "north", "south", "west", "east"], "Direction.values")
    expect(f["horizontal"] == ["north", "east", "south", "west"] and f["vertical"] == ["up", "down"], "Direction.Plane")
    expect(
        f["tick_priority"]
        == {
            "EXTREMELY_HIGH": -3,
            "VERY_HIGH": -2,
            "HIGH": -1,
            "NORMAL": 0,
            "LOW": 1,
            "VERY_LOW": 2,
            "EXTREMELY_LOW": 3,
        },
        "TickPriority",
    )
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "NeighborUpdater, CollectingNeighborUpdater, DefaultRedstoneWireEvaluator, RedstoneWireEvaluator",
            "Level.setBlock, LevelChunk.setBlockState, Block.updateOrDestroy, SignalGetter",
            "RedStoneWireBlock, DiodeBlock, RepeaterBlock, ComparatorBlock, RedstoneTorchBlock, "
            "RedstoneWallTorchBlock, RedstoneLampBlock, LeverBlock, ObserverBlock, PistonBaseBlock",
            "LevelTicks, LevelChunkTicks, ScheduledTick, TickPriority, ServerLevel.tick, ServerLevel.runBlockEvents",
            "MinecraftServer.getMaxChainedNeighborUpdates, Vec3i.hashCode, java.util.HashSet (16 buckets)",
        ],
        "facts": f,
        "cases": [run_case(f, case) for case in cases()],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1) + "\n")


if __name__ == "__main__":
    main()
