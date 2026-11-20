#!/usr/bin/env python3
"""Extract the v0.16 Track B flight and storage facts from the pinned Java 26.3 server.

Covers firework rockets (`FireworkRocketEntity` launch drift, lifetime,
acceleration, gliding boost and burst damage; `FireworkRocketItem`
durations, placement offset and dispenser shot; `CrossbowItem` powers and
durability use; `Fireworks.MAX_EXPLOSIONS`; the explosion shapes; every
dye's `fireworkColor`), elytra (`Items.ELYTRA` durability, rarity and
repair item; `LivingEntity`'s glide drag, wear interval, lift, dive and
climb factors and wall damage), shulker boxes (`ShulkerBoxBlockEntity` and
`Blocks.shulkerBoxProperties` constants, the block list, loot tables,
recipes, and the tags that name them), and the firework recipes.

It then runs a small independent model of
`LivingEntity.updateFallFlyingMovement` (with `Entity.calculateViewVector`
and `Mth.sin`/`Mth.cos` tables, in Java's float and double arithmetic) and of
the rocket boost over a table of cases, written from the decompiled 26.3
classes. The game is compared with the fixture in `tests/java_flight.rs`.

Requires javap. Unknown bytecode forms fail rather than regenerating a
guessed fixture. No VoxelCraft source or existing fixture is consulted.
"""

import argparse
import hashlib
import io
import json
import math
import re
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

COLOURS = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
]
SHULKER_BOXES = ["shulker_box"] + [c + "_shulker_box" for c in COLOURS]
RECIPES = ["firework_rocket", "firework_rocket_simple", "firework_star", "firework_star_fade", "shulker_box"] + [
    c + "_shulker_box" for c in COLOURS
]
TAGS = [
    "block/shulker_boxes",
    "item/shulker_boxes",
    "block/can_glide_through",
    "item/enchantable/durability",
    "item/enchantable/equippable",
    "item/skulls",
]


def f32(x):
    """Round to a Java float."""
    return struct.unpack("f", struct.pack("f", x))[0]


def strip(name):
    if not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name}")
    return name[len("minecraft:") :]


def read_json(jar, path):
    return json.loads(jar.read(path))


def tag_values(jar, path):
    return [v if v.startswith("#") else strip(v) for v in read_json(jar, path)["values"]]


def method(text, signature):
    """The bytecode of the method whose declaration contains `signature`."""
    start = text.find(signature)
    if start < 0:
        raise ValueError(f"missing method {signature}")
    end = text.find("\n\n", start)
    return text[start : end if end > 0 else len(text)]


def constants(body):
    """Numeric `ldc`/`bipush`/`sipush` constants of a method body, in order."""
    out = []
    for m in re.finditer(r"(?:ldc\w*\s+#\d+\s+// (?:double|float|int) ([-0-9.E]+)[df]?|[bs]ipush\s+(-?\d+))", body):
        out.append(float(m[1]) if m[1] is not None else int(m[2]))
    return out


def expect(values, wanted, what):
    if values != wanted:
        raise ValueError(f"unrecognized {what}: {values}")
    return values


def static_int(text, name):
    m = re.search(r"static final int " + name + r" = (-?\d+);", text)
    if not m:
        raise ValueError(f"missing {name}")
    return int(m[1])


def static_float(text, name):
    m = re.search(r"static final (?:float|double) " + name + r" = ([-0-9.E]+)[fd]?;", text)
    if not m:
        raise ValueError(f"missing {name}")
    return float(m[1])


def item_window(items, field):
    upper = field.upper()
    start = re.search(r"getstatic[^\n]*Field net/minecraft/references/ItemIds\." + upper + r":", items)
    if not start:
        raise ValueError(f"no registration for {field}")
    end = re.search(r"putstatic[^\n]*Field " + upper + r":", items[start.end() :])
    if not end:
        raise ValueError(f"no putstatic for {field}")
    return items[start.start() : start.end() + end.end()]


