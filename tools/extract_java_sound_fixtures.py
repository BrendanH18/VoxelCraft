#!/usr/bin/env python3
"""Extract Java 26.3's subtitles, block sound types and container lid timing.

Reads three pinned official artifacts and writes
`tests/fixtures/java/26.3/sounds.json`:

- the client's sound index, `minecraft/sounds.json` (an asset object, not in
  the client jar: asset index 34, listed by the 26.3 version JSON, names it by
  SHA-1 `SOUNDS_SHA1`; download it from
  `https://resources.download.minecraft.net/79/<SOUNDS_SHA1>`). Every sound
  event with a `subtitle` key, the English text of each key from the client
  jar's `assets/minecraft/lang/en_us.json`, and whether each event loops is
  not in this file: Java decides that per `SoundInstance`.
- `SoundType.<clinit>` (server jar): each constant's volume, pitch and its
  break, step, place, hit and fall events (`SoundEvents.<clinit>` names
  them), and whether its hit and fall events play the step event's files.
- Every registered block's `SoundType`, evaluated from `Blocks.<clinit>` by
  `java_block_properties`, then the constructors that replace it: doors,
  trapdoors, buttons and pressure plates take their `BlockSetType`'s, fence
  gates and signs their `WoodType`'s and hanging signs its
  `hangingSignSoundType`. `DecoratedPotBlock.getSoundType` is the one
  state-dependent override (cracked pots).
- The client's subtitle options and `SubtitleOverlay` constants, and the lid
  speeds of `ChestLidController.tickLid` and
  `ShulkerBoxBlockEntity.updateAnimation`.

Usage: extract_java_sound_fixtures.py server.jar client.jar sounds.json
--output tests/fixtures/java/26.3/sounds.json (`javap` must be on PATH).
Never reads VoxelCraft source.
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
from extract_java_registry_names import CLIENT_SHA1
from java_block_properties import BLOCKS, Interpreter, Lambda, Opaque

SOUNDS_SHA1 = "7975165ad4a3c0cb3c05078b914eac0f2c3b5f5f"
ASSET_INDEX = "34 (SHA-1 f3df28b441565ca4e5deb9cda856d90df62e1433)"
SOUND_TYPE = "net/minecraft/world/level/block/SoundType"
SOUND_EVENTS = "net/minecraft/sounds/SoundEvents"
BLOCK_SET_TYPE = "net/minecraft/world/level/block/state/properties/BlockSetType"
WOOD_TYPE = "net/minecraft/world/level/block/state/properties/WoodType"
BLOCK_PACKAGE = "net/minecraft/world/level/block/"
# Block classes whose constructor calls `Properties.sound` with the sound type
# of the set or wood type it is given (the first or, for hanging signs, the
# second `SoundType` of a `WoodType`).
SET_TYPE_CLASSES = ("DoorBlock", "TrapDoorBlock", "ButtonBlock", "BasePressurePlateBlock")
WOOD_TYPE_CLASSES = {
    "FenceGateBlock": 0,
    "StandingSignBlock": 0,
    "WallSignBlock": 0,
    "CeilingHangingSignBlock": 1,
    "WallHangingSignBlock": 1,
}
EVENTS = ("break", "step", "place", "hit", "fall")


def javap(jar, name, *flags):
    return subprocess.check_output(["javap", "-c", "-p", *flags, "-classpath", str(jar), name], text=True)


def clinit(code):
    return code.split("  static {};")[1]


def sound_event_names(jar):
    """`SoundEvents` field -> event name, from each field's `register` call."""
    names, start = {}, 0
    body = clinit(javap(jar, SOUND_EVENTS.replace("/", ".")))
    for match in re.finditer(r"putstatic\s+#\d+\s+// Field (\w+):Lnet/minecraft/sounds/SoundEvent;", body):
        strings = re.findall(r"// String ([\w.]+)\n", body[start : match.start()])
        start = match.end()
        if strings:
            names[match[1]] = strings[-1]
    return names


def sound_types(jar, events):
    """Every `SoundType` constant: volume, pitch and its five events."""
    body = clinit(javap(jar, SOUND_TYPE.replace("/", ".")))
    out, start = {}, 0
    for match in re.finditer(r"putstatic\s+#\d+\s+// Field (\w+):L" + SOUND_TYPE + ";", body):
        chunk = body[start : match.start()]
        start = match.end()
        floats = []
        for m in re.finditer(r"fconst_(\d)|ldc(?:_w)?\s+#\d+\s+// float ([\d.Ee-]+)f", chunk):
            floats.append(float(m[1]) if m[1] is not None else float(m[2]))
        fields = re.findall(r"getstatic\s+#\d+\s+// Field net/minecraft/sounds/SoundEvents\.(\w+):", chunk)
        if "new " not in chunk or len(floats) != 2 or len(fields) != 5:
            raise ValueError(f"SoundType.{match[1]} is not new SoundType(volume, pitch, five events)")
        out[match[1]] = {
            "volume": floats[0],
            "pitch": floats[1],
            **{kind: events[field] for kind, field in zip(EVENTS, fields)},
        }
    return out


