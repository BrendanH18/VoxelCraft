#!/usr/bin/env python3
"""Extract v0.15 Track D's facts from the pinned official Java 26.3 server artifact.

Covers:

* the Wind Charged, Weaving, Oozing and Infested effects' constructor
  arguments in `MobEffects` (colour, Infested's chance, and the counts their
  `ToIntFunction` lambdas return), and the four potions in `Potions`;
* `SpectralArrow`'s Glowing duration, `LivingEntity.travelInAir`'s
  Levitation constants and `WebBlock.entityInside`'s speed multipliers;
* `FrostedIceBlock`'s ticks, ages and neighbour limits, and Frost Walker's
  `replace_disk` effect (`enchantment/frost_walker.json`);
* the trim patterns, the trim materials' description colours, the
  templates' rarities (`Items`), the `trim_materials` and `eggs` item tags;
* the chicken, cow and pig variants (model and spawn biome tag) with the
  warm and cold biome tags expanded, `gameplay/chicken_lay`, and the
  chicken's egg timer;
* every loot table entry that names a spectral arrow, a coloured egg or a
  trim template.

Requires javap. Unknown bytecode forms fail rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
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

EFFECTS = ["wind_charged", "weaving", "oozing", "infested"]
TRACK_ITEMS = re.compile(r"^(spectral_arrow|blue_egg|brown_egg|[a-z]+_armor_trim_smithing_template)$")


def strip(name):
    if not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name}")
    return name[len("minecraft:") :]


def method(code, signature):
    """The bytecode of the first method whose declaration contains `signature`."""
    start = code.find(signature)
    if start < 0:
        raise ValueError(f"missing method: {signature}")
    end = code.find("\n\n", start)
    return code[start : end if end >= 0 else len(code)]


def ops(code):
    out = []
    for line in code.splitlines():
        match = re.match(r"\s+\d+: (\w+)\s*(.*)", line)
        if match:
            out.append(f"{match[1]} {match[2]}".strip())
    return out


def number(op):
    """The constant an int/float/double push leaves on the stack."""
    match = re.match(r"iconst_(m1|\d)", op)
    if match:
        return -1 if match[1] == "m1" else int(match[1])
    match = re.match(r"[bs]ipush\s+(-?\d+)", op)
    if match:
        return int(match[1])
    match = re.match(r"ldc2?_?w?\s+#\d+\s+// (int|float|double) (-?[\d.Ee+-]+)[fd]?$", op)
    if match:
        return int(match[2]) if match[1] == "int" else float(match[2])
    raise ValueError(f"not a constant push: {op}")


def window(code, start_marker, end_marker):
    start = code.find(start_marker)
    if start < 0:
        raise ValueError(f"missing {start_marker}")
    end = code.find(end_marker, start)
    if end < 0:
        raise ValueError(f"missing {end_marker} after {start_marker}")
    return code[start : end + len(end_marker)]


def effect_facts(effects):
    """`MobEffects`' registrations of the four effects and their lambdas."""
    lambdas = {}
    for block in effects.split("\n\n"):
        match = re.search(r"private static int lambda\$static\$(\d+)\(net\.minecraft\.util\.RandomSource\);", block)
        if not match:
            continue
        body = ops(block)
        if len(body) == 2 and body[1] == "ireturn":
            lambdas[int(match[1])] = {"type": "constant", "value": number(body[0])}
        elif len(body) == 5 and "Mth.randomBetweenInclusive" in body[3] and body[4] == "ireturn":
            lambdas[int(match[1])] = {"type": "uniform", "min": number(body[1]), "max": number(body[2])}
        else:
            raise ValueError(f"unrecognized MobEffects lambda {match[1]}: {body}")
    out = {}
    for name in EFFECTS:
        code = window(effects, f"// String {name}\n", f"// Field {name.upper()}:")
        listing = ops(code)
        cls = re.search(r"new\s+#\d+\s+// class net/minecraft/world/effect/(\w+)", code)
        category = re.search(r"MobEffectCategory\.(\w+):", code)
        if not cls or not category:
            raise ValueError(f"unrecognized registration of {name}")
        pushes = [op for op in listing if op.startswith("ldc")]
        fact = {"class": cls[1], "category": category[1].lower(), "colour": number(pushes[0])}
        if len(pushes) > 2:
            raise ValueError(f"{name} gained constructor constants")
        if len(pushes) == 2:
            fact["chance"] = number(pushes[1])
        dynamic = re.findall(r"InvokeDynamic #(\d+):applyAsInt", code)
        if len(dynamic) > 1:
            raise ValueError(f"{name} has several count functions")
        if dynamic:
            fact["count"] = lambdas[int(dynamic[0])]
        out[name] = fact
    return out


