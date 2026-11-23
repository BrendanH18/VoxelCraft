#!/usr/bin/env python3
"""Inventory every Java 26.3 block that takes part in redstone.

A block takes part when its class (or a superclass below `Block`) overrides
one of the signal methods (`isSignalSource`, `getSignal`, `ownSignal`,
`getDirectSignal`, `hasAnalogOutputSignal`, `getAnalogOutputSignal`) or
reads power through a `SignalGetter` query (`hasNeighborSignal`,
`getBestNeighborSignal` and the others) in any method. For every registered
block the fixture records the class that `Blocks.<clinit>` constructs (from
the registration's factory lambda, evaluated with `java_block_properties`),
the integer constructor arguments (button press ticks, weighted plate
maximum), which signal methods its hierarchy overrides and where, which
methods query power, and the literal delays its hierarchy passes to
`scheduleTick`. A few behaviour constants the game checks (the daylight
detector's sample period and sun easing, the lightning rod's pulse, the
copper bulb's absence of a scheduled tick) are asserted and recorded.

Used by fixture extractors only. Never reads VoxelCraft source.
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

import java_block_properties as jbp
from extract_java_item_fixtures import SERVER_SHA1

PACKAGE = "net/minecraft/world/level/block/"
SIGNAL_METHODS = (
    "isSignalSource",
    "getSignal",
    "ownSignal",
    "getDirectSignal",
    "hasAnalogOutputSignal",
    "getAnalogOutputSignal",
)
POWER_QUERIES = (
    "hasNeighborSignal",
    "getBestNeighborSignal",
    "getBestOwnOrNeighbourSignal",
    "hasSignal",
    "getSignal",
    "getDirectSignal",
    "getDirectSignalTo",
    "getControlInputSignal",
)
QUERY_OWNERS = (
    "net/minecraft/world/level/SignalGetter",
    "net/minecraft/world/level/Level",
    "net/minecraft/server/level/ServerLevel",
    "net/minecraft/world/level/LevelReader",
    "net/minecraft/world/level/LevelAccessor",
)
# Which v0.17 track owns each class's behaviour. Everything else is E2's.
OWNERS = {
    "RedstoneWireBlock": "A",
    "DiodeBlock": "A",
    "RepeaterBlock": "A",
    "ComparatorBlock": "A",
    "RedstoneTorchBlock": "A",
    "RedstoneWallTorchBlock": "A",
    "PistonBaseBlock": "B",
    "MovingPistonBlock": "B",
    "PistonHeadBlock": "B",
    "CrafterBlock": "C1",
    "DispenserBlock": "C2",
    "DropperBlock": "C2",
    "HopperBlock": "C2",
    "BaseRailBlock": "D1",
    "RailBlock": "D1",
    "PoweredRailBlock": "D1",
    "DetectorRailBlock": "D1",
    "SculkSensorBlock": "E1",
    "CalibratedSculkSensorBlock": "E1",
    "SculkShriekerBlock": "E1",
}


class Factory:
    """A copper collection's waxed or weathering constructor for one state."""

    def __init__(self, fn, state):
        self.fn, self.state = fn, state


class Interpreter(jbp.Interpreter):
    """Records each registration's factory beside its mining properties."""

    def __init__(self, jar):
        super().__init__(jar)
        self.factories = {}

    def invoke(self, op, owner, method, descriptor, args):
        if (
            owner == jbp.BLOCKS
            and method == "register"
            and descriptor.startswith("(Lnet/minecraft/resources/ResourceKey;Ljava/util/function/Function;")
        ):
            result = self.register(args[0], args[2])
            self.factories[args[0].name] = args[1]
            return result
        return super().invoke(op, owner, method, descriptor, args)

    def color_collection(self, method, args):
        if method == "registerBlocks":
            ids, registrar, factory, props = args
            return ids.map(
                lambda c, key: self.call_function(
                    registrar, [key, factory, self.call_function(props, [jbp.Enum("color", c)])]
                )
            )
        return super().color_collection(method, args)

    def copper_collection(self, owner, method, args):
        if owner == jbp.COPPER_COLLECTION and method == "registerBlocks":
            ids, registrar, waxed, weathering, props = args
            return ids.map(
                lambda k, key: self.call_function(
                    registrar,
                    [
                        key,
                        Factory(waxed if k[0] else weathering, k[1]),
                        self.call_function(props, [jbp.Enum("state", k[1])]),
                    ],
                )
            )
        return super().copper_collection(owner, method, args)