def type_sounds(jar, owner):
    """`BlockSetType` or `WoodType` constant -> the `SoundType`s it names."""
    code = javap(jar, owner.replace("/", "."))
    body = clinit(code)
    sound = r"getstatic\s+#\d+\s+// Field " + SOUND_TYPE + r"\.(\w+):"
    # A constructor without `SoundType` parameters delegates to the full one
    # with defaults (`WoodType(name, setType)` passes `WOOD` and `HANGING_SIGN`).
    defaults = {}
    for block in code.split("\n\n"):
        header = block.strip().split("\n")[0]
        if f"{owner.replace('/', '.')}(" in header and "SoundType" not in header:
            params = re.search(r"\(([^)]*)\)", header)[1]
            defaults[params.count(",") + 1 if params else 0] = re.findall(sound, block)
    out, start = {}, 0
    for match in re.finditer(r"putstatic\s+#\d+\s+// Field (\w+):L" + owner + ";", body):
        chunk = body[start : match.start()]
        start = match.end()
        found = re.findall(sound, chunk)
        if not found:
            init = re.findall(r'Method "<init>":\(([^)]*)\)V', chunk)
            arity = len(re.findall(r"L[^;]+;|[ZBCSIJFD]", init[-1])) if init else -1
            if arity not in defaults or not defaults[arity]:
                raise ValueError(f"{owner}.{match[1]}: no sound types")
            found = defaults[arity]
        out[match[1]] = found
    return out


def factory_types(interpreter, factory):
    """`BlockSetType`/`WoodType` constants a registration's factory reads."""
    found = []
    for value in factory.captured:
        if isinstance(value, Opaque):
            found += re.findall(r"(BlockSetType|WoodType)\.(\w+)$", value.what)
    if factory.kind == "invokeStatic":
        code, _ = interpreter.cls(factory.owner).method(factory.method, factory.descriptor)
        for _, _, comment, _ in code.values():
            m = re.search(r"properties/(BlockSetType|WoodType)\.(\w+):", comment)
            if m:
                found.append((m[1], m[2]))
    return list(dict.fromkeys(found))


def block_sound_types(jar):
    interpreter = Interpreter(jar)
    interpreter.static(BLOCKS, "AIR")
    set_types = type_sounds(jar, BLOCK_SET_TYPE)
    wood_types = type_sounds(jar, WOOD_TYPE)
    out = {}
    for name, props in interpreter.registered.items():
        sound = props.sound
        cls = props.block_class
        ancestry = interpreter.ancestry(cls) if cls else []
        set_class = next((c for c in SET_TYPE_CLASSES if BLOCK_PACKAGE + c in ancestry), None)
        wood_class = next((c for c in WOOD_TYPE_CLASSES if BLOCK_PACKAGE + c in ancestry), None)
        if set_class or wood_class:
            factory = interpreter.factories[name]
            if not isinstance(factory, Lambda):
                raise ValueError(f"{name}: can't read the type its {cls} is given")
            kinds = factory_types(interpreter, factory)
            wanted = "BlockSetType" if set_class else "WoodType"
            kinds = [k for k in kinds if k[0] == wanted]
            if len(kinds) != 1:
                raise ValueError(f"{name}: expected one {wanted}, found {kinds}")
            if set_class:
                (sound,) = set_types[kinds[0][1]]
            else:
                sound = wood_types[kinds[0][1]][WOOD_TYPE_CLASSES[wood_class]]
        if sound in ("unknown", "branch_dependent"):
            raise ValueError(f"{name}: sound type {sound}")
        out[name] = sound
    return dict(sorted(out.items()))


