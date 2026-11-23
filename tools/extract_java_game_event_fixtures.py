#!/usr/bin/env python3
"""Extract Java 26.3 game events, vibration tables and call sites (v0.17 Track E1).

Reads the pinned server jar with `javap` and writes
`tests/fixtures/java/26.3/game_events.json`:

- `events`: the `GameEvent` registry in order with each event's
  notification radius, `VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`
  frequency and membership of the game event tags listeners use;
- `tags`: those tags resolved to event names, and the vibration block tags;
- `listeners`: the sculk listeners' radii and timings;
- `call_sites`: every method that posts a game event (`gameEvent(`,
  `causeUseVibration`, `forceScheduleVibration`), with the events it names.

Usage: python3 tools/extract_java_game_event_fixtures.py SERVER_JAR --output FILE [--javap PATH]
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

EVENT = "net/minecraft/world/level/gameevent/GameEvent"
GAME_EVENT_TAGS = (
    "vibrations",
    "warden_can_listen",
    "shrieker_can_listen",
    "ignore_vibrations_sneaking",
    "allay_can_listen",
)
BLOCK_TAGS = ("occludes_vibration_signals", "dampens_vibrations", "vibration_resonators")
# Methods that hand an event to the dispatcher. Listener methods that only
# compare events (`handleGameEvent`, `canReceiveVibration`) are not posts.
# `ShelfBlockEntity.setChanged(Holder)` posts the event it is given.
POSTERS = re.compile(
    r"// (?:Interface)?Method (?:[\w/$]+\.)?(?:gameEvent|causeUseVibration|forceScheduleVibration):"
    r"|ShelfBlockEntity\.setChanged:\(Lnet/minecraft/core/Holder"
)
# Enchantment effects carry an event the effect posts when it applies.
INDIRECT = {
    ("net.minecraft.world.item.enchantment.Enchantments", "bootstrap"): "frost walker's replace_disk effect",
    ("net.minecraft.world.item.enchantment.effects.SetBlockProperties", "<init>"): "set_block_properties default",
}
CALIBRATED = "net.minecraft.world.level.block.CalibratedSculkSensorBlock"
LISTENER_CLASSES = {
    "sculk_sensor": "net.minecraft.world.level.block.entity.SculkSensorBlockEntity$VibrationUser",
    "calibrated_sculk_sensor": "net.minecraft.world.level.block.entity.CalibratedSculkSensorBlockEntity$VibrationUser",
    "sculk_shrieker": "net.minecraft.world.level.block.entity.SculkShriekerBlockEntity$VibrationUser",
    "warden": "net.minecraft.world.entity.monster.warden.Warden$VibrationUser",
    "allay": "net.minecraft.world.entity.animal.allay.Allay$VibrationUser",
}


def methods(text):
    """Splits javap -c output into (class, method, body lines)."""
    cls, name, body = None, None, []
    for line in text.splitlines():
        header = re.match(r"^(?:[\w-]+ )*(?:class|interface|enum|record) ([\w.$]+)", line)
        if header:
            if name:
                yield cls, name, body
            cls, name, body = header[1], None, []
            continue
        if line.strip() == "static {};" or (re.match(r"^  \S", line) and "(" in line and line.rstrip().endswith(";")):
            if name:
                yield cls, name, body
            name = "<clinit>" if line.strip() == "static {};" else re.search(r"([\w$<>.]+)\(", line)[1]
            name = name.rsplit(".", 1)[-1]
            if name == cls.rsplit(".", 1)[-1]:
                name = "<init>"
            body = []
            continue
        body.append(line)
    if name:
        yield cls, name, body


def int_pushed(line):
    match = re.search(r"\b(?:bipush|sipush)\s+(-?\d+)", line) or re.search(r"\biconst_(\d)", line)
    return int(match[1]) if match else None


def enclosing(method):
    """`lambda$useOn$0` belongs to `useOn`; `lambda$static$0` to `<clinit>`."""
    match = re.match(r"lambda\$(.+)\$\d+$", method)
    if not match:
        return method
    return {"static": "<clinit>", "new": "<init>"}.get(match[1], match[1])


def extract(server, javap):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar_path = Path(directory) / "reference.jar"
        jar_path.write_bytes(inner)
        with zipfile.ZipFile(io.BytesIO(inner)) as jar:

            def read(path):
                return json.loads(jar.read(path))

            classes = sorted(
                name[:-6].replace("/", ".")
                for name in jar.namelist()
                if name.startswith("net/minecraft/") and name.endswith(".class") and EVENT.encode() in jar.read(name)
            )
            classes = sorted(set(classes) | set(LISTENER_CLASSES.values()) | {CALIBRATED})
            game_event_tags = {t: read(f"data/minecraft/tags/game_event/{t}.json")["values"] for t in GAME_EVENT_TAGS}
            block_tags = {t: read(f"data/minecraft/tags/block/{t}.json")["values"] for t in BLOCK_TAGS}

        def javap_c(names, *flags):
            return subprocess.check_output([javap, "-c", "-p", *flags, "-classpath", str(jar_path), *names], text=True)

        code = javap_c(classes)
        constants = javap_c(
            [
                "net.minecraft.world.level.block.SculkSensorBlock",
                "net.minecraft.world.level.block.entity.SculkShriekerBlockEntity",
            ],
            "-constants",
        )

    bodies = {}
    for cls, name, body in methods(code):
        bodies.setdefault((cls, name), []).extend(body)

    # The registry: `ldc name`, an optional radius push, `register`, `putstatic`.
    events = []
    radius = None
    pending = None
    for line in bodies[("net.minecraft.world.level.gameevent.GameEvent", "<clinit>")]:
        name = re.search(r"ldc(?:_w)?\s+#\d+\s+// String (\w+)$", line)
        if name:
            pending, radius = name[1], None
            continue
        if pending and int_pushed(line) is not None:
            radius = int_pushed(line)
        if pending and "Method register:" in line:
            events.append({"name": pending, "radius": radius if radius is not None else 16})
            pending = None
    if len(events) != 61 or events[0]["name"] != "block_activate":
        raise ValueError(f"unexpected game event registry ({len(events)} events)")
    names = {e["name"] for e in events}

    # `VIBRATION_FREQUENCY_FOR_EVENT`: getstatic EVENT, key(), push N, put.
    frequencies = {}
    current = None
    for line in bodies[("net.minecraft.world.level.gameevent.vibrations.VibrationSystem", "lambda$static$0")]:
        field = re.search(rf"getstatic\s+#\d+\s+// Field {EVENT}\.(\w+):", line)
        if field:
            current = field[1].lower()
            continue
        if current and int_pushed(line) is not None:
            frequencies[current] = int_pushed(line)
            current = None
    resonance = []
    for line in bodies[("net.minecraft.world.level.gameevent.vibrations.VibrationSystem", "<clinit>")]:
        field = re.search(rf"getstatic\s+#\d+\s+// Field {EVENT}\.(RESONATE_\d+):", line)
        if field:
            resonance.append(field[1].lower())
    if resonance != [f"resonate_{n}" for n in range(1, 16)]:
        raise ValueError("unexpected resonance event list")
    for n, name in enumerate(resonance, 1):
        frequencies.setdefault(name, n)

    def resolve(values, tags):
        out = []
        for value in values:
            if value.startswith("#"):
                out += resolve(tags[value.split(":", 1)[1]], tags)
            else:
                out.append(value.split(":", 1)[1])
        return out

    tags = {t: resolve(v, game_event_tags) for t, v in game_event_tags.items()}
    for t, values in tags.items():
        if not set(values) <= names:
            raise ValueError(f"tag {t} names an unknown event")
    for event in events:
        event["frequency"] = frequencies.get(event["name"], 0)
        event["tags"] = [t for t in GAME_EVENT_TAGS if event["name"] in tags[t]]

    def returned_int(cls, method):
        pushed = [int_pushed(line) for line in bodies[(cls, method)] if int_pushed(line) is not None]
        if not pushed:
            raise ValueError(f"no constant in {cls}.{method}")
        return pushed[0]

    def constant(name):
        return int(re.search(rf"\b{name} = (\d+);", constants)[1])

    listeners = {key: {"radius": returned_int(cls, "getListenerRadius")} for key, cls in LISTENER_CLASSES.items()}
    sensor = "net.minecraft.world.level.block.SculkSensorBlock"
    listeners["sculk_sensor"]["active_ticks"] = returned_int(sensor, "getActiveTicks")
    listeners["sculk_sensor"]["cooldown_ticks"] = constant("COOLDOWN_TICKS")
    if constant("ACTIVE_TICKS") != listeners["sculk_sensor"]["active_ticks"]:
        raise ValueError("ACTIVE_TICKS disagrees with getActiveTicks")
    listeners["calibrated_sculk_sensor"]["active_ticks"] = returned_int(CALIBRATED, "getActiveTicks")
    listeners["sculk_shrieker"]["shrieking_ticks"] = constant("SHRIEKING_TICKS")
    # `RESONANCE_PITCH_BEND`'s tone map: the value stored by each `iastore`.
    tones, last = [], None
    for line in bodies[(sensor, "lambda$static$0")]:
        if "iastore" in line:
            tones.append(last)
        elif int_pushed(line) is not None:
            last = int_pushed(line)
    if len(tones) != 16:
        raise ValueError("unexpected resonance tone map")

    call_sites = {}
    for (cls, method), body in bodies.items():
        posted = sorted(
            {
                m[1].lower()
                for line in body
                for m in [re.search(rf"getstatic\s+#\d+\s+// Field {EVENT}\.([A-Z_0-9]+):", line)]
                if m and m[1] != "CODEC"
            }
        )
        if not posted or cls.endswith("GameEventTagsProvider") or ".gameevent." in cls:
            continue
        key = (cls, enclosing(method))
        if not any(POSTERS.search(line) for line in body) and key not in INDIRECT:
            continue
        call_sites.setdefault(key, set()).update(posted)
    for (cls, method), posted in call_sites.items():
        if not set(posted) <= names:
            raise ValueError(f"{cls}.{method} posts an unknown event")

    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.level.gameevent.GameEvent static registrations (order, notificationRadius)",
            "net.minecraft.world.level.gameevent.vibrations.VibrationSystem VIBRATION_FREQUENCY_FOR_EVENT "
            "and RESONANCE_EVENTS",
            "data/minecraft/tags/game_event/*.json and data/minecraft/tags/block/*.json",
            "SculkSensorBlock, CalibratedSculkSensorBlock, SculkShriekerBlockEntity and the VibrationUser "
            "classes (getListenerRadius, getActiveTicks, COOLDOWN_TICKS, SHRIEKING_TICKS, RESONANCE_PITCH_BEND)",
            "every net.minecraft class method naming a GameEvent field and calling gameEvent, "
            "causeUseVibration or forceScheduleVibration (lambdas folded into their method)",
        ],
        "events": events,
        "tags": tags,
        "block_tags": block_tags,
        "listeners": listeners,
        "resonance_tones": tones,
        "call_sites": [
            {"class": cls[len("net.minecraft.") :], "method": m, "events": sorted(posted)}
            for (cls, m), posted in sorted(call_sites.items())
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--javap", default="javap")
    args = parser.parse_args()
    try:
        fixture = extract(args.server, args.javap)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
