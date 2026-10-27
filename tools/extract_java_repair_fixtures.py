#!/usr/bin/env python3
"""Extract every Java 26.3 item's anvil repair materials (`repairable`).

`Items` sets the component three ways: tool properties (`sword`, `pickaxe`,
`axe`, `hoe`, `shovel`, `spear`, `tool`) take their `ToolMaterial`'s repair
tag, `humanoidArmor` and `wolfArmor` their `ArmorMaterial`'s, and some
registrations call `repairable` with a tag or a single item. Registrations
are followed through nested callbacks; `Items` fields are resolved to
registry IDs and item tags are expanded from data. Never reads VoxelCraft
source.
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
from java_block_properties import item_registry_ids

PROPS = r"net/minecraft/world/item/Item\$Properties"
TOOL_FORMS = ("sword", "pickaxe", "axe", "hoe", "shovel", "spear", "tool")
ARMOR_FORMS = ("humanoidArmor", "wolfArmor")
UNREPAIRABLE_ARMOR_FORMS = ("horseArmor", "nautilusArmor")


def expand(jar, name, stack=()):
    if name in stack:
        raise ValueError("reference tag cycle")
    values = set()
    for value in json.loads(jar.read(f"data/minecraft/tags/item/{name}.json"))["values"]:
        if not isinstance(value, str):
            raise ValueError("unknown tag entry form")
        if value.startswith("#minecraft:"):
            values |= expand(jar, value.removeprefix("#minecraft:"), (*stack, name))
        elif value.startswith("minecraft:"):
            values.add(value.removeprefix("minecraft:"))
        else:
            raise ValueError("unexpected namespace")
    return values


def chunks(code):
    """`{field: initialization bytecode}` for one class's static fields."""
    clinit = code.split("  static {};")[1]
    out, start = {}, 0
    for match in re.finditer(r"putstatic\s+#[^\n]*// Field ([A-Z0-9_]+):", clinit):
        out[match[1]] = clinit[start:match.start()]
        start = match.end()
    return out