def potion_facts(potions):
    out = {}
    for name in EFFECTS:
        code = window(potions, f"// String {name}\n", f"// Field {name.upper()}:")
        effect = re.findall(r"MobEffects\.(\w+):", code)
        listing = ops(code)
        pushes = [number(op) for op in listing if re.match(r"(sipush|bipush|ldc)", op) and "String" not in op]
        if effect != [name.upper()] or len(pushes) != 1:
            raise ValueError(f"unrecognized potion {name}: {effect} {pushes}")
        out[name] = {"effect": name, "amplifier": 0, "duration": pushes[0]}
    return out


def frosted_ice_facts(frosted):
    place = method(frosted, "public void onPlace(")
    tick = method(frosted, "protected void tick(")
    place_ops = ops(place)
    place_delay = [number(op) for op in place_ops if op.startswith("bipush")]
    tick_ops = ops(tick)
    delays = [number(op) for op in tick_ops if op.startswith("bipush")]
    # `random.nextInt(3) == 0 || fewerNeigboursThan(level, pos, 4)`, then
    # `brightness > 11 - age - lightDampening`, then two reschedules.
    if place_delay != [60, 120] or delays != [11, 20, 40, 20, 40]:
        raise ValueError(f"unrecognized frosted ice delays {place_delay} {delays}")
    if "RandomSource.nextInt" not in tick or "getLightDampening" not in tick or "Level.END" not in tick:
        raise ValueError("unrecognized FrostedIceBlock.tick")
    constants = {}
    for field in ("MAX_AGE", "NEIGHBORS_TO_AGE", "NEIGHBORS_TO_MELT"):
        match = re.search(
            r"static final int " + field + r";\n\s+descriptor: I\n[^\n]*\n\s+ConstantValue: int (\d+)", frosted
        )
        match = match or re.search(r"static final int " + field + r" = (\d+);", frosted)
        if not match:
            raise ValueError(f"missing FrostedIceBlock.{field}")
        constants[field.lower()] = int(match[1])
    chance = re.search(r"iconst_(\d)\s*\n[^\n]*RandomSource\.nextInt", tick)
    if not chance:
        raise ValueError("unrecognized ageing chance")
    return {
        "place_delay": place_delay,
        "tick_delay": delays[1:3],
        "brightness_base": delays[0],
        "age_chance_bound": int(chance[1]),
        **constants,
    }


def travel_facts(living, web, arrow):
    air = method(living, "private void travelInAir(")
    if "MobEffects.LEVITATION" not in air:
        raise ValueError("travelInAir no longer reads Levitation")
    doubles = [float(m) for m in re.findall(r"// double ([\d.]+)d", air)]
    if doubles[:2] != [0.05, 0.2]:
        raise ValueError(f"unrecognized Levitation constants {doubles}")
    inside = method(web, "protected void entityInside(")
    # `new Vec3(0.25, 0.05F, 0.25)`: javac widens the float to a double
    # constant; with Weaving `new Vec3(0.5, 0.25, 0.5)`.
    web_doubles = [float(m) for m in re.findall(r"// double ([\d.]+)d", inside)]
    if "MobEffects.WEAVING" not in inside or len(web_doubles) != 6:
        raise ValueError(f"unrecognized WebBlock.entityInside {web_doubles}")
    durations = set(re.findall(r"sipush\s+(\d+)\s*\n[^\n]*putfield[^\n]*Field duration:I", arrow))
    if durations != {"200"} or "MobEffects.GLOWING" not in arrow:
        raise ValueError(f"unrecognized SpectralArrow duration {durations}")
    return {
        "levitation_speed_per_level": doubles[0],
        "levitation_ease": doubles[1],
        "web_speed": web_doubles[:3],
        "web_speed_weaving": web_doubles[3:],
        "spectral_glowing_ticks": 200,
    }


