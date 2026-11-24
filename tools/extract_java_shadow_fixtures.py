#!/usr/bin/env python3
"""Extract Java 26.3's entity ground shadows from the pinned client jar.

Reads the client with `javap` and writes, for every entity type the client
registers a renderer for, the shadow radius and strength its renderer's
constructors set (`EntityRenderer.shadowRadius` / `shadowStrength`), the
renderers in its class chain that override `getShadowRadius`, and the
literals of those overrides; the babies' `getAgeScale` values; the
constants of `EntityRenderer.extractShadow` and `extractShadowPiece`; and
the Entity Shadows option's key, label, default and place in Video
Settings.

Usage: extract_java_shadow_fixtures.py client.jar --output
tests/fixtures/java/26.3/entity_shadows.json (`javap` must be on PATH).
"""

import argparse
import hashlib
import json
import re
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_registry_names import CLIENT_SHA1

PKG = "net/minecraft/client/renderer/entity/"
RENDERER = "net/minecraft/client/renderer/entity/EntityRenderer"
INSTRUCTION = re.compile(r"^\s+\d+: (\w+)\s*(.*)$")
FLOAT = re.compile(r"// float (\S+?)f$")
DOUBLE = re.compile(r"// double (\S+?)d$")
INVOKE = re.compile(r'// (?:Interface)?Method (?:(\S+?)\.)?("?<?[\w$]+>?"?):\((.*?)\)(\S+)$')
FIELD = re.compile(r"// Field (?:(\S+?)\.)?(\w+):")


def f32(value):
    """A Java float literal as Java widens it to double."""
    return struct.unpack("f", struct.pack("f", value))[0]


def javap(jar, names, verbose=False):
    flags = ["-c", "-p", "-constants"] + (["-v"] if verbose else [])
    return subprocess.check_output(["javap", *flags, "-classpath", str(jar), *names], text=True)


def erase(text):
    """`text` without generic type arguments (which nest)."""
    out, depth = [], 0
    for ch in text:
        depth += ch == "<"
        if depth == 0:
            out.append(ch)
        depth -= ch == ">"
    return "".join(out)


def split_classes(text):
    """{internal name: (parent internal name, [method blocks])} from javap output."""
    out = {}
    for chunk in re.split(r"^Compiled from .*$", text, flags=re.M):
        heads = [erase(line) for line in chunk.split("\n") if re.match(r"^[\w ]*(?:class|interface) ", line)]
        header = re.search(r"^[\w ]*(?:class|interface) ([\w.$]+)(?: extends ([\w.$]+))?", heads[0]) if heads else None
        if not header:
            continue
        name = header[1].replace(".", "/")
        parent = (header[2] or "java/lang/Object").replace(".", "/")
        out[name] = (parent, chunk.split("\n\n"))
    return out


def params(desc):
    """Parameter type letters of a method descriptor's parameter list."""
    out, i = [], 0
    while i < len(desc):
        start = i
        while desc[i] == "[":
            i += 1
        if desc[i] == "L":
            i = desc.index(";", i)
        out.append(desc[start : i + 1])
        i += 1
    return out


def float_slots(desc, static=False):
    """Local variable slots of a descriptor's float parameters, in order."""
    slot, out = (0 if static else 1), []
    for p in params(desc):
        if p == "F":
            out.append(slot)
        slot += 2 if p in ("J", "D") else 1
    return out


def decl(block):
    """A member block's declaration line (the class header may precede it)."""
    lines = [line for line in block.strip().split("\n") if line.strip()]
    for i, line in enumerate(lines):
        if line.strip() == "Code:" and i > 0:
            heads = [x for x in lines[:i] if not x.strip().startswith(("descriptor:", "flags:"))]
            return heads[-1] if heads else ""
    return lines[-1] if lines and not lines[0].rstrip().endswith("{") else (lines[1] if len(lines) > 1 else "")


def instructions(block):
    for line in block.split("\n"):
        m = INSTRUCTION.match(line)
        if m:
            yield m[1], m[2]