def constructed(interpreter, fn, depth=0):
    """The block class a factory constructs and the int constants it passes."""
    if isinstance(fn, Factory):
        return constructed(interpreter, fn.fn, depth)
    if not isinstance(fn, jbp.Lambda):
        raise ValueError(f"unknown factory {fn!r}")
    ints = [c for c in fn.captured if isinstance(c, int) and not isinstance(c, bool)]
    if fn.kind == "newInvokeSpecial":
        return fn.owner, ints
    if fn.kind != "invokeStatic" or depth > 2:
        raise ValueError(f"unresolved factory {fn!r}")
    code, following = interpreter.cls(fn.owner).method(fn.method, fn.descriptor)
    built = None
    offset = 0
    while offset in code:
        op, operand, comment, _ = code[offset]
        if op == "new" and built is None:
            name = comment.split("class ", 1)[1].strip()
            if name.startswith(PACKAGE) and "/state/" not in name:
                built = name
        elif op in ("bipush", "sipush"):
            ints.append(int(operand))
        elif op.startswith("iconst_") and built is not None:
            ints.append(-1 if op.endswith("m1") else int(op[-1]))
        offset = following.get(offset, -1)
    if built is None:
        raise ValueError(f"factory {fn!r} constructs no block")
    return built, ints


class Hierarchy:
    """Declared methods, power queries and scheduled delays per block class."""

    def __init__(self, jar):
        self.jar = jar
        self.cache = {}
        self.texts = {}
        # One javap run for the whole package: a JVM start per class is slow.
        with zipfile.ZipFile(jar) as archive:
            names = sorted(
                n[: -len(".class")]
                for n in archive.namelist()
                if n.startswith(PACKAGE) and n.endswith(".class") and "$" not in n and n.count("/") == 5
            )
        text = subprocess.check_output(
            ["javap", "-classpath", str(jar), "-c", "-p", *[n.replace("/", ".") for n in names]], text=True
        )
        for part in re.split(r"\n(?=Compiled from )", text):
            head = re.search(r"(?:class|interface) ([\w.$]+)", part)
            if head:
                self.texts[head[1].replace(".", "/")] = part

    def info(self, name):
        if name not in self.cache:
            dotted = name.replace("/", ".")
            text = self.texts.get(name) or subprocess.check_output(
                ["javap", "-classpath", str(self.jar), "-c", "-p", dotted], text=True
            )
            head = re.search(r"class [\w.$]+(?: extends ([\w.$]+))?", text)
            parent = head[1].replace(".", "/") if head and head[1] else None
            declared, queries, delays = set(), {}, set()
            for block in text.split("\n\n"):
                lines = block.strip("\n").split("\n")
                header = next((x for x in lines if x.startswith("  ") and not x.startswith("   ")), "")
                match = re.search(r"([\w$]+)\(", jbp.strip_generics(header))
                if not match:
                    continue
                method = match[1]
                declared.add(method)
                previous = []
                for line in lines:
                    call = re.search(r"Method ([\w/$]+)\.([\w$]+):", line)
                    if call and call[1] in QUERY_OWNERS and call[2] in POWER_QUERIES:
                        queries.setdefault(method, set()).add(call[2])
                    # A waterlogged block's water tick (`Fluid.getTickDelay`)
                    # isn't the block's own delay.
                    if call and call[2] == "scheduleTick" and "material/Fluid" not in line:
                        delay = delay_operand(previous)
                        if delay is not None:
                            delays.add(delay)
                    instruction = re.match(r"\s*\d+: (\w+)\s*(.*)", line)
                    if instruction:
                        previous.append(instruction.groups())
            self.cache[name] = (parent, declared, queries, delays)
        return self.cache[name]

    def chain(self, name):
        out = []
        while name and name.startswith(PACKAGE):
            out.append(name)
            name = self.info(name)[0]
            if name in (PACKAGE + "Block", PACKAGE + "state/BlockBehaviour"):
                break
        return out


def delay_operand(previous):
    """The delay pushed for `scheduleTick(pos, block, delay[, priority])`:
    an int literal, a field (`ticksToStayPressed`) or a method result
    (`getDelay`). The priority overload pushes a `TickPriority` last."""
    rest = list(previous)
    if rest and rest[-1][0] == "getstatic" and "TickPriority" in rest[-1][1]:
        rest.pop()
    if not rest:
        return None
    op, operand = rest[-1]
    if op.startswith("iconst_"):
        return int(op[-1])
    if op in ("bipush", "sipush"):
        return int(operand)
    if op in ("getfield", "invokevirtual", "invokestatic", "invokeinterface", "invokespecial", "iload", "iload_1"):
        named = re.search(r"(?:Field|Method) (?:[\w/$]+\.)?([\w$]+)", operand)
        return named[1] if named else op
    return None


