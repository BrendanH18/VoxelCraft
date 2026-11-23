#!/usr/bin/env python3
"""Extract Java 26.3's default (non-experimental) minecart constants.

Reads the pinned server and client jars with `javap` and writes the furnace
minecart's fuel and push rules, `OldMinecartBehavior`'s rail motion,
`AbstractMinecart`'s entity and cart pushes, the TNT minecart's fuse, blast
and rail protection, and the client's rolling and riding sound volumes.

Usage: extract_java_minecart_fixtures.py server.jar client.jar --output
tests/fixtures/java/26.3/minecarts.json (`javap` must be on PATH).
"""

import argparse
import hashlib
import io
import json
import re
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1
from extract_java_registry_names import CLIENT_SHA1

CART = "net.minecraft.world.entity.vehicle.minecart."
LITERAL = re.compile(
    r"\d+: (?:(iconst_m?\d)|[fd]const_(\d)|[bs]ipush\s+(-?\d+)"
    r"|ldc(?:2_w|_w)?\s+#\d+\s+// (float|double|int|long) (\S+?)[fdl]?$)",
    re.M,
)


def javap(jar, name):
    return subprocess.check_output(["javap", "-c", "-p", "-constants", "-classpath", str(jar), name], text=True)


def method(code, signature):
    """The bytecode of the one method whose declaration contains `signature`."""
    blocks = [b for b in code.split("\n\n") if signature in b.strip().split("\n")[0]]
    if len(blocks) != 1:
        raise ValueError(f"expected one method matching {signature!r}, found {len(blocks)}")
    return blocks[0]


def literals(block):
    """Numeric constants in bytecode order (float constants keep their f32 value)."""
    out = []
    for m in LITERAL.finditer(block):
        if m[1]:
            out.append(int(m[1].split("_")[1].replace("m", "-")))
        elif m[2] is not None:
            out.append(float(m[2]))
        elif m[3]:
            out.append(int(m[3]))
        elif m[4] == "float":
            out.append(f32(float(m[5])))
        else:
            out.append(float(m[5]))
    return out


def constant(code, name):
    m = re.search(rf"static final (\w+) {name} = (-?[\d.E-]+)[fdL]?;", code)
    if not m:
        raise ValueError(f"missing constant {name}")
    if m[1] == "float":
        return f32(float(m[2]))
    return float(m[2]) if m[1] == "double" else int(m[2])


def expect(label, got, want):
    if got != want:
        raise ValueError(f"reassess {label}: {got!r} != {want!r}")
    return got


def f32(value):
    """A Java float literal as Java widens it to double."""
    return struct.unpack("f", struct.pack("f", value))[0]


def data_json(jar, name):
    return json.loads(jar.read(f"data/minecraft/{name}.json"))


def strip(name):
    return name.removeprefix("minecraft:")