SCALE = 10430.378350470453


def mth_sin(x):
    return f32(math.sin((int(x * SCALE) & 65535) / SCALE))


def mth_cos(x):
    return f32(math.sin((int(x * SCALE + 16384.0) & 65535) / SCALE))


DEG = f32(math.pi / 180.0)


def view_vector(x_rot, y_rot):
    """`Entity.calculateViewVector` in Java's float arithmetic."""
    rx = f32(x_rot * DEG)
    ry = f32(-y_rot * DEG)
    y_cos, y_sin, x_cos, x_sin = mth_cos(ry), mth_sin(ry), mth_cos(rx), mth_sin(rx)
    return (f32(y_sin * x_cos), -x_sin, f32(y_cos * x_cos))


def glide(m, x_rot, y_rot, gravity, k):
    """`LivingEntity.updateFallFlyingMovement` with the extracted constants `k`."""
    lx, ly, lz = view_vector(x_rot, y_rot)
    lean = f32(x_rot * DEG)
    look_hor = math.sqrt(lx * lx + lz * lz)
    move_hor = math.sqrt(m[0] * m[0] + m[2] * m[2])
    lift = math.cos(lean) ** 2
    x, y, z = m[0], m[1] + gravity * (k["minus_one"] + lift * k["lift"]), m[2]
    if y < 0.0 and look_hor > 0.0:
        convert = y * k["convert"] * lift
        x, y, z = x + lx * convert / look_hor, y + convert, z + lz * convert / look_hor
    if lean < 0.0 and look_hor > 0.0:
        convert = move_hor * -mth_sin(lean) * k["dive"]
        x, y, z = x - lx * convert / look_hor, y + convert * k["climb"], z - lz * convert / look_hor
    if look_hor > 0.0:
        x += (lx / look_hor * move_hor - x) * k["steer"]
        z += (lz / look_hor * move_hor - z) * k["steer"]
    return [x * k["drag_h"], y * k["drag_v"], z * k["drag_h"]]


