#!/usr/bin/env python3
"""Extract Java 26.3 brewing from the SHA-pinned official server (requires javap).

26.3 replaced PotionBrewing with VanillaBrewingProvider and brewing recipe JSON.
No VoxelCraft source or existing fixture is read. Unknown registration forms fail.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_turtle_fixtures import field_initialization, integer_constants


def instructions(code, method):
    body = code.split(f" {method}(", 1)[1].split("\n\n", 1)[0]
    return [line.split(":", 1)[1].strip() for line in body.splitlines()
            if re.match(r"\s+\d+:", line)]


def registrations(code, method, call, arity):
    lines = instructions(code, method)
    rows = []
    while lines and lines[0] != "return":
        group, lines = lines[:arity + 2], lines[arity + 2:]
        if len(group) != arity + 2 or group[0] != "aload_0" or not re.fullmatch(
                rf"invokevirtual\s+#\d+\s+// Method {call}:\([^)]*\)V", group[-1]):
            raise ValueError(f"unknown {method} bytecode: {group}")
        fields = []
        for line in group[1:-1]:
            match = re.fullmatch(r"getstatic\s+#\d+\s+// Field net/minecraft/world/item/(?:alchemy/Potions|Items)\.([A-Z_]+):L[^;]+;", line)
            if not match:
                raise ValueError(f"unknown field operand: {line}")
            fields.append(match[1].lower())
        rows.append(fields)
    if lines != ["return"]:
        raise ValueError(f"unknown method ending: {method}")
    return rows


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-brewing-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(inner)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p", name], text=True)

        provider = inspect("net.minecraft.data.recipes.packs.VanillaBrewingProvider")
        containers = registrations(provider, "addContainers", "addContainer", 1)
        transforms = registrations(provider, "addContainerTransformations", "addContainerTransformation", 3)
        # buildMixes interleaves buildMix and buildStartMix, both straight-line calls.
        lines = instructions(provider, "buildMixes")
        mixes, starts = [], []
        while lines and lines[0] != "return":
            end = next((i for i, line in enumerate(lines) if line.startswith("invokevirtual")), None)
            if end not in (3, 4):
                raise ValueError("unknown mix registration arity")
            group, lines = lines[:end + 1], lines[end + 1:]
            call = "buildStartMix" if end == 3 else "buildMix"
            synthetic = " x();\n" + "\n".join(f" {i}: {line}" for i, line in enumerate(group + ["return"]))
            row = registrations(synthetic, "x", call, end - 1)[0]
            (starts if end == 3 else mixes).append(row)
        if lines != ["return"]:
            raise ValueError("unknown buildMixes ending")
        base = inspect("net.minecraft.data.recipes.BrewingProvider")
        start = instructions(base, "buildStartMix")
        if len(start) != 11 or not all(text in "\n".join(start) for text in
                ("Potions.WATER:", "Potions.MUNDANE:", "Potions.AWKWARD:", "aload_1", "aload_2")):
            raise ValueError("unknown start mix expansion")
        expanded = mixes + [["water", ingredient, "mundane"] for ingredient, _ in starts] + [
            ["awkward", ingredient, result] for ingredient, result in starts]
        potions_code = inspect("net.minecraft.world.item.alchemy.Potions")
        potions = sorted(set(re.findall(r"putstatic\s+#\d+\s+// Field ([A-Z_]+):", potions_code)))
        invisibility = []
        for field in ("INVISIBILITY", "LONG_INVISIBILITY"):
            block = field_initialization(potions_code, field)
            if "MobEffects.INVISIBILITY:" not in block:
                raise ValueError("unknown invisibility effect")
            effect_block = block.split("MobEffects.INVISIBILITY:", 1)[1].split("invokespecial", 1)[0]
            values = integer_constants(effect_block)
            if len(values) != 1:
                raise ValueError("unknown invisibility duration registration")
            invisibility.append({"id": field.lower(), "ticks": values[0], "amplifier": 0})
        # Pin the complete inspected instruction forms, including branches and the
        # armor-slot loop. Changes require reinspection rather than guessed facts.
        checked = [
            ("net.minecraft.world.entity.LivingEntity", "getVisibilityPercent", "9fedf44663d4c9a6f7890381f3b576787dadb33e8c6b6c5533ec8d2c1e79ceaa"),
            ("net.minecraft.world.entity.LivingEntity", "getArmorCoverPercentage", "e0e5d8d7ac2cc45df7d661d8a6557c19b0f490bd09f33e521eaf43d73f132b44"),
            ("net.minecraft.world.entity.ai.targeting.TargetingConditions", "test", "d2bfeac9095bd92bad5e87f257e8b929f6d20ec71446a4977d872b4df0a5a8f0"),
        ]
        forms = {}
        for name, method, digest in checked:
            body = "\n".join(instructions(inspect(name), method))
            if hashlib.sha256(body.encode()).hexdigest() != digest:
                raise ValueError(f"unknown visibility bytecode form: {name}.{method}")
            forms[method] = body
        def number(method, literal):
            values = re.findall(r"// " + literal + r" ([0-9.]+)[fd]", forms[method])
            return [float(v) for v in values]
        visibility_doubles = number("getVisibilityPercent", "double")
        visibility_floats = number("getVisibilityPercent", "float")
        slot_init = inspect("net.minecraft.world.entity.EquipmentSlot").split("  static {};", 1)[1]
        armor_slots = len(re.findall(r"getstatic[^\n]*EquipmentSlot\$Type.HUMANOID_ARMOR:", slot_init))
        if armor_slots == 0:
            raise ValueError("unknown humanoid armor slot registrations")
        visibility = {"sneaking": visibility_doubles[0], "invisible": visibility_doubles[1],
                      "minimum_armor_fraction": visibility_floats[0],
                      "minimum_detection_range": number("test", "double")[-1], "humanoid_armor_slots": armor_slots}
    recipes = []
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        for path in sorted(jar.namelist()):
            if not (path.startswith("data/minecraft/recipe/brewing/") and path.endswith(".json")):
                continue
            rule = json.loads(jar.read(path))
            if (set(rule) != {"type", "input", "output", "reagent"}
                    or rule["type"] != "minecraft:brewing"
                    or set(rule["input"]) != {"item", "potion_contents"}
                    or set(rule["input"]["potion_contents"]) != {"potions"}
                    or set(rule["reagent"]) != {"item"}
                    or set(rule["output"]) != {"id", "components"}
                    or set(rule["output"]["components"]) != {"minecraft:potion_contents"}
                    or set(rule["output"]["components"]["minecraft:potion_contents"]) != {"potion"}):
                raise ValueError(f"unknown brewing JSON form: {path}")
            clean = lambda s: s.removeprefix("minecraft:")
            recipes.append({"path": path, "container": clean(rule["input"]["item"]),
                            "input": clean(rule["input"]["potion_contents"]["potions"]),
                            "ingredient": clean(rule["reagent"]["item"]),
                            "result_container": clean(rule["output"]["id"]),
                            "result": clean(rule["output"]["components"]["minecraft:potion_contents"]["potion"])})
    expected = {(c[0], a, b, c[0], d) for c in containers for a, b, d in expanded}
    for a, b, c in transforms:
        expected.update((a, p.lower(), b, c, p.lower()) for p in {v for row in expanded for v in (row[0], row[2])})
    actual = {(r["container"], r["input"], r["ingredient"], r["result_container"], r["result"]) for r in recipes}
    if actual != expected:
        raise ValueError(f"provider/recipe mismatch: missing {expected - actual}, extra {actual - expected}")
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["net.minecraft.data.recipes.packs.VanillaBrewingProvider",
                        "net.minecraft.data.recipes.BrewingProvider.buildStartMix",
                        "data/minecraft/recipe/brewing/*.json", "net.minecraft.world.item.alchemy.Potions",
                        "net.minecraft.world.entity.LivingEntity.getVisibilityPercent/getArmorCoverPercentage",
                        "net.minecraft.world.entity.ai.targeting.TargetingConditions.test",
                        "net.minecraft.world.entity.EquipmentSlot humanoid armor registrations"],
            "containers": [c[0] for c in containers], "container_mixes": transforms,
            "start_mixes": starts, "potion_mixes": mixes, "potions": [p.lower() for p in potions],
            "invisibility": invisibility, "visibility": visibility, "recipes": recipes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = extract(args.server)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2) + "\n")


if __name__ == "__main__":
    main()
