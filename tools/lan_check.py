#!/usr/bin/env python3
"""LAN acceptance checks against a real desktop host (see docs/lan.md).

Starts `target/release/voxelcraft` as a LAN host in a throwaway data folder and
drives one or two protocol-level clients through races, death, reconnect, a host
restart, joining during fluid/fire/TNT activity, distant streaming and portals,
then prints measurements. Build first: `cargo build --release`.

    python3 tools/lan_check.py [--keep]

Needs a display (the host is the normal desktop game). Exits non-zero on failure.
"""

import json
import math
import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release/voxelcraft")
VERSION = next(
    line.split('"')[1] for line in (ROOT / "Cargo.toml").read_text().splitlines() if line.startswith("version")
)
RESULTS = []


def check(name, ok, detail=""):
    RESULTS.append((name, ok, detail))
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}", flush=True)


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Host:
    """A desktop host on a stone platform at y=150, cheats allowed for LAN players."""

    def __init__(self, data, port, new=True, extra=()):
        data.mkdir(parents=True, exist_ok=True)
        self.port = port
        self.log = data / "host.log"
        previous_log_size = self.log.stat().st_size if self.log.exists() else 0
        args = [
            EXE,
            "--world",
            "lancheck",
            "--rd",
            "4",
            "--mute",
            "--time",
            "0.25",
            "--lan-cheats",
            "--pose",
            "0.5,151,0.5,0,0",
            "--data-dir",
            str(data),
            "--host-lan",
            f"127.0.0.1:{port}",
            "--profile",
            "Host",
            "--open-menu",
            "pause",
        ]
        if new:
            args.append("--new")
            for x in range(-8, 9):
                for z in range(-8, 9):
                    args += ["--place", f"{x},150,{z},stone"]
        args += list(extra)
        env = os.environ.copy()
        env["RUST_LOG"] = "voxelcraft=info"
        self.proc = subprocess.Popen(args, stdout=open(self.log, "a"), stderr=subprocess.STDOUT, env=env)
        # Handshake/chunks can precede the asynchronous --place fixture edits.
        # Wait for this run's marker, including when reusing the restart log.
        deadline = time.monotonic() + 60
        try:
            while True:
                with self.log.open("rb") as log:
                    log.seek(previous_log_size)
                    output = log.read().decode(errors="replace")
                ready = re.search(r"scripted scene ready: (\d+) placements, (\d+) failures", output)
                if ready:
                    if int(ready[2]):
                        raise RuntimeError(f"fixture placement failed; inspect {self.log}")
                    break
                if self.proc.poll() is not None:
                    raise RuntimeError(f"desktop host exited before fixture readiness; inspect {self.log}")
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"desktop fixture readiness; inspect {self.log}")
                time.sleep(0.1)
        except BaseException:
            self.stop()
            raise

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            self.proc.wait(timeout=20)

    def stats(self):
        return [line for line in self.log.read_text(errors="replace").splitlines() if "LAN stats" in line]