def boost(m, x_rot, y_rot, r):
    lx, ly, lz = view_vector(x_rot, y_rot)
    return [m[i] + (look * r["add"] + (look * r["power"] - m[i]) * r["blend"]) for i, look in enumerate((lx, ly, lz))]


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-flight-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        rocket = inspect("world.entity.projectile.FireworkRocketEntity")
        init = method(rocket, "ItemStack);\n")
        expect(constants(init), [0.002297, 0.05, 0.002297, 10, 6, 7], "FireworkRocketEntity launch")
        tick = method(rocket, "public void tick();")
        tick_k = constants(tick)
        expect(tick_k[:12], [1.5, 0.1, 0.1, 1.5, 0.5, 0.1, 1.5, 0.5, 0.1, 1.5, 0.5, 1.15], "rocket tick")
        if 0.04 not in tick_k or 3.0 not in tick_k:
            raise ValueError("rocket tick lacks its climb or launch volume")
        damage = method(rocket, "private void dealExplosionDamage(")
        expect(constants(damage)[:2], [5.0, 5.0], "burst damage base")
        if "iconst_2" not in damage or 25.0 not in constants(damage):
            raise ValueError("unrecognized burst damage")

        rocket_item = inspect("world.item.FireworkRocketItem")
        durations = re.findall(r"iconst_(\d)\n[^\n]*bastore", rocket_item)
        offset = static_float(rocket_item, "ROCKET_PLACEMENT_OFFSET")
        dispense_body = method(rocket_item, "createDispenseConfig()")
        if "fconst_1" not in dispense_body:
            raise ValueError("unrecognized rocket dispense uncertainty")
        dispense = [1.0] + expect(constants(dispense_body), [0.5, 1004], "rocket dispense config")

        crossbow = inspect("world.item.CrossbowItem")
        durability_use = method(crossbow, "protected int getDurabilityUse(")
        if "iconst_3" not in durability_use:
            raise ValueError("unrecognized crossbow durability use")

        fireworks = inspect("world.item.component.Fireworks")
        shape = inspect("world.item.component.FireworkExplosion$Shape")
        shape_names = [s for s in re.findall(r"// String ([a-z_]+)\n", method(shape, "static {};")) if s.islower()]
        if len(shape_names) != 5:
            raise ValueError(f"unrecognized firework shapes {shape_names}")

        dye = inspect("world.item.DyeColor")
        clinit = method(dye, "static {};")
        colour_rows = re.findall(r"// String ([a-z_]+)\n(.*?)invokespecial", clinit, re.S)
        firework_colors = {}
        for name, body in colour_rows:
            # Texture diffuse colour, firework colour, then text colour
            # (black's 0 is an `iconst_0`, after the two `ldc`s).
            literal = re.findall(r"ldc\w*\s+#\d+\s+// int (-?\d+)", body)
            if name in COLOURS:
                if len(literal) < 2:
                    raise ValueError(f"unrecognized DyeColor {name}")
                firework_colors[name] = int(literal[1])
        if sorted(firework_colors) != sorted(COLOURS):
            raise ValueError("missing DyeColor firework colours")

        living = inspect("world.entity.LivingEntity")
        moves = constants(method(living, "updateFallFlyingMovement(net.minecraft.world.phys.Vec3)"))
        expect(len(moves), 11, "updateFallFlyingMovement constants")
        k = {
            "degrees": moves[0],
            "minus_one": moves[1],
            "lift": moves[2],
            "convert": moves[3],
            "dive": moves[4],
            "climb": moves[5],
            "steer": moves[6],
            "drag_h": moves[8],
            "drag_v": moves[9],
        }
        if moves[6] != moves[7] or moves[8] != moves[10]:
            raise ValueError("unrecognized glide steering or drag")
        wall = expect(constants(method(living, "handleFallFlyingCollisions(")), [10.0, 3.0], "wall damage")

        entity_box = inspect("world.level.block.entity.ShulkerBoxBlockEntity")
        blocks = inspect("world.level.block.Blocks")
        props = method(blocks, "shulkerBoxProperties(")
        if "fconst_2" not in props or "strength:(F)" not in props:
            raise ValueError("unrecognized shulker box strength")
        strength = 2.0
        if "PushReaction.POPPED" not in props:
            raise ValueError("shulker boxes no longer pop")

        items = inspect("world.item.Items")
        elytra = item_window(items, "elytra")
        elytra_durability = expect(constants(elytra)[:1], [432], "elytra durability")[0]
        if "Rarity.EPIC" not in elytra or "PHANTOM_MEMBRANE" not in elytra or "EquipmentSlot.CHEST" not in elytra:
            raise ValueError("unrecognized elytra properties")
        rocket_default = item_window(items, "firework_rocket")
        if "iconst_1" not in rocket_default:
            raise ValueError("unrecognized firework rocket default")

        r = {"power": tick_k[0], "add": tick_k[1], "blend": tick_k[4]}
        glide_cases = []
        for x_rot, y_rot, start in [
            (0.0, 0.0, [0.0, 0.0, 0.5]),
            (30.0, 45.0, [0.2, -0.3, 0.6]),
            (-20.0, -120.0, [0.8, 0.1, -0.4]),
            (60.0, 170.0, [0.0, -1.2, 0.3]),
            (-45.0, 90.0, [1.2, -0.2, 0.0]),
        ]:
            m = start
            path_out = []
            for _ in range(40):
                m = glide(m, x_rot, y_rot, 0.08, k)
                path_out.append(m)
            glide_cases.append({"x_rot": x_rot, "y_rot": y_rot, "start": start, "gravity": 0.08, "ticks": path_out})
        boost_cases = []
        for x_rot, y_rot, start in [(0.0, 0.0, [0.0, -0.5, 0.5]), (-30.0, 200.0, [1.0, 0.2, -1.0])]:
            boost_cases.append(
                {"x_rot": x_rot, "y_rot": y_rot, "start": start, "result": boost(start, x_rot, y_rot, r)}
            )

        with zipfile.ZipFile(path) as jar:
            loot = {}
            for b in SHULKER_BOXES:
                table = read_json(jar, f"data/minecraft/loot_table/blocks/{b}.json")
                (pool,) = table["pools"]
                (entry,) = pool["entries"]
                loot[b] = {
                    "item": strip(entry["name"]),
                    "copies": sorted(strip(c) for c in entry["modifier"]["include"]),
                    "conditions": sorted(c["condition"] for c in pool.get("conditions", [])),
                }
            facts = {
                "reference": "Java 26.3 behaviour data only; no artwork",
                "source": "official 26.3 server.jar via tools/extract_java_flight_fixtures.py",
                "server_sha1": SERVER_SHA1,
                "rocket": {
                    "launch_drift": constants(init)[0],
                    "launch_speed": constants(init)[1],
                    "lifetime_per_flight": constants(init)[3],
                    "lifetime_rolls": constants(init)[4:6],
                    "horizontal_acceleration": tick_k[11],
                    "vertical_acceleration": 0.04,
                    "boost": r,
                    "damage_base": 5.0,
                    "damage_per_star": 2,
                    "damage_radius": 5.0,
                },
                "rocket_item": {
                    "craftable_durations": [int(d) for d in durations],
                    "placement_offset": offset,
                    "dispense_uncertainty": dispense[0],
                    "dispense_power": dispense[1],
                    "dispense_event": dispense[2],
                    "default_flight": 1,
                },
                "crossbow": {
                    "firework_power": static_float(crossbow, "FIREWORK_POWER"),
                    "arrow_power": static_float(crossbow, "ARROW_POWER"),
                    "firework_durability_use": 3,
                },
                "fireworks": {"max_explosions": static_int(fireworks, "MAX_EXPLOSIONS"), "shapes": shape_names},
                "firework_colors": firework_colors,
                "elytra": {
                    "durability": elytra_durability,
                    "rarity": "epic",
                    "repair": "phantom_membrane",
                    "slot": "chest",
                    "drag": [
                        static_float(living, "ELYTRA_HORIZONTAL_AIR_DRAG"),
                        static_float(living, "ELYTRA_VERTICAL_AIR_DRAG"),
                    ],
                    "ticks_per_free_fall_event": static_int(living, "TICKS_PER_ELYTRA_FREE_FALL_EVENT"),
                    "free_fall_events_per_break": static_int(living, "FREE_FALL_EVENTS_PER_ELYTRA_BREAK"),
                    "glide_constants": k,
                    "wall_damage": wall,
                },
                "shulker_box": {
                    "container_size": static_int(entity_box, "CONTAINER_SIZE"),
                    "columns": static_int(entity_box, "COLUMNS"),
                    "rows": static_int(entity_box, "ROWS"),
                    "opening_tick_length": static_int(entity_box, "OPENING_TICK_LENGTH"),
                    "max_lid_height": static_float(entity_box, "MAX_LID_HEIGHT"),
                    "max_lid_rotation": static_float(entity_box, "MAX_LID_ROTATION"),
                    "strength": strength,
                    "push_reaction": "popped",
                    "blocks": SHULKER_BOXES,
                    "loot": loot,
                },
                "recipes": {name: read_json(jar, f"data/minecraft/recipe/{name}.json") for name in RECIPES},
                "tags": {t: tag_values(jar, f"data/minecraft/tags/{t}.json") for t in TAGS},
                "glide_cases": glide_cases,
                "boost_cases": boost_cases,
            }
    return facts


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("output", type=Path, help="where to write flight.json")
    args = parser.parse_args()
    facts = extract(args.server)
    args.output.write_text(json.dumps(facts, indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
