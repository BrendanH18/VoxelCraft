#!/usr/bin/env python3
"""Record the Ender Dragon's flight from a verified official Java 26.3 server jar.

v1.0 gap-closing Track 4b (PRO-05). Runs the pinned dedicated server on a
new superflat world with ticking frozen (`tick freeze`), summons dragons in
two phases whose targets need no randomness, and steps one tick at a time
(`tick step 1`), reading each dragon's `Pos`, `Motion` and `Rotation` after
every tick with `data get entity`:

- `hover`: `DragonPhase` 10 (`HOVERING`, fly speed 1), summoned moving and
  turned away; the phase's target is where it stood on its first tick.
- `dying`: `DragonPhase` 9 (`DYING`, fly speed 3), no fight, so it flies to
  the `MOTION_BLOCKING` top of (0, 0) (the superflat grass, y -60) until
  within 10 blocks, where `DragonDeathPhase` lets it die.

Needs `java` (JDK 25) on `PATH`. Never reads VoxelCraft source.
"""

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
import threading
import time
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

TICKS = 400
SCENARIOS = [
    # name, phase, position, motion, rotation (yaw, pitch)
    ("hover", 10, (40.5, 20.0, 30.5), (0.6, 0.1, 0.4), (120.0, 0.0)),
    ("dying", 9, (60.5, 30.0, -45.5), (-0.2, 0.0, 0.3), (-35.0, 0.0)),
]
DATA = re.compile(r"has the following entity data: (.*)$")


class Server:
    def __init__(self, jar, root):
        (root / "eula.txt").write_text("eula=true\n")
        (root / "server.properties").write_text(
            "level-seed=0\nonline-mode=false\nmax-tick-time=-1\nspawn-protection=0\n"
            "level-type=minecraft\\:flat\ngenerate-structures=false\nview-distance=2\n"
            "simulation-distance=2\npause-when-empty-seconds=-1\n"
        )
        self.proc = subprocess.Popen(
            ["java", "-Xmx4G", "-jar", str(jar), "--nogui"],
            cwd=root,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.lines = []
        self.lock = threading.Lock()
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        for line in self.proc.stdout:
            with self.lock:
                self.lines.append(line.rstrip("\n"))

    def wait_for(self, text, since=0, timeout=600):
        end = time.time() + timeout
        while time.time() < end:
            with self.lock:
                for line in self.lines[since:]:
                    if text in line:
                        return
            if self.proc.poll() is not None:
                raise RuntimeError("server exited")
            time.sleep(0.05)
        raise TimeoutError(text)

    def send(self, command):
        self.proc.stdin.write(command + "\n")
        self.proc.stdin.flush()

    def query(self, commands):
        """The entity data each of `commands` printed, in order."""
        with self.lock:
            mark = len(self.lines)
        for c in commands:
            self.send(c)
        end = time.time() + 30
        while time.time() < end:
            with self.lock:
                found = [m.group(1) for line in self.lines[mark:] for m in [DATA.search(line)] if m]
                failed = any("No entity was found" in line for line in self.lines[mark:])
            if len(found) == len(commands):
                return found
            if failed:
                return None
            time.sleep(0.02)
        raise TimeoutError(commands)


def numbers(text):
    return [float(v) for v in re.findall(r"-?[0-9.]+(?:E-?[0-9]+)?", text)]


def state(server, tag):
    got = server.query(
        [
            "data get entity @e[tag=%s,limit=1] Pos" % tag,
            "data get entity @e[tag=%s,limit=1] Motion" % tag,
            "data get entity @e[tag=%s,limit=1] Rotation" % tag,
            "data get entity @e[tag=%s,limit=1] Health" % tag,
        ]
    )
    if got is None:
        return None
    pos, motion, rotation, health = (numbers(g) for g in got)
    return {"pos": pos, "motion": motion, "yaw": rotation[0], "health": health[0]}


def record(jar):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-dragon-flight-") as temporary:
        root = Path(temporary)
        server = Server(jar, root)
        server.wait_for("Done (")
        server.send("gamerule spawn_mobs false")
        server.send("forceload add -64 -64 96 96")
        server.send("tick freeze")
        time.sleep(5)
        out = {}
        for name, phase, pos, motion, rotation in SCENARIOS:
            server.send(
                "summon minecraft:ender_dragon %r %r %r {Tags:[%s],DragonPhase:%d,Motion:[%rd,%rd,%rd],"
                "Rotation:[%rf,%rf]}" % (pos + (name, phase) + motion + rotation)
            )
            time.sleep(1)
            ticks = [state(server, name)]
            for _ in range(TICKS):
                mark = len(server.lines)
                server.send("tick step 1")
                server.wait_for("Stepping", mark)
                time.sleep(0.3)
                s = state(server, name)
                if s is None:
                    break
                ticks.append(s)
                if s["health"] <= 0:
                    break
            server.send("kill @e[tag=%s]" % name)
            out[name] = {
                "phase": phase,
                "start": {"pos": list(pos), "motion": list(motion), "yaw": rotation[0]},
                "ticks": ticks,
            }
        server.send("stop")
        server.proc.wait(timeout=300)
    return out


def extract(server_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-dragon-flight-jar-") as temporary:
        jar = Path(temporary) / "server.jar"
        jar.write_bytes(data)
        scenarios = record(jar)
    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "the pinned dedicated server on a new superflat world (seed 0), ticking frozen: 'summon "
                "minecraft:ender_dragon' with Tags, DragonPhase, Motion and Rotation, then 'tick step 1' and "
                "'data get entity' of Pos, Motion, Rotation and Health after each tick",
            ],
            "format": "scenarios: name -> phase, start (summon pos, motion, yaw), ticks: the state before the "
            "first step and after each, until Health reaches 0 or 400 ticks",
        },
        "scenarios": scenarios,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    text = json.dumps(extract(args.server), indent=1, sort_keys=True)
    flat = re.compile(r"\[\s+([^\[\]{}]*?)\s+\]")
    text = flat.sub(lambda m: "[" + re.sub(r",\s+", ", ", m.group(1)) + "]", text)
    args.output.write_text(text + "\n")


if __name__ == "__main__":
    main()
