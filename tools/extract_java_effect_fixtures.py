#!/usr/bin/env python3
"""Extract Java 26.3's mob effect registry from the pinned official artifacts.

Reads the `MobEffects` static initializer from the server jar's bytecode
(requires javap): registration order, id, effect class, category, colour,
extra constructor arguments, attribute modifiers, blend durations and the
sound played when added. Whether an effect is instantaneous comes from the
`isInstantaneous` override on its class hierarchy, and attribute ids from the
`Attributes` registrations. Display names come from the client's en_us.json.

Every instruction of every registration must match a known form; anything
else fails rather than producing a guessed fixture. No VoxelCraft source or
existing fixture is read.
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
from extract_java_registry_names import CLIENT_SHA1

EFFECT = "net/minecraft/world/effect/"


def instructions(code, header):
    """(opcode, rest) pairs of the method whose javap header line is `header`."""
    body = code.split(f"  {header}\n", 1)[1].split("\n\n", 1)[0]
    result = []
    for line in body.splitlines():
        match = re.match(r"\s+\d+: (\w+)\s*(.*)$", line)
        if match:
            result.append((match[1], match[2]))
    return result


def comment(rest):
    return rest.split("// ", 1)[1] if "// " in rest else ""


def int_operand(op, rest):
    if op.startswith("iconst_"):
        return -1 if op == "iconst_m1" else int(op[len("iconst_"):])
    if op in ("bipush", "sipush"):
        return int(rest.split()[0])
    if op in ("ldc", "ldc_w"):
        match = re.fullmatch(r"int (-?\d+)", comment(rest))
        if match:
            return int(match[1])
    return None


def double_operand(op, rest):
    if op in ("dconst_0", "dconst_1"):
        return float(op[-1])
    if op == "ldc2_w":
        match = re.fullmatch(r"double (-?[0-9.E-]+)d", comment(rest))
        if match:
            return float(match[1])
    return None


def static_field(op, rest, owner):
    if op != "getstatic":
        return None
    match = re.fullmatch("Field " + re.escape(owner) + r"\.([A-Z_0-9]+):\S+", comment(rest))
    return match[1] if match else None


def lambdas(verbose, code):
    """Bootstrap index -> the int provider its `lambda$static$N` returns."""
    table = verbose.split("BootstrapMethods:", 1)[1]
    targets = {}
    for match in re.finditer(r"\n  (\d+): #\d+ REF_invokeStatic java/lang/invoke/LambdaMetafactory\.metafactory.*?"
                             r"REF_invokeStatic net/minecraft/world/effect/MobEffects\.(lambda\$static\$\d+):", table, re.S):
        targets[int(match[1])] = match[2]
    providers = {}
    for index, name in targets.items():
        body = instructions(code, f"private static int {name}(net.minecraft.util.RandomSource);")
        ops = [op for op, _ in body]
        if len(body) == 2 and ops[1] == "ireturn" and int_operand(*body[0]) is not None:
            providers[index] = {"constant": int_operand(*body[0])}
        elif (len(body) == 5 and ops[0] == "aload_0" and ops[4] == "ireturn"
              and comment(body[3][1]).startswith("Method net/minecraft/util/Mth.randomBetweenInclusive:")
              and None not in (int_operand(*body[1]), int_operand(*body[2]))):
            providers[index] = {"random_between_inclusive": [int_operand(*body[1]), int_operand(*body[2])]}
        else:
            raise ValueError(f"unknown int provider form: {name}")
    return providers


def registrations(code, providers, attribute_ids):
    lines = instructions(code, "static {};")
    effects = []
    i = 0

    def take():
        nonlocal i
        if i >= len(lines):
            raise ValueError("MobEffects initializer ended inside a registration")
        i += 1
        return lines[i - 1]

    while lines[i][0] != "return":
        op, rest = take()
        name = re.fullmatch(r"String ([a-z_]+)", comment(rest))
        if op not in ("ldc", "ldc_w") or not name:
            raise ValueError(f"expected an effect id, found {op} {rest}")
        op, rest = take()
        cls = re.fullmatch(r"class " + EFFECT + r"(\w+)", comment(rest))
        if op != "new" or not cls or take()[0] != "dup":
            raise ValueError(f"unknown construction of {name[1]}")
        category = static_field(*take(), EFFECT + "MobEffectCategory")
        colour = int_operand(*take())
        if category is None or colour is None:
            raise ValueError(f"unknown category or colour of {name[1]}")
        extras = []
        while True:
            op, rest = take()
            if op == "invokespecial":
                init = re.fullmatch("Method " + re.escape(EFFECT + cls[1]) + r'\."<init>":\(L'
                                    + re.escape(EFFECT) + r"MobEffectCategory;I(.*)\)V", comment(rest))
                if not init:
                    raise ValueError(f"unknown constructor of {name[1]}: {rest}")
                break
            if op in ("iconst_0", "iconst_1"):
                extras.append({"boolean": op == "iconst_1"})
            elif static_field(op, rest, "net/minecraft/core/particles/ParticleTypes"):
                extras.append({"particle": static_field(op, rest, "net/minecraft/core/particles/ParticleTypes").lower()})
            elif op in ("ldc", "ldc_w") and re.fullmatch(r"float (-?[0-9.E-]+)f", comment(rest)):
                extras.append({"float": float(re.fullmatch(r"float (-?[0-9.E-]+)f", comment(rest))[1])})
            elif op == "invokedynamic":
                index = int(re.fullmatch(r"InvokeDynamic #(\d+):applyAsInt:\(\)Ljava/util/function/ToIntFunction;", comment(rest))[1])
                extras.append({"int_provider": providers[index]})
            else:
                raise ValueError(f"unknown constructor argument of {name[1]}: {op} {rest}")
        descriptor = init[1]
        expected = "".join({"boolean": "Z", "particle": "Lnet/minecraft/core/particles/ParticleOptions;",
                            "float": "F", "int_provider": "Ljava/util/function/ToIntFunction;"}[next(iter(e))]
                           for e in extras)
        if descriptor != expected:
            raise ValueError(f"constructor arguments of {name[1]} do not match {descriptor}")
        modifiers, blend, sound = [], None, None
        while True:
            op, rest = take()
            if op == "invokestatic" and comment(rest).startswith("Method register:"):
                break
            attribute = static_field(op, rest, "net/minecraft/world/entity/ai/attributes/Attributes")
            if attribute:
                op, rest = take()
                modifier_id = re.fullmatch(r"String (\S+)", comment(rest))
                if op not in ("ldc", "ldc_w") or not modifier_id:
                    raise ValueError(f"unknown modifier id of {name[1]}")
                if not comment(take()[1]).startswith("Method net/minecraft/resources/Identifier.withDefaultNamespace:"):
                    raise ValueError(f"unknown modifier identifier of {name[1]}")
                amount = double_operand(*take())
                operation = static_field(*take(), "net/minecraft/world/entity/ai/attributes/AttributeModifier$Operation")
                op, rest = take()
                if amount is None or operation is None or op != "invokevirtual" or ".addAttributeModifier:" not in rest:
                    raise ValueError(f"unknown attribute modifier of {name[1]}")
                modifiers.append({"attribute": attribute_ids[attribute], "id": "minecraft:" + modifier_id[1],
                                  "amount": amount, "operation": operation.lower()})
                continue
            event = static_field(op, rest, "net/minecraft/sounds/SoundEvents")
            if event:
                op, rest = take()
                if op != "invokevirtual" or ".withSoundOnAdded:" not in rest:
                    raise ValueError(f"unknown sound registration of {name[1]}")
                sound = event.lower()
                continue
            ticks = []
            while int_operand(op, rest) is not None:
                ticks.append(int_operand(op, rest))
                op, rest = take()
            if ticks and op == "invokevirtual" and ".setBlendDuration:" in rest and len(ticks) in (1, 3):
                ticks = ticks * 3 if len(ticks) == 1 else ticks
                blend = {"in": ticks[0], "out": ticks[1], "out_advance": ticks[2]}
                continue
            raise ValueError(f"unknown registration step of {name[1]}: {op} {rest}")
        op, rest = take()
        field = re.fullmatch(r"Field ([A-Z_]+):Lnet/minecraft/core/Holder;", comment(rest))
        if op != "putstatic" or not field:
            raise ValueError(f"{name[1]} is not stored in a field")
        effects.append({"registry_index": len(effects), "id": name[1], "field": field[1], "class": cls[1],
                        "category": category.lower(), "color": colour,
                        "constructor_arguments": extras, "attribute_modifiers": modifiers,
                        "particle": next((e["particle"] for e in extras if "particle" in e), None),
                        "sound_on_added": sound, "blend": blend})
    if i != len(lines) - 1:
        raise ValueError("instructions after the final return")
    return effects


def attribute_registrations(code):
    ids = {}
    block = code.split("  static {};", 1)[1]
    parts = re.split(r"putstatic\s+#\d+\s+// Field ([A-Z_0-9]+):Lnet/minecraft/core/Holder;", block)
    for chunk, field in zip(parts[0::2], parts[1::2]):
        names = re.findall(r"// String ([a-z_.]+)\n", chunk)
        if not names:
            raise ValueError(f"unknown attribute registration: {field}")
        ids[field] = names[0]
    return ids


def instantaneous(inspect, cls):
    """The `isInstantaneous` result of the nearest class in the hierarchy declaring it."""
    while True:
        code = inspect(EFFECT.replace("/", ".") + cls)
        header = re.search(r"class " + re.escape(EFFECT.replace("/", ".")) + cls + r" extends ([\w.$]+)", code)
        if "public boolean isInstantaneous();" in code:
            body = [op for op, _ in instructions(code, "public boolean isInstantaneous();")]
            if body not in (["iconst_0", "ireturn"], ["iconst_1", "ireturn"]):
                raise ValueError(f"unknown isInstantaneous in {cls}")
            return body[0] == "iconst_1"
        if not header or not header[1].startswith(EFFECT.replace("/", ".")):
            raise ValueError(f"{cls} has no isInstantaneous on its effect hierarchy")
        cls = header[1].rsplit(".", 1)[1]


def extract(server, client):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    client_data = client.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(io.BytesIO(client_data)) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-effects-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(inner)
        cache = {}

        def inspect(name, *flags):
            key = (name, flags)
            if key not in cache:
                cache[key] = subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p", *flags, name],
                                                     text=True)
            return cache[key]

        code = inspect("net.minecraft.world.effect.MobEffects")
        providers = lambdas(inspect("net.minecraft.world.effect.MobEffects", "-v"), code)
        attribute_ids = attribute_registrations(inspect("net.minecraft.world.entity.ai.attributes.Attributes"))
        effects = registrations(code, providers, attribute_ids)
        for effect in effects:
            effect["instantaneous"] = instantaneous(inspect, effect["class"])
            effect["name"] = lang[f"effect.minecraft.{effect['id']}"]
        tooltip = inspect("net.minecraft.world.effect.MobEffectCategory")
        formatting = {}
        for field in ("BENEFICIAL", "HARMFUL", "NEUTRAL"):
            match = re.search(r"// String " + field + r"\n.*?\n.*?// Field net/minecraft/ChatFormatting\.([A-Z_]+):", tooltip)
            if not match:
                raise ValueError(f"unknown tooltip formatting of {field}")
            formatting[field.lower()] = match[1].lower()
    keys = sorted(k[len("effect.minecraft."):] for k in lang if k.startswith("effect.minecraft."))
    if keys != sorted(e["id"] for e in effects):
        raise ValueError("en_us effect names and MobEffects registrations differ")
    ordered = ["registry_index", "id", "name", "field", "class", "category", "color", "instantaneous",
               "attribute_modifiers", "particle", "sound_on_added", "blend", "constructor_arguments"]
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "client_sha1": CLIENT_SHA1,
        "sources": ["net.minecraft.world.effect.MobEffects static initializer and lambdas",
                    "net.minecraft.world.effect.MobEffectCategory tooltip formatting",
                    "isInstantaneous on each effect class hierarchy",
                    "net.minecraft.world.entity.ai.attributes.Attributes registrations",
                    "assets/minecraft/lang/en_us.json effect.minecraft.*"],
        "category_tooltip_formatting": formatting,
        "effects": [{key: effect[key] for key in ordered} for effect in effects],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (the bundler)")
    parser.add_argument("--client", type=Path, required=True, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = extract(args.server, args.client)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=1) + "\n")


if __name__ == "__main__":
    main()