def rarities(items):
    out = {}
    for match in re.finditer(r"ItemIds\.([A-Z_]+_ARMOR_TRIM_SMITHING_TEMPLATE):", items):
        start = match.start()
        end = items.find("putstatic", start)
        rarity = re.search(r"Rarity\.(\w+):", items[start:end])
        if not rarity:
            raise ValueError(f"no rarity for {match[1]}")
        out[match[1].lower()] = rarity[1].lower()
    if len(out) != 18:
        raise ValueError(f"expected 18 templates, found {len(out)}")
    return out


def chicken_egg_time(chicken):
    init = method(chicken, "public net.minecraft.world.entity.animal.chicken.Chicken(")
    pushes = re.findall(r"sipush\s+(\d+)", init)
    if pushes != ["6000", "6000"]:
        raise ValueError(f"unrecognized egg timer {pushes}")
    return {"bound": 6000, "base": 6000}


def expand(read, tag, seen=None):
    seen = set() if seen is None else seen
    out = []
    for value in read(f"data/minecraft/tags/worldgen/biome/{tag}.json")["values"]:
        if value.startswith("#"):
            inner = strip(value[1:])
            if inner in seen:
                raise ValueError(f"recursive biome tag {inner}")
            seen.add(inner)
            out.extend(expand(read, inner, seen))
        else:
            out.append(strip(value))
    return out