def check_constructors(jar):
    """The constructors above replace the properties' sound type, and only
    `DecoratedPotBlock` overrides `getSoundType`."""
    for cls in (*SET_TYPE_CLASSES, *WOOD_TYPE_CLASSES):
        code = javap(jar, (BLOCK_PACKAGE + cls).replace("/", "."))
        accessor = "hangingSignSoundType" if WOOD_TYPE_CLASSES.get(cls) == 1 else "soundType"
        if not re.search(accessor + r":\(\)L" + SOUND_TYPE + r";\n.*Properties\.sound:", code):
            raise ValueError(f"{cls} no longer takes its type's {accessor}")
    pot = javap(jar, "net.minecraft.world.level.block.DecoratedPotBlock")
    method = pot.split("SoundType getSoundType(")[1].split("\n\n")[0]
    if not ("DECORATED_POT_CRACKED" in method and "DECORATED_POT:" in method and "CRACKED" in method):
        raise ValueError("DecoratedPotBlock.getSoundType changed")


def literal_floats(code):
    return [float(m) for m in re.findall(r"// float ([\d.Ee-]+)f", code)]


def method_body(code, signature):
    blocks = [b for b in code.split("\n\n") if signature in b.strip().split("\n")[0]]
    if len(blocks) != 1:
        raise ValueError(f"expected one method matching {signature!r}, found {len(blocks)}")
    return blocks[0]


def lid_facts(jar):
    chest = javap(jar, "net.minecraft.world.level.block.entity.ChestLidController")
    shulker = javap(jar, "net.minecraft.world.level.block.entity.ShulkerBoxBlockEntity")
    chest_step = literal_floats(method_body(chest, " tickLid("))
    shulker_step = literal_floats(method_body(shulker, " updateAnimation("))
    if set(chest_step) != {0.1} or set(shulker_step) != {0.1}:
        raise ValueError(f"lid speeds changed: {chest_step} {shulker_step}")
    sounds = {}
    for cls, method in (
        ("ChestBlockEntity", " playSound("),
        ("BarrelBlockEntity", " playSound("),
        ("EnderChestBlockEntity$1", " onOpen("),
        ("ShulkerBoxBlockEntity", " startOpen("),
    ):
        body = method_body(javap(jar, f"net.minecraft.world.level.block.entity.{cls}"), method)
        floats = literal_floats(body) + [float(m) for m in re.findall(r"fconst_(\d)", body)]
        if sorted(floats) != [0.1, 0.5, 0.9]:
            raise ValueError(f"{cls} no longer plays at volume 0.5, pitch 0.9 + 0.1 * random: {floats}")
        sounds[cls.removesuffix("$1")] = {"volume": 0.5, "pitch": [0.9, 1.0]}
    return {
        "chest_lid_step_per_tick": 0.1,
        "shulker_progress_step_per_tick": 0.1,
        "sources": [
            "ChestLidController.tickLid",
            "ShulkerBoxBlockEntity.updateAnimation",
            "ChestBlockEntity.playSound, BarrelBlockEntity.playSound, EnderChestBlockEntity onOpen/onClose, "
            "ShulkerBoxBlockEntity.startOpen/stopOpen",
        ],
        "open_close_sounds": sounds,
    }


def client_facts(client_jar, lang):
    options = javap(client_jar, "net.minecraft.client.Options")
    init = method_body(options, "net.minecraft.client.Options(")

    def after(key, pattern):
        tail = init.split(f"// String {key}\n", 1)[1]
        m = re.search(pattern, tail)
        if not m:
            raise ValueError(f"no default for {key}")
        return m

    subtitles = after("options.showSubtitles", r"(iconst_[01])\n[^\n]*\n?[^\n]*createBoolean")
    opacity = after("options.accessibility.text_background_opacity", r"ldc2_w\s+#\d+\s+// double ([\d.]+)d")
    chat_only = after("options.accessibility.text_background", r"(iconst_[01])\n[^\n]*Boolean\.valueOf")
    overlay = javap(client_jar, "net.minecraft.client.gui.components.SubtitleOverlay", "-constants")
    display = re.search(r"DISPLAY_TIME = (\d+)[lL];", overlay)
    looping = re.search(r"LOOPING_DISPLAY_FULL_BRIGHTNESS_TIME = (\d+);", overlay)
    if not display or not looping:
        raise ValueError("SubtitleOverlay constants changed")
    extract = method_body(overlay, " extractRenderState(")
    if "float 0.8f" not in extract or "bipush        35" not in extract:
        raise ValueError("SubtitleOverlay layout changed")
    return {
        "options": {
            "showSubtitles": {
                "label": lang["options.showSubtitles"],
                "tooltip": lang["options.showSubtitles.tooltip"],
                "default": subtitles[1] == "iconst_1",
            },
            "textBackgroundOpacity": {
                "label": lang["options.accessibility.text_background_opacity"],
                "default": float(opacity[1]),
                "range": [0.0, 1.0],
            },
            "backgroundForChatOnly": {
                "label": lang["options.accessibility.text_background"],
                "true": lang["options.accessibility.text_background.chat"],
                "false": lang["options.accessibility.text_background.everywhere"],
                "default": chat_only[1] == "iconst_1",
            },
        },
        "overlay": {
            "display_time_ms": int(display[1]),
            "looping_full_brightness_ms": int(looping[1]),
            "default_background_opacity": 0.8,
            "bottom_offset": 35,
            "brightness": [255, 75],
            "source": "net.minecraft.client.gui.components.SubtitleOverlay",
        },
    }


