#!/usr/bin/env python3
"""Trace a build on the vanilla Java 26.3 server, tick by tick.

Runs the pinned official server jar headless in a superflat world, freezes
the tick (`/tick freeze`), runs a spec's set-up commands (`/setblock` with
block entity data, and so on), then steps one game tick at a time
(`/tick step 1`) and runs the spec's probe commands after each, printing one
JSON line per tick with each probe's reply. Used to check contraption
expectations against Java itself (v0.17 Track C2; Track A checked the
observer clock the same way).

A spec is JSON: {"setup": [commands], "probes": [commands], "ticks": N}.
Commands are console commands without the slash. Needs `java` (JDK 21 or
newer) on `PATH`; the server unpacks its bundled libraries and writes its
world into a scratch folder.
"""

import argparse
import hashlib
import json
import queue
import re
import subprocess
import tempfile
import threading
import time
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

PROPERTIES = """online-mode=false
level-type=minecraft\\:flat
generate-structures=false
spawn-protection=0
view-distance=3
simulation-distance=3
spawn-monsters=false
spawn-animals=false
max-tick-time=-1
sync-chunk-writes=false
"""
LINE = re.compile(r"^\[[^]]*\] \[[^]]*\]: (.*)$")


class Server:
    def __init__(self, jar, folder):
        (folder / "eula.txt").write_text("eula=true\n")
        (folder / "server.properties").write_text(PROPERTIES)
        self.process = subprocess.Popen(
            ["java", "-Xmx2G", "-jar", str(jar), "nogui"],
            cwd=folder,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self.lines = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        self.wait_for(lambda line: line.startswith("Done ("), 600)
        self.marker = 0

    def _read(self):
        for raw in self.process.stdout:
            match = LINE.match(raw.rstrip("\n"))
            self.lines.put(match[1] if match else raw.rstrip("\n"))

    def wait_for(self, done, timeout):
        seen = []
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                line = self.lines.get(timeout=1)
            except queue.Empty:
                continue
            if done(line):
                return seen
            seen.append(line)
        raise TimeoutError(f"server did not answer: {seen[-5:]}")

    def run(self, commands):
        """Each command's reply lines, in order (a `say` marker closes the batch)."""
        self.marker += 1
        mark = f"trace-mark-{self.marker}"
        for command in [*commands, f"say {mark}"]:
            self.process.stdin.write(command + "\n")
        self.process.stdin.flush()
        return self.wait_for(lambda line: line.endswith(mark), 60)

    def stop(self):
        self.process.stdin.write("stop\n")
        self.process.stdin.flush()
        self.process.wait(timeout=120)


def trace(jar, spec):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-trace-") as temporary:
        server = Server(jar, Path(temporary))
        try:
            server.run(["tick freeze", "forceload add -32 -32 32 32", "gamerule doDaylightCycle false"])
            replies = server.run(spec["setup"])
            yield {"tick": 0, "setup": replies}
            for tick in range(1, spec["ticks"] + 1):
                server.run(["tick step 1"])
                # The step runs in the next server tick; let it.
                time.sleep(0.12)
                yield {"tick": tick, "probes": server.run(spec["probes"])}
        finally:
            server.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the pinned server jar")
    parser.add_argument("spec", type=Path)
    args = parser.parse_args()
    if hashlib.sha1(args.server.read_bytes()).hexdigest() != SERVER_SHA1:
        raise SystemExit("not the pinned official Java 26.3 server artifact")
    for line in trace(args.server, json.loads(args.spec.read_text())):
        print(json.dumps(line), flush=True)


if __name__ == "__main__":
    main()