def analyse(block, owner):
    """Float values reaching each constructor call, shadow fields stored, and
    the descriptor of the call to a renderer constructor.

    Values are ("const", float) or ("param", slot)."""
    pending, calls, puts = [], [], []
    for op, arg in instructions(block):
        if op.startswith("fconst_"):
            pending.append(("const", float(op[-1])))
        elif op.startswith("ldc") and FLOAT.search(arg):
            pending.append(("const", f32(float(FLOAT.search(arg)[1]))))
        elif op.startswith("fload"):
            slot = int(op.split("_")[1]) if "_" in op else int(arg)
            pending.append(("param", slot))
        elif op.startswith("invoke") and INVOKE.search(arg):
            m = INVOKE.search(arg)
            target, name, desc, ret = m[1] or owner, m[2].strip('"'), m[3], m[4]
            n = params(desc).count("F")
            args = pending[len(pending) - n :] if n else []
            del pending[len(pending) - n :]
            if name == "<init>":
                calls.append((target, desc, args))
            if ret == "F":
                pending.append(("unknown", None))
        elif op == "putfield" and FIELD.search(arg):
            field = FIELD.search(arg)[2]
            if field in ("shadowRadius", "shadowStrength"):
                puts.append((field, pending.pop()))
    return calls, puts


def method(blocks, signature):
    found = [b for b in blocks if signature in decl(b)]
    if len(found) != 1:
        raise ValueError(f"expected one method matching {signature!r}, found {len(found)}")
    return found[0]


def floats(block):
    """Float and double constants in bytecode order."""
    out = []
    for op, a in instructions(block):
        if op.startswith("ldc") and FLOAT.search(a):
            out.append(f32(float(FLOAT.search(a)[1])))
        elif op.startswith("ldc") and DOUBLE.search(a):
            out.append(float(DOUBLE.search(a)[1]))
        elif op.startswith(("fconst_", "dconst_")):
            out.append(float(op[-1]))
    return out


def expect(label, got, want):
    if got != want:
        raise ValueError(f"reassess {label}: {got!r} != {want!r}")
    return got


class Renderers:
    def __init__(self, classes):
        self.classes = classes

    def ctor(self, name, desc):
        parent, blocks = self.classes[name]
        simple = name.replace("/", ".")
        for b in blocks:
            head = decl(b)
            if re.search(rf"\b{re.escape(simple)}\(", head) and "Code:" in b:
                calls, puts = analyse(b, name)
                own = [c for c in calls if c[0] in (parent, name)]
                if self.descriptor_matches(head, desc):
                    return parent, (own[0] if own else None), puts
        raise ValueError(f"no constructor {name}{desc}")

    @staticmethod
    def descriptor_matches(head, desc):
        java = re.search(r"\((.*)\)", head)[1]
        names = [p.strip().split("<")[0] for p in java.split(",") if p.strip()]
        types = params(desc)
        if len(names) != len(types):
            return False
        prim = {"F": "float", "I": "int", "Z": "boolean", "D": "double", "J": "long"}
        for n, t in zip(names, types):
            want = prim.get(t) or t[1:-1].replace("/", ".")
            if t.startswith("[") or ("." not in n and n not in prim.values()):
                continue
            if n.replace("$", ".") != want.replace("$", "."):
                return False
        return True

    def resolve(self, name, desc, args):
        """(radius, strength) a `new name(desc)` with float slots `args` gets."""
        if name not in self.classes:
            return 0.0, None
        parent, call, puts = self.ctor(name, desc)
        radius, strength = 0.0, None
        if call and call[0] == parent:
            slots = float_slots(call[1])
            radius, strength = self.resolve(parent, call[1], dict(zip(slots, [self.value(v, args) for v in call[2]])))
        elif call and call[0] == name:
            slots = float_slots(call[1])
            return self.resolve(name, call[1], dict(zip(slots, [self.value(v, args) for v in call[2]])))
        for field, v in puts:
            if field == "shadowRadius":
                radius = self.value(v, args)
            else:
                strength = self.value(v, args)
        return radius, strength

    @staticmethod
    def value(v, args):
        kind, x = v
        if kind == "const":
            return x
        if kind == "param" and x in args:
            return args[x]
        raise ValueError(f"unresolved shadow value {v}")

    def chain(self, name):
        out = []
        while name in self.classes:
            out.append(name)
            name = self.classes[name][0]
        return out

    def override(self, name, getter):
        _, blocks = self.classes[name]
        found = [b for b in blocks if re.search(rf"float {getter}\(", decl(b)) and "Code:" in b]
        return found[0] if found else None