def files(index, event):
    """The sound files an event plays, without their per-file volume or pitch."""
    return [s if isinstance(s, str) else s["name"] for s in index.get(event, {}).get("sounds", [])]


def attenuation(index):
    out = {}
    for event, entry in sorted(index.items()):
        distances = [s.get("attenuation_distance", 16) if isinstance(s, dict) else 16 for s in entry.get("sounds", [])]
        if distances and max(distances) != 16:
            out[event] = max(distances)
    return out


def extract(server, client, sounds):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    client_data = client.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    sounds_data = sounds.read_bytes()
    if hashlib.sha1(sounds_data).hexdigest() != SOUNDS_SHA1:
        raise ValueError("not the pinned Java 26.3 sounds.json asset")
    index = json.loads(sounds_data)
    with zipfile.ZipFile(io.BytesIO(client_data)) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-sounds-") as temporary:
        jar = Path(temporary) / "server.jar"
        jar.write_bytes(inner)
        events = sound_event_names(jar)
        types = sound_types(jar, events)
        check_constructors(jar)
        blocks = block_sound_types(jar)
        # `DecoratedPotBlock.getSoundType` replaces its properties' sound.
        blocks["decorated_pot"] = "DECORATED_POT"
        lids = lid_facts(jar)
        client_jar = Path(temporary) / "client.jar"
        client_jar.write_bytes(client_data)
        facts = client_facts(client_jar, lang)
    for name, t in types.items():
        for kind in EVENTS:
            if t[kind] not in index and t[kind] != "intentionally_empty":
                raise ValueError(f"SoundType.{name}.{kind}: {t[kind]} is not in sounds.json")
        step = files(index, t["step"])
        t["hit_plays_step_files"] = files(index, t["hit"]) == step
        t["fall_plays_step_files"] = files(index, t["fall"]) == step
    used = sorted(set(blocks.values()) | {"DECORATED_POT_CRACKED"})
    subtitles = {event: entry["subtitle"] for event, entry in sorted(index.items()) if "subtitle" in entry}
    missing = sorted({k for k in subtitles.values() if k not in lang})
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "client_sha1": CLIENT_SHA1,
        "sounds_json_sha1": SOUNDS_SHA1,
        "asset_index": ASSET_INDEX,
        "sources": [
            "minecraft/sounds.json (subtitle of every sound event)",
            "assets/minecraft/lang/en_us.json (subtitle text)",
            "net.minecraft.sounds.SoundEvents.<clinit>",
            "net.minecraft.world.level.block.SoundType.<clinit>",
            "net.minecraft.world.level.block.Blocks.<clinit> (Properties.sound), the DoorBlock, TrapDoorBlock, "
            "ButtonBlock, BasePressurePlateBlock, FenceGateBlock and sign constructors, BlockSetType and WoodType",
            "DecoratedPotBlock.getSoundType (DECORATED_POT, or DECORATED_POT_CRACKED when cracked)",
            "net.minecraft.client.Options, net.minecraft.client.gui.components.SubtitleOverlay",
        ],
        # Keys with no English text show the key itself in Java.
        "subtitle_keys_without_text": missing,
        "subtitle_text": {k: lang[k] for k in sorted(set(subtitles.values())) if k in lang},
        "subtitles": subtitles,
        # Every other event, to check names against.
        "silent_events": sorted(event for event, entry in index.items() if "subtitle" not in entry),
        # `Sound.getAttenuationDistance` (16 unless a file sets it): the
        # farthest of each event's files, where any differs from 16.
        "attenuation_distances": attenuation(index),
        "sound_types": {name: types[name] for name in used},
        "block_sound_types": blocks,
        # English names, which identify blocks whose VoxelCraft name differs from the ID.
        "block_names": {name: lang.get(f"block.minecraft.{name}", "") for name in blocks},
        "decorated_pot_cracked": "DECORATED_POT_CRACKED",
        "lids": lids,
        **facts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("client", type=Path)
    parser.add_argument("sounds", type=Path, help="the minecraft/sounds.json asset")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = extract(args.server, args.client, args.sounds)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
