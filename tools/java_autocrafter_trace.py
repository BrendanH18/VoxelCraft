#!/usr/bin/env python3
"""Run Track F's crafter autocrafter on the pinned vanilla Java 26.3 server.

Starts the server headless in a superflat world, freezes the tick, builds
`crafter_autocrafter` from `tests/contraptions/builds.rs` with `/setblock`
(same relative positions and placement order), fills the top chest with
27 iron ingots, then steps one game tick at a time and prints, per tick,
the same four probes the contraption test reads: iron blocks in the chest
in front, items in the crafter, in the top chest and in the hopper.

Requires a Java runtime new enough for the server. The server's world and
logs go into the given scratch directory.
"""

import argparse
import queue
import re
import subprocess
import threading
import time
from pathlib import Path

ORIGIN = (0, -60, 0)
# (relative position, block state), in the build's placement order.
BLOCKS = [
    ((2, 1, 0), "crafter[orientation=east_up]"),
    ((3, 1, 0), "chest"),
    ((2, 2, 0), "hopper[facing=down]"),
    ((2, 3, 0), "chest"),
    # VoxelCraft's `comparator(Facing::West)` outputs west; Java's `facing`
    # names the input side.
    ((1, 1, 0), "comparator[facing=east]"),
    ((2, 1, 1), "stone"),
]
DUST = [(0, 1, 0), (0, 1, 1), (0, 1, 2), (0, 1, 3), (0, 1, 4), (1, 1, 4), (2, 1, 4), (2, 1, 3), (2, 1, 2)]
PROBES = [((3, 1, 0), "minecraft:iron_block"), ((2, 1, 0), None), ((2, 3, 0), None), ((2, 2, 0), None)]


def at(rel):
    return " ".join(str(o + r) for o, r in zip(ORIGIN, rel))


class Server:
    def __init__(self, jar, work):
        work.mkdir(parents=True, exist_ok=True)
        (work / "eula.txt").write_text("eula=true\n")
        (work / "server.properties").write_text(
            "level-type=minecraft\\:flat\nonline-mode=false\nspawn-protection=0\ngenerate-structures=false\n"
            "server-port=25599\nspawn-monsters=false\nspawn-animals=false\nview-distance=4\nsimulation-distance=4\n"
            # An empty server would otherwise pause after 60 seconds.
            "pause-when-empty-seconds=0\n"
        )
        self.proc = subprocess.Popen(
            ["java", "-Xmx2G", "-jar", str(jar), "nogui"],
            cwd=work,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self.lines = queue.Queue()
        threading.Thread(target=self.pump, daemon=True).start()
        self.wait(r"Done \(")

    def pump(self):
        for line in self.proc.stdout:
            self.lines.put(line.rstrip("\n"))

    def wait(self, pattern, timeout=300):
        end = time.time() + timeout
        while time.time() < end:
            try:
                line = self.lines.get(timeout=1)
            except queue.Empty:
                continue
            if re.search(pattern, line):
                return line
        raise TimeoutError(pattern)

    def send(self, command):
        self.proc.stdin.write(command + "\n")
        self.proc.stdin.flush()

    def ask(self, command, pattern):
        self.send(command)
        return self.wait(pattern, timeout=30)

    def items(self, rel, item):
        line = self.ask(f"data get block {at(rel)} Items", r"has the following block data|Found no elements")
        total = 0
        for element in re.findall(r"\{[^{}]*\}", line):
            count = re.search(r"count: (\d+)", element)
            name = re.search(r'id: "([\w:]+)"', element)
            if count and name and (item is None or name[1] == item):
                total += int(count[1])
        return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("work", type=Path)
    parser.add_argument("--ticks", type=int, default=240)
    args = parser.parse_args()
    s = Server(args.server.resolve(), args.work)
    try:
        s.ask("tick freeze", r"frozen")
        s.ask("forceload add -16 -16 16 16", r"[Mm]arked|already")
        for rel, state in BLOCKS:
            s.ask(f"setblock {at(rel)} {state}", r"Changed the block|Could not set")
        for rel in DUST:
            below = (rel[0], rel[1] - 1, rel[2])
            s.ask(f"setblock {at(below)} stone", r"Changed the block|Could not set")
        s.ask(f"setblock {at((1, 0, 0))} stone", r"Changed the block|Could not set")
        for rel in DUST:
            s.ask(f"setblock {at(rel)} redstone_wire", r"Changed the block|Could not set")
        s.ask(f"item replace block {at((2, 3, 0))} container.0 with iron_ingot 27", r"Replaced")
        for tick in range(1, args.ticks + 1):
            s.ask("tick step 1", r"Stepping")
            time.sleep(0.12)
            values = [s.items(rel, item) for rel, item in PROBES]
            print(tick, *values, flush=True)
        # The end state, for comparison: the crafter's and hopper's block
        # entities and states, and the game time.
        for rel in [(2, 1, 0), (2, 2, 0)]:
            print(s.ask(f"data get block {at(rel)}", r"has the following block data"), flush=True)
        for check in ["hopper[enabled=true]", "crafter[triggered=false]"]:
            rel = (2, 2, 0) if check.startswith("hopper") else (2, 1, 0)
            print(check, s.ask(f"execute if block {at(rel)} {check}", r"Test (passed|failed)"), flush=True)
        print(s.ask("time query gametime", r"game time is"), flush=True)

        s.send("stop")
        s.proc.wait(timeout=120)
    finally:
        if s.proc.poll() is None:
            s.proc.kill()


if __name__ == "__main__":
    main()