def bootstrap_handles(verbose):
    """Bootstrap index -> implementation handle text."""
    section = verbose.split("BootstrapMethods:", 1)[1]
    out = {}
    for m in re.finditer(
        r"^\s+(\d+): #\d+ REF_invokeStatic java/lang/invoke/LambdaMetafactory.*?\n((?:\s+.*\n)+?)(?=\s+\d+: #|\S|$)",
        section,
        re.M,
    ):
        handles = re.findall(r"REF_\w+ (\S+)", m[2])
        out[int(m[1])] = handles[0] if handles else None
    return out


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned Java 26.3 client")
    with tempfile.TemporaryDirectory() as directory:
        jar_path = Path(directory) / "client.jar"
        jar_path.write_bytes(data)
        with zipfile.ZipFile(jar_path) as jar:
            lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
            names = [n[: -len(".class")] for n in jar.namelist() if n.startswith(PKG) and n.endswith(".class")]
            age = [
                n[: -len(".class")]
                for n in jar.namelist()
                if n.startswith("net/minecraft/world/entity/")
                and n.endswith(".class")
                and b"getAgeScale" in jar.read(n)
            ]
        dotted = [n.replace("/", ".") for n in names]
        classes = split_classes(javap(jar_path, dotted))
        registry = javap(jar_path, ["net.minecraft.client.renderer.entity.EntityRenderers"], verbose=True)
        ages = split_classes(javap(jar_path, [n.replace("/", ".") for n in age]))
        options = split_classes(javap(jar_path, ["net.minecraft.client.Options"]))
        video = split_classes(javap(jar_path, ["net.minecraft.client.gui.screens.options.VideoSettingsScreen"]))
        presets = split_classes(javap(jar_path, ["net.minecraft.client.GraphicsPreset"]))
        dispatcher = split_classes(javap(jar_path, ["net.minecraft.client.renderer.entity.EntityRenderDispatcher"]))
        lightmap = split_classes(javap(jar_path, ["net.minecraft.client.renderer.Lightmap"]))

    r = Renderers(classes)
    base = classes[RENDERER][1]

    # EntityRenderer.extractShadow / extractShadowPiece.
    shadow = method(base, "extractShadow(")
    if "entityShadows" not in shadow or "isInvisible" not in shadow or "distanceToCameraSq" not in shadow:
        raise ValueError("reassess extractShadow's option, invisibility and distance checks")
    expect("extractShadow literals", floats(shadow), [32.0, 0.0, 1.0, 256.0, 0.0, 0.5, 1.0, 0.0])
    piece = method(base, "extractShadowPiece(")
    for needle in (
        "getRenderShape",
        "INVISIBLE",
        "getMaxLocalRawBrightness",
        "isCollisionShapeFullBlock",
        "getBrightness",
    ):
        if needle not in piece:
            raise ValueError(f"reassess extractShadowPiece: {needle}")
    expect("brightness threshold", "iconst_3" in piece, True)
    expect("extractShadowPiece literals", floats(piece), [0.5, 0.5, 0.0, 1.0])
    expect(
        "Lightmap.getBrightness",
        floats(method(lightmap["net/minecraft/client/renderer/Lightmap"][1], "float getBrightness(")),
        [15.0, 4.0, 3.0, 1.0],
    )
    submit = method(dispatcher["net/minecraft/client/renderer/entity/EntityRenderDispatcher"][1], " submit(")
    if "shadowPieces" not in submit or "submitShadow" not in submit:
        raise ValueError("reassess the dispatcher's shadow submission")

    # Entity types and their renderers.
    statics = method(registry.split("BootstrapMethods:", 1)[0].split("\n\n"), "static {}")
    handles = bootstrap_handles(registry)
    registry_blocks = registry.split("\n\n")
    entities = {}
    pending_type = None
    for op, arg in instructions(statics):
        if op == "getstatic" and "EntityTypes." in arg:
            pending_type = re.search(r"EntityTypes\.(\w+):", arg)[1].lower()
        elif op == "invokedynamic" and pending_type:
            index = int(re.search(r"InvokeDynamic #(\d+):", arg)[1])
            handle = handles[index]
            target = re.match(r'(\S+?)\."?([\w$<>]+)"?:\((.*?)\)', handle)
            owner, name, desc = target[1], target[2], target[3]
            if name == "<init>":
                cls, ctor_desc, args = owner, desc, {}
            else:
                block = method(registry_blocks, f" {name}(")
                calls, _ = analyse(block, owner)
                new = [c for c in calls if c[0] in classes and c[0] != owner]
                cls, ctor_desc, values = new[-1]
                values = [Renderers.value(v, {}) for v in values]
                args = dict(zip(float_slots(ctor_desc), values))
            radius, strength = r.resolve(cls, ctor_desc, args)
            chain = r.chain(cls)
            entities[pending_type] = {
                "renderer": cls.rsplit("/", 1)[1],
                "radius": radius,
                "strength": 1.0 if strength is None else strength,
                "radius_overrides": [
                    c.rsplit("/", 1)[1] for c in chain if c != RENDERER and r.override(c, "getShadowRadius")
                ],
                "strength_overrides": [
                    c.rsplit("/", 1)[1] for c in chain if c != RENDERER and r.override(c, "getShadowStrength")
                ],
            }
            pending_type = None
    avatar = "net/minecraft/client/renderer/entity/player/AvatarRenderer"
    radius, strength = r.resolve(avatar, "Lnet/minecraft/client/renderer/entity/EntityRendererProvider$Context;Z", {})
    entities["player"] = {
        "renderer": "AvatarRenderer",
        "radius": radius,
        "strength": 1.0 if strength is None else strength,
        "radius_overrides": [
            c.rsplit("/", 1)[1] for c in r.chain(avatar) if c != RENDERER and r.override(c, "getShadowRadius")
        ],
        "strength_overrides": [],
    }

    overrides = {}
    for info in entities.values():
        for simple in info["radius_overrides"] + info["strength_overrides"]:
            full = PKG + simple if PKG + simple in classes else PKG + "player/" + simple
            block = r.override(full, "getShadowRadius") or r.override(full, "getShadowStrength")
            reads = sorted(set(re.findall(r"Field (?:\S+\.)?(\w+):", block)))
            overrides[simple] = {"literals": floats(block), "reads": reads}

    # LivingEntity.getAgeScale and its overrides (babies).
    age_scales = {}
    for name, (_, blocks) in ages.items():
        found = [b for b in blocks if "float getAgeScale();" in decl(b)]
        if found:
            lits = floats(found[0])
            age_scales[name.rsplit("/", 1)[1]] = lits[0]

    # The option.
    opts = options["net/minecraft/client/Options"][1]
    ctor = [b for b in opts if re.search(r"net\.minecraft\.client\.Options\(", decl(b))][0]
    at = ctor.index("String options.entityShadows")
    default = re.search(r"\d+: (iconst_[01])", ctor[at:])[1] == "iconst_1"
    if "String entityShadows" not in method(opts, "processDumpedOptions(") and "String entityShadows" not in "".join(
        b for b in opts if "entityShadows" in b and "process" in b
    ):
        raise ValueError("reassess the entityShadows options.txt key")
    quality = method(video["net/minecraft/client/gui/screens/options/VideoSettingsScreen"][1], "qualityOptions(")
    order = re.findall(r"Method net/minecraft/client/Options\.(\w+):\(\)", quality)
    preset_blocks = presets["net/minecraft/client/GraphicsPreset"][1]
    enum = re.findall(
        r"putstatic .*// Field (\w+):Lnet/minecraft/client/GraphicsPreset;", method(preset_blocks, "static {}")
    )
    code = "\n".join(preset_blocks)
    values = [m[1] == "iconst_1" for m in re.finditer(r"Options\.entityShadows:.*\n\s+\d+: (iconst_[01])", code)]
    named = [n for n in enum if n != "CUSTOM"]
    expect("graphics presets", len(named), len(values))
    preset_values = dict(zip(named, values))

    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "source": "EntityRenderer.extractShadow/extractShadowPiece, ShadowFeatureRenderer, EntityRenderers, "
        "every entity renderer's constructors and getShadowRadius, LivingEntity.getAgeScale and its overrides, "
        "Lightmap.getBrightness, Options.entityShadows, VideoSettingsScreen.qualityOptions, GraphicsPreset",
        "option": {
            "key": "entityShadows",
            "label": lang["options.entityShadows"],
            "default": default,
            "quality_order": order,
            "graphics_presets": preset_values,
        },
        "constants": {
            "max_radius": 32.0,
            "distance_squared": 256.0,
            "depth_power": 0.5,
            "falloff_per_block": 0.5,
            "alpha_scale": 0.5,
            "min_brightness": 3,
        },
        "age_scales": dict(sorted(age_scales.items())),
        "overrides": dict(sorted(overrides.items())),
        "entities": dict(sorted(entities.items())),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("client", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = extract(args.client)
    args.output.write_text(json.dumps(out, indent=1, sort_keys=False) + "\n")


if __name__ == "__main__":
    main()