def short(name):
    return name.rsplit("/", 1)[-1]


def class_text(jar, name):
    return subprocess.check_output(["javap", "-classpath", str(jar), "-c", "-p", name.replace("/", ".")], text=True)


def constants(jar):
    """Behaviour constants beyond the scheduled delays, each checked in code."""
    daylight = class_text(jar, PACKAGE + "DaylightDetectorBlock")
    tick = daylight.split("tickEntity(")[1].split("\n\n")[0]
    update = daylight.split("updateSignalStrength(")[1].split("\n\n")[0]
    if "ldc2_w" not in tick or "long 20l" not in tick or "lrem" not in tick:
        raise ValueError("reassess the daylight detector's sample period")
    if "float 0.2f" not in update or "getEffectiveSkyBrightness" not in update or "Mth.cos" not in update:
        raise ValueError("reassess the daylight detector's formula")
    rod = class_text(jar, PACKAGE + "LightningRodBlock")
    if "bipush        8" not in rod.split("onLightningStrike(")[1].split("\n\n")[0]:
        raise ValueError("reassess the lightning rod's pulse")
    bulb = class_text(jar, PACKAGE + "CopperBulbBlock")
    if "scheduleTick" in bulb or "hasNeighborSignal" not in bulb:
        raise ValueError("reassess the copper bulb's timing")
    trapped = class_text(jar, PACKAGE + "TrappedChestBlock")
    if "getOpenCount" not in trapped or "Mth.clamp" not in trapped:
        raise ValueError("reassess the trapped chest's signal")
    lamp = class_text(jar, PACKAGE + "RedstoneLampBlock")
    if "iconst_4" not in lamp:
        raise ValueError("reassess the redstone lamp's off delay")
    return {
        "daylight_detector": {
            "sample_period_ticks": 20,
            "sun_angle_easing": 0.2,
            "sky": "getEffectiveSkyBrightness",
        },
        "lightning_rod_pulse_ticks": 8,
        "copper_bulb_scheduled_tick": False,
        "redstone_lamp_off_delay": 4,
        "trapped_chest_signal": "clamp(openCount, 0, 15)",
    }


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar = Path(directory) / "reference.jar"
        jar.write_bytes(inner)
        interpreter = Interpreter(jar)
        interpreter.static(jbp.BLOCKS, "AIR")
        hierarchy = Hierarchy(jar)
        blocks, classes = {}, {}
        for name in interpreter.registered:
            cls, ints = constructed(interpreter, interpreter.factories[name])
            chain = hierarchy.chain(cls)
            overrides, queries, delays = {}, {}, set()
            for member in chain:
                _, declared, member_queries, member_delays = hierarchy.info(member)
                for method in SIGNAL_METHODS:
                    if method in declared:
                        overrides.setdefault(method, short(member))
                for method, calls in member_queries.items():
                    queries.setdefault(method, set()).update(calls)
                delays |= member_delays
            if not overrides and not queries:
                continue
            leaf = short(cls)
            owner = next((OWNERS[short(m)] for m in chain if short(m) in OWNERS), "E2")
            classes[leaf] = {
                "chain": [short(m) for m in chain],
                "signal_methods": dict(sorted(overrides.items())),
                "power_queries": {m: sorted(q) for m, q in sorted(queries.items())},
                "scheduled_delays": sorted(delays, key=lambda d: (isinstance(d, str), str(d).zfill(4))),
                "owner": owner,
            }
            blocks[name] = {"class": leaf, "constructor_ints": ints}
            # The derived rule columns the game compares.
            entry = blocks[name]
            entry["signal_source"] = "isSignalSource" in overrides
            entry["strong"] = "getDirectSignal" in overrides
            entry["analog"] = "hasAnalogOutputSignal" in overrides
            entry["reads_power"] = bool(queries)
        if not blocks:
            raise ValueError("no redstone components found")
        result = {
            "reference": "Minecraft Java Edition 26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "Blocks.<clinit> registrations and their factory lambdas",
                "javap -c -p of each block class and its superclasses below Block",
            ],
            "notes": (
                "signal_source: the hierarchy overrides isSignalSource; strong: it overrides "
                "getDirectSignal; analog: it overrides hasAnalogOutputSignal (a subclass may "
                "still return false, see the class's chain); reads_power: some method queries "
                "SignalGetter power. scheduled_delays lists literal scheduleTick delays, or the "
                "field or method that supplies a computed one."
            ),
            "constants": constants(jar),
            "classes": dict(sorted(classes.items())),
            "blocks": dict(sorted(blocks.items())),
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + "\n")


if __name__ == "__main__":
    main()