def loot_entries(table, path=()):
    """Every item entry of a loot table, with its pool and weight."""
    out = []
    if isinstance(table, dict):
        if table.get("type") == "minecraft:item" and isinstance(table.get("name"), str):
            out.append({"name": strip(table["name"]), "weight": table.get("weight", 1)})
        for value in table.values():
            out.extend(loot_entries(value))
    elif isinstance(table, list):
        for value in table:
            out.extend(loot_entries(value))
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-items-effects-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "-v", "net.minecraft." + name], text=True
            )

        effects = effect_facts(inspect("world.effect.MobEffects"))
        potions = potion_facts(inspect("world.item.alchemy.Potions"))
        frosted = frosted_ice_facts(inspect("world.level.block.FrostedIceBlock"))
        travel = travel_facts(
            inspect("world.entity.LivingEntity"),
            inspect("world.level.block.WebBlock"),
            inspect("world.entity.projectile.arrow.SpectralArrow"),
        )
        rarity = rarities(inspect("world.item.Items"))
        egg_time = chicken_egg_time(inspect("world.entity.animal.chicken.Chicken"))

    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(name):
            return json.loads(jar.read(name))

        patterns = {}
        for name in sorted(jar.namelist()):
            match = re.match(r"data/minecraft/trim_pattern/(\w+)\.json$", name)
            if match:
                p = read(name)
                patterns[match[1]] = {"asset_id": strip(p["asset_id"]), "decal": p.get("decal", False)}
        materials = {}
        for name in sorted(jar.namelist()):
            match = re.match(r"data/minecraft/trim_material/(\w+)\.json$", name)
            if match:
                m = read(name)
                materials[match[1]] = {"colour": m["description"]["color"], "palette": strip(m["palette_id"])}
        variants = {}
        for mob in ("chicken", "cow", "pig"):
            variants[mob] = {}
            for name in sorted(jar.namelist()):
                match = re.match(r"data/minecraft/" + mob + r"_variant/(\w+)\.json$", name)
                if not match:
                    continue
                v = read(name)
                conditions = []
                for c in v["spawn_conditions"]:
                    cond = c.get("condition")
                    if cond is None:
                        conditions.append({"priority": c["priority"]})
                    elif cond["type"] == "minecraft:biome" and cond["biomes"].startswith("#"):
                        conditions.append({"priority": c["priority"], "biome_tag": strip(cond["biomes"][1:])})
                    else:
                        raise ValueError(f"unrecognized spawn condition in {name}")
                variants[mob][match[1]] = {"model": v.get("model", "normal"), "spawn_conditions": conditions}
        tags = sorted(
            {
                c["biome_tag"]
                for mob in variants.values()
                for v in mob.values()
                for c in v["spawn_conditions"]
                if "biome_tag" in c
            }
        )
        biome_tags = {tag: expand(read, tag) for tag in tags}
        lay = read("data/minecraft/loot_table/gameplay/chicken_lay.json")
        chicken_lay = {}
        for child in lay["pools"][0]["entries"][0]["children"]:
            variant = child["condition"]["predicate"]["minecraft:components"]["minecraft:chicken/variant"]
            chicken_lay[strip(variant)] = strip(child["name"])
        walker = read("data/minecraft/enchantment/frost_walker.json")
        disk = walker["effects"]["minecraft:location_changed"][0]["effect"]
        radius = disk["radius"]
        frost_walker = {
            "block": strip(disk["block_state"]["id"]),
            "age": int(disk["block_state"]["properties"]["age"]),
            "offset": disk["offset"],
            "height": disk["height"],
            "radius_base": radius["value"]["base"],
            "radius_per_level_above_first": radius["value"]["per_level_above_first"],
            "radius_min": radius["min"],
            "radius_max": radius["max"],
            "predicates": [p["type"] for p in disk["predicate"]["predicates"]],
            "max_level": walker["max_level"],
        }
        loot = {}
        for name in sorted(jar.namelist()):
            if not name.startswith("data/minecraft/loot_table/") or not name.endswith(".json"):
                continue
            entries = [e for e in loot_entries(read(name)) if TRACK_ITEMS.match(e["name"])]
            if entries:
                loot[name[len("data/minecraft/loot_table/") : -len(".json")]] = entries
        item_tags = {
            t: [strip(v) for v in read(f"data/minecraft/tags/item/{t}.json")["values"]]
            for t in ("trim_materials", "eggs")
        }
        frosted_loot = read("data/minecraft/loot_table/blocks/frosted_ice.json")
        frosted["drops"] = len(frosted_loot.get("pools", []))

    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.effect.MobEffects (WIND_CHARGED, WEAVING, OOZING, INFESTED and their lambdas)",
            "net.minecraft.world.item.alchemy.Potions (WIND_CHARGED, WEAVING, OOZING, INFESTED)",
            "net.minecraft.world.level.block.FrostedIceBlock.onPlace/tick",
            "net.minecraft.world.entity.LivingEntity.travelInAir",
            "net.minecraft.world.level.block.WebBlock.entityInside",
            "net.minecraft.world.entity.projectile.arrow.SpectralArrow (duration)",
            "net.minecraft.world.item.Items (*_ARMOR_TRIM_SMITHING_TEMPLATE rarities)",
            "net.minecraft.world.entity.animal.chicken.Chicken.<init> (eggTime)",
            "data/minecraft/trim_pattern/*.json, data/minecraft/trim_material/*.json",
            "data/minecraft/{chicken,cow,pig}_variant/*.json and their biome tags",
            "data/minecraft/loot_table/gameplay/chicken_lay.json",
            "data/minecraft/enchantment/frost_walker.json",
            "data/minecraft/loot_table/**/*.json (entries naming Track D items)",
            "data/minecraft/tags/item/{trim_materials,eggs}.json",
        ],
        "effects": effects,
        "potions": potions,
        "frosted_ice": frosted,
        "frost_walker": frost_walker,
        "movement": travel,
        "template_rarity": rarity,
        "trim_patterns": patterns,
        "trim_materials": materials,
        "item_tags": item_tags,
        "farm_variants": variants,
        "biome_tags": biome_tags,
        "chicken_lay": chicken_lay,
        "chicken_egg_time": egg_time,
        "loot": loot,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