class Client:
    def __init__(self, port, profile, timeout=30):
        self.profile, self.buf, self.last, self.chat = profile, bytearray(), None, []
        self.inventory_parts = bytearray()
        self.large_inventory_fields = []
        self.inventory_revision = self.acks_during_inventory = 0
        self.items = {"cursor": "-", "inventory": ",".join(["-"] * 36)}
        self.grid = ["-"] * 4
        self.chunks, self.seq, self.selected, self.bytes = 0, 0, 0, 0
        self.welcome_at = self.first_chunk_at = self.ready_at = None
        self.started = time.monotonic()
        deadline = time.monotonic() + timeout
        while True:
            try:
                self.sock = socket.create_connection(("127.0.0.1", port), timeout=1)
                break
            except OSError:
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.1)
        self.sock.settimeout(0.05)
        self.started = time.monotonic()
        self._send(bytes([0]) + struct.pack("<H", 6) + self._text(VERSION) + self._text(profile))

    @staticmethod
    def _text(s):
        b = s.encode()
        return struct.pack("<H", len(b)) + b

    def _send(self, payload):
        self.sock.sendall(struct.pack("<I", len(payload)) + payload)

    def command(self, text):
        self._send(bytes([3]) + self._text(text))

    def control(self, yaw=0.0, pitch=0.0, forward=0.0, buttons=0):
        self.seq += 1
        self._send(bytes([2]) + struct.pack("<IffffBB", self.seq, yaw, pitch, forward, 0.0, buttons, self.selected))

    def click(self, slot, right=0, shift=0, grid_size=None):
        if grid_size is None:
            size = self.last["grid_size"]
            grid_size = size if len(self.grid) == size * size else 0
        self._send(bytes([8]) + struct.pack("<HBBIB", slot, right, shift, self.me()["inventory_revision"], grid_size))

    def write_book(self, page):
        text = page.encode("utf-16le")
        self._send(bytes([10, 0, 100]) + (struct.pack("<H", len(text) // 2) + text) * 100)

    def poll(self):
        try:
            data = self.sock.recv(1 << 20)
            if not data:
                raise ConnectionError("closed")
            self.buf.extend(data)
            self.bytes += len(data)
        except socket.timeout:
            return
        while len(self.buf) >= 4:
            n = struct.unpack_from("<I", self.buf)[0]
            if len(self.buf) < n + 4:
                break
            p = bytes(self.buf[4 : n + 4])
            del self.buf[: n + 4]
            now = time.monotonic() - self.started
            if p[0] == 1 and self.welcome_at is None:
                self.welcome_at = now
            elif p[0] == 5:
                state = json.loads(p[1:])
                if self.inventory_parts and self.last and state["sequence"] > self.last["sequence"]:
                    self.acks_during_inventory += 1
                self.last = state
                self.last["self"].update(self.items)
                self.last["self"]["inventory_revision"] = self.inventory_revision
                cells = state["grid_size"] ** 2
                self.last["grid"] = self.grid if len(self.grid) == cells else ["-"] * cells
                if self.ready_at is None:
                    self.ready_at = now
            elif p[0] == 11:
                self.inventory_parts.extend(p[2:])
                if p[1]:
                    fields = re.split(r"([,|#\n])", self.inventory_parts.decode())
                    for index in range(0, len(fields), 2):
                        field = fields[index]
                        if field.startswith("@"):
                            fields[index] = self.large_inventory_fields[int(field[1:])]
                    self.large_inventory_fields = [field for field in fields[::2] if len(field) > 4096]
                    cursor, grid, inventory = "".join(fields).split("\n", 2)
                    self.items = {"cursor": cursor, "inventory": inventory}
                    self.grid = grid.split(",")[1:]
                    self.inventory_parts.clear()
                    self.inventory_revision += 1
            elif p[0] == 6:
                self.chunks += 1
                if self.first_chunk_at is None:
                    self.first_chunk_at = now
            elif p[0] == 4:
                self.chat.append(p[3:].decode(errors="replace"))
            elif p[0] == 7:
                raise ConnectionError(p[3:].decode(errors="replace"))

    def run(self, secs, *aim, buttons=0, forward=0.0):
        end = time.monotonic() + secs
        while time.monotonic() < end:
            self.control(*(aim or (0.0, 0.0)), forward=forward, buttons=buttons)
            self.poll()
            time.sleep(0.02)

    def until(self, test, secs=15, why=""):
        end = time.monotonic() + secs
        while not (self.last is not None and test(self.last)):
            if time.monotonic() > end:
                pos = self.last and self.last["self"]["pos"]
                raise TimeoutError(f"{self.profile}: {why} (at {pos}, chat {self.chat[-2:]})")
            self.control()
            self.poll()
            time.sleep(0.02)

    def me(self):
        return self.last["self"]

    def inventory(self):
        return self.me()["inventory"].split("|")[0].split("#")[0].split(",")

    def count(self, item_id):
        total = sum(int(s.split(":")[1]) for s in self.inventory() if s.split(":")[0] == str(item_id))
        cursor = self.me()["cursor"]
        if cursor.split(":")[0] == str(item_id):
            total += int(cursor.split(":")[1])
        return total

    def aim_at(self, x, y, z):
        p = self.me()["pos"]
        dx, dy, dz = x - p[0], y - (p[1] + 1.62), z - p[2]
        return math.atan2(dz, dx), math.atan2(dy, math.hypot(dx, dz))

    def use(self, aim, sneak=False):
        extra = 2 if sneak else 0
        if sneak:
            self.run(0.3, *aim, buttons=2)
        self.control(*aim, buttons=16 | extra)
        self.command("use_down")
        self.control(*aim, buttons=extra)
        self.command("use_up")
        self.run(0.4, *aim, buttons=extra)

    def close(self):
        self.sock.close()


# Stable item ids (src/item.rs, block items below 256 share the block id).
DIAMOND, DIRT, APPLE = 261, 2, 263
WRITABLE_BOOK = 1557


def large_books(data):
    port = free_port()
    host = Host(data, port, extra=["--place", "3,151,0,crafting_table"])
    a = b = None
    try:
        a, b = Client(port, "Scribe"), Client(port, "Reader")
        for client in (a, b):
            client.until(lambda s: s["self"]["ground"], why="spawn")
        for slot, page in enumerate(("a" * 1024, "書" * 1024)):
            a.command("/give writable_book")
            a.until(lambda s: a.inventory()[slot] == f"{WRITABLE_BOOK}:1", why="receive an empty book")
            a.selected = slot
            a.run(0.3)
            a.write_book(page)
            a.until(lambda s: len(a.inventory()[slot]) > 100_000, why="receive the edited book")
        check("LAN books: movement acknowledged during inventory transfer", a.acks_during_inventory > 0)
        a.control()
        sent = a.seq
        a.until(lambda s: s["sequence"] >= sent, why="acknowledge movement with large books")
        b.control()
        sent = b.seq
        b.until(lambda s: s["sequence"] >= sent, why="acknowledge movement while another player holds a large book")
        check("LAN books: both guests receive tick state with two full books in one inventory", True)
        ascii_book, cjk_book = a.inventory()[:2]
        a.command("inventory")
        a.until(lambda s: s["open"], why="open inventory")
        a.click(0)
        a.until(lambda s: a.me()["cursor"] == ascii_book, why="pick up the first book")
        a.click(1)
        a.click(65535)  # Outside the panel, still using the revision with the ASCII book.
        a.until(lambda s: a.me()["cursor"] == cjk_book, why="swap books while a stale toss is rejected")
        a.run(0.3)
        check("LAN books: an old cursor click cannot toss the newly swapped book", a.me()["cursor"] == cjk_book)
        a.click(100)
        a.until(lambda s: s["grid"][0] == cjk_book, why="place a large book in the personal crafting grid")
        a.click(100)
        a.until(lambda s: a.me()["cursor"] == cjk_book, why="retrieve the book from the crafting grid")
        check("LAN books: a large book can move through the personal crafting grid", True)

        a.click(0)
        a.until(lambda s: a.me()["cursor"] == "-", why="put the book back")
        a.command("/give dirt 3")
        a.until(lambda s: a.inventory()[2] == f"{DIRT}:3", why="receive three dirt blocks")
        a.click(2)
        a.until(lambda s: a.me()["cursor"] == f"{DIRT}:3", why="pick up the dirt stack")
        a.click(3, right=1)
        a.click(4, right=1)
        a.until(lambda s: a.inventory()[3:5] == [f"{DIRT}:1"] * 2, why="place one item per rapid click")
        check("LAN inventory: rapid right-clicks place one item in each slot", a.me()["cursor"] == f"{DIRT}:1")

        a.command("close")
        a.until(lambda s: not s["open"], why="close the personal inventory")
        a.use(a.aim_at(3.5, 151.5, 0.5))
        a.until(lambda s: s["open"] and s["grid_size"] == 3 and len(a.grid) == 9, why="open the crafting table")
        a.click(3, right=1)
        a.until(lambda s: a.me()["cursor"] == f"{DIRT}:1", why="pick up dirt in the crafting table")
        a.click(102, grid_size=2)  # A click delayed from the 2×2 panel.
        a.click(103)
        a.until(lambda s: s["grid"][3] == f"{DIRT}:1", why="accept the click for the current grid size")
        check(
            "LAN inventory: crafting clicks require the displayed grid size",
            a.last["grid"][2] == "-" and a.me()["cursor"] == "-",
        )
        a.click(103)
        a.until(lambda s: a.me()["cursor"] == f"{DIRT}:1", why="pick up dirt for a fresh outside click")
        a.click(65535)
        a.until(lambda s: a.me()["cursor"] == "-", why="toss the cursor stack with the displayed revision")
        check("LAN inventory: a fresh outside click tosses the cursor stack", True)
    finally:
        for client in (a, b):
            if client:
                client.close()
        host.stop()


def races(data):
    port = free_port()
    host = Host(data, port, extra=["--place", "3,151,0,chest", "--place", "0,151,4,dirt"])
    a = b = None
    try:
        a, b = Client(port, "Alice"), Client(port, "Bob")
        for c in (a, b):
            c.until(lambda s: s["self"]["ground"], why="spawn")
        check(
            "join: handshake, chunks, first state",
            all(c.welcome_at and c.first_chunk_at and c.ready_at for c in (a, b)),
            f"Alice welcome {a.welcome_at:.3f}s, first chunk {a.first_chunk_at:.3f}s, ready {a.ready_at:.3f}s",
        )
        # Stock the chest with exactly 64 diamonds through Alice.
        a.command("/give diamond 64")
        a.run(0.5)
        for _ in range(3):
            a.use(a.aim_at(3.5, 151.5, 0.5))
            if a.last["open"]:
                break
        a.until(lambda s: s["open"] and s["container"]["kind"] == "chest", why="chest open")
        slot = next(i for i, s in enumerate(a.inventory()) if s.startswith(f"{DIAMOND}:"))
        a.click(slot, 0, 1)
        a.until(lambda s: f"{DIAMOND}:64" in s["container"]["state"], why="stock chest")
        b.use(b.aim_at(3.5, 151.5, 0.5))
        b.until(lambda s: s["open"] and s["container"]["kind"] == "chest", why="chest open (Bob)")
        # Both grab chest slot 0 in the same instant.
        a.click(200)
        b.click(200)
        for c in (a, b):
            c.run(0.6)
        chest = sum(
            int(s.split(":")[1]) for s in a.last["container"]["state"].split(",") if s.startswith(f"{DIAMOND}:")
        )
        total = a.count(DIAMOND) + b.count(DIAMOND) + chest
        check(
            "chest race conserves items",
            total == 64,
            f"Alice {a.count(DIAMOND)}, Bob {b.count(DIAMOND)}, chest {chest}",
        )
        for c in (a, b):
            c.command("close")
            c.run(0.3)
        # Both mine one dirt block, then race for the drop.
        aim_a, aim_b = a.aim_at(0.5, 151.5, 4.5), b.aim_at(0.5, 151.5, 4.5)
        for c, aim in ((a, aim_a), (b, aim_b)):
            c.control(*aim, buttons=8)
            c.command("attack_down")
        end = time.monotonic() + 3
        while time.monotonic() < end:
            for c, aim in ((a, aim_a), (b, aim_b)):
                c.control(*aim, buttons=8)
                c.poll()
            time.sleep(0.02)
        for c, aim in ((a, aim_a), (b, aim_b)):
            c.command("attack_up")
            c.run(0.1, *aim)
        # Walk both toward the drop.
        end = time.monotonic() + 3
        while time.monotonic() < end:
            for c in (a, b):
                c.control(*c.aim_at(0.5, 151.2, 4.5), forward=1.0)
                c.poll()
            time.sleep(0.02)
        dirt_items = sum(
            int(i.rsplit(",", 1)[1].split(":")[1])
            for i in (a.last.get("items") or "").split(";")
            if "," in i and i.rsplit(",", 1)[1].startswith(f"{DIRT}:")
        )
        got = a.count(DIRT) + b.count(DIRT)
        check(
            "block race yields one drop, picked up once",
            got + dirt_items == 1,
            f"Alice {a.count(DIRT)}, Bob {b.count(DIRT)}, on ground {dirt_items}",
        )
        # Death and respawn.
        a.command("/kill")
        a.until(lambda s: s["self"]["health"] <= 0, why="death")
        a.command("respawn")
        a.until(lambda s: s["self"]["health"] > 0, why="respawn")
        check("death and respawn", True, f"health {a.me()['health']}")
        # Reconnect keeps the profile.
        b.command("/give apple 5")
        b.run(0.5)
        pos = b.me()["pos"]
        b.close()
        b = Client(port, "Bob")
        b.until(lambda s: s["self"]["ground"], why="rejoin")
        check(
            "reconnect restores inventory and position",
            b.count(APPLE) == 5 and math.dist(pos, b.me()["pos"]) < 1.5,
            f"apples {b.count(APPLE)}, moved {math.dist(pos, b.me()['pos']):.2f}",
        )
        # Portal entry by a LAN player is host-controlled.
        a.command("/setblock 2 151 -2 nether_portal")
        a.run(0.3)
        a.command("/tp 2.5 151 -1.5")
        a.run(1.5)
        check("portal entry reports host-controlled travel", any("host-controlled" in m for m in a.chat), a.chat[-1:])
        # Distant streaming: teleport 2000 blocks away and time the new chunks.
        a.command("/gamemode creative")
        before = a.chunks
        start = time.monotonic()
        a.command("/tp 2000 160 2000")
        a.until(lambda s: math.dist(s["self"]["pos"][::2], [2000, 2000]) < 2, why="tp")
        while a.chunks - before < 300 and time.monotonic() - start < 30:
            a.poll()
            a.control()
        check(
            "distant streaming after a 2.8 km teleport",
            a.chunks - before >= 300,
            f"{a.chunks - before} chunks in {time.monotonic() - start:.1f}s",
        )
        # Steady state for the bandwidth/tick log.
        for c in (a, b):
            c.run(0.1)
        end = time.monotonic() + 21
        while time.monotonic() < end:
            for c in (a, b):
                c.control()
                c.poll()
            time.sleep(0.05)
        for line in host.stats()[-2:]:
            print("     ", line.split("] ", 1)[-1])
        # Save for the restart check.
        b.command("/save-all")
        b.run(1.0)
        check("/save-all acknowledged", any("Saved" in m for m in b.chat), b.chat[-1:])
    finally:
        for c in (a, b):
            if c:
                c.close()
        host.stop()
    # Restart the host from the saved world.
    port = free_port()
    host = Host(data, port, new=False)
    try:
        b = Client(port, "Bob")
        b.until(lambda s: s["self"]["ground"], why="rejoin after restart")
        check("host restart keeps the profile", b.count(APPLE) == 5, f"apples {b.count(APPLE)}")
        b.close()
    finally:
        host.stop()


def busy_join(data):
    """Join while water spreads, fire burns and TNT is primed."""
    port = free_port()
    extra = [
        "--place",
        "-6,151,-6,water",
        "--place",
        "6,151,6,lava",
        "--place",
        "-5,151,5,netherrack",
        "--place",
        "-5,152,5,fire",
        "--place",
        "5,151,-5,tnt",
        "--place",
        "5,151,-4,block_of_redstone",
        "--place",
        "-3,155,0,sand",
    ]
    host = Host(data, port, extra=extra)
    c = None
    try:
        c = Client(port, "Late")
        c.until(lambda s: s["self"]["ground"], secs=30, why="spawn")
        saw_tnt = False
        end = time.monotonic() + 6
        while time.monotonic() < end:
            c.control()
            c.poll()
            saw_tnt |= bool((c.last.get("extras") or {}).get("tnt"))
            time.sleep(0.02)
        check(
            "join during fluid, fire and TNT activity",
            c.ready_at is not None and c.chunks > 0,
            f"ready {c.ready_at:.2f}s, chunks {c.chunks}, saw primed TNT {saw_tnt}",
        )
    finally:
        if c:
            c.close()
        host.stop()


def host_travel(data):
    """The host stands in a Nether portal; joined players follow it into the Nether."""
    port = free_port()
    # A lit 2x3 portal in the x=0 plane around the host's spawn at (0.5, 151, 0.5).
    frame = [(0, y, z) for y in (150, 154) for z in (0, 1)] + [(0, y, z) for y in range(151, 154) for z in (-1, 2)]
    extra = []
    for x, y, z in frame:
        extra += ["--place", f"{x},{y},{z},obsidian"]
    host = Host(data, port, extra=extra + ["--pose", "0.5,151,1.0,90,0"])
    c = None
    try:
        c = Client(port, "Follower")
        c.until(lambda s: s["self"]["ground"], secs=30, why="spawn")
        # Light the frame from outside, as a player would.
        c.command("/tp 2.5 151 0.5")
        c.command("/give flint_and_steel")
        c.run(0.6)
        c.selected = next(i for i, s in enumerate(c.inventory()[:9]) if s != "-")
        c.use(c.aim_at(0.5, 150.95, 0.5))
        start = time.monotonic()
        c.until(lambda s: s["dimension"] == "nether", secs=30, why="host travel")
        chunks = c.chunks
        c.until(lambda s: s["dimension"] == "nether" and s["self"]["ground"], secs=30, why="land in the Nether")
        c.run(2.0)
        check(
            "players follow the host through a portal",
            c.chunks > chunks,
            f"Nether after {time.monotonic() - start:.1f}s, {c.chunks - chunks} Nether chunks, at {[round(x) for x in c.me()['pos']]}",
        )
    finally:
        if c:
            c.close()
        host.stop()


def pvp(data):
    """With PvP on, a sword and a bow hurt another player; with it off, neither does."""
    for enabled in (True, False):
        port = free_port()
        host = Host(data / ("pvp-on" if enabled else "pvp-off"), port, extra=["--lan-pvp"] if enabled else [])
        a = b = None
        try:
            a, b = Client(port, "Ann"), Client(port, "Ben")
            for c in (a, b):
                c.until(lambda s: s["self"]["ground"], why="spawn")
            b.command("/tp 3.5 151 4.5")
            a.command("/tp 0.5 151 4.5")
            a.command("/give diamond_sword")
            a.command("/give bow")
            a.command("/give arrow 16")
            a.run(0.8)
            b.until(lambda s: s["self"]["health"] >= 20.0, why=f"Ben at full health ({b.last['self']})")
            start = b.me()["health"]

            def swing():
                aim = a.aim_at(3.5, 152.0, 4.5)
                a.control(*aim, buttons=8)
                a.command("attack_down")
                a.control(*aim)
                a.command("attack_up")
                a.run(0.8, *aim)
                b.run(0.1)

            def shoot():
                aim = a.aim_at(3.5, 152.3, 4.5)
                a.control(*aim, buttons=16)
                a.command("use_down")
                a.run(1.2, *aim, buttons=16)
                a.command("use_up")
                a.run(1.0, *aim)
                b.run(0.1)

            # A real player swings and shoots again after a miss (a spread arrow
            # or an attack timed inside the cooldown), so allow a few tries
            # when PvP is on; with it off every attempt must do nothing.
            tries = 3 if enabled else 1
            after_sword = start
            for _ in range(tries):
                swing()
                after_sword = b.me()["health"]
                if after_sword < start:
                    break
                a.run(1.0)
            a.selected = 1
            after_bow = after_sword
            for _ in range(tries):
                shoot()
                after_bow = b.me()["health"]
                if after_bow < after_sword:
                    break
            if enabled:
                check(
                    "PvP on: sword and bow hurt another player",
                    start > after_sword > after_bow,
                    f"{start} -> {after_sword:.1f} -> {after_bow:.1f}",
                )
            else:
                check("PvP off: players can't hurt each other", start == after_sword == after_bow, f"{after_bow}")
        finally:
            for c in (a, b):
                if c:
                    c.close()
            host.stop()


def main():
    keep = "--keep" in sys.argv
    data = Path(tempfile.mkdtemp(prefix="vx-lan-check-"))
    try:
        races(data / "races")
        large_books(data / "books")
        busy_join(data / "busy")
        pvp(data)
        host_travel(data / "travel")
    finally:
        if keep:
            print("data kept in", data)
        else:
            shutil.rmtree(data, ignore_errors=True)
    failed = [r for r in RESULTS if not r[1]]
    print(f"\n{len(RESULTS) - len(failed)}/{len(RESULTS)} checks passed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