def single(pattern, text, what):
    found = set(re.findall(pattern, text))
    if len(found) != 1:
        raise ValueError(f"{what}: expected one match of {pattern}, found {found}")
    return found.pop()


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-repair-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(cls, *flags):
            return subprocess.check_output(["javap", "-classpath", str(path), *flags, "-c", "-p", cls], text=True)

        ids = item_registry_ids(path)
        item_props = inspect("net.minecraft.world.item.Item$Properties")
        tool_material = inspect("net.minecraft.world.item.ToolMaterial")

        def method(code, name):
            return code.split(f" {name}(")[1].split("\n\n")[0]

        if "repairable:(Lnet/minecraft/tags/TagKey;)" not in method(tool_material, "applyCommonProperties"):
            raise ValueError("ToolMaterial.applyCommonProperties no longer sets repairable")
        for form in ("applyToolProperties", "applySwordProperties"):
            if "applyCommonProperties:" not in method(tool_material, form):
                raise ValueError(f"ToolMaterial.{form} no longer applies the common properties")
        for form in TOOL_FORMS:
            body = method(item_props, form)
            if not re.search(r"ToolMaterial\.(applyToolProperties|applySwordProperties):|repairItems:", body):
                if not re.search(r"Method (pickaxe|tool|axe|hoe|shovel|sword):", body):
                    raise ValueError(f"Item.Properties.{form} doesn't take its material's repair tag")
        for form in ARMOR_FORMS:
            if "repairable:(Lnet/minecraft/tags/TagKey;)" not in method(item_props, form):
                raise ValueError(f"Item.Properties.{form} no longer sets repairable")
        for form in UNREPAIRABLE_ARMOR_FORMS:
            if "repairable:" in method(item_props, form):
                raise ValueError(f"Item.Properties.{form} now sets repairable; reassess")
        tag_field = r"// Field net/minecraft/tags/ItemTags\.(\w+):"
        tool_tags = {f: single(tag_field, c, f"ToolMaterial.{f}") for f, c in chunks(tool_material).items()}
        armor_tags = {f: single(tag_field, c, f"ArmorMaterials.{f}")
                      for f, c in chunks(inspect("net.minecraft.world.item.equipment.ArmorMaterials")).items()}
        tag_names = {f: single(r"// String (\w+)", c, f"ItemTags.{f}")
                     for f, c in chunks(inspect("net.minecraft.tags.ItemTags")).items() if "Method bind:" in c}

        code = inspect("net.minecraft.world.item.Items")
        verbose = inspect("net.minecraft.world.item.Items", "-v").split("BootstrapMethods:")[1]
        lambdas = {re.search(r"(lambda\$\w+\$\d+)\(", block).group(1): block for block in code.split("\n\n")
                   if re.search(r"(lambda\$\w+\$\d+)\(", block.split("\n")[0])}
        boot = {int(b.split(":")[0].strip()): re.search(r"Items\.(lambda\$\w+\$\d+)", b).group(1)
                for b in re.split(r"\n(?=  \d+:)", verbose) if re.search(r"Items\.(lambda\$\w+\$\d+)", b)}

        def with_callbacks(block, visiting=()):
            text = block
            for number in re.findall(r"InvokeDynamic #(\d+):", block):
                callback = boot.get(int(number))
                if callback and callback not in visiting:
                    text += "\n" + with_callbacks(lambdas[callback], (*visiting, callback))
            return text

        repairs = {}
        for field, block in chunks(code).items():
            if ids.get(field) is None and field in ids:
                continue
            text = with_callbacks(block)
            sources = []
            for form in re.findall(PROPS + r"\.(\w+):\(Lnet/minecraft/world/item/ToolMaterial;", text):
                if form not in TOOL_FORMS:
                    raise ValueError(f"Items.{field}: unknown tool form {form}")
                material = single(r"// Field net/minecraft/world/item/ToolMaterial\.(\w+):", text, field)
                sources.append(("tag", tag_names[tool_tags[material]]))
            for form in re.findall(PROPS + r"\.(\w+):\(Lnet/minecraft/world/item/equipment/ArmorMaterial;", text):
                if form in ARMOR_FORMS:
                    material = single(r"// Field net/minecraft/world/item/equipment/ArmorMaterials\.(\w+):", text,
                                      field)
                    sources.append(("tag", tag_names[armor_tags[material]]))
                elif form not in UNREPAIRABLE_ARMOR_FORMS:
                    raise ValueError(f"Items.{field}: unknown armor form {form}")
            for call in re.finditer(PROPS + r"\.repairable:\(L([\w/]+);", text):
                before = text[:call.start()]
                if call[1] == "net/minecraft/tags/TagKey":
                    sources.append(("tag", tag_names[re.findall(tag_field, before)[-1]]))
                elif call[1] == "net/minecraft/world/item/Item":
                    item = re.findall(r"// Field ([A-Z0-9_]+):Lnet/minecraft/world/item/Item;", before)[-1]
                    sources.append(("item", ids[item][0]))
                else:
                    raise ValueError(f"Items.{field}: unknown repairable form {call[1]}")
            if not sources:
                continue
            if len(set(sources)) != 1 or field not in ids:
                raise ValueError(f"Items.{field}: ambiguous repair sources {sources}")
            for name in ids[field]:
                repairs[name] = sources[0]
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        tags = {name: sorted(expand(jar, name)) for kind, name in sorted(set(repairs.values())) if kind == "tag"}
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["net.minecraft.world.item.Items repairable registrations, including nested callbacks",
                        "Item.Properties tool and armor forms", "ToolMaterial", "ArmorMaterials", "ItemTags",
                        "net.minecraft.references.BlockItemIds and ItemIds (registry IDs)",
                        *(f"data/minecraft/tags/item/{name}.json" for name in sorted(tags))],
            "repairs": [{"item": f"minecraft:{name}", ("tag" if kind == "tag" else "material"): value}
                        for name, (kind, value) in sorted(repairs.items())],
            "tags": tags}


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