def extract(server, client):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned Java 26.3 server")
    client_data = client.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned Java 26.3 client")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory() as directory:
        jar_path = Path(directory) / "server.jar"
        jar_path.write_bytes(inner)
        client_path = Path(directory) / "client.jar"
        client_path.write_bytes(client_data)
        with zipfile.ZipFile(jar_path) as jar:
            fuel_tag = [strip(v) for v in data_json(jar, "tags/item/furnace_minecart_fuel")["values"]]
            rails_tag = [strip(v) for v in data_json(jar, "tags/block/rails")["values"]]
            recipe = data_json(jar, "recipe/furnace_minecart")
        furnace = javap(jar_path, CART + "MinecartFurnace")
        abstract = javap(jar_path, CART + "AbstractMinecart")
        old = javap(jar_path, CART + "OldMinecartBehavior")
        container = javap(jar_path, CART + "AbstractMinecartContainer")
        tnt = javap(jar_path, CART + "MinecartTNT")
        rolling = javap(client_path, "net.minecraft.client.resources.sounds.MinecartSoundInstance")
        local = javap(client_path, "net.minecraft.client.player.LocalPlayer")
        base_sound = javap(client_path, "net.minecraft.client.resources.sounds.AbstractSoundInstance")

    # MinecartFurnace.
    tick = method(furnace, " tick()")
    expect("furnace smoke", literals(tick), [1, 1, 0, 4, 0.8, 0.0, 0.0, 0.0])
    if "LARGE_SMOKE" not in tick:
        raise ValueError("reassess furnace smoke particle")
    speed = expect("furnace max speed", literals(method(furnace, "getMaxSpeed(")), [0.75, 0.5])
    slowdown = expect(
        "furnace slowdown",
        literals(method(furnace, "applyNaturalSlowdown(")),
        [1.0e-7, 0.8, 0.0, 0.8, 0.1, 0.98, 0.0, 0.98],
    )
    along = expect(
        "furnace push along", literals(method(furnace, "calculateNewPushAlong(")), [1.0e-4, 0.001, 1.0e-4, 0.001]
    )
    add_fuel = method(furnace, "addFuel(")
    if "FURNACE_MINECART_FUEL" not in add_fuel or "subtract" not in add_fuel or "horizontal" not in add_fuel:
        raise ValueError("reassess furnace fuelling and push direction")
    per_item = constant(furnace, "FUEL_TICKS_PER_ITEM")
    cap = constant(furnace, "MAX_FUEL_TICKS")
    expect("furnace fuel arithmetic", literals(add_fuel), [per_item, cap, per_item, 0, 1])
    expect("furnace recipe", [strip(i) for i in recipe["ingredients"]], ["furnace", "minecart"])
    if recipe["type"] != "minecraft:crafting_shapeless" or strip(recipe["result"]["id"]) != "furnace_minecart":
        raise ValueError("reassess furnace minecart recipe")
    if "Items.FURNACE_MINECART" not in method(furnace, "getDropItem("):
        raise ValueError("reassess furnace minecart drop")
    for key in ("PushX", "PushZ", "Fuel"):
        if f"String {key}" not in method(furnace, "addAdditionalSaveData("):
            raise ValueError(f"reassess furnace save key {key}")

    # AbstractMinecart and OldMinecartBehavior.
    gravity = expect("gravity", literals(method(abstract, "getDefaultGravity(")), [0.005, 0.04])
    air = expect("air drag", literals(method(abstract, "getAirDrag(")), [f32(0.95)])
    expect("water slowdown", literals(method(abstract, "Vec3 applyNaturalSlowdown(")), [0.0, f32(0.95)])
    push = expect(
        "entity push",
        literals(method(abstract, "void push(net.minecraft.world.entity.Entity)")),
        [f32(1.0e-4), 1.0, 1.0, 1.0, f32(0.1), f32(0.1), 0.5, 0.5, 0.0, 4.0, 0.0, 4.0],
    )
    other = literals(method(abstract, "pushOtherMinecart("))
    expect("cart alignment", other[4], f32(0.8))
    expect("cart push keep", other[5:12], [0.2, 1.0, 0.2, 0.0, 0.95, 1.0, 0.95])
    expect("cart push average", other[19:21], [2.0, 2.0])
    if other.count(0.95) != 4 or other.count(0.2) != 8:
        raise ValueError("reassess pushOtherMinecart momentum")
    move = literals(method(old, "moveAlongTrack("))
    expect("slope", move[4:6], [0.0078125, 0.2])
    expect("passenger nudge", move[22:24], [0.01, 0.001])
    expect("halt", move[27:29], [0.03, 0.5])
    expect("ridden scale", move[42], 0.75)
    expect("height to speed", move[45], 0.05)
    expect("boost", move[48:51], [0.01, 0.06, 0.06])
    expect("start nudge", move[-4:], [0.02, -0.02, 0.02, -0.02])
    pickup_code = method(old, "pushAndPickupEntities(")
    pickup = expect("pickup", literals(pickup_code), [f32(0.2), 0.0, f32(0.2), 0.01, 0])
    excluded = re.findall(r"instanceof\s+#\d+\s+// class (\S+)", pickup_code)
    expect(
        "pickup exclusions",
        [e.rsplit("/", 1)[1] for e in excluded],
        ["Player", "IronGolem", "AbstractMinecart", "AbstractMinecart"],
    )
    max_speed = expect("max speed", literals(method(old, "getMaxSpeed(")), [0.2, 0.4])
    slow = expect("slowdown", literals(method(old, "getSlowdownFactor(")), [0.997, 0.96])
    container_code = method(container, "applyNaturalSlowdown(")
    expect("container slowdown", literals(container_code), [f32(0.98), 15, f32(0.001), f32(0.95), 0.0])

    # MinecartTNT.
    expect("tnt tick", literals(method(tnt, " tick()")), [1, 0.5, 0.0, 0.0, 0.0, f32(0.01)])
    expect("tnt destroy", literals(method(tnt, "destroy(net.minecraft.server.level.ServerLevel")), [f32(0.01), 20, 20])
    blast = expect("tnt blast", literals(method(tnt, "explode(")), [5.0, 1.5, 0])
    fall = expect("tnt fall", literals(method(tnt, "causeFallDamage(")), [3.0, 10.0])
    fuse = expect("tnt fuse", literals(method(tnt, "primeFuse(")), [80, 70, 1.0, 1.0])
    power_base = constant(tnt, "DEFAULT_EXPLOSION_POWER_BASE")
    for name in ("getBlockExplosionResistance(", "shouldBlockExplode("):
        code = method(tnt, name)
        if not ("isPrimed" in code and code.count("BlockTags.RAILS") == 2 and "BlockPos.above" in code):
            raise ValueError(f"reassess tnt {name}")
    expect("tnt protected resistance", literals(method(tnt, "getBlockExplosionResistance(")), [0.0])
    hurt = method(tnt, "hurtServer(")
    if "AbstractArrow" not in hurt or "isOnFire" not in hurt:
        raise ValueError("reassess burning arrows on tnt carts")
    ignites = method(tnt, "damageSourceIgnitesTnt(")
    if "Projectile" not in ignites or "isOnFire" not in ignites or "IS_FIRE" not in ignites:
        raise ValueError("reassess tnt ignition sources")

    # Client sounds: `MinecartSoundInstance` keeps its own private `pitch`,
    # shadowing `AbstractSoundInstance.pitch`, so playback stays at 1.0.
    roll_tick = method(rolling, "public void tick()")
    roll = expect(
        "rolling volume",
        literals(roll_tick),
        [1, 0, f32(0.01), f32(0.0025), 0.0, 1.0, 0.0, f32(0.5), 0.0, f32(0.7), 0.0, 0.0],
    )
    if "private float pitch;" not in rolling or "getPitch" in rolling:
        raise ValueError("reassess the rolling sound's pitch")
    if "protected float pitch;" not in base_sound:
        raise ValueError("reassess AbstractSoundInstance.pitch")
    volume_max = constant(rolling, "VOLUME_MAX")
    if volume_max != roll[9] or "MINECART_RIDING" not in rolling or "SoundSource.NEUTRAL" not in rolling:
        raise ValueError("reassess the rolling sound")
    riding = method(local, "startRiding(")
    inside = expect("riding sound volumes", literals(riding)[:9], [0, 1, 0.0, f32(0.75), 1.0, 0, 0.0, f32(0.75), 1.0])
    if "MINECART_INSIDE_UNDERWATER" not in riding or "MINECART_INSIDE:" not in riding:
        raise ValueError("reassess the riding sound events")

    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "client_sha1": CLIENT_SHA1,
        "sources": [
            "MinecartFurnace (tick, getMaxSpeed, applyNaturalSlowdown, calculateNewPushAlong, addFuel)",
            "data/minecraft/tags/item/furnace_minecart_fuel.json, recipe/furnace_minecart.json",
            "AbstractMinecart (getDefaultGravity, getAirDrag, applyNaturalSlowdown, push, pushOtherMinecart)",
            "OldMinecartBehavior (moveAlongTrack, pushAndPickupEntities, getMaxSpeed, getSlowdownFactor)",
            "AbstractMinecartContainer.applyNaturalSlowdown",
            "MinecartTNT (tick, destroy, explode, causeFallDamage, primeFuse, getBlockExplosionResistance,"
            " shouldBlockExplode, hurtServer, damageSourceIgnitesTnt); data/minecraft/tags/block/rails.json",
            "client MinecartSoundInstance.tick, LocalPlayer.startRiding (RidingMinecartSoundInstance)",
        ],
        "furnace": {
            "fuel_ticks_per_item": per_item,
            "max_fuel_ticks": cap,
            "fuel_items": fuel_tag,
            "recipe": ["furnace", "minecart"],
            "max_speed_factor_water": speed[0],
            "max_speed_factor_land": speed[1],
            "push_epsilon": slowdown[0],
            "push_friction": slowdown[1],
            "water_push_scale": slowdown[4],
            "idle_friction": slowdown[5],
            "push_along_epsilon": along[0],
            "movement_epsilon": along[1],
            "smoke_chance_bound": 4,
            "smoke_height": 0.8,
        },
        "motion": {
            "max_speed_water": max_speed[0],
            "max_speed_land": max_speed[1],
            "slope": move[4],
            "water_slope_factor": move[5],
            "powered_boost": move[49],
            "halt_speed": move[27],
            "slowdown_ridden": slow[0],
            "slowdown_empty": slow[1],
            "water_slowdown": air[0],
            "container_base": f32(0.98),
            "container_step": f32(0.001),
            "water_gravity": gravity[0],
            "gravity": gravity[1],
            "air_drag": air[0],
        },
        "push": {
            "min_distance_sq": push[0],
            "entity_scale": push[4],
            "half": push[6],
            "reaction_divisor": push[9],
            "alignment_dot": other[4],
            "keep": 0.2,
            "furnace_keep": 0.95,
            "pickup_inflate": pickup[0],
            "pickup_speed_sq": pickup[3],
            "pickup_excludes": ["Player", "IronGolem", "AbstractMinecart"],
        },
        "tnt": {
            "activator_fuse": fuse[0],
            "random_fuse_bound": 20,
            "power_base": power_base,
            "speed_factor": blast[1],
            "speed_cap": blast[0],
            "fall_threshold": fall[0],
            "fall_divisor": fall[1],
            "impact_speed_sq": f32(0.01),
            "protected_rails": rails_tag,
            "protects_block_below_rail": True,
            "only_when_primed": True,
        },
        "sound": {
            "rolling": {
                "event": "entity.minecart.riding",
                "source": "neutral",
                "min_speed": roll[2],
                "speed_clamp": roll[7],
                "volume_max": volume_max,
                "pitch": 1.0,
            },
            "inside": {
                "events": ["entity.minecart.inside", "entity.minecart.inside.underwater"],
                "source": "neutral",
                "min_speed": f32(0.01),
                "volume_min": inside[2],
                "volume_max": inside[3],
                "amplifier": inside[4],
                "attenuation": "none",
            },
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar")
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server, args.client), indent=2) + "\n")


if __name__ == "__main__":
    main()
